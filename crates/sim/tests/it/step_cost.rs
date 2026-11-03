//! Sim step cost at the m11_00 spots where the live game's fixed step slowed down (#145, #156,
//! #161, #121), counted in collision work so the tests do not depend on the machine.
//! Skips without the exported maps.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use sekiro_sim::body::Ground;
use sekiro_sim::collision::{CollisionWorld, triangle_visits};

use crate::movement_helpers::cache;

/// The play world's collision ([`CollisionWorld::load_blocks`]: both Ashina blocks as
/// `crates/game/src/map_world.rs` `collision_world` builds them, without the story's switches),
/// built once for all the tests.
pub(crate) fn play_world() -> Option<Arc<CollisionWorld>> {
    static WORLD: OnceLock<Option<Arc<CollisionWorld>>> = OnceLock::new();
    WORLD
        .get_or_init(|| {
            CollisionWorld::load_blocks(&cache()?, &["m11_00_00_00", "m11_01_00_00"]).map(Arc::new)
        })
        .clone()
}

/// What `f` returns, and how many grid entries its collision queries looked at (#163).
fn visits<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let before = triangle_visits();
    let r = f();
    (r, triangle_visits() - before)
}

/// The most triangles any of eight slides of `reach` around `feet` tests on each push-out pass.
fn worst_slide(world: &CollisionWorld, feet: [f32; 3], reach: f32) -> usize {
    (0..8)
        .map(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            world.slide_triangle_count(feet, [a.sin() * reach, a.cos() * reach])
        })
        .max()
        .unwrap()
}

/// A Ground that counts the floor and wall queries it forwards; the rest pass straight
/// through ([`Ground::inner`]).
struct Counted {
    inner: Box<dyn Ground>,
    queries: Arc<AtomicU64>,
}

impl Counted {
    fn count(&self) {
        self.queries.fetch_add(1, Ordering::Relaxed);
    }
}

impl Ground for Counted {
    fn inner(&self) -> Option<&dyn Ground> {
        Some(self.inner.as_ref())
    }
    fn lowest_floor(&self, x: f32, z: f32) -> Option<f32> {
        self.count();
        self.inner.lowest_floor(x, z)
    }
    fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.count();
        self.inner.height(x, z)
    }
    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.count();
        self.inner.floor(x, z, top)
    }
    fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.count();
        self.inner.hug_floor(x, z, top)
    }
    fn ceiling(&self, feet: [f32; 3], rise: f32) -> Option<f32> {
        self.count();
        self.inner.ceiling(feet, rise)
    }
    fn head_contact(&self, feet: [f32; 3]) -> bool {
        self.count();
        self.inner.head_contact(feet)
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.count();
        self.inner.slide(feet, delta)
    }
    fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
        self.count();
        self.inner.slide_assisted(feet, delta, walls)
    }
    fn material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        self.count();
        self.inner.material(x, z, top)
    }
    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        self.count();
        self.inner.floor_with_material(x, z, top)
    }
}

/// At the worst wall spot of #145 a slide's 4 m cells held about 18k triangles, nearly all on
/// other storeys; only those in the body's height band may reach the per-sub-step push-out,
/// and the grid hands the queries only the bands they reach (#163).
#[test]
fn wall_slide_tests_only_the_body_band() {
    let Some(world) = play_world() else {
        return;
    };
    let [x, y, z] = [-257.0, -45.0, 8.0];
    let feet = [x, world.floor(x, z, y + 1.0).unwrap_or(y), z];
    let (tested, slides) = visits(|| worst_slide(&world, feet, 0.13));
    // 7k-18k before the band filter.
    assert!(tested < 2000, "{tested} triangles per push-out pass");
    // 112,136 over the whole 4 m cells, about 9,300 by band, 5,568 by band in 2 m cells.
    assert!(
        slides < 20_000,
        "the eight slides looked at {slides} grid entries"
    );
    // A body's floor: 34,564 over the whole 4 m cell, 2,192 by band and 2 m square, 1,345 in
    // the floors of its 2 m cell.
    let (_, floor) = visits(|| Ground::floor(&*world, feet[0], feet[2], feet[1] + 0.5));
    assert!(floor < 5_000, "a floor looked at {floor} grid entries");
}

