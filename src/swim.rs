//! Swimming and diving: Wolf in deep water swims at the surface (SwimIdle, SwimMoveStart / Loop
//! / End, the game's clips and their root motion), and with the Mibu Breathing Technique
//! (EquipParamGoods 2420, a key item) dives: Ctrl goes under (SwimToDive), and there F rises
//! (DiveMoveUp*; at the top he comes out of the dive, DiveToSwim), Ctrl sinks (DiveMoveDown*) and
//! the stick swims (DiveMove*); Shift held swims fast, a chain of kicks (`kicks`) at the surface
//! and under it. Where the floor comes up near the surface he stands and walks out
//! (SwimToStandMove).
//!
//! The water is boxes (`WATER`: low corner, high corner whose y is the surface) the map sets:
//! the Construct's pool (sandbox.rs). Held in water the body is the swim's (`hold`): the clips'
//! vertical root motion moves it (the tracks of SwimToDive -1.5 m, DiveToSwim +1.6 m, the up /
//! down loops 4.5 / 3.75 m/s), the surface keeps it at `SWIM_DEPTH`, the box and its floor bound
//! it. gap: the game's water volumes (map hit material 20-24) and their depth rules, the fall
//! entry clips (FallToSwim / Swim have no clip in c0000's export), breath, underwater combat and
//! the camera's underwater look are not done.

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
    /// The Mibu Breathing Technique held (the action guide's Dive).
    pub mibu: bool,
    /// Steering while rising or sinking (the up / down clips go straight up) and the camera's
    /// climb or dive while swimming ahead under water: a world velocity `hold` applies.
    drift: Vec3,
    /// Walking out (SwimToStandMove, cut short by the stick from its cancels at 0.3 s).
    leaving: bool,
    /// An attack pressed during a swing: the next of the combo once this one has struck.
    queued: bool,
}

/// The sword in the water (c0000 W_SwimAttackCombo1-3 / W_DiveAttackCombo1-3, the a050 set:
/// a050_310000 / 310010 / 310020 at the surface, 311000 / 311010 / 311020 under it): each strikes
/// at 0.7 s (TAE 1, judges 86 / 87 / 86). A press during one queues the next, chained from
/// `COMBO_NEXT`; the stick swims on from its ChrActionFlag 11 at 1.5 s. gap: the Release ones
/// (the hold) and the kick attacks (no clip) are not used, nor the exe's own chain window.
const COMBO_NEXT: f32 = 1.0;
const ATTACK_TO_MOVE: f32 = 1.5;

/// The water attack state `n` (1-3) at the surface or under it.
fn combo(under: bool, n: u32) -> String {
    format!("{}AttackCombo{n}", if under { "Dive" } else { "Swim" })
}

/// The combo step of a water attack state (1-3), if it is one.
fn combo_step(state: &str) -> Option<u32> {
    let rest = state.strip_prefix("SwimAttackCombo").or_else(|| state.strip_prefix("DiveAttackCombo"))?;
    rest.parse().ok()
}

/// Items in the water: only the Healing Gourd and the Homeward Idol have water clips (c0000
/// Swim / DiveItemGourdDrink a000_250030 / 250040, Swim / DiveItemReturnStatue 251530 / 251540;
/// their TAE 65 at 0.7 / 7.5 s). The use state for `dry` (items.rs use_state / the gourd's), at
/// the surface or under it; None: not usable in the water.
pub fn item_state(dry: &str, under: bool) -> Option<String> {
    matches!(dry, "ItemGourdDrink" | "ItemReturnStatue").then(|| format!("{}{dry}", if under { "Dive" } else { "Swim" }))
}

/// A Healing Gourd drink in the water (player.rs heals at its TAE 65 as on land).
pub fn is_water_drink(state: &str) -> bool {
    matches!(state, "SwimItemGourdDrink" | "DiveItemGourdDrink")
}

/// Under water Wolf stays this far below the surface's swim height [m]: a ceiling the stroke and
/// the camera's climb stop at; only F there brings him out of the dive (the user: "manual
/// underwater swimming shouldn't automatically lead wolf out from diving unless pressing f").
const CEILING: f32 = 0.3;

