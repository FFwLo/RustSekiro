use sekiro_formats::hknp::CollisionMesh;
use sekiro_sim::body::{Body, BodyInput, Ground};
use sekiro_sim::collision::CollisionWorld;
use sekiro_sim::duel::Duel;
use sekiro_sim::hit_filter::HelperCollision;
use sekiro_sim::{Buttons, InputFrame};
use std::path::PathBuf;

/// A quad over `x0..x1` and `-10..10` at height `y`.
fn quad(mesh: &mut CollisionMesh, x0: f32, x1: f32, y: f32, material: u32) {
    let base = mesh.vertices.len() as u32;
    mesh.vertices
        .extend([[x0, y, -10.0], [x1, y, -10.0], [x1, y, 10.0], [x0, y, 10.0]]);
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    mesh.materials.extend([material; 2]);
}

/// A pool: an underwater floor 8 m down, a swim hit's dive plane over half of it and walkable
/// shallow water over the rest.
fn pool() -> CollisionWorld {
    let mut ground = CollisionMesh::default();
    quad(&mut ground, -10.0, 10.0, -8.0, 45);
    quad(&mut ground, 2.0, 10.0, 0.0, 21);
    let mut swim = CollisionMesh::default();
    quad(&mut swim, -10.0, 2.0, 0.0, 22);
    with_swim(&ground, swim)
}

/// The world of `ground` with `swim` as its swim hit (hit filter 27).
fn with_swim(ground: &CollisionMesh, swim: CollisionMesh) -> CollisionWorld {
    let mut world = CollisionWorld::from_mesh(ground);
    world.add_helpers(&HelperCollision {
        swim,
        ..Default::default()
    });
    world
}

#[test]
fn water_planes_are_separate_from_shallow_support_and_underwater_floor() {
    let world = pool();
    assert_eq!(world.water_surface(0.0, 0.0, 1.0).unwrap().material, 22);
    assert_eq!(Ground::floor(&world, 0.0, 0.0, 1.0), Some(-8.0));
    assert_eq!(Ground::material(&world, 0.0, 0.0, 0.01), Some(22));
    assert!(!world.water_surface(3.0, 0.0, 1.0).unwrap().deep());
    assert_eq!(Ground::floor(&world, 3.0, 0.0, 1.0), Some(0.0));
    assert!(world.water_surface(0.0, 0.0, -1.0).is_none());
    assert_eq!(
        world.raycast(glam::Vec3::Y, -glam::Vec3::Y, 20.0),
        Some(9.0)
    );
}

#[test]
fn water_material_roles_follow_hit_mtrl_names() {
    // A material id is its special setting times 100 plus its HitMtrlParam row: the `l`
    // binder's water has special setting 7 (721, 722, 735), and its role is the row's.
    for row in [20, 21, 22, 23, 24, 35, 36, 37, 38, 45, 46, 47] {
        for material in [row, 700 + row] {
            water_material_role(material, row);
        }
    }
}

fn water_material_role(material: u32, row: u32) {
    let mesh = CollisionMesh {
        vertices: vec![[-2.0, 0.0, -2.0], [2.0, 0.0, -2.0], [0.0, 0.0, 2.0]],
        indices: vec![0, 1, 2],
        materials: vec![material],
    };
    let deep = matches!(row, 22 | 36 | 38);
    // In a swim hit (hit filter 27) deep water is the plane bodies swim at, not a floor.
    let world = with_swim(&CollisionMesh::default(), mesh.clone());
    let water = world.water_surface(0.0, 0.0, 1.0);
    assert_eq!(water.is_some(), row < 45, "{material}");
    assert_eq!(water.is_some_and(|w| w.deep()), deep, "{material}");
    assert_eq!(world.floor(0.0, 0.0, 1.0).is_none(), deep, "{material}");
    // Anywhere else it is ground like any other material: the capsule meets the map's group 39
    // whatever the material (#189), and only shallow water is a surface.
    let world = CollisionWorld::from_mesh(&mesh);
    let water = world.water_surface(0.0, 0.0, 1.0);
    assert_eq!(water.is_some(), row < 45 && !deep, "{material}");
    assert_eq!(world.floor(0.0, 0.0, 1.0), Some(0.0), "{material}");
    assert_eq!(world.material(0.0, 0.0, 1.0), Some(material), "{material}");
}

