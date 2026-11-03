//! Camera framing and chase rates from the game's normal camera params.

use crate::hits::load_table;
use crate::tae::{TaeDb, TaeKind};
use glam::Vec3;
use sekiro_formats::param::Row;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraParams {
    /// LockCamParam row 0 `camDistTarget`.
    pub cam_dist: f32,
    /// LockCamParam row 0 `chrOrgOffset_Y`.
    pub chr_org_offset_y: f32,
    /// LockCamParam row 0 `camFovY`.
    pub cam_fov_y: f32,
    /// LockCamParam row 0 `chrLockRangeMaxRadius`.
    pub lock_range: f32,
    /// LockCamParam row 0 `lockRotXShiftRatio`.
    pub lock_rot_x_shift_ratio: f32,
    /// CameraParam row 0 `chrTransChaseRateXZ_ForNormal`.
    pub chr_trans_chase_rate_xz: f32,
    /// CameraParam row 0 `chrTransChaseRateY_ForNormal`.
    pub chr_trans_chase_rate_y: f32,
    /// CameraParam row 0 `rotRangeXMin`.
    pub rot_range_x_min: f32,
    /// CameraParam row 0 `rotRangeXMax`.
    pub rot_range_x_max: f32,
    /// CameraParam row 0 `rotRangeXAtLockMin`.
    pub rot_range_x_at_lock_min: f32,
    /// CameraParam row 0 `rotRangeXAtLockMax`.
    pub rot_range_x_at_lock_max: f32,
    /// CameraParam row 0 `rotRangeLerpBeginHeight`.
    pub rot_range_lerp_begin_height: f32,
    /// CameraParam row 0 `rotRangeLerpEndHeight`.
    pub rot_range_lerp_end_height: f32,
    /// CameraParam row 0 `rotSpeed_MaxX`.
    pub rot_speed_max_x: f32,
    /// CameraParam row 0 `rotSpeed_MaxY`.
    pub rot_speed_max_y: f32,
    /// CameraParam row 0 `lockRotChaseRateX`.
    pub lock_rot_chase_rate_x: f32,
    /// CameraParam row 0 `lockRotChaseRateY`.
    pub lock_rot_chase_rate_y: f32,
    /// CameraParam row 0 `lockTgtPosRate`.
    pub lock_tgt_pos_rate: f32,
    /// CameraParam row 0 `lockCamParamLerpRate`: how fast the distance, field of view and focus
    /// height follow the LockCamParam row, per 1/30 s.
    pub lock_cam_param_lerp_rate: f32,
}

impl Default for CameraParams {
    #[allow(clippy::approx_constant)] // Preserve the literal values stored in the params.
    fn default() -> Self {
        Self {
            cam_dist: 4.5,
            chr_org_offset_y: 1.5,
            cam_fov_y: 43.0,
            lock_range: 30.0,
            lock_rot_x_shift_ratio: 0.45,
            chr_trans_chase_rate_xz: 0.2,
            chr_trans_chase_rate_y: 0.3,
            rot_range_x_min: -0.6981317,
            rot_range_x_max: 1.3962634,
            rot_range_x_at_lock_min: -0.5235988,
            rot_range_x_at_lock_max: 0.5235988,
            rot_range_lerp_begin_height: 0.5,
            rot_range_lerp_end_height: 3.0,
            rot_speed_max_x: 2.9321532,
            rot_speed_max_y: 3.6651914,
            lock_rot_chase_rate_x: 0.6,
            lock_rot_chase_rate_y: 0.3,
            lock_tgt_pos_rate: 0.7,
            lock_cam_param_lerp_rate: 0.05,
        }
    }
}

fn param_dirs(cache: &Path) -> (PathBuf, PathBuf) {
    (
        cache.join("raw/param/gameparam/gameparam.parambnd.d"),
        cache.join("refs/paramdex/Defs"),
    )
}

impl CameraParams {
    /// Loads row 0 of both tables, falling back per field when data is missing.
    pub fn load(cache: &Path) -> Self {
        let (pd, dd) = param_dirs(cache);
        let lock = load_table(&pd, &dd, "LockCamParam", "LockCamParam");
        let camera = load_table(&pd, &dd, "CameraParam", "CameraParam");
        Self::from_rows(
            lock.as_ref().and_then(|t| t.find(0)),
            camera.as_ref().and_then(|t| t.find(0)),
            &Self::default(),
        )
    }

