//! A map area exported by `sr-extract map <id>`, streamed into a Bevy app.
//!
//! Map piece GLBs are spawned at their MSB instance transforms (already in Bevy space in
//! `layout.json`). [`add_map`] puts the map (pieces, draw-group culling around a [`MapFocus`]
//! entity, sun, fog, collision view) into an app; RSekiro uses it (src/sr_map.rs).

#[cfg(test)]
#[path = "map_gate_tests.rs"]
mod gate;

use std::path::{Path, PathBuf};
use std::time::Instant;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::asset::RenderAssetUsages;
use bevy::asset::{LoadState, UntypedAssetId};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::light::GlobalAmbientLight;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::wireframe::{Wireframe, WireframeColor};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use bevy::world_serialization::{WorldInstance, WorldInstanceSpawner};
use serde_json::Value;

#[path = "map_story.rs"]
pub mod story;

#[path = "map_visibility.rs"]
pub mod visibility;

#[path = "map_lod.rs"]
pub(crate) mod lod;
#[path = "map_material.rs"]
mod map_material;
pub(crate) use map_material::batches::StaticBatches;
#[path = "map_mesh.rs"]
mod mesh_merge;
pub(crate) use mesh_merge::{merge_meshes, opaque};

pub(crate) use map_material::{ATTR_MAP_BLEND2, LayeredMaterial};
#[path = "sky.rs"]
pub(crate) mod sky;
#[path = "map_world.rs"]
pub mod world;

#[path = "map_lights.rs"]
mod local_lights;
use local_lights::{LocalLights, update_local_lights};

#[path = "capture_ready.rs"]
pub mod ready;

use crate::lighting::{
    MapLighting, MapProbes, ProbeRegion, ShadowSettings, apply_lighting, apply_probes, spawn_lights,
};

/// How map pieces are culled by draw groups. In the game, the hit collision the player stands
/// on selects which pieces draw, which swaps distant stand-ins for detailed geometry; `Display`
/// uses the collision's display groups (word block 0 of the MSB mask struct), `Draw` its draw
/// groups (block 1), matched against each piece's draw groups (block 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupMode {
    Off,
    Display,
    Draw,
}

/// The map part root (a [`DrawGroups`] entity) at or above `entity`, if it belongs to the map.
pub(crate) fn map_root(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<DrawGroups>>,
) -> Option<Entity> {
    let mut ancestor = entity;
    while !roots.contains(ancestor) {
        ancestor = parents.get(ancestor).ok()?.parent();
    }
    Some(ancestor)
}

/// The world-space axis-aligned bounds of a mesh's local bounds under `transform`.
pub(crate) fn world_bounds(transform: &GlobalTransform, aabb: &Aabb) -> (Vec3, Vec3) {
    let centre = transform.transform_point(Vec3::from(aabb.center));
    let extent = Vec3::from(transform.affine().matrix3.abs() * aabb.half_extents);
    (centre - extent, centre + extent)
}

/// The startup system that spawns map roots and queues their GLBs; models that must load
/// before the map (the player's) are requested `.before` it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapSpawn;

/// A piece's draw groups.
#[derive(Component)]
pub(crate) struct DrawGroups([u32; 8]);

/// A map part whose MSB authors a light set other than the ordinary row 0: it is lit with
/// the collision-selected light set, as characters are (#30).
#[derive(Component)]
pub(crate) struct FollowsSceneLight;

/// A layout part row's MSB GparamConfig ids (light, fog, scattering, env map), -1 when absent.
fn gparam_config(row: &Value) -> [i32; 4] {
    std::array::from_fn(|i| row["gparam"][i].as_i64().unwrap_or(-1) as i32)
}

/// Whether a layout part row authors its own light set (MSB GparamConfig light set id > 0).
pub(crate) fn follows_scene_light(row: &Value) -> bool {
    crate::lighting::part_follows_scene(gparam_config(row)[0])
}

/// Distant render-only scenery is independent of the local collision's draw groups.
#[derive(Component)]
struct FarScenery;

/// The MSB part a legacy (disconnected or arena) scene root was placed from. Unlike
/// [`world::BlockPart`] it opts into nothing but per-part data such as grass.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct MsbPart {
    pub(crate) block: String,
    pub(crate) part_index: usize,
}

impl MsbPart {
    /// The part a layout row of `block` names, when the row records its MSB index.
    pub(crate) fn from_row(block: &str, row: &Value) -> Option<Self> {
        Some(Self {
            block: block.to_owned(),
            part_index: usize::try_from(row["part_index"].as_u64()?).ok()?,
        })
    }
}

type LegacyPieceFilter = (Without<FarScenery>, Without<world::BlockPart>);

struct CollisionPart {
    name: String,
    first: usize,
    count: usize,
    min: Vec2,
    max: Vec2,
    display: [u32; 8],
    draw: [u32; 8],
    /// Light set id of the collision's gparam config (-1 when none).
    light_set: i32,
    fog_set: i32,
    scattering_set: i32,
    env_map: i32,
    /// `SceneGparam` event bytes: the family file index of layers 1 to 4 (-1: none).
    layers: [i8; 4],
}

#[derive(Resource)]
pub(crate) struct Groups {
    mode: GroupMode,
    parts: Vec<CollisionPart>,
    /// Collision part under the camera and the groups in force.
    current: Option<usize>,
    active: Option<[u32; 8]>,
    checked_at: Option<Vec3>,
    dirty: bool,
}

fn groups8(v: &Value) -> [u32; 8] {
    let mut out = [0u32; 8];
    for (o, x) in out.iter_mut().zip(v.as_array().into_iter().flatten()) {
        *o = x
            .as_u64()
            .map(|v| v as u32)
            .or_else(|| x.as_i64().map(|v| v as u32))
            .unwrap_or(0);
    }
    out
}

#[derive(Component)]
struct CollisionView;

/// Enemy and player start markers.
#[derive(Component)]
struct Marker;

/// Map GLBs requested at once while streaming. Each request counts until its textures have
/// loaded too, so the nearest models and their textures finish before farther ones start.
/// Eight keeps the loaders busy (the same total load time as 32 in the Outskirts) while
/// fewer assets finish in one frame: the longest loading frame fell from 3.2-5.2 s to 2.2-2.4 s.
const STREAM_IN_FLIGHT: usize = 8;

/// One map GLB to load and the scene instances that wait for it.
struct StreamRequest {
    glb: String,
    /// The connected-world block that owns the model, for its residency state.
    block: Option<String>,
    /// Bounded sibling paths of the connected world need the explicit override.
    unapproved: bool,
    /// Entities that get this GLB's scene (`level`) once it is requested.
    targets: Vec<(Entity, usize)>,
}

/// Every GLB handle the map needs, to know when loading is done.
/// Map GLBs are requested nearest-first from the start position, a few at a time
/// ([`stream_assets`]), so the area around the player loads first.
#[derive(Resource)]
pub struct Pending {
    handles: Vec<UntypedHandle>,
    /// Map piece scene instances spawned, and how many have finished spawning.
    instances: usize,
    ready: usize,
    ready_roots: std::collections::HashSet<Entity>,
    /// When [`add_map`] was called, for the load time in the log.
    added_at: Instant,
    loaded_at: Option<Instant>,
    /// GLBs in load order, the next one to request, and the GLBs requested but not finished.
    stream: Vec<StreamRequest>,
    stream_index: std::collections::HashMap<String, usize>,
    next: usize,
    in_flight: Vec<UntypedHandle>,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            handles: Vec::new(),
            instances: 0,
            ready: 0,
            ready_roots: std::collections::HashSet::new(),
            added_at: Instant::now(),
            loaded_at: None,
            stream: Vec::new(),
            stream_index: std::collections::HashMap::new(),
            next: 0,
            in_flight: Vec::new(),
        }
    }
}

