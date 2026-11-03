//! [`BrainDriver`]: runs a [`BossBrain`] alongside the boss's Lua AI in a duel.
//!
//! Timing model, per duel step:
//!
//! 1. Poll the controller. An answer is a *plan set* (one plan is a set of one): a weighted
//!    list of plans. The driver samples the next plan from it locally (the controller's
//!    probabilities, a penalty for options that were deflected or guarded repeatedly this
//!    fight and a bonus for untried ones, see [`super::sampler`]) and hands it to the Lua
//!    runtime's act hook, where it waits for the next *slot*: the next time his battle goal
//!    picks an act.
//! 2. The Lua AI thinks. At a slot the waiting plan is queued instead of the script's pick,
//!    unless the moment is a reaction; with no plan waiting, the script's own pick runs (the
//!    fallback: the boss never waits for a controller).
//! 3. After a slot, a request goes out according to the [`RequestMode`]: `slot` after every
//!    slot, `plan_end` when the last step of the running plan starts, `checkpoints` only at
//!    events (fight start, half HP, a lost deathblow mark, Wolf punishing twice or deflecting
//!    three times in a row; each once per fight, a lost mark each time). One request is in
//!    flight at a time. In `slot` and `plan_end` an answer is used for one plan; in
//!    `checkpoints` the set persists and the boss keeps cycling through it, sampled again at
//!    every slot, until the next answer.
//! 4. After the step, hits and action changes feed the [`Tally`], Wolf's habits and metrics.
//!
//! A single-shot plan whose answer is older than [`BrainDriver::max_staleness`] when it would
//! run is dropped and a fresh request is sent.
//!
//! A *fight* runs from the start (or a respawn) until either side dies. Its tally, habits and
//! record ([`FightRecord`]: Wolf's metrics, the choices made, tokens, latency) then go into the
//! [`Memory`] of every fight, which is saved to its file (if any) at the end of each fight,
//! every [`AUTOSAVE_SECS`] and when the driver is dropped (the game exits). Once a fighter is
//! dead nothing is requested or logged until the next fight starts.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use super::actions::BossActions;
use super::coach::{
    CallStats, CoachInput, CoachReply, CoachRequest, FightLine, GamePlan, INPUT_BUDGET, KEEP,
    SetStat,
};
use super::habits::{HabitTracker, WolfFrame};
use super::limits::{self, Pacing};
use super::memory::{
    BossKnowledge, FightEnd, LayerPaths, Loaded, Memory, MemorySummary, PlanFailures,
};
use super::metrics::{FightRecord, MetricsTracker, Trend, percentile};
use super::observation::observe;
use super::plan::{Plan, Step, Weighted};
use super::sampler::{self, Context, EXCLUDE_STREAK, Pick, Rng, Streaks};
use super::tally::{
    CountMap, Counts, Exchange, Side, Tally, TallyEvent, merge_counts, ranked_counts,
};
use super::{BossBrain, BrainReply, BrainRequest, REQUEST_BUDGET, RequestMode};
use crate::duel::{Duel, DuelReport, FLAG_PARRY_TIMING};
use crate::hits::FLAG_GUARD;
use crate::lua_ai::{ActSlot, AiBrain, anim_id_from_clip};

/// Seconds of fight between saves of an unfinished fight.
pub const AUTOSAVE_SECS: f32 = 15.0;

/// Estimated Wolf hits remaining: enough lead time to change the next exchange.
pub const PRESSURE_HITS: f32 = 6.0;
/// Urgent warning, still leaving at least two hits for a decision to matter.
pub const PRESSURE_CRITICAL_HITS: f32 = 3.0;
/// Below this estimate, no pressure warning leaves enough time to act.
pub const PRESSURE_MIN_HITS: f32 = 2.0;
/// Re-arm both warnings after remaining posture recovers above 70% of maximum.
pub const PRESSURE_REARM: f32 = 0.70;

/// Counters for tests and the end-of-fight summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BrainStats {
    pub requests: u32,
    pub replies: u32,
    /// Slots that ran the controller's plan.
    pub planned: u32,
    /// Slots where no plan was waiting, so the Lua AI's own act ran.
    pub fallback: u32,
    /// Slots the Lua AI kept as reactions.
    pub reflex: u32,
    /// Plans dropped as too old, and refused as invalid.
    pub stale: u32,
    pub invalid: u32,
    /// Fights that ended (a death), and memory saves that failed.
    pub fights: u32,
    pub save_errors: u32,
}

/// The plan waiting for its slot.
#[derive(Debug, Clone)]
struct Ready {
    /// When the answer behind it was asked for (a single-shot plan goes stale), or when it was
    /// picked (a set's plan).
    since: f32,
    plan: Plan,
    /// The game plan's opener (put back if it goes stale).
    opener: bool,
    /// Which part of the game plan it came from: `opener`, a set name, or `-`.
    label: String,
}

/// A coach request in flight.
#[derive(Debug, Clone, Copy)]
struct CoachFlight {
    id: u64,
    sent: f32,
    mid: bool,
    fight: u32,
}

/// A request in flight.
#[derive(Debug, Clone, Copy)]
struct InFlight {
    id: u64,
    sent: f32,
    fight: u32,
}

/// Which checkpoint events have fired this fight (see [`RequestMode::Checkpoints`]).
#[derive(Debug, Clone, Default)]
struct Checkpoints {
    wolf_thresholds: [bool; 3],
    wolf_broken: bool,
    previous_posture: f32,
    pressure: bool,
    pressure_critical: bool,
    half_hp: bool,
    marks: Option<i32>,
    punish_run: u32,
    punished_twice: bool,
    deflect_run: u32,
    deflected_thrice: bool,
}

impl Checkpoints {
    fn on_wolf(&mut self, posture: f32, broken: bool) -> (Vec<&'static str>, bool) {
        if posture < 0.15 {
            self.wolf_thresholds = [false; 3];
        }
        let edge = broken && !self.wolf_broken;
        self.wolf_broken = broken;
        let mut events = Vec::new();
        for (i, (level, name)) in [
            (0.25, "wolf_posture_25"),
            (0.5, "wolf_posture_50"),
            (0.75, "wolf_posture_75"),
        ]
        .into_iter()
        .enumerate()
        {
            if posture >= level && !self.wolf_thresholds[i] {
                self.wolf_thresholds[i] = true;
                events.push(name);
            }
        }
        if edge {
            events.push("wolf_broken");
        }
        (events, edge)
    }
    fn on_vitals(&mut self, posture: f32, hp: f32, hits: Option<f32>) -> Option<&'static str> {
        let growing = posture > self.previous_posture;
        if 1.0 - posture > PRESSURE_REARM && 1.0 - self.previous_posture <= PRESSURE_REARM {
            self.pressure = false;
            self.pressure_critical = false;
        }
        self.previous_posture = posture;
        if growing
            && !self.pressure_critical
            && hits.is_some_and(|h| (PRESSURE_MIN_HITS..=PRESSURE_CRITICAL_HITS).contains(&h))
        {
            self.pressure_critical = true;
            self.pressure = true;
            Some("pressure_critical")
        } else if growing
            && !self.pressure
            && hits.is_some_and(|h| (PRESSURE_MIN_HITS..=PRESSURE_HITS).contains(&h))
        {
            self.pressure = true;
            Some("pressure")
        } else if !self.half_hp && hp <= 0.5 {
            self.half_hp = true;
            Some("half_hp")
        } else {
            None
        }
    }

    /// A hit resolved: the checkpoint it raises, if any.
    fn on_hit(&mut self, attacker: Side, guarded: bool, deflected: bool) -> Option<&'static str> {
        match attacker {
            Side::Wolf if !guarded && !deflected => {
                self.punish_run += 1;
                if self.punish_run >= 2 && !self.punished_twice {
                    self.punished_twice = true;
                    return Some("punished_twice");
                }
            }
            Side::Boss => {
                let mut raised = None;
                if deflected {
                    self.deflect_run += 1;
                    if self.deflect_run >= 3 && !self.deflected_thrice {
                        self.deflected_thrice = true;
                        raised = Some("deflected_thrice");
                    }
                } else {
                    self.deflect_run = 0;
                }
                if !guarded && !deflected {
                    self.punish_run = 0;
                }
                return raised;
            }
            Side::Wolf => {}
        }
        None
    }
}

/// What is counted about the fight in progress.
#[derive(Debug, Clone, Default)]
struct FightState {
    punishes: super::moves::PunishMemory,
    contact_samples: Vec<(String, f32, f32)>,
    contact_ranges: BTreeMap<String, [[f32; 2]; 2]>,
    attack_learning: super::adaptation::AttackMemory,
    phase_changes: Vec<(f32, String)>,
    cost_at_start: super::cost::CostTotals,
    break_plans: CountMap,
    action_ranges: BTreeMap<String, (u32, f32, f32)>,
    uses: Vec<PlanUse>,
    failures: BTreeMap<String, PlanFailures>,
    /// Completed melee samples: count, sum of starting distances, sum of measured reaches.
    ranges: BTreeMap<String, (u32, f32, f32)>,
    range_open: Option<AttackRange>,
    metrics: MetricsTracker,
    requests: u32,
    decisions: u32,
    lua_acts: u32,
    choices: BTreeMap<String, u32>,
    /// Request to answer, milliseconds.
    latencies: Vec<f32>,
    tokens_at_start: (u64, u64),
    stats_at_start: [CallStats; 2],
    /// Plan runs `(start, plan text, set label)` in order, for attributing exchanges.
    runs: Vec<(f32, String, String)>,
    /// Outcomes per plan text and per set label this fight.
    by_plan: CountMap,
    by_set: BTreeMap<String, Counts>,
    /// Seconds each set was the active one.
    set_secs: BTreeMap<String, f32>,
}

struct PendingCheckpoint {
    phase: String,
    context: String,
    set: String,
    started: f32,
    metrics: super::metrics::FightMetrics,
    deflected_all: u32,
}

#[derive(Debug, Clone)]
struct PlanUse {
    break_response: bool,
    failed: bool,
    text: String,
    start: f32,
    last_attack: Option<f32>,
    attacks: u32,
    deflected: u32,
    fatal: bool,
    settled: bool,
}

#[derive(Debug, Clone)]
struct PunishUse {
    phase: String,
    bucket: String,
    id: String,
    window_end: f32,
    end: f32,
    throw_started: bool,
    outcome: super::moves::PunishEvidence,
}

#[derive(Debug, Clone)]
struct AttackRange {
    action: String,
    plan: String,
    start: [f32; 3],
    distance: f32,
    reach: Option<f32>,
    first_distance: Option<f32>,
    started: f32,
}

impl FightState {
    fn close_range(&mut self) {
        if let Some(sample) = self.range_open.take()
            && let Some(reach) = sample.reach
        {
            let (n, distance, total_reach) = self.ranges.entry(sample.plan).or_default();
            *n += 1;
            *distance += sample.distance;
            *total_reach += reach;
            let (n, distance, total_reach) = self.action_ranges.entry(sample.action).or_default();
            *n += 1;
            *distance += sample.distance;
            *total_reach += reach;
        }
    }
}

pub struct BrainDriver {
    sheets: BTreeMap<String, super::moves::MoveSheet>,
    relative: [f32; 2],
    wolf_velocity: [f32; 2],
    wolf_position: Option<[f32; 3]>,
    boss_position: Option<[f32; 3]>,
    boss_speed: f32,
    wolf_posture_left: f32,
    idle_secs: f32,
    contact_ranges: BTreeMap<String, [[f32; 2]; 2]>,
    overlay_events: OverlayEvents,
    session_records: Vec<FightRecord>,
    session_actions: CountMap,
    session_ranges: BTreeMap<String, (u32, f32, f32)>,
    session_report_path: Option<PathBuf>,
    break_pending: bool,
    cancel_break: bool,
    wolf_events: std::collections::VecDeque<&'static str>,
    banned: HashSet<String>,
    bans_at_start: HashSet<String>,
    retry_baselines: BTreeMap<String, PlanFailures>,
    brain: Box<dyn BossBrain>,
    actions: &'static BossActions,
    phase_plans: BTreeMap<String, GamePlan>,
    /// This fight's tally.
    pub tally: Tally,
    /// Wolf's habits in this fight.
    pub habits: HabitTracker,
    /// Every earlier fight (loaded from the memory file, plus the fights of this session).
    pub memory: Memory,
    pub boss_knowledge: BossKnowledge,
    pub player: String,
    layer_paths: Option<LayerPaths>,
    memory_path: Option<PathBuf>,
    last_save: f32,
    /// The last step's length, seconds.
    step_dt: f32,
    /// The fight ended (a death) and the respawn has not come yet.
    fight_over: bool,
    /// Fights started so far (an answer to a request of an earlier fight is dropped).
    fight_no: u32,
    pub stats: BrainStats,
    /// Seconds a single-shot answer may age before its plan runs; older plans are dropped.
    pub max_staleness: f32,
    /// Session clock, seconds.
    pub time: f32,
    /// When requests go out.
    pub mode: RequestMode,
    /// Sampling temperature (see [`super::sampler`]); `SEKIRO_BRAIN_TEMPERATURE` sets it.
    pub temperature: f32,
    rng: Rng,
    pub brain_moves: bool,
    pub brain_move_every: u32,
    pub coach_review_every: u32,
    move_fighter: super::move_bandit::Fighter,
    move_observation: super::observation::Observation,
    move_context: String,
    bandit_pending: Option<PendingCheckpoint>,
    fight_bandit: super::bandit::BanditMemory,
    bandit_choices: Vec<(String, String, f32, Option<f32>)>,
    bandit_fight_start: usize,
    streaks: Streaks,
    /// Plan texts chosen this fight.
    tried: HashSet<String>,
    in_flight: Option<InFlight>,
    /// The controller's latest answer, sampled for each act.
    set: Vec<Weighted>,
    /// When the set's request was sent.
    set_time: f32,
    /// A set stays for many picks (checkpoints) rather than one.
    persist: bool,
    ready: Option<Ready>,
    /// A pick (a plan or the Lua AI's act) is waiting for the next slot.
    picked: bool,
    /// `slot` and `plan_end`: a request is due.
    want_request: bool,
    /// `checkpoints`: the event that makes the next request due.
    pending_why: Option<&'static str>,
    checkpoints: Checkpoints,
    next_id: u64,
    /// The plan running now (empty: the Lua AI's own act).
    pub running: Plan,
    /// Every plan that started, in order, for tests.
    pub history: Vec<Plan>,
    /// Seconds since each side last landed an unguarded hit `[boss, wolf]`.
    since_hit: [Option<f32>; 2],
    pub move_sheet: super::moves::MoveSheet,
    pub punish_window: Option<f32>,
    break_windows: BTreeMap<i32, f32>,
    active_punish: Option<PunishUse>,
    boss_open: Option<i32>,
    wolf_open: Option<String>,
    /// A tactic's exchange is open in the tally.
    tactic_open: bool,
    idle_guard_secs: f32,
    guard_exchange: bool,
    stall_recovery: bool,
    fight: FightState,
    /// The record of the last finished fight, for tests and the summary.
    pub last_record: Option<FightRecord>,
    log: Option<BufWriter<File>>,
    log_path: Option<PathBuf>,
    /// The controller is a coach (see [`super::coach`]).
    coach_mode: bool,
    /// Apply the hard limits of [`super::limits`] when sampling plans (on for hosted brains).
    pub anti_stall: bool,
    /// Attack and retreat timing for those limits.
    pub pacing: Pacing,
    /// The game plan being played (coach mode).
    pub game_plan: Option<GamePlan>,
    /// The newest plan that arrived; played from the next fight on.
    next_plan: Option<GamePlan>,
    /// Index of the plan set being played.
    active_set: Option<usize>,
    /// When the active set became active.
    set_since: f32,
    /// The opener still to run this fight.
    opener: Option<Plan>,
    coach_flights: Vec<CoachFlight>,
    next_coach_id: u64,
    /// The id of the newest coach reply used, so an older one never replaces it.
    newest_coach: u64,
    /// Coach calls that completed since the fight began: calls, tokens, milliseconds.
    coach_done: [u64; 5],
    pub coach_interval: f32,
    last_coach_sent: f32,
    review_breaks: u32,
    mid_pending: Option<&'static str>,
    post_pending: Option<CoachInput>,
    mid_plan: Option<GamePlan>,
    staged_mid: Option<GamePlan>,
    mid_done: [u64; 2],
    set_banned_seen: bool,
}

/// Text and transient cards for the game's recording HUD.
#[derive(Debug, Default, Clone)]
pub struct BrainOverlay {
    pub countered: u32,
    pub cost: String,
    pub budget_notice: Option<String>,
    pub fight: u32,
    pub set: String,
    pub running: String,
    pub label: String,
    pub checkpoint: Option<String>,
    pub coach: Option<String>,
    pub thinking: bool,
    pub principles: Vec<String>,
    pub ban: Option<String>,
    pub scorecard: Option<String>,
}

#[derive(Default)]
struct OverlayEvents {
    fights_before: u32,
    step: usize,
    checkpoint: Option<(f32, String)>,
    coach: Option<(f32, String)>,
    ban: Option<(f32, String)>,
    scorecard: Option<(f32, String)>,
}

impl BrainDriver {
    fn action_evidence(&self, key: &str) -> Counts {
        let shared = self.memory.shared_actions.contains(key) || self.tally.shared.contains(key);
        let mut counts = if self.actions.chr == "c7110" && !shared {
            self.boss_knowledge
                .evidence
                .phase_actions
                .get("c7110")
                .and_then(|a| a.get(key))
                .or_else(|| {
                    self.memory
                        .phase_actions
                        .get("c7110")
                        .and_then(|a| a.get(key))
                })
                .copied()
                .unwrap_or_default()
        } else {
            self.boss_knowledge
                .evidence
                .actions
                .get(key)
                .or_else(|| self.memory.boss.get(key))
                .copied()
                .unwrap_or_default()
        };
        if !self.fight_over
            && let Some(c) = self
                .tally
                .phases
                .get(self.actions.chr)
                .and_then(|m| m.get(key))
        {
            counts.merge(c);
        }
        if shared && !self.fight_over {
            let other = if self.actions.chr == "c7110" {
                "c7100"
            } else {
                "c7110"
            };
            if let Some(c) = self.tally.phases.get(other).and_then(|a| a.get(key)) {
                counts.merge(c);
            }
        }
        counts
    }
    fn attack_evidence(&self, key: &str) -> super::adaptation::AttackEvidence {
        use super::adaptation::AttackEvidence;
        let phase = self.actions.chr;
        let personal_counts = if phase == "c7110" {
            self.memory
                .phase_actions
                .get(phase)
                .and_then(|m| m.get(key))
        } else {
            self.memory.boss.get(key)
        };
        let mut own = self
            .memory
            .attack_learning
            .get(phase)
            .and_then(|m| m.get(key))
            .copied()
            .unwrap_or_else(|| {
                AttackEvidence::legacy(personal_counts.copied().unwrap_or_default())
            });
        if !self.fight_over
            && self.fight.metrics.player_active(self.time)
            && let Some(now) = self
                .fight
                .attack_learning
                .get(phase)
                .and_then(|m| m.get(key))
        {
            own.append(*now);
        }
        let mut shared = AttackEvidence::default();
        for (player, snapshot) in &self.boss_knowledge.players {
            if player == &self.player {
                continue;
            }
            let counts = if phase == "c7110" {
                snapshot.phase_actions.get(phase).and_then(|m| m.get(key))
            } else {
                snapshot.actions.get(key)
            };
            let e = snapshot
                .attack_learning
                .get(phase)
                .and_then(|m| m.get(key))
                .copied()
                .unwrap_or_else(|| AttackEvidence::legacy(counts.copied().unwrap_or_default()));
            shared.pool(e);
        }
        own.borrow(shared);
        own
    }

    fn attack_factor(&self, id: i32) -> f32 {
        if !(3000..3100).contains(&id) {
            return 1.0;
        }
        self.attack_evidence(&id.to_string()).multiplier()
    }

    /// Rebinds the same session to the character installed by a map hand-over or reset.
    pub fn rebind(&mut self, duel: &Duel) {
        self.settle_bandit();
        let Some(actions) = BossActions::for_chr(&duel.spec.chr) else {
            return;
        };
        self.close_set_time();
        self.memory.coach.phase_bans.insert(
            self.actions.chr.into(),
            self.banned.iter().cloned().collect(),
        );
        self.banned = self
            .memory
            .coach
            .phase_bans
            .get(actions.chr)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(plan) = self.game_plan.take() {
            self.memory
                .coach
                .phase_plans
                .entry(self.actions.chr.into())
                .or_insert_with(|| plan.clone());
            self.phase_plans.insert(self.actions.chr.into(), plan);
        }
        self.sheets
            .insert(self.actions.chr.into(), self.move_sheet.clone());
        self.actions = actions;
        self.fight
            .phase_changes
            .push((self.time, actions.chr.into()));
        self.brain.change_phase(actions);
        self.tally
            .record(self.time, TallyEvent::End { side: Side::Boss });
        self.tally.phase = actions.chr.into();
        self.move_sheet = self
            .sheets
            .get(actions.chr)
            .cloned()
            .unwrap_or_else(|| super::moves::build(duel, actions));
        self.ready = None;
        self.set.clear();
        self.opener = None;
        self.active_set = None;
        self.next_plan = self
            .next_plan
            .take()
            .filter(|plan| plan.boss_phase.is_empty() || plan.boss_phase == actions.chr);
        self.mid_plan = None;
        self.staged_mid = None;
        self.running = Plan::default();
        self.picked = false;
        self.in_flight = None;
        self.want_request = true;
        self.pending_why = Some("mark_lost");
        self.boss_open = None;
        self.checkpoints.marks = Some(duel.rules.fighters[1].vitality.deathblows_left);
        if actions.chr == "c7110" && self.coach_mode {
            self.mid_pending = Some("phase_change");
        }
        if let Some(plan) = self
            .memory
            .coach
            .phase_plans
            .get(actions.chr)
            .or_else(|| self.phase_plans.get(actions.chr))
            .cloned()
        {
            self.game_plan = Some(plan.validate(actions).plan);
            self.install_plan();
        }
    }

