//! `--play`: Wolf against an enemy (`--enemy`, default the Ashina soldier c1010; `c5060` is the
//! Great Shinobi Owl) on a floor, both driven by the game's own scripts, behaviour graphs and TAE
//! (`sekiro_sim::duel::Duel`).
//!
//! The simulation runs at a fixed 60 steps per second. Each step it receives an [`InputFrame`]
//! built from keyboard, mouse and gamepad (or from an input script) and reports, per character,
//! the full-body clip, its time, whether it just started and its TAE blend-in, plus the body's
//! position and facing. This module renders that: an [`Animator`] per character plays the
//! reported clip at the reported time with root motion disabled, crossfading over the blend-in,
//! and the entity follows the simulated body.
//!
//! Controls are Sekiro's PC defaults: the keyboard and mouse bindings are the consts beside
//! [`CROUCH_KEY`] and [`keyboard_input`], the gamepad ones are in `gather_input`, and the menu
//! keys are `crate::menu::MENU_OK_KEY` and its neighbours. Esc opens the pause menu; minus and
//! equals (or `[` and `]`) change the volume (`crate::audio`).
//!
//! With `--map <id> [--start <n>]` the fight happens on an exported map instead of the floor:
//! Wolf stands at the player start, the enemy 5 m in front of him, both bodies walk on the
//! map's hit collision (`sekiro_sim::collision`), the map renders as in `--map` (see
//! [`crate::map::add_map`]) and the camera is pulled in when a wall is between it and Wolf.
//!
//! With `--arena <name>` (`sekiro_sim::arena`) the fight happens where the game places the
//! boss: his MSB part's transform, Wolf at the arena's player start facing him, and only the
//! map pieces and collision within the arena radius (`--full-map` loads all of it). A
//! character that falls far below its start is put back there.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::app::AppExit;
use bevy::asset::AssetPlugin;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::{CursorGrabMode, CursorOptions};
use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::{Buttons, InputFrame, StepReport, demo};

use crate::character::{AnimLibrary, Animator, Rig, animate, bind_rigs};
use crate::combat_hud;
use crate::draw_mask::{DrawMask, MaskSource, apply_draw_masks};
use crate::map::{self, MapFocus, MapScene, PlayerFocus};
use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::collision::CollisionWorld;
#[path = "arena_c_scene.rs"]
mod arena_c_scene;
#[path = "arena_d_scene.rs"]
mod arena_d_scene;
#[path = "cinematic_fade.rs"]
pub(crate) mod cinematic_fade;
#[path = "game_menu.rs"]
mod game_menu;
#[path = "idol.rs"]
mod idol;
#[path = "idol_found.rs"]
mod idol_found;
#[path = "idol_rest.rs"]
mod idol_rest;
#[path = "idol_travel.rs"]
mod idol_travel;
#[path = "live_check.rs"]
mod live_check;
#[path = "map_enemies.rs"]
mod map_enemies;
#[path = "pickup.rs"]
mod pickup;
#[path = "play_gate.rs"]
mod play_gate;
pub(crate) use play_gate::{PlayGate, fraction};
#[path = "grapple_hud.rs"]
mod grapple_hud;
#[path = "rope.rs"]
mod rope;

pub struct Options {
    pub map_enemies: bool,
    pub brain_overlay: bool,
    pub brain_label: String,
    pub window_size: (u32, u32),
    pub attack_power: sekiro_sim::progression::AttackPower,
    pub cache: PathBuf,
    /// Input script to play instead of live input (`builtin` for the demo).
    pub script: Option<String>,
    /// Raw keyboard and mouse input to replay through the live input path
    /// ([`crate::input_replay`]).
    pub input: Option<String>,
    /// Screenshots to take at given simulation steps of the script: `(step, path)`.
    pub screenshots: Vec<(usize, PathBuf)>,
    /// Exit when the script ends and every screenshot is saved.
    pub exit_after_script: bool,
    /// Let a present enemy fight (`--passive` disables its AI).
    pub enemy: bool,
    /// Include an opponent at all (`--no-enemy` disables its spawn and interactions).
    pub enemy_present: bool,
    /// Which enemy (`--enemy`): its model, animations, scripts, params and AI.
    pub opponent: EnemySpec,
    /// Start locked on to the enemy.
    pub lock_on: bool,
    /// Drive the enemy with the stand-in brain instead of its own AI scripts.
    pub simple_ai: bool,
    /// Testing aid (`--god`): Wolf takes no HP or posture damage; hits, reactions and hit stop
    /// are otherwise unchanged.
    pub god: bool,
    pub immortal: bool,
    /// Testing aid (`--force-act <id>`): the enemy requests this action (an animation id such
    /// as 3040, the Owl's grab) whenever it is free, on top of its AI (or alone with `--passive`).
    pub force_act: Option<i32>,
    /// Testing aid (`--break-posture`): the enemy's posture is held at 1 point outside
    /// deathblows, so Wolf's next landed hit breaks it and opens a deathblow.
    pub break_posture: bool,
    /// Who chooses a boss's acts (`--brain`) and whether its decisions are logged
    /// to `cache/brain_logs/` (`--brain-log`); see `docs/BRAIN.md`.
    pub brain: crate::BrainOptions,
    pub brain_log: bool,
    /// The boss brain's memory file across fights and sessions (`--brain-memory`; by default
    /// `logs/memory/<chr>.json` for hosted brains, none for the others).
    pub brain_memory: Option<PathBuf>,
    pub player: String,
    pub coach_interval: f32,
    pub brain_moves: bool,
    pub brain_move_every: u32,
    pub coach_review_every: u32,
    /// Play on this exported map (`cache/maps/<id>`) instead of the flat floor.
    pub map: Option<String>,
    /// Player start point on the map.
    pub start: usize,
    /// Place Wolf at `x, y, z` (on the floor at or below `y + 1`) facing `yaw` degrees,
    /// instead of at a start point, with the enemy `enemy_distance` in front of him, facing
    /// him. Also works in an arena (a testing aid: put the pair next to a railing).
    pub spawn: Option<[f32; 4]>,
    /// Metres between Wolf and the enemy at a `spawn` or map start (`--enemy-distance`).
    pub enemy_distance: f32,
    /// Time of day (hours) and draw param override for the map lighting.
    pub hour: f32,
    pub gparam: Option<String>,
    /// Start at this idol (BonfireWarpParam `bonfireEntityId`) of `map`, at its warp point:
    /// how Travel to an idol on another map arrives (`--at-idol`).
    pub at_idol: Option<i32>,
    /// What Wolf carries, has learned and found, carried over from the run that travelled
    /// here (`--kit`, JSON).
    pub kit: Option<sekiro_sim::kit::Kit>,
    /// A boss arena (`--arena owl`): the boss's map, his MSB placement and Wolf's start;
    /// replaces `map` and `start`.
    pub arena: Option<&'static ArenaSpec>,
    /// With `arena`, load the whole map instead of the pieces and collision within its radius.
    pub full_map: bool,
    /// On a map, skip map piece models whose name starts with any of these (as `--map --hide`).
    pub hide: Vec<String>,
    /// Also spawn MSB objects on legacy maps and arenas.
    /// Ashina Explore includes ordinary Objects by default, with story-state filtering.
    pub objects: bool,
}

/// The arena's fog walls: where the map's event script puts them. They are drawn with the
/// asset's own effect while the fight's events keep them standing.
#[derive(Resource)]
struct FogWalls(Vec<sekiro_sim::arena::FogWallPlace>);

/// Plays each fog wall's effect on its asset's dummy polygon while the walls stand (their
/// start flag on and end flag off in the fight's events) and lets it fade after.
fn hold_fog_walls(sim: NonSend<Sim>, walls: Res<FogWalls>, mut fx: ResMut<crate::fxr::FxrPlayer>) {
    let standing: Vec<i32> = sim.duel.events.as_ref().map_or_else(Vec::new, |e| {
        e.standing_fog_walls().map(|w| w.object_entity).collect()
    });
    for place in &walls.0 {
        // The dummy polygon (101) sits at the asset's origin, facing along its -Z with +Y up.
        let at = crate::fxr::frame_transform(
            place.position.to_array(),
            (place.rotation * Vec3::NEG_Z).to_array(),
            (place.rotation * Vec3::Y).to_array(),
        );
        fx.hold(
            crate::fxr::HELD,
            place.wall.sfx,
            place.wall.object_entity,
            at,
            standing.contains(&place.wall.object_entity),
        );
    }
}

fn sync_arena_scene(sim: NonSend<Sim>, scene: Option<ResMut<crate::arena_b::Scene>>) {
    if let (Some(mut scene), Some(events)) = (scene, &sim.duel.events) {
        scene.0 = arena_scene(&sim.duel.spec, &events.scripts.cast_scene);
    }
}

