//! Hard limits against stalling. A hosted model is asked to make Wolf play worse, and the
//! easiest way to "win" the numbers would be to never fight: back off, wait, run away. The
//! limits below hold whatever a model answers (the driver applies them when it samples a plan;
//! `BrainDriver::anti_stall`, on for every hosted brain):
//!
//! - **Attack rule.** A plan in a plan set must contain an attack step; a plan with none (a lone
//!   tactic, a back step) is never picked. The coach's `opener` is exempt.
//! - **Passive limit.** After [`MAX_PASSIVE_SECS`] without an attack starting, only plans that
//!   reach an attack within [`OVERDUE_LEAD_SECS`] are eligible; when the active set has none,
//!   an allowed attack from the boss's table runs. A plan is also not eligible when its steps before the
//!   first attack could take longer than the time left (`lead_secs`, the sum of the tactics'
//!   lifetime caps) plus that margin.
//! - **Retreat rate.** At most one plan with a `back_off`, `retreat` or back step per
//!   [`RETREAT_INTERVAL_SECS`].
//!
//! The same numbers feed the metrics: a fight is *passive* at any moment when no attack step
//! has started in the last [`PASSIVE_WINDOW_SECS`], and a fight whose passive share is above
//! [`PASSIVE_FAIL_SHARE`] counts as failed (the coach is told so).

use super::actions::{ActionKind, BossActions};
use super::plan::{Plan, Step, Tactic};

/// Seconds without starting an attack after which only attack-first plans are eligible.
pub const MAX_PASSIVE_SECS: f32 = 6.0;
/// Margin, seconds, an overdue or nearly overdue plan may spend before its first attack.
pub const OVERDUE_LEAD_SECS: f32 = 1.0;
/// Seconds between plans that back off, retreat or step back.
pub const RETREAT_INTERVAL_SECS: f32 = 5.0;
/// A moment is passive when no attack step started in this many seconds (the `passive_share`
/// metric).
pub const PASSIVE_WINDOW_SECS: f32 = 4.0;
/// A fight with a passive share above this failed, whatever its other numbers.
pub const PASSIVE_FAIL_SHARE: f32 = 0.35;

/// Whether a plan has an attack step (a boss action that is not a step).
pub fn has_attack(plan: &Plan, actions: &BossActions) -> bool {
    plan.steps.iter().any(|s| match s {
        Step::Act(id) => actions
            .get(*id)
            .is_some_and(|a| a.kind == ActionKind::Attack),
        Step::Tactic(_) => false,
    })
}

/// Whether a plan backs away: a `back_off` or `retreat` tactic, or one of his back steps.
pub fn is_retreat(plan: &Plan, actions: &BossActions) -> bool {
    plan.steps.iter().any(|s| match s {
        Step::Act(id) => actions.get(*id).is_some_and(|a| a.kind == ActionKind::Step),
        Step::Tactic(t) => matches!(
            t,
            Tactic::BackOff(_) | Tactic::Retreat(_) | Tactic::DodgeBack
        ),
    })
}

/// The most seconds the steps before the plan's first attack can take (the tactics' lifetime
/// caps); the whole plan's tactic time when it has no attack.
pub fn lead_secs(plan: &Plan, actions: &BossActions) -> f32 {
    let mut lead = 0.0;
    for s in &plan.steps {
        match s {
            Step::Tactic(t) => lead += t.cap(),
            Step::Act(id) => {
                if actions
                    .get(*id)
                    .is_some_and(|a| a.kind == ActionKind::Attack)
                {
                    break;
                }
                // A step back takes a couple of seconds.
                lead += 2.0;
            }
        }
    }
    lead
}

/// What the limits need to know of the fight so far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pacing {
    /// When the last attack step started (the fight's start before the first one).
    pub last_attack: f32,
    /// When the last plan that retreats started.
    pub last_retreat: Option<f32>,
}

impl Pacing {
    pub fn new(start: f32) -> Self {
        Self {
            last_attack: start,
            last_retreat: None,
        }
    }

    /// Whether a plan from a plan set may be picked at fight time `now`.
    pub fn eligible(&self, plan: &Plan, now: f32, actions: &BossActions) -> bool {
        if !has_attack(plan, actions) {
            return false;
        }
        if is_retreat(plan, actions)
            && self
                .last_retreat
                .is_some_and(|t| now - t < RETREAT_INTERVAL_SECS)
        {
            return false;
        }
        let left = (MAX_PASSIVE_SECS - (now - self.last_attack)).max(0.0);
        lead_secs(plan, actions) <= left + OVERDUE_LEAD_SECS
    }
}
