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
//!   light; colours RGB with the intensity in W. The exe squares light and hemisphere colours
//!   (`rgb * rgb * W`, gamma 2.0) and takes fog colours at gamma 2.2 (`colour`, `fog_colour`);
//!   one scene-to-Bevy scale (`SCENE_TO_BEVY`) serves the sun, fill, ambient and probes. Both
//!   from sekiro-rs `docs/SHADER-INPUTS.md` (exe traced, THIRD_PARTY.md). The volumetric fog
//!   becomes Bevy's exponential distance fog.
//!
//! Without a map (config `[world] map` unset and no `SHINOBI_MAP`), nothing here runs and the
//! floor is the flat arena at y = 0 (`ground_y` returns 0): the sim tests stay as they were.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve, AutoExposurePlugin};
use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, Side};
use crate::config::GameConfig;
use crate::grading::{ColorGradingLut, GameToneCurve, GradingPlugin};
use crate::player::CAPSULE_HALF_HEIGHT;

/// Body radius for wall contacts: Wolf's map-hit capsule, 1.5 m high, radius 0.4 (exe
/// FUN_140bc3140, hard-coded for the player; NPCs read NpcParam hitHeight / hitRadius in
/// FUN_140bc2e00). Live physics module +0xD0 = 1.5 / 0.4 agrees.
pub const RADIUS: f32 = 0.4;
/// Highest ledge a walking body steps onto, and how far it follows the floor down.
/// gap: the character proxy's step-up is not traced (Havok sweep, no explicit value found).
pub const STEP_HEIGHT: f32 = 0.5;
/// hknpCharacterProxyCinfo maxSlope, set to 60 deg in FUN_14104d3d0 (0x3f860a92 = 1.0472 rad).
const MAX_SLOPE_DEGREES: f32 = 60.0;
/// Heights of the contact spheres' centres above the feet; the lowest sphere's bottom sits at
/// the step height so stairs reach the floor query rather than the wall push.
const SPHERES: [f32; 3] = [STEP_HEIGHT + RADIUS, 1.2, 1.55];
const CELL: f32 = 4.0;
/// Bevy lux per native light unit (the game has no photometric unit: `rgb * rgb * W` goes to
/// the shaders as is). sekiro-rs `crates/game/src/lighting.rs` SCENE_TO_BEVY: 4500 folds its
/// earlier 9000 lux and 0.5 post calibration into one scale. gap: an exe-derived bridge.
pub const SCENE_TO_BEVY: f32 = 4_500.0;
const LUX_PER_UNIT: f32 = SCENE_TO_BEVY;
/// cd/m^2 per unit of hemisphere / probe radiance: the same scale seen as radiance (/ pi).
pub(crate) const ENV_PER_UNIT: f32 = SCENE_TO_BEVY / std::f32::consts::PI;
const AMBIENT_PER_UNIT: f32 = ENV_PER_UNIT;
/// Auto exposure (`auto_exposure`): the game's log2 luminance units onto this renderer's
/// buffer, and the EV base that keeps the 18 h look (-Exposure at 18 h).
const METER_EV: f32 = 2.6;
/// Tone Map FilmicContrast: the castle and gate draw params have none, so the exe's
/// constructor default (sekiro-rs docs/SHADER-INPUTS.md, function 24db30).
const FILMIC_CONTRAST: f32 = 0.5;
const BASE_EV: f32 = -0.8;

/// The hit collision, shared with the systems that cannot take a resource (player.rs floor
/// checks, camera.rs arm clamp, sound.rs floor material) through [`terrain`].
pub struct Terrain {
    verts: Vec<Vec3>,
    tris: Vec<[u32; 3]>,
    normals: Vec<Vec3>,
    materials: Vec<u32>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    min_walk_ny: f32,
    /// Level triangles without a hit material dropped at load (see `from_tris`).
    helper_planes: usize,
}

static TERRAIN: OnceLock<RwLock<Option<Arc<Terrain>>>> = OnceLock::new();

/// The fog-wall collision in force: the terrain it belongs to (held, so its address stays
/// unique) and the gates standing on it ([`set_gates`]).
static GATES_ON: OnceLock<RwLock<(Option<Arc<Terrain>>, Vec<Arc<Terrain>>)>> = OnceLock::new();

/// Switches the loaded map's fog-wall collision (`map_<id>.gates`, sekiro-extract) to these MSB
/// entities: the walls' objects and collision parts while their fight is on (fog_wall.rs).
/// Returns how many of them the map has.
pub(crate) fn set_gates(entities: &[i64]) -> usize {
    static FILES: OnceLock<std::sync::Mutex<HashMap<String, Arc<HashMap<i32, Arc<Terrain>>>>>> = OnceLock::new();
    let base = terrain();
    let all = match (loaded_map(), &base, entities.is_empty()) {
        (Some(id), Some(_), false) if !crate::sandbox::is_generated(&id) => {
            let mut files = FILES.get_or_init(Default::default).lock().unwrap();
            files
                .entry(id.clone())
                .or_insert_with(|| {
                    let d = std::fs::read(crate::paths::extracted().join(format!("map_{id}.gates"))).unwrap_or_default();
                    Arc::new(Terrain::gates_from(&d).into_iter().map(|(e, t)| (e, Arc::new(t))).collect())
                })
                .clone()
        }
        _ => Default::default(),
    };
    let on: Vec<Arc<Terrain>> = entities.iter().filter_map(|e| all.get(&i32::try_from(*e).ok()?).cloned()).collect();
    let n = on.len();
    *GATES_ON.get_or_init(Default::default).write().unwrap() = (base.filter(|_| n > 0), on);
    n
}

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
/// `[world] map = "arenas"`: no base map, only the exported fight arenas (the flat test floor
/// for a fight without one).
pub const ARENAS_ONLY: &str = "arenas";

pub fn map_id(config: &GameConfig) -> Option<String> {
    // "", "off" or "0" turn the map off for one run (PowerShell drops a variable set to "").
    match std::env::var("SHINOBI_MAP") {
        Ok(v) => (!v.is_empty() && v != "off" && v != "0").then_some(v),
        Err(_) => {
            let m = config.world.map.clone().filter(|m| !m.is_empty())?;
            if m == crate::sandbox::MAP_ID {
                return Some(sandbox_exception(&config.enemy.chr, config.enemy.npc_row).unwrap_or(m));
            }
            // The parkour course keeps itself whatever the fight (no boss arena swap).
            if m == crate::sandbox::PARKOUR_ID {
                return Some(m);
            }
            let arena = boss_arena(&config.enemy.chr, config.enemy.npc_row);
            if m == ARENAS_ONLY {
                return arena;
            }
            Some(arena.unwrap_or(m))
        }
    }
}

/// Fights the sandbox cannot hold, which keep their own arena under `map = "sandbox"` (the user,
/// 2026-10-15: "devine dragon need bigger arena becauce he push you away"): the Old Dragons of
/// the Tree and the Divine Dragon. Their map script moves Wolf about the real place (m25
/// 12505885 WarpCopy at the hand-over, the updrafts and the strong wind 52000830 across the cloud
/// floor), and the trees' grapple points ride on its geometry; on the sandbox he started in
/// the air and fell.
const OWN_ARENA_ROWS: [i64; 2] = [52000000, 53000000];

fn sandbox_exception(chr: &str, npc_row: Option<i64>) -> Option<String> {
    fight_row(chr, npc_row).filter(|r| OWN_ARENA_ROWS.contains(r)).and_then(|_| boss_arena(chr, npc_row))
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

/// HitMtrlParam fallDeathDist / disableFallDamage per material id (extracted/json/params).
fn hit_mtrl_fall() -> &'static HashMap<u32, (f32, bool)> {
    static T: std::sync::OnceLock<HashMap<u32, (f32, bool)>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/HitMtrlParam.json");
        let json: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let rows = json["rows"].as_array().cloned().unwrap_or_default();
        rows.iter().filter_map(|r| Some((r["id"].as_u64()? as u32, (r["fallDeathDist"].as_f64()? as f32, r["disableFallDamage"].as_u64().unwrap_or(0) != 0)))).collect()
    })
}

/// How far (m) Wolf may fall onto this ground before the landing kills him, None where fall
/// damage is off. Exe: FUN_14084e5d0 (the kill test) is `fall height >= HitMtrlParam +0x4c
/// (fallDeathDist)`, and only from 20 m up (DAT_143b01948 = 20.0 in the exe's data; 5 m while
/// SpEffect type 0x155 is worn, DAT_143b01950/54). The row is the material under the landing
/// (FUN_1408433f0 reads it from the fall module's material id); no ground = the table's default
/// 40 (gap: the exe's void kill height is not traced). The height is metres (the same 20 as the
/// long stiff landing, c0000_define 213). gap: the non-lethal HP loss (FUN_140843660's factors)
/// is not traced.
pub fn fall_death_dist(material: Option<u32>) -> Option<f32> {
    match material.and_then(|m| hit_mtrl_fall().get(&m)) {
        Some((_, true)) => None,
        Some((dist, false)) => Some(*dist),
        None => Some(40.0),
    }
}

/// Distance along `dir` (unit) to the first collision triangle within `max`.
pub fn raycast(origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    terrain()?.raycast(origin, dir, max)
}

/// Wolf's start (his centre): 4 m in front of the enemy, the player's spot of every arena
/// (tools/sekiro-extract map export: the enemy at (0, y, -4) facing +z, the origin between
/// them). The ground between the two can slope (the Fountainhead bridge rises 0.25 m per
/// metre toward Wolf: 1.7 m up at z 4), so the floor under each spot is followed from the
/// enemy's spot half a metre at a time, 2 m of step at most, instead of being read from a
/// fixed height; a ledge over the spot counts as a floor too, so the nearer of the two to the
/// last floor wins. On a boss's own arena the spot can sit under the boss's cave roof (the
/// Guardian Ape sleeps in a grotto: at (0, 4) the rock h019400 is 1.5 m over the floor and
/// the camera has no room), so the start walks on away from the boss along +z to the first
/// spot with 2.8 m of headroom. A wall across the walk ends it and the plain spot is kept
/// (the Headless Ape's MSB pose faces the pit's wall: tools/boss_arenas.py turns his arena).
/// gap: the game starts the fight where the boss script's warp or the fog gate puts Wolf.
pub fn player_start() -> Vec3 {
    let base = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 4.0);
    let Some(t) = terrain().filter(|_| on_boss_arena()) else { return base };
    // `SHINOBI_START=<x>,<z>` (a debug knob): that spot instead, on the floor nearest y 0.
    if let Some((x, z)) = std::env::var("SHINOBI_START").ok().and_then(|v| v.split_once(',').and_then(|(x, z)| Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?)))) {
        let h = t.floor(x, z, 3.0).unwrap_or(0.0);
        return Vec3::new(x, h + CAPSULE_HALF_HEIGHT, z);
    }
    arena_start(&t, !crate::enemy::on_own_arena()).unwrap_or_else(|kept| kept)
}

/// [`player_start`]'s search on one arena's collision: `Ok` with the spot it found, `Err` with
/// the spot it fell back to (warned). `stranger`: another boss's line-up stands 8 m ahead.
fn arena_start(t: &Terrain, stranger: bool) -> Result<Vec3, Vec3> {
    const HEADROOM: f32 = 2.8;
    let base = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 4.0);
    // The floor under (0, z) nearest to `from`, within 2 m up or down: the highest below
    // `from` + 2, unless that is a ledge more than half a metre up with ground under it.
    let floor_near = |z: f32, from: f32| -> Option<f32> {
        let hi = t.floor(0.0, z, from + 2.0)?;
        if hi > from + 0.5 {
            if let Some(lo) = t.floor(0.0, z, from + 0.5).filter(|lo| *lo >= from - 2.0) {
                return Some(lo);
            }
        }
        (hi >= from - 2.0).then_some(hi)
    };
    let room = |z: f32, h: f32| t.raycast(Vec3::new(0.0, h + 0.1, z), Vec3::Y, HEADROOM).is_none();
    // On another boss's arena (an encounter swap, enemy.rs on_own_arena) the line-up stands
    // 8 m ahead of Wolf instead of at the boss's MSB spot: that spot needs a floor and headroom
    // too, or the enemy starts in the rock. Its floor may sit on a slope (Isshin's field falls
    // 0.27 m per metre toward Wolf: the boss spot is 1.3 m above his), so it is allowed 2 m of
    // height difference, found from 2 m over Wolf's floor down.
    let lineup_ok = |z: f32, h: f32| {
        !stranger || t.floor(0.0, z - 8.0, h + 2.0).filter(|hf| (hf - h).abs() <= 2.0).is_some_and(|hf| room(z - 8.0, hf))
    };
    // The enemy's floor at (0, -4) is the arena's y 0 by construction (the exporter puts the
    // origin at his feet, half a metre of helper collision allowed).
    let Some(h0) = t.floor(0.0, -4.0, 1.0) else { return Err(base) };
    let mut h = h0;
    let mut z = -4.0;
    while z < 16.0 {
        z += 0.5;
        let Some(hz) = floor_near(z, h) else {
            warn!("player start: no floor within 2 m of {h:.2} at (0, {z}); kept at (0, 4) over {h0:.2}");
            return Err(base.with_y(h0 + CAPSULE_HALF_HEIGHT));
        };
        h = hz;
        if z < 4.0 || !room(z, h) || !lineup_ok(z, h) {
            continue;
        }
        if z > 4.0 {
            info!("player start moved to z {z}: no headroom at the arena's spot (floor {h0:.2} at the enemy, {h:.2} here{})", if stranger { "; the line-up 8 m ahead checked too" } else { "" });
        }
        return Ok(Vec3::new(0.0, h + CAPSULE_HALF_HEIGHT, z));
    }
    warn!("player start: no spot with {HEADROOM} m of headroom within 12 m of (0, 4); kept");
    Err(base)
}

/// Puts Wolf on [`player_start`] once the terrain is loaded (player.rs spawns him at the plain
/// spot during Startup, before or after `load_map`).
fn settle_player_start(mut wolf: Query<&mut Transform, With<crate::player::Player>>) {
    if !on_boss_arena() {
        return;
    }
    let start = player_start();
    for mut tf in &mut wolf {
        if tf.translation.distance(start) > 1e-3 {
            info!("player start: Wolf {} -> {start}", tf.translation);
            tf.translation = start;
        }
    }
}

