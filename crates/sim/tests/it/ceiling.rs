//! Jumps under the low roof of the shack by m11_00's start 0 (#160): the rise must stop at the
//! ceiling, or the head goes into the roof and the fall lands on the roof's underside (the
//! mesh's winding is not consistent, so any flat face is a floor), trapping Wolf and the camera
//! in it. Skips without the exported maps.

use sekiro_formats::hknp::CollisionMesh;
use sekiro_sim::body::{Body, BodyInput};
use sekiro_sim::collision::{CollisionWorld, HEAD_TOP, MAX_SLOPE_DEGREES, RADIUS};
use sekiro_sim::tae::VelocityChange;
use sekiro_sim::{Buttons, Character, InputFrame};

use crate::movement_helpers::{DT, cache, settle};
use crate::step_cost::play_world;

/// The heights of the faces over `(x, z)` flat enough to stand on, which the collision also
/// takes for ceilings from below.
fn flat_faces(world: &CollisionWorld, x: f32, z: f32) -> impl Iterator<Item = f32> {
    let min_ny = MAX_SLOPE_DEGREES.to_radians().cos();
    world
        .surfaces_at(x, z)
        .into_iter()
        .filter(move |&(_, ny)| ny.abs() >= min_ny)
        .map(|(h, _)| h)
}

/// The floor at `(x, z)` near the shack's floor level and the lowest ceiling over it.
fn floor_and_ceiling(world: &CollisionWorld, x: f32, z: f32) -> (f32, f32) {
    let floor = world.floor(x, z, -45.5).expect("a floor in the shack");
    let ceiling = flat_faces(world, x, z)
        .filter(|&h| h > floor + 0.1)
        .reduce(f32::min)
        .expect("a ceiling over the shack floor");
    (floor, ceiling)
}

/// Wolf jumps (jump held 3 ticks) with the stick at `move_stick` for `camera_yaw` and runs
/// `ticks` ticks: the highest his feet rose, and whether he left the ground.
fn jump(wolf: &mut Character, move_stick: [f32; 2], camera_yaw: f32, ticks: u32) -> (f32, bool) {
    let (mut peak, mut airborne) = (f32::MIN, false);
    for i in 0..ticks {
        let input = InputFrame {
            move_stick,
            camera_yaw,
            camera_pitch: 0.0,
            buttons: Buttons {
                jump: i < 3,
                ..Buttons::default()
            },
        };
        let report = wolf.tick(&input, DT);
        assert!(
            report.behavior.script_errors.is_empty(),
            "{:?}",
            report.behavior.script_errors
        );
        airborne |= !report.grounded;
        peak = peak.max(report.position[1]);
    }
    (peak, airborne)
}

/// Wolf's own standing jump where the shack's ceiling is 3.13 m over the floor, under the 3.93 m
/// a free jump's head reaches: his head stops under it and he lands back on the floor (before,
/// he rose into the roof and landed in it, 2.38 m up).
#[test]
fn standing_jump_under_the_shack_ceiling_lands_on_the_floor() {
    let (Some(world), Some(cache)) = (play_world(), cache()) else {
        return;
    };
    let [x, z] = [-123.3, -193.33];
    let (floor, ceiling) = floor_and_ceiling(&world, x, z);
    assert!(
        ceiling - floor < 3.9,
        "the ceiling is {:.2} m up",
        ceiling - floor
    );
    let mut wolf = Character::load_from_cache(&cache).unwrap();
    wolf.ground = Box::new(world.clone());
    wolf.body.position = [x, floor, z];
    settle(&mut wolf);
    let (peak, airborne) = jump(&mut wolf, [0.0, 0.0], 0.0, 150);
    assert!(airborne, "never jumped");
    assert!(
        peak + HEAD_TOP <= ceiling + 0.01,
        "head rose to {:.2}, the ceiling is at {ceiling:.2}",
        peak + HEAD_TOP
    );
    let y = wolf.body.position[1];
    assert!(
        wolf.body.grounded && (y - floor).abs() < 0.3,
        "landed at {y:.2}, the floor is at {floor:.2}"
    );
}

