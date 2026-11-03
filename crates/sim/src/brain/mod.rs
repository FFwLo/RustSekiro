//! A pluggable boss brain: the boss's high-level choices (which of his own actions to string
//! together next) can come from his Lua AI or from an outside controller, while his reflexes
//! (deflecting, guarding, reactions, getting up) stay with the Lua goal runtime.
//!
//! - [`Observation`] is what a controller sees: only what the player can see, mirrored.
//! - [`Tally`] is this fight's outcome memory, and [`Memory`] the memory of every fight with
//!   Wolf's habits, kept here (and on disk) because hosted models remember nothing.
//! - A [`Plan`] is 1 to 3 of the boss's own action ids (his [`BossActions`] table, such as
//!   [`actions::OWL`] or [`actions::SOLDIER`]); it runs through his battle goal exactly like
//!   one of his acts (`lua_ai::ActHook`).
//! - [`BossBrain`] is the controller, with a non-blocking submit / poll API; [`BrainDriver`]
//!   keeps one request in flight and falls back to the Lua AI's own act when no answer is
//!   ready, so the boss never freezes.
//!
//! See `docs/BRAIN.md`.

pub mod actions;
pub mod adaptation;
pub mod bandit;
pub mod coach;
pub mod cost;
pub mod digest;
pub mod driver;
pub mod habits;
pub mod limits;
pub mod memory;
pub mod metrics;
pub mod move_bandit;
#[cfg(test)]
mod move_tests;
pub mod moves;
pub mod observation;
pub mod plan;
pub mod sampler;
pub mod tally;

#[cfg(test)]
mod tests;

use serde::Serialize;

pub use actions::{BossAction, BossActions, Combo, GENICHIRO, OWL, SOLDIER};
pub use coach::{
    CallStats, CoachInput, CoachReply, CoachRequest, GamePlan, PlanSet, ScriptedCoach, SetOffer,
    Validated,
};
pub use driver::{BrainDriver, BrainStats};
pub use habits::{HabitSummary, HabitTracker, Habits, WolfFrame};
pub use memory::{FightEnd, Loaded, Memory, MemorySummary};
pub use metrics::{FightMetrics, FightRecord, Trend};
pub use observation::{FighterView, Observation};
pub use plan::{Plan, Step, Tactic, Weighted};
pub use tally::{Counts, Exchange, Outcome, Side, Tally, TallyEvent, TallySummary, TopMap};

/// The most bytes of JSON a request should take; [`BrainRequest::fit`] trims to it.
pub const REQUEST_BUDGET: usize = 1400;

/// One question to a controller: the observation, this fight's tally and the memory of every
/// fight.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BrainRequest {
    /// Same facts as the coach, bounded to four stable ids for a break decision.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub viable_punishes: Vec<moves::Punish>,
    /// Increasing per session; the reply carries it back.
    pub id: u64,
    pub obs: Observation,
    pub tally: TallySummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemorySummary>,
    /// Why this request is sent now (checkpoint mode: `start`, `pressure`, `pressure_critical`, `half_hp`, `mark_lost`,
    /// `punished_twice`, `deflected_thrice`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<&'static str>,
    /// Coach mode: the plan set being played now (the chooser's `keep`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// Coach mode: the sets the checkpoint chooser picks from (a request body offers them, with
    /// `keep`; not part of the input text).
    #[serde(skip)]
    pub sets: Vec<SetOffer>,
    /// Coach mode: checkpoint scalars and at most 768 bytes of coach reasoning;
    /// a hosted controller sends this instead of the serialized request.
    #[serde(skip)]
    pub brief: Option<String>,
    /// Plan texts a hosted controller must not offer this time (deflected or guarded three
    /// times in a row); the request body leaves them out of the choices (not sent as input).
    #[serde(skip)]
    pub excluded: Vec<String>,
}

impl BrainRequest {
    /// Bytes of compact JSON.
    pub fn json_len(&self) -> usize {
        serde_json::to_string(self).map_or(0, |s| s.len())
    }

    /// Trims the least telling entries until the JSON fits in `budget` bytes (or nothing is
    /// left to trim; the observation always stays); returns the final size. Order: earlier
    /// fights' boss counts, Wolf counts, habit timings, answers and responses, then this
    /// fight's Wolf and boss counts, each down to 4, 3, 2, 1 and 0 entries; last the single
    /// habit values and the memory itself.
    pub fn fit(&mut self, budget: usize) -> usize {
        let mut len = self.json_len();
        while len > budget && self.trim_one() {
            len = self.json_len();
        }
        len
    }