/// SwimToStandMove's cancels open (ChrActionFlag 26 / 119 ... at 0.3 s): the stick walks on.
const OUT_CANCEL: f32 = 0.3;
/// The camera's resting pitch (camera.rs START_PITCH 15 deg, looking a little down): swimming
/// ahead with the camera there stays level; tilted from it, Wolf climbs or dives along it.
const PITCH_LEVEL: f32 = 15.0;
/// Under water the camera's tilt within this of level swims level [deg].
const PITCH_DEAD: f32 = 6.0;
/// The swim jump (c0000 W_SwimKickMoveJump*: Ready a000_201600 -> Start 201610, launched by
/// ChrPhysicsVelocityChangeParam 221 (9.15 m/s ahead, 6.82 up, fallType 1) -> Loop 201630; landing
/// LandSwimKickMoveJump 201640 on ground, LandWaterSwimKickMoveJump 201660 back in the water).
pub const JUMP_READY: &str = "SwimKickMoveJumpReady";
pub const JUMP_START: &str = "SwimKickMoveJumpStart";
pub const JUMP_LOOP: &str = "SwimKickMoveJumpLoop";
pub const JUMP_VELOCITY_ROW: i64 = 221;

/// The swim jump in the air (player.rs decide, every tick): its start into its loop.
pub fn air(a: &mut Actor, d: &CharData, ended: bool) {
    if ended && (a.state == JUMP_START || a.state == JUMP_LOOP) {
        a.play_state(d, JUMP_LOOP);
    }
}

/// What the swim step leaves to player.rs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Swimming: the rest of decide does not run.
    Held,
    /// Out of the water (or never in it): decide carries on.
    Free,
    /// Jumped out at the surface: decide launches the jump.
    Jump,
}

/// Steering speed while rising or sinking [m/s] (about the dive stroke's).
const DRIFT_SPEED: f32 = 3.0;
/// The dive stroke's speed [m/s] (DiveMoveLoop: 20.8 m in 4.5 s), the camera's climb or dive
/// taken along it.
const DIVE_SPEED: f32 = 4.6;

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

/// The swim's controls (the user, 2026-10-18): Ctrl dives from the surface and sinks under
/// water, F rises (and at the top comes out of the dive), Shift held swims fast.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// The stick, world space.
    pub stick: Vec3,
    pub rise_held: bool,
    pub sink_held: bool,
    pub dive_pressed: bool,
    pub fast_held: bool,
    /// Jump at the surface: out of the water.
    pub jump_pressed: bool,
    /// The camera's pitch [rad] (positive looking down): under water the way ahead climbs or
    /// dives with it.
    pub pitch: f32,
    pub attack_pressed: bool,
}

/// The fast swim's kicks: at the surface the clips a000_031400 / 031410 (the HKS's
/// W_SwimSprintStart / Loop, exported by key: their states have no clip of their own), under
/// water DiveKickMove1 / 2 (041400 / 041410). Each bursts to 11 / 17 m/s and fades; held, the
/// next kicks in where the clip takes a queued dodge (ChrActionFlag 141 "Underwater_End If Dodge
/// Queued": 0.9 s at the surface, 1.0 s under), let go the stick takes over at 1.5 s
/// (flag 11 "End If LS Move Queued").
const SURFACE_KICKS: [(&str, &str); 2] = [("SwimSprint1", "a000_031400"), ("SwimSprint2", "a000_031410")];
const DIVE_KICKS: [&str; 2] = ["DiveKickMove1", "DiveKickMove2"];
const KICK_CHAIN: [f32; 2] = [0.9, 1.0];
const KICK_TO_MOVE: f32 = 1.5;

/// The fast swim, surface (`under` false) or under water: true while a kick plays or starts.
fn kicks(a: &mut Actor, d: &CharData, c: &Controls, under: bool, moving: bool, ended: bool, dt: f32) -> bool {
    let idx = if under { DIVE_KICKS.iter().position(|k| a.state == *k) } else { SURFACE_KICKS.iter().position(|(k, _)| a.state == *k) };
    let play = |a: &mut Actor, i: usize| {
        if under {
            a.play_state(d, DIVE_KICKS[i]);
        } else {
            a.play(SURFACE_KICKS[i].0, SURFACE_KICKS[i].1);
        }
    };
    match idx {
        None if c.fast_held && moving => {
            play(a, 0);
            true
        }
        None => false,
        Some(i) => {
            turn_to(a, c.stick, dt);
            if c.fast_held && moving && a.t >= KICK_CHAIN[under as usize] {
                play(a, 1 - i);
                true
            } else if ended || (!c.fast_held && moving && a.t >= KICK_TO_MOVE) {
                false
            } else {
                true
            }
        }
    }
}

