//! sekiro-rs's map (AKJama's sekiro-rs, MIT / Apache-2.0, used as a library from the local clone
//! `../sekiro-rs`, branch `local/game-lib`): the real game world streamed at its absolute
//! positions, with sekiro-rs's lighting, sky, fog and post chain, instead of the baked arena's
//! visuals, on every exported arena sekiro-rs has the map of (not the generated sandbox);
//! `SHINOBI_SR_MAP=0` keeps the own map.
//! Collision, grapple points and ledges stay ours (map.rs) and move into the same absolute frame
//! (map.rs `Frame`), so the fight runs where the game has it.
//! Our own scenes (the generated sandbox and Construct, sandbox.rs) keep their own geometry and
//! take only his lighting (`lighting_only`): his map setup for their light source map
//! (`SHINOBI_SANDBOX_SRC`, default m11_01) with no pieces, objects or connected world, so the
//! light set, the story's hour, fog, sky, exposure, bloom, grading and sun shadows are his.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;
use sekiro_rs::map::{GroupMode, MapFocus, MapScene, PlayerFocus};

use crate::config::GameConfig;

/// The asset source of sekiro-rs's export cache: `srcache://maps/<id>/...`.
pub const SOURCE: &str = "srcache";

/// The source map sekiro-rs draws (json "map" of the arena the config starts on), when that
/// map has a sekiro-rs export and [`wanted`]. Fixed for the run.
pub fn active_map() -> Option<&'static str> {
    static MAP: OnceLock<Option<String>> = OnceLock::new();
    MAP.get_or_init(|| {
        if !wanted() {
            return None;
        }
        let map = GameConfig::load().ok().and_then(|c| crate::map::map_id(&c)).and_then(|id| if crate::sandbox::is_generated(&id) { source_map(&crate::sandbox::source()) } else { source_map(&id) });
        match &map {
            Some(m) => info!("sekiro-rs map: {m} from {}", cache().display()),
            None => info!("sekiro-rs map: the start arena has no source map exported by sekiro-rs (or is the generated sandbox); own map kept"),
        }
        map
    })
    .as_deref()
}

/// Whether this run draws its own generated scene (sandbox.rs) under sekiro-rs's lighting only.
pub fn lighting_only() -> bool {
    static ONLY: OnceLock<bool> = OnceLock::new();
    *ONLY.get_or_init(|| active() && GameConfig::load().ok().and_then(|c| crate::map::map_id(&c)).is_some_and(|id| crate::sandbox::is_generated(&id)))
}

/// Whether sekiro-rs draws the game's world this run (not only the lighting of our own scene).
pub fn world() -> bool {
    active() && !lighting_only()
}

/// sekiro-rs's map is the default; `SHINOBI_SR_MAP=0` keeps the own map. Off in the tests unless set.
fn wanted() -> bool {
    match std::env::var("SHINOBI_SR_MAP") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => !cfg!(test),
    }
}

/// Whether a switch to `arena` needs a new run: sekiro-rs draws this run, or would draw that one.
pub fn switch_relaunches(arena: &str) -> bool {
    active() || (wanted() && source_map(arena).is_some())
}

/// Whether sekiro-rs draws the world this run.
pub fn active() -> bool {
    active_map().is_some()
}

/// Whether sekiro-rs grows grass on [`active_map`] (its export has `grass/grass.json`, the
/// Ashina outskirts maps so far); elsewhere ours is drawn (map_grass.rs).
pub fn has_grass() -> bool {
    active_map().is_some_and(|m| cache().join("maps").join(m).join("grass").join("grass.json").is_file())
}

/// sekiro-rs's export cache: `SEKIRO_CACHE`, else `../sekiro-rs/cache` beside this repo.
pub fn cache() -> PathBuf {
    std::env::var_os("SEKIRO_CACHE").map(PathBuf::from).unwrap_or_else(|| crate::paths::root().join("..").join("sekiro-rs").join("cache"))
}

/// The source map of an exported arena (json "map"), when sekiro-rs has exported it.
fn source_map(arena: &str) -> Option<String> {
    if crate::sandbox::is_generated(arena) {
        return None;
    }
    let text = std::fs::read_to_string(crate::paths::extracted().join(format!("map_{arena}.json"))).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let map = json["map"].as_str()?.to_string();
    cache().join("maps").join(&map).join("layout.json").is_file().then_some(map)
}

