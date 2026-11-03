//! Running jumps at the fence beside the wooden gate near m11_00's start 0 (#170). A cap beam
//! juts from the fence's top over the gate side, and its edge passes over the rim of the head
//! of a body pressed against the fence: the rise must stop under it, or the slide shoves the
//! head out of the beam, through the fence. Also the push-out of a body wedged between a wall and
//! an overhang (#176). The gate tests skip without the exported maps.

use glam::{Vec2, Vec3};
use sekiro_formats::hknp::CollisionMesh;
use sekiro_sim::body::Ground;
use sekiro_sim::collision::{CollisionWorld, HEAD_TOP, RADIUS, push_checks};
use sekiro_sim::{Buttons, Character, InputFrame};

use crate::movement_helpers::{DT, cache, settle};
use crate::step_cost::play_world;

/// The ends of a stretch of the fence's gate-side face.
const FENCE: [Vec2; 2] = [Vec2::new(-140.31, -149.62), Vec2::new(-142.98, -150.39)];

/// How far `p` is on the gate side of that stretch of the fence's face: negative only for a
/// point through it, not one past either end.
fn gate_side(p: [f32; 3]) -> f32 {
    let p = Vec2::new(p[0], p[2]);
    let face = FENCE[1] - FENCE[0];
    let along = (p - FENCE[0]).dot(face) / face.length_squared();
    if !(0.0..=1.0).contains(&along) {
        return p.distance(FENCE[0] + face * along.clamp(0.0, 1.0));
    }
    let normal = face.perp().normalize();
    let normal = if normal.y > 0.0 { normal } else { -normal };
    (p - FENCE[0]).dot(normal)
}

/// Wolf sprints at the fence from the gate side and jumps, square on and at 25 degrees (the
/// stick and camera turned together): his feet never cross the fence's face (before, the
/// first of each pair went through and ended about 15 m past it). At least one jump must
/// reach the wedge under the cap beam, pressed against the fence with the beam just over his
/// head, or the test no longer tests the beam.
#[test]
fn running_jumps_at_the_gate_fence_stay_on_the_gate_side() {
    let (Some(world), Some(cache)) = (play_world(), cache()) else {
        return;
    };
    let (mut through, mut wedged) = (Vec::new(), 0);
    for (x, yaw) in [
        (-140.5, 0.0f32),
        (-140.85, 0.0),
        (-143.88, -25.0),
        (-144.23, -25.0),
    ] {
        let mut wolf = Character::load_from_cache(&cache).unwrap();
        wolf.ground = Box::new(world.clone());
        let z = -141.5;
        let feet = Vec3::new(x, world.floor(x, z, -50.0).unwrap(), z);
        wolf.body.position = feet.to_array();
        settle(&mut wolf);
        wolf.snap_yaw(yaw.to_radians());
        let (mut worst, mut wedge) = (f32::MAX, false);
        for i in 0..200 {
            let input = InputFrame {
                move_stick: [0.0, 1.0],
                camera_yaw: yaw.to_radians(),
                camera_pitch: 0.0,
                buttons: Buttons {
                    dodge: i > 5,
                    jump: (40..43).contains(&i),
                    ..Buttons::default()
                },
            };
            let report = wolf.tick(&input, DT);
            assert!(
                report.behavior.script_errors.is_empty(),
                "{:?}",
                report.behavior.script_errors
            );
            let side = gate_side(report.position);
            worst = worst.min(side);
            wedge |= side < 0.6 && world.ceiling(report.position, 0.1).is_some();
        }
        wedged += usize::from(wedge);
        if worst < 0.0 {
            through.push((x, yaw, worst, wolf.body.position));
        }
    }
    assert!(through.is_empty(), "went through the fence: {through:?}");
    assert!(wedged > 0, "no jump reached the wedge under the cap beam");
}

/// A thin wall (0.05 m thick, its near face at z 0) and the low beam of #176 over the body's
/// side of it, as triangles in the order given: `wall_first` puts the wall's before the beam's.
/// The beam's front face, at z 0.25, has its lower edge 0.2 m over the head sphere's centre of
/// a body standing 0.2 m from the wall, so that edge pushes the head towards the wall while
/// the wall pushes both spheres away from it.
fn wedge_world(wall_first: bool) -> CollisionWorld {
    let beam = HEAD_TOP - RADIUS + 0.2;
    let quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| [a, b, c, d];
    let wall = [
        quad(
            [-3.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, 3.0, 0.0],
            [-3.0, 3.0, 0.0],
        ),
        quad(
            [-3.0, 0.0, -0.05],
            [-3.0, 3.0, -0.05],
            [3.0, 3.0, -0.05],
            [3.0, 0.0, -0.05],
        ),
    ];
    let beam = [
        quad(
            [-3.0, beam, 0.0],
            [3.0, beam, 0.0],
            [3.0, beam, 0.25],
            [-3.0, beam, 0.25],
        ),
        quad(
            [-3.0, beam, 0.25],
            [3.0, beam, 0.25],
            [3.0, beam + 0.2, 0.25],
            [-3.0, beam + 0.2, 0.25],
        ),
    ];
    let quads = if wall_first {
        [wall, beam].concat()
    } else {
        [beam, wall].concat()
    };
    let mut m = CollisionMesh::default();
    for q in quads {
        let base = m.vertices.len() as u32;
        m.vertices.extend_from_slice(&q);
        m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        m.materials.extend([0, 0]);
    }
    CollisionWorld::from_mesh(&m)
}