    fn trim_one(&mut self) -> bool {
        for keep in [4, 3, 2, 1, 0] {
            if self.memory.as_mut().is_some_and(|m| m.boss.pop(keep)) {
                return true;
            }
            if self.memory.as_mut().is_some_and(|m| m.wolf.pop(keep)) {
                return true;
            }
            if keep > 0 && self.memory.as_mut().is_some_and(|m| m.habits.trim(keep)) {
                return true;
            }
            if self.tally.wolf.pop(keep) || self.tally.boss.pop(keep) {
                return true;
            }
        }
        if self.memory.as_mut().is_some_and(|m| m.trim(0)) {
            return true;
        }
        self.memory.take().is_some()
    }
}

/// A controller's answer: a plan, and optionally the whole plan set behind it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrainReply {
    /// The request answered.
    pub id: u64,
    /// The plan (the controller's best choice), or `None` to leave this act to the Lua AI.
    pub plan: Option<Plan>,
    /// The whole answer as a weighted set of plans (a hosted model's probability per choice).
    /// Empty means the set is just `plan`. The driver samples from it locally, with its
    /// penalties, so one answer can serve many acts.
    pub set: Vec<Weighted>,
    /// Coach mode: the plan set the chooser picked (`keep` or `None` leaves the current one).
    pub choice: Option<String>,
}

impl BrainReply {
    /// An answer that is one plan (or `None`: leave it to the Lua AI).
    pub fn single(id: u64, plan: Option<Plan>) -> Self {
        Self {
            id,
            plan,
            set: Vec::new(),
            choice: None,
        }
    }
}

/// When the driver asks the controller for a decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestMode {
    /// After every act slot; the answer's plan runs once, at the next slot.
    #[default]
    Slot,
    /// When the last step of the running plan starts, so the answer is ready as it ends (and
    /// at slots with nothing waiting).
    PlanEnd,
    /// Only at events, each at most once per fight (fight start, the boss at half HP, a lost
    /// deathblow mark, Wolf punishing twice in a row, Wolf deflecting three times in a row; a
    /// mark can be lost more than once). An answer is a plan set the boss cycles through
    /// until the next one.
    Checkpoints,
}

impl RequestMode {
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "slot" => Some(Self::Slot),
            "plan_end" | "plan-end" | "planend" => Some(Self::PlanEnd),
            "checkpoints" | "checkpoint" => Some(Self::Checkpoints),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Slot => "slot",
            Self::PlanEnd => "plan_end",
            Self::Checkpoints => "checkpoints",
        }
    }
}

/// A boss controller. Calls never block: a controller that needs time (a hosted model) works
/// on a background thread and answers through [`BossBrain::poll`].
pub trait BossBrain {
    fn checkpoint_chooser(&self) -> bandit::CheckpointChooser {
        bandit::CheckpointChooser::Decisions
    }
    fn change_phase(&mut self, _actions: &'static BossActions) {}
    fn costs(&self) -> cost::CostSnapshot {
        cost::CostSnapshot::default()
    }
    fn budget_exhausted(&self) -> bool {
        false
    }
    /// Short name for logs (`lua`, `scripted`).
    fn name(&self) -> &'static str;
    /// Whether it ever plans; `false` leaves every act to the Lua AI and sends no requests.
    fn plans(&self) -> bool {
        true
    }
    /// Whether plans must run verbatim, without reach gating or generated alternatives.
    fn fixed_plans(&self) -> bool {
        false
    }
    /// The request mode the driver uses unless told otherwise.
    fn default_mode(&self) -> RequestMode {
        RequestMode::Slot
    }
    /// Input and output tokens used so far (0 for a controller that has none).
    fn usage(&self) -> (u64, u64) {
        (0, 0)
    }
    /// Whether this controller is a coach: the driver then sends the fight summary after each
    /// fight ([`BossBrain::submit_coach`]) and uses its game plans, and the checkpoint requests
    /// choose among the plan's sets.
    fn coaches(&self) -> bool {
        false
    }
    /// Per kind of call, `[checkpoint, coach]` (calls, tokens, latency).
    fn call_stats(&self) -> [CallStats; 2] {
        [CallStats::default(); 2]
    }
    /// Takes the coach's request (never blocks); `now` is the session clock in seconds.
    fn submit_coach(&mut self, _request: &CoachRequest, _now: f32) {}
    /// A coach answer, once ready (any time, also during a respawn).
    fn poll_coach(&mut self, _now: f32) -> Option<CoachReply> {
        None
    }
    /// Takes a request. The driver keeps at most one in flight.
    fn submit(&mut self, request: &BrainRequest);
    /// The answer to the request in flight, once ready. `now` is the fight clock in seconds
    /// (deterministic controllers model their latency on it; real ones ignore it).
    fn poll(&mut self, now: f32) -> Option<BrainReply>;
}

/// Today's behaviour: the Lua AI chooses every act.
#[derive(Debug, Clone, Copy, Default)]
pub struct LuaBrain;

impl BossBrain for LuaBrain {
    fn name(&self) -> &'static str {
        "lua"
    }
    fn plans(&self) -> bool {
        false
    }
    fn submit(&mut self, _request: &BrainRequest) {}
    fn poll(&mut self, _now: f32) -> Option<BrainReply> {
        None
    }
}