fn cell_of(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

impl Terrain {
    pub(crate) fn from_hit(d: &[u8]) -> Option<Self> {
        if d.len() < 12 || &d[0..4] != b"SHMC" {
            return None;
        }
        let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let nv = u32_(8) as usize;
        let mut o = 12;
        // A truncated file: None (the loader warns) instead of a slice panic.
        if d.len() < o + nv * 12 + 4 {
            return None;
        }
        // X mirrored into Bevy space (as every model).
        let verts: Vec<Vec3> = (0..nv).map(|i| Vec3::new(-f(o + i * 12), f(o + i * 12 + 4), f(o + i * 12 + 8))).collect();
        o += nv * 12;
        let nt = u32_(o) as usize;
        o += 4;
        if d.len() < o + nt * 16 {
            return None;
        }
        Some(Self::from_tris(verts, (0..nt).map(|i| {
            let k = o + i * 16;
            ([u32_(k), u32_(k + 4), u32_(k + 8)], u32_(k + 12))
        })))
    }

    /// Collision from triangles already in Bevy space (`(indices, material)` each): the hit
    /// files and the sandbox map's generated floor and walls.
    pub(crate) fn from_tris(verts: Vec<Vec3>, tris: impl Iterator<Item = ([u32; 3], u32)>) -> Self {
        let nv = verts.len();
        let mut t = Terrain { verts, tris: Vec::new(), normals: Vec::new(), materials: Vec::new(), cells: HashMap::new(), min_walk_ny: MAX_SLOPE_DEGREES.to_radians().cos(), helper_planes: 0 };
        for (tri, material) in tris {
            if tri.iter().any(|&x| x as usize >= nv) {
                continue;
            }
            let [a, b, c] = tri.map(|x| t.verts[x as usize]);
            let n = (b - a).cross(c - a);
            if n.length_squared() < 1e-12 {
                continue;
            }
            // The h9xxxxx helper collisions without a hit material: their level planes (the
            // Guardian Ape's h900702, a 66 m plane 0.4 m over the pond floor; the maps' kill
            // planes far below) are not floors. Their walls (h900700, the arena's bounds) stay.
            if material == u32::MAX && n.y.abs() >= t.min_walk_ny * n.length() {
                t.helper_planes += 1;
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

    /// No terrain (the map switched to one without a `.hit`, or off).
    pub(crate) fn clear() {
        if let Ok(mut t) = TERRAIN.get_or_init(|| RwLock::new(None)).write() {
            *t = None;
        }
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
        let gates = self.gates_on();
        let gate_tris: Vec<Vec<u32>> = gates.iter().map(|g| (0..g.tris.len() as u32).collect()).collect();
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
                let mut push = self.push_out(p, &tris);
                for (g, t) in gates.iter().zip(&gate_tris) {
                    let gp = g.push_out(p, t);
                    if gp.length_squared() > push.length_squared() {
                        push = gp;
                    }
                }
                if push == Vec2::ZERO {
                    break;
                }
                p.x += push.x;
                p.z += push.y;
            }
        }
        Vec2::new(p.x - feet.x, p.z - feet.z)
    }

    /// The fog-wall collision switched on for this terrain ([`set_gates`]); none for any other
    /// terrain (a test's, or a map loaded since).
    fn gates_on(&self) -> Vec<Arc<Terrain>> {
        GATES_ON.get().and_then(|g| g.read().ok()).filter(|g| g.0.as_ref().is_some_and(|t| std::ptr::eq(Arc::as_ptr(t), self))).map(|g| g.1.clone()).unwrap_or_default()
    }

    /// `map_<id>.gates` "SHMG" v1 (sekiro-extract map.rs): the fog walls' collision per MSB
    /// entity, in the .hit's space.
    pub(crate) fn gates_from(d: &[u8]) -> HashMap<i32, Terrain> {
        let mut out = HashMap::new();
        if d.len() < 12 || &d[0..4] != b"SHMG" {
            return out;
        }
        let u32_ = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let mut o = 12;
        for _ in 0..u32_(8).unwrap_or(0) {
            let (Some(entity), Some(nv)) = (u32_(o), u32_(o + 4)) else { break };
            let nv = nv as usize;
            o += 8;
            let Some(nt) = u32_(o + nv * 12).map(|n| n as usize) else { break };
            if d.len() < o + nv * 12 + 4 + nt * 16 {
                break;
            }
            let verts: Vec<Vec3> = (0..nv).map(|i| Vec3::new(-f(o + i * 12), f(o + i * 12 + 4), f(o + i * 12 + 8))).collect();
            o += nv * 12 + 4;
            let k = o;
            let tris = (0..nt).map(|i| {
                let t = k + i * 16;
                ([u32_(t).unwrap(), u32_(t + 4).unwrap(), u32_(t + 8).unwrap()], u32_(t + 12).unwrap())
            });
            out.insert(entity as i32, Terrain::from_tris(verts, tris));
            o += nt * 16;
        }
        out
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

/// Bindless (map_material.wgsl `#ifdef BINDLESS`), so Bevy batches the map pieces into
/// multi-draws instead of one bind group switch per material: the uniforms pack into one
/// `MapUniform` per material, the textures go through an index table. After sekiro-rs 73eca1d
/// (THIRD_PARTY.md).
#[derive(Asset, bevy::render::render_resource::AsBindGroup, Reflect, Debug, Clone, Default)]
#[data(100, MapUniform, binding_array(124))]
#[bindless(index_table(range(100..123), binding(123)))]
pub struct MapExt {
    #[texture(101)]
    #[sampler(102)]
    pub normal: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    pub over_albedo: Option<Handle<Image>>,
    #[texture(105)]
    #[sampler(106)]
    pub over_normal: Option<Handle<Image>>,
    #[texture(107)]
    #[sampler(108)]
    pub snow_albedo: Option<Handle<Image>>,
    #[texture(109)]
    #[sampler(110)]
    pub snow_normal: Option<Handle<Image>>,
    #[texture(111)]
    #[sampler(112)]
    pub c_albedo: Option<Handle<Image>>,
    #[texture(113)]
    #[sampler(114)]
    pub c_normal: Option<Handle<Image>>,
    #[texture(115)]
    #[sampler(116)]
    pub mask: Option<Handle<Image>>,
    /// x: has a mask, y: blend softness (`SHINOBI_MAP_SOFT`, default 0.25; 0 = plain weights).
    pub mask_params: Vec4,
    /// x: base normal map (bit 1: layer C, bit 2: with normal), y: overlay (bit 1: with
    /// normal), z: snow (bit 1: with normal), w: base albedo (bit 1: show the blend weights,
    /// `SHINOBI_MAP_SHOW_BLEND=1`).
    pub flags: UVec4,
    /// x: overlay uv set, y: snow uv set, z / w: snow up-facing smoothstep on N.y.
    pub params: Vec4,
    /// The game path (bin v5): the detail normal, the layer mask on uv1, the wet flow normal
    /// (sampled with the base normal's sampler).
    #[texture(117)]
    pub detail: Option<Handle<Image>>,
    #[texture(118)]
    pub mask_l: Option<Handle<Image>>,
    #[texture(119)]
    pub wet: Option<Handle<Image>>,
    pub game: MapGame,
    /// The Env_Boxes' irradiance volumes (`apply_giiv`): one atlas, each box's x, y, z, w
    /// stacked along the depth.
    #[texture(120, dimension = "3d")]
    #[sampler(121)]
    pub giiv: Option<Handle<Image>>,
    pub giiv_params: GiivParams,
    /// The variant after the hour (`giiv_params.scale.y` = its weight; sampled with 121).
    #[texture(122, dimension = "3d")]
    pub giiv_next: Option<Handle<Image>>,
}

/// map_material.wgsl `MapUniform`: `MapExt`'s uniforms, packed once per material.
#[derive(bevy::render::render_resource::ShaderType, Debug, Clone)]
pub struct MapUniform {
    pub mask_params: Vec4,
    pub flags: UVec4,
    pub params: Vec4,
    pub game: MapGame,
    pub giiv: GiivParams,
}

impl From<&MapExt> for MapUniform {
    fn from(m: &MapExt) -> Self {
        Self { mask_params: m.mask_params, flags: m.flags, params: m.params, game: m.game.clone(), giiv: m.giiv_params.clone() }
    }
}

/// At most this many Env_Box volumes per map (sekiro-extract map.rs GIIV_MAX; map_material.wgsl
/// `Giiv.boxes`).
pub const GIIV_MAX: usize = 16;

/// map_material.wgsl `GiivBox`: one Env_Box's volume in the atlas: the Bevy point -> its 0..1
/// rows, its size in metres along u / v / w with 1 / fade distance (w), its full voxel grid, the
/// first voxel kept (the exporter crops to the arena), the voxels kept with their first depth
/// slice (w).
#[derive(bevy::render::render_resource::ShaderType, Reflect, Debug, Clone, Default, PartialEq)]
pub struct GiivBox {
    pub uvw: [Vec4; 3],
    pub size: Vec4,
    pub grid: Vec4,
    pub lo: Vec4,
    pub vox: Vec4,
}

/// map_material.wgsl `Giiv`: the Bevy normal -> map-space rows, the atlas texels with the box
/// count (w, 0 = off), the radiance scale (x) and the next variant's weight (y), the boxes in
/// drawing order.
#[derive(bevy::render::render_resource::ShaderType, Reflect, Debug, Clone, Default, PartialEq)]
pub struct GiivParams {
    pub nrm: [Vec4; 3],
    pub dims: Vec4,
    pub scale: Vec4,
    pub boxes: [GiivBox; GIIV_MAX],
}

/// The game path's uniform (map_material.wgsl `MapGame`): flags x = the textures it has
/// (`GAME_*` bits), y = the kind (0 the first reading, 1 M[Multiple], 2 M[MultipleGround]),
/// z = show the blend weights; `p` = the MTD constants as the exporter's `game_params` orders
/// them.
#[derive(bevy::render::render_resource::ShaderType, Reflect, Debug, Clone, Default)]
pub struct MapGame {
    pub flags: UVec4,
    pub p: [Vec4; 16],
}

/// The game path's vertex data (bin v5): b2 tint rgb + b3 wet, and the uv scale.
pub(crate) const ATTR_MAP_BLEND2: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("MapBlend2", 0x5ec1_0a01, bevy::render::render_resource::VertexFormat::Float32x4);
pub(crate) const ATTR_MAP_UV_SCALE: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("MapUvScale", 0x5ec1_0a02, bevy::render::render_resource::VertexFormat::Float32);

impl bevy::pbr::MaterialExtension for MapExt {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/map_material.wgsl".into()
    }
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/map_material.wgsl".into()
    }
    /// The main pass reads the game path's two extra attributes when the mesh has them
    /// (MAP_GAME_ATTRS); the prepass keeps Bevy's own layout.
    fn specialize(
        _pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        use bevy::shader::ShaderDefVal;
        let prepass = descriptor.vertex.shader_defs.iter().any(|d| matches!(d, ShaderDefVal::Bool(n, true) if n == "PREPASS_PIPELINE"));
        let l = &layout.0;
        if prepass || !l.contains(ATTR_MAP_BLEND2) || !l.contains(ATTR_MAP_UV_SCALE) {
            return Ok(());
        }
        let mut attrs = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0), Mesh::ATTRIBUTE_NORMAL.at_shader_location(1)];
        for (a, at) in [(Mesh::ATTRIBUTE_UV_0, 2), (Mesh::ATTRIBUTE_UV_1, 3), (Mesh::ATTRIBUTE_TANGENT, 4), (Mesh::ATTRIBUTE_COLOR, 5)] {
            if l.contains(a.clone()) {
                attrs.push(a.at_shader_location(at));
            }
        }
        attrs.push(ATTR_MAP_BLEND2.at_shader_location(8));
        attrs.push(ATTR_MAP_UV_SCALE.at_shader_location(9));
        descriptor.vertex.buffers = vec![l.get_layout(&attrs)?];
        descriptor.vertex.shader_defs.push("MAP_GAME_ATTRS".into());
        if let Some(f) = descriptor.fragment.as_mut() {
            f.shader_defs.push("MAP_GAME_ATTRS".into());
        }
        Ok(())
    }
}

/// `MapGame.flags.x` bits: which game-path textures the material has.
const GAME_NORMAL: u32 = 1;
const GAME_OVER: u32 = 2;
const GAME_OVER_N: u32 = 4;
const GAME_SNOW: u32 = 8;
const GAME_SNOW_N: u32 = 16;
const GAME_L: u32 = 32;
const GAME_L_N: u32 = 64;
const GAME_MASK: u32 = 128;
const GAME_DETAIL: u32 = 256;
const GAME_MASK_L: u32 = 512;
const GAME_WET: u32 = 1024;
const GAME_ALBEDO: u32 = 2048;

// ---------------------------------------------------------------------------------------------
// Grapple points (docs/kb/grapple.md; the engine's "wire")

/// One wire point of the map (json "grapple", tools/sekiro-extract map export): the hook the
/// rope attaches to, the landing spot, the point's facing and its WireVariationParam row. Bevy
/// space (X mirrored on load).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrapplePoint {
    pub hook: Vec3,
    pub land: Vec3,
    pub fwd: Vec3,
    pub variation: i64,
}

/// The loaded map's wire points (empty without a map, in the sandbox, and on maps exported
/// before the grapple export).
#[derive(Resource, Default)]
pub struct GrapplePoints(pub Vec<GrapplePoint>);

/// A hang (kind 2) or wall-hug (kind 3) edge from the map's and objects' `.edge` files
/// (kb/grapple.md section 1): the record's three vertices raw, in Bevy space. `a`-`b` is the
/// edge's segment; what `c` means per kind is not decoded (gap). Data for the ledge hang and
/// wall hug moves (HKS BEH_A_GROUND_HANG_START, COVER_*); nothing uses it yet but the gizmos.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct LedgeEdge {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub kind: u8,
    pub variation: i64,
}

/// The loaded map's hang / hug edges (empty on maps exported before 2026-10-11's ledge export).
#[derive(Resource, Default)]
pub struct LedgeEdges(pub Vec<LedgeEdge>);

/// The point the wire would fire at ([`grapple_target`]).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // read by the player move (VXf)
pub struct GrappleTarget {
    pub index: usize,
    pub point: GrapplePoint,
    /// Wolf's centre to the hook.
    pub dist: f32,
    /// Inside the row's action distance (the hook can be fired; outside it the marker is shown
    /// greyed, `MENU_Wire_NG`).
    pub in_action_range: bool,
    /// Rideable (landing on the point, LandWireMove) or a swing-through point (targetMarkerType
    /// 1 ring with an arrow: rows 20/23/24 "can't ride", 41 kite).
    pub ride: bool,
}

/// WirePointSearchParam row 0 (WireSetParam 0 = rows [0, 100], the default set).
pub const WIRE_ACTION_NEAR: f32 = 3.0;
pub const WIRE_ACTION_FAR: f32 = 15.0;
pub const WIRE_FIND_FAR: f32 = 25.0;
/// filteringDistanceHeightUpper / Lower 10.2 and filteringDistanceWidth 14.3: the rectangle in
/// the camera's plane a point must be inside to be a candidate; softlockDistanceHeight 10.2 /
/// Width 7.15 the smaller one the current target keeps itself inside. Taken as metres in the
/// camera plane: the height values one-sided, the widths the full width (soft lock = half the
/// filter width). // gap: the exe may scale them with depth (screen fractions); untraced.
const WIRE_FILTER_HEIGHT_UPPER: f32 = 10.2;
const WIRE_FILTER_HEIGHT_LOWER: f32 = 10.2;
const WIRE_FILTER_WIDTH: f32 = 14.3;
const WIRE_SOFTLOCK_HEIGHT: f32 = 10.2;
const WIRE_SOFTLOCK_WIDTH: f32 = 7.15;

/// WireVariationParam: recognitionAngleUp/Down/Left/Right (degrees, the player's side of the
/// point's reference direction), extendWireActionDistance (m, added to actionDistanceFar) and
/// whether the point is ridden (targetMarkerType 0) or swung through (1). Rows 0/3/20/23/24
/// are 90/90/180/180, 1/2/21/22 and 5 (extend 10) 90 all round, 40 90/90/180/180 extend 30
/// (swung through: targetMarkerType 1), 41 90/50/30/10 extend 30; enemy rows 10700..70210 are
/// the SpEffect's (never map points).
fn wire_variation(row: i64) -> ([f32; 4], f32, bool) {
    match row {
        1 | 2 | 21 | 22 => ([90.0, 90.0, 90.0, 90.0], 0.0, row < 20),
        5 => ([90.0, 90.0, 90.0, 90.0], 10.0, true),
        20 | 23 | 24 => ([90.0, 90.0, 180.0, 180.0], 0.0, false),
        40 => ([90.0, 90.0, 180.0, 180.0], 30.0, false),
        41 => ([90.0, 50.0, 30.0, 10.0], 30.0, false),
        // gap: rows 45 (homing +53105) and the enemy rows: taken as row 0.
        _ => ([90.0, 90.0, 180.0, 180.0], 0.0, true),
    }
}