impl Pending {
    /// Every map piece and texture has loaded.
    pub fn loaded(&self) -> bool {
        self.loaded_at.is_some()
    }

    /// Seconds since every map piece and texture loaded (0 before).
    pub fn loaded_for(&self) -> f32 {
        self.loaded_at.map_or(0.0, |t| t.elapsed().as_secs_f32())
    }

    /// Queue `entity` to receive scene `level` of `glb` once that GLB is requested, and return
    /// the GLB's stream index. Requests keep the order of their first call, so callers queue
    /// nearest-first.
    pub(crate) fn queue(
        &mut self,
        glb: &str,
        block: Option<&str>,
        unapproved: bool,
        target: (Entity, usize),
    ) -> usize {
        let index = match self.stream_index.get(glb) {
            Some(&index) => index,
            None => {
                self.stream.push(StreamRequest {
                    glb: glb.to_string(),
                    block: block.map(str::to_string),
                    unapproved,
                    targets: Vec::new(),
                });
                self.stream_index
                    .insert(glb.to_string(), self.stream.len() - 1);
                self.stream.len() - 1
            }
        };
        self.stream[index].targets.push(target);
        self.instances += 1;
        index
    }

    /// A scene instance attached outside the stream (a capture scene, or a level of detail
    /// first selected during play).
    pub(crate) fn add_instance(&mut self) {
        if self.loaded_at.is_none() {
            self.instances += 1;
        }
    }

    /// The GLB of stream request `index` once the stream has requested it, with its override.
    pub(crate) fn requested(&self, index: usize) -> Option<(&str, bool)> {
        (index < self.next).then(|| {
            let request = &self.stream[index];
            (request.glb.as_str(), request.unapproved)
        })
    }

    /// Requested GLBs that have not finished, with their textures.
    pub fn streaming(&self) -> usize {
        self.stream.len() - self.next + self.in_flight.len()
    }
}

/// Whether an asset and its dependencies have loaded, or its load failed.
fn asset_finished(assets: &AssetServer, id: impl Into<UntypedAssetId>) -> bool {
    let id = id.into();
    assets.is_loaded_with_dependencies(id)
        || matches!(assets.get_load_state(id), Some(LoadState::Failed(_)))
}

/// Load a map asset, with the sibling-path override the connected world needs.
pub(crate) fn load_map_asset<A: Asset>(
    assets: &AssetServer,
    path: impl Into<bevy::asset::AssetPath<'static>>,
    unapproved: bool,
) -> Handle<A> {
    if unapproved {
        assets.load_builder().override_unapproved().load(path)
    } else {
        assets.load(path)
    }
}

/// Scene `level` of a map GLB.
pub(crate) fn scene_handle(
    assets: &AssetServer,
    glb: &str,
    level: usize,
    unapproved: bool,
) -> Handle<bevy::world_serialization::WorldAsset> {
    load_map_asset(
        assets,
        GltfAssetLabel::Scene(level).from_asset(glb.to_string()),
        unapproved,
    )
}

/// Request queued map GLBs, nearest first, keeping [`STREAM_IN_FLIGHT`] loading at a time.
fn stream_assets(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut pending: ResMut<Pending>,
    mut world: Option<ResMut<world::MapWorld>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("stream_assets");
    if pending.next >= pending.stream.len() && pending.in_flight.is_empty() {
        return;
    }
    pending
        .in_flight
        .retain(|h| !asset_finished(&assets, h.id()));
    while pending.in_flight.len() < STREAM_IN_FLIGHT && pending.next < pending.stream.len() {
        let index = pending.next;
        pending.next += 1;
        let request = &pending.stream[index];
        let handle: Handle<Gltf> = load_map_asset(&assets, request.glb.clone(), request.unapproved);
        for &(entity, level) in &request.targets {
            commands.entity(entity).insert(WorldAssetRoot(scene_handle(
                &assets,
                &request.glb,
                level,
                request.unapproved,
            )));
        }
        if let (Some(block), Some(world)) = (request.block.clone(), world.as_mut())
            && let Some(block) = world.blocks.get_mut(&block)
        {
            block.handles.push(handle.clone());
        }
        pending.handles.push(handle.clone().untyped());
        pending.in_flight.push(handle.untyped());
    }
}

/// Distance from `focus` to a placed model's oriented bounds (its origin without bounds).
pub(crate) fn placed_distance(model: &Value, focus: Vec3, transform: &Transform) -> f32 {
    match (vec3(&model["min"]), vec3(&model["max"])) {
        (Some(min), Some(max)) => lod::bounds_distance(focus, transform, min, max),
        _ => focus.distance(transform.translation),
    }
}

/// A part row's placement in Bevy space.
pub(crate) fn row_transform(row: &Value) -> Option<Transform> {
    pose(row).map(|p| Transform {
        translation: p.translation,
        rotation: p.rotation,
        scale: p.scale,
    })
}

/// The world-space horizontal bounding box of a model row placed at `transform` (from the `min` and
/// `max` its `models` entry records).
fn placed_box(model: &Value, transform: &Transform) -> Option<(Vec2, Vec2)> {
    let (min, max) = (vec3(&model["min"])?, vec3(&model["max"])?);
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let w = transform.transform_point(corner);
        lo = lo.min(Vec2::new(w.x, w.z));
        hi = hi.max(Vec2::new(w.x, w.z));
    }
    Some((lo, hi))
}

