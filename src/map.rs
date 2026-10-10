//! The game's own map around the fight: the pieces, hit collision and draw params that
//! `sekiro-extract map` wrote for one MSB enemy (`extracted/map_<id>.bin / .hit / .json`).
//!
//! - Pieces are static meshes drawn with the character material (albedo, normal map, cutouts),
//!   X mirrored on load like every model (the exporter writes map space minus the arena origin).
//! - The hit collision is a triangle soup in a 4 m grid: floors (walkable within 50 degrees of
//!   up, stepped onto within STEP_HEIGHT), walls (three stacked spheres pushed out
//!   horizontally, so bodies stop and slide) and a raycast for the camera. Ported from
//!   sekiro-rs `crates/sim/src/collision.rs`. Every triangle carries its HitMtrlParam id
//!   (footstep sounds).
//! - Lighting comes from the area's draw params (`param/drawparam/<area>_0000.gparam`): the
//!   light set the collision under the enemy selects (GparamConfig LightSetID), at a time of
//!   day. Angles are (pitch, yaw) radians with `Ry(yaw) * Rx(pitch) * +Z` pointing toward the
//!   light; colours RGB with the intensity in W. The engine's units are not known: lux / ambient
//!   scales are sekiro-rs's by-eye values (`LUX_PER_UNIT`, `AMBIENT_PER_UNIT`), the volumetric
//!   fog becomes Bevy's exponential distance fog.
//!
//! Without a map (config `[world] map` unset and no `SHINOBI_MAP`), nothing here runs and the
//! floor is the flat arena at y = 0 (`ground_y` returns 0): the sim tests stay as they were.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve, AutoExposurePlugin};
use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, Side};
use crate::config::GameConfig;
use crate::grading::{ColorGradingLut, GradingPlugin};
use crate::model::{SekiroExt, SekiroMaterial};
use crate::player::CAPSULE_HALF_HEIGHT;

/// Body radius for wall contacts. gap: Sekiro's capsule radius (player.rs CAPSULE_RADIUS is the
/// hit capsule; the map-contact radius is not read yet).
pub const RADIUS: f32 = 0.35;
/// Highest ledge a walking body steps onto, and how far it follows the floor down.
pub const STEP_HEIGHT: f32 = 0.5;
const MAX_SLOPE_DEGREES: f32 = 50.0;
/// Heights of the contact spheres' centres above the feet; the lowest sphere's bottom sits at
/// the step height so stairs reach the floor query rather than the wall push.
const SPHERES: [f32; 3] = [STEP_HEIGHT + RADIUS, 1.2, 1.55];
const CELL: f32 = 4.0;
const LUX_PER_UNIT: f32 = 9_000.0;
const AMBIENT_PER_UNIT: f32 = 1_800.0;
/// cd/m^2 per unit of probe radiance: a sun of one unit (LUX_PER_UNIT) seen as radiance,
/// LUX_PER_UNIT / pi, so the probes and the directional lights share the scale.
const ENV_PER_UNIT: f32 = 2_865.0;
/// Auto exposure (`auto_exposure`): the game's log2 luminance units onto this renderer's
/// buffer, and the EV base that keeps the 18 h look (-Exposure at 18 h).
const METER_EV: f32 = 2.6;
const BASE_EV: f32 = -0.8;
/// The most the auto exposure may brighten (EV): the night sets allow 8, far too much here.
const ADAPT_CAP_EV: f32 = 2.0;

/// The hit collision, shared with the systems that cannot take a resource (player.rs floor
/// checks, camera.rs arm clamp, sound.rs floor material) through [`terrain`].
pub struct Terrain {
    verts: Vec<Vec3>,
    tris: Vec<[u32; 3]>,
    normals: Vec<Vec3>,
    materials: Vec<u32>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    min_walk_ny: f32,
}

static TERRAIN: OnceLock<RwLock<Option<Arc<Terrain>>>> = OnceLock::new();

pub fn terrain() -> Option<Arc<Terrain>> {
    TERRAIN.get_or_init(|| RwLock::new(None)).read().ok()?.clone()
}

/// The map id in force: config `[world] map`, overridden by `SHINOBI_MAP` ("" = none).
pub fn map_id(config: &GameConfig) -> Option<String> {
    // "", "off" or "0" turn the map off for one run (PowerShell drops a variable set to "").
    match std::env::var("SHINOBI_MAP") {
        Ok(v) => (!v.is_empty() && v != "off" && v != "0").then_some(v),
        Err(_) => config.world.map.clone().filter(|m| !m.is_empty()),
    }
}

/// Floor height under an actor centre (its feet are CAPSULE_HALF_HEIGHT lower): the highest
/// walkable surface at most STEP_HEIGHT above the feet, else the one below; 0 without a map.
pub fn ground_y(centre: Vec3) -> f32 {
    let Some(t) = terrain() else { return 0.0 };
    let feet = centre.y - CAPSULE_HALF_HEIGHT;
    t.floor(centre.x, centre.z, feet + STEP_HEIGHT).or_else(|| t.floor(centre.x, centre.z, f32::INFINITY)).unwrap_or(feet - 100.0)
}

