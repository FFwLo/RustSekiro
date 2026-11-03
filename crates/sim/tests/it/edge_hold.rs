//! The fall-prevention assist walls at map edges (#119, #149, #86, #203). Two kinds stand along
//! cliff and roof edges: the parts with hit filter 28 (`h900400`, group 92) and the `f` hit
//! binder's bands (`fall_assist.bin`, group 93). The player script asks for the assist with
//! `act(3016, 0)` in its walk, run and idle states and adds `act(3016, 1)` in ground actions
//! (attacks, sprint start and stop); in the exe the first request's cast meets only group 92 and
//! the second's also group 93. So walking at a filter-28 cliff holds Wolf, a sprint is held
//! until its loop starts and then goes over, and a jump pressed at the edge leaves from the
//! ground; the `f` bands hold only ground actions, so a walk crosses them, onto flat ground or
//! off a roof. Skips when the map or its `fall_assist.bin` has not been exported
//! (`sekiro-extract map m11_00_00_00 --fall-assist`).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use glam::{Vec2, Vec3};
use sekiro_formats::hknp::PartCollision;
use sekiro_sim::body::Ground;
use sekiro_sim::collision::CollisionWorld;
use sekiro_sim::hit_filter::{self, FALL_ASSIST_ACTION, FALL_ASSIST_MOVE};
use sekiro_sim::{Buttons, Character, InputFrame};

use crate::movement_helpers::{DT, cache, settle, stick};

const MAP: &str = "maps/m11_00_00_00";

/// An edge: a point on its wall, the way out over it (the wall's outward normal on the ground
/// plane), a height just above the floor inside, and how far in Wolf starts.
struct Edge {
    name: &'static str,
    wall: [f32; 2],
    out: [f32; 2],
    top: f32,
    inside: f32,
}

/// Cliff tops with a filter-28 wall, which holds a walk.
const CLIFFS: [Edge; 2] = [
    // `h900400` on a cliff top 35 m east of start 2: the ground falls 7.5 m, then further.
    Edge {
        name: "cliff east of start 2",
        wall: [-126.8, 28.06],
        out: [0.43, 0.90],
        top: -24.0,
        inside: 4.0,
    },
    // The cliff top 60 m north of start 0, by the bent pine (a 20 m drop): a filter-28 part of
    // the `l` binder, 20 m tall, besides `h023300`'s `f` band.
    Edge {
        name: "cliff north of start 0",
        wall: [-135.21, -132.85],
        out: [0.20, 0.98],
        top: -50.0,
        inside: 2.0,
    },
];

/// The `f` band on the roof of the house by start 0 (`h022800`, 6.9 m above the ground), with no
/// filter-28 wall.
const ROOF: Edge = Edge {
    name: "start 0 roof",
    wall: [-131.96, -193.14],
    out: [-0.82, -0.58],
    top: -40.5,
    inside: 2.0,
};

/// The cache and the map's directory, if the map's fall-assist walls are exported.
fn map_dir() -> Option<(PathBuf, PathBuf)> {
    let cache = cache()?;
    let dir = cache.join(MAP);
    dir.join("fall_assist.bin")
        .is_file()
        .then_some((cache, dir))
}

/// The map with its fall-assist walls, loaded once for the tests that share it.
fn map() -> Option<(PathBuf, Arc<CollisionWorld>)> {
    static MAP_WORLD: OnceLock<Option<(PathBuf, Arc<CollisionWorld>)>> = OnceLock::new();
    MAP_WORLD
        .get_or_init(|| {
            let (cache, dir) = map_dir()?;
            Some((cache, Arc::new(CollisionWorld::load_map(&dir).unwrap())))
        })
        .clone()
}

/// Wolf standing at `feet` (dropped onto the floor below `feet[1] + 1`) on `world`.
fn wolf_on(cache: &std::path::Path, world: Arc<CollisionWorld>, feet: [f32; 3]) -> Character {
    let y = world
        .floor(feet[0], feet[2], feet[1] + 1.0)
        .unwrap_or(feet[1]);
    let mut wolf = Character::load_from_cache(cache).unwrap();
    wolf.ground = Box::new(world);
    wolf.body.position = [feet[0], y, feet[2]];
    settle(&mut wolf);
    wolf
}

