//! The score to beat: how well Wolf played each fight, as numbers the boss's controller is
//! asked to make worse, kept per fight (the last [`HISTORY`] in the memory file) and summed up
//! as a compact trend in every request.
//!
//! Definitions, all about Wolf, over one fight (from its start or a respawn until a death):
//!
//! - `deflected`, `guarded`, `hits_taken`: how each of the boss's hits on Wolf was resolved (a
//!   deflect, a guard that was not a deflect, or an unguarded hit that landed). The perfect-
//!   deflect rate is `deflected / (deflected + guarded + hits_taken)`, the guard rate
//!   `guarded` over the same total.
//! - `hp_lost`: the boss's damage to Wolf, as a fraction of Wolf's max HP, summed over drops
//!   (healing does not take it back).
//! - `posture_gained`: the boss's posture damage to Wolf, as a fraction of his posture bar,
//!   summed over rises (recovery does not take it back).
//! - `punishes`: the boss's actions (attacks and tactics) Wolf hit during or within a second
//!   after them (the tally's `punished`).
//! - `kill_time`: seconds Wolf took to kill the boss, when he did.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Fights kept in the memory file.
pub const HISTORY: usize = 10;
/// Fights in a request's trend.
pub const TREND_FIGHTS: usize = 3;

/// Wolf's numbers for one fight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FightMetrics {
    pub player_first_deathblow: Option<bool>,
    pub countered: u32,
    pub boss_hp_lost: f32,
    pub boss_posture_gained: f32,
    pub boss_damage_taken_per_min: f32,
    pub wolf_posture_breaks: u32,
    pub hits_after_break: u32,
    /// Breaks followed by at least one landed hit, divided by breaks.
    pub break_punish_rate: f32,
    /// Mean seconds from a break to a landed conversion, absent when none converted.
    pub break_conversion_delay: Option<f32>,
    pub wolf_posture_peak: f32,
    pub time_to_kill_wolf: Option<f32>,
    pub duration: f32,
    pub deflected: u32,
    pub guarded: u32,
    pub hits_taken: u32,
    pub hp_lost: f32,
    pub posture_gained: f32,
    pub punishes: u32,
    /// Seconds Wolf took to kill the boss: informational, never a goal (a boss that stalls
    /// would "win" it).
    pub kill_time: Option<f32>,
    /// Attack steps the boss started.
    pub boss_attacks: u32,
    /// Seconds with no attack step started in the last `limits::PASSIVE_WINDOW_SECS`.
    pub passive_secs: f32,
    /// Fight seconds credited only below the passivity threshold.
    pub engaged_time: f32,
    /// Raw input rising edges in the opening ten seconds.
    pub early_inputs: u32,
    /// Seconds with at least one tracked input held.
    pub input_secs: f32,
    /// The player's activity did not qualify this fight for learning.
    pub void: bool,
}

impl FightMetrics {
    pub fn boss_damage_taken_per_min(&self) -> f32 {
        if self.duration > 0.0 {
            (self.boss_hp_lost + self.boss_posture_gained) * 6000.0 / self.duration
        } else {
            0.0
        }
    }
    /// The boss's hits that were resolved against Wolf.
    pub fn resolved(&self) -> u32 {
        self.deflected + self.guarded + self.hits_taken
    }

    fn rate(&self, n: u32) -> Option<f32> {
        (self.resolved() > 0).then(|| n as f32 / self.resolved() as f32)
    }

    /// Perfect-deflect rate, if the boss landed or had hits resolved at all.
    pub fn deflect_rate(&self) -> Option<f32> {
        self.rate(self.deflected)
    }

    pub fn guard_rate(&self) -> Option<f32> {
        self.rate(self.guarded)
    }

    /// The boss's attacks started per minute of fight.
    pub fn attacks_per_min(&self) -> f32 {
        if self.duration > 0.0 {
            self.boss_attacks as f32 * 60.0 / self.duration
        } else {
            0.0
        }
    }

    /// Damage to Wolf per minute, in percent of a bar per minute (HP lost plus posture gained).
    pub fn damage_per_min(&self) -> f32 {
        if self.duration > 0.0 {
            (self.hp_lost + self.posture_gained) * 100.0 * 60.0 / self.duration
        } else {
            0.0
        }
    }