pub(crate) struct Pose {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

fn vec3(v: &Value) -> Option<Vec3> {
    Some(Vec3::new(
        v.get(0)?.as_f64()? as f32,
        v.get(1)?.as_f64()? as f32,
        v.get(2)?.as_f64()? as f32,
    ))
}

/// A layout row's translation, rotation and scale.
pub(crate) fn pose(row: &Value) -> Option<Pose> {
    let r = &row["rotation"];
    Some(Pose {
        translation: vec3(&row["translation"])?,
        rotation: Quat::from_xyzw(
            r.get(0)?.as_f64()? as f32,
            r.get(1)?.as_f64()? as f32,
            r.get(2)?.as_f64()? as f32,
            r.get(3)?.as_f64()? as f32,
        ),
        scale: vec3(&row["scale"]).unwrap_or(Vec3::ONE),
    })
}

#[derive(Resource)]
struct Layout {
    /// The map ID (the map folder's name).
    id: String,
    json: Value,
    collision: Option<sekiro_formats::hknp::CollisionMesh>,
    scene: MapScene,
}

/// How a map is shown.
#[derive(Clone, Debug)]
pub struct MapScene {
    /// Asset path of the map folder below the asset root, with a trailing slash ("" when the
    /// asset root is the map folder).
    pub prefix: String,
    /// Start with the collision wireframe visible.
    pub collision: bool,
    /// Hide the map pieces (collision only).
    pub no_pieces: bool,
    /// Skip models whose name starts with any of these.
    pub hide: Vec<String>,
    /// Also spawn MSB objects.
    pub objects: bool,
    pub groups: GroupMode,
    /// Enemy and player start markers.
    pub markers: bool,
    /// Time of day in hours for the draw params (light sets are keyed by time).
    pub hour: f32,
    /// Draw param file stem (`param/drawparam/<stem>.gparam`); default `<mXX>_<YY>_0000`.
    pub gparam: Option<String>,
    /// Arena collision and object radius. Map pieces use ARENA_VISUAL_RADIUS unless
    /// --no-far-scenery is set; `None` loads the whole map.
    pub within: Option<(Vec3, f32)>,
    /// Where the player or camera starts: map GLBs load nearest-first from here. `None` keeps
    /// file order.
    pub focus: Option<Vec3>,
}

/// Default draw param of a map: `m11_00_00_00` -> `m11_00_0000`.
#[cfg(test)]
fn default_gparam(id: &str) -> String {
    format!("{}_0000", id.get(..6).unwrap_or(id))
}

/// Cinematic layouts use the full map suffix in an `s` draw parameter stem.
fn cinematic_gparam(id: &str) -> Option<String> {
    let fields: Vec<_> = id.strip_prefix('m')?.split('_').collect();
    if fields.len() != 4
        || fields
            .iter()
            .any(|f| f.len() != 2 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(format!(
        "s{}_{}_{}{}",
        fields[0], fields[1], fields[2], fields[3]
    ))
}

/// The entity whose position picks the draw groups (the free camera, or the player).
#[derive(Component)]
pub struct MapFocus;

/// Marks a [`MapFocus`] that is the player: the map follows the collision part he stands on
/// and keeps it while he is off map collision, as the exe's hit collision does
/// (`world::MapWorld::stand`). Without it the focus is a free camera.
#[derive(Component)]
pub struct PlayerFocus {
    /// His physics body has ground contact (`sekiro_sim::body::Body::grounded`). The exe
    /// takes a new hit collision only from a ground contact, so in the air (a jump's apex, a
    /// grapple flight) the part stays.
    pub grounded: bool,
}

/// `cache/maps/<id>`, or an error naming the export command.
pub fn map_dir(cache: &Path, id: &str) -> Result<PathBuf> {
    cache
        .join("maps")
        .join(id)
        .canonicalize()
        .with_context(|| format!("cache/maps/{id} not found: run `sekiro-extract map {id}`"))
}

/// Adds a map to an app built with `DefaultPlugins`: its pieces, draw-group culling around the
/// [`MapFocus`] entity, a sun, ambient light, sky colour, fog and far plane on every 3D camera,
/// and the collision view. The viewer's keys (C collision, G group modes, M markers) are
/// [`run`]'s own: in play Q crouches. A connected world uses the block `stories` already loaded
/// (the play world's collision, [`world::collision_world`]) and loads the rest; a map shown
/// alone uses none.
pub fn add_map(
    app: &mut App,
    dir: &Path,
    scene: MapScene,
    mut stories: story::MapStory,
) -> Result<()> {
    let stage = |name: &str| info!("map setup: {name} at {:.1} s", crate::since_start());
    let json: Value = serde_json::from_slice(
        &std::fs::read(dir.join("layout.json")).context("reading layout.json")?,
    )?;
    let collision = match std::fs::read(dir.join("collision.bin")) {
        Ok(bytes) => Some(sekiro_formats::hknp::CollisionMesh::from_bytes(&bytes)?),
        Err(_) => None,
    };
    let mut group_parts = Vec::new();
    if let Some(c) = &collision {
        for row in json["collisions"].as_array().into_iter().flatten() {
            let Some([first, count]) = row["triangles"]
                .as_array()
                .and_then(|r| Some([r.first()?.as_u64()? as usize, r.get(1)?.as_u64()? as usize]))
            else {
                continue;
            };
            let mut min = Vec2::splat(f32::MAX);
            let mut max = Vec2::splat(f32::MIN);
            for &i in &c.indices[first * 3..(first + count) * 3] {
                let v = c.vertices[i as usize];
                min = min.min(Vec2::new(v[0], v[2]));
                max = max.max(Vec2::new(v[0], v[2]));
            }
            group_parts.push(CollisionPart {
                name: row["name"].as_str().unwrap_or_default().to_string(),
                first,
                count,
                min,
                max,
                display: groups8(&row["display_groups"]),
                draw: groups8(&row["draw_groups"]),
                light_set: row["light_set"].as_i64().unwrap_or(-1) as i32,
                fog_set: row["fog_set"].as_i64().unwrap_or(-1) as i32,
                scattering_set: row["scattering_set"].as_i64().unwrap_or(-1) as i32,
                env_map: row["env_map"].as_i64().unwrap_or(-1) as i32,
                layers: scene_layers(&row["scene_gparam"]),
            });
        }
    }
    let mode = scene.groups;
    let id = dir
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_default()
        .to_string();
    // `dir` is canonical (verbatim on Windows), so walk up instead of joining "..".
    let cache = dir
        .parent()
        .and_then(Path::parent)
        .context("map folder has no cache above it")?;
    let connected = world::connects(&id)
        && scene.within.is_none()
        && !app.world().contains_resource::<world::ArenaMap>()
        && !std::env::args().any(|a| a == "--arena");
    if connected {
        let world = world::MapWorld::load(cache, &id, &scene.prefix)?;
        stories
            .maps
            .retain(|block, _| world.blocks.contains_key(block));
        for (block, layout) in &world.blocks {
            if !stories.maps.contains_key(block)
                && let Some(state) = story::load(cache, block, &layout.json)?
            {
                stories.maps.insert(block.clone(), state);
            }
            if let Some(state) = stories.maps.get(block) {
                info!(
                    "{block}: offline story, {} parts, {} unsupported predicates",
                    state.parts.len(),
                    state.state.unsupported.len()
                );
            }
        }
        app.insert_resource(stories);
        stage("connected world and story");
        app.add_plugins(visibility::MapVisibilityPlugin)
            .insert_resource(world)
            .add_systems(
                Update,
                (world::update, world::update_detail)
                    .chain()
                    .after(track_loading)
                    .after(apply_groups),
            )
            .add_systems(
                PostUpdate,
                world::publish.in_set(visibility::VisibilityInputs),
            )
            .add_systems(
                PostUpdate,
                world::debug_hierarchy
                    .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                    .after(bevy::transform::TransformSystems::Propagate),
            );
    }
    // Older extractor output lacks FogParamID; read the installed-data copy without rewriting it.
    if group_parts
        .iter()
        .any(|p| p.fog_set < 0 || p.scattering_set < 0 || p.env_map < 0)
        && let Ok(bytes) = std::fs::read(cache.join(format!("raw/map/mapstudio/{id}.msb")))
        && let Ok(msb) = sekiro_formats::msb::parse(&bytes)
    {
        for part in &mut group_parts {
            if let Some(g) = msb
                .parts
                .iter()
                .find(|p| p.name == part.name)
                .and_then(|p| p.gparam)
            {
                part.light_set = g[0];
                part.fog_set = g[1];
                part.scattering_set = g[2];
                part.env_map = g[3];
            }
        }
    }
    app.init_resource::<crate::lighting::fog::FogSelection>();
    crate::map_fx::install(app, dir)?;
    app.init_resource::<crate::lighting_selection::RegionSelection>();
    let args: Vec<_> = std::env::args().collect();
    let arena_name = crate::arg_value(&args, "--arena");
    let arena = arena_name
        .as_deref()
        .and_then(|name| sekiro_sim::arena::ArenaSpec::by_name(name).ok());
    let story = story::selection(&args);
    let mut selection = crate::lighting_selection::select(cache, &id, story.as_deref(), arena)?;
    if scene.hour.is_finite() {
        selection.override_hour(scene.hour, args.iter().any(|a| a == "--hour"));
    }
    let hour = selection.hour;
    info!(
        "map story lighting: {id}, hour={hour}, environment={}, sub-ID={}, source={}, flags={:?}, sky={:?}",
        selection.environment, selection.sub_id, selection.source, selection.flags, selection.sky
    );
    let selected_source = selection.source.clone();
    let stem = scene.gparam.clone().unwrap_or_else(|| {
        let standard = selected_source;
        if cache
            .join(format!("raw/param/drawparam/{standard}.gparam"))
            .is_file()
        {
            standard
        } else {
            cinematic_gparam(&id)
                .filter(|s| {
                    cache
                        .join(format!("raw/param/drawparam/{s}.gparam"))
                        .is_file()
                })
                .unwrap_or(standard)
        }
    });
    let gparam_path = cache.join(format!("raw/param/drawparam/{stem}.gparam"));
    let gparam = sekiro_formats::gparam::parse(
        &std::fs::read(&gparam_path)
            .with_context(|| format!("reading {}", gparam_path.display()))?,
    )?;
    let selection = crate::lighting_selection::LightingSelection::load(
        cache,
        &id,
        selection,
        scene.gparam.is_some(),
    )?;
    let gparam = selection.resolve_initial(&stem, gparam);
    let selected_environment = selection.initial.environment;
    app.insert_resource(selection);
    crate::lighting_selection::MapStoryEvents::install(app, cache, &id, story.as_deref())?;
    app.add_systems(PostUpdate, crate::lighting_selection::sync_events);
    crate::lighting::install_post_effects(app, dir)?;
    sky::install(app, cache, &id, hour)?;
    let mut probes = MapProbes {
        prefix: scene.prefix.clone(),
        variant: selected_environment,
        ..default()
    };
    // This flag is shared by viewer and play without changing the play-mode options.
    if !std::env::args().any(|a| a == "--no-env-probe")
        && let Ok(bytes) = std::fs::read(dir.join("env.json"))
    {
        let env: Value = serde_json::from_slice(&bytes).context("reading env.json")?;
        for r in env["regions"].as_array().into_iter().flatten() {
            let probe = r["probe"].as_u64().context("probe index")? as u32;
            let vector = |key: &str| -> Result<Vec3> {
                let values: [f32; 3] = serde_json::from_value(r[key].clone())?;
                Ok(Vec3::from_array(values))
            };
            let rotation: [f32; 4] = serde_json::from_value(r["rotation"].clone())?;
            probes.regions.push(ProbeRegion {
                probe,
                position: vector("position")?,
                rotation: Quat::from_array(rotation),
                size: if r["size"].is_null() {
                    None
                } else {
                    Some(vector("size")?)
                },
            });
        }
        let exists = |probe: u32, variant: u32| {
            dir.join(format!("env/gilm{probe}_{variant:02}.dds"))
                .exists()
                && dir
                    .join(format!("env/gilm{probe}_{variant:02}_diffuse.dds"))
                    .exists()
        };
        if !probes
            .regions
            .iter()
            .any(|r| exists(r.probe, probes.variant))
        {
            warn!(
                "{id}: authored environment variant {} has no complete exported probes; no nearest-time substitution",
                probes.variant
            );
        }
        for r in &probes.regions {
            let probe = r.probe;
            if dir
                .join(format!("env/gilm{probe}_{:02}.dds", probes.variant))
                .exists()
                && dir
                    .join(format!("env/gilm{probe}_{:02}_diffuse.dds", probes.variant))
                    .exists()
            {
                probes.available.insert(probe);
            }
        }
    }
    stage("lighting selection, sky and probes");
    crate::lighting::install_irradiance(app, dir, &probes)?;
    stage("irradiance volumes");
    app.insert_resource(probes);
    let args: Vec<_> = std::env::args().collect();
    if args
        .iter()
        .any(|a| a == "--visual-stats" || a == "--profile-systems")
    {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<bevy::render::diagnostic::RenderDiagnosticsPlugin>() {
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
        }
        app.add_systems(Update, visual_stats);
    }
    if args.iter().any(|a| a == "--profile-systems") {
        app.add_plugins((
            bevy::diagnostic::EntityCountDiagnosticsPlugin::default(),
            bevy::diagnostic::SystemInformationDiagnosticsPlugin,
        ));
        crate::perf_log::install_render_probe(app);
    }
    map_material::install(app, &scene.prefix, dir, &args);
    crate::vram::install(app);
    crate::map_fog_surface::install(app, &scene.prefix);
    crate::map_grass::install(app, dir, &scene.prefix)?;
    stage("materials and grass");
    app.insert_resource(ShadowSettings::from_args(&gparam, hour, &args)?);
    LocalLights::install(app, dir, &args)?;
    app.insert_resource(ClearColor(SKY))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.80, 0.85, 1.0),
            brightness: crate::lighting::ENV_PER_UNIT,
            ..default()
        })
        .insert_resource(Layout {
            id: id.clone(),
            json,
            collision,
            scene,
        })
        .insert_resource(Groups {
            mode,
            parts: group_parts,
            current: None,
            active: None,
            checked_at: None,
            dirty: true,
        })
        .insert_resource(MapLighting {
            gparam,
            hour,
            light_set: 0,
            applied: None,
            source: stem,
        })
        .init_resource::<Pending>()
        .add_observer(map_scene_ready)
        .add_systems(Startup, spawn_map.in_set(MapSpawn))
        .add_systems(
            Update,
            (
                camera_environment,
                (stream_assets, track_loading).chain(),
                budget_map_shadows,
                update_local_lights.after(world::update),
                (pick_groups, apply_groups, apply_lighting, apply_probes).chain(),
            ),
        );
    Ok(())
}

