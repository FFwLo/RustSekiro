use glam::Vec3;
use sekiro_sim::body::{Body, Ground, root_motion_world};
use sekiro_sim::docking::{Docking, MapEdge};
use sekiro_sim::duel::Duel;
use sekiro_sim::{Buttons, InputFrame};

struct Wall;
impl Ground for Wall {
    fn height(&self, _: f32, _: f32) -> Option<f32> {
        Some(0.0)
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        [delta[0], (feet[2] + delta[1]).max(0.3) - feet[2]]
    }
}
fn synthetic(variation: u8) -> Docking {
    Docking::new(vec![MapEdge {
        kind: 3,
        variation,
        start: [-2.0, 0.4, 0.0],
        end: [2.0, 0.4, 0.0],
        source_part: "wall".into(),
        chain: 1,
    }])
}
#[test]
fn cover_search_facing_variations_and_slide_clamps() {
    for variation in [0, 1, 2, 3, 10, 11, 12, 13] {
        let mut docking = synthetic(variation);
        let mut body = Body {
            position: [0.0, 0.0, 0.7],
            ..Body::default()
        };
        docking.search(&body, &Wall);
        assert!(docking.candidate.is_some());
        assert!(docking.cover_step(&mut body, Some("a000_218103"), None, 0.0, &Wall));
        assert!(body.grounded);
        assert!((body.position[2] - 0.7).abs() < 1e-6);
        let anchored = body.position;
        for yaw in [0.0, std::f32::consts::PI] {
            docking.cover_step(&mut body, Some("a000_021200"), Some(yaw), 0.1, &Wall);
            assert_eq!(
                body.position, anchored,
                "wall-normal input slid along the wall"
            );
        }
        // Ends are named facing the wall: with Wolf's back to it (facing +Z), -X is left.
        for (yaw, x, end) in [
            (std::f32::consts::FRAC_PI_2, -2.0, 1),
            (-std::f32::consts::FRAC_PI_2, 2.0, 2),
        ] {
            for _ in 0..100 {
                docking.cover_step(&mut body, Some("a000_021200"), Some(yaw), 0.1, &Wall);
            }
            assert!((body.position[0] - x).abs() < 1e-6);
            assert_eq!(docking.active.as_ref().unwrap().end, end);
        }
        assert!(!docking.cover_step(&mut body, Some("a000_218490"), None, 0.0, &Wall));
        assert!(docking.active.is_none() && body.grounded);
        body.yaw = std::f32::consts::PI;
        docking.search(&body, &Wall);
        assert!(docking.candidate.is_none());
    }
}
#[test]
fn hks_cover_enter_idle_slide_peek_and_exit() {
    use rayon::prelude::*;
    // Each case builds its own duel, so they run side by side.
    let cases = [
        (0, false),
        (1, false),
        (2, false),
        (3, false),
        (10, false),
        (11, false),
        (12, false),
        (13, false),
        (1, true),
    ];
    cases
        .par_iter()
        .for_each(|&(variation, away)| hks_cover(variation, away));
}

const DT: f32 = 1.0 / 60.0;

