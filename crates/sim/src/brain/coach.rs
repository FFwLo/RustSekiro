//! The coach: during and between fights a reasoning model reads the fight summary and writes a **game plan**
//! for the next fight; in the fight the driver runs that plan locally and a cheap model only
//! picks which of the plan's sets to play at the checkpoints.
//!
//! - [`GamePlan`] is the versioned, serde form of the plan (the coach's JSON): an `opener`, 2 to
//!   4 named [`PlanSet`]s of weighted plans (the boss's own choice grammar), a `start_set`,
//!   `avoid` entries and `notes` (hypotheses about the player, kept in memory and sent back next
//!   time). [`GamePlan::validate`] checks it against the boss's [`BossActions`]: invalid plans
//!   and choices are dropped with a warning, and a plan with nothing valid is not usable (the
//!   Lua AI plays).
//! - [`CoachInput`] is what the coach is sent about the current or finished fight (bounded to 8 KB).
//! - [`CoachRequest`] / [`CoachReply`] are the non-blocking hand-off to a controller
//!   ([`super::BossBrain::submit_coach`]); [`ScriptedCoach`] is the offline test controller.
//!
//! See `docs/BRAIN.md`, "The coach".

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use super::actions::BossActions;
use super::habits::HabitSummary;
use super::limits;
use super::metrics::Trend;
use super::plan::{Plan, Weighted};
use super::tally::{Counts, TopMap};
use super::{BossBrain, BrainReply, BrainRequest};

/// The game plan format version.
pub const GAME_PLAN_VERSION: u32 = 2;
/// Longest `notes`, characters.
pub const NOTES_MAX: usize = 400;
/// Most sets kept.
pub const MAX_SETS: usize = 4;
/// Most plans per set.
pub const MAX_PLANS: usize = 8;
/// Most `avoid` entries.
pub const MAX_AVOID: usize = 8;
/// Longest set name, set description and avoid reason, characters.
pub const NAME_MAX: usize = 24;
pub const DESCRIPTION_MAX: usize = 140;
pub const REASON_MAX: usize = 80;
/// The coach input's size target, bytes of compact JSON.
pub const INPUT_BUDGET: usize = 8192;
pub const MID_COACH_INTERVAL: f32 = 30.0;
/// The checkpoint events a set's `when` hint may name.
pub const CHECKPOINTS: [&str; 11] = [
    "start",
    "pressure",
    "pressure_critical",
    "half_hp",
    "mark_lost",
    "punished_twice",
    "deflected_thrice",
    "wolf_posture_25",
    "wolf_posture_50",
    "wolf_posture_75",
    "wolf_broken",
];
/// The checkpoint chooser's "stay on the current set" value.
pub const KEEP: &str = "keep";

/// One plan of a set and its weight.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeightedChoice {
    /// A choice in the boss's grammar (`walk_in4>3013`, `3000+3001`, ...), or `lua`.
    pub plan: String,
    pub weight: f32,
}

/// A named set of plans the fighter samples from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanSet {
    pub name: String,
    /// One line the checkpoint chooser sees.
    pub description: String,
    pub plans: Vec<WeightedChoice>,
    /// A checkpoint this set suits (one of [`CHECKPOINTS`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// A choice the coach does not want used, and why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Avoid {
    pub choice: String,
    pub reason: String,
}

/// The coach's plan for the next fight.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GamePlan {
    /// Distance bucket -> preferred ids in order, each with a one-line reason.
    pub punish_preferences: std::collections::BTreeMap<String, Vec<Avoid>>,
    pub explore: super::move_bandit::Explore,
    pub move_controls: Vec<super::move_bandit::Control>,
    pub lessons: Vec<String>,
    #[serde(deserialize_with = "read_checkpoint_prefs")]
    pub checkpoint_prefs:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, f32>>,
    /// Multipliers for counter animation ids; zero vetoes a callback containing that act.
    pub counter_weights: std::collections::BTreeMap<String, f32>,
    /// Character phase this plan was validated for; old plans default to the installing phase.
    pub boss_phase: String,
    pub boss_principles: Vec<String>,
    pub boss_bans: Vec<Avoid>,
    /// Local response to Wolf's posture break, never requiring a model call.
    pub on_wolf_break: Vec<WeightedChoice>,
    /// Lessons retained by the coach across fights.
    #[serde(rename = "player_principles", alias = "principles")]
    pub principles: Vec<String>,
    /// Format version ([`GAME_PLAN_VERSION`]; a missing one reads as 1).
    pub version: u32,
    /// A plan run first in the fight (`lua` or empty: none).
    pub opener: String,
    pub sets: Vec<PlanSet>,
    /// The set played from the start.
    pub start_set: String,
    pub avoid: Vec<Avoid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub retry: Vec<Avoid>,
    /// The coach's hypotheses about the player (at most [`NOTES_MAX`] characters).
    pub notes: String,
}

