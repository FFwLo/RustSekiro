//! Map collision for character bodies: the hit collision exported by `sekiro-extract map`
//! (`cache/maps/<id>/collision.bin`, a triangle soup in Bevy space) behind a [`Ground`].
//!
//! Triangles are bucketed in a uniform grid over the horizontal plane (2 m cells, the
//! floors apart), and each cell by height in 2 m bands, which suits the queries a walking
//! character makes: what is under my feet, and what do I bump into. Each query reads only the
//! bands it reaches of the cells it crosses (#163).
//!
//! - Floors: [`CollisionWorld::floor`] returns the highest walkable surface (normal within
//!   [`MAX_SLOPE_DEGREES`] of up) under a point, no higher than a given top. Bodies ask for
//!   the floor up to [`STEP_HEIGHT`] above their feet, so stairs and small ledges are stepped
//!   onto, and follow the floor down by the same amount before they start falling.
//! - Walls: the body is two spheres stacked above the step zone (radius [`RADIUS`]).
//!   [`CollisionWorld::slide`] moves them in sub-steps and pushes them out of every triangle
//!   horizontally, so the body stops at walls and slides along them, and steep slopes act as
//!   walls. Vertical pushes are left to the floor query.
//! - [`CollisionWorld::raycast`] serves cameras.
//!
//! The collision mesh does not say which side of a triangle is solid (its winding is not
//! consistent), so floors use `|normal.y|` and pushes go away from the closest point.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::{Vec2, Vec3};
use sekiro_formats::hknp::{CollisionMesh, ObjectCollision, ObjectCollisionPart, PartCollision};

use crate::body::{Ground, WaterSurface};
use crate::hit_filter::{CAMERA, ENEMY, PLAYER};

#[path = "collision_grid.rs"]
mod grid;
use grid::{ALL, BAND, Column, FIRST, Pending, band_of, bands_of, water_material};

/// Body radius in metres: the player's physics capsule is 1.5 m tall with a 0.4 m radius, read
/// from the running game by FwLo's RustSekiro (`docs/kb/movement.md`).
pub const RADIUS: f32 = 0.4;

/// Highest ledge a walking body steps onto, and the furthest it follows the floor down.
pub const STEP_HEIGHT: f32 = 0.5;
/// Steepest walkable surface.
/// Havok character proxy default, FUN_14104d3d0 (FwLo's RustSekiro, map.rs).
pub const MAX_SLOPE_DEGREES: f32 = 60.0;
/// The player's physics capsule height (see [`RADIUS`]).
pub const CAPSULE_HEIGHT: f32 = 1.5;
/// The top of the body's top collision sphere above the feet, the capsule's top: where its
/// head meets a ceiling.
pub const HEAD_TOP: f32 = CAPSULE_HEIGHT;
/// Heights of the collision spheres' centres above the feet. The lowest sphere's bottom sits
/// at the step height, so stairs reach the floor query rather than the wall push; the top
/// sphere's top is the capsule's, so Wolf passes under anything over 1.5 m, as in the game:
/// the broken-wall gap past the Outskirts Wall - Gate Path idol has planks down to 1.77 m
/// above the floor (#146), which a top sphere reaching 1.95 m ran into.
///
/// Against the exe (row E8, 2026-11-04, src/sim_tests/emu_srcollide.rs): Wolf's proxy moves
/// lifted by physics `+0x188` (0.3 m), so his capsule spans 0.3-1.8 m over the feet while it
/// moves (docs/kb/movement.md "Character proxy against the exe"); the top sphere ends at 1.8 m
/// as in RSekiro's own terrain (src/terrain.rs), the middle one closes the gap between them.
const SPHERES: [f32; 3] = [0.3 + RADIUS, 1.05, 1.8 - RADIUS];
/// How far past its radius a body stays off a wall: hknpCharacterProxy keepDistance, 0.05 in
/// Wolf's proxy (the float after its shape; docs/kb/movement.md). Without it the body stopped
/// 0.05 m nearer the arena's rock face than the exe (row E8).
pub const KEEP_DISTANCE: f32 = 0.05;
/// The convex radius of the map's compressed mesh shapes: 0.01 m on 180 of the 184 shapes of
/// m11_00's `l` binder (and 169 of 173 of `h`; the other four 0), read from the hkx
/// `fsnpCustomParamCompressedMeshShape` `convexRadius` (row E8, 2026-11-04). The exe's bodies
/// meet the surface pushed out by it: Wolf stands 0.010-0.012 m above the exported triangles on
/// flat ground (the exe's own 0.05 m casts land 0.01 m above them too). Placed objects carry
/// their own shapes (polytopes already pushed out by theirs at export), so only the map's
/// triangles take it ([`CollisionWorld::skin`]).
pub const MESH_CONVEX_RADIUS: f32 = 0.01;
/// The furthest a wall pushes a body sphere's centre: [`RADIUS`], the keep distance and the
/// mesh's convex radius.
const WALL_REACH: f32 = RADIUS + KEEP_DISTANCE + MESH_CONVEX_RADIUS;
/// cos [`MAX_SLOPE_DEGREES`]: the least upward part of a contact's way out for it to support.
const SUPPORT_UP: f32 = 0.5;
/// How close the fall-prevention assist lets a body come to its walls: `FallAssistMinRadius`,
/// physics module `+0xaec`, 0.16 m from the constructor `FUN_140bbb010` (`+0xae8`,
/// `FallAssistMaxRadius`, is 0.4). `FUN_140bc68d0` slides the body's move along the walls its
/// sphere cast meets and cuts it where the body would come nearer than this to one.
pub const FALL_ASSIST_RADIUS: f32 = 0.16;
/// The side of a grid cell ([`CollisionWorld::columns`]). Smaller cells cost more cell entries
/// for the triangles crossing them; 4 m cells made the slides of the #156 deathblow's pair
/// search read four times the triangles.
const CELL: f32 = 2.0;
/// How far inside the head sphere's rim, seen from above, a ceiling must reach to stop a rise
/// ([`CollisionWorld::ceiling_for`]). The slide leaves a body pressed against a wall at
/// [`RADIUS`] from it, give or take roundoff, so the wall's top edge passes a hair inside the
/// rim; skipping such a face lets the sphere overlap it by at most this.
const CEILING_GRAZE: f32 = 1e-3;
/// The least upward part of the direction from the head sphere's centre to where it first meets
/// a face for that face to stop a rise ([`CollisionWorld::ceiling_for`]): the cosine of 39
/// degrees, so the touch is within 0.25 m of the centre line seen from above at [`RADIUS`]
/// 0.4. Further out the face only meets the sphere's shoulder, the wall slide pushes it off,
/// and stopping there blocked most jumps up m11_00's rock walls under their ledge lips; the gate
/// fence's cap beam (#170) meets it 0.23 m out. 0.25 m is where the sampled test before #170
/// looked, which those walls were climbed with.
const CEILING_UP: f32 = 0.78;
/// `CollisionWorld::part_of` for a triangle in no helper or event part.
const ORDINARY: u16 = 0;
/// Who collides with an ordinary triangle: every body and the camera, not the fall-prevention
/// assist.
const BODIES: u8 = PLAYER | ENEMY | CAMERA;
/// `CollisionWorld::part_of` bit marking an event part's triangle.
const EVENT_PART: u16 = 0x8000;

pub struct CollisionWorld {
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    normals: Vec<Vec3>,
    /// Hit material per triangle (`u32::MAX` when unknown).
    materials: Vec<u32>,
    /// The grid ([`grid`]): triangles by [`CELL`] square over the horizontal plane, the floors
    /// apart, then by who collides with them, then by height band.
    columns: HashMap<(i32, i32), Column>,
    min_walk_ny: f32,
    /// Triangles from this index on form the gate: collision that exists only while a map event
    /// has it enabled (a boss fog wall and its invisible boundary). `u32::MAX` when there is none.
    gate_from: u32,
    gate_open: AtomicBool,
    /// Map event collision switched by entity: triangle range, who collides with it (as in
    /// `masked`) and whether it is on.
    event_parts: Vec<(i32, std::ops::Range<u32>, u8, AtomicBool)>,
    /// Helper parts (see [`crate::hit_filter`]): who collides with each
    /// ([`crate::hit_filter::PLAYER`], `ENEMY`, `CAMERA` bits). Every other triangle is for all.
    masked: Vec<u8>,
    /// Per entry of `masked`, whether that part is a swim hit, whose deep-water triangles are
    /// water planes ([`water_plane`]).
    swim_parts: Vec<bool>,
    /// Per triangle, the part [`CollisionWorld::live`] asks about: [`ORDINARY`], `1 + i` for
    /// `masked[i]` or [`EVENT_PART`]` | i` for `event_parts[i]`.
    part_of: Vec<u16>,
    /// Per triangle, the corner-wise minimum and maximum of its corners: its bounding box.
    bounds: Vec<[Vec3; 2]>,
    /// Map action event walls: untouchable triangles whose contact runs a reaction.
    triggers: Vec<TriggerPart>,
    /// Placed objects' triangle ranges ([`CollisionWorld::add_objects`]) and `<block>/<part>`
    /// names, in triangle order.
    objects: Vec<(std::ops::Range<u32>, String)>,
    /// Per triangle, whether it is an object's (no [`MESH_CONVEX_RADIUS`]); shorter than the
    /// triangles when the last ones are not.
    bare: Vec<bool>,
}

/// One event wall part, kept out of the walkable grid.
struct TriggerPart {
    hit_filter: u8,
    part: String,
    min: Vec3,
    max: Vec3,
    triangles: Vec<[Vec3; 3]>,
}

/// A body's contact with an event wall ([`CollisionWorld::trigger_contact`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerContact {
    pub hit_filter: u8,
    pub part: String,
    /// Where the body's path met the wall.
    pub point: Vec3,
}

/// A triangle's unit normal, `None` for one too thin to collide with (left out of the grid).
fn face_normal(a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
    let n = (b - a).cross(c - a);
    (n.length_squared() >= 1e-12).then(|| n.normalize())
}

/// Copies one part of a mesh of parts (an object's or fall-assist walls'): `source`'s triangles
/// `first..first + count` into `mesh`, each source vertex once (`slot` maps a source vertex to
/// its index in `mesh`, `u32::MAX` for none yet). Triangles too thin to collide with are left
/// out here as the grid would, so a caller's ranges count what it keeps. A part or vertex
/// outside `source` is an error, not a panic or a silent gap.
fn copy_part(
    source: &CollisionMesh,
    first: u32,
    count: u32,
    slot: &mut [u32],
    mesh: &mut CollisionMesh,
) -> Result<(), String> {
    let (first, count) = (first as usize, count as usize);
    let tris = first
        .checked_add(count)
        .and_then(|end| source.indices.get(first * 3..end * 3))
        .ok_or_else(|| {
            format!(
                "triangles {first}..{} outside the mesh's {}",
                first.saturating_add(count),
                source.indices.len() / 3
            )
        })?;
    for (t, tri) in (first..).zip(tris.as_chunks::<3>().0) {
        let corners = tri.map(|i| source.vertices.get(i as usize));
        let [Some(a), Some(b), Some(c)] = corners else {
            return Err(format!(
                "triangle {t}'s vertices {tri:?} outside the mesh's {}",
                source.vertices.len()
            ));
        };
        if face_normal(Vec3::from(*a), Vec3::from(*b), Vec3::from(*c)).is_none() {
            continue;
        }
        for &i in tri {
            let i = i as usize;
            if slot[i] == u32::MAX {
                slot[i] = mesh.vertices.len() as u32;
                mesh.vertices.push(source.vertices[i]);
            }
            mesh.indices.push(slot[i]);
        }
        mesh.materials
            .push(source.materials.get(t).copied().unwrap_or(u32::MAX));
    }
    Ok(())
}