fn arena_scene(
    phase: &sekiro_sim::enemy::EnemySpec,
    events: &sekiro_sim::arena::scene::SceneState,
) -> sekiro_sim::arena::scene::SceneState {
    let mut scene = events.clone();
    // The hand-over event can replay SetLighting(3) from retained story flags.
    // The restarted castle duel owns phase one's story time until Tomoe returns.
    if phase.chr == "c7100" && phase.npc_param == 71_001_000 {
        scene.lighting = None;
        scene.gparam_sub_ids.remove(&(11, 1));
        scene.enabled.insert(8_907_080, false);
        scene.enabled_order.retain(|&entity| entity != 8_907_080);
    }
    if scene.lighting.is_none() {
        scene.lighting = sekiro_sim::arena::boss_events(phase).and_then(|e| e.lighting);
    }
    scene
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn renderer_counts(
    sim: NonSend<Sim>,
    entities: Query<Entity>,
    drawn: Query<&Mesh3d>,
    lights: Query<Entity, Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
    cameras: Query<&Camera>,
    rigs: Query<&Rig>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    layered: crate::map::MaterialStats,
    fx: Res<crate::fxr::FxrPlayer>,
    // Map-only (`MapLighting`) and `--perf`-only (`RenderProbe`) resources.
    lighting: Option<Res<crate::lighting::MapLighting>>,
    probe: Option<Res<crate::perf_log::RenderProbe>>,
    mut last: Local<usize>,
) {
    if sim.step == 0 || sim.step % 60 != 59 || *last == sim.step {
        return;
    }
    *last = sim.step;
    info!(
        "renderer probe fight {}: entities={} mesh_entities={} meshes={} materials={} lights={} cameras={} rigs={} fx={:?} hour={} draw_batches={}",
        sim.step / 60,
        entities.iter().count(),
        drawn.iter().count(),
        meshes.len(),
        materials.len() + layered.count(),
        lights.iter().count(),
        cameras.iter().count(),
        rigs.iter().count(),
        fx.live_counts(),
        lighting.map_or(f32::NAN, |l| l.hour),
        probe.map_or(0, |p| p.0.lock().map_or(0, |p| p.draw_batches))
    );
}

/// The map's collision, for the camera; absent on the flat floor.
#[derive(Resource)]
struct MapCollision(Arc<CollisionWorld>);

/// Where model GLBs live below the asset root.
#[derive(Resource)]
struct ModelPath(&'static str);

#[derive(Resource)]
struct EnemyDrawGroups(Option<[bool; 32]>);

#[derive(Resource)]
struct EnemyScene {
    cache: PathBuf,
    spec: EnemySpec,
}

const STEP_HZ: f64 = 60.0;
/// Most sim steps one rendered frame may catch up on (#145).
/// Bevy's default 0.25 s virtual-time cap allows 15, so one slow step snowballs into a stall;
/// past this the game runs slower instead.
const MAX_CATCH_UP_STEPS: u32 = 3;

/// Caps virtual time per frame, which is what feeds the fixed sim loop, at
/// `MAX_CATCH_UP_STEPS` steps.
fn cap_catch_up(mut time: ResMut<Time<Virtual>>) {
    time.set_max_delta(Duration::from_secs_f64(
        f64::from(MAX_CATCH_UP_STEPS) / STEP_HZ,
    ));
}
/// Radians of camera turn per mouse count (0.0035 was reported as far too sensitive).
const MOUSE_SENSITIVITY: f32 = 0.0012;
const WOLF: usize = 0;
const ENEMY: usize = 1;

/// Which simulated character an entity shows (0 Wolf, 1 the enemy).
#[derive(Component)]
struct Actor(usize);

#[derive(Component)]
struct Hud;

#[derive(Resource)]
struct DebugHud(bool);

#[derive(Component)]
struct FollowCamera;

/// Clip libraries per actor.
#[derive(Resource)]
struct Libraries([AnimLibrary; 2]);

/// The simulation and its last report. Not `Send`: the HKS VM uses `Rc`.
struct Sim {
    duel: Duel,
    last: DuelReport,
    /// Clip starts not yet shown, per actor: (clip, blend-in seconds).
    started: [Option<(String, f32)>; 2],
    /// The last hits, for the HUD.
    hit_log: Vec<String>,
    script: Option<Vec<InputFrame>>,
    /// `--script-route`: waypoints Wolf walks to once the script ends, and the next one.
    route: ScriptRoute,
    step: usize,
    lock_requested: bool,
    break_posture: bool,
    passive: bool,
    enemy_present: bool,
    /// Each actor's position and drawn yaw before the last step, to draw between steps.
    previous: [([f32; 3], f32); 2],
    /// Each actor's own clock after the last step: the sum of the time it advanced, which stops
    /// while a hit stop holds it.
    clock: [f32; 2],
    /// Whether each actor is drawn hidden: faded out by its clip's TAE 193 (the Owl's warps).
    faded: [bool; 2],
    /// Seconds since a perilous-attack warning last reached Wolf, for the kanji.
    perilous_age: Option<f32>,
}

#[cfg(test)]
impl Sim {
    /// `duel` with no script, no foe and nothing shown yet, for system tests.
    fn for_test(duel: Duel) -> Self {
        Self {
            duel,
            last: Default::default(),
            started: [None, None],
            hit_log: Vec::new(),
            script: None,
            route: ScriptRoute::default(),
            step: 0,
            lock_requested: false,
            break_posture: false,
            passive: true,
            enemy_present: false,
            previous: [([0.; 3], 0.); 2],
            clock: [0.; 2],
            faded: [false; 2],
            perilous_age: None,
        }
    }
}

fn apply_perf_tour(
    mut tour: ResMut<crate::perf_log::PerfTour>,
    mut sim: NonSendMut<Sim>,
    mut orbit: ResMut<Orbit>,
    gate: Res<play_gate::PlayGate>,
    collision: Option<Res<MapCollision>>,
) {
    if !gate.open() {
        return;
    }
    let Some([x, y, z, yaw]) = tour.next(sim.step) else {
        return;
    };
    let y = collision
        .as_ref()
        .and_then(|c| c.0.floor(x, z, y + 1.))
        .unwrap_or(y);
    sim.duel.player.snap_yaw(yaw.to_radians());
    let body = &mut sim.duel.player.body;
    body.position = [x, y, z];
    body.velocity = [0.; 3];
    body.grounded = true;
    body.fall_top = y;
    sim.previous[WOLF] = ([x, y, z], yaw.to_radians());
    orbit.focus = Vec3::new(x, y, z) + Vec3::Y * sim.duel.camera_director.focus_height;
    orbit.yaw = yaw.to_radians();
    orbit.look = Default::default();
    info!("perf tour: step {} at {x},{y},{z} yaw {yaw}", sim.step);
}

/// Live input gathered every rendered frame, consumed by the fixed steps.
#[derive(Resource, Default)]
struct LiveInput {
    frame: InputFrame,
}

#[derive(Resource)]
struct Orbit {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    /// The deathblow camera's look limits as they chase TAE 151.
    look: sekiro_sim::camera::LookLimits,
}

#[derive(Resource)]
struct Captures {
    pending: Vec<(usize, PathBuf)>,
    in_flight: Vec<PathBuf>,
    exit_after_script: bool,
}

pub fn run(options: Options) -> Result<()> {
    anyhow::ensure!(
        !options.map_enemies || (options.map.is_some() && options.arena.is_none()),
        "--map-enemies requires --play --map and cannot be combined with a boss arena"
    );
    let cache = options.cache.canonicalize().context("cache directory")?;
    if let Some(arena) = options.arena {
        let missing_b = sekiro_sim::arena_b::missing_models(&cache, arena);
        anyhow::ensure!(
            missing_b.is_empty(),
            "arena {}: missing extracted data: {}",
            arena.name,
            missing_b.join(", ")
        );
        let missing_c = sekiro_sim::arena_c::missing_models(&cache, arena);
        anyhow::ensure!(
            missing_c.is_empty(),
            "arena {}: missing extracted data: {}",
            arena.name,
            missing_c.join(", ")
        );
        let missing =
            sekiro_sim::arenas_a::missing_models(&cache, arena).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            missing.is_empty(),
            "arena {}: missing extracted data: {}",
            arena.name,
            missing.join(", ")
        );
    }
    // Seconds since launch at each loading stage, for load-time reports.
    let stage = |name: &str| eprintln!("startup: {name} at {:.1} s", crate::since_start());
    let roster = sekiro_sim::registry::Registry::load(&cache).map_err(anyhow::Error::msg)?;
    // Duel has two mandatory slots; exploration uses an unoccupied soldier slot, never a boss
    // encounter, and does not spawn its model or controller.
    let opponent = if options.enemy_present {
        options.opponent.clone()
    } else {
        EnemySpec::from_cache(&cache, "soldier").map_err(anyhow::Error::msg)?
    };
    let anim_chr = roster.anim_chr(&opponent.chr, opponent.npc_param);
    let enemy_draw_groups = roster.draw_mask(&cache, &options.opponent);
    let libraries = [
        AnimLibrary::load(&cache, "c0000")?,
        AnimLibrary::load(&cache, &anim_chr)?,
    ];
    stage("roster and animations");
    let mut duel = Duel::load_with(&cache, 6.0, opponent)
        .map_err(|e| anyhow::anyhow!("loading the simulation: {e}"))?;
    stage("simulation");
    duel.rules
        .set_attack_power(options.attack_power)
        .map_err(anyhow::Error::msg)?;
    duel.enemy_active = options.enemy;
    duel.god = options.god;
    duel.immortal = options.immortal;
    duel.force_act = options.force_act;
    if options.simple_ai || !options.enemy_present {
        duel.use_simple_ai();
    } else {
        let brain = options.brain.build()?;
        let brain_name = brain.name();
        let log = duel
            .set_brain(brain, options.brain_log)
            .map_err(|e| anyhow::anyhow!("--brain: {e}"))?;
        eprintln!(
            "boss brain {}{}",
            brain_name,
            log.map(|p| format!(", log {}", p.display()))
                .unwrap_or_default()
        );
        if let Some(driver) = duel.boss_brain.as_mut() {
            driver.enable_session_report(Path::new("logs"));
        }
        if let Some(driver) = duel.boss_brain.as_mut() {
            driver.player.clone_from(&options.player);
            driver.coach_interval = options.coach_interval;
            driver.brain_moves = options.brain_moves;
            driver.brain_move_every = options.brain_move_every;
            driver.coach_review_every = options.coach_review_every;
        }
        if let (Some(path), Some(driver)) = (options.brain_memory, duel.boss_brain.as_mut()) {
            driver.use_player_memory(path, &options.player);
        }
    }
    let mut on_map = None;
    let mut stories = map::story::MapStory::default();
    let mut start_yaw = 0.0;
    let mut idols = idol::Idols::default();
    // The item params, shared by the sim (pickups, Storage, restock) and the game menu.
    let item_data = pickup::item_data(&cache);
    let mut pickups = Vec::new();
    let mut within = None;
    let mut fog_walls = Vec::new();
    if let Some(spec) = options.arena {
        let t = std::time::Instant::now();
        let arena = Arena::load(&cache, spec, options.full_map).map_err(|e| {
            anyhow::anyhow!(
                "arena {}: {e} (run `sekiro-extract map {}`)",
                spec.name,
                spec.map
            )
        })?;
        arena.place(&mut duel);
        fog_walls.clone_from(&arena.fog_walls);
        start_yaw = arena.layout.wolf.yaw;
        if let Some([x, y, z, yaw_degrees]) = options.spawn {
            start_yaw = yaw_degrees.to_radians();
            place_pair(
                &mut duel,
                Vec3::new(x, y, z),
                start_yaw,
                options.enemy_distance,
                |x, z, near| arena.world.floor(x, z, near + 1.0).unwrap_or(near),
            );
        }
        if !options.full_map {
            within = Some((arena.layout.center(), spec.radius));
        }
        eprintln!(
            "arena {} on {}: {} at {:?}, Wolf ({}) at {:?}; {} collision triangles{} in {:.2} s",
            spec.name,
            spec.map,
            spec.boss_part,
            duel.enemy.body.position,
            arena.layout.wolf_source,
            duel.player.body.position,
            arena.world.triangle_count(),
            if options.full_map {
                " (full map)".to_string()
            } else {
                format!(" within {} m", spec.radius)
            },
            t.elapsed().as_secs_f32()
        );
        on_map = Some((
            map::map_dir(&cache, spec.map)?,
            spec.map.to_string(),
            arena.world.clone(),
        ));
        map::raise_io_stack();
    } else if let Some(id) = &options.map {
        let dir = map::map_dir(&cache, id)?;
        let blocks = map::world::play_blocks(&cache, id)?;
        // A connected world's stories, loaded once for its objects' collision and the map.
        if map::world::connects(id) {
            stories = map::story::load_blocks(&cache, &blocks)?;
        }
        let world = Arc::new(map::world::collision_world(&cache, &blocks, &stories)?);
        stage("collision world");
        idols = idol::Idols::load(&cache, &blocks, |x, z, y| world.floor(x, z, y + 1.0));
        duel.set_idols(idols.spots.clone());
        pickups = pickup::load(&cache, &blocks, &item_data);
        let at_idol = options.at_idol.and_then(|e| {
            let i = idols.index(e);
            if i.is_none() {
                warn!("--at-idol {e}: no such idol on {id}");
            }
            i
        });
        let (start, yaw_degrees) = match (at_idol, options.spawn) {
            (Some(i), _) => {
                let p = idols.spots[i].checkpoint;
                (Vec3::from(p.position), p.yaw.to_degrees())
            }
            (None, Some([x, y, z, yaw])) => {
                let y = world.floor(x, z, y + 1.0).unwrap_or(y);
                (Vec3::new(x, y, z), yaw)
            }
            (None, None) => map::player_start(&dir, options.start)?,
        };
        let yaw = yaw_degrees.to_radians();
        start_yaw = yaw;
        place_pair(
            &mut duel,
            start,
            yaw,
            options.enemy_distance,
            |x, z, near| world.floor(x, z, near + 1.0).unwrap_or(near),
        );
        if let Some(i) = at_idol {
            duel.start_at_idol(i);
        }
        duel.player.ground = Box::new(world.clone());
        duel.enemy.ground = Box::new(sekiro_sim::collision::EnemyGround(world.clone()));
        // Placeholder until the map's authored void return volumes are decoded.
        duel.fall_respawn_depth = Some(40.0);
        eprintln!(
            "{id}: start {} at {:?}, {} collision triangles",
            options.start,
            duel.player.body.position,
            world.triangle_count()
        );
        on_map = Some((dir, id.clone(), world));
        map::raise_io_stack();
    }
    if let Some((_, id, world)) = &on_map {
        require_floor(duel.player.body.position, |x, z, top| {
            world.floor(x, z, top)
        })
        .with_context(|| format!("{id}: Wolf's start"))?;
    }
    if let Some(kit) = &options.kit {
        duel.kit = kit.clone();
    }
    let has_pickups = !pickups.is_empty();
    duel.set_pickups(pickups, item_data.clone());
    if let Some((dir, _, _)) = &on_map
        && dir.join("edges.json").exists()
    {
        duel.player.docking = sekiro_sim::docking::Docking::load(&dir.join("edges.json"))
            .map_err(anyhow::Error::msg)?;
    }
    if let Some((dir, _, _)) = &on_map
        && dir.join("grapple.json").exists()
    {
        duel.player.wire = sekiro_sim::wire::Wire::load(&cache, dir).map_err(anyhow::Error::msg)?;
    }
    if !options.enemy_present {
        vacant_enemy_slot(&mut duel);
    }
    if options.map_enemies {
        let (_, id, world) = on_map.as_ref().context("--map-enemies requires a map")?;
        let mut population = sekiro_sim::duel::population::Population::load_map(
            &cache,
            id,
            world.clone(),
            duel.player.body.position,
            stories.maps.get(id).map(|s| &s.state),
        )
        .map_err(anyhow::Error::msg)?;
        let mut kinds = std::collections::BTreeMap::<String, usize>::new();
        for foe in &mut population.foes {
            *kinds.entry(foe.duel.spec.chr.clone()).or_default() += 1;
            foe.duel
                .rules
                .set_attack_power(options.attack_power)
                .map_err(anyhow::Error::msg)?;
            foe.duel.enemy_active = options.enemy;
            foe.duel.force_act = options.force_act;
        }
        eprintln!(
            "map enemies {id}: spawned={} active={} kinds={kinds:?}",
            population.foes.len(),
            population.foes.iter().filter(|f| f.active()).count()
        );
        eprintln!(
            "map enemies excluded: {}",
            serde_json::to_string(&population.excluded)?
        );
        duel.set_population(population);
        stage("map enemies");
    }
    let script = match options.script.as_deref() {
        None => None,
        Some("builtin") => Some(demo::parse(demo::DEMO).map_err(anyhow::Error::msg)?),
        Some(path) => Some(
            sekiro_sim::input::parse_action_script(
                &std::fs::read_to_string(path).with_context(|| path.to_string())?,
            )
            .map_err(anyhow::Error::msg)?,
        ),
    };
    let scripted = script.is_some();
    let replay = match options.input.as_deref() {
        None => None,
        Some(path) => Some(crate::input_replay::InputReplay::new(
            crate::input_replay::parse(
                &std::fs::read_to_string(path).with_context(|| path.to_string())?,
            )
            .map_err(anyhow::Error::msg)?,
        )),
    };
    for (_, path) in &options.screenshots {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }

    stage("traversal data");
    // Map GLBs stream outwards from here.
    let start_position = Vec3::from_array(duel.player.body.position);
    let mut app = App::new();
    #[cfg(feature = "profile-spans")]
    let defaults = DefaultPlugins.set(bevy::log::LogPlugin {
        custom_layer: crate::span_profile::layer,
        ..default()
    });
    #[cfg(not(feature = "profile-spans"))]
    let defaults = DefaultPlugins;
    app.add_plugins(
        defaults
            .set(crate::vram::render_plugin())
            .set(crate::map::layered_gltf_plugin())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "sekiro-rs".into(),
                    resolution: options.window_size.into(),
                    present_mode: crate::presentation::requested_mode(),
                    // Make the first input frame's request a false -> true transition.
                    // Bevy's winit backend only calls focus_window on that transition.
                    focused: false,
                    ..default()
                }),
                // The input system requests focus and cursor lock on the first frame;
                // initial scripted/replay settings remain useful before that system runs.
                primary_cursor_options: Some(CursorOptions {
                    visible: scripted || replay.is_some(),
                    grab_mode: if scripted || replay.is_some() {
                        CursorGrabMode::None
                    } else {
                        CursorGrabMode::Locked
                    },
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                // On a map the asset root is the cache, so models and map pieces both load.
                file_path: if on_map.is_some() {
                    cache.to_string_lossy().into_owned()
                } else {
                    cache.join("models").to_string_lossy().into_owned()
                },
                ..default()
            }),
    )
    .insert_resource(ModelPath(if on_map.is_some() { "models/" } else { "" }))
    .insert_resource(ClearColor(Color::srgb(0.52, 0.60, 0.70)))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.95, 0.95, 1.0),
        brightness: 450.0,
        ..default()
    })
    .insert_resource(bevy::winit::WinitSettings::continuous())
    .insert_resource(crate::map::upload_budget())
    .insert_resource(Time::<Fixed>::from_hz(STEP_HZ))
    .add_systems(Startup, cap_catch_up)
    .insert_resource(Libraries(libraries))
    .insert_resource(EnemyScene {
        cache: cache.clone(),
        spec: duel.spec.clone(),
    })
    .insert_resource(EnemyDrawGroups(enemy_draw_groups))
    .insert_resource(LiveInput::default())
    .insert_resource(DebugHud(options.enemy_present))
    .insert_resource(Orbit {
        focus: Vec3::from_array(duel.player.body.position) + Vec3::Y * duel.camera.chr_org_offset_y,
        // Behind Wolf, so scripted "forward" is the way he faces.
        yaw: start_yaw,
        pitch: 0.25,
        distance: duel.camera.cam_dist,
        look: Default::default(),
    })
    .insert_resource(Captures {
        pending: options.screenshots,
        in_flight: Vec::new(),
        exit_after_script: options.exit_after_script,
    })
    .insert_non_send(Sim {
        last: DuelReport::default(),
        duel,
        started: [None, None],
        hit_log: Vec::new(),
        script,
        route: ScriptRoute::parse(&std::env::args().collect::<Vec<_>>())?,
        step: 0,
        lock_requested: options.lock_on && (options.enemy_present || options.map_enemies),
        break_posture: options.break_posture,
        passive: !options.enemy,
        enemy_present: options.enemy_present,
        previous: [([0.0; 3], 0.0); 2],
        clock: [0.0; 2],
        faded: [false; 2],
        perilous_age: None,
    })
    .init_resource::<combat_hud::Gauges>()
    .add_plugins(grapple_hud::plugin)
    .insert_resource(crate::brain_overlay::RecordingBrain {
        enabled: options.brain_overlay && options.enemy_present,
        mode: options.brain_label,
        player: options.player,
        view: Default::default(),
        unicode_font: false,
    })
    .init_resource::<crate::bullet_fx::BulletFx>()
    .add_plugins(crate::decals::DecalPlugin {
        cache: cache.clone(),
    })
    .add_plugins(crate::fxr::plugin(cache.clone(), |app| {
        app.add_systems(
            FixedUpdate,
            (|sim: NonSend<Sim>,
              mut fx: ResMut<crate::fxr::FxrPlayer>,
              mut resets: MessageWriter<crate::decals::ClearStains>,
              mut stains: MessageWriter<crate::decals::Stain>| {
                if sim.last.respawned {
                    fx.reset_fight();
                    resets.write(crate::decals::ClearStains);
                }
                fx.observe(&sim.duel);
                crate::decals::forward(&mut fx, &sim.duel, &sim.last, &mut stains);
                if std::env::var_os("PF_RESPAWN_PROBE").is_some() && sim.step % 60 == 10 {
                    for _ in 0..16 {
                        stains.write(crate::decals::Stain {
                            id: 50010,
                            at: Vec3::from_array(sim.duel.player.body.position) + Vec3::Y,
                            floor: sim.duel.player.body.position[1],
                            blow: None,
                        });
                    }
                }
            })
            .after(simulate)
            .run_if(crate::menu::active),
        );
    }))
    .add_plugins(cinematic_fade::CinematicFadePlugin)
    .add_plugins(crate::cloth::plugin(cache.clone()))
    .add_plugins(crate::audio::GameAudioPlugin {
        cache: cache.clone(),
        map: on_map.as_ref().map(|(_, id, _)| id.clone()),
        enemy: options
            .enemy_present
            .then(|| (options.opponent.chr.clone(), options.opponent.npc_param)),
    })
    // Wolf's and the enemy's models load before the map's GLBs are queued.
    .add_systems(Startup, setup.before(map::MapSpawn))
    .init_resource::<play_gate::PlayGate>()
    .add_systems(Startup, play_gate::spawn_screen)
    .add_systems(Startup, combat_hud::spawn_reward)
    .add_systems(
        Update,
        (play_gate::check_render, play_gate::update)
            .chain()
            .run_if(play_gate::loading),
    )
    .insert_resource(rope::RopeCache(cache.clone()))
    .add_systems(
        Startup,
        crate::brain_overlay::spawn.run_if({
            let present = options.enemy_present;
            move || present
        }),
    )
    .insert_resource(FogWalls(fog_walls))
    .add_systems(FixedUpdate, simulate.run_if(crate::menu::active))
    .add_systems(
        FixedUpdate,
        hold_fog_walls.after(simulate).run_if(crate::menu::active),
    )
    .add_systems(
        Update,
        (
            // The camera turns first, so the stick read this frame is relative to the camera
            // that is drawn.
            orbit_camera.run_if(crate::menu::active),
            gather_input.run_if(crate::menu::active),
            brain_report,
            recording_brain,
            reload_enemy,
            bind_rigs,
            apply_draw_masks,
            // The map's standing probe reads Wolf's drawn feet of this frame.
            sync_actors.before(map::world::update),
            sync_arena_scene,
            sync_bullets,
            sync_lightning_tint,
            (rope::pose, animate, rope::draw, sheath_sword).chain(),
            follow_camera,
            crate::brain_overlay::update,
            hud,
            grapple_marker,
            (
                combat_hud::update,
                combat_hud::update_status,
                combat_hud::update_perilous,
                combat_hud::update_reward,
            )
                .chain(),
            log_camera,
        )
            .chain(),
    );
    crate::menu::install(&mut app);
    // First of the screens, so its extra menu atlases load with theirs in one read.
    game_menu::install(&mut app, options.attack_power.value(), item_data);
    crate::death_screen::install(&mut app);
    idol::install(&mut app, idols);
    pickup::install(&mut app, has_pickups);
    if options.enemy_present {
        arena_d_scene::install(&mut app, &cache, options.arena, options.objects)?;
        arena_c_scene::install(&mut app, &cache, options.arena)?;
    }
    crate::perf_log::register!(app);
    #[cfg(feature = "profile-spans")]
    app.add_systems(Last, crate::span_profile::report);
    if let Some(check) = live_check::LiveCheck::from_args(
        &std::env::args().collect::<Vec<_>>(),
        on_map.as_ref().map(|(dir, _, _)| dir.as_path()),
    )? {
        app.insert_resource(check).add_systems(
            Last,
            (live_check::observe_world, live_check::observe)
                .chain()
                .before(exit_fast),
        );
    }
    if std::env::var_os("PF_RESPAWN_PROBE").is_some() {
        app.add_systems(Last, renderer_counts);
    }
    if let Some(replay) = replay {
        app.insert_resource(replay).add_systems(
            PreUpdate,
            crate::input_replay::inject.before(bevy::input::InputSystems),
        );
    }
    if let Some((dir, id, world)) = on_map {
        let arena_hidden = options
            .arena
            .map(|a| sekiro_sim::arenas_a::hidden_parts(&cache, a))
            .transpose()
            .map_err(anyhow::Error::msg)?
            .unwrap_or_default();
        // The map's sun, ambient light and sky replace the floor setup's.
        if options.arena.is_some() {
            app.insert_resource(map::world::ArenaMap);
        }
        map::add_map(
            &mut app,
            &dir,
            MapScene {
                prefix: format!("maps/{id}/"),
                collision: false,
                no_pieces: false,
                hide: options
                    .hide
                    .iter()
                    .cloned()
                    .chain(arena_hidden)
                    .chain(
                        options
                            .arena
                            .into_iter()
                            .flat_map(|a| a.hidden_models.iter().map(|m| m.to_string())),
                    )
                    .collect(),
                objects: options.objects || (options.arena.is_none() && map::world::enabled(&id)),
                groups: map::GroupMode::Draw,
                markers: false,
                hour: options.hour,
                gparam: options.gparam.clone(),
                within,
                focus: Some(start_position),
            },
            stories,
        )?;
        app.insert_resource(MapCollision(world));
        if options.arena.is_some() && options.enemy_present {
            crate::arena_b::install(&mut app, &cache, &id)?;
        }
    }
    if options.map_enemies {
        map_enemies::install(&mut app, &cache)?;
    }
    map::ready::install(&mut app);
    app.add_plugins(crate::presentation::PresentationPlugin)
        .add_systems(Last, (capture, exit_fast).chain());
    app.run();
    Ok(())
}

