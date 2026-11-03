use glam::Vec3;
use sekiro_sim::{
    Buttons, InputFrame,
    body::{Body, FlatGround},
    collision::CollisionWorld,
    duel::Duel,
    wire::{Point, Search, Variation, Wire},
};
use std::path::PathBuf;

fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}
fn synthetic(distance: f32) -> Wire {
    let mut wire = Wire::default();
    wire.points.push(Point {
        position: Some([0.0, 0.0, -distance]),
        variation: 1,
        model: "synthetic".into(),
        source_part: "point".into(),
    });
    wire.search = Some(Search {
        action_distance_near: 3.0,
        action_distance_far: 15.0,
        find_distance_far: 25.0,
        filtering_distance_height_upper: 10.2,
        filtering_distance_height_lower: 10.2,
        filtering_distance_width: 14.3,
        softlock_distance_height_upper: 10.2,
        softlock_distance_height_lower: 10.2,
        softlock_distance_width: 7.15,
    });
    wire.variations.insert(
        1,
        Variation {
            id: 1,
            target_marker_type: 0,
            extend_wire_action_distance: 0.0,
            chr_physics_homing_id_offset: 0,
        },
    );
    wire
}
#[test]
fn ranges_angles_and_ranking() {
    let body = Body::default();
    for (distance, selected) in [
        (2.9, false),
        (3.0, true),
        (15.0, true),
        (15.1, true),
        (25.0, true),
        (26.0, false),
    ] {
        let mut wire = synthetic(distance);
        wire.search(&body, 0.0, 0.0, false);
        assert_eq!(wire.preview.is_some(), selected);
        assert_eq!(wire.can_fire(), (3.0..=15.0).contains(&distance));
        if let Some(target) = &wire.preview {
            assert_eq!(
                target.state == sekiro_sim::wire::MarkerState::Ready,
                distance <= 15.0
            );
        }
    }
    let mut wire = synthetic(10.0);
    wire.search(&body, std::f32::consts::PI, 0.0, false);
    assert!(wire.candidate.is_none());
    let mut off_centre = wire.points[0].clone();
    off_centre.position = Some([1.0, 0.0, -5.0]);
    wire.points.insert(0, off_centre);
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.candidate.unwrap().index, 1);
    // A distant, more centred preview must not steal a usable hook (including wire chains).
    let mut wire = synthetic(10.0);
    wire.points[0].position = Some([0.0, 6.0, -10.0]);
    let mut distant = wire.points[0].clone();
    distant.position = Some([0.0, 8.0, -20.0]);
    wire.points.push(distant);
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.preview.as_ref().unwrap().index, 0);
    assert_eq!(
        wire.candidate.as_ref().unwrap().state,
        sekiro_sim::wire::MarkerState::Ready
    );
    assert_eq!(wire.candidate.unwrap().index, 0);
}
#[test]
fn marker_distance_boundaries_and_variation_extension() {
    use sekiro_sim::wire::MarkerState::{Distant, Hidden, Ready};
    let search = synthetic(10.0).search.unwrap();
    for (distance, state) in [
        (2.99, Hidden),
        (3.0, Ready),
        (15.0, Ready),
        (20.0, Distant { fill: 0.5 }),
        (25.0, Distant { fill: 0.0 }),
        (25.01, Hidden),
    ] {
        assert_eq!(search.marker_state(distance, 0.0), state);
    }
    // The extension reaches both bands: usable to 15 plus it, shown to 25 plus it.
    assert_eq!(search.marker_state(20.0, 5.0), Ready);
    assert_eq!(search.marker_state(25.0, 5.0), Distant { fill: 0.5 });
    assert_eq!(search.marker_state(30.0, 5.0), Distant { fill: 0.0 });
    assert_eq!(search.marker_state(30.01, 5.0), Hidden);
    assert_eq!(search.marker_state(25.0, 10.0), Ready);
    // The kite rows (WireVariationParam 40, 41 and 70201 extend by 30 m): usable to 45 m.
    assert_eq!(search.marker_state(45.0, 30.0), Ready);
    assert_eq!(search.marker_state(50.0, 30.0), Distant { fill: 0.5 });
    assert_eq!(search.marker_state(55.01, 30.0), Hidden);
    // Row 70200 extends by 15 m.
    assert_eq!(search.marker_state(30.0, 15.0), Ready);
    assert_eq!(search.marker_state(35.0, 15.0), Distant { fill: 0.5 });
}
/// A kite hook 40 m ahead (extension 30) is marked usable through the search, not only by
/// `marker_state`.
#[test]
fn kite_hook_is_usable_at_forty_metres() {
    let mut wire = synthetic(40.0);
    wire.points[0].position = Some([0.0, 8.0, -40.0]);
    wire.variations
        .get_mut(&1)
        .unwrap()
        .extend_wire_action_distance = 30.0;
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.can_fire());
}
#[test]
fn extracted_counts_transform_and_collision() {
    for (map, objects, edges) in [("m11_01_00_00", 172, 224), ("m11_00_00_00", 237, 364)] {
        let dir = cache().join("maps").join(map);
        if !dir.join("grapple.json").exists() {
            continue;
        }
        let wire = Wire::load(&cache(), &dir).unwrap();
        let names: std::collections::HashSet<_> = wire
            .points
            .iter()
            .filter(|p| p.model.starts_with('o'))
            .map(|p| &p.source_part)
            .collect();
        assert_eq!(names.len(), objects);
        assert_eq!(wire.points.len(), edges);
        if map == "m11_01_00_00" {
            let point = Vec3::from(
                wire.points
                    .iter()
                    .find(|p| p.source_part == "o003610_0014")
                    .unwrap()
                    .position
                    .unwrap(),
            );
            assert!(point.distance(Vec3::new(97.1, 52.8, 251.0)) < 0.1);
            let floor = wire
                .collision
                .as_ref()
                .unwrap()
                .floor(point.x, point.z, point.y + 0.1)
                .unwrap();
            eprintln!("roof point={point:?} floor={floor} gap={}", point.y - floor);
            assert!((point.y - floor).abs() < 2.0);
            let chest = Vec3::new(103.5, 53.995594 + 1.2, 243.6);
            let ray = point - chest;
            eprintln!(
                "roof LOS distance={} hit={:?}",
                ray.length(),
                wire.collision
                    .as_ref()
                    .unwrap()
                    .raycast(chest, ray.normalize(), ray.length())
            );
        }
    }
}
#[test]
fn real_hks_synthetic_ground_shot_and_out_of_range() {
    if !cache().join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return;
    }
    for distance in [10.0, 20.0, 30.0] {
        let mut duel = Duel::load(&cache(), 4.0).unwrap();
        duel.enemy_active = false;
        duel.player.ground = Box::new(FlatGround::default());
        duel.player.body = Body::default();
        duel.player.wire = synthetic(distance);
        for _ in 0..30 {
            duel.step(&InputFrame::default(), 1.0 / 60.0);
        }
        let press = InputFrame {
            buttons: Buttons {
                grapple: true,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&press, 1.0 / 60.0);
        let fired = r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_GroundWireShoot");
        assert_eq!(fired, distance == 10.0, "{:?}", r.player);
        let mut moved = false;
        for tick in 0..120 {
            let r = duel.step(&InputFrame::default(), 1.0 / 60.0);
            moved |= r
                .player
                .animation
                .as_deref()
                .is_some_and(|a| a.starts_with("a000_20201"));
            if tick % 15 == 0 {
                eprintln!(
                    "tick={tick} anim={:?} pos={:?} events={:?}",
                    r.player.animation, r.player.position, r.player.behavior.events
                );
            }
        }
        if fired {
            assert!(moved);
            assert!(
                Vec3::from(duel.player.body.position).distance(Vec3::new(0.0, 0.0, -10.0)) < 0.5
            );
            assert!(duel.player.body.grounded);
        }
    }
}
#[test]
fn line_of_sight_blocks_wall() {
    use sekiro_formats::hknp::CollisionMesh;
    let mut wire = synthetic(10.0);
    wire.collision = Some(CollisionWorld::from_mesh(&CollisionMesh {
        vertices: vec![[-50.0, -50.0, -5.0], [50.0, -50.0, -5.0], [0.0, 50.0, -5.0]],
        indices: vec![0, 1, 2],
        ..CollisionMesh::default()
    }));
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.candidate.is_none());
}