    /// The fields of a LockCamParam and a CameraParam row; a missing row or field keeps the
    /// value from `base`.
    fn from_rows(lock: Option<Row<'_>>, camera: Option<Row<'_>>, base: &Self) -> Self {
        let l = |name: &str, d: f32| lock.as_ref().and_then(|r| r.f32(name).ok()).unwrap_or(d);
        let c = |name: &str, d: f32| camera.as_ref().and_then(|r| r.f32(name).ok()).unwrap_or(d);
        Self {
            cam_dist: l("camDistTarget", base.cam_dist),
            chr_org_offset_y: l("chrOrgOffset_Y", base.chr_org_offset_y),
            cam_fov_y: l("camFovY", base.cam_fov_y),
            lock_range: l("chrLockRangeMaxRadius", base.lock_range),
            lock_rot_x_shift_ratio: l("lockRotXShiftRatio", base.lock_rot_x_shift_ratio),
            chr_trans_chase_rate_xz: c(
                "chrTransChaseRateXZ_ForNormal",
                base.chr_trans_chase_rate_xz,
            ),
            chr_trans_chase_rate_y: c("chrTransChaseRateY_ForNormal", base.chr_trans_chase_rate_y),
            rot_range_x_min: c("rotRangeXMin", base.rot_range_x_min),
            rot_range_x_max: c("rotRangeXMax", base.rot_range_x_max),
            rot_range_x_at_lock_min: c("rotRangeXAtLockMin", base.rot_range_x_at_lock_min),
            rot_range_x_at_lock_max: c("rotRangeXAtLockMax", base.rot_range_x_at_lock_max),
            rot_range_lerp_begin_height: c(
                "rotRangeLerpBeginHeight",
                base.rot_range_lerp_begin_height,
            ),
            rot_range_lerp_end_height: c("rotRangeLerpEndHeight", base.rot_range_lerp_end_height),
            rot_speed_max_x: c("rotSpeed_MaxX", base.rot_speed_max_x),
            rot_speed_max_y: c("rotSpeed_MaxY", base.rot_speed_max_y),
            lock_rot_chase_rate_x: c("lockRotChaseRateX", base.lock_rot_chase_rate_x),
            lock_rot_chase_rate_y: c("lockRotChaseRateY", base.lock_rot_chase_rate_y),
            lock_tgt_pos_rate: c("lockTgtPosRate", base.lock_tgt_pos_rate),
            lock_cam_param_lerp_rate: c("lockCamParamLerpRate", base.lock_cam_param_lerp_rate),
        }
    }

    /// Every field moved `w` (0 to 1) of the way from `self` to `to`.
    pub fn lerp(&self, to: &Self, w: f32) -> Self {
        let m = |a: f32, b: f32| a + (b - a) * w;
        Self {
            cam_dist: m(self.cam_dist, to.cam_dist),
            chr_org_offset_y: m(self.chr_org_offset_y, to.chr_org_offset_y),
            cam_fov_y: m(self.cam_fov_y, to.cam_fov_y),
            lock_range: m(self.lock_range, to.lock_range),
            lock_rot_x_shift_ratio: m(self.lock_rot_x_shift_ratio, to.lock_rot_x_shift_ratio),
            chr_trans_chase_rate_xz: m(self.chr_trans_chase_rate_xz, to.chr_trans_chase_rate_xz),
            chr_trans_chase_rate_y: m(self.chr_trans_chase_rate_y, to.chr_trans_chase_rate_y),
            rot_range_x_min: m(self.rot_range_x_min, to.rot_range_x_min),
            rot_range_x_max: m(self.rot_range_x_max, to.rot_range_x_max),
            rot_range_x_at_lock_min: m(self.rot_range_x_at_lock_min, to.rot_range_x_at_lock_min),
            rot_range_x_at_lock_max: m(self.rot_range_x_at_lock_max, to.rot_range_x_at_lock_max),
            rot_range_lerp_begin_height: m(
                self.rot_range_lerp_begin_height,
                to.rot_range_lerp_begin_height,
            ),
            rot_range_lerp_end_height: m(
                self.rot_range_lerp_end_height,
                to.rot_range_lerp_end_height,
            ),
            rot_speed_max_x: m(self.rot_speed_max_x, to.rot_speed_max_x),
            rot_speed_max_y: m(self.rot_speed_max_y, to.rot_speed_max_y),
            lock_rot_chase_rate_x: m(self.lock_rot_chase_rate_x, to.lock_rot_chase_rate_x),
            lock_rot_chase_rate_y: m(self.lock_rot_chase_rate_y, to.lock_rot_chase_rate_y),
            lock_tgt_pos_rate: m(self.lock_tgt_pos_rate, to.lock_tgt_pos_rate),
            lock_cam_param_lerp_rate: m(self.lock_cam_param_lerp_rate, to.lock_cam_param_lerp_rate),
        }
    }