fn cell_of(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

/// The cells the grid files a triangle whose corners' box is `lo` to `hi` in: each its box
/// reaches, padded by [`grid::slack`] for a floor ([`Column::floors`]) so a floor query reads
/// every floor that can be under its point in the point's cell.
fn filed_cells(lo: Vec3, hi: Vec3, floor: bool) -> impl Iterator<Item = (i32, i32)> {
    let pad = if floor {
        grid::slack(
            (hi - lo).max_element(),
            lo.abs().max(hi.abs()).max_element(),
        )
    } else {
        0.0
    };
    let (x0, z0) = cell_of(lo.x - pad, lo.z - pad);
    let (x1, z1) = cell_of(hi.x + pad, hi.z + pad);
    (x0..=x1).flat_map(move |cx| (z0..=z1).map(move |cz| (cx, cz)))
}

/// Whether a triangle with hit material `material` is a water plane bodies swim at rather than a
/// floor or wall: deep water ([`crate::body::deep_water`]) in a swim hit (`swim`,
/// [`crate::hit_filter::SWIM_FILTER`]). Deep water anywhere else is ground: the capsule meets
/// group 39 whatever the material (#189).
fn water_plane(material: u32, swim: bool) -> bool {
    swim && crate::body::deep_water(material)
}

/// Whether the grid files a triangle with normal `n` and hit material `material`, in a swim hit
/// when `swim`, with the floors: as [`CollisionWorld::walkable`] asks, less whether it is live.
fn files_as_floor(n: Vec3, material: u32, min_walk_ny: f32, swim: bool) -> bool {
    n.y.abs() >= min_walk_ny && !water_plane(material, swim)
}

impl CollisionWorld {
    /// The collision file at `path` exactly as it is, every triangle solid for every query:
    /// no helper masks and no swim hits. A map folder's `collision.bin` is the high-resolution
    /// `h` mesh characters never meet (Havok group 38, [`crate::lo_collision`]); load a map as
    /// they meet it with [`CollisionWorld::load_map`].
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mesh =
            CollisionMesh::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self::from_mesh(&mesh))
    }

    /// A map folder's collision as characters and the camera collide with it: the low-resolution
    /// `l` mesh ([`crate::lo_collision`], `collision.bin` when the folder has no
    /// `collision_lo.bin`), with the helper parts its `layout.json` names out of the walkable mesh
    /// and joined as helpers by hit filter ([`crate::hit_filter::strip_helpers`],
    /// [`CollisionWorld::add_helpers`]), and the fall-prevention assist walls.
    pub fn load_map(dir: &Path) -> Result<Self, String> {
        let path = dir.join("layout.json");
        let layout = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let layout =
            serde_json::from_slice(&layout).map_err(|e| format!("{}: {e}", path.display()))?;
        let block = crate::lo_collision::block_collision(dir, &layout, |_| false)?;
        let mut world = Self::from_mesh(&block.mesh);
        world.add_helpers(&block.helpers);
        world.add_map_fall_assist(dir, |_| true)?;
        Ok(world)
    }

    /// Adds the fall-prevention assist walls of the collision parts `keep` names
    /// (`fall_assist.bin`, a [`PartCollision`] of the map's `f` hit binder) as
    /// [`crate::hit_filter::FALL_ASSIST_ACTION`] walls. The exe gives the `f` body group 93,
    /// which only the ground actions' cast meets (`act(3016, 1)`: attacks, sprint start and
    /// stop, and every NPC's request), so the bands keep an attack or a sprint's start from
    /// carrying Wolf off an edge but let a walk or a run cross them, onto flat ground or over
    /// the edge; walking is held only by the filter-28 walls (docs/OBJECT-COLLISION.md, #203).
    /// Returns the triangles added.
    pub fn add_fall_assist(
        &mut self,
        walls: &PartCollision,
        keep: impl Fn(&str) -> bool,
    ) -> Result<usize, String> {
        let source = &walls.mesh;
        let mut mesh = CollisionMesh::default();
        // Only the kept parts' vertices, so a part left out leaves none behind.
        let mut slot = vec![u32::MAX; source.vertices.len()];
        for part in walls.parts.iter().filter(|p| keep(&p.name)) {
            copy_part(source, part.first, part.count, &mut slot, &mut mesh)
                .map_err(|e| format!("fall-assist part {}: {e}", part.name))?;
        }
        let range = self.add_masked(crate::hit_filter::FALL_ASSIST_ACTION, &mesh);
        Ok(range.len())
    }

    /// [`CollisionWorld::add_fall_assist`] with the `fall_assist.bin` in the map folder `dir`:
    /// the triangles added, or `None` when the folder has none (the map was exported without
    /// them: re-export it with `sekiro-extract map <id> --fall-assist`).
    pub fn add_map_fall_assist(
        &mut self,
        dir: &Path,
        keep: impl Fn(&str) -> bool,
    ) -> Result<Option<usize>, String> {
        let path = dir.join("fall_assist.bin");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let walls =
            PartCollision::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.add_fall_assist(&walls, keep).map(Some)
    }

    /// The collision of map blocks `blocks` as the game's play world builds it
    /// (`crates/game/src/map_world.rs` `collision_world`): each block's character mesh
    /// ([`crate::lo_collision::block_collision`]) with its helper parts stripped from the
    /// walkable mesh and added back by hit filter, the fall-prevention walls, the placed
    /// objects. Without the story's switches and the ride walls' masks. `None` when any block's
    /// `collision.bin` is missing from the cache `cache`. For tests.
    #[doc(hidden)]
    pub fn load_blocks(cache: &Path, blocks: &[&str]) -> Option<Self> {
        let dirs: Vec<_> = blocks.iter().map(|b| cache.join("maps").join(b)).collect();
        if !dirs.iter().all(|d| d.join("collision.bin").is_file()) {
            return None;
        }
        let read = |path: std::path::PathBuf| {
            std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        };
        let mut mesh = CollisionMesh::default();
        let mut helpers = crate::hit_filter::HelperCollision::default();
        for dir in &dirs {
            let layout: serde_json::Value =
                serde_json::from_slice(&read(dir.join("layout.json"))).unwrap();
            // Each block's character collision (`l`, [`crate::lo_collision`]) less its helpers.
            let part = crate::lo_collision::block_collision(dir, &layout, |_| false)
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            helpers.extend(part.helpers);
            mesh.extend(&part.mesh);
        }
        let mut world = Self::from_mesh(&mesh);
        world.add_helpers(&helpers);
        for (dir, block) in dirs.iter().zip(blocks) {
            world.add_map_fall_assist(dir, |_| true).unwrap();
            world.add_block_objects(dir, block, |_, who| who).unwrap();
        }
        Some(world)
    }

    pub fn from_mesh(mesh: &CollisionMesh) -> Self {
        Self::build(mesh, None, false)
    }

    /// [`CollisionWorld::from_mesh`] plus a gate: triangles that block only while
    /// [`CollisionWorld::set_gate`] has them on (off at first).
    pub fn from_mesh_with_gate(mesh: &CollisionMesh, gate: &CollisionMesh) -> Self {
        let mut all = mesh.clone();
        all.extend(gate);
        Self::build(&all, Some(mesh.triangle_count()), false)
    }

    /// Switches the gate's triangles on or off. They are shared by every holder of the world.
    pub fn set_gate(&self, on: bool) {
        self.gate_open.store(on, Ordering::Relaxed);
    }

    /// Whether the gate's triangles currently block.
    pub fn gate(&self) -> bool {
        self.gate_open.load(Ordering::Relaxed)
    }

    /// Whether triangle `t` takes part in `who`'s queries: every ordinary one (except for the
    /// fall-prevention assist's), a helper one when it holds `who`, an event one while on and
    /// holding `who`, a gate one while on.
    fn live(&self, t: u32, who: u8) -> bool {
        match self.part_of[t as usize] {
            ORDINARY => {
                who & BODIES != 0 && (t < self.gate_from || self.gate_open.load(Ordering::Relaxed))
            }
            part if part & EVENT_PART != 0 => {
                let (_, _, mask, on) = &self.event_parts[usize::from(part & !EVENT_PART)];
                mask & who != 0 && on.load(Ordering::Relaxed)
            }
            part => self.masked[usize::from(part) - 1] & who != 0,
        }
    }

    /// Whether triangle `t` is a wall or floor at all: a swim hit's deep water is neither, but
    /// the plane bodies swim at ([`water_plane`]). Every other triangle is solid whatever its
    /// material: in `l` shore ground (m25's `h004200` at (116.0, 104.9, 408.1)) and the water
    /// surface enemies stand on and the camera stops at (hit filter 16, m17's `h900702`) carry
    /// the deep-water material, which the capsule does not care about (#189).
    fn solid(&self, t: u32) -> bool {
        !water_plane(self.materials[t as usize], self.in_swim_hit(t))
    }

    /// Whether triangle `t` belongs to a swim hit ([`CollisionWorld::add_helpers`]).
    fn in_swim_hit(&self, t: u32) -> bool {
        match self.part_of[t as usize] {
            ORDINARY => false,
            part if part & EVENT_PART != 0 => false,
            part => self.swim_parts[usize::from(part) - 1],
        }
    }

    /// Whether triangle `t` is a solid face flat enough to stand on in `who`'s queries. The
    /// winding does not say which side is solid (see the module notes), so it is a floor from
    /// above and a ceiling from below.
    fn walkable(&self, t: u32, who: u8) -> bool {
        self.normals[t as usize].y.abs() >= self.min_walk_ny && self.solid(t) && self.live(t, who)
    }

    /// Records that the triangles `range` belong to `part` (see `part_of`), which is not
    /// [`ORDINARY`]. The grid has them in the layer of their mask already
    /// ([`CollisionWorld::append`]).
    fn mark_part(&mut self, range: &std::ops::Range<u32>, part: u16) {
        self.part_of[range.start as usize..range.end as usize].fill(part);
    }

    /// Adds an event part over the just appended triangles `range` (see `event_parts`), which
    /// the grid holds in layer `who`.
    pub(crate) fn add_event_part(
        &mut self,
        entity: i32,
        range: std::ops::Range<u32>,
        who: u8,
        on: bool,
    ) {
        let part = u16::try_from(self.event_parts.len())
            .ok()
            .filter(|&i| i & EVENT_PART == 0)
            .expect("too many event parts");
        self.mark_part(&range, EVENT_PART | part);
        self.event_parts
            .push((entity, range, who, AtomicBool::new(on)));
    }

    /// The world of `mesh`, with the triangles from `gate_start` on as the gate, and all of them
    /// a swim hit's when `swim` ([`water_plane`]).
    fn build(mesh: &CollisionMesh, gate_start: Option<usize>, swim: bool) -> Self {
        assert!(
            mesh.indices.len() / 3 < 1 << 28,
            "too many triangles for the grid"
        );
        let mut gate_from = u32::MAX;
        let vertices: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from(*v)).collect();
        let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
        let mut normals = Vec::with_capacity(mesh.indices.len() / 3);
        let mut materials = Vec::with_capacity(mesh.indices.len() / 3);
        let mut bounds = Vec::with_capacity(mesh.indices.len() / 3);
        // Per cell its lists so far and its water.
        let mut columns: HashMap<(i32, i32), (Pending<false>, Pending<true>, Vec<u32>)> =
            HashMap::new();
        let min_walk_ny = MAX_SLOPE_DEGREES.to_radians().cos();
        for (source, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if gate_start == Some(source) {
                gate_from = triangles.len() as u32;
            }
            let [a, b, c] = tri.map(|i| vertices[i as usize]);
            let Some(n) = face_normal(a, b, c) else {
                continue;
            };
            let t = triangles.len() as u32;
            triangles.push(*tri);
            normals.push(n);
            let material = mesh.materials.get(source).copied().unwrap_or(u32::MAX);
            materials.push(material);
            let (lo, hi) = (a.min(b).min(c), a.max(b).max(c));
            bounds.push([lo, hi]);
            let floor = files_as_floor(n, material, min_walk_ny, swim);
            let (first, last) = bands_of(lo.y, hi.y);
            for cell in filed_cells(lo, hi, floor) {
                let (rest, floors, water) = columns.entry(cell).or_default();
                if floor {
                    floors.file(first, last, t);
                } else {
                    rest.file(first, last, t);
                }
                // Water surfaces: shallow water anywhere, deep water only as a swim hit's plane.
                if water_material(material) && (swim || !crate::body::deep_water(material)) {
                    water.push(t);
                }
            }
        }
        let columns = columns
            .into_iter()
            .map(|(cell, (rest, floors, water))| {
                let column = Column {
                    rest: rest.pack(BODIES),
                    floors: floors.pack(BODIES),
                    water: water.into_boxed_slice(),
                };
                (cell, column)
            })
            .collect();
        Self {
            part_of: vec![ORDINARY; triangles.len()],
            vertices,
            triangles,
            normals,
            materials,
            bounds,
            columns,
            min_walk_ny,
            gate_from,
            gate_open: AtomicBool::new(false),
            event_parts: Vec::new(),
            masked: Vec::new(),
            swim_parts: Vec::new(),
            triggers: Vec::new(),
            objects: Vec::new(),
            bare: Vec::new(),
        }
    }

    /// Adds the helper collision [`crate::hit_filter::strip_helpers`] took out of the
    /// player's: enemy and camera parts to the grid, queried only by those they hold
    /// ([`EnemyGround`], [`CollisionWorld::camera_raycast`]), the swim hits as the player's and
    /// enemies' water planes, and the event walls to the contact list.
    pub fn add_helpers(&mut self, helpers: &crate::hit_filter::HelperCollision) {
        for (&mask, mesh) in &helpers.parts {
            self.add_masked(mask, mesh);
        }
        let swim = crate::hit_filter::mask(crate::hit_filter::SWIM_FILTER);
        self.add_part(swim, &helpers.swim, true);
        for trigger in &helpers.triggers {
            let mesh = &trigger.mesh;
            let triangles: Vec<[Vec3; 3]> = mesh
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| Vec3::from(mesh.vertices[i as usize])))
                .collect();
            if triangles.is_empty() {
                continue;
            }
            let all = triangles.iter().flatten();
            let min = all.clone().copied().reduce(Vec3::min).unwrap();
            let max = all.copied().reduce(Vec3::max).unwrap();
            self.triggers.push(TriggerPart {
                hit_filter: trigger.hit_filter,
                part: trigger.part.clone(),
                min,
                max,
                triangles,
            });
        }
    }

    /// Adds the collision of a block's placed objects (`object_collision.bin`), each with the
    /// mask `mask` gives it from its part and [`crate::hit_filter::object_mask`] (0 leaves it
    /// out): objects everyone collides with as ordinary triangles, the rest as helpers of their
    /// mask. Each mask's objects join the grid as one mesh, and each object is recorded by its
    /// triangles in it as `<block>/<part>`, since part names repeat across blocks. Returns the
    /// parts and triangles added.
    pub fn add_objects(
        &mut self,
        block: &str,
        objects: &ObjectCollision,
        mut mask: impl FnMut(&ObjectCollisionPart, u8) -> u8,
    ) -> Result<(usize, usize), String> {
        let mut by_mask: std::collections::BTreeMap<u8, Vec<&ObjectCollisionPart>> =
            Default::default();
        for part in &objects.parts {
            let who = crate::hit_filter::object_mask(part.collision_type);
            let who = if who == 0 { 0 } else { mask(part, who) };
            if who != 0 {
                by_mask.entry(who).or_default().push(part);
            }
        }
        let source = &objects.mesh;
        let (mut parts, first) = (0, self.triangles.len());
        // Source vertex -> its index in the group's mesh.
        let mut slot = vec![u32::MAX; source.vertices.len()];
        for (who, group) in by_mask {
            slot.fill(u32::MAX);
            let mut mesh = CollisionMesh::default();
            let mut ranges = Vec::with_capacity(group.len());
            for part in group {
                let start = mesh.triangle_count() as u32;
                copy_part(source, part.first, part.count, &mut slot, &mut mesh)
                    .map_err(|e| format!("{block}/{}: {e}", part.name))?;
                ranges.push((start..mesh.triangle_count() as u32, &part.name));
            }
            let range = self.add_masked(who, &mesh);
            if self.bare.len() < range.end as usize {
                self.bare.resize(range.end as usize, false);
            }
            self.bare[range.start as usize..range.end as usize].fill(true);
            for (r, name) in ranges {
                let r = range.start + r.start..range.start + r.end;
                self.objects.push((r, format!("{block}/{name}")));
                parts += 1;
            }
        }
        Ok((parts, self.triangles.len() - first))
    }

    /// [`CollisionWorld::add_objects`] with a block's `object_collision.bin` in its map folder
    /// `dir`, if it has one (none adds nothing).
    pub fn add_block_objects(
        &mut self,
        dir: &Path,
        block: &str,
        mask: impl FnMut(&ObjectCollisionPart, u8) -> u8,
    ) -> Result<(usize, usize), String> {
        let path = dir.join("object_collision.bin");
        if !path.is_file() {
            return Ok((0, 0));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let objects = ObjectCollision::from_bytes(&bytes).map_err(|e| e.to_string())?;
        self.add_objects(block, &objects, mask)
    }

    /// Appends a mesh only those `mask` holds collide with ([`crate::hit_filter::PLAYER`],
    /// `ENEMY`, `CAMERA` bits); with all three bits it is ordinary collision.
    fn add_masked(&mut self, mask: u8, mesh: &CollisionMesh) -> std::ops::Range<u32> {
        self.add_part(mask, mesh, false)
    }

    /// [`CollisionWorld::add_masked`], as a swim hit when `swim` ([`water_plane`]).
    fn add_part(&mut self, mask: u8, mesh: &CollisionMesh, swim: bool) -> std::ops::Range<u32> {
        let range = self.append_part(mesh, mask, swim);
        if (mask != BODIES || swim) && !range.is_empty() {
            self.masked.push(mask);
            self.swim_parts.push(swim);
            let part = u16::try_from(self.masked.len())
                .ok()
                .filter(|&i| i & EVENT_PART == 0)
                .expect("too many helper parts");
            self.mark_part(&range, part);
        }
        range
    }

    /// The placed object (`<block>/<part>`, [`CollisionWorld::add_objects`]) a triangle belongs
    /// to, if it is an object's.
    pub fn object_at(&self, triangle: u32) -> Option<&str> {
        let i = self
            .objects
            .partition_point(|(range, _)| range.end <= triangle);
        self.objects
            .get(i)
            .filter(|(range, _)| range.contains(&triangle))
            .map(|(_, name)| name.as_str())
    }

    /// The placed object whose collision is the floor [`CollisionWorld::floor`] finds, if the
    /// floor there is an object's.
    pub fn floor_object(&self, x: f32, z: f32, top: f32) -> Option<&str> {
        self.object_at(self.floor_hit(x, z, top)?.1)
    }

    /// Appends a mesh's triangles to the grid, in the layer of those `mask` holds; the caller
    /// records what they are. Each list is rebuilt to its exact size, as the build left them,
    /// rather than grown by doubling.
    pub(crate) fn append(&mut self, mesh: &CollisionMesh, mask: u8) -> std::ops::Range<u32> {
        self.append_part(mesh, mask, false)
    }

    /// [`CollisionWorld::append`], as a swim hit's triangles when `swim` ([`water_plane`]).
    fn append_part(&mut self, mesh: &CollisionMesh, mask: u8, swim: bool) -> std::ops::Range<u32> {
        let part = Self::build(mesh, None, swim);
        let vertex_offset = self.vertices.len() as u32;
        let first = self.triangles.len() as u32;
        let last = first + part.triangles.len() as u32;
        let more = part.triangles.len();
        self.vertices.reserve_exact(part.vertices.len());
        self.vertices.extend(part.vertices);
        self.triangles.reserve_exact(more);
        self.triangles.extend(
            part.triangles
                .into_iter()
                .map(|tri| tri.map(|i| i + vertex_offset)),
        );
        self.normals.reserve_exact(more);
        self.normals.extend(part.normals);
        self.materials.reserve_exact(more);
        self.materials.extend(part.materials);
        self.bounds.reserve_exact(more);
        self.bounds.extend(part.bounds);
        self.part_of.reserve_exact(more);
        self.part_of.resize(self.triangles.len(), ORDINARY);
        for (cell, column) in part.columns {
            let own = self.columns.entry(cell).or_default();
            *own = own.merge(&column, first, mask);
        }
        first..last
    }

    /// The first event wall a body's capsule meets moving from feet `from` to feet `to`:
    /// the path is tested at the capsule's bottom, middle and top.
    pub fn trigger_contact(&self, from: Vec3, to: Vec3) -> Option<TriggerContact> {
        let travel = to - from;
        let length = travel.length();
        if length < 1e-4 {
            return None;
        }
        let dir = travel / length;
        let lo = from.min(to);
        let hi = from.max(to) + Vec3::Y * CAPSULE_HEIGHT;
        let mut best: Option<(f32, TriggerContact)> = None;
        for part in &self.triggers {
            if (hi.cmplt(part.min - RADIUS)).any() || (lo.cmpgt(part.max + RADIUS)).any() {
                continue;
            }
            for h in [0.05, CAPSULE_HEIGHT * 0.5, CAPSULE_HEIGHT - 0.05] {
                let origin = from + Vec3::Y * h;
                for &[a, b, c] in &part.triangles {
                    if let Some(d) = ray_triangle(origin, dir, a, b, c)
                        && d <= length
                        && best.as_ref().is_none_or(|(b, _)| d < *b)
                    {
                        best = Some((
                            d,
                            TriggerContact {
                                hit_filter: part.hit_filter,
                                part: part.part.clone(),
                                point: origin + dir * d,
                            },
                        ));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// Whether an event wall with the fall return's hit filter lies below `(x, y, z)`.
    pub fn fall_return_below(&self, x: f32, y: f32, z: f32) -> bool {
        self.triggers
            .iter()
            .filter(|p| p.hit_filter == crate::hit_filter::FALL_RETURN_FILTER)
            .filter(|p| p.min.y < y && (p.min.x..=p.max.x).contains(&x))
            .filter(|p| (p.min.z..=p.max.z).contains(&z))
            .any(|p| {
                p.triangles
                    .iter()
                    .any(|&[a, b, c]| height_on(x, z, a, b, c).is_some_and(|h| h < y))
            })
    }

    /// Whether a body's capsule standing at `feet` is still within reach of an event wall part
    /// (its bounds widened by the capsule radius): one contact lasts until it leaves.
    pub fn near_trigger(&self, part: &str, feet: Vec3) -> bool {
        let lo = feet;
        let hi = feet + Vec3::Y * CAPSULE_HEIGHT;
        self.triggers.iter().any(|p| {
            p.part == part && !(hi.cmplt(p.min - RADIUS)).any() && !(lo.cmpgt(p.max + RADIUS)).any()
        })
    }

    /// Distance along `dir` (unit length) to the first triangle the camera ray stops at within
    /// `max`: [`CollisionWorld::raycast`] with the camera's helper parts instead of the player's.
    pub fn camera_raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.ray(origin, dir, max, CAMERA).0
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// The convex radius triangle `t`'s surface is pushed out by: [`MESH_CONVEX_RADIUS`] for
    /// the map's, 0 for a placed object's (its own shape's radius is in its triangles).
    fn skin(&self, t: u32) -> f32 {
        if self.bare.get(t as usize).copied().unwrap_or(false) {
            0.0
        } else {
            MESH_CONVEX_RADIUS
        }
    }

    fn corners(&self, t: u32) -> [Vec3; 3] {
        self.triangles[t as usize].map(|i| self.vertices[i as usize])
    }

    /// The highest walkable surface under `(x, z)` whose height is at most `top`.
    pub fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_hit(x, z, top).map(|(h, _)| h)
    }

    /// [`CollisionWorld::floor`] with the floor's triangle, for [`CollisionWorld::object_at`].
    pub fn floor_hit(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        self.floor_triangle(x, z, top, PLAYER)
    }

    /// The hit material of the floor [`CollisionWorld::floor`] finds, when known.
    pub fn floor_material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        self.floor_material_for(x, z, top, PLAYER)
    }

    fn floor_material_for(&self, x: f32, z: f32, top: f32, who: u8) -> Option<u32> {
        let (_, t) = self.floor_triangle(x, z, top, who)?;
        self.triangle_material(t)
    }

    /// Triangle `t`'s hit material, when known.
    fn triangle_material(&self, t: u32) -> Option<u32> {
        self.materials
            .get(t as usize)
            .copied()
            .filter(|&m| m != u32::MAX)
    }

    /// The highest walkable surface under `(x, z)` no higher than `top`: height and triangle.
    /// Of equally high floors, the first triangle's.
    fn floor_triangle(&self, x: f32, z: f32, top: f32, who: u8) -> Option<(f32, u32)> {
        self.floor_triangles([[x, z]], top, who)[0]
    }

    /// [`CollisionWorld::floor_triangle`] at each of `points`, those in one cell read in one
    /// pass over its floors: a body's floor asks five points a few centimetres apart
    /// ([`View::floor_triangle`]).
    ///
    /// A point reads only the floors of the cell holding it ([`Column::floors`], all those that
    /// can be under it), in the layers `who` collides with, from the band of `top` down until a
    /// floor is found above the band.
    fn floor_triangles<const N: usize>(
        &self,
        points: [[f32; 2]; N],
        top: f32,
        who: u8,
    ) -> [Option<(f32, u32)>; N] {
        // Reads `floors` for the open points into `best`.
        let read = |floors: &[u32], best: &mut [Option<(f32, u32)>; N], open: &[bool; N]| {
            for &t in floors {
                if !self.live(t, who) {
                    continue;
                }
                let [a, b, c] = self.corners(t);
                for ((&[x, z], best), _) in points
                    .iter()
                    .zip(best.iter_mut())
                    .zip(open)
                    .filter(|(_, o)| **o)
                {
                    // The surface the exe's bodies meet: the face pushed out by its convex
                    // radius, `skin / |n.y|` higher seen straight down.
                    if let Some(h) = height_on(x, z, a, b, c)
                            .map(|h| h + self.skin(t) / self.normals[t as usize].y.abs())
                        && h <= top
                        && best.is_none_or(|(b, first)| h > b || h == b && t < first)
                    {
                        *best = Some((h, t));
                    }
                }
            }
        };
        let keys = points.map(|[x, z]| cell_of(x, z));
        let mut best: [Option<(f32, u32)>; N] = [None; N];
        let mut done = [false; N];
        for i in 0..N {
            if done[i] {
                continue;
            }
            // The points of this cell still to answer.
            let mut open: [bool; N] = std::array::from_fn(|j| !done[j] && keys[j] == keys[i]);
            for (d, o) in done.iter_mut().zip(&open) {
                *d |= *o;
            }
            let Some(square) = self.columns.get(&keys[i]).map(|c| &c.floors) else {
                continue;
            };
            // The tall floors first, at any height.
            for layer in square.layers_for(who) {
                read(layer.tall(), &mut best, &open);
            }
            let Some((lowest, highest)) = square.span(who) else {
                continue;
            };
            // A floor no higher than `top` is filed in the band of `top` or lower ones.
            let start = band_of(top).min(highest);
            let mut band = start;
            while band >= lowest {
                // Any floor not yet met is filed only in lower bands, so lies below this one.
                for (o, best) in open.iter_mut().zip(&best) {
                    *o &= !best.is_some_and(|(h, _)| h >= (band + 1) as f32 * BAND);
                }
                if !open.contains(&true) {
                    break;
                }
                // Below the first band, the floors reaching higher were read there already; a
                // layer without the first band has none reaching down from above its highest.
                let parts = if band == start { ALL } else { FIRST };
                for layer in square.layers_for(who) {
                    read(layer.parts(band, parts.clone()), &mut best, &open);
                }
                // The next band down any layer has.
                match square.layers_for(who).filter_map(|l| l.below(band)).max() {
                    Some(next) => band = next,
                    None => break,
                }
            }
        }
        best
    }

    /// The highest the feet of `who`'s body at `feet` can rise by at most `rise` before its top
    /// sphere meets a ceiling over it: any surface flat enough to be a floor (the winding does
    /// not say which side is solid, see the module notes) above that sphere's centre. The sphere
    /// is swept straight up against each such face, its interior, edges and corners, so a beam's
    /// edge over the rim or a sloping underside stops it where it first touches (#170). Only a
    /// face the sphere rises into counts: not one it is already in at the start, nor one that
    /// reaches no more than [`CEILING_GRAZE`] inside its rim, such as the top edge of a wall the
    /// body is pressed against, nor one it first meets lower on the sphere than [`CEILING_UP`]
    /// allows, such as a rock lip over the wall it is jumping up. When `Some`, it is below
    /// `feet.y + rise`.
    fn ceiling_for(&self, feet: Vec3, rise: f32, who: u8) -> Option<f32> {
        let centre = feet + Vec3::Y * (HEAD_TOP - RADIUS);
        let mut best: Option<f32> = None;
        self.ceiling_candidates(feet, rise, who, |t| {
            if let Some(up) = self.rise_into(t, centre, rise) {
                best = Some(best.map_or(up, |b| b.min(up)));
            }
        });
        best.map(|up| feet.y + up)
    }

    /// Calls `f` with the triangles [`CollisionWorld::ceiling_for`] sweeps the head sphere
    /// against, each once: those [`CollisionWorld::ceiling_keeps`] keeps, among the floors of
    /// the sweep's cells in its bands.
    fn ceiling_candidates(&self, feet: Vec3, rise: f32, who: u8, mut f: impl FnMut(u32)) {
        let sweep = CeilingSweep::new(feet, rise);
        let (x0, z0) = cell_of(sweep.min.x, sweep.min.y);
        let (x1, z1) = cell_of(sweep.max.x, sweep.max.y);
        let (b0, b1) = (band_of(sweep.centre), band_of(sweep.high));
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let Some(column) = self.columns.get(&(cx, cz)) else {
                    continue;
                };
                // Each face once: in the tall list, or in the first band of the scan that holds
                // it.
                for layer in column.floors.layers_for(who) {
                    for floors in layer.once(b0, b1) {
                        for &t in floors {
                            let [lo, _] = self.bounds[t as usize];
                            // And in the first cell of the scan that holds it.
                            if cell_of(lo.x.max(sweep.min.x), lo.z.max(sweep.min.y)) == (cx, cz)
                                && self.ceiling_keeps(t, &sweep, who)
                            {
                                f(t);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Whether [`CollisionWorld::ceiling_for`] sweeps the head sphere against triangle `t`: a
    /// face flat enough to be a ceiling, reaching from above the sphere's centre to below its top
    /// after the rise, whose box comes within [`ceiling_reach`] of its centre line.
    fn ceiling_keeps(&self, t: u32, sweep: &CeilingSweep, who: u8) -> bool {
        let [lo, hi] = self.bounds[t as usize];
        let (lo2, hi2) = (Vec2::new(lo.x, lo.z), Vec2::new(hi.x, hi.z));
        let reach = ceiling_reach();
        hi.y >= sweep.centre
            && lo.y <= sweep.high
            && sweep.line.distance_squared(sweep.line.clamp(lo2, hi2)) < reach * reach
            && self.walkable(t, who)
    }

    /// How far the sphere of [`RADIUS`] at `centre` rises, less than `rise`, before it first
    /// touches triangle `t`: the least rise at which it meets the face's interior, one of its
    /// edges or one of its corners, 0 if it touches it already. `None` when it does not touch it
    /// within `rise`, when it is already more than [`CEILING_GRAZE`] into it, or when the touch
    /// is lower on the sphere than [`CEILING_UP`] allows (which a face reaching no further than
    /// [`ceiling_reach`] from its centre line, seen from above, always is).
    fn rise_into(&self, t: u32, centre: Vec3, rise: f32) -> Option<f32> {
        let [a, b, c] = self.corners(t);
        let line = centre.with_y(0.0);
        let reach = ceiling_reach();
        let q = closest_on_triangle(line, a.with_y(0.0), b.with_y(0.0), c.with_y(0.0));
        if q.distance_squared(line) >= reach * reach {
            return None;
        }
        let first = first_touch(centre, Vec3::Y, [a, b, c], self.normals[t as usize])?;
        if first >= rise {
            return None;
        }
        let touching = first < 0.0;
        let first = first.max(0.0);
        let at = centre + Vec3::Y * first;
        let q = closest_on_triangle(at, a, b, c);
        let to = q - at;
        // Already touching: within the graze it stops the rise now; deeper, the body is in the
        // face already (a slope it walked under), and rising carries it out.
        let graze = RADIUS - CEILING_GRAZE;
        if touching && to.length_squared() < graze * graze {
            return None;
        }
        // Off the top of the sphere it meets the face with its shoulder, as a jump up an uneven
        // wall meets each rock lip it is pressed against: the slide pushes it off that.
        (to.y >= CEILING_UP * to.length()).then_some(first)
    }

    /// How many triangles a [`CollisionWorld::ceiling_for`] of `rise` from `feet` sweeps the
    /// head sphere against. Only for the step-cost test.
    #[doc(hidden)]
    pub fn ceiling_triangle_count(&self, feet: [f32; 3], rise: f32) -> usize {
        let mut count = 0;
        self.ceiling_candidates(Vec3::from(feet), rise, PLAYER, |_| count += 1);
        count
    }

    /// How far a body at `feet` moves along `delta` before the top half of its head sphere
    /// meets a face: the sphere swept along the move ([`first_touch`]) against every solid face
    /// near it, counting a touch only where the face's closest point is above the sphere's
    /// centre (#168). A wire flight's rays run along the body's centre line only, so a beam or
    /// overhang whose edge is beside that line, inside the head's rim, passed into the head;
    /// what the head meets at its side or below is left to those rays, as before. A face the
    /// sphere is already more than [`CEILING_GRAZE`] into at the start does not count, as for the
    /// ceiling stop: moving on carries it out; nor does one the move does not close on at the
    /// touch, such as an eave the head grazes at the start of a flight away from it. `None` when
    /// nothing is met within `delta`.
    pub fn head_sweep(&self, feet: Vec3, delta: Vec3) -> Option<f32> {
        let len = delta.length();
        if len < 1e-6 || !feet.is_finite() || !delta.is_finite() {
            return None;
        }
        let dir = delta / len;
        let centre = feet + Vec3::Y * (HEAD_TOP - RADIUS);
        let end = centre + delta;
        let (lo, hi) = (
            centre.min(end) - Vec3::splat(RADIUS),
            centre.max(end) + Vec3::splat(RADIUS),
        );
        let (x0, z0) = cell_of(lo.x, lo.z);
        let (x1, z1) = cell_of(hi.x, hi.z);
        let graze = RADIUS - CEILING_GRAZE;
        let mut best: Option<f32> = None;
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let Some(column) = self.columns.get(&(cx, cz)) else {
                    continue;
                };
                for list in column.once(PLAYER, band_of(lo.y), band_of(hi.y)) {
                    for &t in list {
                        let [tlo, thi] = self.bounds[t as usize];
                        // Each face once: in the first cell of the sweep's box that holds it.
                        if tlo.cmpgt(hi).any()
                            || thi.cmplt(lo).any()
                            || cell_of(tlo.x.max(lo.x), tlo.z.max(lo.z)) != (cx, cz)
                            || !self.solid(t)
                            || !self.live(t, PLAYER)
                        {
                            continue;
                        }
                        let corners = self.corners(t);
                        let Some(s) = first_touch(centre, dir, corners, self.normals[t as usize])
                        else {
                            continue;
                        };
                        if s >= len || best.is_some_and(|b| s >= b) {
                            continue;
                        }
                        let s = s.max(0.0);
                        let at = centre + dir * s;
                        let [a, b, c] = corners;
                        let to = closest_on_triangle(at, a, b, c) - at;
                        let already_in = s == 0.0 && to.length_squared() < graze * graze;
                        // Only a move that closes on the face meets it: one grazing it at the
                        // start and moving away or along it does not.
                        let closing = dir.dot(to) > 1e-6;
                        if !already_in && closing && to.y > 1e-4 {
                            best = Some(s);
                        }
                    }
                }
            }
        }
        best
    }

    /// Whether `who`'s head sphere at `feet` touches a solid face over its centre: the cheap
    /// test that gates the push-out after a rise ([`Ground::head_contact`]). Only such a face
    /// can have come into the head by the rise; one at its side was there at the slide's height
    /// too, and the slide has cleared it already. In open air, or rising beside a wall, there is
    /// nothing to push. It stops at the first face it finds.
    fn head_contact_for(&self, feet: Vec3, who: u8) -> bool {
        let centre = feet + Vec3::Y * (HEAD_TOP - RADIUS);
        let (lo, hi) = (
            centre - Vec3::new(RADIUS, 0.0, RADIUS),
            centre + Vec3::splat(RADIUS),
        );
        let (x0, z0) = cell_of(lo.x, lo.z);
        let (x1, z1) = cell_of(hi.x, hi.z);
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let Some(column) = self.columns.get(&(cx, cz)) else {
                    continue;
                };
                for list in column.once(who, band_of(lo.y), band_of(hi.y)) {
                    for &t in list {
                        let [tlo, thi] = self.bounds[t as usize];
                        if thi.y <= centre.y
                            || centre.distance_squared(centre.clamp(tlo, thi)) >= RADIUS * RADIUS
                            || !self.solid(t)
                            || !self.live(t, who)
                        {
                            continue;
                        }
                        let [a, b, c] = self.corners(t);
                        let to = closest_on_triangle(centre, a, b, c) - centre;
                        if to.y > 1e-4 && to.length_squared() < RADIUS * RADIUS {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Every surface the vertical line through `(x, z)` crosses: (height, normal y), highest
    /// first. For debugging and tests.
    pub fn surfaces_at(&self, x: f32, z: f32) -> Vec<(f32, f32)> {
        let mut cell: Vec<u32> = Vec::new();
        if let Some(column) = self.columns.get(&cell_of(x, z)) {
            column
                .once(PLAYER, i32::MIN, i32::MAX)
                .for_each(|list| cell.extend(list));
        }
        cell.sort_unstable();
        let mut out: Vec<(f32, f32)> = cell
            .iter()
            .filter(|&&t| self.live(t, PLAYER))
            .filter_map(|&t| {
                let [a, b, c] = self.corners(t);
                Some((height_on(x, z, a, b, c)?, self.normals[t as usize].y))
            })
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out
    }

    /// Distance along `dir` (unit length) to the first triangle within `max`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.ray(origin, dir, max, PLAYER).0
    }

    /// The ray's `(hit, tested)`: the distance to the first hit, and how many triangles the ray
    /// was tested against.
    fn ray(&self, origin: Vec3, dir: Vec3, max: f32, who: u8) -> (Option<f32>, usize) {
        let end = origin + dir * max;
        if max < 0.0 || !origin.is_finite() || !end.is_finite() {
            return (None, 0);
        }
        // Walk the cells the ray crosses in order, testing in each only the triangles that reach
        // the ray's height over that cell, and stop once the nearest hit lies before the next
        // cell: any triangle not yet tested is met further along. Hot cells hold thousands of
        // triangles from other storeys, and gathering, sorting and testing all of them along a
        // sight ray blocked by the next wall cost about 1 ms a ray (#161).
        let (mut x, mut z) = cell_of(origin.x, origin.z);
        let (ex, ez) = cell_of(end.x, end.z);
        let axis = |start: f32, delta: f32, cell: i32| {
            if delta == 0.0 {
                return (0, f32::INFINITY, f32::INFINITY);
            }
            let step = if delta > 0.0 { 1 } else { -1 };
            let boundary = (cell as f32 + if step > 0 { 1.0 } else { 0.0 }) * CELL;
            (step, (boundary - start) / delta, CELL / delta.abs())
        };
        // Cell crossings as fractions of the whole ray.
        let (sx, mut tx, dx) = axis(origin.x, end.x - origin.x, x);
        let (sz, mut tz, dz) = axis(origin.z, end.z - origin.z, z);
        // Roundoff margins: along the ray (as a fraction of it) and in height and distance (m).
        const ALONG: f32 = 1e-4;
        const PAD: f32 = 1e-3;
        let rise = end.y - origin.y;
        // Tests the triangles of cell `cell` the ray can meet between fractions `enter` and
        // `leave` of it.
        let scan = |cell: (i32, i32), enter: f32, leave: f32, best: &mut Option<f32>| {
            let Some(column) = self.columns.get(&cell) else {
                return 0;
            };
            let [y0, y1] =
                [(enter - ALONG).max(0.0), (leave + ALONG).min(1.0)].map(|t| origin.y + rise * t);
            let (low_y, high_y) = (y0.min(y1) - PAD, y0.max(y1) + PAD);
            let (b0, b1) = (band_of(low_y), band_of(high_y));
            let mut tested = 0;
            // Each triangle once: in the tall list, or in the first band of the scan that holds
            // it.
            for list in column.once(who, b0, b1) {
                for &t in list {
                    let [lo, hi] = self.bounds[t as usize];
                    if lo.y > high_y || hi.y < low_y || !self.solid(t) || !self.live(t, who) {
                        continue;
                    }
                    tested += 1;
                    let [a, b, c] = self.corners(t);
                    if let Some(d) = ray_triangle(origin, dir, a, b, c)
                        && d <= max
                        && best.is_none_or(|b| d < b)
                    {
                        *best = Some(d);
                    }
                }
            }
            tested
        };
        let mut enter = 0.0f32;
        let mut best: Option<f32> = None;
        let mut tested = 0;
        for _ in 0..(u64::from(x.abs_diff(ex)) + u64::from(z.abs_diff(ez)) + 1) {
            let last = (x, z) == (ex, ez);
            // Roundoff must not step an axis beyond the endpoint's cell.
            if x == ex {
                tx = f32::INFINITY;
            }
            if z == ez {
                tz = f32::INFINITY;
            }
            // 1 in the last cell, where both crossings are infinite.
            let leave = tx.min(tz).clamp(enter, 1.0);
            tested += scan((x, z), enter, leave, &mut best);
            // Any triangle not yet tested lies past `leave`; the margin grows with the ray, as
            // the crossings add up roundoff cell by cell.
            if last || best.is_some_and(|b| b < (leave - ALONG) * max - PAD) {
                break;
            }
            if (tx - tz).abs() <= ALONG {
                // Through a corner, as far as roundoff tells: the cells on both sides of it
                // too, which a triangle can touch only there.
                let (near, far) = (tx.min(tz), tx.max(tz).min(1.0));
                for cell in [(x + sx, z), (x, z + sz)] {
                    tested += scan(cell, near, far, &mut best);
                }
                x += sx;
                z += sz;
                tx += dx;
                tz += dz;
            } else if tx < tz {
                x += sx;
                tx += dx;
            } else {
                z += sz;
                tz += dz;
            }
            enter = leave;
        }
        (best, tested)
    }

    /// How many triangles a [`CollisionWorld::raycast`] tests the ray against. Only for the
    /// step-cost test (#161).
    #[doc(hidden)]
    pub fn ray_triangle_count(&self, origin: Vec3, dir: Vec3, max: f32) -> usize {
        self.ray(origin, dir, max, PLAYER).1
    }

    /// Moves a body standing at `feet` by the horizontal `delta`, stopping at walls and
    /// sliding along them. Returns the horizontal displacement actually made.
    pub fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.slide_for(feet, delta, PLAYER, 0, true)
    }

    /// [`CollisionWorld::slide`] without the horizontal cull of [`CollisionWorld::gather_slide`]
    /// and the push-outs' [`Nearby`] list and plane test: every triangle of the move's cells in
    /// the body's height band, each tested in full. Only for the tests that the culls change no
    /// slide (#161, #167).
    #[doc(hidden)]
    pub fn slide_uncull(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.slide_for(feet, delta, PLAYER, 0, false)
    }

    /// How many triangles a [`CollisionWorld::slide`] of `delta` from `feet` tests the body
    /// against on each pass: those near the move, solid, and within the body's height band.
    /// Only for the step-cost test (#145).
    #[doc(hidden)]
    pub fn slide_triangle_count(&self, feet: [f32; 3], delta: [f32; 2]) -> usize {
        let (mut tris, mut walls) = (Vec::new(), Vec::new());
        self.gather_slide(
            Vec3::from(feet),
            Vec2::from(delta),
            [PLAYER, 0],
            &mut tris,
            &mut walls,
            true,
        );
        tris.len()
    }

    /// What an assisted slide ([`Ground::slide_assisted`]) of `delta` from `feet` with the
    /// fall-prevention walls `assist` tests: its sub-steps, and the body triangles and walls it
    /// tests on each push-out pass. Only for the step-cost test (#149).
    #[doc(hidden)]
    pub fn assisted_slide_cost(&self, feet: [f32; 3], delta: [f32; 2], assist: u8) -> [usize; 3] {
        let (mut tris, mut walls) = (Vec::new(), Vec::new());
        let d = Vec2::from(delta);
        let coarse = self.gather_slide(
            Vec3::from(feet),
            d,
            [PLAYER, assist],
            &mut tris,
            &mut walls,
            true,
        );
        [
            slide_steps(coarse, d.length(), walls.is_empty()),
            tris.len(),
            walls.len(),
        ]
    }

    /// Gathers, each once, the triangles near a slide of `d` from `start` that `who[0]`'s body
    /// can touch into `body` ([`CollisionWorld::slide_body_keeps`]), and the fall-prevention
    /// walls in `who[1]` (none for 0) it can be held by into `walls`
    /// ([`CollisionWorld::slide_wall_keeps`]). Returns the slide's coarse sub-step count.
    ///
    /// Each comes from the layers of the move's cells that can hold it, in the bands it can
    /// reach. Only the assist bits of `who[1]` count: the walls are triangles only the assist
    /// collides with, and with a body's bit every ordinary triangle near the feet would be one.
    /// `cull` false skips the horizontal keep box, for [`CollisionWorld::slide_uncull`].
    fn gather_slide(
        &self,
        start: Vec3,
        d: Vec2,
        [who, assist]: [u8; 2],
        body: &mut Vec<u32>,
        walls: &mut Vec<u32>,
        cull: bool,
    ) -> usize {
        let reach = SlideReach::new(start, d, cull);
        let assist = assist & !BODIES;
        body.clear();
        walls.clear();
        let (x0, z0) = cell_of(reach.min.x, reach.min.y);
        let (x1, z1) = cell_of(reach.max.x, reach.max.y);
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let Some(column) = self.columns.get(&(cx, cz)) else {
                    continue;
                };
                // Each triangle once: in the tall list, or in the first band of the scan that
                // holds it.
                let gather = |into: &mut Vec<u32>,
                              mask: u8,
                              [low, high]: [f32; 2],
                              keep: &dyn Fn(u32) -> bool| {
                    if mask == 0 {
                        return;
                    }
                    for list in column.once(mask, band_of(low), band_of(high)) {
                        into.extend(list.iter().copied().filter(|&t| keep(t)));
                    }
                };
                gather(body, who, reach.body, &|t| {
                    self.slide_body_keeps(t, &reach, who)
                });
                gather(walls, assist, reach.feet, &|t| {
                    self.slide_wall_keeps(t, &reach, assist)
                });
            }
        }
        // In triangle order, each once, as the push-outs depend on the order.
        for tris in [body, walls] {
            tris.sort_unstable();
            tris.dedup();
        }
        reach.coarse
    }

    /// Whether a slide ([`SlideReach`]) tests `who`'s body against triangle `t`: a solid one
    /// that reaches the body's spheres' height band at the start and, with the cull, the keep
    /// box. A slide moves the body only horizontally, so any other triangle is one
    /// [`CollisionWorld::push_out`] would reject on every pass; hot cells hold thousands of
    /// them from other storeys (#145).
    fn slide_body_keeps(&self, t: u32, reach: &SlideReach, who: u8) -> bool {
        let [lo, hi] = self.bounds[t as usize];
        hi.y >= reach.body[0]
            && lo.y <= reach.body[1]
            && reach.near(lo, hi)
            && self.solid(t)
            && self.live(t, who)
    }

    /// Whether a slide ([`SlideReach`]) can be held by fall-prevention wall `t` of `assist`:
    /// [`CollisionWorld::hold_inside`] tests the walls at the feet, which a slide moves only
    /// sideways, so a wall whose box is [`FALL_ASSIST_RADIUS`] or more above or below the feet
    /// never holds, nor, with the cull, one outside the keep box. A wall gathered at all makes
    /// the slide take fine sub-steps (`slide_steps`), and m11_00's `f` binder puts walls in a
    /// quarter of the occupied cells (up to 248 a cell), mostly bands of other storeys.
    fn slide_wall_keeps(&self, t: u32, reach: &SlideReach, assist: u8) -> bool {
        let [lo, hi] = self.bounds[t as usize];
        hi.y > reach.feet[0] && lo.y < reach.feet[1] && reach.near(lo, hi) && self.live(t, assist)
    }

    /// [`CollisionWorld::slide`] for `who` that also keeps the body [`FALL_ASSIST_RADIUS`]
    /// from the fall-prevention walls in `assist` ([`crate::hit_filter::FALL_ASSIST_MOVE`],
    /// `FALL_ASSIST_ACTION` bits), which nothing else collides with.
    /// `cull` is [`CollisionWorld::gather_slide`]'s.
    fn slide_for(
        &self,
        feet: [f32; 3],
        delta: [f32; 2],
        who: u8,
        assist: u8,
        cull: bool,
    ) -> [f32; 2] {
        thread_local! {
            static SCRATCH: std::cell::RefCell<(Vec<u32>, Vec<u32>, Contacts)> =
                const { std::cell::RefCell::new((Vec::new(), Vec::new(), Contacts::new())) };
        }
        let start = Vec3::from(feet);
        let d = Vec2::from(delta);
        SCRATCH.with_borrow_mut(|(tris, walls, contacts)| {
            let coarse = self.gather_slide(start, d, [who, assist], tris, walls, cull);
            self.slide_among(start, d, coarse, tris, walls, contacts, cull)
        })
    }

    /// [`CollisionWorld::slide_for`] without fall-prevention walls of each of `deltas` from
    /// `feet`, reading the cells once for all of them (#167): the fan of 0.8 m probes
    /// `docking::wall_contact` makes every airborne step read and filtered the same cells eight
    /// times. Each slide still tests exactly the triangles its own
    /// [`CollisionWorld::gather_slide`] keeps, in the same order: the shared gather keeps those
    /// any of them could (the body band is the same for all, from the same start, and the keep
    /// box is the union of theirs), noting each cell's share, and each slide takes those of its
    /// own cells in its own keep box. So each ends exactly where it would alone.
    fn slide_fan_for<const N: usize>(
        &self,
        feet: [f32; 3],
        deltas: [[f32; 2]; N],
        who: u8,
    ) -> [[f32; 2]; N] {
        thread_local! {
            static SCRATCH: std::cell::RefCell<FanScratch> =
                const { std::cell::RefCell::new(FanScratch::new()) };
        }
        let start = Vec3::from(feet);
        let reaches = deltas.map(|d| SlideReach::new(start, Vec2::from(d), true));
        let Some(mut all) = reaches.first().cloned() else {
            return deltas;
        };
        // One reach covering them all: the same body band and the union of the boxes.
        for r in &reaches {
            all.min = all.min.min(r.min);
            all.max = all.max.max(r.max);
            if let (Some([lo, hi]), Some([rlo, rhi])) = (all.near.as_mut(), r.near) {
                *lo = lo.min(rlo);
                *hi = hi.max(rhi);
            }
        }
        SCRATCH.with_borrow_mut(|s| {
            s.pool.clear();
            s.cells.clear();
            let (x0, z0) = cell_of(all.min.x, all.min.y);
            let (x1, z1) = cell_of(all.max.x, all.max.y);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    let Some(column) = self.columns.get(&(cx, cz)) else {
                        continue;
                    };
                    let from = s.pool.len();
                    // As `gather_slide`: each triangle once a cell.
                    for list in column.once(who, band_of(all.body[0]), band_of(all.body[1])) {
                        s.pool.extend(
                            list.iter()
                                .copied()
                                .filter(|&t| self.slide_body_keeps(t, &all, who)),
                        );
                    }
                    s.cells.push(((cx, cz), from..s.pool.len()));
                }
            }
            std::array::from_fn(|i| {
                let reach = &reaches[i];
                let (x0, z0) = cell_of(reach.min.x, reach.min.y);
                let (x1, z1) = cell_of(reach.max.x, reach.max.y);
                s.tris.clear();
                for ((cx, cz), range) in &s.cells {
                    if (x0..=x1).contains(cx) && (z0..=z1).contains(cz) {
                        s.tris
                            .extend(s.pool[range.clone()].iter().copied().filter(|&t| {
                                let [lo, hi] = self.bounds[t as usize];
                                reach.near(lo, hi)
                            }));
                    }
                }
                s.tris.sort_unstable();
                s.tris.dedup();
                let d = Vec2::from(deltas[i]);
                self.slide_among(start, d, reach.coarse, &s.tris, &[], &mut s.contacts, true)
            })
        })
    }

    /// A slide of `d` from `start` against the triangles `tris` and the assist's walls `walls`
    /// that [`CollisionWorld::gather_slide`] gathered, in `coarse` sub-steps when there are no
    /// such walls. `contacts` is scratch for [`CollisionWorld::push_out`].
    ///
    /// A sub-step that wedges the body ([`CollisionWorld::push_out`] finds no push) is taken
    /// back and replaced by the part of it that goes deeper into none of the faces there
    /// (#176): along the wedge, never into it, and with no push, so it never crosses a face.
    /// A body that was wedged before the sub-step (placed there by a respawn, a warp or a
    /// moving part) instead also takes [`escape`]'s push, so it still gets out.
    ///
    /// `cull` lets the push-outs test only the triangles [`Nearby`] keeps near the body, and
    /// skip those whose plane is out of reach; false only for [`CollisionWorld::slide_uncull`].
    #[allow(clippy::too_many_arguments)]
    fn slide_among(
        &self,
        start: Vec3,
        d: Vec2,
        coarse: usize,
        tris: &[u32],
        walls: &[u32],
        contacts: &mut Contacts,
        cull: bool,
    ) -> [f32; 2] {
        contacts.near.reset(cull);
        let steps = slide_steps(coarse, d.length(), walls.is_empty());
        let step = d / steps as f32;
        let mut p = start;
        for _ in 0..steps {
            let before = p;
            p.x += step.x;
            p.z += step.y;
            for _ in 0..4 {
                let Some(mut push) = self.push_out(p, tris, contacts) else {
                    // Whether the body was wedged before the sub-step: then the contacts are
                    // the ones there, else those it moved into.
                    let started_wedged =
                        p == before || self.push_out(before, tris, contacts).is_none();
                    if !started_wedged {
                        self.push_out(p, tris, contacts);
                    }
                    let mut moved = wedged_move(step, contacts);
                    if started_wedged {
                        moved += escape(&contacts.found);
                    }
                    p = before + Vec3::new(moved.x, 0.0, moved.y);
                    break;
                };
                // The assist's walls last, so they hold where both push.
                if !walls.is_empty() {
                    push += self.hold_inside(p + Vec3::new(push.x, 0.0, push.y), walls);
                }
                if push == Vec2::ZERO {
                    break;
                }
                p.x += push.x;
                p.z += push.y;
            }
        }
        [p.x - start.x, p.z - start.z]
    }

    /// Horizontal correction that keeps the feet [`FALL_ASSIST_RADIUS`] on the inner side of
    /// the fall-prevention wall triangles `walls` they are near. The walls are tested at the feet (the
    /// height the exe tests at is not traced); the map's walls are bands from the floor at the
    /// edge to about 1 m above it. They are one-sided: `FUN_140bc68d0` cuts a move only along a
    /// hit triangle's face normal and only while the body is behind that face, so they keep a
    /// body in and let one outside walk in. The export's mirror into Bevy space reverses the
    /// winding, so the inner side is the one the Bevy-space normal points to.
    fn hold_inside(&self, feet: Vec3, walls: &[u32]) -> Vec2 {
        let mut push = Vec2::ZERO;
        for &t in walls {
            let [lo, hi] = self.bounds[t as usize];
            if feet.distance_squared(feet.clamp(lo, hi)) >= FALL_ASSIST_RADIUS * FALL_ASSIST_RADIUS
            {
                continue;
            }
            let [a, b, c] = self.corners(t);
            if feet.distance(closest_on_triangle(feet, a, b, c)) >= FALL_ASSIST_RADIUS {
                continue;
            }
            let n = self.normals[t as usize];
            let flat = Vec2::new(n.x, n.z);
            let inside = n.dot(feet - a);
            // Outside the wall, or a face too flat to push sideways from.
            if inside < 0.0 || flat.length_squared() < 0.09 {
                continue;
            }
            let dir = flat.normalize();
            let depth = (FALL_ASSIST_RADIUS - inside) / flat.length();
            if depth > push.dot(dir) {
                push += dir * (depth - push.dot(dir));
            }
        }
        push
    }

    /// Horizontal correction that takes the body's spheres out of the given triangles, which
    /// [`CollisionWorld::gather_slide`] has already limited to the solid ones the body can touch:
    /// the shortest push that clears every contact of both spheres at once ([`least_push`]),
    /// so the order of the triangles does not matter (#176). `None` when no push of at most
    /// [`RADIUS`] clears them all: the body is wedged between faces that push opposite ways,
    /// and pushing it out of one would take it into or through another. `contacts` is scratch
    /// and holds the contacts afterwards.
    fn push_out(&self, feet: Vec3, tris: &[u32], contacts: &mut Contacts) -> Option<Vec2> {
        contacts.found.clear();
        let mut near = std::mem::take(&mut contacts.near);
        let cull = near.on;
        let tris = near.around(self, feet, tris);
        for h in SPHERES {
            let centre = feet + Vec3::Y * h;
            for &t in tris {
                // The sphere's reach from this face: its radius, the keep distance and the
                // face's convex radius.
                let r = RADIUS + KEEP_DISTANCE + self.skin(t);
                // The horizontal grid includes stacked floors and walls from other heights.
                // Exact sphere/AABB rejection preserves the narrow-phase result and avoids
                // expensive closest-point tests on geometry this capsule cannot touch.
                let [min, max] = self.bounds[t as usize];
                if centre.distance_squared(centre.clamp(min, max)) >= r * r {
                    continue;
                }
                let [a, b, c] = self.corners(t);
                // Nor any whose plane is a radius away (and a millimetre for roundoff): the
                // closest point is no nearer (#167).
                if cull && self.normals[t as usize].dot(centre - a).abs() >= r + 1e-3 {
                    continue;
                }
                let q = closest_on_triangle(centre, a, b, c);
                let away = centre - q;
                let dist = away.length();
                if dist >= r {
                    continue;
                }
                // A contact below the centre whose way out leans within the proxy's maxSlope of
                // up supports the body rather than walling it: it climbs it (a step's nose).
                // One above whose way out leans as far down is a ceiling the body passes under
                // (the broken-wall planks 1.77 m over the floor past the Gate Path idol, #146),
                // left to the ceiling query.
                if away.y.abs() > SUPPORT_UP * dist {
                    continue;
                }
                // Push horizontally only; a contact straight above or below (beam, floor) has
                // no horizontal direction and is left to the floor query.
                let flat = Vec2::new(away.x, away.z);
                let n = self.normals[t as usize];
                let nf = Vec2::new(n.x, n.z);
                let dir = if flat.length_squared() > 1e-8 {
                    flat.normalize()
                } else {
                    if nf.length_squared() < 1e-4 {
                        continue;
                    }
                    nf.normalize()
                };
                // Depth along the horizontal direction needed to clear the triangle point.
                let needed = (r * r - away.y * away.y).max(0.0).sqrt();
                // The face's own way out, seen from above, on the body's side; none for a face
                // too flat to wall the body in (the threshold is `hold_inside`'s).
                let face = if nf.length_squared() < 0.09 {
                    Vec2::ZERO
                } else {
                    nf.normalize() * nf.dot(dir).signum()
                };
                contacts.add(Contact {
                    dir,
                    depth: needed - flat.dot(dir),
                    gap: flat.dot(dir),
                    face,
                });
            }
        }
        contacts.near = near;
        least_push(&contacts.found, WALL_REACH)
    }
}

/// How far the body may move from where [`Nearby`] last kept its triangles before it keeps
/// them again.
const NEARBY_MARGIN: f32 = 0.25;

/// The triangles of one slide near the body, so its push-outs test only those (#167): a slide
/// keeps every triangle its body could touch anywhere along the move ([`SlideReach`]), and a
/// 0.8 m wall-contact probe in a dense spot kept about 800, each tested on every pass. Kept
/// again whenever the body has moved more than [`NEARBY_MARGIN`] from where they were last
/// kept, they are those whose box comes within [`RADIUS`] and that margin (and a millimetre
/// for roundoff) of the segment between the body's sphere centres there. Any other triangle's
/// box is more than [`RADIUS`] from both centres wherever the body now is, which is exactly
/// the box test [`CollisionWorld::push_out`] skips a triangle by, so the push-outs and every
/// slide come out bit for bit the same.
#[derive(Default)]
struct Nearby {
    /// Whether to keep a nearby list at all (and test planes first, [`CollisionWorld::push_out`]).
    on: bool,
    /// The feet the list was kept around.
    centre: Option<Vec3>,
    tris: Vec<u32>,
}

impl Nearby {
    const fn new() -> Self {
        Self {
            on: false,
            centre: None,
            tris: Vec::new(),
        }
    }

    /// Starts a slide: its triangles are new.
    fn reset(&mut self, on: bool) {
        self.on = on;
        self.centre = None;
    }

    /// The triangles of the slide's `tris` a body at `feet` can touch.
    fn around<'a>(&'a mut self, world: &CollisionWorld, feet: Vec3, tris: &'a [u32]) -> &'a [u32] {
        if !self.on {
            return tris;
        }
        let kept = self.centre.is_some_and(|c| {
            c.y == feet.y
                && Vec2::new(feet.x - c.x, feet.z - c.z).length_squared()
                    <= NEARBY_MARGIN * NEARBY_MARGIN
        });
        if !kept {
            let reach = WALL_REACH + NEARBY_MARGIN + 0.001;
            let [low, high] = [SPHERES[0], SPHERES[SPHERES.len() - 1]].map(|h| feet.y + h);
            self.tris.clear();
            self.tris.extend(tris.iter().copied().filter(|&t| {
                let [min, max] = world.bounds[t as usize];
                let gap = |v: f32, lo: f32, hi: f32| (lo - v).max(v - hi).max(0.0);
                let dy = (min.y - high).max(low - max.y).max(0.0);
                let [dx, dz] = [gap(feet.x, min.x, max.x), gap(feet.z, min.z, max.z)];
                dx * dx + dy * dy + dz * dz < reach * reach
            }));
            self.centre = Some(feet);
        }
        &self.tris
    }
}

/// A sphere's contact with a triangle, for [`CollisionWorld::push_out`]: the horizontal
/// direction away from the triangle's closest point, how far along it the body must move to
/// clear it and how far it is from that point now, and the triangle's horizontal normal turned
/// to the body's side ([`Vec2::ZERO`] for a face too flat to be a wall), for [`wedged_move`].
#[derive(Clone, Copy, Debug)]
struct Contact {
    dir: Vec2,
    depth: f32,
    gap: f32,
    face: Vec2,
}

/// [`CollisionWorld::push_out`]'s contacts and [`wedged_move`]'s constraints, kept between
/// slides so neither allocates.
struct Contacts {
    found: Vec<Contact>,
    toward: Vec<Contact>,
    near: Nearby,
}

impl Contacts {
    const fn new() -> Self {
        Self {
            found: Vec::new(),
            toward: Vec::new(),
            near: Nearby::new(),
        }
    }

    /// Adds `c`, or deepens a contact already found in the same direction and against the same
    /// face (within [`SAME_DIR`]): the triangles of one face, or both spheres against one wall,
    /// are one constraint for [`least_push`].
    fn add(&mut self, c: Contact) {
        if let Some(same) = self.found.iter_mut().find(|f| {
            f.dir.distance_squared(c.dir) < SAME_DIR * SAME_DIR
                && f.face.distance_squared(c.face) < SAME_DIR * SAME_DIR
        }) {
            if c.depth > same.depth {
                *same = c;
            }
        } else {
            self.found.push(c);
        }
    }
}

/// How close two contacts' directions are to count as one ([`Contacts::add`]).
const SAME_DIR: f32 = 1e-3;

/// How far short of a contact's depth a push may stop and still count as clearing it.
const PUSH_SLACK: f32 = 1e-5;

/// The shortest horizontal push `p` with `p . dir >= depth` for every contact, if there is one
/// no longer than `cap`; `None` when the contacts push opposite ways or need a longer push.
/// The constraints are half-planes, so the answer is the point of their intersection nearest
/// the origin: the origin itself, the foot of one contact's line, or the corner of two; it is
/// unique, so it does not depend on the contacts' order. Before, each contact was applied in
/// turn and the last won along its own direction, so between two opposing faces the order
/// chose the side (#176).
fn least_push(contacts: &[Contact], cap: f32) -> Option<Vec2> {
    let clears = |p: Vec2| {
        contacts
            .iter()
            .all(|c| p.dot(c.dir) >= c.depth - PUSH_SLACK)
    };
    if clears(Vec2::ZERO) {
        return Some(Vec2::ZERO);
    }
    // No push is shorter than the deepest contact's depth.
    let deepest = contacts.iter().map(|c| c.depth).fold(0.0, f32::max);
    if deepest > cap || !in_half_circle(contacts) {
        return None;
    }
    // The foot of a line that clears the rest is the nearest point of a superset of the
    // intersection, so it is the answer; all such feet are equally near. Only the deepest
    // contacts' feet can clear the others.
    if let Some(c) = contacts
        .iter()
        .filter(|c| c.depth >= deepest - PUSH_SLACK)
        .find(|c| clears(c.dir * c.depth))
    {
        return Some(c.dir * c.depth);
    }
    let mut best = cap * cap;
    let mut found = None;
    for (i, a) in contacts.iter().enumerate() {
        for b in &contacts[i + 1..] {
            // A corner is no nearer than either of its lines' feet.
            if a.depth.max(b.depth).powi(2) > best {
                continue;
            }
            let det = a.dir.perp_dot(b.dir);
            if det.abs() < 1e-6 {
                continue;
            }
            // Where p . a.dir = a.depth and p . b.dir = b.depth.
            let p = Vec2::new(
                a.depth * b.dir.y - b.depth * a.dir.y,
                b.depth * a.dir.x - a.depth * b.dir.x,
            ) / det;
            if p.length_squared() <= best && clears(p) {
                best = p.length_squared();
                found = Some(p);
            }
        }
    }
    found
}

/// Whether the directions of the contacts that need a push (`depth > 0`) all lie within an
/// open half circle, as they must for any push to clear them all: a push moves the body away
/// from only the directions on one side of it. Faces around more than half the body (a wedge)
/// fail this at once, without [`least_push`]'s search over corners. More than [`MAX_ARC`]
/// contacts are not tested and pass.
fn in_half_circle(contacts: &[Contact]) -> bool {
    let mut angles = [0f32; MAX_ARC];
    let mut n = 0;
    for c in contacts.iter().filter(|c| c.depth > PUSH_SLACK) {
        if n == MAX_ARC {
            return true;
        }
        angles[n] = c.dir.y.atan2(c.dir.x);
        n += 1;
    }
    let angles = &mut angles[..n];
    if n < 2 {
        return true;
    }
    angles.sort_unstable_by(f32::total_cmp);
    // The widest gap between neighbouring directions, round the circle.
    let wrap = angles[0] + std::f32::consts::TAU - angles[n - 1];
    let widest = angles.windows(2).map(|w| w[1] - w[0]).fold(wrap, f32::max);
    widest > std::f32::consts::PI + 1e-4
}

/// The most contacts [`in_half_circle`] sorts.
const MAX_ARC: usize = 64;

/// The part of a sub-step `step` that goes into none of the faces of `contacts`, for a body
/// they wedge: the nearest move to `step` with `move . face >= 0` for each, which slides along
/// the wedge and is zero when the faces surround the body. The faces' normals, not the
/// closest-point directions, as a contact at the inner edge between two triangles of one flat
/// face points off the face and would forbid moving along it either way.
fn wedged_move(step: Vec2, contacts: &mut Contacts) -> Vec2 {
    let Contacts { found, toward, .. } = contacts;
    toward.clear();
    toward.extend(
        found
            .iter()
            .filter(|c| c.face != Vec2::ZERO)
            .map(|c| Contact {
                dir: c.face,
                depth: -step.dot(c.face),
                gap: 0.0,
                face: c.face,
            }),
    );
    // Not moving (`-step`) always clears these, so there is an answer.
    step + least_push(toward, f32::INFINITY).unwrap_or(-step)
}

/// The push for a body that was already wedged by `contacts` before it moved, so taking the
/// move back cannot free it (#176 review): the shortest push that clears them all, at most
/// [`RADIUS`] of it a sub-step, when there is one; otherwise the push that leaves them all
/// equally deep (a few rounds of moving by the mean of the remaining depths). Either is cut
/// short so the body never passes half way to any contact's triangle point, so it is never
/// pushed through a face it is in front of.
fn escape(contacts: &[Contact]) -> Vec2 {
    let mut push = least_push(contacts, f32::INFINITY).unwrap_or_else(|| {
        let mut p = Vec2::ZERO;
        for _ in 0..16 {
            let sum: Vec2 = contacts
                .iter()
                .map(|c| c.dir * (c.depth - p.dot(c.dir)).max(0.0))
                .sum();
            p += sum / contacts.len() as f32;
        }
        p
    });
    if push.length_squared() > RADIUS * RADIUS {
        push = push.normalize() * RADIUS;
    }
    let mut keep = 1.0f32;
    for c in contacts {
        let toward = -push.dot(c.dir);
        if toward > 0.5 * c.gap {
            keep = keep.min(0.5 * c.gap / toward);
        }
    }
    push * keep
}

/// The world as one kind of body sees it: which helper parts hold it.
struct View<'a> {
    world: &'a CollisionWorld,
    who: u8,
}

impl Ground for View<'_> {
    fn water_surface(&self, x: f32, z: f32, top: f32) -> Option<WaterSurface> {
        let w = self.world;
        let water = &w.columns.get(&cell_of(x, z))?.water;
        water
            .iter()
            .filter_map(|&t| {
                let material = w.materials[t as usize];
                if !w.live(t, self.who) {
                    return None;
                }
                let [a, b, c] = w.corners(t);
                let height = height_on(x, z, a, b, c)?;
                (height <= top).then_some(WaterSurface { height, material })
            })
            .max_by(|a, b| a.height.total_cmp(&b.height))
    }

    fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.floor(x, z, f32::INFINITY)
    }

    fn trigger_contact(&self, from: [f32; 3], to: [f32; 3]) -> Option<TriggerContact> {
        self.world.trigger_contact(from.into(), to.into())
    }

    fn near_trigger(&self, part: &str, feet: [f32; 3]) -> bool {
        self.world.near_trigger(part, feet.into())
    }

    fn fall_return_below(&self, x: f32, y: f32, z: f32) -> bool {
        self.world.fall_return_below(x, y, z)
    }

    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.floor_triangle(x, z, top).map(|(h, _)| h)
    }

    fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        View {
            world: self.world,
            who: self.who | crate::hit_filter::WALL_HUG,
        }
        .floor(x, z, top)
    }

    fn ceiling(&self, feet: [f32; 3], rise: f32) -> Option<f32> {
        self.world.ceiling_for(feet.into(), rise, self.who)
    }

    fn head_contact(&self, feet: [f32; 3]) -> bool {
        self.world.head_contact_for(feet.into(), self.who)
    }

    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.slide_assisted(feet, delta, 0)
    }

    fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
        self.world.slide_for(feet, delta, self.who, walls, true)
    }

    fn slide_fan(&self, feet: [f32; 3], deltas: [[f32; 2]; 8]) -> [[f32; 2]; 8] {
        self.world.slide_fan_for(feet, deltas, self.who)
    }

    fn max_step(&self) -> f32 {
        STEP_HEIGHT
    }

    fn material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        self.deep_water_over(x, z, top, self.floor(x, z, top))
            .or_else(|| self.world.floor_material_for(x, z, top, self.who))
    }

    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        let (h, t) = self.floor_triangle(x, z, top)?;
        let material = self
            .deep_water_over(x, z, top, Some(h))
            .or_else(|| self.world.triangle_material(t));
        Some((h, material))
    }
}