fn press() -> InputFrame {
    InputFrame {
        buttons: Buttons {
            action: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

/// Wolf docked into cover on the synthetic wall of this variation, idle, with his back to it
/// (facing +Z) and the camera behind him, so stick left ([`shimmy`]) slides him to +X.
fn docked(variation: u8) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return None;
    }
    let mut duel = Duel::load(&cache, 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.ground = Box::new(Wall);
    duel.player.docking = synthetic(variation);
    duel.player.body.position = [0.0, 0.0, 0.7];
    for _ in 0..30 {
        duel.step(&InputFrame::default(), DT);
    }
    let r = duel.step(&press(), DT);
    assert!(
        r.player
            .behavior
            .events
            .iter()
            .any(|e| e.starts_with("W_StandToCover2")),
        "{:?}",
        r.player
    );
    let mut idle = false;
    for _ in 0..180 {
        let r = duel.step(&InputFrame::default(), DT);
        idle |= r.player.animation.as_deref() == Some("a000_021000");
    }
    assert!(idle, "variation {variation}: no cover idle");
    assert!(duel.player.docking.active.is_some(), "not in cover");
    Some(duel)
}

/// The stick along the wall with the camera behind Wolf: -1 is left, 1 is right.
fn shimmy(stick_x: f32) -> InputFrame {
    InputFrame {
        move_stick: [stick_x, 0.0],
        camera_yaw: std::f32::consts::PI,
        ..InputFrame::default()
    }
}

fn hks_cover(variation: u8, away: bool) {
    let Some(mut duel) = docked(variation) else {
        return;
    };
    assert_eq!(
        duel.player
            .behavior
            .runtime
            .variable("Selector_Environment"),
        Some(if variation >= 10 { 1.0 } else { 0.0 })
    );
    let origin = Vec3::from(duel.player.body.position);
    let slide = shimmy(-1.0);
    let mut peek = false;
    for tick in 0..600 {
        let r = duel.step(&slide, DT);
        peek |= r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_CoverActionStart");
        if tick % 60 == 0 {
            eprintln!(
                "cover tick={tick} pos={:?} anim={:?} events={:?} end={} blocked={:?}",
                duel.player.body.position,
                r.player.animation,
                r.player.behavior.events,
                duel.player.behavior.env.docking_end,
                duel.player.behavior.env.docking_blocked
            );
        }
    }
    assert!(Vec3::from(duel.player.body.position).distance(origin) > 1.0);
    // The stick slides Wolf to +X, the right end facing the wall: free or right-only look.
    assert_eq!(
        peek,
        matches!(variation, 1 | 3 | 11 | 13),
        "variation {variation}"
    );
    assert!(
        duel.player.docking.active.is_some(),
        "peek released docking"
    );
    let end = if away {
        InputFrame {
            move_stick: [0.0, 1.0],
            camera_yaw: std::f32::consts::PI,
            ..InputFrame::default()
        }
    } else {
        press()
    };
    let r = duel.step(&end, DT);
    assert!(
        r.player
            .behavior
            .events
            .iter()
            .any(|e| e.starts_with("W_CoverEnd")),
        "{:?}",
        r.player
    );
    for _ in 0..180 {
        duel.step(&InputFrame::default(), DT);
    }
    assert!(duel.player.docking.active.is_none() && duel.player.body.grounded);
    let before = Vec3::from(duel.player.body.position);
    for _ in 0..60 {
        duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                camera_yaw: std::f32::consts::PI,
                ..InputFrame::default()
            },
            DT,
        );
    }
    assert!(Vec3::from(duel.player.body.position).distance(before) > 0.5);
}

