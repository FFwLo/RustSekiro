//! The ground the fight stands on: the hit collision (floors, walls, a ray), filled by the host's
//! map (this game's map.rs from its own `.hit` export or sekiro-rs's play world, sr_map.rs) and
//! asked by Wolf, the enemies, bullets and effects. Floors are walkable within the slope limit
//! and stepped onto within STEP_HEIGHT; walls push three stacked spheres out horizontally, so
//! bodies stop and slide. Ported from sekiro-rs `crates/sim/src/collision.rs`. Every triangle
//! carries its HitMtrlParam id (footsteps, fall death). Without one the floor is flat at y = 0.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use bevy::prelude::*;

use crate::actor::{Actor, Side};
use crate::player::CAPSULE_HALF_HEIGHT;

/// Body radius for wall contacts: Wolf's map-hit capsule, 1.5 m high, radius 0.4 (exe
/// FUN_140bc3140, hard-coded for the player; NPCs read NpcParam hitHeight / hitRadius in
/// FUN_140bc2e00). Live physics module +0xD0 = 1.5 / 0.4 agrees.
pub const RADIUS: f32 = 0.4;
/// Highest ledge a walking body steps onto (Wolf follows the floor down by
/// player::FALL_START_DROP). gap: the character proxy's step-up is not traced (Havok sweep, no explicit value found).
pub const STEP_HEIGHT: f32 = 0.5;
/// hknpCharacterProxyCinfo maxSlope, set to 60 deg in FUN_14104d3d0 (0x3f860a92 = 1.0472 rad).
const MAX_SLOPE_DEGREES: f32 = 60.0;
/// Heights of the contact spheres' centres above the feet; the lowest sphere's bottom sits at
/// the step height so stairs reach the floor query rather than the wall push.
const SPHERES: [f32; 3] = [STEP_HEIGHT + RADIUS, 1.2, 1.55];
const CELL: f32 = 4.0;

/// A whole play world's collision the host brings (sekiro-rs's `CollisionWorld`, sr_map.rs):
/// answers the floor, ray and wall queries in place of the terrain's own triangles.
pub trait WorldCollision: Send + Sync {
    /// The highest walkable surface under (x, z) no higher than `top`: (height, triangle).
    fn floor_hit(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)>;
    /// HitMtrlParam id of that surface.
    fn floor_material(&self, x: f32, z: f32, top: f32) -> Option<u32>;
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32>;
    /// A body at `feet` moved by the horizontal `delta`, stopped and slid by walls: the move made.
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2];
    fn triangle_count(&self) -> usize;
    /// The highest water plane under (x, z) no higher than `top`: (height, HitMtrlParam id).
    fn water_surface(&self, _x: f32, _z: f32, _top: f32) -> Option<(f32, u32)> {
        None
    }
    /// Whether a body's feet moving `from` -> `to` pass through an event wall that starts the
    /// fall return (the map's hit filter 15 collision over drops into the void).
    fn fall_return_wall(&self, _from: Vec3, _to: Vec3) -> bool {
        false
    }
    /// [`WorldCollision::slide`] that also keeps to the fall-prevention walls `protect` asks for
    /// (Actor::fall_protect bits).
    fn slide_protected(&self, feet: [f32; 3], delta: [f32; 2], _protect: u8) -> [f32; 2] {
        self.slide(feet, delta)
    }
    /// The camera's ray: the world's camera collision (its own hit filters: camera-only walls,
    /// no player-only ones), else the body's.
    fn camera_raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.raycast(origin, dir, max)
    }
    /// The ground foot IK casts onto (anim.rs `foot_ik`): the game casts the feet on the detailed
    /// `h` collision while the body stands on the flat `l` one. The floor by default.
    fn foot_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_hit(x, z, top).map(|(h, _)| h)
    }
    /// The highest the feet of a body at `feet` can rise straight up, by at most `rise`, before
    /// its head meets a ceiling (`None`: nothing stops it that soon). No ceilings by default.
    fn ceiling(&self, _feet: [f32; 3], _rise: f32) -> Option<f32> {
        None
    }
    /// The floor as the enemies stand on it: the world's enemy collision (enemy-only floors and
    /// water surfaces), else the body's.
    fn enemy_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_hit(x, z, top).map(|h| h.0)
    }
    /// The ground an enemy's foot IK casts onto: [`WorldCollision::foot_floor`]'s detailed ground
    /// where the world keeps it (around Wolf; an enemy's feet never move where that is kept),
    /// else the enemy floor.
    fn enemy_foot_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.enemy_floor(x, z, top)
    }
    /// A slide as the enemies take it: stopped by the enemy-only walls too (the ones that keep
    /// them off cliffs and in their areas), else the body's.
    fn enemy_slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.slide(feet, delta)
    }
}