/// Ends a play session as soon as anything asks the app to exit (closing the window, the pause
/// menu, `--exit`, the live check). The simulation (brain memory and session reports) and the
/// perf log are dropped so they save, a pause-menu relaunch starts its process, and the
/// process exits without tearing down the rest of the app: Bevy dropped hundreds of thousands
/// of entities and gigabytes of map assets one by one, which kept a closed window "Not
/// Responding" for about 27 seconds; the operating system reclaims that memory at once.
/// Screenshots still being captured get up to two seconds first.
fn exit_fast(world: &mut World, mut deferred: Local<Option<(AppExit, std::time::Instant)>>) {
    // Any error exit wins over success, in one pass.
    let requested = world.get_resource::<Messages<AppExit>>().and_then(|m| {
        m.iter_current_update_messages()
            .fold(None, |chosen: Option<AppExit>, exit| match chosen {
                Some(error) if error.is_error() => Some(error),
                _ => Some(exit.clone()),
            })
    });
    if let Some(exit) = requested {
        deferred.get_or_insert((exit, std::time::Instant::now()));
    }
    let Some((exit, asked)) = deferred.clone() else {
        return;
    };
    // Screenshots still being captured or written get up to two seconds; the exit request is
    // held back meanwhile so Bevy's runner does not start its slow teardown.
    let capturing = world
        .query_filtered::<(), With<Screenshot>>()
        .iter(world)
        .next()
        .is_some()
        || world
            .get_resource::<Captures>()
            .is_some_and(|c| c.in_flight.iter().any(|p| !p.is_file()));
    if capturing && asked.elapsed().as_secs_f32() < 2.0 {
        world.resource_mut::<Messages<AppExit>>().clear();
        return;
    }
    let started = std::time::Instant::now();
    world.remove_non_send::<Sim>();
    world.remove_resource::<crate::perf_log::PerfLog>();
    eprintln!(
        "shutdown: session saved in {:.2} s, exiting {:.2} s after launch",
        started.elapsed().as_secs_f32(),
        crate::since_start()
    );
    if let Err(error) = crate::menu::relaunch() {
        eprintln!("relaunch: {error:#}");
    }
    std::process::exit(match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    })
}

/// Adapt the two-slot fight simulation to an absent opponent, without defeating a map boss.
/// A vacant slot cannot collide, be aimed at or trigger an enemy-death restart.
fn vacant_enemy_slot(duel: &mut Duel) {
    duel.enemy_present = false;
    duel.enemy_active = false;
    duel.force_act = None;
    duel.events = None;
    duel.player.lock_target = None;
    duel.player.aim_candidates.clear();
    duel.enemy.tae_runtime.frame = Default::default();
    if !duel.rules.fighters[WOLF].dead() {
        duel.respawn_in = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn reload_enemy(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut scene: ResMut<EnemyScene>,
    mut libraries: ResMut<Libraries>,
    model_path: Res<ModelPath>,
    sim: NonSend<Sim>,
    actors: Query<(Entity, &Actor)>,
) {
    if !sim.enemy_present || scene.spec == sim.duel.spec {
        return;
    }
    let Ok(roster) = sekiro_sim::registry::Registry::load(&scene.cache) else {
        return;
    };
    let donor = roster.anim_chr(&sim.duel.spec.chr, sim.duel.spec.npc_param);
    let Ok(mut lib) = AnimLibrary::load(&scene.cache, &donor) else {
        return;
    };
    let Ok(clip) = lib.clip("a000_000000") else {
        return;
    };
    let mut idle = Animator::new(clip, false);
    idle.apply_root_motion = false;
    idle.speed = 0.0;
    for (entity, actor) in &actors {
        if actor.0 == ENEMY {
            commands.entity(entity).despawn();
        }
    }
    let mask = roster
        .draw_mask(&scene.cache, &sim.duel.spec)
        .map_or_else(|| DrawMask::new(MaskSource::Combat), DrawMask::with_groups);
    commands.spawn((
        Actor(ENEMY),
        WorldAssetRoot(
            assets.load(
                GltfAssetLabel::Scene(0)
                    .from_asset(format!("{}{}.glb", model_path.0, sim.duel.spec.chr)),
            ),
        ),
        Transform::IDENTITY,
        Visibility::Inherited,
        Rig::new(Arc::new(lib.skeleton.clone())),
        idle,
        mask,
    ));
    libraries.0[ENEMY] = lib;
    scene.spec = sim.duel.spec.clone();
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut libraries: ResMut<Libraries>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    model_path: Res<ModelPath>,
    enemy_draw_groups: Res<EnemyDrawGroups>,
    on_map: Option<Res<MapCollision>>,
    sim: NonSend<Sim>,
    rope_cache: Res<rope::RopeCache>,
    mut images: ResMut<Assets<Image>>,
    mut wire_materials: ResMut<Assets<grapple_hud::WireFill>>,
) {
    rope::spawn(&mut commands, &assets, &rope_cache.0, model_path.0);
    commands.spawn((
        FollowCamera,
        Camera3d::default(),
        bevy::core_pipeline::prepass::DepthPrepass,
        bevy::core_pipeline::prepass::MotionVectorPrepass,
        Projection::Perspective(PerspectiveProjection {
            fov: sim.duel.camera_director.fov.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, 2.0, 4.0).looking_at(Vec3::new(0.0, 1.2, 0.0), Vec3::Y),
    ));
    combat_hud::spawn(&mut commands);
    grapple_hud::spawn(
        &mut commands,
        &rope_cache.0,
        &mut images,
        &mut wire_materials,
    );
    // Bullets have no model yet (their look is an effect, FXR, not extracted): simple shapes.
    let fx = crate::bullet_fx::spawn(&mut commands, &mut meshes, &mut materials);
    commands.insert_resource(fx);
    let on_map = on_map.is_some();
    if !on_map {
        spawn_floor_stage(&mut commands, &mut meshes, &mut materials);
    }
    let models = [
        format!("{}c0000.glb", model_path.0),
        format!("{}{}.glb", model_path.0, sim.duel.spec.chr),
    ];
    for actor in [WOLF, ENEMY] {
        if actor == ENEMY && !sim.enemy_present {
            continue;
        }
        let lib = &mut libraries.0[actor];
        let mut idle = Animator::new(
            lib.clip("a000_000000").expect("idle clip is extracted"),
            false,
        );
        idle.apply_root_motion = false;
        idle.speed = 0.0;
        // Placed where the simulation has him from the first frame, so nothing (the map's
        // standing probe) ever sees him at the origin.
        let (position, yaw) = shown(&sim, actor, 0.0);
        let mut e = commands.spawn((
            Actor(actor),
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(models[actor].clone()))),
            Transform::from_translation(position).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::Inherited,
            Rig::new(Arc::new(lib.skeleton.clone())),
            idle,
        ));
        if actor == ENEMY {
            let groups = enemy_draw_groups.0;
            e.insert(
                groups.map_or_else(|| DrawMask::new(MaskSource::Combat), DrawMask::with_groups),
            );
        }
        if actor == WOLF && on_map {
            // The map follows the collision part Wolf stands on, never the camera.
            e.insert((
                MapFocus,
                PlayerFocus {
                    grounded: sim.duel.player.body.grounded,
                },
            ));
        }
    }
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(15.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
}

/// Fails when no collision floor lies under `position`, as `floor(x, z, top)` finds it from
/// 1 m above. The play gate waits for that floor before play starts, and neither the collision
/// world nor Wolf changes while loading, so a start over a hole (a `--spawn` whose ground moved,
/// #231) would hold the loading screen forever.
fn require_floor(position: [f32; 3], floor: impl Fn(f32, f32, f32) -> Option<f32>) -> Result<()> {
    let [x, y, z] = position;
    if floor(x, z, y + 1.0).is_none() {
        anyhow::bail!(
            "no collision floor under ({x:.2}, {y:.2}, {z:.2}); play would never start (pick a --spawn over the map's ground)"
        );
    }
    Ok(())
}

/// Puts Wolf at `start` facing `yaw` and the enemy `distance` metres in front of him, facing
/// him, both on the floor `floor_at(x, z, near_y)` finds.
fn place_pair(
    duel: &mut Duel,
    start: Vec3,
    yaw: f32,
    distance: f32,
    floor_at: impl Fn(f32, f32, f32) -> f32,
) {
    let wolf = &mut duel.player.body;
    wolf.position = [start.x, floor_at(start.x, start.z, start.y), start.z];
    wolf.yaw = yaw;
    wolf.fall_top = wolf.position[1];
    let ahead = Quat::from_rotation_y(yaw) * Vec3::NEG_Z * distance;
    let (ex, ez) = (start.x + ahead.x, start.z + ahead.z);
    let enemy = &mut duel.enemy.body;
    enemy.position = [ex, floor_at(ex, ez, start.y), ez];
    enemy.yaw = yaw + std::f32::consts::PI;
    enemy.fall_top = enemy.position[1];
}

/// Two lights and a checkered floor, so movement and speed are visible off the map.
fn spawn_floor_stage(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            color: Color::srgb(1.0, 0.96, 0.90),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 2_500.0,
            color: Color::srgb(0.85, 0.9, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 2.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // A checkered floor so movement and speed are visible.
    let tile = meshes.add(Plane3d::default().mesh().size(2.0, 2.0));
    let dark = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.19, 0.17),
        perceptual_roughness: 0.95,
        ..default()
    });
    let light = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.28, 0.25),
        perceptual_roughness: 0.95,
        ..default()
    });
    for i in -30..30 {
        for j in -30..30 {
            let mat = if (i + j) % 2 == 0 { &dark } else { &light };
            commands.spawn((
                Mesh3d(tile.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(i as f32 * 2.0 + 1.0, 0.0, j as f32 * 2.0 + 1.0),
            ));
        }
    }
}

