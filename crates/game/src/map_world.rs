//! Bounded connected-block loading for Ashina exploration.
//!
//! Positions in layout v2 already include MSB MapOffset. No block-root transform is added.
//! The warm cache is the direct neighbourhood of the two Ashina blocks, not a recursive
//! graph walk. Demand and retirement are separate from cached asset ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use sekiro_formats::hknp::CollisionMesh;
use serde_json::{Value, json};

use super::{GroupMode, MapFocus, Pending, PlayerFocus, groups8, vertical_hit, visibility};
use bevy::world_serialization::{WorldAssetRoot, WorldInstance, WorldInstanceSpawner};

pub(crate) type Mask = [u32; 8];
const ASHINA: [&str; 2] = ["m11_00_00_00", "m11_01_00_00"];

/// Explicit bootstrap marker, including arena sessions launched from the menu.
#[derive(Resource)]
pub(crate) struct ArenaMap;

/// MSB explicitly disables sun casting for this part and its mesh descendants.
#[derive(Component)]
pub(crate) struct AuthoredNoShadow;

pub fn enabled(id: &str) -> bool {
    ASHINA.contains(&id)
}

/// Material-renderer hook: layer URIs are relative to their own model GLB, not
/// the primary map. This also preserves Bevy's named asset-source identity.
pub(crate) fn material_texture_path(
    assets: &AssetServer,
    material: &Handle<StandardMaterial>,
    uri: &str,
) -> Option<String> {
    relative_material_texture(&assets.get_path(material.id())?, uri)
}

fn relative_material_texture(path: &bevy::asset::AssetPath<'_>, uri: &str) -> Option<String> {
    if !uri.starts_with("tex/") || uri.contains("..") {
        return None;
    }
    path.without_label()
        .resolve_embed_str(uri)
        .ok()
        .map(|p| p.to_string())
}

pub(crate) fn asset_has_extension(
    server: &AssetServer,
    id: impl Into<bevy::asset::UntypedAssetId>,
    ext: &str,
) -> bool {
    server.get_path(id).is_some_and(|p| {
        p.path()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    })
}

pub(crate) fn asset_under(
    server: &AssetServer,
    id: impl Into<bevy::asset::UntypedAssetId>,
    first_dir: &str,
) -> bool {
    server.get_path(id).is_some_and(|p| {
        p.path()
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() == first_dir)
    })
}

/// Executable target matching: signed negative block selects zero; other negative
/// fields bypass equality. The all-negative sentinel requests nothing.
pub(crate) fn targets(pattern: [u8; 4], inventory: &[String]) -> Vec<String> {
    if pattern == [255; 4] {
        return Vec::new();
    }
    inventory
        .iter()
        .filter(|id| {
            let Some(fields) = id.strip_prefix('m') else {
                return false;
            };
            let fields: Vec<_> = fields
                .split('_')
                .filter_map(|v| v.parse::<u8>().ok())
                .collect();
            fields.len() == 4
                && pattern.iter().enumerate().all(|(i, &p)| {
                    if (p as i8) < 0 {
                        i != 1 || fields[i] == 0
                    } else {
                        p == fields[i]
                    }
                })
        })
        .cloned()
        .collect()
}

pub(crate) fn union(into: &mut Mask, mask: Mask) {
    for (a, b) in into.iter_mut().zip(mask) {
        *a |= b;
    }
}

/// Condition is a group index, never a story flag.
pub(crate) fn display_mask(row: &Value) -> Mask {
    let data = &row["display_group"];
    let mut mask = groups8(&data["disp_groups"]);
    if let Some(bit) = data["condition"].as_i64().filter(|b| (0..256).contains(b)) {
        mask[bit as usize / 32] |= 1 << (bit as usize % 32);
    }
    mask
}

/// Loader preprocessing, retaining the editor's field names in JSON.
pub(crate) fn effective_masks(row: &Value) -> (Mask, Mask) {
    let masks = visibility::SourceMasks::from_row(row)
        .expect("validated MSB display data")
        .effective(row["kind"].as_str().unwrap_or_default());
    (masks.draw, masks.backread)
}

/// Stable block identity on every scene root, for jobs C/E/F and material routing.
#[derive(Component, Clone, Debug)]
pub(crate) struct BlockPart {
    pub(crate) block: String,
    pub(crate) part_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestState {
    Dormant,
    Requested,
    Ready,
    Retiring,
}

/// How long a back-read record that lost its demand stays retiring before it goes (both of the
/// exe's distance sets).
pub(crate) const RETIRE_SECONDS: f32 = 3.0;

/// A back-read record's state (WorldBackRead, `docs/kb/map.md` 2026-11-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackState {
    /// Demanded within the start distance, waiting out the promote time.
    Pending = 1,
    /// Holds its block.
    Active = 2,
    /// Lost its demand or went past the end distance; still holds its block until the retire
    /// time runs out, and comes back only from within the start distance.
    Retiring = 3,
}

/// One navmesh's record in a [`BackReadSet`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct BackRecord<K> {
    pub(crate) key: K,
    pub(crate) state: BackState,
    /// Seconds in this state (a new record starts with a head start, more when nearer).
    pub(crate) timer: f32,
    /// The distance it was last demanded at.
    pub(crate) distance: f32,
    /// Demanded since the last step.
    demanded: bool,
}

/// One of the exe's two back-read distance sets: the navmeshes around the player it keeps,
/// with a promote delay going in and a retire delay going out. RSekiro's own code for the rule
/// in `docs/kb/map.md` 2026-11-04, checked against the exe by `back_read_matches_the_exe`.
#[derive(Clone, Debug)]
pub(crate) struct BackReadSet<K> {
    /// New records are made only within this distance, and a retiring one comes back only from
    /// within it.
    pub(crate) start: f32,
    /// Demand beyond this distance is ignored, and an active record past it retires.
    pub(crate) end: f32,
    pub(crate) promote: f32,
    pub(crate) retire: f32,
    /// Newest first, as the exe keeps its list (the order breaks distance ties).
    pub(crate) records: Vec<BackRecord<K>>,
}

impl<K: Copy + PartialEq> BackReadSet<K> {
    /// The set for the navmesh the player stands on.
    pub(crate) fn standing() -> Self {
        Self::new(0.0, 1.0, 0.2, RETIRE_SECONDS)
    }

    /// The set for the navmeshes around him.
    pub(crate) fn around() -> Self {
        Self::new(80.0, 90.0, 0.2, RETIRE_SECONDS)
    }

    pub(crate) fn new(start: f32, end: f32, promote: f32, retire: f32) -> Self {
        Self {
            start,
            end,
            promote,
            retire,
            records: Vec::new(),
        }
    }

    /// A navmesh `distance` from the player this frame.
    pub(crate) fn demand(&mut self, key: K, distance: f32) {
        if distance > self.end {
            return;
        }
        if let Some(r) = self.records.iter_mut().find(|r| r.key == key) {
            r.distance = distance;
            r.demanded = true;
            return;
        }
        if distance > self.start {
            return;
        }
        let share = distance / self.start.max(1.0);
        let share = if share < 0.0 { 0.0 } else { share.min(1.0) };
        self.records.insert(
            0,
            BackRecord {
                key,
                state: BackState::Pending,
                timer: (1.0 - share) * self.promote,
                distance,
                demanded: true,
            },
        );
    }

    /// One frame of `dt` seconds: every record moves on, nearest first. Returns the records
    /// that went, in that order.
    pub(crate) fn step(&mut self, dt: f32) -> Vec<K> {
        self.start = self.start.min(self.end - 1.0);
        let mut order = std::mem::take(&mut self.records);
        order.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // A pending record farther than this waits while a nearer one is pending: 5 m, or the
        // farthest record already in use before it.
        let mut wait_beyond = 5.0f32;
        let mut nearer_pending = false;
        let mut gone = Vec::new();
        for mut r in order {
            r.timer += dt;
            let demanded = std::mem::take(&mut r.demanded);
            let inside_end = r.distance <= self.end;
            let back_in = demanded && inside_end && r.distance <= self.start;
            let next = match r.state {
                BackState::Pending if !back_in => None,
                BackState::Pending
                    if r.timer < self.promote || (nearer_pending && r.distance > wait_beyond) =>
                {
                    Some(BackState::Pending)
                }
                BackState::Pending => Some(BackState::Active),
                BackState::Active if demanded && inside_end => Some(BackState::Active),
                BackState::Active => Some(BackState::Retiring),
                BackState::Retiring if back_in => Some(BackState::Active),
                BackState::Retiring if r.timer >= self.retire => None,
                BackState::Retiring => Some(BackState::Retiring),
            };
            if r.state == BackState::Pending {
                nearer_pending = true;
            } else {
                wait_beyond = wait_beyond.max(r.distance);
            }
            match next {
                None => gone.push(r.key),
                Some(state) => {
                    if state != r.state {
                        r.state = state;
                        r.timer = 0.0;
                    }
                    self.records.insert(0, r);
                }
            }
        }
        gone
    }
}

/// What a block's request-state step asks of its parts and assets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Residency {
    Keep,
    /// Neither demanded nor held by a back-read record: despawn its parts and drop its strong
    /// handles.
    Free,
    /// Demanded or held again after it was freed: queue its parts again, nearest first.
    Respawn,
}

/// Whether the block's parts are spawned.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Lifecycle {
    /// Its parts were spawned for the player (it reached Ready, or reloaded for a back-read
    /// record): only such a block is freed. Blocks pre-warmed at startup and never demanded
    /// stay warm (#271 follow-up).
    owned: bool,
    /// Its parts were despawned and its handles dropped.
    freed: bool,
}

impl Lifecycle {
    /// One frame of the request state machine: `demanded` from the masks, `held` while a
    /// back-read record of one of its collision parts is active or retiring ([`BackReadSet`]),
    /// `assets_ready` once every GLB of the block has loaded, and `can_free` while freeing is
    /// allowed (after the first full load and only in play: never in the viewer, captures or
    /// with draw groups off).
    ///
    /// The block has no timer of its own, as in the exe: it is freed once it is neither
    /// demanded nor held (its last record retired [`RETIRE_SECONDS`] after the player left its
    /// range), and a freed block loads again as soon as a record holds it, before its demand
    /// (what he can see) returns.
    pub(crate) fn step(
        &mut self,
        state: RequestState,
        demanded: bool,
        held: bool,
        assets_ready: bool,
        can_free: bool,
    ) -> (RequestState, Residency) {
        let respawn = self.freed && (demanded || held);
        if respawn {
            self.freed = false;
            self.owned = true;
        }
        if demanded {
            if respawn {
                return (RequestState::Requested, Residency::Respawn);
            }
            if assets_ready {
                self.owned = true;
                return (RequestState::Ready, Residency::Keep);
            }
            return (RequestState::Requested, Residency::Keep);
        }
        if respawn {
            return (RequestState::Dormant, Residency::Respawn);
        }
        if self.freed {
            return (RequestState::Dormant, Residency::Keep);
        }
        match state {
            RequestState::Ready | RequestState::Requested => {
                (RequestState::Retiring, Residency::Keep)
            }
            RequestState::Retiring | RequestState::Dormant if held => (state, Residency::Keep),
            RequestState::Retiring | RequestState::Dormant if self.owned && can_free => {
                self.owned = false;
                self.freed = true;
                (RequestState::Dormant, Residency::Free)
            }
            RequestState::Retiring | RequestState::Dormant => {
                (RequestState::Dormant, Residency::Keep)
            }
        }
    }
}

/// A freed block's parts were all despawned and its handles dropped, for caches that hold
/// per-block assets (grass merges, single-placement batch groups).
#[derive(Message, Clone, Debug)]
pub(crate) struct BlockFreed(pub(crate) String);

pub(crate) struct Block {
    pub(crate) id: String,
    pub(crate) prefix: String,
    pub(crate) json: Value,
    pub(crate) collision: CollisionMesh,
    pub(crate) normal: Mask,
    pub(crate) additional: Mask,
    pub(crate) state: RequestState,
    pub(crate) handles: Vec<Handle<Gltf>>,
    assets_ready: bool,
    /// Distinct GLBs this block streams; it is ready once all of them have loaded.
    glb_total: usize,
    warm_mask: Mask,
    collisions: Vec<Surface>,
    lifecycle: Lifecycle,
    detail: BTreeMap<String, [bool; 3]>,
}

struct Surface {
    name: String,
    first: usize,
    count: usize,
    min: Vec2,
    max: Vec2,
    demand: Mask,
    backread: Mask,
    /// The player stands on this part (`sekiro_sim::hit_filter::player_collides`).
    player: bool,
}

/// How far above a player-collidable part Wolf's feet may be and still stand on it. A
/// jump's apex, a fall or the wire perch over a ravine is off the map collision, and the
/// part he last stood on stays in force, as the exe keeps the character's last hit collision.
const STAND_REACH: f32 = 1.0;