    pub fn pitch_limits(&self, locked: bool, height_gap: f32) -> (f32, f32) {
        if !locked {
            return (self.rot_range_x_min, self.rot_range_x_max);
        }
        let t = ((height_gap.abs() - self.rot_range_lerp_begin_height)
            / (self.rot_range_lerp_end_height - self.rot_range_lerp_begin_height))
            .clamp(0.0, 1.0);
        (
            self.rot_range_x_at_lock_min
                + t * (self.rot_range_x_min - self.rot_range_x_at_lock_min),
            self.rot_range_x_at_lock_max
                + t * (self.rot_range_x_max - self.rot_range_x_at_lock_max),
        )
    }

    pub fn lock_target_pitch(&self, focus: Vec3, target: Vec3) -> f32 {
        let p = focus + self.lock_tgt_pos_rate * (target - focus);
        let half = self.cam_fov_y.to_radians() * self.lock_rot_x_shift_ratio * 0.5;
        let delta = p - focus;
        let ratio = self.cam_dist * half.sin() / delta.length();
        let a = if ratio <= 1.0 {
            ratio.asin()
        } else {
            std::f32::consts::FRAC_PI_2
        };
        let e = delta.y.atan2(delta.x.hypot(delta.z));
        a + half - e
    }
}

/// Fraction of the gap closed at the game's 30 Hz camera chase rate.
pub fn chase(rate: f32, dt: f32) -> f32 {
    1.0 - (1.0 - rate).powf(dt * 30.0)
}

/// How far in front of a wall the follow camera stops (m), and the closest it comes to its
/// focus. Not traced.
pub const CAMERA_WALL_MARGIN: f32 = 0.3;
pub const CAMERA_MIN_DISTANCE: f32 = 0.4;

/// The follow camera's distance from `focus` along `back` (unit, focus to camera), pulled in
/// front of walls between them: `distance` when nothing is in the way. Shared by the camera on
/// screen and the hook search's camera (`wire.rs`), so both stand in the same place.
pub fn wall_pulled_distance(
    world: &crate::collision::CollisionWorld,
    focus: Vec3,
    back: Vec3,
    distance: f32,
) -> f32 {
    world
        .camera_raycast(focus, back, distance + CAMERA_WALL_MARGIN)
        .map_or(distance, |hit| {
            (hit - CAMERA_WALL_MARGIN)
                .min(distance)
                .max(CAMERA_MIN_DISTANCE.min(distance))
        })
}

/// Matches the facing convention of Character::yaw_to.
pub fn lock_target_yaw(focus: Vec3, target: Vec3) -> f32 {
    let delta = target - focus;
    (-delta.x).atan2(-delta.z)
}

/// The number of CameraSetParam slots (`lockParamId0` .. `lockParamId44`); TAE 155 accepts 1 to
/// 45 (FwLo's trace of `FUN_140737430`, `docs/kb/camera.md`).
pub const CAMERA_SLOTS: usize = 45;

/// One CameraSetParam slot: the LockCamParam and CameraParam rows it uses and the seconds it
/// blends in over (`beginTime`) and back out over (`endTime`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSlot {
    pub lock_param: i32,
    pub cam_param: i32,
    pub begin: f32,
    pub end: f32,
}