#[derive(Default)]
struct MouseCapture {
    focus_requested: bool,
    first_click_grabbed: bool,
    grab_click: bool,
}

/// Sekiro's PC defaults (DefaultKeyAssignParam00, as read by FwLo's RustSekiro) for the play
/// buttons that are not plain movement; the HUD names them with [`key_label`].
const CROUCH_KEY: KeyCode = KeyCode::KeyQ;
const ACTION_KEY: KeyCode = KeyCode::KeyE;
const LOCK_ON_BUTTON: MouseButton = MouseButton::Middle;
/// The keyboard keys of the item cluster's buttons (quick item, prosthetic tool), and the labels
/// the HUD's item boxes show for them.
const ITEM_KEY: KeyCode = KeyCode::KeyR;
const PROSTHETIC_KEY: KeyCode = KeyCode::ControlLeft;
pub(crate) const ITEM_KEY_LABELS: [&str; 2] = [key_label(ITEM_KEY), key_label(PROSTHETIC_KEY)];

/// What the HUD calls a bound key.
const fn key_label(key: KeyCode) -> &'static str {
    match key {
        KeyCode::KeyQ => "Q",
        KeyCode::KeyE => "E",
        KeyCode::KeyR => "R",
        KeyCode::ControlLeft => "Ctrl",
        _ => "?",
    }
}

/// What the HUD calls a bound mouse button.
const fn button_label(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "LMB",
        MouseButton::Right => "RMB",
        MouseButton::Middle => "MMB",
        _ => "?",
    }
}

/// The keyboard and mouse part of a frame's input: the move stick and the buttons, from what
/// is held.
fn keyboard_input(
    key: impl Fn(KeyCode) -> bool,
    button: impl Fn(MouseButton) -> bool,
) -> (Vec2, Buttons) {
    let stick = Vec2::new(
        (key(KeyCode::KeyD) as i32 - key(KeyCode::KeyA) as i32) as f32,
        (key(KeyCode::KeyW) as i32 - key(KeyCode::KeyS) as i32) as f32,
    );
    let b = Buttons {
        attack: button(MouseButton::Left),
        guard: button(MouseButton::Right),
        jump: key(KeyCode::Space),
        dodge: key(KeyCode::ShiftLeft) || key(KeyCode::ShiftRight),
        crouch: key(CROUCH_KEY),
        grapple: key(KeyCode::KeyF),
        action: key(ACTION_KEY),
        prosthetic: key(PROSTHETIC_KEY),
        item: key(ITEM_KEY),
        combat_art: false,
        lock_on: button(LOCK_ON_BUTTON),
    };
    (stick, b)
}

#[allow(clippy::too_many_arguments)]
fn gather_input(
    fade: Res<cinematic_fade::Fade>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    held: Res<crate::menu::HeldThroughMenu>,
    modal: Option<Res<crate::menu::Modal>>,
    gamepads: Query<&Gamepad>,
    orbit: Res<Orbit>,
    mut live: ResMut<LiveInput>,
    mut windows: Query<(&mut Window, &mut CursorOptions)>,
    mut capture: Local<MouseCapture>,
) {
    if fade.alpha > 0.0 {
        live.frame = InputFrame::default();
        return;
    }
    if !capture.focus_requested
        && let Ok((mut window, mut c)) = windows.single_mut()
    {
        window.focused = true;
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
        capture.focus_requested = true;
    }
    // A click while the cursor is free (after Esc) takes the mouse back for the camera; that
    // click is not an attack. Not while a screen over the game (the idol menu) has the mouse.
    if !mouse.pressed(MouseButton::Left) {
        capture.grab_click = false;
    }
    if mouse.just_pressed(MouseButton::Left)
        && !modal.is_some_and(|m| m.0)
        && let Ok((window, mut c)) = windows.single_mut()
        && window.focused
        && window.cursor_position().is_some()
        && (!capture.first_click_grabbed || c.grab_mode != CursorGrabMode::Locked)
    {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
        capture.first_click_grabbed = true;
        capture.grab_click = true;
    }
    // Keys still held from a menu that just closed wait for their release.
    let (mut stick, mut b) = keyboard_input(|k| held.key(&keys, k), |m| held.button(&mouse, m));
    b.attack &= !capture.grab_click;
    for pad in &gamepads {
        let s = pad.left_stick();
        if s.length() > 0.2 {
            stick = s;
        }
        b.attack |= pad.pressed(GamepadButton::RightTrigger);
        b.guard |= pad.pressed(GamepadButton::LeftTrigger);
        b.jump |= pad.pressed(GamepadButton::South);
        b.dodge |= pad.pressed(GamepadButton::East);
        b.crouch |= pad.pressed(GamepadButton::LeftThumb);
        b.grapple |= pad.pressed(GamepadButton::LeftTrigger2);
        b.action |= pad.pressed(GamepadButton::North);
        b.prosthetic |= pad.pressed(GamepadButton::RightTrigger2);
        b.item |= pad.pressed(GamepadButton::West);
        b.lock_on |= pad.pressed(GamepadButton::RightThumb);
    }
    if stick.length() > 1.0 {
        stick = stick.normalize();
    }
    live.frame = InputFrame {
        move_stick: [stick.x, stick.y],
        camera_yaw: orbit.yaw,
        camera_pitch: orbit.pitch,
        buttons: b,
    };
}

fn orbit_camera(
    fade: Res<cinematic_fade::Fade>,
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Query<&CursorOptions>,
    gamepads: Query<&Gamepad>,
    sim: NonSend<Sim>,
    mut orbit: ResMut<Orbit>,
) {
    if fade.alpha > 0.0 {
        return;
    }
    // Locked on, the camera swings behind Wolf toward the target. The rates are the camera
    // slot's (TAE 155: CameraParam 4000 in deathblows).
    let director = &sim.duel.camera_director;
    let params = &sekiro_sim::camera::CameraParams {
        // The lock-on pitch frames the target from where the camera is (TAE 153 brings it close
        // in deathblows), as FwLo's port of the exe's formula does.
        cam_dist: director.distance,
        ..director.params()
    };
    let dt = time.delta_secs();
    clamp_to_look_at(&sim, &mut orbit, dt);
    if let Some(t) = sim.duel.player.lock_target {
        let target = Vec3::from_array(t) + Vec3::Y * params.chr_org_offset_y;
        let want = sekiro_sim::camera::lock_target_yaw(orbit.focus, target);
        let d = sekiro_sim::input::angle_diff(want, orbit.yaw);
        orbit.yaw += d * sekiro_sim::camera::chase(params.lock_rot_chase_rate_y, dt);
        let pitch = params.lock_target_pitch(orbit.focus, target);
        orbit.pitch +=
            (pitch - orbit.pitch) * sekiro_sim::camera::chase(params.lock_rot_chase_rate_x, dt);
        let (min, max) = params.pitch_limits(true, target.y - orbit.focus.y);
        orbit.pitch = orbit.pitch.clamp(min, max);
        return;
    }
    let locked = cursor
        .single()
        .is_ok_and(|c| c.grab_mode == CursorGrabMode::Locked);
    // Mouse motion while the map loads (the window appearing, the cursor being captured) would
    // spin the camera away from the start view before the fight begins.
    if locked && sim.step > 0 {
        orbit.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
        orbit.pitch += motion.delta.y * MOUSE_SENSITIVITY;
    }
    for pad in &gamepads {
        let r = pad.right_stick();
        if r.length() > 0.2 {
            orbit.yaw -= r.x * params.rot_speed_max_y * dt;
            orbit.pitch -= r.y * params.rot_speed_max_x * dt;
        }
    }
    let (min, max) = params.pitch_limits(false, 0.0);
    orbit.pitch = orbit.pitch.clamp(min, max);
}