/// How far the player's feet may move between two [`MapWorld::stand`] calls before it is a
/// teleport (respawn, fall return, a tour stop) that re-arms the spawn probe. Drawn motion
/// (sprints, grapple flights, falls) covers well under a metre a frame.
const TELEPORT: f32 = 10.0;

/// C/E consume the per-block demand here; F uses BlockPart and its authored row.
/// Cached GLBs stay warm for the bounded neighbourhood after demand retires.
#[derive(Resource)]
pub(crate) struct MapWorld {
    pub(crate) blocks: BTreeMap<String, Block>,
    pub(crate) primary: String,
    pub(crate) selected: Option<(String, String)>,
    /// The collision part (block, index into its surfaces) the player last stood on, and
    /// whether he really stood on it rather than it standing in before his first landing.
    standing: Option<(String, usize)>,
    stood: bool,
    /// The player's feet at the last [`Self::stand`], to tell a teleport.
    last_feet: Option<Vec3>,
    visibility_maps: BTreeMap<String, visibility::VisibilityMap>,
    requests: BTreeMap<String, visibility::BlockRequest>,
    lod: super::lod::LodTable,
    /// Residency changes [`update`] found this frame, for [`residency`].
    changes: Vec<(String, Residency)>,
    /// The exe's two back-read sets over the blocks' player collision parts (block index in
    /// `blocks`, surface index), standing in for its navmeshes: they hold blocks around the
    /// player in play ([`MapWorld::back_read`]).
    back_read: [BackReadSet<(usize, usize)>; 2],
    /// How the parts were spawned, to spawn a freed block's parts again.
    scene: Option<super::MapScene>,
}

#[derive(Component)]
pub(crate) struct DetailPart {
    state: super::lod::LodState,
    rule: super::lod::LodRule,
    scales: super::lod::LodScales,
    min: Vec3,
    max: Vec3,
    available: [bool; 3],
    slots: [Option<Entity>; 3],
    /// The model's GLB stream request (`None` for offscreen captures, which spawn every level
    /// up front), the level drawn now, and which levels have a scene instance.
    stream: Option<usize>,
    shown: usize,
    spawned: [bool; 3],
    /// Whether the slots' meshes carry crossfade coverage tags that must be cleared.
    fading: bool,
    applied: Option<DetailApplied>,
}

#[derive(PartialEq)]
struct DetailApplied {
    weights: [f32; 3],
    ready: [bool; 3],
    slots: [Option<Entity>; 3],
    stream: Option<usize>,
}

/// Submission weight retained for the eventual retail coverage shader.
#[derive(Component, PartialEq)]
pub(crate) struct DetailWeight(pub(crate) f32);

fn detail_levels(path: &Path) -> Result<[bool; 3]> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0; 20];
    file.read_exact(&mut header)?;
    anyhow::ensure!(
        &header[..4] == b"glTF" && &header[16..] == b"JSON",
        "invalid GLB header"
    );
    let size = u32::from_le_bytes(header[12..16].try_into()?) as usize;
    anyhow::ensure!(size <= 64 * 1024 * 1024, "GLB JSON exceeds metadata limit");
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes)?;
    let doc: Value = serde_json::from_slice(&bytes)?;
    if doc["extras"]["sekiro_lod_version"] != 1 {
        return Ok([true, false, false]);
    }
    Ok(std::array::from_fn(|i| {
        doc["scenes"][i]["nodes"]
            .as_array()
            .is_some_and(|n| !n.is_empty())
    }))
}

fn read_layout(dir: &Path) -> Result<Value> {
    let mut layout: Value = serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
    if layout["layout_version"].as_u64().unwrap_or(0) >= 2 {
        return Ok(layout);
    }
    // Neighbours need not be re-exported. Read their metadata without changing the cache.
    let id = dir.file_name().and_then(|v| v.to_str()).context("map ID")?;
    let cache = dir.parent().and_then(Path::parent).context("cache root")?;
    let msb = sekiro_formats::msb::parse(&std::fs::read(
        cache.join(format!("raw/map/mapstudio/{id}.msb")),
    )?)?;
    let mut parts = Vec::new();
    let mut connections = Vec::new();
    for (index, p) in msb.parts.iter().enumerate() {
        let kind = format!("{:?}", p.kind);
        let key = match kind.as_str() {
            "MapPiece" => "pieces",
            "Object" => "objects",
            "Collision" => "collisions",
            _ => "",
        };
        let mut row = layout[key]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["name"] == p.name))
            .cloned()
            .unwrap_or_else(|| json!({}));
        row["name"] = json!(p.name);
        let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let (scale, rotation, translation) =
            (mirror * p.matrix() * mirror).to_scale_rotation_translation();
        row["translation"] = json!(translation.to_array());
        row["rotation"] = json!(rotation.normalize().to_array());
        row["scale"] = json!(scale.to_array());
        row["model"] = json!(msb.model(p).map(|m| &m.name));
        row["part_index"] = json!(index);
        row["kind"] = json!(kind);
        row["entity_id"] = json!(p.entity_id);
        row["entity_group_ids"] = json!(p.entity_group_ids);
        row["lod_param_id"] = json!(p.lod_param_id);
        row["gparam"] = json!(p.gparam);
        row["display_data"] = p.display_data.as_ref().map(|d| json!({"display_groups":d.display_groups,"draw_groups":d.draw_groups,"collision_mask":d.collision_mask,"condition1":d.condition1,"condition2":d.condition2})).unwrap_or(Value::Null);
        row["display_group"] = p
            .display_group
            .as_ref()
            .map(|d| json!({"condition":d.condition,"disp_groups":d.disp_groups}))
            .unwrap_or(Value::Null);
        if let Some((ci, map_id)) = p.connect {
            row["connection"] = json!({"map_id":map_id,"source_collision":msb.collision_by_index(ci).map(|c| json!({"name":c.name}))});
            connections.push(row.clone());
        }
        parts.push(row);
    }
    layout["parts"] = json!(parts);
    layout["connections"] = json!(connections);
    Ok(layout)
}

fn map_inventory(cache: &Path) -> Result<Vec<String>> {
    let mut ids: Vec<_> = std::fs::read_dir(cache.join("maps"))?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("layout.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();
    Ok(ids)
}

fn pattern(row: &Value) -> Option<[u8; 4]> {
    serde_json::from_value(row["connection"]["map_id"].clone()).ok()
}

fn neighbourhood(cache: &Path) -> Result<BTreeMap<String, Value>> {
    let inventory = map_inventory(cache)?;
    let mut layouts = BTreeMap::new();
    let mut wanted = BTreeSet::new();
    for id in ASHINA {
        let layout = read_layout(&cache.join("maps").join(id))?;
        wanted.insert(id.to_string());
        for row in layout["connections"].as_array().into_iter().flatten() {
            // Unbound rows cannot contribute demand.
            if row["connection"]["source_collision"].is_null() {
                continue;
            }
            if let Some(p) = pattern(row) {
                wanted.extend(targets(p, &inventory));
            }
        }
        layouts.insert(id.to_string(), layout);
    }
    for id in wanted {
        if let std::collections::btree_map::Entry::Vacant(entry) = layouts.entry(id) {
            let layout = read_layout(&cache.join("maps").join(entry.key()))?;
            entry.insert(layout);
        }
    }
    Ok(layouts)
}

/// Whether `id` plays as the connected world of its neighbouring blocks rather than alone
/// (`--no-connected-blocks`).
pub fn connects(id: &str) -> bool {
    enabled(id) && !std::env::args().any(|a| a == "--no-connected-blocks")
}

/// The blocks the play world's collision holds for `id`, with their layouts: the connected
/// neighbourhood when `id` [`connects`], else `id` alone.
pub fn play_blocks(cache: &Path, id: &str) -> Result<BTreeMap<String, Value>> {
    if connects(id) {
        neighbourhood(cache)
    } else {
        Ok(BTreeMap::from([(
            id.to_string(),
            read_layout(&cache.join("maps").join(id))?,
        )]))
    }
}

/// Physics bootstrap uses the same bounded block inventory as rendering ([`play_blocks`]).
/// Collision/group records remain separate in MapWorld; only the broad-phase query
/// mesh is concatenated, without adding MapOffset or translating any vertex.
/// Each block's mesh is the low-resolution `l` collision characters and the camera collide
/// with (`sekiro_sim::lo_collision`, #189), or its `collision.bin` when it has none (logged).
/// Collision parts and placed objects the block's `story` switches off are left out.
/// Helper parts the player does not collide with (`sekiro_sim::hit_filter`) leave the
/// walkable mesh and join the world as camera-only triangles or event walls.
pub fn collision_world(
    cache: &Path,
    layouts: &BTreeMap<String, Value>,
    story: &super::story::MapStory,
) -> Result<sekiro_sim::collision::CollisionWorld> {
    collision_world_switched(cache, layouts, story, &Switchable::default())
}

/// The collision the map events switch at run time (`src/enemy/world.rs` runs the blocks'
/// events): collision parts their 2011[1] / [4] name and placed objects they break,
/// (de)activate or open (2005[1] / [2] / [3] / [7], the MSB's ObjActs), by MSB entity id or an
/// entity group the part is in. [`collision_world_switched`] adds these as event parts
/// (`CollisionWorld::append_event_part`) keyed by that id, on as the story starts them, for
/// `CollisionWorld::set_event_part` to switch.
#[derive(Clone, Debug, Default)]
pub struct Switchable {
    pub collisions: BTreeSet<i32>,
    pub objects: BTreeSet<i32>,
}

/// The id a layout row is switched by: its entity id, else the first of its entity groups in
/// `ids`.
fn switch_key(row: &Value, ids: &BTreeSet<i32>) -> Option<i32> {
    let own = row["entity_id"].as_i64().unwrap_or(-1) as i32;
    if own > 0 && ids.contains(&own) {
        return Some(own);
    }
    groups8(&row["entity_group_ids"])
        .into_iter()
        .map(|g| g as i32)
        .find(|g| *g > 0 && ids.contains(g))
}

/// Appends triangles `first..first + count` of `source` to `out` (three vertices each),
/// skipping collapsed ones.
fn sub_mesh(source: &CollisionMesh, first: usize, count: usize, out: &mut CollisionMesh) {
    let end = (first + count).min(source.indices.len() / 3);
    for t in first.min(end)..end {
        let tri = &source.indices[t * 3..t * 3 + 3];
        if tri[0] == tri[1] && tri[1] == tri[2] {
            continue;
        }
        let Some(corners) = tri
            .iter()
            .map(|&i| source.vertices.get(i as usize).copied())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let base = out.vertices.len() as u32;
        out.vertices.extend(corners);
        out.indices.extend([base, base + 1, base + 2]);
        out.materials
            .push(source.materials.get(t).copied().unwrap_or(u32::MAX));
    }
}

/// Whether a collision part with this hit filter can be an event part: one bodies or the
/// camera collide with, not a swim hit or an event wall (those keep their own handling).
fn switchable_filter(filter: u8) -> bool {
    use sekiro_sim::hit_filter as hf;
    filter != hf::SWIM_FILTER
        && !hf::is_event_wall(filter)
        && hf::mask(filter) & (hf::PLAYER | hf::ENEMY | hf::CAMERA) != 0
}

/// [`collision_world`] with the map events' [`Switchable`] collision as event parts. gap: the
/// fall-prevention walls of a switched part or object stay as the story left them.
pub fn collision_world_switched(
    cache: &Path,
    layouts: &BTreeMap<String, Value>,
    story: &super::story::MapStory,
    switch: &Switchable,
) -> Result<sekiro_sim::collision::CollisionWorld> {
    // (switch id, mesh, who collides with it, on at the start)
    let mut events: Vec<(i32, CollisionMesh, u8, bool)> = Vec::new();
    let mut mesh = CollisionMesh::default();
    let mut helpers = sekiro_sim::hit_filter::HelperCollision::default();
    let mut story_off = 0;
    for (block, layout) in layouts {
        let dir = cache.join("maps").join(block);
        // A block exports `collision_lo.bin` beside `collision.bin`, never alone.
        if dir.join(sekiro_sim::lo_collision::HIGH_FILE).is_file() {
            let story = story.maps.get(block);
            // The parts the events switch leave the static mesh and come back as event parts.
            let switched: BTreeMap<&str, (i32, u8)> = layout["collisions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let key = switch_key(row, &switch.collisions)?;
                    let filter = sekiro_sim::hit_filter::row_filter(row)?;
                    let name = row["name"].as_str()?;
                    switchable_filter(filter)
                        .then(|| (name, (key, sekiro_sim::hit_filter::mask(filter))))
                })
                .collect();
            let part = sekiro_sim::lo_collision::block_collision(&dir, layout, |n| {
                story.is_some_and(|s| s.switched_off(n)) || switched.contains_key(n)
            })
            .map_err(anyhow::Error::msg)?;
            if !switched.is_empty() {
                let lo =
                    sekiro_sim::lo_collision::read(&dir, layout).map_err(anyhow::Error::msg)?;
                for row in lo.rows["collisions"].as_array().into_iter().flatten() {
                    let Some(name) = row["name"].as_str() else {
                        continue;
                    };
                    let Some(&(key, who)) = switched.get(name) else {
                        continue;
                    };
                    let (Some(first), Some(count)) =
                        (row["triangles"][0].as_u64(), row["triangles"][1].as_u64())
                    else {
                        continue;
                    };
                    let mut part_mesh = CollisionMesh::default();
                    sub_mesh(&lo.mesh, first as usize, count as usize, &mut part_mesh);
                    let on = !story.is_some_and(|s| s.switched_off(name));
                    events.push((key, part_mesh, who, on));
                }
            }
            story_off += part.story_off;
            helpers.extend(part.helpers);
            mesh.extend(&part.mesh);
        }
    }
    eprintln!(
        "collision: {story_off} parts switched off by the story, helper triangles kept off the player by hit filter {:?}, {} event walls",
        helpers.removed,
        helpers.triggers.len()
    );
    let mut world = sekiro_sim::collision::CollisionWorld::from_mesh(&mesh);
    world.add_helpers(&helpers);
    for block in layouts.keys() {
        // Objects the story has switched off (`DeactivateObject`) have no collision either.
        // The fall-prevention walls of the groups the block's events hand to common event
        // 20005560 are on only while Wolf has SpEffect 4100 (stands on a rideable object),
        // whatever the story says: they hold only riders (#119).
        let story = story.maps.get(block);
        // The fall-prevention assist walls of the block's collision parts, those of parts the
        // story switches off left out with them (#149).
        let walls = world
            .add_map_fall_assist(&cache.join("maps").join(block), |n| {
                story.is_none_or(|s| !s.switched_off(n))
            })
            .map_err(anyhow::Error::msg)?;
        // Only a block whose `f` binder was unpacked but never exported is missing walls; one
        // without the binder has none, nor do parts the story has switched off.
        let binder = cache.join(format!("raw/map/{block}/f{}.hkxbhd", &block[1..]));
        if walls.is_none() && binder.is_file() {
            eprintln!(
                "collision: {block}: no fall-assist walls (re-export with `sekiro-extract map \
                 {block} --fall-assist`)"
            );
        }
        let groups =
            sekiro_sim::hit_filter::load_ride_wall_groups(cache, block).unwrap_or_else(|e| {
                eprintln!("collision: {block}: no ride wall groups: {e}");
                Vec::new()
            });
        let riders = ride_walls(&layouts[block], &groups);
        let mut switched_off = 0;
        // The objects the events switch, by name: their switch id.
        let object_keys: BTreeMap<&str, i32> = layouts[block]["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| Some((row["name"].as_str()?, switch_key(row, &switch.objects)?)))
            .collect();
        let mut object_events: Vec<(String, i32, u8, bool)> = Vec::new();
        let (parts, triangles) = world
            .add_block_objects(&cache.join("maps").join(block), block, |part, who| {
                if riders.contains(part.name.as_str()) {
                    return sekiro_sim::hit_filter::ride_mask(who);
                }
                let on = story
                    .and_then(|s| s.part(&part.name))
                    .is_none_or(|p| p.enabled);
                if let Some(&key) = object_keys.get(part.name.as_str()) {
                    object_events.push((part.name.clone(), key, who, on));
                    return 0;
                }
                switched_off += usize::from(!on);
                if on { who } else { 0 }
            })
            .map_err(anyhow::Error::msg)?;
        if !object_events.is_empty() {
            let path = cache.join("maps").join(block).join("object_collision.bin");
            let bytes = std::fs::read(&path).with_context(|| path.display().to_string())?;
            let objects = sekiro_formats::hknp::ObjectCollision::from_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
            for (name, key, who, on) in object_events {
                let Some(p) = objects.parts.iter().find(|p| p.name == name) else {
                    continue;
                };
                let mut part_mesh = CollisionMesh::default();
                sub_mesh(
                    &objects.mesh,
                    p.first as usize,
                    p.count as usize,
                    &mut part_mesh,
                );
                events.push((key, part_mesh, who, on));
            }
        }
        if parts > 0 {
            eprintln!(
                "collision: {block}: {parts} placed objects with collision, {triangles} \
                 triangles ({switched_off} switched off by the story)"
            );
        }
    }
    let event_parts = events.len();
    let mut event_triangles = 0;
    for (key, part_mesh, who, on) in events {
        event_triangles += part_mesh.triangle_count();
        if part_mesh.triangle_count() > 0 {
            world.append_event_part(key, &part_mesh, who, on);
        }
    }
    if event_parts > 0 {
        eprintln!(
            "collision: {event_parts} collision parts and objects switched by the map events, \
             {event_triangles} triangles"
        );
    }
    Ok(world)
}

/// Names of a block's objects in any of `groups`, the entity groups whose fall-prevention
/// walls common event 20005560 switches on while Wolf has SpEffect 4100
/// ([`sekiro_sim::hit_filter::ride_wall_groups`], [`sekiro_sim::hit_filter::RIDE_ASSIST_MOVE`]).
fn ride_walls<'a>(layout: &'a Value, groups: &[u32]) -> BTreeSet<&'a str> {
    layout["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            groups8(&row["entity_group_ids"])
                .iter()
                .any(|g| *g != 0 && groups.contains(g))
        })
        .filter_map(|row| row["name"].as_str())
        .collect()
}