/// Wolf settled `inside` metres inside the edge, and the camera yaw that pushes him out.
fn wolf_inside(edge: &Edge, inside: f32) -> Option<(Character, f32)> {
    let (cache, world) = map()?;
    let (x, z) = (
        edge.wall[0] - edge.out[0] * inside,
        edge.wall[1] - edge.out[1] * inside,
    );
    let y = world
        .floor(x, z, edge.top)
        .unwrap_or_else(|| panic!("{}: no floor inside the edge", edge.name));
    let wolf = wolf_on(&cache, world, [x, y, z]);
    // The stick's forward is the camera's: yaw 0 looks along -Z.
    Some((wolf, (-edge.out[0]).atan2(-edge.out[1])))
}

/// What one push at the edge did.
struct Push {
    /// The furthest Wolf got past the wall's line, in metres along the way out.
    past: f32,
    /// How far he ended up below where he started.
    fell: f32,
    /// The first tick he was past the line, if he got there.
    crossed: Option<usize>,
    /// The first tick he left the ground, and whether his feet then rose.
    airborne: Option<usize>,
    rose: bool,
    /// Whether the script asked for the walls while he was grounded.
    asked: bool,
}

/// What Wolf does at the edge on one tick.
#[derive(Clone, Copy)]
enum Move {
    Run,
    Sprint,
    /// Stand facing out and attack every third of a second.
    Attack,
}

/// Pushes Wolf out over the edge for `ticks` with `how`, pressing jump for three ticks from
/// `jump_at` when it is set.
fn push_out(
    wolf: &mut Character,
    edge: &Edge,
    yaw: f32,
    how: Move,
    jump_at: Option<usize>,
    ticks: usize,
) -> Push {
    let start_y = wolf.body.position[1];
    let mut push = Push {
        past: f32::MIN,
        fell: 0.0,
        crossed: None,
        airborne: None,
        rose: false,
        asked: false,
    };
    let mut takeoff_y = 0.0;
    for tick in 0..ticks {
        let (stick_y, dodge, attack) = match how {
            Move::Run => (1.0, false, false),
            Move::Sprint => (1.0, true, false),
            // A short nudge turns him to face out; then he stands and attacks.
            Move::Attack => (if tick < 4 { 0.2 } else { 0.0 }, false, tick % 20 < 3),
        };
        let input = InputFrame {
            buttons: Buttons {
                dodge,
                attack: attack && tick > 10,
                jump: jump_at.is_some_and(|t| (t..t + 3).contains(&tick)),
                ..Buttons::default()
            },
            ..stick(0.0, stick_y, yaw)
        };
        let report = wolf.tick(&input, DT);
        assert!(
            report.behavior.script_errors.is_empty(),
            "{:?}",
            report.behavior.script_errors
        );
        let p = wolf.body.position;
        let past = (p[0] - edge.wall[0]) * edge.out[0] + (p[2] - edge.wall[1]) * edge.out[1];
        push.past = push.past.max(past);
        if past > 0.0 && push.crossed.is_none() {
            push.crossed = Some(tick);
        }
        if !report.grounded && push.airborne.is_none() {
            push.airborne = Some(tick);
            takeoff_y = p[1];
        }
        if push.airborne.is_some() && p[1] > takeoff_y + 0.3 {
            push.rose = true;
        }
        push.asked |=
            wolf.body.grounded && wolf.behavior.env.fall_assist.get() & FALL_ASSIST_MOVE != 0;
    }
    push.fell = start_y - wolf.body.position[1];
    push
}

