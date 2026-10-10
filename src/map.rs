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
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve, AutoExposurePlugin};
use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, Side};
use crate::config::GameConfig;
use crate::grading::{ColorGradingLut, GradingPlugin};
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

/// Set when the loaded map is a boss's own arena (boss_arena).
static BOSS_ARENA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Is the fight on the boss's own arena (its MSB surroundings, so script positions hold)?
pub fn on_boss_arena() -> bool {
    BOSS_ARENA.load(std::sync::atomic::Ordering::Relaxed) && terrain().is_some()
}

pub fn terrain() -> Option<Arc<Terrain>> {
    TERRAIN.get_or_init(|| RwLock::new(None)).read().ok()?.clone()
}

/// The map id in force: config `[world] map`, overridden by `SHINOBI_MAP` ("" = none). With the
/// config's map on (not the sandbox), a boss fights in its own arena when one is exported
/// (boss_arena).
pub fn map_id(config: &GameConfig) -> Option<String> {
    // "", "off" or "0" turn the map off for one run (PowerShell drops a variable set to "").
    match std::env::var("SHINOBI_MAP") {
        Ok(v) => (!v.is_empty() && v != "off" && v != "0").then_some(v),
        Err(_) => {
            let m = config.world.map.clone().filter(|m| !m.is_empty())?;
            if m == crate::sandbox::MAP_ID {
                return Some(m);
            }
            Some(boss_arena(&config.enemy.chr, config.enemy.npc_row).unwrap_or(m))
        }
    }
}

/// The boss's own arena: `extracted/map_boss_arenas.json` (tools/boss_arenas.py) maps a boss
/// script's NpcParam row to the map exported around the boss's MSB placement (its map and
/// entity id from the script, enemy/script.rs). The row is the config's, else the chr's default
/// (its roster entry, as data.rs picks it).
pub fn boss_arena(chr: &str, npc_row: Option<i64>) -> Option<String> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<(String, Option<i64>), Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().ok()?.get(&(chr.to_string(), npc_row)) {
        return v.clone();
    }
    let dir = crate::paths::extracted();
    let found = (|| {
        let index: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("map_boss_arenas.json")).ok()?).ok()?;
        let row = match npc_row {
            Some(r) => r,
            None => {
                let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("enemies/{chr}.json"))).ok()?).ok()?;
                v["roster"]["npc"].as_i64()?
            }
        };
        let stem = index[row.to_string()].as_str()?.to_string();
        dir.join(format!("map_{stem}.bin")).exists().then_some(stem)
    })();
    if let Ok(mut c) = cache.lock() {
        c.insert((chr.to_string(), npc_row), found.clone());
    }
    found
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
        Some(Self::from_tris(verts, (0..nt).map(|i| {
            let k = o + i * 16;
            ([u32_(k), u32_(k + 4), u32_(k + 8)], u32_(k + 12))
        })))
    }

    /// Collision from triangles already in Bevy space (`(indices, material)` each): the hit
    /// files and the sandbox map's generated floor and walls.
    pub(crate) fn from_tris(verts: Vec<Vec3>, tris: impl Iterator<Item = ([u32; 3], u32)>) -> Self {
        let nv = verts.len();
        let mut t = Terrain { verts, tris: Vec::new(), normals: Vec::new(), materials: Vec::new(), cells: HashMap::new(), min_walk_ny: MAX_SLOPE_DEGREES.to_radians().cos() };
        for (tri, material) in tris {
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
            t.materials.push(material);
            let (x0, z0) = cell_of(a.x.min(b.x).min(c.x), a.z.min(b.z).min(c.z));
            let (x1, z1) = cell_of(a.x.max(b.x).max(c.x), a.z.max(b.z).max(c.z));
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    t.cells.entry((cx, cz)).or_default().push(id);
                }
            }
        }
        t
    }

    pub(crate) fn install(self) {
        *TERRAIN.get_or_init(|| RwLock::new(None)).write().unwrap() = Some(Arc::new(self));
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
// Map material: the game's layered map materials (map_material.wgsl, kb/map.md "Layers").

/// StandardMaterial (base albedo, alpha mode) + the base normal map, the overlay and the snow
/// layers blended by the vertex blend bytes (ATTRIBUTE_COLOR) over the two UV sets.
pub type MapMaterial = bevy::pbr::ExtendedMaterial<StandardMaterial, MapExt>;

#[derive(Asset, bevy::render::render_resource::AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct MapExt {
    #[texture(100)]
    #[sampler(101)]
    pub normal: Option<Handle<Image>>,
    #[texture(102)]
    #[sampler(103)]
    pub over_albedo: Option<Handle<Image>>,
    #[texture(104)]
    #[sampler(105)]
    pub over_normal: Option<Handle<Image>>,
    #[texture(106)]
    #[sampler(107)]
    pub snow_albedo: Option<Handle<Image>>,
    #[texture(108)]
    #[sampler(109)]
    pub snow_normal: Option<Handle<Image>>,
    #[texture(112)]
    #[sampler(113)]
    pub c_albedo: Option<Handle<Image>>,
    #[texture(114)]
    #[sampler(115)]
    pub c_normal: Option<Handle<Image>>,
    #[texture(116)]
    #[sampler(117)]
    pub mask: Option<Handle<Image>>,
    /// x: has a mask, y: blend softness (`SHINOBI_MAP_SOFT`, default 0.25; 0 = plain weights).
    #[uniform(118)]
    pub mask_params: Vec4,
    /// x: base normal map (bit 1: layer C, bit 2: with normal), y: overlay (bit 1: with
    /// normal), z: snow (bit 1: with normal), w: base albedo (bit 1: show the blend weights,
    /// `SHINOBI_MAP_SHOW_BLEND=1`).
    #[uniform(110)]
    pub flags: UVec4,
    /// x: overlay uv set, y: snow uv set, z / w: snow up-facing smoothstep on N.y.
    #[uniform(111)]
    pub params: Vec4,
}