/// Near and far planes for every new 3D camera, keeping the field of view it was spawned with;
/// lighting installs shader fog separately.
fn camera_environment(mut cameras: Query<&mut Projection, Added<Camera3d>>) {
    for mut projection in &mut cameras {
        if let Projection::Perspective(p) = &mut *projection {
            p.near = 0.1;
            p.far = 20_000.0;
        }
    }
}

const SKY: Color = Color::srgb(0.62, 0.68, 0.74);

/// GPU upload budget per frame while a map streams in. Without it the render world uploads
/// every mesh and texture that finished loading in one frame, stalling for seconds (Windows
/// then reports the window as not responding).
pub(crate) const UPLOAD_BYTES_PER_FRAME: usize = 96 << 20;

pub fn upload_budget() -> bevy::render::render_asset::RenderAssetBytesPerFrame {
    bevy::render::render_asset::RenderAssetBytesPerFrame::new(UPLOAD_BYTES_PER_FRAME)
}

/// Anisotropic filtering for map and model textures, so surfaces seen at a grazing angle (cliff
/// and rock faces) stay sharp instead of blurring to a low mip (#45). 8 is a stand-in that
/// removed the blur in the 2026-10-17 playtest; the real game's own setting is untraced.
const TEXTURE_ANISOTROPY: u16 = 8;
const _: () = assert!(TEXTURE_ANISOTROPY > 1);