/// HitMtrlParam id of the floor under an actor centre (None without a map or over nothing).
pub fn floor_material(centre: Vec3) -> Option<u32> {
    let t = terrain()?;
    t.floor_tri(centre.x, centre.z, centre.y - CAPSULE_HALF_HEIGHT + STEP_HEIGHT).map(|(_, tri)| t.materials[tri as usize])
}

/// Distance along `dir` (unit) to the first collision triangle within `max`.
pub fn raycast(origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    terrain()?.raycast(origin, dir, max)
}

fn cell_of(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

impl Terrain {
    fn from_hit(d: &[u8]) -> Option<Self> {
        if d.len() < 12 || &d[0..4] != b"SHMC" {
            return None;
        }
        let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let nv = u32_(8) as usize;
        let mut o = 12;
        // X mirrored into Bevy space (as every model).
        let verts: Vec<Vec3> = (0..nv).map(|i| Vec3::new(-f(o + i * 12), f(o + i * 12 + 4), f(o + i * 12 + 8))).collect();
        o += nv * 12;
        let nt = u32_(o) as usize;
        o += 4;
        let mut t = Terrain { verts, tris: Vec::with_capacity(nt), normals: Vec::with_capacity(nt), materials: Vec::with_capacity(nt), cells: HashMap::new(), min_walk_ny: MAX_SLOPE_DEGREES.to_radians().cos() };
        for i in 0..nt {
            let k = o + i * 16;
            let tri = [u32_(k), u32_(k + 4), u32_(k + 8)];
            if tri.iter().any(|&x| x as usize >= nv) {
                continue;
            }
            let [a, b, c] = tri.map(|x| t.verts[x as usize]);
            let n = (b - a).cross(c - a);
            if n.length_squared() < 1e-12 {
                continue;
            }
            let id = t.tris.len() as u32;
            t.tris.push(tri);
            t.normals.push(n.normalize());
            t.materials.push(u32_(k + 12));
            let (x0, z0) = cell_of(a.x.min(b.x).min(c.x), a.z.min(b.z).min(c.z));
            let (x1, z1) = cell_of(a.x.max(b.x).max(c.x), a.z.max(b.z).max(c.z));
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    t.cells.entry((cx, cz)).or_default().push(id);
                }
            }
        }
        Some(t)
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    fn corners(&self, t: u32) -> [Vec3; 3] {
        self.tris[t as usize].map(|i| self.verts[i as usize])
    }

    fn gather(&self, min: Vec2, max: Vec2, out: &mut Vec<u32>) {
        out.clear();
        let (x0, z0) = cell_of(min.x, min.y);
        let (x1, z1) = cell_of(max.x, max.y);
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                if let Some(c) = self.cells.get(&(cx, cz)) {
                    out.extend_from_slice(c);
                }
            }
        }
        if x0 != x1 || z0 != z1 {
            out.sort_unstable();
            out.dedup();
        }
    }

    /// The highest walkable surface under (x, z) no higher than `top`: (height, triangle).
    /// Feet are not a point: a few centimetres around count too, so seams between triangles
    /// do not drop the body.
    pub fn floor_tri(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        const R: f32 = 0.08;
        let mut best: Option<(f32, u32)> = None;
        for (dx, dz) in [(0.0, 0.0), (R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R)] {
            let (px, pz) = (x + dx, z + dz);
            let Some(cell) = self.cells.get(&cell_of(px, pz)) else { continue };
            for &t in cell {
                if self.normals[t as usize].y.abs() < self.min_walk_ny {
                    continue;
                }
                let [a, b, c] = self.corners(t);
                if let Some(h) = height_on(px, pz, a, b, c) {
                    if h <= top && best.is_none_or(|(bh, _)| h > bh) {
                        best = Some((h, t));
                    }
                }
            }
        }
        best
    }

    pub fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_tri(x, z, top).map(|(h, _)| h)
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let end = origin + dir * max;
        let mut tris = Vec::new();
        self.gather(Vec2::new(origin.x.min(end.x), origin.z.min(end.z)), Vec2::new(origin.x.max(end.x), origin.z.max(end.z)), &mut tris);
        let mut best: Option<f32> = None;
        for t in tris {
            let [a, b, c] = self.corners(t);
            if let Some(d) = ray_triangle(origin, dir, a, b, c) {
                if d <= max && best.is_none_or(|b| d < b) {
                    best = Some(d);
                }
            }
        }
        best
    }

    /// Moves a body standing at `feet` by the horizontal `delta`, stopping at walls and
    /// sliding along them. Returns the horizontal displacement actually made.
    pub fn slide(&self, feet: Vec3, delta: Vec2) -> Vec2 {
        let len = delta.length();
        if len < 1e-6 {
            return Vec2::ZERO;
        }
        // Sub-steps of at most half the radius so a thin post cannot be stepped over (a tick's
        // move is centimetres; the cap only bounds a wild teleport).
        let steps = ((len / (RADIUS * 0.5)).ceil() as usize).clamp(1, 512);
        let mut p = feet;
        let mut tris = Vec::new();
        let reach = RADIUS + len / steps as f32 + 0.05;
        self.gather(
            Vec2::new(p.x.min(p.x + delta.x), p.z.min(p.z + delta.y)) - reach,
            Vec2::new(p.x.max(p.x + delta.x), p.z.max(p.z + delta.y)) + reach,
            &mut tris,
        );
        for _ in 0..steps {
            p.x += delta.x / steps as f32;
            p.z += delta.y / steps as f32;
            for _ in 0..4 {
                let push = self.push_out(p, &tris);
                if push == Vec2::ZERO {
                    break;
                }
                p.x += push.x;
                p.z += push.y;
            }
        }
        Vec2::new(p.x - feet.x, p.z - feet.z)
    }

    fn push_out(&self, feet: Vec3, tris: &[u32]) -> Vec2 {
        let mut total = Vec2::ZERO;
        for h in SPHERES {
            let centre = feet + Vec3::Y * h;
            let mut push = Vec2::ZERO;
            for &t in tris {
                let [a, b, c] = self.corners(t);
                let q = closest_on_triangle(centre, a, b, c);
                let away = centre - q;
                let dist = away.length();
                if dist >= RADIUS {
                    continue;
                }
                // Horizontal pushes only; a contact straight above or below (beam, floor) is
                // left to the floor query.
                let flat = Vec2::new(away.x, away.z);
                let dir = if flat.length_squared() > 1e-8 {
                    flat.normalize()
                } else {
                    let n = self.normals[t as usize];
                    let nf = Vec2::new(n.x, n.z);
                    if nf.length_squared() < 1e-4 {
                        continue;
                    }
                    nf.normalize()
                };
                let needed = (RADIUS * RADIUS - away.y * away.y).max(0.0).sqrt();
                let depth = needed - flat.dot(dir);
                if depth > push.dot(dir) {
                    push += dir * (depth - push.dot(dir));
                }
            }
            if push.length_squared() > total.length_squared() {
                total = push;
            }
        }
        total
    }
}