impl View<'_> {
    /// The highest walkable surface under the feet at `(x, z)` no higher than `top`: height and
    /// triangle. Feet are not a point: it looks a few centimetres around too, so seams between
    /// triangles do not drop the body.
    fn floor_triangle(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        const R: f32 = 0.08;
        let points =
            [(0.0, 0.0), (R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R)].map(|(dx, dz)| [x + dx, z + dz]);
        self.world
            .floor_triangles(points, top, self.who)
            .into_iter()
            .flatten()
            .reduce(|best, hit| if hit.0 > best.0 { hit } else { best })
    }

    /// The material of deep water over the floor `floor` at `(x, z)`: the body stands in that
    /// water rather than on the floor.
    fn deep_water_over(&self, x: f32, z: f32, top: f32, floor: Option<f32>) -> Option<u32> {
        self.water_surface(x, z, top)
            .filter(|water| water.deep() && floor.is_none_or(|floor| floor < water.height))
            .map(|water| water.material)
    }
}

/// Implements [`Ground`] for a type by forwarding every query to its [`View`].
macro_rules! ground_via_view {
    ($ty:ty, $view:ident) => {
        impl Ground for $ty {
            fn water_surface(&self, x: f32, z: f32, top: f32) -> Option<WaterSurface> {
                self.$view().water_surface(x, z, top)
            }

            fn height(&self, x: f32, z: f32) -> Option<f32> {
                self.$view().height(x, z)
            }

            fn lowest_floor(&self, x: f32, z: f32) -> Option<f32> {
                self.$view().lowest_floor(x, z)
            }

            fn trigger_contact(&self, from: [f32; 3], to: [f32; 3]) -> Option<TriggerContact> {
                self.$view().trigger_contact(from, to)
            }

            fn near_trigger(&self, part: &str, feet: [f32; 3]) -> bool {
                self.$view().near_trigger(part, feet)
            }

            fn fall_return_below(&self, x: f32, y: f32, z: f32) -> bool {
                self.$view().fall_return_below(x, y, z)
            }

            fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
                self.$view().floor(x, z, top)
            }

            fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
                self.$view().hug_floor(x, z, top)
            }

            fn ceiling(&self, feet: [f32; 3], rise: f32) -> Option<f32> {
                self.$view().ceiling(feet, rise)
            }

            fn head_contact(&self, feet: [f32; 3]) -> bool {
                self.$view().head_contact(feet)
            }

            fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
                self.$view().slide(feet, delta)
            }

            fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
                self.$view().slide_assisted(feet, delta, walls)
            }

            fn slide_fan(&self, feet: [f32; 3], deltas: [[f32; 2]; 8]) -> [[f32; 2]; 8] {
                self.$view().slide_fan(feet, deltas)
            }

            fn max_step(&self) -> f32 {
                self.$view().max_step()
            }

            fn material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
                self.$view().material(x, z, top)
            }

            fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
                self.$view().floor_with_material(x, z, top)
            }
        }
    };
}

