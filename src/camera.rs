//! Third-person camera. It orbits the player with the mouse, follows the
//! player, and turns toward the lock-on target (Q or middle mouse).
//!
//! Click in the window to capture the mouse; press Esc to release it.
//! In another game (`embed` without `host`) it takes over that game's camera instead of spawning
//! one, keeping its lights, fog and post.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::actor::Actor;
use crate::config::GameConfig;
use crate::data::Combat;

/// LockCamParam row 0 (the default player camera).
struct LockCam {
    dist: f32,
    focus_y: f32,
    fov_y: f32,
    min_pitch: f32,
    lock_range: f32,
}

fn lock_cam(combat: &Combat) -> LockCam {
    let r = combat.param("LockCamParam", 0);
    let f = |k: &str, d: f64| r[k].as_f64().unwrap_or(d) as f32;
    LockCam {
        dist: f("camDistTarget", 4.5),
        focus_y: f("chrOrgOffset_Y", 1.5),
        fov_y: f("camFovY", 43.0),
        // rotRangeMinX is the camera's lowest look angle; our pitch sign is the opposite.
        min_pitch: f("rotRangeMinX", -40.0),
        lock_range: f("chrLockRangeMaxRadius", 30.0),
    }
}
use crate::enemy::Enemy;
use crate::player::Player;

/// Holds the camera's orbit angles (radians) and the smoothed point it looks at.
#[derive(Component)]
pub struct OrbitCamera {
    /// Left/right angle around the player.
    pub yaw: f32,
    /// Up/down angle. Positive = camera above the player, looking down.
    pub pitch: f32,
    focus: Vec3,
    /// TAE 155 / 153 camera state.
    pub cam: CamState,
}

/// ChrCam distance and param-slot state (exe FUN_14073c260 / FUN_1407374e0).
#[derive(Clone, Debug)]
pub struct CamState {
    /// TAE 155 SetLockParamID slot (1 = default; CameraSetParam manager DAT_143d59938 +0x3ec),
    /// the blend clock +0x3f4 and its length +0x3f0 (CameraSetParam beginTime of the new slot,
    /// endTime of the old one when going back to 1), and +0x1e8: the blended CameraParam as it
    /// was at the switch, which the blend starts from (`cam_param`).
    pub slot: i64,
    pub(crate) blend_t: f32,
    pub(crate) blend_len: f32,
    pub(crate) cam_from: std::sync::Arc<std::collections::HashMap<String, f32>>,
    /// +0x218 the distance (+0x214, the one used, is set to it each frame), +0x21c the distance
    /// a SlowStart override eases from, singleton +0x80 EndInterpolationSpeed and +0x7c the
    /// time it still holds (TransitionKeyframe, counted down after the event).
    pub(crate) dist_raw: f32,
    pub dist: f32,
    pub(crate) dist_from: f32,
    pub(crate) end_rate: f32,
    pub(crate) end_time: f32,
    /// The 153 event seen last frame (anim key, start), to spot its first frame.
    last_153: Option<(String, f32)>,
    /// TAE 156 SetCameraAngle: the event running (anim key, start), the (yaw, pitch) at its
    /// first frame (exe ChrCam +0x1f4 / +0x1f0) and Wolf's yaw then (its target +0x1e0 is
    /// taken once, at the first frame).
    angle_from: Option<((String, f32), (f32, f32), f32)>,
    /// TAE 151 after the event: the focus chases Wolf at singleton +0x48 (the event's unk3)
    /// while +0x44 (unk2, seconds) counts down (FUN_140838b50): (time left, rate).
    pub(crate) look_after: (f32, f32),
    /// TAE 152 AttachCameraToNode: singleton +0x68 (unk1, seconds, counted down by
    /// FUN_140838b50 after each camera frame) and +0x6c (unk2, the distance chase rate while it
    /// is set; cleared with the clock at 0). Set anew every frame the event runs.
    pub(crate) attach_after: (f32, f32),
    /// TAE 151: camera +0x1a0, the cast from Wolf's focus to the dummy hit something this
    /// event, so the focus stays on Wolf at the normal rates and the look limits keep their
    /// defaults; +0x240 the yaw (exe convention) the limits' centre is unwrapped near, taken
    /// from the camera at the event's first frame (None between events).
    pub(crate) look_blocked: bool,
    pub(crate) look_ref: Option<f32>,
    /// +0x50 FOV (radians) and +0x204 focus height, lerped to the LockCamParam row.
    fov: f32,
    focus_y: f32,
    /// The look limits (exe camera +0x230..+0x23c, FUN_140742230 `look_limits_step`):
    /// [pitch_min, pitch_max, yaw_min, yaw_max] radians, the yaw ones in the exe's convention
    /// (`exe_yaw`), chasing TAE 151's or the param defaults at CameraParam.lockCamParamLerpRate.
    /// `None` until the first frame (then the defaults).
    look: Option<[f32; 4]>,
    /// +0x100 the camera position chasing the orbit's point (`chase_position`), before
    /// collision; `None` until the first frame.
    pos: Option<Vec3>,
    /// +0x110 / +0x1e4: the camera after last frame's collision cast and the hit-wall clock.
    wall: WallCast,
    /// +0x150 / +0x154: camera control snaps the position chase (rate 1), held for
    /// rotCtrlEndWaitTime and released over rotCtrlEndReleaseTime.
    ctrl_snap: f32,
    ctrl_hold: f32,
    /// The orbit angles follow_player left last frame: a change before the next one is the
    /// mouse (or a script) turning the camera.
    seen: Option<(f32, f32)>,
    /// +0xe0 the fulcrum (`drag_fulcrum` / `place_fulcrum`) and the focus it was dragged from
    /// last frame (+0x90 saved at the update's start); `None` until placed.
    fulcrum: Option<(Vec3, Vec3)>,
    /// +0x194 / +0x198: how fully the fulcrum follows (1 = all of the move, no swing) and the
    /// wait before it falls (transChaseByControlEnd*).
    trans_snap: f32,
    trans_hold: f32,
    /// +0x1b0..+0x1dc, +0x410: the focus follow (`FocusChase`, on Wolf's position before
    /// the focus height +0x204 is added); `None` until the first frame.
    focus_chase: Option<FocusChase>,
    /// The camera closer than playerFadeDist to Wolf this frame (`cam_fade_step`).
    near: bool,
    /// Wolf's CamFade alpha (`cam_fade_step`), 1 = drawn as usual.
    alpha: f32,
    /// The lock point the locked camera frames (+0x280, `LockPointAverage`).
    lock_point: LockPointAverage,
    /// +0x220 / +0x224 / +0x228: the working rates of the focus follow (X / Z, Y) and of the
    /// position chase (X / Z), eased toward the CameraParam row by dt each frame or set to the
    /// row's _ForEscapeWall values by an escape-wall wing hit (`working_rates_step`). `None`
    /// until the first frame (then the row's).
    rates: Option<Vec3>,
}

/// The CamFade rates (exe FUN_140b216b0 defaults of the debug floats
/// Game.Camera.Obstacle.AlphaDecreaseRate / AlphaIncreaseRate) and the player's alpha floor
/// (camera +0x40c "CamFade Player Model Target Alpha", 0 at FUN_14073c260's init).
const CAM_FADE_DOWN: f32 = 2.5;
const CAM_FADE_UP: f32 = 5.0;
const CAM_FADE_PLAYER_MIN: f32 = 0.0;

/// Wolf's fade when the camera comes close (exe CSCamFadeSlot, FUN_140b21ca0 / FUN_140b22000):
/// while the camera is asked to fade him (`near`, each frame) his alpha falls at 2.5 / s to the
/// floor; once not, it climbs back at 5 / s to 1 (ExtensionOfTime 0: no wait).
pub(crate) fn cam_fade_step(alpha: f32, near: bool, dt: f32) -> f32 {
    if near { (alpha - CAM_FADE_DOWN * dt).max(CAM_FADE_PLAYER_MIN) } else { (alpha + CAM_FADE_UP * dt).min(1.0) }
}

/// The camera's safe margin (exe FUN_14073c260 at the end, under Game.IsUseSPChrExFollowCam,
/// default 1 by FUN_14097b330): the final camera is pulled toward the focus by
/// min((d - 1e-5) * camSafeMarginRate, camSafeMarginMax) (0.1, 0.3 m in every CameraParam row).
pub(crate) fn safe_margin(dist: f32, rate: f32, max: f32) -> f32 {
    if dist > 1e-5 { dist - ((dist - 1e-5) * rate).min(max) } else { dist }
}

/// The wall state of the ChrCam update (FUN_14073c260).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WallCast {
    /// +0x110: last frame's camera after the collision cast, before the safe margin; the
    /// distance chase starts from it (`chase_position_from`). `None` until the first frame.
    pub post: Option<Vec3>,
    /// +0x1e4: time left on the hit-wall distance rates, set to targetChaseDistRateTime by
    /// every hit of the final cast (`hit_wall_rates`).
    pub hit_time: f32,
}

/// The shortest cut of the fulcrum cast (FUN_14073c260 14073.c 11040-11060, with
/// Game.IsUseSPChrExFollowCam on): a hit closer than 1 cm keeps the fulcrum 1 cm out.
pub(crate) const FULCRUM_CUT_MIN: f32 = 0.01;
/// The shortest cut of the orbit point and final casts (14073.c 11230-11250, 12350-12361).
pub(crate) const CAST_CUT_MIN: f32 = 1e-5;

/// A cast's end pulled in to its hit (FUN_14073c260 after FUN_14094dfc0 / FUN_14094dc70): the
/// cast from `start` along `v` hit at `frac` of `v`, so the end moves to
/// start + v * max(frac * |v|, min) / |v|; no hit leaves start + v. The exe's zero-length
/// case (the final cast puts the camera 1e-5 m out along the orbit point) is left out.
pub(crate) fn cast_cut(start: Vec3, v: Vec3, frac: Option<f32>, min: f32) -> Vec3 {
    let len = v.length();
    match frac {
        Some(t) if len >= 1e-5 => start + v * ((t * len).max(min) / len),
        _ => start + v,
    }
}

/// The working rates +0x220 / +0x224 / +0x228 (FUN_14073c260, 14073.c 11751-11766): (focus
/// follow X / Z, focus follow Y, position chase X / Z). An escape-wall wing hit this frame sets
/// them to the row's chrTransChaseRateXZ_ForEscapeWall / chrTransChaseRateY_ForEscapeWall /
/// targetChaseRateXZForEscapeWall (`escape`); otherwise each moves (row - rate) * dt toward the
/// row's _ForNormal values (`row`), so after a TAE 155 slot switch or a wing hit they ease back
/// at 1 / s. The focus follow reads them before this step (last frame's), the position chase
/// after.
pub(crate) fn working_rates_step(rates: Vec3, row: Vec3, escape: Option<Vec3>, dt: f32) -> Vec3 {
    match escape {
        Some(e) => e,
        None => rates + (row - rates) * dt,
    }
}

/// The escape-wall wings (FUN_14073c260, 14073.c 11339-11440, the option on): two sphere casts
/// (escapeWallWingRadius) of escapeWallWingLen along the arm from Wolf's point `wolf` (+0xa0)
/// toward the orbit point `orbit` (+0xf0), turned by -/+ escapeWallWingAngY about Y, each
/// starting 1 m back along the arm and 1 / cos(angle) m out along its own direction, so
/// tan(angle) to the side (fitted on the emulated casts: 1.7e-4 m with every row's 0.01 deg;
/// gap: the divisor FUN_1423867b0 gives that 1 m is not traced). With the rows' 1 cm wings at
/// Wolf's centre they never hit a wall the proxy keeps him off (none hit in 1,000 emulated
/// frames). Returns (start, cast vector) per wing.
pub(crate) fn escape_wings(wolf: Vec3, orbit: Vec3, ang: f32, len: f32) -> [(Vec3, Vec3); 2] {
    let dn = (orbit - wolf).normalize_or_zero();
    [-ang, ang].map(|a| {
        let r = Quat::from_rotation_y(a) * dn;
        (wolf - dn + r / a.cos(), r * len)
    })
}

/// The focus pull-in of the ChrCam update (FUN_14073c260, 14073.c 9563-9664), all points lifted
/// by the focus offset (+0x200): a ray (FUN_14094cf90) from last frame's focus `old` toward this
/// frame's chased focus `new`, |new - old| plus half Wolf's radius (FUN_1409efb80) long; when it
/// hits something, a sphere cast of Wolf's radius (FUN_14094dc70) from Wolf's root `root`
/// toward `old` puts the focus where that sphere stops (at least 1e-5 m out); a ray hit with no
/// sphere hit leaves `new`. So a focus that would follow Wolf through a wall (a teleport, a
/// warp) is kept on his side. This frame's chase output +0x1d0 keeps the unpulled point, but the
/// next frame's chase starts in X / Z from the pulled focus (+0x90 less the offset; velocity and
/// last target as they were: emulated, 8 frames after pull-ins exact, the Y not exercised), so
/// `follow_player` writes the pulled X / Z back into the chase. `ray(start, v)` is the ray's
/// hit, `cast(start, v, radius)` the sphere's fraction of `v`.
pub(crate) fn focus_pull_in(old: Vec3, new: Vec3, root: Vec3, radius: f32, ray: impl Fn(Vec3, Vec3) -> bool, cast: impl Fn(Vec3, Vec3, f32) -> Option<f32>) -> Vec3 {
    let d = new - old;
    let len = d.length();
    if len <= 0.0 || !ray(old, d * ((radius * 0.5 + len) / len)) {
        return new;
    }
    let v = old - root;
    let l = v.length();
    if l <= 0.0 {
        return new;
    }
    match cast(root, v, radius) {
        Some(t) => root + v * ((t * l).max(CAST_CUT_MIN) / l),
        None => new,
    }
}

/// The distance chase rates of the position chase (FUN_14073c260 14073.c 11948-11979): for
/// targetChaseDistRateTime `time` after a hit of the final cast (`hit_time` = +0x1e4) the X / Z
/// and Y rates (+0x1e8 / +0x1ec) are targetChaseDistRateXZ / Y_ForHitWall (`hit`, 1.0) for its
/// first half, then fall linearly to the normal targetChaseDistRateXZ / Y (`normal`, 0.4):
/// t = clamp(2 hit_time / time). The clock then counts down by dt. Returns the rates and the
/// clock; so the arm grows back at once after a wall for a second, then eases.
pub(crate) fn hit_wall_rates(hit_time: f32, time: f32, normal: Vec2, hit: Vec2, dt: f32) -> (Vec2, f32) {
    if hit_time <= 0.0 || time <= 0.0 {
        return (normal, 0.0);
    }
    let t = (2.0 * hit_time / time).clamp(0.0, 1.0);
    (hit * t + normal * (1.0 - t), (hit_time - dt).max(0.0))
}

/// RSekiro's stand-in for the exe's camera sphere cast (FUN_14094dfc0 / FUN_14094dc70: a Havok
/// linear cast of a `radius` sphere from `start` along `v`, returning the fraction of `v` where
/// it first touches): a ray over |v| + radius on the map's collision, stopped short by the
/// radius. A touch within 1 mm past the end counts (the exe's casts report the camera resting
/// on a wall as a hit at fraction 1, which refreshes the hit-wall clock). With no map loaded
/// the arena floor y = 0 stands in.
fn map_cast(start: Vec3, v: Vec3, radius: f32) -> Option<f32> {
    let len = v.length();
    if len < 1e-5 {
        return None;
    }
    let dir = v / len;
    let hit = match crate::terrain::camera_raycast(start, dir, len + radius + 1e-3) {
        Some(h) => h - radius,
        None if dir.y < -1e-3 && crate::terrain::terrain().is_none() => (start.y - radius) / -dir.y,
        None => return None,
    };
    (hit <= len + 1e-3).then(|| (hit / len).clamp(0.0, 1.0))
}