#[test]
fn falling_body_lands_on_water_then_dives_through_plane() {
    let world = pool();
    let mut body = Body {
        position: [0.0, 25.0, 0.0],
        grounded: false,
        fall_top: 25.0,
        ..Body::default()
    };
    for _ in 0..150 {
        body.step(&BodyInput::default(), 1.0 / 60.0, &world);
        if body.grounded {
            break;
        }
    }
    assert_eq!(body.position[1], 0.0);
    assert_eq!(body.last_fall_height, 25.0);
    body.step(
        &BodyInput {
            water_mode: Some(true),
            root_motion: [0.0, -2.0, 0.0, 0.0],
            ..BodyInput::default()
        },
        1.0 / 60.0,
        &world,
    );
    assert_eq!(body.position[1], -2.0);
    body.step(
        &BodyInput {
            water_mode: Some(false),
            ..BodyInput::default()
        },
        1.0 / 60.0,
        &world,
    );
    assert_eq!(body.position[1], 0.0);
}

fn cache() -> PathBuf {
    crate::arena_c_support::cache()
}

#[test]
fn dungeon_water_route() {
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.enemy_active = false;
    // The pool as the player meets it in play: the swim hit (`h001000`, hit filter 27) is his
    // water, and the water surface only enemies and the camera meet (`h900100`, hit filter 16)
    // is a helper (#189: in `l` it lies a metre below `h`'s, over a shallow bed).
    let world = CollisionWorld::load_map(&cache().join("maps/m13_00_00_00")).unwrap();
    duel.player.body.position = [467.0, world.floor(467.0, 346.0, -57.0).unwrap(), 346.0];
    let yaw = -45.0f32.to_radians();
    duel.player.body.yaw = yaw;
    duel.player.ground = Box::new(world);
    let script =
        sekiro_sim::input::parse_action_script(include_str!("../../../game/scripts/m13_swim.txt"))
            .unwrap();
    let mut events = Vec::new();
    for (tick, mut frame) in script.into_iter().enumerate() {
        frame.camera_yaw += yaw;
        let r = duel.step(&frame, 1.0 / 60.0);
        events.extend(r.player.behavior.events.clone());
        if tick % 30 == 0 || !r.player.behavior.events.is_empty() {
            eprintln!(
                "route {tick} {:?} {:?} {:?} water={}",
                r.player.position,
                r.player.animation,
                r.player.behavior.events,
                duel.player.behavior.env.in_water
            );
        }
    }
    assert!(events.iter().any(|e| e == "W_SwimToDive"), "{events:?}");
    assert!(
        events
            .iter()
            .any(|e| e == "W_DiveToSwim" || e == "W_DiveToAir"),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| e.starts_with("W_LandWater")),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| e == "W_LandSwimKickMoveJump"),
        "{events:?}"
    );
    assert!(!duel.player.behavior.env.in_water && duel.player.body.grounded);
}