/// CameraSetParam row 0 and the camera rows its slots name, read once.
#[derive(Debug, Clone, Default)]
pub struct CameraBook {
    slots: Vec<CameraSlot>,
    /// Params per (LockCamParam row, CameraParam row).
    params: HashMap<(i32, i32), CameraParams>,
    default: CameraParams,
}

impl CameraBook {
    /// Reads CameraSetParam row 0 and every LockCamParam / CameraParam pair it names; without
    /// the params, every slot is the default camera.
    pub fn load(cache: &Path) -> Self {
        let (pd, dd) = param_dirs(cache);
        let lock = load_table(&pd, &dd, "LockCamParam", "LockCamParam");
        let camera = load_table(&pd, &dd, "CameraParam", "CameraParam");
        let sets = load_table(&pd, &dd, "CameraSetParam", "CameraSetParam");
        let default = CameraParams::load(cache);
        let mut book = CameraBook {
            default,
            ..Default::default()
        };
        let Some(row) = sets.as_ref().and_then(|t| t.find(0)) else {
            return book;
        };
        for i in 0..CAMERA_SLOTS {
            // The row ids are integer fields and the times f32; reading an id as f32 fails and
            // would send every slot to row 0.
            let id = |f: &str| row.i32(&format!("{f}{i}")).unwrap_or(0);
            let time = |f: &str| row.f32(&format!("{f}{i}")).unwrap_or(0.0).max(0.0);
            let slot = CameraSlot {
                lock_param: id("lockParamId"),
                cam_param: id("camParamId"),
                begin: time("beginTime"),
                end: time("endTime"),
            };
            let key = (slot.lock_param, slot.cam_param);
            book.params.entry(key).or_insert_with(|| {
                CameraParams::from_rows(
                    lock.as_ref().and_then(|t| t.find(key.0)),
                    camera.as_ref().and_then(|t| t.find(key.1)),
                    &default,
                )
            });
            book.slots.push(slot);
        }
        book
    }

    /// Slot `n` (1-based, as TAE 155 names it).
    pub fn slot(&self, n: i32) -> Option<CameraSlot> {
        let i = usize::try_from(n - 1).ok()?;
        self.slots.get(i).copied()
    }

    /// The params of slot `n`; the default camera when the slot or its rows are missing.
    pub fn params(&self, n: i32) -> CameraParams {
        self.slot(n)
            .and_then(|s| self.params.get(&(s.lock_param, s.cam_param)))
            .copied()
            .unwrap_or(self.default)
    }
}

/// TAE 151 as the camera follows it this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAt {
    /// The character's dummy poly the focus chases (142 in Wolf's deathblow clips, on his
    /// animated camera bone `z_dummy_camera_P`).
    pub dummy: i32,
    /// Chase rate per 1/30 s on every axis; 0 keeps the normal rates.
    pub rate: f32,
    /// Look up, down, left and right limits, degrees around the direction to the dummy.
    pub limits: [f32; 4],
}

/// The camera as the player's animation drives it: the slot (TAE 155), the distance (TAE 153)
/// and the look-at target (TAE 151). Bevy-free, so the deathblow camera can be tested.
#[derive(Debug, Clone)]
pub struct CameraDirector {
    pub book: Arc<CameraBook>,
    /// The CameraSetParam slot in use (1 is the default camera) and the one it blends from.
    pub slot: i32,
    prev_slot: i32,
    blend_t: f32,
    blend_len: f32,
    /// The distance being chased (+0x218 in FwLo's notes); [`Self::distance`] follows it.
    dist_raw: f32,
    /// Metres from the focus to the camera (+0x214).
    pub distance: f32,
    dist_from: f32,
    /// The last `EndInterpolationSpeed` above 0: the rate back to the normal distance (it
    /// persists after the event, FwLo's trace).
    end_rate: f32,
    /// The TAE 153 event of the last step (clip, start), to find its first frame.
    last_153: Option<(String, u32)>,
    /// Field of view (degrees) and focus height (metres above the feet), following the slot's
    /// LockCamParam row at `lockCamParamLerpRate`.
    pub fov: f32,
    pub focus_height: f32,
    /// TAE 151 while it is active.
    pub look_at: Option<LookAt>,
    /// The look limits as they chase the event.
    look: LookLimits,
}