fn height_on(x: f32, z: f32, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a.y + l2 * b.y + l3 * c.y)
}

/// Moller-Trumbore, both faces.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t = o - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 0.0).then_some(dist)
}

/// Closest point on triangle abc to p (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

// ---------------------------------------------------------------------------------------------
// Draw params

/// The draw params JSON (`map_<id>.json` "drawparam": group -> param -> [{id, time, value}]).
#[derive(Resource)]
pub struct MapLighting {
    pub drawparam: serde_json::Value,
    pub light_set: i32,
    pub hour: f32,
    pub applied: bool,
    /// The GI probe's cube maps by time of day: (hour, `map_<id>_env_<v>` stem). Empty = none
    /// exported; then the hemisphere ambient of the light set stands in.
    pub env: Vec<(f32, String)>,
    /// The exporter's arena turn (radians about Y): the light angles are in map space.
    pub arena_yaw: f32,
    /// The colour-grading LUT ids exported as `map_<id>_lut_<n>.dds` (json "luts").
    pub luts: Vec<u32>,
    /// `map_<id>`: the asset stem of the LUT files.
    pub stem: String,
}

impl MapLighting {
    /// A param's value for the light set at the hour, interpolated between the keyed times
    /// (wrapping at midnight); id 0 when the set has none (sekiro-rs gparam::Param::at).
    pub fn at(&self, group: &str, name: &str, id: i32) -> Option<Vec<f32>> {
        let keys = self.keys(group, name, id)?;
        let hour = self.hour.rem_euclid(24.0);
        let after = keys.iter().position(|e| e.0 > hour);
        let (a, b) = match after {
            Some(0) | None => (&keys[keys.len() - 1], &keys[0]),
            Some(i) => (&keys[i - 1], &keys[i]),
        };
        let span = (b.0 - a.0).rem_euclid(24.0);
        let t = if span > 0.0 { (hour - a.0).rem_euclid(24.0) / span } else { 0.0 };
        Some(a.1.iter().zip(&b.1).map(|(x, y)| x + (y - x) * t).collect())
    }

    /// A param's value at the keyed time nearest the hour (for ids and switches, which must
    /// not be interpolated: the LUT id).
    pub fn nearest(&self, group: &str, name: &str, id: i32) -> Option<Vec<f32>> {
        let keys = self.keys(group, name, id)?;
        let hour = self.hour.rem_euclid(24.0);
        let d = |h: f32| (h - hour).rem_euclid(24.0).min((hour - h).rem_euclid(24.0));
        keys.into_iter().min_by(|a, b| d(a.0).total_cmp(&d(b.0))).map(|k| k.1)
    }

    /// The param's (time, value) keys for the light set, sorted by time; id 0's when the set
    /// has none.
    fn keys(&self, group: &str, name: &str, id: i32) -> Option<Vec<(f32, Vec<f32>)>> {
        let params = self.drawparam.as_object()?.iter().find(|(g, _)| g.contains(group))?.1.as_object()?;
        let entries = params.iter().find(|(p, _)| p.contains(name))?.1.as_array()?;
        let mut keys: Vec<(f32, Vec<f32>)> = entries
            .iter()
            .filter(|e| e["id"].as_i64() == Some(id as i64))
            .map(|e| (e["time"].as_f64().unwrap_or(0.0) as f32, e["value"].as_array().map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default()))
            .collect();
        if keys.is_empty() {
            return if id != 0 { self.keys(group, name, 0) } else { None };
        }
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(keys)
    }
}