/// The sampler for map and model textures: trilinear with [`TEXTURE_ANISOTROPY`] (wgpu needs
/// every filter linear once anisotropy is on). GLB textures keep their glTF wrap modes, but
/// Bevy then ignores their authored filters; the exporter writes only linear ones.
pub(crate) fn texture_sampler() -> bevy::image::ImageSamplerDescriptor {
    bevy::image::ImageSamplerDescriptor {
        anisotropy_clamp: TEXTURE_ANISOTROPY,
        ..bevy::image::ImageSamplerDescriptor::linear()
    }
}

pub fn layered_gltf_plugin() -> bevy::gltf::GltfPlugin {
    bevy::gltf::GltfPlugin {
        default_sampler: texture_sampler(),
        ..default()
    }
    // gltf-json strips the leading underscore before Bevy looks up custom semantics.
    .add_custom_vertex_attribute("MAP_BLEND2", ATTR_MAP_BLEND2)
    .add_custom_vertex_attribute(
        "MAP_COLOR1",
        bevy::mesh::MeshVertexAttribute::new(
            "MapColor1",
            0x5ec1_0a03,
            bevy::render::render_resource::VertexFormat::Float32x4,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_map(
    mut commands: Commands,
    _assets: Res<AssetServer>,
    layout: Res<Layout>,
    mut pending: ResMut<Pending>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    shadow: Res<ShadowSettings>,
    connected: Option<ResMut<world::MapWorld>>,
) {
    spawn_lights(&mut commands, &shadow);

    let scene = &layout.scene;
    let far_enabled = !std::env::args().any(|a| a == "--no-far-scenery");
    if let Some(mut connected) = connected {
        world::spawn(&mut commands, &mut connected, scene, &mut pending);
    } else if !scene.no_pieces {
        let models = &layout.json["models"];
        let mut spawned = 0;
        let kinds: &[&str] = if scene.objects {
            &["pieces", "objects"]
        } else {
            &["pieces"]
        };
        // Nearest first, so the stream loads the area around the start before the rest.
        let mut rows: Vec<(f32, &Value, Transform)> = kinds
            .iter()
            .flat_map(|k| layout.json[*k].as_array().into_iter().flatten())
            .filter_map(|row| {
                let transform = row_transform(row)?;
                let distance = match (scene.focus, row["model"].as_str()) {
                    (Some(focus), Some(model)) => {
                        placed_distance(&models[model], focus, &transform)
                    }
                    _ => 0.0,
                };
                Some((distance, row, transform))
            })
            .collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, row, transform) in rows {
            let Some(model) = row["model"].as_str() else {
                continue;
            };
            if scene.hide.iter().any(|h| model.starts_with(h.as_str())) {
                continue;
            }
            if scene
                .hide
                .iter()
                .any(|h| row["name"].as_str() == Some(h.as_str()))
            {
                continue;
            }
            let Some(glb) = models[model]["glb"].as_str() else {
                continue;
            };
            let glb = format!("{}{glb}", scene.prefix);
            if let Some((center, collision_radius)) = scene.within {
                let radius = if model.starts_with('m') && far_enabled {
                    sekiro_sim::arena::ARENA_VISUAL_RADIUS
                } else {
                    collision_radius
                };
                let inside = placed_box(&models[model], &transform).is_some_and(|(lo, hi)| {
                    sekiro_sim::arena::box_within(lo, hi, center.xz(), radius)
                });
                if !inside {
                    continue;
                }
            }
            let far_scenery = scene.within.is_some_and(|(center, radius)| {
                placed_box(&models[model], &transform).is_some_and(|(lo, hi)| {
                    !sekiro_sim::arena::box_within(lo, hi, center.xz(), radius)
                })
            });
            let mut entity = commands.spawn((DrawGroups(groups8(&row["draw_groups"])), transform));
            pending.queue(&glb, None, false, (entity.id(), 0));
            if far_scenery {
                entity.insert(FarScenery);
            }
            if follows_scene_light(row) {
                entity.insert(FollowsSceneLight);
            }
            if let Some(part) = MsbPart::from_row(&layout.id, row) {
                entity.insert(part);
            }
            spawned += 1;
        }
        info!("spawned {spawned} map piece and object instances");
    }

    // Enemy (red, dummy enemies orange) and player start (blue) markers, 1.8 m tall, facing
    // their MSB direction (the nose points along the character's -Z forward).
    let body = meshes.add(Capsule3d::new(0.3, 1.2));
    let nose = meshes.add(Cuboid::new(0.12, 0.12, 0.5));
    let mut marker_material = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * 0.3,
            ..default()
        })
    };
    let enemy = marker_material(Color::srgb(0.85, 0.12, 0.10));
    let dummy = marker_material(Color::srgb(0.95, 0.55, 0.10));
    let player = marker_material(Color::srgb(0.15, 0.35, 0.95));
    let marker_sets: &[(&str, &Handle<StandardMaterial>)] = if scene.markers {
        &[("enemies", &enemy), ("players", &player)]
    } else {
        &[]
    };
    for &(key, default_material) in marker_sets {
        for row in layout.json[key].as_array().into_iter().flatten() {
            let Some(p) = pose(row) else { continue };
            let material = if row["dummy"].as_bool() == Some(true) {
                dummy.clone()
            } else {
                default_material.clone()
            };
            commands
                .spawn((
                    Marker,
                    Transform::from_translation(p.translation).with_rotation(p.rotation),
                    Visibility::Visible,
                ))
                .with_children(|c| {
                    c.spawn((
                        Mesh3d(body.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0.0, 0.9, 0.0),
                    ));
                    c.spawn((
                        Mesh3d(nose.clone()),
                        MeshMaterial3d(material),
                        Transform::from_xyz(0.0, 1.5, -0.35),
                    ));
                });
        }
    }

    let cropped = match (&layout.collision, scene.within) {
        (Some(c), Some((center, radius))) => Some(sekiro_sim::arena::crop(c, center, radius)),
        _ => None,
    };
    if let Some(c) = cropped.as_ref().or(layout.collision.as_ref()) {
        let visibility = if scene.collision {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, c.vertices.clone());
        mesh.insert_indices(Indices::U32(c.indices.clone()));
        commands.spawn((
            CollisionView,
            Mesh3d(meshes.add(mesh)),
            Wireframe,
            WireframeColor {
                color: Color::srgb(1.0, 0.25, 0.1),
            },
            NoFrustumCulling,
            visibility,
        ));
        if scene.no_pieces {
            // Without map pieces, also draw the collision solid, flat shaded and coloured by
            // hit material, so its shape reads clearly.
            commands.spawn((
                CollisionView,
                Mesh3d(meshes.add(solid_collision(c))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.9,
                    ..default()
                })),
                NoFrustumCulling,
                visibility,
            ));
        }
        info!("collision: {} triangles", c.indices.len() / 3);
    }
}