/// A block's asset path prefix: the primary block's, with its map id swapped for the block's
/// (`maps/<id>/`, or `<source>://maps/<id>/` when a host loads the cache as a named asset
/// source), or `../<id>/` beside an unprefixed primary.
pub(crate) fn block_prefix(prefix: &str, primary: &str, id: &str) -> String {
    if id == primary {
        prefix.to_string()
    } else if prefix.is_empty() {
        format!("../{id}/")
    } else if let Some(base) = prefix.strip_suffix(&format!("{primary}/")) {
        format!("{base}{id}/")
    } else {
        format!("maps/{id}/")
    }
}

impl MapWorld {
    pub(crate) fn load(cache: &Path, primary: &str, prefix: &str) -> Result<Self> {
        let mut blocks = BTreeMap::new();
        for (id, json) in neighbourhood(cache)? {
            let dir = cache.join("maps").join(&id);
            // The draw groups follow the part the player's capsule stands on, so the parts are
            // read from the `l` mesh it collides with (#189), each part's range from
            // `collision_lo.bin`; a block without one falls back to `collision.bin` and the
            // layout's ranges ([`sekiro_sim::lo_collision::read`]).
            let (collision, low_ranges) =
                match sekiro_sim::lo_collision::read_low(&dir).map_err(anyhow::Error::msg)? {
                    Some(low) => {
                        let ranges: BTreeMap<String, (usize, usize)> = low
                            .parts
                            .iter()
                            .map(|p| (p.name.clone(), (p.first as usize, p.count as usize)))
                            .collect();
                        (low.mesh, Some(ranges))
                    }
                    None => {
                        let high = dir.join(sekiro_sim::lo_collision::HIGH_FILE);
                        let mesh = if high.is_file() {
                            CollisionMesh::from_bytes(&std::fs::read(high)?)?
                        } else {
                            CollisionMesh::default()
                        };
                        (mesh, None)
                    }
                };
            let mut collisions = Vec::new();
            for row in json["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["kind"] == "Collision")
            {
                let (first, count) = match &low_ranges {
                    // A part with no `l` model is met by no character (an empty range).
                    Some(ranges) => row["name"]
                        .as_str()
                        .and_then(|n| ranges.get(n))
                        .copied()
                        .unwrap_or((0, 0)),
                    None => {
                        let Some(range) = row["triangles"].as_array() else {
                            continue;
                        };
                        let first = range.first().and_then(Value::as_u64).unwrap_or(0) as usize;
                        let count = range.get(1).and_then(Value::as_u64).unwrap_or(0) as usize;
                        (first, count)
                    }
                };
                let Some(indices) = collision.indices.get(first * 3..(first + count) * 3) else {
                    anyhow::bail!("{id}: collision triangle range is invalid");
                };
                if indices.is_empty() {
                    continue;
                }
                let mut min = Vec2::splat(f32::MAX);
                let mut max = Vec2::splat(f32::MIN);
                for &i in indices {
                    let v = collision
                        .vertices
                        .get(i as usize)
                        .context("collision vertex index")?;
                    min = min.min(Vec2::new(v[0], v[2]));
                    max = max.max(Vec2::new(v[0], v[2]));
                }
                let (_, backread) = effective_masks(row);
                collisions.push(Surface {
                    name: row["name"].as_str().unwrap_or_default().to_string(),
                    first,
                    count,
                    min,
                    max,
                    demand: display_mask(row),
                    backread,
                    // Rows without a hit filter are ordinary map collision.
                    player: sekiro_sim::hit_filter::row_filter(row)
                        .is_none_or(sekiro_sim::hit_filter::player_collides),
                });
            }
            let block_prefix = block_prefix(prefix, primary, &id);
            let mut detail = BTreeMap::new();
            if enabled(&id) {
                for (model, entry) in json["models"].as_object().context("models table")? {
                    if model.starts_with('m')
                        && let Some(glb) = entry["glb"].as_str()
                    {
                        detail.insert(model.clone(), detail_levels(&dir.join(glb))?);
                    }
                }
            }
            blocks.insert(
                id.clone(),
                Block {
                    id,
                    prefix: block_prefix,
                    json,
                    collision,
                    collisions,
                    normal: [0; 8],
                    additional: [0; 8],
                    state: RequestState::Dormant,
                    handles: Vec::new(),
                    assets_ready: false,
                    glb_total: 0,
                    warm_mask: [0; 8],
                    lifecycle: Lifecycle::default(),
                    detail,
                },
            );
        }
        info!(
            "connected world {primary}: bounded warm blocks {:?}",
            blocks.keys().collect::<Vec<_>>()
        );
        let mut visibility_maps = BTreeMap::new();
        for (id, block) in &blocks {
            if let Some(map) = visibility::VisibilityMap::from_layout(id, &block.json)? {
                visibility_maps.insert(id.clone(), map);
            }
        }
        let inventory: Vec<_> = blocks.keys().cloned().collect();
        let mut warm_masks = BTreeMap::<String, Mask>::new();
        for id in ASHINA {
            for row in blocks[id].json["connections"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if row["connection"]["source_collision"].is_null() {
                    continue;
                }
                if let Some(p) = pattern(row) {
                    for target in targets(p, &inventory) {
                        union(warm_masks.entry(target).or_default(), display_mask(row));
                    }
                }
            }
        }
        for (id, block) in &mut blocks {
            block.warm_mask = if enabled(id) {
                [u32::MAX; 8]
            } else {
                warm_masks.get(id).copied().unwrap_or_default()
            };
        }
        let lod = super::lod::LodTable::load(cache, 6)?;
        for (id, block) in &blocks {
            if !enabled(id) {
                continue;
            }
            for row in block.json["parts"].as_array().context("canonical parts")? {
                if matches!(row["kind"].as_str(), Some("MapPiece" | "Object")) {
                    visibility::VisibilityPart::from_row(id, row)?;
                    if row["kind"] == "MapPiece" {
                        lod.map_piece(
                            row["lod_param_id"].as_u64().context("authored LOD ID")? as u8
                        )?;
                    }
                }
            }
        }
        Ok(Self {
            blocks,
            primary: primary.to_string(),
            selected: None,
            standing: None,
            stood: false,
            last_feet: None,
            visibility_maps,
            requests: BTreeMap::new(),
            lod,
            changes: Vec::new(),
            back_read: [BackReadSet::standing(), BackReadSet::around()],
            scene: None,
        })
    }

    /// Whether `surface` of block `id` is switched off by the story.
    fn disabled(story: Option<&super::story::MapStory>, id: &str, surface: &Surface) -> bool {
        story
            .and_then(|s| s.maps.get(id))
            .and_then(|s| s.part(&surface.name))
            .is_some_and(|p| !p.enabled)
    }

    /// Every collision part crossing the vertical line through `pos`, scored by the nearest
    /// surface below (or, failing that, above), with the height of `pos` over that surface.
    /// `player` keeps only the parts the player collides with.
    fn hits(
        &self,
        pos: Vec3,
        story: Option<&super::story::MapStory>,
        player: bool,
    ) -> Vec<(f32, f32, String, usize)> {
        let mut hits = Vec::new();
        for (id, block) in &self.blocks {
            for (index, surface) in block.collisions.iter().enumerate() {
                if (player && !surface.player) || Self::disabled(story, id, surface) {
                    continue;
                }
                if pos.xz().cmplt(surface.min).any() || pos.xz().cmpgt(surface.max).any() {
                    continue;
                }
                let best = block.collision.indices
                    [surface.first * 3..(surface.first + surface.count) * 3]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|tri| {
                        let [a, b, c] =
                            tri.map(|i| Vec3::from(block.collision.vertices[i as usize]));
                        vertical_hit(pos, a, b, c)
                    })
                    .map(|h| (if h >= -0.5 { h } else { 1000.0 - h }, h))
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((score, height)) = best {
                    hits.push((score, height, id.clone(), index));
                }
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        hits
    }

    /// Camera-driven demand for the map viewer and fixed captures, where the focus is a free
    /// camera rather than a character: every part under each position, with a 0.5 m tie band
    /// so a camera over a seam takes both sides.
    pub(crate) fn demand(&mut self, positions: &[Vec3], story: Option<&super::story::MapStory>) {
        let mut sources = Vec::new();
        for &pos in positions {
            let hits = self.hits(pos, story, false);
            let Some(&(best, ..)) = hits.first() else {
                continue;
            };
            sources.extend(
                hits.into_iter()
                    .take_while(|h| h.0 <= best + 0.5)
                    .map(|(_, _, id, index)| (id, index)),
            );
        }
        self.apply(&sources, story);
    }

    /// Play demand from the collision part the player stands on (`docs/MAP-COMPLETENESS.md`,
    /// "The player's hit collision selects the map"): with ground contact (`grounded`), the
    /// nearest player-collidable surface below his feet within [`STAND_REACH`], or one at most
    /// 0.5 m above them. In the air, or grounded off map collision (the wire perch), the last
    /// part stays. Until he first stands on one, the nearest player-collidable part below him
    /// stands in, like the exe's spawn cast, and is re-probed as he moves (the character is
    /// placed after the first frames). A jump of more than [`TELEPORT`] in one call (respawn,
    /// fall return, a tour stop) re-arms that probe, as the exe re-runs its cast on map entry.
    /// The camera never takes part. Returns whether the selection changed.
    pub(crate) fn stand(
        &mut self,
        feet: Vec3,
        grounded: bool,
        story: Option<&super::story::MapStory>,
    ) -> bool {
        if self
            .last_feet
            .is_some_and(|last| last.distance(feet) > TELEPORT)
        {
            self.stood = false;
        }
        self.last_feet = Some(feet);
        if self.stood && !grounded {
            return false;
        }
        // Hits are sorted nearest below first, then above, so only the first can be in reach.
        let Some((_, height, id, index)) = self.hits(feet, story, true).into_iter().next() else {
            return false;
        };
        let found = if grounded && (-0.5..=STAND_REACH).contains(&height) {
            self.stood = true;
            Some((id, index))
        } else if !self.stood && height >= -0.5 {
            Some((id, index))
        } else {
            None
        };
        if found.is_none() || found == self.standing {
            return false;
        }
        self.standing = found;
        let sources: Vec<_> = self.standing.iter().cloned().collect();
        self.apply(&sources, story);
        true
    }

    /// Recalculate all masks from the participating collision parts (block, surface index),
    /// so stale connection demand cannot leak across blocks. The first part selects the
    /// lighting region.
    fn apply(&mut self, sources: &[(String, usize)], story: Option<&super::story::MapStory>) {
        for block in self.blocks.values_mut() {
            block.normal = [0; 8];
            block.additional = [0; 8];
        }
        let inventory: Vec<_> = self.blocks.keys().cloned().collect();
        let mut contributions = Vec::new();
        let mut participating = BTreeMap::<String, BTreeSet<String>>::new();
        self.selected = None;
        for (id, index) in sources {
            let block = &self.blocks[id];
            let surface = &block.collisions[*index];
            participating
                .entry(id.clone())
                .or_default()
                .insert(surface.name.clone());
            if self.selected.is_none() {
                self.selected = Some((id.clone(), surface.name.clone()));
            }
            contributions.push((id.clone(), surface.demand, surface.backread));
            for row in block.json["connections"].as_array().into_iter().flatten() {
                if row["connection"]["source_collision"]["name"].as_str() != Some(&surface.name) {
                    continue;
                }
                if let Some(p) = pattern(row) {
                    for target in targets(p, &inventory) {
                        contributions.push((target, display_mask(row), [0; 8]));
                    }
                }
            }
        }
        for (id, normal, additional) in contributions {
            let block = self.blocks.get_mut(&id).expect("inventory target");
            union(&mut block.normal, normal);
            union(&mut block.additional, additional);
        }
        self.requests.clear();
        for (id, names) in participating {
            if let Some(map) = self.visibility_maps.get(&id) {
                for (target, request) in map.requests(
                    names.iter().map(String::as_str),
                    |source, _| {
                        !story
                            .and_then(|s| s.maps.get(&id))
                            .and_then(|s| s.part(source))
                            .is_some_and(|p| !p.enabled)
                    },
                    |target| self.blocks.contains_key(target),
                ) {
                    self.requests.entry(target).or_default().merge(request);
                }
            }
        }
    }
}

impl MapWorld {
    /// One frame of the exe's back-read demand (WorldBackRead, `docs/kb/map.md` 2026-11-04):
    /// every player collision part within a set's end distance of `feet` (horizontal distance
    /// to the part's bounds) is demanded in both sets, then both step `dt`. Returns which blocks
    /// a record holds, by index in `blocks`.
    fn back_read(
        &mut self,
        feet: Vec3,
        dt: f32,
        story: Option<&super::story::MapStory>,
    ) -> Vec<bool> {
        let reach = self.back_read.iter().map(|s| s.end).fold(0.0, f32::max);
        for (b, (id, block)) in self.blocks.iter().enumerate() {
            for (i, surface) in block.collisions.iter().enumerate() {
                if !surface.player || Self::disabled(story, id, surface) {
                    continue;
                }
                let distance = (feet.xz().clamp(surface.min, surface.max) - feet.xz()).length();
                if distance <= reach {
                    for set in &mut self.back_read {
                        set.demand((b, i), distance);
                    }
                }
            }
        }
        let mut held = vec![false; self.blocks.len()];
        for set in &mut self.back_read {
            set.step(dt);
            for r in &set.records {
                if r.state != BackState::Pending {
                    held[r.key.0] = true;
                }
            }
        }
        held
    }
}

/// Map demand from the [`MapFocus`]: the collision part a [`PlayerFocus`] stands on (kept
/// while he is off map collision), else the parts under the free camera. Other cameras never
/// take part: in play the orbit camera swinging out over a ravine must not retire the map.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut world: ResMut<MapWorld>,
    assets: Res<AssetServer>,
    time: Res<Time>,
    pending: Res<Pending>,
    groups: Res<super::Groups>,
    focus: Query<(&Transform, Option<&PlayerFocus>), With<MapFocus>>,
    mut last: Local<Option<Vec<(Vec3, bool)>>>,
    story: Res<super::story::MapStory>,
) {
    let positions: Vec<_> = focus
        .iter()
        .map(|(t, player)| (t.translation, player.is_some_and(|p| p.grounded)))
        .collect();
    if last.as_ref() != Some(&positions) {
        match focus.iter().find_map(|(t, player)| Some((t, player?))) {
            Some((feet, player)) => {
                world.stand(feet.translation, player.grounded, Some(&story));
            }
            None => {
                let cameras: Vec<_> = positions.iter().map(|&(p, _)| p).collect();
                world.demand(&cameras, Some(&story));
            }
        }
        *last = Some(positions);
    }
    // Only play frees blocks: the viewer, sweeps and render gates keep everything they loaded.
    let playing = focus.iter().any(|(_, player)| player.is_some());
    let can_free = playing && pending.loaded() && groups.mode != GroupMode::Off;
    let delta = time.delta_secs();
    // Blocks around the player stay resident by the exe's back-read rule; only play keeps its
    // records.
    let held = match focus
        .iter()
        .find_map(|(t, player)| player.map(|_| t.translation))
    {
        Some(feet) => world.back_read(feet, delta, Some(&story)),
        None => vec![false; world.blocks.len()],
    };
    let world = &mut *world;
    for (block, &held) in world.blocks.values_mut().zip(&held) {
        // Streaming requests a block's GLBs over time; it is ready once every one has loaded,
        // with its textures.
        if !block.assets_ready
            && block.handles.len() >= block.glb_total
            && block
                .handles
                .iter()
                .all(|h| assets.is_loaded_with_dependencies(h.id()))
        {
            block.assets_ready = true;
            info!(
                "block {} assets ready: {} GLBs with dependencies, {:?}",
                block.id, block.glb_total, block.state
            );
        }
        let demanded = block.normal != [0; 8] || block.additional != [0; 8];
        let (next, change) =
            block
                .lifecycle
                .step(block.state, demanded, held, block.assets_ready, can_free);
        if change != Residency::Keep {
            world.changes.push((block.id.clone(), change));
        }
        if next != block.state {
            info!(
                "block {} {:?} -> {:?}, normal={:08x?}, additional={:08x?}",
                block.id, block.state, next, block.normal, block.additional
            );
            block.state = next;
        }
    }
}

pub(crate) fn publish(
    world: Res<MapWorld>,
    groups: Res<super::Groups>,
    mut blocks: ResMut<visibility::VisibilityBlocks>,
    story: Res<super::story::MapStory>,
    mut authored: ResMut<visibility::AuthoredVisibility>,
) {
    if story.is_changed() {
        let next = story
            .maps
            .iter()
            .map(|(id, map)| {
                (
                    id.clone(),
                    visibility::EventState {
                        by_name: map
                            .parts
                            .iter()
                            .map(|part| {
                                (
                                    part.name.clone(),
                                    map.enabled(part.entity_id, &part.name).unwrap_or(true),
                                )
                            })
                            .collect(),
                        by_entity: BTreeMap::new(),
                    },
                )
            })
            .collect();
        crate::change::set(&mut authored.reborrow(), |a| &mut a.maps, next);
    }
    if !world.is_changed() && !groups.is_changed() && !blocks.is_added() {
        return;
    }
    for (id, block) in &world.blocks {
        let empty = visibility::BlockRequest::default();
        let request = world.requests.get(id).unwrap_or(&empty);
        let mut normal = request.normal;
        let mut additional_backread = request.additional_backread;
        union(&mut normal, block.normal);
        union(&mut additional_backread, block.additional);
        let groups_off = groups.mode == GroupMode::Off;
        let resident =
            matches!(block.state, RequestState::Ready | RequestState::Requested) || groups_off;
        // Streaming can mark MapWorld changed without changing this block's visibility.
        // Compare the source list by reference before allocating a new snapshot.
        if blocks.0.get(id).is_some_and(|current| {
            current.request.normal == normal
                && current.request.additional_backread == additional_backread
                && current.request.sources == request.sources
                && current.draw == block.normal
                && current.resident == resident
                && current.ready
                && current.groups_off == groups_off
        }) {
            continue;
        }
        let next = visibility::BlockVisibility {
            request: visibility::BlockRequest {
                normal,
                additional_backread,
                sources: request.sources.clone(),
            },
            draw: block.normal,
            // A demanded block draws while it streams: each part appears once its own
            // scene has spawned, so the area around the player never waits for the rest.
            resident,
            ready: true,
            groups_off,
        };
        crate::change::insert_if_neq(
            blocks.reborrow().map_unchanged(|b| &mut b.0),
            id.clone(),
            next,
        );
    }
}

/// Instantiate eligible canonical rows once; geometry stays warm while its request
/// state changes. Jobs C/E/F attach state through BlockPart without duplicate roots.
///
/// Interactive worlds queue each row's GLB nearest-first from `scene.focus` and stream them
/// ([`super::stream_assets`]); a MapPiece instantiates only the level of detail it shows at
/// the start, and [`update_detail`] adds another level the first time it is selected.
pub(crate) fn spawn(
    commands: &mut Commands,
    world: &mut MapWorld,
    scene: &super::MapScene,
    pending: &mut Pending,
) {
    world.scene = Some(scene.clone());
    spawn_blocks(commands, world, scene, pending, None, scene.focus);
}

/// [`spawn`] for every block, or `only` one block (a freed block requested again), queued
/// nearest-first from `focus`.
fn spawn_blocks(
    commands: &mut Commands,
    world: &mut MapWorld,
    scene: &super::MapScene,
    pending: &mut Pending,
    only: Option<&str>,
    focus: Option<Vec3>,
) {
    if scene.no_pieces {
        return;
    }
    let mut spawned = 0;
    let args: Vec<_> = std::env::args().collect();
    let only_part = crate::arg_value(&args, "--world-part");
    // Nearest first across every block, so the stream loads outwards from the start.
    let mut order = Vec::new();
    for (id, block) in &world.blocks {
        if only.is_some_and(|only| only != id) {
            continue;
        }
        for (index, row) in block.json["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(transform) = super::row_transform(row) else {
                continue;
            };
            let distance = match (focus, row["model"].as_str()) {
                (Some(focus), Some(model)) => {
                    super::placed_distance(&block.json["models"][model], focus, &transform)
                }
                _ => 0.0,
            };
            order.push((distance, id.clone(), index, transform));
        }
    }
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut seen = BTreeMap::<String, BTreeSet<String>>::new();
    let mut skipped = BTreeMap::<String, BTreeMap<String, usize>>::new();
    for (_, id, index, transform) in order {
        let block = world.blocks.get_mut(&id).expect("listed block");
        let row = &block.json["parts"][index];
        if only_part
            .as_ref()
            .is_some_and(|name| row["name"].as_str() != Some(name))
        {
            continue;
        }
        if row["kind"] != "MapPiece" && row["kind"] != "Object" {
            continue;
        }
        let Some(model) = row["model"].as_str() else {
            continue;
        };
        let reason = row["render_status"]["reason"].as_str().unwrap_or("legacy");
        if !matches!(reason, "exported" | "legacy_zero_draw_groups" | "legacy") {
            *skipped
                .entry(block.id.clone())
                .or_default()
                .entry(reason.to_string())
                .or_default() += 1;
            continue;
        }
        if scene
            .hide
            .iter()
            .any(|h| model.starts_with(h) || row["name"].as_str() == Some(h))
        {
            continue;
        }
        let Some(glb) = block.json["models"][model]["glb"].as_str() else {
            continue;
        };
        let (draw, backread) = effective_masks(row);
        if !enabled(&block.id)
            && !draw
                .into_iter()
                .zip(block.warm_mask)
                .any(|(a, b)| a & b != 0)
            && !backread
                .into_iter()
                .zip(block.warm_mask)
                .any(|(a, b)| a & b != 0)
        {
            continue;
        }
        let glb = format!("{}{glb}", block.prefix);
        seen.entry(block.id.clone())
            .or_default()
            .insert(glb.clone());
        let source = match visibility::SourceMasks::from_row(row) {
            Ok(source) => source,
            Err(error) => {
                error!("{} {}: {error:#}", block.id, row["name"]);
                continue;
            }
        };
        let part_visibility = visibility::VisibilityPart::from_row(&block.id, row)
            .expect("validated canonical row")
            .unwrap_or_else(|| visibility::VisibilityPart {
                map: block.id.clone(),
                name: row["name"].as_str().unwrap_or_default().to_string(),
                entity_id: row["entity_id"].as_i64().unwrap_or(0) as i32,
                entity_groups: groups8(&row["entity_group_ids"]).map(|v| v as i32),
                effective: source.effective(row["kind"].as_str().unwrap_or_default()),
                source,
            });
        let root = commands
            .spawn((
                part_visibility,
                BlockPart {
                    block: block.id.clone(),
                    part_index: row["part_index"].as_u64().unwrap_or(0) as usize,
                },
                super::DrawGroups(draw),
                transform,
                Visibility::Hidden,
            ))
            .id();
        if super::follows_scene_light(row) {
            commands.entity(root).insert(super::FollowsSceneLight);
        }
        if row["entity_data"]["is_shadow_src"].as_bool() == Some(false) {
            commands.entity(root).insert(AuthoredNoShadow);
        }
        if enabled(&block.id)
            && row["kind"] == "MapPiece"
            && let (Some(min), Some(max), Some(&available)) = (
                super::vec3(&block.json["models"][model]["min"]),
                super::vec3(&block.json["models"][model]["max"]),
                block.detail.get(model),
            )
        {
            let rule = world
                .lod
                .map_piece(row["lod_param_id"].as_u64().unwrap_or(255) as u8)
                .expect("validated LOD row");
            let scales = super::lod::LodScales {
                platform: world.lod.platform_scale,
                crossfade_time: super::lod::CROSSFADE_STRETCH,
                ..default()
            };
            // The level the first update selects; the file-order fallback keeps the first
            // available level.
            let initial = match focus {
                Some(focus) => {
                    let distance = super::lod::bounds_distance(focus, &transform, min, max);
                    let weights = super::lod::LodState::default()
                        .update(rule, scales, distance, 0.0, available);
                    dominant(weights)
                }
                _ => available.iter().position(|&v| v).unwrap_or(0),
            };
            let mut slots = [None; 3];
            let mut stream = None;
            for (level, &exists) in available.iter().enumerate() {
                if !exists {
                    continue;
                }
                let child = commands.spawn((
                    Transform::IDENTITY,
                    if level == initial {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    DetailWeight(f32::from(level == initial)),
                    ChildOf(root),
                ));
                if level == initial {
                    stream = Some(pending.queue(&glb, Some(&block.id), true, (child.id(), level)));
                }
                slots[level] = Some(child.id());
            }
            commands.entity(root).insert(DetailPart {
                state: super::lod::LodState::default(),
                rule,
                scales,
                min,
                max,
                available,
                slots,
                stream,
                shown: initial,
                spawned: std::array::from_fn(|level| level == initial),
                fading: false,
                applied: None,
            });
        } else {
            pending.queue(&glb, Some(&block.id), true, (root, 0));
        }
        spawned += 1;
    }
    for block in world.blocks.values_mut() {
        if only.is_some_and(|only| only != block.id) {
            continue;
        }
        block.glb_total = seen.get(&block.id).map_or(0, BTreeSet::len);
        info!(
            "{} warm models {}, skipped render reasons {:?}",
            block.id,
            block.glb_total,
            skipped.get(&block.id).cloned().unwrap_or_default()
        );
    }
    info!(
        "spawned {spawned} connected map piece and ordinary Object instances, {} GLBs queued nearest-first",
        pending.streaming()
    );
}

/// Main-thread time per frame for despawning freed blocks' part roots (each with its level
/// scenes, grass and merged batches), so leaving a block of several thousand parts spreads its
/// teardown over many frames.
const TEARDOWN_BUDGET: std::time::Duration = std::time::Duration::from_millis(2);

/// GLBs whose strong handles a freed block drops per frame once its parts are gone. Dropping
/// the last handle frees the GLB's meshes, materials and textures in the asset, material and
/// render systems of the next frames, outside the teardown's own time; dropping a block's
/// hundreds of GLBs at once cost frames of 200 to 960 ms there (#271).
const GLBS_RELEASED_PER_FRAME: usize = 1;

/// A freed block on its way out: first its part roots are despawned, then its GLB handles are
/// dropped a few per frame.
struct Freeing {
    id: String,
    roots: Vec<Entity>,
    /// Strong handles per GLB (the stream's and the block's).
    glbs: Vec<Vec<UntypedHandle>>,
    /// [`BlockFreed`] was sent once its parts were all gone.
    announced: bool,
    since: std::time::Instant,
}

/// Freed blocks whose part roots or handles are still being released, oldest first.
#[derive(Resource, Default)]
pub(crate) struct Teardown {
    queue: std::collections::VecDeque<Freeing>,
}

/// Apply [`update`]'s residency changes: a block neither demanded nor held by a back-read record
/// despawns its part roots and then drops its strong GLB handles over the next frames
/// ([`teardown`]); a freed block requested again queues its parts nearest-first from the player
/// through the same stream as at startup. Collision is one world and stays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn residency(
    mut commands: Commands,
    mut world: ResMut<MapWorld>,
    mut pending: ResMut<Pending>,
    parts: Query<(Entity, &BlockPart)>,
    focus: Query<(&Transform, Has<PlayerFocus>), With<MapFocus>>,
    mut teardown: ResMut<Teardown>,
    upload: Option<ResMut<bevy::render::render_asset::RenderAssetBytesPerFrame>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("block residency");
    // A block requested again streams in while the player moves: a smaller upload budget than
    // the loading screen's keeps its meshes and textures from stalling frames.
    if world.changes.iter().any(|(_, c)| *c == Residency::Respawn)
        && let Some(mut upload) = upload
    {
        upload.max_bytes = Some(super::RESTREAM_UPLOAD_BYTES_PER_FRAME);
    }
    if !world.changes.is_empty() {
        let changes = std::mem::take(&mut world.changes);
        for (id, change) in changes {
            match change {
                Residency::Keep => {}
                Residency::Free => {
                    let roots: Vec<_> = parts
                        .iter()
                        .filter(|(_, part)| part.block == id)
                        .map(|(entity, _)| entity)
                        .collect();
                    let block = world.blocks.get_mut(&id).expect("changed block");
                    block.assets_ready = false;
                    let mut glbs = BTreeMap::<_, Vec<UntypedHandle>>::new();
                    for handle in pending
                        .release_block(&id)
                        .into_iter()
                        .chain(block.handles.drain(..).map(Handle::untyped))
                    {
                        glbs.entry(handle.id()).or_default().push(handle);
                    }
                    info!(
                        "block {id} freed: releasing {} GLBs, despawning {} part roots",
                        glbs.len(),
                        roots.len()
                    );
                    teardown.queue.push_back(Freeing {
                        id,
                        roots,
                        glbs: glbs.into_values().collect(),
                        announced: false,
                        since: std::time::Instant::now(),
                    });
                }
                Residency::Respawn => {
                    let Some(scene) = world.scene.clone() else {
                        continue;
                    };
                    // Parts still waiting to despawn would draw twice beside the new ones. Its
                    // GLBs still held are found loaded again instead of read afresh.
                    let mut stale = 0;
                    for freeing in teardown.queue.iter_mut().filter(|f| f.id == id) {
                        stale += freeing.roots.len();
                        for root in freeing.roots.drain(..) {
                            commands.entity(root).try_despawn();
                        }
                    }
                    let at = focus
                        .iter()
                        .max_by_key(|(_, player)| *player)
                        .map(|(t, _)| t.translation);
                    let before = pending.streaming();
                    spawn_blocks(
                        &mut commands,
                        &mut world,
                        &scene,
                        &mut pending,
                        Some(&id),
                        at,
                    );
                    info!(
                        "block {id} requested again: {} GLBs queued nearest-first, {stale} stale part roots despawned",
                        pending.streaming() - before
                    );
                }
            }
        }
    }
}