/// HitMtrlParam's water planes (Paramdex names): 20 puddle, 21 walkable, 22 dive, 23 / 24 poison
/// swamp, 35 walkable slope, 36 swim slope, 37 / 38 water and grass (walkable / swimming).
pub const fn water_material(m: u32) -> bool {
    matches!(m, 20..=24 | 35..=38)
}

/// The deep ones (22 / 36 / 38: fallDeathDist 80, fallDamageRate 0.5): a surface Wolf swims in,
/// never a floor. gap: the exe's swim trigger is not traced (the native BEH_R_STAND_MOVE_TO_SWIM
/// request); these rows by their names and fall values.
pub const fn deep_water(m: u32) -> bool {
    matches!(m, 22 | 36 | 38)
}

/// The highest water plane under (x, z) no higher than `top` (height, material), from the map's
/// own hit triangles or the host's world.
pub fn water_surface(x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
    terrain()?.water(x, z, top)
}

/// The hit collision, shared with the systems that cannot take a resource (player.rs floor
/// checks, the host's camera arm clamp and floor sounds) through [`terrain`].
pub struct Terrain {
    pub(crate) verts: Vec<Vec3>,
    pub(crate) tris: Vec<[u32; 3]>,
    pub(crate) normals: Vec<Vec3>,
    pub(crate) materials: Vec<u32>,
    pub(crate) cells: HashMap<(i32, i32), Vec<u32>>,
    pub(crate) min_walk_ny: f32,
    /// Level triangles without a hit material dropped at load (see `from_tris`).
    pub(crate) helper_planes: usize,
    /// Under sekiro-rs's map: its whole play world's collision (sr_map.rs `collision`), which
    /// answers the floor, ray and wall queries in place of the triangles above (left empty).
    world: Option<Arc<dyn WorldCollision>>,
}

static TERRAIN: OnceLock<RwLock<Option<Arc<Terrain>>>> = OnceLock::new();

/// The fog-wall collision in force: the terrain it belongs to (held, so its address stays
/// unique) and the gates standing on it (the host's map: map.rs `set_gates`).
static GATES_ON: OnceLock<RwLock<(Option<Arc<Terrain>>, Vec<Arc<Terrain>>)>> = OnceLock::new();

/// Switches the fog-wall collision on: `on` stands on `base` while their fight is on (an empty
/// `on` / no `base`: none).
pub(crate) fn set_gates_on(base: Option<Arc<Terrain>>, on: Vec<Arc<Terrain>>) {
    *GATES_ON.get_or_init(Default::default).write().unwrap() = (base, on);
}

pub fn terrain() -> Option<Arc<Terrain>> {
    TERRAIN.get_or_init(|| RwLock::new(None)).read().ok()?.clone()
}

/// Floor height under an actor centre (its feet are CAPSULE_HALF_HEIGHT lower): the highest
/// walkable surface at most STEP_HEIGHT above the feet, else the one below; 0 without a map.
/// How far from the centre the capsule's rim rests on a ledge (sekiro-rs sim body.rs
/// RIM_SUPPORT: the radius x 0.866).
const RIM_SUPPORT: f32 = RADIUS * 0.866;

/// The floor Wolf's capsule rim still rests on when the floor under his centre has dropped away
/// (sim body.rs `rim_support`): the highest under eight points round the feet that is no higher
/// than them and within the walk-down (`FALL_START_DROP`); he stands on a ledge's edge as the
/// game's capsule does, instead of dropping once his centre is past it.
pub fn rim_floor(centre: Vec3) -> Option<f32> {
    let t = terrain()?;
    let feet = centre.y - CAPSULE_HALF_HEIGHT;
    (0..8)
        .filter_map(|i| {
            let (s, c) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
            t.floor(centre.x + RIM_SUPPORT * c, centre.z + RIM_SUPPORT * s, feet + 0.05)
        })
        .filter(|h| feet - h < crate::player::FALL_START_DROP)
        .reduce(f32::max)
}