impl CollisionWorld {
    /// The world itself is the player's view.
    fn player_view(&self) -> View<'_> {
        View {
            world: self,
            who: PLAYER,
        }
    }
}

ground_via_view!(CollisionWorld, player_view);


impl<T: Ground + ?Sized> Ground for std::sync::Arc<T> {
    fn water_surface(&self, x: f32, z: f32, top: f32) -> Option<WaterSurface> {
        (**self).water_surface(x, z, top)
    }
    fn height(&self, x: f32, z: f32) -> Option<f32> {
        (**self).height(x, z)
    }

    fn lowest_floor(&self, x: f32, z: f32) -> Option<f32> {
        (**self).lowest_floor(x, z)
    }

    fn trigger_contact(&self, from: [f32; 3], to: [f32; 3]) -> Option<TriggerContact> {
        (**self).trigger_contact(from, to)
    }

    fn near_trigger(&self, part: &str, feet: [f32; 3]) -> bool {
        (**self).near_trigger(part, feet)
    }

    fn fall_return_below(&self, x: f32, y: f32, z: f32) -> bool {
        (**self).fall_return_below(x, y, z)
    }

    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        (**self).floor(x, z, top)
    }

    fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        (**self).hug_floor(x, z, top)
    }

    fn ceiling(&self, feet: [f32; 3], rise: f32) -> Option<f32> {
        (**self).ceiling(feet, rise)
    }

    fn head_contact(&self, feet: [f32; 3]) -> bool {
        (**self).head_contact(feet)
    }

    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        (**self).slide(feet, delta)
    }

    fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
        (**self).slide_assisted(feet, delta, walls)
    }

    fn slide_fan(&self, feet: [f32; 3], deltas: [[f32; 2]; 8]) -> [[f32; 2]; 8] {
        (**self).slide_fan(feet, deltas)
    }

    fn max_step(&self) -> f32 {
        (**self).max_step()
    }

    fn material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        (**self).material(x, z, top)
    }

    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        (**self).floor_with_material(x, z, top)
    }
}

