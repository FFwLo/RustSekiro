//! Swimming and diving: Wolf in deep water swims at the surface (SwimIdle, SwimMoveStart / Loop
//! / End, the game's clips and their root motion), and with the Mibu Breathing Technique
//! (EquipParamGoods 2420, a key item) dives: crouch goes under (SwimToDive), and there jump (or
//! E, the user's key for it) rises (DiveMoveUp*), crouch sinks (DiveMoveDown*), the stick swims (DiveMove*) and step
//! kicks ahead (DiveKickMove1); up at the surface he comes out of the dive (DiveToSwim). Where
//! the floor comes up near the surface he stands and walks out (SwimToStandMove).
//!
//! The water is boxes (`WATER`: low corner, high corner whose y is the surface) the map sets:
//! the Construct's pool (sandbox.rs). Held in water the body is the swim's (`hold`): the clips'
//! vertical root motion moves it (the tracks of SwimToDive -1.5 m, DiveToSwim +1.6 m, the up /
//! down loops 4.5 / 3.75 m/s), the surface keeps it at `SWIM_DEPTH`, the box and its floor bound
//! it. gap: the game's water volumes (map hit material 20-24) and their depth rules, the swim
//! sprints and fall entry clips (FallToSwim / Swim / SwimSprint* / DiveSprint* have no clip in
//! c0000's export), breath, underwater combat and the camera's underwater look are not done.

use std::sync::RwLock;

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::CharData;
use crate::player::{Player, CAPSULE_HALF_HEIGHT};

/// The Mibu Breathing Technique (EquipParamGoods 2420): diving needs it held.
pub const MIBU_BREATHING: i64 = 2420;

/// The map's water: (low corner, high corner); the high corner's y is the surface.
pub static WATER: RwLock<Vec<(Vec3, Vec3)>> = RwLock::new(Vec::new());

/// Feet under the surface while swimming at it [m] (the head just out of the water).
pub const SWIM_DEPTH: f32 = 1.35;
/// Water this deep over the feet takes Wolf into the swim [m].
const ENTER_DEPTH: f32 = 1.0;
/// Water shallower than this over the floor lets him stand [m].
const STAND_DEPTH: f32 = 1.2;
/// Turn toward the stick [rad/s].
const TURN_RATE: f32 = 3.5;

#[derive(Clone, Debug, Default)]
pub struct Swim {
    pub diving: bool,
    /// Walking out (SwimToStandMove playing).
    leaving: bool,
}

/// The water box `feet` is in (inside it across, under its surface).
pub fn water_at(feet: Vec3) -> Option<(Vec3, Vec3)> {
    WATER.read().ok()?.iter().copied().find(|(lo, hi)| feet.x > lo.x && feet.x < hi.x && feet.z > lo.z && feet.z < hi.z && feet.y < hi.y && feet.y > lo.y - 1.0)
}

/// The floor under `at` below the surface `surface` (the pool's bottom or its ramp).
fn floor_under(at: Vec3, surface: f32) -> f32 {
    crate::map::terrain().and_then(|t| t.floor(at.x, at.z, surface + 0.2)).unwrap_or(f32::NEG_INFINITY)
}