/// Before `DefaultPlugins`: the cache as a named asset source.
pub fn register_source(app: &mut App) {
    app.register_asset_source(SOURCE, AssetSourceBuilder::platform_default(&cache().to_string_lossy(), None));
}

/// After `DefaultPlugins`: sekiro-rs's map setup for [`active_map`], and Wolf as its focus.
pub fn install(app: &mut App) {
    let Some(id) = active_map() else { return };
    let cache = cache();
    let config = GameConfig::load().ok();
    // The fight's own draw param variant and hour (map.rs FIGHT_LIGHTING, e.g. the castle top),
    // else the config's hour on the map's default draw param.
    let fight = config.as_ref().and_then(|c| {
        let arena = crate::map::map_id(c)?;
        let (chr, row) = fight_env().unwrap_or((c.enemy.chr.clone(), c.enemy.npc_row));
        crate::map::fight_lighting(&arena, crate::map::fight_row(&chr, row))
    });
    // `SHINOBI_HOUR` first (as map.rs), so his sky, probes and light sets start at that hour.
    let hour = std::env::var("SHINOBI_HOUR").ok().and_then(|v| v.parse().ok()).or(fight.map(|(_, h)| h)).or(config.as_ref().map(|c| c.world.hour)).unwrap_or(f32::NAN);
    let gparam = fight.filter(|(v, _)| *v != "0000").map(|(v, _)| format!("{}_{v}", id.get(..6).unwrap_or(id)));
    let result = (|| -> Result<(), String> {
        let dir = sekiro_rs::map::map_dir(&cache, id).map_err(|e| e.to_string())?;
        if lighting_only() {
            // Our scene's own geometry under his lighting: no pieces, objects, collision or
            // connected world (`within` keeps the map apart from its neighbours).
            app.insert_resource(sekiro_rs::map::upload_budget());
            let scene = MapScene {
                prefix: format!("{SOURCE}://maps/{id}/"),
                collision: false,
                no_pieces: true,
                hide: Vec::new(),
                objects: false,
                groups: GroupMode::Draw,
                markers: false,
                hour,
                gparam,
                within: Some((Vec3::ZERO, 0.0)),
                focus: None,
            };
            sekiro_rs::map::add_map(app, &dir, scene, Default::default()).map_err(|e| e.to_string())?;
            sekiro_rs::map::ready::install(app);
            info!("sekiro-rs lighting of {id} on our own scene");
            return Ok(());
        }
        let layouts = sekiro_rs::map::world::play_blocks(&cache, id).map_err(|e| e.to_string())?;
        let stories = sekiro_rs::map::story::load_blocks(&cache, &layouts).map_err(|e| e.to_string())?;
        let _ = IDOL_SPOTS.set(idol_spots(&layouts));
        let _ = BLOCKS.set(layouts.keys().cloned().collect());
        // The whole play world's collision (its blocks, helpers kept off the player, placed
        // objects the story has on), for map.rs's Terrain in place of the arena's.
        let t0 = std::time::Instant::now();
        match sekiro_rs::map::world::collision_world(&cache, &layouts, &stories) {
            Ok(w) => {
                info!("sekiro-rs map {id}: play-world collision, {} triangles over {:?}, in {:.1} s", w.triangle_count(), layouts.keys().collect::<Vec<_>>(), t0.elapsed().as_secs_f32());
                let _ = WORLD.set(Arc::new(w));
            }
            Err(e) => warn!("sekiro-rs map {id}: no play-world collision ({e}); the arena's kept"),
        }
        app.insert_resource(sekiro_rs::map::upload_budget());
        sekiro_rs::map::add_map(
            app,
            &dir,
            MapScene {
                prefix: format!("{SOURCE}://maps/{id}/"),
                collision: false,
                no_pieces: false,
                hide: Vec::new(),
                objects: sekiro_rs::map::world::enabled(id),
                groups: GroupMode::Draw,
                markers: false,
                hour,
                gparam,
                within: None,
                focus: None,
            },
            stories,
        )
        .map_err(|e| e.to_string())?;
        sekiro_rs::map::ready::install(app);
        Ok(())
    })();
    if let Err(e) = result {
        panic!("sekiro-rs map {id}: {e}");
    }
    app.add_systems(Update, sync_hour.before(sekiro_rs::lighting::apply_lighting));
    if lighting_only() {
        app.add_systems(Update, scene_light_set.before(sekiro_rs::lighting::apply_lighting));
    } else {
        app.add_systems(Update, follow_wolf);
    }
}