#[test]
fn outskirts_wall_route() {
    let cache = sekiro_sim::cache::root();
    let dir = cache.join("maps/m11_00_00_00");
    if !dir.join("edges.json").exists() {
        return;
    }
    let mut duel = Duel::load(&cache, 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.ground = Box::new(sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap());
    duel.player.docking = Docking::load(&dir.join("edges.json")).unwrap();
    duel.player.body.position = [172.70456, -68.85, 63.21088];
    let yaw = 14.819877f32.to_radians();
    duel.player.body.yaw = yaw;
    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../game/scripts/m11_cover.txt"),
    )
    .unwrap();
    let mut entered = false;
    let mut idle = false;
    let mut peek = false;
    let mut exited = false;
    let mut first = None;
    let mut last = None;
    for (tick, mut frame) in sekiro_sim::input::parse_action_script(&script)
        .unwrap()
        .into_iter()
        .enumerate()
    {
        frame.camera_yaw += yaw;
        let r = duel.step(&frame, 1.0 / 60.0);
        entered |= r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e.starts_with("W_StandToCover2"));
        idle |= r.player.animation.as_deref() == Some("a000_021000");
        peek |= r.player.animation.as_deref() == Some("a000_205020");
        exited |= r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_CoverEndFromCoverLook");
        if let Some(t) = &duel.player.docking.active {
            assert_eq!(duel.player.docking.edges[t.edge].source_part, "h018700");
            assert_eq!(duel.player.docking.edges[t.edge].chain, 1);
            first.get_or_insert(Vec3::from(duel.player.body.position));
            last = Some(Vec3::from(duel.player.body.position));
            let clearance = duel
                .player
                .ground
                .slide(duel.player.body.position, [0.0; 2]);
            assert!(clearance[0].hypot(clearance[1]) <= 0.05);
        }
        if tick % 30 == 0 || !r.player.behavior.events.is_empty() {
            eprintln!(
                "wall route tick={tick} pos={:?} animation={:?} events={:?}",
                duel.player.body.position, r.player.animation, r.player.behavior.events
            );
        }
        if matches!(tick, 150 | 270) {
            let bones = duel.player.bone_source_matrices();
            let world = duel.player.source_to_world();
            for (i, bone) in duel
                .player
                .skeleton
                .bones
                .iter()
                .enumerate()
                .filter(|(_, b)| b.name.ends_with("Head") || b.name.ends_with("Foot"))
            {
                let p = world.transform_point3(bones[i].transform_point3(Vec3::ZERO));
                eprintln!("cover pose tick={tick} {}={p:?}", bone.name);
            }
            if let Some(clip) = duel.player.clips.clip("a000_205010") {
                eprintln!(
                    "cover peek root={:?}",
                    clip.root_motion.as_ref().map(|r| r.total())
                );
            }
        }
    }
    assert!(
        entered && idle && peek && exited,
        "entered={entered} idle={idle} peek={peek} exited={exited}"
    );
    assert!(first.unwrap().distance(last.unwrap()) > 0.5);
    assert!(duel.player.docking.active.is_none() && duel.player.body.grounded);
}

// Issue #172: docked on the free-look wall, Wolf runs to each end in turn. On the way the
// shuffle clip the script picks must carry him the way he slides (its sides are named facing
// the wall, so with his back to it his own left is its right), and at the end he must peek out
// past the open end, not back along the wall.
#[test]
fn cover_shuffle_and_peek_follow_the_slide() {
    let Some(mut duel) = docked(1) else {
        return;
    };
    let head = duel
        .player
        .skeleton
        .bones
        .iter()
        .position(|b| b.name.ends_with("Head"))
        .unwrap();
    // Stick left runs to the +X end (the right one, facing the wall), stick right to -X.
    for (stick_x, out, clip) in [(-1.0, 1.0, "a000_205021"), (1.0, -1.0, "a000_205020")] {
        let mut shuffled = 0;
        let mut peek_ticks = 0;
        let mut lean = f32::NEG_INFINITY;
        for _ in 0..600 {
            let before = duel.player.body.position[0];
            let r = duel.step(&shimmy(stick_x), DT);
            let moved = duel.player.body.position[0] - before;
            let anim = r.player.animation.unwrap_or_default();
            if anim == clip {
                // The largest lean over the first ticks of the loop, so a blend frame cannot
                // decide it.
                let bones = duel.player.bone_source_matrices();
                let world = duel.player.source_to_world();
                let p = world.transform_point3(bones[head].transform_point3(Vec3::ZERO));
                lean = lean.max((p.x - duel.player.body.position[0]) * out);
                peek_ticks += 1;
                if peek_ticks == 20 {
                    break;
                }
            } else if (anim.starts_with("a000_0214") || anim.starts_with("a000_0215"))
                && moved != 0.0
            {
                let total = duel
                    .player
                    .clips
                    .clip(&anim)
                    .and_then(|c| c.root_motion.as_ref().map(|m| m.total()))
                    .expect("cover move clip without root motion");
                let [clip_x, _] = root_motion_world(total, duel.player.body.yaw.sin_cos());
                assert!(
                    clip_x * moved > 0.0,
                    "stick {stick_x}: {anim} carries the root {clip_x} along X, Wolf slid {moved}"
                );
                assert!(moved * stick_x < 0.0, "stick {stick_x} slid {moved}");
                shuffled += 1;
            }
        }
        assert!(
            shuffled > 20,
            "stick {stick_x}: only {shuffled} shuffle ticks"
        );
        assert!(peek_ticks > 0, "stick {stick_x}: no {clip} peek");
        assert!(
            lean > 0.1,
            "stick {stick_x}: head leans {lean} toward the open end"
        );
        // Stick back the other way ends the peek.
        for _ in 0..60 {
            duel.step(&shimmy(-stick_x), DT);
        }
    }
}
#[test]
fn cover_rejects_missing_wall_bad_height_and_unsupported_variation() {
    let mut docking = synthetic(1);
    let mut body = Body {
        position: [0.0, 0.0, 0.7],
        ..Body::default()
    };
    docking.search(&body, &sekiro_sim::body::FlatGround::default());
    assert!(docking.candidate.is_none());
    body.position[1] = 2.0;
    docking.search(&body, &Wall);
    assert!(docking.candidate.is_none());
    body.position = [0.0, 0.0, 1.1];
    docking.search(&body, &Wall);
    assert!(docking.candidate.is_none());
    body.position = [0.0, 0.0, 0.7];
    for variation in [4, 9, 14, 255] {
        docking.edges[0].variation = variation;
        docking.search(&body, &Wall);
        assert!(docking.candidate.is_none());
    }
}