/// Serialized coach reasoning added to the checkpoint brief, including JSON overhead.
pub const CHECKPOINT_CONTEXT_BUDGET: usize = 768;

impl GamePlan {
    pub fn checkpoint_context(
        &self,
        active: Option<&str>,
        boss_principles: &[String],
        lessons: &[String],
    ) -> serde_json::Value {
        let mut controls: Vec<_> = self.move_controls.iter().collect();
        controls.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        let mut context = serde_json::json!({
            "notes": clip(&self.notes, 200),
            "intent": clip(self.sets.iter().find(|s| Some(s.name.as_str()) == active)
                .map_or("", |s| s.description.as_str()), 100),
            "boss": boss_principles.iter().take(2).map(|s| clip(s, 70)).collect::<Vec<_>>(),
            "player": self.principles.iter().take(2).map(|s| clip(s, 70)).collect::<Vec<_>>(),
            "lessons": lessons.iter().rev().take(2).map(|s| clip(s, 90)).collect::<Vec<_>>()
                .into_iter().rev().collect::<Vec<_>>(),
            "confidence": controls.into_iter().take(3).map(|c|
                serde_json::json!([c.plan, (f64::from(c.confidence) * 100.0).round() / 100.0, c.lock]))
                .collect::<Vec<_>>()
        });
        while context.to_string().len() > CHECKPOINT_CONTEXT_BUDGET {
            let mut trimmed = false;
            for key in ["confidence", "lessons", "boss", "player"] {
                let rows = context[key].as_array_mut().unwrap();
                if !rows.is_empty() {
                    if key == "lessons" {
                        rows.remove(0);
                    } else {
                        rows.pop();
                    }
                    trimmed = true;
                    break;
                }
            }
            if !trimmed {
                for key in ["notes", "intent"] {
                    let text = context[key].as_str().unwrap();
                    if !text.is_empty() {
                        context[key] = clip(text, text.chars().count() / 2).into();
                        break;
                    }
                }
            }
        }
        context
    }
}

fn read_checkpoint_prefs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, f32>>, D::Error> {
    #[derive(Deserialize)]
    struct Preference {
        checkpoint: String,
        set: String,
        weight: f32,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Preferences {
        Map(std::collections::BTreeMap<String, std::collections::BTreeMap<String, f32>>),
        Rows(Vec<Preference>),
    }
    match Preferences::deserialize(deserializer)? {
        Preferences::Map(map) => Ok(map),
        Preferences::Rows(rows) => {
            let mut map =
                std::collections::BTreeMap::<String, std::collections::BTreeMap<String, f32>>::new(
                );
            for row in rows {
                map.entry(row.checkpoint)
                    .or_default()
                    .insert(row.set, row.weight);
            }
            Ok(map)
        }
    }
}

/// A plan checked against a boss: what survived, and what was dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Validated {
    pub plan: GamePlan,
    pub warnings: Vec<String>,
}

impl Validated {
    /// Whether anything in the plan can be played (an opener or a set).
    pub fn usable(&self) -> bool {
        self.plan.usable()
    }
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((i, _)) => text[..i].trim_end().to_string(),
        None => text.to_string(),
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && name != KEEP
        && name != "lua"
}

impl GamePlan {
    /// Reads the coach's JSON (the structured output text).
    pub fn from_json(text: &str) -> Result<GamePlan, String> {
        serde_json::from_str(text).map_err(|e| format!("not a game plan: {e}"))
    }

    /// Whether anything can be played.
    pub fn usable(&self) -> bool {
        !self.sets.is_empty()
            || !self.opener.is_empty()
            || !self.on_wolf_break.is_empty()
            || self.punish_preferences.values().any(|p| !p.is_empty())
    }

