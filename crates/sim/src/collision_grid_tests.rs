//! The height-banded grid (#163) finds, for every query, exactly the triangles a brute-force
//! scan of the query's cells keeps with the same predicates, on the play world's real
//! collision.
use std::collections::HashMap;

use rayon::prelude::*;

use super::*;
use crate::hit_filter::{FALL_ASSIST_ACTION, FALL_ASSIST_MOVE, RIDE_ASSIST_MOVE};

/// Every triangle of each cell the grid files it in ([`filed_cells`]), at every height and for
/// everyone: what the brute-force answers scan.
struct Cells<'a> {
    world: &'a CollisionWorld,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl<'a> Cells<'a> {
    fn new(world: &'a CollisionWorld) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (t, &[lo, hi]) in world.bounds.iter().enumerate() {
            let swim = world.in_swim_hit(t as u32);
            let floor = files_as_floor(
                world.normals[t],
                world.materials[t],
                world.min_walk_ny,
                swim,
            );
            for cell in filed_cells(lo, hi, floor) {
                cells.entry(cell).or_default().push(t as u32);
            }
        }
        Self { world, cells }
    }

    /// The triangles of the cells from the one holding `min` to the one holding `max`, sorted,
    /// each once.
    fn between(&self, min: Vec2, max: Vec2) -> Vec<u32> {
        let (x0, z0) = cell_of(min.x, min.y);
        let (x1, z1) = cell_of(max.x, max.y);
        let mut out: Vec<u32> = (x0..=x1)
            .flat_map(|cx| (z0..=z1).map(move |cz| (cx, cz)))
            .filter_map(|key| self.cells.get(&key))
            .flatten()
            .copied()
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The highest walkable surface under `(x, z)` no higher than `top`, the first triangle's
    /// of equally high ones.
    fn floor(&self, x: f32, z: f32, top: f32, who: u8) -> Option<(f32, u32)> {
        let w = self.world;
        let mut best: Option<(f32, u32)> = None;
        for t in self.between(Vec2::new(x, z), Vec2::new(x, z)) {
            let [a, b, c] = w.corners(t);
            if w.walkable(t, who)
                && let Some(h) = height_on(x, z, a, b, c)
                    .map(|h| h + w.skin(t) / w.normals[t as usize].y.abs())
                && h <= top
                && best.is_none_or(|(b, _)| h > b)
            {
                best = Some((h, t));
            }
        }
        best
    }

    /// The nearest hit within `max` on any solid triangle `who` collides with, among those of
    /// every cell the segment's box reaches.
    fn ray(&self, origin: Vec3, dir: Vec3, max: f32, who: u8) -> Option<f32> {
        let w = self.world;
        let end = origin + dir * max;
        let (lo, hi) = (origin.min(end), origin.max(end));
        self.between(Vec2::new(lo.x, lo.z), Vec2::new(hi.x, hi.z))
            .into_iter()
            .filter(|&t| {
                let [a, b] = w.bounds[t as usize];
                a.cmple(hi).all() && b.cmpge(lo).all() && w.solid(t) && w.live(t, who)
            })
            .filter_map(|t| {
                let [a, b, c] = w.corners(t);
                ray_triangle(origin, dir, a, b, c).filter(|&d| d <= max)
            })
            .min_by(f32::total_cmp)
    }
}

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `[lo, hi)`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[(self.next() % of.len() as u64) as usize]
    }
}