fn colour(v: &[f32]) -> (Color, f32) {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0).max(0.0);
    (Color::linear_rgb(g(0), g(1), g(2)), v.get(3).copied().unwrap_or(1.0))
}

/// Direction toward a light from a (pitch, yaw) map-space angle: turned into the arena like
/// the pieces (`arena_yaw`), then into Bevy space (X mirrored); always from above.
fn toward(angle: &[f32], arena_yaw: f32) -> Vec3 {
    let (pitch, yaw) = (angle.first().copied().unwrap_or(-0.8), angle.get(1).copied().unwrap_or(0.0));
    let v = Quat::from_rotation_y(arena_yaw) * Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::Z;
    let v = Vec3::new(-v.x, v.y, v.z);
    if v.y < 0.0 { -v } else { v }
}

#[derive(Component)]
pub struct Sun;
#[derive(Component)]
pub struct FillLight;

/// Yebis auto exposure (ToneMap group) as Bevy's: the game meters the scene's average
/// luminance and exposes it toward "Middle Gray" (log2 target), the adaptation clamped to
/// [Adaption Min, Adaption Max] EV (at noon +-0.01: a fixed exposure; at night up to +8),
/// plus "Exposure" EV. Bevy's compensation curve gives the target exposure per average log
/// luminance L: `base + Exposure + clamp(MiddleGray + METER - L, min, max)`. `METER` shifts
/// the game's luminance units onto this renderer's buffer (LUX_PER_UNIT, Bevy's Exposure) and
/// `base` cancels the 18 h Exposure so that hour keeps the look judged against the game; both
/// by eye (`SHINOBI_MAP_METER`, `SHINOBI_MAP_EV` add to them). `SHINOBI_MAP_NO_AE=1` turns it off.
fn auto_exposure(lighting: &MapLighting, curves: &mut Assets<AutoExposureCompensationCurve>) -> Option<AutoExposure> {
    if std::env::var("SHINOBI_MAP_NO_AE").is_ok() {
        return None;
    }
    let tone = |name: &str| lighting.at("Tone Map", name, 0).and_then(|v| v.first().copied());
    if tone("Enable AutoExposure").unwrap_or(0.0) <= 0.0 {
        return None;
    }
    let knob = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    let middle_gray = tone("Middle Gray").unwrap_or(-6.6);
    let exposure = tone("ToneMap-Exposure").unwrap_or(0.8);
    // Adaption Max is 8 at night: read as +8 EV it washed the 0 h gate out white, so the
    // brightening is capped at ADAPT_CAP_EV until the night look is judged (kb/map.md open).
    let (ad_min, ad_max) = (tone("Adaption Min").unwrap_or(-0.5), tone("Adaption Max").unwrap_or(1.0).min(ADAPT_CAP_EV));
    let meter = METER_EV + knob("SHINOBI_MAP_METER");
    let base = BASE_EV + knob("SHINOBI_MAP_EV");
    let (lo, hi) = (-10.0f32, 6.0f32);
    let target = |l: f32| base + exposure + (middle_gray + meter - l).clamp(ad_min.min(ad_max), ad_max.max(ad_min));
    // Compensation c(L) = target(L) + L: slope 1 below the adaptation range, flat inside, 1 above.
    let mut xs = vec![lo, (middle_gray + meter - ad_max).clamp(lo, hi), (middle_gray + meter - ad_min).clamp(lo, hi), hi];
    xs.sort_by(|a, b| a.total_cmp(b));
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    // Snapped to 1/64 EV: Bevy rejects the spline when a segment's end sample (a + (b - a) * 1)
    // is not bit-exactly the next point, which inexact fractions like 0.2 trip; sixty-fourths
    // below 16 add exactly in f32.
    let snap = |v: f32| (v * 64.0).round() / 64.0;
    let points: Vec<Vec2> = xs.iter().map(|&l| Vec2::new(snap(l), snap(target(l) + l))).collect();
    let curve = match AutoExposureCompensationCurve::from_curve(bevy::math::cubic_splines::LinearSpline::new(points.clone())) {
        Ok(c) => c,
        Err(e) => {
            warn!("map auto exposure: no compensation curve ({e:?}) from {points:?}");
            return None;
        }
    };
    info!("map auto exposure: middle grey {middle_gray} exposure {exposure} adaption {ad_min}..{ad_max} (meter {meter}, base {base})");
    Some(AutoExposure { range: lo..=hi, compensation_curve: curves.add(curve), ..default() })
}