    /// Checks the plan against the boss's table: unknown choices and plans are dropped with a
    /// warning (text echoed back is clipped, never copied whole), names and lengths are
    /// enforced, and a plan equal to an `avoid` choice is removed from the sets.
    pub fn validate(&self, actions: &BossActions) -> Validated {
        let mut warnings = Vec::new();
        let mut out = GamePlan {
            version: GAME_PLAN_VERSION,
            ..GamePlan::default()
        };
        out.boss_phase = actions.chr.into();
        for (bucket, preferences) in &self.punish_preferences {
            if !matches!(bucket.as_str(), "near" | "mid" | "far") {
                continue;
            }
            out.punish_preferences.insert(
                bucket.clone(),
                preferences
                    .iter()
                    .filter(|p| actions.parse(&p.choice).is_ok() && !p.reason.trim().is_empty())
                    .take(8)
                    .map(|p| Avoid {
                        choice: p.choice.clone(),
                        reason: clip(
                            &p.reason.split_whitespace().collect::<Vec<_>>().join(" "),
                            120,
                        ),
                    })
                    .collect(),
            );
        }
        out.explore = self.explore;
        out.lessons = self
            .lessons
            .iter()
            .map(|s| clip(s.trim(), 120))
            .filter(|s| !s.is_empty())
            .take(6)
            .collect();
        for c in self.move_controls.iter().take(32) {
            if actions.parse(&c.plan).is_ok()
                && c.confidence.is_finite()
                && (0.0..=1.0).contains(&c.confidence)
            {
                out.move_controls.push(c.clone());
            } else {
                warnings.push("invalid move control dropped".into());
            }
        }
        if !self.boss_phase.is_empty() && self.boss_phase != actions.chr {
            warnings.push("game plan belongs to another boss phase".into());
            return Validated {
                plan: out,
                warnings,
            };
        }
        for (id, weight) in &self.counter_weights {
            if id
                .parse::<i32>()
                .is_ok_and(|id| actions.counter_ids().contains(&id))
                && weight.is_finite()
                && (0.0..=10.0).contains(weight)
            {
                out.counter_weights.insert(id.clone(), *weight);
            } else {
                warnings.push(format!("counter weight dropped: {id}"));
            }
        }
        if self.version > GAME_PLAN_VERSION {
            warnings.push(format!("unsupported game plan version {}", self.version));
            return Validated {
                plan: out,
                warnings,
            };
        }
        let opener = self.opener.trim();
        if !opener.is_empty() && opener != "lua" {
            match actions.parse(opener) {
                Ok(_) => out.opener = opener.to_string(),
                Err(e) => warnings.push(format!("opener dropped: {e}")),
            }
        }
        for a in self.avoid.iter().take(MAX_AVOID) {
            let choice = a.choice.trim();
            if choice != "lua" && actions.parse(choice).is_ok() {
                out.avoid.push(Avoid {
                    choice: choice.to_string(),
                    reason: clip(&a.reason, REASON_MAX),
                });
            } else {
                warnings.push(format!("avoid entry dropped: {}", clip(choice, 40)));
            }
        }
        let avoided = |p: &str| out.avoid.iter().any(|a| a.choice == p);
        for r in self.retry.iter().take(MAX_AVOID) {
            let choice = r.choice.trim();
            let reason = clip(&r.reason, REASON_MAX);
            if actions.parse(choice).is_ok() && !reason.is_empty() && !avoided(choice) {
                out.retry.push(Avoid {
                    choice: choice.to_string(),
                    reason,
                });
            } else {
                warnings.push(format!(
                    "retry entry dropped: {} (known choice and reason required)",
                    clip(choice, 40)
                ));
            }
        }
        let mut sets: Vec<PlanSet> = Vec::new();
        for set in &self.sets {
            let name = set.name.trim();
            if !valid_name(name) {
                warnings.push(format!("set dropped: bad name {}", clip(name, 40)));
                continue;
            }
            if sets.iter().any(|s| s.name == name) {
                warnings.push(format!("set {name} dropped: duplicate name"));
                continue;
            }
            if sets.len() >= MAX_SETS {
                warnings.push(format!("set {name} dropped: more than {MAX_SETS} sets"));
                continue;
            }
            let mut plans = Vec::new();
            for w in set.plans.iter().take(MAX_PLANS) {
                let text = w.plan.trim();
                let parsed = actions.parse(text).ok();
                let known = text == "lua" || parsed.is_some();
                if known
                    && text != "lua"
                    && !parsed
                        .as_ref()
                        .is_some_and(|p| limits::has_attack(p, actions))
                {
                    warnings.push(format!("set {name}: {text} dropped, it has no attack step"));
                } else if !known {
                    warnings.push(format!("set {name}: plan dropped: {}", clip(text, 60)));
                } else if avoided(text) {
                    warnings.push(format!("set {name}: {text} dropped, it is in avoid"));
                } else if !(w.weight.is_finite() && w.weight > 0.0) {
                    warnings.push(format!("set {name}: {text} dropped, weight not positive"));
                } else {
                    plans.push(WeightedChoice {
                        plan: text.to_string(),
                        weight: w.weight,
                    });
                }
            }
            if plans.is_empty() {
                warnings.push(format!("set {name} dropped: no valid plan"));
                continue;
            }
            let when = set
                .when
                .as_deref()
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .and_then(|w| {
                    if CHECKPOINTS.contains(&w) {
                        Some(w.to_string())
                    } else {
                        warnings.push(format!("set {name}: unknown checkpoint {}", clip(w, 30)));
                        None
                    }
                });
            sets.push(PlanSet {
                name: name.to_string(),
                description: clip(&set.description, DESCRIPTION_MAX),
                plans,
                when,
            });
        }
        out.start_set = self.start_set.trim().to_string();
        if !sets.is_empty() && !sets.iter().any(|s| s.name == out.start_set) {
            if !out.start_set.is_empty() {
                warnings.push(format!(
                    "start_set {} unknown, using {}",
                    clip(&out.start_set, 40),
                    sets[0].name
                ));
            }
            out.start_set = sets[0].name.clone();
        }
        if sets.is_empty() {
            out.start_set.clear();
        }
        if sets.len() == 1 {
            warnings.push("only one valid set: no checkpoint choices".into());
        }
        out.sets = sets;
        for w in self.on_wolf_break.iter().take(3) {
            if w.weight.is_finite()
                && w.weight > 0.0
                && actions
                    .parse(w.plan.trim())
                    .is_ok_and(|p| limits::has_attack(&p, actions))
                && !out.avoid.iter().any(|a| a.choice == w.plan.trim())
            {
                out.on_wolf_break.push(WeightedChoice {
                    plan: w.plan.trim().into(),
                    weight: w.weight,
                });
            } else {
                warnings.push(format!("on_wolf_break dropped: {}", clip(&w.plan, 60)));
            }
        }
        for set in &mut out.sets {
            let total: f64 = set
                .plans
                .iter()
                .filter(|p| p.plan != "lua")
                .map(|p| f64::from(p.weight))
                .sum();
            let lua: f64 = set
                .plans
                .iter()
                .filter(|p| p.plan == "lua")
                .map(|p| f64::from(p.weight))
                .sum();
            if lua > total / 3.0 {
                for p in set.plans.iter_mut().filter(|p| p.plan == "lua") {
                    p.weight = (f64::from(p.weight) * total / (3.0 * lua)) as f32;
                }
                set.plans.retain(|p| p.weight > 0.0);
                warnings.push(format!("set {}: lua capped at 25%", set.name));
            }
        }
        out.sets.retain(|s| !s.plans.is_empty());
        for (context, prefs) in &self.checkpoint_prefs {
            for (name, weight) in prefs {
                if CHECKPOINTS.contains(&context.as_str())
                    && out.set_index(name).is_some()
                    && weight.is_finite()
                    && (0.0..=1.0).contains(weight)
                {
                    out.checkpoint_prefs
                        .entry(context.clone())
                        .or_default()
                        .insert(name.clone(), *weight);
                } else {
                    warnings.push(format!("checkpoint preference dropped: {context}/{name}"));
                }
            }
        }
        if !out.sets.iter().any(|s| s.name == out.start_set) {
            out.start_set = out.sets.first().map(|s| s.name.clone()).unwrap_or_default();
        }
        out.principles = self
            .principles
            .iter()
            .take(6)
            .map(|s| clip(s, 90))
            .filter(|s| !s.is_empty())
            .collect();
        out.notes = clip(&self.notes, NOTES_MAX);
        out.boss_principles = self
            .boss_principles
            .iter()
            .take(6)
            .map(|s| clip(s, 90))
            .filter(|s| !s.is_empty())
            .collect();
        out.boss_bans = self
            .boss_bans
            .iter()
            .take(MAX_AVOID)
            .filter(|a| actions.parse(&a.choice).is_ok() && !a.reason.trim().is_empty())
            .map(|a| Avoid {
                choice: a.choice.clone(),
                reason: clip(&a.reason, REASON_MAX),
            })
            .collect();
        Validated {
            plan: out,
            warnings,
        }
    }

