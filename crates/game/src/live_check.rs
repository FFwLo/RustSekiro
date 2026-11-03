//! `--live-check <summary.json>`: measurements of the real play path, for the live regression
//! test (`tests/it/live_play.rs`) and for diagnosing a slow or incomplete map by hand.
//!
//! Records, in seconds since the process started: the first window frame, Wolf's rig bound,
//! the area ready (play starts after a short settle, see `play_gate.rs`),
//! the map ground under Wolf drawn, the first simulation step (the player can move) and every
//! map asset loaded. Once the player can move it samples Wolf at 4 Hz: the collision floor
//! under him and whether a drawn map triangle lies on it (a ground hole is a grounded sample
//! with collision but no drawn ground), plus how much of the collision floor within 20 m lies
//! inside drawn map mesh bounds, and each map foe within 60 m (think row, goal chain, battle
//! state, LOD, clip, route point and position). Frame times are taken after a settle period. Without a map (`--play` with no
//! `--map`, the flat-floor fight) samples and stops record Wolf's position, grounded state and
//! step only.
//!
//! `--live-tour x,y,z;x,y,z;...` (or `starts` for every player start of the map) moves Wolf to
//! each point in turn once play has started, waits three seconds and records a stop sample;
//! `--live-shots <dir>` also saves a screenshot per stop. At a point written `drop:x,y,z` Wolf is
//! let go in the air instead, and the stop, eight seconds later, records the lowest height he
//! fell to and his contacts with map action event walls (`sekiro_sim::hit_filter`). The run
//! ends after the tour, the input script (plus two seconds, and its `--script-route`) or
//! `--live-seconds` of play, writes the summary and exits.
//! `--live-orbit <step>,<seconds>` turns the play camera through a full circle around Wolf,
//! starting at that simulation step and taking that long (with `--live-shots`, a screenshot every
//! 45 degrees); the run also waits for the turn. Every connected map block's request state
//! change after play starts (`map_world`) is recorded with the collision part selecting the map,
//! so a run can assert that a block never retires while the camera turns; `selections` lists
//! every collision part that selected the map after play started.
//! Every line is also logged as `live check:` for manual runs.
use super::{MapCollision, Sim, WOLF};
use bevy::app::AppExit;
use bevy::camera::primitives::Aabb;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Radius of the floor-coverage survey around Wolf.
const SURVEY_RADIUS: f32 = 20.0;
const SURVEY_SPACING: f32 = 2.0;
/// A drawn triangle this close above or below the collision floor counts as its ground.
const GROUND_TOLERANCE: f32 = 1.0;
/// Seconds at each tour stop before it is sampled: visibility, detail levels and streaming
/// react to the move first.
const TOUR_WAIT: f32 = 3.0;
/// Seconds at a `drop:x,y,z` tour stop: a 30 m fall, the fall return's fade and the return.
const DROP_WAIT: f32 = 8.0;
/// Map foes within this distance of Wolf are listed in each sample.
const FOE_RADIUS: f32 = 60.0;
/// Degrees between `--live-orbit` screenshots.
const ORBIT_SHOT_DEGREES: f32 = 45.0;

/// The `--live-orbit` camera turn: when it starts, how long it takes, and its progress.
struct OrbitTurn {
    step: usize,
    seconds: f32,
    /// When the turn began and the camera yaw it began from.
    began: Option<(Instant, f32)>,
    shots: usize,
    done: bool,
}