/// Every query at many places near the play world's triangles (picked uniformly, so mostly in
/// the dense cells, a third of them on triangles spanning several bands and a sixth on
/// fall-prevention walls), at heights from below to above each, reads exactly the triangles the
/// brute-force scan keeps: the slides and ceiling sweeps the same sets (the sweeps each once),
/// the floors, water and surfaces the same answers to the bit, the rays the same distance. The
/// world has its fall-prevention walls and an event wall that holds both the assist and enemies.
#[test]
fn banded_grid_reads_what_a_brute_force_scan_keeps() {
    let cache = crate::cache::root();
    let Some(mut world) = CollisionWorld::load_blocks(&cache, &["m11_00_00_00", "m11_01_00_00"])
    else {
        eprintln!("skipping: no m11 collision in {}", cache.display());
        return;
    };
    let wall = CollisionMesh {
        vertices: vec![
            [-257.0, -50.0, 6.0],
            [-253.0, -50.0, 6.0],
            [-253.0, -40.0, 6.0],
            [-257.0, -40.0, 6.0],
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        materials: vec![0, 0],
    };
    world.append_event_part(1, &wall, FALL_ASSIST_MOVE | ENEMY, true);
    let world = &world;
    let cells = Cells::new(world);
    // Heap bytes of the lists and the maps' entries.
    let mb = |bytes: usize| bytes as f64 / (1 << 20) as f64;
    let bytes: usize = world
        .columns
        .values()
        .map(|c| {
            c.rest.bytes()
                + c.floors.bytes()
                + c.water.len() * 4
                + size_of::<((i32, i32), Column)>()
        })
        .sum();
    let rest: usize = world.columns.values().map(|c| c.rest.entries().len()).sum();
    let floors: usize = world
        .columns
        .values()
        .map(|c| c.floors.entries().len())
        .sum();
    eprintln!(
        "{} triangles; {} cells, {rest} entries and {floors} floor entries, {:.1} MB",
        world.triangle_count(),
        world.columns.len(),
        mb(bytes),
    );
    let triangles = 0..world.triangle_count() as u32;
    let tall: Vec<u32> = triangles
        .clone()
        .filter(|&t| {
            let [lo, hi] = world.bounds[t as usize];
            hi.y - lo.y > 2.0 * BAND
        })
        .collect();
    assert!(tall.len() > 1000, "{} tall triangles", tall.len());
    let assist: Vec<u32> = triangles
        .filter(|&t| world.live(t, FALL_ASSIST_MOVE))
        .collect();
    assert!(assist.len() > 1000, "{} fall-assist walls", assist.len());
    let all = world.triangle_count() as u64;
    let mismatches: Vec<String> = (0..12_000u64)
        .into_par_iter()
        .flat_map_iter(|i| {
            let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (i + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9));
            let t = match i % 6 {
                0 | 3 => rng.pick(&tall),
                1 => rng.pick(&assist),
                _ => (rng.next() % all) as u32,
            };
            let [lo, hi] = world.bounds[t as usize];
            let [mut x, mut z] = [
                rng.range(lo.x - 1.0, hi.x + 1.0),
                rng.range(lo.z - 1.0, hi.z + 1.0),
            ];
            // Some on a corner, where the triangles meeting there give equal floors.
            if i % 5 == 1 {
                let corner = world.corners(t)[(i % 3) as usize];
                [x, z] = [corner.x, corner.z];
            }
            let y = rng.range(lo.y - 3.0, hi.y + 3.0);
            let who = rng.pick(&[PLAYER, ENEMY, CAMERA]);
            let mut out = Vec::new();
            let mut check = |what: &str, same: bool, detail: String| {
                if !same {
                    out.push(format!("{what} at ({x}, {y}, {z}) for {who}: {detail}"));
                }
            };
            let bits = |f: Option<(f32, u32)>| f.map(|(h, t)| (h.to_bits(), t));
            for top in [y, y + rng.range(0.0, 0.6), f32::INFINITY] {
                let (a, b) = (
                    world.floor_triangle(x, z, top, who),
                    cells.floor(x, z, top, who),
                );
                check(
                    "floor",
                    bits(a) == bits(b),
                    format!("top {top}: {a:?} vs {b:?}"),
                );
                // A body's five points, read in one pass.
                const R: f32 = 0.08;
                let a = View { world, who }.floor_triangle(x, z, top);
                let b = [(0.0, 0.0), (R, 0.0), (-R, 0.0), (0.0, R), (0.0, -R)]
                    .into_iter()
                    .filter_map(|(dx, dz)| cells.floor(x + dx, z + dz, top, who))
                    .reduce(|best, hit| if hit.0 > best.0 { hit } else { best });
                check(
                    "feet",
                    bits(a) == bits(b),
                    format!("top {top}: {a:?} vs {b:?}"),
                );
            }
            let feet = Vec3::new(x, y, z);
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let length = rng.pick(&[0.0, 0.05, 0.13, 0.4, 0.8, 2.0]) * rng.range(0.5, 1.0);
            let d = Vec2::new(angle.sin(), angle.cos()) * length;
            let assist = rng.pick(&[
                0,
                FALL_ASSIST_MOVE,
                FALL_ASSIST_ACTION,
                FALL_ASSIST_MOVE | RIDE_ASSIST_MOVE,
                FALL_ASSIST_MOVE | PLAYER,
            ]);
            let cull = i % 8 != 0;
            let (mut body, mut walls) = (Vec::new(), Vec::new());
            world.gather_slide(feet, d, [who, assist], &mut body, &mut walls, cull);
            let reach = SlideReach::new(feet, d, cull);
            let near = cells.between(reach.min, reach.max);
            let keep = |f: &dyn Fn(u32) -> bool| -> Vec<u32> {
                near.iter().copied().filter(|&t| f(t)).collect()
            };
            let brute = keep(&|t| world.slide_body_keeps(t, &reach, who));
            check(
                "slide body",
                body == brute,
                format!("{d} cull {cull}: {body:?} vs {brute:?}"),
            );
            let brute = keep(&|t| world.slide_wall_keeps(t, &reach, assist & !BODIES));
            check(
                "slide walls",
                walls == brute,
                format!("{d} assist {assist}: {walls:?} vs {brute:?}"),
            );
            let rise = rng.range(0.0, 3.0);
            let mut swept = Vec::new();
            world.ceiling_candidates(feet, rise, who, |t| swept.push(t));
            // Not deduplicated: each must come once.
            swept.sort_unstable();
            let sweep = CeilingSweep::new(feet, rise);
            let brute: Vec<u32> = cells
                .between(sweep.min, sweep.max)
                .into_iter()
                .filter(|&t| world.ceiling_keeps(t, &sweep, who))
                .collect();
            check(
                "ceiling",
                swept == brute,
                format!("rise {rise}: {swept:?} vs {brute:?}"),
            );
            if i % 2 == 0 {
                let origin = Vec3::new(x, y + rng.range(0.0, 2.0), z);
                let dir = Vec3::new(
                    rng.range(-1.0, 1.0),
                    rng.range(-0.6, 0.6),
                    rng.range(-1.0, 1.0),
                )
                .normalize_or(Vec3::X);
                let max = rng.pick(&[0.5, 4.0, 25.0, 60.0]);
                let (a, b) = (
                    world.ray(origin, dir, max, who).0,
                    cells.ray(origin, dir, max, who),
                );
                check(
                    "ray",
                    // Equal, 0 and -0 alike: the walk meets equally near hits in another order.
                    a == b,
                    format!("{origin} {dir} {max}: {a:?} vs {b:?}"),
                );
            }
            let here = cells.between(Vec2::new(x, z), Vec2::new(x, z));
            let brute = here
                .iter()
                .copied()
                .filter(|&t| {
                    // Shallow water anywhere, deep water only as a swim hit's plane.
                    let m = world.materials[t as usize];
                    let deep_dry = crate::body::deep_water(m) && !world.in_swim_hit(t);
                    water_material(m) && !deep_dry && world.live(t, who)
                })
                .filter_map(|t| {
                    let [a, b, c] = world.corners(t);
                    let height = height_on(x, z, a, b, c).filter(|&h| h <= y)?;
                    let material = world.materials[t as usize];
                    Some(WaterSurface { height, material })
                })
                .max_by(|a, b| a.height.total_cmp(&b.height));
            let banded = View { world, who }.water_surface(x, z, y);
            check("water", banded == brute, format!("{banded:?} vs {brute:?}"));
            if i % 16 == 0 {
                let mut brute: Vec<(f32, f32)> = here
                    .iter()
                    .copied()
                    .filter(|&t| world.live(t, PLAYER))
                    .filter_map(|t| {
                        let [a, b, c] = world.corners(t);
                        Some((height_on(x, z, a, b, c)?, world.normals[t as usize].y))
                    })
                    .collect();
                brute.sort_by(|a, b| b.0.total_cmp(&a.0));
                let banded = world.surfaces_at(x, z);
                check(
                    "surfaces",
                    banded == brute,
                    format!("{banded:?} vs {brute:?}"),
                );
            }
            out
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} mismatches, first: {:#?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(5)]
    );
}
