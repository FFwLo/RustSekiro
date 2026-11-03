use glam::Vec3;
use sekiro_sim::body::{Body, Ground};
use sekiro_sim::docking::{Docking, HANG_END_MARGIN, MapEdge};
use sekiro_sim::duel::Duel;
use sekiro_sim::{Buttons, InputFrame};
use std::path::PathBuf;

struct Step;
impl Ground for Step {
    fn height(&self, _: f32, z: f32) -> Option<f32> {
        Some(if z >= 0.0 { 0.0 } else { -4.0 })
    }
}
fn synthetic() -> Docking {
    Docking::new(vec![MapEdge {
        kind: 2,
        variation: 0,
        start: [-2.0, 0.0, 0.0],
        end: [2.0, 0.0, 0.0],
        source_part: "synthetic".into(),
        chain: 0,
    }])
}
fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

fn press(buttons: Buttons) -> InputFrame {
    InputFrame {
        buttons,
        ..InputFrame::default()
    }
}

#[test]
fn closest_reach_facing_and_ends() {
    let mut docking = synthetic();
    assert_eq!(
        docking.edges[0].closest(Vec3::new(4.0, 9.0, 2.0)),
        Vec3::new(2.0, 0.0, 0.0)
    );
    let mut body = Body {
        position: [0.0, 0.0, 0.4],
        ..Body::default()
    };
    docking.search(&body, &Step);
    let target = docking.candidate.as_ref().unwrap();
    assert_eq!(target.outward, -Vec3::Z);
    assert_eq!(
        target.floor,
        Vec3::new(0.0, 0.0, sekiro_sim::docking::CLIMB_INWARD)
    );
    body.yaw = std::f32::consts::PI;
    docking.search(&body, &Step);
    assert!(docking.candidate.is_none());
    body.yaw = 0.0;
    body.position[2] = 1.1;
    docking.search(&body, &Step);
    assert!(docking.candidate.is_none());
    body.position = [1.99, 0.0, 0.4];
    docking.search(&body, &Step);
    assert_eq!(docking.candidate.unwrap().end, 1);
}

#[test]
fn short_ledge_allows_hanging_at_its_midpoint() {
    let mut docking = synthetic();
    docking.edges[0].start[0] = -0.2;
    docking.edges[0].end[0] = 0.2;
    let target = docking
        .target(0, Vec3::new(0.15, 0.0, 0.4), &Step)
        .expect("a short edge still supports hanging");
    assert_eq!(target.point, Vec3::ZERO);
    assert!(target.end_blocked(true));
    assert!(target.end_blocked(false));
    let mut body = Body::default();
    docking.active = Some(target);
    for motion in [-1.0, 1.0] {
        assert!(docking.step(
            &mut body,
            Some("a000_020202"),
            0.0,
            None,
            [motion, 0.0, 0.0, 0.0],
            &Step,
        ));
        assert_eq!(docking.active.as_ref().unwrap().point, Vec3::ZERO);
    }
}

#[test]
fn hang_travel_updates_endpoint_flags_with_reversed_edges() {
    for reversed in [false, true] {
        let mut docking = synthetic();
        if reversed {
            let edge = &mut docking.edges[0];
            std::mem::swap(&mut edge.start, &mut edge.end);
        }
        docking.active = docking.target(0, Vec3::new(0.0, 0.0, 0.4), &Step);
        let mut body = Body::default();
        // Facing the wall (outward -Z), +X is Wolf's left: the end bits follow his sides,
        // not the edge's stored order.
        for (motion, end) in [(100.0, 1), (-100.0, 2)] {
            assert!(docking.step(
                &mut body,
                Some("a000_020202"),
                0.0,
                None,
                [motion, 0.0, 0.0, 0.0],
                &Step,
            ));
            let target = docking.active.as_ref().unwrap();
            assert_eq!(target.end, end);
            assert_eq!(target.end_blocked(true), motion > 0.0);
            assert_eq!(target.end_blocked(false), motion < 0.0);
        }
    }
}