/// The wire point Wolf would fire at: of the points within findDistanceFar whose recognition
/// angles admit him and that sit inside the camera-plane rectangle, the one nearest the view
/// axis; `prev` (last frame's target) is kept while it stays inside the smaller soft-lock
/// rectangle (WirePointSearchParam 0). A collision triangle between Wolf's chest and the hook
/// hides the point. `cam_fwd` unit; `wolf` his centre.
pub fn grapple_target(cam_pos: Vec3, cam_fwd: Vec3, wolf: Vec3, prev: Option<usize>, points: &[GrapplePoint]) -> Option<GrappleTarget> {
    let right = cam_fwd.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = right.cross(cam_fwd).normalize();
    let chest = wolf + Vec3::Y * 0.3;
    // (index, dist, lateral, vertical, depth, ride, action far) of every candidate.
    let mut found: Vec<(usize, f32, f32, f32, f32, bool, f32)> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let to_hook = p.hook - wolf;
        let dist = to_hook.length();
        if dist > WIRE_FIND_FAR || dist < 1e-3 {
            continue;
        }
        let (angles, extend, ride) = wire_variation(p.variation);
        // Wolf in the point's frame: ahead = the reference direction, horizontal angle to the
        // right positive, vertical angle up positive. // gap: which side "left" is (mirrored X)
        // and whether the angles are read from the hook or from the landing point.
        let fwd = p.fwd.try_normalize().unwrap_or(Vec3::Z);
        let pright = fwd.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let to_wolf = -to_hook / dist;
        let horizontal = to_wolf.x.hypot(to_wolf.z).max(1e-4);
        let h = pright.dot(to_wolf).atan2(fwd.dot(to_wolf)).to_degrees();
        let v = (to_wolf.y / horizontal).atan().to_degrees();
        let ok_v = if v >= 0.0 { v <= angles[0] } else { -v <= angles[1] };
        let ok_h = if h >= 0.0 { h <= angles[3] } else { -h <= angles[2] };
        if !ok_v || !ok_h {
            continue;
        }
        // The camera-plane rectangle.
        let rel = p.hook - cam_pos;
        let depth = rel.dot(cam_fwd);
        if depth <= 0.0 {
            continue;
        }
        let lateral = rel.dot(right);
        let vertical = rel.dot(up);
        if lateral.abs() > WIRE_FILTER_WIDTH / 2.0 || vertical > WIRE_FILTER_HEIGHT_UPPER || -vertical > WIRE_FILTER_HEIGHT_LOWER {
            continue;
        }
        // Nothing in the way (the rope has no collision: kb/grapple.md).
        if let Some(t) = raycast(chest, (p.hook - chest).normalize(), (p.hook - chest).length() - 0.3) {
            if t < (p.hook - chest).length() - 0.3 {
                continue;
            }
        }
        found.push((i, dist, lateral, vertical, depth, ride, WIRE_ACTION_FAR + extend));
    }
    if found.is_empty() {
        return None;
    }
    let pick = prev
        .and_then(|k| found.iter().find(|f| f.0 == k && f.2.abs() <= WIRE_SOFTLOCK_WIDTH / 2.0 && f.3.abs() <= WIRE_SOFTLOCK_HEIGHT))
        .or_else(|| found.iter().min_by(|a, b| (a.2 / a.4).hypot(a.3 / a.4).total_cmp(&(b.2 / b.4).hypot(b.3 / b.4))))?;
    let &(index, dist, _, _, _, ride, far) = pick;
    Some(GrappleTarget { index, point: points[index], dist, in_action_range: (WIRE_ACTION_NEAR..=far).contains(&dist), ride })
}

/// Debug (H / the debug menu, with the hurtboxes): every wire point as a green hook sphere, a
/// line to its landing spot and a short facing tick; the current target yellow with a line
/// from Wolf (grey when out of the action range).
fn draw_grapple_points(
    points: Res<GrapplePoints>,
    show: Res<crate::combat::ShowHurtboxes>,
    cams: Query<&GlobalTransform, With<crate::camera::OrbitCamera>>,
    wolf: Query<&Transform, With<crate::player::Player>>,
    mut prev: Local<Option<usize>>,
    mut gizmos: Gizmos,
) {
    if !show.0 || points.0.is_empty() {
        return;
    }
    let green = Color::srgba(0.2, 0.9, 0.3, 0.9);
    for p in &points.0 {
        gizmos.sphere(Isometry3d::from_translation(p.hook), 0.25, green);
        gizmos.line(p.hook, p.land, green);
        gizmos.line(p.hook, p.hook + p.fwd * 0.6, Color::srgba(0.2, 0.5, 1.0, 0.9));
    }
    let (Ok(cam), Ok(w)) = (cams.single(), wolf.single()) else { return };
    let (_, rot, pos) = cam.to_scale_rotation_translation();
    let target = grapple_target(pos, rot * Vec3::NEG_Z, w.translation, *prev, &points.0);
    *prev = target.map(|t| t.index);
    if let Some(t) = target {
        let c = if t.in_action_range { Color::srgb(1.0, 0.9, 0.2) } else { Color::srgb(0.6, 0.6, 0.6) };
        gizmos.sphere(Isometry3d::from_translation(t.point.hook), 0.4, c);
        gizmos.line(w.translation, t.point.hook, c);
    }
}

/// A json [x, y, z] as a Bevy point (X mirrored into Bevy space like every model).
fn json_vec(p: &serde_json::Value, k: &str) -> Option<Vec3> {
    let a = p[k].as_array()?;
    Some(Vec3::new(-(a.first()?.as_f64()? as f32), a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}

/// The map's wire points from its json.
fn load_grapple(json: &serde_json::Value) -> Vec<GrapplePoint> {
    let vec = json_vec;
    json["grapple"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| Some(GrapplePoint { hook: vec(p, "hook")?, land: vec(p, "land")?, fwd: vec(p, "fwd")?, variation: p["var"].as_i64().unwrap_or(0) })).collect())
        .unwrap_or_default()
}

/// The map's hang / hug edges from its json ("ledges").
pub(crate) fn load_ledges(json: &serde_json::Value) -> Vec<LedgeEdge> {
    json["ledges"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| Some(LedgeEdge { a: json_vec(p, "a")?, b: json_vec(p, "b")?, c: json_vec(p, "c")?, kind: p["kind"].as_u64()? as u8, variation: p["var"].as_i64().unwrap_or(0) }))
                .collect()
        })
        .unwrap_or_default()
}