static WORLD: OnceLock<Arc<sekiro_sim::collision::CollisionWorld>> = OnceLock::new();

/// The play world's Sculptor's Idols: entity -> (base, the way its front faces).
static IDOL_SPOTS: OnceLock<HashMap<i64, (Vec3, Vec3)>> = OnceLock::new();

/// BonfireWarpParam's idols in the play blocks' layouts: the objects of its `bonfireEntityId`s
/// (o005000 / o005500 / o115040), at their place (Bevy space, as the map draws them), facing
/// the carved front's way (its -Z, idol.rs IDOL_FACING).
fn idol_spots(layouts: &std::collections::BTreeMap<String, serde_json::Value>) -> HashMap<i64, (Vec3, Vec3)> {
    let ids: std::collections::HashSet<i64> = crate::idol::warp_rows().iter().map(|r| r.entity).collect();
    let mut out = HashMap::new();
    for layout in layouts.values() {
        for o in layout["objects"].as_array().into_iter().flatten() {
            let Some(e) = o["entity_id"].as_i64().filter(|e| ids.contains(e)) else { continue };
            let f = |v: &serde_json::Value, i: usize| v[i].as_f64().unwrap_or(0.0) as f32;
            let (t, r) = (&o["translation"], &o["rotation"]);
            let rot = Quat::from_xyzw(f(r, 0), f(r, 1), f(r, 2), f(r, 3)).normalize();
            let out_dir = (rot * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::Z);
            out.insert(e, (Vec3::new(f(t, 0), f(t, 1), f(t, 2)), out_dir));
        }
    }
    info!("sekiro-rs map: {} Sculptor's Idols in the play world", out.len());
    out
}

/// An idol of the play world: (base, front's way).
pub fn idol_spot(entity: i64) -> Option<(Vec3, Vec3)> {
    IDOL_SPOTS.get()?.get(&entity).copied()
}

/// The source map an entity is in: its id's area and block (Sekiro's AABxxxx: 1111950 is
/// m11_01_00_00).
pub fn map_of(entity: i64) -> String {
    format!("m{:02}_{:02}_00_00", entity / 100_000, (entity / 10_000) % 10)
}

/// The exported arena a run on `map` starts from (its "map" in map_<arena>.json, with
/// sekiro-rs's export): one that is no boss's arena first. None when no arena is on it.
pub fn arena_on(map: &str) -> Option<String> {
    static ARENAS: OnceLock<HashMap<String, String>> = OnceLock::new();
    ARENAS
        .get_or_init(|| {
            let mut out: HashMap<String, String> = HashMap::new();
            let Ok(dir) = std::fs::read_dir(crate::paths::extracted()) else { return out };
            let mut names: Vec<String> = dir.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter_map(|n| Some(n.strip_prefix("map_")?.strip_suffix(".json")?.to_string())).collect();
            // Not a boss's arena first, then by name.
            names.sort_by_key(|n| (n.starts_with("boss_"), n.clone()));
            for arena in names {
                if let Some(m) = source_map(&arena) {
                    out.entry(m).or_insert(arena);
                }
            }
            out
        })
        .get(map)
        .cloned()
}

/// The run's start when nothing names a map (no `SHINOBI_MAP`, no config world.map): the
/// Ashina Castle idol (BonfireWarpParam bonfireEntityId 1111950) in the connected world, no fight.
pub const DEFAULT_START: (&str, i64) = ("m11_01_00_00", 1_111_950);