/// Running jumps across the shack, in eight directions at the jump's two forward speeds: none
/// ends with a ceiling inside the body (before, 4 of these 16 landed in the roof, 2 m up).
#[test]
fn running_jumps_in_the_shack_stay_under_its_roof() {
    let Some(world) = play_world() else {
        return;
    };
    let [x, z] = [-128.3, -189.08];
    let (floor, _) = floor_and_ceiling(&world, x, z);
    let mut trapped = Vec::new();
    for k in 0..8 {
        for horizontal in [3.0, 6.4] {
            let yaw = k as f32 * std::f32::consts::FRAC_PI_4;
            let mut body = Body {
                position: [x, floor, z],
                yaw,
                fall_top: floor,
                ..Default::default()
            };
            let jump = VelocityChange {
                fall_type: 0,
                horizontal_scale: 0.0,
                vertical_scale: 0.0,
                horizontal,
                vertical: 9.64,
                horizontal_angle: 0.0,
            };
            body.step(
                &BodyInput {
                    velocity_change: Some(jump),
                    ..Default::default()
                },
                DT,
                &*world,
            );
            for _ in 0..120 {
                if body.grounded {
                    break;
                }
                body.step(&BodyInput::default(), DT, &*world);
            }
            let [ex, ey, ez] = body.position;
            if flat_faces(&world, ex, ez).any(|h| h > ey + 0.1 && h < ey + HEAD_TOP - 0.05) {
                trapped.push((yaw.to_degrees().round(), horizontal, body.position));
            }
        }
    }
    assert!(trapped.is_empty(), "landed inside the roof: {trapped:?}");
}

/// A floor at y 0 for z > 0 and a wall at z 0 whose top, `top` m up, runs back over z < 0.
fn wall_world(top: f32) -> CollisionWorld {
    let mut m = CollisionMesh::default();
    for q in [
        [
            [-3.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, 0.0, 3.0],
            [-3.0, 0.0, 3.0],
        ],
        [
            [-3.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, top, 0.0],
            [-3.0, top, 0.0],
        ],
        [
            [-3.0, top, -3.0],
            [3.0, top, -3.0],
            [3.0, top, 0.0],
            [-3.0, top, 0.0],
        ],
    ] {
        let base = m.vertices.len() as u32;
        m.vertices.extend_from_slice(&q);
        m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        m.materials.extend([0, 0]);
    }
    CollisionWorld::from_mesh(&m)
}

/// A running jump into a wall whose top is within its reach: pressed against the wall as its
/// head sphere's centre passes the top, whose edge is then at the sphere's rim, the body rises
/// as high as by a wall too tall to reach the top of, so ledge grabs and wall jumps still carry
/// it up past the top.
#[test]
fn a_jump_flush_against_a_wall_rises_past_its_top() {
    let top = 2.5;
    let head = HEAD_TOP - RADIUS;
    // The peak height and where the body was as its head's centre passed `top`.
    let rise = |world: &CollisionWorld| {
        let mut body = Body {
            position: [0.0, 0.0, 0.6],
            ..Default::default()
        };
        let jump = VelocityChange {
            fall_type: 0,
            horizontal_scale: 0.0,
            vertical_scale: 0.0,
            horizontal: 3.0,
            vertical: 9.64,
            horizontal_angle: 0.0,
        };
        let mut input = BodyInput {
            velocity_change: Some(jump),
            ..Default::default()
        };
        let (mut peak, mut passing) = (f32::MIN, None);
        for _ in 0..120 {
            body.step(&input, DT, world);
            input = BodyInput::default();
            peak = peak.max(body.position[1]);
            if body.position[1] + head > top {
                passing.get_or_insert(body.position);
            }
        }
        (peak, passing)
    };
    let (peak, passing) = rise(&wall_world(top));
    assert!(
        passing.is_some_and(|p| (p[2] - RADIUS).abs() < 0.01),
        "not pressed against the wall as the head passed its top: {passing:?}"
    );
    let (free, _) = rise(&wall_world(20.0));
    assert!(
        (peak - free).abs() < 1e-3,
        "peaked at {peak}, {free} by a taller wall"
    );
}

/// Wolf jumps up the rock wall of the gully by m11_00's start 0 (about (-132, -38, -106)),
/// pressed against it under the ledge he means to catch: each jump rises as high as a free one
/// (2.13 m, as before #170). The wall's uneven rock juts lips over his head sphere's shoulder,
/// up to 0.4 m off its centre line, and stopping at those held these jumps to 0.5 to 1.2 m.
#[test]
fn jumps_up_the_gully_rock_wall_rise_past_its_lips() {
    let (Some(world), Some(cache)) = (play_world(), cache()) else {
        return;
    };
    let mut low = Vec::new();
    for (x, z, yaw) in [
        (-132.83, -105.78, 191.25f32),
        (-130.98, -106.23, 202.5),
        (-131.83, -105.89, 213.75),
    ] {
        let floor = world.floor(x, z, -37.0).expect("the gully floor");
        let mut wolf = Character::load_from_cache(&cache).unwrap();
        wolf.ground = Box::new(world.clone());
        wolf.body.position = [x, floor, z];
        settle(&mut wolf);
        wolf.snap_yaw(yaw.to_radians());
        let (peak, _) = jump(&mut wolf, [0.0, 1.0], yaw.to_radians(), 90);
        if peak - floor < 2.0 {
            low.push((x, z, peak - floor));
        }
    }
    assert!(low.is_empty(), "stopped low on the wall: {low:?}");
}