/// Debug gizmos (H, with the hurtboxes): hang edges orange, hug edges cyan, `a`-`b` as a line
/// with a short tick towards `c`; only those within 30 m of Wolf.
fn draw_ledges(ledges: Res<LedgeEdges>, show: Res<crate::combat::ShowHurtboxes>, wolf: Query<&Transform, With<crate::player::Player>>, mut gizmos: Gizmos) {
    if !show.0 || ledges.0.is_empty() {
        return;
    }
    let Ok(w) = wolf.single() else { return };
    for l in ledges.0.iter().filter(|l| l.a.distance(w.translation) < 30.0) {
        let c = if l.kind == 2 { Color::srgba(1.0, 0.6, 0.1, 0.9) } else { Color::srgba(0.2, 0.8, 1.0, 0.9) };
        gizmos.line(l.a, l.b, c);
        let mid = (l.a + l.b) * 0.5;
        gizmos.line(mid, mid + (l.c - mid).clamp_length_max(0.3), c);
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
    /// The reverb of the arena's floor collision (MSB SoundSpaceType, json "sound_space_type"):
    /// 0 none .. 8, fmod.rs apply_reverb.
    pub sound_space: u8,
    /// The game map the arena comes from (json "map", e.g. "m11_01_00_00"; empty on older
    /// exports): picks the area battle music (area_music.rs).
    pub source_map: String,
    /// The export origin in the source map's MSB space (json "origin"; none on generated maps).
    pub map_origin: Option<Vec3>,
}

/// An arena (Bevy) point back in the source map's MSB space: X un-mirrored, turned back by
/// arena_yaw, plus the export origin (the inverse of enemy.rs `msb_patrol`'s point).
pub fn arena_to_map(origin: Vec3, arena_yaw: f32, p: Vec3) -> Vec3 {
    let (s, c) = (-arena_yaw).sin_cos();
    let (x, z) = (-p.x, p.z);
    origin + Vec3::new(x * c + z * s, p.y, -x * s + z * c)
}

impl MapLighting {
    /// `arena_to_map` for the loaded map (None on a generated one).
    pub fn to_map(&self, p: Vec3) -> Option<Vec3> {
        Some(arena_to_map(self.map_origin?, self.arena_yaw, p))
    }

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

    /// The fog's density per metre: VolumetricFog FixedDensity / GameScale (sekiro-rs
    /// SHADER-INPUTS.md "GameScale", exe 39e7e0: the blended record's per-length densities are
    /// divided by it, its lengths multiplied; castle noon 0.0015 / 2.5). Before, the raw value
    /// (2.5x too dense at noon) with a hand-set floor. The fog itself (with its height term) is
    /// height_fog.rs's pass, which packs the same way.
    pub fn fog_density(&self, id: i32) -> f32 {
        let fog = |name: &str| self.at("VolumetricFog", name, id).and_then(|v| v.first().copied());
        let gs = fog("GameScale").unwrap_or(1.0).max(f32::MIN_POSITIVE);
        (fog("FixedDensity").unwrap_or(0.0) / gs).max(0.0)
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

/// A light or hemisphere colour: the exe's LightSet blend (sekiro-rs SHADER-INPUTS.md "Lights
/// and ambient", function 310630) uploads `(r*r*W, g*g*W, b*b*W)`: gamma 2.0, W a plain
/// linear multiplier (returned apart, the callers scale by it).
fn colour(v: &[f32]) -> (Color, f32) {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0).max(0.0).powi(2);
    (Color::linear_rgb(g(0), g(1), g(2)), v.get(3).copied().unwrap_or(1.0))
}

/// A fog colour: the exe's fog copy quantises the clamped RGB to bytes, then gamma 2.2 (sekiro-rs
/// `fog_colour`, SHADER-INPUTS.md "Fog colours and extinction"); W returned apart.
pub(crate) fn fog_colour(v: &[f32]) -> (Color, f32) {
    let g = |i: usize| ((v.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0).round() / 255.0).powf(2.2);
    (Color::linear_rgb(g(0), g(1), g(2)), v.get(3).copied().unwrap_or(1.0))
}

/// The exe's soft Adaption Min / Max limit (sekiro-rs SHADER-INPUTS.md, function 24d920): the
/// adaptation `x` EV approaches +max / -|min| instead of clamping.
fn soft_adapt(x: f32, min: f32, max: f32) -> f32 {
    if x > 0.0 && max > 0.0 {
        max * (1.0 - (-x / max).exp())
    } else if x < 0.0 && min.abs() > 0.0 {
        min.abs() * ((x / min.abs()).exp() - 1.0)
    } else {
        0.0
    }
}

/// Direction toward a light from a (pitch, yaw) map-space angle: turned into the arena like
/// the pieces (`arena_yaw`), then into Bevy space (X mirrored); always from above.
/// Toward the light set's sun (Angle0) in the arena at the map's hour (the sky's lobe).
pub(crate) fn sun_toward(l: &MapLighting) -> Vec3 {
    toward(&l.at("LightSet", "Angle0", l.light_set).unwrap_or_default(), l.arena_yaw)
}

/// The light a lit FXR particle takes (fxr.rs): the game's PS_VFX_SingleTextureAllLight /
/// BumpLight (shader/gxffxshader.shaderbnd) shade `saturate(texture x colour) x (SH ambient +
/// sat(N.L) x FC_DirLight Col_0 x g_ps_envCbColScale x lightMask x shadow)`. Here in game units,
/// the same as the map's: the hemisphere ambient (gap: the game's SH is the GIIV volumes plus
/// the point lights at the particle) and the light set's sun (DiffColor0 x W). Returns
/// (ambient, sun, toward the sun).
pub(crate) fn particle_light(l: &MapLighting) -> (Vec3, Vec3, Vec3) {
    let get = |name: &str| l.at("LightSet", name, l.light_set);
    let lin = |c: Color, w: f32| {
        let c = c.to_linear();
        Vec3::new(c.red, c.green, c.blue) * w
    };
    let (sun, sun_w) = colour(&get("DiffColor0").unwrap_or(vec![1.0, 0.95, 0.85, 1.5]));
    let (up, up_w) = colour(&get("Hemi Color Up").unwrap_or(vec![0.6, 0.7, 0.8, 0.5]));
    let (down, down_w) = colour(&get("Hemi Color Down").unwrap_or(vec![0.4, 0.4, 0.4, 0.2]));
    ((lin(up, up_w) + lin(down, down_w)) * 0.5, lin(sun, sun_w), sun_toward(l))
}

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
/// luminance and exposes it toward "Middle Gray" (log2 target), the adaptation soft-limited to
/// [Adaption Min, Adaption Max] EV (`soft_adapt`; at noon +-0.01: a fixed exposure; at night
/// up to +8), plus "Exposure" EV. Bevy's compensation curve gives the target exposure per
/// average log luminance L: `base + Exposure - soft_adapt(L - MiddleGray - METER, min, max)`. `METER` shifts
/// the game's luminance units onto this renderer's buffer (LUX_PER_UNIT, Bevy's Exposure) and
/// `base` cancels the 18 h Exposure so that hour keeps the look judged against the game; both
/// by eye (`SHINOBI_MAP_METER`, `SHINOBI_MAP_EV` add to them). `SHINOBI_MAP_NO_AE=1` turns it off.
/// The game's metering weight at screen `uv` (sekiro-rs docs/SHADERS.md "Metering and
/// adaptation", ToneMap_ColorToLuminancePS): `1 - (1 - 2^-r)^3`, `r = |2 (uv - 0.5) * (cw,
/// cw * h / w)|` with g_AdaptCenterWeight = (CenterWeight, CenterWeight * viewH / viewW).
pub(crate) fn centre_weight(uv: Vec2, cw: f32, h_over_w: f32) -> f32 {
    let r = (2.0 * (uv - Vec2::splat(0.5)) * Vec2::new(cw, cw * h_over_w)).length();
    1.0 - (1.0 - (-r).exp2()).powi(3)
}

/// Bevy's exposure speed (EV / s) that, with `exponential_transition_distance` = `span` (the
/// whole range, so the linear cap never binds), moves the exposure as the game's adaptation:
/// `adapted += (target - old) * (1 - 10^(-dt * speed))` (ToneMap_AdaptedLuminancePS, g_AdaptParam
/// = (MiddleGray, dt * LightAdapt, dt * DarkAdapt), exe 2508b0). Bevy steps `delta * speed * dt
/// / span`; equal to the game's to first order in dt. gap: the unit of the exe's dt (presumed
/// seconds) and its reset frame (4.0: instant) are not modelled.
pub(crate) fn adapt_speed(game_speed: f32, span: f32) -> f32 {
    std::f32::consts::LN_10 * game_speed * span
}

fn auto_exposure(lighting: &MapLighting, curves: &mut Assets<AutoExposureCompensationCurve>, images: &mut Assets<Image>, h_over_w: f32) -> Option<AutoExposure> {
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
    let (ad_min, ad_max) = (tone("Adaption Min").unwrap_or(-0.5), tone("Adaption Max").unwrap_or(1.0));
    let meter = METER_EV + knob("SHINOBI_MAP_METER");
    let base = BASE_EV + knob("SHINOBI_MAP_EV");
    let (lo, hi) = (-10.0f32, 6.0f32);
    let target = |l: f32| base + exposure - soft_adapt(l - middle_gray - meter, ad_min.min(ad_max), ad_max.max(ad_min));
    // Compensation c(L) = target(L) + L, sampled every half EV (the soft limit is a curve).
    let xs: Vec<f32> = (0..=((hi - lo) * 2.0) as i32).map(|i| lo + i as f32 * 0.5).collect();
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
    // Speeds and centre weight: the authored ones, else the exe's constructor defaults (24db30:
    // LightAdapt 1, DarkAdapt 0.5, CenterWeight 4; the castle's draw params author none).
    let light_adapt = tone("LightAdapt").or_else(|| tone("Light Adapt")).unwrap_or(1.0);
    let dark_adapt = tone("DarkAdapt").or_else(|| tone("Dark Adapt")).unwrap_or(0.5);
    let cw = tone("Center Weight").or_else(|| tone("CenterWeight")).unwrap_or(4.0);
    const MASK: u32 = 64;
    let mask: Vec<u8> = (0..MASK * MASK)
        .flat_map(|i| {
            let uv = (Vec2::new((i % MASK) as f32, (i / MASK) as f32) + 0.5) / MASK as f32;
            let w = (centre_weight(uv, cw, h_over_w) * 255.0).round() as u8;
            [w, w, w, 255]
        })
        .collect();
    let mask = images.add(Image::new(
        bevy::render::render_resource::Extent3d { width: MASK, height: MASK, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        mask,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));
    info!("map auto exposure: middle grey {middle_gray} exposure {exposure} adaption {ad_min}..{ad_max} (meter {meter}, base {base}), adapt {light_adapt} / {dark_adapt}, centre weight {cw}");
    Some(AutoExposure {
        range: lo..=hi,
        // The game averages every (weighted) sample's log luminance: no percentile cut.
        filter: 0.0..=1.0,
        speed_brighten: adapt_speed(light_adapt, hi - lo),
        speed_darken: adapt_speed(dark_adapt, hi - lo),
        exponential_transition_distance: hi - lo,
        metering_mask: mask,
        compensation_curve: curves.add(curve),
    })
}

/// The draw params' bloom (Bloom[Yebis]) as Bevy's: the exe's composite (sekiro-rs
/// SHADER-INPUTS.md "Bloom and glare", function 2508b0) weighs blur level i by
/// `G * Alpha * Luminance * Smooth^i / sum_{j<NumLevels} Smooth^j` (G, a global scale, 1), its
/// bright pass is a hard knee on luminance at the raw Glare Threshold (SHADERS.md). Bevy's
/// additive upsampling weighs level i by `b(0) * b(1) * .. * b(i)`, so b(0) = that level-0 weight
/// and b(m >= 1) = Smooth: intensity = the level-0 weight, low_frequency_boost = Smooth - it,
/// and a curvature near 1 that makes the boost a step at mip 1. NumLevels mips (Bevy's count is
/// log2(max_mip_dimension) - 1). None with the bloom off (Enable 0).
/// gap: the threshold is against the exe's exposure-scaled luminance, Bevy's prefilter sees the
/// buffer before auto exposure; the seven-tap Gaussian (GaussianRadiusScale), the level-0
/// resolution, Saturate Ctrl, the temporal FeedbackWeight and the renderer quality scale on
/// Luminance are not modelled; the 3-level composite shader's fixed 1 / 0.5 / 0.25 weights
/// leave open whether Smooth is the level ratio in every variant.
pub(crate) fn map_bloom(l: &MapLighting, id: i32) -> Option<bevy::post_process::bloom::Bloom> {
    use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
    let at = |name: &str| l.at("Bloom", name, id).and_then(|v| v.first().copied());
    let near = |name: &str| l.nearest("Bloom", name, id).and_then(|v| v.first().copied());
    if near("Enable").is_some_and(|e| e < 0.5) {
        return None;
    }
    let levels = near("GlareNumLevels").unwrap_or(7.0).round().clamp(2.0, 12.0) as i32;
    let smooth = at("GlareBloom Smooth").unwrap_or(0.8).clamp(0.0, 1.0);
    let sum: f32 = (0..levels).map(|i| smooth.powi(i)).sum();
    let level0 = (at("GlareBloom Alpha").unwrap_or(0.0) * at("Glare Luminance").unwrap_or(1.0)).max(0.0) / sum.max(f32::MIN_POSITIVE);
    Some(Bloom {
        intensity: level0,
        low_frequency_boost: smooth - level0,
        low_frequency_boost_curvature: 0.9999,
        high_pass_frequency: 1.0,
        prefilter: BloomPrefilter { threshold: at("Glare Threshold").unwrap_or(0.0).max(0.0), threshold_softness: 0.0 },
        composite_mode: BloomCompositeMode::Additive,
        max_mip_dimension: 1 << (levels + 1),
        ..Bloom::OLD_SCHOOL
    })
}

/// A camera `apply_lighting` has dressed.
#[derive(Component)]
struct Lit;

fn apply_lighting(
    mut lighting: ResMut<MapLighting>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), (With<Sun>, Without<FillLight>)>,
    mut fill: Query<(&mut DirectionalLight, &mut Transform), (With<FillLight>, Without<Sun>)>,
    mut commands: Commands,
    cameras: Query<Entity, With<Camera3d>>,
    new_cameras: Query<Entity, (With<Camera3d>, Without<Lit>)>,
    assets: Res<AssetServer>,
    mut curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
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
    let (sky, _) = fog_colour(&fog("SkyColor").unwrap_or(vec![0.5, 0.55, 0.65, 1.0]));
    let (ul, dl) = (up.to_linear(), down.to_linear());
    let amb = Color::linear_rgb((ul.red * up_w + dl.red * down_w) * 0.5, (ul.green * up_w + dl.green * down_w) * 0.5, (ul.blue * up_w + dl.blue * down_w) * 0.5);
    let density = lighting.fog_density(id);
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
    // The game's fog (height_fog.rs) in place of Bevy's DistanceFog, its colours on the probes'
    // radiance scale; `SHINOBI_MAP_NO_FOG=1` none.
    let bloom = map_bloom(&lighting, id);
    let game_fog = std::env::var("SHINOBI_MAP_NO_FOG")
        .is_err()
        .then(|| crate::height_fog::GameFog::from_lighting(&lighting, id, toward(&get("Angle0").unwrap_or_default(), lighting.arena_yaw), ENV_PER_UNIT * scale))
        .flatten();
    for e in &cameras {
        let mut c = commands.entity(e);
        c.insert(Lit);
        match &game_fog {
            Some(f) => c.insert(f.clone()),
            None => c.remove::<crate::height_fog::GameFog>(),
        };
        // The map's bloom in place of the camera's default (camera.rs); `SHINOBI_MAP_BLOOM=camera`
        // keeps that one.
        if std::env::var("SHINOBI_MAP_BLOOM").as_deref() != Ok("camera") {
            match &bloom {
                Some(b) => c.insert(b.clone()),
                None => c.remove::<bevy::post_process::bloom::Bloom>(),
            };
        }
        if let Some((_, specular, diffuse)) = &probe {
            c.insert((
                EnvironmentMapLight { diffuse_map: diffuse.clone(), specular_map: specular.clone(), intensity: ENV_PER_UNIT * scale * env_scale, ..default() },
                bevy::core_pipeline::Skybox { image: Some(specular.clone()), brightness: ENV_PER_UNIT * scale * env_scale, ..default() },
            ));
        }
        if let Some(n) = lut {
            c.insert(ColorGradingLut(assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false).load(format!("{}_lut_{n:04}.dds", lighting.stem))));
            // The game's tone curve in the grading pass instead of Bevy's (grading.rs);
            // `SHINOBI_MAP_TONEMAP=bevy` keeps Bevy's for comparison.
            if std::env::var("SHINOBI_MAP_TONEMAP").as_deref() != Ok("bevy") {
                c.insert((GameToneCurve { contrast: FILMIC_CONTRAST }, bevy::core_pipeline::tonemapping::Tonemapping::None));
            }
        }
        let h_over_w = window.iter().next().map_or(9.0 / 16.0, |w| w.height() / w.width().max(1.0));
        if let Some(exposure) = auto_exposure(&lighting, &mut curves, &mut images, h_over_w) {
            c.insert(exposure);
        }
    }
    if !lighting.applied {
        info!("map light set {id} at {:.1} h: sun {:?} x{sun_w}, fill x{fill_w}, sky {:?}, fog {density}, probe {:?}, lut {lut:?}", lighting.hour, sun_color, sky, probe.as_ref().map(|p| &p.0));
        info!("map fog: {game_fog:?}");
        info!("map bloom: {:?}", bloom.as_ref().map(|b| (b.intensity, b.low_frequency_boost, b.prefilter.threshold, b.max_mip_dimension)));
    }
    lighting.applied = true;
}

/// `[` / `]`: the time of day one hour back / on (wrapping), the whole lighting re-applied
/// (light set, probe, LUT, exposure): a walk through the day on any map.
/// The Env_Boxes' irradiance volumes on every map material (map_material.wgsl `lit`): the two
/// variants around the hour, blended as the probe cubes are (`probe_maps`; within 2 % of a
/// variant's hour that one alone), from `map_<id>_giiv_<v>.bin` + `map_<id>_giiv.json`
/// (sekiro-extract map.rs `export_giiv`: up to GIIV_MAX boxes in drawing order), in place of the
/// probe cube's diffuse where they cover, on the cube's radiance scale. `SHINOBI_MAP_NO_GIIV=1`
/// keeps the cube's diffuse.
fn apply_giiv(lighting: Res<MapLighting>, mut materials: ResMut<Assets<MapMaterial>>, mut images: ResMut<Assets<Image>>, mut applied: Local<Option<(String, usize, GiivParams)>>, mut volumes: Local<HashMap<String, Option<(Handle<Image>, [u32; 3])>>>, mut stem_of: Local<String>, mut seen: Local<Option<(String, u32, usize)>>) {
    // Only when the map, the hour or the material count changes (the checks below read files:
    // 0.48 ms a frame in the bug fixes session's trace, 2026-10-13).
    let input = (lighting.stem.clone(), lighting.hour.to_bits(), materials.len());
    if seen.as_ref() == Some(&input) {
        return;
    }
    *seen = Some(input);
    let dir = crate::paths::extracted();
    if *stem_of != lighting.stem {
        volumes.clear();
        *stem_of = lighting.stem.clone();
    }
    let mut env: Vec<(f32, String)> = if std::env::var("SHINOBI_MAP_NO_GIIV").is_ok() {
        Vec::new()
    } else {
        lighting.env.iter().map(|(h, stem)| (*h, stem.replace("_env_", "_giiv_"))).filter(|(_, s)| dir.join(format!("{s}.bin")).exists()).collect()
    };
    env.sort_by(|a, b| a.0.total_cmp(&b.0));
    // The variants around the hour and the later one's weight (probe_maps' rule).
    let hour = lighting.hour.rem_euclid(24.0);
    let pick = (!env.is_empty()).then(|| {
        let (a, b) = match env.iter().position(|e| e.0 > hour) {
            Some(0) | None => (&env[env.len() - 1], &env[0]),
            Some(i) => (&env[i - 1], &env[i]),
        };
        let span = (b.0 - a.0).rem_euclid(24.0);
        let t = if span > 0.0 { (hour - a.0).rem_euclid(24.0) / span } else { 0.0 };
        if t < 0.02 {
            (a.1.clone(), a.1.clone(), 0.0)
        } else if t > 0.98 {
            (b.1.clone(), b.1.clone(), 0.0)
        } else {
            (a.1.clone(), b.1.clone(), t)
        }
    });
    let mut get = |stem: &str| -> Option<(Handle<Image>, [u32; 3])> {
        volumes
            .entry(stem.to_string())
            .or_insert_with(|| {
                let v = load_giiv(&dir.join(format!("{stem}.bin")), &mut images);
                if let Some((_, d)) = &v {
                    info!("map giiv: {stem} {}x{}x{}", d[0], d[1], d[2]);
                }
                v
            })
            .clone()
    };
    let (first, next, t) = match &pick {
        Some((a, b, t)) => (get(a), get(b), *t),
        None => (None, None, 0.0),
    };
    // Both variants of one probe have the same voxels; another size cannot blend.
    let (next, t) = match (&first, next) {
        (Some((_, da)), Some((h, db))) if *da == db => (Some(h), t),
        _ => (None, 0.0),
    };
    let params = first
        .as_ref()
        .and_then(|(_, d)| {
            let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{}_giiv.json", lighting.stem))).ok()?).ok()?;
            let mut params = giiv_params(&j)?;
            if params.dims.truncate() != Vec3::new(d[0] as f32, d[1] as f32, d[2] as f32) {
                warn!("map giiv: {} atlas {d:?} does not match its json", lighting.stem);
                return None;
            }
            let lux = std::env::var("SHINOBI_MAP_LUX").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32);
            let env = std::env::var("SHINOBI_MAP_ENV").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32);
            params.scale = Vec4::new(ENV_PER_UNIT * lux * env, t, 0.0, 0.0);
            Some(params)
        })
        .unwrap_or_default();
    let key = (pick.as_ref().map(|(a, b, _)| format!("{a}+{b}")).unwrap_or_default(), materials.len(), params.clone());
    if applied.as_ref() == Some(&key) {
        return;
    }
    let on = params.dims.w > 0.0;
    let first = first.filter(|_| on).map(|(h, _)| h);
    let next = next.filter(|_| on).or_else(|| first.clone());
    for (_, m) in materials.iter_mut() {
        m.extension.giiv = first.clone();
        m.extension.giiv_next = next.clone();
        m.extension.giiv_params = params.clone();
    }
    *applied = Some(key);
}

/// `map_<id>_giiv.json`'s boxes (sekiro-extract `export_giiv`) as the shader's table, without
/// the radiance scale; None when it has no boxes.
fn giiv_params(j: &serde_json::Value) -> Option<GiivParams> {
    let f = |v: &serde_json::Value, i: usize| v[i].as_f64().unwrap_or(0.0) as f32;
    let list = j["boxes"].as_array().filter(|l| !l.is_empty())?;
    let mut p = GiivParams {
        nrm: [0, 1, 2].map(|r| Vec4::new(f(&j["nrm"], r * 3), f(&j["nrm"], r * 3 + 1), f(&j["nrm"], r * 3 + 2), 0.0)),
        dims: Vec4::new(f(&j["atlas"], 0), f(&j["atlas"], 1), f(&j["atlas"], 2), list.len().min(GIIV_MAX) as f32),
        ..default()
    };
    for (b, v) in p.boxes.iter_mut().zip(list) {
        let u = &v["uvw"];
        b.uvw = [0, 1, 2].map(|r| Vec4::new(f(u, r * 4), f(u, r * 4 + 1), f(u, r * 4 + 2), f(u, r * 4 + 3)));
        // A box without a fade distance cuts off at its faces.
        let fade = v["fade"].as_f64().unwrap_or(0.0) as f32;
        b.size = Vec4::new(f(&v["size"], 0), f(&v["size"], 1), f(&v["size"], 2), if fade > 0.0 { 1.0 / fade } else { 1e6 });
        b.grid = Vec4::new(f(&v["grid"], 0), f(&v["grid"], 1), f(&v["grid"], 2), 0.0);
        b.lo = Vec4::new(f(&v["lo"], 0), f(&v["lo"], 1), f(&v["lo"], 2), 0.0);
        b.vox = Vec4::new(f(&v["vox"], 0), f(&v["vox"], 1), f(&v["vox"], 2), f(&v["vox"], 3));
    }
    Some(p)
}

