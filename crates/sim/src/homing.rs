//! How an attack finds its target: the unlocked attack auto-aim (the engine's
//! `CSChrAutoHomingModule`) and the TAE 760 lunge (`BoostRootMotionToReachTarget`).
//!
//! Both rules were traced from the executable by FwLo's RustSekiro (`docs/kb/attacks.md`,
//! 2026-10-07 entries); this is our own implementation of them. See `docs/HKS-COMBAT.md`.

use crate::hits::load_table;
use crate::input::angle_diff;
use crate::tae::ReachTarget;
use std::path::Path;

/// HKS `act(156)`: refresh the attack auto-aim target (called by `_UpdateAutoAim` while not
/// locked on, for the first 1/6 s of an attack).
pub const ACT_UPDATE_AUTO_AIM: i32 = 156;
/// HKS `act(157)`: drop the auto-aim target (`_StopAutoAim`, when the attack is left).
pub const ACT_STOP_AUTO_AIM: i32 = 157;

/// Height above the feet the engine measures auto-aim distances from, metres (the eye point
/// of the lock-on search, traced by FwLo's RustSekiro).
pub const AIM_EYE_HEIGHT: f32 = 1.5;

/// Auto-aim limits: the `close*` fields of LockCamParam row 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoAimLimits {
    /// `closeMaxRadius`: 3D reach from the eye point, metres.
    pub max_radius: f32,
    /// `closeMinHeight`: how far below the eye point a target may be, metres.
    pub min_height: f32,
    /// `closeMaxHeight`: how far above, metres.
    pub max_height: f32,
    /// `closeAngRange`: largest angle off the aim direction, degrees.
    pub ang_range: f32,
}

impl Default for AutoAimLimits {
    /// The values of LockCamParam row 0 ("Normal"), for when the params are not extracted.
    fn default() -> Self {
        Self {
            max_radius: 3.0,
            min_height: 3.0,
            max_height: 1.0,
            ang_range: 30.0,
        }
    }
}

impl AutoAimLimits {
    /// Reads LockCamParam row 0 from the extracted params, falling back per field.
    pub fn load(cache: &Path) -> Self {
        let table = load_table(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
            "LockCamParam",
            "LockCamParam",
        );
        let row = table.as_ref().and_then(|t| t.find(0));
        let d = Self::default();
        let f = |name: &str, fallback: f32| {
            row.as_ref()
                .and_then(|r| r.f32(name).ok())
                .unwrap_or(fallback)
        };
        Self {
            max_radius: f("closeMaxRadius", d.max_radius),
            min_height: f("closeMinHeight", d.min_height),
            max_height: f("closeMaxHeight", d.max_height),
            ang_range: f("closeAngRange", d.ang_range),
        }
    }

