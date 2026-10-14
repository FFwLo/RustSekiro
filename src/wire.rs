//! The Grappling Hook (the engine's "wire"): Wolf's side of docs/kb/grapple.md (sections 3 and 5).
//! The points and the search are map.rs (`map::grapple_target`); this is the move: the shoot, the
//! reel, the launch with guaranteed arrival and the landing.
//!
//! The states (c0000 behavior, HKS BEH_A_GROUND_WIRE_SHOOT 107 / BEH_A_AIR_WIRE_SHOOT 108):
//! GroundWireShoot (a000_202000) / AirWireShoot (202100) -> Ground/AirWireMoveStart_<index>
//! (202010-202012 / 202110-202112) -> WireMove (202020, the flight loop) -> LandWireMove (202035).
//! Out of a wire move the press fires again: WireMoveWireShoot (202600) -> WireMoveWireMoveStart
//! (202610), and from that one WireMoveWire2Shoot (202700) -> WireMoveWire2MoveStart (202710).
//!
//! What the TAE does (exe handlers in the TAE dispatcher FUN_140b59170):
//! - 900 WireSyncWithTarget_Start {50} (FUN_140b58290): the rope's tip flies to the hook at the
//!   argument's speed, m/s (the wire object's +0x604, read by FUN_140e719b0: tip += dir * 50 * dt
//!   until it reaches the target). 901 WireSyncWithTarget_Travel (FUN_140b58240) keeps the tip on
//!   the hook while Wolf reels in.
//! - 904 WireSlideToStartPoint (FUN_140b54c90 -> FUN_140bb1870): a TAE 922-style velocity blend
//!   over the event with ChrPhysicsVelocityChangeParam 1101 ("ground wire acceleration": +30 m/s
//!   along the facing, +30 m/s up) and the curves from .data 0x143b09db0 (horizontal Linear 1,
//!   vertical EaseIn 10) - player.rs runs it with the 922 code.
//! - 920 row 1100 (fallType 2 Wire, homingId 300000000) launches: ChrPhysicsHomingParam
//!   300000000 initialVelocityCorrection 1, fallCorrectionGuaranteeArrival 2. gap: the exe's
//!   correction is not traced; the flight is read off the TAE instead: every move start switches
//!   the camera slot 18 -> 19 (TAE 155) a fixed time after its launch (202010 0.67 s, 202011 /
//!   202012 0.83 s, the air and wire-to-wire ones 0.67 s) - taken as the arrival time, the launch
//!   velocity solved so the arc under the Wire fall's gravity arrives then, and the horizontal
//!   velocity re-aimed every frame (as the plunge's guaranteed arrival, player.rs). The real
//!   game's skills clip (7:53, an air grapple onto a roof) flies about 0.6 s.
//! - 905 WireDisableFalling: holds Wolf in the air (AirWireShoot 0-0.5 s).

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::{CharData, FALL_TYPES};
use crate::map::{GrapplePoints, GrappleTarget};
use crate::player::{CAPSULE_HALF_HEIGHT, Player};

/// ChrPhysicsHomingParam rows of the wire arc: 300000000 + the point's chrPhysicsHomingIdOffset
/// (WireVariationParam: 20 "can't ride default", 24 "can't ride, castle snake valley", 40 / 41 the
/// kites; every other map row 0).
pub const WIRE_HOMING: i64 = 300_000_000;

/// The Wire fall type (ChrPhysicsVelocityChangeParam 1100 fallType 2).
const WIRE_FALL: u8 = 2;

/// TAE 155 SetLockParamID slot of the flight's end (see the module doc).
const LOCK_SLOT_ARRIVED: i64 = 19;
/// Flight time when an anim has no slot-19 event (the air move starts' value).
const DEFAULT_FLIGHT: f32 = 0.667;

/// TAE 224 SetTurnSpeed of the shoots: 1080 deg/s over 0-0.2 s (202000 / 202100), 2700 deg/s
/// over 0-0.1 s (the wire-to-wire 202600 / 202700).
const SHOOT_TURN: [(&str, f32, f32); 4] =
    [("GroundWireShoot", 1080.0, 0.2), ("AirWireShoot", 1080.0, 0.2), ("WireMoveWireShoot", 2700.0, 0.1), ("WireMoveWire2Shoot", 2700.0, 0.1)];