#[derive(Resource)]
pub(super) struct LiveCheck {
    output: PathBuf,
    play_seconds: f32,
    /// Tour points, each with whether Wolf is dropped there (`drop:x,y,z`).
    tour: Vec<(Vec3, bool)>,
    tour_index: usize,
    tour_arrived: Option<Instant>,
    /// Lowest feet height since the current tour stop began.
    lowest_y: f32,
    last_stop: Option<Instant>,
    shots: Option<PathBuf>,
    first_frame: Option<f32>,
    rig_bound: Option<f32>,
    play_ready: Option<f32>,
    ground_drawn: Option<f32>,
    sim_started: Option<(f32, Instant)>,
    map_loaded: Option<f32>,
    frames_ms: Vec<f32>,
    /// Longest frame before play started (a main-thread stall while loading), and the longest
    /// in the first three seconds of play (a hitch as the loading screen lifts).
    longest_loading_ms: f32,
    longest_start_ms: f32,
    /// This frame ran a 4 Hz ground sample, whose cost lands in the next frame's time.
    sampled: bool,
    samples: Vec<serde_json::Value>,
    stops: Vec<serde_json::Value>,
    holes: usize,
    grounded_samples: usize,
    last_sample: Option<Instant>,
    coverage: Vec<f32>,
    orbit: Option<OrbitTurn>,
    /// Request state per map block, and every change after play started.
    block_states: BTreeMap<String, String>,
    block_changes: Vec<serde_json::Value>,
    /// Every collision part selecting the map after play started, in order, without repeats.
    selections: Vec<Option<String>>,
    finished: bool,
}

fn parse_orbit(value: &str) -> anyhow::Result<OrbitTurn> {
    let (step, seconds) = value
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("--live-orbit is <step>,<seconds>"))?;
    let seconds: f32 = seconds.trim().parse()?;
    anyhow::ensure!(seconds > 0.0, "--live-orbit needs a positive duration");
    Ok(OrbitTurn {
        step: step.trim().parse()?,
        seconds,
        began: None,
        shots: 0,
        done: false,
    })
}

fn parse_tour(value: &str, map_dir: Option<&Path>) -> anyhow::Result<Vec<(Vec3, bool)>> {
    if value == "starts" {
        let dir = map_dir.ok_or_else(|| anyhow::anyhow!("--live-tour starts needs --map"))?;
        let mut points = Vec::new();
        while let Ok((p, _)) = crate::map::player_start(dir, points.len()) {
            points.push((p, false));
        }
        return Ok(points);
    }
    value
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|point| {
            let point = point.trim();
            let (point, drop) = match point.strip_prefix("drop:") {
                Some(rest) => (rest, true),
                None => (point, false),
            };
            let v: Vec<f32> = point
                .split(',')
                .map(|c| c.trim().parse::<f32>())
                .collect::<Result<_, _>>()?;
            anyhow::ensure!(v.len() == 3, "--live-tour points are x,y,z or drop:x,y,z");
            Ok((Vec3::new(v[0], v[1], v[2]), drop))
        })
        .collect()
}

impl LiveCheck {
    pub(super) fn from_args(
        args: &[String],
        map_dir: Option<&Path>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(output) = crate::arg_value(args, "--live-check") else {
            return Ok(None);
        };
        anyhow::ensure!(!output.starts_with("--"), "--live-check wants a file path");
        let play_seconds = crate::arg_f32(args, "--live-seconds", 20.0)?;
        let tour = crate::arg_value(args, "--live-tour")
            .map(|v| parse_tour(&v, map_dir))
            .transpose()?
            .unwrap_or_default();
        Ok(Some(Self {
            output: PathBuf::from(output),
            play_seconds,
            tour,
            tour_index: 0,
            tour_arrived: None,
            lowest_y: f32::INFINITY,
            last_stop: None,
            shots: crate::arg_value(args, "--live-shots")
                .map(|dir| -> anyhow::Result<PathBuf> {
                    std::fs::create_dir_all(&dir)?;
                    Ok(PathBuf::from(dir))
                })
                .transpose()?,
            first_frame: None,
            rig_bound: None,
            play_ready: None,
            ground_drawn: None,
            sim_started: None,
            map_loaded: None,
            frames_ms: Vec::new(),
            longest_loading_ms: 0.0,
            longest_start_ms: 0.0,
            sampled: false,
            samples: Vec::new(),
            stops: Vec::new(),
            holes: 0,
            grounded_samples: 0,
            last_sample: None,
            coverage: Vec::new(),
            orbit: crate::arg_value(args, "--live-orbit")
                .map(|v| parse_orbit(&v))
                .transpose()?,
            block_states: BTreeMap::new(),
            block_changes: Vec::new(),
            selections: Vec::new(),
            finished: false,
        }))
    }
}