#[test]
fn obstruction_during_travel_releases_wire() {
    use sekiro_formats::hknp::CollisionMesh;
    let mut wire = synthetic(10.0);
    let mut body = Body::default();
    wire.search(&body, 0.0, 0.0, false);
    assert!(wire.step(
        &mut body,
        Some("a000_202000"),
        None,
        1.0 / 60.0,
        &FlatGround::default()
    ));
    wire.collision = Some(CollisionWorld::from_mesh(&CollisionMesh {
        vertices: vec![[-50.0, -50.0, -5.0], [50.0, -50.0, -5.0], [0.0, 50.0, -5.0]],
        indices: vec![0, 1, 2],
        ..CollisionMesh::default()
    }));
    for _ in 0..60 {
        wire.step(
            &mut body,
            Some("a000_202012"),
            Some((0.0, 0.5)),
            1.0 / 60.0,
            &FlatGround::default(),
        );
    }
    assert!(body.position[2] > -5.0);
    assert!(wire.active.is_none());
    assert!(!body.grounded);
}

#[test]
fn nonrideable_offset_and_release() {
    let mut wire = synthetic(10.0);
    let variation = wire.variations.get_mut(&1).unwrap();
    variation.target_marker_type = 1;
    variation.chr_physics_homing_id_offset = 20;
    wire.offsets.insert(300000020, Vec3::new(0.0, -2.5, 7.5));
    let mut body = Body::default();
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(
        wire.candidate.as_ref().unwrap().arrival,
        Vec3::new(0.0, -2.5, -17.5)
    );
    wire.step(
        &mut body,
        Some("a000_202000"),
        None,
        1.0 / 60.0,
        &FlatGround::default(),
    );
    for _ in 0..31 {
        wire.step(
            &mut body,
            Some("a000_202012"),
            Some((0.0, 0.5)),
            1.0 / 60.0,
            &FlatGround::default(),
        );
    }
    body.velocity[1] = -3.0;
    assert!(body.position[2] < -10.0, "swing-through must pass the hook");
    assert!(wire.perch.is_none());
    assert!(!wire.step(
        &mut body,
        Some("a000_201030"),
        None,
        1.0 / 60.0,
        &FlatGround::default()
    ));
    assert_eq!(body.velocity[1], -3.0);
    assert!(!body.grounded);
}

#[test]
fn castle_shachi_clear_approach_and_blocked_original_spawn() {
    let dir = cache().join("maps/m11_01_00_00");
    if !dir.join("grapple.json").exists() {
        return;
    }
    let mut wire = Wire::load(&cache(), &dir).unwrap();
    let body = Body {
        position: [103.5, 53.995594, 243.6],
        yaw: 139f32.to_radians(),
        ..Body::default()
    };
    wire.search(&body, body.yaw, 0.0, false);
    assert!(
        wire.candidate.is_none(),
        "original balcony spawn must not grapple through collision"
    );
    for (position, yaw, positive) in [
        ([104.03038, 53.234768, 254.9751], 60.0f32, true),
        ([103.5, 53.995594, 243.6], -41.0, false),
    ] {
        let mut duel = Duel::load(&cache(), 4.0).unwrap();
        duel.enemy_active = false;
        duel.player.ground = Box::new(CollisionWorld::load_map(&dir).unwrap());
        duel.player.wire = Wire::load(&cache(), &dir).unwrap();
        duel.player.body = Body {
            position,
            yaw: yaw.to_radians(),
            ..Body::default()
        };
        let script = sekiro_sim::input::parse_action_script(include_str!(
            "../../../game/scripts/grapple_shachi.txt"
        ))
        .unwrap();
        let mut shot = false;
        let mut move_start = false;
        let mut arrival_tick = None;
        let anchor = Vec3::new(97.10218, 52.843166, 250.9751);
        for (tick, mut frame) in script.into_iter().enumerate() {
            frame.camera_yaw = yaw.to_radians();
            frame.camera_pitch = 0.25;
            let report = duel.step(&frame, 1.0 / 60.0);
            shot |= report
                .player
                .behavior
                .events
                .iter()
                .any(|e| e == "W_GroundWireShoot");
            move_start |= report.player.animation.as_deref() == Some("a000_202012");
            if shot
                && arrival_tick.is_none()
                && Vec3::from(report.player.position).distance(anchor) < 0.01
            {
                arrival_tick = Some(tick);
            }
        }
        if positive {
            assert!(shot && move_start);
            assert_eq!(arrival_tick, Some(140));
            assert!(Vec3::from(duel.player.body.position).distance(anchor) < 0.5);
            assert!(duel.player.body.grounded);
        } else {
            assert!(!shot);
        }
    }
}