/// Release freed blocks a little each frame: despawn their part roots for [`TEARDOWN_BUDGET`],
/// then send [`BlockFreed`] and drop [`GLBS_RELEASED_PER_FRAME`] GLBs' handles per frame.
pub(crate) fn teardown(world: &mut World) {
    if world.resource::<Teardown>().queue.is_empty() {
        return;
    }
    let _timer = crate::perf_log::SystemTimer::start("block teardown");
    let started = std::time::Instant::now();
    let mut queue = std::mem::take(&mut world.resource_mut::<Teardown>().queue);
    for freeing in queue.iter_mut() {
        while started.elapsed() < TEARDOWN_BUDGET
            && let Some(entity) = freeing.roots.pop()
        {
            if let Ok(entity) = world.get_entity_mut(entity) {
                entity.despawn();
            }
        }
        if !freeing.roots.is_empty() {
            break;
        }
    }
    let mut releases = GLBS_RELEASED_PER_FRAME;
    for freeing in queue.iter_mut().take_while(|f| f.roots.is_empty()) {
        if !freeing.announced {
            freeing.announced = true;
            info!(
                "block {} parts despawned in {:.2} s",
                freeing.id,
                freeing.since.elapsed().as_secs_f32()
            );
            world.write_message(BlockFreed(freeing.id.clone()));
        }
        let n = releases.min(freeing.glbs.len());
        freeing.glbs.drain(..n);
        releases -= n;
    }
    while let Some(front) = queue.front()
        && front.roots.is_empty()
        && front.glbs.is_empty()
    {
        let freeing = queue.pop_front().expect("front");
        info!(
            "block {} teardown done in {:.2} s",
            freeing.id,
            freeing.since.elapsed().as_secs_f32()
        );
    }
    world.resource_mut::<Teardown>().queue = queue;
}