/// Running at each filter-28 cliff for 3 s holds Wolf on top; sprinting at it holds him through
/// the sprint's start, as the user saw in the game (0.5 to 1.5 s, #86), and then he goes over.
#[test]
fn walking_at_a_cliff_holds_wolf_and_sprinting_goes_over() {
    for edge in &CLIFFS {
        let Some((mut wolf, yaw)) = wolf_inside(edge, edge.inside) else {
            return;
        };
        let run = push_out(&mut wolf, edge, yaw, Move::Run, None, 180);
        assert!(
            run.asked,
            "{}: the run never asked for the walls",
            edge.name
        );
        assert!(
            run.past < 0.0 && run.fell.abs() < 0.5 && wolf.body.grounded,
            "{}: ran {:.2} m past the wall and fell {:.2} m",
            edge.name,
            run.past,
            run.fell
        );
        let sprint = push_out(&mut wolf, edge, yaw, Move::Sprint, None, 240);
        let over = sprint.crossed.map(|t| t as f32 * DT);
        assert!(
            over.is_some_and(|s| (0.3..1.0).contains(&s)) && sprint.past > 1.0 && sprint.fell > 3.0,
            "{}: a sprint went over after {over:?} s, {:.2} m past the wall, and fell {:.2} m",
            edge.name,
            sprint.past,
            sprint.fell
        );
    }
}

/// #203: the `f` bands are the ground actions' walls (group 93). At the roof by start 0, which
/// has only a band, an attack at the edge is held, while a run goes off the roof.
#[test]
fn a_roof_band_holds_an_attack_and_lets_a_run_off() {
    let Some((mut wolf, yaw)) = wolf_inside(&ROOF, 0.4) else {
        return;
    };
    let attack = push_out(&mut wolf, &ROOF, yaw, Move::Attack, None, 240);
    assert!(
        attack.past < 0.0 && attack.fell.abs() < 0.5,
        "attacked {:.2} m past the band and fell {:.2} m",
        attack.past,
        attack.fell
    );
    let Some((mut wolf, yaw)) = wolf_inside(&ROOF, ROOF.inside) else {
        return;
    };
    let run = push_out(&mut wolf, &ROOF, yaw, Move::Run, None, 120);
    assert!(
        run.past > 1.0 && run.fell > 3.0,
        "ran {:.2} m past the band and fell {:.2} m",
        run.past,
        run.fell
    );
}

/// #86: jump pressed while the cliff holds Wolf, from a run and in a sprint's hold, takes off
/// from the ground within 0.5 s of the press (his feet rise after he leaves it) instead of
/// dropping off.
#[test]
fn jump_pressed_at_the_edge_jumps() {
    let edge = &CLIFFS[1];
    for (sprint, at) in [(false, 150), (true, 15)] {
        let Some((mut wolf, yaw)) = wolf_inside(edge, edge.inside) else {
            return;
        };
        // Run up to the wall first, so the press lands while he is held at it.
        if sprint {
            let run = push_out(&mut wolf, edge, yaw, Move::Run, None, 150);
            assert!(run.past < 0.0, "ran {:.2} m past the wall", run.past);
        }
        let how = if sprint { Move::Sprint } else { Move::Run };
        let jump = push_out(&mut wolf, edge, yaw, how, Some(at), at + 90);
        let left = jump.airborne.map(|t| t as f32 * DT);
        assert!(
            jump.airborne.is_some_and(|t| (at..at + 30).contains(&t)) && jump.rose,
            "sprint {sprint}: left the ground at {left:?} s (jump at {:.2} s), rose {}",
            at as f32 * DT,
            jump.rose
        );
    }
}

/// #203, the user's spot: on the snowy cliff top by the bent pine (`o003500_0004`), looking
/// along the cliff, `h023300`'s band crosses flat ground 11 m west of the pine (4 m of floor
/// beyond it, then a drop). Walking across it goes on, as no walk is held by an `f` band.
#[test]
fn walking_across_a_band_on_the_cliff_top_goes_on() {
    let Some((cache, world)) = map() else {
        return;
    };
    let (wall, inward) = ([-148.9, -51.3, -131.3], [0.90, 0.43]);
    let start = [
        wall[0] + inward[0] * 1.5,
        wall[1] + 0.5,
        wall[2] + inward[1] * 1.5,
    ];
    let mut wolf = wolf_on(&cache, world, start);
    let y = wolf.body.position[1];
    let yaw = f32::atan2(inward[0], inward[1]);
    for _ in 0..90 {
        let report = wolf.tick(&stick(0.0, 1.0, yaw), DT);
        assert!(report.behavior.script_errors.is_empty());
    }
    let p = wolf.body.position;
    let past = -((p[0] - wall[0]) * inward[0] + (p[2] - wall[2]) * inward[1]);
    assert!(
        past > 1.5 && (p[1] - y).abs() < 1.0 && wolf.body.grounded,
        "walked {past:.2} m past the band, to {p:?} from y {y:.2}"
    );
}

