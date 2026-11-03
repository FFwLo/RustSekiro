//! Job F interface for the two Ashina maps.
//!
//! Load `LodTable` once, resolve each MapPiece's authored byte, and keep one `LodState`
//! per canonical `part_index` (never per shared model). Call `update` with camera-to-OBB
//! distance each frame. Submit scene N with weight N, combining it with the owner's
//! residency/event/group decision. Scene 0 remains the legacy representation.
//! Objects need their model-side default before using this interface for ID 255.
//! Crossfade weights describe dual submissions; the retail coverage shader is untraced.

use std::{collections::HashMap, path::Path};

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result, bail};
use bevy::prelude::*;
use serde_json::Value;

fn rows(v: &Value) -> Result<&Vec<Value>> {
    v["rows"].as_array().context("LOD table has no rows")
}

#[derive(Clone, Copy, Debug)]
pub struct LodRule {
    pub boundary: [f32; 2],
    pub play: [f32; 2],
    pub crossfade: bool,
    pub seconds: f32,
}

pub struct LodTable {
    rows: HashMap<i32, LodRule>,
    pub platform_scale: [f32; 2],
}

impl LodTable {
    /// Windows quality rows are 4 (low), 5 (medium), 6 (high).
    /// Missing explicit IDs are errors, matching the exe resolver's lack of fallback.
    pub fn load(cache: &Path, platform_row: i32) -> Result<Self> {
        if !(4..=6).contains(&platform_row) {
            bail!("Windows LOD quality must select platform row 4, 5 or 6");
        }
        let read = |name: &str| -> Result<Value> {
            Ok(serde_json::from_slice(&std::fs::read(
                cache.join("params").join(format!("{name}.json")),
            )?)?)
        };
        let param = read("LodParam")?;
        let platform = read("LodPlatform")?;
        let float = |v: &Value, field: &str| -> Result<f32> {
            let value = v[field]
                .as_f64()
                .with_context(|| format!("missing LOD field {field}"))?
                as f32;
            if !value.is_finite() {
                bail!("nonfinite LOD field {field}");
            }
            Ok(value)
        };
        let p = rows(&platform)?
            .iter()
            .find(|p| p["id"].as_i64() == Some(platform_row as i64))
            .context("requested LodPlatform row absent")?;
        let platform_scale = [
            float(p, "lv01_BorderDist_Scale")?,
            float(p, "lv12_BorderDist_Scale")?,
        ];
        let mut result = HashMap::new();
        for row in rows(&param)? {
            let id = row["id"].as_i64().context("LOD row missing ID")? as i32;
            result.insert(
                id,
                LodRule {
                    boundary: [
                        float(row, "lv01_BorderDist")?,
                        float(row, "lv12_BorderDist")?,
                    ],
                    play: [float(row, "lv01_PlayDist")?, float(row, "lv12_PlayDist")?],
                    crossfade: row["lodCrossFadeEnable"]
                        .as_u64()
                        .context("LOD crossfade flag missing")?
                        != 0,
                    seconds: float(row, "lodCrossFadeTime")?,
                },
            );
        }
        Ok(Self {
            rows: result,
            platform_scale,
        })
    }

    pub fn map_piece(&self, authored: u8) -> Result<LodRule> {
        // The executable reads the byte as signed; negative MapPiece IDs default to 0.
        let id = i32::from(authored as i8).max(0);
        self.rows
            .get(&id)
            .copied()
            .with_context(|| format!("LodParam row {id} absent"))
    }
}

/// Map pieces' crossfade time scale ([`LodScales::crossfade_time`]): side by side with the
/// real game (2026-10-17 playtest, #63), its dissolve at the same 30 m switch looked about
/// three times longer than the param's 0.2 s. How the game derives its effective fade time
/// is untraced.
pub const CROSSFADE_STRETCH: f32 = 3.0;