impl CameraDirector {
    pub fn new(book: Arc<CameraBook>) -> Self {
        let p = book.params(1);
        Self {
            book,
            slot: 1,
            prev_slot: 1,
            blend_t: 0.0,
            blend_len: 0.0,
            dist_raw: p.cam_dist,
            distance: p.cam_dist,
            dist_from: p.cam_dist,
            end_rate: 0.0,
            last_153: None,
            fov: p.cam_fov_y,
            focus_height: p.chr_org_offset_y,
            look_at: None,
            look: LookLimits::default(),
        }
    }

    pub fn load(cache: &Path) -> Self {
        Self::new(Arc::new(CameraBook::load(cache)))
    }

    /// The camera params now: the current slot's, blended from the previous slot's.
    pub fn params(&self) -> CameraParams {
        let w = if self.blend_len > 0.0 {
            (self.blend_t / self.blend_len).clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.book
            .params(self.prev_slot)
            .lerp(&self.book.params(self.slot), w)
    }

    /// Whether the animation is driving the camera: a slot other than the default, a look-at
    /// target, or a distance still away from the normal one.
    pub fn active(&self) -> bool {
        self.slot != 1
            || self.look_at.is_some()
            || (self.distance - self.params().cam_dist).abs() > 0.05
    }

    /// Advances by `dt` seconds with the character's full-body `clip` at `time` (`None` when it
    /// plays nothing), reading the clip's camera events from `tae`.
    pub fn update(&mut self, clip: Option<(&str, f32)>, tae: &TaeDb, dt: f32) {
        let events = clip.and_then(|(name, t)| Some((name, t, tae.anim(name)?.events.clone())));
        let active = |pick: &dyn Fn(&TaeKind) -> bool| {
            events.as_ref().and_then(|(name, t, ev)| {
                ev.iter()
                    .find(|e| e.active_at(*t) && pick(&e.kind))
                    .map(|e| (name.to_string(), *t, e.clone()))
            })
        };
        // TAE 155: set every frame the event runs, back to slot 1 otherwise. A change blends over
        // the new slot's beginTime, or the old slot's endTime going back to 1.
        let want = active(&|k| matches!(k, TaeKind::CameraSlot(_)))
            .and_then(|(_, _, e)| match e.kind {
                TaeKind::CameraSlot(n) => Some(n),
                _ => None,
            })
            .filter(|n| (1..=CAMERA_SLOTS as i32).contains(n))
            .unwrap_or(1);
        if want != self.slot {
            self.blend_len = if want == 1 {
                self.book.slot(self.slot).map_or(0.0, |s| s.end)
            } else {
                self.book.slot(want).map_or(0.0, |s| s.begin)
            };
            self.prev_slot = self.slot;
            self.slot = want;
            self.blend_t = 0.0;
        }
        self.blend_t += dt;
        let p = self.params();
        let step = |rate: f32| chase(rate.clamp(0.0, 1.0), dt);

        // TAE 153: the distance target and its rates.
        let mut rate = if self.end_rate > 0.0 {
            self.end_rate
        } else {
            p.lock_cam_param_lerp_rate
        };
        let distance_event = active(&|k| matches!(k, TaeKind::CameraDistance { .. }));
        match distance_event {
            Some((
                name,
                t,
                crate::tae::TimedEvent {
                    start,
                    end,
                    kind:
                        TaeKind::CameraDistance {
                            distance,
                            start_speed,
                            end_speed,
                            slow_start,
                        },
                    ..
                },
            )) => {
                let id = (name, start.to_bits());
                let first = self.last_153.as_ref() != Some(&id);
                self.last_153 = Some(id);
                self.end_rate = end_speed;
                if slow_start {
                    if first {
                        self.dist_from = self.dist_raw;
                    }
                    let k = ((t - start) / (end - start).max(1e-4)).min(1.0);
                    self.dist_raw = self.dist_from
                        + (distance - self.dist_from) * (1.0 - (k - 1.0) * (k - 1.0));
                } else {
                    rate = if start_speed > 0.0 {
                        start_speed
                    } else {
                        p.lock_cam_param_lerp_rate
                    };
                    self.dist_raw += (distance - self.dist_raw) * step(rate);
                }
            }
            _ => {
                self.last_153 = None;
                self.dist_raw += (p.cam_dist - self.dist_raw) * step(rate);
            }
        }
        self.distance += (self.dist_raw - self.distance) * step(0.1);
        // Field of view and focus height follow the slot's LockCamParam row.
        let lerp = step(p.lock_cam_param_lerp_rate);
        self.fov += (p.cam_fov_y - self.fov) * lerp;
        self.focus_height += (p.chr_org_offset_y - self.focus_height) * lerp;

        // TAE 151.
        self.look_at =
            active(&|k| matches!(k, TaeKind::CameraLookAt { .. })).and_then(|(_, _, e)| {
                match e.kind {
                    TaeKind::CameraLookAt {
                        dummy,
                        rate,
                        limits,
                    } if dummy >= 0 => Some(LookAt {
                        dummy,
                        rate,
                        limits,
                    }),
                    _ => None,
                }
            });
    }

    /// Clamps the camera's `yaw` and `pitch` to the look-at limits ([`LookLimits::clamp`]) with
    /// this director's own limit state; for a camera kept elsewhere, keep a [`LookLimits`] there.
    pub fn clamp_look(&mut self, yaw: f32, pitch: f32, dir: Vec3, dt: f32) -> (f32, f32) {
        let rate = self.params().lock_cam_param_lerp_rate;
        self.look.clamp(self.look_at, rate, yaw, pitch, dir, dt)
    }
}

/// The TAE 151 look limits as they chase the event (radians: pitch min, pitch max, yaw min, yaw
/// max), held by whoever owns the camera angles.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LookLimits(Option<[f32; 4]>);

impl LookLimits {
    /// Clamps the camera's `yaw` and `pitch` (radians; pitch positive looking down, as the orbit
    /// camera keeps it) to `look`'s limits around `dir`, the direction from the character to the
    /// dummy. The limits chase the event at `rate` per 1/30 s (`lockCamParamLerpRate`). Without
    /// a look-at the angles pass unchanged and the limits are forgotten.
    pub fn clamp(
        &mut self,
        look: Option<LookAt>,
        rate: f32,
        yaw: f32,
        pitch: f32,
        dir: Vec3,
        dt: f32,
    ) -> (f32, f32) {
        let Some(look) = look else {
            self.0 = None;
            return (yaw, pitch);
        };
        // FwLo centres the limits on the direction to the dummy; the exe builds the centre from
        // the dummy's own basis (a gap he notes, kept here).
        let center_yaw = lock_target_yaw(Vec3::ZERO, dir);
        let center_pitch = -dir.y.atan2(dir.x.hypot(dir.z).max(1e-3));
        let [up, down, left, right] = look.limits.map(f32::to_radians);
        let target = [
            center_pitch + up,
            center_pitch + down,
            center_yaw + left,
            center_yaw + right,
        ];
        let k = chase(rate.clamp(0.0, 1.0), dt);
        let cur = self.0.unwrap_or(target);
        let next: [f32; 4] =
            std::array::from_fn(|i| cur[i] + crate::input::angle_diff(target[i], cur[i]) * k);
        self.0 = Some(next);
        (
            clamp_angle(yaw, next[2], next[3]),
            clamp_angle(pitch, next[0], next[1]),
        )
    }
}

/// Clamps an angle into `[lo, hi]` by the representative of `x` nearest the interval's centre,
/// so a range across +-pi works; a full turn is no limit (FwLo's `clamp_angle`).
pub fn clamp_angle(x: f32, lo: f32, hi: f32) -> f32 {
    use std::f32::consts::TAU;
    if (hi - lo).abs() >= TAU - 1e-3 {
        return x;
    }
    let span = crate::input::angle_diff(hi, lo);
    let center = lo + span * 0.5;
    center + crate::input::angle_diff(x, center).clamp(-span.abs() * 0.5, span.abs() * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)] // Check the exact param defaults.
    fn camera_defaults() {
        let p = CameraParams::default();
        assert_eq!(p.cam_dist, 4.5);
        assert_eq!(p.chr_org_offset_y, 1.5);
        assert_eq!(p.cam_fov_y, 43.0);
        assert_eq!(p.lock_range, 30.0);
        assert_eq!(p.lock_rot_x_shift_ratio, 0.45);
        assert_eq!(p.chr_trans_chase_rate_xz, 0.2);
        assert_eq!(p.chr_trans_chase_rate_y, 0.3);
        assert_eq!(p.rot_range_x_min, -0.6981317);
        assert_eq!(p.rot_range_x_max, 1.3962634);
        assert_eq!(p.rot_range_x_at_lock_min, -0.5235988);
        assert_eq!(p.rot_range_x_at_lock_max, 0.5235988);
        assert_eq!(p.rot_range_lerp_begin_height, 0.5);
        assert_eq!(p.rot_range_lerp_end_height, 3.0);
        assert_eq!(p.rot_speed_max_x, 2.9321532);
        assert_eq!(p.rot_speed_max_y, 3.6651914);
        assert_eq!(p.lock_rot_chase_rate_x, 0.6);
        assert_eq!(p.lock_rot_chase_rate_y, 0.3);
        assert_eq!(p.lock_tgt_pos_rate, 0.7);
    }