/// `map_<id>_giiv_<v>.bin` "SHGI" v2 as one RGBA8 3D image (the boxes' coefficient volumes
/// stacked along the depth), filtered linearly and clamped; with its texel counts.
fn load_giiv(path: &std::path::Path, images: &mut Assets<Image>) -> Option<(Handle<Image>, [u32; 3])> {
    let d = std::fs::read(path).ok()?;
    let u32_ = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    if d.get(0..4) != Some(b"SHGI") || u32_(4)? != 2 {
        warn!("map giiv: {} is not SHGI v2 (re-export the map's probe: MAP_ENV_ONLY=1)", path.display());
        return None;
    }
    let dims = [u32_(8)?, u32_(12)?, u32_(16)?];
    let n = dims.iter().map(|&x| x as usize).product::<usize>() * 4;
    let data = d.get(20..20 + n)?.to_vec();
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width: dims[0], height: dims[1], depth_or_array_layers: dims[2] },
        bevy::render::render_resource::TextureDimension::D3,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    Some((images.add(image), dims))
}

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

/// The map id loaded last (`load_map_id`): what the world shows now, after any encounter
/// swap (`switch_map`). None before the first load or with the map off.
static LOADED_MAP: RwLock<Option<String>> = RwLock::new(None);

pub fn loaded_map() -> Option<String> {
    LOADED_MAP.read().ok()?.clone()
}

fn load_map(config: Res<GameConfig>, mut commands: Commands, assets: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<MapMaterial>>) {
    let Some(map) = map_id(&config) else { return };
    load_map_id(&map, &config, fight_row(&config.enemy.chr, config.enemy.npc_row), &mut commands, &assets, &mut meshes, &mut materials);
}

/// The castle top's lighting per fight, from m11_01_00_00.emevd (tools/boss_events.py
/// emevd_events; the survey is in kb/map.md "Castle top lighting"): (arena, boss NpcParam row)
/// -> (draw param variant `m11_01_<v>.gparam`, hour of its keys).
/// - Genichiro 11115800: Set Area Gparam Sub ID 0 + Set Lighting time of day 1 (the base file;
///   the real fight is flat grey daylight, which is the 12 h key of light set 100: the cool,
///   weak, high sun; the 6 h key is a warm low sun. gap: the exe's time-of-day -> hour table).
/// - After him 11115820: sub 10 + time of day 3 (dusk); the Owl 11115860 sub 50, Emma 11115910
///   and Isshin Ashina (m11_01_54_10 11115200) sub 40: `_0750` / `_0740` differ from the base
///   only at the 18 h key (sun colour and angle: the Owl's golden haze, Isshin's fiery sunset).
///   Isshin Ashina's time of day 4 (11115930) comes after his death.
/// `SHINOBI_HOUR` and `[` / `]` still override the hour; other fights keep the config's.
const FIGHT_LIGHTING: [(&str, i64, &str, f32); 5] = [
    ("boss_71001000", 71001000, "0000", 12.0),
    ("boss_71001000", 50600000, "0750", 18.0),
    ("boss_71001000", 74000010, "0740", 18.0),
    ("boss_71001000", 54300000, "0740", 18.0),
    ("boss_71001000", 71100000, "0000", 12.0),
];

fn fight_lighting(map: &str, row: Option<i64>) -> Option<(&'static str, f32)> {
    let row = row?;
    FIGHT_LIGHTING.iter().find(|f| f.0 == map && f.1 == row).map(|f| (f.2, f.3))
}

/// The fight's boss row: config enemy.npc_row, else the chr's roster row (enemy.rs set_fight
/// resolves it the same way; this runs before it at startup).
fn fight_row(chr: &str, npc_row: Option<i64>) -> Option<i64> {
    npc_row.or_else(|| {
        let text = std::fs::read_to_string(crate::paths::extracted().join(format!("enemies/{chr}.json"))).ok()?;
        serde_json::from_str::<serde_json::Value>(&text).ok()?["roster"]["npc"].as_i64()
    })
}

/// An encounter swap (enemy.rs SpawnEncounter, the picker) moves the fight to the new boss's
/// own arena, or back to the config's map, without a relaunch: the old pieces and lights go,
/// the new map loads (terrain, lighting, pieces; `apply_lighting` re-dresses the camera from
/// the fresh MapLighting) and Wolf stands on its start. Only with the config's `[world] map`
/// on; on the sandbox only to and from the fights it cannot hold (`OWN_ARENA_ROWS`);
/// `SHINOBI_MAP` pins one map for the run.
fn switch_map(
    mut msgs: MessageReader<crate::enemy::SpawnEncounter>,
    config: Res<GameConfig>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MapMaterial>>,
    old: Query<Entity, Or<(With<MapPiece>, With<Sun>, With<FillLight>)>>,
    mut wolf: Query<&mut Transform, With<crate::player::Player>>,
) {
    let Some(m) = msgs.read().last().cloned() else { return };
    let Some(base) = config.world.map.clone().filter(|m| !m.is_empty()) else { return };
    if std::env::var("SHINOBI_MAP").is_ok() {
        return;
    }
    let want = match boss_arena(&m.chr, m.npc_row) {
        // The sandbox, but for the fights it cannot hold (OWN_ARENA_ROWS).
        _ if base == crate::sandbox::MAP_ID => sandbox_exception(&m.chr, m.npc_row).unwrap_or(base),
        Some(a) => a,
        None if base == ARENAS_ONLY => {
            // No arena for this fight: back to the flat test floor.
            if loaded_map().is_some() {
                warn!("map switch: no arena for {} {:?}; the map stays until a relaunch (arenas only)", m.chr, m.npc_row);
            }
            return;
        }
        None => base,
    };
    if loaded_map().as_deref() == Some(want.as_str()) {
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    Terrain::clear();
    info!("map switch: {:?} -> {want} for {} {:?}", loaded_map(), m.chr, m.npc_row);
    load_map_id(&want, &config, fight_row(&m.chr, m.npc_row), &mut commands, &assets, &mut meshes, &mut materials);
    // The fight's row before the start search: `on_own_arena` must see the new boss, not the
    // old one, in a frame where enemy.rs reads the message after this (Bosses' request).
    crate::enemy::set_fight(&m.chr, m.npc_row);
    let start = player_start();
    for mut tf in &mut wolf {
        tf.translation = start;
    }
}

fn load_map_id(map: &str, config: &GameConfig, fight: Option<i64>, commands: &mut Commands, assets: &AssetServer, meshes: &mut Assets<Mesh>, materials: &mut Assets<MapMaterial>) {
    let map = map.to_string();
    if let Ok(mut l) = LOADED_MAP.write() {
        *l = Some(map.clone());
    }
    let dir = crate::paths::extracted();
    let t0 = std::time::Instant::now();
    // The sandbox (sandbox.rs): a generated arena under a real map's lighting and materials.
    let sandbox = crate::sandbox::is_generated(&map);
    let parkour = map == crate::sandbox::PARKOUR_ID;
    crate::docking::WALL_JUMP_LIMIT.store(if parkour { u8::MAX } else { crate::docking::WALL_JUMP_COUNT_MAX }, std::sync::atomic::Ordering::Relaxed);
    let id = if sandbox { crate::sandbox::source() } else { map.clone() };
    BOSS_ARENA.store(id.starts_with("boss_"), std::sync::atomic::Ordering::Relaxed);
    // Hit collision.
    if !sandbox {
        match std::fs::read(dir.join(format!("map_{id}.hit"))).ok().and_then(|d| Terrain::from_hit(&d)) {
            Some(t) => {
                info!("map {id}: {} collision triangles ({} material-less helper planes dropped)", t.triangle_count(), t.helper_planes);
                t.install();
            }
            None => warn!("map {id}: no map_{id}.hit (run `sekiro-extract map`)"),
        }
    }
    // Draw params + light set; the fight's own variant and hour on the castle top.
    let json: serde_json::Value = std::fs::read_to_string(dir.join(format!("map_{id}.json"))).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let fight = fight_lighting(&id, fight);
    let drawparam = match fight {
        Some(("0000", _)) | None => json["drawparam"].clone(),
        Some((v, _)) if !json["drawparam_variants"][v].is_null() => json["drawparam_variants"][v].clone(),
        Some((v, _)) => {
            warn!("map {id}: no draw param variant {v} in map_{id}.json (re-run tools/boss_arenas.py -force)");
            json["drawparam"].clone()
        }
    };
    let hour = std::env::var("SHINOBI_HOUR").ok().and_then(|v| v.parse().ok()).or(fight.map(|f| f.1)).unwrap_or(config.world.hour);
    if let Some((v, h)) = fight {
        info!("map {id}: fight lighting: draw params _{v} at {h} h");
    }
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
        drawparam,
        light_set: std::env::var("SHINOBI_LIGHT_SET").ok().and_then(|v| v.parse().ok()).unwrap_or(json["light_set"].as_i64().unwrap_or(0) as i32),
        hour,
        applied: false,
        env: if std::env::var("SHINOBI_MAP_NO_ENV").is_ok() { Vec::new() } else { env },
        arena_yaw: json["arena_yaw"].as_f64().unwrap_or(0.0) as f32,
        luts: json["luts"].as_array().map(|l| l.iter().filter_map(|n| n.as_u64().map(|n| n as u32)).filter(|n| dir.join(format!("map_{id}_lut_{n:04}.dds")).exists()).collect()).unwrap_or_default(),
        stem: format!("map_{id}"),
        sound_space: json["sound_space_type"].as_u64().unwrap_or(0).min(8) as u8,
        source_map: json["map"].as_str().unwrap_or_default().to_string(),
        map_origin: if sandbox { None } else { json["origin"].as_array().filter(|a| a.len() == 3).map(|a| Vec3::from_array(std::array::from_fn(|i| a[i].as_f64().unwrap_or(0.0) as f32))) },
    });
    // Wire points (grapple.md). The sandbox arena is generated: its source map's points would
    // hang in the air, so it has none.
    let grapple = if sandbox { Vec::new() } else { load_grapple(&json) };
    let ledges = if parkour { crate::sandbox::parkour_course().1 } else if sandbox { Vec::new() } else { load_ledges(&json) };
    info!("map {id}: {} grapple points, {} hang / hug edges", grapple.len(), ledges.len());
    commands.insert_resource(GrapplePoints(grapple));
    commands.insert_resource(LedgeEdges(ledges));
    // Sun shadows: `SHINOBI_MAP_SHADOW=<cascades>,<max m>` (default 2,60: the cascades are the
    // map's frame-rate cost, kb/map.md), `SHINOBI_MAP_NO_SHADOW=1` none.
    let shadow: Option<(usize, f32)> = std::env::var("SHINOBI_MAP_SHADOW").ok().and_then(|v| {
        let d: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (d.len() == 2).then(|| (d[0].max(1.0) as usize, d[1]))
    });
    let (cascades, max_distance) = shadow.unwrap_or((2, 60.0));
    if let Some(size) = std::env::var("SHINOBI_MAP_SHADOW_RES").ok().and_then(|v| v.parse::<usize>().ok()).or(config.video.shadow_res) {
        commands.insert_resource(bevy::light::DirectionalLightShadowMap { size: size.clamp(256, 8192) });
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
        crate::sandbox::build(&d, commands, assets, meshes, materials, &mut mats, &options, parkour);
        info!("map sandbox ({id}): loaded in {:.1} s", t0.elapsed().as_secs_f32());
        return;
    }
    let tex_override = options.tex_override.clone();
    let mut n = 0;
    let mut tris = 0;
    let mut no_cast = 0;
    let probe: Option<(f32, f32)> = std::env::var("SHINOBI_MAP_PROBE").ok().and_then(|v| {
        let (x, z) = v.split_once(',')?;
        Some((x.trim().parse().ok()?, z.trim().parse().ok()?))
    });
    let ok = walk_bin(&d, |b| {
        n += 1;
        let n_batch = n;
        tris += b.index_count / 3;
        let mut key = b.key();
        // `#<index>` skips one batch by its position in the bin.
        if key.0.iter().any(|n| options.skip.iter().any(|s| n.contains(s.as_str()))) || options.skip.iter().any(|s| s.strip_prefix('#').and_then(|x| x.parse::<usize>().ok()) == Some(n - 1)) {
            return;
        }
        // `SHINOBI_MAP_ONLY=#<index>[,#<index>]`: load only those batches.
        if !options.only.is_empty() && !options.only.contains(&(n - 1)) {
            return;
        }
        if options.no_layers {
            key.0[2..].iter_mut().for_each(|l| l.clear());
            key.3 = GameKey::default();
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
        if (!key.0[1].is_empty() || key.3.kind > 0) && !options.no_normals {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents(&pos, &nrm, &uv, &indices));
        }
        if key.3.kind > 0 {
            let (blend2, uv_scale) = b.game_vertices();
            mesh.insert_attribute(ATTR_MAP_BLEND2, blend2);
            mesh.insert_attribute(ATTR_MAP_UV_SCALE, uv_scale);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        if layered {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv2);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, blend);
        }
        // `SHINOBI_MAP_PROBE=x,z`: the final (mirrored, re-wound) triangles of this batch over
        // that spot below 3 m, with their geometric normal: is a floor front-facing from above?
        if let Some((px, pz)) = probe {
            let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap_or(&[]);
            for t in indices.chunks_exact(3) {
                let [a, b, c] = [Vec3::from(pos[t[0] as usize]), Vec3::from(pos[t[1] as usize]), Vec3::from(pos[t[2] as usize])];
                let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| {
                    let s = |p: Vec2, q: Vec2| (p.x - q.x) * (p.y - q.y);
                    let d1 = (p.x - b.x) * (a.y - b.y) - (a.x - b.x) * (p.y - b.y);
                    let d2 = (p.x - c.x) * (b.y - c.y) - (b.x - c.x) * (p.y - c.y);
                    let d3 = (p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y);
                    let _ = s;
                    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
                };
                if inside(Vec2::new(px, pz), a.xz(), b.xz(), c.xz()) && a.y.max(b.y).max(c.y) < 3.0 && a.y.min(b.y).min(c.y) > -1.0 {
                    let n = (b - a).cross(c - a).normalize_or_zero();
                    info!("map probe ({px}, {pz}): batch {} {:?} tri y {:.2} {:.2} {:.2} face normal {:.2?} ({})", n_batch - 1, key.0[0], a.y, b.y, c.y, n, if n.y > 0.0 { "up: front from above" } else { "down: culled from above" });
                }
            }
        }
        mesh.insert_indices(Indices::U32(indices));
        let material = mats.get(assets, materials, &options, &key);
        let mut e = commands.spawn((MapPiece, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default()));
        if !b.casts {
            e.insert(bevy::light::NotShadowCaster);
            no_cast += 1;
        }
    });
    if ok.is_none() {
        warn!("map {id}: map_{id}.bin is not a SHMP file or is cut short (re-export it)");
    }
    info!("map {id}: {n} meshes ({no_cast} without shadows), {tris} triangles, {} materials, loaded in {:.1} s", mats.len(), t0.elapsed().as_secs_f32());
}

// ---------------------------------------------------------------------------------------------
// The SHMP bin and its materials (shared with the sandbox)

/// A material of the bin: the nine texture stems (base albedo / normal, overlay albedo /
/// normal, snow albedo / normal, layer C (game path: L) albedo / normal, mask), the alpha mode,
/// two-sided, and the game path (bin v5).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct MaterialKey(pub [String; 9], pub u8, pub bool, pub GameKey);

