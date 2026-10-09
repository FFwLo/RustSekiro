//! Third-person camera. It orbits the player with the mouse, follows the
//! player, and turns toward the lock-on target (Q or middle mouse).
//!
//! Click in the window to capture the mouse; press Esc to release it.

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
    /// TAE 155 SetLockParamID slot (1 = default), the slot it is blending from, and the
    /// blend clock: CameraSetParam beginTime of the new slot (endTime of the old one when
    /// going back to 1).
    pub slot: i64,
    prev_slot: i64,
    blend_t: f32,
    blend_len: f32,
    /// +0x218 distance target being chased, +0x214 the smoothed distance used, +0x21c the
    /// distance a SlowStart override eases from, +0x80 EndInterpolationSpeed (kept after the
    /// event: the rate back to the param distance).
    dist_raw: f32,
    pub dist: f32,
    dist_from: f32,
    end_rate: f32,
    /// The 153 event seen last frame (anim key, start), to spot its first frame.
    last_153: Option<(String, f32)>,
    /// +0x50 FOV (radians) and +0x204 focus height, lerped to the LockCamParam row.
    fov: f32,
    focus_y: f32,
    /// TAE 151 CameraLookAtTarget look limits (exe camera +0x230..+0x23c):
    /// [pitch_min, pitch_max, yaw_min, yaw_max] radians, absolute, chasing the event at
    /// CameraParam.lockCamParamLerpRate. `None` while no look-at target is active.
    look: Option<[f32; 4]>,
}

impl CamState {
    pub(crate) fn for_tests(combat: &Combat) -> Self {
        Self::new(&lock_cam(combat))
    }

    fn new(lc: &LockCam) -> Self {
        Self {
            slot: 1,
            prev_slot: 1,
            blend_t: 0.0,
            blend_len: 0.0,
            dist_raw: lc.dist,
            dist: lc.dist,
            dist_from: lc.dist,
            end_rate: 0.0,
            last_153: None,
            fov: lc.fov_y.to_radians(),
            focus_y: lc.focus_y,
            look: None,
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

#[derive(Resource, Default)]
pub struct LockOn {
    pub target: Option<Entity>,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LockOn>()
            .add_systems(Startup, spawn_camera)
            .init_resource::<CameraShake>()
            .add_systems(Update, start_shakes)
            .add_systems(
                PostUpdate,
                (capture_mouse, toggle_lock, orbit_with_mouse, follow_player, apply_shakes).chain().before(TransformSystems::Propagate),
            );
    }
}

fn spawn_camera(mut commands: Commands, combat: Res<Combat>) {
    let lc = lock_cam(&combat);
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: lc.fov_y.to_radians(), ..default() }),
        OrbitCamera { yaw: 0.0, pitch: 15f32.to_radians(), focus: Vec3::ZERO, cam: CamState::new(&lc) },
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

fn toggle_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    combat: Res<Combat>,
    mut lock: ResMut<LockOn>,
    player: Single<&Transform, With<Player>>,
    enemies: Query<(Entity, &Transform), With<Enemy>>,
) {
    if let Some(t) = lock.target {
        let still_valid = enemies.get(t).is_ok_and(|(_, tf)| tf.translation.distance(player.translation) < lock_cam(&combat).lock_range);
        if !still_valid {
            lock.target = None;
        }
    }
    if keys.just_pressed(KeyCode::KeyQ) || mouse.just_pressed(MouseButton::Middle) {
        lock.target = match lock.target {
            Some(_) => None,
            None => enemies
                .iter()
                .filter(|(_, tf)| tf.translation.distance(player.translation) < lock_cam(&combat).lock_range)
                .min_by(|a, b| a.1.translation.distance(player.translation).total_cmp(&b.1.translation.distance(player.translation)))
                .map(|(e, _)| e),
        };
    }
}