/// A floor at y 0 and a beam 0.3 m deep and 0.6 m wide whose underside is 2.3 m up and whose
/// near edge is `edge` m from z 0 (negative: over the line z 0), as a closed box of quads.
fn beam_world(edge: f32) -> CollisionWorld {
    let (y0, y1, z0, z1) = (2.3, 2.6, edge, edge + 0.6);
    let mut m = CollisionMesh::default();
    for q in [
        [
            [-3.0, 0.0, -3.0],
            [3.0, 0.0, -3.0],
            [3.0, 0.0, 3.0],
            [-3.0, 0.0, 3.0],
        ],
        [[-3.0, y0, z0], [3.0, y0, z0], [3.0, y0, z1], [-3.0, y0, z1]],
        [[-3.0, y1, z0], [3.0, y1, z0], [3.0, y1, z1], [-3.0, y1, z1]],
        [[-3.0, y0, z0], [3.0, y0, z0], [3.0, y1, z0], [-3.0, y1, z0]],
        [[-3.0, y0, z1], [3.0, y0, z1], [3.0, y1, z1], [-3.0, y1, z1]],
    ] {
        let base = m.vertices.len() as u32;
        m.vertices.extend_from_slice(&q);
        m.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        m.materials.extend([0, 0]);
    }
    CollisionWorld::from_mesh(&m)
}

/// Standing jumps at z 0 under beams whose near edge is from over the head's centre line to
/// just inside its rim (#168): at the end of no tick is the head sphere more than 1 cm into the
/// beam, and each lands back on the floor. Before, the rise stopped only at a touch within
/// 0.25 m of the centre line, so an edge further out passed into the head sphere's shoulder by
/// up to a tick's rise before the next tick's slide pushed it off.
#[test]
fn a_jump_under_an_off_centre_beam_keeps_the_head_out_of_it() {
    let mut bad = Vec::new();
    for edge in [-0.2, 0.1, 0.2, 0.26, 0.3, 0.35, 0.39] {
        let world = beam_world(edge);
        let mut body = Body {
            position: [0.0, 0.0, 0.0],
            fall_top: 0.0,
            ..Default::default()
        };
        let jump = VelocityChange {
            fall_type: 0,
            horizontal_scale: 0.0,
            vertical_scale: 0.0,
            horizontal: 0.0,
            vertical: 9.64,
            horizontal_angle: 0.0,
        };
        let mut input = BodyInput {
            velocity_change: Some(jump),
            ..Default::default()
        };
        let mut deepest = 0.0f32;
        for _ in 0..120 {
            body.step(&input, DT, &world);
            input = BodyInput::default();
            let [x, y, z] = body.position;
            let head = glam::Vec3::new(x, y + HEAD_TOP - RADIUS, z);
            let lo = glam::Vec3::new(-3.0, 2.3, edge);
            let hi = glam::Vec3::new(3.0, 2.6, edge + 0.6);
            deepest = deepest.max(RADIUS - head.distance(head.clamp(lo, hi)));
        }
        if deepest > 0.01 || !body.grounded || body.position[1].abs() > 1e-3 {
            bad.push((edge, deepest, body.position, body.grounded));
        }
    }
    assert!(bad.is_empty(), "(edge, depth, end, grounded): {bad:?}");
}

/// A body whose feet are 0.3 m under the shack roof's underside, falling (any way of getting
/// there the rise stop does not cover, #168): it falls to the floor rather than landing on the
/// underside, which it came at from below. The underside is a lone plane, the next face over it
/// 3.7 m up, so no slab-thickness filter can tell it from a floor.
#[test]
fn a_body_under_the_shack_roof_never_lands_on_its_underside() {
    let Some(world) = play_world() else {
        return;
    };
    let [x, z] = [-123.3, -193.33];
    let (floor, ceiling) = floor_and_ceiling(&world, x, z);
    for below in [0.05, 0.3, 0.45] {
        let mut body = Body {
            position: [x, ceiling - below, z],
            grounded: false,
            fall_top: ceiling - below,
            ..Default::default()
        };
        for _ in 0..120 {
            body.step(&BodyInput::default(), DT, &*world);
        }
        let y = body.position[1];
        assert!(
            body.grounded && (y - floor).abs() < 0.3,
            "{below} m under the roof: landed at {y:.2}, the floor is at {floor:.2}, the roof at {ceiling:.2}"
        );
    }
}