/// A rising head's ceiling sweep tests only the flat faces in its height band whose boxes reach
/// its rim: at the worst wall spot of #145, and just under the cap beam pressed against the gate
/// fence (#170).
#[test]
fn ceiling_sweeps_test_only_nearby_faces() {
    use sekiro_sim::body::Ground;
    let Some(world) = play_world() else {
        return;
    };
    let [x, z] = [-257.0, 8.0];
    let wall = [x, world.floor(x, z, -44.0).unwrap(), z];
    let [x, z] = [-141.79, -149.49];
    let floor = [x, world.floor(x, z, -50.0).unwrap(), z];
    let beam = world
        .ceiling(floor, 5.0)
        .expect("the cap beam over the fence");
    for feet in [wall, [x, beam - 0.1, z]] {
        let tested = world.ceiling_triangle_count(feet, 0.2);
        // 27 and 26.
        assert!(tested < 100, "{tested} triangles at {feet:?}");
    }
}

/// The deathblow on the soldier where the live game stalled (#156, Wolf at about
/// (-126.3, -47.2, -193.9) facing 270 degrees): no step of the throw may spend more than a
/// bounded number of ground queries searching for pair clearance (the start's search tried
/// 640 placements of the whole paired path).
#[test]
fn deathblow_pair_search_is_bounded() {
    use sekiro_sim::{Buttons, InputFrame, arena::yaw_toward, duel::Duel};
    // The world builds while the duel loads; the duel's result waits for the skip check.
    let world = std::thread::spawn(play_world);
    let Some(cache) = cache() else {
        return;
    };
    let duel = Duel::load(&cache, 1.6);
    let Some(world) = world.join().unwrap() else {
        return;
    };
    let mut duel = duel.unwrap();
    let queries = Arc::new(AtomicU64::new(0));
    duel.player.ground = Box::new(Counted {
        inner: Box::new(world.clone()),
        queries: queries.clone(),
    });
    duel.enemy.ground = Box::new(Counted {
        inner: Box::new(sekiro_sim::collision::EnemyGround(world.clone())),
        queries: queries.clone(),
    });
    let heading = 270.0f32.to_radians();
    let wolf = glam::Vec3::new(-126.28, -47.23, -193.88);
    let foe = wolf + glam::Vec3::new(heading.sin(), 0.0, heading.cos()) * 1.6;
    for (c, p, target) in [(&mut duel.player, wolf, foe), (&mut duel.enemy, foe, wolf)] {
        let y = world.floor(p.x, p.z, p.y + 1.0).unwrap_or(p.y);
        c.body.position = [p.x, y, p.z];
        c.snap_yaw(yaw_toward(p, target));
        c.body.fall_top = y;
    }
    duel.enemy_active = false;
    duel.god = true;
    let (mut started, mut broke, mut thrown, mut worst, mut most) = (false, false, 0, 0, 0);
    for step in 0..900 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        queries.store(0, Ordering::Relaxed);
        let (r, seen) = visits(|| {
            duel.step(
                &InputFrame {
                    buttons: Buttons {
                        attack: !started && step % if broke { 10 } else { 40 } == 0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.0 / 60.0,
            )
        });
        broke |= r
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= r.deathblow.started.is_some();
        if started {
            worst = worst.max(queries.load(Ordering::Relaxed));
            most = most.max(seen);
            // The start and the first steps of paired travel; the rest repeats them.
            thrown += 1;
            if thrown > 30 {
                break;
            }
        }
    }
    assert!(started, "the deathblow never started");
    // 93,722 before the bounded search, about 10,500 after (most of them the wall projection).
    assert!(worst < 20_000, "{worst} ground queries in one throw step");
    // 124.6M with the wall projection unbounded over whole cells, 12.9M once it shares the
    // search's budget and the grid is banded (#163), 2,714,098 in 2 m cells with the floors
    // apart.
    assert!(most < 3_000_000, "{most} grid entries in one throw step");
}

/// Where Wolf stood idle against a wall and the step took about 12 ms (#161): the hook search
/// casts a sight ray to each grapple point in view, and every one is blocked here. A ray may
/// only test the triangles of the cells it crosses that reach its height over each cell.
#[test]
fn blocked_hook_rays_test_only_their_band() {
    use sekiro_sim::wire::{SIGHT_LIFT, Wire, sight_ray};
    let Some(world) = play_world() else {
        return;
    };
    let cache = cache().expect("the play world loaded from it");
    let wire = Wire::load(&cache, &cache.join("maps/m11_00_00_00")).unwrap();
    let [x, y, z] = [-214.998, -38.577, 13.956];
    let feet = glam::Vec3::new(x, world.floor(x, z, y + 1.0).unwrap_or(y), z);
    let chest = feet + glam::Vec3::Y * SIGHT_LIFT;
    let (mut rays, mut tested) = (0, 0);
    for point in wire.points.iter().filter_map(|p| p.position) {
        let (dir, length) = sight_ray(chest, point.into());
        if length < 25.0 {
            rays += 1;
            tested += world.ray_triangle_count(chest, dir, length);
        }
    }
    assert!(rays >= 5, "only {rays} hooks near the spot");
    // 168,198 for these 9 rays before (after gathering and sorting 284,900 cell entries),
    // 26,750 by band in 4 m cells, 6,594 in 2 m cells.
    assert!(
        tested < 60_000,
        "{tested} triangles tested by {rays} hook rays"
    );
}

/// The wall jump's air kick into the ceiling at about (-256.25, -42.2, 11.43) took up to 30 ms
/// a step (#161): `docking::wall_contact` probes eight 0.8 m slides from the chest, and each
/// tested every triangle of its 4 m cells in the body's height band. And each of the eight
/// read and filtered the same cells again (#167): now the fan reads them once.
#[test]
fn wall_contact_slides_test_only_nearby_triangles() {
    use sekiro_sim::docking::{WALL_PROBE_LIFT, WALL_REACH, wall_contact};
    let Some(world) = play_world() else {
        return;
    };
    let [x, y, z] = [-256.25, -42.2, 11.43];
    let tested = worst_slide(&world, [x, y + WALL_PROBE_LIFT, z], WALL_REACH);
    // 2,815 before the horizontal cull, about 1,100 after in 4 m cells, 787 in 2 m cells.
    assert!(tested < 2000, "{tested} triangles per push-out pass");
    let feet = glam::Vec3::new(x, y, z);
    let (_, fan) = visits(|| wall_contact(feet, 0.0, world.as_ref()));
    let (_, lone) = visits(|| {
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            let d = [a.sin() * WALL_REACH, a.cos() * WALL_REACH];
            world.slide([x, y + WALL_PROBE_LIFT, z], d);
        }
    });
    // The same entries eight lone slides read ({lone}, 2,4xx at #167) once: about a fifth.
    assert!(
        fan * 3 < lone,
        "the probe fan read {fan} grid entries, eight slides {lone}"
    );
}