/// CameraParam fulcrum values (`drag_fulcrum` / `place_fulcrum`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FulcrumParams {
    /// targetTransChaseRateXZ / Y (0x60 / 0x64; row 0: 0.4).
    pub rate_xz: f32,
    pub rate_y: f32,
    /// fulcrumDistRateByLrMoveMaxDist / Min / Max (0x40-0x48: 0.3 m, 0.1, 0.99), fulcrumDistMin
    /// (0x50: 2 m).
    pub lr_max_dist: f32,
    pub lr_min: f32,
    pub lr_max: f32,
    pub dist_min: f32,
}

/// The fulcrum's drag (FUN_14073c260 ~0x14073d3xx, 14073.c 9752-9770): the fulcrum (+0xe0)
/// moves by targetTransChaseRate of the focus's move this frame, lifted toward all of it by the
/// control snap (+0x194): r = rate + (1 - rate) * snap. The free camera's yaw is then read back
/// from the fulcrum -> focus direction (~0x14073e015, FUN_140734070), so a sideways run, which
/// leaves the fulcrum behind, swings the camera round toward the run.
pub(crate) fn drag_fulcrum(fulcrum: Vec3, moved: Vec3, snap: f32, p: FulcrumParams) -> Vec3 {
    let (xz, y) = (p.rate_xz + (1.0 - p.rate_xz) * snap, p.rate_y + (1.0 - p.rate_y) * snap);
    fulcrum + Vec3::new(moved.x * xz, moved.y * y, moved.z * xz)
}

/// Whether the free camera's pitch is read back from the fulcrum this frame (14073.c 9840-9880):
/// CameraParam isEnableChrChaseRotX, and then always when chrChaseRotXInvalidAngle (80 deg) is 0,
/// the stick is let go, or the stick doesn't point at the camera: the exe tests the manipulator's
/// +0xc0 (the move stick turned into the camera's yaw, unnormalised, PadManipulator
/// FUN_1407f0f50) against the camera's backward axis, dot < cos(angle). So running toward the
/// camera keeps the pitch. gap: the side of the camera axis (+0x30 row taken as forward, so its
/// negation is toward the camera) is inferred, not tested in game.
pub(crate) fn chases_pitch(enabled: bool, invalid_angle: f32, stick: Vec2) -> bool {
    let stick = stick.clamp_length_max(1.0);
    enabled && (invalid_angle <= 0.0 || stick.length_squared() <= 1.192_092_9e-7 || -stick.y < invalid_angle.cos())
}

/// The fulcrum put back on the camera's arm (14073.c 10923 / 10930-10990): along `arm` (focus ->
/// camera, from this frame's angles) at a distance of (1 - k) times the camera's last distance,
/// at least fulcrumDistMin; k = the focus's sideways move this frame (across the camera's right
/// axis) / fulcrumDistRateByLrMoveMaxDist, clamped to Min..Max. The exe takes that move per
/// frame with no dt (`per_exe_frame`: at 30 fps a frame's whole move).
pub(crate) fn place_fulcrum(focus: Vec3, arm: Vec3, cam_dist: f32, sideways: f32, dt: f32, p: FulcrumParams) -> Vec3 {
    let k = (per_exe_frame(sideways, dt) / p.lr_max_dist.max(1e-3)).clamp(p.lr_min, p.lr_max);
    focus + arm * ((1.0 - k) * cam_dist).max(p.dist_min)
}

/// CameraParam rates of the camera position chase (`chase_position`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PosChase {
    pub xz: f32,
    pub y: f32,
    pub ang_min: f32,
    pub ang_max: f32,
    pub dist_xz: f32,
    pub dist_y: f32,
}

/// The camera position chase of the ChrCam update (FUN_14073c260 at 0x140740764-0x140740d40):
/// - the position `prev` (+0x100) moves toward the orbit's point `raw` (+0xf0) by `xz` on X / Z
///   and `y` on Y each frame: targetChaseRateAtLockXZ / Y (0.4 / 0.3) locked on, else the
///   working rate +0x228 (eased to targetChaseRateXZForNormal 0.1) and targetChaseRateY (0.4);
/// - the X / Z rate scales by the angle between raw and prev seen from the focus (cosines of
///   targetChaseRateAng(AtLock)Min / Max, cos via FUN_1419dcb40): 0 inside Min, rising to 1 at
///   Max (both 0 at lock: always 1);
/// - camera control lifts both toward 1: r + (1 - r) * snap (+0x150);
/// - then the distance to the focus chases separately on X / Z and Y from the last frame's one
///   to raw's at targetChaseDistRateXZ / Y (0.4): the X / Z and Y parts of (pos - focus) scale
///   by distXZ / |pos - focus| and distY / |pos - focus|.
///
/// - with `pitch` = the pitch limits (+0x230 / +0x234, the elevation of camera - focus, as
///   orbit pitch): when both raw and prev sit above the top limit (prev past it, raw past it
///   less 0.1 deg) or both below the bottom one (prev under it, raw under it plus 0.1 deg), the
///   X / Z rate is 1 (14073.c 11883-11898, pitch FUN_140733e50 = -atan2(y, xz)), so a camera
///   outside its pitch range goes straight back.
///
/// The exe applies the rates once a frame with no time factor (`per_frame`: at 30 fps 0.4 a
/// frame, as the exe; checked on the emulated exe at 1/30 s). TAE 152's FUN_140831190 /
/// FUN_1408311b0 override is `chase_position_attached`. This form starts
/// the distance chase from |prev - focus|; the follow camera uses `chase_position_from`.
pub(crate) fn chase_position(prev: Vec3, raw: Vec3, focus: Vec3, r: PosChase, pitch: Option<(f32, f32)>, snap: f32, dt: f32) -> Vec3 {
    chase_position_from(prev, (prev - focus).length(), raw, focus, r, pitch, snap, dt)
}

/// `chase_position` with the distance chase starting from `last_dist`: the exe's is the last
/// camera after the collision cast (|+0x110 - focus|, 14073.c 11992-12030), so after a wall
/// cut the arm grows back from the cut distance, at the `hit_wall_rates` (in `r.dist_xz` /
/// `r.dist_y`). Checked on the emulated exe at walls and the ground (sim test
/// `follow_camera_at_walls_matches_the_exe`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn chase_position_from(prev: Vec3, last_dist: f32, raw: Vec3, focus: Vec3, r: PosChase, pitch: Option<(f32, f32)>, snap: f32, dt: f32) -> Vec3 {
    chase_position_by(prev, last_dist, raw, focus, r, pitch, snap, |rate| per_frame(rate, dt))
}

/// `chase_position_from` with the share of a rate taken this frame given by `step` (the exe:
/// the rate itself, `per_frame`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn chase_position_by(prev: Vec3, last_dist: f32, raw: Vec3, focus: Vec3, r: PosChase, pitch: Option<(f32, f32)>, snap: f32, step: impl Fn(f32) -> f32) -> Vec3 {
    let (to_raw, to_prev) = (raw - focus, prev - focus);
    // The exe's f32 rounding (divide by sqrt((x² + z²) + y²), dot ((x + y) + z)): with the lock's
    // AngAtLockMin 0 (cos 1.0) the X / Z chase stops on frames where nearly parallel vectors
    // round above 1.0 (the camera then rests up to ~0.4 mm off; 2026-11-03 lockcam).
    let unit = |v: Vec3| v / (v.x * v.x + v.z * v.z + v.y * v.y).sqrt();
    let cos = if to_raw.length() <= 1e-5 || to_prev.length() <= 1e-5 {
        1.0
    } else {
        let (a, b) = (unit(to_raw), unit(to_prev));
        a.x * b.x + a.y * b.y + a.z * b.z
    };
    let (c_min, c_max) = (r.ang_min.cos(), r.ang_max.cos());
    let k = if cos > c_min {
        0.0
    } else if c_max < cos && c_max < c_min {
        (cos - c_min) / (c_max - c_min)
    } else {
        1.0
    };
    let elev = |v: Vec3| f32::atan2(v.y, v.with_y(0.0).length());
    const SLACK: f32 = 0.0017453292;
    let outside = pitch.is_some_and(|(lo, hi)| {
        let (e_raw, e_prev) = (elev(to_raw), elev(to_prev));
        (hi < e_prev && hi - SLACK < e_raw) || (e_prev < lo && e_raw < lo + SLACK)
    });
    let (xz, y) = (if outside { 1.0 } else { r.xz * k }, r.y);
    chase_step(prev, last_dist, raw, focus, (xz, y), (r.dist_xz, r.dist_y), snap, step)
}

/// TAE 152 AttachCameraToNode's position chase (FUN_14073c260 after the angle factor: while
/// singleton +0x60 >= 0 and BlendStrength (+0x64, FUN_1408311b0) > 0, X / Z and Y both chase
/// at BlendStrength, no angle factor and no pitch rule), then lifted by +0x150 as always.
/// Checked on the emulated exe (sim test `camera_leftovers_match_the_exe`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn chase_position_attached(prev: Vec3, last_dist: f32, raw: Vec3, focus: Vec3, blend: f32, dist: (f32, f32), snap: f32, dt: f32) -> Vec3 {
    chase_step(prev, last_dist, raw, focus, (blend, blend), dist, snap, |rate| per_frame(rate, dt))
}

/// TAE 152 AttachCameraToNode's angles and arm (FUN_14073c260, after the fulcrum is placed and
/// cast and before the orbit cast; FUN_140831190 = singleton +0x60 >= 0): the orbit point
/// +0xf0 is the DummyPoly's world point (FUN_1409e6db0 on the camera's chr), yaw / pitch
/// (+0x174 / +0x170) are read from the focus -> node direction, and the fulcrum is put on that
/// line at its own distance (at least 1e-5). So the camera chases the node on Wolf's model,
/// looking at the focus. Returns (yaw, pitch, fulcrum) in RSekiro's angles; the orbit point is
/// `node`. Checked on the emulated exe (sim test `camera_leftovers_match_the_exe`).
pub(crate) fn attach_to_node(focus: Vec3, fulcrum: Vec3, node: Vec3) -> (f32, f32, Vec3) {
    let v = node - focus;
    let (yaw, pitch) = (f32::atan2(v.x, v.z), f32::atan2(v.y, v.with_y(0.0).length()));
    let dir = Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0) * Vec3::Z;
    (yaw, pitch, focus + dir * (fulcrum - focus).length().max(1e-5))
}

/// The position and distance chase once the X / Z and Y rates are known (`chase_position_by`).
#[allow(clippy::too_many_arguments)]
fn chase_step(prev: Vec3, last_dist: f32, raw: Vec3, focus: Vec3, (xz, y): (f32, f32), (dist_xz, dist_y): (f32, f32), snap: f32, step: impl Fn(f32) -> f32) -> Vec3 {
    let to_raw = raw - focus;
    let (xz, y) = (step(xz + (1.0 - xz) * snap), step(y + (1.0 - y) * snap));
    let d = raw - prev;
    let p = prev + Vec3::new(d.x * xz, d.y * y, d.z * xz);
    let (raw_d, prev_d) = (to_raw.length(), last_dist);
    // A distance rate over 1 (TAE 152's unk2, 1.5 / 2.0 in Wolf's anims) is taken as it is:
    // the exe lerps past the target (no clamp). gap: frames shorter than 1/60 s take it whole too.
    let dstep = |r: f32| if r > 1.0 { r } else { step(r) };
    let dist_xz = prev_d + (raw_d - prev_d) * dstep(dist_xz);
    let dist_y = prev_d + (raw_d - prev_d) * dstep(dist_y);
    let v = p - focus;
    let l = v.length();
    if l <= 1e-5 {
        return p;
    }
    focus + Vec3::new(v.x * dist_xz / l, v.y * dist_y / l, v.z * dist_xz / l)
}

/// The focus follow of the ChrCam update: FUN_141155d20 on camera +0x1b0 (velocity), +0x1c0
/// (last frame's target), +0x1d0 (output), with the clock +0x410 (time since the last 1/30 s
/// step, FUN_141155ec0). The focus moves at a velocity (m per 1/30 s) that changes only at the
/// 1/30 s step boundaries, each closing `rate` (CameraParam chrTransChaseRateXZ / Y_ForNormal
/// eased into +0x220 / +0x224) of the gap to where the target is at that step; the target's
/// own velocity this frame is taken as constant across it. So the focus starts slowly and lags
/// behind a run like a spring, rather than closing a share of the gap each frame. Matched to the
/// exe on 17 emulated scenarios within 1e-5 m (sim test `follow_camera_matches_the_exe`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct FocusChase {
    pub vel: Vec3,
    pub last_target: Vec3,
    pub out: Vec3,
    pub clock: f32,
}

const FOCUS_STEP: f32 = 0.033333335;

impl FocusChase {
    pub(crate) fn at(target: Vec3) -> Self {
        Self { vel: Vec3::ZERO, last_target: target, out: target, clock: 0.0 }
    }

    /// One frame toward `target` at `rate` per axis; returns the new focus. The exe returns
    /// zero for a frame under 1.2e-7 s; here the focus stays put.
    pub(crate) fn update(&mut self, target: Vec3, rate: Vec3, dt: f32) -> Vec3 {
        self.clock += dt;
        while self.clock >= FOCUS_STEP {
            self.clock -= FOCUS_STEP;
        }
        if dt < 1.192_092_9e-7 {
            return self.out;
        }
        let last = std::mem::replace(&mut self.last_target, target);
        let since = self.clock - dt;
        if since > 0.0 {
            // No step boundary this frame: on at the current velocity.
            self.out += self.vel * (dt / FOCUS_STEP);
            return self.out;
        }
        let (mut since, mut t) = (since, self.clock);
        while since < -FOCUS_STEP {
            since += FOCUS_STEP;
            t += FOCUS_STEP;
        }
        let tv = (target - last) / dt;
        let part = (since + FOCUS_STEP) / FOCUS_STEP;
        let mut gap = (target - self.out) + self.vel * part - tv * t;
        let mut x = rate * gap;
        let mut moved = x - self.vel * part;
        while t >= FOCUS_STEP {
            t -= FOCUS_STEP;
            gap += tv * FOCUS_STEP - x;
            x = rate * gap;
            moved += x;
        }
        self.vel = (tv * FOCUS_STEP - x + gap) * rate;
        self.out += self.vel * (self.clock / FOCUS_STEP) + moved;
        self.out
    }
}

/// The exe's wrap of an angle into -pi..pi (FUN_14073c260's lock chase): x - trunc((x + pi
/// with x's sign) / 2pi) * 2pi, so an exact half turn goes the way of its sign.
pub(crate) fn wrap_pi(x: f32) -> f32 {
    const PI: f32 = std::f32::consts::PI;
    const TAU: f32 = 6.283_185_5;
    x - ((x + PI.copysign(x)) / TAU).trunc() * TAU
}