/// A drawn map mesh near `centre`: its entity, world bounds and transform. Map descendants
/// only, so Wolf's own meshes never count.
struct Drawn {
    mesh: AssetId<Mesh>,
    lo: Vec3,
    hi: Vec3,
    transform: GlobalTransform,
}

type MeshQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static InheritedVisibility,
        &'static GlobalTransform,
        &'static Aabb,
        &'static Mesh3d,
    ),
>;

fn drawn_map_meshes(
    centre: Vec3,
    radius: f32,
    meshes: &MeshQuery,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<crate::map::DrawGroups>>,
) -> Vec<Drawn> {
    let mut out = Vec::new();
    for (entity, visible, transform, aabb, mesh) in meshes {
        if !visible.get() {
            continue;
        }
        let (lo, hi) = crate::map::world_bounds(transform, aabb);
        let reach = ((hi - lo) * 0.5).xz().length();
        if ((lo + hi) * 0.5).xz().distance(centre.xz()) > radius + reach {
            continue;
        }
        if crate::map::map_root(entity, parents, roots).is_some() {
            out.push(Drawn {
                mesh: mesh.id(),
                lo,
                hi,
                transform: *transform,
            });
        }
    }
    out
}

fn covered(drawn: &[Drawn], p: Vec3) -> bool {
    const SLACK: Vec3 = Vec3::new(0.25, GROUND_TOLERANCE, 0.25);
    drawn
        .iter()
        .any(|d| p.cmpge(d.lo - SLACK).all() && p.cmple(d.hi + SLACK).all())
}

/// Whether a drawn map triangle crosses the vertical line through `p` within
/// [`GROUND_TOLERANCE`] of `p.y`. `None` when no candidate mesh keeps CPU vertex data.
fn ground_triangle(drawn: &[Drawn], meshes: &Assets<Mesh>, p: Vec3) -> Option<bool> {
    let mut known = false;
    for d in drawn {
        if p.x < d.lo.x - 0.25
            || p.x > d.hi.x + 0.25
            || p.z < d.lo.z - 0.25
            || p.z > d.hi.z + 0.25
            || p.y < d.lo.y - GROUND_TOLERANCE
            || p.y > d.hi.y + GROUND_TOLERANCE
        {
            continue;
        }
        let Some(mesh) = meshes.get(d.mesh) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).ok()
        else {
            continue;
        };
        known = true;
        let inverse = d.transform.affine().inverse();
        let top = p + Vec3::Y * GROUND_TOLERANCE;
        let origin = inverse.transform_point3(top);
        let direction = inverse.transform_vector3(Vec3::NEG_Y * (2.0 * GROUND_TOLERANCE));
        let hit = |a: usize, b: usize, c: usize| {
            let (Some(a), Some(b), Some(c)) =
                (positions.get(a), positions.get(b), positions.get(c))
            else {
                return false;
            };
            // The segment parameter 0..1 spans the tolerance band.
            sekiro_sim::collision::ray_triangle(
                origin,
                direction,
                Vec3::from(*a),
                Vec3::from(*b),
                Vec3::from(*c),
            )
            .is_some_and(|t| t <= 1.0)
        };
        let found = match mesh.indices() {
            Some(Indices::U16(i)) => i
                .as_chunks::<3>()
                .0
                .iter()
                .any(|t| hit(t[0] as usize, t[1] as usize, t[2] as usize)),
            Some(Indices::U32(i)) => i
                .as_chunks::<3>()
                .0
                .iter()
                .any(|t| hit(t[0] as usize, t[1] as usize, t[2] as usize)),
            None => (0..positions.len() / 3).any(|t| hit(3 * t, 3 * t + 1, 3 * t + 2)),
        };
        if found {
            return Some(true);
        }
    }
    known.then_some(false)
}