/// The probe fan's shared gather and the push-outs' nearby list (#167) change no probe: around
/// the air-kick spot, from the feet and the chest, eight 0.8 m probes for eight facings end bit
/// for bit where lone slides and slides without any cull end. (Checked over 2.96 million
/// slides at four m11_00 spots when it landed.)
#[test]
fn wall_contact_fan_matches_lone_slides() {
    use sekiro_sim::docking::{WALL_PROBE_LIFT, WALL_REACH};
    let Some(world) = play_world() else {
        return;
    };
    let w: &CollisionWorld = &world;
    let mut checked = 0;
    for i in -3..=3 {
        for j in -3..=3 {
            let (x, z) = (-256.25 + i as f32 * 0.21, 11.43 + j as f32 * 0.21);
            let floor = w.floor(x, z, -41.2).unwrap_or(-42.2);
            for feet in [0.0, WALL_PROBE_LIFT].map(|lift| [x, floor + lift, z]) {
                for k in 0..8 {
                    let yaw = k as f32 * 0.8;
                    let deltas: [[f32; 2]; 8] = std::array::from_fn(|r| {
                        let a = yaw - r as f32 * std::f32::consts::FRAC_PI_4;
                        [a.sin() * WALL_REACH, a.cos() * WALL_REACH]
                    });
                    let fan = w.slide_fan(feet, deltas);
                    for (d, got) in deltas.into_iter().zip(fan) {
                        assert_eq!(got, w.slide(feet, d), "lone slide {d:?} from {feet:?}");
                        assert_eq!(got, w.slide_uncull(feet, d), "uncull {d:?} from {feet:?}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 7 * 7 * 2 * 8 * 8);
}

/// The horizontal cull of a slide's triangles (#161) and the push-outs' nearby list (#167)
/// change no slide: at both #161 spots,
/// moves of every length a step makes, up to 2 m along each axis into the slanted walls there,
/// end where they end without it.
#[test]
fn slide_cull_keeps_every_contact() {
    use rayon::prelude::*;
    let Some(world) = play_world() else {
        return;
    };
    let world: &CollisionWorld = &world;
    let mut starts = Vec::new();
    for ([x, y, z], lifts) in [
        ([-215.0f32, -38.6, 14.0], &[0.0][..]),
        (
            [-256.0, -42.2, 11.4],
            &[0.0, sekiro_sim::docking::WALL_PROBE_LIFT][..],
        ),
    ] {
        for i in -2..=2 {
            for j in -2..=2 {
                let (x, z) = (x + i as f32 * 0.25, z + j as f32 * 0.25);
                let floor = world.floor(x, z, y + 1.0).unwrap_or(y);
                // Only where a body can stand: one that starts inside a wall is pushed out of
                // it, as far as the wall is thick and not along the move.
                starts.extend(
                    lifts
                        .iter()
                        .map(|lift| [x, floor + lift, z])
                        .filter(|&feet| world.slide_uncull(feet, [0.0, 0.0]) == [0.0, 0.0]),
                );
            }
        }
    }
    let moves: Vec<[f32; 2]> = (0..16)
        .flat_map(|k| {
            let a = k as f32 * std::f32::consts::PI / 8.0;
            [0.13, 0.8, 1.4, 2.0].map(|len| [a.sin() * len, a.cos() * len])
        })
        .collect();
    // And bodies pressed against the walls there (slid 2 m eight ways), where a slide starts in
    // contact.
    let pressed: Vec<[f32; 3]> = starts
        .iter()
        .flat_map(|&feet| {
            moves.iter().skip(3).step_by(8).map(move |&d| {
                let [dx, dz] = world.slide(feet, d);
                [feet[0] + dx, feet[1], feet[2] + dz]
            })
        })
        .collect();
    starts.extend(pressed);
    let worst = starts
        .par_iter()
        .flat_map_iter(|&feet| {
            moves.iter().map(move |&d| {
                let [a, b] = [world.slide(feet, d), world.slide_uncull(feet, d)];
                (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
            })
        })
        .reduce(|| 0.0, f32::max);
    assert!(worst <= 1e-5, "the cull moved a slide by {worst} m");
}

/// Wolf walking into the wooden fence corner beside the snowy firewood in the samurai general's
/// courtyard and pushing there (#121, Wolf stuck at about (-221.6, -38.0, 11.5) on log
/// o114541_1809): before #159 and #161 a step there took about 16.6 ms in the live game, so the
/// fixed step's catch-up ran 15 steps a frame and the game fell to 3-10 fps. With the placed
/// objects, as the play world has them, no step of the walk and the push may look at more than
/// a bounded number of grid entries.
#[test]
fn fence_corner_push_is_bounded() {
    use glam::Vec3;
    use sekiro_sim::{Character, InputFrame};

    use crate::movement_helpers::{DT, settle};
    let Some(cache) = cache() else {
        return;
    };
    let Some(mut world) = CollisionWorld::load_blocks(&cache, &["m11_00_00_00", "m11_01_00_00"])
    else {
        return;
    };
    let dir = cache.join("maps/m11_00_00_00");
    if dir.join("object_collision.bin").is_file() {
        world
            .add_block_objects(&dir, "m11_00_00_00", |_, who| who)
            .unwrap();
    }
    let world = Arc::new(world);
    let mut wolf = Character::load_from_cache(&cache).unwrap();
    wolf.ground = Box::new(world.clone());
    // The hook search's sight rays are part of every step's cost here (#161).
    wolf.wire = sekiro_sim::wire::Wire::load(&cache, &dir).unwrap();
    // The last two points of the issue's route: from (-220, 9) toward (-224, 12), past the corner.
    let (x, z) = (-220.0, 9.0);
    wolf.body.position = [x, world.floor(x, z, -37.0).unwrap(), z];
    settle(&mut wolf);
    let toward = Vec3::new(-4.0, 0.0, 3.0).normalize();
    // The camera looks along `rotate_y(yaw) * -Z`, so forward on the stick walks along `toward`.
    let yaw = (-toward.x).atan2(-toward.z);
    let input = InputFrame {
        move_stick: [0.0, 1.0],
        camera_yaw: yaw,
        ..Default::default()
    };
    let (mut worst, mut pushed) = (0, 0);
    for _ in 0..600 {
        let before = wolf.body.position;
        let (report, seen) = visits(|| wolf.tick(&input, DT));
        assert!(
            report.behavior.script_errors.is_empty(),
            "{:?}",
            report.behavior.script_errors
        );
        worst = worst.max(seen);
        let moved = (report.position[0] - before[0]).hypot(report.position[2] - before[2]);
        pushed += usize::from(moved < 0.001);
    }
    let [x, y, z] = wolf.body.position;
    eprintln!(
        "fence corner: worst step {worst} grid entries, {pushed} steps without moving, ends at ({x:.2}, {y:.2}, {z:.2})"
    );
    assert!(
        pushed > 300 && Vec3::new(x, 0.0, z).distance(Vec3::new(-221.6, 0.0, 11.5)) < 1.0,
        "Wolf never pressed into the corner: ends at ({x}, {y}, {z}) after {pushed} still steps"
    );
    // 1,282 (Wolf's slides, floors and the hook search's sight rays). Before #159 and #161 a
    // single slide pass tested 7k-18k triangles and the hook rays 168k at the spot 7 m away.
    assert!(worst < 10_000, "{worst} grid entries in one step");
}