/// A body wedged between the thin wall and the beam's edge (#176) is never pushed through the
/// wall, whatever the order of the triangles, standing still, pressing towards the wall or
/// moving along it; and the order changes nothing. Before, with the wall's triangles first,
/// the beam's push came last and won, and the slide ended with the feet at z -0.45, through
/// the wall.
#[test]
fn a_wedged_body_is_never_pushed_through_a_thin_wall() {
    let feet = [0.0, 0.0, 0.2];
    let mut failures = Vec::new();
    for delta in [[0.0, 0.0], [0.0, -0.3], [0.5, 0.0], [0.4, -0.2]] {
        let moved = [true, false].map(|wall_first| wedge_world(wall_first).slide(feet, delta));
        for (wall_first, d) in [true, false].into_iter().zip(moved) {
            if feet[2] + d[1] < 0.0 {
                failures.push(format!(
                    "{delta:?} wall first {wall_first}: through to z {}",
                    feet[2] + d[1]
                ));
            }
        }
        if Vec2::from(moved[0]).distance(Vec2::from(moved[1])) > 1e-4 {
            failures.push(format!("{delta:?}: order changes the slide {moved:?}"));
        }
        // Still free to move along the wall.
        if moved[0][0] < delta[0] * 0.9 {
            failures.push(format!("{delta:?}: stuck in the wedge {moved:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Slides of a body standing or jumping beside the gate fence, within a body radius of its face,
/// where the cap beam's sloping underside reaches over the head while the fence touches the
/// legs (#176): none ends through the fence, whatever the move, and none costs the push-out
/// more than a bounded number of contact tests. Before, the head's push towards the fence won
/// over the legs' push away from it, and 1251 of these 2925 slides ended through it.
#[test]
fn slides_beside_the_gate_fence_never_end_through_it() {
    let Some(world) = play_world() else {
        return;
    };
    let face = FENCE[1] - FENCE[0];
    let normal = face.perp().normalize();
    let normal = if normal.y > 0.0 { normal } else { -normal };
    let along = face.normalize();
    let ground = world.floor(-141.7, -149.6, -47.0).unwrap();
    let (mut through, mut checks) = (Vec::new(), 0);
    for a in 1..10 {
        for o in 0..5 {
            let p = FENCE[0] + face * (a as f32 * 0.1) + normal * (0.2 + o as f32 * 0.05);
            for h in 0..13 {
                let feet = [p.x, ground + h as f32 * 0.1, p.y];
                for delta in [Vec2::ZERO, -normal, along, -along, (along - normal) * 0.7] {
                    let before = push_checks();
                    let d = world.slide(feet, (delta * 0.1).to_array());
                    checks = checks.max(push_checks() - before);
                    let end = [feet[0] + d[0], feet[1], feet[2] + d[1]];
                    if gate_side(end) < 0.0 {
                        through.push((feet, delta, gate_side(end)));
                    }
                }
            }
        }
    }
    assert!(
        through.is_empty(),
        "{} slides went through: {:?}",
        through.len(),
        &through[..through.len().min(5)]
    );
    // The push-out's own work, in contact tests (`least_push`), for the slide with the most
    // contacts: 6,938, and 71,716 before wedges were told from the contacts' directions at
    // once (#176 review).
    assert!(checks < 15_000, "{checks} contact tests in one slide");
}

/// A body already deep in an acute corner, as a respawn, a warp or a moving part can leave it:
/// clearing both walls takes a push longer than a body radius, which counts as wedged, but as
/// it did not move in it still gets out, within a few steps and without crossing either wall
/// (#176 review). Two 3 m walls meet at 30 degrees at the origin and open towards +z.
#[test]
fn a_body_deep_in_an_acute_corner_gets_out() {
    let half = 15f32.to_radians();
    let ends = [
        Vec2::new(half.sin(), half.cos()),
        Vec2::new(-half.sin(), half.cos()),
    ];
    let mut m = CollisionMesh::default();
    for e in ends {
        let (x, z) = (e.x * 5.0, e.y * 5.0);
        let base = m.vertices.len() as u32;
        m.vertices
            .extend_from_slice(&[[0.0, 0.0, 0.0], [x, 0.0, z], [x, 3.0, z], [0.0, 3.0, 0.0]]);
        m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        m.materials.extend([0, 0]);
    }
    let world = CollisionWorld::from_mesh(&m);
    // Inside the corner, 0.155 m from each wall.
    let mut feet = [0.0, 0.0, 0.6];
    let clearance = |p: [f32; 3]| {
        let p = Vec2::new(p[0], p[2]);
        ends.map(|e| e.perp_dot(p).abs().min(p.length()))
            .into_iter()
            .fold(f32::MAX, f32::min)
    };
    let mut steps = 0;
    while clearance(feet) < RADIUS - 1e-3 {
        assert!(
            steps < 5,
            "still in the corner after {steps} steps at {feet:?}"
        );
        let d = world.slide(feet, [0.0, 0.0]);
        feet = [feet[0] + d[0], feet[1], feet[2] + d[1]];
        let p = Vec2::new(feet[0], feet[2]);
        assert!(
            ends.iter().all(|e| e.perp_dot(p) * e.x.signum() >= 0.0) && p.y > 0.0,
            "crossed a wall to {feet:?}"
        );
        steps += 1;
    }
}