/// Shortest signed difference `a - b` in radians (wraps through +-pi).
fn angle_delta(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Clamp an angle into [lo, hi], picking the representative of `x` (mod 2pi) nearest the
/// interval centre so a range that crosses +-pi still works (exe FUN_14073c260 yaw clamp).
fn clamp_angle(x: f32, lo: f32, hi: f32) -> f32 {
    // A full turn (e.g. LookLeft -180 / LookRight +180) is no limit at all.
    if (hi - lo).abs() >= std::f32::consts::TAU - 1e-3 {
        return x;
    }
    let span = angle_delta(hi, lo);
    let center = lo + span * 0.5;
    center + angle_delta(x, center).clamp(-span.abs() * 0.5, span.abs() * 0.5)
}

/// TAE 151 CameraLookAtTarget look limits. The handler FUN_140b517a0 writes four degrees into
/// the camera singleton (+0x4c LookUp, +0x50 LookDown, +0x54 LookLeft, +0x58 LookRight);
/// FUN_140742230 turns them into absolute limits around the DummyPolyID direction and stores
/// them at +0x230..+0x23c, chasing at CameraParam.lockCamParamLerpRate. Pitch is bounded by
/// LookUp/DownLimit and yaw by LookLeft/RightLimit (exe: +0x230..+0x23c clamp +0x170/+0x174).
/// `dir` is player -> dummy. Returns the clamped (yaw, pitch).
pub(crate) fn apply_look_limits(
    yaw: f32,
    pitch: f32,
    look: &mut Option<[f32; 4]>,
    dir: Vec3,
    angles: [f32; 4],
    rate: f32,
    dt: f32,
) -> (f32, f32) {
    // Center on the target: the lock-on chase yaw, and the lock-on pitch to frame it.
    // gap: the exe builds the centre from the DummyPoly transform basis (FUN_14073a1e0 then
    // FUN_140733d70/FUN_140733fb0), not from the player -> dummy vector we have here.
    let center_yaw = f32::atan2(-dir.x, -dir.z);
    let center_pitch = -f32::atan2(dir.y, dir.x.hypot(dir.z).max(1e-3));
    let target = [
        center_pitch + angles[0].to_radians(),
        center_pitch + angles[1].to_radians(),
        center_yaw + angles[2].to_radians(),
        center_yaw + angles[3].to_radians(),
    ];
    let step = 1.0 - (1.0 - rate.clamp(0.0, 1.0)).powf(dt * 30.0);
    // gap: the exe keeps +0x230..+0x23c across the whole session and always chases the param
    // defaults when no event runs; we start at the target and clear on event end.
    let cur = look.unwrap_or(target);
    let mut next = [0.0f32; 4];
    for i in 0..4 {
        next[i] = cur[i] + angle_delta(target[i], cur[i]) * step;
    }
    *look = Some(next);
    (clamp_angle(yaw, next[2], next[3]), clamp_angle(pitch, next[0], next[1]))
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
    let cam = &config.camera;
    let sensitivity = cam.mouse_sensitivity.to_radians();
    if lock.target.is_none() {
        camera.yaw -= motion.delta.x * sensitivity;
    }
    let (lo, hi) = pitch_range(&combat, lock.target.is_some());
    camera.pitch = (camera.pitch + motion.delta.y * sensitivity).clamp(lo, hi);
}

fn follow_player(
    time: Res<Time>,
    combat: Res<Combat>,
    lock: Res<LockOn>,
    player_q: Single<(&Transform, &Actor, Option<&crate::model::Dummies>), (With<Player>, Without<OrbitCamera>)>,
    dummy_tf: Query<&GlobalTransform>,
    enemies: Query<&Transform, (With<Enemy>, Without<OrbitCamera>)>,
    mut camera: Single<(&mut Transform, &mut OrbitCamera, &mut Projection)>,
) {
    let (player, actor, dummies) = *player_q;
    let (transform, orbit, projection) = &mut *camera;
    let dt = time.delta_secs();
    update_cam_slot(&combat, actor, &mut orbit.cam, dt);
    // The active CameraParam / LockCamParam rows, blended from the previous slot's.
    let (pl, pc, _, _) = cam_slot(&combat, orbit.cam.prev_slot);
    let (nl, nc, _, _) = cam_slot(&combat, orbit.cam.slot);
    let w = if orbit.cam.blend_len > 0.0 { (orbit.cam.blend_t / orbit.cam.blend_len).clamp(0.0, 1.0) } else { 1.0 };
    let blended = |table: &str, a: i64, b: i64, k: &str, d: f64| {
        let get = |row: i64| {
            let v = combat.param(table, row);
            if v.is_null() { combat.param(table, 0)[k].as_f64().unwrap_or(d) } else { v[k].as_f64().unwrap_or(d) }
        };
        (get(a) + (get(b) - get(a)) * w as f64) as f32
    };
    let cf = |k: &str, d: f64| blended("CameraParam", pc, nc, k, d);
    let lf = |k: &str, d: f64| blended("LockCamParam", pl, nl, k, d);
    // Lock-on chase (FUN_14073c260): each frame the angle moves diff * rate * (dt / (1/30)),
    // yaw at lockRotChaseRateY (0.3), pitch at lockRotChaseRateX (0.6).
    let step = |rate: f32| (rate * dt * 30.0).min(1.0);
    let lc = lock_cam(&combat);
    update_cam_distance(&combat, actor, &mut orbit.cam, lf("camDistTarget", 4.5), cf("lockCamParamLerpRate", 0.05), dt);
    // +0x50 FOV and +0x204 focus height chase the LockCamParam row at lockCamParamLerpRate.
    let lerp = 1.0 - (1.0 - cf("lockCamParamLerpRate", 0.05).clamp(0.0, 1.0)).powf(dt * 30.0);
    orbit.cam.fov += (lf("camFovY", 43.0).clamp(0.0, 180.0).to_radians() - orbit.cam.fov) * lerp;
    orbit.cam.focus_y += (lf("chrOrgOffset_Y", 1.5) - orbit.cam.focus_y) * lerp;
    if let Projection::Perspective(pp) = &mut **projection {
        pp.fov = orbit.cam.fov;
    }
    let mut range = pitch_range(&combat, lock.target.is_some());
    if let Some(target) = lock.target.and_then(|e| enemies.get(e).ok()) {
        let dir = (target.translation - player.translation).with_y(0.0);
        if dir.length_squared() > 0.01 {
            let want = f32::atan2(-dir.x, -dir.z);
            let diff = (want - orbit.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            orbit.yaw += diff * step(cf("lockRotChaseRateY", 0.3));
        }
        // Pitch: limits widen from rotRangeXAtLock toward the free range as the height gap goes
        // rotRangeLerpBeginHeight -> EndHeight; the target pitch is the elevation to the target
        // plus FOV * 0.5 * lockRotXShiftRatio (the target sits above centre). Our pitch is
        // positive looking down.
        let dy = target.translation.y - player.translation.y;
        let (b, e) = (cf("rotRangeLerpBeginHeight", 0.5), cf("rotRangeLerpEndHeight", 3.0));
        let t = ((dy.abs() - b) / (e - b).max(1e-3)).clamp(0.0, 1.0);
        let free = pitch_range(&combat, false);
        range = (range.0 + (free.0 - range.0) * t, range.1 + (free.1 - range.1) * t);
        let shift = lf("lockRotXShiftRatio", 0.45);
        let elev = f32::atan2(dy, dir.length().max(0.1));
        let want_pitch = (lc.fov_y.to_radians() * 0.5 * shift - elev).clamp(range.0, range.1);
        orbit.pitch += (want_pitch - orbit.pitch) * step(cf("lockRotChaseRateX", 0.6));
    }
    orbit.pitch = orbit.pitch.clamp(range.0, range.1);
    // chrOrgOffset_Y is measured from the feet; the capsule's origin is its middle.
    let feet = player.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    let target_focus = feet + Vec3::Y * orbit.cam.focus_y;
    // Focus follow (exe FUN_141155d20 from the ChrCam update FUN_14073c260): fixed 1/30 s steps,
    // each closing CameraParam chrTransChaseRateXZ_ForNormal (0.2) / _Y (0.3) of the gap.
    let step = |rate: f32| 1.0 - (1.0 - rate.clamp(0.0, 1.0)).powf(dt * 30.0);
    let (mut bxz, mut by) = (step(cf("chrTransChaseRateXZ_ForNormal", 0.2)), step(cf("chrTransChaseRateY_ForNormal", 0.3)));
    // TAE 151 CameraLookAtTarget (exe FUN_140b517a0 -> camera singleton +0x3c dummy, +0x40 rate;
    // FUN_14073c260): the focus chases that DummyPoly on Wolf at min(rate, 1) on every axis
    // (the normal rates when the rate is <= 0). Big deflects and the deathblow throws use dmy 142.
    let mut target_focus = target_focus;
    let look = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.t).find(|e| e.kind == 151) };
    // The look limits share the event's DummyPolyID: the four Look*Limit degrees bound the
    // camera pitch/yaw around the player -> dummy direction (exe FUN_140742230).
    let mut look_angles: Option<[f32; 4]> = None;
    let mut look_dir = Vec3::ZERO;
    if let Some(e) = look {
        let dmy = e.arg_i64("DummyPolyID").unwrap_or(-1);
        let pos = dummies.and_then(|d| d.0.get(&(dmy as i16))).and_then(|&m| dummy_tf.get(m).ok()).map(|g| g.translation());
        if let (true, Some(pos)) = (dmy >= 0, pos) {
            target_focus = pos;
            look_dir = pos - player.translation;
            let f = |k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            look_angles = Some([f("LookUpLimit"), f("LookDownLimit"), f("LookLeftLimit"), f("LookRightLimit")]);
            let rate = e.args.get("unk1").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            if rate > 0.0 {
                bxz = step(rate.min(1.0));
                by = bxz;
            }
        }
    }
    if let Some(angles) = look_angles {
        let (yaw, pitch) = apply_look_limits(orbit.yaw, orbit.pitch, &mut orbit.cam.look, look_dir, angles, cf("lockCamParamLerpRate", 0.05), dt);
        orbit.yaw = yaw;
        orbit.pitch = pitch;
    } else {
        orbit.cam.look = None;
    }
    orbit.focus.x += (target_focus.x - orbit.focus.x) * bxz;
    orbit.focus.z += (target_focus.z - orbit.focus.z) * bxz;
    orbit.focus.y += (target_focus.y - orbit.focus.y) * by;
    let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
    transform.translation = orbit.focus + rotation * Vec3::new(0.0, 0.0, orbit.cam.dist);
    transform.look_at(orbit.focus, Vec3::Y);
}