#[allow(clippy::too_many_arguments)]
fn simulate(
    mut sim: NonSendMut<Sim>,
    mut fade: ResMut<cinematic_fade::Fade>,
    live: Res<LiveInput>,
    mut orbit: ResMut<Orbit>,
    gate: Res<play_gate::PlayGate>,
    mut sounds: ResMut<crate::audio::SoundLibrary>,
    mut cues: MessageWriter<crate::audio::SoundCue>,
    replay: Option<ResMut<crate::input_replay::InputReplay>>,
) {
    // Hold the simulation behind the loading screen until the whole map has loaded, is drawable
    // and frames have settled (play_gate.rs; its stable-frame wait also covers the one-off
    // presentation stall after the map's upload). Scripted and replayed runs wait the same
    // way, so their frames line up run to run.
    if !gate.open() {
        return;
    }
    if sim
        .duel
        .cinematic
        .as_ref()
        .is_some_and(|c| !c.applied && c.elapsed >= sekiro_sim::events::Cinematic::FADE)
        && fade.black_frames < 2
    {
        return;
    }
    if let Some(mut r) = replay {
        r.running = true;
    }
    let dt = 1.0 / STEP_HZ as f32;
    // Opt-in renderer lifecycle probe: force Tomoe's scene identity, then use Duel::reset.
    if std::env::var_os("PF_RESPAWN_PROBE").is_some() {
        let cycle = sim.step % 60;
        if cycle == 15
            && sim.duel.spec.chr == "c7100"
            && (sim.step < 60 || std::env::var("PF_RESPAWN_PROBE").is_ok_and(|v| v == "tomoe-each"))
        {
            let cache = sekiro_sim::cache::root();
            let tomoe = sekiro_sim::enemy::EnemySpec::from_cache(&cache, "c7110:71100000")
                .expect("probe Tomoe");
            sim.duel.spec = tomoe;
        } else if cycle == 30 {
            sim.duel.reset().expect("probe restart");
        }
    }
    let script_ended = sim.script.as_ref().is_some_and(|s| sim.step >= s.len());
    let feet = sim.duel.player.body.position;
    let route = script_ended
        .then(|| sim.route.steer(Vec2::new(feet[0], feet[2])))
        .flatten();
    // The step's camera is the orbit's yaw and pitch (the last frame's, as on screen): the hook
    // search builds its camera from them and the simulation's own body and camera director, so
    // it does not depend on frame timing.
    let mut frame = match (&sim.script, route) {
        // The route's stick direction is already in world space.
        (_, Some(f)) => InputFrame {
            camera_pitch: orbit.pitch,
            ..f
        },
        // Scripted sticks are camera-relative to the current orbit, like a player's, its yaw and
        // pitch columns turning that camera (`InputFrame::from_camera`).
        (Some(frames), None) => frames
            .get(sim.step)
            .copied()
            .unwrap_or_default()
            .from_camera(orbit.yaw, orbit.pitch),
        // The stick is relative to the camera on screen now (the orbit of the last frame).
        (None, None) => InputFrame {
            camera_yaw: orbit.yaw,
            camera_pitch: orbit.pitch,
            ..live.frame
        },
    };
    if sim.lock_requested && sim.step == 0 {
        frame.buttons.lock_on = true;
    }
    sim.previous = [
        (sim.duel.player.body.position, sim.duel.player.visual_yaw()),
        (sim.duel.enemy.body.position, sim.duel.enemy.visual_yaw()),
    ];
    if sim.break_posture && sim.duel.deathblow.is_none() && sim.duel.cinematic.is_none() {
        let posture = &mut sim.duel.rules.fighters[1].posture;
        posture.remaining = posture.remaining.min(1);
    }
    let before = sim.duel.player.body.position;
    if sim.passive {
        sim.duel.enemy_active = false;
    }
    let cinematic_applied = sim.duel.cinematic.as_ref().is_some_and(|c| c.applied);
    let mut report = sim.duel.step(&frame, dt);
    if std::env::var_os("PF_RESPAWN_PROBE").is_some() && sim.step % 60 == 30 {
        report.respawned = true;
    }
    fade.alpha = sim.duel.cinematic.as_ref().map_or(0.0, |c| c.alpha());
    if !cinematic_applied && sim.duel.cinematic.as_ref().is_some_and(|c| c.applied) {
        orbit.focus = Vec3::from_array(sim.duel.player.body.position)
            + Vec3::Y * sim.duel.camera_director.focus_height;
        orbit.yaw = sim.duel.player.body.yaw;
        orbit.pitch = 0.25;
        orbit.look = Default::default();
        sim.previous = [
            (sim.duel.player.body.position, sim.duel.player.visual_yaw()),
            (sim.duel.enemy.body.position, sim.duel.enemy.visual_yaw()),
        ];
    }
    if std::env::var_os("PLAY_LOG_COMBO").is_some()
        && sim.step.is_multiple_of(15)
        && let Some(c) = &sim.duel.cinematic
    {
        eprintln!(
            "cinematic step={} elapsed={:.3} alpha={:.3} applied={} boss={}",
            sim.step,
            c.elapsed,
            c.alpha(),
            c.applied,
            sim.duel.spec.chr
        );
    }
    if std::env::var_os("PLAY_LOG_WIRE").is_some() {
        let wolf = &sim.duel.player;
        let target = wolf.wire.active.as_ref().or(wolf.wire.candidate.as_ref());
        eprintln!(
            "WIRE step={} env3000={} pos={:?} target={:?} arrival={:?} distance={:?} anim={:?} events={:?} precompute={:?} flight={:?} grounded={}",
            sim.step,
            wolf.behavior.env.wire_candidate,
            wolf.body.position,
            target.map(|t| &wolf.wire.points[t.index].source_part),
            target.map(|t| t.arrival.to_array()),
            target.map(|t| Vec3::from_array(wolf.body.position).distance(t.arrival)),
            report.player.animation,
            report.player.behavior.events,
            wolf.behavior.env.wire_precompute.get(),
            wolf.wire.flight_time(),
            wolf.body.grounded,
        );
        if report.player.clip_started
            && let Some(anim) = report
                .player
                .animation
                .as_ref()
                .and_then(|a| wolf.tae.anim(a))
        {
            for e in anim.events.iter() {
                if matches!(e.kind, sekiro_sim::tae::TaeKind::Wire { .. }) {
                    eprintln!("WIRE TAE {e:?}");
                }
            }
        }
    }
    if std::env::var_os("PLAY_LOG_DOCKING").is_some() {
        let wolf = &sim.duel.player;
        let target = wolf
            .docking
            .active
            .as_ref()
            .or(wolf.docking.candidate.as_ref());
        eprintln!(
            "DOCK step={} env3044={} env3045={} env3020={} env3050={} env3043={} end={} variation={} standby={:?} requested={:?} state={:?} blocked={:?} pos={:?} edge={:?} point={:?} anim={:?} events={:?}",
            sim.step,
            wolf.behavior.env.hang_candidate && wolf.behavior.env.docking_searches.get() & 2 != 0,
            wolf.behavior.env.air_hang_type,
            wolf.behavior.env.wall_contact,
            wolf.behavior.env.hang_can_climb,
            wolf.behavior.env.cover_candidate && wolf.behavior.env.docking_searches.get() & 1 != 0,
            wolf.behavior.env.docking_end,
            wolf.behavior.env.docking_variation,
            wolf.behavior.env.standby,
            wolf.behavior.env.requested,
            report.player.behavior.state_path.last(),
            wolf.behavior.env.docking_blocked,
            wolf.body.position,
            target.map(|t| (
                &wolf.docking.edges[t.edge].source_part,
                wolf.docking.edges[t.edge].chain
            )),
            target.map(|t| t.point.to_array()),
            report.player.animation,
            report.player.behavior.events
        );
    }
    if report.respawned {
        // A respawn teleports the camera with Wolf instead of chasing up from the ravine.
        orbit.focus = Vec3::from_array(sim.duel.player.body.position)
            + Vec3::Y * sim.duel.camera_director.focus_height;
        orbit.yaw = sim.duel.player.body.yaw;
        orbit.pitch = 0.25;
        orbit.look = Default::default();
    }
    sim.clock[0] += report.player.advanced;
    sim.clock[1] += report.enemy.advanced;
    if log_enabled() {
        log_move(sim.step, &frame, before, &sim.duel, &report.player);
    }
    if std::env::var_os("PLAY_LOG_COMBO").is_some() {
        log_combo(sim.step, &frame, &sim.duel, &report);
    }
    if let Some(population) = &sim.duel.population {
        crate::audio::emit_population(&sim.duel, population, &mut sounds, &mut cues);
    } else {
        crate::audio::emit(&sim.duel, &report, &mut sounds, &mut cues);
    }
    for (i, r) in [&report.player, &report.enemy].into_iter().enumerate() {
        if !r.behavior.script_errors.is_empty() {
            warn!("script errors ({i}): {:?}", r.behavior.script_errors);
        }
        if (r.clip_started || report.respawned)
            && let Some(name) = &r.animation
        {
            // Teleports start a fresh pose, without blending from the falling death clip.
            let blend = if report.respawned { 0.0 } else { r.blend_in };
            sim.started[i] = Some((name.clone(), blend));
        }
    }
    let step = sim.step;
    for (hit, res) in &report.hits {
        let text = format!(
            "step {step}: {} hit {} ({}), AtkParam {}: HP -{}, posture -{} / attacker -{}",
            if hit.attacker == 0 {
                "Wolf".into()
            } else {
                format!("enemy {}", hit.attacker)
            },
            if hit.defender == 0 {
                "Wolf".into()
            } else {
                format!("enemy {}", hit.defender)
            },
            if res.deflected {
                "deflected"
            } else if res.guarded {
                "blocked"
            } else {
                "hit"
            },
            hit.attack.atk_id,
            res.hp_damage,
            res.posture_damage,
            res.attacker_posture_damage
        );
        info!("{text}");
        sim.hit_log.push(text);
        if sim.hit_log.len() > 4 {
            sim.hit_log.remove(0);
        }
    }
    if sim.script.is_some()
        && (step.is_multiple_of(30) || std::env::var_os("PLAY_LOG_ALL").is_some())
    {
        info!(
            "step {step} wolf {} {} at {:.2?}{} | enemy {} {} | fall_cm {:.1} vy_cm_s {:.1} respawn {} speed_m_s {:.3} anim_t {:.6} root_blend {:.6}",
            report.player.behavior.state_path.join("/"),
            report.player.animation.as_deref().unwrap_or("-"),
            report.player.position,
            if report.player.grounded { "" } else { " AIR" },
            report.enemy.behavior.state_path.join("/"),
            report.enemy.animation.as_deref().unwrap_or("-"),
            sim.duel.player.behavior.env.fall_height,
            sim.duel.player.behavior.env.vertical_velocity,
            report.respawned,
            (report.player.position[0] - before[0]).hypot(report.player.position[2] - before[2])
                / dt,
            report.player.anim_time,
            report.player.root_blend_weight,
        );
    }
    sim.perilous_age = match (&report.perilous, sim.perilous_age) {
        (Some(w), _) => {
            info!("perilous warning: judge {} {:?}", w.judge, w.kind);
            Some(0.0)
        }
        (None, age) => age.map(|a| a + dt),
    };
    if report.jump_counter {
        info!("jump counter: the kick landed on a perilous sweep");
    }
    sim.last = report;
    sim.step += 1;
}

fn sync_actors(
    mut sim: NonSendMut<Sim>,
    fixed: Res<Time<Fixed>>,
    mut libraries: ResMut<Libraries>,
    mut fx: ResMut<crate::bullet_fx::BulletFx>,
    mut fxr: ResMut<crate::fxr::FxrPlayer>,
    mut q: Query<(
        &Actor,
        &mut Animator,
        &mut Transform,
        &mut Visibility,
        Option<&mut PlayerFocus>,
    )>,
) {
    for (actor, mut anim, mut tf, mut vis, focus) in &mut q {
        let i = actor.0;
        if i == ENEMY && sim.duel.screen_monkeys.is_some() {
            *vis = Visibility::Hidden;
            continue;
        }
        let (position, yaw) = shown(&sim, i, fixed.overstep_fraction());
        tf.translation = position;
        tf.rotation = Quat::from_rotation_y(yaw);
        if let Some(mut focus) = focus {
            // Only a ground contact can change the map's standing part.
            let grounded = sim.duel.player.body.grounded;
            if focus.grounded != grounded {
                focus.grounded = grounded;
            }
        }
        // TAE 193 fades the model (and the weapon in it) out and back in; drawn hidden past
        // half, with a puff where it goes and where it comes back (stand-ins for the game's
        // warp effects, FFX 650680 / 650682).
        let fade = if i == WOLF {
            sim.last.player.fade
        } else {
            sim.last.enemy.fade
        };
        let faded = fade >= 0.5;
        if faded != sim.faded[i] {
            sim.faded[i] = faded;
            // The game's own warp effects (FFX 650680 / 650682) play through the FXR player.
            if !fxr.has(650680) {
                fx.puff(position + Vec3::Y * 1.0);
            }
        }
        *vis = if faded {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        anim.walk_twist = if i == WOLF {
            sim.duel.player.walk_twist
        } else {
            sim.duel.enemy.walk_twist
        };
        let report: &StepReport = if i == WOLF {
            &sim.last.player
        } else {
            &sim.last.enemy
        };
        let Some(name) = report.animation.clone() else {
            continue;
        };
        if sim.step > 0 {
            // The fight runs: decode the rest of the clips in the background.
            libraries.0[i].prefetch_all();
        }
        let alpha = fixed.overstep_fraction();
        anim.clock = Some(sim.clock[i] - behind(report, alpha));
        let time = drawn_clip_time(report, alpha);
        let blend_in = report.blend_in;
        let started = sim.started[i].take();
        let restart = started.as_ref().is_some_and(|(n, _)| *n == name);
        if anim.clip_name() != name || restart {
            // A previously active layered clip can become the main clip without restarting.
            // Use its own TAE, including when several fixed ticks preceded this draw.
            let blend = started
                .filter(|(n, _)| *n == name)
                .map_or(blend_in, |(_, b)| b);
            match libraries.0[i].clip(&name) {
                Ok(clip) => {
                    anim.play_named(&name, clip, false, blend);
                    anim.apply_root_motion = false;
                    anim.speed = 0.0;
                }
                // Not extracted: keep showing the previous clip.
                Err(_) => continue,
            }
        }
        anim.time = time;
    }
}

/// How far a frame drawn `alpha` of the way into the next step trails the last step on the
/// actor's own clock: the part of the step's advance not yet reached. A frozen actor advanced
/// nothing, so it holds exactly still; while a hit stop eases out the advance is short, and so
/// is this.
fn behind(report: &StepReport, alpha: f32) -> f32 {
    (1.0 - alpha) * report.advanced
}

/// The full-body clip time drawn `alpha` of the way into the next step, matching the drawn
/// position. A clip that started in the last step is drawn from its start.
fn drawn_clip_time(report: &StepReport, alpha: f32) -> f32 {
    (report.anim_time - behind(report, alpha)).max(0.0)
}

/// Where actor `i` is drawn: between its position and yaw before and after the last step, by
/// how far the render time is into the next step, so motion is smooth at any frame rate
/// (drawing the last step alone stalls and jumps whenever a frame runs no step or two).
fn shown(sim: &Sim, i: usize, alpha: f32) -> (Vec3, f32) {
    if i == ENEMY
        && let Some(encounter) = &sim.duel.screen_monkeys
    {
        let target = &encounter.monkeys[encounter.selected].duel.enemy;
        return (Vec3::from_array(target.body.position), target.visual_yaw());
    }
    let chr = if i == WOLF {
        &sim.duel.player
    } else {
        &sim.duel.enemy
    };
    shown_transform(sim.previous[i], chr, alpha, sim.step == 0)
}

fn shown_transform(
    previous: ([f32; 3], f32),
    chr: &sekiro_sim::character::Character,
    alpha: f32,
    first: bool,
) -> (Vec3, f32) {
    let (now, yaw) = (Vec3::from_array(chr.body.position), chr.visual_yaw());
    let (before, yaw_before) = previous;
    let before = Vec3::from_array(before);
    if first || before.distance(now) > 2.0 {
        return (now, yaw);
    }
    (
        interpolate_position(before, now, alpha),
        yaw_before + sekiro_sim::input::angle_diff(yaw, yaw_before) * alpha,
    )
}

fn interpolate_position(before: Vec3, now: Vec3, alpha: f32) -> Vec3 {
    if before == now {
        now
    } else {
        before.lerp(now, alpha)
    }
}

/// Draws the bullets in flight and their areas (see [`crate::bullet_fx`]).
fn sync_lightning_tint(
    sim: NonSend<Sim>,
    roots: Query<&Actor>,
    parents: Query<&ChildOf>,
    parts: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut original: Local<std::collections::HashMap<AssetId<StandardMaterial>, (Color, LinearRgba)>>,
) {
    for (entity, material) in &parts {
        let mut current = entity;
        let mut actor = None;
        for _ in 0..32 {
            if let Ok(root) = roots.get(current) {
                actor = Some(root.0);
                break;
            }
            let Ok(parent) = parents.get(current) else {
                break;
            };
            current = parent.parent();
        }
        let Some(i) = actor else {
            continue;
        };
        let Some(mut mat) = materials.get_mut(&material.0) else {
            continue;
        };
        let saved = original
            .entry(material.0.id())
            .or_insert((mat.base_color, mat.emissive));
        let status = &sim.duel.rules.fighters[i].status;
        if status.has(9445) || status.has(9492) {
            mat.base_color = Color::srgb(0.25, 0.65, 1.0);
            mat.emissive = LinearRgba::rgb(0.15, 0.4, 0.9);
        } else {
            (mat.base_color, mat.emissive) = *saved;
        }
    }
}

/// Draws the bullets in flight and their areas (see [`crate::bullet_fx`]).
#[allow(clippy::too_many_arguments)]
fn sync_bullets(
    sim: NonSend<Sim>,
    time: Res<Time>,
    mut fx: ResMut<crate::bullet_fx::BulletFx>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&crate::bullet_fx::Marker, &mut Transform, &mut Visibility)>,
    mut gizmos: Gizmos,
    mut lightning: Local<crate::lightning_fx::LightningFx>,
    mut fxr: ResMut<crate::fxr::FxrPlayer>,
) {
    // Bullets whose own effect is installed draw through the FXR player; the stand-ins stay for
    // the rest (and for `--no-fxr`).
    let covered: Vec<u64> = sim
        .duel
        .bullets
        .live
        .iter()
        .filter(|b| fxr.covers_bullet(b.def.id, b.look()))
        .map(|b| b.serial)
        .collect();
    let mut shown: Vec<_> = sim
        .duel
        .bullets
        .live
        .iter()
        .filter(|b| !covered.contains(&b.serial))
        .cloned()
        .collect();
    if let Some(bird) = sim.duel.father_bird.as_ref().filter(|b| b.enabled) {
        shown.extend(bird.duel.bullets.live.iter().cloned());
    }
    if let Some(population) = &sim.duel.population {
        for (i, foe) in population
            .foes
            .iter()
            .enumerate()
            .filter(|(_, f)| f.active())
        {
            shown.extend(
                foe.duel
                    .bullets
                    .live
                    .iter()
                    .filter(|b| b.shooter == 1 || population.selected == Some(i))
                    .cloned()
                    .map(|mut b| {
                        b.serial ^= (i as u64 + 1) << 48;
                        b
                    }),
            );
        }
    }
    crate::lightning_fx::draw(
        &sim.duel,
        time.elapsed_secs(),
        time.delta_secs(),
        &mut lightning,
        &mut gizmos,
        &covered,
    );
    crate::bullet_fx::sync(&shown, time.delta_secs(), &mut fx, &mut materials, &mut q);
}