/// The game path of a material (bin v5; kind 0 = none): M[Multiple] (1) / M[MultipleGround]
/// (2), the detail normal / layer mask / wet normal stems and the 64 MTD constants (f32 bits).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct GameKey {
    pub kind: u8,
    pub extra: [String; 3],
    pub params: Vec<u32>,
}

impl MaterialKey {
    /// Has a layer beyond the base (needs the blend bytes and the second UV set).
    pub(crate) fn layered(&self) -> bool {
        !self.0[2].is_empty() || !self.0[4].is_empty() || !self.0[6].is_empty() || self.3.kind > 0
    }
}

/// One batch of the bin, with its vertex and index data still packed.
pub(crate) struct BinBatch<'a> {
    d: &'a [u8],
    version: u32,
    albedo: String,
    normal: String,
    layers: [String; 7],
    game: GameKey,
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
        MaterialKey([self.albedo.clone(), self.normal.clone(), l[0].clone(), l[1].clone(), l[2].clone(), l[3].clone(), l[4].clone(), l[5].clone(), l[6].clone()], self.alpha, self.two_sided, self.game.clone())
    }

    /// Positions, normals (X mirrored into Bevy space), the two UV sets and the blend bytes
    /// (0..1), in that order.
    #[allow(clippy::type_complexity)]
    pub(crate) fn vertices(&self) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<[f32; 4]>) {
        let d = self.d;
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let vc = self.vertex_count;
        let stride = vertex_stride(self.version);
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

    /// The game path's extra vertex data (bin v5): (b2 tint rgb, b3 wet) and the uv scale;
    /// empty for older bins.
    pub(crate) fn game_vertices(&self) -> (Vec<[f32; 4]>, Vec<f32>) {
        if self.version < 5 {
            return (Vec::new(), Vec::new());
        }
        let d = self.d;
        let stride = vertex_stride(self.version);
        (0..self.vertex_count)
            .map(|i| {
                let v = self.vertices_at + i * stride;
                let b = |k: usize| d[v + 44 + k] as f32 / 255.0;
                ([b(0), b(1), b(2), b(3)], f32::from_le_bytes(d[v + 48..v + 52].try_into().unwrap()))
            })
            .unzip()
    }

    /// The triangle list as exported. FLVER triangles are DirectX clockwise; mirroring X
    /// (`vertices`) alone turns them into Bevy's counter-clockwise front faces. (Until
    /// 2026-10-11 the order was also swapped, as the Havok hit mesh needs (`Terrain::from_hit`,
    /// right-handed data): every drawn surface was inside out. Closed rocks and planks still
    /// looked right from their far walls, but open floors such as the Guardian Ape arena's
    /// pond bed were back-face culled away, kb/map.md.)
    pub(crate) fn indices(&self) -> Vec<u32> {
        let d = self.d;
        let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let mut indices = Vec::with_capacity(self.index_count);
        for t in 0..self.index_count / 3 {
            let i = self.indices_at + t * 12;
            indices.extend([u32_(i), u32_(i + 4), u32_(i + 8)]);
        }
        indices
    }
}

/// Bytes per vertex of a bin version.
fn vertex_stride(version: u32) -> usize {
    match version {
        0..=2 => 32,
        3 | 4 => 44,
        _ => 52,
    }
}

/// Walks the batches of a SHMP bin (versions 1-5; kb/map.md "Bin"); None when it is no SHMP.
/// Version 2 adds a shadow-caster byte per batch, 3 the layers and 44-byte vertices, 4 the
/// mask, 5 the game path (kind, three more stems, 64 constants; 52-byte vertices).
pub(crate) fn walk_bin(d: &[u8], mut f: impl FnMut(BinBatch)) -> Option<()> {
    if d.len() < 12 || &d[0..4] != b"SHMP" {
        return None;
    }
    // A truncated file (a copy that did not finish) ends the walk with None before any batch
    // reaches past the data; nothing here indexes `d` unchecked.
    let u16_ = |o: usize| Some(u16::from_le_bytes([*d.get(o)?, *d.get(o + 1)?]));
    let u32_ = |o: usize| Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?));
    let string = |o: &mut usize| -> Option<String> {
        let l = u16_(*o)? as usize;
        let s = String::from_utf8_lossy(d.get(*o + 2..*o + 2 + l)?).into_owned();
        *o += 2 + l;
        Some(s)
    };
    let version = u32_(4)?;
    let n = u32_(8)? as usize;
    let mut o = 12;
    for _ in 0..n {
        let albedo = string(&mut o)?;
        let normal = string(&mut o)?;
        let mut layers: [String; 7] = Default::default();
        if version >= 3 {
            for l in layers.iter_mut().take(if version >= 4 { 7 } else { 6 }) {
                *l = string(&mut o)?;
            }
        }
        let mut game = GameKey::default();
        if version >= 5 {
            game.kind = *d.get(o)?;
            o += 1;
            for e in game.extra.iter_mut() {
                *e = string(&mut o)?;
            }
            game.params = (0..64).map(|k| u32_(o + k * 4)).collect::<Option<Vec<u32>>>()?;
            o += 256;
        }
        let alpha = *d.get(o)?;
        let two_sided = *d.get(o + 1)? != 0;
        o += 2;
        let casts = version < 2 || {
            o += 1;
            *d.get(o - 1)? != 0
        };
        let vertex_count = u32_(o)? as usize;
        o += 4;
        let vertices_at = o;
        o += vertex_count * vertex_stride(version);
        let index_count = u32_(o)? as usize;
        o += 4;
        let indices_at = o;
        o += index_count * 4;
        if o > d.len() {
            return None;
        }
        f(BinBatch { d, version, albedo, normal, layers, game, alpha, two_sided, casts, vertex_count, vertices_at, index_count, indices_at });
    }
    Some(())
}

/// The environment knobs of the map materials (kb/map.md "Knobs").
pub(crate) struct MaterialOptions {
    /// `SHINOBI_MAP_NO_NORMALS=1`: no normal maps.
    pub no_normals: bool,
    /// `SHINOBI_MAP_TWO_SIDED=1`: no back-face culling (debug: is a piece wound inside out?).
    pub two_sided: bool,
    /// `SHINOBI_MAP_UNLIT=1`: the raw albedo.
    pub unlit: bool,
    /// `SHINOBI_MAP_TEX=<stem>`: one texture on every piece (a UV check).
    pub tex_override: Option<String>,
    /// `SHINOBI_MAP_SKIP=<text>[,<text>]`: batches whose texture names contain one of them
    /// are not drawn (which surface is that?).
    pub skip: Vec<String>,
    /// Batch indices to load, all when empty (`SHINOBI_MAP_ONLY`).
    pub only: Vec<usize>,
    /// `SHINOBI_MAP_NO_LAYERS=1`: the base layer only.
    pub no_layers: bool,
    /// `SHINOBI_MAP_SOFT`: the mask blend softness (default 0.25).
    pub softness: f32,
    /// `SHINOBI_MAP_SMOKE_SPEED`: the cloud sea's pace (map_material.wgsl `smoke`, default 1).
    pub smoke_speed: f32,
    /// `SHINOBI_MAP_SMOKE_TILE`: how often the cloud sea's mask repeats over its uv (default 8).
    pub smoke_tile: f32,
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
            two_sided: std::env::var("SHINOBI_MAP_TWO_SIDED").is_ok(),
            unlit: std::env::var("SHINOBI_MAP_UNLIT").is_ok(),
            tex_override: std::env::var("SHINOBI_MAP_TEX").ok().filter(|s| !s.is_empty()),
            skip: std::env::var("SHINOBI_MAP_SKIP").map(|v| v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
            only: std::env::var("SHINOBI_MAP_ONLY").map(|v| v.split(',').filter_map(|s| s.trim().trim_start_matches('#').parse().ok()).collect()).unwrap_or_default(),
            no_layers: std::env::var("SHINOBI_MAP_NO_LAYERS").is_ok(),
            softness: std::env::var("SHINOBI_MAP_SOFT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25),
            smoke_speed: std::env::var("SHINOBI_MAP_SMOKE_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            smoke_tile: std::env::var("SHINOBI_MAP_SMOKE_TILE").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0),
            snow_up,
            w_flags: ((std::env::var("SHINOBI_MAP_SHOW_BLEND").is_ok() as u32) << 1) | byte2,
        }
    }
}

/// The map materials built so far, one per key.
#[derive(Default)]
pub(crate) struct MaterialCache {
    built: HashMap<MaterialKey, Handle<MapMaterial>>,
    /// Missing textures already warned about (once per load, not once per material).
    warned: HashSet<String>,
}

impl MaterialCache {
    pub(crate) fn len(&self) -> usize {
        self.built.len()
    }

    /// The material for the key, built on first use (kb/map.md "Layers").
    pub(crate) fn get(&mut self, assets: &AssetServer, materials: &mut Assets<MapMaterial>, o: &MaterialOptions, key: &MaterialKey) -> Handle<MapMaterial> {
        if let Some(h) = self.built.get(key) {
            return h.clone();
        }
        let MaterialKey(names, alpha, two_sided, game) = key;
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
        // A texture the export did not write (`m17_rock_92_3m` on the Guardian Ape arena: the
        // FLVER names a mask that is in none of the game's m17 packs, so the export reports it
        // missing) must not reach the material: Bevy never prepares a material whose image
        // failed to load, so the whole batch would stay undrawn. The layer is dropped instead.
        let warned = &mut self.warned;
        let mut have = |stem: &str| {
            let ok = stem.is_empty() || crate::paths::extracted().join(format!("tex/{stem}.dds")).is_file();
            if !ok && warned.insert(stem.to_string()) {
                warn!("map texture tex/{stem}.dds missing (not in the game's packs either?): layer dropped");
            }
            ok
        };
        let normal_h = (!normal.is_empty() && !o.no_normals && have(normal)).then(|| tex(normal, false));
        let over = (!over_a.is_empty() && have(over_a)).then(|| tex(over_a, true));
        let over_n = (over.is_some() && !over_n.is_empty() && !o.no_normals && have(over_n)).then(|| tex(over_n, false));
        let snow = (!snow_a.is_empty() && have(snow_a)).then(|| tex(snow_a, true));
        let snow_n = (snow.is_some() && !snow_n.is_empty() && !o.no_normals && have(snow_n)).then(|| tex(snow_n, false));
        let c = (!c_a.is_empty() && have(c_a)).then(|| tex(c_a, true));
        let c_n = (c.is_some() && !c_n.is_empty() && !o.no_normals && have(c_n)).then(|| tex(c_n, false));
        // Alpha 3, the Divine Dragon arena's cloud sea (M[Flow]_Smoke, the exporter's `choose`):
        // the "normal" is its flow vector map, the mask its height / blend mask; the shader's
        // `smoke` scrolls them (kb/map.md). The MTD's light tint M_Flow__Smoke color_2 is the base
        // colour, color_3 the shadow tint in the shader; `SHINOBI_MAP_SMOKE_SPEED` its pace.
        let smoke = *alpha == 3;
        let mask = ((layered || smoke) && !mask_t.is_empty() && have(mask_t)).then(|| tex(mask_t, false));
        let bits = |a: &Option<Handle<Image>>, n: &Option<Handle<Image>>| a.is_some() as u32 | ((n.is_some() as u32) << 1);
        // The game path's own textures (bin v5).
        let [detail_t, mask_l_t, wet_t] = &game.extra;
        let on = game.kind > 0;
        let detail = (on && !detail_t.is_empty() && !o.no_normals && have(detail_t)).then(|| tex(detail_t, false));
        let mask_l = (on && !mask_l_t.is_empty() && have(mask_l_t)).then(|| tex(mask_l_t, false));
        let wet = (on && !wet_t.is_empty() && !o.no_normals && have(wet_t)).then(|| tex(wet_t, false));
        let albedo_ok = !albedo.is_empty() && have(albedo);
        let flags = UVec4::new(normal_h.is_some() as u32 | (bits(&c, &c_n) << 1), bits(&over, &over_n), bits(&snow, &snow_n), (!albedo.is_empty()) as u32 | o.w_flags);
        // Water surfaces (the Guardian Ape's pond and stream, the gate's river): no albedo, the
        // shared `m99_water_n` ripple normal, and the game's own water shader (reflection,
        // refraction, flow). Stand-in: a dark translucent surface so a wader's legs show
        // through instead of an opaque grey floor at knee height. gap: the water shader.
        let water = albedo.is_empty() && normal.contains("water");
        let h = materials.add(MapMaterial {
            base: StandardMaterial {
                base_color_texture: (!albedo.is_empty() && have(albedo)).then(|| tex(albedo, true)),
                base_color: if water {
                    Color::srgba(0.10, 0.15, 0.16, 0.6)
                } else if smoke {
                    Color::srgb(0.784, 0.863, 0.980)
                } else if albedo.is_empty() {
                    Color::srgb(0.45, 0.43, 0.40)
                } else {
                    Color::WHITE
                },
                perceptual_roughness: 0.85,
                unlit: o.unlit || smoke,
                alpha_mode: match alpha {
                    _ if water || smoke => AlphaMode::Blend,
                    1 => AlphaMode::AlphaToCoverage,
                    2 => AlphaMode::Blend,
                    _ => AlphaMode::Opaque,
                },
                double_sided: *two_sided || o.two_sided,
                cull_mode: if *two_sided || o.two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
                ..default()
            },
            extension: {
                let game_flags = [
                    (normal_h.is_some(), GAME_NORMAL),
                    (over.is_some(), GAME_OVER),
                    (over_n.is_some(), GAME_OVER_N),
                    (snow.is_some(), GAME_SNOW),
                    (snow_n.is_some(), GAME_SNOW_N),
                    (c.is_some(), GAME_L),
                    (c_n.is_some(), GAME_L_N),
                    (mask.is_some(), GAME_MASK),
                    (detail.is_some(), GAME_DETAIL),
                    (mask_l.is_some(), GAME_MASK_L),
                    (wet.is_some(), GAME_WET),
                    (albedo_ok, GAME_ALBEDO),
                ]
                .iter()
                .fold(0, |f, (has, bit)| if *has { f | bit } else { f });
                let mut p = [Vec4::ZERO; 16];
                if game.params.len() == 64 {
                    for (k, v) in p.iter_mut().enumerate() {
                        *v = Vec4::from_array(std::array::from_fn(|c| f32::from_bits(game.params[k * 4 + c])));
                    }
                    // The snow direction is in arena space; Bevy mirrors X on load.
                    p[14].x = -p[14].x;
                }
                let game_u = MapGame { flags: UVec4::new(game_flags, game.kind as u32, (o.w_flags >> 1) & 1, 0), p };
                MapExt { normal: normal_h, over_albedo: over, over_normal: over_n, snow_albedo: snow, snow_normal: snow_n, c_albedo: c, c_normal: c_n, mask_params: Vec4::new(mask.is_some() as u32 as f32, o.softness, smoke as u32 as f32, o.smoke_speed), mask, flags, params: Vec4::new(if smoke { o.smoke_tile } else { 0.0 }, 1.0, o.snow_up[0], o.snow_up[1]), detail, mask_l, wet, game: game_u, ..default() }
            },
        });
        self.built.insert(key.clone(), h.clone());
        h
    }
}

// ---------------------------------------------------------------------------------------------
// Bodies on the map

/// The longest step one fixed tick can take as a walk (2 m at 60 Hz is 120 m/s, past any dash
/// or grapple pull); a longer one is a teleport and skips the wall slide.
pub const TELEPORT: f32 = 2.0;

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
        // Held at an edge (docking.rs hold places the body): no slide, no floor snap.
        if a.docked {
            state.prev = tf.translation;
            continue;
        }
        let prev = state.prev;
        let feet_prev = prev - Vec3::Y * CAPSULE_HALF_HEIGHT;
        let delta = Vec2::new(tf.translation.x - prev.x, tf.translation.z - prev.z);
        // A move longer than TELEPORT in one tick is a warp (script / event warps, the parking
        // spot, fight resets), not a walk: no slide along the line. The slide gathered every
        // triangle in the whole move's box and ran 512 substeps, so a 110 m script warp on the
        // Folding Screen Monkeys arena stalled one frame for about 33 s, then stopped at the
        // first wall and undid the warp (the fight's "3 fps", 2026-10-12 bug hunt).
        if delta.length_squared() > 1e-10 && delta.length_squared() <= TELEPORT * TELEPORT {
            let moved = t.slide(feet_prev, delta);
            tf.translation.x = prev.x + moved.x;
            tf.translation.z = prev.z + moved.y;
        }
        let data = crate::actor::data_for(&combat, &a);
        let lifted = !a.anim.is_empty() && data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY);
        // An enemy coming down from a SetNoGravity lift (actor.rs) lands by itself.
        if !a.airborne && !lifted && a.lift_ground.is_none() {
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
        // `SHINOBI_MESH_OWN_SLABS=1`: every mesh in its own GPU buffer (no shared slabs), to
        // tell a mesh-allocator fault from a material one when a piece is not drawn.
        if std::env::var("SHINOBI_MESH_OWN_SLABS").is_ok() {
            if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
                info!("mesh allocator: every mesh in its own slab (SHINOBI_MESH_OWN_SLABS)");
                render_app.insert_resource(bevy::render::mesh::allocator::MeshAllocatorSettings {
                    slab_allocator_settings: bevy::render::slab_allocator::SlabAllocatorSettings { large_threshold: 0, ..default() },
                    ..default()
                });
            }
        }
        bevy::asset::embedded_asset!(app, "map_material.wgsl");
        app.add_plugins(MaterialPlugin::<MapMaterial>::default());
        app.init_resource::<GrapplePoints>();
        app.init_resource::<LedgeEdges>();
        app.add_systems(Update, (draw_grapple_points, draw_ledges).run_if(resource_exists::<crate::combat::ShowHurtboxes>));
        app.add_systems(Startup, (load_map, log_bindless))
            .add_systems(PostStartup, settle_player_start)
            .add_systems(Update, ((hour_keys, apply_lighting, apply_giiv).chain().run_if(resource_exists::<MapLighting>), frame_rate_log, switch_map))
            .add_systems(Startup, crate::sandbox::spawn_enemies.after(load_map))
            .add_systems(Update, (crate::sandbox::place_wolf, crate::sandbox::watch, mesh_events, crate::sandbox::respawn_lineup, crate::sandbox::catalog_shots.run_if(resource_exists::<crate::sandbox::Catalog>)))
            .add_systems(FixedUpdate, bodies_on_terrain.after(ActorSet::Advance).before(crate::anim::PoseSet));
    }
}

