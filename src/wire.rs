//! The Grappling Hook (the engine's "wire"): Wolf's side of docs/kb/grapple.md (sections 3 and 5).
//! The points and the search are map.rs (`crate::traversal::grapple_target`); this is the move: the shoot, the
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
//!   300000000 initialVelocityCorrection 1 (FUN_140bb3580 -> FUN_140bb3930: the least-speed arc
//!   to the landing spot under the Wire fall's braking and gravity, `arc`) and
//!   fallCorrectionGuaranteeArrival (the arc flies on its own for its flight time, then Wolf is
//!   put on the spot: `Wire::steer`). Checked frame by frame against the exe (tools/emu-cases
//!   `wire`, sim test grapple_frames_match_the_exe; docs/kb/grapple.md 2026-11-04).
//! - 905 WireDisableFalling: holds Wolf in the air (AirWireShoot 0-0.5 s).

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::{CharData, FALL_TYPES};
use crate::traversal::{GrapplePoints, GrappleTarget};
use crate::player::{CAPSULE_HALF_HEIGHT, Player};

/// ChrPhysicsHomingParam rows of the wire arc: 300000000 + the point's chrPhysicsHomingIdOffset
/// (WireVariationParam: 20 "can't ride default", 24 "can't ride, castle snake valley", 40 / 41 the
/// kites; every other map row 0).
pub const WIRE_HOMING: i64 = 300_000_000;

/// The Wire fall type (ChrPhysicsVelocityChangeParam 1100 fallType 2).
const WIRE_FALL: u8 = 2;

/// TAE 224 SetTurnSpeed of the shoots: 1080 deg/s over 0-0.2 s (202000 / 202100), 2700 deg/s
/// over 0-0.1 s (the wire-to-wire 202600 / 202700).
const SHOOT_TURN: [(&str, f32, f32); 4] =
    [("GroundWireShoot", 1080.0, 0.2), ("AirWireShoot", 1080.0, 0.2), ("WireMoveWireShoot", 2700.0, 0.1), ("WireMoveWire2Shoot", 2700.0, 0.1)];

/// The point the hook would fire at this frame: `crate::traversal::grapple_target` over the camera view, the
/// map's points and the enemies open to the wire (enemy::grapple::EnemyGrapples).
#[derive(Resource, Default)]
pub struct WireAim {
    pub target: Option<GrappleTarget>,
    /// The target is this enemy, with its arc row (ChrPhysicsHomingParam 3000<variation>).
    pub enemy: Option<(Entity, i64)>,
    /// A host game aims the wire with its own search and marker: it sets `target` itself, and
    /// this crate's search and HUD wire mark stand aside.
    pub host: bool,
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
        // The map's wire points and ledge edges (empty until the host's map fills them).
        app.init_resource::<crate::traversal::GrapplePoints>()
            .init_resource::<crate::traversal::LedgeEdges>()
            .init_resource::<WireAim>()
            .init_resource::<WireRope>()
            .add_systems(FixedUpdate, aim_wire.before(crate::actor::ActorSet::Decide))
            .add_systems(FixedUpdate, hook_enemy.after(crate::actor::ActorSet::Decide).before(crate::actor::ActorSet::Resolve))
            .add_systems(PostUpdate, draw_rope.after(bevy::transform::TransformSystems::Propagate));
    }
}