/// `--lod-debug` logs detail-level changes only for parts this close to Wolf (bounds distance),
/// past the outermost installed MapPiece boundary plus its play distance (60 + 4 m).
const LOD_DEBUG_RADIUS: f32 = 70.0;

// GLBs are already resident, but instancing their other scenes and preparing their materials
// can still hitch. Warm nearby levels without changing their visibility or authored weights.
const DETAIL_PREFETCH_METRES: f32 = 8.0;
const DETAIL_SCENES_PER_FRAME: usize = 4;

fn detail_requests(part: &DetailPart, distance: f32, weights: [f32; 3]) -> [bool; 3] {
    let mut requested = weights.map(|w| w > 0.);
    for distance in [
        (distance - DETAIL_PREFETCH_METRES).max(0.),
        distance + DETAIL_PREFETCH_METRES,
    ] {
        let mut state = part.state;
        let ahead = state.update(part.rule, part.scales, distance, f32::MAX, part.available);
        for (requested, weight) in requested.iter_mut().zip(ahead) {
            *requested |= weight > 0.;
        }
    }
    requested
}

/// [`update_detail`]'s per-thread finds: parts whose level weights or readiness changed, and
/// level scenes to request.
#[derive(Default)]
pub(crate) struct DetailScratch {
    changed: bevy::utils::Parallel<Vec<(Entity, f32, DetailApplied)>>,
    requests: bevy::utils::Parallel<Vec<(bool, f32, Entity, usize)>>,
}

fn prioritize_detail_requests(requests: &mut Vec<(bool, f32, Entity, usize)>) {
    requests.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| (a.2, a.3).cmp(&(b.2, b.3)))
    });
    let demanded = requests.partition_point(|r| !r.0);
    requests.truncate(demanded + DETAIL_SCENES_PER_FRAME);
}