    #[test]
    fn camera_chase() {
        assert!((chase(0.2, 1.0 / 30.0) - 0.2).abs() < 1e-5);
        assert!((chase(0.2, 2.0 / 30.0) - 0.36).abs() < 1e-5);
    }

    #[test]
    fn camera_pitch_limits() {
        let p = CameraParams::default();
        let free = (p.rot_range_x_min, p.rot_range_x_max);
        assert_eq!(p.pitch_limits(false, 0.0), free);
        assert_eq!(
            p.pitch_limits(true, 0.0),
            (p.rot_range_x_at_lock_min, p.rot_range_x_at_lock_max)
        );
        assert_eq!(p.pitch_limits(true, 3.0), free);
    }

    #[test]
    fn camera_lock_target_pitch() {
        let p = CameraParams::default();
        let focus = Vec3::new(0.0, 1.5, 0.0);
        let target = Vec3::new(0.0, 1.5, -2.5);
        let flat = p.lock_target_pitch(focus, target);
        assert!((0.55..0.65).contains(&flat));
        assert!(p.lock_target_pitch(focus, target + Vec3::Y * 2.0) < flat);
    }

    #[test]
    fn camera_lock_target_yaw() {
        assert_eq!(lock_target_yaw(Vec3::ZERO, Vec3::NEG_Z), 0.0);
        assert_eq!(
            lock_target_yaw(Vec3::ZERO, Vec3::X),
            -std::f32::consts::FRAC_PI_2
        );
    }