fn apply_lighting(
    mut lighting: ResMut<MapLighting>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<FillLight>)>,
    mut fill: Query<(&mut DirectionalLight, &mut Transform), (With<FillLight>, Without<Sun>)>,
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
    assets: Res<AssetServer>,
    mut curves: ResMut<Assets<AutoExposureCompensationCurve>>,
) {
    if lighting.applied && cameras.is_empty() {
        return;
    }
    let id = lighting.light_set;
    let get = |name: &str| lighting.at("LightSet", name, id);
    let fog = |name: &str| lighting.at("VolumetricFog", name, id);
    let (sun_color, sun_w) = colour(&get("DiffColor0").unwrap_or(vec![1.0, 0.95, 0.85, 1.5]));
    let (fill_color, fill_w) = colour(&get("DiffColor1").unwrap_or(vec![0.5, 0.6, 0.7, 0.2]));
    let (up, up_w) = colour(&get("Hemi Color Up").unwrap_or(vec![0.6, 0.7, 0.8, 0.5]));
    let (down, down_w) = colour(&get("Hemi Color Down").unwrap_or(vec![0.4, 0.4, 0.4, 0.2]));
    let (sky, _) = colour(&fog("SkyColor").unwrap_or(vec![0.5, 0.55, 0.65, 1.0]));
    let (fog_sun, fog_sun_w) = colour(&fog("DirectColor").unwrap_or(vec![1.0, 0.8, 0.6, 1.0]));
    let (ul, dl) = (up.to_linear(), down.to_linear());
    let amb = Color::linear_rgb((ul.red * up_w + dl.red * down_w) * 0.5, (ul.green * up_w + dl.green * down_w) * 0.5, (ul.blue * up_w + dl.blue * down_w) * 0.5);
    let density = fog("FixedDensity").and_then(|v| v.first().copied()).unwrap_or(0.002).max(0.0003);
    let scale = std::env::var("SHINOBI_MAP_LUX").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    for (mut l, mut t) in &mut sun {
        l.color = sun_color;
        l.illuminance = sun_w * LUX_PER_UNIT * scale;
        *t = Transform::IDENTITY.looking_to(-toward(&get("Angle0").unwrap_or_default(), lighting.arena_yaw), Vec3::Y);
    }
    for (mut l, mut t) in &mut fill {
        l.color = fill_color;
        l.illuminance = fill_w * LUX_PER_UNIT * scale;
        *t = Transform::IDENTITY.looking_to(-toward(&get("Angle1").unwrap_or_default(), lighting.arena_yaw), Vec3::Y);
    }
    // The GI probe nearest in time: real sky light and reflections (and the sky itself) in
    // place of the flat hemisphere ambient and clear colour.
    let probe = lighting.env.iter().min_by(|a, b| {
        let d = |h: f32| (h - lighting.hour).rem_euclid(24.0).min((lighting.hour - h).rem_euclid(24.0));
        d(a.0).total_cmp(&d(b.0))
    });
    let env_scale = std::env::var("SHINOBI_MAP_ENV").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    // The colour-grading LUT the draw params select at the hour (nearest key: an id).
    let lut = (std::env::var("SHINOBI_MAP_NO_LUT").is_err() && lighting.nearest("ColorGrading", "Enable", id).and_then(|v| v.first().copied()).unwrap_or(1.0) > 0.0)
        .then(|| lighting.nearest("ColorGrading", "LutSourceId", id).and_then(|v| v.first().copied()).map(|n| n as u32))
        .flatten()
        .filter(|n| lighting.luts.contains(n));
    ambient.color = amb;
    ambient.brightness = if probe.is_some() { 0.0 } else { AMBIENT_PER_UNIT * scale };
    clear.0 = sky;
    for e in &cameras {
        let mut c = commands.entity(e);
        c.insert(DistanceFog {
            color: sky,
            directional_light_color: fog_sun.with_alpha((fog_sun_w * 0.4).clamp(0.0, 1.0)),
            directional_light_exponent: 16.0,
            falloff: FogFalloff::Exponential { density },
        });
        if let Some((_, stem)) = probe {
            let specular: Handle<Image> = assets.load(format!("{stem}.dds"));
            c.insert((
                EnvironmentMapLight { diffuse_map: assets.load(format!("{}.dds", stem.replace("_env_", "_envd_"))), specular_map: specular.clone(), intensity: ENV_PER_UNIT * scale * env_scale, ..default() },
                bevy::core_pipeline::Skybox { image: Some(specular), brightness: ENV_PER_UNIT * scale * env_scale, ..default() },
            ));
        }
        if let Some(n) = lut {
            c.insert(ColorGradingLut(assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false).load(format!("{}_lut_{n:04}.dds", lighting.stem))));
        }
        if let Some(exposure) = auto_exposure(&lighting, &mut curves) {
            c.insert(exposure);
        }
    }
    if !lighting.applied {
        info!("map light set {id} at {:.1} h: sun {:?} x{sun_w}, fill x{fill_w}, sky {:?}, fog {density}, probe {:?}, lut {lut:?}", lighting.hour, sun_color, sky, probe.map(|p| &p.1));
    }
    lighting.applied = true;
}

// ---------------------------------------------------------------------------------------------
// Loading

#[derive(Component)]
pub struct MapPiece;

