//! What a controller sees: only what the player can see of the boss, mirrored the other way.
//!
//! Both sides get the same fields (HP and posture as fractions, lives, what they are doing);
//! there are no item counts, AI numbers, params or other hidden state. Numbers are rounded to
//! two decimals so a request stays small (well under 1 KB of JSON).

use serde::Serialize;

use crate::character::Character;
use crate::deathblow::deathblow_marks;
use crate::duel::{Duel, FLAG_PARRY_TIMING};
use crate::fight::Fighter;
use crate::hits::FLAG_GUARD;
use crate::lua_ai::anim_id_from_clip;

/// One side as the other sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FighterView {
    /// HP left, fraction of max.
    pub hp: f32,
    /// Posture damage built up, fraction of max (the bar the game shows: 0 calm, 1 broken).
    pub posture: f32,
    /// Lives left: deathblow marks for the boss, Wolf's own for him.
    pub lives: i32,
    /// The behaviour state playing (its leaf name, such as `Attack3005` or `StandDamageMiddle`).
    pub state: String,
    /// The boss's current action id (an attack, step or guard animation); `None` for Wolf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act: Option<i32>,
    /// Attacking: for Wolf, his clip is in the window the game signals to enemies as an
    /// incoming attack (TAE flag 63); for the boss, an attack action plays.
    pub attacking: bool,
    pub guarding: bool,
    pub airborne: bool,
    /// In a damage or recoil reaction (behaviour state group `*Damage*`).
    pub staggered: bool,
    /// Can be deathblown now (posture broken, or 1 HP with lives left).
    pub deathblow_open: bool,
}

/// One request's view of the fight.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Observation {
    /// Fight time, seconds.
    pub t: f32,
    pub boss: FighterView,
    pub wolf: FighterView,
    /// Horizontal distance, metres.
    pub dist: f32,
    /// Wolf's height above the boss, metres.
    pub dy: f32,
    /// Where Wolf is from the boss's facing, degrees, positive to the left.
    pub wolf_bearing: f32,
    /// Where the boss is from Wolf's facing, degrees, positive to the left.
    pub boss_bearing: f32,
    /// Wolf is locked on to the boss.
    pub locked_on: bool,
    /// Seconds since the boss last landed an unguarded hit on Wolf (`None`: never).
    pub since_boss_hit: Option<f32>,
    /// Seconds since Wolf last landed an unguarded hit on the boss.
    pub since_wolf_hit: Option<f32>,
    /// The plan running now (empty while the Lua AI's own act runs).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub plan: String,
}

/// Rounds to two decimals.
pub fn r2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

/// Signed angle in degrees from `from`'s facing to `to`, positive to the left (yaw 0 faces -Z,
/// as in `crate::body`).
fn bearing(from: &Character, to: &Character) -> f32 {
    let (p, q) = (from.body.position, to.body.position);
    let (dx, dz) = (q[0] - p[0], q[2] - p[2]);
    if dx == 0.0 && dz == 0.0 {
        return 0.0;
    }
    let yaw = (-dx).atan2(-dz);
    let mut a = (yaw - from.body.yaw) % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a.to_degrees()
}

fn view(duel: &Duel, c: &Character, f: &Fighter, i: usize) -> FighterView {
    let path = c.behavior.runtime.main_state_path();
    let act = (i == 1)
        .then(|| {
            c.behavior
                .runtime
                .main_clip()
                .and_then(|clip| anim_id_from_clip(&clip.animation))
        })
        .flatten()
        .filter(|&a| (3000..10000).contains(&a));
    let frame = c.tae_frame();
    FighterView {
        hp: r2(f.hp_fraction()),
        posture: r2(f.posture_fraction()),
        lives: if f.dead() {
            0
        } else {
            deathblow_marks(f.vitality.deathblows_left)
        },
        state: path.last().cloned().unwrap_or_default(),
        act,
        attacking: if i == 0 {
            frame.flag(FLAG_PARRY_TIMING)
        } else {
            act.is_some_and(|a| (3000..3100).contains(&a))
        },
        guarding: frame.flag(FLAG_GUARD),
        airborne: !c.body.grounded,
        staggered: path.first().is_some_and(|s| s.contains("Damage")),
        deathblow_open: duel.rules.deathblow_open(i),
    }
}

/// The observation for the duel now. `t` is the fight clock; `since` the seconds since each
/// side's last landed hit `[boss, wolf]`; `plan` the plan running.
pub fn observe(duel: &Duel, t: f32, since: [Option<f32>; 2], plan: &str) -> Observation {
    let (p, e) = (&duel.player, &duel.enemy);
    let (pp, ep) = (p.body.position, e.body.position);
    Observation {
        t: r2(t),
        boss: view(duel, e, &duel.rules.fighters[1], 1),
        wolf: view(duel, p, &duel.rules.fighters[0], 0),
        dist: r2(((pp[0] - ep[0]).powi(2) + (pp[2] - ep[2]).powi(2)).sqrt()),
        dy: r2(pp[1] - ep[1]),
        // + 0.0 turns -0 into 0.
        wolf_bearing: bearing(e, p).round() + 0.0,
        boss_bearing: bearing(p, e).round() + 0.0,
        locked_on: p.lock_target.is_some(),
        since_boss_hit: since[0].map(r2),
        since_wolf_hit: since[1].map(r2),
        plan: plan.to_string(),
    }
}