    #[test]
    fn camera_cache() {
        let cache = &crate::cache::root();
        if cache.join("params/LockCamParam.json").is_file() {
            assert_eq!(CameraParams::load(cache).lock_range, 30.0);
        }
    }

    /// #46: the free camera (slot 1) is LockCamParam 0 / CameraParam 0: 43 degrees vertical,
    /// 4.5 m from a focus 1.5 m above the feet; the other slots name their own rows.
    #[test]
    fn camera_book_slots_name_their_rows() {
        let cache = &crate::cache::root();
        let (pd, _) = param_dirs(cache);
        if !pd.join("CameraSetParam.param").is_file() {
            return;
        }
        let book = CameraBook::load(cache);
        let free = book.slot(1).unwrap();
        assert_eq!((free.lock_param, free.cam_param), (0, 0));
        let p = book.params(1);
        assert_eq!(
            (p.cam_fov_y, p.cam_dist, p.chr_org_offset_y),
            (43.0, 4.5, 1.5)
        );
        let second = book.slot(2).unwrap();
        assert_eq!((second.lock_param, second.cam_param), (100, 100));
        assert_eq!(book.params(2).cam_dist, 3.5);
        let director = CameraDirector::new(Arc::new(book));
        assert_eq!((director.fov, director.distance), (43.0, 4.5));
    }
}