fn yaw_of(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

fn turn_to(a: &mut Actor, stick: Vec3, dt: f32) {
    if stick.length_squared() < 0.01 {
        return;
    }
    let want = yaw_of(stick);
    let diff = (want - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a.yaw += diff.clamp(-TURN_RATE * dt, TURN_RATE * dt);
}

/// Plays `state` unless it is playing; a finished one starts over (the loops).
fn keep(a: &mut Actor, d: &CharData, state: &str, ended: bool) {
    if a.state != state || ended {
        a.play_state(d, state);
    }
}

/// The swim's input step, in player.rs decide before the hug / hang: true while Wolf swims (the
/// rest of decide does not run). `stick` world space; `jump_held` / `crouch_held` move him up
/// and down under water.
#[allow(clippy::too_many_arguments)]
pub fn tick(p: &mut Player, a: &mut Actor, d: &CharData, tf: &Transform, stick: Vec3, jump_held: bool, crouch_held: bool, crouch_pressed: bool, step_pressed: bool, ended: bool, mibu: bool, dt: f32, log: &mut crate::hud::CombatLog) -> bool {
    let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    let Some((_, hi)) = water_at(feet) else {
        if p.swim.take().is_some() {
            a.play("StandIdle", "");
        }
        return false;
    };
    let surface = hi.y;
    let floor = floor_under(feet, surface);
    let Some(swim) = p.swim.as_mut() else {
        // In: deep enough at the feet, over water too deep to stand in.
        if feet.y < surface - ENTER_DEPTH && surface - floor > STAND_DEPTH {
            let falling = a.airborne;
            p.swim = Some(Swim::default());
            a.airborne = false;
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
            // Walked in: StandMoveToSwim; fallen in (no FallToSwim clip): straight to the idle.
            if falling || !a.play_state(d, "StandMoveToSwim") {
                a.play_state(d, "SwimIdle");
            }
            log.push("swimming", Color::srgb(0.5, 0.75, 1.0));
            return true;
        }
        return false;
    };
    if swim.leaving {
        if ended {
            p.swim = None;
            a.play("StandIdle", "");
        }
        return true;
    }
    let moving = stick.length_squared() > 0.01;
    let busy = matches!(a.state.as_str(), "StandMoveToSwim" | "SwimToDive" | "DiveToSwim" | "DiveKickMove1" | "DiveKickMove2") && !ended;
    if !swim.diving {
        // Out where it gets shallow.
        if surface - floor < STAND_DEPTH && moving {
            swim.leaving = true;
            a.play_state(d, "SwimToStandMove");
            return true;
        }
        if busy {
            turn_to(a, stick, dt);
            return true;
        }
        if crouch_pressed {
            if mibu {
                swim.diving = true;
                a.play_state(d, "SwimToDive");
                log.push("dive", Color::srgb(0.5, 0.75, 1.0));
                return true;
            }
            log.push("diving needs the Mibu Breathing Technique", Color::srgb(0.8, 0.8, 0.8));
        }
        if moving {
            turn_to(a, stick, dt);
            match a.state.as_str() {
                "SwimMoveStart" if ended => a.play_state(d, "SwimMoveLoop"),
                "SwimMoveStart" => true,
                "SwimMoveLoop" => {
                    keep(a, d, "SwimMoveLoop", ended);
                    true
                }
                _ => a.play_state(d, "SwimMoveStart"),
            };
        } else {
            match a.state.as_str() {
                "SwimMoveStart" | "SwimMoveLoop" => {
                    a.play_state(d, "SwimMoveEnd");
                }
                "SwimMoveEnd" if !ended => {}
                _ => keep(a, d, "SwimIdle", ended),
            }
        }
        return true;
    }
    // Diving.
    if busy {
        turn_to(a, stick, dt);
        return true;
    }
    // Up at the surface: out of the dive.
    if jump_held && feet.y >= surface - SWIM_DEPTH - 0.05 {
        swim.diving = false;
        a.play_state(d, "DiveToSwim");
        return true;
    }
    if step_pressed && a.play_state(d, "DiveKickMove1") {
        return true;
    }
    turn_to(a, stick, dt);
    // The loop for what is held, through its start; letting go plays its end.
    let (group, start, lp, end) = if jump_held {
        ("Up", "DiveMoveUpStart", "DiveMoveUpLoop", "DiveMoveUpEnd")
    } else if crouch_held {
        ("Down", "DiveMoveDownStart", "DiveMoveDownLoop", "DiveMoveDownEnd")
    } else if moving {
        ("Move", "DiveMoveStart", "DiveMoveLoop", "DiveMoveEnd")
    } else {
        ("", "", "", "")
    };
    let cur = a.state.clone();
    let same = !group.is_empty() && (cur == start || cur == lp);
    if same {
        if cur == start && ended {
            a.play_state(d, lp);
        } else if cur == lp {
            keep(a, d, lp, ended);
        }
    } else if !group.is_empty() {
        a.play_state(d, start);
    } else {
        let end_of = |s: &str| match s {
            "DiveMoveUpStart" | "DiveMoveUpLoop" => Some("DiveMoveUpEnd"),
            "DiveMoveDownStart" | "DiveMoveDownLoop" => Some("DiveMoveDownEnd"),
            "DiveMoveStart" | "DiveMoveLoop" => Some("DiveMoveEnd"),
            _ => None,
        };
        match end_of(&cur) {
            Some(e) => {
                a.play_state(d, e);
            }
            None if cur.ends_with("End") && !ended => {}
            None => keep(a, d, "DiveIdle", ended),
        }
    }
    let _ = end;
    true
}

/// The clip's vertical root motion over this step (a restarted clip counts from its start).
fn root_dy(d: &CharData, a: &Actor) -> f32 {
    if a.anim.is_empty() {
        return 0.0;
    }
    if a.t >= a.prev_t {
        d.root_y_at(&a.anim, a.t) - d.root_y_at(&a.anim, a.prev_t)
    } else {
        d.root_y_at(&a.anim, a.t) - d.root_y_at(&a.anim, 0.0)
    }
}

/// FixedUpdate after the root motion: Wolf held in the water (no gravity, no floor snap: the
/// Actor's `docked`), at the surface or where the dive's vertical root motion takes him, inside
/// the water box and over its floor.
pub fn hold(combat: Res<crate::data::Combat>, mut q: Query<(&Player, &mut Actor, &mut Transform)>) {
    let d = &combat.player;
    for (p, mut a, mut tf) in &mut q {
        let Some(swim) = p.swim.as_ref() else { continue };
        if swim.leaving {
            // Walking out: back on the floor under him.
            let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
            let floor = crate::map::terrain().and_then(|t| t.floor(feet.x, feet.z, feet.y + 1.5)).unwrap_or(feet.y);
            tf.translation.y = floor + CAPSULE_HALF_HEIGHT;
            a.docked = false;
            continue;
        }
        a.docked = true;
        a.airborne = false;
        a.vel_y = 0.0;
        let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
        let Some((lo, hi)) = water_at(feet).or_else(|| WATER.read().ok()?.iter().copied().find(|(lo, hi)| feet.x > lo.x - 1.0 && feet.x < hi.x + 1.0 && feet.z > lo.z - 1.0 && feet.z < hi.z + 1.0)) else { continue };
        let top = hi.y - SWIM_DEPTH;
        let r = crate::player::CAPSULE_RADIUS;
        let mut f = feet;
        f.x = f.x.clamp(lo.x + r, hi.x - r);
        f.z = f.z.clamp(lo.z + r, hi.z - r);
        let floor = floor_under(f, hi.y);
        let dy = root_dy(d, &a);
        f.y = if swim.diving || a.state == "DiveToSwim" { f.y + dy } else { top };
        f.y = f.y.min(top).max(floor + 0.05);
        tf.translation = f + Vec3::Y * CAPSULE_HALF_HEIGHT;
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_holds_inside_its_box_under_its_surface() {
        // Far from every other test's ground (WATER is global; the sim tests run alongside).
        let o = Vec3::new(-2000.0, 0.0, 0.0);
        WATER.write().unwrap().push((o + Vec3::new(-10.0, -5.0, -5.0), o + Vec3::new(10.0, -0.4, 5.0)));
        assert!(water_at(o + Vec3::new(0.0, -2.0, 0.0)).is_some());
        assert!(water_at(o + Vec3::new(0.0, 0.0, 0.0)).is_none(), "above the surface");
        assert!(water_at(o + Vec3::new(11.0, -2.0, 0.0)).is_none(), "beside the pool");
        WATER.write().unwrap().retain(|w| w.0.x > -1500.0);
    }
}
