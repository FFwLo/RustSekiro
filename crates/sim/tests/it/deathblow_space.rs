//! Castle-top paired placement reproductions with installed animation and collision data.
use glam::Vec3;
use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec, yaw_toward},
    duel::Duel,
};
use std::{io::Write, path::PathBuf};

const DT: f32 = 1.0 / 60.0;

#[test]
fn paired_path_reserves_floor_without_blocking_ordinary_movement() {
    use sekiro_sim::{
        body::{Body, BodyInput, Ground},
        collision::throw_pair_path_shift,
    };
    struct Roof;
    impl Ground for Roof {
        fn height(&self, _x: f32, z: f32) -> Option<f32> {
            (z >= -1.0).then_some(0.0)
        }
    }
    let path = [
        [[0.0, 0.0, 0.0], [0.0, 0.0, -0.8]],
        [[0.0, 0.0, -0.6], [0.0, 0.0, -1.4]],
    ];
    let shift = throw_pair_path_shift([&Roof, &Roof], &path).unwrap();
    assert!(shift[1] >= 0.4, "paired path leaves roof: {shift:?}");
    // This protection belongs to throws, not to the ordinary character controller.
    let mut body = Body {
        position: path[0][1],
        ..Default::default()
    };
    body.step(
        &BodyInput {
            root_motion: [0.0, 0.0, -1.0, 0.0],
            ..Default::default()
        },
        DT,
        &Roof,
    );
    assert!(body.position[2] < -1.0);
    assert!(!body.grounded);
}

#[test]
fn paired_wall_search_cannot_relocate_through_the_wall() {
    use sekiro_sim::{
        body::Ground,
        collision::{throw_pair_path_shift, throw_pair_step_shift},
    };
    struct Door;
    impl Ground for Door {
        fn height(&self, _x: f32, _z: f32) -> Option<f32> {
            Some(0.0)
        }
        fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
            if feet[2] >= -1.0 {
                [delta[0], delta[1].max(-1.0 - feet[2])]
            } else {
                [delta[0], delta[1].min(-1.0 - feet[2])]
            }
        }
    }
    let path = [
        [[0.0, 0.0, 0.0], [0.0, 0.0, -0.8]],
        [[0.0, 0.0, -0.6], [0.0, 0.0, -1.4]],
    ];
    let shift = throw_pair_path_shift([&Door, &Door], &path).unwrap();
    assert!(shift[1] >= 0.4 - 1e-4);
    assert!(
        throw_pair_step_shift([&Door, &Door], &path).is_none(),
        "unsafe step must be held when a small correction cannot clear it"
    );
    let flat = sekiro_sim::body::FlatGround::default();
    assert_eq!(throw_pair_path_shift([&flat, &flat], &path), Some([0.0; 2]));
}

#[test]
#[ignore = "slow: full installed castle collision and paired timelines"]
fn castle_deathblow_door() {
    castle_case("door");
}

#[test]
#[ignore = "slow: full installed castle collision and paired timelines"]
fn castle_deathblow_ledge() {
    castle_case("ledge");
}

#[test]
#[ignore = "slow: full installed castle collision and paired timelines"]
fn castle_deathblow_wall() {
    castle_case("wall");
}

#[test]
#[ignore = "slow: full installed castle collision and paired timelines"]
fn castle_deathblow_open_floor() {
    castle_case("open");
}