#[test]
fn researched_edge_has_upper_floor_and_outward_drop() {
    let dir = cache().join("maps/m11_01_00_00");
    if !dir.join("edges.json").exists() || !dir.join("collision.bin").exists() {
        return;
    }
    let docking = Docking::load(&dir.join("edges.json")).unwrap();
    let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
    let mut found = false;
    for (i, edge) in docking
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.source_part == "h019900" && e.chain == 7 && e.kind == 2)
    {
        let point = (Vec3::from(edge.start) + Vec3::from(edge.end)) * 0.5;
        let target = docking.target(i, point, &world);
        eprintln!(
            "research edge {:?} -> {:?}; target={target:?}",
            edge.start, edge.end
        );
        if let Some(t) = target {
            assert!((t.floor.y - point.y).abs() <= 0.3);
            found = true;
        }
    }
    assert!(found, "research edge does not sit on a collision ledge");
}

fn hanging(variation: u8) -> Option<Duel> {
    if !cache().join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return None;
    }
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.ground = Box::new(Step);
    duel.player.docking = synthetic();
    duel.player.docking.edges[0].variation = variation;
    duel.player.body.position = [0.0, 0.0, 0.4];
    let dt = 1.0 / 60.0;
    for _ in 0..30 {
        duel.step(&InputFrame::default(), dt);
    }
    let action = press(Buttons {
        action: true,
        ..Buttons::default()
    });
    let r = duel.step(&action, dt);
    assert!(
        r.player
            .behavior
            .events
            .iter()
            .any(|e| e.starts_with("W_StandToHang")),
        "{:?}",
        r.player
    );
    let mut idle = false;
    for _ in 0..180 {
        let r = duel.step(&InputFrame::default(), dt);
        idle |= r.player.animation.as_deref()
            == Some(if variation == 0 {
                "a000_020000"
            } else {
                "a000_020010"
            });
    }
    assert!(idle, "did not reach HangIdle");
    assert!((duel.player.body.position[1] + sekiro_sim::docking::HANG_DEPTH).abs() < 0.01);
    Some(duel)
}

#[test]
fn hks_ground_hang_climb_and_drop() {
    let Some(mut duel) = hanging(0) else { return };
    let dt = 1.0 / 60.0;
    let action = press(Buttons {
        action: true,
        ..Buttons::default()
    });
    let r = duel.step(&action, dt);
    assert_eq!(r.player.animation.as_deref(), Some("a000_217500"));
    for _ in 0..180 {
        duel.step(&InputFrame::default(), dt);
    }
    assert!(duel.player.body.grounded);
    assert!(
        Vec3::from(duel.player.body.position).distance(Vec3::new(
            0.0,
            0.0,
            sekiro_sim::docking::CLIMB_INWARD
        )) < 0.01
    );
    duel.player.body.position = [0.0, 0.0, 0.4];
    duel.player.body.yaw = 0.0;
    duel.step(&action, dt);
    for _ in 0..180 {
        duel.step(&InputFrame::default(), dt);
    }
    let drop = press(Buttons {
        dodge: true,
        ..Buttons::default()
    });
    let r = duel.step(&drop, dt);
    assert!(
        r.player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_HangToFreeFall")
    );
    assert!(duel.player.docking.active.is_none());
    assert!(!duel.player.body.grounded);
}