#[test]
fn solved_arc_arrives_ten_metres_away_five_up_at_point_eight_three() {
    let mut wire = synthetic(10.0);
    wire.points[0].position = Some([0.0, 5.0, -10.0]);
    let mut body = Body::default();
    wire.search(&body, 0.0, 0.0, false);
    wire.step(
        &mut body,
        Some("a000_202000"),
        None,
        1.0 / 60.0,
        &FlatGround::default(),
    );
    let mut apex = 0.0_f32;
    for tick in 0..50 {
        wire.step(
            &mut body,
            Some("a000_202011"),
            Some((0.0, 0.83)),
            1.0 / 60.0,
            &FlatGround::default(),
        );
        apex = apex.max(body.position[1]);
        if tick == 24 {
            assert!(body.position[1] > 2.5);
        }
    }
    let distance = Vec3::from(body.position).distance(Vec3::new(0.0, 5.0, -10.0));
    eprintln!("arc flight=0.83 s distance={distance} apex={apex}");
    assert!(distance < 0.2);
    assert!(apex > 5.0);
}

fn synthetic_duel() -> Option<Duel> {
    if !cache().join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return None;
    }
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.use_simple_ai();
    duel.enemy_active = false;
    duel.player.ground = Box::new(FlatGround::default());
    duel.player.body = Body::default();
    duel.player.wire = synthetic(10.0);
    for _ in 0..30 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    Some(duel)
}

#[test]
fn jump_fires_air_wire_and_holds_during_905() {
    let Some(mut duel) = synthetic_duel() else {
        return;
    };
    let jump = InputFrame {
        buttons: Buttons {
            jump: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    };
    duel.step(&jump, 1.0 / 60.0);
    let mut fired = false;
    let mut held: Option<f32> = None;
    for tick in 0..180 {
        let input = InputFrame {
            buttons: Buttons {
                grapple: tick == 24,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&input, 1.0 / 60.0);
        if tick % 10 == 0 {
            eprintln!(
                "air tick={tick} anim={:?} pos={:?} events={:?}",
                r.player.animation, r.player.position, r.player.behavior.events
            );
        }
        fired |= r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_AirWireShoot");
        if r.player.animation.as_deref() == Some("a000_202100") && r.player.anim_time < 0.49 {
            if let Some(y) = held {
                assert!((r.player.position[1] - y).abs() < 0.001);
            }
            held = Some(r.player.position[1]);
        }
    }
    assert!(fired && held.is_some());
    assert!(Vec3::from(duel.player.body.position).distance(Vec3::new(0.0, 0.0, -10.0)) < 0.5);
}

#[test]
fn wire_fall_chains_to_a_second_synthetic_hook() {
    let Some(mut duel) = synthetic_duel() else {
        return;
    };
    // Shoot again during the first arc, before the ridden point becomes a perch.
    duel.player.wire.points[0].position = Some([0.0, 6.0, -10.0]);
    let mut second = duel.player.wire.points[0].clone();
    second.position = Some([0.0, 8.0, -20.0]);
    second.source_part = "second".into();
    duel.player.wire.points.push(second);
    let mut chain = false;
    let mut shot = false;
    for tick in 0..240 {
        let in_fall = duel.player.wire.active.is_some() && duel.player.body.position[2] < -8.0;
        let press = tick == 0 || (in_fall && !shot);
        if in_fall && !shot {
            shot = true;
        }
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    grapple: press,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            1.0 / 60.0,
        );
        chain |= r
            .player
            .animation
            .as_deref()
            .is_some_and(|a| matches!(a, "a000_202600" | "a000_202700"));
        if tick % 15 == 0 {
            eprintln!(
                "chain tick={tick} anim={:?} pos={:?} events={:?}",
                r.player.animation, r.player.position, r.player.behavior.events
            );
        }
    }
    assert!(chain);
    assert!(duel.player.body.position[2] < -18.0);
}

#[test]
fn castle_roof_chain_route() {
    let dir = cache().join("maps/m11_01_00_00");
    let Some(mut duel) = synthetic_duel() else {
        return;
    };
    duel.player.ground = Box::new(CollisionWorld::load_map(&dir).unwrap());
    duel.player.wire = Wire::load(&cache(), &dir).unwrap();
    duel.player.body = Body {
        position: [104.03038, 53.234768, 254.9751],
        yaw: 60f32.to_radians(),
        ..Body::default()
    };
    let script = sekiro_sim::input::parse_action_script(include_str!(
        "../../../game/scripts/grapple2_chain.txt"
    ))
    .unwrap();
    let mut chain = false;
    for (tick, mut frame) in script.into_iter().enumerate() {
        frame.camera_yaw = 60f32.to_radians();
        frame.camera_pitch = 0.25;
        let r = duel.step(&frame, 1.0 / 60.0);
        chain |= r
            .player
            .animation
            .as_deref()
            .is_some_and(|a| matches!(a, "a000_202600" | "a000_202700"));
        if tick % 15 == 0 || tick == 120 {
            eprintln!(
                "roof chain tick={tick} anim={:?} pos={:?} candidate={:?} active={:?} events={:?}",
                r.player.animation,
                r.player.position,
                duel.player
                    .wire
                    .candidate
                    .as_ref()
                    .map(|t| &duel.player.wire.points[t.index].source_part),
                duel.player
                    .wire
                    .active
                    .as_ref()
                    .map(|t| &duel.player.wire.points[t.index].source_part),
                r.player.behavior.events
            );
        }
    }
    assert!(chain);
    assert!(duel.player.body.grounded);
    assert!(
        Vec3::from(duel.player.body.position).distance(Vec3::new(93.31853, 53.092392, 254.30566))
            < 0.6
    );
}

#[test]
fn third_hook_uses_wire_move_wire_two_shoot() {
    let Some(mut duel) = synthetic_duel() else {
        return;
    };
    for (distance, height) in [(10.0, 6.0), (20.0, 8.0), (30.0, 10.0)] {
        let mut point = duel.player.wire.points[0].clone();
        point.position = Some([0.0, height, -distance]);
        if distance == 10.0 {
            duel.player.wire.points[0] = point;
        } else {
            duel.player.wire.points.push(point);
        }
    }
    let mut second = false;
    let mut third = false;
    let mut wire_two = false;
    for tick in 0..300 {
        let z = duel.player.body.position[2];
        let press = if tick == 0 {
            true
        } else if z < -8.0 && !second {
            second = true;
            true
        } else if z < -16.0 && !third {
            third = true;
            true
        } else {
            false
        };
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    grapple: press,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            1.0 / 60.0,
        );
        wire_two |= r.player.animation.as_deref() == Some("a000_202700");
        if press {
            eprintln!(
                "third tick={tick} anim={:?} events={:?}",
                r.player.animation, r.player.behavior.events
            );
        }
    }
    assert!(wire_two);
    assert!(duel.player.body.position[2] < -28.0);
}