impl bevy::pbr::MaterialExtension for MapExt {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/map_material.wgsl".into()
    }
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

/// The probe cube maps for the hour: (label, specular, diffuse). Between two variants the
/// RGBA16F DDS files (`map_<id>_env_<v>.dds` / `_envd_`) are blended pixel by pixel and
/// added as images (same layout: the exporter writes every variant the same way); within
/// 2 % of a variant's hour that variant's files load as they are.
fn probe_maps(lighting: &MapLighting, assets: &AssetServer, images: &mut Assets<Image>) -> Option<(String, Handle<Image>, Handle<Image>)> {
    if lighting.env.is_empty() {
        return None;
    }
    let mut env: Vec<&(f32, String)> = lighting.env.iter().collect();
    env.sort_by(|a, b| a.0.total_cmp(&b.0));
    let hour = lighting.hour.rem_euclid(24.0);
    let (a, b) = match env.iter().position(|e| e.0 > hour) {
        Some(0) | None => (env[env.len() - 1], env[0]),
        Some(i) => (env[i - 1], env[i]),
    };
    let span = (b.0 - a.0).rem_euclid(24.0);
    let t = if span > 0.0 { (hour - a.0).rem_euclid(24.0) / span } else { 0.0 };
    let single = |e: &(f32, String)| Some((e.1.clone(), assets.load(format!("{}.dds", e.1)), assets.load(format!("{}.dds", e.1.replace("_env_", "_envd_")))));
    if t < 0.02 {
        return single(a);
    }
    if t > 0.98 {
        return single(b);
    }
    let dir = crate::paths::extracted();
    let mut blend = |suffix: &str| -> Option<Handle<Image>> {
        let read = |stem: &str| std::fs::read(dir.join(format!("{}.dds", stem.replace("_env_", suffix)))).ok();
        let bytes = blend_f16_dds(&read(&a.1)?, &read(&b.1)?, t)?;
        let image = Image::from_buffer(&bytes, ImageType::Extension("dds"), CompressedImageFormats::all(), false, ImageSampler::Default, RenderAssetUsages::RENDER_WORLD)
            .map_err(|e| warn!("map probe blend: {e}"))
            .ok()?;
        Some(images.add(image))
    };
    match (blend("_env_"), blend("_envd_")) {
        (Some(spec), Some(diff)) => Some((format!("{} x{:.2} + {} x{:.2}", a.1, 1.0 - t, b.1, t), spec, diff)),
        _ => single(if t < 0.5 { a } else { b }),
    }
}