#[test]
fn castle_start_two_route() {
    let dir = cache().join("maps/m11_01_00_00");
    if !dir.join("edges.json").exists() {
        return;
    }
    for script_name in ["m11_01_ledge_hang.txt", "m11_01_ledge_shimmy.txt"] {
        let mut duel = Duel::load(&cache(), 4.0).unwrap();
        duel.enemy_active = false;
        let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
        let y = world.floor(83.307, 243.649, 31.0).unwrap();
        duel.player.ground = Box::new(world);
        duel.player.docking = Docking::load(&dir.join("edges.json")).unwrap();
        duel.player.body.position = [83.307, y, 243.649];
        duel.player.body.yaw = 133.216f32.to_radians();
        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../game/scripts")
                .join(script_name),
        )
        .unwrap();
        let frames = sekiro_sim::input::parse_action_script(&script).unwrap();
        let mut entered = false;
        let mut idle = false;
        let mut climbed = false;
        let mut edge_point = None;
        let mut idle_frames = 0;
        let mut first_point = None;
        let mut max_travel = 0.0f32;
        for (step, mut frame) in frames.into_iter().enumerate() {
            frame.camera_yaw += 133.216f32.to_radians();
            let r = duel.step(&frame, 1.0 / 60.0);
            let hanging = r.player.animation.as_deref() == Some("a000_020000");
            idle_frames += usize::from(hanging);
            if hanging && idle_frames == 16 {
                let bones = duel.player.bone_source_matrices();
                let world = duel.player.source_to_world();
                for (i, bone) in duel
                    .player
                    .skeleton
                    .bones
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.name.ends_with("Hand"))
                {
                    let hand = world.transform_point3(bones[i].transform_point3(Vec3::ZERO));
                    // The hands rest on the lip: at its height and on its line (the floor right at
                    // the line can be the drop below, so the lip itself is the reference).
                    let lip = duel.player.docking.active.as_ref().unwrap();
                    assert!(
                        (hand.y - lip.point.y).abs() < 0.1,
                        "{} floats above the ledge: {hand:?} lip={:?}",
                        bone.name,
                        lip.point
                    );
                    assert!(
                        (hand - lip.point).dot(lip.outward).abs() < 0.1,
                        "{} off the lip line: {hand:?} lip={:?}",
                        bone.name,
                        lip.point
                    );
                    eprintln!("hang hand {} at {:?}", bone.name, hand);
                }
            }
            entered |= r
                .player
                .behavior
                .events
                .iter()
                .any(|e| e.starts_with("W_StandToHang"));
            idle |= r.player.animation.as_deref() == Some("a000_020000");
            climbed |= r.player.animation.as_deref() == Some("a000_217500");
            if let Some(t) = &duel.player.docking.active {
                edge_point = Some(t.point);
                let first = *first_point.get_or_insert(t.point);
                max_travel = max_travel.max(first.distance(t.point));
            }
            if step % 10 == 0
                || !r.player.behavior.events.is_empty()
                || duel.player.docking.candidate.is_some()
            {
                eprintln!(
                    "route step {step} pos={:?} env3044={} candidate={:?} anim={:?} events={:?}",
                    duel.player.body.position,
                    duel.player.behavior.env.hang_candidate,
                    duel.player.docking.candidate,
                    r.player.animation,
                    r.player.behavior.events
                );
            }
        }
        assert!(entered && idle && climbed);
        assert!(idle_frames >= 16, "hand placement was checked");
        if script_name == "m11_01_ledge_shimmy.txt" {
            assert!(max_travel > 1.0, "did not shimmy: {max_travel}");
            assert!(
                first_point.unwrap().distance(edge_point.unwrap()) < 0.1,
                "did not return along the edge"
            );
        }
        assert!(duel.player.body.grounded);
        // On the top, the climb's own root motion in from the lip.
        assert!(
            Vec3::from(duel.player.body.position).distance(edge_point.unwrap())
                < sekiro_sim::docking::CLIMB_INWARD + 0.1
        );
    }
}