/// #203 across the map: no `f` band triangle that stands on a floor with 2 m of floor at about
/// the same height beyond it holds a walk's slide (only the walls the walk's request meets,
/// [`FALL_ASSIST_MOVE`]) where the map without its bands lets the walk go (81 of them share
/// their spot with a filter-28 wall, which holds it), while the ground actions' request still
/// stops at them.
#[test]
fn no_band_on_flat_ground_holds_a_walk() {
    let Some((_, dir)) = map_dir() else {
        return;
    };
    let (_, world) = map().unwrap();
    let layout = serde_json::from_slice(&std::fs::read(dir.join("layout.json")).unwrap()).unwrap();
    let block = sekiro_sim::lo_collision::block_collision(&dir, &layout, |_| false).unwrap();
    let mut bare = CollisionWorld::from_mesh(&block.mesh);
    bare.add_helpers(&block.helpers);
    let walls =
        PartCollision::from_bytes(&std::fs::read(dir.join("fall_assist.bin")).unwrap()).unwrap();
    let m = &walls.mesh;
    let (mut flat, mut walk_held, mut action_held) = (0, Vec::new(), 0);
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|k| Vec3::from(m.vertices[k as usize]));
        let n = (b - a).cross(c - a);
        let flat_n = Vec2::new(n.x, n.z);
        if flat_n.length() < 0.5 * n.length() || n.length_squared() < 1e-12 {
            continue;
        }
        // The side the wall faces, where a body it holds stands.
        let inward = flat_n.normalize();
        let centre = (a + b + c) / 3.0;
        let base = a.y.min(b.y).min(c.y);
        let floor =
            |d: f32, top: f32| world.floor(centre.x + inward.x * d, centre.z + inward.y * d, top);
        let Some(inner) = floor(0.4, base + 0.6).filter(|f| (f - base).abs() < 0.6) else {
            continue;
        };
        if ![-1.0, -2.0]
            .iter()
            .all(|&d| floor(d, inner + 0.6).is_some_and(|f| f > inner - 1.0))
        {
            continue;
        }
        let feet = [centre.x + inward.x * 0.4, inner, centre.z + inward.y * 0.4];
        let delta = [-inward.x * 1.2, -inward.y * 1.2];
        let along = |s: [f32; 2]| -(s[0] * inward.x + s[1] * inward.y);
        if along(bare.slide_assisted(feet, delta, FALL_ASSIST_MOVE)) < 1.0 {
            continue;
        }
        flat += 1;
        if along(world.slide_assisted(feet, delta, FALL_ASSIST_MOVE)) < 1.0 {
            walk_held.push(centre);
        }
        action_held +=
            usize::from(along(world.slide_assisted(feet, delta, FALL_ASSIST_ACTION)) < 0.6);
    }
    assert!(
        walk_held.is_empty(),
        "{} of {flat} band triangles on flat ground hold a walk, the first at {:?}",
        walk_held.len(),
        walk_held.first()
    );
    assert!(
        flat > 1000 && action_held * 2 > flat,
        "{action_held} of {flat} band triangles on flat ground hold a ground action"
    );
}