pub fn ground_y(centre: Vec3) -> f32 {
    let Some(t) = terrain() else { return 0.0 };
    let feet = centre.y - CAPSULE_HALF_HEIGHT;
    t.floor(centre.x, centre.z, feet + STEP_HEIGHT).or_else(|| t.floor(centre.x, centre.z, f32::INFINITY)).unwrap_or(feet - 100.0)
}

/// Whether an actor centre moving `from` -> `to` passes a fall-return event wall (the host's
/// world only; the own map has none).
pub fn fall_return_wall(from: Vec3, to: Vec3) -> bool {
    let feet = Vec3::Y * CAPSULE_HALF_HEIGHT;
    terrain().and_then(|t| t.world.clone()).is_some_and(|w| w.fall_return_wall(from - feet, to - feet))
}

/// HitMtrlParam id of the floor under an actor centre (None without a map or over nothing).
pub fn floor_material(centre: Vec3) -> Option<u32> {
    let t = terrain()?;
    let top = centre.y - CAPSULE_HALF_HEIGHT + STEP_HEIGHT;
    if let Some(w) = &t.world {
        return w.floor_material(centre.x, centre.z, top);
    }
    t.floor_tri(centre.x, centre.z, top).map(|(_, tri)| t.materials[tri as usize])
}

/// HitMtrlParam fallDeathDist / disableFallDamage / fallDamageRate per material id
/// (extracted/json/params).
fn hit_mtrl_fall() -> &'static HashMap<u32, (f32, bool, f32)> {
    static T: std::sync::OnceLock<HashMap<u32, (f32, bool, f32)>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/HitMtrlParam.json");
        let json: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let rows = json["rows"].as_array().cloned().unwrap_or_default();
        rows.iter().filter_map(|r| Some((r["id"].as_u64()? as u32, (r["fallDeathDist"].as_f64()? as f32, r["disableFallDamage"].as_u64().unwrap_or(0) != 0, r["fallDamageRate"].as_f64().unwrap_or(1.0) as f32)))).collect()
    })
}

/// How far (m) Wolf may fall onto this ground before the landing kills him, None where fall
/// damage is off. Exe: FUN_14084e5d0 (the kill test) is `fall height >= HitMtrlParam +0x4c
/// (fallDeathDist)`, and only from 20 m up (DAT_143b01948 = 20.0 in the exe's data; 5 m while
/// SpEffect type 0x155 is worn, DAT_143b01950/54). The row is the material under the landing
/// (FUN_1408433f0 reads it from the fall module's material id); no ground = the table's default
/// 40 (gap: the exe's void kill height is not traced). The height is metres (the same 20 as the
/// long stiff landing, c0000_define 213).
pub fn fall_death_dist(material: Option<u32>) -> Option<f32> {
    match material.and_then(|m| hit_mtrl_fall().get(&m)) {
        Some((_, true, _)) => None,
        Some((dist, false, _)) => Some(*dist),
        None => Some(40.0),
    }
}

/// The share of max HP a landing that does not kill costs (exe FUN_140843660, landing
/// FUN_140b97b30: damage = maxHP x this): 0 under `from` (20 m; the caller passes 5 m while
/// stateInfo 341, aging, is worn: DAT_143b01950/54), else CalcCorrectGraph 210 (the identity
/// from 20 to 100) x 0.01 x graphs 211 / 212 (a flat 100 %) x HitMtrlParam fallDamageRate (0.5
/// on the deep-water planes 22 / 36 / 38). 0 on a disableFallDamage floor; the caller zeroes it
/// under stateInfo 47 (SpEffects 640 / 4050 / 4053, FUN_140ba8cb0). gap: the factors
/// FUN_1408433f0 feeds the graphs (flat here, so they do not matter) are not decoded.
pub fn fall_damage_share(h: f32, material: Option<u32>, from: f32) -> f32 {
    let (off, rate) = match material.and_then(|m| hit_mtrl_fall().get(&m)) {
        Some((_, off, rate)) => (*off, *rate),
        None => (false, 1.0),
    };
    if off || h < from {
        return 0.0;
    }
    (h.clamp(20.0, 100.0) * 0.01 * rate).min(1.0)
}

/// Distance along `dir` (unit) to the first collision triangle within `max`.
pub fn raycast(origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    terrain()?.raycast(origin, dir, max)
}