/// TAE 151 (deathblows): the camera's pitch and yaw stay within the event's limits around the
/// direction from Wolf to his camera dummy ([`sekiro_sim::camera::CameraDirector::clamp_look`]).
fn clamp_to_look_at(sim: &Sim, orbit: &mut Orbit, dt: f32) {
    let director = &sim.duel.camera_director;
    let Some(look) = director.look_at else {
        orbit.look = Default::default();
        return;
    };
    let Some(dummy) = i16::try_from(look.dummy)
        .ok()
        .and_then(|d| sim.duel.dummy_position(WOLF, d))
    else {
        return;
    };
    // From Wolf's body centre, as FwLo's RustSekiro measures it (his capsule centre, 0.9 m above
    // the feet); from the feet the dummy (about 1.1 m up and 0.4 m ahead in the Owl's deathblow)
    // would centre the limits far below and turn the camera up from under the pair.
    let centre = Vec3::from_array(sim.duel.player.body.position) + Vec3::Y * 0.9;
    let dir = Vec3::from_array(dummy) - centre;
    let rate = director.params().lock_cam_param_lerp_rate;
    let (yaw, pitch) = orbit
        .look
        .clamp(Some(look), rate, orbit.yaw, orbit.pitch, dir, dt);
    orbit.yaw = yaw;
    orbit.pitch = pitch;
}

fn follow_camera(
    sim: NonSend<Sim>,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    mut orbit: ResMut<Orbit>,
    collision: Option<Res<MapCollision>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<FollowCamera>>,
) {
    let Ok((mut tf, mut projection)) = cam.single_mut() else {
        return;
    };
    // The field of view follows the camera slot's LockCamParam row (`camFovY`, vertical).
    if let Some(followed) = follow_fov(&projection, sim.duel.camera_director.fov) {
        *projection = followed;
    }
    let wolf = shown(&sim, WOLF, fixed.overstep_fraction()).0;
    // Scripted inspection angle: `--extra hang-view`, parsed here to avoid main.rs.
    if std::env::args().any(|a| a == "hang-view")
        && let Some(target) = &sim.duel.player.docking.active
    {
        orbit.yaw = target.yaw() - 30.0f32.to_radians();
        orbit.pitch = 0.1;
    }
    let director = &sim.duel.camera_director;
    let params = director.params();
    let mut wanted = wolf + Vec3::Y * director.focus_height;
    let mut xz = sekiro_sim::camera::chase(params.chr_trans_chase_rate_xz, time.delta_secs());
    let mut y = sekiro_sim::camera::chase(params.chr_trans_chase_rate_y, time.delta_secs());
    // TAE 151 (Wolf's deathblow clips): the focus chases his camera dummy, at the event's rate
    // on every axis when it gives one (FwLo's trace, `docs/kb/camera.md`).
    if let Some(look) = director.look_at
        && let Some(dummy) = i16::try_from(look.dummy)
            .ok()
            .and_then(|d| sim.duel.dummy_position(WOLF, d))
    {
        wanted = Vec3::from_array(dummy);
        if look.rate > 0.0 {
            xz = sekiro_sim::camera::chase(look.rate.min(1.0), time.delta_secs());
            y = xz;
        }
    }
    // TAE 153: the distance the animation asks for.
    orbit.distance = director.distance;
    orbit.focus.x += (wanted.x - orbit.focus.x) * xz;
    orbit.focus.z += (wanted.z - orbit.focus.z) * xz;
    orbit.focus.y += (wanted.y - orbit.focus.y) * y;
    let target = orbit.focus;
    let rot = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
    let back = rot * Vec3::Z;
    // Pull the camera in front of walls between it and Wolf (as the hook search's camera is).
    let distance = collision.map_or(orbit.distance, |c| {
        sekiro_sim::camera::wall_pulled_distance(&c.0, target, back, orbit.distance)
    });
    let eye = target + back * distance;
    *tf = Transform::from_translation(eye).looking_at(target, Vec3::Y);
}

/// The perspective projection at a vertical field of view of `degrees`, or `None` when it already
/// has it, so the camera is only marked changed (and its frustum rebuilt) while the fov eases.
fn follow_fov(projection: &Projection, degrees: f32) -> Option<Projection> {
    let Projection::Perspective(p) = projection else {
        return None;
    };
    let fov = degrees.to_radians();
    ((p.fov - fov).abs() > 1e-6)
        .then(|| Projection::Perspective(PerspectiveProjection { fov, ..p.clone() }))
}

/// Names of the status effects the Owl's tools leave on Wolf, for the HUD.
const STATUS_NAMES: [(i32, &str); 5] = [
    (9445, "Electrified"),
    (9492, "Lightning Reversal ready"),
    (9050, "Poisoned"),
    (3506090, "Recovery sealed"),
    (113000, "Posture recovery slowed"),
];

/// Wolf's status line: each named effect with its seconds left, and blinding while it plays.
fn status_line(status: &sekiro_sim::status::Status, states: &[String]) -> String {
    let mut parts: Vec<String> = STATUS_NAMES
        .iter()
        .filter_map(|(id, name)| status.remaining(*id).map(|s| format!("{name} {s:.0} s")))
        .collect();
    if status.terror.value > 0.0 {
        parts.push(format!("Terror {:.0}%", status.terror.fraction() * 100.0));
    }
    if states.iter().any(|s| s.contains("Blinding")) {
        parts.push("Blinded".into());
    }
    parts.join("   ")
}

/// The enemy's head for its floating bars and deathblow mark: its skeleton's Head bone in the
/// drawn pose (`head`, model space), held over its drawn `root` so a swaying head does not shake
/// the bars (Fireguy32's RustSekiro fork, MIT, commit d11ef65). Without the bone or a pose yet,
/// just above the body capsule of `height` (NpcParam hitHeight: 1.8 m soldier, 2.5 m Owl).
fn enemy_head(root: Vec3, head: Option<&Mat4>, height: f32) -> Vec3 {
    match head {
        Some(m) => Vec3::new(root.x, root.y + m.w_axis.y, root.z),
        None => root + Vec3::Y * (height + 0.2),
    }
}

/// The Head bone's index in `rig`'s skeleton, looked up once per skeleton (`cache` keeps the
/// skeleton it was looked up for).
fn head_bone(rig: &Rig, cache: &mut Option<(usize, Option<usize>)>) -> Option<usize> {
    let key = Arc::as_ptr(&rig.skeleton) as usize;
    match *cache {
        Some((k, bone)) if k == key => bone,
        _ => {
            let bone = rig.skeleton.bone_index("Head");
            *cache = Some((key, bone));
            bone
        }
    }
}

#[allow(clippy::type_complexity)]
fn hud(
    sim: NonSend<Sim>,
    recording: Res<crate::brain_overlay::RecordingBrain>,
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugHud>,
    mut text: Query<&mut Text, With<Hud>>,
    mut gauges: ResMut<combat_hud::Gauges>,
    (live, actors, mut head_cache): (
        Option<Res<LiveInput>>,
        Query<(&Actor, &Transform, &Rig)>,
        Local<Option<(usize, Option<usize>)>>,
    ),
) {
    let f = &sim.duel.rules.fighters;
    let held = live.map_or([false; 2], |l| {
        [l.frame.buttons.item, l.frame.buttons.prosthetic]
    });
    // Hidden with him while a warp fades him out.
    let enemy_alive = sim.enemy_present
        && !f[1].dead()
        && sim.duel.enemy_bar_shown()
        && (!sim.faded[1] || sekiro_sim::duel::old_dragons::owns(&sim.duel.spec));
    let boss_bar = sim.enemy_present
        && sim
            .duel
            .events
            .as_ref()
            .is_some_and(|events| events.boss_bar.is_some());
    let deathblow = sim.enemy_present
        && sim.duel.deathblow_available()
        && !sim.duel.brown_deathblow_available();
    // Only the floating bars and the deathblow mark use the head.
    let enemy_head = (enemy_alive && (!boss_bar || deathblow)).then(|| {
        let height = sim.duel.fighters[ENEMY].height;
        match actors.iter().find(|(actor, ..)| actor.0 == ENEMY) {
            Some((_, tf, rig)) => {
                let head = head_bone(rig, &mut head_cache).and_then(|i| rig.model.get(i));
                enemy_head(tf.translation, head, height)
            }
            None => enemy_head(Vec3::from_array(sim.duel.enemy.body.position), None, height),
        }
    });
    *gauges = combat_hud::Gauges {
        hp: [0, 1].map(|i| f[i].hp_fraction()),
        wolf_max_hp: f[0].vitality.max_hp,
        posture: [0, 1].map(|i| f[i].posture_fraction()),
        broken: [0, 1].map(|i| f[i].posture.broken()),
        enemy_head,
        enemy_alive,
        boss_bar,
        deathblow,
        enemy_name: if sim.enemy_present {
            sim.duel.spec.display_name()
        } else {
            ""
        },
        enemy_marks: if sim.enemy_present {
            sim.duel.enemy_bar_marks()
        } else {
            0
        },
        poison: f[0].status.poison.fraction(),
        status: status_line(&f[0].status, &sim.last.player.behavior.state_path),
        respawn_in: sim.duel.respawn_in,
        // Lifted from the chest dummy to above Wolf's head (ours; the effect's own offset is in
        // its undecoded FXR).
        perilous_at: sim
            .duel
            .dummy_position(WOLF, sekiro_sim::perilous::KANJI_DUMMY)
            .map(|p| Vec3::from_array(p) + Vec3::Y * 0.6),
        perilous_age: sim.perilous_age,
        held,
        sen: sim.duel.kit.sen,
        skill_points: sim.duel.kit.skill_points,
        xp_fill: sim.duel.kit.xp_fill(),
        rewards: sim.duel.kit.rewards,
    };
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    if keys.just_pressed(KeyCode::F3) {
        debug.0 = !debug.0;
    }
    if recording.enabled && sim.enemy_present {
        text.0.clear();
        return;
    }
    let r = &sim.last;
    let mut flags = r.player.action_flags.clone();
    flags.retain(|f| [3, 7, 11, 26, 115, 117, 119].contains(f));
    let mut s = String::new();
    if debug.0 {
        s = format!(
            "Wolf  {}  [{} t={:.2}]  HP {}{}\nrefs {:?}  windows {:?}\n",
            r.player.behavior.state_path.join(" / "),
            r.player.animation.as_deref().unwrap_or("-"),
            r.player.anim_time,
            sim.duel.rules.fighters[0].vitality.hp,
            if r.locked_on { "  LOCKED ON" } else { "" },
            r.player.behavior_refs,
            flags,
        );
        if sim.enemy_present {
            s += &format!(
                "Enemy  {}  [{} t={:.2}]  HP {}\n",
                r.enemy.behavior.state_path.join(" / "),
                r.enemy.animation.as_deref().unwrap_or("-"),
                r.enemy.anim_time,
                f[1].vitality.hp
            );
        }
        for line in &sim.hit_log {
            s += line;
            s.push('\n');
        }
    }
    let action = key_label(ACTION_KEY);
    match sim.duel.player.behavior.env.action_guide.get() {
        Some((10, 12)) => s += &format!("{action} / Y: Hug wall\n"),
        Some((11, 12)) => s += &format!("{action} / Y: Leave wall\n"),
        Some((20, 12)) => s += &format!("{action} / Y: Hang\n"),
        Some((21, 12) | (22, 5)) => {
            if sim.duel.player.behavior.env.hang_can_climb {
                s += &format!("{action} / Y: Climb up   ");
            }
            s += "Shift / B: Drop\n";
        }
        _ => {}
    }
    if sim.duel.player.behavior.env.in_water {
        s += &format!(
            "{} / L3: Dive or descend   Space / A: Rise   Shift / B: Swim kick\n",
            key_label(CROUCH_KEY)
        );
    }
    if sim
        .duel
        .screen_monkeys
        .as_ref()
        .is_some_and(|encounter| encounter.bell.available)
    {
        s += &format!("{action} / Y: Ring bell\n");
    }
    s += &format!(
        "WASD move, mouse look, LMB attack, RMB guard, Space jump, Shift step/sprint, {} crouch, ",
        key_label(CROUCH_KEY)
    );
    if sim.enemy_present || sim.duel.population.is_some() {
        s += &format!("{} lock-on, ", button_label(LOCK_ON_BUTTON));
    }
    s += "Esc pause, F3 debug text, -/= volume";
    text.0 = s;
}