/// Opt-in reproducible render measurements, including arena sessions.
fn visual_stats(
    pending: Res<Pending>,
    diagnostics: Res<DiagnosticsStore>,
    probe: Option<Res<crate::perf_log::RenderProbe>>,
    mut last: Local<u32>,
) {
    let seconds = pending.loaded_for() as u32;
    if seconds < 2 || seconds == *last {
        return;
    }
    *last = seconds;
    let frame = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.);
    info!("visual stats: {seconds} s after loading, frame {frame:.3} ms");
    if let Some(ref probe) = probe {
        let (max, sum, frames, ready, waiting, sets) = probe.take();
        info!(
            "profile render: mean {:.3} ms, max {max:.3} ms, pipelines {ready} ready / {waiting} waiting, sets {sets}",
            sum / frames.max(1) as f64
        );
        crate::perf_log::report_systems();
    }
    for d in diagnostics.iter().filter(|d| {
        let path = d.path().as_str();
        if probe.is_some() {
            path.ends_with("elapsed_gpu")
                || path.ends_with("elapsed_cpu")
                || path.contains("cpu_usage")
                || path == "entity_count"
        } else {
            (path.contains("game_fog") || path.contains("native_post"))
                && path.ends_with("elapsed_gpu")
        }
    }) {
        if let Some(ms) = d.smoothed() {
            info!("visual stats: {} {ms:.3} {}", d.path(), d.suffix);
        }
    }
}

/// Flat-shaded triangle soup with one colour per hit material id.
fn solid_collision(c: &sekiro_formats::hknp::CollisionMesh) -> Mesh {
    let tris = c.indices.len() / 3;
    let mut positions = Vec::with_capacity(tris * 3);
    let mut normals = Vec::with_capacity(tris * 3);
    let mut colors = Vec::with_capacity(tris * 3);
    for (t, tri) in c.indices.as_chunks::<3>().0.iter().enumerate() {
        let p = tri.map(|i| Vec3::from(c.vertices[i as usize]));
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        let m = c.materials.get(t).copied().unwrap_or(u32::MAX);
        // A stable pastel colour per material id.
        let h = m.wrapping_mul(2_654_435_761);
        let color = Color::hsl((h % 360) as f32, 0.45, 0.6)
            .to_linear()
            .to_f32_array();
        for v in p {
            positions.push(v.to_array());
            normals.push(n.to_array());
            colors.push(color);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// The four layer file indices of a collision's `SceneGparam` (-1 where absent).
fn scene_layers(scene_gparam: &Value) -> [i8; 4] {
    std::array::from_fn(|i| {
        scene_gparam["event_ids"][i]
            .as_i64()
            .and_then(|v| i8::try_from(v).ok())
            .unwrap_or(-1)
    })
}

/// Reuse the row index only while the current layout still names the selected collision there.
fn cached_index<T>(
    parts: &[T],
    cached: Option<usize>,
    matches: impl Fn(&T) -> bool,
) -> Option<usize> {
    cached
        .filter(|&i| parts.get(i).is_some_and(&matches))
        .or_else(|| parts.iter().position(matches))
}

fn selected_collision_row<'a>(
    parts: &'a [Value],
    collision: &str,
    cached: &mut Option<usize>,
) -> Option<&'a Value> {
    *cached = cached_index(parts, *cached, |p| p["name"].as_str() == Some(collision));
    cached.and_then(|i| parts.get(i))
}

/// Finds the collision part under the camera (the nearest surface below, else the nearest
/// above) and takes its groups and lighting configuration whenever the focus moves.
#[allow(clippy::too_many_arguments)]
fn pick_groups(
    layout: Res<Layout>,
    mut groups: ResMut<Groups>,
    mut lighting: ResMut<MapLighting>,
    mut fog: ResMut<crate::lighting::fog::FogSelection>,
    mut region: ResMut<crate::lighting_selection::RegionSelection>,
    mut selection: Option<ResMut<crate::lighting_selection::LightingSelection>>,
    focus: Query<&Transform, With<MapFocus>>,
    connected: Option<Res<world::MapWorld>>,
    mut selected_row: Local<Option<usize>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("pick_groups");
    if let Some(world) = connected {
        if let Some((block, collision)) = &world.selected
            && let Some(row) = world.blocks[block].json["parts"]
                .as_array()
                .and_then(|parts| selected_collision_row(parts, collision, &mut selected_row))
            && row["gparam"].is_array()
        {
            let config = gparam_config(row);
            let mut next_region = *region;
            next_region.enter(config);
            region.set_if_neq(next_region);
            if block == &world.primary
                && let Some(selection) = selection.as_mut()
            {
                crate::lighting_selection::LightingSelection::set_overlays(
                    &mut selection.reborrow(),
                    scene_layers(&row["scene_gparam"]),
                );
            }
            if config[0] >= 0 {
                crate::change::set(
                    &mut lighting.reborrow(),
                    |l| &mut l.light_set,
                    crate::lighting::forced_light_set().unwrap_or(config[0]),
                );
            }
            if config[1] >= 0 {
                crate::change::set(&mut fog.reborrow(), |f| &mut f.0, config[1]);
            }
            let current = (block == &world.primary)
                .then(|| cached_index(&groups.parts, groups.current, |p| &p.name == collision))
                .flatten();
            crate::change::set(&mut groups.reborrow(), |g| &mut g.current, current);
        }
        return;
    }
    let (Some(t), Some(c)) = (focus.iter().next(), &layout.collision) else {
        return;
    };
    let pos = t.translation;
    if groups.checked_at == Some(pos) {
        return;
    }
    groups.checked_at = Some(pos);
    let p2 = Vec2::new(pos.x, pos.z);
    let mut best: Option<(f32, usize)> = None;
    for (pi, part) in groups.parts.iter().enumerate() {
        if p2.cmplt(part.min).any() || p2.cmpgt(part.max).any() {
            continue;
        }
        let tris = &c.indices[part.first * 3..(part.first + part.count) * 3];
        for tri in tris.as_chunks::<3>().0 {
            let [a, b, cc] = tri.map(|i| Vec3::from(c.vertices[i as usize]));
            if let Some(h) = vertical_hit(pos, a, b, cc) {
                // Prefer the nearest surface below; surfaces above count only if none is below.
                let score = if h >= -0.5 { h } else { 1000.0 - h };
                if best.is_none_or(|(s, _)| score < s) {
                    best = Some((score, pi));
                }
            }
        }
    }
    let Some((_, pi)) = best else { return };
    let part = &groups.parts[pi];
    let g = match groups.mode {
        GroupMode::Display => part.display,
        _ => part.draw,
    };
    let light_set = part.light_set;
    if let Some(selection) = selection.as_mut() {
        crate::lighting_selection::LightingSelection::set_overlays(
            &mut selection.reborrow(),
            part.layers,
        );
    }
    let mut next_region = *region;
    next_region.enter([light_set, part.fog_set, part.scattering_set, part.env_map]);
    region.set_if_neq(next_region);
    if part.fog_set >= 0 {
        crate::change::set(&mut fog.reborrow(), |f| &mut f.0, part.fog_set);
    }
    crate::change::set(&mut groups.reborrow(), |g| &mut g.current, Some(pi));
    if light_set >= 0 {
        crate::change::set(
            &mut lighting.reborrow(),
            |l| &mut l.light_set,
            crate::lighting::forced_light_set().unwrap_or(light_set),
        );
    }
    // A collision with no groups leaves the previous selection in force.
    if g.iter().any(|&w| w != 0) && groups.active != Some(g) {
        groups.active = Some(g);
        groups.dirty = true;
    }
}

/// Height of `p` above the triangle (`p.y - surface y`) where the vertical line through `p`
/// crosses it.
fn vertical_hit(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (x, z) = (p.x, p.z);
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
        return None;
    }
    Some(p.y - (l1 * a.y + l2 * b.y + l3 * c.y))
}