#[test]
fn wire_attack_and_guard_cancel_flight_through_hks() {
    for guard in [false, true] {
        let Some(mut duel) = synthetic_duel() else {
            return;
        };
        duel.player.behavior.env.unlocked.insert(14);
        duel.player.behavior.env.unlocked.insert(15);
        duel.player.wire.points[0].position = Some([0.0, 6.0, -10.0]);
        let expected = if guard {
            "W_WireMoveDeflectGuardStart"
        } else {
            "W_WireFallAttackCombo1"
        };
        let mut cancelled = false;
        for tick in 0..160 {
            let r = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        grapple: tick == 0,
                        attack: !guard && tick == 90,
                        guard: guard && (90..130).contains(&tick),
                        ..Buttons::default()
                    },
                    ..InputFrame::default()
                },
                1.0 / 60.0,
            );
            cancelled |= r.player.behavior.events.iter().any(|e| e == expected);
            if guard && (tick % 10 == 0 || !r.player.behavior.events.is_empty()) {
                eprintln!(
                    "guard tick={tick} anim={:?} flags={:?} events={:?}",
                    r.player.animation, r.player.action_flags, r.player.behavior.events
                );
            }
        }
        assert!(cancelled, "{expected} did not fire");
        assert!(duel.player.wire.active.is_none());
    }
}

#[test]
fn fast_wire_fall_uses_land_ready_before_land_wire_move() {
    let Some(mut duel) = synthetic_duel() else {
        return;
    };
    duel.player.wire.points[0].position = Some([0.0, 10.0, -10.0]);
    duel.player
        .wire
        .variations
        .get_mut(&1)
        .unwrap()
        .target_marker_type = 1;
    let mut ready = false;
    let mut landed = false;
    for tick in 0..360 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    grapple: tick == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            1.0 / 60.0,
        );
        ready |= r.player.animation.as_deref() == Some("a000_202045");
        landed |= r.player.animation.as_deref() == Some("a000_202035");
        if r.player.clip_started {
            eprintln!(
                "landing tick={tick} anim={:?} grounded={} vy={} events={:?}",
                r.player.animation,
                r.player.grounded,
                duel.player.behavior.env.vertical_velocity,
                r.player.behavior.events
            );
        }
    }
    assert!(ready && landed);
}

#[test]
fn perch_support_ends_outside_point_six_metres() {
    use sekiro_sim::{body::Ground, wire::PerchGround};
    let terrain = FlatGround::default();
    let ground = PerchGround {
        terrain: &terrain,
        perch: Some(Vec3::new(0.0, 5.0, 0.0)),
    };
    assert_eq!(ground.floor(0.59, 0.0, 6.0), Some(5.0));
    assert_eq!(ground.floor(0.61, 0.0, 6.0), Some(0.0));
    assert_eq!(ground.floor(0.0, 0.0, 4.0), Some(0.0));
}

/// Terrain with deep water at y 0 over a lake bed at y -10, and an answer to every other
/// [`Ground`] query that is not its default, so a wrapper that drops one is caught (#169).
struct Lake;

impl sekiro_sim::body::Ground for Lake {
    fn height(&self, _: f32, _: f32) -> Option<f32> {
        Some(-10.0)
    }
    fn water_surface(&self, _: f32, _: f32, top: f32) -> Option<sekiro_sim::body::WaterSurface> {
        // HitMtrlParam row 22 is deep water.
        (top >= 0.0).then_some(sekiro_sim::body::WaterSurface {
            height: 0.0,
            material: 22,
        })
    }
    fn lowest_floor(&self, _: f32, _: f32) -> Option<f32> {
        Some(-12.0)
    }
    fn trigger_contact(
        &self,
        _: [f32; 3],
        to: [f32; 3],
    ) -> Option<sekiro_sim::collision::TriggerContact> {
        Some(sekiro_sim::collision::TriggerContact {
            hit_filter: 13,
            part: "event wall".into(),
            point: to.into(),
        })
    }
    fn near_trigger(&self, part: &str, _: [f32; 3]) -> bool {
        part == "event wall"
    }
    fn fall_return_below(&self, _: f32, _: f32, _: f32) -> bool {
        true
    }
    fn hug_floor(&self, _: f32, _: f32, top: f32) -> Option<f32> {
        (top >= -9.0).then_some(-9.0)
    }
    fn ceiling(&self, feet: [f32; 3], _: f32) -> Option<f32> {
        Some(feet[1] + 0.25)
    }
    fn head_contact(&self, _: [f32; 3]) -> bool {
        false
    }
    fn slide(&self, _: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        [delta[0] * 0.5, delta[1] * 0.5]
    }
    fn slide_assisted(&self, _: [f32; 3], delta: [f32; 2], _: u8) -> [f32; 2] {
        [delta[0] * 0.25, delta[1] * 0.25]
    }
    fn max_step(&self) -> f32 {
        0.45
    }
    fn material(&self, _: f32, _: f32, _: f32) -> Option<u32> {
        Some(4)
    }
    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        self.floor(x, z, top).map(|h| (h, Some(5)))
    }
}