/// TAE 155 SetLockParamID (exe FUN_140b51420 -> FUN_140737430: slot 1..45 written every
/// frame the event runs, reset to 1 after each camera update in FUN_1407374e0). A slot
/// change blends over the new slot's CameraSetParam beginTime, or the old one's endTime
/// when going back to slot 1.
fn update_cam_slot(combat: &Combat, actor: &Actor, cam: &mut CamState, dt: f32) {
    let want = if actor.anim.is_empty() {
        1
    } else {
        combat
            .player
            .events_at(&actor.anim, actor.t)
            .find(|e| e.kind == 155)
            .and_then(|e| e.arg_i64("LockParamID"))
            .filter(|&id| id > 0 && id < 0x2e)
            .unwrap_or(1)
    };
    if want != cam.slot {
        cam.blend_len = if want == 1 { cam_slot(combat, cam.slot).3 } else { cam_slot(combat, want).2 };
        cam.prev_slot = cam.slot;
        cam.slot = want;
        cam.blend_t = 0.0;
    }
    cam.blend_t += dt;
}

/// TAE 153 CameraModule3 (exe FUN_140b51680 -> FUN_140832810/830/7f0, consumed in
/// FUN_14073c260; the override block is cleared every frame by FUN_140833e40):
/// - the target distance is the LockCamParam camDistTarget, chased at
///   CameraParam.lockCamParamLerpRate, or at the last EndInterpolationSpeed when > 0;
/// - while an event runs, the target is CamDistTargetOverride, chased at
///   StartInterpolationSpeed (the param rate when <= 0);
/// - SlowStart instead eases from the distance at the event's first frame:
///   from + (target - from) * (1 - (p - 1)^2), p = event progress;
/// - the distance used then follows that target at 0.1 per frame (+0x214).
pub(crate) fn update_cam_distance(combat: &Combat, actor: &Actor, cam: &mut CamState, param_dist: f32, param_rate: f32, dt: f32) {
    let step = |rate: f32| 1.0 - (1.0 - rate.clamp(0.0, 1.0)).powf(dt * 30.0);
    let ev = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.t).find(|e| e.kind == 153) };
    let f = |e: &crate::data::Event, k: &str| e.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let mut rate = if cam.end_rate > 0.0 { cam.end_rate } else { param_rate };
    let target;
    match ev {
        Some(e) => {
            let id = (actor.anim.clone(), e.start);
            let first = cam.last_153.as_ref() != Some(&id);
            cam.last_153 = Some(id);
            target = f(e, "CamDistTargetOverride");
            cam.end_rate = f(e, "EndInterpolationSpeed");
            let slow_start = e.args.get("SlowStart").and_then(|v| v.as_bool()).unwrap_or(false);
            if slow_start {
                if first {
                    cam.dist_from = cam.dist_raw;
                }
                let p = ((actor.t - e.start) / (e.end - e.start).max(1e-4)).min(1.0);
                cam.dist_raw = cam.dist_from + (target - cam.dist_from) * (1.0 - (p - 1.0) * (p - 1.0));
            } else {
                let start_rate = f(e, "StartInterpolationSpeed");
                rate = if start_rate > 0.0 { start_rate } else { param_rate };
                cam.dist_raw += (target - cam.dist_raw) * step(rate);
            }
        }
        None => {
            cam.last_153 = None;
            target = param_dist;
            cam.dist_raw += (target - cam.dist_raw) * step(rate);
        }
    }
    cam.dist += (cam.dist_raw - cam.dist) * step(0.1);
}

/// Active camera shakes: (RumbleCam id, time, gain).
#[derive(Resource, Default)]
struct CameraShake(Vec<(String, f32, f32)>);

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
        let d = crate::actor::data_for(&combat, a.side);
        let Some(anim) = d.anim(&a.anim) else { continue };
        for e in anim.events.iter().filter(|e| matches!(e.kind, 144..=147)) {
            if !(e.start > a.prev_t && e.start <= a.t) && !(a.prev_t == 0.0 && e.start == 0.0) {
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

fn apply_shakes(time: Res<Time>, combat: Res<Combat>, mut shake: ResMut<CameraShake>, mut cam: Query<&mut Transform, With<OrbitCamera>>) {
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