fn apply_groups(
    mut groups: ResMut<Groups>,
    mut pieces: Query<(&DrawGroups, &mut Visibility), LegacyPieceFilter>,
    added: Query<(), Added<DrawGroups>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("apply_groups");
    if !groups.dirty && added.is_empty() {
        return;
    }
    groups.dirty = false;
    let active = match groups.mode {
        GroupMode::Off => None,
        _ => groups.active,
    };
    for (g, mut v) in &mut pieces {
        let show = active.is_none_or(|a| a.iter().zip(g.0).any(|(x, y)| x & y != 0));
        let want = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// Only map descendants are budgeted; character meshes always retain their shadows.
/// Use the world-space bounds, not the instance origin, to retain large nearby walls.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn budget_map_shadows(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<ShadowSettings>,
    focus: Query<&GlobalTransform, With<MapFocus>>,
    meshes: Query<
        (Entity, &GlobalTransform, &Aabb, Option<&NotShadowCaster>),
        (With<Mesh3d>, Without<world::AuthoredNoShadow>),
    >,
    parents: Query<&ChildOf>,
    roots: Query<(), With<DrawGroups>>,
    mut last: Local<f32>,
    mut counts: Local<Option<(usize, usize)>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("budget_map_shadows");
    let Some(caster_distance) = settings.caster_distance else {
        return;
    };
    if time.elapsed_secs() - *last < 0.5 {
        return;
    }
    *last = time.elapsed_secs();
    let Some(focus) = focus.iter().next() else {
        return;
    };
    let position = focus.translation();
    let mut total = 0;
    let mut casting = 0;
    for (entity, transform, bounds, excluded) in &meshes {
        if map_root(entity, &parents, &roots).is_none() {
            continue;
        }
        let (lo, hi) = world_bounds(transform, bounds);
        let cast = settings.enabled
            && position.distance_squared(position.clamp(lo, hi)) <= caster_distance.powi(2);
        total += 1;
        casting += usize::from(cast);
        if cast && excluded.is_some() {
            commands.entity(entity).remove::<NotShadowCaster>();
        }
        if !cast && excluded.is_none() {
            commands.entity(entity).insert(NotShadowCaster);
        }
    }
    if *counts != Some((casting, total)) {
        info!("map shadow casters: {casting}/{total} meshes within {caster_distance} m of focus");
        *counts = Some((casting, total));
    }
}

#[allow(clippy::type_complexity)]
fn map_scene_ready(
    event: On<WorldInstanceReady>,
    roots: Query<(), Or<(With<DrawGroups>, With<world::DetailWeight>)>>,
    mut pending: ResMut<Pending>,
) {
    if roots.contains(event.entity) && pending.ready_roots.insert(event.entity) {
        pending.ready += 1;
    }
}

#[allow(clippy::type_complexity)]
fn track_loading(
    assets: Res<AssetServer>,
    mut pending: ResMut<Pending>,
    instances: Res<WorldInstanceSpawner>,
    worlds: Query<(Entity, Option<&WorldInstance>), With<WorldAssetRoot>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<DrawGroups>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("track_loading");
    if pending.loaded_at.is_some() {
        return;
    }
    if pending.next < pending.stream.len() || pending.ready < pending.instances {
        return;
    }
    // A GLTF scene contains nested node worlds. The outer scene's Ready event
    // does not mean those node instances have finished creating their meshes.
    for (entity, instance) in &worlds {
        if map_root(entity, &parents, &roots).is_some()
            && instance.is_none_or(|i| !instances.instance_is_ready(**i))
        {
            return;
        }
    }
    let done = pending
        .handles
        .iter()
        .all(|h| asset_finished(&assets, h.id()));
    if done {
        info!(
            "all map assets loaded ({} instances) {:.1} s after the map was added",
            pending.instances,
            pending.added_at.elapsed().as_secs_f32()
        );
        pending.loaded_at = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cached_collision_selection_tracks_names_and_layout_changes() {
        let mut parts = vec![
            serde_json::json!({"name": "h0", "gparam": [0, 1, 2, 3]}),
            serde_json::json!({"name": "h1", "gparam": [4, 5, 6, 7]}),
        ];
        let mut cached = None;
        for name in ["h0", "h0", "h1", "missing", "h0"] {
            assert_eq!(
                selected_collision_row(&parts, name, &mut cached),
                parts.iter().find(|p| p["name"] == name),
            );
        }
        parts.swap(0, 1);
        assert_eq!(
            selected_collision_row(&parts, "h0", &mut cached),
            Some(&parts[1])
        );
        // Another block or a replaced layout can reuse the name with different configuration.
        parts[1]["gparam"] = serde_json::json!([8, 9, 10, 11]);
        assert_eq!(
            gparam_config(selected_collision_row(&parts, "h0", &mut cached).unwrap()),
            [8, 9, 10, 11],
        );
        parts.clear();
        assert!(selected_collision_row(&parts, "h0", &mut cached).is_none());
    }

    /// #46: the map's far plane must not reset a camera's field of view (it forced 60 degrees on
    /// Wolf's 43-degree follow camera).
    #[test]
    fn camera_environment_keeps_the_field_of_view() {
        let mut app = App::new();
        app.add_systems(Update, camera_environment);
        let fov = 43f32.to_radians();
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection { fov, ..default() }),
            ))
            .id();
        app.update();
        let Some(Projection::Perspective(p)) = app.world().get::<Projection>(camera) else {
            panic!("perspective projection");
        };
        assert_eq!((p.fov, p.near, p.far), (fov, 0.1, 20_000.0));
    }

    /// #45: GLB textures (rock base colour and normals) must sample with anisotropic, trilinear
    /// filtering, or faces seen at a grazing angle blur to a low mip.
    #[test]
    fn gltf_textures_sample_anisotropically() {
        use bevy::image::ImageFilterMode::Linear;
        let sampler = layered_gltf_plugin().default_sampler;
        assert_eq!(sampler.anisotropy_clamp, TEXTURE_ANISOTROPY);
        assert_eq!(
            [
                sampler.mag_filter,
                sampler.min_filter,
                sampler.mipmap_filter
            ],
            [Linear; 3]
        );
    }

    #[test]
    fn readiness_ignores_nested_worlds_and_repeated_root_events() {
        let mut app = App::new();
        app.init_resource::<Pending>().add_observer(map_scene_ready);
        let root = app.world_mut().spawn(DrawGroups([1; 8])).id();
        let nested = app.world_mut().spawn(ChildOf(root)).id();
        let unrelated = app.world_mut().spawn_empty().id();
        let detail = app.world_mut().spawn(world::DetailWeight(1.)).id();
        let instance_id = WorldInstanceSpawner::default()
            .spawn(Handle::<bevy::world_serialization::WorldAsset>::default());
        for entity in [nested, unrelated, root, root, detail] {
            app.world_mut().trigger(WorldInstanceReady {
                entity,
                instance_id,
            });
        }
        assert_eq!(app.world().resource::<Pending>().ready, 2);
    }

    #[test]
    fn gltf_loader_preserves_native_material_blend_streams() {
        let target = std::env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target".into());
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(target)
            .join(format!("tests/map-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tint_wet = [0.2f32, 0.3, 0.4, 0.5];
        let mut bin = Vec::new();
        for value in [0.0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        for _ in 0..3 {
            for value in tint_wet {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        let doc = serde_json::json!({
            "asset": {"version": "2.0"}, "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 48}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                "min": [0,0,0], "max": [1,1,0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC4"}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "_MAP_BLEND2": 1}}]}],
            "nodes": [{"mesh": 0}], "scenes": [{"nodes": [0]}], "scene": 0
        });
        let mut json = serde_json::to_vec(&doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((28 + json.len() + bin.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&bin);
        std::fs::write(dir.join("stream.glb"), glb).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: dir.to_string_lossy().into_owned(),
                ..default()
            },
            bevy::mesh::MeshPlugin,
            bevy::image::ImagePlugin::default(),
            bevy::scene::ScenePlugin,
            bevy::transform::TransformPlugin,
            layered_gltf_plugin(),
        ))
        .init_asset::<StandardMaterial>()
        .init_asset::<bevy::world_serialization::WorldAsset>();
        app.finish();
        app.cleanup();
        let handle = app
            .world()
            .resource::<AssetServer>()
            .load::<bevy::gltf::Gltf>("stream.glb");
        let started = Instant::now();
        loop {
            app.update();
            if let Some(gltf) = app
                .world()
                .resource::<Assets<bevy::gltf::Gltf>>()
                .get(&handle)
            {
                let meshes = app.world().resource::<Assets<bevy::gltf::GltfMesh>>();
                let primitive = &meshes.get(&gltf.meshes[0]).unwrap().primitives[0];
                let mesh = app
                    .world()
                    .resource::<Assets<Mesh>>()
                    .get(&primitive.mesh)
                    .unwrap();
                let Some(bevy::mesh::VertexAttributeValues::Float32x4(values)) =
                    mesh.attribute(ATTR_MAP_BLEND2)
                else {
                    panic!("native tint/wet stream lost");
                };
                assert_eq!(values, &[tint_wet; 3]);
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "GLB load timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn collision_regions_select_all_four_ids_even_with_culling_off() {
        let collision = sekiro_formats::hknp::CollisionMesh {
            vertices: vec![
                [0., 0., 0.],
                [10., 0., 0.],
                [0., 0., 10.],
                [20., 0., 0.],
                [30., 0., 0.],
                [20., 0., 10.],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            materials: vec![0, 0],
        };
        let parts = [([110, 7, 9, 12], 0.), ([0; 4], 20.)]
            .into_iter()
            .enumerate()
            .map(|(first, (config, x))| CollisionPart {
                name: format!("h{first}"),
                first,
                count: 1,
                min: Vec2::new(x, 0.),
                max: Vec2::new(x + 10., 10.),
                display: [0; 8],
                draw: [0; 8],
                light_set: config[0],
                fog_set: config[1],
                scattering_set: config[2],
                env_map: config[3],
                layers: [-1; 4],
            })
            .collect();
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(Layout {
                id: String::new(),
                json: serde_json::json!({}),
                collision: Some(collision),
                scene: MapScene {
                    prefix: String::new(),
                    collision: false,
                    no_pieces: false,
                    hide: vec![],
                    objects: false,
                    groups: GroupMode::Off,
                    markers: false,
                    hour: 6.,
                    gparam: None,
                    within: None,
                    focus: None,
                },
            })
            .insert_resource(Groups {
                mode: GroupMode::Off,
                parts,
                current: None,
                active: None,
                checked_at: None,
                dirty: false,
            })
            .insert_resource(MapLighting {
                gparam: Default::default(),
                hour: 6.,
                light_set: 0,
                applied: None,
                source: String::new(),
            })
            .init_resource::<crate::lighting::fog::FogSelection>()
            .init_resource::<crate::lighting_selection::RegionSelection>()
            .add_systems(Update, pick_groups);
        let focus = app
            .world_mut()
            .spawn((MapFocus, Transform::from_xyz(2., 1., 2.)))
            .id();
        for (x, ids) in [(2., [110, 7, 9, 12]), (22., [0; 4])] {
            app.world_mut()
                .get_mut::<Transform>(focus)
                .unwrap()
                .translation
                .x = x;
            app.update();
            assert_eq!(
                app.world()
                    .resource::<crate::lighting_selection::RegionSelection>()
                    .0,
                ids
            );
            assert_eq!(app.world().resource::<MapLighting>().light_set, ids[0]);
            assert_eq!(
                app.world()
                    .resource::<crate::lighting::fog::FogSelection>()
                    .0,
                ids[1]
            );
        }
    }

    #[test]
    fn draw_param_stems_preserve_cinematic_suffix() {
        assert_eq!(default_gparam("m11_02_00_00"), "m11_02_0000");
        assert_eq!(
            cinematic_gparam("m89_00_00_99").as_deref(),
            Some("s89_00_0099")
        );
        assert_eq!(cinematic_gparam("invalid"), None);
        assert_eq!(cinematic_gparam("m89_00_0x_99"), None);
    }

    #[test]
    fn shadow_budget_preserves_characters_and_large_walls_and_follows_focus() {
        let mut app = App::new();
        let settings = ShadowSettings::from_args(
            &sekiro_formats::gparam::Gparam { groups: vec![] },
            12.,
            &["--shadow-casters", "300"].map(String::from),
        )
        .unwrap();
        app.insert_resource(settings)
            .insert_resource(Time::<()>::default())
            .add_systems(Update, budget_map_shadows);
        let root = app.world_mut().spawn(DrawGroups([0; 8])).id();
        let focus = app
            .world_mut()
            .spawn((MapFocus, GlobalTransform::default()))
            .id();
        let bounds = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::ONE.into(),
        };
        let character = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                GlobalTransform::from_translation(Vec3::X * 400.),
                bounds,
            ))
            .id();
        let far = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                ChildOf(root),
                GlobalTransform::from_translation(Vec3::X * 400.),
                bounds,
            ))
            .id();
        let wall = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                ChildOf(root),
                GlobalTransform::from_translation(Vec3::X * 400.),
                Aabb {
                    center: Vec3::ZERO.into(),
                    half_extents: Vec3::splat(150.).into(),
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert!(app.world().get::<NotShadowCaster>(character).is_none());
        assert!(app.world().get::<NotShadowCaster>(far).is_some());
        assert!(app.world().get::<NotShadowCaster>(wall).is_none());
        *app.world_mut().get_mut::<GlobalTransform>(focus).unwrap() =
            GlobalTransform::from_translation(Vec3::X * 400.);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert!(app.world().get::<NotShadowCaster>(far).is_none());
    }
}