    /// The opener as a plan (`None` when there is none).
    pub fn opener_plan(&self, actions: &BossActions) -> Option<Plan> {
        (!self.opener.is_empty() && self.opener != "lua")
            .then(|| actions.parse(&self.opener).ok())
            .flatten()
    }

    pub fn set_index(&self, name: &str) -> Option<usize> {
        self.sets.iter().position(|s| s.name == name)
    }

    /// The index of the start set.
    pub fn start_index(&self) -> Option<usize> {
        self.set_index(&self.start_set)
            .or(if self.sets.is_empty() { None } else { Some(0) })
    }

    /// A set as the sampler's weighted plans (`lua` is the Lua AI's own act).
    pub fn weighted(&self, index: usize, actions: &BossActions) -> Vec<Weighted> {
        self.sets.get(index).map_or_else(Vec::new, |s| {
            s.plans
                .iter()
                .filter_map(|w| {
                    let plan = if w.plan == "lua" {
                        None
                    } else {
                        Some(actions.parse(&w.plan).ok()?)
                    };
                    Some(Weighted {
                        plan,
                        weight: w.weight,
                    })
                })
                .collect()
        })
    }

    /// What the checkpoint chooser is offered: each set's name and description (with its `when`
    /// hint).
    pub fn offers(&self) -> Vec<SetOffer> {
        self.sets
            .iter()
            .map(|s| SetOffer {
                name: s.name.clone(),
                description: match &s.when {
                    Some(w) => format!("{} (suits: {w})", s.description),
                    None => s.description.clone(),
                },
            })
            .collect()
    }