fn castle_case(case: &str) {
    let Some(cache) = crate::movement_helpers::cache()
        .filter(|cache| cache.join("maps/m11_01_00_00/collision.bin").exists())
    else {
        eprintln!("skipping: castle map not extracted");
        return;
    };
    let spec = ArenaSpec::by_name("genichiro").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 1.6, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.enemy_active = false;
    duel.god = true;
    for _ in 0..360 {
        duel.step(&InputFrame::default(), DT);
    }
    // Keep this phase and its arena collision fixed while tracing the complete pair.
    duel.events = None;
    duel.enemy_active = false;
    arena.world.set_gate(true);
    if case == "door" {
        // Drain the intro's last AI-requested action before placing the passive pair.
        for _ in 0..360 {
            duel.step(&InputFrame::default(), DT);
        }
    }
    let center = arena.floor_at(arena.layout.center());
    let doorway = arena
        .fog_walls
        .first()
        .expect("castle entrance gate")
        .position;
    let mut door_normal = arena.fog_walls[0].rotation * Vec3::Z;
    if (doorway - center).dot(door_normal) < 0.0 {
        door_normal = -door_normal;
    }
    let door_direction = Vec3::new(doorway.x - center.x, 0.0, doorway.z - center.z).normalize();
    let door_distance = arena
        .world
        .raycast(center + Vec3::Y * 0.8, door_direction, 30.0)
        .expect("closed castle entrance");
    let door_plane = center + door_direction * door_distance;
    let (wolf, boss) = match case {
        "door" => {
            let boss = door_plane - door_direction * 0.65;
            (boss - door_direction * 1.6, boss)
        }
        "wall" => (
            Vec3::new(107.829, 54.13, 226.686),
            Vec3::new(107.829, 54.13, 225.086),
        ),
        "ledge" => {
            // Find the actual end of the roof, ignoring lower roofs and ground far below.
            let mut found = None;
            'directions: for degree in 0..360 {
                let angle = (degree as f32).to_radians();
                let dir = Vec3::new(angle.sin(), 0.0, angle.cos());
                for n in 30..220 {
                    let edge = center + dir * (n as f32 * 0.1);
                    let floor = arena.world.floor(edge.x, edge.z, center.y + 0.5);
                    if floor.is_some_and(|y| (y - center.y).abs() < 0.5) {
                        continue;
                    }
                    let boss = edge - dir * 0.5;
                    let wolf = boss - dir * 1.6;
                    if [wolf, boss].iter().all(|p| {
                        arena
                            .world
                            .floor(p.x, p.z, center.y + 0.5)
                            .is_some_and(|y| (y - center.y).abs() < 0.5)
                    }) && arena.world.raycast(wolf + Vec3::Y, dir, 2.1).is_none()
                    {
                        found = Some((wolf, boss));
                        break 'directions;
                    }
                    break;
                }
            }
            found.expect("no accessible roof edge")
        }
        _ => (center + Vec3::Z * 1.6, center),
    };
    for (c, p, target) in [
        (&mut duel.player, wolf, boss),
        (&mut duel.enemy, boss, wolf),
    ] {
        c.body.position = arena.floor_at(p).to_array();
        c.body.yaw = yaw_toward(p, target);
        c.body.fall_top = c.body.position[1];
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/alt-db/captures");
    std::fs::create_dir_all(&out).unwrap();
    let mut log = std::fs::File::create(out.join(format!("{case}-positions.txt"))).unwrap();
    eprintln!(
        "{case} setup: Wolf={:?} boss={:?} yaw={:.3} door_plane={door_plane:?}",
        duel.player.body.position,
        duel.enemy.body.position,
        duel.player.body.yaw.to_degrees()
    );
    writeln!(
        log,
        "setup Wolf={:?} boss={:?} yaw={:.3}",
        duel.player.body.position,
        duel.enemy.body.position,
        duel.player.body.yaw.to_degrees()
    )
    .unwrap();
    let mut started = false;
    let mut broke = false;
    let mut after = 0;
    let mut hit = false;
    let mut violations = Vec::new();
    for step in 0..1500 {
        let before = [duel.player.body.position, duel.enemy.body.position];
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: !started && step % if broke { 10 } else { 40 } == 0,
                    lock_on: case == "door" && (2..4).contains(&step),
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        broke |= r
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= r.deathblow.started.is_some();
        hit |= r.deathblow.hit;
        let positions = [duel.player.body.position, duel.enemy.body.position];
        if case == "open" && r.deathblow.hit {
            // Open-floor horizontal placement must remain unchanged for both actors:
            // the collision-safe pair placement must not move a pair that has room.
            // Re-recorded after 86b8434 (#109): its RustSekiro root-motion blend-in and
            // approach-side body separation start the pair 0.17 m further apart before
            // the deathblow, so the hit lands about 0.35 m from the f99e8b7 values
            // ([106.79679, 230.90317], [107.11729, 229.8592]); the pair placement did not change.
            for (p, expected) in positions
                .iter()
                .zip([[106.65166, 231.23984], [106.91727, 230.15114]])
            {
                assert!(
                    (p[0] - expected[0]).hypot(p[2] - expected[1]) < 0.002,
                    "open-floor hit position {p:?}, expected x/z {expected:?}"
                );
            }
        }
        writeln!(
            log,
            "{step} t={:.3} Wolf={:?} boss={:?} phase={:?}",
            step as f32 * DT,
            positions[0],
            positions[1],
            duel.deathblow.as_ref().map(|d| d.phase)
        )
        .unwrap();
        if step == 0 || r.deathblow.started.is_some() || r.deathblow.hit {
            eprintln!(
                "{case} step {step}: Wolf={:?} boss={:?}",
                positions[0], positions[1]
            );
        }
        if started {
            for (actor, p) in positions.iter().enumerate() {
                let floor = arena.world.floor(p[0], p[2], p[1] + 0.5);
                if case == "door" && (Vec3::from(*p) - door_plane).dot(door_normal) > -0.1 {
                    violations.push(format!("step {step} actor {actor} left arena side of door"));
                }
                // Authored get-up velocity can lift a fighter, but support must remain
                // on this roof and both feet must settle onto it after the pair ends.
                if !floor.is_some_and(|y| {
                    (y - center.y).abs() < 1.0 && (after == 0 || (y - p[1]).abs() < 0.3)
                }) {
                    violations.push(format!(
                        "step {step} actor {actor} position {p:?} floor {floor:?}"
                    ));
                }
                let from = Vec3::from(before[actor]);
                let delta = Vec3::from(*p) - from;
                let len = delta.length();
                if len > 0.001
                    && arena
                        .world
                        .raycast(from + Vec3::Y * 0.8, delta / len, len)
                        .is_some_and(|h| h < len - 0.001)
                {
                    violations.push(format!("step {step} actor {actor} crossed wall"));
                }
            }
            if duel.deathblow.is_none() {
                after += 1;
                if after == 60 {
                    for c in [&duel.player, &duel.enemy] {
                        let p = c.body.position;
                        assert!(c.body.grounded, "{case}: airborne after recovery: {p:?}");
                        assert!(
                            arena
                                .world
                                .floor(p[0], p[2], p[1] + 0.5)
                                .is_some_and(|y| (y - p[1]).abs() < 0.06),
                            "{case}: feet do not rest on floor: {p:?}"
                        );
                    }
                    eprintln!(
                        "{case} final step {step}: Wolf={:?} boss={:?}",
                        positions[0], positions[1]
                    );
                    break;
                }
            }
        }
    }
    assert!(
        broke && started && hit && after == 60,
        "incomplete deathblow {case}"
    );
    assert!(
        violations.is_empty(),
        "{case}: {}",
        violations
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