/// Effective configuration inputs beyond the param row.
/// The extra per-type tables/globals are not fully traced, so callers explicitly supply
/// them rather than treating param distances as proven universal effective thresholds.
#[derive(Clone, Copy, Debug)]
pub struct LodScales {
    pub platform: [f32; 2],
    pub selector_type: [f32; 2],
    pub selector_adjust: [f32; 2],
    pub distance_type: [f32; 2],
    pub distance_adjust: [f32; 2],
    pub world_distance: f32,
    /// Multiplies the param's crossfade time.
    pub crossfade_time: f32,
}

impl Default for LodScales {
    fn default() -> Self {
        Self {
            platform: [1.; 2],
            selector_type: [1.; 2],
            selector_adjust: [0.; 2],
            distance_type: [1.; 2],
            distance_adjust: [0.; 2],
            world_distance: 1.,
            crossfade_time: 1.,
        }
    }
}

/// Distance to the nearest point on the instance's oriented, scaled model bounds.
pub fn bounds_distance(view: Vec3, transform: &Transform, min: Vec3, max: Vec3) -> f32 {
    let center = transform.transform_point((min + max) * 0.5);
    let half = (max - min).abs() * 0.5 * transform.scale.abs();
    let local = transform.rotation.inverse() * (view - center);
    (local - local.clamp(-half, half)).length()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LodState {
    pub current: usize,
    target: Option<usize>,
    progress: f32,
}

impl LodRule {
    fn boundaries(self, scales: LodScales, distance_mode: bool) -> [f32; 2] {
        let (types, adjust) = if distance_mode {
            (scales.distance_type, scales.distance_adjust)
        } else {
            (scales.selector_type, scales.selector_adjust)
        };
        let first = self.boundary[0] * scales.platform[0] * types[0] + adjust[0];
        let second = self.boundary[1] * scales.platform[1] * types[1] + adjust[1];
        [first, second.max(first)]
    }

    pub fn select(self, current: usize, distance: f32, scales: LodScales) -> usize {
        if self.boundary[0] < 0. {
            return 0;
        }
        let [b01, b12] = self.boundaries(scales, false);
        let [p01, p12] = self.play;
        let first = match current {
            0 => b01 + p01,
            1 => b01 - p01,
            _ => b01,
        };
        let second = match current {
            0 => b12,
            1 => b12 + p12,
            _ => b12 - p12,
        };
        if distance < first {
            0
        } else if self.boundary[1] < 0. || distance < second {
            1
        } else {
            2
        }
    }
}

impl LodState {
    /// Returns complementary scene weights, including direct 0/2 jumps.
    /// Missing entire levels reuse the nearest authored model level; never substitute
    /// individual LOD0 meshes into an otherwise authored lower-detail representation.
    pub fn update(
        &mut self,
        rule: LodRule,
        scales: LodScales,
        bounds_distance: f32,
        delta_seconds: f32,
        available: [bool; 3],
    ) -> [f32; 3] {
        let distance = bounds_distance * scales.world_distance;
        let mut weights = [0.; 3];
        if rule.crossfade && rule.seconds <= 0. && rule.boundary[0] >= 0. {
            let borders = rule.boundaries(scales, true);
            let mut chosen = false;
            for i in 0..2 {
                if rule.boundary[i] < 0. {
                    break;
                }
                let p = rule.play[i].max(0.);
                if distance < borders[i] + p {
                    let blend = if p > 0. {
                        ((distance - (borders[i] - p)) / (2. * p)).clamp(0., 1.)
                    } else {
                        0.
                    };
                    weights[i] = 1. - blend;
                    weights[i + 1] = blend;
                    chosen = true;
                    break;
                }
            }
            if !chosen {
                weights[if rule.boundary[1] < 0. { 1 } else { 2 }] = 1.;
            }
            self.target = None;
        } else {
            if self.target.is_none() {
                let selected = rule.select(self.current, distance, scales);
                if selected != self.current {
                    if rule.crossfade && rule.seconds > 0. {
                        self.target = Some(selected);
                        self.progress = 0.;
                    } else {
                        self.current = selected;
                    }
                }
            }
            if let Some(target) = self.target {
                self.progress = (self.progress
                    + delta_seconds.max(0.) / (rule.seconds * scales.crossfade_time))
                    .min(1.);
                weights[self.current] = 1. - self.progress;
                weights[target] = self.progress;
                if self.progress >= 1. {
                    self.current = target;
                    self.target = None;
                }
            } else {
                weights[self.current] = 1.;
            }
        }
        let mut submitted = [0.; 3];
        for (level, weight) in weights.into_iter().enumerate() {
            if let Some(actual) = (0..3)
                .filter(|&i| available[i])
                .min_by_key(|&i| (i.abs_diff(level), i))
            {
                submitted[actual] += weight;
            }
        }
        submitted
    }
}

/// `MeshTag` bit marking an instance drawn with crossfade coverage (`map_coverage.wesl`).
pub const FADE_TAG: u32 = 1 << 31;
/// `MeshTag` bit marking the higher level of a crossfading pair.
pub const FADE_HIGHER: u32 = 1 << 30;

/// Which levels to draw and their coverage tags, from the submission weights, the level drawn
/// now and which levels' scenes are ready. `None` hides a level and `Some(0)` draws it whole.
/// While another ready level has a fractional weight, the two levels share the screen through
/// complementary dither coverage, as the retail renderer submits both representations with
/// complementary weights during a transition. The low 16 bits carry the higher level's weight.
pub fn crossfade_tags(weights: [f32; 3], shown: usize, ready: [bool; 3]) -> [Option<u32>; 3] {
    let mut tags = [None; 3];
    let partner = (0..3)
        .filter(|&l| l != shown && ready[l] && weights[l] > 0. && weights[l] < 1.)
        .max_by(|&a, &b| weights[a].total_cmp(&weights[b]));
    let Some(partner) = partner else {
        tags[shown] = Some(0);
        return tags;
    };
    let (low, high) = (shown.min(partner), shown.max(partner));
    // The partner's weight is in (0, 1), so the total is positive.
    let weight = weights[high] / (weights[low] + weights[high]);
    let quantized = (weight * 65535.).round() as u32;
    tags[low] = Some(FADE_TAG | quantized);
    tags[high] = Some(FADE_TAG | FADE_HIGHER | quantized);
    tags
}

/// Whether a pixel with dither threshold `noise` in (0, 1) is drawn, as `lod_coverage` decides.
#[cfg(test)]
fn covers(tag: u32, noise: f32) -> bool {
    if tag & FADE_TAG == 0 {
        return true;
    }
    let weight = (tag & 0xffff) as f32 / 65535.;
    (noise < weight) == (tag & FADE_HIGHER != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #63: row 0 (every MapPiece with LOD ID 255) leaves LOD0 at 32 m. The level change
    /// must dissolve over the row's crossfade time instead of replacing the mesh in one frame,
    /// stretched for map pieces to 0.6 s.
    #[test]
    fn row0_switch_at_32m_dissolves_over_crossfade_time() {
        let rule = rule();
        let mut state = LodState::default();
        let ready = [true; 3];
        assert_eq!(
            state.update(rule, LodScales::default(), 31.9, 1. / 60., ready),
            [1., 0., 0.]
        );
        let weights = state.update(rule, LodScales::default(), 32.1, 0.05, ready);
        assert!((weights[0] - 0.75).abs() < 1e-5 && (weights[1] - 0.25).abs() < 1e-5);
        let tags = crossfade_tags(weights, 0, ready);
        let (low, high) = (tags[0].unwrap(), tags[1].unwrap());
        assert_eq!(tags[2], None);
        // Every pixel is drawn by exactly one level, and the share follows the weights.
        let noise: Vec<f32> = (0..64).map(|i| (i as f32 + 0.5) / 64.).collect();
        assert!(noise.iter().all(|&n| covers(low, n) != covers(high, n)));
        assert_eq!(noise.iter().filter(|&&n| covers(high, n)).count(), 16);
        // A level whose scene is not ready yet never takes pixels from the drawn level.
        assert_eq!(
            crossfade_tags(weights, 0, [true, false, true]),
            [Some(0), None, None]
        );
        for _ in 0..2 {
            state.update(rule, LodScales::default(), 32.1, 0.1, ready);
        }
        assert_eq!(state.current, 1);
        assert_eq!(
            crossfade_tags([0., 1., 0.], 1, ready),
            [None, Some(0), None]
        );
        // Map pieces stretch the fade: still dissolving after 0.5 s, done at 0.6 s.
        let stretched = LodScales {
            crossfade_time: CROSSFADE_STRETCH,
            ..LodScales::default()
        };
        let mut state = LodState::default();
        state.update(rule, stretched, 32.1, 0.0, ready);
        let weights = state.update(rule, stretched, 32.1, 0.5, ready);
        assert!(weights[0] > 0. && weights[1] < 1.);
        state.update(rule, stretched, 32.1, 0.1, ready);
        assert_eq!(state.current, 1);
    }

    fn rule() -> LodRule {
        LodRule {
            boundary: [30., 60.],
            play: [2., 4.],
            crossfade: true,
            seconds: 0.2,
        }
    }

    #[test]
    fn exe_hysteresis_boundaries_direct_jumps_and_negative_limits() {
        for (level, distance, expected) in [
            (0, 31.99, 0),
            (0, 32., 1),
            (0, 60., 2),
            (1, 27.99, 0),
            (1, 28., 1),
            (1, 64., 2),
            (2, 29.99, 0),
            (2, 55.99, 1),
            (2, 56., 2),
        ] {
            assert_eq!(
                rule().select(level, distance, LodScales::default()),
                expected
            );
        }
        assert_eq!(
            LodRule {
                boundary: [-1., 60.],
                ..rule()
            }
            .select(2, 100., LodScales::default()),
            0
        );
        assert_eq!(
            LodRule {
                boundary: [30., -1.],
                ..rule()
            }
            .select(0, 100., LodScales::default()),
            1
        );
    }

    #[test]
    fn timed_and_distance_crossfade_submit_complementary_levels() {
        let mut state = LodState::default();
        assert_eq!(
            state.update(rule(), LodScales::default(), 100., 0.1, [true; 3]),
            [0.5, 0., 0.5]
        );
        assert_eq!(
            state.update(rule(), LodScales::default(), 100., 0.1, [true; 3]),
            [0., 0., 1.]
        );
        let distance_rule = LodRule {
            seconds: 0.,
            ..rule()
        };
        assert_eq!(
            state.update(distance_rule, LodScales::default(), 30., 0., [true; 3]),
            [0.5, 0.5, 0.]
        );
        assert_eq!(
            state.update(distance_rule, LodScales::default(), 60., 0., [true; 3]),
            [0., 0.5, 0.5]
        );
        assert_eq!(
            state.update(
                distance_rule,
                LodScales::default(),
                60.,
                0.,
                [true, false, false]
            ),
            [1., 0., 0.]
        );
    }

    #[test]
    fn oriented_bounds_use_camera_surface_distance_and_installed_params() {
        let pose = Transform::from_xyz(10., 0., 0.)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::new(2., 1., 1.));
        assert!(
            (bounds_distance(Vec3::new(10., 0., -5.), &pose, -Vec3::ONE, Vec3::ONE) - 3.).abs()
                < 0.0001
        );
        assert_eq!(
            bounds_distance(Vec3::new(10., 0., 0.), &pose, -Vec3::ONE, Vec3::ONE),
            0.
        );
        let cache = sekiro_sim::cache::root();
        if cache.join("params/LodParam.json").exists() {
            let table = LodTable::load(&cache, 6).unwrap();
            assert_eq!(table.map_piece(255).unwrap().boundary, [30., 60.]);
            assert!(!table.map_piece(60).unwrap().crossfade);
            assert_eq!(
                table
                    .map_piece(61)
                    .unwrap()
                    .select(2, 9999., LodScales::default()),
                0
            );
            assert!(table.map_piece(127).is_err());
        }
    }
}