/// Whether the map material batches (bindless): the GPU must have binding arrays and room for
/// the slab's samplers, otherwise Bevy quietly binds one material at a time.
fn log_bindless(device: Option<Res<bevy::render::renderer::RenderDevice>>) {
    if let Some(d) = device {
        let l = d.limits();
        info!("map material bindless: {} (standard {}; sampler array limit {}, storage buffers {})", bevy::pbr::material_uses_bindless_resources::<MapMaterial>(&d), bevy::pbr::material_uses_bindless_resources::<StandardMaterial>(&d), l.max_binding_array_sampler_elements_per_shader_stage, l.max_storage_buffers_per_shader_stage);
    }
}

/// One line with the average frame rate over seconds 5..15 after start (the map's cost).
fn frame_rate_log(time: Res<Time<Real>>, mut acc: Local<(f32, u32, f32)>) {
    // Real time: the title screen pauses the virtual clock (menu.rs), which never reached 15 s.
    // One line per 10 s window from 5 s to 65 s (the first ones include the load).
    let t = time.elapsed_secs();
    if t < 5.0 || acc.2 >= 65.0 {
        return;
    }
    if acc.2 == 0.0 {
        acc.2 = 5.0;
    }
    acc.0 += time.delta_secs();
    acc.1 += 1;
    if t >= acc.2 + 10.0 {
        info!("map frame rate: {:.0} fps over {} frames, {:.0}-{:.0} s ({})", acc.1 as f32 / acc.0, acc.1, acc.2, acc.2 + 10.0, if terrain().is_some() { "map" } else { "no map" });
        *acc = (0.0, 0, acc.2 + 10.0);
    }
}