/// The camera arm's ray ([`WorldCollision::camera_raycast`] under the host's world).
pub fn camera_raycast(origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
    let t = terrain()?;
    match &t.world {
        Some(w) => w.camera_raycast(origin, dir, max),
        None => t.raycast(origin, dir, max),
    }
}

/// Where a body whose centre rose from `from_y` to `to.y` this step stops under a ceiling: the
/// centre height its head meets one at (the host's world only).
pub fn ceiling_stop(to: Vec3, from_y: f32) -> Option<f32> {
    let rise = to.y - from_y;
    if rise <= 0.0 {
        return None;
    }
    let w = terrain()?.world.clone()?;
    w.ceiling([to.x, from_y - CAPSULE_HALF_HEIGHT, to.z], rise).map(|top| top.max(from_y - CAPSULE_HALF_HEIGHT) + CAPSULE_HALF_HEIGHT)
}

fn cell_of(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

impl Terrain {
    /// Collision from triangles already in Bevy space (`(indices, material)` each): the hit
    /// files and the sandbox map's generated floor and walls.
    pub(crate) fn from_tris(verts: Vec<Vec3>, tris: impl Iterator<Item = ([u32; 3], u32)>) -> Self {
        let nv = verts.len();
        let mut t = Terrain { verts, tris: Vec::new(), normals: Vec::new(), materials: Vec::new(), cells: HashMap::new(), min_walk_ny: MAX_SLOPE_DEGREES.to_radians().cos(), helper_planes: 0, world: None };
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

    /// A whole play world's collision as the terrain (sekiro-rs's: sr_map.rs `collision`).
    pub fn from_world(world: Arc<dyn WorldCollision>) -> Self {
        let mut t = Self::from_tris(Vec::new(), std::iter::empty());
        t.world = Some(world);
        t
    }

    /// No terrain (the map switched to one without a `.hit`, or off).
    pub(crate) fn clear() {
        if let Ok(mut t) = TERRAIN.get_or_init(|| RwLock::new(None)).write() {
            *t = None;
        }
    }

    pub fn install(self) {
        *TERRAIN.get_or_init(|| RwLock::new(None)).write().unwrap() = Some(Arc::new(self));
    }

    pub fn triangle_count(&self) -> usize {
        self.world.as_ref().map_or(self.tris.len(), |w| w.triangle_count())
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
        if let Some(w) = &self.world {
            return w.floor_hit(x, z, top);
        }
        const R: f32 = 0.08;
        // (the highest dry floor, the highest deep-water plane)
        let (mut best, mut wet): (Option<(f32, u32)>, Option<(f32, u32)>) = (None, None);
        for (dx, dz) in [(0.0, 0.0), (R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R)] {
            let (px, pz) = (x + dx, z + dz);
            let Some(cell) = self.cells.get(&cell_of(px, pz)) else { continue };
            for &t in cell {
                if self.normals[t as usize].y.abs() < self.min_walk_ny {
                    continue;
                }
                let [a, b, c] = self.corners(t);
                if let Some(h) = height_on(px, pz, a, b, c) {
                    let slot = if deep_water(self.materials[t as usize]) { &mut wet } else { &mut best };
                    if h <= top && slot.is_none_or(|(bh, _)| h > bh) {
                        *slot = Some((h, t));
                    }
                }
            }
        }
        // Deep water over a bed is a surface to swim in, not a floor (swim.rs, `water_surface`);
        // with nothing under it the plane is the floor (the Yasha Ape's pool, m11 51000000).
        best.or(wet)
    }

    /// [`Terrain::floor`] as the enemies stand on it ([`WorldCollision::enemy_floor`]).
    pub fn enemy_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        match &self.world {
            Some(w) => w.enemy_floor(x, z, top),
            None => self.floor(x, z, top),
        }
    }

    pub fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_tri(x, z, top).map(|(h, _)| h)
    }

    /// [`WorldCollision::foot_floor`] on this collision.
    pub fn foot_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        match &self.world {
            Some(w) => w.foot_floor(x, z, top),
            None => self.floor(x, z, top),
        }
    }

    /// [`WorldCollision::enemy_foot_floor`] on this collision.
    pub fn enemy_foot_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        match &self.world {
            Some(w) => w.enemy_foot_floor(x, z, top),
            None => self.floor(x, z, top),
        }
    }

    /// `water_surface` on this collision.
    pub fn water(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        if let Some(w) = &self.world {
            return w.water_surface(x, z, top);
        }
        self.cells
            .get(&cell_of(x, z))?
            .iter()
            .filter(|&&tri| water_material(self.materials[tri as usize]))
            .filter_map(|&tri| {
                let [a, b, c] = self.corners(tri);
                height_on(x, z, a, b, c).filter(|h| *h <= top).map(|h| (h, self.materials[tri as usize]))
            })
            .max_by(|p, q| p.0.total_cmp(&q.0))
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        if let Some(w) = &self.world {
            return w.raycast(origin, dir, max);
        }
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
        self.slide_protected(feet, delta, 0)
    }

    /// [`Terrain::slide`] keeping to the fall-prevention walls `protect` asks for (the host's
    /// world has them; the own map's collision does not).
    pub fn slide_protected(&self, feet: Vec3, delta: Vec2, protect: u8) -> Vec2 {
        self.slide_as(feet, delta, protect, false)
    }

    /// The slide for a body of either side: an enemy's keeps to the world's enemy-only walls
    /// ([`WorldCollision::enemy_slide`]), Wolf's to the fall-prevention walls `protect` asks for.
    pub fn slide_as(&self, feet: Vec3, delta: Vec2, protect: u8, enemy: bool) -> Vec2 {
        let len = delta.length();
        if len < 1e-6 {
            return Vec2::ZERO;
        }
        // Sub-steps of at most half the radius so a thin post cannot be stepped over (a tick's
        // move is centimetres; the cap only bounds a wild teleport).
        let steps = ((len / (RADIUS * 0.5)).ceil() as usize).clamp(1, 512);
        let gates = self.gates_on();
        let gate_tris: Vec<Vec<u32>> = gates.iter().map(|g| (0..g.tris.len() as u32).collect()).collect();
        // sekiro-rs's world slides the body; the fog walls on it push it back out after.
        if let Some(w) = &self.world {
            let moved = Vec2::from(if enemy { w.enemy_slide(feet.to_array(), delta.to_array()) } else { w.slide_protected(feet.to_array(), delta.to_array(), protect) });
            let mut p = feet + Vec3::new(moved.x, 0.0, moved.y);
            for _ in 0..4 {
                let push = gates.iter().zip(&gate_tris).map(|(g, t)| g.push_out(p, t)).max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec2::ZERO);
                if push == Vec2::ZERO {
                    break;
                }
                p.x += push.x;
                p.z += push.y;
            }
            return Vec2::new(p.x - feet.x, p.z - feet.z);
        }
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

    /// The fog-wall collision switched on for this terrain ([`set_gates_on`]); none for any other
    /// terrain (a test's, or a map loaded since).
    fn gates_on(&self) -> Vec<Arc<Terrain>> {
        GATES_ON.get().and_then(|g| g.read().ok()).filter(|g| g.0.as_ref().is_some_and(|t| std::ptr::eq(Arc::as_ptr(t), self))).map(|g| g.1.clone()).unwrap_or_default()
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

/// The longest step one fixed tick can take as a walk (2 m at 60 Hz is 120 m/s, past any dash
/// or grapple pull); a longer one is a teleport and skips the wall slide.
pub const TELEPORT: f32 = 2.0;

/// Where the actor was before this tick's movement, for the wall slide.
#[derive(Component)]
pub struct OnTerrain {
    pub(crate) prev: Vec3,
}

/// After the actors moved: push them out of walls along their step, then put grounded ones on
/// the floor (following it up to STEP_HEIGHT up and FALL_START_DROP down; a bigger drop leaves them in the air,
/// which player.rs turns into FreeFall; enemies stay put, they have no fall of their own).
pub(crate) fn bodies_on_terrain(combat: Res<crate::data::Combat>, mut commands: Commands, mut q: Query<(Entity, &Actor, &mut Transform, Option<&mut OnTerrain>)>) {
    let _prof = crate::perf::span("bodies_on_terrain");
    let Some(t) = terrain() else { return };
    for (e, a, mut tf, state) in &mut q {
        let Some(mut state) = state else {
            commands.entity(e).insert(OnTerrain { prev: tf.translation });
            continue;
        };
        // Not due this tick (Actor::lod): the next due tick takes the whole move.
        if !a.lod_due {
            continue;
        }
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
            let _p = crate::perf::span("terrain_slide");
            let moved = t.slide_as(feet_prev, delta, a.fall_protect, a.side == Side::Enemy);
            tf.translation.x = prev.x + moved.x;
            tf.translation.z = prev.z + moved.y;
        }
        let data = crate::actor::data_for(&combat, &a);
        let lifted = !a.anim.is_empty() && data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY);
        // An enemy coming down from a SetNoGravity lift (actor.rs) lands by itself.
        if !a.airborne && !lifted && a.lift_ground.is_none() {
            let feet = tf.translation.y - CAPSULE_HALF_HEIGHT;
            let pf = crate::perf::span("terrain_floor");
            let floor = if a.side == Side::Enemy { t.enemy_floor(tf.translation.x, tf.translation.z, feet + STEP_HEIGHT) } else { t.floor(tf.translation.x, tf.translation.z, feet + STEP_HEIGHT) };
            drop(pf);
            // Wolf follows the floor down only FALL_START_DROP (HKS BEH_R_FALL: a smaller drop
            // never leaves the move state, so following it is the same); a deeper one falls.
            let down = if a.side == Side::Enemy { f32::INFINITY } else { crate::player::FALL_START_DROP };
            // Wolf over a drop his rim still rests on: held at its edge (`rim_floor`).
            let floor = match floor {
                Some(h) if h >= feet - down => Some(h),
                _ if a.side != Side::Enemy => rim_floor(tf.translation).or(floor),
                _ => floor,
            };
            match floor {
                Some(h) if h >= feet - down => tf.translation.y = h + CAPSULE_HALF_HEIGHT,
                Some(_) => {}
                None => {
                    // Over nothing walkable within reach: the enemy keeps its height.
                }
            }
        }
        state.prev = tf.translation;
    }
}