// Adapted from RustSekiro src/sim_tests/part12.rs: shimmy, climb and drop.
#[test]
fn shimmy_both_ways_stops_at_ends_and_can_climb_or_drop() {
    for (variation, climb, clip) in [
        (0, false, "a000_217520"),
        (0, true, "a000_217500"),
        (1, false, "a000_217620"),
        (1, true, "a000_217600"),
    ] {
        let Some(mut duel) = hanging(variation) else {
            return;
        };
        let dt = 1.0 / 60.0;
        let idle = InputFrame::default();
        let edge = &duel.player.docking.edges[0];
        let half_span =
            Vec3::from(edge.start).distance(Vec3::from(edge.end)) * 0.5 - HANG_END_MARGIN;
        let initial = Vec3::from(duel.player.body.position);
        for side in [-1.0, 1.0] {
            let moving = InputFrame {
                move_stick: [side, 0.0],
                camera_yaw: std::f32::consts::PI,
                ..idle
            };
            let before = duel.player.body.position[0];
            let mut clips = std::collections::HashSet::new();
            for _ in 0..60 {
                clips.insert(duel.step(&moving, dt).player.animation.unwrap_or_default());
            }
            let after = duel.player.body.position[0];
            assert!(
                (after - before) * -side > 0.2,
                "variation {variation}, side {side}: no shimmy {before} -> {after}, {clips:?}"
            );
            for _ in 0..600 {
                duel.step(&moving, dt);
            }
            let at = Vec3::from(duel.player.body.position);
            assert!((at.x + side * half_span).abs() < 0.01, "end: {at:?}");
            assert!(
                (at.y - initial.y).abs() < 0.01 && (at.z - initial.z).abs() < 0.01,
                "left edge line: {at:?}"
            );
            for _ in 0..40 {
                duel.step(&idle, dt);
            }
        }
        let along = duel.player.body.position[0];
        let leave = press(Buttons {
            action: climb,
            dodge: !climb,
            ..Buttons::default()
        });
        let r = duel.step(&leave, dt);
        assert_eq!(r.player.animation.as_deref(), Some(clip));
        for _ in 0..180 {
            duel.step(&idle, dt);
        }
        assert!(duel.player.docking.active.is_none());
        assert!((duel.player.body.position[0] - along).abs() < 0.01);
        if climb {
            assert!(duel.player.body.grounded);
            assert!(duel.player.body.position[1].abs() < 0.01);
            assert!(duel.player.body.position[2] > 0.0);
        } else {
            assert!(duel.player.body.position[1] < initial.y - 1.0);
        }
    }
}

/// [`synthetic`]'s ledge with a second edge joined on at its end (2, 0, 0), stored end to
/// start like the extractor's chains, turning by `turn` degrees toward the floor side (+Z).
fn two_edge_ledge(turn: f32) -> (Docking, Vec3) {
    let mut docking = synthetic();
    let dir = Vec3::new(turn.to_radians().cos(), 0.0, turn.to_radians().sin());
    let start = Vec3::new(2.0, 0.0, 0.0);
    docking.edges.push(MapEdge {
        start: start.to_array(),
        end: (start + dir * 3.0).to_array(),
        ..docking.edges[0].clone()
    });
    docking.rebuild_links();
    docking.active = docking.target(0, Vec3::new(0.0, 0.0, 0.4), &Step);
    (docking, dir)
}

/// Shimmy `ticks` hang steps of `motion` metres along the wall; the target after the last.
fn shimmy(docking: &mut Docking, motion: f32, ticks: usize) -> sekiro_sim::docking::Target {
    let mut body = Body::default();
    for _ in 0..ticks {
        assert!(docking.step(
            &mut body,
            Some("a000_020202"),
            0.0,
            None,
            [motion, 0.0, 0.0, 0.0],
            &Step,
        ));
    }
    docking.active.clone().unwrap()
}

/// A ledge is a chain of edges (#147): the shimmy goes on over a gentle joint onto the next
/// edge, keeps to the same side of the ledge, and stops only near the chain's own ends.
#[test]
fn hang_travel_follows_the_edge_chain_round_a_corner() {
    let (mut docking, dir) = two_edge_ledge(15.0);
    // Facing the wall (outward -Z), Wolf's left is +X, toward the joint.
    let target = shimmy(&mut docking, 0.1, 60);
    assert_eq!(target.edge, 1, "{target:?}");
    let outward = Vec3::new(dir.z, 0.0, -dir.x);
    assert!(target.outward.distance(outward) < 1e-4, "{target:?}");
    assert_eq!(target.end, 1, "at the ledge's left end: {target:?}");
    let far = Vec3::new(2.0, 0.0, 0.0) + dir * (3.0 - HANG_END_MARGIN);
    assert!(target.point.distance(far) < 1e-4, "{target:?}");
    assert!(target.end_blocked(true) && !target.end_blocked(false));
    // Back over the joint to the first edge's free end, the ledge's right end.
    let target = shimmy(&mut docking, -0.1, 80);
    assert_eq!(target.edge, 0, "{target:?}");
    assert!(target.outward.distance(-Vec3::Z) < 1e-4, "{target:?}");
    assert_eq!(target.end, 2);
    assert!(
        target
            .point
            .distance(Vec3::new(-2.0 + HANG_END_MARGIN, 0.0, 0.0))
            < 1e-4
    );
}