/// Issue #169: a perch only adds its floor. Water, event walls, fall return walls, ceilings,
/// head contact, walls and materials are still the terrain's while it is set, near the perch
/// and away from it (the wrapper used to drop the water and trigger queries, so a body on a
/// perch could not swim or hit an event wall).
#[test]
fn perch_ground_keeps_every_terrain_query() {
    use sekiro_sim::body::Ground;
    use sekiro_sim::wire::PerchGround;
    let ground = PerchGround {
        terrain: &Lake,
        perch: Some(Vec3::new(0.0, 2.0, 0.0)),
    };
    for [x, z] in [[0.0, 0.0], [5.0, 5.0]] {
        let feet = [x, 2.0, z];
        let water = ground.water_surface(x, z, f32::INFINITY);
        assert_eq!(water.map(|w| (w.height, w.material)), Some((0.0, 22)));
        let trigger = ground.trigger_contact([x, 3.0, z], feet);
        assert_eq!(
            trigger.map(|t| (t.hit_filter, t.part)),
            Some((13, "event wall".into()))
        );
        assert!(ground.near_trigger("event wall", feet));
        assert!(ground.fall_return_below(x, 2.0, z));
        assert_eq!(ground.ceiling(feet, 1.0), Some(2.25));
        assert!(!ground.head_contact(feet));
        assert_eq!(ground.slide(feet, [1.0, 0.0]), [0.5, 0.0]);
        assert_eq!(ground.slide_assisted(feet, [1.0, 0.0], 1), [0.25, 0.0]);
        assert_eq!(ground.max_step(), 0.45);
        assert_eq!(ground.material(x, z, 0.0), Some(4));
        assert_eq!(ground.lowest_floor(x, z), Some(-12.0));
    }
    // The floor family still has the perch inside its footprint, and the terrain's own
    // answers (wall-hug parts, material) under and beside it.
    assert_eq!(ground.floor(0.0, 0.0, 3.0), Some(2.0));
    assert_eq!(ground.hug_floor(0.0, 0.0, 3.0), Some(2.0));
    assert_eq!(ground.hug_floor(5.0, 5.0, 3.0), Some(-9.0));
    assert_eq!(
        ground.floor_with_material(0.0, 0.0, 3.0),
        Some((2.0, Some(4)))
    );
    assert_eq!(
        ground.floor_with_material(5.0, 5.0, 3.0),
        Some((-10.0, Some(5)))
    );
    assert_eq!(ground.floor(5.0, 5.0, 3.0), Some(-10.0));
}

/// Issue #169: a body dropping off a perch into deep water lands on the water, as it does
/// without the perch, instead of sinking through to the lake bed.
#[test]
fn body_leaving_a_perch_lands_on_the_water() {
    use sekiro_sim::body::BodyInput;
    use sekiro_sim::wire::PerchGround;
    let ground = PerchGround {
        terrain: &Lake,
        perch: Some(Vec3::new(0.0, 2.0, 0.0)),
    };
    let mut body = Body {
        position: [3.0, 2.0, 0.0],
        grounded: false,
        fall_top: 2.0,
        ..Body::default()
    };
    for _ in 0..120 {
        body.step(&BodyInput::default(), 1.0 / 60.0, &ground);
        if body.grounded {
            break;
        }
    }
    assert!(body.grounded, "never landed: {:?}", body.position);
    assert_eq!(body.position[1], 0.0, "landed at {:?}", body.position);
}

/// A Ready hook for the view tests, with the given variation.
fn hook(position: [f32; 3], variation: u32) -> Point {
    Point {
        position: Some(position),
        variation,
        model: "synthetic".into(),
        source_part: format!("hook {position:?}"),
    }
}

/// Issue #178: the game's search only looks through the camera (`WireActionTargetMng`), so a
/// hook behind the camera or off the side of the screen is never marked, and so never pulled
/// to (the old search let WireVariationParam recognition angles of 180 degrees through).
#[test]
fn hooks_behind_the_camera_or_off_screen_are_never_marked() {
    // Wolf's feet at the origin; the camera looks down -Z from behind him.
    let body = Body::default();
    let mut wire = synthetic(12.0);
    let behind = hook([0.0, 2.0, 10.0], 1);
    // In front of the camera but 84 degrees to Wolf's right: off the screen.
    let side = hook([10.0, 0.0, -1.0], 1);
    for (name, point) in [("behind the camera", &behind), ("off screen", &side)] {
        wire.points = vec![point.clone()];
        wire.preview = None;
        wire.search(&body, 0.0, 0.0, false);
        assert!(wire.preview.is_none(), "a hook {name} must not be marked");
        assert!(!wire.can_fire(), "a hook {name} must not be pulled to");
    }
    // With an on-screen hook too, it is the one marked and the one pulled to.
    wire.points = vec![behind, side, hook([0.0, 4.0, -12.0], 1)];
    wire.preview = None;
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.preview.as_ref().map(|t| t.index), Some(2));
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(2));
    // Turning the camera round marks the hook that was behind it.
    wire.preview = None;
    wire.search(&body, std::f32::consts::PI, 0.0, false);
    assert_eq!(wire.preview.as_ref().map(|t| t.index), Some(0));
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(0));
}