    pub fn new(brain: Box<dyn BossBrain>, actions: &'static BossActions) -> Self {
        let env = |name: &str| std::env::var(name).ok();
        let coach_mode = brain.coaches();
        let anti_stall = brain.default_mode() == RequestMode::Checkpoints;
        let mode = if coach_mode {
            RequestMode::Checkpoints
        } else {
            env("SEKIRO_BRAIN_MODE")
                .and_then(|m| RequestMode::by_name(&m))
                .unwrap_or_else(|| brain.default_mode())
        };
        let temperature = env("SEKIRO_BRAIN_TEMPERATURE")
            .and_then(|t| t.parse().ok())
            .unwrap_or(1.0);
        let seed = env("SEKIRO_BRAIN_SEED")
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |d| d.as_nanos() as u64)
            });
        Self {
            overlay_events: OverlayEvents::default(),
            session_records: Vec::new(),
            session_actions: Default::default(),
            session_ranges: Default::default(),
            session_report_path: None,
            break_pending: false,
            cancel_break: false,
            wolf_events: Default::default(),
            banned: HashSet::new(),
            bans_at_start: HashSet::new(),
            retry_baselines: BTreeMap::new(),
            brain,
            actions,
            tally: Tally::default(),
            habits: HabitTracker::new(actions.back_steps),
            memory: Memory::new(if actions.chr == "c7110" {
                "c7100"
            } else {
                actions.chr
            }),
            boss_knowledge: BossKnowledge::new(if actions.chr == "c7110" {
                "c7100"
            } else {
                actions.chr
            }),
            player: "default".into(),
            layer_paths: None,
            memory_path: None,
            last_save: 0.0,
            step_dt: 0.0,
            fight_over: false,
            fight_no: 1,
            phase_plans: Default::default(),
            stats: BrainStats::default(),
            max_staleness: 6.0,
            time: 0.0,
            mode,
            temperature,
            rng: Rng::new(seed),
            bandit_pending: None,
            fight_bandit: Default::default(),
            brain_moves: true,
            brain_move_every: super::move_bandit::MOVE_EVERY,
            coach_review_every: super::move_bandit::REVIEW_EVERY,
            move_fighter: Default::default(),
            move_observation: Default::default(),
            move_context: String::new(),
            bandit_choices: Vec::new(),
            bandit_fight_start: 0,
            streaks: Streaks::default(),
            tried: HashSet::new(),
            in_flight: None,
            set: Vec::new(),
            set_time: 0.0,
            persist: false,
            ready: None,
            picked: false,
            want_request: true,
            pending_why: (!coach_mode).then_some("start"),
            checkpoints: Checkpoints::default(),
            next_id: 1,
            running: Plan::default(),
            history: Vec::new(),
            since_hit: [None, None],
            relative: [0.0; 2],
            sheets: Default::default(),
            wolf_velocity: [0.0; 2],
            wolf_position: None,
            boss_position: None,
            boss_speed: 0.0,
            wolf_posture_left: f32::MAX,
            idle_secs: 0.0,
            contact_ranges: Default::default(),
            move_sheet: Default::default(),
            punish_window: None,
            break_windows: Default::default(),
            active_punish: None,
            boss_open: None,
            wolf_open: None,
            tactic_open: false,
            idle_guard_secs: 0.0,
            guard_exchange: false,
            stall_recovery: false,
            fight: FightState {
                phase_changes: vec![(0.0, actions.chr.into())],
                ..FightState::default()
            },
            last_record: None,
            log: None,
            log_path: None,
            coach_mode,
            anti_stall,
            pacing: Pacing::new(0.0),
            game_plan: None,
            next_plan: None,
            active_set: None,
            set_since: 0.0,
            opener: None,
            coach_flights: Vec::new(),
            next_coach_id: 1,
            newest_coach: 0,
            coach_done: [0; 5],
            coach_interval: super::coach::MID_COACH_INTERVAL,
            last_coach_sent: 0.0,
            review_breaks: 0,
            mid_pending: None,
            post_pending: None,
            mid_plan: None,
            staged_mid: None,
            mid_done: [0; 2],
            set_banned_seen: false,
        }
    }

    pub fn brain_name(&self) -> &'static str {
        self.brain.name()
    }

    /// A read-only recording view, independent of logging and persistence.
    pub fn overlay(&self) -> BrainOverlay {
        let events = &self.overlay_events;
        let plan = self.memory.coach.plan.as_ref().or(self.game_plan.as_ref());
        let set = self
            .game_plan
            .as_ref()
            .and_then(|p| p.sets.get(self.active_set?));
        let step = self.running.steps.get(events.step);
        let label = match step {
            Some(Step::Act(id)) => self
                .actions
                .actions
                .iter()
                .find(|a| a.id == self.boss_open.unwrap_or(*id))
                .or_else(|| self.actions.actions.iter().find(|a| a.id == *id))
                .map(|a| a.label.to_string())
                .unwrap_or_default(),
            Some(Step::Tactic(t)) => t.label(),
            None => self
                .boss_open
                .and_then(|id| self.actions.actions.iter().find(|a| a.id == id))
                .map(|a| a.label.to_string())
                .unwrap_or_default(),
        };
        let running = if self.running.is_empty() {
            "Lua".into()
        } else {
            self.running
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let text = match s {
                        Step::Act(id) => id.to_string(),
                        Step::Tactic(t) => t.token(),
                    };
                    if i == events.step {
                        format!("[{text}]")
                    } else {
                        text
                    }
                })
                .collect::<Vec<_>>()
                .join(" > ")
        };
        let recent = |event: &Option<(f32, String)>, seconds: f32| {
            event
                .as_ref()
                .filter(|(t, _)| self.time - t < seconds)
                .map(|(_, text)| text.clone())
        };
        BrainOverlay {
            cost: self.brain.costs().totals.line("session"),
            budget_notice: self.brain.costs().budget_notice,
            fight: self.overlay_events.fights_before + self.fight_no,
            countered: self.fight.metrics.metrics.countered,
            set: set
                .map(|s| format!("{}: {}", s.name, s.description))
                .unwrap_or_else(|| {
                    if self.brain.name() == "scripted" {
                        "scripted: deterministic test plans".into()
                    } else if self.set.is_empty() {
                        "Lua".into()
                    } else {
                        "Controller plan set".into()
                    }
                }),
            running,
            label: self
                .move_fighter
                .display
                .as_ref()
                .map_or(label.clone(), |m| format!("{label}\nMOVE {m}")),
            checkpoint: events
                .checkpoint
                .as_ref()
                .map(|(t, text)| format!("{text} · {:.1} s ago", self.time - t)),
            coach: recent(&events.coach, 8.0),
            thinking: !self.coach_flights.is_empty(),
            principles: plan
                .map(|p| {
                    p.boss_principles
                        .iter()
                        .map(|s| format!("Boss · {s}"))
                        .chain(p.principles.iter().map(|s| format!("Player · {s}")))
                        .collect()
                })
                .unwrap_or_default(),
            ban: recent(&events.ban, 5.0),
            scorecard: recent(&events.scorecard, 6.0),
        }
    }

    pub fn actions(&self) -> &'static BossActions {
        self.actions
    }

    /// Seeds the sampler (tests; `SEKIRO_BRAIN_SEED` otherwise).
    pub fn seed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    /// Remembers across sessions in `path` (`memory::default_path` is
    /// `logs/memory/<chr>.json`): loads it now (a missing or unusable file starts fresh, with a
    /// line on stderr) and saves to it from then on.
    pub fn use_memory(&mut self, path: PathBuf) -> Loaded {
        self.use_player_memory(path, "default")
    }

    pub fn use_player_memory(&mut self, path: PathBuf, player: &str) -> Loaded {
        let chr = self.memory.chr.clone();
        if let Err(e) = super::memory::player_name(player) {
            return Loaded::Corrupt(e);
        }
        let paths = LayerPaths::new(&path, player);
        let existed = paths.player.exists();
        let (boss, memory) = match paths.load(&chr) {
            Ok(layers) => layers,
            Err(e) => {
                eprintln!("warning: memory layers: {e}; persistence disabled");
                return Loaded::Corrupt(e.to_string());
            }
        };
        let how = if existed || memory.fights > 0 {
            Loaded::Read
        } else {
            Loaded::Missing
        };
        self.player = player.into();
        self.boss_knowledge = boss;
        self.layer_paths = Some(paths);
        eprintln!(
            "boss knowledge: {} fights from {} players; player profile: {} ({} fights)",
            self.boss_knowledge.evidence.fights,
            self.boss_knowledge.summary().players,
            player,
            memory.fights
        );
        match &how {
            Loaded::Read => eprintln!(
                "brain memory: {} earlier fights against {chr} from {}",
                memory.fights,
                path.display()
            ),
            Loaded::Missing => eprintln!(
                "brain memory: none yet at {}, starting fresh",
                path.display()
            ),
            Loaded::Corrupt(why) => eprintln!(
                "warning: brain memory {} is unusable ({why}); starting fresh, the old file is kept as {}.corrupt",
                path.display(),
                path.display()
            ),
        }
        self.memory = memory;
        self.banned.extend(
            self.memory
                .coach
                .phase_bans
                .get(self.actions.chr)
                .cloned()
                .unwrap_or_default(),
        );
        self.overlay_events.fights_before = self.memory.fights;
        // Old phase_bans included automatic evidence exclusions. Preserve coach judgements.
        self.banned.retain(|choice| {
            self.memory
                .coach
                .phase_plans
                .get(self.actions.chr)
                .or(self.memory.coach.plan.as_ref())
                .is_some_and(|plan| plan.avoid.iter().any(|avoid| &avoid.choice == choice))
        });
        self.apply_boss_bans();
        self.bans_at_start = self.banned.clone();
        self.memory_path = Some(path);
        if self.coach_mode {
            // The newest plan of an earlier session plays from the first fight (validated
            // again: the file may be from another build).
            let saved = self
                .memory
                .coach
                .phase_plans
                .get(self.actions.chr)
                .cloned()
                .or_else(|| {
                    self.memory
                        .coach
                        .plan
                        .take()
                        .filter(|p| p.boss_phase.is_empty() || p.boss_phase == self.actions.chr)
                        .or_else(|| self.boss_knowledge.starter())
                });
            if let Some(saved) = saved {
                let v = saved.validate(self.actions);
                for w in &v.warnings {
                    eprintln!("coach: saved game plan: {w}");
                }
                if v.usable() {
                    self.overlay_events.coach = Some((
                        self.time,
                        format!("coach (pre-fight, 0 s): {}", v.plan.notes),
                    ));
                    self.memory.coach.plan = Some(v.plan.clone());
                    self.memory
                        .coach
                        .phase_plans
                        .insert(self.actions.chr.into(), v.plan.clone());
                    self.game_plan = Some(v.plan);
                }
            }
            self.install_plan();
            if self.game_plan.is_some() {
                eprintln!(
                    "coach: playing the saved game plan from fight {} on",
                    self.memory.fights + 1
                );
            }
        }
        how
    }

    /// Coach mode: sets up the fight that is starting from [`Self::game_plan`]: the opener, the
    /// start set, and the `start` checkpoint (with no plan nothing is requested and his own AI
    /// plays).
    fn install_plan(&mut self) {
        self.opener = None;
        self.active_set = None;
        self.set.clear();
        self.pending_why = None;
        let Some(plan) = self.game_plan.clone() else {
            return;
        };
        self.persist = true;
        self.opener = plan.opener_plan(self.actions);
        if let Some(i) = plan.start_index() {
            self.active_set = Some(i);
            self.set = plan.weighted(i, self.actions);
            self.set_since = self.time;
            self.pending_why = Some("start");
        }
    }

    /// Name of the plan set being played (coach mode).
    pub fn active_set_name(&self) -> Option<&str> {
        let plan = self.game_plan.as_ref()?;
        Some(plan.sets.get(self.active_set?)?.name.as_str())
    }

    /// Switches to the named set (coach mode); the next act is sampled from it. An opener
    /// still waiting stays.
    fn switch_set(&mut self, name: &str) -> bool {
        let Some(plan) = self.game_plan.as_ref() else {
            return false;
        };
        let Some(i) = plan.set_index(name) else {
            return false;
        };
        if self.active_set == Some(i) {
            return true;
        }
        let set = plan.weighted(i, self.actions);
        self.close_set_time();
        self.active_set = Some(i);
        self.set = set;
        if self
            .ready
            .as_ref()
            .is_none_or(|r| !r.opener && r.label != "on_wolf_break")
        {
            self.ready = None;
            self.picked = false;
        }
        true
    }

    /// Adds the time the active set has been played to this fight's seconds.
    fn close_set_time(&mut self) {
        if let Some(name) = self.active_set_name().map(str::to_string) {
            let name = if self.actions.chr == "c7110" {
                format!("c7110:{name}")
            } else {
                name
            };
            *self.fight.set_secs.entry(name).or_default() += (self.time - self.set_since).max(0.0);
        }
        self.set_since = self.time;
    }

    pub fn memory_path(&self) -> Option<&Path> {
        self.memory_path.as_deref()
    }

    fn apply_boss_bans(&mut self) {
        for ban in &self.boss_knowledge.bans {
            if ban.reason.starts_with("shared: ") {
                continue;
            }
            if self.actions.chr == "c7110" && !self.memory.shared_actions.contains(&ban.choice) {
                continue;
            }
            if self.banned.insert(ban.choice.clone()) {
                self.overlay_events.ban = Some((
                    self.time,
                    format!(
                        "banned {}: {}",
                        ban.choice,
                        ban.reason.trim_start_matches("shared: ")
                    ),
                ));
            }
            for choice in self.actions.choices() {
                if choice
                    .value
                    .split(['>', '+'])
                    .any(|step| step == ban.choice)
                {
                    self.banned.insert(choice.value);
                }
            }
        }
    }

    /// Enable exit/F9 reports in the game; offline tests do not write reports.
    pub fn enable_session_report(&mut self, logs: &Path) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        self.session_report_path = Some(logs.join(format!("brain-session-{stamp}.md")));
    }

    pub fn session_report(&self) -> String {
        let mut actions = self.session_actions.clone();
        let mut ranges = self.session_ranges.clone();
        if !self.fight_over && self.fight.metrics.player_active(self.time) {
            let (tally, _) = self.fight_snapshot();
            merge_counts(&mut actions, tally.all(Side::Boss));
            for (key, &(n, distance, reach)) in &self.fight.action_ranges {
                let entry = ranges.entry(key.clone()).or_default();
                entry.0 += n;
                entry.1 += distance;
                entry.2 += reach;
            }
        }
        let mut report = format!(
            "# Brain session: {}\n\n| action | n | hit | blocked | deflected | whiffed | punished | countered | distance / reach (m) | first-hit hit / miss (m) |\n|---|---:|---:|---:|---:|---:|---:|---:|---|---|\n",
            self.actions.chr
        );
        for (key, c) in actions {
            let range = ranges.get(&key).map_or("-".into(), |&(n, d, r)| {
                format!("{:.2} / {:.2}", d / n as f32, r / n as f32)
            });
            let contact = self
                .contact_ranges
                .get(&format!("{}:{key}", self.actions.chr));
            let mean = |i: usize| {
                contact
                    .filter(|r| r[i][0] > 0.0)
                    .map_or("-".into(), |r| format!("{:.2}", r[i][1] / r[i][0]))
            };
            report.push_str(&format!(
                "| {key} | {} | {} | {} | {} | {} | {} | {} | {range} | {} / {} |\n",
                c.n,
                c.hit,
                c.blocked,
                c.deflected,
                c.whiffed,
                c.punished,
                c.countered,
                mean(0),
                mean(1)
            ));
        }
        report.push_str("\n| fight | winner | duration (s) | time to kill Wolf (s) | Wolf posture breaks | hits after break | break punish rate | Wolf posture peak | Wolf damage/min | boss damage taken/min | passive share | Wolf kill time (s) |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
        for (i, r) in self.session_records.iter().enumerate() {
            let m = r.metrics;
            report.push_str(&format!(
                "| {} | {} | {:.1} | {} | {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {} |\n",
                i + 1,
                r.winner,
                m.duration,
                m.time_to_kill_wolf
                    .map_or("-".into(), |t| format!("{t:.1}")),
                m.wolf_posture_breaks,
                m.hits_after_break,
                m.break_punish_rate * 100.0,
                m.wolf_posture_peak * 100.0,
                m.damage_per_min(),
                m.boss_damage_taken_per_min(),
                m.passive_share() * 100.0,
                m.kill_time.map_or("-".into(), |t| format!("{t:.1}"))
            ));
        }
        if !self.fight_over && self.fight.metrics.player_active(self.time) {
            let m = self.fight.metrics.snapshot(self.time);
            report.push_str(&format!(
                "| current | unfinished | {:.1} | - | {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | - |\n",
                m.duration,
                m.wolf_posture_breaks,
                m.hits_after_break,
                m.break_punish_rate * 100.0,
                m.wolf_posture_peak * 100.0, m.damage_per_min(), m.boss_damage_taken_per_min(), m.passive_share() * 100.0
            ));
        }
        report.push_str("\n| break action / hit | breaks | converted |\n|---|---:|---:|\n");
        let mut breaks: Vec<_> = self
            .session_records
            .iter()
            .flat_map(|r| r.breaks.clone())
            .collect();
        if !self.fight_over && self.fight.metrics.player_active(self.time) {
            breaks.extend(self.fight.metrics.breaks.clone());
        }
        for (key, counts) in super::metrics::break_summary(&breaks) {
            report.push_str(&format!(
                "| {key} | {} | {} / {} |\n",
                counts[0], counts[1], counts[0]
            ));
        }
        report.push_str("\n| action | hit | state | wait (s) | on_wolf_break | start delay (s) | outcome |\n|---|---|---|---:|---|---:|---|\n");
        for r in breaks {
            report.push_str(&format!(
                "| {} | {}/{} | {} | {:.3} | {} | {} | {} |\n",
                r.action,
                r.hit,
                r.hits,
                r.state,
                r.wait,
                r.started,
                r.start_delay.map_or("-".into(), |d| format!("{d:.3}")),
                r.outcome
            ));
        }
        for (i, r) in self.session_records.iter().enumerate() {
            report.push_str(&format!(
                "\nFight {} break_conversion_delay: {} s.\n",
                i + 1,
                r.metrics
                    .break_conversion_delay
                    .map_or("-".into(), |d| format!("{d:.3}"))
            ));
        }
        report.push_str("\n## Per-fight attacks\n\n| fight | phase | action | n | hit | blocked | deflected | whiffed | punished | Mikiri | jump | other |\n|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
        for (fight, record) in self.session_records.iter().enumerate() {
            for (phase, attacks) in &record.attacks {
                for (action, c) in attacks {
                    report.push_str(&format!("| {} | {phase} | {action} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                        fight + 1, c.n, c.hit, c.blocked, c.deflected, c.whiffed, c.punished,
                        c.counter_kinds[0], c.counter_kinds[1], c.counter_kinds[2]));
                }
            }
        }
        let costs = self.brain.costs();
        report.push_str(&format!("\n## Cost\n\n{}\n\n| kind | model | calls | input | cached | output | reasoning | plan tokens (proxy) | API tokens | billed USD |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n", costs.totals.line("session")));
        for row in costs.rows {
            let t = row.totals;
            report.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                row.kind.replace('|', "/"),
                row.model.replace('|', "/"),
                t.calls,
                t.input,
                t.cached,
                t.output,
                t.reasoning,
                t.plan_tokens,
                t.api_tokens,
                if t.api_tokens > 0 || t.unknown_billed > 0 {
                    super::cost::usd(t.billed_usd, t.unknown_billed)
                } else {
                    "-".into()
                }
            ));
        }
        if let Some(notice) = costs.budget_notice {
            report.push_str(&format!("\n{notice}\n"));
        }
        report.push_str("\n## Local checkpoints\n\n| context | set | posterior mean | reward |\n|---|---|---:|---:|\n");
        for (context, set, mean, reward) in &self.bandit_choices {
            report.push_str(&format!(
                "| {context} | {set} | {mean:.3} | {} |\n",
                reward.map_or_else(|| "pending".into(), |r| format!("{r:.3}"))
            ));
        }
        report
    }

    pub fn print_session_report(&self) -> std::io::Result<()> {
        let report = self.session_report();
        eprintln!("{report}");
        if let Some(path) = &self.session_report_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            file.write_all(report.as_bytes())?;
        }
        Ok(())
    }

    /// The memory as it would be saved now: every earlier fight, plus this one as unfinished
    /// if anything happened in it.
    pub fn memory_now(&self) -> Memory {
        let mut m = self.memory.clone();
        if !self.fight_over && !self.tally.is_empty() && self.fight.metrics.player_active(self.time)
        {
            super::moves::merge_punishes(&mut m.punishes, &self.fight.punishes);
            let (tally, habits) = self.fight_snapshot();
            m.add_fight(&tally, &habits.habits, FightEnd::Unfinished);
            super::adaptation::append(&mut m.attack_learning, &self.fight.attack_learning);
            super::bandit::decay(&mut m.move_bandit);
            let evidence = self
                .move_fighter
                .snapshot(self.fight.metrics.snapshot(self.time).stalled());
            super::bandit::merge(&mut m.move_bandit, &evidence, 1.0);
            for (key, n) in &self.move_fighter.progress {
                let entry = m.coach.move_progress.entry(key.clone()).or_default();
                entry[0] += n[0];
                entry[1] += n[1];
            }
            for (key, samples) in &self.fight.contact_ranges {
                let entry = m.coach.contact_ranges.entry(key.clone()).or_default();
                for i in 0..2 {
                    for j in 0..2 {
                        entry[i][j] += samples[i][j];
                    }
                }
            }
        }
        m
    }

    /// Saves [`Self::memory_now`] to the memory file, if there is one.
    pub fn save_memory(&mut self) {
        self.last_save = self.time;
        let Some(path) = self.memory_path.clone() else {
            return;
        };
        let now = self.memory_now();
        self.boss_knowledge.update(&self.player, &self.memory);
        self.apply_boss_bans();
        let saved = self.layer_paths.as_ref().map_or_else(
            || now.save(&path).map(|()| self.boss_knowledge.clone()),
            |p| p.save(&self.boss_knowledge, &now),
        );
        match saved {
            Ok(boss) => {
                self.boss_knowledge = boss;
                self.apply_boss_bans();
            }
            Err(e) => {
                self.stats.save_errors += 1;
                eprintln!("brain memory: saving {} failed: {e}", path.display());
            }
        }
    }

    /// What a request carries: every fight so far, this one included (the fight count and
    /// wins are the earlier fights'), and Wolf's recent play.
    pub fn memory_summary(&self) -> MemorySummary {
        let mut m = self.memory.clone();
        if !self.fight_over && self.fight.metrics.player_active(self.time) {
            let (tally, habits) = self.fight_snapshot();
            m.add_data(&tally, &habits.habits);
        }
        let now = (!self.fight_over).then(|| self.fight.metrics.snapshot(self.time));
        let mut summary = m.summary_with(now.as_ref());
        if self.actions.chr == "c7110" {
            summary.boss =
                ranked_counts(&m.phase_actions.get("c7110").cloned().unwrap_or_default());
        }
        summary
    }

    /// Copies of this fight's tally and habits with everything still open settled.
    fn fight_snapshot(&self) -> (Tally, HabitTracker) {
        let mut tally = self.tally.clone();
        tally.flush(self.time);
        let mut habits = self.habits.clone();
        habits.flush(self.time);
        (tally, habits)
    }

    /// Writes a JSONL log of requests, replies, slots and outcomes into `dir` (created if
    /// missing), one file per session; returns its path.
    pub fn log_to(&mut self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let path = dir.join(format!(
            "{stamp}-{}-{}.jsonl",
            self.actions.chr,
            self.brain.name()
        ));
        self.log = Some(BufWriter::new(File::create(&path)?));
        self.log_path = Some(path.clone());
        let choices: Vec<_> = self
            .actions
            .choices()
            .iter()
            .map(|c| json!({"value": c.value, "label": c.label}))
            .collect();
        self.write(json!({
            "ev": "start",
            "brain": self.brain.name(),
            "mode": self.mode.name(),
            "chr": self.actions.chr,
            "max_plan": self.actions.max_plan,
            "choices": choices,
        }));
        Ok(path)
    }

    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    fn write(&mut self, line: serde_json::Value) {
        self.write_line(&line);
    }

    /// Writes any serializable line (serialized directly, so `f32` fields stay short).
    fn write_line<T: Serialize>(&mut self, line: &T) {
        if let Some(w) = self.log.as_mut() {
            let ok = serde_json::to_writer(&mut *w, line).is_ok()
                && w.write_all(b"\n").is_ok()
                && w.flush().is_ok();
            if !ok {
                eprintln!("brain log: write failed, logging stops");
                self.log = None;
            }
        }
    }

    /// Advances the clock; call once at the start of every duel step.
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        self.step_dt = dt;
        for s in self.since_hit.iter_mut().flatten() {
            *s += dt;
        }
        if self.coach_mode {
            while let Some(reply) = self.brain.poll_coach(self.time) {
                self.take_coach_reply(reply);
            }
            if self.coach_flights.is_empty()
                && let Some(input) = self.post_pending.take()
            {
                self.submit_coach_input(input, false);
            }
        }
    }

    /// A coach answer arrived (at any time, also during a respawn): a usable plan waits for the
    /// next fight to start; a failure or a plan with nothing valid keeps the previous one.
    fn take_coach_reply(&mut self, reply: CoachReply) {
        let Some(at) = self.coach_flights.iter().position(|f| f.id == reply.id) else {
            return;
        };
        let flight = self.coach_flights.remove(at);
        if flight.mid && flight.fight == self.fight_no {
            self.mid_done[1] += u64::from(reply.latency_ms);
        }
        let waited = self.time - flight.sent;
        self.coach_done[0] += 1;
        for (i, t) in reply.tokens.iter().enumerate() {
            self.coach_done[1 + i] += t;
        }
        self.coach_done[4] += u64::from(reply.latency_ms);
        let tokens = format!(
            "tokens {} in / {} out, {} reasoning, {:.1} s",
            reply.tokens[0],
            reply.tokens[1],
            reply.tokens[2],
            f64::from(reply.latency_ms) / 1000.0
        );
        let Some(raw) = reply.plan else {
            eprintln!(
                "coach: no game plan ({}); keeping the previous plan ({tokens})",
                reply.error.as_deref().unwrap_or("failed")
            );
            self.write(
                json!({"ev": "coach_failed", "t": ms(self.time), "id": reply.id,
                "error": reply.error, "waited": ms(waited)}),
            );
            return;
        };
        if !raw.boss_phase.is_empty() && raw.boss_phase != self.actions.chr {
            return;
        }
        let v = raw.validate(self.actions);
        for w in &v.warnings {
            eprintln!("coach: warning: {w}");
        }
        if !v.usable() {
            eprintln!(
                "coach: the game plan had nothing valid; keeping the previous plan ({tokens})"
            );
            self.write(
                json!({"ev": "coach_failed", "t": ms(self.time), "id": reply.id,
                "error": "nothing valid", "warnings": v.warnings}),
            );
            return;
        }
        if reply.id < self.newest_coach {
            return;
        }
        self.newest_coach = reply.id;
        self.overlay_events.coach = Some((
            self.time,
            format!(
                "coach ({}, {:.0} s): {}",
                if flight.mid {
                    "mid-fight"
                } else {
                    "post-fight"
                },
                waited,
                v.plan.notes
            ),
        ));
        self.boss_knowledge.learn(&v.plan);
        for retry in &v.plan.retry {
            if self
                .boss_knowledge
                .bans
                .iter()
                .any(|b| b.choice == retry.choice)
            {
                continue;
            }
            self.banned.remove(&retry.choice);
            self.bans_at_start.remove(&retry.choice);
            // New evidence is evaluated against new uses after the explicit retry.
            self.retry_baselines
                .insert(retry.choice.clone(), self.plan_failures(&retry.choice));
            let response = format!("on_wolf_break:{}", retry.choice);
            self.banned.remove(&response);
            self.bans_at_start.remove(&response);
            self.retry_baselines
                .insert(response.clone(), self.plan_failures(&response));
        }
        let application = if flight.mid && flight.fight == self.fight_no && !self.fight_over {
            "next decision slot"
        } else {
            "next fight"
        };
        eprintln!(
            "coach: game plan arrived after {waited:.1} s, applied at {application} ({tokens})"
        );
        for line in v.plan.summary_lines() {
            eprintln!("  {line}");
        }
        self.write(
            json!({"ev": "coach_plan", "t": ms(self.time), "id": reply.id,
            "waited": ms(waited), "tokens": reply.tokens, "plan": v.plan,
            "warnings": v.warnings}),
        );
        self.memory.coach.plan = Some(v.plan.clone());
        super::digest::remember(&mut self.memory.coach.lessons, &v.plan.lessons);
        self.memory
            .coach
            .phase_plans
            .insert(self.actions.chr.into(), v.plan.clone());
        self.apply_boss_bans();
        if flight.mid && flight.fight == self.fight_no && !self.fight_over {
            self.mid_plan = Some(v.plan);
        } else {
            self.next_plan = Some(v.plan);
        }
        self.save_memory();
    }

    /// Before the Lua AI thinks: installs the act hook, collects an answer, picks the plan for
    /// the next slot and drops a stale one.
    pub fn before_think(&mut self, ai: &mut AiBrain, hit_radius: f32) {
        ai.enable_act_hook(self.actions.reflex_acts);
        if let Some(hook) = ai.state.act_hook.as_mut() {
            hook.reachable =
                (!self.brain.fixed_plans() && !self.move_sheet.is_empty()).then(|| {
                    self.move_sheet
                        .iter()
                        .filter_map(|(key, row)| {
                            let id = key.parse::<i32>().ok()?;
                            (!row.contact || row.fits(self.relative, self.wolf_velocity))
                                .then_some(id)
                        })
                        .collect()
                });
        }
        if self.brain.plans()
            && let Some(hook) = ai.state.act_hook.as_mut()
        {
            hook.counter_weights = self
                .actions
                .counters()
                .iter()
                .map(|(index, ids)| {
                    let factor = ids
                        .iter()
                        .map(|id| {
                            let key = id.to_string();
                            let avoided = self.banned.contains(&key)
                                || self.game_plan.as_ref().is_some_and(|p| {
                                    p.avoid.iter().any(|avoid| avoid.choice == key)
                                });
                            let planned = if avoided {
                                0.0
                            } else {
                                self.game_plan
                                    .as_ref()
                                    .and_then(|p| p.counter_weights.get(&key))
                                    .copied()
                                    .unwrap_or(1.0)
                            };
                            f64::from(self.attack_factor(*id) * planned)
                        })
                        .reduce(f64::min)
                        .unwrap_or(1.0);
                    (*index, factor)
                })
                .collect();
        }
        if self.mid_plan.is_some() && !self.fight_over {
            self.ready = None;
            self.picked = false;
            if let Some(hook) = ai.state.act_hook.as_mut() {
                hook.choice = None;
            }
        }
        if self.cancel_break {
            self.cancel_break = false;
            if let Some(hook) = ai.state.act_hook.as_mut() {
                if hook.break_followup.take().is_some() {
                    hook.choice = None;
                }
                hook.break_response = false;
            }
            if self
                .ready
                .as_ref()
                .is_some_and(|r| r.label == "on_wolf_break")
            {
                self.ready = None;
                self.picked = false;
            }
        }
        if let Some(reply) = self.brain.poll(self.time) {
            self.take_reply(reply);
        }
        if self.fight_over {
            if let Some(hook) = ai.state.act_hook.as_mut() {
                hook.break_followup = None;
            }
            self.ready = None;
        }
        // A plan that is no longer waiting (a new fight began) must not run.
        if self.ready.is_none()
            && let Some(hook) = ai.state.act_hook.as_mut()
        {
            hook.choice = None;
            if hook.break_followup.is_none() {
                hook.break_response = false;
            }
        }
        if self.fight_over {
            return;
        }
        if self.anti_stall
            && !self.stall_recovery
            && self.idle_guard_secs >= 0.5
            && self.time - self.pacing.last_attack >= limits::MAX_PASSIVE_SECS
        {
            // A guard goal or an unstartable plan can keep Lua from opening another slot.
            // Replan through the existing hook, without interrupting a live Wolf attack.
            ai.state.replan = true;
            self.stall_recovery = true;
            self.ready = None;
            self.picked = false;
        }
        if self.break_pending {
            self.break_pending = false;
            let mut choices: Vec<_> = self
                .game_plan
                .as_ref()
                .map(|p| p.on_wolf_break.as_slice())
                .unwrap_or_default()
                .iter()
                .filter(|w| {
                    !self.banned.contains(&w.plan)
                        && !self.banned.contains(&format!("on_wolf_break:{}", w.plan))
                })
                .filter_map(|w| {
                    let plan = self.actions.parse(&w.plan).ok()?;
                    if self.brain_moves
                        && self.anti_stall
                        && !self.pacing.eligible(&plan, self.time, self.actions)
                    {
                        return None;
                    }
                    let factor = if self.brain_moves {
                        1.0
                    } else {
                        super::adaptation::plan_multiplier(&plan, |id| self.attack_factor(id))
                    };
                    Some(Weighted {
                        plan: Some(plan),
                        weight: w.weight * factor,
                    })
                })
                .collect();
            if !self.move_sheet.is_empty() && self.brain.plans() && !self.brain.fixed_plans() {
                choices = self
                    .move_sheet
                    .iter()
                    .filter_map(|(id, row)| {
                        let p = self.actions.parse(id).ok()?;
                        if !row.damaging() {
                            return None;
                        }
                        Some(Weighted {
                            plan: Some(p),
                            weight: 1.0,
                        })
                    })
                    .collect();
            }
            let approach = if choices.is_empty() && self.brain.plans() && !self.brain.fixed_plans()
            {
                self.approach_plan()
            } else {
                None
            };
            if let Some(plan) = approach.as_ref() {
                choices.push(Weighted {
                    plan: Some(plan.clone()),
                    weight: 1.0,
                });
            }
            let pick = if approach.is_some() {
                Pick::Plan(choices.len() - 1)
            } else {
                self.pick_move(&choices, true)
            };
            if pick == Pick::Nothing
                && self
                    .fight
                    .metrics
                    .breaks
                    .last()
                    .is_some_and(|r| self.time - r.time < 2.5)
            {
                self.break_pending = true;
            }
            if let Pick::Plan(i) = pick
                && let Some(plan) = choices[i].plan.clone()
                && let Ok(mut choice) = self.actions.act_for_plan(&plan, hit_radius)
                && let Some(hook) = ai.state.act_hook.as_mut()
            {
                // A break response is a follow-up: request it through the existing combo
                // cancel path, preserving the animation's own TAE windows.
                if let Some(first) = choice.goals.first_mut()
                    && first.goal == "GOAL_COMMON_AttackTunableSpin"
                {
                    first.goal = "GOAL_COMMON_ComboFinal";
                    first.params.truncate(6);
                }
                hook.break_followup = Some(choice.clone());
                // Keep a queued response through long data-defined recoveries as well.
                let wait = self.fight.metrics.breaks.last().map_or(0.0, |r| r.wait);
                hook.break_until = ai.state.time + 2.5_f32.max(wait + 0.5);
                hook.choice = Some(choice);
                hook.break_response = true;
                self.ready = Some(Ready {
                    since: self.time,
                    plan,
                    opener: false,
                    label: "on_wolf_break".into(),
                });
                self.picked = true;
            }
        }
        let stale = self.ready.as_ref().is_some_and(|r| {
            self.time - r.since > self.max_staleness || self.banned.contains(&r.plan.value())
        });
        if stale {
            let ready = self.ready.take();
            if let Some(r) = ready.as_ref().filter(|r| r.opener) {
                // The opener is not lost: it runs at the next slot.
                self.opener = Some(r.plan.clone());
            }
            let plan = ready.map(|r| r.plan);
            if let Some(hook) = ai.state.act_hook.as_mut() {
                hook.choice = None;
                hook.break_response = false;
            }
            self.picked = false;
            if !self.persist {
                self.stats.stale += 1;
                self.want_request = true;
                self.write(json!({"ev": "stale", "t": ms(self.time),
                    "plan": plan.map(|p| p.value())}));
            }
        }
        if ai
            .state
            .act_hook
            .as_ref()
            .is_some_and(|h| h.break_followup.is_some())
        {
            self.picked = true;
        }
        if !self.picked
            && (!self.set.is_empty()
                || self.opener.is_some()
                || self.mid_plan.is_some()
                || self.staged_mid.is_some()
                || (self.anti_stall
                    && self.time - self.pacing.last_attack >= limits::MAX_PASSIVE_SECS))
        {
            self.pick_next(ai, hit_radius);
        }
    }

    /// An answer arrived: it becomes the plan set.
    fn take_reply(&mut self, reply: BrainReply) {
        let sent = match self.in_flight {
            Some(f) if f.id == reply.id => {
                self.in_flight = None;
                f
            }
            _ => {
                // An answer to a request we no longer wait for.
                if !self.fight_over {
                    self.write(json!({"ev": "late", "t": ms(self.time), "id": reply.id}));
                }
                return;
            }
        };
        if self.fight_over || sent.fight != self.fight_no {
            return;
        }
        self.stats.replies += 1;
        let latency = (self.time - sent.sent) * 1000.0;
        self.fight.latencies.push(latency);
        if self.coach_mode {
            // The chooser's answer: a set name, or `keep`. A failed call keeps the set too.
            let chosen = reply.choice.as_deref().unwrap_or(KEEP);
            let previous = self.active_set;
            let switched = chosen != KEEP && self.switch_set(chosen);
            if let Some((time, checkpoint)) = self.overlay_events.checkpoint.as_mut() {
                *time = self.time;
                *checkpoint = format!(
                    "{} -> {}",
                    checkpoint.split(" -> ").next().unwrap_or("checkpoint"),
                    if switched && previous != self.active_set {
                        format!("set {chosen}")
                    } else {
                        "keep".into()
                    }
                );
            }
            self.write(json!({
                "ev": "reply", "t": ms(self.time), "id": reply.id, "choice": chosen,
                "switched": switched, "set": self.active_set_name(),
                "latency": ms(latency / 1000.0),
            }));
            self.want_request = false;
            return;
        }
        let mut set = if reply.set.is_empty() {
            vec![Weighted {
                plan: reply.plan.clone(),
                weight: 1.0,
            }]
        } else {
            reply.set
        };
        let before = set.len();
        set.retain(|w| {
            w.plan
                .as_ref()
                .is_none_or(|p| self.actions.act_for_plan(p, 0.0).is_ok())
        });
        self.stats.invalid += (before - set.len()) as u32;
        self.write(json!({
            "ev": "reply", "t": ms(self.time), "id": reply.id,
            "plan": reply.plan.as_ref().map(Plan::value),
            "options": set.len(), "latency": ms(latency / 1000.0),
        }));
        self.persist = self.mode == RequestMode::Checkpoints;
        if let Some((time, checkpoint)) = self.overlay_events.checkpoint.as_mut() {
            *time = self.time;
            *checkpoint = format!(
                "{} -> plan set ({} choices)",
                checkpoint.split(" -> ").next().unwrap_or("checkpoint"),
                set.len()
            );
        }
        self.set = set;
        self.set_time = sent.sent;
        self.ready = None;
        self.picked = false;
        self.want_request = false;
    }

    /// Samples the plan for the next slot from the set and hands it to the act hook.
    fn pick_next(&mut self, ai: &mut AiBrain, hit_radius: f32) {
        if let Some(plan) = self.mid_plan.take().or_else(|| self.staged_mid.take()) {
            // Prepare the next slot without replacing the currently running plan or goals.
            let old_plan = self.game_plan.replace(plan.clone());
            let old_set = self.set.clone();
            let old_active = self.active_set;
            let old_since = self.set_since;
            let old_opener = self.opener.clone();
            let old_why = self.pending_why;
            self.install_plan();
            self.opener = None;
            self.pick_from_active(ai, hit_radius);
            self.game_plan = old_plan;
            self.set = old_set;
            self.active_set = old_active;
            self.set_since = old_since;
            self.opener = old_opener;
            self.pending_why = old_why;
            self.staged_mid = Some(plan);
        } else {
            self.pick_from_active(ai, hit_radius);
        }
    }

    fn pick_from_active(&mut self, ai: &mut AiBrain, hit_radius: f32) {
        let opener = self
            .opener
            .take()
            .filter(|p| !self.banned.contains(&p.value()))
            .filter(|p| self.plan_fits(p))
            .filter(|p| {
                let factor =
                    super::adaptation::plan_multiplier(p, |id| self.attack_factor(id)).min(1.0);
                self.rng.next_f32() < factor
            });
        let is_opener = opener.is_some();
        let mut plan = if let Some(plan) = opener {
            if self.brain_moves {
                self.move_context =
                    super::move_bandit::context(&self.move_observation, &self.move_fighter.last);
                self.move_fighter.last = super::move_bandit::family(&plan.value());
            }
            Some(plan)
        } else {
            // The hard limits (see `limits`): only eligible plans are sampled; with none, his
            // own AI's act runs.
            let eligible: Vec<usize> = (0..self.set.len())
                .filter(|&i| {
                    self.set[i]
                        .plan
                        .as_ref()
                        .is_none_or(|p| !self.banned.contains(&p.value()))
                        && (!self.anti_stall
                            || self.set[i].plan.as_ref().is_none_or(|p| {
                                self.pacing.eligible(p, self.time, self.actions)
                                    && self.has_sheet_attack(p)
                            }))
                        && self.set[i].plan.as_ref().is_none_or(|p| self.plan_fits(p))
                })
                .collect();
            if self.set.is_empty()
                && (!self.anti_stall
                    || self.time - self.pacing.last_attack < limits::MAX_PASSIVE_SECS)
            {
                return;
            }
            let mut view: Vec<Weighted> = eligible
                .iter()
                .map(|&i| {
                    let mut entry = self.set[i].clone();
                    if let Some(plan) = &entry.plan
                        && !self.brain_moves
                    {
                        entry.weight *=
                            super::adaptation::plan_multiplier(plan, |id| self.attack_factor(id));
                    }
                    entry
                })
                .collect();
            if let Some(combo) = self.breaking_combo()
                && !view.iter().any(|w| w.plan.as_ref() == Some(&combo))
            {
                view.push(Weighted {
                    plan: Some(combo),
                    weight: 1.0,
                });
            }
            if view.is_empty() {
                self.approach_plan()
            } else {
                let picked = self.pick_move(&view, false);
                match picked {
                    Pick::Nothing => None,
                    Pick::Lua => None,
                    Pick::Plan(i) => view[i].plan.clone(),
                }
            }
        };
        if self.anti_stall
            && (self.stall_recovery
                || (plan.is_none()
                    && self.time - self.pacing.last_attack >= limits::MAX_PASSIVE_SECS))
        {
            // Lua is not a guaranteed attack. Use an allowed attack from this boss's table.
            plan = self
                .actions
                .actions
                .iter()
                .filter(|a| a.kind == super::actions::ActionKind::Attack)
                .find_map(|a| {
                    let p = self.actions.parse(&a.id.to_string()).ok()?;
                    (self.plan_fits(&p)
                        && !self.banned.contains(&p.value())
                        && self.game_plan.as_ref().is_none_or(|plan| {
                            !plan.avoid.iter().any(|avoid| avoid.choice == p.value())
                        }))
                    .then_some(p)
                });
        }
        if plan.is_none()
            && self.anti_stall
            && !self.move_sheet.is_empty()
            && (self.time - self.pacing.last_attack >= limits::MAX_PASSIVE_SECS
                || !self
                    .set
                    .iter()
                    .any(|w| w.plan.as_ref().is_some_and(|p| self.plan_fits(p))))
        {
            plan = self.approach_plan();
        }
        if !self.persist && !is_opener {
            self.set.clear();
        }
        self.picked = true;
        let Some(plan) = plan else { return };
        let label = if is_opener {
            "opener".to_string()
        } else {
            self.active_set_name().unwrap_or("-").to_string()
        };
        match self.actions.act_for_plan(&plan, hit_radius) {
            Ok(choice) => {
                if let Some(hook) = ai.state.act_hook.as_mut() {
                    hook.choice = Some(choice);
                }
                let since = if self.persist {
                    self.time
                } else {
                    self.set_time
                };
                self.ready = Some(Ready {
                    since,
                    plan,
                    opener: is_opener,
                    label,
                });
            }
            Err(e) => {
                self.stats.invalid += 1;
                self.write(json!({"ev": "invalid", "t": ms(self.time), "error": e.to_string()}));
            }
        }
    }

    fn pick_move(&mut self, view: &[Weighted], broken: bool) -> Pick {
        if broken && !self.move_sheet.is_empty() && !self.brain.fixed_plans() {
            let mut observation = self.move_observation.clone();
            observation.wolf.deathblow_open = true;
            self.move_context = super::move_bandit::context(&observation, &self.move_fighter.last);
            let viable = self.live_punishes();
            let candidates: Vec<_> = view
                .iter()
                .enumerate()
                .filter_map(|(i, w)| {
                    let p = w.plan.as_ref()?;
                    viable.iter().find(|v| v.id == p.value()).map(|v| (i, v))
                })
                .collect();
            let bucket = super::moves::bucket(self.relative[0].hypot(self.relative[1]));
            let preferences = self
                .game_plan
                .as_ref()
                .and_then(|p| p.punish_preferences.get(bucket))
                .map_or(&[][..], Vec::as_slice);
            if let Some(chosen) = super::moves::choose_punish(
                &candidates.iter().map(|(_, p)| *p).collect::<Vec<_>>(),
                preferences,
                &mut self.rng,
            ) {
                return Pick::Plan(
                    candidates
                        .iter()
                        .find(|(_, p)| p.id == chosen.id)
                        .unwrap()
                        .0,
                );
            }
            return view
                .iter()
                .enumerate()
                .filter_map(|(i, w)| {
                    let p = w.plan.as_ref()?;
                    if !self.plan_fits(p) {
                        return None;
                    }
                    let time = self
                        .move_sheet
                        .get(&p.value())?
                        .reachable_time(self.relative, self.wolf_velocity)?;
                    Some((i, time))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map_or(Pick::Nothing, |(i, _)| Pick::Plan(i));
        }
        if !self.brain_moves {
            let empty_streaks = Streaks::default();
            let empty_tried = HashSet::new();
            let ctx = Context {
                streaks: if broken {
                    &empty_streaks
                } else {
                    &self.streaks
                },
                tried: if broken { &empty_tried } else { &self.tried },
                temperature: self.temperature,
            };
            return sampler::pick(view, &ctx, &mut self.rng);
        }
        let mut observation = self.move_observation.clone();
        if broken {
            observation.wolf.deathblow_open = true;
        }
        self.move_context = super::move_bandit::context(&observation, &self.move_fighter.last);
        if let Some(combo) = self.breaking_combo()
            && let Some(row) = self.move_sheet.get(&combo.value())
        {
            self.move_context = format!(
                "{}/combo:{}:{:.0}",
                self.move_context,
                combo.value(),
                row.posture_total
            );
        }
        self.move_fighter.evidence = super::move_bandit::in_context(
            &self.memory.move_bandit,
            self.actions.chr,
            &self.move_context,
        );
        let mut shared = super::bandit::BanditMemory::new();
        for (player, snapshot) in &self.boss_knowledge.players {
            if player != &self.player {
                let context = super::move_bandit::in_context(
                    &snapshot.move_bandit,
                    self.actions.chr,
                    &self.move_context,
                );
                super::bandit::merge(&mut shared, &context, 1.0);
            }
        }
        super::bandit::transfer(&mut self.move_fighter.evidence, &shared);
        let controls = self
            .game_plan
            .as_ref()
            .map_or(&[][..], |p| p.move_controls.as_slice());
        let explore = self
            .game_plan
            .as_ref()
            .map_or(Default::default(), |p| p.explore);
        let settings = super::move_bandit::Settings {
            controls,
            explore,
            every: self.brain_move_every,
        };
        self.move_fighter.choose(
            self.actions.chr,
            &self.move_context,
            view,
            settings,
            &mut self.rng,
        )
    }

    fn plan_fits(&self, plan: &Plan) -> bool {
        if self.brain.fixed_plans() || self.move_sheet.is_empty() {
            return true;
        }
        // Setups are admitted at their attack's target distance, then checked again live.
        let mut relative = self.relative;
        let mut setup = false;
        for step in &plan.steps {
            match step {
                super::plan::Step::Tactic(
                    super::plan::Tactic::WalkIn(d) | super::plan::Tactic::RunIn(d),
                ) => {
                    setup = true;
                    let length = relative[0].hypot(relative[1]).max(0.001);
                    let target = length.min(*d);
                    relative = [relative[0] * target / length, relative[1] * target / length];
                }
                super::plan::Step::Act(id) if (3000..3100).contains(id) => {
                    if self
                        .move_sheet
                        .get(&id.to_string())
                        .is_some_and(|m| !m.contact)
                    {
                        setup = true;
                        continue;
                    }
                    if setup
                        && let Some(a) = self.actions.get(*id)
                        && a.approach > 0.0
                    {
                        let length = relative[0].hypot(relative[1]).max(0.001);
                        let scale = length.min(a.approach) / length;
                        relative = [relative[0] * scale, relative[1] * scale];
                    }
                    return self
                        .move_sheet
                        .get(&id.to_string())
                        .is_some_and(|m| m.fits(relative, self.wolf_velocity));
                }
                super::plan::Step::Tactic(_) => setup = true,
                _ => {}
            }
        }
        true
    }

    fn has_sheet_attack(&self, plan: &Plan) -> bool {
        if self.move_sheet.is_empty() {
            return limits::has_attack(plan, self.actions);
        }
        plan.actions().iter().any(|id| {
            self.move_sheet
                .get(&id.to_string())
                .is_some_and(|m| m.contact)
        })
    }

    fn close_contact_range(&mut self) {
        if let Some(sample) = &self.fight.range_open
            && let Some(distance) = sample.first_distance
        {
            self.fight
                .contact_samples
                .push((sample.action.clone(), sample.started, distance));
        }
    }

    pub(crate) fn observe_space(&mut self, duel: &Duel) {
        self.break_windows = super::moves::break_recoveries(&duel.cache);
        self.punish_window = self.break_windows.get(&1001).copied();
        self.move_observation = observe(duel, self.time, self.since_hit, &self.running.value());
        let p = duel.player.body.position;
        let e = duel.enemy.body.position;
        self.relative = [p[0] - e[0], p[2] - e[2]];
        self.wolf_position = Some(p);
        self.wolf_velocity = [0.0; 2];
        self.boss_position = Some(e);
    }

    fn punish_evidence(&self, bucket: &str) -> BTreeMap<String, super::moves::PunishEvidence> {
        let mut evidence = self
            .memory
            .punishes
            .get(self.actions.chr)
            .and_then(|m| m.get(bucket))
            .cloned()
            .unwrap_or_default();
        if !self.fight_over
            && let Some(current) = self
                .fight
                .punishes
                .get(self.actions.chr)
                .and_then(|m| m.get(bucket))
        {
            for (id, observed) in current {
                evidence.entry(id.clone()).or_default().merge(observed);
            }
        }
        evidence
    }

    fn live_punishes(&self) -> Vec<super::moves::Punish> {
        let elapsed = self
            .fight
            .metrics
            .breaks
            .last()
            .map_or(0.0, |r| (self.time - r.time).max(0.0));
        let wait = self
            .fight
            .metrics
            .breaks
            .last()
            .map_or(0.0, |r| (r.wait - elapsed).max(0.0));
        let bucket = super::moves::bucket(self.relative[0].hypot(self.relative[1]));
        let start = [
            self.relative[0] + self.wolf_velocity[0] * wait,
            self.relative[1] + self.wolf_velocity[1] * wait,
        ];
        let mut list = super::moves::viable_punishes(
            &self.move_sheet,
            self.actions,
            start,
            self.wolf_velocity,
            self.punish_window.unwrap_or(0.0) - elapsed - wait,
            &self.punish_evidence(bucket),
        );
        for p in &mut list {
            p.first += wait;
        }
        list
    }

    fn settle_punish(&mut self) {
        let Some(use_) = self.active_punish.take() else {
            return;
        };
        let e = self
            .fight
            .punishes
            .entry(use_.phase)
            .or_default()
            .entry(use_.bucket)
            .or_default()
            .entry(use_.id)
            .or_default();
        e.uses += 1;
        e.landed += use_.outcome.landed;
        e.converted += use_.outcome.converted;
        e.window_hp += use_.outcome.window_hp;
        e.window_posture += use_.outcome.window_posture;
    }

    fn approach_plan(&self) -> Option<Plan> {
        let id = self
            .actions
            .actions
            .iter()
            .filter(|a| !self.banned.contains(&a.id.to_string()))
            .filter(|a| {
                self.game_plan
                    .as_ref()
                    .is_none_or(|p| !p.avoid.iter().any(|w| w.choice == a.id.to_string()))
            })
            .filter_map(|a| {
                let m = self.move_sheet.get(&a.id.to_string())?;
                Some((a.id, m.first_reach?, m.first_hit(0.0)?))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))?;
        Some(Plan {
            steps: vec![
                super::plan::Step::Tactic(super::plan::Tactic::RunIn((id.1 * 0.7).max(0.5))),
                super::plan::Step::Act(id.0),
            ],
        })
    }

    fn breaking_combo(&self) -> Option<Plan> {
        if self.brain.fixed_plans() || self.wolf_posture_left <= 0.0 {
            return None;
        }
        self.move_sheet
            .iter()
            .filter(|(key, _)| key.contains('+'))
            .filter_map(|(key, row)| {
                let plan = self.actions.parse(key).ok()?;
                if !self.plan_fits(&plan) || self.banned.contains(key) {
                    return None;
                }
                if self
                    .game_plan
                    .as_ref()
                    .is_some_and(|p| p.avoid.iter().any(|w| w.choice == *key))
                {
                    return None;
                }
                Some((
                    plan,
                    row.breaking_followup(self.wolf_posture_left, self.move_observation.dist)?,
                ))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|p| p.0)
    }

    /// Whether nothing should be requested or logged: a fighter is dead.
    fn quiet(&self, duel: &Duel) -> bool {
        self.fight_over
            || duel.respawn_in.is_some()
            || duel.rules.fighters[0].dead()
            || duel.rules.fighters[1].dead()
    }

    fn checkpoint_blocked(&self, duel: &Duel) -> bool {
        self.quiet(duel)
            || duel.deathblow.is_some()
            || duel.deathblow_available()
            || duel.rules.deathblow_open(1)
            || duel
                .enemy
                .behavior
                .runtime
                .main_state_path()
                .first()
                .is_some_and(|s| s.contains("Damage") || s.contains("Throw"))
    }

    fn hits_to_break(&self, duel: &Duel) -> Option<f32> {
        let anim = duel.player.tae.anim("a050_300000")?;
        let attack = anim.events.iter().find_map(|e| match e.kind {
            crate::tae::TaeKind::Attack { judge, .. } => duel.fighters[0].attacks.get(&judge),
            _ => None,
        })?;
        let damage = duel
            .rules
            .wolf_posture_per_hit(attack, [&duel.player, &duel.enemy])?;
        Some(duel.rules.fighters[1].posture.remaining as f32 / damage)
    }

    /// After the Lua AI thought: notes who chose any act that started and sends the next
    /// request when one is due.
    pub fn after_think(&mut self, ai: &mut AiBrain, duel: &Duel) {
        let (slots, started) = ai
            .state
            .act_hook
            .as_mut()
            .map(|h| (std::mem::take(&mut h.slots), std::mem::take(&mut h.started)))
            .unwrap_or_default();
        let quiet = self.quiet(duel);
        for (_, slot) in slots {
            if self.tactic_open {
                self.tally
                    .record(self.time, TallyEvent::End { side: Side::Boss });
                self.tactic_open = false;
            }
            if !matches!(slot, ActSlot::Reflex)
                && let Some(plan) = self.staged_mid.take()
            {
                self.close_set_time();
                self.game_plan = Some(plan);
                self.install_plan();
                self.opener = None;
            }
            let by = match slot {
                ActSlot::External => {
                    let ready = self.ready.take();
                    let label = ready.as_ref().map(|r| r.label.clone()).unwrap_or_default();
                    self.running = ready.map(|r| r.plan).unwrap_or_default();
                    if label == "on_wolf_break" {
                        self.settle_punish();
                        let id = self.running.value();
                        let window_end = self
                            .fight
                            .metrics
                            .breaks
                            .last()
                            .map_or(self.time, |r| r.time)
                            + self.punish_window.unwrap_or(0.0);
                        self.active_punish = Some(PunishUse {
                            phase: self.actions.chr.into(),
                            bucket: super::moves::bucket(self.relative[0].hypot(self.relative[1]))
                                .into(),
                            end: (self.time + self.move_sheet.get(&id).map_or(0.0, |m| m.duration))
                                .max(window_end),
                            id,
                            window_end,
                            throw_started: false,
                            outcome: Default::default(),
                        });
                    }
                    if self.brain_moves {
                        self.move_fighter.runs.push((
                            self.time,
                            self.move_context.clone(),
                            self.running.value(),
                        ));
                    }
                    self.overlay_events.step = 0;
                    if limits::is_retreat(&self.running, self.actions) {
                        self.pacing.last_retreat = Some(self.time);
                    }
                    self.history.push(self.running.clone());
                    self.stats.planned += 1;
                    self.picked = false;
                    let value = self.running.value();
                    self.tried.insert(value.clone());
                    if !quiet {
                        self.fight.decisions += 1;
                        self.fight
                            .runs
                            .push((self.time, value.clone(), label.clone()));
                        self.fight.uses.push(PlanUse {
                            break_response: label == "on_wolf_break",
                            failed: false,
                            text: if label == "on_wolf_break" {
                                format!("on_wolf_break:{value}")
                            } else {
                                value.clone()
                            },
                            start: self.time,
                            last_attack: None,
                            attacks: 0,
                            deflected: 0,
                            fatal: false,
                            settled: false,
                        });
                        *self.fight.choices.entry(value).or_default() += 1;
                    }
                    "plan"
                }
                ActSlot::Script => {
                    self.move_fighter
                        .runs
                        .push((self.time, String::new(), "lua".into()));
                    self.running = Plan::default();
                    self.stats.fallback += 1;
                    self.fight.lua_acts += 1;
                    self.picked = false;
                    if !quiet {
                        self.fight
                            .runs
                            .push((self.time, "lua".into(), "lua".into()));
                    }
                    "lua"
                }
                ActSlot::Reflex => {
                    self.move_fighter
                        .runs
                        .push((self.time, String::new(), "lua".into()));
                    self.running = Plan::default();
                    self.stats.reflex += 1;
                    self.fight.lua_acts += 1;
                    if !quiet {
                        self.fight
                            .runs
                            .push((self.time, "lua".into(), "lua".into()));
                    }
                    "reflex"
                }
            };
            match self.mode {
                RequestMode::Slot => self.want_request = true,
                RequestMode::PlanEnd if by != "plan" => self.want_request = true,
                _ => {}
            }
            if !quiet {
                let plan = self.running.value();
                self.write(json!({"ev": "slot", "t": ms(self.time), "by": by, "plan": plan}));
            }
        }
        for (_, step) in started {
            self.overlay_events.step = step;
            if quiet {
                continue;
            }
            if let Some(Step::Tactic(t)) = self.running.steps.get(step) {
                // A tactic is an exchange of its own in the tally (Wolf hitting him during it
                // is a punish).
                let key = t.token();
                self.tally.record(
                    self.time,
                    TallyEvent::Start {
                        side: Side::Boss,
                        key,
                    },
                );
                self.tactic_open = true;
            }
            if self.mode == RequestMode::PlanEnd && step + 1 >= self.running.len() {
                self.want_request = true;
            }
        }
        if quiet
            || !self.brain.plans()
            || self.in_flight.is_some()
            || (self.mode == RequestMode::Checkpoints && self.checkpoint_blocked(duel))
        {
            return;
        }
        if self.mode == RequestMode::Checkpoints {
            if self.pending_why.is_none() {
                self.pending_why = self.wolf_events.pop_front();
            }
            let limit = match self.pending_why {
                Some("pressure") => Some(PRESSURE_HITS),
                Some("pressure_critical") => Some(PRESSURE_CRITICAL_HITS),
                _ => None,
            };
            if let Some(limit) = limit
                && self
                    .hits_to_break(duel)
                    .is_none_or(|hits| !(PRESSURE_MIN_HITS..=limit).contains(&hits))
            {
                self.pending_why = None;
            }
        }
        let why = match self.mode {
            RequestMode::Slot | RequestMode::PlanEnd => {
                (self.want_request && self.ready.is_none() && (!self.picked || self.set.is_empty()))
                    .then_some(None)
            }
            RequestMode::Checkpoints => self.pending_why.map(Some),
        };
        let Some(why) = why else { return };
        if self.brain.budget_exhausted()
            && (!self.coach_mode
                || self.brain.checkpoint_chooser() == super::bandit::CheckpointChooser::Decisions)
        {
            return;
        }
        self.want_request = false;
        self.pending_why = None;
        let (active, sets) = if self.coach_mode {
            // Checkpoints only choose among a game plan's sets: without a plan (the first
            // fight, or a coach that failed) no call is made and his own AI plays.
            let plan = self.game_plan.as_ref().filter(|p| p.sets.len() >= 2);
            let Some(plan) = plan else { return };
            (self.active_set_name().map(str::to_string), plan.offers())
        } else {
            (None, Vec::new())
        };
        let obs = observe(duel, self.time, self.since_hit, &self.running.value());
        if self.coach_mode {
            use super::bandit::CheckpointChooser;
            match self.brain.checkpoint_chooser() {
                CheckpointChooser::Off => return,
                CheckpointChooser::Local => {
                    self.settle_bandit();
                    let context = why.unwrap_or("start");
                    let plan = self.game_plan.as_ref().expect("offered game plan");
                    let mut evidence = self.memory.bandit.clone();
                    super::bandit::merge(&mut evidence, &self.fight_bandit, 1.0);
                    // Borrow other players' evidence at quarter strength, without duplicating
                    // this player's evidence or writing borrowed counts into their profile.
                    let mut shared_evidence = super::bandit::BanditMemory::new();
                    for (player, shared) in &self.boss_knowledge.players {
                        if player == &self.player {
                            continue;
                        }
                        super::bandit::merge(&mut shared_evidence, &shared.bandit, 1.0);
                    }
                    super::bandit::transfer(&mut evidence, &shared_evidence);
                    let quality: BTreeMap<String, f32> = plan
                        .sets
                        .iter()
                        .enumerate()
                        .map(|(index, set)| {
                            let weighted = plan.weighted(index, self.actions);
                            let total: f32 = weighted.iter().map(|w| w.weight).sum();
                            let quality: f32 = weighted
                                .iter()
                                .map(|w| {
                                    w.weight
                                        * w.plan.as_ref().map_or(1.0, |p| {
                                            super::adaptation::plan_multiplier(p, |id| {
                                                self.attack_factor(id)
                                            })
                                        })
                                })
                                .sum();
                            (set.name.clone(), quality / total.max(1e-6))
                        })
                        .collect();
                    if let Some((chosen, mean)) = super::bandit::choose_with_quality(
                        &evidence,
                        self.actions.chr,
                        context,
                        plan,
                        &quality,
                        &mut self.rng,
                    ) {
                        self.switch_set(&chosen);
                        self.overlay_events.checkpoint = Some((
                            self.time,
                            format!("{context} -> set {chosen} (mean {mean:.2})"),
                        ));
                        self.bandit_pending = Some(PendingCheckpoint {
                            phase: self.actions.chr.into(),
                            context: context.into(),
                            set: chosen.clone(),
                            started: self.time,
                            metrics: self.fight.metrics.snapshot(self.time),
                            deflected_all: self
                                .fight
                                .failures
                                .values()
                                .map(|f| f.deflected_all)
                                .sum(),
                        });
                        self.bandit_choices.push((
                            format!("{}/{context}", self.actions.chr),
                            chosen,
                            mean,
                            None,
                        ));
                    }
                    return;
                }
                CheckpointChooser::Decisions => {}
            }
        }
        let (tally, memory, brief) = if self.coach_mode {
            // Scalars plus bounded coach reasoning; full histories remain with the coach.
            let r2 = |x: f32| (f64::from(x) * 100.0).round() / 100.0;
            let mut brief = json!({"why": why, "set": active, "boss_hp": r2(obs.boss.hp),
                "boss_posture": r2(obs.boss.posture), "wolf_hp": r2(obs.wolf.hp),
                "wolf_posture": r2(obs.wolf.posture), "dist": r2(obs.dist)});
            if let Some(plan) = &self.game_plan {
                brief["coach"] = plan.checkpoint_context(
                    active.as_deref(),
                    &self.boss_knowledge.principles,
                    &self.memory.coach.lessons,
                );
            }
            if why == Some("wolf_broken") || obs.wolf.deathblow_open {
                brief["viable_punishes"] = serde_json::to_value(
                    self.live_punishes().into_iter().take(4).collect::<Vec<_>>(),
                )
                .unwrap();
            }
            (Default::default(), None, Some(brief.to_string()))
        } else {
            (self.tally.summary(), Some(self.memory_summary()), None)
        };
        let mut request = BrainRequest {
            viable_punishes: if why == Some("wolf_broken") || obs.wolf.deathblow_open {
                self.live_punishes().into_iter().take(4).collect()
            } else {
                Vec::new()
            },
            id: self.next_id,
            obs,
            tally,
            memory,
            why,
            active,
            sets,
            brief,
            excluded: self.excluded(),
        };
        request.fit(REQUEST_BUDGET);
        self.next_id += 1;
        self.stats.requests += 1;
        self.fight.requests += 1;
        self.in_flight = Some(InFlight {
            id: request.id,
            sent: self.time,
            fight: self.fight_no,
        });
        if let Some(why) = request.why {
            self.overlay_events.checkpoint = Some((self.time, format!("{why} -> thinking")));
        }
        self.brain.submit(&request);
        if self.log.is_some() {
            self.write_line(&RequestLine {
                ev: "request",
                t: ms(self.time),
                request: &request,
            });
        }
    }

    /// The offered plans whose steps were deflected or guarded [`EXCLUDE_STREAK`] times in a
    /// row this fight.
    fn excluded(&self) -> Vec<String> {
        self.actions
            .choices()
            .into_iter()
            .filter(|c| {
                self.actions
                    .parse(&c.value)
                    .is_ok_and(|p| self.streaks.worst(&p) >= EXCLUDE_STREAK)
            })
            .map(|c| c.value)
            .collect()
    }

    /// After the duel step: action changes and hits into the tally and the habits; a death
    /// ends the fight, a respawn starts the next.
    pub fn after_step(&mut self, duel: &Duel, report: &DuelReport, input: &crate::InputFrame) {
        if duel.rules.fighters[0].posture.broken()
            && duel.player.state().contains("Break")
            && let Some(clip) = duel.player.behavior.runtime.main_clip()
            && let Some(window) = super::moves::recovery_window(&duel.player, &clip.animation)
        {
            self.punish_window = Some(window);
        }
        if let Some(use_) = self.active_punish.as_mut() {
            if report.grab.started.is_some() && self.running.value() == use_.id {
                use_.throw_started = true;
            }
            for (index, (hit, result)) in report.hits.iter().enumerate() {
                let source = report.projectile_sources.get(&index).copied().or_else(|| {
                    duel.enemy
                        .behavior
                        .runtime
                        .main_clip()
                        .and_then(|c| anim_id_from_clip(&c.animation))
                });
                let belongs = source.is_some_and(|source| {
                    use_.id.split('+').any(|id| id.parse::<i32>() == Ok(source))
                });
                if hit.attacker == 1 && belongs {
                    if !result.guarded && !result.deflected {
                        use_.outcome.landed = 1;
                    }
                    if self.time < use_.window_end {
                        use_.outcome.window_hp += result.hp_damage as f32;
                        use_.outcome.window_posture += result.posture_damage as f32;
                    }
                    if duel.rules.fighters[0].dead() {
                        use_.outcome.converted = 1;
                    }
                }
            }
            if use_.throw_started {
                for (_, hp, posture) in &report.grab.hits {
                    use_.outcome.landed = 1;
                    if self.time < use_.window_end {
                        use_.outcome.window_hp += *hp as f32;
                        use_.outcome.window_posture += *posture as f32;
                    }
                }
                if duel.rules.fighters[0].dead() {
                    use_.outcome.converted = 1;
                }
            }
            if (self.time >= use_.end && (!use_.throw_started || duel.enemy_throw.is_none()))
                || use_.outcome.converted > 0
            {
                self.settle_punish();
            }
        }
        self.move_observation = observe(duel, self.time, self.since_hit, &self.running.value());
        let p = duel.player.body.position;
        let e = duel.enemy.body.position;
        self.relative = [p[0] - e[0], p[2] - e[2]];
        if let Some(previous) = self.wolf_position {
            self.wolf_velocity = [
                (p[0] - previous[0]) / self.step_dt.max(0.001),
                (p[2] - previous[2]) / self.step_dt.max(0.001),
            ];
        }
        self.wolf_position = Some(p);
        self.boss_speed = self.boss_position.map_or(0.0, |old| {
            (e[0] - old[0]).hypot(e[2] - old[2]) / self.step_dt.max(0.001)
        });
        self.boss_position = Some(e);
        self.wolf_posture_left = (duel.rules.fighters[0].posture.max as f32
            * (1.0 - duel.rules.fighters[0].posture_fraction()))
        .max(0.0);
        if report.respawned {
            return;
        }
        if self.fight_over {
            return;
        }
        let t = self.time;
        self.fight.metrics.settle_break(t, false);
        self.fight.metrics.input_tick(input, t, self.step_dt);
        self.tally.phase = self.actions.chr.into();
        let mut events = Vec::new();
        if report.mikiri.is_some() {
            self.fight.metrics.countered();
            events.push(TallyEvent::Countered {
                kind: super::tally::CounterKind::Mikiri,
            });
        }
        if report.jump_counter {
            self.fight.metrics.countered();
            events.push(TallyEvent::Countered {
                kind: super::tally::CounterKind::Jump,
            });
        }
        if report.other_counter {
            self.fight.metrics.countered();
            events.push(TallyEvent::Countered {
                kind: super::tally::CounterKind::Other,
            });
        }
        // The boss's actions: his attack and step animations (guards and deflects are
        // reactions, not actions).
        let boss = duel
            .enemy
            .behavior
            .runtime
            .main_clip()
            .and_then(|c| anim_id_from_clip(&c.animation))
            .filter(|a| (3000..3100).contains(a) || (5200..5300).contains(a));
        if boss != self.boss_open || (boss.is_some() && report.enemy.clip_started) {
            if let Some(id) = boss
                && let Some(row) = self.move_sheet.get(&id.to_string())
            {
                let until = row
                    .projectiles
                    .iter()
                    .map(|p| p[0] + p[2])
                    .fold(0.0, f32::max);
                if until > 0.0 {
                    self.tally.retain_projectile(&id.to_string(), t + until);
                }
            }
            let response = self
                .fight
                .runs
                .last()
                .is_some_and(|r| r.2 == "on_wolf_break");
            if response
                && report.enemy.clip_started
                && boss.is_some_and(|a| self.running.actions().contains(&a))
            {
                self.fight.metrics.break_started(t);
            }
            self.close_contact_range();
            self.fight.close_range();
            events.push(match boss {
                Some(a) => TallyEvent::Start {
                    side: Side::Boss,
                    key: a.to_string(),
                },
                None => TallyEvent::End { side: Side::Boss },
            });
            if boss.is_some_and(|a| a < 3100) {
                if let Some((_, plan, _)) = self.fight.runs.last() {
                    let start = duel.enemy.body.position;
                    let wolf = duel.player.body.position;
                    self.fight.range_open = Some(AttackRange {
                        action: boss.unwrap().to_string(),
                        plan: plan.clone(),
                        start,
                        distance: ((wolf[0] - start[0]).powi(2) + (wolf[2] - start[2]).powi(2))
                            .sqrt(),
                        reach: None,
                        first_distance: None,
                        started: t,
                    });
                }
                // An attack step started (a back step does not count).
                if self
                    .move_sheet
                    .get(&boss.unwrap().to_string())
                    .is_none_or(|m| m.contact)
                {
                    self.fight.metrics.attack_started(t);
                    self.pacing.last_attack = t;
                }
                self.stall_recovery = false;
                if let Some(use_) = self.fight.uses.last_mut()
                    && self.fight.runs.last().is_some_and(|r| r.0 == use_.start)
                {
                    use_.last_attack = Some(t);
                    use_.attacks += 1;
                }
            }
            self.boss_open = boss;
            // An attack or step starting closes an open tactic; its end closes nothing more.
            if boss.is_some() {
                self.tactic_open = false;
            }
        }
        if let Some(sample) = self.fight.range_open.as_mut()
            && let Some(reach) =
                duel.fighters[1].active_reach(&duel.enemy, sample.start, duel.fighters[0].radius)
        {
            sample.reach = Some(sample.reach.unwrap_or(0.0).max(reach));
        }
        if let Some(sample) = self.fight.range_open.as_mut()
            && let Some(row) = self.move_sheet.get(&sample.action)
            && row.measurement_time(sample.distance).is_some_and(|first| {
                let clock = if row.projectiles.is_empty() {
                    duel.enemy
                        .behavior
                        .runtime
                        .main_clip()
                        .map_or(0.0, |c| c.time)
                } else {
                    t - sample.started
                };
                clock + 0.001 >= first
            })
            && sample.first_distance.is_none()
        {
            sample.first_distance = Some(self.move_observation.dist);
        }
        // Wolf's attacks: from the step his clip raises the parry-timing flag (the signal the
        // game gives enemies) until the clip changes.
        let clip = duel
            .player
            .behavior
            .runtime
            .main_clip()
            .map(|c| c.animation.clone());
        if self.wolf_open.is_some() && (self.wolf_open != clip || report.player.clip_started) {
            events.push(TallyEvent::End { side: Side::Wolf });
            self.wolf_open = None;
        }
        let state = report
            .player
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        if self.wolf_open.is_none() && duel.player.tae_frame().flag(FLAG_PARRY_TIMING) {
            let key = if state.is_empty() {
                clip.clone().unwrap_or_default()
            } else {
                wolf_key(&state)
            };
            events.push(TallyEvent::Start {
                side: Side::Wolf,
                key,
            });
            self.wolf_open = clip;
        }
        let mut punished_break = false;
        let wolf = &duel.rules.fighters[0];
        let (wolf_events, wolf_broke) = self
            .checkpoints
            .on_wolf(wolf.posture_fraction(), wolf.posture.broken());
        for why in wolf_events {
            if !self.wolf_events.contains(&why) {
                self.wolf_events.push_back(why);
            }
        }
        let breaking_hit = wolf_broke
            .then(|| {
                report.hits.iter().position(|(hit, result)| {
                    result.posture_broke
                        && (if hit.defender == 0 {
                            matches!(result.defender.damage_type, 1001 | 1004 | 1005)
                        } else {
                            result.attacker.as_ref().is_some_and(|damage| {
                                matches!(damage.damage_type, 1001 | 1004 | 1005)
                            })
                        })
                })
            })
            .flatten();
        if wolf_broke && breaking_hit.is_none() {
            self.move_fighter.openings.push(t);
            self.fight.metrics.wolf_break(t);
        }
        if wolf_broke {
            self.fight.metrics.settle_break(t, true);
            let code = duel
                .player
                .pending_damage
                .as_ref()
                .map_or(1001, |d| d.damage_type);
            self.punish_window = self.break_windows.get(&code).copied();
            if duel.player.state().contains("Break")
                && let Some(clip) = duel.player.behavior.runtime.main_clip()
                && let Some(window) = super::moves::recovery_window(&duel.player, &clip.animation)
            {
                self.punish_window = Some(window);
            }
            let clip = duel.enemy.behavior.runtime.main_clip();
            let time = clip.map_or(0.0, |c| c.time);
            let action = boss.map(|a| a.to_string()).unwrap_or_else(|| "free".into());
            let row = self.move_sheet.get(&action);
            let judge = breaking_hit.map(|i| report.hits[i].0.judge);
            let local_hit = row
                .and_then(|m| {
                    m.hits
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, h)| {
                            h[0] <= time + 0.001 && judge.is_none_or(|j| h[3] as i32 == j)
                        })
                        .map(|(i, _)| i + 1)
                })
                .unwrap_or(0);
            let step = self
                .overlay_events
                .step
                .min(self.running.steps.len().saturating_sub(1));
            let group_start = self.running.steps[..step]
                .iter()
                .rposition(|s| matches!(s, Step::Tactic(_)))
                .map_or(0, |i| i + 1);
            let ids: Vec<_> = self.running.steps[group_start..]
                .iter()
                .take_while(|s| matches!(s, Step::Act(_)))
                .filter_map(|s| match s {
                    Step::Act(id) => Some(*id),
                    _ => None,
                })
                .collect();
            let plan = ids.iter().map(i32::to_string).collect::<Vec<_>>().join("+");
            let combo = ids.len() > 1 && ids.contains(&boss.unwrap_or(0));
            let actual_step = ids
                .iter()
                .enumerate()
                .filter(|(_, id)| **id == boss.unwrap_or(0))
                .min_by_key(|(i, _)| i.abs_diff(step - group_start))
                .map_or(0, |(i, _)| i);
            let hit_counts: Vec<_> = ids
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    self.move_sheet.get(&a.to_string()).map_or(0, |m| {
                        let end = if i + 1 < ids.len() {
                            m.wait(0.0)
                        } else {
                            m.duration
                        };
                        m.hits.iter().filter(|h| h[0] <= end).count()
                    })
                })
                .collect();
            let previous = hit_counts.iter().take(actual_step).sum::<usize>();
            let recovering = row.is_some_and(|m| m.hits.last().is_some_and(|h| time > h[0]))
                && duel.enemy.tae_frame().attacks.is_empty();
            let frame = duel.enemy.tae_frame();
            let attack_open = frame.flag(crate::character::ai_cancel::COMBO)
                || frame.flag(crate::character::ai_cancel::ATTACK);
            let free = !frame.restricted
                || frame.flag(crate::tae::cancel::MOVE)
                || clip.is_some_and(|c| c.at_end());
            let wait = if attack_open || free {
                0.0
            } else {
                row.map_or_else(
                    || {
                        clip.and_then(|c| {
                            duel.enemy
                                .clips
                                .clip(&c.animation)
                                .map(|data| (data.duration - time).max(0.0))
                        })
                        .unwrap_or(0.0)
                    },
                    |m| m.wait(time),
                )
            };
            let record = super::metrics::BreakRecord {
                time: t,
                action: if combo { plan } else { action },
                hit: if local_hit == 0 {
                    0
                } else {
                    local_hit + if combo { previous } else { 0 }
                },
                hits: if combo {
                    hit_counts.iter().sum()
                } else {
                    row.map_or(0, |m| m.hits.len())
                },
                state: if free {
                    "free"
                } else if recovering || boss.is_none() {
                    "recovering"
                } else {
                    "attacking"
                }
                .into(),
                wait,
                outcome: "pending".into(),
                ..Default::default()
            };
            self.write(json!({"ev": "wolf_break", "break": record}));
            self.fight.metrics.breaks.push(record);
        }
        if wolf_broke
            && !self.quiet(duel)
            && duel.deathblow.is_none()
            && !duel.rules.deathblow_open(1)
        {
            self.break_pending = true;
        }
        for (index, (hit, result)) in report.hits.iter().enumerate() {
            if breaking_hit == Some(index) {
                self.move_fighter.openings.push(t);
                self.fight.metrics.wolf_break(t);
            }
            if (result.posture_broke && duel.rules.fighters[1].posture.broken())
                || (hit.attacker == 0 && matches!(result.defender.damage_type, 1001 | 1004 | 1005))
            {
                punished_break = true;
                self.fatal_plans(t, true);
            }
            let attacker = Side::from_index(hit.attacker);
            if attacker == Side::Boss
                && !result.guarded
                && !result.deflected
                && report
                    .projectile_sources
                    .get(&index)
                    .is_none_or(|id| Some(*id) == self.boss_open)
                && let Some(sample) = self.fight.range_open.as_mut()
            {
                sample
                    .first_distance
                    .get_or_insert(self.move_observation.dist);
            }
            if attacker == Side::Boss
                && let Some(id) = self.boss_open
            {
                let wolf = &duel.rules.fighters[0];
                let bars = result.hp_damage.max(0) as f32 / wolf.vitality.max_hp.max(1) as f32
                    + result.posture_damage.max(0) as f32 / wolf.posture.max.max(1) as f32;
                self.move_fighter.damage.push((t, id.to_string(), bars));
            }
            if attacker == Side::Boss
                && let Some(source) = report.projectile_sources.get(&index)
            {
                events.push(TallyEvent::ProjectileHit {
                    key: source.to_string(),
                    guarded: result.guarded,
                    deflected: result.deflected,
                    progress: result.hp_damage > 0 || result.posture_damage > 0,
                });
            } else {
                events.push(TallyEvent::Hit {
                    attacker,
                    guarded: result.guarded,
                    deflected: result.deflected,
                });
            }
            if result.hp_damage > 0 || result.posture_damage > 0 {
                if attacker == Side::Wolf {
                    events.push(TallyEvent::Damaged { side: Side::Boss });
                }
                if !report.projectile_sources.contains_key(&index) {
                    events.push(TallyEvent::Progress {
                        attacker,
                        broke: result.posture_broke
                            && duel.rules.fighters[attacker.other().index()]
                                .posture
                                .broken(),
                    });
                }
            }
            if !result.guarded && !result.deflected {
                // since_hit is [boss, wolf].
                self.since_hit[if attacker == Side::Boss { 0 } else { 1 }] = Some(0.0);
            }
            if attacker == Side::Boss {
                if breaking_hit != Some(index) && (result.guarded || result.deflected) {
                    self.fight.metrics.break_guarded(t);
                }
                // The blow that caused the break is not its follow-up.
                if breaking_hit != Some(index) && !result.guarded && !result.deflected {
                    if let Some(id) = self.boss_open {
                        self.move_fighter.contacts.push((t, id.to_string()));
                    }
                    self.fight.metrics.post_break_hit(t);
                }
                self.fight
                    .metrics
                    .boss_hit(result.guarded, result.deflected);
            }
            if let Some(why) = self
                .checkpoints
                .on_hit(attacker, result.guarded, result.deflected)
            {
                self.pending_why.get_or_insert(why);
            }
        }
        let wolf = &duel.rules.fighters[0];
        self.fight
            .metrics
            .wolf(wolf.hp_fraction(), wolf.posture_fraction());
        self.fight.metrics.passive_tick(t, self.step_dt);
        let boss_fighter = &duel.rules.fighters[1];
        self.fight
            .metrics
            .boss(boss_fighter.hp_fraction(), boss_fighter.posture_fraction());
        let hits = self.hits_to_break(duel);
        if let Some(why) = self.checkpoints.on_vitals(
            boss_fighter.posture_fraction(),
            boss_fighter.hp_fraction(),
            hits,
        ) {
            // A request may still be in flight when pressure grows. Keep the newest
            // posture warning instead of sending an obsolete half-meter/HP warning later.
            if (why == "pressure_critical"
                && matches!(self.pending_why, Some("pressure" | "half_hp")))
                || (why == "pressure" && self.pending_why == Some("half_hp"))
            {
                self.pending_why = Some(why);
            } else {
                self.pending_why.get_or_insert(why);
            }
        }
        let marks = boss_fighter.vitality.deathblows_left.max(0);
        let lost_mark = self.checkpoints.marks.is_some_and(|m| marks < m);
        if lost_mark && self.fight.metrics.metrics.player_first_deathblow.is_none() {
            self.fight.metrics.metrics.player_first_deathblow = Some(true);
        }
        if boss_fighter.dead() || lost_mark {
            self.fatal_plans(t, false);
        }
        if marks > 0 && self.checkpoints.marks.is_some_and(|m| marks < m) {
            self.pending_why = Some("mark_lost");
            self.mid_pending.get_or_insert("mark_lost");
        }
        self.checkpoints.marks = Some(marks);
        if self.quiet(duel) || duel.deathblow.is_some() || duel.rules.deathblow_open(1) {
            self.wolf_events.clear();
            self.break_pending = false;
            self.cancel_break = true;
            // A lost mark is useful after the get-up; pressure during a deathblow opening is obsolete.
            self.pending_why = self
                .pending_why
                .filter(|why| matches!(*why, "mark_lost" | "start"));
        }
        for e in &events {
            self.tally.record(t, e.clone());
            if let TallyEvent::Start {
                side: Side::Boss,
                key,
            } = e
                && self.move_sheet.get(key).is_some_and(|m| !m.contact)
            {
                self.tally.no_contact();
            }
        }
        let guarding = self.move_observation.boss.guarding && self.boss_open.is_none();
        let threatened = self.move_observation.wolf.attacking;
        let idle = self.boss_open.is_none()
            && !self.move_observation.boss.staggered
            && self.boss_speed < 0.1;
        self.idle_secs = if idle && !threatened {
            self.idle_secs + self.step_dt
        } else {
            0.0
        };
        if (guarding || self.idle_secs > 1.5) && !threatened && !self.quiet(duel) {
            self.idle_guard_secs += self.step_dt;
            if !self.tactic_open && self.boss_open.is_none() && !self.guard_exchange {
                self.tally.record(
                    t,
                    TallyEvent::Start {
                        side: Side::Boss,
                        key: if guarding { "guard" } else { "idle" }.into(),
                    },
                );
                self.guard_exchange = true;
            }
            if self.idle_guard_secs >= 0.5 || self.idle_secs > 1.5 {
                self.tally.idle_guard();
            }
        } else {
            self.idle_guard_secs = 0.0;
        }
        if self.guard_exchange && !guarding && self.idle_secs <= 1.5 {
            self.tally.record(t, TallyEvent::End { side: Side::Boss });
            self.guard_exchange = false;
        }
        let (p, e) = (duel.player.body.position, duel.enemy.body.position);
        let frame = WolfFrame {
            t,
            dt: self.step_dt,
            dist: ((p[0] - e[0]).powi(2) + (p[2] - e[2]).powi(2)).sqrt(),
            state: &state,
            airborne: !duel.player.body.grounded,
            guarding: duel.player.tae_frame().flag(FLAG_GUARD),
            hp: duel.rules.fighters[0].hp_fraction(),
        };
        self.habits.step(&frame, &events);
        let done = self.tally.advance(t);
        self.settle(&done);
        if boss_fighter.dead() || lost_mark || punished_break {
            self.fatal_plans(t, punished_break);
        }
        self.settle_plans(false);
        if self
            .bandit_pending
            .as_ref()
            .is_some_and(|pending| t - pending.started >= 4.0)
            || self.pending_why.is_some()
            || !self.wolf_events.is_empty()
        {
            self.settle_bandit();
        }
        self.log_outcomes(t, &done);
        self.review_mid_fight(duel);
        self.finish_if_over(duel);
        if !self.fight_over && self.memory_path.is_some() && t - self.last_save >= AUTOSAVE_SECS {
            self.save_memory();
        }
    }

    /// Map events own encounter victory, including final phases and scripted finishes.
    /// A dead phase actor alone is insufficient: it can be waiting for a handover.
    /// The respawn a Travel within the play world asks for ends nothing.
    pub(crate) fn finish_if_over(&mut self, duel: &Duel) {
        if duel.idol_arriving() {
            return;
        }
        let end = if duel.respawn_in.is_some() && duel.rules.fighters[0].dead() {
            Some(FightEnd::BossWon)
        } else if duel.events.as_ref().is_some_and(|events| events.defeated)
            || (duel.events.is_none() && duel.respawn_in.is_some())
        {
            Some(FightEnd::WolfWon)
        } else {
            None
        };
        if let Some(end) = end {
            self.end_fight(end);
        }
    }

    /// Finished boss exchanges update the streaks the sampler penalises: an attack is repelled
    /// when it was deflected or guarded and never landed, a tactic when Wolf hit him during it.
    fn settle(&mut self, done: &[Exchange]) {
        for x in done.iter().filter(|x| x.side == Side::Boss) {
            if let Some(index) = self
                .fight
                .contact_samples
                .iter()
                .position(|s| s.0 == x.key && (s.1 - x.start).abs() < 0.001)
            {
                let (_, _, distance) = self.fight.contact_samples.remove(index);
                if x.outcome.hit || x.whiffed() {
                    let column = usize::from(!x.outcome.hit);
                    let key = format!("{}:{}", x.phase, x.key);
                    for map in [&mut self.contact_ranges, &mut self.fight.contact_ranges] {
                        let row = map.entry(key.clone()).or_default();
                        row[column][0] += 1.0;
                        row[column][1] += distance;
                    }
                }
            }
        }
        self.attribute(done);
        for x in done.iter().filter(|x| x.side == Side::Boss) {
            if self.brain_moves {
                self.move_fighter.settle(x);
            }
            let progress = self
                .move_fighter
                .progress
                .entry(format!("{}:{}", x.phase, x.key))
                .or_default();
            progress[0] += 1;
            progress[1] += u32::from(x.outcome.progress || x.outcome.broke);
            super::adaptation::record(&mut self.fight.attack_learning, x);
            let o = &x.outcome;
            let repelled = if x.key.parse::<i32>().is_ok() {
                let (success, failure) = super::adaptation::signal(x);
                failure > success
            } else {
                o.punished || o.idle_guard
            };
            self.streaks.record(&x.key, repelled);
        }
    }

    fn plan_failures(&self, text: &str) -> PlanFailures {
        let key = if self.actions.chr == "c7110" {
            format!("c7110:{text}")
        } else {
            text.into()
        };
        let mut f = self
            .memory
            .coach
            .failures
            .get(&key)
            .copied()
            .unwrap_or_default();
        if !self.fight_over
            && let Some(now) = self.fight.failures.get(&key)
        {
            f.uses += now.uses;
            f.deflected_all += now.deflected_all;
            f.fatal += now.fatal;
            f.failed += now.failed;
        }
        f
    }

    fn fatal_plans(&mut self, now: f32, posture_break: bool) {
        let active = self.fight.runs.last().map(|r| r.0);
        for use_ in &mut self.fight.uses {
            if !use_.settled
                && (use_
                    .last_attack
                    .is_some_and(|last| now - last <= 3.0 + 1e-3)
                    || (posture_break && active == Some(use_.start)))
            {
                use_.fatal = true;
            }
        }
    }

    fn settle_plans(&mut self, flush: bool) {
        let active = self.fight.runs.last().map(|r| r.0);
        for use_ in &mut self.fight.uses {
            if use_.settled
                || (!flush
                    && (active == Some(use_.start)
                        || self.time - use_.last_attack.unwrap_or(use_.start) <= 3.0 + 1e-3))
            {
                continue;
            }
            use_.settled = true;
            let phase = self
                .fight
                .phase_changes
                .iter()
                .rev()
                .find(|(time, _)| *time <= use_.start)
                .map(|(_, phase)| phase.as_str())
                .unwrap_or(self.actions.chr);
            let key = if phase == "c7110" {
                format!("c7110:{}", use_.text)
            } else {
                use_.text.clone()
            };
            let f = self.fight.failures.entry(key).or_default();
            f.uses += 1;
            f.deflected_all += u32::from(use_.attacks > 0 && use_.attacks == use_.deflected);
            f.fatal += u32::from(use_.fatal);
            f.failed += u32::from(
                use_.failed && !use_.fatal && (use_.attacks == 0 || use_.attacks != use_.deflected),
            );
        }
    }

    /// Counts each finished boss exchange for the plan run it began under (the latest plan or
    /// Lua act that started at or before it), and that run's set.
    fn attribute(&mut self, done: &[Exchange]) {
        for x in done.iter().filter(|x| x.side == Side::Boss) {
            let Some((start, plan, set)) = self
                .fight
                .runs
                .iter()
                .rev()
                .find(|(start, _, _)| *start <= x.start + 1e-3)
                .cloned()
            else {
                continue;
            };
            if x.outcome.idle_guard
                && let Some(use_) = self.fight.uses.iter_mut().find(|u| u.start == start)
                && !use_.settled
            {
                use_.failed = true;
            }
            if x.key
                .parse::<i32>()
                .is_ok_and(|a| (3000..3100).contains(&a))
                && let Some(use_) = self.fight.uses.iter_mut().find(|u| u.start == start)
                && !use_.settled
            {
                // The last attack's end, rather than the beginning of its wind-up, anchors
                // the post-plan punishment window. It remains live after a new run starts.
                use_.last_attack = Some(
                    use_.last_attack
                        .unwrap_or(x.start)
                        .max(x.end.unwrap_or(self.time)),
                );
                use_.deflected +=
                    u32::from(x.outcome.deflected && !x.outcome.hit && !x.outcome.blocked);
                let (success, failure) = super::adaptation::signal(x);
                use_.failed |= failure > 0.0 && (use_.break_response || failure > success);
            }
            let mut one = Counts::default();
            one.merge(&Counts {
                countered: u32::from(x.outcome.countered.is_some()),
                counter_kinds: [
                    u32::from(x.outcome.countered == Some(super::tally::CounterKind::Mikiri)),
                    u32::from(x.outcome.countered == Some(super::tally::CounterKind::Jump)),
                    u32::from(x.outcome.countered == Some(super::tally::CounterKind::Other)),
                ],
                n: 1,
                hit: u32::from(x.outcome.hit),
                blocked: u32::from(x.outcome.blocked),
                deflected: u32::from(x.outcome.deflected),
                whiffed: u32::from(x.whiffed()),
                punished: u32::from(x.outcome.punished),
            });
            if set == "on_wolf_break" {
                self.fight
                    .break_plans
                    .entry(plan.clone())
                    .or_default()
                    .merge(&one);
            }
            let phase_key = |key: String| {
                if x.phase == "c7110" {
                    format!("c7110:{key}")
                } else {
                    key
                }
            };
            self.fight
                .by_plan
                .entry(phase_key(plan))
                .or_default()
                .merge(&one);
            self.fight
                .by_set
                .entry(phase_key(set))
                .or_default()
                .merge(&one);
        }
    }

    fn settle_bandit(&mut self) {
        let Some(pending) = self.bandit_pending.take() else {
            return;
        };
        let after = self.fight.metrics.snapshot(self.time);
        if !self.fight.metrics.player_active(self.time) {
            return;
        }
        let deflected_all: u32 = self.fight.failures.values().map(|f| f.deflected_all).sum();
        let reward = super::bandit::reward(
            pending.metrics,
            after,
            deflected_all.saturating_sub(pending.deflected_all),
        );
        self.fight_bandit
            .entry(pending.phase)
            .or_default()
            .entry(pending.context)
            .or_default()
            .entry(pending.set)
            .or_default()
            .record(reward);
        if let Some(choice) = self.bandit_choices.last_mut() {
            choice.3 = Some(reward);
        }
    }

    /// Ends the fight: settles it, adds it and its record to the memory and saves.
    pub fn end_fight(&mut self, end: FightEnd) {
        self.settle_punish();
        if self.fight_over {
            return;
        }
        let t = self.time;
        let done = self.tally.flush(t);
        self.close_contact_range();
        self.fight.close_range();
        self.settle(&done);
        if end == FightEnd::WolfWon {
            self.fatal_plans(t, false);
        }
        self.settle_plans(true);
        self.settle_bandit();
        self.close_set_time();
        self.log_outcomes(t, &done);
        self.habits.flush(t);
        self.fight_over = true;
        self.stats.fights += 1;
        let winner = match end {
            FightEnd::BossWon => "boss",
            FightEnd::WolfWon => "wolf",
            FightEnd::Unfinished => "none",
        };
        let punishes = self
            .tally
            .all(Side::Boss)
            .values()
            .map(|c| c.punished)
            .sum();
        let mut metrics = self
            .fight
            .metrics
            .finish(t, punishes, end == FightEnd::WolfWon);
        metrics.time_to_kill_wolf = (end == FightEnd::BossWon).then_some(metrics.duration);
        if end != FightEnd::Unfinished && metrics.player_first_deathblow.is_none() {
            metrics.player_first_deathblow = Some(end == FightEnd::WolfWon);
        }
        let usage = self.brain.usage();
        self.fight.metrics.settle_break(t + 2.501, true);
        let record = FightRecord {
            attacks: self.tally.phases.clone(),
            cost: self.brain.costs().totals.since(&self.fight.cost_at_start),
            session_cost: self.brain.costs().totals,
            breaks: self.fight.metrics.breaks.clone(),
            winner: winner.to_string(),
            metrics,
            decisions: self.fight.decisions,
            lua_acts: self.fight.lua_acts,
            requests: self.fight.requests,
            choices: self.fight.choices.clone(),
            tokens: [
                usage.0.saturating_sub(self.fight.tokens_at_start.0),
                usage.1.saturating_sub(self.fight.tokens_at_start.1),
            ],
            latency_ms: [
                percentile(&self.fight.latencies, 0.5).round() as u32,
                percentile(&self.fight.latencies, 0.9).round() as u32,
            ],
            brain: format!("{}/{}", self.brain.name(), self.mode.name()),
            reasoning: self.brain.call_stats()[0]
                .since(&self.fight.stats_at_start[0])
                .reasoning,
            coach: self.coach_done,
            mid_coach: self.mid_done,
            sets: self.fight.set_secs.clone(),
        };
        eprintln!("{}", record.summary_line());
        self.overlay_events.scorecard = Some((
            self.time,
            format!(
                "{} wins - kill Wolf {} - {} posture breaks - deflect {} - conversion {}",
                record.winner,
                record
                    .metrics
                    .time_to_kill_wolf
                    .map(|s| format!("{s:.1} s"))
                    .unwrap_or_else(|| "n/a".into()),
                record.metrics.wolf_posture_breaks,
                record
                    .metrics
                    .deflect_rate()
                    .map(|r| format!("{:.0}%", r * 100.0))
                    .unwrap_or_else(|| "n/a".into()),
                record
                    .metrics
                    .break_conversion_delay
                    .map_or("n/a".into(), |d| format!("{d:.3} s"))
            ),
        ));
        self.write(json!({"ev": "fight_end", "t": ms(t), "winner": winner,
            "void": record.metrics.void, "fights": self.memory.fights + u32::from(!record.metrics.void), "summary": record.summary_line()}));
        self.write_line(&json!({"ev": "fight_summary", "t": ms(t), "record": record}));
        if !record.metrics.void {
            super::moves::merge_punishes(&mut self.memory.punishes, &self.fight.punishes);
            if let Some(kill) = record.metrics.time_to_kill_wolf
                && !record.metrics.stalled()
                && let Some((_, context, arm)) = self.move_fighter.runs.last()
                && arm != "lua"
            {
                let e = self
                    .move_fighter
                    .provisional
                    .entry(self.actions.chr.into())
                    .or_default()
                    .entry(context.clone())
                    .or_default()
                    .entry(arm.clone())
                    .or_default();
                for _ in 0..3 {
                    e.record(0.98 + 0.02 * 60.0 / (60.0 + kill.max(0.0)));
                }
            }
            if record.metrics.stalled() {
                for arms in self
                    .fight_bandit
                    .values_mut()
                    .flat_map(|contexts| contexts.values_mut())
                {
                    for arm in arms.values_mut() {
                        arm.failure += arm.success;
                        arm.success = 0.0;
                    }
                }
                for (_, _, _, reward) in &mut self.bandit_choices[self.bandit_fight_start..] {
                    if reward.is_some() {
                        *reward = Some(0.0);
                    }
                }
            }
            super::bandit::decay(&mut self.memory.bandit);
            self.move_fighter
                .finish(&mut self.memory.move_bandit, record.metrics.stalled());
            for (key, samples) in &self.fight.contact_ranges {
                let entry = self
                    .memory
                    .coach
                    .contact_ranges
                    .entry(key.clone())
                    .or_default();
                for i in 0..2 {
                    for j in 0..2 {
                        entry[i][j] += samples[i][j];
                    }
                }
            }
            for (key, n) in &self.move_fighter.progress {
                let entry = self
                    .memory
                    .coach
                    .move_progress
                    .entry(key.clone())
                    .or_default();
                entry[0] += n[0];
                entry[1] += n[1];
            }
            super::bandit::merge(&mut self.memory.bandit, &self.fight_bandit, 1.0);
            super::adaptation::decay(&mut self.memory.attack_learning);
            super::adaptation::append(
                &mut self.memory.attack_learning,
                &self.fight.attack_learning,
            );
            self.session_records.push(record.clone());
            merge_counts(&mut self.session_actions, self.tally.all(Side::Boss));
            for (action, &(n, distance, reach)) in &self.fight.action_ranges {
                let entry = self.session_ranges.entry(action.clone()).or_default();
                entry.0 += n;
                entry.1 += distance;
                entry.2 += reach;
            }
            self.memory.add_fight(&self.tally, &self.habits.habits, end);
            self.memory.push_record(record.clone());
            if let Some(plan) = &self.game_plan {
                self.memory
                    .coach
                    .hypotheses
                    .push(super::digest::Hypothesis {
                        fight: self.memory.fights,
                        idea: plan.notes.chars().take(120).collect(),
                        plans: plan
                            .sets
                            .iter()
                            .flat_map(|s| s.plans.iter().map(|p| p.plan.as_str()))
                            .collect::<Vec<_>>()
                            .join(",")
                            .chars()
                            .take(160)
                            .collect(),
                        outcome: [
                            record.metrics.time_to_kill_wolf.unwrap_or(-1.0),
                            record.metrics.break_punish_rate,
                            record.metrics.damage_per_min(),
                        ],
                    });
                let excess = self.memory.coach.hypotheses.len().saturating_sub(6);
                self.memory.coach.hypotheses.drain(..excess);
            }
            merge_counts(&mut self.memory.coach.plans, &self.fight.by_plan);
            merge_counts(&mut self.memory.coach.break_plans, &self.fight.break_plans);
            for (text, f) in &self.fight.failures {
                let all = self.memory.coach.failures.entry(text.clone()).or_default();
                all.uses += f.uses;
                all.deflected_all += f.deflected_all;
                all.fatal += f.fatal;
                all.failed += f.failed;
            }
        } else {
            self.banned = self.bans_at_start.clone();
            for (text, base) in &mut self.retry_baselines {
                let committed = self
                    .memory
                    .coach
                    .failures
                    .get(text)
                    .copied()
                    .unwrap_or_default();
                base.uses = base.uses.min(committed.uses);
                base.deflected_all = base.deflected_all.min(committed.deflected_all);
                base.fatal = base.fatal.min(committed.fatal);
                base.failed = base.failed.min(committed.failed);
            }
        }
        if self.coach_mode && !record.metrics.void {
            self.boss_knowledge.update(&self.player, &self.memory);
            if let Some(plan) = &self.game_plan {
                self.boss_knowledge.consider(plan, &self.memory);
            }
            self.send_coach(&record);
        }
        self.last_record = Some(record);
        self.save_memory();
    }

    /// The coach's input for the fight just over (called after its record is in the memory).
    pub fn coach_input(&self, record: &FightRecord) -> CoachInput {
        let m = &record.metrics;
        let pct = |r: Option<f32>| (r.unwrap_or(0.0) * 100.0).round() as u32;
        let top = |map: &CountMap| ranked_counts(map);
        let mut sets: BTreeMap<String, SetStat> = BTreeMap::new();
        for (name, c) in &self.fight.by_set {
            sets.entry(name.clone()).or_default().c = *c;
        }
        for (name, s) in &self.fight.set_secs {
            sets.entry(name.clone()).or_default().secs = (s * 10.0).round() / 10.0;
        }
        let mut input = CoachInput {
            punish_window: self.punish_window,
            viable_punishes: [("near", 1.5), ("mid", 4.0), ("far", 8.0)]
                .into_iter()
                .map(|(bucket, d)| {
                    (
                        bucket.into(),
                        super::moves::viable_punishes(
                            &self.move_sheet,
                            self.actions,
                            [d, 0.0],
                            [0.0; 2],
                            self.punish_window.unwrap_or(0.0),
                            &self.punish_evidence(bucket),
                        ),
                    )
                })
                .collect(),
            wolf_posture: self.move_observation.wolf.posture,
            digest: super::digest::build(
                self.actions,
                &self.memory,
                self.tally
                    .phases
                    .get(self.actions.chr)
                    .unwrap_or_else(|| self.tally.all(Side::Boss)),
                &self.memory.coach.move_progress,
                self.coach_review_every,
            ),
            attack_rates: {
                let ranked = super::tally::TopMap::ranked(
                    self.actions
                        .actions
                        .iter()
                        .map(|a| a.id)
                        .chain(self.actions.counter_ids())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .map(|id| {
                            let key = id.to_string();
                            let evidence = self.attack_evidence(&key);
                            let total = evidence.success + evidence.failure;
                            let recent = self
                                .fight
                                .attack_learning
                                .get(self.actions.chr)
                                .and_then(|m| m.get(&key))
                                .map_or(0.0, |e| e.failure);
                            (
                                key,
                                (
                                    [
                                        evidence.n as f32,
                                        evidence.success / total.max(1.0),
                                        evidence.failure / total.max(1.0),
                                    ],
                                    evidence.failure + recent,
                                ),
                            )
                        })
                        .filter(|(_, (rates, _))| {
                            rates[0] >= super::adaptation::MIN_SAMPLES as f32
                        }),
                    |(_, priority)| *priority,
                );
                super::tally::TopMap(
                    ranked
                        .0
                        .into_iter()
                        .map(|(key, (rates, _))| (key, rates))
                        .collect(),
                )
            },
            checkpoints: {
                let mut summary = BTreeMap::<String, BTreeMap<String, [f32; 2]>>::new();
                for (context, set, _, reward) in &self.bandit_choices[self.bandit_fight_start..] {
                    if let Some(reward) = reward {
                        let entry = summary
                            .entry(context.clone())
                            .or_default()
                            .entry(set.clone())
                            .or_default();
                        entry[0] += 1.0;
                        entry[1] += reward;
                    }
                }
                for value in summary.values_mut().flat_map(|arms| arms.values_mut()) {
                    value[1] /= value[0];
                }
                summary
            },
            counters: super::tally::TopMap::ranked(
                self.actions
                    .counter_ids()
                    .iter()
                    .map(|id| (id.to_string(), self.action_evidence(&id.to_string()))),
                |c| c.n as f32,
            ),
            boss_phase: self.actions.chr.into(),
            move_sheet: self.move_sheet.clone(),
            break_log: self.fight.metrics.breaks.clone(),
            break_summary: super::metrics::break_summary(&self.fight.metrics.breaks),
            boss_knowledge: self.boss_knowledge.summary(),
            phase: "post".into(),
            elapsed: None,
            vitals: None,
            marks: None,
            on_wolf_break: self.fight.break_plans.clone(),
            actions: {
                let mut actions = self
                    .tally
                    .phases
                    .get(self.actions.chr)
                    .or_else(|| self.tally.phases.get(""))
                    .cloned()
                    .unwrap_or_default();
                for action in self.actions.actions {
                    actions.entry(action.id.to_string()).or_default();
                }
                for id in self.actions.counter_ids() {
                    actions.insert(id.to_string(), self.action_evidence(&id.to_string()));
                }
                actions
            },
            action_range: self
                .fight
                .action_ranges
                .iter()
                .map(|(key, &(n, distance, reach))| {
                    (
                        key.clone(),
                        [
                            (distance / n as f32 * 100.0).round() / 100.0,
                            (reach / n as f32 * 100.0).round() / 100.0,
                        ],
                    )
                })
                .collect(),
            principles: self
                .memory
                .coach
                .plan
                .as_ref()
                .map(|p| p.principles.clone())
                .unwrap_or_default(),
            banned: {
                let mut list: Vec<_> = self
                    .banned
                    .iter()
                    .filter(|choice| {
                        !self.boss_knowledge.bans.iter().any(|ban| {
                            *choice == &ban.choice
                                || choice.split(['>', '+']).any(|step| step == ban.choice)
                        })
                    })
                    .cloned()
                    .collect();
                list.sort();
                list
            },
            failures: self
                .memory
                .coach
                .failures
                .iter()
                .filter_map(|(k, f)| {
                    if self.actions.chr == "c7110" {
                        k.strip_prefix("c7110:").map(|k| (k.to_owned(), f))
                    } else if k.starts_with("c7110:") {
                        None
                    } else {
                        Some((k.clone(), f))
                    }
                })
                .map(|(k, f)| (k.clone(), [f.uses, f.deflected_all, f.fatal, f.failed]))
                .collect(),
            fight: self.memory.fights,
            result: FightLine {
                countered: m.countered,
                boss_damage_taken_per_min: m.boss_damage_taken_per_min(),
                wolf_posture_breaks: m.wolf_posture_breaks,
                hits_after_break: m.hits_after_break,
                break_punish_rate: m.break_punish_rate,
                break_conversion_delay: m.break_conversion_delay,
                wolf_posture_peak: m.wolf_posture_peak,
                time_to_kill_wolf: m.time_to_kill_wolf,
                won: record.winner.clone(),
                secs: m.duration.round() as u32,
                defl: pct(m.deflect_rate()),
                guard: pct(m.guard_rate()),
                hits: m.hits_taken,
                hp: (m.hp_lost * 100.0).round() as u32,
                posture: (m.posture_gained * 100.0).round() as u32,
                punishes: m.punishes,
                apm: m.attacks_per_min().round() as u32,
                dpm: m.damage_per_min().round() as u32,
                passive: (m.passive_share() * 100.0).round() as u32,
                engaged_time: m.engaged_time.round() as u32,
            },
            trend: Trend::last_ten(&self.memory.history),
            plans: top(&self.fight.by_plan),
            range: self
                .fight
                .ranges
                .iter()
                .map(|(plan, &(n, distance, reach))| {
                    let round = |x: f32| (x / n as f32 * 100.0).round() / 100.0;
                    (plan.clone(), [round(distance), round(reach)])
                })
                .collect(),
            sets,
            wolf: top(self.tally.all(Side::Wolf)),
            overall: top(&self.memory.coach.plans),
            habits: self.memory.habits.summary(),
            played: self.game_plan.clone(),
            notes: self
                .memory
                .coach
                .plan
                .as_ref()
                .map(|p| p.notes.clone())
                .unwrap_or_default(),
        };
        if !self.fight_over {
            for key in self.fight.failures.keys() {
                let key = if self.actions.chr == "c7110" {
                    let Some(key) = key.strip_prefix("c7110:") else {
                        continue;
                    };
                    key
                } else if key.starts_with("c7110:") {
                    continue;
                } else {
                    key
                };
                let f = self.plan_failures(key);
                input
                    .failures
                    .insert(key.into(), [f.uses, f.deflected_all, f.fatal, f.failed]);
            }
        }
        let mut table = String::from(
            "id label n hit dg counter whiff progress recent/life% first_s reach_m recovery_s posture/hit best_combo hit/miss_m\n",
        );
        let used = input
            .played
            .as_ref()
            .map(|p| p.summary_lines().join(" "))
            .unwrap_or_default();
        let mut lines: Vec<_> = input.digest.moves.lines().skip(1).collect();
        lines.sort_by_key(|line| {
            let key = line.split_whitespace().next().unwrap_or_default();
            (
                std::cmp::Reverse(used.contains(key)),
                std::cmp::Reverse(self.action_evidence(key).n),
                key.to_string(),
            )
        });
        for line in lines {
            let Some(key) = line.split_whitespace().next() else {
                continue;
            };
            let Some(row) = self.move_sheet.get(key) else {
                continue;
            };
            let best = row
                .chains
                .iter()
                .filter_map(|c| Some((c, self.move_sheet.get(c)?)))
                .max_by(|a, b| a.1.posture_total.total_cmp(&b.1.posture_total));
            let combo = best.map_or("-".into(), |(key, m)| {
                format!("{key}:{:.0}/{:.2}", m.posture_total, m.last_hit)
            });
            let mut ranges = self
                .memory
                .coach
                .contact_ranges
                .get(&format!("{}:{key}", self.actions.chr))
                .copied()
                .unwrap_or_default();
            if !self.fight_over
                && let Some(now) = self
                    .fight
                    .contact_ranges
                    .get(&format!("{}:{key}", self.actions.chr))
            {
                for i in 0..2 {
                    for j in 0..2 {
                        ranges[i][j] += now[i][j];
                    }
                }
            }
            let mean = |i: usize| {
                if ranges[i][0] == 0.0 {
                    "-".into()
                } else {
                    format!("{:.2}", ranges[i][1] / ranges[i][0])
                }
            };
            let posture = row
                .hits
                .iter()
                .map(|h| h[1])
                .chain(row.projectiles.iter().map(|p| p[5]))
                .map(|p| format!("{p:.0}"))
                .collect::<Vec<_>>()
                .join(",");
            let posture = if posture.is_empty() { "-" } else { &posture };
            table.push_str(&format!(
                "{line} {} {} {:.2} {posture} {combo} {}/{}\n",
                row.first_hit(3.0).map_or("-".into(), |t| format!("{t:.2}")),
                row.first_reach
                    .or_else(|| row
                        .projectiles
                        .iter()
                        .map(|p| p[1] * p[2] + p[3] + p[6])
                        .max_by(f32::total_cmp))
                    .map_or("-".into(), |r| format!("{r:.2}")),
                row.recovery,
                mean(0),
                mean(1)
            ));
        }
        input.digest.moves = table;
        // The notes of the plan played are the notes asked for again: send them once.
        if let Some(p) = input.played.as_mut()
            && p.notes == input.notes
        {
            p.notes.clear();
        }
        if let Some(p) = input.played.as_mut()
            && p.principles == input.principles
        {
            p.principles.clear();
        }
        if self.actions.chr == "c7110" {
            input.boss_knowledge.actions = ranked_counts(
                &self
                    .boss_knowledge
                    .evidence
                    .phase_actions
                    .get("c7110")
                    .cloned()
                    .unwrap_or_default(),
            );
            input.boss_knowledge.best = None;
            input
                .boss_knowledge
                .plans
                .0
                .retain(|(key, _)| key.starts_with("c7110:"));
            input.plans.0.retain(|(key, _)| key.starts_with("c7110:"));
            input.overall.0.retain(|(key, _)| key.starts_with("c7110:"));
        }
        input
    }

    /// Sends the fight summary to the coach; the plan comes back whenever it is ready.
    fn send_coach(&mut self, record: &FightRecord) {
        if !self.brain.plans() || self.fight.runs.is_empty() {
            return;
        }
        self.mid_pending = None;
        self.mid_plan = None;
        self.staged_mid = None;
        let input = self.coach_input(record);
        if !self.coach_flights.is_empty() {
            self.post_pending = Some(input);
            return;
        }
        self.submit_coach_input(input, false);
    }

    fn submit_coach_input(&mut self, mut input: CoachInput, mid: bool) {
        if self.brain.budget_exhausted() {
            return;
        }
        input.phase = if mid { "mid" } else { "post" }.into();
        let bytes = input.fit(INPUT_BUDGET);
        if bytes > INPUT_BUDGET {
            return;
        }
        let Ok(text) = serde_json::to_string(&input) else {
            return;
        };
        if input.digest.review {
            self.memory.coach.last_review_fight =
                self.memory.boss_wins.saturating_add(self.memory.wolf_wins);
        }
        let request = CoachRequest {
            id: self.next_coach_id,
            fight: self.fight_no,
            input: text,
        };
        self.next_coach_id += 1;
        self.coach_flights.push(CoachFlight {
            id: request.id,
            sent: self.time,
            mid,
            fight: self.fight_no,
        });
        if mid {
            self.mid_done[0] += 1;
        }
        self.last_coach_sent = self.time;
        self.review_breaks = self.fight.metrics.metrics.wolf_posture_breaks;
        if !mid {
            eprintln!(
                "coach: fight {} over, sent the fight summary ({bytes} bytes); the new game plan plays from the next fight if it arrives during the respawn, else the one after",
                self.fight_no
            );
        }
        self.write(json!({"ev": "coach_request", "kind": "coach", "phase": input.phase, "t": ms(self.time), "id": request.id,
            "bytes": bytes, "input": serde_json::from_str::<serde_json::Value>(&request.input).ok()}));
        self.brain.submit_coach(&request, self.time);
    }

    fn review_mid_fight(&mut self, duel: &Duel) {
        if !self.coach_mode || self.coach_interval <= 0.0 || self.fight_over {
            return;
        }
        let metrics = self.fight.metrics.snapshot(self.time);
        if metrics
            .wolf_posture_breaks
            .saturating_sub(self.review_breaks)
            >= 2
        {
            self.mid_pending.get_or_insert("wolf_broken_twice");
        }
        let set_banned = self.active_set_banned();
        if set_banned && !self.set_banned_seen {
            self.mid_pending.get_or_insert("set_banned");
        }
        self.set_banned_seen = set_banned;
        if self.time - self.last_coach_sent + 1e-3 >= self.coach_interval {
            self.mid_pending.get_or_insert("timer");
        }
        if metrics.duration < 20.0
            || metrics.void
            || self.checkpoint_blocked(duel)
            || duel.rules.fighters.iter().any(|f| f.posture.broken())
            || !self.coach_flights.is_empty()
            || !self.brain.plans()
        {
            return;
        }
        let Some(why) = self.mid_pending.take() else {
            return;
        };
        self.close_set_time();
        let (tally, habits) = self.fight_snapshot();
        let record = FightRecord {
            metrics: super::metrics::FightMetrics {
                punishes: tally.all(Side::Boss).values().map(|c| c.punished).sum(),
                ..metrics
            },
            ..FightRecord::default()
        };
        let mut input = self.coach_input(&record);
        for (key, counts) in tally.all(Side::Boss) {
            input.actions.insert(key.clone(), *counts);
        }
        input.wolf = ranked_counts(tally.all(Side::Wolf));
        input.habits = habits.habits.summary();
        input.fight = self.fight_no;
        input.vitals = Some([
            duel.rules.fighters[0].hp_fraction(),
            duel.rules.fighters[0].posture_fraction(),
            duel.rules.fighters[1].hp_fraction(),
            duel.rules.fighters[1].posture_fraction(),
        ]);
        input.marks = Some(duel.rules.fighters[1].vitality.deathblows_left);
        input.elapsed = Some(metrics.duration);
        eprintln!(
            "coach: mid-fight review at {:.1} s (why: {why})",
            metrics.duration
        );
        self.submit_coach_input(input, true);
    }

    fn active_set_banned(&self) -> bool {
        self.set.iter().any(|w| w.plan.is_some())
            && self
                .set
                .iter()
                .filter_map(|w| w.plan.as_ref())
                .all(|p| self.banned.contains(&p.value()))
    }

    /// Starts the next fight (after a respawn): a fresh tally and habits; the memory keeps
    /// everything before.
    pub fn start_fight(&mut self) {
        self.wolf_position = None;
        self.boss_position = None;
        self.wolf_velocity = [0.0; 2];
        self.idle_secs = 0.0;
        if !self.fight_over && !self.tally.is_empty() {
            self.end_fight(FightEnd::Unfinished);
        }
        let shared = self.tally.shared.clone();
        self.tally = Tally::default();
        self.tally.shared = shared;
        self.tally.phase = self.actions.chr.into();
        self.habits = HabitTracker::new(self.actions.back_steps);
        self.since_hit = [None, None];
        self.boss_open = None;
        self.wolf_open = None;
        self.tactic_open = false;
        self.idle_guard_secs = 0.0;
        self.guard_exchange = false;
        self.stall_recovery = false;
        self.fight_over = false;
        self.fight_no += 1;
        self.bandit_fight_start = self.bandit_choices.len();
        self.fight_bandit.clear();
        self.move_fighter.reset();
        self.bandit_pending = None;
        self.streaks.clear();
        self.tried.clear();
        self.set.clear();
        self.ready = None;
        self.picked = false;
        self.want_request = true;
        self.running = Plan::default();
        self.checkpoints = Checkpoints::default();
        self.wolf_events.clear();
        self.break_pending = false;
        self.cancel_break = true;
        self.pending_why = Some("start");
        self.fight = FightState {
            phase_changes: vec![(self.time, self.actions.chr.into())],
            cost_at_start: self.brain.costs().totals,
            metrics: MetricsTracker::new(self.time),
            tokens_at_start: self.brain.usage(),
            stats_at_start: self.brain.call_stats(),
            ..FightState::default()
        };
        self.bans_at_start = self.banned.clone();
        self.coach_done = [0; 5];
        self.mid_done = [0; 2];
        self.last_coach_sent = self.time;
        self.review_breaks = 0;
        self.mid_pending = None;
        self.mid_plan = None;
        self.staged_mid = None;
        self.set_banned_seen = false;
        self.pacing = Pacing::new(self.time);
        if self.brain.budget_exhausted() {
            self.game_plan = None;
            self.next_plan = None;
            self.opener = None;
            self.active_set = None;
            self.pending_why = None;
            self.want_request = false;
        } else if self.coach_mode {
            // The newest plan that has arrived plays; with none, the previous one stays.
            if let Some(plan) = self.next_plan.take() {
                self.game_plan = Some(plan);
            }
            self.install_plan();
            match (&self.game_plan, self.active_set_name()) {
                (Some(_), Some(set)) => {
                    eprintln!(
                        "coach: fight {} plays the coach's game plan, start set {set}",
                        self.fight_no
                    )
                }
                (Some(_), None) => {
                    eprintln!(
                        "coach: fight {} plays the coach's opener, then his own AI",
                        self.fight_no
                    )
                }
                (None, _) => eprintln!(
                    "coach: no game plan yet, fight {} is his own AI",
                    self.fight_no
                ),
            }
        }
        self.write(json!({"ev": "fight_start", "t": ms(self.time)}));
    }

    fn log_outcomes(&mut self, t: f32, done: &[Exchange]) {
        if self.log.is_none() {
            return;
        }
        for x in done {
            self.write_line(&OutcomeLine {
                ev: "outcome",
                t: ms(t),
                whiffed: x.whiffed(),
                exchange: x,
            });
        }
    }
}