/// `a * (1 - t) + b * t` over two RGBA16F DDS files of the same layout (148-byte DX10
/// header kept from `a`).
fn blend_f16_dds(a: &[u8], b: &[u8], t: f32) -> Option<Vec<u8>> {
    const HEADER: usize = 148;
    if a.len() != b.len() || a.len() < HEADER || &a[..4] != b"DDS " || a[..HEADER] != b[..HEADER] {
        return None;
    }
    let mut out = a[..HEADER].to_vec();
    out.reserve(a.len() - HEADER);
    for (x, y) in a[HEADER..].chunks_exact(2).zip(b[HEADER..].chunks_exact(2)) {
        let v = half_to_f32(u16::from_le_bytes([x[0], x[1]])) * (1.0 - t) + half_to_f32(u16::from_le_bytes([y[0], y[1]])) * t;
        out.extend_from_slice(&f32_to_half(v).to_le_bytes());
    }
    Some(out)
}

fn half_to_f32(h: u16) -> f32 {
    let (sign, exp, mant) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    let bits = match exp {
        0 if mant == 0 => sign << 31,
        0 => {
            // Subnormal: normalise.
            let (mut e, mut m) = (113u32, mant);
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | (e << 23) | ((m & 0x3ff) << 13)
        }
        31 => (sign << 31) | 0x7f80_0000 | (mant << 13),
        _ => (sign << 31) | ((exp + 112) << 23) | (mant << 13),
    };
    f32::from_bits(bits)
}