/// The CameraParam values the lock chase reads (FUN_14073c260, follow camera +0x2a0 set).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LockChaseParams {
    /// lockTgtPosRate (0.7): the framed point's share of the way to the lock point.
    pub tgt_pos_rate: f32,
    /// lockRotChaseRateX / Y (0.6 / 0.3) per 1/30 s.
    pub rate_x: f32,
    pub rate_y: f32,
    /// lockRotChasePlayAngX / Y (0 in row 0).
    pub play_x: f32,
    pub play_y: f32,
    /// rotRangeXAtLockMin / Max (-30 / 30 deg).
    pub at_lock: (f32, f32),
    /// rotRangeLerpBeginHeight / EndHeight (0.5 / 3 m).
    pub lerp_height: (f32, f32),
}

/// The locked camera's angles this frame, as FUN_14073c260 turns them while the follow camera's
/// lock flag +0x2a0 is set (checked on the emulated exe, test `lock_camera_matches_the_exe`):
/// - the current angles come from focus (+0xd0) - fulcrum (+0xe0) after the fulcrum's drag;
/// - P = focus + lockTgtPosRate * (lock point (+0x280) - focus) (+0x290);
/// - half = `half` (FOV +0x50 * lockRotXShiftRatio +0x278 * 0.5); r = |focus - fulcrum| (the
///   fulcrum's distance, not the camera's); a = asin(r sin(half) / |P - focus|), pi/2 when P is
///   that close (the law of sines: P sits `half` off the screen centre);
/// - target pitch = a + half - elevation of P, clamped to rotRangeXAtLock widened toward `free`
///   (+0x230 / +0x234) as |P.y - focus.y| goes rotRangeLerpBegin -> EndHeight;
/// - target yaw: the camera behind the focus from P;
/// - each moves rate * dt / (1/30) of the wrapped difference; with a play angle, none inside it
///   and the play taken off the difference when the step would fall inside it.
/// `fulcrum` is on the camera side; returns (pitch, yaw) in the orbit's terms (yaw of the
/// focus -> camera arm, pitch + = camera above).
pub(crate) fn lock_chase(focus: Vec3, fulcrum: Vec3, lock_point: Vec3, half: f32, free: (f32, f32), p: LockChaseParams, dt: f32) -> (f32, f32) {
    // FUN_140734070 (yaw atan2(x, z), the fallback when x = z = 0) / FUN_140733e50 (pitch
    // -atan2(y, |xz|)), on camera -> focus vectors as the exe.
    let yaw_of = |v: Vec3, or: f32| if v.x == 0.0 && v.z == 0.0 { or } else { f32::atan2(v.x, v.z) };
    let pitch_of = |v: Vec3| -f32::atan2(v.y, v.with_y(0.0).length());
    let w = focus - fulcrum;
    let (pitch0, yaw0) = (pitch_of(w), yaw_of(w, 0.0));
    let v = (lock_point - focus) * p.tgt_pos_rate;
    let reach = half.sin() * w.length();
    let a = if v.length() <= reach { std::f32::consts::FRAC_PI_2 } else { (reach / v.length()).asin() };
    let (b, e) = p.lerp_height;
    let t = ((v.y.abs() - b) / (e - b)).clamp(0.0, 1.0);
    let lo = p.at_lock.0 + (free.0 - p.at_lock.0) * t;
    let hi = p.at_lock.1 + (free.1 - p.at_lock.1) * t;
    let want = a + half + pitch_of(v);
    let want = if want < lo { lo } else { want.min(hi) };
    let k = dt / 0.033_333_335;
    let chase = |from: f32, to: f32, rate: f32, play: f32| {
        let d = wrap_pi(to - from);
        let next = wrap_pi(from + d * rate * k);
        if d.abs() < play {
            from
        } else if wrap_pi(next - from).abs() < play {
            wrap_pi(from + (d - play.copysign(d)) * rate * k)
        } else {
            next
        }
    };
    let pitch = chase(pitch0, want, p.rate_x, p.play_x);
    let yaw = chase(yaw0, yaw_of(v, yaw0), p.rate_y, p.play_y);
    // The orbit's yaw is the arm's (focus -> camera), the exe's the view's.
    (pitch, wrap_pi(yaw + std::f32::consts::PI))
}

/// The lock point the camera frames (follow camera +0x280): the target's lock point averaged
/// over the last 0.5 s in the target's own frame. The lock manager hands the camera the target
/// entry each frame (FUN_1409c5f20 -> the ChrCam's lock helper +0x90); FUN_140736090 stores the
/// point in the target matrix's frame with the frame's dt in a CSPositionRecorder_mSecond (240
/// samples, FUN_140735e80), and FUN_140735a70 averages the newest while their summed dt is
/// within DAT_143afe848 (0.5, a debug float), turned back through the matrix (+0x1020). A new
/// target (or a frame without one) starts it over. So a lock point that jumps on the target
/// (an attack animation) is followed over 0.5 s; a target that moves as a whole is followed
/// at once. Checked against the exe (test `lock_camera_matches_the_exe`).
#[derive(Clone, Debug, Default)]
pub(crate) struct LockPointAverage {
    target: Option<Entity>,
    samples: std::collections::VecDeque<(Vec3, f32)>,
}

const LOCK_POINT_WINDOW: f32 = 0.5;
const LOCK_POINT_SAMPLES: usize = 240;

impl LockPointAverage {
    /// This frame's averaged lock point; `frame` = the target's (translation, rotation).
    pub(crate) fn update(&mut self, target: Entity, frame: (Vec3, Quat), point: Vec3, dt: f32) -> Vec3 {
        if self.target != Some(target) {
            self.samples.clear();
            self.target = Some(target);
        }
        if dt > 0.0 {
            if self.samples.len() >= LOCK_POINT_SAMPLES {
                self.samples.pop_front();
            }
            self.samples.push_back((frame.1.inverse() * (point - frame.0), dt));
        }
        let (mut sum, mut t, mut n) = (Vec3::ZERO, 0.0f32, 0);
        for (p, d) in self.samples.iter().rev() {
            if LOCK_POINT_WINDOW < t {
                break;
            }
            sum += *p;
            t += d;
            n += 1;
        }
        let local = if n > 0 { sum / n as f32 } else { Vec3::ZERO };
        frame.0 + frame.1 * local
    }

    /// No lock this frame: the next one starts over.
    pub(crate) fn clear(&mut self) {
        self.target = None;
        self.samples.clear();
    }
}

impl CamState {
    #[cfg(test)]
    pub(crate) fn for_tests(combat: &Combat) -> Self {
        Self::new(&lock_cam(combat))
    }

    fn new(lc: &LockCam) -> Self {
        Self {
            slot: 1,
            blend_t: 0.0,
            blend_len: 0.0,
            cam_from: Default::default(),
            dist_raw: lc.dist,
            dist: lc.dist,
            dist_from: lc.dist,
            end_rate: 0.0,
            end_time: 0.0,
            last_153: None,
            angle_from: None,
            look_after: (0.0, 0.0),
            attach_after: (0.0, 0.0),
            look_blocked: false,
            look_ref: None,
            fov: lc.fov_y.to_radians(),
            focus_y: lc.focus_y,
            look: None,
            pos: None,
            wall: WallCast::default(),
            ctrl_snap: 0.0,
            ctrl_hold: 0.0,
            fulcrum: None,
            trans_snap: 1.0,
            trans_hold: 0.0,
            focus_chase: None,
            near: false,
            alpha: 1.0,
            seen: None,
            lock_point: LockPointAverage::default(),
            rates: None,
        }
    }
}

/// CameraSetParam row 0, slot `slot` (1-based; the exe reads entry slot - 1):
/// (lockParamId, camParamId, beginTime, endTime).
fn cam_slot(combat: &Combat, slot: i64) -> (i64, i64, f32, f32) {
    let r = combat.param("CameraSetParam", 0);
    let i = (slot - 1).max(0);
    let n = |k: &str| r[format!("{k}{i}")].as_f64().unwrap_or(0.0);
    (n("lockParamId") as i64, n("camParamId") as i64, n("beginTime") as f32, n("endTime") as f32)
}

// The lock target and the snap are the combat core's (view.rs); the camera sets them and fills
// `View` after it moves (`publish_view`).
pub use crate::view::{CameraSnap, LockOn};

/// The camera's pitch when it is placed behind Wolf.
/// gap: hand-set; the game's starting camera angle (on load / at a checkpoint) is not traced.
const START_PITCH: f32 = 15.0;

/// [`START_PITCH`] in radians; `SHINOBI_CAM_PITCH=<deg>` (a debug knob for sky / lighting shots,
/// negative looks up) replaces it.
fn start_pitch() -> f32 {
    std::env::var("SHINOBI_CAM_PITCH").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(START_PITCH).to_radians()
}

pub struct CameraPlugin;

/// The TAE camera shakes: started on the TAE edges (prev_t, t), which move once per fixed
/// step - in Update every frame between two steps pushed the same shake again.
/// The camera's focus on Wolf: chrOrgOffset_Y (`focus_y`) is measured from the feet, and the
/// capsule's origin (`translation`) is its middle. The snap (start, reset, Retry) and the follow
/// share it: the snap put it 0.9 m high, and the camera sank for ~0.3 s (round 18).
pub(crate) fn chr_focus(translation: Vec3, focus_y: f32) -> Vec3 {
    translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT + Vec3::Y * focus_y
}

pub(crate) fn add_shakes(app: &mut App) {
    app.init_resource::<CameraShake>().add_systems(FixedUpdate, start_shakes.after(crate::actor::ActorSet::Advance));
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        add_shakes(app);
        app.add_plugins(crate::view::ViewPlugin)
            .add_systems(CAMERA_START, (spawn_camera, publish_view).chain())
            .add_systems(

                PostUpdate,
                (shot_knobs, capture_mouse, toggle_lock, orbit_with_mouse, orbit_with_stick, follow_player, apply_shakes, fade_player, publish_view).chain().before(TransformSystems::Propagate),
            );
    }
}

/// The whole game spawns its camera at the start; in another game that game's camera is taken
/// over once it exists.
#[cfg(feature = "host")]
const CAMERA_START: Startup = Startup;
#[cfg(not(feature = "host"))]
const CAMERA_START: Update = Update;

/// The pause / death screens and the idol menu (menu.rs).
type Screen<'w> = Option<Res<'w, State<crate::menu::Screen>>>;

/// Behind a menu (paused, the death screen).
fn in_menu(_screen: &Screen) -> bool {
    _screen.as_ref().is_some_and(|s| *s.get() != crate::menu::Screen::Playing)
}

/// The camera as the combat core reads it (view.rs `View`): the orbit and the camera's place.
fn publish_view(cam: Query<(&OrbitCamera, &Transform)>, mut view: ResMut<crate::view::View>) {
    *view = match cam.single() {
        Ok((oc, t)) => crate::view::View { orbit: Some((oc.yaw, oc.pitch)), eye: Some((t.translation, t.rotation)) },
        Err(_) => crate::view::View::default(),
    };
}

/// Screenshot knobs that hold the camera: SHINOBI_SKILL_CAM="yaw,pitch" [deg] (ffx.rs's skill
/// shots), SHINOBI_FXR_TEST (fxr.rs's effect shots: pitch 5 deg).
fn shot_knobs(mut cam: Query<&mut OrbitCamera>) {
    let Ok(mut oc) = cam.single_mut() else { return };
    if std::env::var("SHINOBI_SKILL").is_ok() {
        if let Ok(v) = std::env::var("SHINOBI_SKILL_CAM") {
            let v: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            if let Some(&y) = v.first() {
                oc.yaw = y.to_radians();
                oc.pitch = v.get(1).copied().unwrap_or(10.0).to_radians();
            }
        }
    }
    if std::env::var("SHINOBI_FXR_TEST").is_ok() {
        oc.pitch = 5f32.to_radians();
    }
}

/// Put by another game on its main camera, the one this camera takes over (needed when it has
/// more than one, e.g. probe or effect cameras).
#[cfg(not(feature = "host"))]
#[derive(Component)]
pub struct HostCamera;

#[cfg(not(feature = "host"))]
fn spawn_camera(
    mut commands: Commands,
    combat: Res<Combat>,
    mut cams: Query<(Entity, &mut Projection, Has<HostCamera>), (With<Camera3d>, Without<OrbitCamera>)>,
    done: Query<(), With<OrbitCamera>>,
) {
    if !done.is_empty() {
        return;
    }
    // The marked camera, else the only one.
    let marked = cams.iter().any(|c| c.2);
    let mut pick = cams.iter_mut().filter(|c| c.2 || !marked);
    let (Some((e, mut projection, _)), None) = (pick.next(), pick.next()) else { return };
    let lc = lock_cam(&combat);
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = lc.fov_y.to_radians();
    }
    commands.entity(e).insert(OrbitCamera { yaw: 0.0, pitch: start_pitch(), focus: Vec3::ZERO, cam: CamState::new(&lc) });
}

#[cfg(feature = "host")]
fn spawn_camera(mut commands: Commands, combat: Res<Combat>) {
    let lc = lock_cam(&combat);
    commands.spawn((
        Camera3d::default(),
        // Bloom (HDR) so the clash sparks and flashes glow (vfx.rs).
        // Only over-bright (HDR > 1) colours bloom: the effects, not the lit scene.
        bevy::post_process::bloom::Bloom {
            intensity: 0.25,
            prefilter: bevy::post_process::bloom::BloomPrefilter { threshold: 1.2, threshold_softness: 0.3 },
            ..bevy::post_process::bloom::Bloom::OLD_SCHOOL
        },
        Projection::Perspective(PerspectiveProjection { fov: lc.fov_y.to_radians(), ..default() }),
        OrbitCamera { yaw: 0.0, pitch: start_pitch(), focus: Vec3::ZERO, cam: CamState::new(&lc) },
        Transform::from_xyz(0.0, 3.0, 9.0).looking_at(Vec3::Y, Vec3::Y),
        // Ears for positional sound effects (ear gap in metres).
        SpatialListener::new(0.25),
    ));
}