/// The point the hook would fire at this frame: `map::grapple_target` over the camera view, the
/// map's points and the enemies open to the wire (enemy::grapple::EnemyGrapples).
#[derive(Resource, Default)]
pub struct WireAim {
    pub target: Option<GrappleTarget>,
    /// The target is this enemy, with its arc row (ChrPhysicsHomingParam 3000<variation>).
    pub enemy: Option<(Entity, i64)>,
}

/// A wire move in progress.
#[derive(Clone, Copy, Debug)]
pub struct Wire {
    pub hook: Vec3,
    /// The landing spot (Wolf's feet) of a ridden point.
    pub land: Vec3,
    pub ride: bool,
    /// ChrPhysicsHomingParam row of the arc.
    pub homing: i64,
    /// HKS WireMoveStartIndex: 0 upslope, 1 vertical, 2 horizontal.
    pub index: usize,
    /// Where the arc ends (Wolf's centre), set at the launch.
    pub dest: Vec3,
    /// Guaranteed-arrival time left after the launch (None before it and once arrived).
    pub flight: Option<f32>,
    /// The enemy the hook is fired at, and whether its WireHit went out.
    pub enemy: Option<Entity>,
    pub hit_sent: bool,
}

/// The rope's ends this frame (hand dummy 144 -> the hook's tip) while TAE 717 runs, for the
/// wire model (model/tool.rs); `model`: that model is loaded and draws it (else a plain line).
#[derive(Resource, Default)]
pub struct WireRope {
    pub ends: Option<(Vec3, Vec3)>,
    pub model: bool,
}

pub struct WirePlugin;

impl Plugin for WirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WireAim>()
            .init_resource::<WireRope>()
            .add_systems(FixedUpdate, aim_wire.before(crate::actor::ActorSet::Decide))
            .add_systems(FixedUpdate, hook_enemy.after(crate::actor::ActorSet::Decide))
            .add_systems(Update, draw_rope);
    }
}

fn aim_wire(
    points: Res<GrapplePoints>,
    enemies: Option<Res<crate::enemy::grapple::EnemyGrapples>>,
    cams: Query<&GlobalTransform, With<crate::camera::OrbitCamera>>,
    wolf: Query<&Transform, With<Player>>,
    mut aim: ResMut<WireAim>,
) {
    // Without a camera (the sim tests) the aim is left as set.
    let (Ok(cam), Ok(w)) = (cams.single(), wolf.single()) else { return };
    let prev = aim.target.map(|t| t.index);
    // The enemies join the map's points: the hook on their wire dummy, recognised from every
    // side (gap: the enemy rows' recognition angles are not read; taken as all round).
    let mut pts = points.0.clone();
    let n = pts.len();
    let foes = enemies.as_ref().map_or(&[][..], |e| &e.0[..]);
    for g in foes {
        let fwd = (w.translation - g.hook).with_y(0.0).try_normalize().unwrap_or(Vec3::Z);
        pts.push(crate::map::GrapplePoint { hook: g.hook, land: g.land, fwd, variation: 0 });
    }
    let (_, rot, pos) = cam.to_scale_rotation_translation();
    aim.target = crate::map::grapple_target(pos, rot * Vec3::NEG_Z, w.translation, prev, &pts);
    aim.enemy = None;
    if let Some(t) = aim.target.as_mut().filter(|t| t.index >= n) {
        let g = &foes[t.index - n];
        // An object's point (the Divine Dragon's trees, o259920-922): ridden like a map point
        // (WireVariationParam 45 / 0: targetMarkerType 0), its row's arc, nothing hit.
        if g.object {
            t.point.variation = g.variation;
            return;
        }
        // An enemy is not ridden: the arc ends by him and Wolf falls on.
        t.ride = false;
        t.point.variation = g.variation;
        aim.enemy = Some((g.entity, g.homing_row));
    }
}