fn f32_to_half(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - exp + 13);
        return sign | m as u16;
    }
    sign | ((exp as u16) << 10) | (mant >> 13) as u16
}

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
    cameras: Query<Entity, With<Camera3d>>,
    new_cameras: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
    assets: Res<AssetServer>,
    mut curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Once at start, again for a camera that has none of it yet, and whenever `applied` is
    // cleared (the hour keys).
    if lighting.applied && new_cameras.is_empty() {
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
    // The GI probe at the hour (blended between the two variants around it): real sky light
    // and reflections (and the sky itself) in place of the flat hemisphere ambient and clear
    // colour.
    let probe = (!cameras.is_empty()).then(|| probe_maps(&lighting, &assets, &mut images)).flatten();
    let env_scale = std::env::var("SHINOBI_MAP_ENV").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    // The colour-grading LUT the draw params select at the hour (nearest key: an id).
    let lut = (std::env::var("SHINOBI_MAP_NO_LUT").is_err() && lighting.nearest("ColorGrading", "Enable", id).and_then(|v| v.first().copied()).unwrap_or(1.0) > 0.0)
        .then(|| lighting.nearest("ColorGrading", "LutSourceId", id).and_then(|v| v.first().copied()).map(|n| n as u32))
        .flatten()
        .filter(|n| lighting.luts.contains(n));
    ambient.color = amb;
    ambient.brightness = if lighting.env.is_empty() { AMBIENT_PER_UNIT * scale } else { 0.0 };
    clear.0 = sky;
    for e in &cameras {
        let mut c = commands.entity(e);
        c.insert(DistanceFog {
            color: sky,
            directional_light_color: fog_sun.with_alpha((fog_sun_w * 0.4).clamp(0.0, 1.0)),
            directional_light_exponent: 16.0,
            falloff: FogFalloff::Exponential { density },
        });
        if let Some((_, specular, diffuse)) = &probe {
            c.insert((
                EnvironmentMapLight { diffuse_map: diffuse.clone(), specular_map: specular.clone(), intensity: ENV_PER_UNIT * scale * env_scale, ..default() },
                bevy::core_pipeline::Skybox { image: Some(specular.clone()), brightness: ENV_PER_UNIT * scale * env_scale, ..default() },
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
        info!("map light set {id} at {:.1} h: sun {:?} x{sun_w}, fill x{fill_w}, sky {:?}, fog {density}, probe {:?}, lut {lut:?}", lighting.hour, sun_color, sky, probe.as_ref().map(|p| &p.0));
    }
    lighting.applied = true;
}

/// `[` / `]`: the time of day one hour back / on (wrapping), the whole lighting re-applied
/// (light set, probe, LUT, exposure): a walk through the day on any map.
fn hour_keys(keys: Res<ButtonInput<KeyCode>>, mut lighting: ResMut<MapLighting>, mut log: ResMut<crate::hud::CombatLog>) {
    let step = if keys.just_pressed(KeyCode::BracketRight) {
        1.0
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        -1.0
    } else {
        return;
    };
    lighting.hour = (lighting.hour + step).rem_euclid(24.0);
    lighting.applied = false;
    log.push(format!("map: {:.0} h", lighting.hour), Color::WHITE);
}

// ---------------------------------------------------------------------------------------------
// Loading

#[derive(Component)]
pub struct MapPiece;

/// Per-vertex tangents from the UV gradients (Lengyel), accumulated per triangle: fast enough
/// for millions of triangles where Mesh::generate_tangents (MikkTSpace) is not.
pub(crate) fn tangents(pos: &[[f32; 3]], normal: &[[f32; 3]], uv: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
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
    mut materials: ResMut<Assets<MapMaterial>>,
) {
    let Some(map) = map_id(&config) else { return };
    let dir = crate::paths::extracted();
    let t0 = std::time::Instant::now();
    // The sandbox (sandbox.rs): a generated arena under a real map's lighting and materials.
    let sandbox = map == crate::sandbox::MAP_ID;
    let id = if sandbox { crate::sandbox::source() } else { map.clone() };
    BOSS_ARENA.store(id.starts_with("boss_"), std::sync::atomic::Ordering::Relaxed);
    // Hit collision.
    if !sandbox {
        match std::fs::read(dir.join(format!("map_{id}.hit"))).ok().and_then(|d| Terrain::from_hit(&d)) {
            Some(t) => {
                info!("map {id}: {} collision triangles", t.triangle_count());
                t.install();
            }
            None => warn!("map {id}: no map_{id}.hit (run `sekiro-extract map`)"),
        }
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
    // Sun shadows: `SHINOBI_MAP_SHADOW=<cascades>,<max m>` (default 2,60: the cascades are the
    // map's frame-rate cost, kb/map.md), `SHINOBI_MAP_NO_SHADOW=1` none.
    let shadow: Option<(usize, f32)> = std::env::var("SHINOBI_MAP_SHADOW").ok().and_then(|v| {
        let d: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (d.len() == 2).then(|| (d[0].max(1.0) as usize, d[1]))
    });
    let (cascades, max_distance) = shadow.unwrap_or((2, 60.0));
    if let Some(size) = std::env::var("SHINOBI_MAP_SHADOW_RES").ok().and_then(|v| v.parse::<usize>().ok()) {
        commands.insert_resource(bevy::light::DirectionalLightShadowMap { size });
    }
    commands.spawn((
        Sun,
        DirectionalLight { shadow_maps_enabled: std::env::var("SHINOBI_MAP_NO_SHADOW").is_err(), ..default() },
        bevy::light::CascadeShadowConfigBuilder { num_cascades: cascades, maximum_distance: max_distance, first_cascade_far_bound: 12.0, ..default() }.build(),
        Transform::default(),
    ));
    commands.spawn((FillLight, DirectionalLight { shadow_maps_enabled: false, ..default() }, Transform::default()));

    // Pieces.
    let Ok(d) = std::fs::read(dir.join(format!("map_{id}.bin"))) else {
        warn!("map {id}: no map_{id}.bin");
        return;
    };
    let options = MaterialOptions::from_env();
    let mut mats = MaterialCache::default();
    if sandbox {
        crate::sandbox::build(&d, &mut commands, &assets, &mut meshes, &mut materials, &mut mats, &options);
        info!("map sandbox ({id}): loaded in {:.1} s", t0.elapsed().as_secs_f32());
        return;
    }
    let tex_override = options.tex_override.clone();
    let mut n = 0;
    let mut tris = 0;
    let mut no_cast = 0;
    let ok = walk_bin(&d, |b| {
        n += 1;
        tris += b.index_count / 3;
        let mut key = b.key();
        if options.no_layers {
            key.0[2..].iter_mut().for_each(|l| l.clear());
        }
        if let Some(t) = &tex_override {
            if !key.0[0].is_empty() {
                key.0[0] = t.clone();
            }
        }
        let layered = key.layered();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        let (pos, nrm, uv, uv2, blend) = b.vertices();
        let indices = b.indices();
        if !key.0[1].is_empty() && !options.no_normals {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents(&pos, &nrm, &uv, &indices));
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        if layered {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv2);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, blend);
        }
        mesh.insert_indices(Indices::U32(indices));
        let material = mats.get(&assets, &mut materials, &options, &key);
        let mut e = commands.spawn((MapPiece, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default()));
        if !b.casts {
            e.insert(bevy::light::NotShadowCaster);
            no_cast += 1;
        }
    });
    if ok.is_none() {
        warn!("map {id}: map_{id}.bin is not a SHMP file");
    }
    info!("map {id}: {n} meshes ({no_cast} without shadows), {tris} triangles, {} materials, loaded in {:.1} s", mats.len(), t0.elapsed().as_secs_f32());
}

// ---------------------------------------------------------------------------------------------
// The SHMP bin and its materials (shared with the sandbox)

/// A material of the bin: the nine texture stems (base albedo / normal, overlay albedo /
/// normal, snow albedo / normal, layer C albedo / normal, mask), the alpha mode and two-sided.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct MaterialKey(pub [String; 9], pub u8, pub bool);

impl MaterialKey {
    /// Has a layer beyond the base (needs the blend bytes and the second UV set).
    pub(crate) fn layered(&self) -> bool {
        !self.0[2].is_empty() || !self.0[4].is_empty() || !self.0[6].is_empty()
    }
}

/// One batch of the bin, with its vertex and index data still packed.
pub(crate) struct BinBatch<'a> {
    d: &'a [u8],
    version: u32,
    albedo: String,
    normal: String,
    layers: [String; 7],
    pub alpha: u8,
    pub two_sided: bool,
    pub casts: bool,
    pub vertex_count: usize,
    vertices_at: usize,
    pub index_count: usize,
    indices_at: usize,
}

impl BinBatch<'_> {
    pub(crate) fn key(&self) -> MaterialKey {
        let l = &self.layers;
        MaterialKey([self.albedo.clone(), self.normal.clone(), l[0].clone(), l[1].clone(), l[2].clone(), l[3].clone(), l[4].clone(), l[5].clone(), l[6].clone()], self.alpha, self.two_sided)
    }

    /// Positions, normals (X mirrored into Bevy space), the two UV sets and the blend bytes
    /// (0..1), in that order.
    #[allow(clippy::type_complexity)]
    pub(crate) fn vertices(&self) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<[f32; 4]>) {
        let d = self.d;
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let vc = self.vertex_count;
        let stride = if self.version >= 3 { 44 } else { 32 };
        let mut pos = Vec::with_capacity(vc);
        let mut nrm = Vec::with_capacity(vc);
        let mut uv = Vec::with_capacity(vc);
        let mut uv2 = Vec::with_capacity(vc);
        let mut blend: Vec<[f32; 4]> = Vec::with_capacity(vc);
        for i in 0..vc {
            let v = self.vertices_at + i * stride;
            pos.push([-f(v), f(v + 4), f(v + 8)]);
            nrm.push([-f(v + 12), f(v + 16), f(v + 20)]);
            uv.push([f(v + 24), f(v + 28)]);
            if self.version >= 3 {
                uv2.push([f(v + 32), f(v + 36)]);
                blend.push([d[v + 40] as f32 / 255.0, d[v + 41] as f32 / 255.0, d[v + 42] as f32 / 255.0, d[v + 43] as f32 / 255.0]);
            }
        }
        (pos, nrm, uv, uv2, blend)
    }

    /// The triangle list, winding flipped for the mirrored X.
    pub(crate) fn indices(&self) -> Vec<u32> {
        let d = self.d;
        let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let mut indices = Vec::with_capacity(self.index_count);
        for t in 0..self.index_count / 3 {
            let i = self.indices_at + t * 12;
            indices.extend([u32_(i), u32_(i + 8), u32_(i + 4)]);
        }
        indices
    }
}