/// Whether this run takes [`DEFAULT_START`]: sekiro-rs's map is wanted and nothing names a map or
/// a fight (`SHINOBI_FIGHT` fights on its own arena, map.rs map_id).
pub fn default_start(config: &GameConfig) -> bool {
    wanted() && std::env::var_os("SHINOBI_MAP").is_none() && fight_env().is_none() && config.world.map.as_deref().is_none_or(str::is_empty)
}

/// `SHINOBI_IDOL=<entity>`: the run starts at that idol (idol travel to another map, [`travel`]);
/// else [`DEFAULT_START`]'s idol on a run that takes it.
pub fn idol_start() -> Option<i64> {
    static DEFAULT: OnceLock<bool> = OnceLock::new();
    match std::env::var("SHINOBI_IDOL") {
        Ok(v) => v.parse().ok(),
        Err(_) => (*DEFAULT.get_or_init(|| GameConfig::load().is_ok_and(|c| default_start(&c)))).then_some(DEFAULT_START.1),
    }
}

/// Idol travel to another map: this exe again on an arena of `entity`'s map, up at the idol,
/// without the arena's fight; this run ends.
pub fn travel(entity: i64) -> ! {
    let map = map_of(entity);
    let Some(arena) = arena_on(&map) else {
        error!("idol travel: no exported arena on {map}");
        std::process::exit(1)
    };
    info!("idol travel: relaunching on {arena} ({map}) at idol {entity}");
    let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).env("SHINOBI_MAP", &arena).env("SHINOBI_IDOL", entity.to_string()).env_remove("SHINOBI_FIGHT").spawn());
    if let Err(e) = started {
        error!("idol travel: relaunch failed: {e}");
    }
    std::process::exit(0)
}

/// The play world's maps (sekiro-rs `play_blocks`, set up by [`install`]).
static BLOCKS: OnceLock<Vec<String>> = OnceLock::new();

/// The play world's maps (none outside sekiro-rs's map).
pub fn blocks() -> Vec<String> {
    BLOCKS.get().cloned().unwrap_or_default()
}

/// The play world's wire points and hang / hug edges: every block's whole-map export
/// (`sekiro-extract edges`, `extracted/map_edges_<map>.json`, MSB space, X mirrored on load
/// like an arena's). None outside sekiro-rs's map or when no block has one (then the arena's).
pub fn edges() -> Option<(Vec<crate::traversal::GrapplePoint>, Vec<crate::traversal::LedgeEdge>)> {
    let (mut grapple, mut ledges, mut read) = (Vec::new(), Vec::new(), Vec::new());
    for map in BLOCKS.get()? {
        let path = crate::paths::extracted().join(format!("map_edges_{map}.json"));
        let Some(json) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            warn!("sekiro-rs map: no {} (sekiro-extract edges), its wire points and ledges are missing", path.display());
            continue;
        };
        grapple.extend(crate::map::grapple_in(&json, crate::place::Frame::IDENTITY));
        ledges.extend(crate::map::ledges_in(&json, crate::place::Frame::IDENTITY));
        read.push(map.as_str());
    }
    info!("sekiro-rs map: {} wire points, {} hang / hug edges over {read:?}", grapple.len(), ledges.len());
    (!read.is_empty()).then_some((grapple, ledges))
}

/// sekiro-rs's play-world collision for [`active_map`] (set up by [`install`]).
pub fn collision() -> Option<Arc<sekiro_sim::collision::CollisionWorld>> {
    WORLD.get().cloned()
}

/// `SHINOBI_FIGHT=<chr>[:<row>]` (menu.rs, and [`relaunch`]): the fight this run starts.
pub fn fight_env() -> Option<(String, Option<i64>)> {
    let v = std::env::var("SHINOBI_FIGHT").ok()?;
    let mut it = v.split(':');
    Some((it.next()?.to_string(), it.next().and_then(|r| r.parse().ok())))
}

/// sekiro-rs's map is set up once per run, so another arena means a new run: this exe again
/// on `map` (`SHINOBI_MAP`) with the fight (`SHINOBI_FIGHT`, menu.rs), and this one ends.
pub fn relaunch(map: &str, chr: &str, npc_row: Option<i64>) -> ! {
    let fight = npc_row.map_or(chr.to_string(), |r| format!("{chr}:{r}"));
    info!("sekiro-rs map: relaunching on {map} for {fight}");
    let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).env("SHINOBI_MAP", map).env("SHINOBI_FIGHT", &fight).env_remove("SHINOBI_IDOL").spawn());
    if let Err(e) = started {
        error!("sekiro-rs map: relaunch failed: {e}");
    }
    std::process::exit(0)
}