/// The level with the largest weight, as [`update_detail`] draws it.
fn dominant(weights: [f32; 3]) -> usize {
    weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_detail(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut pending: ResMut<Pending>,
    instances: Res<WorldInstanceSpawner>,
    time: Res<Time>,
    groups: Res<super::Groups>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut parts: Query<
        (
            Entity,
            &Transform,
            &BlockPart,
            &visibility::VisibilityPart,
            &mut DetailPart,
        ),
        Without<Camera3d>,
    >,
    mut slots: Query<(&mut Visibility, &mut DetailWeight)>,

    roots: Query<(Option<&WorldAssetRoot>, Option<&WorldInstance>)>,
    children: Query<&Children>,
    meshes: Query<Option<&MeshTag>, With<Mesh3d>>,
    wolf: Query<&Transform, (With<PlayerFocus>, Without<Camera3d>)>,
    mut debug: Local<Option<bool>>,
    mut found: Local<DetailScratch>,
) {
    let _timer = crate::perf_log::SystemTimer::start("update_detail");
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let debug = *debug.get_or_insert_with(|| std::env::args().any(|a| a == "--lod-debug"));
    let wolf = wolf.iter().next().map(|t| t.translation);
    let groups_off = groups.mode == GroupMode::Off;
    let delta = time.delta_secs();
    // Every part's distance, weights and scene readiness, on all cores: a castle frame walks
    // over 10,000 parts, and on one thread this took about 2 ms of the main schedule (#70).
    // Only parts whose levels must change come back for the writes below.
    let DetailScratch { changed, requests } = &mut *found;
    {
        let pending = &*pending;
        parts
            .par_iter_mut()
            .for_each(|(part_entity, transform, _, _, mut part)| {
                let distance =
                    super::lod::bounds_distance(camera.translation, transform, part.min, part.max);
                let available = part.available;
                let mut state = part.state;
                let weights = if groups_off {
                    let mut weights = [0.; 3];
                    if let Some(first) = available.iter().position(|&v| v) {
                        weights[first] = 1.;
                    }
                    weights
                } else {
                    state.update(part.rule, part.scales, distance, delta, available)
                };
                crate::change::set(&mut part, |p| &mut p.state, state);
                // Every level with weight gets its scene, so a crossfade's incoming level
                // exists before it dominates. Every level lives in the same GLB, so once the
                // stream has requested it the scene comes from that load; before then the
                // current level stays, keeping the stream's order and in-flight limit.
                let wanted = if part.stream.is_none()
                    || part
                        .available
                        .iter()
                        .zip(part.spawned)
                        .all(|(&exists, spawned)| !exists || spawned)
                {
                    [false; 3]
                } else if groups_off {
                    weights.map(|w| w > 0.)
                } else {
                    detail_requests(&part, distance, weights)
                };
                for (level, &wanted) in wanted.iter().enumerate() {
                    if wanted
                        && !part.spawned[level]
                        && part.slots[level].is_some()
                        && part
                            .stream
                            .and_then(|index| pending.requested(index))
                            .is_some()
                    {
                        requests.borrow_local_mut().push((
                            weights[level] == 0.,
                            distance,
                            part_entity,
                            level,
                        ));
                    }
                }
                // A level is ready once its scene has spawned its meshes, so a switch never
                // leaves a gap. Capture scenes are spawned synchronously.
                let ready: [bool; 3] = std::array::from_fn(|level| {
                    weights[level] > 0.
                        && part.slots[level].is_some_and(|entity| {
                            part.stream.is_none()
                                || roots.get(entity).is_ok_and(|(root, instance)| {
                                    root.is_some()
                                        && instance
                                            .is_some_and(|i| instances.instance_is_ready(**i))
                                })
                        })
                });
                // Settled levels still check distance and scene readiness, but need no slot
                // writes. Finish clearing a fade's mesh tags before taking this path.
                let applied = DetailApplied {
                    weights,
                    ready,
                    slots: part.slots,
                    stream: part.stream,
                };
                if part.fading || part.applied.as_ref() != Some(&applied) {
                    changed
                        .borrow_local_mut()
                        .push((part_entity, distance, applied));
                }
            });
    }
    let mut changed_parts = Vec::new();
    changed.drain_into(&mut changed_parts);
    // The threads' finds come back in no fixed order.
    changed_parts.sort_unstable_by_key(|(entity, _, _)| *entity);
    for (part_entity, distance, applied) in changed_parts {
        let Ok((_, transform, identity, msb, mut part)) = parts.get_mut(part_entity) else {
            continue;
        };
        let (weights, ready) = (applied.weights, applied.ready);
        let rule = part.rule;
        let selected = dominant(weights);
        part.applied = Some(applied);
        if part.shown != selected && ready[selected] {
            part.shown = selected;
        }
        // During a transition the retail renderer submits both levels with complementary
        // weights; draw them with complementary dither coverage instead of a one-frame swap.
        // Offscreen captures are stills taken with held time, so they keep the dominant level.
        let fade_ready = if part.stream.is_some() {
            ready
        } else {
            [false; 3]
        };
        let tags = super::lod::crossfade_tags(weights, part.shown, fade_ready);
        let fading = tags.iter().any(|tag| tag.is_some_and(|tag| tag != 0));
        for (level, entity) in part.slots.iter().enumerate() {
            let Some(entity) = *entity else {
                continue;
            };
            if let Ok((mut visibility, mut weight)) = slots.get_mut(entity) {
                let desired = if tags[level].is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if visibility.set_if_neq(desired) {
                    // Only parts near Wolf, where a switch can be seen, keep the log short.
                    let from_wolf = wolf
                        .filter(|_| debug)
                        .map(|p| super::lod::bounds_distance(p, transform, part.min, part.max));
                    if from_wolf.is_some_and(|d| d < LOD_DEBUG_RADIUS) {
                        info!(
                            "lod switch: {} part {} {} level {} {}, camera {:.2} m, Wolf {:.2} m (boundaries {:?}, play {:?}), weights {:?}, Wolf at {:.1?}",
                            identity.block,
                            identity.part_index,
                            msb.name,
                            level,
                            if desired == Visibility::Hidden {
                                "hidden"
                            } else {
                                "drawn"
                            },
                            distance,
                            from_wolf.unwrap_or_default(),
                            rule.boundary,
                            rule.play,
                            weights,
                            wolf.unwrap_or_default()
                        );
                    }
                }
                weight.set_if_neq(DetailWeight(weights[level]));
            }
            if fading || part.fading {
                // Write only changed tags, so a mesh is re-extracted only when its coverage moves.
                for mesh in children.iter_descendants(entity) {
                    let Ok(current) = meshes.get(mesh) else {
                        continue;
                    };
                    match (tags[level].filter(|&tag| tag != 0), current) {
                        (Some(tag), current) if current.is_none_or(|c| c.value != tag) => {
                            commands.entity(mesh).insert(MeshTag::new(tag));
                        }
                        (None, Some(_)) => {
                            commands.entity(mesh).remove::<MeshTag>();
                        }
                        _ => {}
                    }
                }
            }
        }
        part.fading = fading;
    }
    // Current demand wins over speculative warm-up, then nearest bounds first. Deferred
    // requests are reconsidered next frame, so movement cannot leave a stale backlog.
    let mut requests = Vec::new();
    found.requests.drain_into(&mut requests);
    prioritize_detail_requests(&mut requests);
    for (_, _, entity, level) in requests {
        let (_, _, _, _, mut part) = parts.get_mut(entity).expect("queried detail part");
        let (glb, unapproved) = pending.requested(part.stream.unwrap()).unwrap();
        let scene = super::scene_handle(&assets, glb, level, unapproved);
        commands
            .entity(part.slots[level].unwrap())
            .insert(WorldAssetRoot(scene));
        part.spawned[level] = true;
        pending.add_instance();
    }
}

/// One-shot hierarchy audit after loading, for diagnosing scene handoffs at fixed poses.
pub(crate) fn debug_hierarchy(world: &mut World) {
    if !std::env::args().any(|a| a == "--world-debug")
        || !world.resource::<Pending>().loaded()
        || world.contains_resource::<HierarchyReported>()
    {
        return;
    }
    let roots: Vec<_> = world
        .query::<(Entity, &BlockPart, &DetailPart)>()
        .iter(world)
        .filter(|(_, id, _)| id.block == "m11_01_00_00")
        .take(3)
        .map(|(entity, id, detail)| (entity, id.part_index, detail.slots))
        .collect();
    for (root, index, slots) in roots {
        info!(
            "world hierarchy root part {}: visibility {:?}, inherited {:?}, transform {:?}",
            index,
            world.get::<Visibility>(root),
            world.get::<InheritedVisibility>(root),
            world.get::<GlobalTransform>(root)
        );
        for (level, slot) in slots.iter().enumerate() {
            let Some(slot) = slot else {
                continue;
            };
            info!(
                "world hierarchy level {}: visibility {:?}, inherited {:?}, parent {:?}, transform {:?}",
                level,
                world.get::<Visibility>(*slot),
                world.get::<InheritedVisibility>(*slot),
                world.get::<ChildOf>(*slot),
                world.get::<GlobalTransform>(*slot)
            );
            let mut queue = vec![*slot];
            let mut meshes = 0;
            while let Some(entity) = queue.pop() {
                if world.get::<Mesh3d>(entity).is_some() {
                    if meshes < 2 {
                        info!(
                            "world hierarchy mesh: visibility {:?}, inherited {:?}, transform {:?}",
                            world.get::<Visibility>(entity),
                            world.get::<InheritedVisibility>(entity),
                            world.get::<GlobalTransform>(entity)
                        );
                    }
                    meshes += 1;
                }
                if let Some(children) = world.get::<Children>(entity) {
                    queue.extend(children.iter());
                }
            }
            info!("world hierarchy level {level}: {meshes} meshes");
        }
    }
    world.insert_resource(HierarchyReported);
}

#[derive(Resource)]
struct HierarchyReported;

#[cfg(test)]
mod tests {
    #[test]
    fn block_frees_once_neither_demanded_nor_held() {
        use super::{Lifecycle, RequestState as S, Residency as R};
        // (demanded, held by a back-read record, assets ready, can free) per frame, and the
        // state and residency change expected after it.
        type Frame = (bool, bool, bool, bool, S, R);
        let cases: [(&str, &[Frame]); 5] = [
            (
                "held while its records retire, then freed; demand again respawns",
                &[
                    (true, true, true, true, S::Ready, R::Keep),
                    (false, true, true, true, S::Retiring, R::Keep),
                    (false, true, true, true, S::Retiring, R::Keep),
                    (false, false, true, true, S::Dormant, R::Free),
                    (false, false, false, true, S::Dormant, R::Keep),
                    (true, true, false, true, S::Requested, R::Respawn),
                    (true, true, false, true, S::Requested, R::Keep),
                    (true, true, true, true, S::Ready, R::Keep),
                ],
            ),
            (
                "never ready (pre-warmed or still streaming) stays warm",
                &[
                    (true, false, false, true, S::Requested, R::Keep),
                    (false, false, false, true, S::Retiring, R::Keep),
                    (false, false, true, true, S::Dormant, R::Keep),
                ],
            ),
            (
                "waits to free until freeing is allowed",
                &[
                    (true, false, true, false, S::Ready, R::Keep),
                    (false, false, true, false, S::Retiring, R::Keep),
                    (false, false, true, false, S::Dormant, R::Keep),
                    (false, false, true, true, S::Dormant, R::Free),
                ],
            ),
            (
                "a held block stays resident out of sight",
                &[
                    (true, true, true, true, S::Ready, R::Keep),
                    (false, true, true, true, S::Retiring, R::Keep),
                    (false, true, true, true, S::Retiring, R::Keep),
                    (false, false, true, true, S::Dormant, R::Free),
                ],
            ),
            (
                "a freed block reloads unseen once a record holds it",
                &[
                    (true, false, true, true, S::Ready, R::Keep),
                    (false, false, true, true, S::Retiring, R::Keep),
                    (false, false, true, true, S::Dormant, R::Free),
                    (false, true, false, true, S::Dormant, R::Respawn),
                    (false, true, false, true, S::Dormant, R::Keep),
                    (true, true, true, true, S::Ready, R::Keep),
                    (false, false, true, true, S::Retiring, R::Keep),
                    (false, false, true, true, S::Dormant, R::Free),
                    // Reloaded for a record but never seen: freed again once it goes.
                    (false, true, false, true, S::Dormant, R::Respawn),
                    (false, false, false, true, S::Dormant, R::Free),
                ],
            ),
        ];
        for (name, frames) in cases {
            let mut life = Lifecycle::default();
            let mut state = S::Dormant;
            for (i, &(demanded, held, ready, can_free, want, change)) in frames.iter().enumerate() {
                let (next, got) = life.step(state, demanded, held, ready, can_free);
                assert_eq!((next, got), (want, change), "{name}: frame {i}");
                state = next;
            }
        }
    }

    /// The exe's back-read records (tools/emu-cases `worldbackread`: FUN_14081f020 demand and
    /// FUN_14081eae0 step on built sets and navmeshes): every frame's records (list order,
    /// state, timer, distance), the deleted ones and the ones that reach the group masks must
    /// match. Skipped when extracted/emu/worldbackread.json is missing.
    #[test]
    fn back_read_matches_the_exe() {
        use super::{BackReadSet, BackState};
        let dir = std::env::var_os("SHINOBI_EMU")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("SHINOBI_ROOT")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
                    })
                    .join("extracted/emu")
            });
        let Ok(text) = std::fs::read_to_string(dir.join("worldbackread.json")) else {
            eprintln!(
                "skipped: no extracted/emu/worldbackread.json (tools/emu-cases worldbackread)"
            );
            return;
        };
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let (mut frames, mut records) = (0, 0);
        for case in cases["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let p: Vec<f32> = case["set"].as_array().unwrap().iter().map(f).collect();
            let mut set = BackReadSet::<u32>::new(p[0], p[1], p[2], p[3]);
            for (i, frame) in case["frames"].as_array().unwrap().iter().enumerate() {
                for q in frame["query"].as_array().unwrap() {
                    if q[2].as_bool().unwrap() {
                        set.demand(q[0].as_u64().unwrap() as u32, f(&q[1]));
                    }
                }
                let gone = set.step(f(&frame["dt"]));
                let want_gone: Vec<u32> = frame["deleted"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
                assert_eq!(gone, want_gone, "{name} frame {i}: deleted");
                // The exe reads the masks of the kept active and retiring records as it walks
                // them, nearest first: the reverse of the new list.
                let masked: Vec<u32> = set
                    .records
                    .iter()
                    .rev()
                    .filter(|r| r.state != BackState::Pending)
                    .map(|r| r.key)
                    .collect();
                let want_masked: Vec<u32> = frame["masked"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap() as u32)
                    .collect();
                assert_eq!(masked, want_masked, "{name} frame {i}: masked");
                assert_eq!(set.start, f(&frame["start"]), "{name} frame {i}: start");
                let want = frame["records"].as_array().unwrap();
                assert_eq!(set.records.len(), want.len(), "{name} frame {i}: records");
                for (got, want) in set.records.iter().zip(want) {
                    let w = (
                        want["nav"].as_u64().unwrap() as u32,
                        want["state"].as_u64().unwrap() as u8,
                        f(&want["timer"]),
                        f(&want["distance"]),
                    );
                    assert_eq!(
                        (got.key, got.state as u8, got.timer, got.distance),
                        w,
                        "{name} frame {i}"
                    );
                }
                frames += 1;
                records += want.len();
            }
        }
        eprintln!("back read: {frames} frames, {records} records match the exe");
    }

    #[test]
    fn detail_spawn_budget_caps_only_prefetch_and_keeps_all_demand() {
        let mut requests: Vec<_> = [
            (true, 1.),
            (false, 50.),
            (true, 5.),
            (false, 20.),
            (true, 3.),
            (true, 2.),
            (true, 4.),
            (false, 30.),
            (false, 40.),
            (false, 60.),
            (false, 70.),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (prefetch, distance))| (prefetch, distance, Entity::PLACEHOLDER, i))
        .collect();
        prioritize_detail_requests(&mut requests);
        assert_eq!(
            requests.iter().map(|r| r.3).collect::<Vec<_>>(),
            [3, 7, 8, 1, 9, 10, 0, 5, 4, 6]
        );
    }

    #[test]
    fn settled_detail_is_untouched_and_resumes_on_distance_or_readiness_change() {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), AssetPlugin::default()))
            .init_resource::<Pending>()
            .init_resource::<WorldInstanceSpawner>()
            .init_resource::<Time>()
            .insert_resource(super::super::Groups {
                mode: GroupMode::Display,
                parts: Vec::new(),
                current: None,
                active: None,
                checked_at: None,
                dirty: false,
            })
            .add_systems(Update, update_detail);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.1));
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Transform::IDENTITY))
            .id();
        let first = app
            .world_mut()
            .spawn((Visibility::Inherited, DetailWeight(1.)))
            .id();
        let second = app
            .world_mut()
            .spawn((Visibility::Hidden, DetailWeight(0.)))
            .id();
        let row = json!({
            "kind": "MapPiece", "name": "test", "entity_id": -1,
            "entity_group_ids": vec![0; 8], "display_data": {
                "collision_mask": vec![0; 32], "display_groups": vec![0; 8],
                "draw_groups": vec![0; 8], "condition1": 0, "condition2": 0
            }
        });
        let part = app
            .world_mut()
            .spawn((
                Transform::IDENTITY,
                BlockPart {
                    block: ASHINA[0].into(),
                    part_index: 0,
                },
                visibility::VisibilityPart::from_row(ASHINA[0], &row)
                    .unwrap()
                    .unwrap(),
                DetailPart {
                    state: Default::default(),
                    rule: super::super::lod::LodRule {
                        boundary: [30., 60.],
                        play: [2., 4.],
                        crossfade: true,
                        seconds: 0.2,
                    },
                    scales: super::super::lod::LodScales {
                        crossfade_time: super::super::lod::CROSSFADE_STRETCH,
                        ..default()
                    },
                    min: Vec3::ZERO,
                    max: Vec3::ZERO,
                    available: [true, true, false],
                    slots: [Some(first), None, None],
                    stream: None,
                    shown: 0,
                    spawned: [true, true, false],
                    fading: false,
                    applied: None,
                },
            ))
            .id();
        let detail = app.world().get::<DetailPart>(part).unwrap();
        assert_eq!(
            detail_requests(detail, 0., [1., 0., 0.]),
            [true, false, false]
        );
        assert_eq!(
            detail_requests(detail, 25., [1., 0., 0.]),
            [true, true, false]
        );
        assert_eq!(detail.state, super::super::lod::LodState::default());
        app.update();
        let tick = app
            .world()
            .entity(first)
            .get_ref::<DetailWeight>()
            .unwrap()
            .last_changed();
        let part_tick = app
            .world()
            .entity(part)
            .get_ref::<DetailPart>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(first)
                .get_ref::<DetailWeight>()
                .unwrap()
                .last_changed(),
            tick
        );
        assert_eq!(
            app.world()
                .entity(part)
                .get_ref::<DetailPart>()
                .unwrap()
                .last_changed(),
            part_tick
        );
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .x = 40.;
        for _ in 0..7 {
            app.update();
        }
        assert_eq!(
            *app.world().get::<Visibility>(first).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<DetailWeight>(first).unwrap().0, 0.);
        app.world_mut().get_mut::<DetailPart>(part).unwrap().slots[1] = Some(second);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(first).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(second).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<DetailWeight>(second).unwrap().0, 1.);
        // Replacing a ready slot must initialize it even when weights/readiness stay equal.
        let replacement = app
            .world_mut()
            .spawn((Visibility::Hidden, DetailWeight(0.)))
            .id();
        app.world_mut().get_mut::<DetailPart>(part).unwrap().slots[1] = Some(replacement);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(replacement).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<DetailWeight>(replacement).unwrap().0, 1.);
        let tick = app
            .world()
            .entity(second)
            .get_ref::<DetailWeight>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(second)
                .get_ref::<DetailWeight>()
                .unwrap()
                .last_changed(),
            tick
        );
    }

    #[test]
    fn material_layers_resolve_from_the_owning_model_and_preserve_asset_source() {
        for owner in ["m11_00_00_00", "m11_01_00_00", "m13_00_00_00"] {
            for source in ["", "cache://"] {
                let model = format!("{source}maps/{owner}/models/ground.glb#Material0");
                let path = bevy::asset::AssetPath::parse(&model);
                assert_eq!(
                    super::relative_material_texture(&path, "tex/ground.dds")
                        .map(|p| p.replace('\\', "/")),
                    Some(format!("{source}maps/{owner}/models/tex/ground.dds"))
                );
                assert!(super::relative_material_texture(&path, "tex/../other.dds").is_none());
            }
        }
    }

    use super::*;

    #[test]
    fn retail_wildcards_and_connection_mask_union() {
        let inventory = [
            "m11_00_00_00",
            "m11_01_00_00",
            "m13_00_00_00",
            "m13_01_00_00",
            "m13_00_00_99",
        ]
        .map(str::to_string);
        assert_eq!(
            targets([13, 255, 255, 255], &inventory),
            vec!["m13_00_00_00", "m13_00_00_99"]
        );
        assert!(targets([255; 4], &inventory).is_empty());
        let mut mask = display_mask(
            &json!({"display_group":{"condition":65,"disp_groups":[1,0,0,0,0,0,0,0]}}),
        );
        union(
            &mut mask,
            display_mask(
                &json!({"display_group":{"condition":66,"disp_groups":[8,0,0,0,0,0,0,0]}}),
            ),
        );
        assert_eq!(mask, [9, 0, 6, 0, 0, 0, 0, 0]);
        assert_eq!(
            display_mask(&json!({"display_group":{"condition":-1,"disp_groups":[16]}})),
            [16, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn installed_neighbourhood_keeps_blocks_and_baked_transforms() {
        let cache = sekiro_sim::cache::root();
        if !cache.join("maps/m11_01_00_00/layout.json").is_file() {
            return;
        }
        let mut world = MapWorld::load(&cache, ASHINA[0], "maps/m11_00_00_00/").unwrap();
        assert_eq!(
            world.blocks.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![
                "m11_00_00_00",
                "m11_01_00_00",
                "m11_02_00_00",
                "m13_00_00_00",
                "m17_00_00_00",
                "m20_00_00_00"
            ]
        );
        let original: Value = serde_json::from_slice(
            &std::fs::read(cache.join("maps/m11_00_00_00/layout.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            world.blocks[ASHINA[0]].json["parts"][0]["translation"],
            original["parts"][0]["translation"]
        );
        let inventory: Vec<_> = world.blocks.keys().cloned().collect();
        for (source, target) in [
            (ASHINA[0], ASHINA[1]),
            (ASHINA[1], ASHINA[0]),
            (ASHINA[1], "m11_02_00_00"),
            (ASHINA[1], "m13_00_00_00"),
            (ASHINA[1], "m17_00_00_00"),
        ] {
            let block = &world.blocks[source];
            let mut candidates = Vec::new();
            for row in block.json["connections"].as_array().unwrap() {
                if !pattern(row)
                    .is_some_and(|p| targets(p, &inventory).iter().any(|id| id == target))
                {
                    continue;
                }
                let Some(name) = row["connection"]["source_collision"]["name"].as_str() else {
                    continue;
                };
                let Some(surface) = block.collisions.iter().find(|s| s.name == name) else {
                    continue;
                };
                for tri in block.collision.indices
                    [surface.first * 3..(surface.first + surface.count) * 3]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .take(32)
                {
                    let [a, b, c] = tri.map(|i| Vec3::from(block.collision.vertices[i as usize]));
                    if (b - a).cross(c - a).normalize_or_zero().y.abs() < 50f32.to_radians().cos() {
                        continue;
                    }
                    candidates.push(((a + b + c) / 3. + Vec3::Y * 1.65, name.to_string()));
                }
            }
            let found = candidates.into_iter().find(|(pos, _)| {
                world.demand(&[*pos], None);
                world.selected.as_ref().is_some_and(|(id, _)| id == source)
                    && world.blocks[target].normal != [0; 8]
            });
            let (pose, collision) =
                found.unwrap_or_else(|| panic!("no occupied {source} -> {target} connection"));
            println!(
                "native connection {source} -> {target}: camera {pose:?}, source {collision}, target mask {:08x?}",
                world.blocks[target].normal
            );
        }
        // Exercise a grounded seam with multiple attached rows and target blocks.
        // Use the loaded world so the asset and block inventory contract is identical.
        world.visibility_maps.clear();
        world.blocks.retain(|id, _| {
            matches!(
                id.as_str(),
                "m11_00_00_00" | "m11_01_00_00" | "m11_02_00_00"
            )
        });
        for (id, lo, hi, bit) in [
            (ASHINA[0], -10., 0., 1),
            (ASHINA[1], 0., 10., 2),
            ("m11_02_00_00", 20., 30., 4),
        ] {
            let block = world.blocks.get_mut(id).unwrap();
            block.collision = CollisionMesh {
                vertices: vec![[lo, 0., -10.], [hi, 0., -10.], [hi, 0., 10.], [lo, 0., 10.]],
                indices: vec![0, 1, 2, 0, 2, 3],
                materials: vec![1, 1],
            };
            block.collisions = vec![Surface {
                name: "support".into(),
                first: 0,
                count: 2,
                min: Vec2::new(lo, -10.),
                max: Vec2::new(hi, 10.),
                demand: [bit, 0, 0, 0, 0, 0, 0, 0],
                backread: [32, 0, 0, 0, 0, 0, 0, 0],
                player: true,
            }];
            block.json = json!({"connections":[]});
        }
        world.blocks.get_mut(ASHINA[0]).unwrap().json["connections"] = json!([
            {"connection":{"source_collision":{"name":"support"},"map_id":[11,1,255,255]},"display_group":{"condition":65,"disp_groups":[8,0,0,0,0,0,0,0]}},
            {"connection":{"source_collision":{"name":"support"},"map_id":[11,1,255,255]},"display_group":{"condition":66,"disp_groups":[0,0,0,0,0,0,0,0]}},
            {"connection":{"source_collision":{"name":"support"},"map_id":[11,2,255,255]},"display_group":{"condition":32,"disp_groups":[0,0,0,0,0,0,0,0]}},
            {"connection":{"source_collision":null,"map_id":[11,1,255,255]},"display_group":{"condition":90,"disp_groups":[0,0,0,0,0,0,0,0]}}
        ]);
        world.demand(&[Vec3::new(-1., 1., 0.)], None);
        assert_eq!(world.blocks[ASHINA[1]].normal, [8, 0, 6, 0, 0, 0, 0, 0]);
        assert_eq!(
            world.blocks["m11_02_00_00"].normal,
            [0, 1, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            world.blocks[ASHINA[0]].additional,
            [32, 0, 0, 0, 0, 0, 0, 0]
        );
        world.demand(&[Vec3::new(0., 1., 0.)], None);
        assert_eq!(world.blocks[ASHINA[1]].normal, [10, 0, 6, 0, 0, 0, 0, 0]);
        world.demand(&[Vec3::new(1., 1., 0.)], None);
        assert_eq!(world.blocks[ASHINA[1]].normal, [2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(world.blocks[ASHINA[0]].normal, [0; 8]);
    }

    /// The grapple from the Outskirts ledge to the branch over the ravine: on the perch Wolf
    /// stands on no map collision (only the event walls h900000 and h900001, which he does not
    /// collide with, lie below), so the ledge's part stays, whatever the camera does. A
    /// teleport re-arms the spawn probe even in the air.
    #[test]
    fn standing_part_survives_the_branch_and_follows_teleports() {
        let cache = sekiro_sim::cache::root();
        if !cache.join("maps/m11_01_00_00/layout.json").is_file() {
            return;
        }
        let mut world = MapWorld::load(&cache, ASHINA[0], "maps/m11_00_00_00/").unwrap();
        // On the ledge's part `h023300` 6 m north of where `h` has its lip at (-132.5, -44.6,
        // -124.0): `l`'s hull there is a slope too steep to stand on (#189).
        let ledge = Vec3::new(-136.5, -42.54, -118.0);
        // Both ends of the branch: the user's perch and the one the live check's grapple
        // reaches from the ledge.
        let perches = [
            Vec3::new(-137.6, -44.3, -124.5),
            Vec3::new(-141.58, -44.26, -123.86),
        ];
        let ledge_part = Some((ASHINA[0].to_string(), "h023300".to_string()));

        // The event walls under the branch are not the player's collision.
        for wall in ["h900000", "h900001"] {
            let surface = world.blocks[ASHINA[0]]
                .collisions
                .iter()
                .find(|s| s.name == wall)
                .unwrap_or_else(|| panic!("{wall} missing"));
            assert!(!surface.player, "{wall} is player collision");
        }
        // No player-collidable part is within reach of either perch.
        for perch in perches {
            let below = world.hits(perch, None, true).into_iter().next();
            assert!(
                below
                    .as_ref()
                    .is_none_or(|h| !(-0.5..=STAND_REACH).contains(&h.1)),
                "{perch} stands on {below:?}"
            );
        }

        // The viewer's camera probe over the ravine finds only zero-demand helper walls.
        world.demand(&[perches[0]], None);
        // `l`'s hulls of the two walls overlap differently from `h`'s, so either may be first.
        assert!(
            world
                .selected
                .as_ref()
                .is_some_and(|(block, part)| block == ASHINA[0]
                    && ["h900000", "h900001"].contains(&part.as_str())),
            "{:?}",
            world.selected
        );
        assert_eq!(world.blocks[ASHINA[0]].normal, [0; 8]);

        // Normal ground: the ledge's own part, and a jump's apex keeps it.
        assert!(world.stand(ledge, true, None));
        assert_eq!(world.selected, ledge_part);
        let ledge_mask = world.blocks[ASHINA[0]].normal;
        assert_ne!(ledge_mask, [0; 8]);
        assert!(!world.stand(ledge + Vec3::Y * 1.5, false, None));
        assert_eq!(world.selected, ledge_part);

        // The play path: Wolf on the ledge, then perched on each end of the branch. A camera
        // over the ravine guards against cameras taking part again.
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_resource::<super::super::story::MapStory>()
            .init_resource::<Pending>()
            .insert_resource(crate::map::Groups {
                mode: GroupMode::Off,
                parts: Vec::new(),
                current: None,
                active: None,
                checked_at: None,
                dirty: false,
            })
            .insert_resource(world)
            .add_systems(Update, update);
        let wolf = app
            .world_mut()
            .spawn((
                MapFocus,
                PlayerFocus { grounded: true },
                Transform::from_translation(ledge),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Transform::from_translation(ledge)))
            .id();
        app.update();
        for perch in perches {
            app.world_mut()
                .get_mut::<Transform>(wolf)
                .unwrap()
                .translation = perch;
            app.update();
            let world = app.world().resource::<MapWorld>();
            assert_eq!(world.selected, ledge_part, "{perch}");
            assert_eq!(world.blocks[ASHINA[0]].normal, ledge_mask, "{perch}");
            assert!(
                matches!(
                    world.blocks[ASHINA[0]].state,
                    RequestState::Requested | RequestState::Ready
                ),
                "m11_00 retired at {perch}: {:?}",
                world.blocks[ASHINA[0]].state
            );
        }
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = perches[0] + Vec3::new(-4.5, 1.5, 0.0);
        app.update();
        assert_eq!(app.world().resource::<MapWorld>().selected, ledge_part);

        // A teleport to the ground elsewhere: the part under his feet takes over, Ashina
        // Castle's first player start.
        let mut world = app.world_mut().remove_resource::<MapWorld>().unwrap();
        let castle = Vec3::new(202.6, -26.4, 176.7);
        assert!(world.stand(castle, true, None));
        let (block, part) = world.selected.clone().unwrap();
        assert_eq!(block, ASHINA[1]);
        let surface = world.blocks[&block]
            .collisions
            .iter()
            .find(|s| s.name == part)
            .unwrap();
        assert!(surface.player, "{part} is not player collision");
        assert_ne!(world.blocks[ASHINA[1]].normal, [0; 8]);
        println!("castle start stands on {block}/{part}");

        // A teleport into the air (a tour stop or a fall return above the ground) re-arms the
        // spawn probe: the part below him takes over before he lands.
        assert!(world.stand(ledge + Vec3::Y * 5.0, false, None));
        assert_eq!(world.selected, ledge_part);
    }

    /// The play world around the Outskirts start (`play_blocks`, the stories and
    /// `collision_world`), as the game builds it.
    struct PlayWorld {
        stories: super::super::story::MapStory,
        world: std::sync::Arc<sekiro_sim::collision::CollisionWorld>,
    }

    /// [`PlayWorld`], built once and shared by the tests; `None` without the cache.
    fn play_world() -> Option<&'static PlayWorld> {
        static WORLD: std::sync::OnceLock<Option<PlayWorld>> = std::sync::OnceLock::new();
        WORLD
            .get_or_init(|| {
                let cache = sekiro_sim::cache::root();
                if !cache
                    .join("maps/m11_00_00_00/object_collision.bin")
                    .is_file()
                {
                    return None;
                }
                let t = std::time::Instant::now();
                let blocks = play_blocks(&cache, ASHINA[0]).unwrap();
                let stories = super::super::story::load_blocks(&cache, &blocks).unwrap();
                let world = collision_world(&cache, &blocks, &stories).unwrap();
                println!(
                    "collision world built in {:.0} ms",
                    t.elapsed().as_secs_f64() * 1e3
                );
                Some(PlayWorld {
                    stories,
                    world: std::sync::Arc::new(world),
                })
            })
            .as_ref()
    }

    /// The play world's collision (`collision_world`, objects included and the story's switched
    /// off ones left out) holds Wolf on the grapple pine over the Outskirts start ravine, at the
    /// trunk's tip the sim data test (`crates/sim/tests/it/object_collision.rs`) starts from.
    #[test]
    fn play_collision_world_stands_on_the_grapple_pine() {
        let Some(play) = play_world() else {
            return;
        };
        // Both blocks have a story to leave switched-off objects out with.
        let stories = &play.stories;
        assert!(stories.maps.contains_key(ASHINA[0]) && stories.maps.contains_key(ASHINA[1]));
        let (x, z) = (-140.9, -124.1);
        let floor = play
            .world
            .floor(x, z, -40.0)
            .expect("a floor at the trunk's tip");
        assert!((floor + 44.1).abs() < 0.3, "tip floor at {floor}");
        assert_eq!(
            play.world.floor_object(x, z, -40.0),
            Some("m11_00_00_00/o002500_0005")
        );
    }

    /// The castle's HUB door o111410_8000 (entity 1111700), switched by the map events
    /// ([`Switchable`]), is an event part of the collision world: it blocks a ray through it
    /// while on, and the ray passes once `set_event_part` turns it off (the door opened).
    #[test]
    fn a_switched_door_blocks_until_the_events_open_it() {
        let cache = sekiro_sim::cache::root();
        let castle = ASHINA[1];
        if !cache
            .join("maps")
            .join(castle)
            .join("object_collision.bin")
            .is_file()
        {
            return;
        }
        let blocks = BTreeMap::from([(
            castle.to_string(),
            read_layout(&cache.join("maps").join(castle)).unwrap(),
        )]);
        let stories = super::super::story::load_blocks(&cache, &blocks).unwrap();
        let door = blocks[castle]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["entity_id"] == 1111700)
            .unwrap();
        let f = |v: &Value, i: usize| v[i].as_f64().unwrap() as f32;
        let (t, r) = (&door["translation"], &door["rotation"]);
        let at = Vec3::new(f(t, 0), f(t, 1), f(t, 2));
        let normal = Quat::from_xyzw(f(r, 0), f(r, 1), f(r, 2), f(r, 3)) * Vec3::Z;
        // Through one leaf (the two leaves meet at the door's middle).
        let side = Vec3::new(normal.z, 0.0, -normal.x);
        let origin = at + Vec3::Y * 1.2 + side * 0.8 + normal * 1.0;
        let switch = Switchable {
            objects: BTreeSet::from([1111700]),
            ..Default::default()
        };
        let world = collision_world_switched(&cache, &blocks, &stories, &switch).unwrap();
        let hit = world.raycast(origin, -normal, 2.0);
        assert!(
            hit.is_some_and(|d| d < 1.4),
            "the shut door blocks: {hit:?}"
        );
        world.set_event_part(1111700, false);
        let hit = world.raycast(origin, -normal, 2.0);
        assert!(
            hit.is_none_or(|d| d > 1.4),
            "the open door lets it through: {hit:?}"
        );
        world.set_event_part(1111700, true);
        assert!(world.raycast(origin, -normal, 2.0).is_some_and(|d| d < 1.4));
    }

    /// #85: the first-visit story switches off h003401 and h003501, the other state of the
    /// courtyard before the Outskirts samurai general (`c1020_0000`), whose floors stand up to
    /// 6 m above the ground there. Left in, Wolf ran into them and stood on them in mid-air.
    #[test]
    fn story_switched_off_collision_parts_hold_no_one() {
        let cache = sekiro_sim::cache::root();
        if !cache.join("maps/m11_00_00_00/collision.bin").is_file() {
            return;
        }
        let blocks = BTreeMap::from([(
            ASHINA[0].to_string(),
            read_layout(&cache.join("maps").join(ASHINA[0])).unwrap(),
        )]);
        let stories = super::super::story::load_blocks(&cache, &blocks).unwrap();
        assert!(stories.maps[ASHINA[0]].switched_off("h003401"));
        let world = collision_world(&cache, &blocks, &stories).unwrap();
        // Inside h003401's footprint, 5.7 m above the courtyard: the floor is the ground.
        let floor = world.floor(-212.0, -1.0, -20.0).expect("courtyard ground");
        assert!((floor + 40.5).abs() < 0.3, "floor at {floor}");
    }

    /// The grapple ledge and the trunk walk's lower ground (spawns of sekiro-rs's former live
    /// play check) stand over the play collision; a spawn over a hole made that game refuse to
    /// start (#231).
    #[test]
    fn live_check_spawns_stand_on_the_play_collision() {
        let Some(p) = play_world() else { return };
        for (name, [x, y, z], ground) in [
            ("grapple", [-136.5, -42.54, -118.0], -42.54),
            ("trunk", [-140.0, -50.0, -132.0], -50.78),
        ] {
            let floor = p.world.floor(x, z, y + 1.0);
            assert!(
                floor.is_some_and(|f| (f - ground).abs() < 0.1),
                "{name} spawn ({x}, {y}, {z}): floor {floor:?}, expected {ground}"
            );
        }
    }
}