fn capture(
    mut commands: Commands,
    mut readiness: crate::map::ready::Ready,
    sim: NonSend<Sim>,
    mut cap: ResMut<Captures>,
    rigs: Query<&Rig>,
    mut exit: MessageWriter<AppExit>,
    replay: Option<Res<crate::input_replay::InputReplay>>,
) {
    if rigs.is_empty() || !rigs.iter().all(|r| r.bound) {
        return;
    }
    let step = sim.step;
    if cap.pending.iter().any(|(s, _)| *s <= step) && !readiness.allow() {
        return;
    }
    cap.in_flight.retain(|p| !p.is_file());
    // The window takes one screenshot per frame (Bevy skips a duplicate render target), so
    // shots that fall due together, such as after a readiness wait, go one after another.
    if cap.in_flight.is_empty()
        && let Some(index) = cap.pending.iter().position(|(s, _)| *s <= step)
    {
        let (_, path) = cap.pending.remove(index);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        cap.in_flight.push(path);
    }
    let script_done =
        sim.script.as_ref().is_some_and(|s| step >= s.len()) || replay.is_some_and(|r| r.done());
    if cap.exit_after_script && script_done && cap.pending.is_empty() && cap.in_flight.is_empty() {
        std::thread::sleep(Duration::from_millis(200));
        exit.write(AppExit::Success);
    }
}

/// `--script-route x,z;x,z;...`: once the `--script` input runs out, Wolf walks (0.3 stick)
/// to each point in turn, steered toward it every step with the stick's world direction
/// (not relative to the orbit camera), so a live run can follow a narrow path such as a tree
/// trunk; it needs a `--script` (whose idle lines wait for the load). A point counts as reached
/// within 0.25 m; after [`ScriptRoute::LIMIT`] steps without
/// finishing, the route is abandoned. The live check waits for it.
#[derive(Default)]
pub(crate) struct ScriptRoute {
    points: Vec<Vec2>,
    next: usize,
    steps: usize,
}

impl ScriptRoute {
    const LIMIT: usize = 3600;

    fn parse(args: &[String]) -> Result<Self> {
        let Some(value) = crate::arg_value(args, "--script-route") else {
            return Ok(Self::default());
        };
        // The route starts when the script ends, and the script's idle lines are what wait for
        // the load, so a route alone would never run.
        anyhow::ensure!(
            crate::arg_value(args, "--script").is_some(),
            "--script-route needs a --script to run after"
        );
        let points = value
            .split(';')
            .map(|p| {
                let (x, z) = p.split_once(',').context("--script-route points are x,z")?;
                Ok(Vec2::new(x.trim().parse()?, z.trim().parse()?))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            points,
            ..Self::default()
        })
    }

    /// Whether every point is reached (or the route was abandoned).
    pub(crate) fn done(&self) -> bool {
        self.next >= self.points.len()
    }

    fn steer(&mut self, feet: Vec2) -> Option<InputFrame> {
        while let Some(&target) = self.points.get(self.next) {
            self.steps += 1;
            if self.steps > Self::LIMIT {
                warn!(
                    "script route abandoned before point {} at {feet}",
                    self.next
                );
                self.next = self.points.len();
                return None;
            }
            let d = target - feet;
            if d.length() < 0.25 {
                info!(
                    "script route: reached point {} {target} at {feet}",
                    self.next
                );
                self.next += 1;
                continue;
            }
            return Some(InputFrame::toward_world_yaw((-d.x).atan2(-d.y), 0.3));
        }
        None
    }
}

/// Per-frame camera and per-step movement logs, on for `--input` replays or with
/// `PLAY_LOG_CAMERA` set.
fn log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("PLAY_LOG_CAMERA").is_some() || std::env::args().any(|a| a == "--input")
    })
}

/// One `move` line per simulation step: the camera yaw the step used, the stick and its world
/// direction, Wolf's facing and direction of travel, the travel error against the stick, and
/// his feet's height and whether he is grounded (a bounce on stairs shows in `y`, #104).
fn log_move(step: usize, frame: &InputFrame, before: [f32; 3], duel: &Duel, r: &StepReport) {
    let wolf = &duel.player;
    let (dx, dz) = (r.position[0] - before[0], r.position[2] - before[2]);
    let speed = (dx * dx + dz * dz).sqrt() * STEP_HZ as f32;
    let travel = (-dx).atan2(-dz);
    let stick = wolf.input.stick_world_yaw();
    let err = match stick {
        Some(s) if speed > 0.05 => sekiro_sim::input::angle_diff(travel, s).to_degrees(),
        _ => f32::NAN,
    };
    let var = |n: &str| wolf.behavior.runtime.variable(n).unwrap_or(f32::NAN);
    eprintln!(
        "move {step} cam {:.1} stick {:.2},{:.2} stick_yaw {:.1} face {:.1} travel {:.1} speed {:.2} err {:.1} MoveAngle {:.1} walk_twist {:.1} y {:.4} grounded {} lock {} {} {}",
        frame.camera_yaw.to_degrees(),
        frame.move_stick[0],
        frame.move_stick[1],
        stick.map_or(f32::NAN, f32::to_degrees),
        r.yaw.to_degrees(),
        travel.to_degrees(),
        speed,
        err,
        var("MoveAngle"),
        wolf.walk_twist,
        r.position[1],
        r.grounded,
        wolf.lock_target.is_some(),
        r.behavior.state_path.last().map_or("-", String::as_str),
        r.animation.as_deref().unwrap_or("-"),
    );
}

/// One `combo` line per simulation step with `PLAY_LOG_COMBO` set: the attack button, Wolf's
/// buffered presses, state, clip and clip time, both hit stops, this step's hits and the
/// enemy's state and clip.
fn log_combo(step: usize, frame: &InputFrame, duel: &Duel, r: &DuelReport) {
    let mut pending: Vec<i32> = duel.player.input.pending_actions().collect();
    pending.sort_unstable();
    let hits: Vec<String> = r
        .hits
        .iter()
        .map(|(h, res)| {
            let kind = if res.deflected {
                "deflect"
            } else if res.guarded {
                "guard"
            } else {
                "hit"
            };
            format!("{}>{}:{}:{kind}", h.attacker, h.defender, h.attack.atk_id)
        })
        .collect();
    let last = |s: &StepReport| s.behavior.state_path.last().cloned().unwrap_or_default();
    let (a, b) = (r.player.position, r.enemy.position);
    eprintln!(
        "combo {step} atk {} pend {pending:?} wolf {} {} {:.3} stop {:.3},{:.3} hits [{}] owl {} {} {:.3} dist {:.2}",
        frame.buttons.attack as u8,
        last(&r.player),
        r.player.animation.as_deref().unwrap_or("-"),
        r.player.anim_time,
        duel.hit_stop[0].remaining(),
        duel.hit_stop[1].remaining(),
        hits.join(" "),
        last(&r.enemy),
        r.enemy.animation.as_deref().unwrap_or("-"),
        r.enemy.anim_time,
        ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt(),
    );
}

/// One `cam` line per rendered frame: cursor grab, the frame's mouse motion, lock-on, the orbit
/// and the rendered camera's forward direction as a yaw.
#[allow(clippy::too_many_arguments)]
fn log_camera(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Query<&CursorOptions>,
    orbit: Res<Orbit>,
    live: Res<LiveInput>,
    sim: NonSend<Sim>,
    cam: Query<(&Transform, &Projection), With<FollowCamera>>,
    replay: Option<Res<crate::input_replay::InputReplay>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    fixed: Res<Time<Fixed>>,
) {
    if !log_enabled() {
        return;
    }
    let grab = cursor
        .single()
        .map_or("none".to_string(), |c| format!("{:?}", c.grab_mode));
    let focus = orbit.focus;
    let (fwd, fov, aspect, eye_dist) = match cam.single() {
        Ok((t, projection)) => {
            let (fov, aspect) = match projection {
                Projection::Perspective(p) => (p.fov.to_degrees(), p.aspect_ratio),
                _ => (f32::NAN, f32::NAN),
            };
            let eye_dist = t.translation.distance(focus);
            (t.forward().as_vec3(), fov, aspect, eye_dist)
        }
        Err(_) => (Vec3::NAN, f32::NAN, f32::NAN, f32::NAN),
    };
    let cam_yaw = (-fwd.x).atan2(-fwd.z);
    let wolf = Vec3::from_array(sim.duel.player.body.position);
    let drawn = shown(&sim, WOLF, fixed.overstep_fraction()).0;
    eprintln!(
        "cam {} t {:.3} dt {:.4} focused {} grab {grab} mouse {:.1},{:.1} lock {} orbit_yaw {:.1} pitch {:.1} cam_yaw {:.1} stick {:.2},{:.2} live_cam {:.1} step {} focus_lag {:.3} drawn {:.3},{:.3} sim {:.3},{:.3} fov {:.2} aspect {:.3} dist {:.2} boom {:.2} focus_h {:.2}",
        replay.as_ref().map_or(0, |r| r.frame),
        time.elapsed_secs(),
        time.delta_secs(),
        window.single().is_ok_and(|w| w.focused),
        motion.delta.x,
        motion.delta.y,
        sim.duel.player.lock_target.is_some(),
        orbit.yaw.to_degrees(),
        orbit.pitch.to_degrees(),
        cam_yaw.to_degrees(),
        live.frame.move_stick[0],
        live.frame.move_stick[1],
        live.frame.camera_yaw.to_degrees(),
        sim.step,
        Vec2::new(focus.x - wolf.x, focus.z - wolf.z).length(),
        drawn.x,
        drawn.z,
        wolf.x,
        wolf.z,
        fov,
        aspect,
        eye_dist,
        sim.duel.camera_director.distance,
        focus.y - wolf.y,
    );
}

fn brain_report(keys: Res<ButtonInput<KeyCode>>, sim: NonSend<Sim>) {
    if keys.just_pressed(KeyCode::F9)
        && let Some(driver) = sim.duel.boss_brain.as_ref()
        && let Err(e) = driver.print_session_report()
    {
        eprintln!("brain session report: {e}");
    }
}

fn recording_brain(sim: NonSend<Sim>, mut brain: ResMut<crate::brain_overlay::RecordingBrain>) {
    if let Some(driver) = sim.duel.boss_brain.as_ref() {
        brain.view = driver.overlay();
    }
}

/// TAE 720's right-hand sheath window frees both hands for docking.
fn sheath_sword(
    sim: NonSend<Sim>,
    actors: Query<(Entity, &Actor)>,
    children: Query<&Children>,
    names: Query<&Name>,
    globals: Query<&GlobalTransform>,
    mut attachments: Query<(&ChildOf, &mut Transform)>,
    mut original: Local<std::collections::HashMap<Entity, Transform>>,
) {
    let Some((root, _)) = actors.iter().find(|(_, a)| a.0 == WOLF) else {
        return;
    };
    let descendants: Vec<_> = children.iter_descendants(root).collect();
    let find = |name: &str| {
        descendants
            .iter()
            .copied()
            .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == name))
    };
    // The katana sits on WepAbsorpPosParam `right_0` and goes into the scabbard on `rightHang_0`.
    let Some(at) = sim.duel.fighters[0].weapon_dummies else {
        return;
    };
    let Some(blade) = find(&format!("dmy:{}", at.held)) else {
        // Models extracted before #103 hold the katana on `dmyblade:1`, at the wrong roll.
        if find("dmyblade:1").is_some() {
            warn_once!(
                "c0000.glb predates the katana fix (#103): run `sekiro-extract models c0000`"
            );
        }
        return;
    };
    let Some(sheath) = find(&format!("dmy:{}", at.sheathed)) else {
        return;
    };
    let Ok((parent, mut local)) = attachments.get_mut(blade) else {
        return;
    };
    let saved = *original.entry(blade).or_insert(*local);
    let report = &sim.last.player;
    // Cover's TAE has no 720 command: docking retains the sheath across these clips.
    let sheathed = sim
        .duel
        .player
        .docking
        .active
        .as_ref()
        .is_some_and(|t| sim.duel.player.docking.is_cover(t))
        || report
            .animation
            .as_ref()
            .and_then(|n| sim.duel.player.tae.anim(n))
            .is_some_and(|a| {
                a.events.iter().any(|e| {
                    e.start <= report.anim_time
                        && report.anim_time < e.end
                        && matches!(&e.kind, sekiro_sim::tae::TaeKind::Wire { kind: 720, args }
                if args.get(..2) == Some(&[0, 1]))
                })
            });
    if sheathed {
        if let (Ok(parent), Ok(sheath)) = (globals.get(parent.parent()), globals.get(sheath)) {
            *local =
                Transform::from_matrix(Mat4::from(parent.affine().inverse() * sheath.affine()));
        }
    } else {
        *local = saved;
    }
}