/// The light a lit FXR particle takes under sekiro-rs's map (fxr.rs, as map.rs `particle_light`
/// on the own map): his light set in force (the collision under Wolf, the story's hour) through
/// his `LightSet` (library), in game units: (hemisphere ambient, sun DiffColor0 x W, toward the
/// sun).
pub fn particle_light(l: &sekiro_rs::lighting::MapLighting) -> (Vec3, Vec3, Vec3) {
    let set = sekiro_rs::lighting::LightSet::from_gparam(&l.gparam, l.light_set, l.hour);
    let lin = |c: Color| {
        let c = c.to_linear();
        Vec3::new(c.red, c.green, c.blue)
    };
    (lin(set.ambient), lin(set.sun_color) * set.sun_lux / sekiro_rs::lighting::SCENE_TO_BEVY, set.sun_toward)
}

/// The hour our side asks for (`SHINOBI_HOUR`, the `[` / `]` keys, the debug menu's hour and
/// day-night cycle: map.rs MapLighting, `applied` cleared) goes to his lighting, which applies
/// that time of day's light set and post settings again. gap: his sky dome, probes and fog
/// variant stay the run's start ones.
pub(crate) fn sync_hour(ours: Option<ResMut<crate::map::MapLighting>>, his: Option<ResMut<sekiro_rs::lighting::MapLighting>>) {
    let (Some(mut ours), Some(mut his)) = (ours, his) else { return };
    if ours.applied {
        return;
    }
    ours.applied = true;
    if his.hour != ours.hour {
        his.hour = ours.hour;
        his.applied = None;
    }
}

/// Our own scene stands on no collision of his map, so its light set is the one our export took
/// from the collision under the source arena's fight spot (map.rs MapLighting, json "light_set":
/// the MSB part's GparamConfig LightSetID; `SHINOBI_LIGHT_SET` overrides). His row 0 is the map
/// pieces' own (dark at night: they are lit per instance), the collision rows light characters.
pub(crate) fn scene_light_set(ours: Option<Res<crate::map::MapLighting>>, his: Option<ResMut<sekiro_rs::lighting::MapLighting>>) {
    if let (Some(ours), Some(mut his)) = (ours, his) {
        if his.light_set != ours.light_set {
            his.light_set = ours.light_set;
        }
    }
}

/// Wolf is the map's focus: the draw groups and light sets follow the part he stands on, kept
/// while he is in the air (sekiro-rs `PlayerFocus`).
fn follow_wolf(mut commands: Commands, wolf: Query<(Entity, &crate::actor::Actor, Option<&PlayerFocus>), With<crate::player::Player>>) {
    for (e, actor, focus) in &wolf {
        let grounded = !actor.airborne;
        match focus {
            Some(f) if f.grounded == grounded => {}
            Some(_) => {
                commands.entity(e).insert(PlayerFocus { grounded });
            }
            None => {
                commands.entity(e).insert((MapFocus, PlayerFocus { grounded }));
            }
        }
    }
}

/// sekiro-rs's play world as the core's ground (terrain.rs).
impl crate::terrain::WorldCollision for sekiro_sim::collision::CollisionWorld {
    fn floor_hit(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        sekiro_sim::collision::CollisionWorld::floor_hit(self, x, z, top)
    }
    fn floor_material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        sekiro_sim::collision::CollisionWorld::floor_material(self, x, z, top)
    }
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        sekiro_sim::collision::CollisionWorld::raycast(self, origin, dir, max)
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        sekiro_sim::collision::CollisionWorld::slide(self, feet, delta)
    }
    fn triangle_count(&self) -> usize {
        sekiro_sim::collision::CollisionWorld::triangle_count(self)
    }
    fn water_surface(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        sekiro_sim::body::Ground::water_surface(self, x, z, top).map(|w| (w.height, w.material))
    }
}