#[test]
fn hks_swim_dive_surface_and_shallow_exit() {
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.ground = Box::new(pool());
    duel.player.body.position = [0.0; 3];
    let mut events = Vec::new();
    let mut lowest = 0.0f32;
    for (ticks, buttons, stick) in [
        (180, Buttons::default(), [0.0, 1.0]),
        (
            180,
            Buttons {
                crouch: true,
                ..Buttons::default()
            },
            [0.0; 2],
        ),
        (
            240,
            Buttons {
                jump: true,
                ..Buttons::default()
            },
            [0.0; 2],
        ),
    ] {
        for t in 0..ticks {
            let r = duel.step(
                &InputFrame {
                    buttons,
                    move_stick: stick,
                    ..InputFrame::default()
                },
                1.0 / 60.0,
            );
            events.extend(r.player.behavior.events.clone());
            lowest = lowest.min(r.player.position[1]);
            assert!(r.player.position[1] >= -8.001, "crossed the seabed");
            if t % 60 == 0 {
                eprintln!(
                    "pos={:?} anim={:?} events={:?} mode={:?}",
                    r.player.position,
                    r.player.animation,
                    r.player.behavior.events,
                    duel.player.behavior.env.water_mode.get()
                );
            }
            assert!(r.player.behavior.script_errors.is_empty());
        }
        eprintln!("phase end {:?}", duel.player.body.position);
    }
    assert!(
        events.iter().any(|e| e.starts_with("W_StandMoveToSwim")),
        "{events:?}"
    );
    assert!(events.iter().any(|e| e == "W_SwimToDive"), "{events:?}");
    assert!(events.iter().any(|e| e == "W_DiveToSwim"), "{events:?}");
    assert!(lowest < -2.0);
    assert!(duel.player.body.position[1].abs() < 0.01);
    for _ in 0..120 {
        let r = duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                camera_yaw: -std::f32::consts::FRAC_PI_2,
                ..InputFrame::default()
            },
            1.0 / 60.0,
        );
        events.extend(r.player.behavior.events);
    }
    assert!(
        events.iter().any(|e| e.starts_with("W_SwimToStandMove")),
        "{events:?}"
    );
    assert!(duel.player.body.grounded);
}

#[test]
fn water_long_stiff_and_material_fall_death() {
    for (height, lethal) in [(25.0, false), (60.0, false), (85.0, true)] {
        let mut duel = Duel::load(&cache(), 4.0).unwrap();
        duel.enemy_active = false;
        duel.player.ground = Box::new(pool());
        duel.player.body = Body {
            position: [0.0, height, 0.0],
            fall_top: height,
            grounded: false,
            ..Body::default()
        };
        let mut landed = false;
        let mut long_stiff = false;
        for _ in 0..300 {
            let r = duel.step(&InputFrame::default(), 1.0 / 60.0);
            long_stiff |= r
                .player
                .behavior
                .events
                .iter()
                .any(|e| e == "W_LandWaterFreeFallLongStiff");
            if duel.player.body.grounded {
                landed = true;
            }
            if landed && (long_stiff || lethal) {
                break;
            }
        }
        assert!(landed);
        assert_eq!(
            duel.rules.fighters[0].vitality.hp == 0,
            lethal,
            "height {height}"
        );
        if !lethal {
            assert!(long_stiff, "height {height}");
        }
    }
}

/// #189 review: `l`'s ordinary shore `h004200` on m25 carries the deep material (row 22) where
/// `h` has sand, and the swim hit `h021500` lies 1.3 m under it. The exe swims only on swim-hit
/// contact (env 3025), so the shore stays dry ground and Wolf stands on it.
#[test]
fn deep_material_on_ordinary_shore_is_dry_ground() {
    let dir = cache().join("maps/m25_00_00_00");
    if !dir.join(sekiro_sim::lo_collision::LOW_FILE).is_file() {
        eprintln!("skipping: m25 has no collision_lo.bin in {dir:?}");
        return;
    }
    let world = CollisionWorld::load_map(&dir).unwrap();
    let (x, z) = (116.0, 408.1);
    let floor = world.floor(x, z, 105.5).expect("the shore is a floor");
    assert!((floor - 104.89).abs() < 0.05, "{floor}");
    assert_eq!(world.material(x, z, 105.5).map(|m| m % 100), Some(22));
    if let Some(water) = world.water_surface(x, z, 105.5) {
        assert!(water.height < floor, "{water:?} over the shore at {floor}");
    }
}