    /// A short human-readable summary for the console: 3 to 5 lines.
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "opener: {}",
            if self.opener.is_empty() {
                "none (his own AI)"
            } else {
                &self.opener
            }
        ));
        let sets: Vec<String> = self
            .sets
            .iter()
            .map(|s| {
                format!(
                    "{}{} ({} plans: {})",
                    s.name,
                    if s.name == self.start_set {
                        " [start]"
                    } else {
                        ""
                    },
                    s.plans.len(),
                    clip(&s.description, 60)
                )
            })
            .collect();
        lines.push(format!(
            "sets: {}",
            if sets.is_empty() {
                "none".to_string()
            } else {
                sets.join("; ")
            }
        ));
        if !self.avoid.is_empty() {
            let avoid: Vec<String> = self
                .avoid
                .iter()
                .map(|a| format!("{} ({})", a.choice, a.reason))
                .collect();
            lines.push(format!("avoid: {}", avoid.join("; ")));
        }
        if !self.retry.is_empty() {
            let retry: Vec<String> = self
                .retry
                .iter()
                .map(|r| format!("{} ({})", r.choice, r.reason))
                .collect();
            lines.push(format!("retry: {}", retry.join("; ")));
        }
        if !self.notes.is_empty() {
            lines.push(format!("notes: {}", self.notes));
        }
        if !self.on_wolf_break.is_empty() {
            lines.push(format!(
                "on_wolf_break: {}",
                self.on_wolf_break
                    .iter()
                    .map(|w| w.plan.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.principles.is_empty() {
            lines.push(format!(
                "principles: player: {}",
                self.principles.join("; ")
            ));
        }
        if !self.boss_principles.is_empty() {
            lines.push(format!(
                "principles: boss: {}",
                self.boss_principles.join("; ")
            ));
        }
        lines
    }
}

/// One set offered to the checkpoint chooser.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetOffer {
    pub name: String,
    pub description: String,
}

/// Wolf's result in the fight just over, in percent and whole numbers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FightLine {
    pub countered: u32,
    /// `boss`, `wolf` or `none`.
    pub won: String,
    pub time_to_kill_wolf: Option<f32>,
    pub wolf_posture_breaks: u32,
    pub hits_after_break: u32,
    pub break_punish_rate: f32,
    pub break_conversion_delay: Option<f32>,
    pub wolf_posture_peak: f32,
    pub secs: u32,
    /// Perfect-deflect percent, guard percent.
    pub defl: u32,
    pub guard: u32,
    /// Hits the boss landed, Wolf's HP and posture lost in percent, the boss actions Wolf
    /// punished.
    pub hits: u32,
    pub hp: u32,
    pub posture: u32,
    pub punishes: u32,
    /// Boss attacks started per minute, damage to Wolf per minute (percent of a bar, HP plus
    /// posture) and the passive share in percent (above 35 the fight failed).
    pub apm: u32,
    pub dpm: u32,
    pub boss_damage_taken_per_min: f32,
    pub passive: u32,
    pub engaged_time: u32,
}

/// How one of the plan's sets did in the fight: seconds played and the outcome counts of the
/// plans run under it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SetStat {
    pub secs: f32,
    pub c: Counts,
}