/// Height of the triangle where the vertical line through `(x, z)` crosses it.
fn height_on(x: f32, z: f32, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    const EPS: f32 = -1e-5;
    if l1 < EPS || l2 < EPS || l3 < EPS {
        return None;
    }
    Some(l1 * a.y + l2 * b.y + l3 * c.y)
}

/// [`CollisionWorld::slide_fan_for`]'s scratch, kept between fans so none allocates: the
/// shared gather's triangles, each cell read with its share of them, one slide's triangles,
/// and the push-out contacts.
struct FanScratch {
    pool: Vec<u32>,
    cells: Vec<((i32, i32), std::ops::Range<usize>)>,
    tris: Vec<u32>,
    contacts: Contacts,
}

impl FanScratch {
    const fn new() -> Self {
        Self {
            pool: Vec::new(),
            cells: Vec::new(),
            tris: Vec::new(),
            contacts: Contacts::new(),
        }
    }
}

/// What a slide ([`CollisionWorld::gather_slide`]) can touch.
#[derive(Clone)]
struct SlideReach {
    /// The slide's coarse sub-step count.
    coarse: usize,
    /// The box of the cells to read: the move padded by a body radius and a sub-step.
    min: Vec2,
    max: Vec2,
    /// The horizontal keep box, `None` without the cull: the move's reach plus the move's length
    /// and a body radius more. Push-outs carry the body off the straight line, along a slanted
    /// wall for one: each sub-step's push only undoes what that sub-step pushed into a wall, so
    /// together they drift it by at most about the move's length, plus up to a radius for a body
    /// that starts pressed into a wall. Within that box the body meets every triangle it can
    /// touch. A long probe (`docking::wall_contact`'s eight 0.8 m slides) tested about 3k far
    /// triangles on each pass without it, about 20 ms a step (#161).
    near: Option<[Vec2; 2]>,
    /// The body's spheres' height band at the start, padded so roundoff cannot drop a contact.
    body: [f32; 2],
    /// The heights a fall-prevention wall's box must reach into to hold the feet.
    feet: [f32; 2],
}

