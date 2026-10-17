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
    /// TAE 156 SetCameraAngle: the event running (anim key, start) and the (yaw, pitch) at its
    /// first frame (exe ChrCam +0x1f4 / +0x1f0).
    angle_from: Option<((String, f32), (f32, f32))>,
    /// +0x50 FOV (radians) and +0x204 focus height, lerped to the LockCamParam row.
    fov: f32,
    focus_y: f32,
    /// TAE 151 CameraLookAtTarget look limits (exe camera +0x230..+0x23c):
    /// [pitch_min, pitch_max, yaw_min, yaw_max] radians, absolute, chasing the event at
    /// CameraParam.lockCamParamLerpRate. `None` while no look-at target is active.
    look: Option<[f32; 4]>,
    /// +0x100 the camera position chasing the orbit's point (`chase_position`), before
    /// collision; `None` until the first frame.
    pos: Option<Vec3>,
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
    /// The camera closer than playerFadeDist to Wolf this frame (`cam_fade_step`).
    near: bool,
    /// Wolf's CamFade alpha (`cam_fade_step`), 1 = drawn as usual.
    alpha: f32,
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
/// frame at 60 fps; here it is scaled to 1/60 s.
pub(crate) fn place_fulcrum(focus: Vec3, arm: Vec3, cam_dist: f32, sideways: f32, dt: f32, p: FulcrumParams) -> Vec3 {
    let per_frame = sideways / (dt * 60.0).max(1e-3);
    let k = (per_frame / p.lr_max_dist.max(1e-3)).clamp(p.lr_min, p.lr_max);
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
/// The exe applies the rates once a frame with no time factor; taken here as per 1/60 s (the
/// game's frame cap). gap: the FUN_140831190 / 1408311b0 override (untraced); the exe measures the last
/// distance from the post-collision point (+0x110) and lifts the distance rate to
/// targetChaseDistRateXZ / Y_ForHitWall (1.0) for targetChaseDistRateTime after a wall hit,
/// which the pre-collision point used here stands in for.
pub(crate) fn chase_position(prev: Vec3, raw: Vec3, focus: Vec3, r: PosChase, pitch: Option<(f32, f32)>, snap: f32, dt: f32) -> Vec3 {
    let step = |rate: f32| 1.0 - (1.0 - rate.clamp(0.0, 1.0)).powf(dt * 60.0);
    let (to_raw, to_prev) = (raw - focus, prev - focus);
    let cos = if to_raw.length() <= 1e-5 || to_prev.length() <= 1e-5 { 1.0 } else { to_raw.normalize().dot(to_prev.normalize()) };
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
    let (xz, y) = (step(xz + (1.0 - xz) * snap), step(y + (1.0 - y) * snap));
    let d = raw - prev;
    let p = prev + Vec3::new(d.x * xz, d.y * y, d.z * xz);
    let (raw_d, prev_d) = (to_raw.length(), to_prev.length());
    let dist_xz = prev_d + (raw_d - prev_d) * step(r.dist_xz);
    let dist_y = prev_d + (raw_d - prev_d) * step(r.dist_y);
    let v = p - focus;
    let l = v.length();
    if l <= 1e-5 {
        return p;
    }
    focus + Vec3::new(v.x * dist_xz / l, v.y * dist_y / l, v.z * dist_xz / l)
}

impl CamState {
    #[cfg(test)]
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
            angle_from: None,
            fov: lc.fov_y.to_radians(),
            focus_y: lc.focus_y,
            look: None,
            pos: None,
            ctrl_snap: 0.0,
            ctrl_hold: 0.0,
            fulcrum: None,
            trans_snap: 1.0,
            trans_hold: 0.0,
            near: false,
            alpha: 1.0,
            seen: None,
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

/// Set when Wolf is placed (the first frame, a retry): the camera jumps behind him at the
/// starting pitch instead of chasing there from where it was.
#[derive(Resource)]
pub struct CameraSnap(pub bool);

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
        app.init_resource::<LockOn>()
            .insert_resource(CameraSnap(true))
            .add_systems(Startup, spawn_camera)
            .add_systems(

                PostUpdate,
                (capture_mouse, toggle_lock, orbit_with_mouse, orbit_with_stick, follow_player, apply_shakes, fade_player).chain().before(TransformSystems::Propagate),
            );
    }
}

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
/// frame snapshot (FUN_140960650, 14096.c:3190): analog actions 0x54 / 0x55 times -1 unless the
/// option bytes +0x715 / +0x716 are set (likely the camera invert options, not verified), so with
/// the defaults the switch sees the raw axis. The only DirectInput float source is the mouse poll
/// FUN_141a8f540 (141a8.c:10829): DIMOUSESTATE lX / lY as is, relative counts per poll, no scale
/// or accumulation found (the action 0x54 binding itself is not traced). Bevy's
/// AccumulatedMouseMotion is the same unit (raw counts a frame, y down), so 50 / 25 apply as is.
/// The pick's d is NDC (FUN_1409ca480, 1409c.c:6512-6532: view x projection, / w, no viewport
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn toggle_lock(
    _keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    combat: Res<Combat>,
    time: Res<Time>,
    mut lock: ResMut<LockOn>,
    mut cooldown: Local<f32>,
    pads: Query<&Gamepad>,
    config: Res<GameConfig>,
    player: Single<(&Transform, &crate::actor::Actor), With<Player>>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
    cams: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    (screen, cursor): (Option<Res<State<crate::menu::Screen>>>, Option<Single<&CursorOptions, With<PrimaryWindow>>>),
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
    // Only in play: behind a menu (paused, the death screen) the mouse and stick are the menu's
    // and the game clock - the switch cooldown's - stands still.
    if screen.is_some_and(|s| *s.get() != crate::menu::Screen::Playing) {
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
                    .filter(|(_, tf, e)| lockable(tf, e) && cam.is_none_or(|c| acquirable(&c, wolf, wolf_fwd, tf.translation)))
                    .min_by(|a, b| a.1.translation.distance(wolf).total_cmp(&b.1.translation.distance(wolf)))
                    .map(|(e, ..)| e)
            }
        };
    }
    // Several enemies: the mouse switch (see LOCK_SWITCH_FIRE), or the right stick
    // (FUN_1409ca2d0: |stick| < 0.5 re-arms, > 0.95 fires; the stick is not negated, so up on
    // the stick is up on screen: viewport y down).
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
        .filter(|(e, tf, en)| *e != cur && lockable(tf, en) && switchable(cam_tf, tf.translation))
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
    // gap: the exe's centre is the view along minus row 2 of the dummy's world matrix
    // (FUN_14073a1e0 -> FUN_1409e6c90; pitch FUN_140733d70 = -atan2(y, xz), yaw FUN_140733fb0
    // = atan2(x, z)), not the player -> dummy vector used here; whether that row is the FLVER
    // dummy forward (Wolf's dummy 142 points forward, -Z in model space) is not settled.
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