/// First click captures and hides the cursor. Esc gives it back.
fn capture_mouse(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if cursor.grab_mode == CursorGrabMode::None && mouse.get_just_pressed().next().is_some() {
        cursor.grab_mode = CursorGrabMode::Confined;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Lock-on target switch by mouse (exe LockTgtManImp, FUN_1409ca120, called every frame from
/// the update FUN_1409c5fe0): a frame's mouse movement above 50 (@0x1432892f0) switches, below
/// 25 (@0x1432892d0) re-arms (cooldown 0). A switch sets the cooldown to 0.5 s, so a held
/// movement repeats every 0.5 s. The exe reads the value from the pad object's mouse state
/// (FUN_14095fd40 / 50: +0x38f8 -> +0x20 / +0x24), negates both axes and scores with
/// d.x * in.x - d.y * in.y (the stick, FUN_1409ca2d0, is not negated). The state is the pad's
/// frame snapshot (FUN_140960650): analog actions 0x54 / 0x55 times -1 unless the
/// option bytes +0x715 / +0x716 are set (likely the camera invert options, not verified), so with
/// the defaults the switch sees the raw axis. The only DirectInput float source is the mouse poll
/// FUN_141a8f540: DIMOUSESTATE lX / lY as is, relative counts per poll, no scale
/// or accumulation found (the action 0x54 binding itself is not traced). Bevy's
/// AccumulatedMouseMotion is the same unit (raw counts a frame, y down), so 50 / 25 apply as is.
/// The pick's d is NDC (FUN_1409ca480: view x projection, / w, no viewport
/// step), so y up: with in.y = +lY a flick down picks the lower target, as here. gap: the sign
/// the 0x55 binding gives lY (not traced), and the stick, unnegated, would then pick downward on
/// stick up unless its struct stores +y down; here stick up picks up.
const LOCK_SWITCH_FIRE: f32 = 50.0;
const LOCK_SWITCH_REARM: f32 = 25.0;
const LOCK_SWITCH_COOLDOWN: f32 = 0.5;

/// A point's angles from the camera's front, sideways and up / down (degrees), and its
/// distance: the exe takes the target and the camera's forward into the camera's matrix and
/// compares them in its xz and yz planes (1409c.c ~10760-10850).
fn cam_angles(cam: &GlobalTransform, p: Vec3) -> (f32, f32, f32) {
    let local = cam.affine().inverse().transform_point3(p);
    let ahead = -local.z;
    (local.x.atan2(ahead).abs().to_degrees(), local.y.atan2(ahead).abs().to_degrees(), local.length())
}

/// Who can be switched to while locked (FUN_1409c5fe0 with the player's lock flag):
/// CamFront{Near,Far}RangeLockChangeHalfAng — within 8 m 60 deg sideways, else 45 deg (up to
/// 200 m), and 30 deg up / down from the camera's front.
pub(crate) fn switchable(cam: &GlobalTransform, p: Vec3) -> bool {
    let (yaw, pitch, dist) = cam_angles(cam, p);
    let (half, radius) = if dist <= 8.0 { (60.0, 8.0) } else { (45.0, 200.0) };
    yaw <= half && pitch <= 30.0 && dist <= radius
}

/// Who a new lock can take (FUN_1409c5fe0, LockTgtManImp +0x2944..+0x2984, defaults 1409c.c
/// 3580-3599): ChrFrontRange - within 5 m (+0x294c) and 30 deg (+0x2948) of Wolf's facing - or
/// CamFrontNear - 60 deg sideways / 30 deg up or down (+0x2960 / 64) within 8 m (+0x2970) of
/// the camera - or CamFrontFar - 45 / 30 deg (+0x2974 / 78) out to the lock range (+0x2984 200,
/// here LockCamParam's chrLockRangeMaxRadius, which lockable already holds to).
/// gap: the extra distance terms (fVar71, local_2a8) the exe adds are not traced; taken as 0.
pub(crate) fn acquirable(cam: &GlobalTransform, wolf: Vec3, wolf_fwd: Vec3, p: Vec3) -> bool {
    let to = (p - wolf).with_y(0.0);
    let fwd = wolf_fwd.with_y(0.0);
    let chr_front = to.length() <= 5.0 && (to.length_squared() < 1e-6 || fwd.angle_between(to).to_degrees() <= 30.0);
    let (yaw, pitch, dist) = cam_angles(cam, p);
    let near = dist <= 8.0 && yaw <= 60.0 && pitch <= 30.0;
    let far = yaw <= 45.0 && pitch <= 30.0;
    chr_front || near || far
}

/// LockTgtMan's sight test (FUN_1409c5fe0 -> FUN_14094e310, the lock's own ray collector,
/// filter 0xB1): a ray from the camera to the candidate; a hit hides it. A hidden enemy cannot be
/// taken and a held one is let go after LOCK_HIDDEN_TIMEOUT. gap: the exe aims at the chr's
/// position; here the body's centre (the feet's ray grazes the floor).
fn in_sight(cam: Vec3, p: Vec3) -> bool {
    let d = p - cam;
    let len = d.length();
    len < 1e-3 || crate::terrain::raycast(cam, d / len, len).is_none()
}
/// VisibleCheckIntervalTime (LockTgtManImp +0x2924): how often a candidate's ray runs.
const LOCK_SIGHT_INTERVAL: f32 = 0.5;
/// 不可視ロックタイムアウト時間 (+0x2928): how long a held target may stay hidden.
const LOCK_HIDDEN_TIMEOUT: f32 = 2.0;

/// The held target's sight clock: (next ray in, hidden for). Returns whether the lock holds.
fn hold_hidden(clock: &mut (f32, f32), dt: f32, sight: impl FnOnce() -> bool) -> bool {
    clock.0 -= dt;
    if clock.0 <= 0.0 {
        clock.0 = LOCK_SIGHT_INTERVAL;
        if sight() {
            clock.1 = 0.0;
            return true;
        }
    } else if clock.1 == 0.0 {
        return true;
    }
    clock.1 += if clock.1 == 0.0 { LOCK_SIGHT_INTERVAL } else { dt };
    clock.1 < LOCK_HIDDEN_TIMEOUT
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn toggle_lock(
    _keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    combat: Res<Combat>,
    time: Res<Time>,
    mut lock: ResMut<LockOn>,
    mut cooldown: Local<f32>,
    mut sight: Local<(f32, f32)>,
    pads: Query<&Gamepad>,
    config: Res<GameConfig>,
    player: Single<(&Transform, &crate::actor::Actor), With<Player>>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
    cams: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    (screen, cursor): (Screen, Option<Single<&CursorOptions, With<PrimaryWindow>>>),
) {
    let range = lock_cam(&combat).lock_range;
    let (wolf, wolf_fwd) = (player.0.translation, player.1.forward());
    // Lockable: alive and within chrLockRangeMaxRadius.
    let lockable = |tf: &Transform, e: &Enemy| !e.is_dead() && tf.translation.distance(wolf) < range;
    if let Some(t) = lock.target {
        // A dead target drops the lock (gap: whether the exe moves it to the next enemy).
        if !enemies.get(t).is_ok_and(|(_, tf, e)| lockable(tf, e)) {
            lock.target = None;
        }
    }
    // Out of sight behind a wall for LOCK_HIDDEN_TIMEOUT: the lock lets go.
    let cam_pos = cams.iter().next().map(|(_, g)| g.translation());
    if let (Some(t), Some(c)) = (lock.target, cam_pos) {
        if let Ok((_, tf, _)) = enemies.get(t) {
            if !hold_hidden(&mut sight, time.delta_secs(), || in_sight(c, tf.translation)) {
                lock.target = None;
            }
        }
    } else {
        *sight = (LOCK_SIGHT_INTERVAL, 0.0);
    }
    // Only in play: behind a menu (paused, the death screen) the mouse and stick are the menu's
    // and the game clock - the switch cooldown's - stands still.
    if in_menu(&screen) {
        return;
    }
    let pad_lock = config.input.gamepad && pads.iter().any(|g| g.just_pressed(GamepadButton::RightThumb));
    // Lock on: the middle mouse button, the game's default (DefaultKeyAssignParam00, player.rs
    // read_input); Q is the game's crouch key.
    if mouse.just_pressed(MouseButton::Middle) || pad_lock {
        lock.target = match lock.target {
            Some(_) => None,
            // The nearest of those in front (acquirable); no camera (headless): any.
            None => {
                let cam = cams.iter().next().map(|(_, g)| *g);
                enemies
                    .iter()
                    .filter(|(_, tf, e)| lockable(tf, e) && cam.is_none_or(|c| acquirable(&c, wolf, wolf_fwd, tf.translation) && in_sight(c.translation(), tf.translation)))
                    .min_by(|a, b| a.1.translation.distance(wolf).total_cmp(&b.1.translation.distance(wolf)))
                    .map(|(e, ..)| e)
            }
        };
    }
    // Several enemies: the mouse switch (see LOCK_SWITCH_FIRE), or the right stick
    // (FUN_1409ca2d0: |stick| < 0.5 re-arms, > 0.95 fires; FUN_140960650 stores the right stick
    // in the pad frame +0x10 / +0x14 as (x, -y), so up on the stick is up on screen: viewport y
    // down; 2026-11-03 lockcam).
    let Some(cur) = lock.target else { return };
    let stick = if config.input.gamepad { pads.iter().map(|g| g.right_stick()).max_by(|a, b| a.length().total_cmp(&b.length())).unwrap_or_default() } else { Vec2::ZERO };
    // The mouse turns only with the cursor captured (as orbit_with_mouse).
    let mouse_delta = if cursor.is_some_and(|c| c.grab_mode != CursorGrabMode::None) { motion.delta } else { Vec2::ZERO };
    let (flick, mag, rearm, fire) = if stick.length() >= mouse_delta.length() / LOCK_SWITCH_FIRE {
        (Vec2::new(stick.x, -stick.y), stick.length(), 0.5, 0.95)
    } else {
        (mouse_delta, mouse_delta.length(), LOCK_SWITCH_REARM, LOCK_SWITCH_FIRE)
    };
    if mag < rearm {
        *cooldown = 0.0;
    } else {
        *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    }
    if *cooldown > 0.0 || mag <= fire {
        return;
    }
    let Ok((_, ctf, _)) = enemies.get(cur) else { return };
    let Some((camera, cam_tf)) = cams.iter().next() else { return };
    // FUN_1409ca480: the targets on screen; d = candidate - current, score = cos(angle between
    // d and the movement) / |d| (asm 0x1409ca82e-0x1409ca85c); the best above cos(90 deg)
    // (+0x2854 = pi/2) wins: the nearest on screen within 90 deg of the flick.
    let Ok(at) = camera.world_to_viewport(cam_tf, ctf.translation) else { return };
    let dir = flick / mag;
    let next = enemies
        .iter()
        .filter(|(e, tf, en)| *e != cur && lockable(tf, en) && switchable(cam_tf, tf.translation) && in_sight(cam_tf.translation(), tf.translation))
        .filter_map(|(e, tf, _)| {
            let d = camera.world_to_viewport(cam_tf, tf.translation).ok()? - at;
            let len2 = d.length_squared();
            (len2 > 1e-6).then(|| (e, d.dot(dir) / len2))
        })
        .filter(|(_, score)| *score > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e);
    if let Some(n) = next {
        lock.target = Some(n);
        *cooldown = LOCK_SWITCH_COOLDOWN;
    }
}

/// Shortest signed difference `a - b` in radians (wraps through +-pi).
fn angle_delta(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// A per-frame rate of the exe over a frame of `dt`. The exe applies these once a frame with no
/// dt in them (FUN_14073c260: the position chase, distance and FOV chases, the look limits), so
/// at 30 fps it closes `rate` a frame, not two 60 fps frames' worth (checked on the emulated
/// exe at 1/30 s, sim tests `follow_camera_at_walls_matches_the_exe` / `lock_camera_matches_the_exe`).
/// The game never runs faster than 60 fps; a shorter RSekiro frame takes its share of one
/// 60 fps frame.
pub(crate) fn per_frame(rate: f32, dt: f32) -> f32 {
    let rate = rate.clamp(0.0, 1.0);
    if dt * 60.0 >= 1.0 { rate } else { 1.0 - (1.0 - rate).powf(dt * 60.0) }
}

/// The move of one exe frame from this frame's move: the exe measures moves per frame with no dt
/// (the fulcrum's sideways move, `place_fulcrum`), so a 30 fps frame's move counts whole; a
/// frame shorter than the game's 1/60 s is scaled up to one.
pub(crate) fn per_exe_frame(moved: f32, dt: f32) -> f32 {
    moved / (dt * 60.0).clamp(1e-3, 1.0)
}

/// TAE 156 SetCameraAngle (exe FUN_140b51490 -> FUN_1408327a0 fills the camera singleton
/// DAT_143d5c0d0 +0x88..+0x9b each frame the event runs on the player; read by FUN_1408310d0 in
/// the ChrCam update, 14073.c 10766-10855). `from` = the camera's (yaw, pitch) at the event's
/// first frame (+0x1f4 / +0x1f0), `p` = (t - start) / (end - start) clamped to 1, eased to
/// w = 1 - (p - 1)^2:
/// - pitch (+0x170) = from + (VerticalAngle - from) * w when OverrideVertical (an absolute
///   angle; positive looks down, as `OrbitCamera::pitch`);
/// - yaw (+0x174) the same toward HorizontalAngle + the character's yaw (FUN_1409eec10 +
///   FUN_14073a320 wrap) when OverrideHorizontal. Our map mirrors the game's x (map.md), so the
///   game's angle offset turns the other way here: target = Wolf's yaw - HorizontalAngle
///   (0 = behind him). InterpolationType 0 takes the short way round, 1 ("Clockwise") the way
///   the game's angle grows, 2 the way it falls (14073.c 10801-10831).
pub(crate) fn set_camera_angle(from: (f32, f32), chr_yaw: f32, horizontal: (bool, f32), vertical: (bool, f32), interp: i64, p: f32) -> (f32, f32) {
    use std::f32::consts::{PI, TAU};
    let wrap = |a: f32| (a + PI).rem_euclid(TAU) - PI;
    let p = p.clamp(0.0, 1.0);
    let w = 1.0 - (p - 1.0) * (p - 1.0);
    let (mut yaw, mut pitch) = from;
    if vertical.0 {
        pitch = wrap(from.1 + (vertical.1 - from.1) * w);
    }
    if horizontal.0 {
        let start = wrap(from.0);
        let mut target = wrap(chr_yaw - horizontal.1);
        // The game's angle grows = ours falls, and the other way round.
        match interp {
            1 if target > start => target -= TAU,
            2 if target < start => target += TAU,
            1 | 2 => {}
            _ if target - start > PI => target -= TAU,
            _ if target - start < -PI => target += TAU,
            _ => {}
        }
        yaw = wrap(start + (target - start) * w);
    }
    (yaw, pitch)
}

/// The param side of the look limits (FUN_140742230): LockCamParam rotRangeMinX (radians) and
/// CameraParam rotRangeXMax for pitch, CameraParam rotRangeYMin / Max and rotBaseY for yaw.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LookParams {
    pub pitch: (f32, f32),
    pub yaw: (f32, f32),
    pub base_y: f32,
}

/// The look limits (camera +0x230..+0x23c: pitch min / max, yaw min / max; yaw in the exe's
/// convention, `exe_yaw`) one frame on, as FUN_140742230 (from FUN_14073c260 each frame):
/// - TAE 151 running (`event` = its LookUp / Down / Left / RightLimit, radians, singleton
///   +0x4c..+0x58) and its cast not blocked: the limits are the event's around `centre`, the
///   view along minus row 2 of the DummyPolyID's world matrix (FUN_14073a1e0; pitch
///   FUN_140733d70 = -atan2(y, xz), yaw FUN_140733fb0 = atan2(x, z)), or around (0, 0) without a
///   dummy. The event's yaw limits are used only when they are not the full circle;
/// - no event (or the cast blocked: `blocked`, camera +0x1a0): the param's, absolute when its yaw
///   range is the full circle (every row but 6), else around Wolf's facing (`chr`, chr matrix
///   row 2) + rotBaseY. Blocked with the full circle, the event's pitch limits stay but around 0;
/// - the centre's yaw is unwrapped to within pi of +0x240 (`look_ref`), which is the camera's
///   yaw (+0x174, `cam_yaw`) when first needed and cleared between events;
/// - each limit chases its target at CameraParam.lockCamParamLerpRate per frame (`rate`, already
///   a per-frame step), but yaw limits that are not chased are set to the param's at once.
#[allow(clippy::too_many_arguments)]
pub(crate) fn look_limits_step(
    look: &mut [f32; 4],
    look_ref: &mut Option<f32>,
    event: Option<[f32; 4]>,
    blocked: bool,
    centre: Option<(f32, f32)>,
    chr: (f32, f32),
    p: LookParams,
    cam_yaw: f32,
    rate: f32,
) {
    use std::f32::consts::{PI, TAU};
    let mut chase_yaw = false;
    let (mut c_pitch, mut c_yaw) = (0.0, 0.0);
    let mut lim = event.unwrap_or([0.0; 4]);
    let mut target = [p.pitch.0, p.pitch.1, p.yaw.0, p.yaw.1];
    let full = p.yaw.0 <= -PI && PI <= p.yaw.1;
    let unwrap = if event.is_none() || blocked {
        if full {
            event.is_some()
        } else {
            chase_yaw = true;
            lim = target;
            (c_pitch, c_yaw) = (chr.0, chr.1 + p.base_y);
            true
        }
    } else {
        chase_yaw = -PI < lim[2] || lim[3] < PI;
        if let Some(c) = centre {
            (c_pitch, c_yaw) = c;
        }
        true
    };
    if unwrap {
        let r = *look_ref.get_or_insert(cam_yaw);
        if (c_yaw + TAU - r).abs() < (c_yaw - r).abs() {
            c_yaw += TAU;
        } else if (c_yaw - TAU - r).abs() < (c_yaw - r).abs() {
            c_yaw -= TAU;
        }
        target = [c_pitch + lim[0], c_pitch + lim[1], c_yaw + lim[2], c_yaw + lim[3]];
    } else {
        *look_ref = None;
    }
    look[0] += (target[0] - look[0]) * rate;
    look[1] += (target[1] - look[1]) * rate;
    if chase_yaw {
        look[2] += (target[2] - look[2]) * rate;
        look[3] += (target[3] - look[3]) * rate;
    } else {
        look[2] = p.yaw.0;
        look[3] = p.yaw.1;
    }
}

/// The free camera's yaw clamp (FUN_14073c260, 14073.c 10339-10378, the exe's convention): the
/// wrapped yaw is compared with [lo, hi] shifted by a turn when it lies past the midpoint of the
/// gap outside the range, so a range crossing +-pi still clamps to its nearer end.
pub(crate) fn clamp_yaw_exe(yaw: f32, lo: f32, hi: f32) -> f32 {
    use std::f32::consts::TAU;
    let v = wrap_pi(yaw);
    let r = if (lo + hi - TAU) * 0.5 <= v {
        if v <= (lo + TAU + hi) * 0.5 {
            if v < lo {
                lo
            } else if hi < v {
                hi
            } else {
                v
            }
        } else if lo + TAU <= v {
            v.min(hi + TAU)
        } else {
            lo + TAU
        }
    } else if lo - TAU <= v {
        v.min(hi - TAU)
    } else {
        lo - TAU
    };
    wrap_pi(r)
}

/// The free camera's angles clamped by the look limits (`look_limits_step`; exe FUN_14073c260
/// after the stick turn: pitch +0x170 plainly into [+0x230, +0x234], yaw +0x174 by
/// `clamp_yaw_exe` into [+0x238, +0x23c]). `yaw` is ours (`exe_yaw` converts); returns ours,
/// the yaw untouched when the clamp leaves it where it was.
pub(crate) fn apply_look_limits(yaw: f32, pitch: f32, look: &[f32; 4]) -> (f32, f32) {
    let pitch = if pitch < look[0] {
        look[0]
    } else if look[1] < pitch {
        look[1]
    } else {
        pitch
    };
    let e = exe_yaw(yaw);
    let c = clamp_yaw_exe(e, look[2], look[3]);
    (if angle_delta(c, e).abs() < 1e-6 { yaw } else { exe_yaw(c) }, pitch)
}

/// Our camera yaw to the exe's (+0x174) and back: our map mirrors the game's x (map.md) and our
/// yaw points from the focus to the camera, the exe's from the camera to the focus, so the exe's
/// is pi - ours (the conversion is its own inverse).
pub(crate) fn exe_yaw(yaw: f32) -> f32 {
    wrap_pi(std::f32::consts::PI - yaw)
}

/// Which way of our dummy's local Z the exe's view along minus row 2 of the dummy's world
/// matrix is (TAE 151's centre). Checked on Wolf's dummy 142 against the emulated exe (sim test
/// `tae_camera_events_match_the_exe`).
const DUMMY_VIEW: f32 = 1.0;

/// TAE 151's centre for a dummy (FUN_140742230): its view's (pitch, yaw) in the exe's angles
/// (pitch FUN_140733d70 = -atan2(y, xz), positive looking down as ours; yaw that of a camera
/// looking along it, `exe_yaw`).
pub(crate) fn dummy_look(g: &GlobalTransform) -> (f32, f32) {
    let d = g.rotation() * Vec3::Z * DUMMY_VIEW;
    (-f32::atan2(d.y, d.xz().length()), exe_yaw(f32::atan2(-d.x, -d.z)))
}

/// Camera pitch limits [rad] from CameraParam row 0: rotRangeXMin..rotRangeXMax (-40..80 deg),
/// rotRangeXAtLockMin..Max (-30..30 deg) while locked on. Same sign as LockCamParam rotRangeMinX.
fn pitch_range(combat: &Combat, locked: bool) -> (f32, f32) {
    let r = combat.param("CameraParam", 0);
    let f = |k: &str, d: f32| r[k].as_f64().map_or(d, |v| v as f32);
    if locked {
        (f("rotRangeXAtLockMin", -std::f32::consts::FRAC_PI_6), f("rotRangeXAtLockMax", std::f32::consts::FRAC_PI_6))
    } else {
        (f("rotRangeXMin", lock_cam(combat).min_pitch.to_radians()), f("rotRangeXMax", 1.3963))
    }
}

/// The right stick's turn rate [rad/s] per axis (x: yaw, y: pitch) from CameraParam (deg/s in
/// the paramdef, radians in the export), as the exe: the camera speed option (options byte 0,
/// default 5) is stored as s = clamp(byte / 10, 0, 1) (FUN_140734790 -> camera +0x270), and each
/// frame the camera keeps lerp(rotSpeed_Min, rotSpeed_Max, s * s) and lerp(rotHiSpeed_Min,
/// rotHiSpeed_Max, s * s) (FUN_14073c260 -> +0x250..+0x264); the stick turns at
/// lerp(normal, high, |ramp|) (`hi`, see orbit_with_stick).
pub(crate) fn stick_rate(combat: &Combat, speed: f32, hi: f32) -> Vec2 {
    let c = combat.param("CameraParam", 0);
    let f = |k: &str, d: f64| c[k].as_f64().unwrap_or(d) as f32;
    let s = (speed / 10.0).clamp(0.0, 1.0);
    let t = s * s;
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let base = Vec2::new(lerp(f("rotSpeed_MinY", 0.977), f("rotSpeed_MaxY", 3.665)), lerp(f("rotSpeed_MinX", 0.977), f("rotSpeed_MaxX", 2.932)));
    let high = Vec2::new(lerp(f("rotHiSpeed_MinY", 2.688), f("rotHiSpeed_MaxY", 7.575)), lerp(f("rotHiSpeed_MinX", 2.199), f("rotHiSpeed_MaxX", 6.109)));
    base + (high - base) * hi.clamp(0.0, 1.0)
}

/// The fast turn ramp (exe camera +0x160) for a stick `r` (x: the yaw axis): see
/// orbit_with_stick; `inc` = dt / rotHiSpeedRateIncTime.
pub(crate) fn hi_ramp(hi: f32, r: Vec2, inc: f32) -> f32 {
    if r.length() <= 0.998 || r.x.abs() <= 1e-3 {
        0.0
    } else if r.x > 0.0 {
        (hi.max(0.0) + inc).min(1.0)
    } else {
        (hi.min(0.0) - inc).max(-1.0)
    }
}

/// The right stick turns the camera (unlocked; locked it switches targets, toggle_lock).
fn orbit_with_stick(
    time: Res<Time>,
    config: Res<GameConfig>,
    combat: Res<Combat>,
    lock: Res<LockOn>,
    pads: Query<&Gamepad>,
    mut camera: Single<&mut OrbitCamera>,
    mut hi: Local<f32>,
) {
    if !config.input.gamepad || lock.target.is_some() {
        *hi = 0.0;
        return;
    }
    let Some(r) = pads.iter().map(|g| g.right_stick()).max_by(|a, b| a.length().total_cmp(&b.length())) else { return };
    let dt = time.delta_secs();
    // The fast turn (exe camera +0x160): only with the stick all the way out (|stick| > 0.998)
    // and turning sideways, it ramps toward +-1 (the turn's side) by dt / rotHiSpeedRateIncTime,
    // starting over from 0 when the side flips; otherwise 0. gap: the exe's sideways threshold
    // (thunk_FUN_1419dcdb0) is read as "not zero". The exe's yaw slot is the stick's x and its
    // pitch slot -y (FUN_1409668f0 / FUN_140966960 with the default options 10 / 11 = 1; both
    // flip when cleared, invert_y is option 11; checked in emu, 2026-11-03 lockcam). Exe yaw
    // + x is our yaw - x (mirrored world); exe pitch -y lowers the camera: stick up looks up.
    let inc = dt / combat.param("CameraParam", 0)["rotHiSpeedRateIncTime"].as_f64().unwrap_or(0.6).max(1e-3) as f32;
    *hi = hi_ramp(*hi, r, inc);
    if r.length() < 1e-3 {
        return;
    }
    let rate = stick_rate(&combat, config.input.camera_speed, hi.abs());
    let sign = if config.input.invert_y { -1.0 } else { 1.0 };
    // In the fast turn the exe drops the stick's size for yaw: the full rate, the stick's side
    // only (FUN_14073c260 ~0x14073e5xx; the emulated diagonal stick turns as fast as a full
    // sideways one), and the pitch goes to the stick's tilt (`direct_pitch`).
    let fast = *hi != 0.0;
    let side = if fast { r.x.signum() } else { r.x };
    camera.yaw -= side * rate.x * dt;
    let (lo, hi_pitch) = pitch_range(&combat, false);
    // Stick up looks up: the camera goes under the focus (pitch down).
    camera.pitch = if fast {
        let c = combat.param("CameraParam", 0);
        let f = |k: &str, d: f64| c[k].as_f64().unwrap_or(d) as f32;
        direct_pitch(camera.pitch, -sign * r.y, (lo, hi_pitch), f("directPitchAng", 1.0472), f("directPitchRate", 0.06), dt)
    } else {
        camera.pitch - sign * r.y * rate.y * dt
    }
    .clamp(lo, hi_pitch);
}

/// The pitch in the fast turn (camera +0x160 not 0): it chases the stick's tilt `up` (+ raises
/// the camera) over CameraParam directPitchAng (60 deg) times the pitch limit on that side, at
/// directPitchRate (0.06) per 1/30 s; a full sideways stick levels the camera. Fitted to the
/// exe on 5 emulated scenarios (diagonals up and down, 0.3 up, a level stick from 0.73 rad;
/// 600 frames within 3e-6 rad, sim test `follow_camera_matches_the_exe`); the code in
/// FUN_14073c260 is not traced.
pub(crate) fn direct_pitch(pitch: f32, up: f32, (lo, hi): (f32, f32), ang: f32, rate: f32, dt: f32) -> f32 {
    let want = up / ang.sin().max(1e-3) * if up > 0.0 { hi } else { -lo };
    pitch + (want - pitch) * (rate * dt * 30.0).min(1.0)
}

fn orbit_with_mouse(
    motion: Res<AccumulatedMouseMotion>,
    config: Res<GameConfig>,
    combat: Res<Combat>,
    lock: Res<LockOn>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut camera: Single<&mut OrbitCamera>,
) {
    if cursor.grab_mode == CursorGrabMode::None {
        return;
    }
    // Locked on, the lock camera owns both angles (in the game the right stick then switches
    // targets). Mouse pitch on top fought the lock chase every frame: the camera shook whenever
    // the mouse moved, most of all on head kicks where the chase is fastest.
    // gap: CameraParam lockCamAdjustRot_MaxX/Y (6 / 8 deg) / _ChaseRate small manual offsets
    // are not read by the ChrCam update FUN_14073c260 (no +0x128..+0x130 load off the param
    // row there) nor the other CameraParam getter callers; their reader is not found.
    if lock.target.is_some() {
        return;
    }
    let cam = &config.camera;
    let sensitivity = cam.mouse_sensitivity.to_radians();
    camera.yaw -= motion.delta.x * sensitivity;
    let (lo, hi) = pitch_range(&combat, false);
    let sign = if config.input.invert_y { -1.0 } else { 1.0 };
    camera.pitch = (camera.pitch + sign * motion.delta.y * sensitivity).clamp(lo, hi);
}

pub(crate) fn follow_player(
    time: Res<Time>,
    combat: Res<Combat>,
    lock: Res<LockOn>,
    player_q: Single<(&Transform, &Actor, Option<&crate::model::Dummies>), (With<Player>, Without<OrbitCamera>)>,
    dummy_tf: Query<&GlobalTransform>,
    enemies: Query<(&Transform, Option<&crate::model::Dummies>, &Actor), (With<Enemy>, Without<OrbitCamera>)>,
    mut camera: Single<(&mut Transform, &mut OrbitCamera, &mut Projection)>,
    mut snap: ResMut<CameraSnap>,
    pad: Option<Res<crate::player::PadInput>>,
    config: Option<Res<crate::config::GameConfig>>,
) {
    let (player, actor, dummies) = *player_q;
    let (transform, orbit, projection) = &mut *camera;
    let dt = time.delta_secs();
    if std::mem::take(&mut snap.0) {
        // A fresh camera (a Retry after the death camera's 7.5 m slot, a reset): the default
        // slot, distance, FOV and focus height, no chase or look-at left over; Wolf's fade kept.
        let alpha = orbit.cam.alpha;
        orbit.cam = CamState::new(&lock_cam(&combat));
        orbit.cam.alpha = alpha;
        // Behind Wolf: the arm (yaw rotation of +Z) points opposite his facing (yaw rotation of -Z).
        orbit.yaw = actor.yaw;
        orbit.pitch = start_pitch();
        orbit.focus = chr_focus(player.translation, orbit.cam.focus_y);
    }
    update_cam_slot(&combat, actor, &mut orbit.cam, dt);
    // The CameraParam the camera reads (the slot's row blended from the last switch, `CamBlend`)
    // and the slot's LockCamParam row (not blended: its values are chased below).
    let blend = orbit.cam.blend();
    let lock_row = orbit.cam.lock_row(&combat);
    let cf = |k: &str, d: f64| blend.get(&combat, k, d);
    let lf = |k: &str, d: f64| lock_row[k].as_f64().unwrap_or(d) as f32;
    // +0x154 counts down, then +0x150 falls over rotCtrlEndReleaseTime (FUN_14073c260
    // 0x14073e8xx); turning the camera (the right stick / mouse, unlocked) sets it back to 1
    // and +0x154 to rotCtrlEndWaitTime.
    orbit.cam.ctrl_hold = (orbit.cam.ctrl_hold - dt).max(0.0);
    if orbit.cam.ctrl_hold <= 0.0 {
        orbit.cam.ctrl_snap = (orbit.cam.ctrl_snap - dt / cf("rotCtrlEndReleaseTime", 1.0).max(1e-3)).max(0.0);
    }
    // TAE 152 AttachCameraToNode (handler FUN_140b51750, each frame it runs on the camera's chr:
    // FUN_140832860 -> singleton +0x60 DummyPolyID / +0x64 BlendStrength, FUN_140832880 -> +0x68
    // unk1 / +0x6c unk2): the camera rides the DummyPoly (`attach_to_node`), chases it at
    // BlendStrength (`chase_position_attached`), and the stick does not turn it (FUN_14073c260
    // reads the camera stick only while +0x60 < 0); the distance chases at unk2 while unk1
    // seconds count down after the event. Wolf's 7101xx and a212_5103xx anims, dummy 143.
    let attach = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.step()).find(|e| e.kind == 152) };
    let attach = attach.filter(|e| e.arg_i64("DummyPolyID").unwrap_or(-1) >= 0).map(|e| {
        let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        orbit.cam.attach_after = (f("unk1"), f("unk2"));
        (e.arg_i64("DummyPolyID").unwrap_or(-1) as i16, f("BlendStrength"))
    });
    // gap: a DummyPoly Wolf's model lacks leaves the camera free (the exe's FUN_1409e6db0
    // answer for a missing dummy is not traced).
    let attach_node = attach.and_then(|(dmy, _)| dummies.and_then(|d| d.0.get(&dmy)).and_then(|&m| dummy_tf.get(m).ok())).map(|g| g.translation());
    // This frame's turning by hand (mouse / stick, before the scripted look limits move the
    // angles); the fulcrum's yaw resync adds it back. None while TAE 152 holds the camera.
    let turning = orbit.cam.seen.filter(|_| attach.is_none()).map(|(y, p)| (angle_delta(orbit.yaw, y), orbit.pitch - p)).filter(|d| d.0.abs() > 1e-6 || d.1.abs() > 1e-6);
    if attach.is_none() && orbit.cam.seen.is_some_and(|s| s != (orbit.yaw, orbit.pitch)) && lock.target.is_none() {
        orbit.cam.ctrl_snap = 1.0;
        orbit.cam.ctrl_hold = cf("rotCtrlEndWaitTime", 2.0);
    }
    let lerp_rate = cf("lockCamParamLerpRate", 0.05);
    update_cam_distance(&combat, actor, &mut orbit.cam, lf("camDistTarget", 4.5), lerp_rate, dt);
    // +0x50 the FOV chases LockCamParam camFovY at lockCamParamLerpRate per frame; +0x204 the
    // focus height is CameraParam chrOrgOffsetY moved that share toward LockCamParam
    // chrOrgOffset_Y, anew each frame (FUN_14073c260, 14073.c 9056-9082). gap: +0x278
    // (lockRotXShiftRatio, chased the same way) is read straight from the row by the lock-on pitch.
    orbit.cam.fov += (lf("camFovY", 43.0).clamp(0.0, 180.0).to_radians() - orbit.cam.fov) * per_frame(lerp_rate, dt);
    let org_y = cf("chrOrgOffsetY", 1.5);
    orbit.cam.focus_y = org_y + (lf("chrOrgOffset_Y", 1.5) - org_y) * lerp_rate;
    if let Projection::Perspective(pp) = &mut **projection {
        pp.fov = orbit.cam.fov;
    }
    let range = pitch_range(&combat, lock.target.is_some());
    // TAE 156 SetCameraAngle takes the place of the lock-on turning while it runs (14073.c 10606:
    // the lock chase only when FUN_140831ea0, "an angle event is set", is false).
    let set_angle = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.step()).find(|e| e.kind == 156) };
    // Locked on, the point the camera frames (+0x280): the target's DummyPoly 220 averaged over
    // 0.5 s in its own frame (`LockPointAverage`); the turning comes after the focus follow and
    // the fulcrum's drag (`lock_chase`, the exe's order). gap: DummyPoly 220 as the lock target
    // entry's point (written by the lock manager, not traced).
    let lock_point = match lock.target.and_then(|e| Some((e, enemies.get(e).ok()?))) {
        Some((e, (target, tdummies, _))) if set_angle.is_none() => {
            let point = tdummies
                .and_then(|d| d.0.get(&220))
                .and_then(|&m| dummy_tf.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(target.translation + Vec3::Y * (lf("chrOrgOffset_Y", 1.5) - crate::player::CAPSULE_HALF_HEIGHT));
            Some(orbit.cam.lock_point.update(e, (target.translation, target.rotation), point, dt))
        }
        _ => {
            orbit.cam.lock_point.clear();
            None
        }
    };
    if set_angle.is_none() && lock_point.is_none() {
        orbit.pitch = orbit.pitch.clamp(range.0, range.1);
    }
    let mut target_focus = chr_focus(player.translation, orbit.cam.focus_y);
    // The hug peek (docking.rs CoverAction*): the camera leans out round the wall end with Wolf,
    // 1.2 m to the peek's side and 0.3 m forward (gap: the game's cover camera rows are not
    // read). The clips' _R peeks to Wolf's own left (named from the front, as the hug moves).
    if crate::docking::is_peek(&actor.state) {
        let fwd = Quat::from_rotation_y(actor.yaw) * Vec3::NEG_Z;
        let right = fwd.cross(Vec3::Y);
        let side = if actor.state.ends_with('R') { -right } else { right };
        target_focus += side * 1.2 + fwd * 0.3;
    }
    // Focus follow (exe FUN_141155d20 from the ChrCam update FUN_14073c260, `FocusChase`) at the
    // working rates +0x220 / +0x224 as last frame left them (`working_rates_step`: eased toward
    // CameraParam chrTransChaseRateXZ_ForNormal 0.2 / _Y 0.3).
    let row_rates = Vec3::new(cf("chrTransChaseRateXZ_ForNormal", 0.2), cf("chrTransChaseRateY_ForNormal", 0.3), cf("targetChaseRateXZForNormal", 0.1));
    let rates = *orbit.cam.rates.get_or_insert(row_rates);
    let mut rate = Vec3::new(rates.x, rates.y, rates.x);
    // TAE 151 CameraLookAtTarget (handler FUN_140b517a0 -> camera singleton +0x3c DummyPolyID,
    // +0x40 unk1, +0x44 unk2, +0x48 unk3, +0x4c.. the limits; FUN_14073c260, 14073.c 9434-9506):
    // the focus chases that DummyPoly on Wolf at min(unk1, 1) on every axis (the normal rates
    // when <= 0), unless the cast from Wolf's focus to the dummy hits something: then (camera
    // +0x1a0, until the event ends) the focus stays on Wolf at the normal rates. After the event
    // it chases Wolf at unk3 while unk2 seconds count down (FUN_140838b50). Big deflects and the
    // deathblow throws use dmy 142. gap: the exe's cast (FUN_14094cf90, its own hit filter) is
    // terrain::raycast here.
    let mut target_focus = target_focus;
    let look = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.step()).find(|e| e.kind == 151) };
    let look = look.filter(|e| e.arg_i64("DummyPolyID").unwrap_or(-1) >= 0);
    let (mut look_limits, mut look_centre) = (None, None);
    match look {
        Some(e) => {
            let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            orbit.cam.look_after = (f("unk2"), f("unk3"));
            look_limits = Some([f("LookUpLimit"), f("LookDownLimit"), f("LookLeftLimit"), f("LookRightLimit")].map(f32::to_radians));
            let dmy = e.arg_i64("DummyPolyID").unwrap_or(-1) as i16;
            if let Some(g) = dummies.and_then(|d| d.0.get(&dmy)).and_then(|&m| dummy_tf.get(m).ok()) {
                look_centre = Some(dummy_look(g));
                let pos = g.translation();
                if !orbit.cam.look_blocked {
                    let v = pos - target_focus;
                    let len = v.length();
                    orbit.cam.look_blocked = len > 0.0 && crate::terrain::raycast(target_focus, v / len, len).is_some();
                }
                if !orbit.cam.look_blocked {
                    target_focus = pos;
                    let r = f("unk1").min(1.0);
                    if r > 0.0 {
                        rate = Vec3::splat(r);
                    }
                }
            }
        }
        None => {
            orbit.cam.look_blocked = false;
            if orbit.cam.look_after.1 > 0.0 {
                rate = Vec3::splat(orbit.cam.look_after.1);
            }
        }
    }
    // The look limits, every frame (FUN_140742230, `look_limits_step`); they clamp the free
    // camera after the resync below.
    let lp = LookParams {
        pitch: (lf("rotRangeMinX", -40.0).to_radians(), cf("rotRangeXMax", 1.3963)),
        yaw: (cf("rotRangeYMin", -std::f32::consts::PI as f64), cf("rotRangeYMax", std::f32::consts::PI as f64)),
        base_y: cf("rotBaseY", 0.0),
    };
    {
        let (cam_yaw, chr_yaw) = (exe_yaw(orbit.yaw), exe_yaw(actor.yaw));
        let cam = &mut orbit.cam;
        let look = cam.look.get_or_insert([lp.pitch.0, lp.pitch.1, lp.yaw.0, lp.yaw.1]);
        // gap: Wolf's chr matrix row 2 is taken as his facing, level (only the 6 CameraParam rows
        // with a yaw range under the full circle read it; none was run in the emulator).
        look_limits_step(look, &mut cam.look_ref, look_limits, cam.look_blocked, look_centre, (0.0, chr_yaw), lp, cam_yaw, per_frame(lerp_rate, dt));
    }
    // The exe chases Wolf's position (+0x90) and adds the focus height (+0x204) after.
    let lift = Vec3::Y * orbit.cam.focus_y;
    let start = orbit.focus - lift;
    let chase = orbit.cam.focus_chase.get_or_insert_with(|| FocusChase::at(start));
    let chased = chase.update(target_focus - lift, rate, dt) + lift;
    // The focus pull-in (`focus_pull_in`): a chased focus that crossed a wall this frame is put
    // back on Wolf's side. Wolf's radius (FUN_1409efb80) is the proxy's.
    let root = player.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT + lift;
    let ray = |o: Vec3, v: Vec3| {
        let l = v.length();
        l > 0.0 && crate::terrain::raycast(o, v / l, l).is_some()
    };
    orbit.focus = focus_pull_in(orbit.focus, chased, root, crate::player::CAPSULE_RADIUS, ray, map_cast);
    let pulled = orbit.focus - lift;
    if let Some(chase) = orbit.cam.focus_chase.as_mut() {
        (chase.out.x, chase.out.z) = (pulled.x, pulled.z);
    }
    orbit.cam.look_after.0 -= dt;
    if orbit.cam.look_after.0 <= 0.0 {
        orbit.cam.look_after = (0.0, 0.0);
    }
    // The fulcrum (camera +0xe0; CameraParam row values, FulcrumParams). +0x194 (14073.c
    // 9305-9397): 0 while the camera is being turned, 1 when the turning stops (held for
    // transChaseByControlEndWaitTime, then falling over transChaseByControlEndReleaseTime), 1
    // while locked on. gap: the exe's second "turning" input (FUN_1409efea0 +0xd0) is not known;
    // the camera's own angle change stands for both.
    let fp = FulcrumParams {
        rate_xz: cf("targetTransChaseRateXZ", 0.4),
        rate_y: cf("targetTransChaseRateY", 0.4),
        lr_max_dist: cf("fulcrumDistRateByLrMoveMaxDist", 0.3),
        lr_min: cf("fulcrumDistRateByLrMoveMin", 0.1),
        lr_max: cf("fulcrumDistRateByLrMoveMax", 0.99),
        dist_min: cf("fulcrumDistMin", 2.0),
    };
    if lock.target.is_some() {
        orbit.cam.trans_snap = 1.0;
        orbit.cam.trans_hold = cf("transChaseByControlEndWaitTime", 0.0);
    } else if turning.is_some() {
        orbit.cam.trans_snap = 0.0;
        orbit.cam.trans_hold = -1.0;
    } else if orbit.cam.trans_hold < 0.0 {
        orbit.cam.trans_snap = 1.0;
        orbit.cam.trans_hold = cf("transChaseByControlEndWaitTime", 0.0);
    } else {
        orbit.cam.trans_hold -= dt;
        if orbit.cam.trans_hold < 0.0 {
            orbit.cam.trans_hold = 0.0;
            orbit.cam.trans_snap = (orbit.cam.trans_snap - dt / cf("transChaseByControlEndReleaseTime", 1.0).max(1e-3)).max(0.0);
        }
    }
    let moved = orbit.cam.fulcrum.map_or(Vec3::ZERO, |(_, last)| orbit.focus - last);
    // The resync runs under TAE 151 too (the unlocked branch, 14073.c 9832, has no test for it):
    // Gyoubu's deathblow then swings up over Wolf's shoulder as in the clip (2026-10-17).
    if let Some(lp) = lock_point {
        // The lock chase (`lock_chase`), from the dragged fulcrum (before the first placement:
        // the arm at the camera's distance). The free pitch limits it widens toward are
        // +0x230 / +0x234: the TAE 151 ones while they run.
        let fulcrum = match orbit.cam.fulcrum {
            Some((f, _)) => drag_fulcrum(f, moved, orbit.cam.trans_snap, fp),
            None => orbit.focus + Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0) * Vec3::Z * orbit.cam.dist,
        };
        let free = orbit.cam.look.map_or(pitch_range(&combat, false), |l| (l[0], l[1]));
        let lp_params = LockChaseParams {
            tgt_pos_rate: cf("lockTgtPosRate", 0.7),
            rate_x: cf("lockRotChaseRateX", 0.6),
            rate_y: cf("lockRotChaseRateY", 0.3),
            play_x: cf("lockRotChasePlayAngX", 0.0),
            play_y: cf("lockRotChasePlayAngY", 0.0),
            at_lock: range,
            lerp_height: (cf("rotRangeLerpBeginHeight", 0.5), cf("rotRangeLerpEndHeight", 3.0)),
        };
        let half = orbit.cam.fov * lf("lockRotXShiftRatio", 0.45) * 0.5;
        (orbit.pitch, orbit.yaw) = lock_chase(orbit.focus, fulcrum, lp, half, free, lp_params, dt);
    } else if let (Some((f, _)), None) = (orbit.cam.fulcrum, lock.target) {
        // The free camera's yaw from the dragged fulcrum (FUN_140734070 of focus - fulcrum), plus
        // this frame's turning (the exe turns after the resync).
        let v = drag_fulcrum(f, moved, orbit.cam.trans_snap, fp) - orbit.focus;
        if v.with_y(0.0).length_squared() > 1e-6 {
            // The whole swing, no factor: the exe's own loop (chr frame, then this update) with
            // the move stick held sideways circles Wolf at about 63 deg/s at a run (5.6 m/s) and
            // 105 at a sprint (8.8 m/s), +0x194 at 0 all along (emu-cases `camswing`, sim test
            // `free_camera_swing_on_a_sideways_run_matches_the_exe`; config follow_swing 0.5
            // halved it before).
            orbit.yaw = f32::atan2(v.x, v.z) + turning.map_or(0.0, |d| d.0);
            // The pitch too (FUN_140733e50; 14073.c 9840-9880) when `chases_pitch`: a climb
            // or a fall leaves the fulcrum behind in height, which tilts the camera.
            let stick = pad.as_ref().map_or(Vec2::ZERO, |p| p.stick);
            if chases_pitch(cf("isEnableChrChaseRotX", 1.0) != 0.0, cf("chrChaseRotXInvalidAngle", 1.3963), stick) {
                let (lo, hi) = pitch_range(&combat, false);
                orbit.pitch = (f32::atan2(v.y, v.with_y(0.0).length()) + turning.map_or(0.0, |d| d.1)).clamp(lo, hi);
            }
        }
    }
    // The free camera's turned angles go into the look limits (14073.c 10330-10500: the
    // unlocked branch only, after the resync; TAE 156 below is not clamped).
    if let (Some(l), None) = (orbit.cam.look, lock.target) {
        (orbit.yaw, orbit.pitch) = apply_look_limits(orbit.yaw, orbit.pitch, &l);
    }
    match set_angle {
        Some(e) => {
            let id = (actor.anim.clone(), e.start);
            if orbit.cam.angle_from.as_ref().is_none_or(|(k, _, _)| *k != id) {
                orbit.cam.angle_from = Some((id, (orbit.yaw, orbit.pitch), actor.yaw));
            }
            let (from, chr_yaw) = orbit.cam.angle_from.as_ref().map_or(((orbit.yaw, orbit.pitch), actor.yaw), |f| (f.1, f.2));
            let arg = |k: &str| e.arg_i64(k).unwrap_or(0) as f32;
            let on = |k: &str| e.args.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            let interp = e.args.get("InterpolationType").and_then(|v| v.as_str()).and_then(|s| s.split(':').next()?.trim().parse().ok()).or(e.arg_i64("InterpolationType")).unwrap_or(0);
            let p = (actor.t - e.start) / (e.end - e.start).max(1e-4);
            let (yaw, pitch) = set_camera_angle(
                from,
                chr_yaw,
                (on("OverrideHorizontal"), arg("HorizontalAngle").to_radians()),
                (on("OverrideVertical"), arg("VerticalAngle").to_radians()),
                interp,
                p,
            );
            orbit.yaw = yaw;
            orbit.pitch = pitch;
            // +0x150 = 1, +0x154 = 0: the position chase snaps to the new angle.
            orbit.cam.ctrl_snap = 1.0;
            orbit.cam.ctrl_hold = 0.0;
        }
        None => orbit.cam.angle_from = None,
    }
    let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
    // camCastSphereRadius (0.05 m): the radius of every collision cast below (`map_cast`).
    let radius = cf("camCastSphereRadius", 0.05);
    let escape = config.as_ref().is_some_and(|c| c.input.escape_wall);
    let fulcrum = {
        let arm = rotation * Vec3::Z;
        let cam_dist = if orbit.cam.pos.is_some() { (transform.translation - orbit.focus).length() } else { orbit.cam.dist };
        // The right axis of last frame's camera (+0x10, the final matrix), not this frame's
        // angles (exe FUN_14073c260; checked on the emulated run, test
        // `follow_camera_matches_the_exe`).
        let right = transform.rotation * Vec3::X;
        let sideways = (moved.x * right.x + moved.z * right.z).abs();
        let f = if orbit.cam.fulcrum.is_some() {
            place_fulcrum(orbit.focus, arm, cam_dist, sideways, dt, fp)
        } else {
            // First frame (14073.c 9262-9285): fulcrumDistRate of the camera distance, at least
            // fulcrumDistMin.
            orbit.focus + arm * (cam_dist * cf("fulcrumDistRate", 0.5)).max(fp.dist_min)
        };
        // The fulcrum cast (14073.c 10993-11048, skipped with the escape-wall option on): a
        // wall or the ground between the focus and the fulcrum pulls the fulcrum in to it (1 cm
        // at least), so a dragged fulcrum near a wall swings the camera faster.
        let f = if escape { f } else { cast_cut(orbit.focus, f - orbit.focus, map_cast(orbit.focus, f - orbit.focus, radius), FULCRUM_CUT_MIN) };
        orbit.cam.fulcrum = Some((f, orbit.focus));
        f
    };
    // TAE 152: the angles from the node, the fulcrum on its line (`attach_to_node`).
    let fulcrum = match attach_node {
        Some(node) => {
            let (yaw, pitch, f) = attach_to_node(orbit.focus, fulcrum, node);
            (orbit.yaw, orbit.pitch) = (yaw, pitch);
            orbit.cam.fulcrum = Some((f, orbit.focus));
            f
        }
        None => fulcrum,
    };
    // The orbit point (+0xf0, 14073.c 11066-11085): the camera distance (+0x214) along the
    // fulcrum's direction; then the orbit cast (14073.c 11196-11251) pulls it in to a wall or
    // the ground between it and the focus, so the position chase below runs toward the cut
    // point, not through the wall.
    let arm_dir = (fulcrum - orbit.focus).try_normalize().unwrap_or(rotation * Vec3::Z);
    let raw = attach_node.unwrap_or(orbit.focus + arm_dir * orbit.cam.dist);
    let raw = if escape { raw } else { cast_cut(orbit.focus, raw - orbit.focus, map_cast(orbit.focus, raw - orbit.focus, radius), CAST_CUT_MIN) };
    let locked = lock.target.is_some();
    // The escape-wall option (14073.c 11331-11766): two short wings from Wolf's point (+0xa0)
    // along the arm (`escape_wings`); a hit sets the working rates to the row's _ForEscapeWall
    // values, else they ease toward the row (`working_rates_step`). gap: the yaw turn away from
    // the wall (+0x334, escapeWallRotAdd* / WeakRotAdd* / RotMax*) is not ported: 0 in all 142
    // CameraParam rows, so it never turns.
    let wing_hit = escape
        && escape_wings(root, raw, cf("escapeWallWingAngY", 1.745e-4), cf("escapeWallWingLen", 0.01))
            .iter()
            .any(|&(o, v)| map_cast(o, v, cf("escapeWallWingRadius", 0.01)).is_some());
    let escape_rates = Vec3::new(cf("chrTransChaseRateXZ_ForEscapeWall", 0.15), cf("chrTransChaseRateY_ForEscapeWall", 0.3), cf("targetChaseRateXZForEscapeWall", 0.1));
    let rates = working_rates_step(rates, row_rates, wing_hit.then_some(escape_rates), dt);
    orbit.cam.rates = Some(rates);
    // The distance chase rates after a hit of the final cast (+0x1e4 / +0x1e8 / +0x1ec,
    // `hit_wall_rates`).
    let (dist_rate, hit_time) = hit_wall_rates(
        orbit.cam.wall.hit_time,
        cf("targetChaseDistRateTime", 2.0),
        Vec2::new(cf("targetChaseDistRateXZ", 0.4), cf("targetChaseDistRateY", 0.4)),
        Vec2::new(cf("targetChaseDistRateXZ_ForHitWall", 1.0), cf("targetChaseDistRateY_ForHitWall", 1.0)),
        dt,
    );
    orbit.cam.wall.hit_time = hit_time;
    // TAE 152's unk2 (singleton +0x6c, FUN_1408311a0) replaces both distance rates while set.
    let dist_rate = if orbit.cam.attach_after.1 > 0.0 { Vec2::splat(orbit.cam.attach_after.1) } else { dist_rate };
    let chase = if locked {
        PosChase {
            xz: cf("targetChaseRateAtLockXZ", 0.4),
            y: cf("targetChaseRateAtLockY", 0.3),
            ang_min: cf("targetChaseRateAngAtLockMin", 0.0),
            ang_max: cf("targetChaseRateAngAtLockMax", 0.0),
            dist_xz: dist_rate.x,
            dist_y: dist_rate.y,
        }
    } else {
        // The working X / Z rate +0x228 (`working_rates_step`, toward targetChaseRateXZForNormal).
        PosChase {
            xz: rates.z,
            y: cf("targetChaseRateY", 0.4),
            ang_min: cf("targetChaseRateAngMin", 0.1745),
            ang_max: cf("targetChaseRateAngMax", 0.3491),
            dist_xz: dist_rate.x,
            dist_y: dist_rate.y,
        }
    };
    // +0x230 / +0x234: the look limits (`look_limits_step`).
    let pitch_lim = orbit.cam.look.map_or((lf("rotRangeMinX", -40.0).to_radians(), cf("rotRangeXMax", 1.3963)), |l| (l[0], l[1]));
    let pos = match orbit.cam.pos {
        Some(prev) => {
            let last = orbit.cam.wall.post.unwrap_or(prev);
            match attach {
                Some((_, blend)) if blend > 0.0 => chase_position_attached(prev, (last - orbit.focus).length(), raw, orbit.focus, blend, (chase.dist_xz, chase.dist_y), orbit.cam.ctrl_snap, dt),
                _ => chase_position_from(prev, (last - orbit.focus).length(), raw, orbit.focus, chase, Some(pitch_lim), orbit.cam.ctrl_snap, dt),
            }
        }
        None => raw,
    };
    // FUN_140838b50 after the camera frame: +0x68 counts down, and +0x6c goes with it at 0.
    orbit.cam.attach_after.0 -= dt;
    if orbit.cam.attach_after.0 <= 0.0 {
        orbit.cam.attach_after = (0.0, 0.0);
    }
    orbit.cam.pos = Some(pos);
    orbit.cam.seen = Some((orbit.yaw, orbit.pitch));
    let arm = (pos - orbit.focus).try_normalize().unwrap_or(rotation * Vec3::Z);
    // The final cast (14073.c 12318-12411) from the focus to the chased position: the camera
    // stops at the hit, max(hit, 1e-5) of the arm (no minimum distance: Wolf fades instead,
    // `cam_fade_step`), and the hit starts the hit-wall clock. +0x110 keeps the result.
    let hit = map_cast(orbit.focus, pos - orbit.focus, radius);
    if hit.is_some() {
        orbit.cam.wall.hit_time = cf("targetChaseDistRateTime", 2.0);

    }
    let post = cast_cut(orbit.focus, pos - orbit.focus, hit, CAST_CUT_MIN);
    orbit.cam.wall.post = Some(post);
    let dist = safe_margin((post - orbit.focus).length(), cf("camSafeMarginRate", 0.1), cf("camSafeMarginMax", 0.3));
    transform.translation = orbit.focus + arm * dist;
    transform.look_at(orbit.focus, Vec3::Y);
    // CameraParam playerFadeDist (0.5 m) from the camera to Wolf's chased point (camera +0xa0).
    // gap: +0xa0 is taken as the focus. The obstacle fade (14073.c 11253-11320: a
    // camCastSphereRadiusForFade 0.3 m cast from last frame's +0x100 toward Wolf fades what it
    // hits to alpha 0.2) is not ported: in the emulated runs it hit only map objects (kind 6;
    // RSekiro draws them merged into the map), and no enemy of the snapshot passes its
    // character test.
    orbit.cam.near = dist < cf("playerFadeDist", 0.5);
}