#[allow(clippy::type_complexity)]
fn grapple_marker(
    sim: NonSend<Sim>,
    cameras: Query<(&Camera, &GlobalTransform), With<FollowCamera>>,
    mut markers: Query<(&mut Node, &mut ImageNode), With<grapple_hud::Marker>>,
    sprites: Option<Res<grapple_hud::Sprites>>,
    mut materials: ResMut<Assets<grapple_hud::WireFill>>,
    mut cursors: Query<
        &mut Node,
        (
            With<grapple_hud::Cursor>,
            Without<grapple_hud::Fill>,
            Without<grapple_hud::Marker>,
        ),
    >,
    mut fills: Query<
        (&mut Node, &MaterialNode<grapple_hud::WireFill>),
        (With<grapple_hud::Fill>, Without<grapple_hud::Marker>),
    >,
) {
    use crate::change::set;
    let Ok((mut node, mut image)) = markers.single_mut() else {
        return;
    };
    let Some(target) = sim.duel.player.wire.preview.as_ref() else {
        set(&mut node, |n| &mut n.display, Display::None);
        return;
    };
    let Ok((camera, transform)) = cameras.single() else {
        set(&mut node, |n| &mut n.display, Display::None);
        return;
    };
    let state = target.state;
    let Ok(p) = camera.world_to_viewport(transform, Vec3::from_array(target.point.to_array()))
    else {
        set(&mut node, |n| &mut n.display, Display::None);
        return;
    };
    let Some(sprites) = sprites else { return };
    set(&mut node, |n| &mut n.display, Display::Flex);
    // At 1440p the ring sprite box is 66 px (the distance sprite's native size);
    // its visible outer diameter is about 49 px. Scale with the viewport height.
    let size = sprites.base_size * combat_hud::viewport_size(Some(camera)).y / 1440.0;
    set(&mut node, |n| &mut n.width, px(size.x));
    set(&mut node, |n| &mut n.height, px(size.y));
    set(&mut node, |n| &mut n.left, px(p.x - size.x * 0.5));
    set(&mut node, |n| &mut n.top, px(p.y - size.y * 0.5));
    set(&mut image, |i| &mut i.image, sprites.ring(state));
    if let Ok((mut inner, handle)) = fills.single_mut() {
        set(
            &mut inner,
            |n| &mut n.display,
            if state == sekiro_sim::wire::MarkerState::Ready && !target.rideable {
                Display::None
            } else {
                Display::Flex
            },
        );
        if let Some(mut material) = materials.get_mut(&handle.0) {
            sprites.draw(state, &mut material);
        }
    }
    if let Ok(mut cursor) = cursors.single_mut() {
        set(
            &mut cursor,
            |n| &mut n.display,
            if target.rideable {
                Display::None
            } else {
                Display::Flex
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sekiro_sim::fight::FighterKind;

    #[test]
    fn a_start_over_a_hole_fails_instead_of_loading_forever() {
        // Ground only west of x = -134 and no higher than -42.5, like the Outskirts ledge.
        let floor = |x: f32, _z: f32, top: f32| (x < -134.0 && top >= -42.5).then_some(-42.5);
        assert!(require_floor([-136.5, -42.54, -118.0], floor).is_ok());
        let err = require_floor([-132.3, -42.0, -123.9], floor).unwrap_err();
        assert!(
            err.to_string()
                .contains("no collision floor under (-132.30, -42.00, -123.90)"),
            "{err}"
        );
    }

    #[test]
    fn default_keys_crouch_lock_on_and_use_the_prosthetic() {
        let held = |k| k == KeyCode::KeyQ || k == KeyCode::ControlLeft;
        let (_, b) = keyboard_input(held, |m| m == MouseButton::Middle);
        assert!(b.crouch && b.prosthetic && b.lock_on);
        assert!(!b.action && !b.item && !b.attack);
    }

    #[test]
    fn identical_position_endpoints_stay_exact_at_every_render_fraction() {
        for position in [
            Vec3::new(-197.4507, -48.14, 150.72162),
            Vec3::new(-125.8, -47.23, -193.08),
        ] {
            for frame in 0..=100 {
                let alpha = frame as f32 / 100.;
                assert_eq!(interpolate_position(position, position, alpha), position);
                let next = position + Vec3::new(0.1, 0.2, 0.3);
                assert_eq!(
                    interpolate_position(position, next, alpha),
                    position.lerp(next, alpha)
                );
            }
        }
    }

    /// #46: the follow camera's projection tracks the sim camera's eased field of view, and is
    /// left untouched once it matches.
    #[test]
    fn projection_fov_follows_the_sim_camera() {
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: 60f32.to_radians(),
            ..default()
        });
        let Some(Projection::Perspective(p)) = follow_fov(&projection, 43.0) else {
            panic!("perspective projection");
        };
        assert!((p.fov.to_degrees() - 43.0).abs() < 1e-4);
        assert!(follow_fov(&Projection::Perspective(p), 43.0).is_none());
    }

    #[test]
    fn no_enemy_rejects_lock_and_contacts_without_restarting_wolf() {
        let cache = sekiro_sim::cache::root();
        let mut duel = Duel::load(&cache, 4.).unwrap();
        duel.use_simple_ai();
        let start = duel.player.body.position;
        // A hidden passive opponent here would push Wolf, be a target and receive attacks.
        duel.enemy.body.position = start;
        duel.force_act = Some(3000);
        vacant_enemy_slot(&mut duel);
        let dt = 1. / STEP_HZ as f32;
        for step in 0..480 {
            let frame = InputFrame {
                move_stick: [0., 1.],
                buttons: Buttons {
                    lock_on: step % 2 == 0,
                    attack: step % 120 == 0,
                    ..default()
                },
                ..default()
            };
            let report = duel.step(&frame, dt);
            assert!(duel.player.lock_target.is_none());
            assert!(duel.player.aim_candidates.is_empty());
            assert!(!report.locked_on && !report.respawned);
            assert!(report.hits.is_empty());
            assert!(duel.respawn_in.is_none());
            if step == 0 {
                assert!(
                    Vec3::from_array(report.player.position).distance(Vec3::from_array(start))
                        < 0.2,
                    "absent opponent pushed Wolf"
                );
            }
        }
        assert!(Vec3::from_array(duel.player.body.position).distance(Vec3::from_array(start)) > 1.);
        assert_eq!(
            duel.rules.fighters[WOLF].vitality.hp,
            duel.rules.fighters[WOLF].vitality.max_hp
        );

        duel.rules.fighters[WOLF].vitality.hp = 0;
        let mut respawned = false;
        for _ in 0..(STEP_HZ as f32 * (sekiro_sim::duel::RESPAWN_DELAY + 1.)) as usize {
            let report = duel.step(&InputFrame::default(), dt);
            respawned |= report.respawned;
            assert!(duel.player.lock_target.is_none());
            assert!(duel.player.aim_candidates.is_empty());
        }
        assert!(respawned, "player death no longer restarts exploration");
        assert!(!duel.rules.fighters[WOLF].dead());
        assert!(duel.respawn_in.is_none());
    }

    #[test]
    fn explore_hud_hides_enemy_and_debug_text_but_f3_toggles_player_details() {
        let cache = sekiro_sim::cache::root();
        let mut duel = Duel::load(&cache, 4.).unwrap();
        duel.use_simple_ai();
        vacant_enemy_slot(&mut duel);
        let sim = Sim::for_test(duel);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_non_send(sim)
            .insert_resource(DebugHud(false))
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(crate::brain_overlay::RecordingBrain {
                enabled: false,
                mode: "lua".into(),
                player: "default".into(),
                view: Default::default(),
                unicode_font: false,
            })
            .init_resource::<combat_hud::Gauges>()
            .add_systems(Startup, |mut commands: Commands| {
                combat_hud::spawn(&mut commands)
            })
            .add_systems(Update, (hud, combat_hud::update).chain());
        let text_entity = app.world_mut().spawn((Hud, Text::new(""))).id();
        app.update();
        for visible in [false, true, false] {
            if visible != app.world().resource::<DebugHud>().0 {
                let mut keys = ButtonInput::default();
                keys.press(KeyCode::F3);
                app.insert_resource(keys);
                app.update();
            }
            let text = &app.world().get::<Text>(text_entity).unwrap().0;
            assert!(text.contains("F3 debug text"));
            assert!(text.contains(&format!("{} crouch", key_label(CROUCH_KEY))));
            assert_eq!(text.contains("refs"), visible);
            assert!(
                !text.contains("Enemy") && !text.contains("Ashina") && !text.contains("lock-on")
            );
            let gauges = app.world().resource::<combat_hud::Gauges>();
            assert!(!gauges.enemy_alive && !gauges.boss_bar && !gauges.deathblow);
            assert!(gauges.enemy_head.is_none());
            assert!(gauges.enemy_name.is_empty());
            let world = app.world_mut();
            let mut parts = world.query::<(&combat_hud::HudPart, &Node)>();
            for (part, node) in parts.iter(world) {
                if matches!(
                    part,
                    combat_hud::HudPart::EnemyPanel | combat_hud::HudPart::BossPanel
                ) {
                    assert_eq!(node.display, Display::None);
                }
            }
        }
    }

    #[test]
    fn enemy_head_follows_the_head_bone_over_the_root() {
        let cache = sekiro_sim::cache::root();
        let Ok(mut lib) = AnimLibrary::load(&cache, "c1010") else {
            eprintln!("skipping: cache not extracted");
            return;
        };
        let idle = lib.clip("a000_000000").unwrap();
        let mut rig = Rig::new(Arc::new(lib.skeleton.clone()));
        rig.model = rig.skeleton.model_space(&idle.sample(0.0, &rig.skeleton));
        let mut cache_slot = None;
        let bone = head_bone(&rig, &mut cache_slot).expect("the soldier has a Head bone");
        assert_eq!(head_bone(&rig, &mut cache_slot), Some(bone));
        let root = Vec3::new(1.0, 2.0, 3.0);
        let head = enemy_head(root, rig.model.get(bone), 1.8);
        // Over the root, at a standing soldier's head height.
        assert_eq!((head.x, head.z), (root.x, root.z));
        assert!((1.3..2.0).contains(&(head.y - root.y)), "head at {head}");
        // Without a pose, just above the 1.8 m body capsule.
        assert_eq!(enemy_head(root, None, 1.8), root + Vec3::Y * 2.0);
    }

    #[test]
    fn castle_restart_restores_phase_one_story_lighting() {
        let cache = sekiro_sim::cache::root();
        let arena = ArenaSpec::by_name("genichiro").unwrap();
        let phase = arena.enemy_spec();
        let mut duel = Duel::load_with(&cache, 6., phase).unwrap();
        // The phase-two flag survives a fight reload, as it does after Tomoe.
        let events = duel.events.as_mut().unwrap();
        events.scripts.flags.insert(11_115_802);
        events.scripts.cast_scene.lighting = Some(3);
        events.scripts.cast_scene.enabled_order.push(8_907_080);
        events
            .scripts
            .cast_scene
            .gparam_sub_ids
            .insert((11, 1), (10, 0.));
        duel.reset().unwrap();
        for _ in 0..150 {
            duel.step(&InputFrame::default(), 1. / 60.);
            let scene = arena_scene(
                &duel.spec,
                &duel.events.as_ref().unwrap().scripts.cast_scene,
            );
            let hour = scene
                .lighting
                .and_then(sekiro_sim::story::StoryTime::from_variant)
                .map_or_else(
                    || arena.story_time(&duel.spec).hour(),
                    sekiro_sim::story::StoryTime::hour,
                );
            assert_eq!(hour, 6., "storm survived phase-one restart");
            assert!(
                !scene.enabled_order.contains(&8_907_080),
                "storm sky survived restart"
            );
            assert!(
                !scene.gparam_sub_ids.contains_key(&(11, 1)),
                "storm GPARAM survived restart"
            );
        }
    }

    /// Drawn frames between steps never run a clip backwards, hold exactly still while a hit stop
    /// holds the character, and resume from where they stopped. The soldier's row is set to the
    /// full hit stop types (his own are 0, as the Owl's), so Wolf's slash freezes both.
    #[test]
    fn drawn_clip_time_holds_through_hit_stop() {
        let cache = sekiro_sim::cache::root();
        if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
            eprintln!("skipping: cache not extracted");
            return;
        }
        let mut duel = Duel::load(&cache, 4.0).unwrap();
        duel.use_simple_ai();
        duel.enemy_active = false;
        let FighterKind::Npc(npc) = &mut duel.rules.fighters[1].kind else {
            panic!("the enemy is an NPC")
        };
        npc.hit_stop_type = 2;
        npc.hit_stop_type_defender = 1;
        let dt = 1.0 / STEP_HZ as f32;
        let input = |i: usize| match i {
            0..=90 => InputFrame {
                move_stick: [0.0, 1.0],
                ..InputFrame::default()
            },
            120 => InputFrame {
                buttons: Buttons {
                    attack: true,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            _ => InputFrame::default(),
        };
        let mut last: [Option<(String, f32)>; 2] = [None, None];
        let mut held_frames = [0; 2];
        let mut hit = false;
        for i in 0..260 {
            let report = duel.step(&input(i), dt);
            hit |= report.hits.iter().any(|(h, _)| h.attacker == 0);
            for (side, r) in [&report.player, &report.enemy].into_iter().enumerate() {
                let Some(name) = r.animation.clone() else {
                    continue;
                };
                let held = duel.hit_stop[side].active() && r.advanced == 0.0;
                for alpha in [0.0, 0.25, 0.5, 0.75, 0.999] {
                    let t = drawn_clip_time(r, alpha);
                    if let Some((last_name, last_t)) = &last[side]
                        && *last_name == name
                        && !r.clip_started
                    {
                        // Float rounding of `time - advanced` aside.
                        assert!(
                            t >= *last_t - 1e-6,
                            "side {side} step {i}: {t} after {last_t}"
                        );
                    }
                    if held {
                        assert_eq!(t, r.anim_time, "side {side} moved while held at step {i}");
                    }
                    last[side] = Some((name.clone(), t));
                }
                held_frames[side] += usize::from(held);
            }
        }
        assert!(hit, "Wolf's slash never connected");
        assert!(
            held_frames[0] >= 3 && held_frames[1] >= 3,
            "{held_frames:?}"
        );
    }

    #[test]
    fn script_route_steers_to_each_point_in_world_space_then_finishes() {
        let args = ["--script", "idle.txt", "--script-route", "0,-5; 3,-5"].map(String::from);
        let mut route = ScriptRoute::parse(&args).unwrap();
        assert!(!route.done());
        let walk = |f: InputFrame| Vec2::new(-f.camera_yaw.sin(), -f.camera_yaw.cos());
        // From the origin the stick walks straight toward the first point.
        let f = route.steer(Vec2::ZERO).unwrap();
        assert_eq!(f.move_stick, [0.0, 0.3]);
        assert!(walk(f).distance(Vec2::new(0.0, -1.0)) < 1e-5);
        // Within 0.25 m the point counts as reached and the next one is steered for.
        let f = route.steer(Vec2::new(0.1, -4.9)).unwrap();
        assert!(walk(f).distance(Vec2::new(2.9, -0.1).normalize()) < 1e-5);
        assert!(route.steer(Vec2::new(3.0, -5.0)).is_none());
        assert!(route.done());
    }

    #[test]
    fn script_route_is_empty_without_the_flag_needs_a_script_and_gives_up_after_its_limit() {
        assert!(ScriptRoute::parse(&[]).unwrap().done());
        let bad = ["--script", "idle.txt", "--script-route", "1;2"].map(String::from);
        assert!(ScriptRoute::parse(&bad).is_err());
        let alone = ["--script-route", "1,2"].map(String::from);
        assert!(ScriptRoute::parse(&alone).is_err());
        let args = ["--script", "idle.txt", "--script-route", "100,0"].map(String::from);
        let mut route = ScriptRoute::parse(&args).unwrap();
        for _ in 0..ScriptRoute::LIMIT {
            assert!(route.steer(Vec2::ZERO).is_some());
        }
        assert!(route.steer(Vec2::ZERO).is_none());
        assert!(route.done());
    }
}