/// Collision floor and drawn-ground state at `p`, with the floor coverage around it.
fn sample_ground(
    collision: &MapCollision,
    drawn: &[Drawn],
    meshes: &Assets<Mesh>,
    p: Vec3,
) -> serde_json::Value {
    let hit = collision.0.floor_hit(p.x, p.z, p.y + 1.0);
    let floor = hit.map(|(y, _)| y);
    let under = floor.map(|y| Vec3::new(p.x, y, p.z));
    let precise = under.and_then(|u| ground_triangle(drawn, meshes, u));
    let in_bounds = under.is_some_and(|u| covered(drawn, u));
    let (mut floors, mut hits) = (0usize, 0usize);
    let steps = (SURVEY_RADIUS / SURVEY_SPACING) as i32;
    for i in -steps..=steps {
        for j in -steps..=steps {
            let q = p.xz() + Vec2::new(i as f32, j as f32) * SURVEY_SPACING;
            if q.distance(p.xz()) > SURVEY_RADIUS {
                continue;
            }
            let Some(y) = collision.0.floor(q.x, q.y, p.y + 4.0) else {
                continue;
            };
            if (y - p.y).abs() > 4.0 {
                continue;
            }
            floors += 1;
            hits += usize::from(covered(drawn, Vec3::new(q.x, y, q.y)));
        }
    }
    serde_json::json!({
        "position": p.to_array(),
        "floor": floor,
        "floor_object": hit.and_then(|(_, t)| collision.0.object_at(t)),
        // A drawn triangle on the floor; falls back to drawn bounds when no mesh keeps
        // CPU vertex data.
        "ground_drawn": precise.unwrap_or(in_bounds),
        "ground_triangle": precise,
        "ground_in_bounds": in_bounds,
        "floor_coverage": if floors == 0 { 1.0 } else { hits as f32 / floors as f32 },
        "drawn_meshes_near": drawn.len(),
    })
}

