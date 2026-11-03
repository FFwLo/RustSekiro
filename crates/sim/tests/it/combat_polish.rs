use std::path::PathBuf;

fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

#[test]
fn audit_transition_data() {
    use sekiro_formats::{emedf, emevd::Emevd};
    let path = cache().join("raw/event/m11_01_00_00.emevd");
    if !path.exists() {
        return;
    }
    let file = Emevd::parse(&std::fs::read(path).unwrap()).unwrap();
    let mut cinematic = None;
    for id in [11115820, 11115870] {
        let start = file.starts_of(id).into_iter().next();
        for ins in file
            .event(id)
            .unwrap()
            .resolve(start.as_ref().map_or(&[], |s| &s.args))
        {
            if let Some((_, args)) = emedf::decode(&ins)
                && ins.bank == 2002
            {
                assert_eq!(id, 11115820, "Owl heat-up has no cinematic");
                cinematic = Some(args.iter().map(|(_, v)| v.as_i64()).collect::<Vec<_>>());
            }
        }
    }
    assert_eq!(
        cinematic.unwrap()[..6],
        [11010020, 16, 1112813, 11, 1, 10000]
    );
    let warp = sekiro_sim::events::warp_point(&cache(), 11, 1, 1112813).unwrap();
    assert!((warp.position[0] - 115.906).abs() < 0.001);
    assert!((warp.position[1] - 54.03).abs() < 0.001);
    assert!((warp.position[2] - 234.886).abs() < 0.001);
    assert!((warp.yaw.to_degrees() - 126.6).abs() < 0.001);
    assert!(sekiro_sim::events::warp_point(&cache(), 11, 1, -123).is_none());
}

fn walls(corner: bool) -> sekiro_sim::collision::CollisionWorld {
    use sekiro_formats::hknp::CollisionMesh;
    let mut mesh = CollisionMesh::default();
    let mut quad = |v: [[f32; 3]; 4]| {
        let first = mesh.vertices.len() as u32;
        mesh.vertices.extend(v);
        mesh.indices
            .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        mesh.materials.extend([1, 1]);
    };
    quad([
        [-10.0, 0.0, -10.0],
        [10.0, 0.0, -10.0],
        [10.0, 0.0, 10.0],
        [-10.0, 0.0, 10.0],
    ]);
    quad([
        [-10.0, 0.0, -1.4],
        [10.0, 0.0, -1.4],
        [10.0, 4.0, -1.4],
        [-10.0, 4.0, -1.4],
    ]);
    if corner {
        quad([
            [0.3, 0.0, -10.0],
            [0.3, 0.0, 10.0],
            [0.3, 4.0, 10.0],
            [0.3, 4.0, -10.0],
        ]);
    }
    sekiro_sim::collision::CollisionWorld::from_mesh(&mesh)
}

#[test]
fn aligned_throw_keeps_weapon_and_hit_dummy_connected_at_wall_and_corner() {
    use sekiro_sim::{
        body::{Body, BodyInput},
        collision::throw_pair_path_shift,
        deathblow::{AbsorbSlide, Absorbed, DummyFrame, Placement},
    };
    for corner in [false, true] {
        let wall = walls(corner);
        let attacker = Placement::default();
        let defender = Placement {
            position: [0.0, 0.0, -0.8],
            yaw: std::f32::consts::PI,
        };
        let sorb = DummyFrame {
            position: [0.0, 0.0, -1.2],
            yaw: std::f32::consts::PI,
        };
        let aligned = sorb.placement(&attacker);
        let path = [
            [attacker.position, defender.position],
            [attacker.position, aligned.position],
            [[0.0, 0.0, -0.2], [0.0, 0.0, -1.4]],
        ];
        let shift = throw_pair_path_shift([&wall, &wall], &path).unwrap();
        assert!(
            shift[1] > 0.39,
            "wall requires the whole pair to move: {shift:?}"
        );
        if corner {
            assert!(shift[0] < -0.09);
        }
        let mut a = Body {
            position: [shift[0], 0.0, shift[1]],
            ..Body::default()
        };
        let mut d = Body {
            position: [shift[0], 0.0, defender.position[2] + shift[1]],
            yaw: defender.yaw,
            ..Body::default()
        };
        let mut absorb = AbsorbSlide::new(Absorbed::Defender, &sorb, &attacker, &defender, 0.5);
        // Strike at 0.9 seconds: the synthetic blade dummy is 1.2 m forward at torso height,
        // and the hit dummy is at the defender's torso. No wall push may distort the pair.
        for _ in 0..54 {
            let mut input = BodyInput {
                ignore_walls: true,
                root_motion: [0.0, 0.0, -0.2 / 54.0, 0.0],
                ..BodyInput::default()
            };
            a.step(&input, 1.0 / 60.0, &wall);
            input.root_motion[2] = 0.2 / 54.0;
            d.step(&input, 1.0 / 60.0, &wall);
            if let Some(step) = absorb.advance(1.0 / 60.0) {
                d.position[0] += step.delta[0];
                d.position[2] += step.delta[2];
                d.yaw += step.yaw;
            }
        }
        let weapon = glam::Vec3::from(a.position) + glam::Vec3::new(0.0, 1.0, -1.2);
        let hit = glam::Vec3::from(d.position) + glam::Vec3::Y;
        assert!(
            weapon.distance(hit) < 0.005,
            "strike gap {}",
            weapon.distance(hit)
        );
        for body in [&a, &d] {
            assert!(body.grounded);
            assert_eq!(body.position[1], 0.0);
            assert!(glam::Vec2::from(wall.slide(body.position, [0.0; 2])).length() < 0.001);
        }
    }
}