#[test]
fn cover_obstacle_queries_distinguish_each_end() {
    struct Corner;
    impl Ground for Corner {
        fn height(&self, _: f32, _: f32) -> Option<f32> {
            Some(0.0)
        }
        fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
            [
                (feet[0] + delta[0]).min(2.05) - feet[0],
                (feet[2] + delta[1]).max(0.3) - feet[2],
            ]
        }
    }
    let mut docking = synthetic(1);
    let body = Body {
        position: [2.0, 0.0, 0.7],
        ..Body::default()
    };
    docking.search(&body, &Corner);
    let target = docking.candidate.as_ref().unwrap();
    // The +X end is the right one, facing the wall.
    assert!(!docking.cover_blocked(target, &Corner, true));
    assert!(docking.cover_blocked(target, &Corner, false));
}

/// m11_00's snake pit wall `h013600` with the play world. With `low`, only when the cache has
/// both blocks' `l` collision (`sekiro-extract map <block> --lo-collision`), whose ledge and
/// hug slab the test is about; without, on whichever mesh the blocks have (`collision.bin`
/// when they have no `l`).
fn snake_pit(
    low: bool,
) -> Option<(
    Docking,
    std::sync::Arc<sekiro_sim::collision::CollisionWorld>,
)> {
    let cache = crate::arena_c_support::cache();
    let dir = cache.join("maps/m11_00_00_00");
    let has_low = ["m11_00_00_00", "m11_01_00_00"].iter().all(|b| {
        cache
            .join("maps")
            .join(b)
            .join(sekiro_sim::lo_collision::LOW_FILE)
            .is_file()
    });
    if (low && !has_low) || !dir.join("edges.json").is_file() {
        eprintln!("skipping the snake pit: no collision_lo.bin or edges.json in {cache:?}");
        return None;
    }
    let world = crate::step_cost::play_world()?;
    Some((Docking::load(&dir.join("edges.json")).unwrap(), world))
}

/// The cover edges of `h013600`'s `chain`, in order.
fn wall(docking: &Docking, chain: u32) -> Vec<usize> {
    (0..docking.edges.len())
        .filter(|&i| {
            let e = &docking.edges[i];
            e.source_part == "h013600" && e.kind == 3 && e.chain == chain
        })
        .collect()
}