/// What the coach is told about the fight just over. Counts are
/// `[exchanges, hit, blocked, deflected, whiffed, punished]` per plan text.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(into = "CoachWire")]
pub struct CoachInput {
    pub punish_window: Option<f32>,
    pub viable_punishes: std::collections::BTreeMap<String, Vec<super::moves::Punish>>,
    pub wolf_posture: f32,
    pub digest: super::digest::Digest,
    /// Mature recent outcomes: [samples, success rate, severity-weighted failure rate].
    pub attack_rates: TopMap<[f32; 3]>,
    pub checkpoints:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, [f32; 2]>>,
    pub counters: TopMap<Counts>,
    pub boss_phase: String,
    pub move_sheet: super::moves::MoveSheet,
    pub break_log: Vec<super::metrics::BreakRecord>,
    pub break_summary: std::collections::BTreeMap<String, [u32; 2]>,
    pub boss_knowledge: super::memory::BossSummary,
    pub phase: String,
    pub result: FightLine,
    pub plans: TopMap<Counts>,
    pub sets: std::collections::BTreeMap<String, SetStat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played: Option<GamePlan>,
    pub elapsed: Option<f32>,
    pub vitals: Option<[f32; 4]>,
    pub marks: Option<i32>,
    pub on_wolf_break: std::collections::BTreeMap<String, Counts>,
    pub principles: Vec<String>,
    /// Every observed boss action, including acts selected by Lua.
    pub actions: std::collections::BTreeMap<String, Counts>,
    pub action_range: std::collections::BTreeMap<String, [f32; 2]>,
    pub banned: Vec<String>,
    pub failures: std::collections::BTreeMap<String, [u32; 4]>,
    /// Fights with this enemy so far, this one included.
    pub fight: u32,
    /// Wolf's play over the last 10 finished fights, this one last.
    pub trend: Trend,
    /// Per-plan mean [attack-start distance, observed melee reach], metres.
    /// Reach includes root travel, hit capsules and Wolf's radius; projectile-only attacks
    /// have no sample. Trimmed together with the plan outcomes.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub range: std::collections::BTreeMap<String, [f32; 2]>,
    /// Wolf's attacks this fight.
    pub wolf: TopMap<Counts>,
    /// Every fight, per plan text.
    pub overall: TopMap<Counts>,
    /// Wolf's habits over every fight.
    pub habits: HabitSummary,
    /// The coach's own previous notes.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl From<CoachInput> for serde_json::Value {
    fn from(i: CoachInput) -> Self {
        let mut options = serde_json::Map::new();
        let mut buckets = serde_json::Map::new();
        for (bucket, rows) in &i.viable_punishes {
            let compact: Vec<_> = rows
                .iter()
                .map(|p| {
                    let mut row = serde_json::to_value(p).unwrap().as_array().unwrap().clone();
                    options
                        .entry(p.id.clone())
                        .or_insert_with(|| serde_json::json!([row[1], row[6], row[9]]));
                    row.remove(9);
                    row.remove(6);
                    row.remove(1);
                    serde_json::Value::Array(row)
                })
                .collect();
            buckets.insert(bucket.clone(), compact.into());
        }
        let mut value = serde_json::json!({
            "punish_window": i.punish_window, "punish_options": options, "viable_punishes": buckets,
            "wolf_posture": i.wolf_posture,
            "digest": i.digest,
            "attack_rates": i.attack_rates,
            "checkpoints": i.checkpoints,
            "move_sheet": i.move_sheet, "break_log": i.break_log,
            "break_summary": i.break_summary,
            "boss_knowledge": i.boss_knowledge,
            "player_profile": {"fight": i.fight, "trend": i.trend, "habits": i.habits,
                "player_principles": i.principles, "notes": i.notes, "overall": i.overall,
                "banned": i.banned, "failures": i.failures},
            "phase": i.phase, "boss_phase": i.boss_phase, "counters": i.counters, "result": i.result, "plans": i.plans, "sets": i.sets,
            "played": i.played, "elapsed": i.elapsed, "vitals": i.vitals, "marks": i.marks,
            "on_wolf_break": i.on_wolf_break, "actions": i.actions,
            "action_range": i.action_range, "range": i.range, "wolf": i.wolf
        });
        if value["played"].is_null() {
            value
                .as_object_mut()
                .expect("coach wire object")
                .remove("played");
        }
        value
    }
}

#[derive(Clone)]
struct CoachWire(serde_json::Value);

impl From<CoachInput> for CoachWire {
    fn from(input: CoachInput) -> Self {
        Self(input.into())
    }
}