#[cfg(test)]
mod fall_tests {
    use super::*;

    /// Atlas movement.fall-damage: exe FUN_140843660 - nothing under 20 m, then h / 100 of max HP
    /// (CalcCorrectGraph 210), all of it from 100 m; half on the deep-water planes (HitMtrlParam
    /// 22 fallDamageRate 0.5); none on a disableFallDamage row; from 5 m when aged (the caller's
    /// `from`, graph 210 floors at 20 %).
    #[test]
    fn fall_damage_is_a_hundredth_of_max_hp_per_metre_from_twenty() {
        assert_eq!(fall_damage_share(19.9, None, 20.0), 0.0);
        assert!((fall_damage_share(25.0, None, 20.0) - 0.25).abs() < 1e-6);
        assert!((fall_damage_share(150.0, None, 20.0) - 1.0).abs() < 1e-6);
        assert!((fall_damage_share(30.0, Some(22), 20.0) - 0.15).abs() < 1e-6);
        assert!((fall_damage_share(6.0, None, 5.0) - 0.2).abs() < 1e-6);
        let off = hit_mtrl_fall().iter().find(|(_, r)| r.1).map(|(id, _)| *id).expect("a disableFallDamage row");
        assert_eq!(fall_damage_share(30.0, Some(off), 20.0), 0.0);
    }