fn aim_wire(
    points: Res<GrapplePoints>,
    enemies: Option<Res<crate::enemy::grapple::EnemyGrapples>>,
    view: Res<crate::view::View>,
    wolf: Query<&Transform, With<Player>>,
    mut aim: ResMut<WireAim>,
) {
    // Without a camera (the sim tests) or with the host aiming, the aim is left as set.
    let (Some((pos, rot)), Ok(w), false) = (view.eye, wolf.single(), aim.host) else { return };
    let prev = aim.target.map(|t| t.index);
    // The enemies join the map's points: the hook on their wire dummy, recognised from every
    // side (gap: the enemy rows' recognition angles are not read; taken as all round).
    let mut pts = points.0.clone();
    let n = pts.len();
    let foes = enemies.as_ref().map_or(&[][..], |e| &e.0[..]);
    for g in foes {
        let fwd = (w.translation - g.hook).with_y(0.0).try_normalize().unwrap_or(Vec3::Z);
        pts.push(crate::traversal::GrapplePoint { hook: g.hook, land: g.land, fwd, variation: 0 });
    }
    aim.target = crate::traversal::grapple_target(pos, rot * Vec3::NEG_Z, w.translation, prev, &pts);
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
/// horizontal. `wolf` is the capsule centre; the angle is taken from the feet (`dramatic_angle`).
pub fn start_index(wolf: Vec3, hook: Vec3) -> usize {
    let angle = dramatic_angle(wolf, hook);
    if angle > 50.0 {
        1
    } else if angle > 15.0 {
        0
    } else {
        2
    }
}

/// The graph variable WireDramaticAngle the engine writes for the script's GetWireMoveStartIndex
/// (degrees): the hook's elevation seen from Wolf's feet (`wolf` is the capsule centre).
/// tools/emu-cases `wire` (row E7): 5 m and 6.5 m from hook o003600_0024 (19.1 and 15.05 deg from
/// the feet, 9.4 and 7.4 from the centre) the exe plays 202010 (index 0), at 9.7 and 13 m (10.1 and
/// 8.7 deg) 202012 (index 2). gap: the engine's own computation (its reference point exactly) is
/// not traced, only bracketed by these four.
pub fn dramatic_angle(wolf: Vec3, hook: Vec3) -> f32 {
    let to = hook - (wolf - Vec3::Y * CAPSULE_HALF_HEIGHT);
    to.y.atan2(to.x.hypot(to.z)).to_degrees()
}

/// The shoots (the graph picks which: HKS BEH_A_GROUND_WIRE_SHOOT / BEH_A_AIR_WIRE_SHOOT, and
/// their move starts by the selectors on WireMoveStartIndex; hks_wolf takes them).
pub fn is_shoot(state: &str) -> bool {
    matches!(state, "GroundWireShoot" | "AirWireShoot" | "AirBreakFallWireShoot" | "WireMoveWireShoot" | "WireMoveWire2Shoot")
}

/// The wire states (STYLE_TYPE_WIRE_FALL once launched).
pub fn is_wire_state(state: &str) -> bool {
    state == "WireThrow"
        || is_shoot(state) || state.contains("WireMoveStart") || state.starts_with("WireMove") || state.starts_with("WireFall")
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

    /// TAE 920 row 1100 (homing 300000000, initialVelocityCorrection 1): the launch velocity of
    /// the exe's least-speed arc to the destination under the Wire fall's horizontal braking and
    /// gravity ([`arc`]), and its flight time for the guaranteed arrival (`steer`). Ridden points
    /// end on the landing spot; a swing-through point ends at the hook plus the homing row's
    /// targetOffset, an enemy at its landing dummy plus its row's (the Ogre grabbing 300050200:
    /// (0, 1.5, -2)), taken in the travel frame (Z along Wolf -> hook, Y up). gap: the offset's
    /// frame and base are not traced (the emulator cases are all ridden map points).
    pub fn launch(&mut self, _d: &CharData, a: &mut Actor, pos: Vec3) {
        self.dest = self.dest_for(pos);
        let ft = FALL_TYPES[WIRE_FALL as usize];
        let to = self.dest - pos;
        let flat = to.with_y(0.0);
        let (flight, vh, vy) = arc(flat.length(), to.y, ft.h_accel, ft.gravity());
        a.fall_type = WIRE_FALL;
        a.airborne = true;
        a.vel_y = vy;
        a.air_base = flat.normalize_or_zero() * vh;
        a.move_vel = a.air_base;
        self.flight = Some(flight);
    }

    /// Guaranteed arrival (fallCorrectionGuaranteeArrival): the arc flies on its own (the Wire
    /// fall's braking and gravity, no re-aim: tools/emu-cases `wire`, the direction holds to
    /// 1e-3) while the flight time runs, counted from the launch's own step, to the nearest step:
    /// on the first step that starts under half a step from the end the exe puts Wolf on the
    /// destination and stops his horizontal speed, the vertical one going on (h24: T 0.954 s =
    /// 57.2 steps, put there on the 58th; h24_d13: 1.099 s = 65.97 steps, on the 67th).
    /// Returns where he is put that step (his centre). fallCorrectionTurn 1 faces the way he
    /// flies. gap: a swing-through or enemy arc's end is not in the cases; it keeps its speed and
    /// is not put anywhere.
    pub fn steer(&mut self, a: &mut Actor, pos: Vec3, dt: f32) -> Option<Vec3> {
        let left = self.flight? - dt;
        let to = (self.dest - pos).with_y(0.0);
        if to.length_squared() > 1e-4 && a.air_base.length_squared() > 1e-6 {
            a.yaw = f32::atan2(-a.air_base.x, -a.air_base.z);
        }
        if left > -0.5 * dt {
            self.flight = Some(left);
            return None;
        }
        self.flight = None;
        if !self.ride {
            return None;
        }
        a.air_base = Vec3::ZERO;
        a.move_vel = Vec3::ZERO;
        Some(self.dest)
    }

    /// The shoot's turn (TAE 224 SetTurnSpeed, `SHOOT_TURN`): toward the landing spot of a ridden
    /// point, else the hook. tools/emu-cases `wire` (row E7): from 5-13 m the exe ends the turn
    /// facing o003600_0024's landing spot to 0.001 rad, 0.005 rad off the hook's direction.
    /// gap: no swing-through or enemy case was run, so those keep the hook.
    pub fn turn(&self, a: &mut Actor, pos: Vec3, dt: f32) {
        let Some(&(_, rate, until)) = SHOOT_TURN.iter().find(|(s, _, _)| *s == a.state) else { return };
        if a.t >= until {
            return;
        }
        let to = (if self.ride && self.enemy.is_none() { self.land } else { self.hook } - pos).with_y(0.0);
        if to.length_squared() < 1e-4 {
            return;
        }
        let target = f32::atan2(-to.x, -to.z);
        let diff = (target - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let step = rate.to_radians() * dt;
        a.yaw += diff.clamp(-step, step);
    }
}

/// The wire arc (exe FUN_140bb3930, from FUN_140bb3580's initialVelocityCorrection 1): to a point
/// `dist` m away on the flat and `dy` m up, with the fall type's horizontal acceleration `h_acc`
/// (negative: braking along the flight) and gravity `g` (negative). The flight time is the one of
/// the least launch speed, T = (4 |d|^2 / |acc|^2)^(1/4) with d = (dist, dy) and acc = (h_acc, g);
/// the vertical speed dy / T - g T / 2; the speed squared |acc| |d| - acc . d, of which the
/// horizontal part is what the vertical leaves. Returns (T, horizontal speed, vertical speed).
/// gap: the exe then keeps the solution only within the fall module's speed caps (+0x1c8 / +0x1cc),
/// not read.
pub fn arc(dist: f32, dy: f32, h_acc: f32, g: f32) -> (f32, f32, f32) {
    let d2 = dist * dist + dy * dy;
    let a2 = h_acc * h_acc + g * g;
    if d2 <= 0.0 || a2 <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let t = (4.0 * d2 / a2).sqrt().sqrt();
    let vy = dy / t - g * t * 0.5;
    let v2 = (a2 * d2).sqrt() - (g * dy + dist * h_acc);
    (t, (v2 - vy * vy).max(0.0).sqrt(), vy)
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

/// A ChrPhysicsHomingParam row's correction settings (extracted/json/params): the jump arts'
/// 20000 = initialVelocityCorrection 2, fallCorrectionChase 1, chase acceleration 10 m/s^2,
/// max velocity rate 1, targetOffset (0, 0, -1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HomingRow {
    /// targetOffset in Bevy axes (X mirrored, as `homing_rows`).
    pub offset: Vec3,
    pub initial_correction: u8,
    pub chase: bool,
    pub chase_accel: f32,
    pub chase_speed_rate: f32,
}

pub fn homing_row(id: i64) -> Option<HomingRow> {
    static ROWS: std::sync::OnceLock<std::collections::HashMap<i64, HomingRow>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/ChrPhysicsHomingParam.json");
        let v: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let f = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let b = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u8;
        v.get("rows")
            .and_then(|r| r.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        let row = HomingRow {
                            offset: Vec3::new(-f(r, "targetOffsetX"), f(r, "targetOffsetY"), f(r, "targetOffsetZ")),
                            initial_correction: b(r, "initialVelocityCorrection"),
                            chase: b(r, "fallCorrectionChase") != 0,
                            chase_accel: f(r, "fallCorrectionChaseMaxAcceleration"),
                            chase_speed_rate: f(r, "fallCorrectionChaseMaxVelocityRate"),
                        };
                        Some((r.get("id")?.as_i64()?, row))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
    .get(&id)
    .copied()
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
        if !an.events.iter().any(|e| e.kind == 717 && e.dispatched(a.step())) {
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
        let wolf = Vec3::Y * CAPSULE_HALF_HEIGHT;
        assert_eq!(start_index(wolf, Vec3::new(0.0, 1.0, -10.0)), 2);
        assert_eq!(start_index(wolf, Vec3::new(0.0, 5.0, -10.0)), 0);
        assert_eq!(start_index(wolf, Vec3::new(0.0, 15.0, -5.0)), 1);
    }

    /// The exe's h24 launch (tools/emu-cases `wire`): 9.94 m on the flat, 1.19 m up, braking 10 and
    /// gravity 19.6 -> 0.954 s, 15.19 m/s across, 10.60 m/s up (its first step leaves 15.02 /
    /// 10.27).
    #[test]
    fn the_arc_is_the_exes_least_speed_launch() {
        let (t, vh, vy) = arc(9.941, 1.191, -10.0, -19.6);
        assert!((t - 0.954).abs() < 1e-3 && (vh - 15.19).abs() < 0.01 && (vy - 10.598).abs() < 0.01, "{t} {vh} {vy}");
    }
}