/// Per-vertex tangents from the UV gradients (Lengyel), accumulated per triangle: fast enough
/// for millions of triangles where Mesh::generate_tangents (MikkTSpace) is not.
fn tangents(pos: &[[f32; 3]], normal: &[[f32; 3]], uv: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    let mut tan = vec![Vec3::ZERO; pos.len()];
    let mut bit = vec![Vec3::ZERO; pos.len()];
    for t in indices.chunks_exact(3) {
        let [i0, i1, i2] = [t[0] as usize, t[1] as usize, t[2] as usize];
        let (p0, p1, p2) = (Vec3::from(pos[i0]), Vec3::from(pos[i1]), Vec3::from(pos[i2]));
        let (w0, w1, w2) = (Vec2::from(uv[i0]), Vec2::from(uv[i1]), Vec2::from(uv[i2]));
        let (e1, e2) = (p1 - p0, p2 - p0);
        let (d1, d2) = (w1 - w0, w2 - w0);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let s = (e1 * d2.y - e2 * d1.y) * r;
        let u = (e2 * d1.x - e1 * d2.x) * r;
        for i in [i0, i1, i2] {
            tan[i] += s;
            bit[i] += u;
        }
    }
    (0..pos.len())
        .map(|i| {
            let n = Vec3::from(normal[i]);
            let t = (tan[i] - n * n.dot(tan[i])).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
            let w = if n.cross(t).dot(bit[i]) < 0.0 { -1.0 } else { 1.0 };
            [t.x, t.y, t.z, w]
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn load_map(
    config: Res<GameConfig>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SekiroMaterial>>,
) {
    let Some(id) = map_id(&config) else { return };
    let dir = crate::paths::extracted();
    let t0 = std::time::Instant::now();
    // Hit collision.
    match std::fs::read(dir.join(format!("map_{id}.hit"))).ok().and_then(|d| Terrain::from_hit(&d)) {
        Some(t) => {
            info!("map {id}: {} collision triangles", t.triangle_count());
            *TERRAIN.get_or_init(|| RwLock::new(None)).write().unwrap() = Some(Arc::new(t));
        }
        None => warn!("map {id}: no map_{id}.hit (run `sekiro-extract map`)"),
    }
    // Draw params + light set.
    let json: serde_json::Value = std::fs::read_to_string(dir.join(format!("map_{id}.json"))).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let hour = std::env::var("SHINOBI_HOUR").ok().and_then(|v| v.parse().ok()).unwrap_or(config.world.hour);
    let env: Vec<(f32, String)> = json["env_hours"]
        .as_array()
        .map(|hs| {
            hs.iter()
                .enumerate()
                .filter_map(|(v, h)| {
                    let stem = format!("map_{id}_env_{v:02}");
                    (dir.join(format!("{stem}.dds")).exists() && dir.join(format!("map_{id}_envd_{v:02}.dds")).exists()).then(|| (h.as_f64().unwrap_or(0.0) as f32, stem))
                })
                .collect()
        })
        .unwrap_or_default();
    if std::env::var("SHINOBI_MAP_NO_ENV").is_ok() {
        info!("map {id}: probe cube maps off (SHINOBI_MAP_NO_ENV)");
    }
    commands.insert_resource(MapLighting {
        drawparam: json["drawparam"].clone(),
        light_set: std::env::var("SHINOBI_LIGHT_SET").ok().and_then(|v| v.parse().ok()).unwrap_or(json["light_set"].as_i64().unwrap_or(0) as i32),
        hour,
        applied: false,
        env: if std::env::var("SHINOBI_MAP_NO_ENV").is_ok() { Vec::new() } else { env },
        arena_yaw: json["arena_yaw"].as_f64().unwrap_or(0.0) as f32,
        luts: json["luts"].as_array().map(|l| l.iter().filter_map(|n| n.as_u64().map(|n| n as u32)).filter(|n| dir.join(format!("map_{id}_lut_{n:04}.dds")).exists()).collect()).unwrap_or_default(),
        stem: format!("map_{id}"),
    });
    commands.spawn((
        Sun,
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        bevy::light::CascadeShadowConfigBuilder { num_cascades: 4, maximum_distance: 120.0, first_cascade_far_bound: 12.0, ..default() }.build(),
        Transform::default(),
    ));
    commands.spawn((FillLight, DirectionalLight { shadow_maps_enabled: false, ..default() }, Transform::default()));

    // Pieces.
    let Ok(d) = std::fs::read(dir.join(format!("map_{id}.bin"))) else {
        warn!("map {id}: no map_{id}.bin");
        return;
    };
    if &d[0..4] != b"SHMP" {
        return;
    }
    let u16_ = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let string = |o: &mut usize| {
        let l = u16_(*o) as usize;
        let s = String::from_utf8_lossy(&d[*o + 2..*o + 2 + l]).into_owned();
        *o += 2 + l;
        s
    };
    let no_normals = std::env::var("SHINOBI_MAP_NO_NORMALS").is_ok();
    // Debug views: SHINOBI_MAP_UNLIT=1 shows the raw albedo, SHINOBI_MAP_TEX=<stem> puts one
    // texture on every piece (a UV check).
    let unlit = std::env::var("SHINOBI_MAP_UNLIT").is_ok();
    let tex_override = std::env::var("SHINOBI_MAP_TEX").ok().filter(|s| !s.is_empty());
    let n = u32_(8) as usize;
    let mut o = 12;
    let mut tris = 0;
    let mut mat_cache: HashMap<(String, String, u8, bool), Handle<SekiroMaterial>> = HashMap::new();
    for _ in 0..n {
        let mut albedo = string(&mut o);
        let normal = string(&mut o);
        if let Some(t) = &tex_override {
            if !albedo.is_empty() {
                albedo = t.clone();
            }
        }
        let alpha = d[o];
        let two_sided = d[o + 1] != 0;
        o += 2;
        let vc = u32_(o) as usize;
        o += 4;
        let mut pos = Vec::with_capacity(vc);
        let mut nrm = Vec::with_capacity(vc);
        let mut uv = Vec::with_capacity(vc);
        for i in 0..vc {
            let v = o + i * 32;
            pos.push([-f(v), f(v + 4), f(v + 8)]);
            nrm.push([-f(v + 12), f(v + 16), f(v + 20)]);
            uv.push([f(v + 24), f(v + 28)]);
        }
        o += vc * 32;
        let ic = u32_(o) as usize;
        o += 4;
        let mut indices = Vec::with_capacity(ic);
        for t in 0..ic / 3 {
            let i = o + t * 12;
            // Mirroring flips the winding.
            indices.extend([u32_(i), u32_(i + 8), u32_(i + 4)]);
        }
        o += ic * 4;
        tris += ic / 3;
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        if !normal.is_empty() && !no_normals {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents(&pos, &nrm, &uv, &indices));
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_indices(Indices::U32(indices));
        let key = (albedo.clone(), normal.clone(), alpha, two_sided);
        let material = mat_cache
            .entry(key)
            .or_insert_with(|| {
                // Map textures tile (UVs run far outside 0..1); Bevy's default sampler clamps to
                // the edge, which smears one edge texel over the whole piece (flat grey beams).
                let tex = |stem: &str, srgb: bool| {
                    assets
                        .load_builder()
                        .with_settings(move |s: &mut ImageLoaderSettings| {
                            s.is_srgb = srgb;
                            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                                address_mode_u: ImageAddressMode::Repeat,
                                address_mode_v: ImageAddressMode::Repeat,
                                mag_filter: ImageFilterMode::Linear,
                                min_filter: ImageFilterMode::Linear,
                                mipmap_filter: ImageFilterMode::Linear,
                                anisotropy_clamp: 8,
                                ..default()
                            });
                        })
                        .load(format!("tex/{stem}.dds"))
                };
                let normal_h = (!normal.is_empty() && !no_normals).then(|| tex(&normal, false));
                materials.add(SekiroMaterial {
                    base: StandardMaterial {
                        base_color_texture: (!albedo.is_empty()).then(|| tex(&albedo, true)),
                        base_color: if albedo.is_empty() { Color::srgb(0.45, 0.43, 0.40) } else { Color::WHITE },
                        perceptual_roughness: 0.85,
                        unlit,
                        alpha_mode: match alpha {
                            1 => AlphaMode::AlphaToCoverage,
                            2 => AlphaMode::Blend,
                            _ => AlphaMode::Opaque,
                        },
                        double_sided: two_sided,
                        cull_mode: if two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                        ..default()
                    },
                    extension: SekiroExt { flags: UVec4::new(normal_h.is_some() as u32, 0, 0, 0), normal: normal_h, metallic: None },
                })
            })
            .clone();
        commands.spawn((MapPiece, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default()));
    }
    info!("map {id}: {n} meshes, {tris} triangles, {} materials, loaded in {:.1} s", mat_cache.len(), t0.elapsed().as_secs_f32());
}

// ---------------------------------------------------------------------------------------------
// Bodies on the map

/// Where the actor was before this tick's movement, for the wall slide.
#[derive(Component)]
pub struct OnTerrain {
    prev: Vec3,
}

/// After the actors moved: push them out of walls along their step, then put grounded ones on
/// the floor (following it up to STEP_HEIGHT up or down; a bigger drop leaves them in the air,
/// which player.rs turns into FreeFall; enemies stay put, they have no fall of their own).
fn bodies_on_terrain(combat: Res<crate::data::Combat>, mut commands: Commands, mut q: Query<(Entity, &Actor, &mut Transform, Option<&mut OnTerrain>)>) {
    let Some(t) = terrain() else { return };
    for (e, a, mut tf, state) in &mut q {
        let Some(mut state) = state else {
            commands.entity(e).insert(OnTerrain { prev: tf.translation });
            continue;
        };
        let prev = state.prev;
        let feet_prev = prev - Vec3::Y * CAPSULE_HALF_HEIGHT;
        let delta = Vec2::new(tf.translation.x - prev.x, tf.translation.z - prev.z);
        if delta.length_squared() > 1e-10 {
            let moved = t.slide(feet_prev, delta);
            tf.translation.x = prev.x + moved.x;
            tf.translation.z = prev.z + moved.y;
        }
        let data = crate::actor::data_for(&combat, a.side);
        let lifted = !a.anim.is_empty() && data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY);
        if !a.airborne && !lifted {
            let feet = tf.translation.y - CAPSULE_HALF_HEIGHT;
            let floor = t.floor(tf.translation.x, tf.translation.z, feet + STEP_HEIGHT);
            match floor {
                Some(h) if h >= feet - STEP_HEIGHT || a.side == Side::Enemy => tf.translation.y = h + CAPSULE_HALF_HEIGHT,
                Some(_) => {}
                None => {
                    // Over nothing walkable within reach: the enemy keeps its height.
                }
            }
        }
        state.prev = tf.translation;
    }
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AutoExposurePlugin>() {
            app.add_plugins(AutoExposurePlugin);
        }
        app.add_plugins(GradingPlugin);
        app.add_systems(Startup, load_map)
            .add_systems(Update, (apply_lighting.run_if(resource_exists::<MapLighting>), frame_rate_log))
            .add_systems(FixedUpdate, bodies_on_terrain.after(ActorSet::Advance));
    }
}