/// Walks the batches of a SHMP bin (versions 1-4; kb/map.md "Bin"); None when it is no SHMP.
/// Version 2 adds a shadow-caster byte per batch, 3 the layers and 44-byte vertices, 4 the
/// mask.
pub(crate) fn walk_bin(d: &[u8], mut f: impl FnMut(BinBatch)) -> Option<()> {
    if d.len() < 12 || &d[0..4] != b"SHMP" {
        return None;
    }
    let u16_ = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let string = |o: &mut usize| {
        let l = u16_(*o) as usize;
        let s = String::from_utf8_lossy(&d[*o + 2..*o + 2 + l]).into_owned();
        *o += 2 + l;
        s
    };
    let version = u32_(4);
    let n = u32_(8) as usize;
    let mut o = 12;
    for _ in 0..n {
        let albedo = string(&mut o);
        let normal = string(&mut o);
        let mut layers: [String; 7] = Default::default();
        if version >= 3 {
            for l in layers.iter_mut().take(if version >= 4 { 7 } else { 6 }) {
                *l = string(&mut o);
            }
        }
        let alpha = d[o];
        let two_sided = d[o + 1] != 0;
        o += 2;
        let casts = version < 2 || {
            o += 1;
            d[o - 1] != 0
        };
        let vertex_count = u32_(o) as usize;
        o += 4;
        let vertices_at = o;
        o += vertex_count * if version >= 3 { 44 } else { 32 };
        let index_count = u32_(o) as usize;
        o += 4;
        let indices_at = o;
        o += index_count * 4;
        f(BinBatch { d, version, albedo, normal, layers, alpha, two_sided, casts, vertex_count, vertices_at, index_count, indices_at });
    }
    Some(())
}