/// A deterministic test controller: opens with [`ScriptedBrain::opener`] every time until its
/// first action has been deflected [`ScriptedBrain::switch_after_deflects`] times, then uses
/// [`ScriptedBrain::fallback_plan`]. Answers [`ScriptedBrain::delay`] seconds of fight time after
/// each request, to exercise the timeout path.
#[derive(Debug, Clone)]
pub struct ScriptedBrain {
    follow_phase: bool,
    fixed_plans: bool,
    pub opener: Plan,
    pub fallback_plan: Plan,
    pub switch_after_deflects: u32,
    /// Artificial decision latency, seconds of fight time.
    pub delay: f32,
    pending: Option<(u64, f32, Plan)>,
}

impl Default for ScriptedBrain {
    /// The Owl's: kick (3005); after two deflected kicks, the recovery-sealing throw (3031).
    fn default() -> Self {
        Self::for_actions(&OWL)
    }
}

impl ScriptedBrain {
    pub fn new(opener: Plan, fallback_plan: Plan, switch_after_deflects: u32, delay: f32) -> Self {
        Self {
            opener,
            follow_phase: false,
            fixed_plans: true,
            fallback_plan,
            switch_after_deflects,
            delay,
            pending: None,
        }
    }

    /// Uses scripted proposals to exercise adaptive reach gating and move selection.
    pub fn with_move_selection(mut self) -> Self {
        self.fixed_plans = false;
        self
    }

    /// The boss's own test plans ([`BossActions::scripted`]): switches after two deflected
    /// openers, answering after 0.1 s.
    pub fn for_actions(actions: &BossActions) -> Self {
        let [opener, fallback] = actions.scripted;
        let mut brain = Self::new(Plan::new(opener), Plan::new(fallback), 2, 0.1);
        brain.follow_phase = true;
        brain
    }

    /// The plan this controller picks for a tally.
    pub fn choose(&self, tally: &TallySummary) -> Plan {
        let deflected = self
            .opener
            .actions()
            .first()
            .and_then(|a| tally.boss.get(&a.to_string()))
            .map_or(0, |c| c.deflected);
        if deflected >= self.switch_after_deflects {
            self.fallback_plan.clone()
        } else {
            self.opener.clone()
        }
    }
}

impl BossBrain for ScriptedBrain {
    fn fixed_plans(&self) -> bool {
        self.fixed_plans
    }
    fn change_phase(&mut self, actions: &'static BossActions) {
        if self.follow_phase {
            let next = Self::for_actions(actions);
            self.opener = next.opener;
            self.fallback_plan = next.fallback_plan;
            self.pending = None;
        }
    }
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn submit(&mut self, request: &BrainRequest) {
        let plan = self.choose(&request.tally);
        self.pending = Some((request.id, request.obs.t + self.delay, plan));
    }
    fn poll(&mut self, now: f32) -> Option<BrainReply> {
        let (id, ready_at, _) = self.pending.as_ref()?;
        if now + 1e-4 < *ready_at {
            return None;
        }
        let id = *id;
        let (_, _, plan) = self.pending.take()?;
        Some(BrainReply::single(id, Some(plan)))
    }
}

/// Which controller a fight uses (`--brain lua|scripted`), or the repeat testing aid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrainKind {
    #[default]
    Lua,
    Scripted,
    /// Testing aid: plans the one action over and over, answering at once.
    Repeat(i32),
}

impl BrainKind {
    pub fn by_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "lua" => Ok(Self::Lua),
            "scripted" => Ok(Self::Scripted),
            other => Err(format!("unknown brain {other}: known are lua, scripted")),
        }
    }

    /// The controller for the enemy `chr` (the scripted one plays that enemy's test plans;
    /// an enemy without a table keeps its own Lua AI).
    pub fn build(self, chr: &str) -> Box<dyn BossBrain> {
        if self == Self::Scripted && BossActions::for_chr(chr).is_none() {
            eprintln!("{chr}: no scripted plan table; retaining the character's own Lua AI");
            return Box::new(LuaBrain);
        }
        match self {
            Self::Lua => Box::new(LuaBrain),
            Self::Scripted => Box::new(ScriptedBrain::for_actions(
                BossActions::for_chr(chr).unwrap_or(&OWL),
            )),
            Self::Repeat(id) => {
                let plan = Plan::new(&[id]);
                Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0))
            }
        }
    }
}