/// Docks at `entry` facing chain 2's third edge, as the playtest did.
fn dock_in_snake_pit(docking: &mut Docking, world: &dyn Ground, entry: [f32; 3]) -> Body {
    let corner = &docking.edges[wall(docking, 2)[2]];
    let run = Vec3::from(corner.end) - Vec3::from(corner.start);
    let mut body = Body {
        position: entry,
        yaw: (-run.z).atan2(run.x),
        ..Body::default()
    };
    docking.active = None;
    docking.search(&body, world);
    assert!(
        docking
            .candidate
            .as_ref()
            .is_some_and(|t| docking.is_cover(t)),
        "no cover at {entry:?}: {:?}",
        docking.candidate
    );
    assert!(docking.cover_step(&mut body, Some("a000_218103"), None, 0.0, world));
    body
}

/// The two spots the #147 playtest docked at in the snake pit.
const SNAKE_PIT_ENTRIES: [[f32; 3]; 2] = [[-3.5467, -141.634, 5.6632], [-2.2022, -141.610, 8.1418]];

/// The wall hug in m11_00's foggy snake pit (#147): wall `h013600` is chain 2 (variation 0)
/// joined end to start to chain 3 (variation 10), a narrow ledge. From both spots the playtest
/// docked at, Wolf hugs back to chain 2's corner and on across the joint onto chain 3, where he
/// used to stop at the joint or at once, and past its first half (on `l` he hugs on further,
/// [`snake_pit_wall_hug_reaches_the_far_end_on_the_hug_slab`]).
#[test]
fn snake_pit_wall_hug_crosses_the_chain_joint() {
    let Some((mut docking, world)) = snake_pit(false) else {
        return;
    };
    let world = &*world;
    let (corner, narrow) = (wall(&docking, 2)[2], wall(&docking, 3)[0]);
    // The edges after the narrow ledge's first one, in hug order.
    let beyond: Vec<usize> = wall(&docking, 3)[1..]
        .iter()
        .chain(&wall(&docking, 4))
        .copied()
        .collect();
    let run = |e: &MapEdge| Vec3::from(e.end) - Vec3::from(e.start);
    for entry in SNAKE_PIT_ENTRIES {
        // Toward chain 2's start the hug stops at the sharp corner; the other way it runs on.
        for (sign, edge, past) in [(-1.0f32, corner, 0.0f32), (1.0, narrow, 0.5)] {
            let mut body = dock_in_snake_pit(&mut docking, world, entry);
            let d = run(&docking.edges[corner]) * sign;
            let stick = (-d.x).atan2(-d.z);
            for _ in 0..300 {
                docking.cover_step(&mut body, Some("a000_021203"), Some(stick), 0.05, world);
            }
            let target = docking.active.clone().unwrap();
            let e = &docking.edges[edge];
            let along = (target.point - Vec3::from(e.start)).dot(run(e)) / run(e).length_squared();
            assert!(
                target.edge == edge && along >= past - 1e-3
                    || sign > 0.0 && beyond.contains(&target.edge),
                "from {entry:?} sign {sign}: stopped at {target:?}, {along} along edge {}",
                target.edge
            );
            if sign < 0.0 {
                assert_ne!(target.end, 0, "no end at the corner: {target:?}");
            }
        }
    }
}