/// Steers Wolf at each of `route`'s waypoints in turn, sprinting on the legs `sprint` names,
/// until he is within 0.8 m of it. Returns the ticks on which a slide from where he stood tested
/// assist walls and the most walls one tested, or where he got stuck.
fn follow(
    wolf: &mut Character,
    world: &CollisionWorld,
    route: &[[f32; 3]],
    sprint: &[usize],
) -> Result<[usize; 2], String> {
    let (mut walled, mut most) = (0, 0);
    for (i, target) in route.iter().enumerate() {
        let mut reached = false;
        for _ in 0..600 {
            let p = wolf.body.position;
            let (dx, dz) = (target[0] - p[0], target[2] - p[2]);
            let len = dx.hypot(dz);
            if len < 0.8 {
                reached = true;
                break;
            }
            let step = [dx / len * 0.1, dz / len * 0.1];
            let walls =
                world.assisted_slide_cost(p, step, FALL_ASSIST_MOVE | FALL_ASSIST_ACTION)[2];
            walled += usize::from(walls > 0);
            most = most.max(walls);
            let input = InputFrame {
                buttons: Buttons {
                    dodge: sprint.contains(&i),
                    ..Buttons::default()
                },
                ..stick(0.0, 1.0, (-dx).atan2(-dz))
            };
            let report = wolf.tick(&input, DT);
            assert!(report.behavior.script_errors.is_empty());
        }
        let p = wolf.body.position;
        if !reached || (p[1] - target[1]).abs() > 1.0 {
            return Err(format!("stuck at {p:?} short of waypoint {i} {target:?}"));
        }
    }
    Ok([walled, most])
}

/// From player start 3 to the closed Ashina Castle gate and back down the long stairs, with the
/// assist walls loaded. The start terrace's 11 m drop to the moat has only a band (`f` walls of
/// the terrace's part), so Wolf runs off it; then he runs through the moat gate, up the stairs
/// past the bands along the landings' sides, and back down: a stair up or down is not an edge.
#[test]
fn castle_gate_route_from_start_3_runs_off_the_terrace_and_up_the_stairs() {
    let Some((cache, world)) = map() else {
        return;
    };
    let route = [
        [-190.77, -47.98, 146.37],
        [-183.98, -59.16, 144.36],
        [-180.63, -59.20, 138.53],
        [-176.32, -58.40, 136.84],
        [-157.75, -52.73, 131.63],
        [-152.47, -51.29, 130.56],
        [-148.38, -49.97, 133.46],
        [-143.62, -47.79, 136.40],
        [-138.94, -45.94, 139.30],
        [-129.50, -45.19, 145.13],
        [-125.60, -43.85, 147.40],
    ];
    let down: Vec<[f32; 3]> = route[3..].iter().rev().copied().collect();
    let mut wolf = wolf_on(&cache, world.clone(), [-197.45, -48.16, 150.72]);
    let up = follow(&mut wolf, &world, &route, &[]).unwrap();
    let back = follow(&mut wolf, &world, &down, &[]).unwrap();
    // The bands are in reach along the way (the first landing alone has 79 triangles within
    // 4 m), so the stairs are walked with the walls tested; but a slide tests only the walls at
    // its feet's height and in its reach, not every band in the cells it touches (#149: up to
    // 276 a slide, which with its fine sub-steps cost far more than the bare map's slide).
    let (walled, most) = (up[0] + back[0], up[1].max(back[1]));
    assert!(
        walled > 200 && most < 100,
        "walls on {walled} ticks, up to {most} a slide"
    );
}

/// #149, #203: the `h023300` cliff band crosses the root of the rideable pine over the start
/// ravine (`o002500_0005`), facing the platform, as the map's bands cross every edge the map
/// collision ends at. A band holds only ground actions, so Wolf walks from the platform onto the
/// trunk and back.
#[test]
fn walking_onto_the_pine_trunk_crosses_its_root_band() {
    let Some((cache, dir)) = map_dir() else {
        return;
    };
    let mut world = CollisionWorld::load_map(&dir).unwrap();
    world
        .add_block_objects(&dir, "m11_00_00_00", |part, who| {
            // The trees' own walls hold only riders, as in the play world.
            if part.collision_type == 6 {
                hit_filter::ride_mask(who)
            } else {
                who
            }
        })
        .unwrap();
    let world = Arc::new(world);
    let (platform, trunk) = ([-135.5, -42.4, -117.0], [-136.0, -43.2, -120.0]);
    let mut wolf = wolf_on(&cache, world.clone(), trunk);
    follow(&mut wolf, &world, &[platform], &[]).expect("walked off the trunk onto the platform");
    follow(&mut wolf, &world, &[[-135.9, -42.8, -118.8], trunk], &[])
        .expect("walked from the platform onto the trunk");
}