/// The environment knobs of the map materials (kb/map.md "Knobs").
pub(crate) struct MaterialOptions {
    /// `SHINOBI_MAP_NO_NORMALS=1`: no normal maps.
    pub no_normals: bool,
    /// `SHINOBI_MAP_UNLIT=1`: the raw albedo.
    pub unlit: bool,
    /// `SHINOBI_MAP_TEX=<stem>`: one texture on every piece (a UV check).
    pub tex_override: Option<String>,
    /// `SHINOBI_MAP_NO_LAYERS=1`: the base layer only.
    pub no_layers: bool,
    /// `SHINOBI_MAP_SOFT`: the mask blend softness (default 0.25).
    pub softness: f32,
    /// `SHINOBI_MAP_SNOW=a,b`: the up-facing smoothstep (N.y) of the snow (default 0.2..0.7).
    pub snow_up: [f32; 2],
    /// `SHINOBI_MAP_SHOW_BLEND=1` (bit 1) and `SHINOBI_MAP_BYTE2` = c (bit 3, default) / ao
    /// (bit 2) / off, as the w flags of map_material.wgsl.
    pub w_flags: u32,
}

impl MaterialOptions {
    pub(crate) fn from_env() -> Self {
        let snow_up: [f32; 2] = std::env::var("SHINOBI_MAP_SNOW")
            .ok()
            .and_then(|v| {
                let d: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (d.len() == 2).then(|| [d[0], d[1]])
            })
            .unwrap_or([0.2, 0.7]);
        // Byte 2 blends layer C (slots 7 / 0; kb/map.md "Layers" on why).
        let byte2 = match std::env::var("SHINOBI_MAP_BYTE2").as_deref() {
            Ok("ao") => 4,
            Ok("off") => 0,
            _ => 8,
        };
        Self {
            no_normals: std::env::var("SHINOBI_MAP_NO_NORMALS").is_ok(),
            unlit: std::env::var("SHINOBI_MAP_UNLIT").is_ok(),
            tex_override: std::env::var("SHINOBI_MAP_TEX").ok().filter(|s| !s.is_empty()),
            no_layers: std::env::var("SHINOBI_MAP_NO_LAYERS").is_ok(),
            softness: std::env::var("SHINOBI_MAP_SOFT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25),
            snow_up,
            w_flags: ((std::env::var("SHINOBI_MAP_SHOW_BLEND").is_ok() as u32) << 1) | byte2,
        }
    }
}

/// The map materials built so far, one per key.
#[derive(Default)]
pub(crate) struct MaterialCache(HashMap<MaterialKey, Handle<MapMaterial>>);

impl MaterialCache {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// The material for the key, built on first use (kb/map.md "Layers").
    pub(crate) fn get(&mut self, assets: &AssetServer, materials: &mut Assets<MapMaterial>, o: &MaterialOptions, key: &MaterialKey) -> Handle<MapMaterial> {
        if let Some(h) = self.0.get(key) {
            return h.clone();
        }
        let MaterialKey(names, alpha, two_sided) = key;
        let [albedo, normal, over_a, over_n, snow_a, snow_n, c_a, c_n, mask_t] = names;
        let layered = key.layered();
        // Map textures tile (UVs run far outside 0..1); Bevy's default sampler clamps to the
        // edge, which smears one edge texel over the whole piece (flat grey beams).
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
        let normal_h = (!normal.is_empty() && !o.no_normals).then(|| tex(normal, false));
        let over = (!over_a.is_empty()).then(|| tex(over_a, true));
        let over_n = (over.is_some() && !over_n.is_empty() && !o.no_normals).then(|| tex(over_n, false));
        let snow = (!snow_a.is_empty()).then(|| tex(snow_a, true));
        let snow_n = (snow.is_some() && !snow_n.is_empty() && !o.no_normals).then(|| tex(snow_n, false));
        let c = (!c_a.is_empty()).then(|| tex(c_a, true));
        let c_n = (c.is_some() && !c_n.is_empty() && !o.no_normals).then(|| tex(c_n, false));
        let mask = (layered && !mask_t.is_empty()).then(|| tex(mask_t, false));
        let bits = |a: &Option<Handle<Image>>, n: &Option<Handle<Image>>| a.is_some() as u32 | ((n.is_some() as u32) << 1);
        let flags = UVec4::new(normal_h.is_some() as u32 | (bits(&c, &c_n) << 1), bits(&over, &over_n), bits(&snow, &snow_n), (!albedo.is_empty()) as u32 | o.w_flags);
        let h = materials.add(MapMaterial {
            base: StandardMaterial {
                base_color_texture: (!albedo.is_empty()).then(|| tex(albedo, true)),
                base_color: if albedo.is_empty() { Color::srgb(0.45, 0.43, 0.40) } else { Color::WHITE },
                perceptual_roughness: 0.85,
                unlit: o.unlit,
                alpha_mode: match alpha {
                    1 => AlphaMode::AlphaToCoverage,
                    2 => AlphaMode::Blend,
                    _ => AlphaMode::Opaque,
                },
                double_sided: *two_sided,
                cull_mode: if *two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..default()
            },
            extension: MapExt { normal: normal_h, over_albedo: over, over_normal: over_n, snow_albedo: snow, snow_normal: snow_n, c_albedo: c, c_normal: c_n, mask_params: Vec4::new(mask.is_some() as u32 as f32, o.softness, 0.0, 0.0), mask, flags, params: Vec4::new(0.0, 1.0, o.snow_up[0], o.snow_up[1]) },
        });
        self.0.insert(key.clone(), h.clone());
        h
    }
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
        let data = crate::actor::data_for(&combat, &a);
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
        bevy::asset::embedded_asset!(app, "map_material.wgsl");
        app.add_plugins(MaterialPlugin::<MapMaterial>::default());
        app.add_systems(Startup, load_map)
            .add_systems(Update, ((hour_keys, apply_lighting).chain().run_if(resource_exists::<MapLighting>), frame_rate_log))
            .add_systems(Startup, crate::sandbox::spawn_enemies.after(load_map))
            .add_systems(Update, (crate::sandbox::place_wolf, crate::sandbox::watch, crate::sandbox::catalog_shots.run_if(resource_exists::<crate::sandbox::Catalog>)))
            .add_systems(FixedUpdate, bodies_on_terrain.after(ActorSet::Advance));
    }
}