/// The whole snake pit hug (#189): from both entry spots Wolf hugs `h013600` across chain 3 (the
/// narrow ledge, variation 10) and chain 4 to the far end, every step docked on a floor a
/// wall-hugging body stands on, at the ledge's height. On `l` the ledge has no character floor
/// under the docked feet along most of chain 3; the wall-hug slab (`h900100`, hit filter 25,
/// [`sekiro_sim::hit_filter::WALL_HUG`]) is the floor there, so without it the hug stops at the
/// ledge's start.
#[test]
fn snake_pit_wall_hug_reaches_the_far_end_on_the_hug_slab() {
    let Some((mut docking, world)) = snake_pit(true) else {
        return;
    };
    let world = &*world;
    let (ledge, far) = (wall(&docking, 3), wall(&docking, 4));
    let far_end = *far.last().unwrap();
    let run = |e: &MapEdge| Vec3::from(e.end) - Vec3::from(e.start);
    for entry in SNAKE_PIT_ENTRIES {
        let mut body = dock_in_snake_pit(&mut docking, world, entry);
        let mut visited = std::collections::BTreeSet::new();
        let mut arrived = None;
        for step in 0..1500 {
            let d = run(&docking.edges[docking.active.as_ref().unwrap().edge]);
            let stick = (-d.x).atan2(-d.z);
            docking.cover_step(&mut body, Some("a000_021203"), Some(stick), 0.05, world);
            let target = docking.active.clone().unwrap();
            visited.insert(target.edge);
            let feet = target.floor;
            let hug = world.hug_floor(feet.x, feet.z, feet.y + 0.5);
            assert!(
                hug.is_some_and(|y| (y - feet.y).abs() < 0.05),
                "from {entry:?} step {step}: docked on {feet:?}, hug floor {hug:?}: {target:?}"
            );
            // At the ledge's height under the wall, never dropping toward the pit: the ledge
            // climbs from about -141.6 to -140.1 and the feet stay within 0.25 m of its edge.
            assert!(
                (body.position[1] - feet.y).abs() < 0.05 && (feet.y - target.point.y).abs() < 0.3,
                "from {entry:?} step {step}: Wolf at {:?}, floor {feet:?}, edge at {:?}",
                body.position,
                target.point
            );
            let e = &docking.edges[target.edge];
            let along = (target.point - Vec3::from(e.start)).dot(run(e)) / run(e).length_squared();
            if target.edge == far_end && along >= 0.9 && target.end != 0 {
                arrived = Some(step);
                break;
            }
        }
        assert!(
            arrived.is_some(),
            "from {entry:?}: never reached the far end, stopped at {:?}, visited {visited:?}",
            docking.active
        );
        for edge in ledge.iter().chain(&far) {
            assert!(
                visited.contains(edge),
                "from {entry:?}: skipped edge {edge}"
            );
        }
    }
}

/// Walking the snake pit ledge without hugging (#189): the player's capsule (group 1) does not
/// collide with the wall-hug slab (group 85), and on `l` the ledge itself gives no floor under
/// about half of chain 3 where the hug stands (0.7 m out,
/// [`sekiro_sim::docking::DOCKING_TRANSLATION_OFFSET_COVER`]), in gaps over 3 m long, over the
/// pit's fall-return trigger. So a walking Wolf falls into the pit there; only the hug carries
/// him across, as the game's wall-hug group is built for.
#[test]
fn snake_pit_ledge_has_no_walking_floor() {
    let Some((docking, world)) = snake_pit(true) else {
        return;
    };
    let world = &*world;
    let (mut samples, mut missing, mut gap, mut longest) = (0, 0, 0.0f32, 0.0f32);
    for &i in &wall(&docking, 3) {
        let e = &docking.edges[i];
        let (start, end) = (Vec3::from(e.start), Vec3::from(e.end));
        let d = (end - start).with_y(0.0);
        let outward = Vec3::new(-d.z, 0.0, d.x).normalize();
        let steps = (d.length() / 0.05) as usize;
        for s in 0..=steps {
            let p = start.lerp(end, s as f32 / steps as f32)
                + outward * sekiro_sim::docking::DOCKING_TRANSLATION_OFFSET_COVER;
            samples += 1;
            let hug = world.hug_floor(p.x, p.z, p.y + 0.5);
            assert!(
                hug.is_some_and(|y| (y - p.y).abs() <= 0.3),
                "no hug floor at {p:?}: {hug:?}"
            );
            let walk = world.floor(p.x, p.z, p.y + 0.5);
            if walk.is_some_and(|y| (y - p.y).abs() <= 0.3) {
                gap = 0.0;
                continue;
            }
            assert!(
                walk.is_none(),
                "a floor off the ledge's height at {p:?}: {walk:?}"
            );
            assert!(
                world.fall_return_below(p.x, p.y, p.z),
                "no fall return under {p:?}"
            );
            missing += 1;
            gap += d.length() / steps as f32;
            longest = longest.max(gap);
        }
    }
    assert!(
        missing * 3 > samples && longest > 3.0,
        "{missing} of {samples} ledge points without a walking floor, longest gap {longest} m"
    );
}