impl SlideReach {
    fn new(start: Vec3, d: Vec2, cull: bool) -> Self {
        let len = d.length();
        let coarse = ((len / (RADIUS * 0.5)).ceil() as usize).clamp(1, 32);
        let reach = WALL_REACH + len / coarse as f32 + 0.05;
        let min = Vec2::new(start.x.min(start.x + d.x), start.z.min(start.z + d.y)) - reach;
        let max = Vec2::new(start.x.max(start.x + d.x), start.z.max(start.z + d.y)) + reach;
        let pad = WALL_REACH + len;
        Self {
            coarse,
            min,
            max,
            near: cull.then_some([min - pad, max + pad]),
            body: [
                start.y + SPHERES[0] - WALL_REACH - 0.01,
                start.y + SPHERES[SPHERES.len() - 1] + WALL_REACH + 0.01,
            ],
            feet: [start.y - FALL_ASSIST_RADIUS, start.y + FALL_ASSIST_RADIUS],
        }
    }

    /// Whether a triangle with box `lo`..`hi` comes into the keep box.
    fn near(&self, lo: Vec3, hi: Vec3) -> bool {
        self.near.is_none_or(|[min, max]| {
            hi.x >= min.x && lo.x <= max.x && hi.z >= min.y && lo.z <= max.y
        })
    }
}