/// Wolf's hook stuck in the enemy (the move start: TAE 901 keeps the rope on the hook) ->
/// enemy::grapple::WireHit (its SP_DAMAGE_WIRE reaction).
fn hook_enemy(mut wolf: Query<(&mut Player, &Actor)>, mut hits: MessageWriter<crate::enemy::grapple::WireHit>) {
    for (mut p, a) in &mut wolf {
        let Some(w) = p.wire.as_mut() else { continue };
        if let (Some(e), false) = (w.enemy, w.hit_sent) {
            if a.state.contains("WireMoveStart") {
                hits.write(crate::enemy::grapple::WireHit(e));
                w.hit_sent = true;
            }
        }
    }
}

/// HKS GetWireMoveStartIndex: env WireDramaticAngle > 50 deg vertical, > 15 upslope, else
/// horizontal. gap: the engine's WireDramaticAngle is not traced; taken as the elevation of the
/// hook seen from Wolf's centre.
pub fn start_index(wolf: Vec3, hook: Vec3) -> usize {
    let to = hook - wolf;
    let angle = to.y.atan2(to.x.hypot(to.z)).to_degrees();
    if angle > 50.0 {
        1
    } else if angle > 15.0 {
        0
    } else {
        2
    }
}

/// The move start after a shoot: the behavior's selectors (bound to WireMoveStartIndex) list
/// [0] _UpperSlope, [1] _Up, [2] _Vertical (c0000.hkx GroundWireMoveStart_Selector /
/// AirWireMoveStart Selector); the wire-to-wire shoots have one each.
pub fn move_start(shoot: &str, index: usize) -> String {
    let sel = ["_UpperSlope", "_Up", "_Vertical"][index.min(2)];
    match shoot {
        "GroundWireShoot" => format!("GroundWireMoveStart{sel}"),
        "AirWireShoot" => format!("AirWireMoveStart{sel}"),
        "AirBreakFallWireShoot" => "AirWireMoveStartFromBreakFall".to_string(),
        "WireMoveWireShoot" => "WireMoveWireMoveStart".to_string(),
        _ => "WireMoveWire2MoveStart".to_string(),
    }
}

pub fn is_shoot(state: &str) -> bool {
    matches!(state, "GroundWireShoot" | "AirWireShoot" | "AirBreakFallWireShoot" | "WireMoveWireShoot" | "WireMoveWire2Shoot")
}

/// The wire states (STYLE_TYPE_WIRE_FALL once launched).
pub fn is_wire_state(state: &str) -> bool {
    state == "WireThrow"
        || is_shoot(state) || state.contains("WireMoveStart") || state.starts_with("WireMove") || state.starts_with("WireFall")
}

/// The shoot to fire from `state`: out of a wire move the wire-to-wire ones (HKS
/// BEH_A_AIR_WIRE_SHOOT: W_WireMoveWire2Shoot from WireMoveWireMoveStart, else
/// W_WireMoveWireShoot in the wire-fall style), in the air AirWireShoot, else GroundWireShoot.
pub fn shoot_state(state: &str, airborne: bool, in_wire: bool) -> &'static str {
    if state == "WireMoveWireMoveStart" {
        "WireMoveWire2Shoot"
    } else if in_wire && airborne {
        "WireMoveWireShoot"
    } else if airborne {
        "AirWireShoot"
    } else {
        "GroundWireShoot"
    }
}

impl Wire {
    pub fn new(t: &GrappleTarget, enemy: Option<(Entity, i64)>, wolf: Vec3) -> Self {
        Wire {
            hook: t.point.hook,
            land: t.point.land,
            ride: t.ride,
            homing: enemy.map_or(WIRE_HOMING + homing_offset(t.point.variation), |(_, row)| row),
            index: start_index(wolf, t.point.hook),
            dest: t.point.land + Vec3::Y * CAPSULE_HALF_HEIGHT,
            flight: None,
            enemy: enemy.map(|(e, _)| e),
            hit_sent: false,
        }
    }

    /// An enemy target moves on while the shoot and the flight play (Gyoubu's drifts a000_003032
    /// / 003033 cover 7-10 m in a second, the Ape's 003018): the hook and the landing follow the
    /// live dummies (enemy::grapple::EnemyGrapples, refreshed each frame) instead of the shoot's
    /// snapshot, and an arc in flight re-aims at the new landing (the exe keeps the rope tip on
    /// the hook, TAE 901, and the homing row's guaranteed arrival steers at the target).
    pub fn track(&mut self, g: &crate::enemy::grapple::EnemyGrapple, pos: Vec3) {
        self.hook = g.hook;
        self.land = g.land;
        if self.flight.is_some() {
            self.dest = self.dest_for(pos);
        }
    }