/// One line with the average frame rate over seconds 5..15 after start (the map's cost).
fn frame_rate_log(time: Res<Time>, mut acc: Local<(f32, u32, bool)>) {
    let t = time.elapsed_secs();
    if acc.2 || t < 5.0 {
        return;
    }
    acc.0 += time.delta_secs();
    acc.1 += 1;
    if t >= 15.0 {
        acc.2 = true;
        info!("map frame rate: {:.0} fps over {} frames ({})", acc.1 as f32 / acc.0, acc.1, if terrain().is_some() { "map" } else { "no map" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_and_probe_blend() {
        for v in [0.0f32, 1.0, 0.5, 3.25, 1e-3, 1e-5, 65504.0, -2.0] {
            let back = half_to_f32(f32_to_half(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-7, "{v} -> {back}");
        }
        let mut a = b"DDS ".to_vec();
        a.resize(148, 0);
        let mut b = a.clone();
        a.extend_from_slice(&f32_to_half(1.0).to_le_bytes());
        b.extend_from_slice(&f32_to_half(3.0).to_le_bytes());
        let out = blend_f16_dds(&a, &b, 0.25).unwrap();
        assert_eq!(out.len(), 150);
        assert!((half_to_f32(u16::from_le_bytes([out[148], out[149]])) - 1.5).abs() < 1e-3);
        assert!(blend_f16_dds(&a, &b[..100], 0.5).is_none());
    }

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