/// Wolf's CamFade alpha (`cam_fade_step`) onto his actor (model.rs update_opacity draws it).
fn fade_player(time: Res<Time>, mut camera: Single<&mut OrbitCamera>, mut wolf: Single<&mut Actor, With<Player>>) {
    let cam = &mut camera.cam;
    cam.alpha = cam_fade_step(cam.alpha, cam.near, time.delta_secs());
    wolf.cam_alpha = cam.alpha;
}

/// TAE 155 SetLockParamID (exe FUN_140b51420 -> FUN_140737430: slot 1..45 written every
/// frame the event runs, reset to 1 after each camera update in FUN_1407374e0): the slot this
/// frame asks for.
fn tae_cam_slot(combat: &Combat, actor: &Actor) -> i64 {
    if actor.anim.is_empty() {
        return 1;
    }
    combat
        .player
        .events_at(&actor.anim, actor.step())
        .find(|e| e.kind == 155)
        .and_then(|e| e.arg_i64("LockParamID"))
        .filter(|&id| id > 0 && id < 0x2e)
        .unwrap_or(1)
}

fn update_cam_slot(combat: &Combat, actor: &Actor, cam: &mut CamState, dt: f32) {
    cam_slot_step(combat, cam, tae_cam_slot(combat, actor), dt);
}