/// A sharp joint is a ledge end: the real game turns it with its hang corner moves, so the
/// shimmy stops [`HANG_END_MARGIN`] short of a 90 degree corner, blocked on that side.
#[test]
fn hang_travel_stops_at_a_sharp_corner() {
    let (mut docking, _) = two_edge_ledge(90.0);
    let target = shimmy(&mut docking, 0.1, 60);
    assert_eq!(target.edge, 0, "{target:?}");
    assert!(target.outward.distance(-Vec3::Z) < 1e-4, "{target:?}");
    assert_eq!(target.end, 1, "{target:?}");
    assert!(
        target
            .point
            .distance(Vec3::new(2.0 - HANG_END_MARGIN, 0.0, 0.0))
            < 1e-4
    );
}

/// The wall in m11_00's foggy pit (#147): one foot-hold ledge of seven edges (h013500 chain
/// 1). Hung from its second edge, Wolf shimmies to both of the ledge's ends, not each edge's.
#[test]
fn snake_pit_shimmy_runs_the_whole_ledge() {
    let dir = cache().join("maps/m11_00_00_00");
    if !dir.join("edges.json").exists() || !dir.join("collision.bin").exists() {
        return;
    }
    let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
    let docking = Docking::load(&dir.join("edges.json")).unwrap();
    let ledge: Vec<usize> = (0..docking.edges.len())
        .filter(|&i| {
            let e = &docking.edges[i];
            e.source_part == "h013500" && e.chain == 1 && e.kind == 2
        })
        .collect();
    assert_eq!(ledge.len(), 7);
    let start = ledge[1];
    let e = &docking.edges[start];
    let mid = (Vec3::from(e.start) + Vec3::from(e.end)) * 0.5;
    let target = docking.target(start, mid, &world).unwrap();
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.body.position = target.floor.to_array();
    duel.player.body.yaw = (-target.outward.x).atan2(-target.outward.z);
    duel.player.ground = Box::new(world);
    duel.player.docking = docking;
    let dt = 1.0 / 60.0;
    for _ in 0..30 {
        duel.step(&InputFrame::default(), dt);
    }
    let action = press(Buttons {
        action: true,
        ..Buttons::default()
    });
    let r = duel.step(&action, dt);
    assert!(
        r.player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_StandToHangFootHold1"),
        "{:?}",
        r.player.behavior.events
    );
    for _ in 0..90 {
        duel.step(&InputFrame::default(), dt);
    }
    // Docking left (stick -1) runs toward the chain's first edge's start, right to the last's end.
    let edges = &duel.player.docking.edges;
    let ends = [
        (-1.0, ledge[0], Vec3::from(edges[ledge[0]].start)),
        (1.0, ledge[6], Vec3::from(edges[ledge[6]].end)),
    ];
    for (side, edge, end) in ends {
        let target = (0..900)
            .find_map(|_| {
                let yaw = duel.player.docking.active.as_ref().unwrap().yaw();
                let moving = InputFrame {
                    move_stick: [side, 0.0],
                    camera_yaw: yaw,
                    ..InputFrame::default()
                };
                let r = duel.step(&moving, dt);
                let t = duel.player.docking.active.clone().expect("still hanging");
                (r.player.animation.as_deref() == Some("a000_020010") && t.end != 0).then_some(t)
            })
            .unwrap_or_else(|| {
                panic!(
                    "side {side}: never settled at an end: {:?}",
                    duel.player.docking.active
                )
            });
        assert_eq!(target.edge, edge, "side {side}: stopped short: {target:?}");
        let gap = Vec3::new(target.point.x - end.x, 0.0, target.point.z - end.z).length();
        assert!(
            (gap - HANG_END_MARGIN).abs() < 0.02,
            "side {side}: {gap} m from the ledge's end"
        );
        // The outward carried along the chain is the face the floor probe finds there.
        let probed = duel
            .player
            .docking
            .target(edge, target.point, duel.player.ground.as_ref())
            .expect("a hang face at the ledge's end");
        assert!(
            target.outward.distance(probed.outward) < 1e-3,
            "side {side}: {target:?} vs {probed:?}"
        );
    }
}