/// Leaving the hug on the snake pit ledge (#189 review): the wall-hug slab holds only a hugging
/// body (group 86 while the docking type is 1, `FUN_1407df900` / `FUN_140b7d5c0`), so once the
/// hug ends Wolf stands on what his capsule (group 1) meets, as in the exe. Where `l`'s ledge
/// reaches under the 0.4 m capsule's rounded bottom he rests on its lip, though nothing is under
/// his centre; where `l` has no ledge within the capsule's radius (a gap of about 2.7 m along chain
/// 3) nothing holds him and he drops off, as a walking Wolf does
/// ([`snake_pit_ledge_has_no_walking_floor`]).
#[test]
fn snake_pit_hug_exit_rests_on_the_lip() {
    let Some((docking, world)) = snake_pit(true) else {
        return;
    };
    let world = &*world;
    let rim = sekiro_sim::collision::RADIUS * 0.866;
    let (mut on_lip, mut dropped) = (0, 0);
    for &i in &wall(&docking, 3) {
        let e = &docking.edges[i];
        let (start, end) = (Vec3::from(e.start), Vec3::from(e.end));
        let d = (end - start).with_y(0.0);
        let outward = Vec3::new(-d.z, 0.0, d.x).normalize();
        let steps = (d.length() / 0.1) as usize;
        for s in 0..=steps {
            let p = start.lerp(end, s as f32 / steps as f32);
            let feet = p + outward * sekiro_sim::docking::DOCKING_TRANSLATION_OFFSET_COVER;
            let y = world
                .hug_floor(feet.x, feet.z, p.y + 0.5)
                .expect("the hug stands everywhere on the ledge");
            // The nearest walking floor at the ledge's height around the feet, if any is within
            // the capsule's radius.
            let lip = (0..32)
                .flat_map(|k| (0..=8).map(move |r| (k, r as f32 * 0.05)))
                .filter(|&(k, r)| {
                    let a = k as f32 * std::f32::consts::TAU / 32.0;
                    let q = feet + Vec3::new(a.cos(), 0.0, a.sin()) * r;
                    world
                        .floor(q.x, q.z, y + 0.05)
                        .is_some_and(|h| (y - h).abs() < 0.3)
                })
                .map(|(_, r)| r)
                .reduce(f32::min);
            let mut body = Body {
                position: [feet.x, y, feet.z],
                grounded: true,
                ..Body::default()
            };
            for _ in 0..30 {
                body.step(&sekiro_sim::body::BodyInput::default(), DT, world);
            }
            let stayed = body.grounded && (body.position[1] - y).abs() < 0.3;
            match lip {
                Some(r) if r <= rim => {
                    on_lip += usize::from(r > 0.0);
                    assert!(
                        stayed,
                        "left the hug at {feet:?} over a lip {r} m away: {body:?}"
                    );
                }
                None => {
                    dropped += 1;
                    assert!(
                        !stayed,
                        "no ledge under {feet:?}, yet Wolf stands: {body:?}"
                    );
                }
                // Between the rim's reach and the radius: a contact too steep to stand on.
                Some(_) => {}
            }
        }
    }
    eprintln!("{on_lip} ledge points on the lip, {dropped} dropped");
    assert!(
        on_lip >= 5 && dropped >= 5,
        "{on_lip} on the lip, {dropped} dropped"
    );
}