/// The slot switch of FUN_1407374e0 (CameraSetParam manager DAT_143d59938, before the camera
/// reads its blended CameraParam): when the slot asked for (`want`) differs from +0x3ec, the
/// blended CameraParam as it is now is kept (+8 -> +0x1e8, `cam_from`), the blend length +0x3f0
/// becomes the new slot's CameraSetParam beginTime (the old slot's endTime when going back to
/// slot 1) and the clock +0x3f4 restarts at 0; the clock then advances by dt while below the
/// length, clamped to it, before the weight is taken (`CamBlend`). The LockCamParam id (+0x3cc)
/// is the new slot's at once (no blend: the camera chases its values, follow_player).
pub(crate) fn cam_slot_step(combat: &Combat, cam: &mut CamState, want: i64, dt: f32) {
    if want != cam.slot {
        let from = cam.blend().all(combat);
        cam.blend_len = if want == 1 { cam_slot(combat, cam.slot).3 } else { cam_slot(combat, want).2 };
        cam.cam_from = std::sync::Arc::new(from);
        cam.slot = want;
        cam.blend_t = 0.0;
    }
    if cam.blend_t < cam.blend_len {
        cam.blend_t = (cam.blend_t + dt).min(cam.blend_len);
    }
}

/// The CameraParam the camera reads this frame (CameraSetParam manager +8): the slot's
/// camParamId row blended from `from` (+0x1e8) by FUN_1407381d0 at p = clock / length
/// (1 for a length under 1.2e-7).
#[derive(Clone)]
pub(crate) struct CamBlend {
    slot: i64,
    p: f32,
    from: std::sync::Arc<std::collections::HashMap<String, f32>>,
}