/// The marked hook keeps its mark while it stays inside the soft-lock box (7.15 m to each side
/// at 15.8 m), even with another hook nearer the screen centre, and loses it outside.
#[test]
fn marked_hook_keeps_its_soft_lock_inside_the_narrower_box() {
    let body = Body::default();
    let mut wire = synthetic(12.0);
    wire.points = vec![hook([2.5, 2.0, -12.0], 1)];
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(0));
    // A hook straight ahead appears; the marked one is 6 m right, 16.5 m from the camera.
    wire.points[0].position = Some([6.0, 2.0, -12.0]);
    wire.points.push(hook([0.0, 2.0, -12.0], 1));
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.preview.as_ref().map(|t| t.index), Some(0));
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(0));
    // 9 m right leaves the soft-lock box (still inside the view box): the centred hook wins.
    wire.points[0].position = Some([9.0, 2.0, -12.0]);
    wire.search(&body, 0.0, 0.0, false);
    assert_eq!(wire.preview.as_ref().map(|t| t.index), Some(1));
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(1));
}

/// Ashina Outskirts, on the low ground south of the ravine north of start 0 (the live check's
/// trunk spawn): the rideable pine `o002500_0005` is marked when the camera faces it, and not
/// when the camera looks east with the pine about 65 degrees to its right, off the screen (the
/// old search marked and fired at it there).
#[test]
fn outskirts_pine_is_marked_only_when_the_camera_faces_it() {
    let dir = cache().join("maps/m11_00_00_00");
    if !dir.join("grapple.json").exists() {
        return;
    }
    let mut wire = Wire::load(&cache(), &dir).unwrap();
    let body = Body {
        position: [-140.0, -50.73, -132.0],
        ..Body::default()
    };
    wire.search(&body, (-90f32).to_radians(), 0.25, false);
    assert!(wire.preview.is_none() && !wire.can_fire());
    wire.search(&body, 180f32.to_radians(), 0.25, false);
    let marked = wire.preview.as_ref().expect("the pine ahead is marked");
    assert_eq!(wire.points[marked.index].source_part, "o002500_0005");
    assert_eq!(wire.candidate.as_ref().map(|t| t.index), Some(marked.index));
}

/// Whether `point` passes the search's player back filter: horizontally, from Wolf at the
/// origin, within `limit` degrees of the camera's look direction (yaw 0 looks down -Z).
fn passes_back_filter(point: Vec3, limit: f32) -> bool {
    Vec3::new(point.x, 0.0, point.z)
        .normalize()
        .dot(Vec3::NEG_Z)
        .acos()
        .to_degrees()
        < limit
}

/// The view box's sides: a usable, unblocked hook the back filter lets through, but 46.8
/// degrees right of the camera's axis (the box's 14.3 m at 15.8 m is 42.1), is not marked.
#[test]
fn hook_past_the_view_box_width_is_not_marked() {
    let point = Vec3::new(8.0, 1.5, -3.0);
    assert!(passes_back_filter(point, 75.0));
    let mut wire = synthetic(12.0);
    wire.points = vec![hook(point.to_array(), 1)];
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.preview.is_none() && !wire.can_fire());
    // Straight ahead at the same distance from the camera it is marked.
    wire.points = vec![hook([0.0, 1.5, -3.0], 1)];
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.can_fire());
}

/// The view box's top: a hook straight ahead of Wolf but 39 degrees above the camera's axis
/// (the box's 10.2 m at 15.8 m is 32.8) is not marked.
#[test]
fn hook_above_the_view_box_is_not_marked() {
    let point = Vec3::new(0.0, 10.0, -6.0);
    assert!(passes_back_filter(point, 75.0));
    let mut wire = synthetic(12.0);
    wire.points = vec![hook(point.to_array(), 1)];
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.preview.is_none() && !wire.can_fire());
    // Pitching the camera up to it marks it.
    wire.search(&Body::default(), 0.0, -0.4, false);
    assert!(wire.can_fire());
}

/// The check ray's start: a hook inside the view box's angles and just ahead of Wolf, but only
/// 0.7 m in front of a camera looking almost straight down at him, is not marked.
#[test]
fn hook_before_the_check_ray_start_is_not_marked() {
    let body = Body::default();
    let mut wire = synthetic(12.0);
    let pitch = 85f32.to_radians();
    let camera = wire.search_camera(&body, 0.0, pitch);
    let up = camera.up().unwrap();
    // 0.7 m along the camera's axis, then up the screen until 5 cm ahead of Wolf's feet.
    let on_axis = camera.position + camera.forward * 0.7;
    let rise = (-0.05 - on_axis.z) / up.z;
    let point = on_axis + up * rise;
    assert!((point - camera.position).dot(camera.forward) < sekiro_sim::wire::CHECK_RAY_START);
    assert!(rise / 0.7 < 10.2 / 15.8, "inside the box's angles");
    assert!(passes_back_filter(point, 75.0));
    assert!(point.length() > 3.0, "usable");
    wire.points = vec![hook(point.to_array(), 1)];
    wire.search(&body, 0.0, pitch, false);
    assert!(wire.preview.is_none() && !wire.can_fire());
}

/// Locked on, the back filter lets hooks up to 135 degrees from the camera's look direction
/// through (field +0x14c, 45): a hook 127 degrees round, still on screen, is marked only then.
#[test]
fn locked_on_back_filter_reaches_one_hundred_and_thirty_five_degrees() {
    let point = Vec3::new(2.4, 3.0, 1.8);
    assert!(!passes_back_filter(point, 75.0) && passes_back_filter(point, 135.0));
    let mut wire = synthetic(12.0);
    wire.points = vec![hook(point.to_array(), 1)];
    wire.search(&Body::default(), 0.0, 0.0, false);
    assert!(wire.preview.is_none());
    wire.search(&Body::default(), 0.0, 0.0, true);
    assert!(wire.can_fire());
}