impl Serialize for CoachWire {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        for key in ["boss_knowledge", "player_profile"] {
            map.serialize_entry(key, &self.0[key])?;
        }
        for (key, value) in self.0.as_object().expect("coach wire object") {
            if key != "boss_knowledge" && key != "player_profile" {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

impl CoachInput {
    pub fn json_len(&self) -> usize {
        serde_json::to_string(self).map_or(0, |s| s.len())
    }

    /// Drops the least telling entries until the JSON fits in `budget` bytes; returns the size.
    pub fn fit(&mut self, budget: usize) -> usize {
        // Shared lessons and bans are sent in the boss layer, not duplicated in the plan.
        if let Some(plan) = self.played.as_mut() {
            plan.boss_principles.clear();
            plan.boss_bans.clear();
        }
        self.range
            .retain(|key, _| self.plans.0.iter().any(|(plan, _)| plan == key));
        let mut len = self.json_len();
        while len > budget && self.trim_one() {
            self.range
                .retain(|key, _| self.plans.0.iter().any(|(plan, _)| plan == key));
            len = self.json_len();
        }
        len
    }

    fn trim_one(&mut self) -> bool {
        // Preserve punish facts ahead of the duplicate raw animation sheet.
        if !self.viable_punishes.is_empty() && !self.move_sheet.is_empty() {
            self.move_sheet.pop_last();
            return true;
        }
        if self.digest.moves.lines().count() > 8 {
            self.digest.moves = self
                .digest
                .moves
                .lines()
                .take(self.digest.moves.lines().count() - 1)
                .collect::<Vec<_>>()
                .join("\n");
            return true;
        }
        if self.attack_rates.pop(3) {
            return true;
        }
        if self.checkpoints.len() > 4 {
            self.checkpoints.pop_last();
            return true;
        }
        if self.counters.pop(4) {
            return true;
        }
        if self.move_sheet.len() > 6 {
            let used = self
                .played
                .as_ref()
                .map(|p| p.summary_lines().join(" "))
                .unwrap_or_default();
            if let Some(key) = self
                .move_sheet
                .keys()
                .filter(|k| !used.contains(k.as_str()))
                .min_by_key(|k| self.actions.get(*k).map_or(0, |c| c.n))
                .cloned()
            {
                self.move_sheet.remove(&key);
                return true;
            }
        }
        if self.break_log.len() > 4 {
            self.break_log.remove(0);
            return true;
        }
        if self.boss_knowledge.trim() {
            return true;
        }
        if self.failures.len() > 6
            && let Some(key) = self
                .failures
                .keys()
                .find(|k| !self.banned.contains(k))
                .cloned()
        {
            self.failures.remove(&key);
            return true;
        }
        for keep in [6, 4, 3, 2, 1, 0] {
            if self.overall.pop(keep) || self.wolf.pop(keep) {
                return true;
            }
            if self.habits.trim(keep) {
                return true;
            }
            if keep <= 4 && self.plans.pop(keep.max(3)) {
                return true;
            }
        }
        if self.trend.trim() {
            return true;
        }
        if self.action_range.pop_last().is_some() || self.actions.pop_last().is_some() {
            return true;
        }
        if self.principles.pop().is_some() {
            return true;
        }
        if self.range.pop_last().is_some() {
            return true;
        }
        if let Some(plan) = self.played.as_mut()
            && let Some(set) = plan.sets.iter_mut().find(|s| !s.description.is_empty())
        {
            set.description.clear();
            return true;
        }
        if self
            .played
            .as_mut()
            .is_some_and(|plan| plan.checkpoint_prefs.pop_last().is_some())
        {
            return true;
        }
        // Counter statistics must fit without erasing which game plan was played.
        // Detailed timelines can be reduced before that higher-value context.
        if self.move_sheet.len() > 3 {
            let used = self
                .played
                .as_ref()
                .map(|p| p.summary_lines().join(" "))
                .unwrap_or_default();
            if let Some(key) = self
                .move_sheet
                .keys()
                .min_by_key(|key| {
                    (
                        self.break_log.last().is_some_and(|r| r.action == **key),
                        key.contains('+') && used.contains(key.as_str()),
                        used.contains(key.as_str()),
                        self.actions.get(*key).map_or(0, |c| c.n),
                    )
                })
                .cloned()
            {
                self.move_sheet.remove(&key);
                return true;
            }
        }
        if self.phase != "mid" && self.played.take().is_some() {
            return true;
        }
        if self.phase == "mid"
            && let Some(plan) = self.played.as_mut()
        {
            if let Some(preference) = plan
                .punish_preferences
                .values_mut()
                .flat_map(|v| v.iter_mut())
                .find(|p| !p.reason.is_empty())
            {
                preference.reason.clear();
                return true;
            }
            if plan.avoid.pop().is_some()
                || plan.retry.pop().is_some()
                || plan.principles.pop().is_some()
                || plan.on_wolf_break.pop().is_some()
            {
                return true;
            }
            if !plan.notes.is_empty() {
                plan.notes.clear();
                return true;
            }
        }
        if self.plans.pop(0) || self.sets.pop_last().is_some() {
            return true;
        }
        if let Some(key) = self
            .failures
            .keys()
            .find(|k| !self.banned.contains(k))
            .cloned()
        {
            self.failures.remove(&key);
            return true;
        }
        if self.notes.chars().count() > 80 {
            self.notes = clip(&self.notes, self.notes.chars().count() - 80);
            return true;
        }
        if self.action_range.pop_last().is_some() {
            return true;
        }
        if self.actions.pop_last().is_some() {
            return true;
        }
        if self.on_wolf_break.pop_last().is_some() {
            return true;
        }
        if self.failures.pop_last().is_some() {
            return true;
        }
        self.break_summary.pop_last().is_some()
            || self
                .move_sheet
                .keys()
                .rev()
                .find(|key| self.break_log.last().is_none_or(|r| r.action != **key))
                .cloned()
                .is_some_and(|key| self.move_sheet.remove(&key).is_some())
            || (!self.break_log.is_empty() && {
                self.break_log.remove(0);
                true
            })
            || (!self.digest.moves.is_empty() && {
                self.digest.moves = self
                    .digest
                    .moves
                    .lines()
                    .take(self.digest.moves.lines().count().saturating_sub(1))
                    .collect::<Vec<_>>()
                    .join("\n");
                true
            })
            || (self.digest.fights.len() > 1 && {
                self.digest.fights.remove(0);
                true
            })
            || (!self.digest.lessons.is_empty() && {
                self.digest.lessons.remove(0);
                true
            })
            || (self.digest.hypotheses.len() > 1 && {
                self.digest.hypotheses.remove(0);
                true
            })
            || self.banned.pop().is_some()
            || self.checkpoints.pop_last().is_some()
            || self.boss_knowledge.trim_last()
    }
}

/// A request to the coach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoachRequest {
    pub id: u64,
    /// The fight just over (the plan is for the next one).
    pub fight: u32,
    /// The compact JSON of a [`CoachInput`].
    pub input: String,
}

/// The coach's answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoachReply {
    pub id: u64,
    /// The plan as the model wrote it (not yet validated), `None` on any failure.
    pub plan: Option<GamePlan>,
    /// Why there is no plan (a failure reason, never a payload).
    pub error: Option<String>,
    /// Input, output and reasoning tokens the call used.
    pub tokens: [u64; 3],
    /// Wall-clock time the call took, milliseconds.
    pub latency_ms: u32,
}

/// Counts for one kind of hosted call (checkpoint chooser or coach).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CallStats {
    pub calls: u64,
    pub failed: u64,
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    /// Sum and maximum of the round trips, milliseconds.
    pub total_ms: f64,
    pub max_ms: f64,
}