#[test]
fn paired_root_motion_bypasses_wall_push_only_when_requested() {
    use sekiro_sim::body::{Body, BodyInput};
    let wall = walls(false);
    let mut ordinary = Body::default();
    let mut paired = Body::default();
    let mut input = BodyInput {
        root_motion: [0.0, 0.0, -1.3, 0.0],
        ..Default::default()
    };
    ordinary.step(&input, 1.0 / 60.0, &wall);
    input.ignore_walls = true;
    paired.step(&input, 1.0 / 60.0, &wall);
    assert!((ordinary.position[2] + 1.0).abs() < 0.01);
    assert!((paired.position[2] + 1.3).abs() < 0.001);
}

#[test]
fn cinematic_pauses_input_and_swaps_tomoe_only_under_black_at_authored_warp() {
    use sekiro_sim::{Buttons, InputFrame, duel::Duel, registry::Registry};
    if !cache().join("anim/c7100/skeleton.bin").exists() {
        return;
    }
    let spec = Registry::load(&cache())
        .unwrap()
        .resolve("c7100:71001000")
        .unwrap();
    let mut duel = Duel::load_with(&cache(), 1.6, spec).unwrap();
    duel.ai = None;
    duel.god = true;
    duel.enemy_active = false;
    let script = sekiro_sim::input::parse_action_script(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../game/scripts/genichiro_deathblows.txt"),
        )
        .unwrap(),
    );
    let mut fade_ticks = 0;
    let mut swap = false;
    let mut finished = false;
    for (step, scripted) in script.unwrap().iter().enumerate() {
        if duel.deathblow.is_none() && duel.cinematic.is_none() {
            duel.rules.fighters[1].posture.remaining =
                duel.rules.fighters[1].posture.remaining.min(1);
        }
        let before = [duel.player.body.position, duel.enemy.body.position];
        let hp = duel.rules.fighters[0].vitality.hp;
        let paused = duel.deathblow.is_none() && duel.cinematic.is_some();
        let old = duel.spec.chr.clone();
        let input = if paused {
            InputFrame {
                move_stick: [1.0, 1.0],
                buttons: Buttons {
                    attack: true,
                    jump: true,
                    lock_on: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            *scripted
        };
        duel.enemy_active = false;
        let report = duel.step(&input, 1.0 / 60.0);
        if paused {
            fade_ticks += 1;
            assert_eq!(report.player.advanced, 0.0);
            assert_eq!(report.enemy.advanced, 0.0);
            assert_eq!(duel.rules.fighters[0].vitality.hp, hp);
            assert!(report.hits.is_empty());
            assert!(report.player.behavior.events.is_empty());
            if old != duel.spec.chr {
                assert_eq!(duel.spec.chr, "c7110");
                assert_eq!(duel.cinematic.as_ref().unwrap().alpha(), 1.0);
                let warp = sekiro_sim::events::warp_point(&cache(), 11, 1, 1112813).unwrap();
                assert_eq!(duel.player.body.position, warp.position);
                assert_eq!(duel.player.body.yaw, warp.yaw);
                assert_eq!(duel.enemy.body.position, [103.509, 53.989, 243.125]);
                swap = true;
                eprintln!("cinematic swap under black at step {step}");
            } else {
                assert_eq!(duel.player.body.position, before[0]);
                assert_eq!(duel.enemy.body.position, before[1]);
            }
            if duel.cinematic.is_none() {
                finished = true;
                break;
            }
        }
    }
    assert!(swap && finished, "swap={swap} finished={finished}");
    assert!(
        (105..=106).contains(&fade_ticks),
        "{fade_ticks} paused ticks"
    );
    assert!(duel.player.input.pending_actions().next().is_none());
    let positions = [duel.player.body.position, duel.enemy.body.position];
    duel.cinematic = Some(sekiro_sim::events::Cinematic {
        id: 0,
        elapsed: 0.0,
        warp: None,
        applied: false,
    });
    for _ in 0..106 {
        let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert_eq!(duel.player.body.position, positions[0]);
        assert_eq!(duel.enemy.body.position, positions[1]);
        assert!(report.hits.is_empty());
        if duel.cinematic.is_none() {
            break;
        }
    }
    assert!(duel.cinematic.is_none());
}