/// The right stick's turn rate [rad/s] per axis (x: yaw, y: pitch) from CameraParam (deg/s in
/// the paramdef, radians in the export), as the exe: the camera speed option (options byte 0,
/// default 5) is stored as s = clamp(byte / 10, 0, 1) (FUN_140734790 -> camera +0x270), and each
/// frame the camera keeps lerp(rotSpeed_Min, rotSpeed_Max, s * s) and lerp(rotHiSpeed_Min,
/// rotHiSpeed_Max, s * s) (FUN_14073c260 -> +0x250..+0x264); the stick turns at
/// lerp(normal, high, |ramp|) (`hi`, see orbit_with_stick).
fn stick_rate(combat: &Combat, speed: f32, hi: f32) -> Vec2 {
    let c = combat.param("CameraParam", 0);
    let f = |k: &str, d: f64| c[k].as_f64().unwrap_or(d) as f32;
    let s = (speed / 10.0).clamp(0.0, 1.0);
    let t = s * s;
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let base = Vec2::new(lerp(f("rotSpeed_MinY", 0.977), f("rotSpeed_MaxY", 3.665)), lerp(f("rotSpeed_MinX", 0.977), f("rotSpeed_MaxX", 2.932)));
    let high = Vec2::new(lerp(f("rotHiSpeed_MinY", 2.688), f("rotHiSpeed_MaxY", 7.575)), lerp(f("rotHiSpeed_MinX", 2.199), f("rotHiSpeed_MaxX", 6.109)));
    base + (high - base) * hi.clamp(0.0, 1.0)
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
    // (thunk_FUN_1419dcdb0) is read as "not zero", and which stick component signs it as x.
    let inc = dt / combat.param("CameraParam", 0)["rotHiSpeedRateIncTime"].as_f64().unwrap_or(0.6).max(1e-3) as f32;
    *hi = if r.length() <= 0.998 || r.x.abs() <= 1e-3 {
        0.0
    } else if r.x > 0.0 {
        (hi.max(0.0) + inc).min(1.0)
    } else {
        (hi.min(0.0) - inc).max(-1.0)
    };
    if r.length() < 1e-3 {
        return;
    }
    let rate = stick_rate(&combat, config.input.camera_speed, hi.abs());
    let sign = if config.input.invert_y { -1.0 } else { 1.0 };
    camera.yaw -= r.x * rate.x * dt;
    let (lo, hi_pitch) = pitch_range(&combat, false);
    // Stick up looks up: the camera goes under the focus (pitch down).
    camera.pitch = (camera.pitch - sign * r.y * rate.y * dt).clamp(lo, hi_pitch);
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
    // +0x154 counts down, then +0x150 falls over rotCtrlEndReleaseTime (FUN_14073c260
    // 0x14073e8xx); turning the camera (the right stick / mouse, unlocked) sets it back to 1
    // and +0x154 to rotCtrlEndWaitTime.
    orbit.cam.ctrl_hold = (orbit.cam.ctrl_hold - dt).max(0.0);
    if orbit.cam.ctrl_hold <= 0.0 {
        orbit.cam.ctrl_snap = (orbit.cam.ctrl_snap - dt / cf("rotCtrlEndReleaseTime", 1.0).max(1e-3)).max(0.0);
    }
    // This frame's turning by hand (mouse / stick, before the scripted look limits move the
    // angles); the fulcrum's yaw resync adds it back.
    let turning = orbit.cam.seen.map(|(y, p)| (angle_delta(orbit.yaw, y), orbit.pitch - p)).filter(|d| d.0.abs() > 1e-6 || d.1.abs() > 1e-6);
    if orbit.cam.seen.is_some_and(|s| s != (orbit.yaw, orbit.pitch)) && lock.target.is_none() {
        orbit.cam.ctrl_snap = 1.0;
        orbit.cam.ctrl_hold = cf("rotCtrlEndWaitTime", 2.0);
    }
    // Lock-on chase (FUN_14073c260): each frame the angle moves diff * rate * (dt / (1/30)),
    // yaw at lockRotChaseRateY (0.3), pitch at lockRotChaseRateX (0.6).
    let step = |rate: f32| (rate * dt * 30.0).min(1.0);
    update_cam_distance(&combat, actor, &mut orbit.cam, lf("camDistTarget", 4.5), cf("lockCamParamLerpRate", 0.05), dt);
    // +0x50 FOV and +0x204 focus height chase the LockCamParam row at lockCamParamLerpRate.
    let lerp = 1.0 - (1.0 - cf("lockCamParamLerpRate", 0.05).clamp(0.0, 1.0)).powf(dt * 30.0);
    orbit.cam.fov += (lf("camFovY", 43.0).clamp(0.0, 180.0).to_radians() - orbit.cam.fov) * lerp;
    orbit.cam.focus_y += (lf("chrOrgOffset_Y", 1.5) - orbit.cam.focus_y) * lerp;
    if let Projection::Perspective(pp) = &mut **projection {
        pp.fov = orbit.cam.fov;
    }
    let mut range = pitch_range(&combat, lock.target.is_some());
    // TAE 156 SetCameraAngle takes the place of the lock-on turning while it runs (14073.c 10606:
    // the lock chase only when FUN_140831ea0, "an angle event is set", is false).
    let set_angle = if actor.anim.is_empty() { None } else { combat.player.events_at(&actor.anim, actor.t).find(|e| e.kind == 156) };
    if set_angle.is_some() {
        // Set below, after the fulcrum resync (the exe's order).
    } else if let Some((target, tdummies, ta)) = lock.target.and_then(|e| enemies.get(e).ok()) {
        let dir = (target.translation - player.translation).with_y(0.0);
        // Right over the target (head kick, vault, plunge) the direction to it flips within a few
        // centimetres: hold the yaw chase while the bodies overlap horizontally (Wolf's radius +
        // NpcParam hitRadius) instead of whipping the camera around.
        let npc_radius = combat.npc(ta)["hitRadius"].as_f64().unwrap_or(0.5) as f32;
        let overhead = dir.length() < crate::actor::PLAYER_BODY_RADIUS + npc_radius;
        if !overhead && dir.length_squared() > 0.01 {
            let want = f32::atan2(-dir.x, -dir.z);
            let diff = (want - orbit.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            orbit.yaw += diff * step(cf("lockRotChaseRateY", 0.3));
        }
        // Pitch, as the exe's lock camera (FUN_14073c260 at 0x14073d6xx-0x14073dd0e):
        // - the framed point P = focus + lockTgtPosRate (0.7) * (lock point - focus); the lock
        //   point is the target's DummyPoly 220;
        // - half = FOV * lockRotXShiftRatio * 0.5; r = camera-to-focus distance; the law of sines
        //   gives the angle at P that puts P half off the screen centre: a = asin(r sin(half) / |FP|)
        //   (pi/2 when |FP| <= r sin(half));
        // - target pitch = a + half - elevation(P - focus) (positive = camera above, looking down);
        // - limits rotRangeXAtLock widen toward the free range as |P.y - focus.y| goes
        //   rotRangeLerpBeginHeight -> EndHeight;
        // - chased at lockRotChaseRateX * dt / (1/30), outside lockRotChasePlayAngX (0 here).
        let lock_point = tdummies
            .and_then(|d| d.0.get(&220))
            .and_then(|&m| dummy_tf.get(m).ok())
            .map(|g| g.translation())
            .unwrap_or(target.translation + Vec3::Y * (lf("chrOrgOffset_Y", 1.5) - crate::player::CAPSULE_HALF_HEIGHT));
        let focus = orbit.focus;
        let fp = (lock_point - focus) * cf("lockTgtPosRate", 0.7);
        let (b, e) = (cf("rotRangeLerpBeginHeight", 0.5), cf("rotRangeLerpEndHeight", 3.0));
        let t = ((fp.y.abs() - b) / (e - b).max(1e-3)).clamp(0.0, 1.0);
        let free = pitch_range(&combat, false);
        range = (range.0 + (free.0 - range.0) * t, range.1 + (free.1 - range.1) * t);
        let half = orbit.cam.fov * lf("lockRotXShiftRatio", 0.45) * 0.5;
        let r = orbit.cam.dist;
        let reach = half.sin() * r;
        let a = if fp.length() <= reach { std::f32::consts::FRAC_PI_2 } else { (reach / fp.length()).asin() };
        let elev = f32::atan2(fp.y, fp.with_y(0.0).length());
        let want_pitch = (a + half - elev).clamp(range.0, range.1);
        let diff = want_pitch - orbit.pitch;
        let play = cf("lockRotChasePlayAngX", 0.0);
        if diff.abs() >= play {
            orbit.pitch += (diff - play * diff.signum()) * step(cf("lockRotChaseRateX", 0.6));
        }
    } else {
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
    } else if let Some(cur) = orbit.cam.look {
        // FUN_140742230 without the event: the pitch limits chase back to LockCamParam
        // rotRangeMinX (degrees) / CameraParam rotRangeXMax at lockCamParamLerpRate; the yaw
        // limits take rotRangeYMin / Max at once (the full circle: no yaw limit). gap: the 6
        // CameraParam rows with a +-90 deg yaw range centre it on Wolf's facing + rotBaseY.
        let lerp = 1.0 - (1.0 - cf("lockCamParamLerpRate", 0.05).clamp(0.0, 1.0)).powf(dt * 30.0);
        let def = [lf("rotRangeMinX", -40.0).to_radians(), cf("rotRangeXMax", 1.3963)];
        let next = [cur[0] + angle_delta(def[0], cur[0]) * lerp, cur[1] + angle_delta(def[1], cur[1]) * lerp];
        orbit.pitch = clamp_angle(orbit.pitch, next[0], next[1]);
        let back = angle_delta(def[0], next[0]).abs() < 1e-3 && angle_delta(def[1], next[1]).abs() < 1e-3;
        orbit.cam.look = (!back).then_some([next[0], next[1], -std::f32::consts::PI, std::f32::consts::PI]);
    }
    orbit.focus.x += (target_focus.x - orbit.focus.x) * bxz;
    orbit.focus.z += (target_focus.z - orbit.focus.z) * bxz;
    orbit.focus.y += (target_focus.y - orbit.focus.y) * by;
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
    // gap: where the exe's TAE 151 look limits (FUN_140742230) sit against the resync is not
    // traced; here they clamp before it.
    if let (Some((f, _)), None) = (orbit.cam.fulcrum, lock.target) {
        // The free camera's yaw from the dragged fulcrum (FUN_140734070 of focus - fulcrum), plus
        // this frame's turning (the exe turns after the resync).
        let v = drag_fulcrum(f, moved, orbit.cam.trans_snap, fp) - orbit.focus;
        if v.with_y(0.0).length_squared() > 1e-6 {
            // config camera.follow_swing scales how far the camera swings round after a sideways
            // run (1 = the CameraParam fulcrum's full swing: 68 deg/s at a run, 143 at a sprint,
            // which ran Wolf in tight circles with the stick held sideways; the user: "spinning
            // like a compass", the movement "feels off").
            let swing = config.as_ref().map_or(1.0, |c| c.camera.follow_swing).clamp(0.0, 1.0);
            let chased = f32::atan2(v.x, v.z);
            orbit.yaw = orbit.yaw + angle_delta(chased, orbit.yaw) * swing + turning.map_or(0.0, |d| d.0);
            // The pitch too (FUN_140733e50; 14073.c 9840-9880) when `chases_pitch`: a climb
            // or a fall leaves the fulcrum behind in height, which tilts the camera.
            let stick = pad.as_ref().map_or(Vec2::ZERO, |p| p.stick);
            if chases_pitch(cf("isEnableChrChaseRotX", 1.0) != 0.0, cf("chrChaseRotXInvalidAngle", 1.3963), stick) {
                let (lo, hi) = pitch_range(&combat, false);
                orbit.pitch = (f32::atan2(v.y, v.with_y(0.0).length()) + turning.map_or(0.0, |d| d.1)).clamp(lo, hi);
            }
        }
    }
    match set_angle {
        Some(e) => {
            let id = (actor.anim.clone(), e.start);
            if orbit.cam.angle_from.as_ref().is_none_or(|(k, _)| *k != id) {
                orbit.cam.angle_from = Some((id, (orbit.yaw, orbit.pitch)));
            }
            let from = orbit.cam.angle_from.as_ref().map_or((orbit.yaw, orbit.pitch), |f| f.1);
            let arg = |k: &str| e.arg_i64(k).unwrap_or(0) as f32;
            let on = |k: &str| e.args.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            let interp = e.args.get("InterpolationType").and_then(|v| v.as_str()).and_then(|s| s.split(':').next()?.trim().parse().ok()).or(e.arg_i64("InterpolationType")).unwrap_or(0);
            let p = (actor.t - e.start) / (e.end - e.start).max(1e-4);
            let (yaw, pitch) = set_camera_angle(
                from,
                actor.yaw,
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
    {
        let arm = rotation * Vec3::Z;
        let cam_dist = if orbit.cam.pos.is_some() { (transform.translation - orbit.focus).length() } else { orbit.cam.dist };
        let right = rotation * Vec3::X;
        let sideways = (moved.x * right.x + moved.z * right.z).abs();
        let f = if orbit.cam.fulcrum.is_some() {
            place_fulcrum(orbit.focus, arm, cam_dist, sideways, dt, fp)
        } else {
            // First frame (14073.c 9262-9285): fulcrumDistRate of the camera distance, at least
            // fulcrumDistMin.
            orbit.focus + arm * (cam_dist * cf("fulcrumDistRate", 0.5)).max(fp.dist_min)
        };
        orbit.cam.fulcrum = Some((f, orbit.focus));
    }
    // Camera collision (the game sphere-casts from the focus with CameraParam camCastSphereRadius):
    // the arena floor (y = 0) cuts the arm short, so a camera looking up from below the focus
    // stays above ground instead of going under it (the floor is one-sided: everything then
    // looked to float).
    let raw = orbit.focus + rotation * Vec3::Z * orbit.cam.dist;
    let locked = lock.target.is_some();
    let chase = if locked {
        PosChase {
            xz: cf("targetChaseRateAtLockXZ", 0.4),
            y: cf("targetChaseRateAtLockY", 0.3),
            ang_min: cf("targetChaseRateAngAtLockMin", 0.0),
            ang_max: cf("targetChaseRateAngAtLockMax", 0.0),
            dist_xz: cf("targetChaseDistRateXZ", 0.4),
            dist_y: cf("targetChaseDistRateY", 0.4),
        }
    } else {
        // gap: the exe eases a working X / Z rate (+0x228) toward this by dt each frame, and
        // snaps it to the _ForEscapeWall rates while escaping a wall.
        PosChase {
            xz: cf("targetChaseRateXZForNormal", 0.1),
            y: cf("targetChaseRateY", 0.4),
            ang_min: cf("targetChaseRateAngMin", 0.1745),
            ang_max: cf("targetChaseRateAngMax", 0.3491),
            dist_xz: cf("targetChaseDistRateXZ", 0.4),
            dist_y: cf("targetChaseDistRateY", 0.4),
        }
    };
    // +0x230 / +0x234: the TAE 151 limits while they run, else the defaults they chase back to.
    let pitch_lim = orbit.cam.look.map_or((lf("rotRangeMinX", -40.0).to_radians(), cf("rotRangeXMax", 1.3963)), |l| (l[0], l[1]));
    let pos = orbit.cam.pos.map_or(raw, |prev| chase_position(prev, raw, orbit.focus, chase, Some(pitch_lim), orbit.cam.ctrl_snap, dt));
    orbit.cam.pos = Some(pos);
    orbit.cam.seen = Some((orbit.yaw, orbit.pitch));
    let arm = (pos - orbit.focus).try_normalize().unwrap_or(rotation * Vec3::Z);
    let radius = cf("camCastSphereRadius", 0.05);
    let mut dist = (pos - orbit.focus).length();
    // The exe's cast keeps max(hit, 1e-5) of the arm (14073.c 12318-12395): no minimum distance
    // (it was 0.3 m here); Wolf fades instead (`cam_fade_step`).
    if let Some(hit) = crate::map::raycast(orbit.focus, arm, dist + radius) {
        // The map's walls and floors (camCastSphereRadius stand-in: the ray stops short by it).
        dist = dist.min((hit - radius).max(1e-5));
    } else if arm.y < -1e-3 && crate::map::terrain().is_none() {
        let room = (orbit.focus.y - radius) / -arm.y;
        dist = dist.min(room.max(1e-5));
    }
    dist = safe_margin(dist, cf("camSafeMarginRate", 0.1), cf("camSafeMarginMax", 0.3));
    transform.translation = orbit.focus + arm * dist;
    transform.look_at(orbit.focus, Vec3::Y);
    // CameraParam playerFadeDist (0.5 m) from the camera to Wolf's chased point (camera +0xa0).
    // gap: +0xa0 is taken as the focus; the exe's obstacle fade (anything within
    // camCastSphereRadiusForFade 0.3 m of the camera -> Wolf line, down to alpha 0.2) is not
    // ported.
    orbit.cam.near = dist < cf("playerFadeDist", 0.5);
}

/// Wolf's CamFade alpha (`cam_fade_step`) onto his actor (model.rs update_opacity draws it).
fn fade_player(time: Res<Time>, mut camera: Single<&mut OrbitCamera>, mut wolf: Single<&mut Actor, With<Player>>) {
    let cam = &mut camera.cam;
    cam.alpha = cam_fade_step(cam.alpha, cam.near, time.delta_secs());
    wolf.cam_alpha = cam.alpha;
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
            // An event at 0 fires in the anim's first step (as sound.rs sound_windows).
            if !(e.start > a.prev_t && e.start <= a.t) && !(a.prev_t == 0.0 && e.start == 0.0 && a.t > 0.0 && a.t < 0.05) {
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