impl CallStats {
    /// What was added since `earlier` (counts and sums; the maximum is kept).
    pub fn since(&self, earlier: &CallStats) -> CallStats {
        CallStats {
            calls: self.calls.saturating_sub(earlier.calls),
            failed: self.failed.saturating_sub(earlier.failed),
            input: self.input.saturating_sub(earlier.input),
            output: self.output.saturating_sub(earlier.output),
            reasoning: self.reasoning.saturating_sub(earlier.reasoning),
            total_ms: (self.total_ms - earlier.total_ms).max(0.0),
            max_ms: self.max_ms,
        }
    }

    pub fn mean_ms(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.total_ms / self.calls as f64
        }
    }
}

/// A deterministic test controller for the coach flow: answers the coach requests from a queue
/// (each after `coach_delay` seconds of fight time; an exhausted queue is a failure) and the
/// checkpoint chooser with `choice` (a set name or `keep`), after `delay` seconds.
#[derive(Debug, Clone, Default)]
pub struct ScriptedCoach {
    pub plans: VecDeque<Option<GamePlan>>,
    pub coach_delay: f32,
    pub choice: Option<String>,
    pub delay: f32,
    /// Every coach request's input, and the checkpoint requests' count (for assertions).
    pub coach_inputs: Vec<String>,
    pub coach_times: Vec<f32>,
    pub checkpoint_requests: Vec<(Option<&'static str>, Vec<String>)>,
    /// The chooser's input text of each checkpoint request.
    pub checkpoint_briefs: Vec<Option<String>>,
    pending: Option<(u64, f32)>,
    coach_pending: Vec<(u64, f32)>,
}

impl ScriptedCoach {
    /// Answers the coach requests from `plans` after `coach_delay` seconds and the checkpoint
    /// chooser (see `choice`) after `delay`.
    pub fn new(plans: Vec<Option<GamePlan>>, coach_delay: f32, delay: f32) -> Self {
        Self {
            plans: plans.into(),
            coach_delay,
            delay,
            ..Self::default()
        }
    }
}

impl BossBrain for ScriptedCoach {
    fn name(&self) -> &'static str {
        "coach"
    }
    fn coaches(&self) -> bool {
        true
    }
    fn default_mode(&self) -> super::RequestMode {
        super::RequestMode::Checkpoints
    }
    fn submit(&mut self, request: &BrainRequest) {
        self.checkpoint_requests.push((
            request.why,
            request.sets.iter().map(|s| s.name.clone()).collect(),
        ));
        self.checkpoint_briefs.push(request.brief.clone());
        self.pending = Some((request.id, request.obs.t + self.delay));
    }
    fn poll(&mut self, now: f32) -> Option<BrainReply> {
        let (id, ready) = self.pending?;
        if now + 1e-4 < ready {
            return None;
        }
        self.pending = None;
        Some(BrainReply {
            id,
            choice: self.choice.clone(),
            ..BrainReply::default()
        })
    }
    fn submit_coach(&mut self, request: &CoachRequest, now: f32) {
        assert!(self.coach_pending.is_empty(), "overlapping coach requests");
        self.coach_times.push(now);
        self.coach_inputs.push(request.input.clone());
        self.coach_pending
            .push((request.id, now + self.coach_delay));
    }
    fn poll_coach(&mut self, now: f32) -> Option<CoachReply> {
        let i = self
            .coach_pending
            .iter()
            .position(|(_, ready)| now + 1e-4 >= *ready)?;
        let (id, _) = self.coach_pending.remove(i);
        let plan = self.plans.pop_front().flatten();
        Some(CoachReply {
            id,
            error: plan.is_none().then(|| "scripted failure".to_string()),
            plan,
            tokens: [1000, 200, 150],
            latency_ms: 1234,
        })
    }
}