/// Where [`CollisionWorld::ceiling_for`] sweeps the head sphere.
struct CeilingSweep {
    /// The sphere's centre's height at the start.
    centre: f32,
    /// The sphere's top's height after the rise.
    high: f32,
    /// The sphere's centre line, seen from above.
    line: Vec2,
    /// The box of the cells to read.
    min: Vec2,
    max: Vec2,
}

impl CeilingSweep {
    fn new(feet: Vec3, rise: f32) -> Self {
        let line = Vec2::new(feet.x, feet.z);
        Self {
            centre: feet.y + HEAD_TOP - RADIUS,
            high: feet.y + HEAD_TOP + rise,
            line,
            min: line - RADIUS,
            max: line + RADIUS,
        }
    }
}

/// The sub-steps of a slide of length `len` whose coarse count is `coarse`: with
/// fall-prevention walls near it, sub-steps of half the smallest clearance instead, so no wall
/// is stepped across.
fn slide_steps(coarse: usize, len: f32, no_walls: bool) -> usize {
    if no_walls {
        coarse
    } else {
        ((len / (FALL_ASSIST_RADIUS * 0.5)).ceil() as usize).clamp(1, 64)
    }
}

/// M??ller-Trumbore, both faces.
/// Returns the ray parameter `t >= 0` where `o + t * d` meets triangle `abc`.
pub fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}

/// The furthest off the head sphere's centre line, seen from above, a touch that stops a rise
/// can be: where [`CEILING_UP`] puts it on the sphere's surface.
fn ceiling_reach() -> f32 {
    RADIUS * (1.0 - CEILING_UP * CEILING_UP).sqrt()
}

/// How far a sphere of [`RADIUS`] at `centre` moves along the unit `dir` before it first
/// touches the triangle `corners` of plane normal `n` (either way round): the least distance
/// at which it meets the face's interior, one of its edges or one of its corners, negative when
/// it touches it already (where along `dir` the touch began). `None` when moving on along `dir`
/// never touches it. The same narrow phase as [`closest_on_triangle`], swept: a straight-up
/// sweep is the ceiling stop ([`CollisionWorld::ceiling_for`]), any other the head sweep of a
/// wire flight ([`CollisionWorld::head_sweep`]).
fn first_touch(centre: Vec3, dir: Vec3, corners: [Vec3; 3], n: Vec3) -> Option<f32> {
    let [a, b, c] = corners;
    let r2 = RADIUS * RADIUS;
    let mut first = f32::INFINITY;
    // The interior: from wholly behind the face's plane up to touching it.
    let n = if n.dot(dir) < 0.0 { -n } else { n };
    let behind = n.dot(centre - a);
    let closing = n.dot(dir);
    if behind <= -RADIUS && closing > 1e-6 {
        let s = (-RADIUS - behind) / closing;
        if in_triangle(centre + dir * s + n * RADIUS, corners, n) {
            first = s;
        }
    }
    // The edges: the line of the move meets the cylinder of RADIUS round each, between its ends.
    for (p, e) in [(a, b - a), (b, c - b), (c, a - c)] {
        let ee = e.length_squared();
        let m = centre - p;
        let m_off = m - e * (m.dot(e) / ee);
        let d_off = dir - e * (e.dot(dir) / ee);
        let aa = d_off.length_squared();
        let bb = m_off.dot(d_off);
        let disc = bb * bb - aa * (m_off.length_squared() - r2);
        // An edge along the move is met first at a corner.
        if aa < 1e-9 || disc <= 0.0 {
            continue;
        }
        let root = disc.sqrt();
        let s = (-bb - root) / aa;
        let along = (m.dot(e) + s * e.dot(dir)) / ee;
        if -bb + root > 0.0 && (0.0..=1.0).contains(&along) {
            first = first.min(s);
        }
    }
    // The corners.
    for p in corners {
        let m = p - centre;
        let ahead = m.dot(dir);
        let off2 = (m.length_squared() - ahead * ahead).max(0.0);
        if off2 < r2 {
            let half = (r2 - off2).sqrt();
            if ahead + half > 0.0 {
                first = first.min(ahead - half);
            }
        }
    }
    first.is_finite().then_some(first)
}

/// Whether `p`, on the plane of the triangle `corners` with normal `n`, lies inside it: the
/// barycentric test of [`height_on`], on the plane seen along its normal's largest axis.
fn in_triangle(p: Vec3, corners: [Vec3; 3], n: Vec3) -> bool {
    let n = n.abs();
    let flat = |v: Vec3| {
        if n.y >= n.x && n.y >= n.z {
            Vec3::new(v.x, v.y, v.z)
        } else if n.x >= n.z {
            Vec3::new(v.z, v.x, v.y)
        } else {
            Vec3::new(v.x, v.z, v.y)
        }
    };
    let [a, b, c] = corners.map(flat);
    let p = flat(p);
    height_on(p.x, p.z, a, b, c).is_some()
}