    /// Atlas movement.swim-surface: a deep water plane (HitMtrlParam 22) 3 m over a lakebed is no
    /// floor (Wolf sinks through it to swim) but is the water's surface; a walkable one (21) is
    /// a floor.
    #[test]
    fn a_deep_water_plane_is_a_surface_not_a_floor() {
        let quad = |y: f32| [Vec3::new(-5.0, y, -5.0), Vec3::new(5.0, y, -5.0), Vec3::new(5.0, y, 5.0), Vec3::new(-5.0, y, 5.0)];
        for (m, floor) in [(22u32, 0.0f32), (21, 3.0)] {
            let mut verts = quad(0.0).to_vec();
            verts.extend(quad(3.0));
            let t = Terrain::from_tris(verts, [([0, 2, 1], 45), ([0, 3, 2], 45), ([4, 6, 5], m), ([4, 7, 6], m)].into_iter());
            assert_eq!(t.floor(0.0, 0.0, 10.0), Some(floor), "material {m}");
            assert_eq!(t.water(0.0, 0.0, 10.0), Some((3.0, m)));
            assert_eq!(t.water(0.0, 0.0, 2.0), None, "under the feet only");
        }
        // With no bed under it the plane is the floor.
        let t = Terrain::from_tris(quad(3.0).to_vec(), [([0, 2, 1], 22), ([0, 3, 2], 22)].into_iter());
        assert_eq!(t.floor(0.0, 0.0, 10.0), Some(3.0));
    }
}