impl Drop for BrainDriver {
    /// The game is closing: keep an unfinished fight in the memory file.
    fn drop(&mut self) {
        if self.session_report_path.is_some()
            && let Err(e) = self.print_session_report()
        {
            eprintln!("brain session report: {e}");
        }
        if self.memory_path.is_some() && !self.fight_over && !self.tally.is_empty() {
            self.save_memory();
        }
    }
}

/// Wolf's attack state as a tally key, shortened for requests: `GroundAttackCombo1Release`
/// becomes `AttackCombo1`.
pub fn wolf_key(state: &str) -> String {
    let short = state
        .strip_prefix("Ground")
        .unwrap_or(state)
        .replace("Release", "");
    if short.is_empty() {
        state.to_string()
    } else {
        short
    }
}

/// Seconds rounded to milliseconds, for the log.
fn ms(x: f32) -> f64 {
    (f64::from(x) * 1000.0).round() / 1000.0
}

#[derive(Serialize)]
struct RequestLine<'a> {
    ev: &'static str,
    t: f64,
    request: &'a BrainRequest,
}

#[derive(Serialize)]
struct OutcomeLine<'a> {
    ev: &'static str,
    t: f64,
    whiffed: bool,
    #[serde(flatten)]
    exchange: &'a Exchange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_brains_keep_unreachable_acts_and_do_not_generate_combos() {
        for kind in [
            crate::brain::BrainKind::Repeat(3000),
            crate::brain::BrainKind::Scripted,
        ] {
            let mut d = BrainDriver::new(kind.build("c1010"), &crate::brain::SOLDIER);
            d.relative = [20.0, 0.0];
            let row = super::super::moves::Move {
                hits: vec![[0.2, 40.0, 20.0, 1.0], [0.5, 40.0, 20.0, 2.0]],
                first_reach: Some(2.0),
                contact: true,
                ..Default::default()
            };
            assert!(!row.fits(d.relative, d.wolf_velocity));
            d.move_sheet.insert("3000".into(), row.clone());
            d.move_sheet.insert("3000+3000".into(), row);
            d.wolf_posture_left = 30.0;
            assert!(d.plan_fits(&Plan::new(&[3000])));
            assert!(d.breaking_combo().is_none());
        }
    }

    #[test]
    fn brain_sheet_gate_uses_bullet_range_beyond_melee_reach() {
        let mut d = learning_driver();
        d.move_sheet.insert(
            "3000".into(),
            super::super::moves::Move {
                hits: vec![[0.2, 20.0, 10.0, 1.0]],
                first_reach: Some(2.0),
                projectiles: vec![[0.2, 10.0, 2.0, 0.0, 10.0, 12.0, 0.0, 8.0, 4.0, 0.2]],
                contact: true,
                ..Default::default()
            },
        );
        for (distance, reachable) in [(8.0, true), (21.0, false)] {
            d.relative = [distance, 0.0];
            assert_eq!(d.plan_fits(&Plan::new(&[3000])), reachable);
        }
    }

    #[test]
    fn punish_preferences_and_neutral_bandit_only_choose_viable_options() {
        let mut d = learning_driver();
        d.punish_window = Some(1.3);
        d.relative = [1.0, 0.0];
        let view: Vec<_> = [(3000, 0.2), (3005, 0.8), (3007, 1.4)]
            .into_iter()
            .map(|(id, time)| {
                d.move_sheet.insert(
                    id.to_string(),
                    super::super::moves::Move {
                        hits: vec![[time, 20.0, 10.0, 100.0]],
                        damage: vec![[10.0, 20.0]],
                        first_reach: Some(4.0),
                        contact: true,
                        ..Default::default()
                    },
                );
                Weighted {
                    plan: Some(Plan::new(&[id])),
                    weight: 1.0,
                }
            })
            .collect();
        d.game_plan = Some(GamePlan {
            punish_preferences: BTreeMap::from([(
                "near".into(),
                vec![
                    super::super::coach::Avoid {
                        choice: "3007".into(),
                        reason: "too late".into(),
                    },
                    super::super::coach::Avoid {
                        choice: "3005".into(),
                        reason: "follow-up guard pressure".into(),
                    },
                ],
            )]),
            ..Default::default()
        });
        assert_eq!(d.pick_move(&view, true), Pick::Plan(1));
        d.game_plan.as_mut().unwrap().punish_preferences.clear();
        let mut picked = HashSet::new();
        for _ in 0..100 {
            let Pick::Plan(i) = d.pick_move(&view, true) else {
                panic!("missing viable punish")
            };
            assert!(i < 2);
            picked.insert(i);
        }
        assert_eq!(
            picked.len(),
            2,
            "neutral cold start explores both viable arms"
        );
        d.memory
            .punishes
            .entry(d.actions.chr.into())
            .or_default()
            .entry("near".into())
            .or_default()
            .insert(
                "3005".into(),
                super::super::moves::PunishEvidence {
                    uses: 100,
                    landed: 100,
                    converted: 100,
                    ..Default::default()
                },
            );
        assert_eq!(d.pick_move(&view, true), Pick::Plan(1));
    }

    #[test]
    fn brain_sheet_gate_predicts_retreat_and_break_punish_uses_fastest_reachable() {
        let mut d = learning_driver();
        for (id, time, reach) in [(3000, 0.8, 4.0), (3005, 0.2, 2.0), (3007, 0.4, 4.0)] {
            d.move_sheet.insert(
                id.to_string(),
                super::super::moves::Move {
                    hits: vec![[time, 20.0, 10.0, 100.0]],
                    first_reach: Some(reach),
                    contact: true,
                    ..Default::default()
                },
            );
        }
        d.relative = [3.0, 0.0];
        d.move_observation.dist = 3.0;
        d.wolf_velocity = [2.0, 0.0];
        assert!(!d.plan_fits(&Plan::new(&[3000])));
        assert!(d.plan_fits(&Plan::new(&[3007])));
        let view: Vec<_> = [3000, 3005, 3007]
            .iter()
            .map(|id| Weighted {
                plan: Some(Plan::new(&[*id])),
                weight: 1.0,
            })
            .collect();
        assert_eq!(d.pick_move(&view, true), Pick::Plan(2));
        d.relative = [1.0, 0.0];
        d.wolf_velocity = [0.0; 2];
        assert_eq!(d.pick_move(&view, true), Pick::Plan(1));
    }

    #[test]
    fn move_off_matches_the_previous_sampler_exactly_and_does_not_learn() {
        let mut d = learning_driver();
        d.brain_moves = false;
        d.seed(91);
        let mut expected = Rng::new(91);
        let view: Vec<_> = [3000, 3001, 3002]
            .into_iter()
            .map(|id| Weighted {
                plan: Some(d.actions.parse(&id.to_string()).unwrap()),
                weight: 1.0,
            })
            .collect();
        d.streaks.record("3000", true);
        for _ in 0..100 {
            let want = sampler::pick(
                &view,
                &Context {
                    streaks: &d.streaks,
                    tried: &d.tried,
                    temperature: d.temperature,
                },
                &mut expected,
            );
            assert_eq!(d.pick_move(&view, false), want);
        }
        d.settle(&[Exchange {
            phase: "c1010".into(),
            side: Side::Boss,
            key: "3000".into(),
            start: 0.0,
            end: Some(0.5),
            outcome: Default::default(),
        }]);
        assert!(d.move_fighter.provisional.is_empty());
    }

    #[test]
    fn recent_counter_failures_survive_a_fight_and_remain_coach_evidence() {
        let mut d = learning_driver();
        for i in 0..3 {
            d.settle(&[Exchange {
                phase: "c1010".into(),
                side: Side::Boss,
                key: "3000".into(),
                start: i as f32,
                end: Some(i as f32 + 0.5),
                outcome: super::super::tally::Outcome {
                    countered: Some(super::super::tally::CounterKind::Mikiri),
                    ..Default::default()
                },
            }]);
        }
        assert!(d.attack_factor(3000) > 0.0 && d.attack_factor(3000) < 0.1);
        for id in [3001, 3002, 3003, 3004, 3005, 3007, 3008, 3013, 3014] {
            d.memory.boss.insert(
                id.to_string(),
                Counts {
                    n: 100,
                    whiffed: 100,
                    ..Default::default()
                },
            );
        }
        let before_review = d.coach_input(&FightRecord::default());
        assert!(
            before_review
                .attack_rates
                .0
                .iter()
                .take(3)
                .any(|(key, _)| key == "3000"),
            "new counter failures must survive trimming ahead of old whiff totals"
        );
        d.end_fight(FightEnd::BossWon);
        assert_eq!(d.memory.attack_learning["c1010"]["3000"].n, 3);
        d.start_fight();
        assert!(d.attack_factor(3000) > 0.0 && d.attack_factor(3000) < 0.1);
        assert!(d.banned.is_empty());
        let mut input = d.coach_input(&FightRecord::default());
        input.fit(INPUT_BUDGET);
        assert_eq!(input.attack_rates.get("3000").unwrap()[2], 1.0);
        assert!(input.banned.is_empty());
    }

    #[test]
    fn move_fast_wins_teach_more_than_slow_wins_and_passive_wins_do_not_pay() {
        let reward = |seconds| {
            let mut d = learning_driver();
            d.time = seconds;
            d.move_fighter
                .runs
                .push((0.0, "near/blow/wlow/300".into(), "3000".into()));
            d.fight.metrics.metrics.input_secs = seconds;
            d.end_fight(FightEnd::BossWon);
            d.memory.move_bandit["c1010"]["near/blow/wlow/300"]["3000"].success
        };
        assert!(reward(5.0) > reward(20.0));
        // Passive wins create no terminal success observations.
        let mut d = learning_driver();
        d.move_fighter
            .runs
            .push((0.0, "near/blow/wlow/300".into(), "3000".into()));
        d.fight.metrics.metrics.passive_secs = d.time;
        d.end_fight(FightEnd::BossWon);
        assert!(d.memory.move_bandit.is_empty());
    }

    #[test]
    fn local_checkpoints_drive_real_duel_without_controller_requests_or_memory_writes() {
        struct LocalCoach;
        impl BossBrain for LocalCoach {
            fn name(&self) -> &'static str {
                "coach"
            }
            fn coaches(&self) -> bool {
                true
            }
            fn default_mode(&self) -> RequestMode {
                RequestMode::Checkpoints
            }
            fn checkpoint_chooser(&self) -> super::super::bandit::CheckpointChooser {
                super::super::bandit::CheckpointChooser::Local
            }
            fn submit(&mut self, _: &BrainRequest) {
                panic!("local checkpoint called controller");
            }
            fn poll(&mut self, _: f32) -> Option<BrainReply> {
                None
            }
        }
        let cache = crate::cache::root();
        if !cache.join("anim/c1010/tae.json").exists() {
            eprintln!("skipping real-data local chooser: soldier not extracted");
            return;
        }
        let mut duel = Duel::load_with(&cache, 2.5, crate::enemy::EnemySpec::soldier()).unwrap();
        duel.set_brain(Box::new(LocalCoach), false).unwrap();
        let driver = duel.boss_brain.as_mut().unwrap();
        driver.game_plan = Some(GamePlan::from_json(r#"{"sets":[{"name":"press","plans":[{"plan":"3000+3001","weight":1}]},{"name":"bait","plans":[{"plan":"3008","weight":1}]}],"start_set":"press"}"#).unwrap().validate(&crate::brain::SOLDIER).plan);
        driver.install_plan();
        for i in 0..600 {
            let input = crate::InputFrame {
                move_stick: [0.0, 0.1],
                buttons: crate::Buttons {
                    attack: i % 8 == 0,
                    guard: i % 8 != 0,
                    ..Default::default()
                },
                ..Default::default()
            };
            duel.step(&input, 1.0 / 60.0);
            for fighter in &mut duel.rules.fighters {
                fighter.vitality.hp = fighter.vitality.max_hp;
                fighter.posture.remaining = fighter.posture.max;
            }
        }
        let driver = duel.boss_brain.as_mut().unwrap();
        assert!(driver.memory_path().is_none());
        assert_eq!(driver.stats.requests, 0);
        assert!(!driver.bandit_choices.is_empty());
        assert!(
            driver
                .bandit_choices
                .iter()
                .any(|choice| choice.3.is_some())
        );
        assert!(driver.overlay().checkpoint.unwrap().contains("mean"));
        driver.end_fight(FightEnd::BossWon);
        assert!(!driver.memory.bandit.is_empty());
        let mut input = driver.coach_input(driver.last_record.as_ref().unwrap());
        assert!(!input.checkpoints.is_empty());
        assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
        assert!(driver.session_report().contains("Local checkpoints"));
        eprintln!(
            "local chooser: {} choices, zero controller requests, rewards retained",
            driver.bandit_choices.len()
        );
    }

    #[test]
    fn void_fights_discard_provisional_bandit_evidence() {
        let mut driver = learning_driver();
        driver
            .fight_bandit
            .entry("c1010".into())
            .or_default()
            .entry("start".into())
            .or_default()
            .entry("press".into())
            .or_default()
            .record(1.0);
        driver.time = 100.0;
        driver.end_fight(FightEnd::Unfinished);
        assert!(driver.memory.bandit.is_empty());
    }

    #[test]
    fn stalled_fights_turn_positive_bandit_rewards_into_failures() {
        let mut driver = learning_driver();
        driver
            .fight_bandit
            .entry("c1010".into())
            .or_default()
            .entry("start".into())
            .or_default()
            .entry("press".into())
            .or_default()
            .record(1.0);
        driver.fight.metrics.metrics.passive_secs = 3.0;
        driver.end_fight(FightEnd::BossWon);
        let arm = driver.memory.bandit["c1010"]["start"]["press"];
        assert_eq!((arm.success, arm.failure), (0.0, 1.0));
    }

    #[test]
    fn tomoe_plan_failures_and_counter_weights_are_phase_scoped() {
        use crate::brain::actions::{GENICHIRO, TOMOE};
        let mut driver = BrainDriver::new(Box::new(crate::brain::LuaBrain), &GENICHIRO);
        driver.memory.coach.failures.insert(
            "3000".into(),
            PlanFailures {
                uses: 25,
                failed: 24,
                ..Default::default()
            },
        );
        driver.memory.coach.failures.insert(
            "c7110:3000".into(),
            PlanFailures {
                uses: 3,
                fatal: 1,
                ..Default::default()
            },
        );
        assert_eq!(driver.plan_failures("3000").uses, 25);
        driver.actions = &TOMOE;
        assert_eq!(driver.plan_failures("3000").uses, 3);
        let plan = GamePlan {
            boss_phase: "c7110".into(),
            counter_weights: [("3043".into(), 0.0), ("3060".into(), 0.0)].into(),
            ..Default::default()
        };
        assert_eq!(plan.validate(&TOMOE).plan.counter_weights.len(), 1);
        assert!(plan.validate(&GENICHIRO).plan.counter_weights.is_empty());
    }

    #[test]
    fn exhausted_budget_keeps_current_plan_then_resets_to_own_ai() {
        struct BudgetBrain(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl BossBrain for BudgetBrain {
            fn name(&self) -> &'static str {
                "coach"
            }
            fn coaches(&self) -> bool {
                true
            }
            fn budget_exhausted(&self) -> bool {
                self.0.load(std::sync::atomic::Ordering::Relaxed)
            }
            fn submit(&mut self, _: &BrainRequest) {
                panic!("no request after budget");
            }
            fn submit_coach(&mut self, _: &CoachRequest, _: f32) {
                panic!("no coach after budget");
            }
            fn poll(&mut self, _: f32) -> Option<BrainReply> {
                None
            }
        }
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut d = BrainDriver::new(
            Box::new(BudgetBrain(stopped.clone())),
            &crate::brain::SOLDIER,
        );
        let plan = GamePlan::from_json(r#"{"sets":[{"name":"press","plans":[{"plan":"3000","weight":1}]}],"start_set":"press"}"#).unwrap();
        d.game_plan = Some(plan);
        d.install_plan();
        assert!(!d.set.is_empty());
        stopped.store(true, std::sync::atomic::Ordering::Relaxed);
        d.submit_coach_input(CoachInput::default(), true);
        assert!(d.game_plan.is_some());
        assert!(!d.set.is_empty());
        assert!(d.coach_flights.is_empty());
        d.start_fight();
        assert!(d.game_plan.is_none() && d.set.is_empty() && d.opener.is_none());
        assert!(d.active_set.is_none() && !d.want_request);
        assert_eq!(d.overlay().running, "Lua");
    }

    #[test]
    fn overlay_tracks_steps_and_expires_cards_without_logging() {
        let mut d = learning_driver();
        d.running = d.actions.parse("walk_in4>hold2>3013").unwrap();
        d.overlay_events.step = 1;
        d.overlay_events.checkpoint = Some((3.0, "wolf_posture_50 -> keep".into()));
        d.overlay_events.ban = Some((5.0, "banned 3010: 9/9 whiffs".into()));
        d.overlay_events.scorecard = Some((5.0, "boss wins".into()));
        d.memory.coach.plan = Some(GamePlan {
            principles: vec!["Wolf guards after a kick".into()],
            boss_principles: vec!["Kick closes distance".into()],
            ..Default::default()
        });
        d.overlay_events.coach = Some((5.0, "coach (mid-fight, 14 s): press".into()));
        d.coach_flights.push(CoachFlight {
            id: 1,
            sent: 5.0,
            mid: true,
            fight: 1,
        });
        let view = d.overlay();
        assert_eq!(view.running, "walk_in4 > [hold2] > 3013");
        assert!(view.checkpoint.unwrap().ends_with("2.0 s ago"));
        assert!(view.thinking && view.coach.is_some() && view.ban.is_some());
        assert!(view.principles[0].starts_with("Boss ·"));
        assert!(view.principles[1].starts_with("Player ·"));
        d.time = 10.0;
        assert!(d.overlay().ban.is_none());
        assert!(d.overlay().scorecard.is_some());
        d.time = 11.0;
        assert!(d.overlay().scorecard.is_none());
        d.time = 13.0;
        assert!(d.overlay().coach.is_none());
        assert_eq!(d.overlay().principles.len(), 2);
        d.running = Plan::default();
        assert_eq!(d.overlay().running, "Lua");
        let mut owl = BrainDriver::new(Box::new(crate::brain::LuaBrain), &crate::brain::OWL);
        owl.running = owl.actions.parse("3009").unwrap();
        owl.boss_open = Some(3014);
        assert_eq!(owl.overlay().label, "second shuriken");
    }

    #[test]
    fn overlay_uses_newest_accepted_coach_note_and_shared_ban_reason() {
        let mut d = learning_driver();
        d.coach_flights.push(CoachFlight {
            id: 2,
            sent: 1.0,
            mid: true,
            fight: 1,
        });
        d.take_coach_reply(CoachReply {
            id: 2,
            plan: Some(GamePlan {
                opener: "3005".into(),
                notes: "new lesson".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(
            d.overlay().coach.as_deref(),
            Some("coach (mid-fight, 4 s): new lesson")
        );
        d.coach_flights.push(CoachFlight {
            id: 1,
            sent: 1.0,
            mid: true,
            fight: 1,
        });
        d.take_coach_reply(CoachReply {
            id: 1,
            plan: Some(GamePlan {
                opener: "3005".into(),
                notes: "old lesson".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(d.overlay().coach.unwrap().contains("new lesson"));
        d.boss_knowledge.bans.push(super::super::coach::Avoid {
            choice: "3000".into(),
            reason: "shared: 9/9 whiffs".into(),
        });
        d.apply_boss_bans();
        assert!(d.overlay().ban.is_none(), "legacy automatic ban is retired");
        d.boss_knowledge.bans[0].reason = "Coach: repeated shared whiffs".into();
        d.apply_boss_bans();
        assert_eq!(
            d.overlay().ban.as_deref(),
            Some("banned 3000: Coach: repeated shared whiffs")
        );
    }

    fn layer_test_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sekiro-layers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("c1010.json")
    }

    #[test]
    fn new_player_uses_rich_shared_plan_and_bans_but_not_other_player_habits() {
        let path = layer_test_path();
        let mut a = learning_driver();
        a.coach_mode = true;
        a.use_player_memory(path.clone(), "alice");
        a.memory.fights = 21;
        a.memory.habits.jumps = 77;
        a.memory.coach.failures.insert(
            "3000".into(),
            PlanFailures {
                uses: 9,
                fatal: 9,
                ..Default::default()
            },
        );
        a.memory.boss.insert(
            "3003".into(),
            Counts {
                n: 9,
                whiffed: 9,
                ..Default::default()
            },
        );
        a.coach_flights.push(CoachFlight {
            id: 1,
            sent: a.time,
            mid: false,
            fight: a.fight_no,
        });
        a.take_coach_reply(CoachReply {
            id: 1,
            plan: Some(GamePlan {
                opener: "3005".into(),
                boss_principles: vec!["kick has short recovery".into()],
                boss_bans: vec![super::super::coach::Avoid {
                    choice: "3003".into(),
                    reason: "Coach: repeated shared whiffs".into(),
                }],
                principles: vec!["Alice jumps".into()],
                notes: "Alice is aggressive".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        a.boss_knowledge
            .consider(a.memory.coach.plan.as_ref().unwrap(), &a.memory);
        a.save_memory();
        let mut b = learning_driver();
        b.coach_mode = true;
        b.use_player_memory(path.clone(), "bob");
        assert_eq!(b.game_plan.as_ref().unwrap().opener, "3005");
        assert_eq!(b.opener.as_ref().unwrap().value(), "3005");
        assert!(b.banned.contains("3003"));
        assert!(!b.banned.contains("3000"));
        assert_eq!(b.memory.habits.jumps, 0);
        let request = serde_json::to_value(b.coach_input(&FightRecord::default())).unwrap();
        assert_eq!(
            request["boss_knowledge"]["principles"][0],
            "kick has short recovery"
        );
        assert!(!request.to_string().contains("Alice"));
        b.memory.fights = 1;
        b.save_memory();
        b.save_memory();
        assert_eq!(b.boss_knowledge.evidence.fights, 22);
        assert_eq!(b.boss_knowledge.summary().players, 2);
        // A stale Alice driver must not overwrite Bob's durable contribution.
        a.save_memory();
        assert_eq!(a.boss_knowledge.evidence.fights, 22);
        let paths = LayerPaths::new(&layer_test_path(), "bob");
        let existing = LayerPaths::new(&path, "bob");
        let (mut boss, profile) = existing.load("c1010").unwrap();
        boss.players.get_mut("alice").unwrap().fights = 2;
        boss.update("bob", &profile);
        paths.save(&boss, &profile).unwrap();
        let mut c = learning_driver();
        c.coach_mode = true;
        c.use_player_memory(paths.legacy, "charlie");
        assert!(c.game_plan.is_none() && c.opener.is_none());
        assert!(c.banned.contains("3003"));
    }

    #[test]
    fn a_lua_fallback_does_not_hide_a_banned_active_set() {
        let mut d = learning_driver();
        d.set = vec![
            Weighted {
                plan: Some(d.actions.parse("3000").unwrap()),
                weight: 1.0,
            },
            Weighted {
                plan: None,
                weight: 0.25,
            },
        ];
        assert!(!d.active_set_banned());
        d.banned.insert("3000".into());
        assert!(d.active_set_banned());
        d.set.push(Weighted {
            plan: Some(d.actions.parse("3001").unwrap()),
            weight: 1.0,
        });
        assert!(!d.active_set_banned());
    }

    #[test]
    fn coach_keeps_lessons_and_reports_lua_actions_separately() {
        let mut d = learning_driver();
        d.memory.coach.plan = Some(GamePlan {
            principles: vec!["kick 3005 lands".into()],
            ..Default::default()
        });
        d.game_plan = d.memory.coach.plan.clone();
        d.tally.record(
            0.0,
            TallyEvent::Start {
                side: Side::Boss,
                key: "3005".into(),
            },
        );
        d.tally.record(
            0.5,
            TallyEvent::Hit {
                attacker: Side::Boss,
                guarded: false,
                deflected: false,
            },
        );
        d.tally.flush(3.0);
        d.fight.action_ranges.insert("3005".into(), (2, 4.0, 6.0));
        let mut input = d.coach_input(&FightRecord::default());
        assert_eq!(input.principles, ["kick 3005 lands"]);
        assert!(input.played.as_ref().unwrap().principles.is_empty());
        assert_eq!(input.actions["3005"].hit, 1);
        assert_eq!(input.actions["3000"].n, 0);
        assert_eq!(input.action_range["3005"], [2.0, 3.0]);
        assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
        assert_eq!(input.principles, ["kick 3005 lands"]);
        let encoded = serde_json::to_string(&d.memory).unwrap();
        let decoded: Memory = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.coach.plan.unwrap().principles, ["kick 3005 lands"]);
    }

    #[test]
    fn session_report_contains_action_ranges_and_break_trend_and_appends_at_exit() {
        let mut d = learning_driver();
        d.session_actions.insert(
            "3000".into(),
            Counts {
                n: 2,
                hit: 1,
                whiffed: 1,
                ..Default::default()
            },
        );
        d.session_ranges.insert("3000".into(), (2, 8.0, 6.0));
        d.session_records.push(FightRecord {
            winner: "boss".into(),
            metrics: super::super::metrics::FightMetrics {
                duration: 31.0,
                time_to_kill_wolf: Some(31.0),
                wolf_posture_breaks: 2,
                hits_after_break: 1,
                break_punish_rate: 0.5,
                wolf_posture_peak: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });
        let report = d.session_report();
        assert!(report.contains("| 3000 | 2 | 1 | 0 | 0 | 1 | 0 | 0 | 4.00 / 3.00 |"));
        assert!(report.contains("| 1 | boss | 31.0 | 31.0 | 2 | 1 | 50% | 100% |"));
        d.enable_session_report(&std::env::temp_dir());
        let path = d.session_report_path.clone().unwrap();
        d.print_session_report().unwrap();
        drop(d);
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.matches("# Brain session:").count(), 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn wolf_posture_checkpoints_are_once_per_cycle_and_break_is_an_edge() {
        let mut c = Checkpoints::default();
        assert_eq!(c.on_wolf(0.26, false).0, ["wolf_posture_25"]);
        assert!(c.on_wolf(0.3, false).0.is_empty());
        assert_eq!(
            c.on_wolf(0.76, false).0,
            ["wolf_posture_50", "wolf_posture_75"]
        );
        assert_eq!(c.on_wolf(1.0, true), (vec!["wolf_broken"], true));
        assert!(c.on_wolf(1.0, true).0.is_empty());
        assert!(c.on_wolf(0.2, false).0.is_empty());
        assert!(c.on_wolf(0.14, false).0.is_empty());
        assert_eq!(
            c.on_wolf(0.5, false).0,
            ["wolf_posture_25", "wolf_posture_50"]
        );
    }

    #[test]
    fn failed_break_responses_are_reported_independently_without_automatic_bans() {
        let mut d = learning_driver();
        for i in 0..3 {
            let start = i as f32;
            d.fight
                .runs
                .push((start, "3000".into(), "on_wolf_break".into()));
            d.fight.uses.push(PlanUse {
                break_response: true,
                failed: false,
                text: "on_wolf_break:3000".into(),
                start,
                last_attack: Some(start),
                attacks: 1,
                deflected: 0,
                fatal: false,
                settled: false,
            });
            d.attribute(&[Exchange {
                phase: "c5060".into(),
                side: Side::Boss,
                key: "3000".into(),
                start,
                end: Some(start + 0.5),
                outcome: super::super::tally::Outcome {
                    punished: i == 1,
                    hit: i == 2,
                    ..Default::default()
                },
            }]);
        }
        d.settle_plans(true);
        assert!(!d.banned.contains("on_wolf_break:3000"));
        assert!(!d.banned.contains("3000"));
        assert_eq!(d.fight.break_plans["3000"].n, 3);
        assert_eq!(d.fight.break_plans["3000"].hit, 1);
        assert_eq!(d.fight.break_plans["3000"].whiffed, 2);
        assert_eq!(d.fight.break_plans["3000"].punished, 1);
    }

    #[test]
    fn brain_idle_guard_fails_a_plan_even_when_no_attack_could_start() {
        let mut d = learning_driver();
        d.fight
            .runs
            .push((0.0, "guard1>3000".into(), "press".into()));
        d.fight.uses.push(PlanUse {
            text: "guard1>3000".into(),
            start: 0.0,
            last_attack: None,
            attacks: 0,
            deflected: 0,
            failed: false,
            fatal: false,
            settled: false,
            break_response: false,
        });
        d.settle(&[Exchange {
            phase: "c1010".into(),
            side: Side::Boss,
            key: "guard1".into(),
            start: 0.0,
            end: Some(1.0),
            outcome: super::super::tally::Outcome {
                idle_guard: true,
                ..Default::default()
            },
        }]);
        d.settle_plans(true);
        assert_eq!(d.fight.failures["guard1>3000"].failed, 1);
        assert_eq!(d.fight.by_plan["guard1>3000"].whiffed, 0);
        assert_eq!(d.streaks.get("guard1"), 1);
        assert_eq!(d.fight.metrics.snapshot(d.time).boss_attacks, 0);
    }

    fn learning_driver() -> BrainDriver {
        let mut d = BrainDriver::new(Box::new(crate::brain::LuaBrain), &crate::brain::SOLDIER);
        d.time = 5.0;
        d.fight.metrics.input_tick(
            &crate::InputFrame {
                move_stick: [0.0, 1.0],
                buttons: crate::Buttons {
                    attack: true,
                    guard: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            0.1,
            5.0,
        );
        d
    }

    #[test]
    fn a_fight_that_becomes_inactive_discards_its_provisional_learning() {
        let mut d = learning_driver();
        d.fight.failures.insert(
            "3000".into(),
            PlanFailures {
                failed: 0,
                uses: 3,
                deflected_all: 3,
                fatal: 0,
            },
        );
        d.settle_plans(false);
        assert!(!d.banned.contains("3000"));
        d.time = 20.0;
        d.end_fight(FightEnd::BossWon);
        assert!(d.last_record.as_ref().unwrap().metrics.void);
        assert!(d.banned.is_empty());
        assert!(d.memory.coach.failures.is_empty());
        assert!(d.memory.history.is_empty());
        assert_eq!(d.memory.fights, 0);
    }

    #[test]
    fn losing_plan_is_reported_and_only_a_reasoned_retry_reopens_a_coach_ban() {
        let mut d = learning_driver();
        d.memory.coach.failures.insert(
            "3000".into(),
            PlanFailures {
                failed: 0,
                uses: 2,
                deflected_all: 1,
                fatal: 0,
            },
        );
        d.fight.runs = vec![
            (0.0, "3000".into(), "press".into()),
            (1.0, "lua".into(), "lua".into()),
        ];
        d.fight.uses.push(PlanUse {
            break_response: false,
            failed: false,
            text: "3000".into(),
            start: 0.0,
            last_attack: Some(0.1),
            attacks: 1,
            deflected: 0,
            fatal: false,
            settled: false,
        });
        d.settle(&[Exchange {
            phase: "c5060".into(),
            side: Side::Boss,
            key: "3000".into(),
            start: 0.1,
            end: Some(0.7),
            outcome: super::super::tally::Outcome {
                deflected: true,
                ..Default::default()
            },
        }]);
        d.settle_plans(false);
        assert!(!d.banned.contains("3000"));
        assert_eq!(d.fight.failures["3000"].deflected_all, 1);
        d.end_fight(FightEnd::BossWon);
        assert_eq!(d.memory.coach.failures["3000"].uses, 3);
        assert_eq!(d.memory.coach.failures["3000"].deflected_all, 2);
        let mut input = d.coach_input(d.last_record.as_ref().unwrap());
        assert!(input.banned.is_empty());
        assert_eq!(input.failures["3000"], [3, 2, 0, 0]);
        assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
        d.start_fight();
        assert!(!d.banned.contains("3000"));
        d.banned.insert("3000".into());
        for (id, reason) in [(1, ""), (2, "Try after a bait to change the timing")] {
            d.coach_flights.push(CoachFlight {
                id,
                sent: d.time,
                mid: false,
                fight: d.fight_no,
            });
            d.take_coach_reply(CoachReply {
                id,
                plan: Some(GamePlan {
                    opener: "3000".into(),
                    retry: vec![super::super::coach::Avoid {
                        choice: "3000".into(),
                        reason: reason.into(),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            });
            assert_eq!(d.banned.contains("3000"), reason.is_empty());
        }
        // A retry preserves history; new failures remain evidence for coach judgement.
        assert_eq!(d.memory.coach.failures["3000"].uses, 3);
        d.fight.failures.insert(
            "3000".into(),
            PlanFailures {
                failed: 0,
                uses: 3,
                deflected_all: 2,
                fatal: 0,
            },
        );
        d.time = 10.0;
        d.fight.metrics.input_tick(
            &crate::InputFrame {
                move_stick: [0.0, 1.0],
                buttons: crate::Buttons {
                    attack: true,
                    guard: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            5.1,
            5.0,
        );
        d.settle_plans(false);
        assert!(!d.banned.contains("3000"));
    }

    #[test]
    fn fatal_window_and_punished_posture_break_are_attributed_once() {
        let mut d = learning_driver();
        d.fight.runs.push((0.0, "3000".into(), "press".into()));
        d.fight.uses.push(PlanUse {
            break_response: false,
            failed: false,
            text: "3000".into(),
            start: 0.0,
            last_attack: Some(1.0),
            attacks: 1,
            deflected: 0,
            fatal: false,
            settled: false,
        });
        d.fatal_plans(4.1, false);
        assert!(!d.fight.uses[0].fatal);
        d.fatal_plans(4.0, false);
        assert!(d.fight.uses[0].fatal);
        d.fight.uses[0].fatal = false;
        d.fatal_plans(4.1, true);
        assert!(d.fight.uses[0].fatal);
        d.settle_plans(true);
        d.settle_plans(true);
        assert_eq!(d.fight.failures["3000"].fatal, 1);
        assert_eq!(d.fight.failures["3000"].uses, 1);
        assert_eq!(d.fight.failures["3000"].deflected_all, 0);
    }

    #[test]
    fn dying_during_the_last_attack_is_fatal_when_the_fight_flushes() {
        let mut d = learning_driver();
        d.fight.runs.push((0.0, "3000".into(), "press".into()));
        d.fight.uses.push(PlanUse {
            break_response: false,
            failed: false,
            text: "3000".into(),
            start: 0.0,
            last_attack: Some(1.0),
            attacks: 1,
            deflected: 0,
            fatal: false,
            settled: false,
        });
        d.tally.record(
            1.0,
            TallyEvent::Start {
                side: Side::Boss,
                key: "3000".into(),
            },
        );
        d.end_fight(FightEnd::WolfWon);
        assert_eq!(d.memory.coach.failures["3000"].fatal, 1);
    }

    #[test]
    fn posture_checkpoints_rearm_and_leave_time_to_act() {
        let mut c = Checkpoints::default();
        assert_eq!(c.on_vitals(0.4, 1.0, Some(6.1)), None);
        assert_eq!(c.on_vitals(0.41, 1.0, Some(6.0)), Some("pressure"));
        assert_eq!(c.on_vitals(0.5, 1.0, Some(3.1)), None);
        assert_eq!(c.on_vitals(0.6, 1.0, Some(3.0)), Some("pressure_critical"));
        assert_eq!(c.on_vitals(0.3, 1.0, Some(6.0)), None);
        assert_eq!(c.on_vitals(0.6, 1.0, Some(2.0)), None);
        assert_eq!(c.on_vitals(0.29, 1.0, Some(8.0)), None);
        assert_eq!(c.on_vitals(0.6, 1.0, Some(2.0)), Some("pressure_critical"));
        let mut c = Checkpoints::default();
        assert_eq!(c.on_vitals(0.9, 1.0, Some(1.9)), None);
        assert_eq!(c.on_vitals(0.9, 0.4, None), Some("half_hp"));
    }
    #[test]
    fn wolf_landing_two_hits_in_a_row_raises_one_checkpoint() {
        let mut c = Checkpoints::default();
        assert_eq!(c.on_hit(Side::Wolf, false, false), None);
        assert_eq!(c.on_hit(Side::Wolf, false, false), Some("punished_twice"));
        // Once per fight.
        assert_eq!(c.on_hit(Side::Wolf, false, false), None);
        // A boss hit that lands breaks the run; a hit he guards does not count as a landing.
        let mut c = Checkpoints::default();
        c.on_hit(Side::Wolf, false, false);
        c.on_hit(Side::Boss, false, false);
        assert_eq!(c.on_hit(Side::Wolf, false, false), None);
        c.on_hit(Side::Wolf, true, false);
        assert_eq!(c.on_hit(Side::Wolf, false, false), Some("punished_twice"));
    }

    #[test]
    fn three_deflects_in_a_row_raise_one_checkpoint() {
        let mut c = Checkpoints::default();
        assert_eq!(c.on_hit(Side::Boss, true, true), None);
        assert_eq!(c.on_hit(Side::Boss, true, true), None);
        // A guard that is not a deflect breaks the run.
        assert_eq!(c.on_hit(Side::Boss, true, false), None);
        assert_eq!(c.on_hit(Side::Boss, true, true), None);
        assert_eq!(c.on_hit(Side::Boss, true, true), None);
        assert_eq!(c.on_hit(Side::Boss, true, true), Some("deflected_thrice"));
        assert_eq!(c.on_hit(Side::Boss, true, true), None);
    }
}