/// A param value as a number (the export keeps the b32 flags as booleans).
fn param_num(v: &serde_json::Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32).or_else(|| v.as_bool().map(|b| b as u8 as f32))
}

/// FUN_1407381d0 for one CameraParam field: at p <= 1.2e-7 the whole row is `from`; otherwise
/// w = 1 - (p - 1)^2 and each field is (1 - w) * from + w * to, but the three isEnable* flags
/// are rounded ((int)(... + 0.5)) and rotBaseY / resetBaseY take the new row's value.
pub(crate) fn blend_field(k: &str, from: Option<f32>, to: f32, p: f32) -> f32 {
    let Some(from) = from else { return to };
    if p <= 1.192_092_9e-7 {
        return from;
    }
    let w = 1.0 - (p - 1.0) * (p - 1.0);
    match k {
        "rotBaseY" | "resetBaseY" => to,
        _ if k.starts_with("isEnable") => ((1.0 - w) * from.trunc() + w * to.trunc() + 0.5).trunc(),
        _ => (1.0 - w) * from + w * to,
    }
}

impl CamBlend {
    fn row<'a>(&self, combat: &'a Combat) -> &'a serde_json::Value {
        let r = combat.param("CameraParam", cam_slot(combat, self.slot).1);
        if r.is_null() { combat.param("CameraParam", 0) } else { r }
    }

    pub(crate) fn get(&self, combat: &Combat, k: &str, d: f64) -> f32 {
        let to = param_num(&self.row(combat)[k]).unwrap_or(d as f32);
        blend_field(k, self.from.get(k).copied(), to, self.p)
    }

    /// Every field, for the next switch's `from`.
    fn all(&self, combat: &Combat) -> std::collections::HashMap<String, f32> {
        let row = self.row(combat).as_object().into_iter().flatten();
        row.filter_map(|(k, v)| Some((k.clone(), blend_field(k, self.from.get(k).copied(), param_num(v)?, self.p)))).collect()
    }
}

impl CamState {
    pub(crate) fn blend(&self) -> CamBlend {
        let p = if self.blend_len > 1.192_092_9e-7 { (self.blend_t / self.blend_len).clamp(0.0, 1.0) } else { 1.0 };
        CamBlend { slot: self.slot, p, from: self.cam_from.clone() }
    }

    /// The LockCamParam row of the slot (+0x3cc; row 0 when missing).
    pub(crate) fn lock_row<'a>(&self, combat: &'a Combat) -> &'a serde_json::Value {
        let r = combat.param("LockCamParam", cam_slot(combat, self.slot).0);
        if r.is_null() { combat.param("LockCamParam", 0) } else { r }
    }
}