/// The search's camera is built from the step's yaw and pitch and the simulation's body and
/// rig, pulled in front of a wall behind Wolf as the camera on screen is.
#[test]
fn search_camera_follows_the_rig_and_is_pulled_in_by_walls() {
    use sekiro_formats::hknp::CollisionMesh;
    let body = Body::default();
    let mut wire = synthetic(12.0);
    let camera = wire.search_camera(&body, 0.0, 0.0);
    assert!(camera.position.distance(Vec3::new(0.0, 1.5, 4.5)) < 1e-5);
    wire.rig.distance = 3.0;
    wire.rig.focus_height = 1.2;
    let camera = wire.search_camera(&body, 0.0, 0.0);
    assert!(camera.position.distance(Vec3::new(0.0, 1.2, 3.0)) < 1e-5);
    // A wall 2 m behind Wolf: the camera stops 0.3 m in front of it.
    wire.collision = Some(CollisionWorld::from_mesh(&CollisionMesh {
        vertices: vec![[-50.0, -50.0, 2.0], [50.0, -50.0, 2.0], [0.0, 50.0, 2.0]],
        indices: vec![0, 1, 2],
        ..CollisionMesh::default()
    }));
    let camera = wire.search_camera(&body, 0.0, 0.0);
    assert!(
        (camera.position.z - 1.7).abs() < 1e-4,
        "{:?}",
        camera.position
    );
}