/// The swim's input step, in player.rs decide before the hug / hang: true while Wolf swims (the
/// rest of decide does not run).
#[allow(clippy::too_many_arguments)]
pub fn tick(p: &mut Player, a: &mut Actor, d: &CharData, tf: &Transform, c: Controls, ended: bool, mibu: bool, dt: f32, log: &mut crate::hud::CombatLog) -> Step {
    use Step::{Free, Held};
    let stick = c.stick;
    let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    let Some((_, hi)) = water_at(feet) else {
        if p.swim.take().is_some() {
            a.play("StandIdle", "");
        }
        return Free;
    };
    let surface = hi.y;
    let floor = floor_under(feet, surface);
    let Some(swim) = p.swim.as_mut() else {
        // In: deep enough at the feet, over water too deep to stand in; not on the way up out of
        // it (a jump from the surface).
        let rising = a.airborne && a.vel_y > 0.0;
        if feet.y < surface - ENTER_DEPTH && surface - floor > STAND_DEPTH && !rising {
            let falling = a.airborne;
            p.swim = Some(Swim::default());
            a.airborne = false;
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
            // Into the water: a swim jump lands back in it (LandWaterSwimKickMoveJump), a fall
            // lands in place or swimming on (LandWaterFreeFallInSitu / SwimMoveStartFromLandWater;
            // the HKS's LandWaterFreeFall has no clip), a walk wades in (StandMoveToSwim).
            let state = if a.state.starts_with("SwimKickMoveJump") {
                "LandWaterSwimKickMoveJump"
            } else if falling && stick.length_squared() > 0.01 {
                "SwimMoveStartFromLandWater"
            } else if falling {
                "LandWaterFreeFallInSitu"
            } else {
                "StandMoveToSwim"
            };
            if !a.play_state(d, state) {
                a.play_state(d, "SwimIdle");
            }
            log.push("swimming", Color::srgb(0.5, 0.75, 1.0));
            return Held;
        }
        return Free;
    };
    swim.mibu = mibu;
    swim.drift = Vec3::ZERO;
    let moving = stick.length_squared() > 0.01;
    // Walking out: the clip until the stick cuts it at its cancels (the user: "remove the endlag",
    // then "add transition from water to surface").
    if swim.leaving {
        if ended || (moving && a.t >= OUT_CANCEL) {
            p.swim = None;
            a.docked = false;
            a.play("StandIdle", "");
            return Free;
        }
        return Held;
    }
    // The sword (`combo`): a press starts or queues; a struck swing chains the queued one; the
    // stick swims on from its flag 11; at its end back to swimming.
    let under = swim.diving;
    if let Some(n) = combo_step(&a.state) {
        if c.attack_pressed {
            swim.queued = true;
        }
        if swim.queued && n < 3 && a.t >= COMBO_NEXT && a.play_state(d, &combo(under, n + 1)) {
            swim.queued = false;
            if moving {
                a.yaw = yaw_of(stick);
            }
            return Held;
        }
        if !(ended || (moving && a.t >= ATTACK_TO_MOVE)) {
            return Held;
        }
        swim.queued = false;
    } else if c.attack_pressed && !matches!(a.state.as_str(), "SwimToDive" | "DiveToSwim" | JUMP_READY) {
        if moving {
            a.yaw = yaw_of(stick);
        }
        if a.play_state(d, &combo(under, 1)) {
            swim.queued = false;
            return Held;
        }
    }
    // An item in the water (player.rs started it): its clip plays out.
    if (a.state.contains("ItemGourdDrink") || a.state.contains("ItemReturnStatue")) && !ended {
        return Held;
    }
    let landing = matches!(a.state.as_str(), "LandWaterSwimKickMoveJump" | "LandWaterFreeFallInSitu" | "SwimMoveStartFromLandWater");
    let busy = (matches!(a.state.as_str(), "StandMoveToSwim" | "SwimToDive" | "DiveToSwim" | JUMP_READY) || landing) && !ended;
    if !swim.diving {
        // Out where it gets shallow: SwimToStandMove (SwimToStandMoveInSitu standing still).
        if surface - floor < STAND_DEPTH && moving {
            swim.leaving = true;
            a.play_state(d, "SwimToStandMove");
            return Held;
        }
        // The swim jump: its Ready, then out of the water (player.rs launches the Start). The
        // landings and the wade-in give way to it; the dive turns do not.
        if a.state == JUMP_READY && ended {
            p.swim = None;
            a.docked = false;
            return Step::Jump;
        }
        if c.jump_pressed && !(busy && !landing && a.state != "StandMoveToSwim") {
            if moving {
                a.yaw = yaw_of(stick);
            }
            a.play_state(d, JUMP_READY);
            return Held;
        }
        // A landing in the water goes on into swimming as soon as the stick moves.
        if landing && moving && a.t >= OUT_CANCEL {
            a.play_state(d, "SwimMoveLoop");
            return Held;
        }
        if busy {
            turn_to(a, stick, dt);
            return Held;
        }
        if c.dive_pressed {
            if mibu {
                swim.diving = true;
                a.play_state(d, "SwimToDive");
                log.push("dive", Color::srgb(0.5, 0.75, 1.0));
                return Held;
            }
            log.push("diving needs the Mibu Breathing Technique", Color::srgb(0.8, 0.8, 0.8));
        }
        if kicks(a, d, &c, false, moving, ended, dt) {
            return Held;
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
        return Held;
    }
    // Diving.
    if busy {
        turn_to(a, stick, dt);
        return Held;
    }
    // Swimming ahead, the camera's tilt from level climbs or dives along it (the user: "there
    // should be vertical movement underwater, not pressing f or ctrl").
    let tilt = c.pitch.to_degrees() - PITCH_LEVEL;
    let climb = if moving && tilt.abs() > PITCH_DEAD { -(tilt - PITCH_DEAD * tilt.signum()).to_radians().sin() * DIVE_SPEED } else { 0.0 };
    // Up at the ceiling: out of the dive with F only (swimming up into it stops there).
    let at_top = feet.y >= surface - SWIM_DEPTH - CEILING - 0.05;
    if at_top && c.rise_held {
        swim.diving = false;
        a.play_state(d, "DiveToSwim");
        return Held;
    }
    if kicks(a, d, &c, true, moving, ended, dt) {
        return Held;
    }
    turn_to(a, stick, dt);
    // Rising or sinking, the stick steers him across too.
    if (c.rise_held || c.sink_held) && moving {
        swim.drift = stick.with_y(0.0).normalize_or_zero() * DRIFT_SPEED;
    } else {
        swim.drift.y = climb;
    }
    // The loop for what is held, through its start; letting go plays its end.
    let (start, lp) = if c.rise_held {
        ("DiveMoveUpStart", "DiveMoveUpLoop")
    } else if c.sink_held {
        ("DiveMoveDownStart", "DiveMoveDownLoop")
    } else if moving {
        ("DiveMoveStart", "DiveMoveLoop")
    } else {
        ("", "")
    };
    let cur = a.state.clone();
    if !start.is_empty() && (cur == start || cur == lp) {
        if cur == start && ended {
            a.play_state(d, lp);
        } else if cur == lp {
            keep(a, d, lp, ended);
        }
    } else if !start.is_empty() {
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
    Held
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
pub fn hold(time: Res<Time>, combat: Res<crate::data::Combat>, mut q: Query<(&Player, &mut Actor, &mut Transform)>) {
    let d = &combat.player;
    for (p, mut a, mut tf) in &mut q {
        let Some(swim) = p.swim.as_ref() else { continue };
        if swim.leaving {
            // Walking out on the ramp: the floor under him.
            let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
            if let Some(floor) = crate::map::terrain().and_then(|t| t.floor(feet.x, feet.z, feet.y + 1.5)) {
                tf.translation.y = floor + CAPSULE_HALF_HEIGHT;
            }
            a.docked = false;
            continue;
        }
        a.docked = true;
        tf.translation += swim.drift * time.delta_secs();
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
        // Under water the ceiling (`CEILING`) holds him below the surface until F brings him out.
        let ceiling = if swim.diving { top - CEILING } else { top };
        f.y = f.y.min(ceiling).max(floor + 0.05);
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