    /// The nearest candidate (by index) the attack may home in on, for a character standing
    /// at `feet` and aiming along world yaw `aim_yaw` (the stick, else the facing). Every
    /// candidate is measured from its own feet raised by [`AIM_EYE_HEIGHT`] too.
    ///
    /// Gap: the engine also needs a clear line of sight and uses the `_forD` / `_forPD`
    /// radii in darkness; neither is modelled.
    pub fn pick(&self, feet: [f32; 3], aim_yaw: f32, candidates: &[[f32; 3]]) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                // Both points sit AIM_EYE_HEIGHT above their feet, so the offsets cancel.
                let v = [c[0] - feet[0], c[1] - feet[1], c[2] - feet[2]];
                let dist = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if dist > self.max_radius || v[1] < -self.min_height || v[1] > self.max_height {
                    return None;
                }
                let off = if v[0].abs() + v[2].abs() < 1e-6 {
                    0.0
                } else {
                    angle_diff((-v[0]).atan2(-v[2]), aim_yaw).abs()
                };
                (off.to_degrees() <= self.ang_range).then_some((i, dist))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

/// The TAE 760 root-motion scale for a character at `me` attacking `target` (ground plane):
/// `clamp(|arrive - me|, range_min, range_max) / reference_dist`, where the arrival point
/// lies `arrive_dist` from the target toward the character, turned by `arrive_angle` about
/// the vertical. Already inside `arrive_dist` the scale is `range_min / reference_dist`;
/// a zero `reference_dist` gives 1.
///
/// Gap: the direction of a non-zero `arrive_angle` in our mirrored space is not verified.
pub fn reach_scale(r: &ReachTarget, me: [f32; 3], target: [f32; 3]) -> f32 {
    if r.reference_dist.abs() < f32::EPSILON {
        return 1.0;
    }
    let away = [me[0] - target[0], me[2] - target[2]];
    let len = (away[0] * away[0] + away[1] * away[1]).sqrt();
    let dist = if len < r.arrive_dist || len < 1e-6 {
        r.range_min
    } else {
        let (s, c) = r.arrive_angle.to_radians().sin_cos();
        let dir = [away[0] / len, away[1] / len];
        // Rotation about +Y applied to (x, z).
        let turned = [c * dir[0] + s * dir[1], -s * dir[0] + c * dir[1]];
        let arrive = [
            target[0] + turned[0] * r.arrive_dist,
            target[2] + turned[1] * r.arrive_dist,
        ];
        let (dx, dz) = (arrive[0] - me[0], arrive[1] - me[2]);
        // Shipped c5080 a000_003043 has min 6, max 4. Preserve both bounds without
        // Rust's ordered-bound assertion; upper-bound precedence is not exe-traced.
        (dx * dx + dz * dz).sqrt().max(r.range_min).min(r.range_max)
    };
    dist / r.reference_dist
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach() -> ReachTarget {
        ReachTarget {
            reference_dist: 2.0,
            range_min: 1.0,
            range_max: 4.0,
            arrive_angle: 0.0,
            arrive_dist: 1.0,
        }
    }

    #[test]
    fn reach_scale_follows_the_distance_to_the_arrival_point() {
        let r = reach();
        // 4 m away: arrive 1 m short, 3 m to go -> 1.5.
        assert!((reach_scale(&r, [0.0, 0.0, 0.0], [0.0, 0.0, -4.0]) - 1.5).abs() < 1e-5);
        // Far away: clamped to range_max.
        assert!((reach_scale(&r, [0.0, 0.0, 0.0], [0.0, 0.0, -20.0]) - 2.0).abs() < 1e-5);
        // Inside arrive_dist: range_min.
        assert!((reach_scale(&r, [0.0, 0.0, 0.0], [0.0, 0.0, -0.5]) - 0.5).abs() < 1e-5);
        // No reference distance: unscaled.
        let zero = ReachTarget {
            reference_dist: 0.0,
            ..r
        };
        assert_eq!(reach_scale(&zero, [0.0; 3], [0.0, 0.0, -4.0]), 1.0);
    }

    #[test]
    fn auto_aim_takes_the_nearest_enemy_inside_the_cone() {
        let l = AutoAimLimits::default();
        let at = |deg: f32, d: f32| {
            let a = deg.to_radians();
            [-a.sin() * d, 0.0, -a.cos() * d]
        };
        // Facing yaw 0 (-Z): 20 deg off passes, 60 deg off does not, 4 m is too far.
        assert_eq!(l.pick([0.0; 3], 0.0, &[at(20.0, 2.0)]), Some(0));
        assert_eq!(l.pick([0.0; 3], 0.0, &[at(60.0, 2.0)]), None);
        assert_eq!(l.pick([0.0; 3], 0.0, &[at(0.0, 4.0)]), None);
        // The nearer of two.
        assert_eq!(
            l.pick([0.0; 3], 0.0, &[at(10.0, 2.5), at(-10.0, 1.5)]),
            Some(1)
        );
        // Height window: 0.9 m above passes, 1.5 m above does not.
        assert_eq!(l.pick([0.0; 3], 0.0, &[[0.0, 0.9, -1.0]]), Some(0));
        assert_eq!(l.pick([0.0; 3], 0.0, &[[0.0, 1.5, -1.0]]), None);
    }
}