    /// Fraction of the fight with no attack started in the last 4 s (0 to 1).
    pub fn passive_share(&self) -> f32 {
        if self.duration > 0.0 {
            (self.passive_secs / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Whether the fight failed for passivity (see `limits::PASSIVE_FAIL_SHARE`).
    pub fn stalled(&self) -> bool {
        self.passive_share() > super::limits::PASSIVE_FAIL_SHARE
    }
}

/// Collects [`FightMetrics`] as a fight goes.
#[derive(Debug, Clone, Default)]
pub struct MetricsTracker {
    pub breaks: Vec<BreakRecord>,
    conversion_delay: f32,
    last_break: Option<f32>,
    break_punished: bool,
    punished_breaks: u32,
    pub metrics: FightMetrics,
    start: f32,
    prev_hp: Option<f32>,
    prev_posture: Option<f32>,
    prev_boss: Option<[f32; 2]>,
    last_attack: Option<f32>,
    previous_input: [bool; 5],
}

/// One posture break, measured from real clip transitions and contacts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BreakRecord {
    pub time: f32,
    pub action: String,
    pub hit: usize,
    pub hits: usize,
    pub state: String,
    pub wait: f32,
    pub started: bool,
    pub start_delay: Option<f32>,
    pub outcome: String,
}

pub fn break_summary(records: &[BreakRecord]) -> BTreeMap<String, [u32; 2]> {
    let mut out = BTreeMap::new();
    for r in records {
        let counts = out
            .entry(format!("{} hit {}/{}", r.action, r.hit, r.hits))
            .or_insert([0, 0]);
        counts[0] += 1;
        counts[1] += u32::from(r.outcome == "hit");
    }
    out
}

impl MetricsTracker {
    pub fn countered(&mut self) {
        self.metrics.countered += 1;
    }
    pub fn boss(&mut self, hp: f32, posture: f32) {
        if let Some([old_hp, old_posture]) = self.prev_boss {
            self.metrics.boss_hp_lost += (old_hp - hp).max(0.0);
            self.metrics.boss_posture_gained += (posture - old_posture).max(0.0);
        }
        self.prev_boss = Some([hp, posture]);
    }
    pub fn wolf_break(&mut self, now: f32) {
        self.metrics.wolf_posture_breaks += 1;
        self.metrics.break_punish_rate =
            self.punished_breaks as f32 / self.metrics.wolf_posture_breaks as f32;
        self.metrics.wolf_posture_peak = 1.0;
        self.last_break = Some(now);
        self.break_punished = false;
    }

    pub fn post_break_hit(&mut self, now: f32) {
        if self
            .last_break
            .is_some_and(|t| now - t > 0.001 && now - t <= 2.5)
        {
            self.metrics.hits_after_break += 1;
            if !self.break_punished {
                self.punished_breaks += 1;
                self.break_punished = true;
                self.conversion_delay += now - self.last_break.unwrap();
                self.metrics.break_conversion_delay =
                    Some(self.conversion_delay / self.punished_breaks as f32);
                if let Some(r) = self.breaks.last_mut() {
                    r.outcome = "hit".into();
                }
            }
            self.metrics.break_punish_rate =
                self.punished_breaks as f32 / self.metrics.wolf_posture_breaks as f32;
        }
    }
    pub fn break_started(&mut self, now: f32) {
        if let Some(r) = self.breaks.last_mut()
            && !r.started
        {
            r.started = true;
            r.start_delay = Some(now - r.time);
        }
    }

    pub fn break_guarded(&mut self, now: f32) {
        if let Some(r) = self.breaks.last_mut()
            && now - r.time <= 2.5
            && r.outcome != "hit"
        {
            r.outcome = "guarded".into();
        }
    }

    pub fn settle_break(&mut self, now: f32, ended: bool) {
        if let Some(r) = self.breaks.last_mut()
            && r.outcome == "pending"
            && (ended || now - r.time > 2.5)
        {
            r.outcome = if r.started {
                "whiffed"
            } else {
                "missed window"
            }
            .into();
        }
    }
    /// Raw input presses and held time, including during throws and hit stop.
    pub fn input_tick(&mut self, input: &crate::InputFrame, now: f32, dt: f32) {
        let b = input.buttons;
        let held = [
            b.attack,
            b.guard,
            b.dodge,
            b.jump,
            input.move_stick[0].hypot(input.move_stick[1]) > 0.1,
        ];
        if now - self.start <= 10.0 + 1e-3 {
            self.metrics.early_inputs += held
                .iter()
                .zip(self.previous_input)
                .filter(|(down, prev)| **down && !prev)
                .count() as u32;
        }
        if held.iter().any(|down| *down) {
            self.metrics.input_secs += dt;
        }
        self.previous_input = held;
    }

    pub fn player_active(&self, now: f32) -> bool {
        let duration = (now - self.start).max(0.0);
        self.metrics.early_inputs >= 3
            && duration > 0.0
            && self.metrics.input_secs / duration >= 0.4
    }

    pub fn new(start: f32) -> Self {
        Self {
            start,
            ..Self::default()
        }
    }

    /// One of the boss's hits on Wolf was resolved.
    pub fn boss_hit(&mut self, guarded: bool, deflected: bool) {
        if deflected {
            self.metrics.deflected += 1;
        } else if guarded {
            self.metrics.guarded += 1;
        } else {
            self.metrics.hits_taken += 1;
        }
    }

    /// The boss started an attack step at `now`.
    pub fn attack_started(&mut self, now: f32) {
        self.metrics.boss_attacks += 1;
        self.last_attack = Some(now);
    }

    /// One step of `dt` seconds ending at `now`: passive when no attack started in the last
    /// `PASSIVE_WINDOW_SECS`.
    pub fn passive_tick(&mut self, now: f32, dt: f32) {
        let since = now - self.last_attack.unwrap_or(self.start);
        if since > super::limits::PASSIVE_WINDOW_SECS {
            self.metrics.passive_secs += dt;
        }
    }

    /// Wolf's HP and posture (fractions) after a step.
    pub fn wolf(&mut self, hp: f32, posture: f32) {
        self.metrics.wolf_posture_peak = self.metrics.wolf_posture_peak.max(posture);
        if let Some(p) = self.prev_hp {
            self.metrics.hp_lost += (p - hp).max(0.0);
        }
        if let Some(p) = self.prev_posture {
            self.metrics.posture_gained += (posture - p).max(0.0);
        }
        self.prev_hp = Some(hp);
        self.prev_posture = Some(posture);
    }

    /// The numbers so far, at fight time `now`.
    pub fn snapshot(&self, now: f32) -> FightMetrics {
        let mut m = FightMetrics {
            duration: now - self.start,
            ..self.metrics
        };
        m.engaged_time = if m.passive_share() < super::limits::PASSIVE_FAIL_SHARE {
            m.duration
        } else {
            0.0
        };
        m.void = !self.player_active(now);
        m.boss_damage_taken_per_min = m.boss_damage_taken_per_min();
        m
    }

    /// The fight is over at `now`: the final numbers.
    pub fn finish(&self, now: f32, punishes: u32, wolf_won: bool) -> FightMetrics {
        let mut m = self.snapshot(now);
        // A confirmed victory is evidence even when sparse input fails the idle-session gate.
        m.void &= !wolf_won;
        m.punishes = punishes;
        m.kill_time = wolf_won.then_some(m.duration);
        m
    }
}

/// One finished fight as the memory file keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FightRecord {
    /// Phase-specific animation outcomes, including Lua callbacks and their follow-ups.
    pub attacks: BTreeMap<String, super::tally::CountMap>,
    pub cost: super::cost::CostTotals,
    pub session_cost: super::cost::CostTotals,
    pub breaks: Vec<BreakRecord>,
    pub mid_coach: [u64; 2],
    /// `boss`, `wolf` or `none` (unfinished).
    pub winner: String,
    pub metrics: FightMetrics,
    /// Plans of the controller that ran.
    pub decisions: u32,
    /// Acts the Lua AI chose (no plan waiting) or kept as reflexes.
    pub lua_acts: u32,
    /// Requests sent in the fight.
    pub requests: u32,
    /// Plans run, by text form (`lua` for the Lua AI's own acts is not counted).
    pub choices: BTreeMap<String, u32>,
    /// Input and output tokens used in the fight.
    pub tokens: [u64; 2],
    /// Median and 90th percentile of the time from a request to its answer, milliseconds.
    pub latency_ms: [u32; 2],
    /// The brain and request mode of the fight (`decisions/checkpoints`).
    pub brain: String,
    /// Reasoning tokens of the checkpoint calls in the fight.
    pub reasoning: u64,
    /// Coach calls that completed during the fight (usually the previous fight's): calls,
    /// input, output and reasoning tokens, and their total wall-clock milliseconds.
    pub coach: [u64; 5],
    /// Coach mode: seconds each set of the game plan was played (`opener`, `lua` for his own
    /// AI's acts are counted as plans, not here).
    pub sets: BTreeMap<String, f32>,
}

impl FightRecord {
    /// One line for the log and the console.
    pub fn summary_line(&self) -> String {
        let m = &self.metrics;
        if m.void {
            return format!(
                "fight void: no player input ({:.1}s); {}; {}",
                m.duration,
                self.cost.line("fight"),
                self.session_cost.line("session")
            );
        }
        let pct = |r: Option<f32>| r.map_or("-".to_string(), |r| format!("{:.0}%", r * 100.0));
        let mut top: Vec<_> = self.choices.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let hist = top
            .iter()
            .take(6)
            .map(|(k, n)| format!("{k}x{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let coach = if self.coach[0] > 0 {
            format!(
                "; coach {} call(s) tokens {}/{} (reasoning {}) {} ms",
                self.coach[0], self.coach[1], self.coach[2], self.coach[3], self.coach[4]
            )
        } else {
            String::new()
        };
        let sets = if self.sets.is_empty() {
            String::new()
        } else {
            format!(
                "; sets {}",
                self.sets
                    .iter()
                    .map(|(k, s)| format!("{k} {s:.0}s"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        let line = format!(
            "fight over: {} won in {:.1}s ({}); {} plans, {} lua acts, {} requests [{}]; wolf deflect {} guard {} hits taken {} hp lost {:.0}% posture {:.0}% punishes {}; attacks/min {:.0} damage/min {:.0}% passive {:.0}%{}; tokens {}/{} (reasoning {}); latency p50 {} ms p90 {} ms{}{}",
            match self.winner.as_str() {
                "boss" => "boss",
                "wolf" => "Wolf",
                _ => "nobody",
            },
            m.duration,
            self.brain,
            self.decisions,
            self.lua_acts,
            self.requests,
            hist,
            pct(m.deflect_rate()),
            pct(m.guard_rate()),
            m.hits_taken,
            m.hp_lost * 100.0,
            m.posture_gained * 100.0,
            m.punishes,
            m.attacks_per_min(),
            m.damage_per_min(),
            m.passive_share() * 100.0,
            if m.stalled() {
                " (STALLED: failed)"
            } else {
                ""
            },
            self.tokens[0],
            self.tokens[1],
            self.reasoning,
            self.latency_ms[0],
            self.latency_ms[1],
            coach,
            sets,
        );
        let conversion = m
            .break_conversion_delay
            .map_or("-".into(), |d| format!("{d:.3}s"));
        let line = format!(
            "{line}; {}; {}",
            self.cost.line("fight"),
            self.session_cost.line("session")
        );
        format!(
            "break_conversion_delay {conversion}; time_to_kill_wolf {}; wolf_posture_breaks {} hits_after_break {}; boss_damage_taken_per_min {:.0}%; mid-coach {} reviews {} ms; {line}; engaged_time {:.1}s; wolf_posture_breaks {} hits_after_break {} break_punish_rate {:.0}% wolf_posture_peak {:.0}% time_to_kill_wolf {}; wolf_kill_time {}",
            m.time_to_kill_wolf
                .map_or("-".into(), |t| format!("{t:.1}s")),
            m.wolf_posture_breaks,
            m.hits_after_break,
            m.boss_damage_taken_per_min(),
            self.mid_coach[0],
            self.mid_coach[1],
            m.engaged_time,
            m.wolf_posture_breaks,
            m.hits_after_break,
            m.break_punish_rate * 100.0,
            m.wolf_posture_peak * 100.0,
            m.time_to_kill_wolf
                .map_or("-".into(), |t| format!("{t:.1}s")),
            m.kill_time.map_or("-".into(), |t| format!("{t:.1}s"))
        )
    }
}

/// The `p`th percentile (0 to 1) of unsorted values, 0 for none.
pub fn percentile(values: &[f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p).round() as usize]
}

/// Wolf's recent play as a controller sees it, last fights last: percentages and counts per
/// fight, plus this fight so far.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Trend {
    pub boss_damage_taken_per_min: Vec<f32>,
    pub duration: Vec<f32>,
    pub wolf_posture_breaks: Vec<u32>,
    pub hits_after_break: Vec<u32>,
    pub break_punish_rate: Vec<f32>,
    pub break_conversion_delay: Vec<Option<f32>>,
    pub wolf_posture_peak: Vec<f32>,
    pub time_to_kill_wolf: Vec<Option<f32>>,
    /// Perfect-deflect rate, percent.
    pub defl: Vec<u32>,
    /// Guard rate, percent.
    pub guard: Vec<u32>,
    /// Hits the boss landed on Wolf.
    pub hits: Vec<u32>,
    /// The boss's damage to Wolf, percent of his max HP.
    pub hp: Vec<u32>,
    /// Seconds Wolf took to kill the boss (`null`: the boss won or it was unfinished).
    pub kill_s: Vec<Option<u32>>,
    /// Boss attacks started per minute.
    pub apm: Vec<u32>,
    /// Damage to Wolf per minute, percent of a bar (HP plus posture).
    pub dpm: Vec<u32>,
    /// Passive share, percent of the fight (above 35 the fight failed).
    pub passive: Vec<u32>,
    pub engaged_time: Vec<u32>,
    /// This fight so far: deflect percent and hits landed on Wolf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<[u32; 2]>,
}

fn pct(r: Option<f32>) -> u32 {
    (r.unwrap_or(0.0) * 100.0).round() as u32
}

impl Trend {
    /// The trend over the last [`TREND_FIGHTS`] records and this fight's numbers so far.
    pub fn new(history: &[FightRecord], now: Option<&FightMetrics>) -> Self {
        Self::window(history, now, TREND_FIGHTS)
    }

    pub fn last_ten(history: &[FightRecord]) -> Self {
        Self::window(history, None, 10)
    }

    fn window(history: &[FightRecord], now: Option<&FightMetrics>, count: usize) -> Self {
        let recent = &history[history.len().saturating_sub(count)..];
        Self {
            boss_damage_taken_per_min: recent
                .iter()
                .map(|r| r.metrics.boss_damage_taken_per_min())
                .collect(),
            duration: recent.iter().map(|r| r.metrics.duration).collect(),
            wolf_posture_breaks: recent
                .iter()
                .map(|r| r.metrics.wolf_posture_breaks)
                .collect(),
            hits_after_break: recent.iter().map(|r| r.metrics.hits_after_break).collect(),
            break_punish_rate: recent.iter().map(|r| r.metrics.break_punish_rate).collect(),
            break_conversion_delay: recent
                .iter()
                .map(|r| r.metrics.break_conversion_delay)
                .collect(),
            wolf_posture_peak: recent.iter().map(|r| r.metrics.wolf_posture_peak).collect(),
            time_to_kill_wolf: recent.iter().map(|r| r.metrics.time_to_kill_wolf).collect(),
            defl: recent
                .iter()
                .map(|r| pct(r.metrics.deflect_rate()))
                .collect(),
            guard: recent.iter().map(|r| pct(r.metrics.guard_rate())).collect(),
            hits: recent.iter().map(|r| r.metrics.hits_taken).collect(),
            hp: recent
                .iter()
                .map(|r| (r.metrics.hp_lost * 100.0).round() as u32)
                .collect(),
            kill_s: recent
                .iter()
                .map(|r| r.metrics.kill_time.map(|k| k.round() as u32))
                .collect(),
            apm: recent
                .iter()
                .map(|r| r.metrics.attacks_per_min().round() as u32)
                .collect(),
            dpm: recent
                .iter()
                .map(|r| r.metrics.damage_per_min().round() as u32)
                .collect(),
            passive: recent
                .iter()
                .map(|r| (r.metrics.passive_share() * 100.0).round() as u32)
                .collect(),
            engaged_time: recent
                .iter()
                .map(|r| r.metrics.engaged_time.round() as u32)
                .collect(),
            now: now.map(|m| [pct(m.deflect_rate()), m.hits_taken]),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.defl.is_empty() && self.now.is_none()
    }

    /// Drops the oldest fight (to shrink a request); true if one went.
    pub fn trim(&mut self) -> bool {
        if self.defl.len() <= 1 {
            return false;
        }
        self.defl.remove(0);
        self.boss_damage_taken_per_min.remove(0);
        self.duration.remove(0);
        self.wolf_posture_breaks.remove(0);
        self.hits_after_break.remove(0);
        self.break_punish_rate.remove(0);
        self.break_conversion_delay.remove(0);
        self.wolf_posture_peak.remove(0);
        self.time_to_kill_wolf.remove(0);
        self.guard.remove(0);
        self.hits.remove(0);
        self.hp.remove(0);
        self.kill_s.remove(0);
        self.apm.remove(0);
        self.dpm.remove(0);
        self.passive.remove(0);
        self.engaged_time.remove(0);
        true
    }
}