/// Closest point on triangle `abc` to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
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

#[path = "arena_c_collision.rs"]
mod arena_c_collision;

#[cfg(test)]
#[path = "collision_grid_tests.rs"]
mod grid_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{Body, BodyInput};

    /// A floor at y 0, a 0.3 m step up at z < -4, a 1.5 m ledge at z < -8 and a wall at x = 3.
    fn test_world() -> CollisionWorld {
        let mut m = CollisionMesh::default();
        let mut quad = |p: [[f32; 3]; 4]| {
            let base = m.vertices.len() as u32;
            m.vertices.extend_from_slice(&p);
            m.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            m.materials.extend([0, 0]);
        };
        let floor = |y: f32, z0: f32, z1: f32| {
            [[-10.0, y, z0], [10.0, y, z0], [10.0, y, z1], [-10.0, y, z1]]
        };
        quad(floor(0.0, 10.0, -4.0));
        quad(floor(0.3, -4.0, -8.0));
        quad(floor(1.5, -8.0, -20.0));
        // Riser faces.
        quad([
            [-10.0, 0.0, -4.0],
            [10.0, 0.0, -4.0],
            [10.0, 0.3, -4.0],
            [-10.0, 0.3, -4.0],
        ]);
        quad([
            [-10.0, 0.3, -8.0],
            [10.0, 0.3, -8.0],
            [10.0, 1.5, -8.0],
            [-10.0, 1.5, -8.0],
        ]);
        // A wall along x = 3.
        quad([
            [3.0, -1.0, 10.0],
            [3.0, -1.0, -20.0],
            [3.0, 5.0, -20.0],
            [3.0, 5.0, 10.0],
        ]);
        CollisionWorld::from_mesh(&m)
    }

    fn walk(body: &mut Body, world: &CollisionWorld, local: [f32; 2], steps: usize) {
        for _ in 0..steps {
            body.step(
                &BodyInput {
                    // Source space: forward is -Z, and X is mirrored into Bevy space.
                    root_motion: [-local[0], 0.0, local[1], 0.0],
                    ..BodyInput::default()
                },
                1.0 / 60.0,
                world,
            );
        }
    }

    #[test]
    fn steps_up_small_ledges_but_not_tall_ones() {
        let world = test_world();
        let mut b = Body::default();
        // Walk forward (-Z) 0.08 m per tick for 2.5 s.
        walk(&mut b, &world, [0.0, -0.08], 150);
        // The step's top pushed out by the mesh's convex radius.
        assert!(
            (b.position[1] - 0.3 - MESH_CONVEX_RADIUS).abs() < 1e-4,
            "on the step: {:?}",
            b.position
        );
        assert!(b.grounded);
        // Stopped by the 1.2 m riser at z = -8, a radius, the keep distance and the convex
        // radius away.
        assert!(
            (b.position[2] + 8.0 - WALL_REACH).abs() < 0.05,
            "{:?}",
            b.position
        );
    }

    #[test]
    fn slides_along_walls() {
        let world = test_world();
        let mut b = Body::default();
        // Diagonal toward +X and -Z: the wall at x = 3 stops X, Z keeps going.
        walk(&mut b, &world, [0.06, -0.03], 100);
        assert!(
            (b.position[0] - (3.0 - WALL_REACH)).abs() < 0.03,
            "{:?}",
            b.position
        );
        assert!(b.position[2] < -2.5, "{:?}", b.position);
    }

    #[test]
    fn walks_off_edges_and_falls() {
        let world = test_world();
        let mut b = Body {
            position: [0.0, 1.5, -12.0],
            ..Body::default()
        };
        // Walk back (+Z) off the ledge at z = -8.
        walk(&mut b, &world, [0.0, 0.08], 90);
        assert!(b.grounded);
        assert!(
            (b.position[1] - 0.3 - MESH_CONVEX_RADIUS).abs() < 1e-4,
            "{:?}",
            b.position
        );
        assert!(b.last_fall_height > 1.0, "fell {}", b.last_fall_height);
    }

    #[test]
    fn raycast_hits_wall() {
        let world = test_world();
        let d = world.raycast(Vec3::new(0.0, 1.0, 0.0), Vec3::X, 10.0);
        assert!((d.unwrap() - 3.0).abs() < 1e-4);
    }

    #[test]
    fn ray_cell_traversal_matches_all_triangles_at_boundaries() {
        let world = test_world();
        for origin in [
            Vec3::new(-9.0, 1.0, -9.0),
            Vec3::new(8.0, 1.0, -8.0),
            Vec3::new(4.0, 1.0, 4.0),
            Vec3::new(-4.0, 1.0, -4.0),
        ] {
            for direction in [
                Vec3::X,
                -Vec3::X,
                Vec3::Z,
                -Vec3::Z,
                Vec3::X + Vec3::Z,
                Vec3::X - Vec3::Z,
                -Vec3::X + Vec3::Z,
                -Vec3::X - Vec3::Z,
                -Vec3::Y,
                // Sloped rays meet the floors and risers in a later cell than they start in,
                // which each cell's height band must keep.
                Vec3::new(0.4, -0.15, -1.0),
                Vec3::new(-1.0, -0.1, 0.3),
                Vec3::new(0.7, 0.05, -0.6),
            ] {
                let dir = direction.normalize();
                for max in [0.1, 25.0] {
                    let expected = (0..world.triangles.len() as u32)
                        .filter(|&t| world.live(t, PLAYER))
                        .filter_map(|t| {
                            let [a, b, c] = world.corners(t);
                            ray_triangle(origin, dir, a, b, c)
                        })
                        .filter(|&d| d <= max)
                        .min_by(f32::total_cmp);
                    assert_eq!(
                        world.raycast(origin, dir, max),
                        expected,
                        "{origin:?} {dir:?} max {max}"
                    );
                }
            }
        }
    }

    #[test]
    fn floor_material_is_the_highest_floor_s() {
        // A cobblestone floor (1) under a wooden one (4) 1 m higher.
        let mut m = CollisionMesh::default();
        for (y, material) in [(0.0, 1), (1.0, 4)] {
            let base = m.vertices.len() as u32;
            m.vertices.extend_from_slice(&[
                [-5.0, y, -5.0],
                [5.0, y, -5.0],
                [5.0, y, 5.0],
                [-5.0, y, 5.0],
            ]);
            m.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            m.materials.extend([material, material]);
        }
        let world = CollisionWorld::from_mesh(&m);
        assert_eq!(world.floor_material(0.0, 0.0, 2.0), Some(4));
        assert_eq!(world.floor_material(0.0, 0.0, 0.5), Some(1));
        assert_eq!(Ground::material(&world, 0.0, 0.0, 0.5), Some(1));
        assert_eq!(world.floor_material(9.0, 0.0, 2.0), None);
    }

    /// A beam 0.25 m above the head, whose edge is 0.2 m out, inside the head sphere's rim but
    /// clear of its centre line, stops the rise where the sphere meets that edge (#170); 0.3 m
    /// out, past [`ceiling_reach`], it only meets the sphere's shoulder and does not.
    #[test]
    fn a_beam_edge_over_the_head_rim_stops_the_rise() {
        let y = HEAD_TOP + 0.25;
        let mut m = CollisionMesh::default();
        m.vertices.extend_from_slice(&[
            [-2.0, y, -2.0],
            [2.0, y, -2.0],
            [2.0, y, -0.2],
            [-2.0, y, -0.2],
        ]);
        m.indices.extend([0, 1, 2, 0, 2, 3]);
        m.materials.extend([0, 0]);
        let world = CollisionWorld::from_mesh(&m);
        let top = world.ceiling_for(Vec3::ZERO, 1.0, PLAYER);
        let want = y - (RADIUS * RADIUS - 0.2 * 0.2).sqrt() - (HEAD_TOP - RADIUS);
        assert!(
            top.is_some_and(|t| (t - want).abs() < 1e-4),
            "{top:?}, want {want}"
        );
        for z in [0.1, 0.21] {
            assert_eq!(
                world.ceiling_for(Vec3::new(0.0, 0.0, z), 1.0, PLAYER),
                None,
                "{z}"
            );
        }
    }

    /// A world of the given quads, each two triangles.
    fn quads(quads: &[[[f32; 3]; 4]]) -> CollisionWorld {
        let mut m = CollisionMesh::default();
        for q in quads {
            let base = m.vertices.len() as u32;
            m.vertices.extend_from_slice(q);
            m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
            m.materials.extend([0, 0]);
        }
        CollisionWorld::from_mesh(&m)
    }

    /// An underside sloping down away from the centre line, its near edge 0.1 m out, meets the
    /// head sphere first on its face, before the edge nearest the line would.
    #[test]
    fn a_sloped_underside_stops_the_rise_at_its_first_touch() {
        let (y0, k) = (HEAD_TOP + 0.3, 0.7);
        let y = |x: f32| y0 - k * (x - 0.1);
        let world = quads(&[[
            [0.1, y(0.1), -3.0],
            [3.0, y(3.0), -3.0],
            [3.0, y(3.0), 3.0],
            [0.1, y(0.1), 3.0],
        ]]);
        let head = HEAD_TOP - RADIUS;
        let face = y0 + 0.1 * k - RADIUS * (1.0 + k * k).sqrt() - head;
        let edge = y0 - (RADIUS * RADIUS - 0.1 * 0.1).sqrt() - head;
        assert!(face < edge - 0.02, "{face} {edge}");
        let top = world.ceiling_for(Vec3::ZERO, 1.0, PLAYER);
        assert!(
            top.is_some_and(|t| (t - face).abs() < 1e-4),
            "{top:?}, want {face}"
        );
    }

    /// The top of a wall the slide leaves the body pressed against, its edge at the head
    /// sphere's rim, does not stop a jump up past it (ledge grabs and wall jumps); within
    /// [`ceiling_reach`] of the centre line, it does.

    #[test]
    fn a_wall_top_at_the_rim_does_not_stop_the_rise() {
        let y = 2.0;
        let world = quads(&[
            [
                [-3.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [3.0, 0.0, 3.0],
                [-3.0, 0.0, 3.0],
            ],
            [
                [-3.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [3.0, y, 0.0],
                [-3.0, y, 0.0],
            ],
            [
                [-3.0, y, -3.0],
                [3.0, y, -3.0],
                [3.0, y, 0.0],
                [-3.0, y, 0.0],
            ],
        ]);
        let [dx, dz] = world.slide([0.3, 0.0, 1.0], [0.0, -1.0]);
        let pressed = Vec3::new(0.3 + dx, 0.0, 1.0 + dz);
        assert!((pressed.z - WALL_REACH).abs() < 1e-3, "{pressed}");
        for feet in [pressed, Vec3::new(0.3, 0.0, RADIUS)] {
            assert_eq!(world.ceiling_for(feet, 1.0, PLAYER), None, "{feet}");
        }
        assert!(
            world
                .ceiling_for(Vec3::new(0.3, 0.0, 0.2), 1.0, PLAYER)
                .is_some()
        );
    }

    /// A fall-assist part whose triangles or vertices lie outside the mesh is an error naming
    /// it, not a panic or a silent gap; the parts `keep` leaves out add nothing.
    #[test]
    fn fall_assist_parts_outside_the_mesh_are_errors() {
        let mut mesh = CollisionMesh::default();
        mesh.vertices
            .extend([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        mesh.indices.extend([0, 1, 2, 0, 1, 7]);
        mesh.materials.extend([0, 0]);
        let part = |name: &str, first, count| sekiro_formats::hknp::PartRange {
            name: name.into(),
            first,
            count,
        };
        let walls = PartCollision {
            parts: vec![
                part("good", 0, 1),
                part("bad vertex", 1, 1),
                part("past the end", 1, 5),
            ],
            mesh,
        };
        let add = |keep: &str| {
            CollisionWorld::from_mesh(&CollisionMesh::default())
                .add_fall_assist(&walls, |n| n == keep)
        };
        assert_eq!(add("good"), Ok(1));
        assert_eq!(add("none"), Ok(0));
        for bad in ["bad vertex", "past the end"] {
            assert!(add(bad).is_err_and(|e| e.contains(bad)), "{bad}");
        }
    }

    /// The [`Ground`] methods `source` defines (`fn name(` at the given indentation).
    fn ground_methods(source: &str, indent: &str) -> Vec<String> {
        let start = format!("{indent}fn ");
        source
            .lines()
            .filter_map(|line| line.strip_prefix(start.as_str()))
            .filter_map(|rest| rest.split(['(', '<']).next())
            .map(str::to_owned)
            .collect()
    }

    /// Issue #169: the two full forwarders that cannot use [`Ground::inner`] (the views built
    /// per query, and `Arc`, whose ground may be unsized) forward every [`Ground`] method, so a
    /// new method cannot silently fall back to its default through them.
    #[test]
    fn full_ground_forwarders_forward_every_method() {
        // Each block ends at the first line that is just a closing brace.
        let block = |source: &'static str, from: &str| {
            let start = source.find(from).unwrap();
            &source[start..start + source[start..].find("\n}").unwrap()]
        };
        let mut methods =
            ground_methods(block(include_str!("body.rs"), "pub trait Ground {"), "    ");
        methods.retain(|m| m != "inner");
        assert!(methods.len() > 10, "{methods:?}");
        let source = include_str!("collision.rs");
        for (name, block, indent) in [
            (
                "ground_via_view!",
                block(source, "macro_rules! ground_via_view"),
                "            ",
            ),
            (
                "Arc",
                block(
                    source,
                    "impl<T: Ground + ?Sized> Ground for std::sync::Arc<T>",
                ),
                "    ",
            ),
        ] {
            let forwarded = ground_methods(block, indent);
            for m in &methods {
                assert!(forwarded.contains(m), "{name} does not forward Ground::{m}");
            }
        }
    }
}