/// TAE 153 CameraModule3 as its handler (FUN_140b51680 -> FUN_140832810 / 830 / 7f0) leaves the
/// camera singleton DAT_143d5c0d0 this frame: +0x70 CamDistTargetOverride (`target`; < 0 =
/// none), +0x74 StartInterpolationSpeed, +0x78 the event's progress p and +0x84 its first frame
/// (SlowStart only), +0x7c TransitionKeyframe and +0x80 EndInterpolationSpeed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DistEvent {
    pub target: f32,
    pub start_rate: f32,
    pub slow: Option<(f32, bool)>,
    pub transition: f32,
    pub end_rate: f32,
}

/// The camera distance one frame on (FUN_14073c260, 14073.c 9083-9107; per-frame rates, ported
/// with `per_frame`):
/// - target LockCamParam camDistTarget (`param_dist`) at CameraParam.lockCamParamLerpRate
///   (`param_rate`), or at EndInterpolationSpeed while its TransitionKeyframe time lasts
///   (singleton +0x80 / +0x7c: set each frame of the event, counted down by dt after the camera
///   and cleared together at 0 by FUN_140838b50);
/// - a 153 override (>= 0) is the target instead, at StartInterpolationSpeed (the param rate
///   when <= 0);
/// - SlowStart: from + (target - from) * (1 - (p - 1)^2), `from` (+0x21c) the distance at the
///   event's first frame.
/// The distance used (+0x214) is set to this one (+0x218) each frame (14073.c 9751): no smoothing.
pub(crate) fn cam_distance_step(cam: &mut CamState, ev: Option<DistEvent>, param_dist: f32, param_rate: f32, dt: f32) {
    if let Some(e) = ev {
        cam.end_time = e.transition;
        cam.end_rate = e.end_rate;
    }
    let mut target = param_dist;
    let mut rate = if cam.end_rate > 0.0 { cam.end_rate } else { param_rate };
    if let Some(e) = ev.filter(|e| e.target >= 0.0) {
        target = e.target;
        rate = if e.start_rate > 0.0 { e.start_rate } else { param_rate };
    }
    match ev.and_then(|e| e.slow) {
        Some((p, first)) => {
            if first {
                cam.dist_from = cam.dist_raw;
            }
            cam.dist_raw = cam.dist_from + (target - cam.dist_from) * (1.0 - (p - 1.0) * (p - 1.0));
        }
        None => cam.dist_raw += (target - cam.dist_raw) * per_frame(rate, dt),
    }
    cam.dist = cam.dist_raw;
    cam.end_time -= dt;
    if cam.end_time <= 0.0 {
        cam.end_time = 0.0;
        cam.end_rate = 0.0;
    }
}

/// `cam_distance_step` with Wolf's TAE 153 at his animation time. SlowEnd (+0x85, read by
/// FUN_140831e70 at the end of FUN_14073c260) only copies the final camera matrix to ChrCam
/// +0x350.. (with +0x348 set) for a reader outside the camera; with it on or off every camera
/// field the update moves is the same (emu-cases `camleft`, sim test
/// `camera_leftovers_match_the_exe`), so it is not read here.
pub(crate) fn update_cam_distance(combat: &Combat, actor: &Actor, cam: &mut CamState, param_dist: f32, param_rate: f32, dt: f32) {
    let ev = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.step()).find(|e| e.kind == 153) };
    let ev = ev.map(|e| {
        let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let id = (actor.anim.clone(), e.start);
        let first = cam.last_153.as_ref() != Some(&id);
        cam.last_153 = Some(id);
        let slow = e.args.get("SlowStart").and_then(|v| v.as_bool()).unwrap_or(false);
        let p = ((actor.t - e.start) / (e.end - e.start).max(1e-4)).clamp(0.0, 1.0);
        DistEvent {
            target: f("CamDistTargetOverride"),
            start_rate: if slow { 0.0 } else { f("StartInterpolationSpeed") },
            slow: slow.then_some((p, first)),
            transition: f("TransitionKeyframe"),
            end_rate: f("EndInterpolationSpeed"),
        }
    });
    if ev.is_none() {
        cam.last_153 = None;
    }
    cam_distance_step(cam, ev, param_dist, param_rate, dt);
}

/// Active camera shakes: (RumbleCam id, time, gain).
#[derive(Resource, Default)]
pub(crate) struct CameraShake(pub(crate) Vec<(String, f32, f32)>);

/// TAE RumbleCam events: 145/146 global (the player's own actions), 144/147 local with
/// a distance falloff (FalloffStart..FalloffEnd metres) from the emitting character.
/// The shakes themselves are other/default.rumblebnd camera_NNN.hkx (Havok anims).
fn start_shakes(
    combat: Res<Combat>,
    mut shake: ResMut<CameraShake>,
    actors: Query<(&crate::actor::Actor, &Transform)>,
    player: Query<&Transform, With<Player>>,
) {
    let me = player.iter().next().map(|t| t.translation).unwrap_or_default();
    for (a, tf) in &actors {
        if a.anim.is_empty() || a.t == a.prev_t {
            continue;
        }
        let d = crate::actor::data_for(&combat, &a);
        let Some(anim) = d.anim(&a.anim) else { continue };
        for e in anim.events.iter().filter(|e| matches!(e.kind, 144..=147)) {
            // An event at 0 fires in the anim's first step (`tae_window::starts`).
            if !crate::tae_window::starts(e.start, a.prev_t, a.t) {
                continue;
            }
            let Some(id) = e.arg_i64("RumbleCamID") else { continue };
            let gain = if matches!(e.kind, 145 | 146) {
                1.0
            } else {
                let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let (s0, s1) = (f("FalloffStart"), f("FalloffEnd").max(f("FalloffStart") + 0.01));
                let dist = tf.translation.distance(me);
                (1.0 - (dist - s0) / (s1 - s0)).clamp(0.0, 1.0)
            };
            if gain > 0.0 {
                shake.0.push((id.to_string(), 0.0, gain));
            }
        }
    }
}

pub(crate) fn apply_shakes(time: Res<Time>, combat: Res<Combat>, mut shake: ResMut<CameraShake>, mut cam: Query<&mut Transform, With<OrbitCamera>>) {
    let dt = time.delta_secs();
    let mut delta = Quat::IDENTITY;
    shake.0.retain_mut(|(id, t, gain)| {
        *t += dt;
        let Some(r) = combat.rumble.get(id.as_str()) else { return false };
        if *t >= r.duration || r.frames.is_empty() {
            return false;
        }
        let f = (*t / r.duration * (r.frames.len() - 1) as f32).clamp(0.0, (r.frames.len() - 1) as f32);
        let (i0, k) = (f.floor() as usize, f.fract());
        let i1 = (i0 + 1).min(r.frames.len() - 1);
        // Havok -> game (mirror X), relative to the first frame (the camera bone's base pose).
        let q = |i: usize| {
            let v = r.frames[i];
            Quat::from_xyzw(v[3], -v[4], -v[5], v[6]).normalize()
        };
        let base = q(0);
        let cur = q(i0).slerp(q(i1), k);
        let rel = base.inverse() * cur;
        delta *= Quat::IDENTITY.slerp(rel, *gain);
        true
    });
    if delta != Quat::IDENTITY {
        for mut tf in &mut cam {
            tf.rotation *= delta;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: PosChase = PosChase { xz: 0.4, y: 0.3, ang_min: 0.0, ang_max: 0.0, dist_xz: 0.4, dist_y: 0.4 };
    const FREE: PosChase = PosChase { xz: 0.1, y: 0.4, ang_min: 0.1745, ang_max: 0.3491, dist_xz: 0.4, dist_y: 0.4 };

    /// The fulcrum: standing still it keeps the yaw; a sideways run (6 m/s across the view)
    /// leaves it behind, so the yaw read back from it turns toward the run, and the turn stops
    /// once the run is along the view.
    #[test]
    fn fulcrum_swings_the_camera_toward_a_sideways_run() {
        let p = FulcrumParams { rate_xz: 0.4, rate_y: 0.4, lr_max_dist: 0.3, lr_min: 0.1, lr_max: 0.99, dist_min: 2.0 };
        let dt = 1.0 / 60.0;
        let yaw_of = |f: Vec3, focus: Vec3| {
            let v = f - focus;
            f32::atan2(v.x, v.z)
        };
        // Camera behind on +Z, focus at the origin.
        let mut focus = Vec3::ZERO;
        let mut yaw = 0.0f32;
        let rot = |yaw: f32| Quat::from_rotation_y(yaw);
        let mut f = place_fulcrum(focus, rot(yaw) * Vec3::Z, 4.5, 0.0, dt, p);
        // Still: nothing moves.
        let g = drag_fulcrum(f, Vec3::ZERO, 0.0, p);
        assert!((yaw_of(g, focus) - yaw).abs() < 1e-6);
        // Running along +X (sideways) for a second.
        for _ in 0..60 {
            let moved = Vec3::X * 0.1;
            let g = drag_fulcrum(f, moved, 0.0, p);
            focus += moved;
            yaw = yaw_of(g, focus);
            let right = rot(yaw) * Vec3::X;
            f = place_fulcrum(focus, rot(yaw) * Vec3::Z, 4.5, moved.dot(right).abs(), dt, p);
        }
        // The arm swung toward -X (the camera trails the run): the view turns to +X.
        assert!(yaw < -0.5, "{yaw}");
        // With the full snap (camera just turned by hand) the fulcrum moves with the focus.
        let g = drag_fulcrum(f, Vec3::X, 1.0, p);
        assert!((yaw_of(g, focus + Vec3::X) - yaw).abs() < 1e-5);
    }

    /// CamFade: 0.4 s down to nothing while the camera is close, 0.2 s back; the safe margin
    /// takes 10 % off a short arm and at most 0.3 m off a long one.
    #[test]
    fn wolf_fades_when_the_camera_is_close() {
        let dt = 0.1;
        let mut a = 1.0;
        for _ in 0..4 {
            a = cam_fade_step(a, true, dt);
        }
        assert!(a.abs() < 1e-5, "{a}");
        a = cam_fade_step(a, false, dt);
        a = cam_fade_step(a, false, dt);
        assert!((a - 1.0).abs() < 1e-5, "{a}");
        assert!((safe_margin(1.0, 0.1, 0.3) - 0.900001).abs() < 1e-5);
        assert!((safe_margin(4.5, 0.1, 0.3) - 4.2).abs() < 1e-5);
    }

    #[test]
    fn set_camera_angle_eases_out_to_the_event_angle() {
        // Gyoubu's deathblow (a225_510000, 1.5-1.967 s): VerticalAngle -20, OverrideHorizontal off.
        let from = (0.3, 0.2);
        let v = (true, (-20f32).to_radians());
        assert!((set_camera_angle(from, 1.0, (false, 0.9), v, 0, 0.0).1 - from.1).abs() < 1e-5);
        let (y, p) = set_camera_angle(from, 1.0, (false, 0.9), v, 0, 0.5);
        assert_eq!(y, 0.3);
        assert!((p - (0.2 + (v.1 - 0.2) * 0.75)).abs() < 1e-5);
        assert!((set_camera_angle(from, 1.0, (false, 0.9), v, 0, 2.0).1 - v.1).abs() < 1e-5);
        // HorizontalAngle 180 puts the camera in front of Wolf, the short way round.
        let (y, _) = set_camera_angle((0.1, 0.0), 0.0, (true, std::f32::consts::PI), (false, 0.0), 0, 1.0);
        assert!((y.abs() - std::f32::consts::PI).abs() < 1e-4);
        // "Clockwise" goes the way the game's angle grows: ours falls.
        let (y, _) = set_camera_angle((0.0, 0.0), 0.0, (true, -1.0), (false, 0.0), 1, 0.1);
        assert!(y < 0.0, "{y}");
    }

    /// The pitch chase: on with the stick let go or pushed forward / sideways; off while the
    /// stick points back at the camera (within 80 deg, at more than cos 80 = 0.17 of it), or when
    /// the row disables it.
    #[test]
    fn pitch_chase_skips_a_run_toward_the_camera() {
        let a = 80f32.to_radians();
        assert!(chases_pitch(true, a, Vec2::ZERO));
        assert!(chases_pitch(true, a, Vec2::Y));
        assert!(chases_pitch(true, a, Vec2::X));
        assert!(!chases_pitch(true, a, -Vec2::Y));
        assert!(!chases_pitch(true, a, Vec2::new(0.7, -0.7)));
        assert!(chases_pitch(true, a, Vec2::new(0.0, -0.1)));
        assert!(!chases_pitch(false, a, Vec2::ZERO));
        assert!(chases_pitch(true, 0.0, -Vec2::Y));
    }

    #[test]
    fn camera_position_chases_as_the_exe() {
        let focus = Vec3::ZERO;
        let raw = Vec3::new(0.0, 1.0, 4.5);
        let side = Quat::from_rotation_y(0.3) * raw;
        // Locked on: one 60 fps frame closes 0.4 of the X / Z gap.
        let p = chase_position(side, raw, focus, LOCK, None, 0.0, 1.0 / 60.0);
        assert!((p - side).length() > 0.1 && (p - raw).length() < (side - raw).length());
        // Free camera: inside targetChaseRateAngMin (10 deg) X / Z holds; the distance stays.
        let near = Quat::from_rotation_y(0.1) * raw;
        let p = chase_position(near, raw, focus, FREE, None, 0.0, 1.0 / 60.0);
        assert!((p.with_y(0.0).normalize() - near.with_y(0.0).normalize()).length() < 1e-4);
        assert!((p.length() - raw.length()).abs() < 1e-3);
        // Camera control snaps it.
        let p = chase_position(near, raw, focus, FREE, None, 1.0, 1.0 / 60.0);
        assert!((p - raw).length() < 1e-4);
        // The distance chases at 0.4.
        let far = raw * 2.0;
        let p = chase_position(far, raw, focus, LOCK, None, 0.0, 1.0 / 60.0);
        assert!((p.length() - (far.length() + (raw.length() - far.length()) * 0.4)).abs() < 1e-3);
        // Both above the top pitch limit: X / Z goes straight to raw even inside AngMin; inside
        // the limits it holds as before.
        let (high, high_near) = (Vec3::new(0.0, 4.0, 1.0), Quat::from_rotation_y(0.1) * Vec3::new(0.0, 4.0, 1.0));
        let p = chase_position(high_near, high, focus, FREE, Some((-0.7, 0.5)), 0.0, 1.0 / 60.0);
        assert!((p.with_y(0.0) - high.with_y(0.0)).length() < 1e-4, "{p}");
        let p = chase_position(high_near, high, focus, FREE, Some((-0.7, 1.5)), 0.0, 1.0 / 60.0);
        assert!((p.with_y(0.0).normalize() - high_near.with_y(0.0).normalize()).length() < 1e-4);
    }
}

#[cfg(test)]
mod lock_sight_tests {
    use super::*;

    /// LockTgtMan: a held target out of sight is let go after 2 s (the ray every 0.5 s); one
    /// seen again in time keeps the lock.
    #[test]
    fn a_hidden_lock_target_is_let_go_after_two_seconds() {
        let dt = 1.0 / 60.0;
        let mut clock = (0.0, 0.0);
        let mut held = 0.0;
        while hold_hidden(&mut clock, dt, || false) {
            held += dt;
            assert!(held < 3.0);
        }
        assert!((1.4..=2.1).contains(&held), "held {held}");
        let mut clock = (0.0, 0.0);
        for i in 0..240 {
            assert!(hold_hidden(&mut clock, dt, || i > 60), "dropped at tick {i}");
        }
    }
}