/// Turns the camera for `--live-orbit` and records map block request states, before
/// [`observe`] decides whether the run is done.
pub(super) fn observe_world(
    mut commands: Commands,
    mut check: ResMut<LiveCheck>,
    sim: NonSend<Sim>,
    mut orbit: ResMut<super::Orbit>,
    world: Option<Res<crate::map::world::MapWorld>>,
    camera: Query<&GlobalTransform, With<super::FollowCamera>>,
) {
    if check.finished {
        return;
    }
    let now = crate::since_start();
    let playing = check.sim_started.is_some();
    let mut world_selected = None;
    if let Some(world) = world {
        let selected = world.selected.as_ref().map(|(b, c)| format!("{b}/{c}"));
        world_selected.clone_from(&selected);
        if playing && check.selections.last() != Some(&selected) {
            info!(
                "live check: map selected by {selected:?} at step {}",
                sim.step
            );
            check.selections.push(selected.clone());
        }
        for (id, block) in &world.blocks {
            // A demanded block with no display groups draws none of its parts.
            let empty = if block.normal == [0; 8] {
                " (no groups)"
            } else {
                ""
            };
            let state = format!("{:?}{empty}", block.state);
            if check.block_states.get(id) == Some(&state) {
                continue;
            }
            check.block_states.insert(id.clone(), state.clone());
            if playing {
                let change = serde_json::json!({
                    "t": now,
                    "step": sim.step,
                    "block": id,
                    "state": state,
                    "selected": selected,
                    "position": sim.duel.player.body.position,
                    "camera_yaw_degrees": orbit.yaw.to_degrees(),
                });
                info!("live check: block {change}");
                check.block_changes.push(change);
            }
        }
    }
    let shots = check.shots.clone();
    let Some(turn) = check.orbit.as_mut().filter(|t| !t.done && playing) else {
        return;
    };
    if turn.began.is_none() && sim.step >= turn.step {
        turn.began = Some((Instant::now(), orbit.yaw));
        info!(
            "live check: orbit starts at step {} from yaw {:.1}",
            sim.step,
            orbit.yaw.to_degrees()
        );
    }
    let Some((began, base)) = turn.began else {
        return;
    };
    let fraction = (began.elapsed().as_secs_f32() / turn.seconds).min(1.0);
    let turned = 360.0 * fraction;
    orbit.yaw = base + turned.to_radians();
    if turned >= turn.shots as f32 * ORBIT_SHOT_DEGREES {
        if let Some(dir) = &shots {
            let path = dir.join(format!(
                "orbit-{:03}.png",
                turn.shots as f32 * ORBIT_SHOT_DEGREES
            ));
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
        info!(
            "live check: orbit {:.0} degrees, camera yaw {:.1} at {:?}, Wolf at {:?}, selected {:?}",
            turned,
            orbit.yaw.to_degrees(),
            camera.iter().next().map(|t| t.translation()),
            sim.duel.player.body.position,
            world_selected,
        );
        turn.shots += 1;
    }
    if fraction >= 1.0 {
        turn.done = true;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn observe(
    mut commands: Commands,
    mut check: ResMut<LiveCheck>,
    mut sim: NonSendMut<Sim>,
    gate: Res<super::play_gate::PlayGate>,
    collision: Option<Res<MapCollision>>,
    pending: Option<Res<crate::map::Pending>>,
    rigs: Query<&crate::character::Rig>,
    meshes: MeshQuery,
    mesh_assets: Res<Assets<Mesh>>,
    entities: Query<()>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<crate::map::DrawGroups>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.finished {
        return;
    }
    let now = crate::since_start();
    if check.first_frame.is_none() {
        check.first_frame = Some(now);
        info!("live check: first frame {now:.1} s");
    }
    if check.rig_bound.is_none() && !rigs.is_empty() && rigs.iter().all(|r| r.bound) {
        check.rig_bound = Some(now);
        info!("live check: rig bound {now:.1} s");
    }
    if check.play_ready.is_none() && gate.ready_at.is_some() {
        check.play_ready = Some(now);
        info!("live check: area ready {now:.1} s");
    }
    if check.map_loaded.is_none() && pending.as_ref().is_some_and(|p| p.loaded()) {
        check.map_loaded = Some(now);
        info!("live check: all map assets loaded {now:.1} s");
    }
    if check.sim_started.is_none() && check.first_frame != Some(now) {
        check.longest_loading_ms = check.longest_loading_ms.max(time.delta_secs() * 1000.0);
    }
    if check.sim_started.is_none() && sim.step > 0 {
        check.sim_started = Some((now, Instant::now()));
        info!("live check: simulation started {now:.1} s");
        // The first frame without the loading screen: nearby pieces must be drawn textured.
        if let Some(dir) = &check.shots {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(dir.join("play-start.png")));
        }
    }
    // Play frames, without the ones paying for this check's own ground samples.
    let sampled_before = std::mem::take(&mut check.sampled);
    if let Some((_, started)) = check.sim_started
        && check.tour.is_empty()
        && !sampled_before
    {
        let ms = time.delta_secs() * 1000.0;
        check.frames_ms.push(ms);
        if started.elapsed().as_secs_f32() <= 3.0 {
            check.longest_start_ms = check.longest_start_ms.max(ms);
        }
    }
    // Without a map (the flat-floor fight) there is no collision or map ground to survey;
    // samples, stops and the end of the run still work, with position only.
    let ground = |p: Vec3| match &collision {
        Some(collision) => {
            let drawn = drawn_map_meshes(p, SURVEY_RADIUS, &meshes, &parents, &roots);
            sample_ground(collision, &drawn, &mesh_assets, p)
        }
        None => serde_json::json!({ "position": p.to_array() }),
    };
    let p = Vec3::from_array(sim.duel.player.body.position);
    // Tour: move Wolf to the next point, wait, then sample it.
    if check.sim_started.is_some() && check.tour_index < check.tour.len() {
        let (target, drop) = check.tour[check.tour_index];
        let wait = if drop { DROP_WAIT } else { TOUR_WAIT };
        match check.tour_arrived {
            None => {
                let y = match &collision {
                    Some(collision) if !drop => collision
                        .0
                        .floor(target.x, target.z, target.y + 1.0)
                        .unwrap_or(target.y),
                    _ => target.y,
                };
                let body = &mut sim.duel.player.body;
                body.position = [target.x, y, target.z];
                body.velocity = [0.0; 3];
                body.grounded = !drop;
                body.fall_top = y;
                let yaw = sim.duel.player.visual_yaw();
                sim.previous[WOLF] = ([target.x, y, target.z], yaw);
                check.tour_arrived = Some(Instant::now());
                check.lowest_y = y;
                sim.duel.helper_events.clear();
            }
            Some(arrived) if arrived.elapsed().as_secs_f32() >= wait => {
                let mut stop = ground(p);
                stop["stop"] = serde_json::json!(check.tour_index);
                stop["t"] = serde_json::json!(now);
                stop["grounded"] = serde_json::json!(sim.duel.player.body.grounded);
                if drop {
                    stop["lowest_y"] = serde_json::json!(check.lowest_y);
                    stop["helper_events"] = serde_json::json!(sim.duel.helper_events);
                }
                if let Some(dir) = &check.shots {
                    let path = dir.join(format!("tour-{:02}.png", check.tour_index));
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(path.clone()));
                    stop["screenshot"] = serde_json::json!(path);
                }
                info!("live check: stop {stop}");
                check.stops.push(stop);
                check.tour_index += 1;
                check.tour_arrived = None;
                check.last_stop = Some(Instant::now());
            }
            Some(_) => check.lowest_y = check.lowest_y.min(p.y),
        }
    }
    // Sample at 4 Hz: the bounds scan walks every mesh.
    if check
        .last_sample
        .is_some_and(|t| t.elapsed().as_secs_f32() < 0.25)
    {
        return;
    }
    check.last_sample = Some(Instant::now());
    check.sampled = true;
    let mut sample = ground(p);
    let drawn_ground = sample["ground_drawn"].as_bool() == Some(true);
    if drawn_ground && check.ground_drawn.is_none() {
        check.ground_drawn = Some(now);
        info!("live check: ground under Wolf drawn {now:.1} s");
    }
    if check.sim_started.is_none() {
        return;
    }
    let body = &sim.duel.player.body;
    let grounded = body.grounded && sample["floor"].is_number();
    check.grounded_samples += usize::from(grounded);
    if grounded && !drawn_ground {
        check.holes += 1;
    }
    if let Some(c) = sample["floor_coverage"].as_f64() {
        check.coverage.push(c as f32);
    }
    sample["t"] = serde_json::json!(now);
    sample["step"] = serde_json::json!(sim.step);
    sample["grounded"] = serde_json::json!(body.grounded);
    let wire = &sim.duel.player.wire;
    // The wire perch still holding Wolf up, if any (#230: it ends once he leaves it).
    sample["perch"] = serde_json::json!(wire.perch.map(|p| p.to_array()));
    sample["grapple"] = wire
        .preview
        .as_ref()
        .map_or(serde_json::Value::Null, |target| {
            serde_json::json!({
                "point": target.point.to_array(),
                "distance": target.point.distance(Vec3::from_array(body.position)),
                "state": format!("{:?}", target.state),
                "can_fire": wire.can_fire(),
            })
        });
    // Map foes near Wolf: their AI's think row, goal chain and state (battle, target state 0
    // none / 1 caution / 2 find / 3 battle, the heard AiSoundParam row), scheduling level, the
    // route point of a patrol and position, to see which stand, walk or fight and why.
    sample["foes"] = sim
        .duel
        .population
        .as_ref()
        .map(|population| {
            population
                .foes
                .iter()
                .filter_map(|foe| {
                    let at = foe.duel.enemy.body.position;
                    if Vec3::from_array(at).distance(Vec3::from_array(body.position)) >= FOE_RADIUS
                    {
                        return None;
                    }
                    let ai = foe.duel.ai.as_ref();
                    let patrol = ai.and_then(|ai| ai.state.patrol.as_ref());
                    Some(serde_json::json!({
                        "part": foe.part,
                        "think": ai.map(|ai| ai.state.think.id),
                        "goals": ai.map(|ai| ai.goal_stack().join(">")),
                        "battle": ai.is_some_and(|ai| ai.state.battle),
                        "target_state": ai.map(|ai| ai.state.target_state),
                        "sound": ai.and_then(|ai| ai.state.sound.map(|s| s.id)),
                        "lod": format!("{:?}", foe.lod),
                        "clip": foe.report.enemy.animation,
                        "point": patrol.map(|p| p.current),
                        "previous": patrol.map(|p| p.previous),
                        "at": at,
                        "hp": foe.duel.rules.fighters[1].vitality.hp,
                        "fall_death": foe.duel.enemy_fall.death.as_ref().map(|d| format!("{d:?}")),
                        "opacity": foe.duel.enemy_fall_opacity(),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .into();
    info!("live check: sample {sample}");
    check.samples.push(sample);
    // The last stop's screenshot is written before the run ends.
    let tour_done = !check.tour.is_empty()
        && check.tour_index >= check.tour.len()
        && check
            .last_stop
            .is_some_and(|t| t.elapsed().as_secs_f32() > 1.5);
    let script_done = sim
        .script
        .as_ref()
        .is_some_and(|s| sim.step >= s.len() + 120 && sim.route.done());
    let timed_out = check
        .sim_started
        .is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= check.play_seconds);
    let orbit_done = check.orbit.as_ref().is_none_or(|t| t.done);
    let done = orbit_done
        && if !check.tour.is_empty() {
            tour_done
        } else {
            script_done || (sim.script.is_none() && timed_out)
        };
    if !done {
        return;
    }
    check.finished = true;
    let frames = &check.frames_ms;
    let pick =
        |q: f32| (!frames.is_empty()).then(|| sekiro_sim::brain::metrics::percentile(frames, q));
    let longest = frames.iter().copied().reduce(f32::max);
    let min_coverage = check.coverage.iter().copied().fold(1.0f32, f32::min);
    let stop_holes = check
        .stops
        .iter()
        .filter(|s| s["floor"].is_number() && s["ground_drawn"].as_bool() != Some(true))
        .count();
    let summary = serde_json::json!({
        "first_frame_s": check.first_frame,
        "rig_bound_s": check.rig_bound,
        "play_ready_s": check.play_ready,
        "ground_drawn_s": check.ground_drawn,
        "sim_started_s": check.sim_started.map(|s| s.0),
        "map_loaded_s": check.map_loaded,
        "frames": frames.len(),
        "frame_ms_median": pick(0.5),
        "frame_ms_p95": pick(0.95),
        "frame_ms_max": longest,
        "longest_loading_frame_ms": check.longest_loading_ms,
        "longest_start_frame_ms": check.longest_start_ms,
        "grounded_samples": check.grounded_samples,
        "ground_holes": check.holes,
        "floor_coverage_min": min_coverage,
        "stops": check.stops,
        "stop_holes": stop_holes,
        "mesh_entities": meshes.iter().count(),
        "entities": entities.iter().count(),
        "final_position": sim.duel.player.body.position,
        "orbit_turned": check.orbit.as_ref().map(|t| t.done),
        "block_states": check.block_states,
        "block_changes": check.block_changes,
        "selections": check.selections,
        "samples": check.samples,
    });
    info!(
        "live check: summary first frame {:?} s, rig {:?} s, ready {:?} s, ground {:?} s, sim {:?} s, map {:?} s, frame median {:?} ms p95 {:?} ms max {:?} ms, longest loading frame {:.0} ms, holes {}/{}, stop holes {}/{}, min floor coverage {:.2}",
        check.first_frame,
        check.rig_bound,
        check.play_ready,
        check.ground_drawn,
        check.sim_started.map(|s| s.0),
        check.map_loaded,
        pick(0.5),
        pick(0.95),
        longest,
        check.longest_loading_ms,
        check.holes,
        check.grounded_samples,
        stop_holes,
        check.stops.len(),
        min_coverage
    );
    match serde_json::to_vec_pretty(&summary) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&check.output, bytes) {
                error!("live check: writing {}: {error}", check.output.display());
            }
        }
        Err(error) => error!("live check: {error}"),
    }
    info!(
        "live check: exit requested {:.2} s after launch",
        crate::since_start()
    );
    exit.write(AppExit::Success);
}