    /// The arc's destination from the current hook / landing (see `launch`).
    fn dest_for(&self, pos: Vec3) -> Vec3 {
        if self.ride {
            self.land + Vec3::Y * CAPSULE_HALF_HEIGHT
        } else {
            let fwd = (self.hook - pos).with_y(0.0).normalize_or_zero();
            let right = Vec3::Y.cross(fwd);
            let off = homing_offset_vec(self.homing);
            let base = if self.enemy.is_some() { self.land } else { self.hook };
            base + right * off.x + Vec3::Y * off.y + fwd * off.z + Vec3::Y * CAPSULE_HALF_HEIGHT * self.enemy.is_some() as u8 as f32
        }
    }

    /// TAE 920 row 1100: solve the arc that arrives at the destination when the anim's camera
    /// slot turns 19 (module doc). Ridden points end on the landing spot; a swing-through point
    /// ends at the hook plus the homing row's targetOffset, an enemy at its landing dummy plus
    /// its row's (the Ogre grabbing 300050200: (0, 1.5, -2)), taken in the travel frame (Z along
    /// Wolf -> hook, Y up). gap: the offset's frame and base are not traced.
    pub fn launch(&mut self, d: &CharData, a: &mut Actor, pos: Vec3) {
        let flight = flight_time(d, &a.anim, a.t);
        self.dest = self.dest_for(pos);
        let g = FALL_TYPES[WIRE_FALL as usize].gravity();
        let to = self.dest - pos;
        a.fall_type = WIRE_FALL;
        a.airborne = true;
        a.vel_y = (to.y - 0.5 * g * flight * flight) / flight;
        a.air_base = to.with_y(0.0) / flight;
        a.move_vel = a.air_base;
        self.flight = Some(flight);
    }

    /// Guaranteed arrival (fallCorrectionGuaranteeArrival): the horizontal velocity re-aimed at
    /// the destination over the time left; fallCorrectionTurn 1 faces the way he flies.
    pub fn steer(&mut self, a: &mut Actor, pos: Vec3, dt: f32) {
        let Some(left) = self.flight else { return };
        let left = left - dt;
        let to = (self.dest - pos).with_y(0.0);
        if left > 1e-3 {
            a.air_base = to / left;
            self.flight = Some(left);
        } else {
            self.flight = None;
            // A ridden point: over the landing spot now, so no more horizontal speed. The arc to
            // a high spot can arrive still rising, and the flight speed carried him past the
            // 0.6 m perch (2026-10-12 bug hunt round 20); he settles onto it as he falls.
            if self.ride {
                a.air_base = Vec3::ZERO;
                a.move_vel = Vec3::ZERO;
            }
        }
        if to.length_squared() > 1e-4 {
            a.yaw = f32::atan2(-to.x, -to.z);
        }
    }