/// One line with the average frame rate over seconds 5..15 after start (the map's cost).
fn frame_rate_log(time: Res<Time>, mut acc: Local<(f32, u32, bool)>) {
    let t = time.elapsed_secs();
    if acc.2 || t < 5.0 || terrain().is_none() {
        return;
    }
    acc.0 += time.delta_secs();
    acc.1 += 1;
    if t >= 15.0 {
        acc.2 = true;
        info!("map frame rate: {:.0} fps over {} frames", acc.1 as f32 / acc.0, acc.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit() -> Option<Terrain> {
        let d = std::fs::read(crate::paths::extracted().join("map_m11_01_00_00.hit")).ok()?;
        Terrain::from_hit(&d)
    }

    /// Prints wall distances and slides around the spawn spots (`-- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn arena_probe() {
        let Some(t) = hit() else { return };
        for z in [-6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0] {
            let y = t.floor(0.0, z, 1.0).unwrap_or(f32::NAN);
            let feet = Vec3::new(0.0, y, z);
            let wall = |d: Vec3| t.raycast(feet + Vec3::Y * 1.0, d, 30.0).map_or(-1.0, |d| d);
            println!(
                "z {z:5.1}: floor {y:6.2}  down {:?} up {:?}  wall +x {:5.1} -x {:5.1} +z {:5.1} -z {:5.1}  slide +x {:?} -x {:?}",
                t.raycast(feet + Vec3::Y * 1.5, -Vec3::Y, 3.0),
                t.raycast(feet + Vec3::Y * 1.5, Vec3::Y, 30.0),
                wall(Vec3::X), wall(-Vec3::X), wall(Vec3::Z), wall(-Vec3::Z),
                t.slide(feet, Vec2::new(30.0, 0.0)), t.slide(feet, Vec2::new(-30.0, 0.0))
            );
        }
    }

    /// The exported arena: both spawn spots stand on a floor close to the origin height, the
    /// floor has a hit material, and the gate's walls stop a body walking sideways.
    #[test]
    fn arena_floor_and_walls() {
        let Some(t) = hit() else {
            eprintln!("no map_m11_01_00_00.hit: skipped");
            return;
        };
        assert!(t.triangle_count() > 100_000);
        for z in [4.0, -4.0, 0.0] {
            let h = t.floor(0.0, z, 1.0).unwrap_or_else(|| panic!("no floor at z {z}"));
            assert!((-1.5..=0.5).contains(&h), "floor at z {z} = {h}");
            assert!(t.floor_tri(0.0, z, 1.0).is_some_and(|(_, tri)| t.materials[tri as usize] != u32::MAX));
        }
        // Walking sideways out of the gate passage in tick-sized moves ends at its posts
        // (raycast: 1.9 m either side at z = 2).
        for sign in [1.0, -1.0] {
            let mut feet = Vec3::new(0.0, t.floor(0.0, 2.0, 1.0).unwrap(), 2.0);
            for _ in 0..200 {
                let d = t.slide(feet, Vec2::new(sign * 0.15, 0.0));
                feet.x += d.x;
                feet.z += d.y;
                feet.y = t.floor(feet.x, feet.z, feet.y + STEP_HEIGHT).unwrap_or(feet.y);
            }
            assert!(feet.x.abs() < 2.5, "walked through the gate's post: {feet}");
        }
        // The camera ray from head height straight down hits the floor, and up it meets the
        // gate's lintel (a ceiling about 3.2 m up at z = 2).
        let feet = Vec3::new(0.0, t.floor(0.0, 2.0, 1.0).unwrap(), 2.0);
        assert!(t.raycast(feet + Vec3::Y * 1.5, -Vec3::Y, 3.0).is_some_and(|d| (d - 1.5).abs() < 0.1));
        assert!(t.raycast(feet + Vec3::Y * 1.5, Vec3::Y, 30.0).is_some_and(|d| d < 5.0));
    }
}