/// An upward flight to a hook 10 m away and 5 m up under a plate jutting over the flight's
/// line from the side, its edge `edge` m off it (#168): the head sphere never goes more than
/// 1 cm into the plate, and the flight releases on meeting it. Before, the flight cast rays only
/// along the body's centre line, which pass beside the edge.
#[test]
fn an_upward_flight_under_an_overhang_stops_before_the_head_enters_it() {
    use sekiro_formats::hknp::CollisionMesh;
    use sekiro_sim::collision::{HEAD_TOP, RADIUS};
    // The flight with `collision`: the feet each tick until it releases or arrives.
    let fly = |collision: Option<CollisionWorld>| {
        let mut wire = synthetic(10.0);
        wire.points[0].position = Some([0.0, 5.0, -10.0]);
        let mut body = Body::default();
        wire.search(&body, 0.0, 0.0, false);
        wire.step(
            &mut body,
            Some("a000_202000"),
            None,
            1.0 / 60.0,
            &FlatGround::default(),
        );
        wire.collision = collision;
        let mut path = Vec::new();
        for _ in 0..50 {
            wire.step(
                &mut body,
                Some("a000_202011"),
                Some((0.0, 0.83)),
                1.0 / 60.0,
                &FlatGround::default(),
            );
            path.push(Vec3::from(body.position));
            if wire.active.is_none() {
                break;
            }
        }
        (path, wire.active.is_some())
    };
    let (free, _) = fly(None);
    // Where the head sphere's centre passes z -5, rising.
    let mid = free
        .iter()
        .find(|p| p.z < -5.0)
        .expect("the flight passes z -5");
    let centre = mid.y + HEAD_TOP - RADIUS;
    for edge in [0.15, 0.3] {
        // Over the centre's height there, so the head rises into it.
        let y = centre + 0.2;
        let (lo, hi) = (Vec3::new(edge, y, -7.0), Vec3::new(3.0, y, -3.0));
        let plate = CollisionWorld::from_mesh(&CollisionMesh {
            vertices: vec![
                [lo.x, y, lo.z],
                [hi.x, y, lo.z],
                [hi.x, y, hi.z],
                [lo.x, y, hi.z],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..CollisionMesh::default()
        });
        let (path, flying) = fly(Some(plate));
        let deepest = path
            .iter()
            .map(|feet| {
                let head = *feet + Vec3::Y * (HEAD_TOP - RADIUS);
                RADIUS - head.distance(head.clamp(lo, hi))
            })
            .fold(f32::MIN, f32::max);
        assert!(
            deepest <= 0.01,
            "edge {edge}: the head went {deepest:.3} m into the plate"
        );
        assert!(!flying, "edge {edge}: the flight never met the plate");
    }
}

/// The same flight starting with the head sphere grazing, by less than a millimetre, the lower
/// edge of a beam behind it and just over its centre (#168 review): the flight moves away from
/// the beam, so the head sweep does not count the graze and the flight still arrives. Before,
/// any face over the head's centre that it already grazed stopped the flight on its first tick.
#[test]
fn a_flight_away_from_a_beam_the_head_grazes_still_arrives() {
    use sekiro_formats::hknp::CollisionMesh;
    use sekiro_sim::collision::{HEAD_TOP, RADIUS};
    let centre = HEAD_TOP - RADIUS;
    let (y0, y1) = (centre + 0.05, centre + 0.35);
    // The beam's lower near edge 0.9995 radii from the head's centre.
    let z0 = ((RADIUS * 0.9995).powi(2) - 0.05f32.powi(2)).sqrt();
    let z1 = z0 + 0.5;
    let mut m = CollisionMesh::default();
    for q in [
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
    let beam = CollisionWorld::from_mesh(&m);
    // Grazing it, the head sweep meets the beam moving into it and not moving away or down.
    let feet = Vec3::ZERO;
    for (delta, meets) in [
        (Vec3::new(0.0, 0.0, 0.2), true),
        (Vec3::new(0.0, 0.0, -0.2), false),
        (Vec3::new(0.0, -0.2, 0.0), false),
        (Vec3::new(0.0, 0.2, -0.17), false),
    ] {
        assert_eq!(beam.head_sweep(feet, delta).is_some(), meets, "{delta}");
    }
    let mut wire = synthetic(10.0);
    wire.points[0].position = Some([0.0, 5.0, -10.0]);
    let mut body = Body::default();
    wire.search(&body, 0.0, 0.0, false);
    wire.step(
        &mut body,
        Some("a000_202000"),
        None,
        1.0 / 60.0,
        &FlatGround::default(),
    );
    wire.collision = Some(beam);
    for _ in 0..50 {
        wire.step(
            &mut body,
            Some("a000_202011"),
            Some((0.0, 0.83)),
            1.0 / 60.0,
            &FlatGround::default(),
        );
    }
    let distance = Vec3::from(body.position).distance(Vec3::new(0.0, 5.0, -10.0));
    assert!(distance < 0.2, "ended {distance:.2} m from the hook");
}

/// Issue #169 on the real map: on the Outskirts ravine branch (the perch the map tests use,
/// `map_world::standing_part_survives_the_branch_and_follows_teleports`) the event walls and
/// the fall return wall below are the same through the perch as without it.
#[test]
fn ravine_branch_perch_keeps_the_map_triggers() {
    use sekiro_sim::body::Ground;
    use sekiro_sim::wire::PerchGround;
    let Some(world) = crate::step_cost::play_world() else {
        return;
    };
    let world: &dyn Ground = world.as_ref();
    for perch in [
        Vec3::new(-137.6, -44.3, -124.5),
        Vec3::new(-141.58, -44.26, -123.86),
    ] {
        let ground = PerchGround {
            terrain: world,
            perch: Some(perch),
        };
        let feet = perch.to_array();
        let below = [perch.x, perch.y - 40.0, perch.z];
        let direct = (
            world.trigger_contact(feet, below).map(|t| t.part),
            world.fall_return_below(perch.x, perch.y, perch.z),
            world
                .water_surface(perch.x, perch.z, f32::INFINITY)
                .map(|w| w.height),
        );
        let perched = (
            ground.trigger_contact(feet, below).map(|t| t.part),
            ground.fall_return_below(perch.x, perch.y, perch.z),
            ground
                .water_surface(perch.x, perch.z, f32::INFINITY)
                .map(|w| w.height),
        );
        eprintln!("{perch}: {direct:?}");
        assert_eq!(perched, direct, "{perch}");
        assert!(
            direct.0.is_some() || direct.1,
            "{perch}: no trigger below, {direct:?}"
        );
    }
}

/// Grapples the synthetic duel's Wolf to its rideable hook raised to `[0, 5, -10]` and steps
/// until he stands on the perch. `None` without the cache.
fn perched_duel() -> Option<Duel> {
    let mut duel = synthetic_duel()?;
    duel.player.wire.points[0].position = Some([0.0, 5.0, -10.0]);
    for tick in 0..240 {
        let input = InputFrame {
            buttons: Buttons {
                grapple: tick == 0,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        duel.step(&input, 1.0 / 60.0);
    }
    let perch = duel
        .player
        .wire
        .perch
        .expect("Wolf did not land on the perch");
    let feet = Vec3::from(duel.player.body.position);
    eprintln!("perched at {feet}, perch {perch}");
    assert!(feet.distance(Vec3::new(0.0, 5.0, -10.0)) < 0.2, "{feet}");
    assert!(duel.player.body.grounded);
    Some(duel)
}

/// Issue #230: once Wolf walks off a perch it is no longer ground, so a later fall through the
/// perch point (here dropped 3 m above it) lands on the terrain below. Before, `Wire::perch`
/// stayed set for the rest of the session and held him up 5 m in the air there.
#[test]
fn walking_off_a_perch_clears_it() {
    let Some(mut duel) = perched_duel() else {
        return;
    };
    let walk = InputFrame::toward_world_yaw(0.0, 1.0);
    let mut left_at = None;
    for tick in 0..240 {
        duel.step(&walk, 1.0 / 60.0);
        if left_at.is_none() && duel.player.wire.perch.is_none() {
            left_at = Some(Vec3::from(duel.player.body.position));
            eprintln!("perch cleared at tick {tick}, {}", left_at.unwrap());
        }
    }
    for _ in 0..120 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let left_at = left_at.expect("the perch was never cleared after walking off it");
    // Cleared as he left its 0.6 m footprint, not later.
    let off = left_at.x.hypot(left_at.z + 10.0);
    assert!((0.6..1.2).contains(&off), "cleared {off} m from the perch");
    assert!(duel.player.body.grounded);
    assert!(duel.player.body.position[1].abs() < 0.05);

    // Back over the perch point from above: nothing holds him there any more.
    duel.player.body.position = [0.0, 8.0, -10.0];
    duel.player.body.grounded = false;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let feet = Vec3::from(duel.player.body.position);
    eprintln!("dropped over the old perch, now at {feet}");
    assert!(duel.player.body.grounded);
    assert!(feet.y.abs() < 0.05, "held up at {feet}");
}

/// A jump straight up from a perch lands back on it (FwLo's RustSekiro keeps the perch through
/// a jump while Wolf stays over it), and a new grapple shot leaves it behind.
#[test]
fn a_jump_keeps_the_perch_and_a_new_shot_clears_it() {
    let Some(mut duel) = perched_duel() else {
        return;
    };
    let jump = InputFrame {
        buttons: Buttons {
            jump: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    };
    duel.step(&jump, 1.0 / 60.0);
    let mut airborne = false;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        airborne |= !duel.player.body.grounded;
    }
    let feet = Vec3::from(duel.player.body.position);
    eprintln!("after the jump at {feet}");
    assert!(airborne);
    assert!(duel.player.body.grounded);
    assert!((feet.y - 5.0).abs() < 0.1, "{feet}");
    assert!(duel.player.wire.perch.is_some());

    // A second rideable hook further on: the shot at it ends the first perch.
    let mut second = duel.player.wire.points[0].clone();
    second.position = Some([0.0, 5.0, -20.0]);
    second.source_part = "second".into();
    duel.player.wire.points.push(second);
    let first = duel.player.wire.perch;
    let mut cleared = false;
    for tick in 0..240 {
        let input = InputFrame {
            buttons: Buttons {
                grapple: tick == 0,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        duel.step(&input, 1.0 / 60.0);
        cleared |= duel.player.wire.active.is_some() && duel.player.wire.perch.is_none();
    }
    let feet = Vec3::from(duel.player.body.position);
    eprintln!("second perch {:?}, feet {feet}", duel.player.wire.perch);
    assert!(cleared, "the first perch {first:?} outlived the shot");
    assert!(feet.distance(Vec3::new(0.0, 5.0, -20.0)) < 0.2, "{feet}");
}
