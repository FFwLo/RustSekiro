//! Choosing the next plan from a plan set locally, so variety needs no network calls.
//!
//! A controller's answer is a weighted set of plans (a hosted model's probability for each
//! choice). Each act, the driver samples one with
//!
//! `weight = probability^(1/temperature) * 0.5^streak * (1.5 if untried this fight)`
//!
//! where `streak` is how many times in a row the plan's actions (or tactics) were repelled this
//! fight (an attack that was deflected or guarded and never landed; a tactic during which Wolf
//! landed a hit), summed over the plan's steps. A set that is a single plan (no probabilities,
//! the Responses backend) is not sampled: that plan is dropped for the Lua AI's choice once any
//! of its steps was repelled [`EXCLUDE_STREAK`] times in a row.

use std::collections::{HashMap, HashSet};

use super::plan::{Plan, Step, Weighted};

/// Repelled this many times in a row, an option is excluded where there is no distribution to
/// sample, and left out of a Responses request.
pub const EXCLUDE_STREAK: u32 = 3;
/// Weight factor per repelled exchange in a row.
pub const PENALTY: f32 = 0.5;
/// Weight factor for a plan not chosen yet in this fight.
pub const UNTRIED_BONUS: f32 = 1.5;

/// A tiny xorshift generator, so sampling is seeded and deterministic in tests.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub(crate) fn from_state(state: u64) -> Self {
        Self(state)
    }
    pub fn new(seed: u64) -> Self {
        // Never zero (xorshift would stay there).
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
}

/// Consecutive repelled exchanges per action id or tactic token in this fight.
#[derive(Debug, Clone, Default)]
pub struct Streaks(HashMap<String, u32>);

impl Streaks {
    /// Notes how an exchange of `key` ended: repelled adds one, anything else resets.
    pub fn record(&mut self, key: &str, repelled: bool) {
        if repelled {
            *self.0.entry(key.to_string()).or_default() += 1;
        } else {
            self.0.remove(key);
        }
    }

    pub fn get(&self, key: &str) -> u32 {
        self.0.get(key).copied().unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// The most repelled steps in a row in `plan`.
    pub fn worst(&self, plan: &Plan) -> u32 {
        plan.steps
            .iter()
            .map(|s| self.get(&key_of(s)))
            .max()
            .unwrap_or(0)
    }

    /// The sum over a plan's steps.
    fn total(&self, plan: &Plan) -> u32 {
        plan.steps.iter().map(|s| self.get(&key_of(s))).sum()
    }
}

/// The streak key of a step: the action id or the tactic token.
pub fn key_of(step: &Step) -> String {
    match step {
        Step::Act(a) => a.to_string(),
        Step::Tactic(t) => t.token(),
    }
}

/// What the sampler weighs besides the controller's probabilities.
pub struct Context<'a> {
    pub streaks: &'a Streaks,
    /// Plan texts chosen already in this fight.
    pub tried: &'a HashSet<String>,
    /// Softmax temperature: above 1 flattens the probabilities, below 1 sharpens them, 0 or
    /// less always takes the heaviest.
    pub temperature: f32,
}

fn weight(entry: &Weighted, ctx: &Context<'_>) -> f32 {
    let base = entry.weight.max(1e-6);
    let mut w = if ctx.temperature > 0.0 {
        base.powf(1.0 / ctx.temperature)
    } else {
        base
    };
    if let Some(plan) = &entry.plan {
        w *= PENALTY
            .powi(ctx.streaks.total(plan).min(30) as i32)
            .max(super::adaptation::FLOOR);
        if !ctx.tried.contains(&plan.value()) {
            w *= UNTRIED_BONUS;
        }
    }
    w
}

/// What [`pick`] chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// The set is empty: nothing was chosen.
    Nothing,
    /// Leave this act to the Lua AI.
    Lua,
    /// The plan at this index.
    Plan(usize),
}

/// The plan to run next from a set.
pub fn pick(set: &[Weighted], ctx: &Context<'_>, rng: &mut Rng) -> Pick {
    match set {
        [] => return Pick::Nothing,
        [only] => {
            // Nothing to sample: drop a plan that keeps being repelled for the Lua AI's act.
            let worn = only
                .plan
                .as_ref()
                .is_some_and(|p| ctx.streaks.worst(p) >= EXCLUDE_STREAK);
            return if worn || only.plan.is_none() {
                Pick::Lua
            } else {
                Pick::Plan(0)
            };
        }
        _ => {}
    }
    let weights: Vec<f32> = set.iter().map(|e| weight(e, ctx)).collect();
    let chosen = |i: usize| {
        if set[i].plan.is_some() {
            Pick::Plan(i)
        } else {
            Pick::Lua
        }
    };
    if ctx.temperature <= 0.0 {
        let best = weights
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or(0, |(i, _)| i);
        return chosen(best);
    }
    let total: f32 = weights.iter().sum();
    let mut x = rng.next_f32() * total;
    for (i, w) in weights.iter().enumerate() {
        if x < *w {
            return chosen(i);
        }
        x -= w;
    }
    chosen(set.len() - 1)
}