    /// The shoot's turn toward the hook (TAE 224 SetTurnSpeed, `SHOOT_TURN`).
    pub fn turn(&self, a: &mut Actor, pos: Vec3, dt: f32) {
        let Some(&(_, rate, until)) = SHOOT_TURN.iter().find(|(s, _, _)| *s == a.state) else { return };
        if a.t >= until {
            return;
        }
        let to = (self.hook - pos).with_y(0.0);
        if to.length_squared() < 1e-4 {
            return;
        }
        let target = f32::atan2(-to.x, -to.z);
        let diff = (target - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let step = rate.to_radians() * dt;
        a.yaw += diff.clamp(-step, step);
    }
}

/// The flight time of the move start `anim` launched at `t`: to the start of its slot-19 TAE 155.
pub fn flight_time(d: &CharData, anim: &str, t: f32) -> f32 {
    d.anim(anim)
        .and_then(|an| {
            an.events
                .iter()
                .filter(|e| e.kind == 155 && e.arg_i64("LockParamID") == Some(LOCK_SLOT_ARRIVED))
                .map(|e| e.start - t)
                .find(|&s| s > 0.05)
        })
        .unwrap_or(DEFAULT_FLIGHT)
}

/// WireVariationParam chrPhysicsHomingIdOffset of the map and object rows (docs/kb/grapple.md
/// section 2).
fn homing_offset(variation: i64) -> i64 {
    match variation {
        20 | 24 | 40 | 41 => variation,
        // The Divine Dragon's trees' o259920 / 921 (enemy/grapple.rs): 300053105.
        45 => 53105,
        _ => 0,
    }
}

/// ChrPhysicsHomingParam targetOffset (extracted/json/params): 300000020 (0, -2.5, 7.5),
/// 300000024 (0, 3, 2), 300000040 (0, -30, 30), 300000041 (0, -22, 25); 300000000 none.
fn homing_offset_vec(row: i64) -> Vec3 {
    homing_rows().get(&row).copied().unwrap_or(Vec3::ZERO)
}

fn homing_rows() -> &'static std::collections::HashMap<i64, Vec3> {
    static ROWS: std::sync::OnceLock<std::collections::HashMap<i64, Vec3>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/ChrPhysicsHomingParam.json");
        let v: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let f = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        v.get("rows")
            .and_then(|r| r.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        let id = r.get("id")?.as_i64()?;
                        // Game space -> Bevy: X mirrored (map.rs loads the points the same way).
                        Some((id, Vec3::new(-f(r, "targetOffsetX"), f(r, "targetOffsetY"), f(r, "targetOffsetZ"))))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// The rope while TAE 717 WireModelEventUnk runs (the hook arm out): from the hook model's dummy
/// 144 (TAE 715 OverrideWeaponModelLocation, "Grappling Hook") to the tip. In the shoots the tip
/// leaves at TAE 900's start and flies at its 50 m/s (module doc); in the move starts it is on the
/// hook (901). gap: the rope and hook FXR / model are not found yet; a line stands in.
pub(crate) fn draw_rope(
    combat: Res<crate::data::Combat>,
    wolf: Query<(&Player, &Actor, &GlobalTransform, Option<&crate::model::Dummies>)>,
    dummies: Query<&GlobalTransform>,
    mut rope: ResMut<WireRope>,
    mut gizmos: Gizmos,
) {
    rope.ends = None;
    let d = &combat.player;
    for (p, a, g, dm) in &wolf {
        let Some(w) = p.wire else { continue };
        let Some(an) = d.anim(&a.anim) else { continue };
        if !an.events.iter().any(|e| e.kind == 717 && e.in_time(a.t)) {
            continue;
        }
        let hand = dm.and_then(|dm| dm.0.get(&144)).and_then(|&m| dummies.get(m).ok()).map_or(g.translation() + Vec3::Y * 0.4, |h| h.translation());
        let tip = if is_shoot(&a.state) {
            let Some(start) = an.events.iter().find(|e| e.kind == 900).map(|e| e.start) else { continue };
            if a.t < start {
                continue;
            }
            let speed = an.events.iter().find(|e| e.kind == 900).and_then(|e| e.args.get("unk0")).and_then(|v| v.as_f64()).unwrap_or(50.0) as f32;
            let to = w.hook - hand;
            hand + to.normalize_or_zero() * (speed * (a.t - start)).min(to.length())
        } else {
            w.hook
        };
        rope.ends = Some((hand, tip));
        if !rope.model {
            gizmos.line(hand, tip, Color::srgb(0.35, 0.28, 0.2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_start_index_follows_the_hook_elevation() {
        let wolf = Vec3::ZERO;
        assert_eq!(start_index(wolf, Vec3::new(0.0, 1.0, -10.0)), 2);
        assert_eq!(start_index(wolf, Vec3::new(0.0, 5.0, -10.0)), 0);
        assert_eq!(start_index(wolf, Vec3::new(0.0, 15.0, -5.0)), 1);
        assert_eq!(move_start("GroundWireShoot", 1), "GroundWireMoveStart_Up");
        assert_eq!(move_start("AirWireShoot", 2), "AirWireMoveStart_Vertical");
    }
}