/// `SHINOBI_MESH_EVENTS=1`: logs every mesh asset event of the first 600 frames with the frame
/// number and, for a removed or modified mesh, which entities carry it (the startup
/// `slab_allocator Use-after-free` error: a mesh copied for the GPU after its slab was freed).
fn mesh_events(mut events: MessageReader<AssetEvent<Mesh>>, mut frame: Local<u32>, carriers: Query<(Entity, &Mesh3d, Option<&Name>, Option<&ChildOf>)>, names: Query<&Name>, meshes: Res<Assets<Mesh>>) {
    *frame += 1;
    if *frame > 600 || std::env::var("SHINOBI_MESH_EVENTS").is_err() {
        events.clear();
        return;
    }
    for ev in events.read() {
        let (kind, id) = match ev {
            AssetEvent::Added { id } => ("added", *id),
            AssetEvent::Modified { id } => ("modified", *id),
            AssetEvent::Removed { id } => ("removed", *id),
            AssetEvent::Unused { id } => ("unused", *id),
            AssetEvent::LoadedWithDependencies { id } => ("loaded", *id),
        };
        if kind == "added" {
            // The first frames only: what each new mesh is (vertices, attributes), so a mesh
            // removed before any entity carried it can be told apart.
            if *frame <= 3 {
                let what = meshes.get(id).map(|m| format!("{} vertices, attrs [{}]", m.count_vertices(), m.attributes().map(|(a, _)| a.name).collect::<Vec<_>>().join(" ")));
                info!("mesh event frame {}: added {id:?}: {}", *frame, what.unwrap_or_default());
            }
            continue;
        }
        let who: Vec<String> = carriers
            .iter()
            .filter(|(_, m, _, _)| m.0.id() == id)
            .map(|(e, _, n, p)| format!("{e} {} under {}", n.map_or("-", |n| n.as_str()), p.and_then(|p| names.get(p.parent()).ok()).map_or("-", |n| n.as_str())))
            .collect();
        info!("mesh event frame {}: {kind} {id:?} on [{}]", *frame, who.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[world] map = "arenas"`: a fight with an exported arena loads it, any other fight gets
    /// no map (the flat test floor), never the base map.
    #[test]
    fn arenas_only_loads_just_the_arenas() {
        if std::env::var("SHINOBI_MAP").is_ok() || boss_arena("c1020", Some(10200000)).is_none() {
            return;
        }
        let mut config: GameConfig = toml::from_str(&std::fs::read_to_string(crate::paths::root().join("config.toml")).unwrap()).unwrap();
        config.world.map = Some(ARENAS_ONLY.into());
        config.enemy.chr = "c1020".into();
        config.enemy.npc_row = Some(10200000);
        assert_eq!(map_id(&config), boss_arena("c1020", Some(10200000)));
        config.enemy.npc_row = Some(1);
        assert_eq!(map_id(&config), None);
        config.world.map = Some("m11_01_00_00".into());
        assert_eq!(map_id(&config).as_deref(), Some("m11_01_00_00"));
    }

    /// Every exported map's Env_Box volumes (sekiro-extract `export_giiv`) fit the shader's
    /// table: at most GIIV_MAX boxes in priority order, each one's kept voxels inside its grid
    /// and inside the atlas, the atlas of each variant as the json says.
    #[test]
    fn giiv_boxes_fit_their_atlas() {
        let dir = crate::paths::extracted();
        let mut seen = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(stem) = name.strip_suffix("_giiv.json") else { continue };
            let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(e.path()).unwrap()).unwrap();
            let p = giiv_params(&j).unwrap_or_else(|| panic!("{name}: no boxes"));
            let count = p.dims.w as usize;
            assert!(count > 0 && count <= GIIV_MAX, "{name}: {count} boxes");
            let prio: Vec<u64> = j["boxes"].as_array().unwrap().iter().map(|b| b["priority"].as_u64().unwrap()).collect();
            assert!(prio.windows(2).all(|w| w[0] <= w[1]), "{name}: priorities {prio:?} out of drawing order");
            let mut depth = 0.0;
            for b in &p.boxes[..count] {
                assert!((b.lo + b.vox).truncate().cmple(b.grid.truncate()).all(), "{name}: crop past the grid {b:?}");
                assert!(b.vox.x <= p.dims.x && b.vox.y <= p.dims.y && b.vox.w == depth, "{name}: box outside the atlas {b:?}");
                assert!(b.size.w > 0.0, "{name}: no fade {b:?}");
                depth += 4.0 * b.vox.z;
            }
            assert_eq!(depth, p.dims.z, "{name}: atlas depth");
            for v in j["variants"].as_array().unwrap() {
                let bin = std::fs::read(dir.join(format!("{stem}_giiv_{:02}.bin", v.as_u64().unwrap()))).unwrap();
                let w = |o: usize| u32::from_le_bytes(bin[o..o + 4].try_into().unwrap()) as f32;
                assert_eq!((&bin[0..4], w(4)), (&b"SHGI"[..], 2.0), "{name}");
                assert_eq!(Vec3::new(w(8), w(12), w(16)), p.dims.truncate(), "{name}: variant {v}");
                assert_eq!(bin.len(), 20 + (p.dims.x * p.dims.y * p.dims.z) as usize * 4, "{name}: variant {v} size");
            }
            seen += 1;
        }
        assert!(seen > 0, "no map_*_giiv.json exported");
    }

    /// The map material stays bindless (base and extension both), so Bevy can batch the map
    /// pieces into multi-draws.
    #[test]
    fn map_material_is_bindless() {
        use bevy::render::render_resource::AsBindGroup;
        assert!(<MapMaterial as AsBindGroup>::bindless_slot_count().is_some());
    }

    /// A fog wall's collision (map_<id>.gates) stops a body only on the terrain it was switched
    /// on for, and only while it stands.
    #[test]
    fn fog_wall_gates_block_only_while_on() {
        // One gate (entity 7): a 4 m x 3 m wall at export x = 1 (Bevy x = -1, X mirrored).
        let mut d = b"SHMG".to_vec();
        for x in [1u32, 1, 7, 4] {
            d.extend_from_slice(&x.to_le_bytes());
        }
        for v in [[1.0f32, 0.0, -2.0], [1.0, 3.0, -2.0], [1.0, 3.0, 2.0], [1.0, 0.0, 2.0]] {
            v.iter().for_each(|f| d.extend_from_slice(&f.to_le_bytes()));
        }
        d.extend_from_slice(&2u32.to_le_bytes());
        for t in [[0u32, 1, 2, 29], [0, 2, 3, 29]] {
            t.iter().for_each(|x| d.extend_from_slice(&x.to_le_bytes()));
        }
        let mut gates = Terrain::gates_from(&d);
        assert_eq!(gates.len(), 1);
        let gate = Arc::new(gates.remove(&7).unwrap());
        assert_eq!(gate.triangle_count(), 2);
        assert!(Terrain::gates_from(&d[..d.len() - 3]).is_empty(), "a truncated file must give no gate");
        // A floor of 20 m.
        let floor = |y: f32| {
            let v = vec![Vec3::new(-10.0, y, -10.0), Vec3::new(10.0, y, -10.0), Vec3::new(10.0, y, 10.0), Vec3::new(-10.0, y, 10.0)];
            Arc::new(Terrain::from_tris(v, [([0, 2, 1], 1), ([0, 3, 2], 1)].into_iter()))
        };
        let (base, other) = (floor(0.0), floor(0.0));
        let walk = Vec2::new(-3.0, 0.0);
        let set = |on: Option<&Arc<Terrain>>| *GATES_ON.get_or_init(Default::default).write().unwrap() = (on.cloned(), on.map(|_| vec![gate.clone()]).unwrap_or_default());
        set(Some(&base));
        let moved = base.slide(Vec3::ZERO, walk);
        assert!(moved.x > -1.0 + RADIUS - 0.05 && moved.x < -0.5, "the wall did not stop the walk: {moved}");
        assert!((other.slide(Vec3::ZERO, walk).x + 3.0).abs() < 1e-3, "another terrain must not see the gate");
        set(None);
        assert!((base.slide(Vec3::ZERO, walk).x + 3.0).abs() < 1e-3, "the wall still stands after it was switched off");
    }

    /// The metering weight is 1 at the centre and falls with the screen radius; the adaptation
    /// speed reproduces the game's exponential rate.
    #[test]
    fn metering_follows_the_games_centre_weight_and_rate() {
        assert!((centre_weight(Vec2::splat(0.5), 4.0, 9.0 / 16.0) - 1.0).abs() < 1e-6);
        let edge = centre_weight(Vec2::new(1.0, 0.5), 4.0, 9.0 / 16.0);
        assert!((edge - (1.0 - 0.9375f32.powi(3))).abs() < 1e-5, "{edge}");
        assert!(centre_weight(Vec2::new(0.5, 1.0), 4.0, 9.0 / 16.0) > edge, "the short axis is weighted less steeply");
        // One 1/60 s frame 3 EV off: Bevy's step (delta * speed * dt / span) vs the game's.
        let (dt, span, delta) = (1.0 / 60.0, 16.0, 3.0);
        let bevy = delta * adapt_speed(1.0, span) * dt / span;
        let game = delta * (1.0 - 10f32.powf(-dt * 1.0));
        assert!((bevy - game).abs() / game < 0.03, "{bevy} vs {game}");
    }

    /// The castle roof's bloom (Bloom[Yebis], id 0): at 18 h Alpha 0.3, Luminance 1, Smooth 0.8,
    /// 7 levels, threshold 0. Bevy's additive chain must weigh level i as the exe's
    /// Alpha * Smooth^i / sum Smooth^j: b(0) * b(1) * .. * b(i) with Bevy's own blend factor.
    #[test]
    fn the_bloom_weighs_its_levels_like_the_games() {
        let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("map_boss_71001000.json")) else {
            eprintln!("no castle export: skipped");
            return;
        };
        let j: serde_json::Value = serde_json::from_str(&text).unwrap();
        let l = MapLighting {
            drawparam: j["drawparam"].clone(),
            light_set: 0,
            hour: 18.0,
            applied: false,
            env: Vec::new(),
            arena_yaw: 0.0,
            luts: Vec::new(),
            stem: String::new(),
            sound_space: 0,
            source_map: String::new(),
            map_origin: None,
        };
        let b = map_bloom(&l, 0).expect("the castle's bloom is on");
        assert_eq!(b.prefilter.threshold, 0.0);
        let mips = b.max_mip_dimension.ilog2().max(2) - 1;
        assert_eq!(mips, 7);
        // Bevy 0.19 bloom/mod.rs compute_blend_factor, Additive.
        let blend = |mip: f32, max: f32| {
            let lf = (1.0 - (1.0 - mip / max).powf(1.0 / (1.0 - b.low_frequency_boost_curvature))) * b.low_frequency_boost;
            let hp = 1.0 - (((mip / max) - b.high_pass_frequency) / b.high_pass_frequency).clamp(0.0, 1.0);
            (b.intensity + lf) * hp
        };
        let sum: f32 = (0..7).map(|i| 0.8f32.powi(i)).sum();
        let mut w = 1.0;
        for i in 0..mips {
            w *= blend(i as f32, (mips - 1) as f32);
            let game = 0.3 * 0.8f32.powi(i as i32) / sum;
            assert!((w - game).abs() < 1e-4 * game.max(1e-3), "level {i}: {w} vs {game}");
        }
        // The night key's threshold (0 h: 0.075).
        let night = MapLighting { hour: 0.0, ..l };
        assert!((map_bloom(&night, 0).unwrap().prefilter.threshold - 0.075).abs() < 1e-6);
    }

    /// The castle's fog at noon: FixedDensity 0.0015 over GameScale 2.5 (exe 39e7e0).
    #[test]
    fn fog_density_is_divided_by_game_scale() {
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("map_m11_01_00_00.json")).unwrap()).unwrap();
        let l = MapLighting {
            drawparam: j["drawparam"].clone(),
            light_set: 0,
            hour: 12.0,
            applied: false,
            env: Vec::new(),
            arena_yaw: 0.0,
            luts: Vec::new(),
            stem: String::new(),
            sound_space: 0,
            source_map: String::new(),
            map_origin: None,
        };
        assert!((l.fog_density(0) - 0.0015 / 2.5).abs() < 1e-7, "{}", l.fog_density(0));
    }

    /// Bin v5 (the exporter's writer): the game path's kind, stems and constants come back,
    /// and the 52-byte vertices give the blend bytes, the tint / wet bytes and the uv scale.
    #[test]
    fn bin_v5_reads_the_game_path() {
        let mut d = b"SHMP".to_vec();
        d.extend(5u32.to_le_bytes());
        d.extend(1u32.to_le_bytes());
        let s = |d: &mut Vec<u8>, t: &str| {
            d.extend((t.len() as u16).to_le_bytes());
            d.extend(t.as_bytes());
        };
        for t in ["base_a", "base_n", "", "", "", "", "wood_51_a", "", "m9_3m"] {
            s(&mut d, t);
        }
        d.push(1);
        for t in ["detail_n", "wood_20_3m", ""] {
            s(&mut d, t);
        }
        (0..64).for_each(|k| d.extend((k as f32 * 0.5).to_le_bytes()));
        d.extend([0, 0, 1]);
        d.extend(1u32.to_le_bytes());
        for x in [1.0f32, 2.0, 3.0, 0.0, 1.0, 0.0, 0.25, 0.5, 0.75, 1.0] {
            d.extend(x.to_le_bytes());
        }
        d.extend([255, 128, 0, 255, 51, 102, 153, 204]);
        d.extend(2.5f32.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        let mut seen = 0;
        walk_bin(&d, |b| {
            seen += 1;
            let k = b.key();
            assert_eq!(k.3.kind, 1);
            assert_eq!(k.3.extra, ["detail_n".to_string(), "wood_20_3m".into(), String::new()]);
            assert_eq!(f32::from_bits(k.3.params[7]), 3.5);
            assert!(k.layered() && b.casts);
            let (pos, _, _, uv2, blend) = b.vertices();
            assert_eq!(pos[0], [-1.0, 2.0, 3.0]);
            assert_eq!(uv2[0], [0.75, 1.0]);
            assert_eq!(blend[0], [1.0, 128.0 / 255.0, 0.0, 1.0]);
            let (b2, scale) = b.game_vertices();
            assert_eq!(b2[0], [0.2, 0.4, 0.6, 0.8]);
            assert_eq!(scale[0], 2.5);
        })
        .unwrap();
        assert_eq!(seen, 1);
    }

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

    #[test]
    #[ignore]
    fn grapple_floor_probe() {
        let Some(t) = hit() else { return };
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("map_m11_01_00_00.json")).unwrap()).unwrap();
        let mut g = load_grapple(&j);
        g.sort_by(|a, b| a.hook.xz().length().total_cmp(&b.hook.xz().length()));
        let mut landed = 0;
        for p in &g {
            // A floor under the landing spot within 1.5 m: the branch / roof the object's
            // own collision gives (none before 2026-10-11's object hit export).
            let floor = t.floor(p.land.x, p.land.z, p.land.y + 0.3);
            if floor.is_some_and(|f| p.land.y - f <= 1.5) {
                landed += 1;
            }
        }
        eprintln!("{} of {} landing spots have a floor within 1.5 m under them", landed, g.len());
        for p in g.iter().take(8) {
            let floor = t.floor(p.land.x, p.land.z, p.land.y + 0.3);
            eprintln!("hook {:?} land y {:.2} var {} floor under the landing spot {:?}", p.hook, p.land.y, p.variation, floor);
        }
    }

    /// Where the objects' own collision (2026-10-11) changes the walkable floor around the
    /// General's fight: a grid over the arena, the gate .hit against a `MAP_NO_OBJECT_HIT=1
    /// MAP_NAME=gate_noobj` export of the same map (not kept; re-export it for this).
    #[test]
    #[ignore]
    fn object_collision_floor_diff() {
        let load = |n: &str| std::fs::read(crate::paths::extracted().join(n)).ok().and_then(|d| Terrain::from_hit(&d));
        let (Some(with), Some(without)) = (load("map_m11_01_00_00.hit"), load("map_gate_noobj.hit")) else { return };
        let mut raised = Vec::new();
        let mut n = 0;
        for ix in -40..=40 {
            for iz in -40..=40 {
                let (x, z) = (ix as f32 * 0.5, iz as f32 * 0.5);
                let base = without.floor(x, z, 3.0);
                let now = with.floor(x, z, 3.0);
                if let (Some(b), Some(w)) = (base, now) {
                    n += 1;
                    if w - b > 0.05 {
                        raised.push((x, z, b, w));
                    }
                }
            }
        }
        eprintln!("{} of {n} grid spots (0.5 m, +-20 m, from 3 m up) stand higher with the object collision", raised.len());
        if let Some((y, tri)) = with.floor_tri(3.0, -0.5, 3.0) {
            let t = with.tris[tri as usize];
            eprintln!("spot (3, -0.5): floor {y:.2} triangle {tri} vertices {:?} material {} at {:?} {:?} {:?}", t, with.materials[tri as usize], with.verts[t[0] as usize], with.verts[t[1] as usize], with.verts[t[2] as usize]);
        }
        raised.sort_by(|a, b| (b.3 - b.2).total_cmp(&(a.3 - a.2)));
        for (x, z, b, w) in raised.iter() {
            eprintln!("  ({x:5.1}, {z:5.1}) floor {b:6.2} -> {w:6.2} (+{:.2})", w - b);
        }
    }

    /// A copied map file that did not finish is reported, not a crash (release build C5).
    #[test]
    fn truncated_map_files_are_refused() {
        let dir = crate::paths::extracted();
        if let Ok(d) = std::fs::read(dir.join("map_m11_01_00_00.hit")) {
            assert!(Terrain::from_hit(&d[..d.len() / 2]).is_none());
            assert!(Terrain::from_hit(&d[..20]).is_none());
        }
        if let Ok(d) = std::fs::read(dir.join("map_m11_01_00_00.bin")) {
            let mut n = 0;
            assert!(walk_bin(&d[..d.len() / 3], |_| n += 1).is_none());
            assert!(walk_bin(&d[..13], |_| n += 1).is_none());
            let mut all = 0;
            assert!(walk_bin(&d, |_| all += 1).is_some());
            assert!(n < all && all > 0, "{n} of {all}");
        }
        let mut short = b"SHMP".to_vec();
        short.extend_from_slice(&4u32.to_le_bytes());
        short.extend_from_slice(&1u32.to_le_bytes());
        short.extend_from_slice(&[7, 0, b'a']);
        assert!(walk_bin(&short, |_| {}).is_none());
    }

    fn hit() -> Option<Terrain> {
        let d = std::fs::read(crate::paths::extracted().join("map_m11_01_00_00.hit")).ok()?;
        Terrain::from_hit(&d)
    }

    /// Every exported boss arena (extracted/map_boss_*.hit): the boss's spot has a floor, the
    /// player start search finds a spot with headroom (no fallback), and from that spot four
    /// 3 m walks (+-x, +-z, tick-sized slides as in play) never step over a hole (no collision
    /// at all under the feet). After AKJama/sekiro-rs `crates/sim/examples/map_baseline.rs`
    /// (every player start of every map, four walks each; MIT / Apache). Skipped when no arena
    /// was exported.
    #[test]
    fn every_arena_start_stands_and_walks() {
        let Ok(dir) = std::fs::read_dir(crate::paths::extracted()) else { return };
        let mut files: Vec<_> = dir.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("map_boss_") && n.ends_with(".hit"))).collect();
        files.sort();
        let mut bad = Vec::new();
        for f in &files {
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            let Some(t) = std::fs::read(f).ok().and_then(|d| Terrain::from_hit(&d)) else {
                bad.push(format!("{name}: unreadable"));
                continue;
            };
            if t.floor(0.0, -4.0, 1.0).is_none() {
                bad.push(format!("{name}: no floor under the boss's spot (0, -4)"));
            }
            let start = match arena_start(&t, false) {
                Ok(s) => s,
                Err(kept) => {
                    bad.push(format!("{name}: start search fell back to {kept}"));
                    kept
                }
            };
            let stranger = arena_start(&t, true);
            if stranger.is_ok_and(|s| s.distance(start) > 0.01) {
                println!("{name}: another boss's line-up moves the start to {}", stranger.unwrap());
            }
            let lineup = stranger.is_ok();
            let mut walks = Vec::new();
            for dir in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                let mut feet = start - Vec3::Y * CAPSULE_HALF_HEIGHT;
                let (mut walked, mut drop) = (0.0f32, 0.0f32);
                for _ in 0..20 {
                    let d = t.slide(feet, dir * 0.15);
                    feet.x += d.x;
                    feet.z += d.y;
                    walked += d.length();
                    match t.floor(feet.x, feet.z, feet.y + STEP_HEIGHT) {
                        Some(h) => {
                            drop = drop.max(feet.y - h);
                            feet.y = h;
                        }
                        None => {
                            bad.push(format!("{name}: walking {dir} from {start} hits a hole at {feet}"));
                            break;
                        }
                    }
                }
                walks.push(format!("{dir}: {walked:.1} m, drop {drop:.1}"));
            }
            eprintln!("{name}: start {start} (another boss's line-up fits: {lineup}); walks {}", walks.join(", "));
        }
        assert!(bad.is_empty(), "{} arenas checked:\n{}", files.len(), bad.join("\n"));
    }

    #[test]
    fn grapple_search_picks_the_point_on_the_view_axis_and_keeps_the_soft_lock() {
        let pt = |hook: Vec3, var: i64| GrapplePoint { hook, land: hook + Vec3::Y * 0.5, fwd: -Vec3::Z, variation: var };
        // Wolf at the origin looking +z from a camera 3 m behind him; the points ahead and up.
        let points = vec![
            pt(Vec3::new(0.0, 4.0, 8.0), 0),   // nearest the view axis, 8.9 m
            pt(Vec3::new(4.0, 6.0, 8.0), 0),   // off to the side
            pt(Vec3::new(0.0, 6.0, 30.0), 0),  // past findDistanceFar
            pt(Vec3::new(0.0, 6.0, -8.0), 0),  // behind the camera
            pt(Vec3::new(5.0, 2.0, 12.0), 41), // kite row (90/50/30/10): Wolf 23 deg left, 9 below
        ];
        let cam = Vec3::new(0.0, 1.5, -3.0);
        let fwd = Vec3::Z;
        let t = grapple_target(cam, fwd, Vec3::ZERO, None, &points).unwrap();
        assert_eq!(t.index, 0);
        assert!(t.in_action_range && t.ride, "{t:?}");
        assert!((t.dist - 80f32.sqrt()).abs() < 1e-3);
        // The side point stays the target while inside the soft-lock rectangle (3.575 m wide:
        // 4 m lateral is outside it, so the axis point wins again).
        assert_eq!(grapple_target(cam, fwd, Vec3::ZERO, Some(1), &points).unwrap().index, 0);
        let near = vec![points[0], pt(Vec3::new(2.0, 4.0, 8.0), 0)];
        assert_eq!(grapple_target(cam, fwd, Vec3::ZERO, Some(1), &near).unwrap().index, 1);
        // Too close (under actionDistanceNear 3 m) is found but not fireable.
        let close = vec![pt(Vec3::new(0.0, 2.0, 1.0), 0)];
        let t = grapple_target(cam, fwd, Vec3::ZERO, None, &close).unwrap();
        assert!(!t.in_action_range);
        // Row 20 "can't ride" and row 5's extended range.
        let far = vec![pt(Vec3::new(0.0, 6.0, 22.0), 5), pt(Vec3::new(0.0, 6.0, 22.0), 20)];
        let t = grapple_target(cam, fwd, Vec3::ZERO, None, &far).unwrap();
        assert!(t.in_action_range && t.ride, "{t:?}");
        assert!(!grapple_target(cam, fwd, Vec3::ZERO, Some(1), &far).unwrap().ride);
        // Behind the point's facing (fwd -z, Wolf at +z) with row 1's 90 deg all round: refused.
        let back = vec![GrapplePoint { hook: Vec3::new(0.0, 6.0, 8.0), land: Vec3::ZERO, fwd: Vec3::Z, variation: 1 }];
        assert!(grapple_target(cam, fwd, Vec3::ZERO, None, &back).is_none());
        // The json loader mirrors X.
        let j: serde_json::Value = serde_json::json!({"grapple": [{"hook": [1.0, 2.0, 3.0], "land": [1.0, 2.5, 3.0], "fwd": [1.0, 0.0, 0.0], "var": 3}]});
        let g = load_grapple(&j);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].hook, Vec3::new(-1.0, 2.0, 3.0));
        assert_eq!(g[0].fwd, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(g[0].variation, 3);
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
        // The line-up at z -4 reaches Wolf's start at z 4: the gatehouse door (o111410_8000,
        // an ObjAct door) is left out of the export, so no wall stands between them.
        let mut feet = Vec3::new(0.0, t.floor(0.0, -4.0, 1.0).unwrap(), -4.0);
        for _ in 0..200 {
            let d = t.slide(feet, Vec2::new(0.0, 0.15));
            feet.x += d.x;
            feet.z += d.y;
            feet.y = t.floor(feet.x, feet.z, feet.y + STEP_HEIGHT).unwrap_or(feet.y);
            if feet.z >= 4.0 {
                break;
            }
        }
        assert!(feet.z >= 4.0, "the walk from the line-up to Wolf's start stopped at {feet}");
    }

    /// The wire point table follows WireVariationParam: angles, extend and ridden / swung
    /// (targetMarkerType 0 / 1) for every map row the table names (row 5 was 180 left / right,
    /// row 40 ridden; 2026-10-12 bug hunt).
    #[test]
    fn wire_variation_matches_the_param() {
        let path = crate::paths::extracted().join("json/params/WireVariationParam.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("no WireVariationParam.json: skipped");
            return;
        };
        let j: serde_json::Value = serde_json::from_str(&text).unwrap();
        for r in j["rows"].as_array().unwrap() {
            let id = r["id"].as_i64().unwrap();
            if ![0, 1, 2, 3, 5, 20, 21, 22, 23, 24, 40, 41].contains(&id) {
                continue;
            }
            let f = |k: &str| r[k].as_f64().unwrap() as f32;
            let (ang, extend, ridden) = wire_variation(id);
            assert_eq!(ang, [f("recognitionAngleUp"), f("recognitionAngleDown"), f("recognitionAngleLeft"), f("recognitionAngleRight")], "row {id} angles");
            assert_eq!(extend, f("extendWireActionDistance"), "row {id} extend");
            assert_eq!(ridden, r["targetMarkerType"].as_i64() == Some(0), "row {id} ridden");
        }
    }
}
