//! Job E: executable mask algebra and a single map-root visibility writer.
//!
//! Job D installs `MapVisibilityPlugin`, spawns roots with `VisibilityPart::from_row`,
//! and supplies `BlockVisibility` for each block. Its legacy group writer must exclude
//! `VisibilityPart`. Residency/readiness are inputs, never inferred from draw visibility.
//! Job C replaces `AuthoredVisibility::maps` with its per-map event snapshots.
//! Arena staging publishes a separate snapshot, combined here without writing Visibility.
//!
//! `VisibilityMap::requests` accepts ALL participating collision names, including retained
//! navigation demand supplied by D. It does not choose the nearest vertical surface.
//! Unknown airborne/stacked-floor occupancy stays with that upstream policy.

use std::collections::BTreeMap;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use serde_json::Value;

pub type Mask = [u32; 8];

pub fn supported(map: &str) -> bool {
    matches!(map, "m11_00_00_00" | "m11_01_00_00")
}

fn intersects(a: Mask, b: Mask) -> bool {
    a.into_iter().zip(b).any(|(a, b)| a & b != 0)
}

fn union(a: &mut Mask, b: Mask) {
    for (a, b) in a.iter_mut().zip(b) {
        *a |= b;
    }
}

fn words<const N: usize>(value: &Value) -> Result<[u32; N]> {
    let array = value.as_array().context("mask is not an array")?;
    anyhow::ensure!(array.len() == N, "expected {N} mask words");
    let mut out = [0; N];
    for (out, value) in out.iter_mut().zip(array) {
        // The separate DispGroups array is exported signed; preserve its bit pattern.
        *out = if let Some(word) = value.as_u64() {
            u32::try_from(word).context("mask word exceeds u32")?
        } else {
            i32::try_from(value.as_i64().context("invalid mask word")?)? as u32
        };
    }
    Ok(out)
}

/// Original six slices, kept for diagnostic provenance after preprocessing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMasks {
    pub primary_draw: Mask,
    pub alternative_draw: Mask,
    pub draw_selector: Mask,
    pub primary_backread: Mask,
    pub alternative_backread: Mask,
    pub backread_selector: Mask,
    pub condition1: u8,
    pub condition2: u8,
}

impl SourceMasks {
    pub fn from_row(row: &Value) -> Result<Self> {
        let display = &row["display_data"];
        let collision = words::<32>(&display["collision_mask"])?;
        Ok(Self {
            primary_draw: words(&display["display_groups"])?,
            alternative_draw: words(&display["draw_groups"])?,
            draw_selector: collision[..8].try_into().unwrap(),
            primary_backread: collision[8..16].try_into().unwrap(),
            alternative_backread: collision[16..24].try_into().unwrap(),
            backread_selector: collision[24..].try_into().unwrap(),
            condition1: u8::try_from(
                display["condition1"]
                    .as_u64()
                    .context("missing Condition1")?,
            )?,
            condition2: u8::try_from(
                display["condition2"]
                    .as_u64()
                    .context("missing Condition2")?,
            )?,
        })
    }

    pub fn effective(&self, kind: &str) -> EffectiveMasks {
        let preprocess = matches!(kind, "MapPiece" | "Object" | "Collision")
            && self.condition1 == 0
            && self.condition2 != 0;
        let select = |primary: Mask, alternative: Mask, selector: Mask| {
            std::array::from_fn(|i| (primary[i] & selector[i]) | (alternative[i] & !selector[i]))
        };
        let mut draw = self.primary_draw;
        let mut backread = self.primary_backread;
        if preprocess {
            draw = select(draw, self.alternative_draw, self.draw_selector);
            backread = select(backread, self.alternative_backread, self.backread_selector);
        }
        if kind == "Collision" {
            union(&mut backread, draw);
        }
        let zero_map_piece = kind == "MapPiece" && draw == [0; 8];
        if zero_map_piece {
            draw = [u32::MAX; 8];
            backread = draw;
        }
        EffectiveMasks {
            draw,
            backread,
            preprocess,
            zero_map_piece,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveMasks {
    pub draw: Mask,
    pub backread: Mask,
    pub preprocess: bool,
    pub zero_map_piece: bool,
}

/// Collision/ConnectCollision accessor: explicit DispGroups plus a valid authored bit.
pub fn display_mask(row: &Value) -> Result<Mask> {
    let display = &row["display_group"];
    let mut mask = words(&display["disp_groups"])?;
    let condition = display["condition"]
        .as_i64()
        .context("missing display Condition")?;
    // Out-of-range values have no established semantics; do not emulate unchecked shifts.
    anyhow::ensure!(
        condition < 256,
        "display Condition outside 0..255: {condition}"
    );
    if condition >= 0 {
        mask[condition as usize / 32] |= 1 << (condition as usize % 32);
    }
    Ok(mask)
}

/// Put this on each rendered MapPiece/Object ROOT, never on GLB descendants.
#[derive(Component, Clone, Debug)]
pub struct VisibilityPart {
    pub map: String,
    pub name: String,
    pub entity_id: i32,
    pub entity_groups: [i32; 8],
    pub source: SourceMasks,
    pub effective: EffectiveMasks,
}

impl VisibilityPart {
    /// None preserves every other map's legacy loader and visibility behavior.
    pub fn from_row(map: &str, row: &Value) -> Result<Option<Self>> {
        if !supported(map) {
            return Ok(None);
        }
        let kind = row["kind"]
            .as_str()
            .context("missing canonical part kind")?;
        anyhow::ensure!(
            matches!(kind, "MapPiece" | "Object"),
            "unsupported render part {kind}"
        );
        let source = SourceMasks::from_row(row)?;
        Ok(Some(Self {
            map: map.to_string(),
            name: row["name"]
                .as_str()
                .context("missing part name")?
                .to_string(),
            entity_id: i32::try_from(row["entity_id"].as_i64().context("missing entity ID")?)?,
            entity_groups: words::<8>(&row["entity_group_ids"])?.map(|v| v as i32),
            effective: source.effective(kind),
            source,
        }))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventState {
    pub by_name: BTreeMap<String, bool>,
    /// Includes entity-group IDs when supplied by the event host.
    pub by_entity: BTreeMap<i32, bool>,
}

impl EventState {
    fn enabled(&self, part: &VisibilityPart) -> Option<bool> {
        self.by_name
            .get(&part.name)
            .copied()
            .or_else(|| {
                (part.entity_id > 0)
                    .then(|| self.by_entity.get(&part.entity_id).copied())
                    .flatten()
            })
            .or_else(|| {
                part.entity_groups
                    .iter()
                    .filter(|&&id| id > 0)
                    .find_map(|id| self.by_entity.get(id).copied())
            })
    }
}

/// Job C input, default empty. Replace snapshots so event restarts release stale overrides.
#[derive(Resource, Default)]
pub struct AuthoredVisibility {
    pub maps: BTreeMap<String, EventState>,
}

/// Arena staging input, kept separate so an arena restart cannot erase Job C's state.
#[derive(Resource, Default)]
pub struct ArenaVisibility {
    pub maps: BTreeMap<String, EventState>,
}

/// Map items' marker parts taken (`crate::play` pickups): a taken one is hidden, and the
/// snapshot has no opinion on any other part.
#[derive(Resource, Default)]
pub struct TakenVisibility {
    pub maps: BTreeMap<String, EventState>,
}

/// The map events' live state (RSekiro's `src/enemy/world.rs` runs the blocks' EMEVD): map
/// parts (2012[1] Activate Map Part) and objects (destroyed, deactivated or opened) by entity id
/// or entity group. It comes before the arena's, Job C's and the story's opinion, after the
/// taken items'; a part it does not name is theirs.
#[derive(Resource, Default)]
pub struct EventVisibility {
    pub maps: BTreeMap<String, EventState>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskContribution {
    pub source_map: String,
    pub collision: String,
    pub connection: Option<String>,
    pub mask: Mask,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockRequest {
    pub normal: Mask,
    pub additional_backread: Mask,
    pub sources: Vec<MaskContribution>,
}

impl BlockRequest {
    /// Merge contributions from multiple source blocks; never let the last source win.
    pub fn merge(&mut self, other: Self) {
        union(&mut self.normal, other.normal);
        union(&mut self.additional_backread, other.additional_backread);
        self.sources.extend(other.sources);
    }
}

/// D provides block residency and asset readiness, plus the accumulated request masks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockVisibility {
    pub request: BlockRequest,
    pub draw: Mask,
    pub resident: bool,
    pub ready: bool,
    /// `--groups off` diagnostic override; event and asset gates still apply.
    pub groups_off: bool,
}

#[derive(Resource, Default)]
pub struct VisibilityBlocks(pub BTreeMap<String, BlockVisibility>);

#[derive(Clone, Debug)]
struct Connection {
    name: String,
    source: String,
    targets: Vec<String>,
    mask: Mask,
}

/// Decode once when loading a canonical layout, rather than parsing JSON each frame.
#[derive(Clone, Debug)]
pub struct VisibilityMap {
    pub map: String,
    pub collisions: BTreeMap<String, Mask>,
    connections: Vec<Connection>,
    pub zero_primary: BTreeMap<String, usize>,
    pub zero_effective: BTreeMap<String, usize>,
    pub zero_map_piece_fallbacks: usize,
}

impl VisibilityMap {
    pub fn from_layout(map: &str, layout: &Value) -> Result<Option<Self>> {
        if !supported(map) {
            return Ok(None);
        }
        anyhow::ensure!(
            layout["layout_version"] == 2,
            "Job E requires layout version 2 for {map}"
        );
        let mut result = Self {
            map: map.to_string(),
            collisions: BTreeMap::new(),
            connections: Vec::new(),
            zero_primary: BTreeMap::new(),
            zero_effective: BTreeMap::new(),
            zero_map_piece_fallbacks: 0,
        };
        for part in layout["parts"]
            .as_array()
            .context("missing canonical parts")?
        {
            let kind = part["kind"].as_str().context("missing part kind")?;
            let name = part["name"].as_str().context("missing part name")?;
            if !part["display_data"].is_null() {
                let source = SourceMasks::from_row(part).with_context(|| name.to_string())?;
                let effective = source.effective(kind);
                if source.primary_draw == [0; 8] {
                    *result.zero_primary.entry(kind.to_string()).or_default() += 1;
                }
                // Count pre-construction zero masks, including the MapPiece fallback.
                if effective.draw == [0; 8] || effective.zero_map_piece {
                    *result.zero_effective.entry(kind.to_string()).or_default() += 1;
                }
                result.zero_map_piece_fallbacks += usize::from(effective.zero_map_piece);
            }
            if kind == "Collision" {
                result
                    .collisions
                    .insert(name.to_string(), display_mask(part)?);
            }
            if kind == "ConnectCollision" {
                let connection = &part["connection"];
                let Some(source) = connection["source_collision"]["name"].as_str() else {
                    continue; // Explicit unbound/invalid references remain nonparticipating.
                };
                let targets = connection["target_blocks"]
                    .as_array()
                    .context("missing target blocks")?
                    .iter()
                    .map(|target| {
                        target
                            .as_str()
                            .context("invalid target block")
                            .map(str::to_string)
                    })
                    .collect::<Result<Vec<_>>>()?;
                result.connections.push(Connection {
                    name: name.to_string(),
                    source: source.to_string(),
                    targets,
                    mask: display_mask(part)?,
                });
            }
        }
        info!(
            "visibility masks {map}: zero primary {:?}, zero effective before construction {:?}, MapPiece all-bit fallbacks {}",
            result.zero_primary, result.zero_effective, result.zero_map_piece_fallbacks
        );
        Ok(Some(result))
    }

    /// Current/retained occupied collisions contribute to their own block; enabled attached
    /// connections contribute ONLY their authored target masks to approved target worlds.
    /// D applies the runtime source-enable/target-world gates with these two callbacks.
    pub fn requests<'a>(
        &self,
        participating: impl IntoIterator<Item = &'a str>,
        connection_enabled: impl Fn(&str, &str) -> bool,
        target_enabled: impl Fn(&str) -> bool,
    ) -> BTreeMap<String, BlockRequest> {
        let mut requests = BTreeMap::<String, BlockRequest>::new();
        for collision in participating {
            let Some(&mask) = self.collisions.get(collision) else {
                continue;
            };
            let request = requests.entry(self.map.clone()).or_default();
            union(&mut request.normal, mask);
            request.sources.push(MaskContribution {
                source_map: self.map.clone(),
                collision: collision.to_string(),
                connection: None,
                mask,
            });
            for connection in &self.connections {
                if connection.source != collision
                    || !connection_enabled(collision, &connection.name)
                {
                    continue;
                }
                for target in &connection.targets {
                    if !target_enabled(target) {
                        continue;
                    }
                    let request = requests.entry(target.clone()).or_default();
                    union(&mut request.normal, connection.mask);
                    request.sources.push(MaskContribution {
                        source_map: self.map.clone(),
                        collision: collision.to_string(),
                        connection: Some(connection.name.clone()),
                        mask: connection.mask,
                    });
                }
            }
        }
        requests
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyRequest {
    None,
    Backread,
    Normal,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct VisibilityDecision {
    pub residency: ResidencyRequest,
    pub event_enabled: bool,
    pub group_visible: bool,
    pub visible: bool,
}

/// One pure decision shared by runtime and audits. A backread request is not a draw request.
pub fn decide(
    part: &VisibilityPart,
    block: &BlockVisibility,
    event_enabled: bool,
) -> VisibilityDecision {
    let residency = if block.groups_off || intersects(part.effective.draw, block.request.normal) {
        ResidencyRequest::Normal
    } else if intersects(part.effective.backread, block.request.additional_backread) {
        ResidencyRequest::Backread
    } else {
        ResidencyRequest::None
    };
    let group_visible = block.groups_off || intersects(part.effective.draw, block.draw);
    VisibilityDecision {
        residency,
        event_enabled,
        group_visible,
        visible: block.resident
            && block.ready
            && event_enabled
            && group_visible
            && residency != ResidencyRequest::None,
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct VisibilityInputs;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct VisibilityWrite;

/// Install once. All event/residency producers run in or before `VisibilityInputs`.
pub struct MapVisibilityPlugin;

impl Plugin for MapVisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AuthoredVisibility>()
            .init_resource::<ArenaVisibility>()
            .init_resource::<TakenVisibility>()
            .init_resource::<EventVisibility>()
            .init_resource::<VisibilityBlocks>()
            .configure_sets(
                PostUpdate,
                (VisibilityInputs, VisibilityWrite)
                    .chain()
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(
                PostUpdate,
                write_visibility
                    .in_set(VisibilityWrite)
                    .run_if(visibility_changed()),
            );
    }
}

type VisibilityChanges = Or<(
    Changed<VisibilityPart>,
    Changed<Visibility>,
    Without<VisibilityDecision>,
)>;

// Keep this input list in sync with write_visibility below.
fn visibility_changed() -> impl SystemCondition<()> {
    IntoSystem::into_system(
        resource_changed::<VisibilityBlocks>
            .or_eager(resource_changed::<AuthoredVisibility>)
            .or_eager(resource_changed::<ArenaVisibility>)
            .or_eager(resource_changed::<TakenVisibility>)
            .or_eager(resource_changed_or_removed::<EventVisibility>)
            .or_eager(resource_changed_or_removed::<super::story::MapStory>)
            .or_eager(
                |parts: Query<(&VisibilityPart, &Visibility), VisibilityChanges>| !parts.is_empty(),
            ),
    )
}

fn write_visibility(
    mut commands: Commands,
    blocks: Res<VisibilityBlocks>,
    authored: Res<AuthoredVisibility>,
    (taken, arena): (Res<TakenVisibility>, Res<ArenaVisibility>),
    (story, events): (
        Option<Res<super::story::MapStory>>,
        Option<Res<EventVisibility>>,
    ),
    mut last_debug: Local<BTreeMap<Entity, Value>>,
    mut parts: Query<(
        Entity,
        &VisibilityPart,
        &mut Visibility,
        Option<&VisibilityDecision>,
    )>,
) {
    let _timer = crate::perf_log::SystemTimer::start("write_visibility");
    static DEBUG: std::sync::OnceLock<(bool, Option<String>)> = std::sync::OnceLock::new();
    let (debug, debug_part) = DEBUG.get_or_init(|| {
        let args: Vec<_> = std::env::args().collect();
        (
            args.iter().any(|a| a == "--visibility-debug"),
            crate::arg_value(&args, "--visibility-part"),
        )
    });
    for (entity, part, mut visibility, previous) in &mut parts {
        let default_block = BlockVisibility::default();
        let block = blocks.0.get(&part.map).unwrap_or(&default_block);
        let event = taken
            .maps
            .get(&part.map)
            .and_then(|s| s.enabled(part))
            .or_else(|| events.as_ref()?.maps.get(&part.map)?.enabled(part))
            .or_else(|| arena.maps.get(&part.map).and_then(|s| s.enabled(part)))
            .or_else(|| authored.maps.get(&part.map).and_then(|s| s.enabled(part)))
            .or_else(|| {
                story
                    .as_ref()?
                    .maps
                    .get(&part.map)?
                    .enabled(part.entity_id, &part.name)
            })
            .unwrap_or(true);
        let decision = decide(part, block, event);
        let desired = if decision.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != desired {
            *visibility = desired;
        }
        if debug_part.as_deref() == Some(&part.name) {
            let trace = diagnostic(part, block, &decision);
            // Producers may republish the same snapshot each frame. Log input changes,
            // not Bevy's mutable-resource tick, so captures do not flood the log.
            if last_debug.get(&entity) != Some(&trace) {
                info!("{trace}");
                last_debug.insert(entity, trace);
            }
        }
        if previous != Some(&decision) {
            if *debug {
                info!(map = part.map, part = part.name, ?decision, source = ?part.source,
                    effective = ?part.effective, collisions = ?block.request.sources,
                    normal = ?block.request.normal, additional = ?block.request.additional_backread,
                    draw = ?block.draw, resident = block.resident, ready = block.ready,
                    "map visibility decision");
            }
            commands.entity(entity).insert(decision);
        }
    }
}

/// Capture/debug consumers can inspect any part, including one whose decision is unchanged.
pub fn diagnostic(
    part: &VisibilityPart,
    block: &BlockVisibility,
    decision: &VisibilityDecision,
) -> Value {
    serde_json::json!({
        "map": part.map, "part": part.name, "entity_id": part.entity_id,
        "entity_groups": part.entity_groups,
        "source_masks": {
            "primary_draw": part.source.primary_draw,
            "alternative_draw": part.source.alternative_draw,
            "draw_selector": part.source.draw_selector,
            "primary_backread": part.source.primary_backread,
            "alternative_backread": part.source.alternative_backread,
            "backread_selector": part.source.backread_selector,
            "condition1": part.source.condition1, "condition2": part.source.condition2,
        },
        "effective_draw": part.effective.draw, "effective_backread": part.effective.backread,
        "preprocessed": part.effective.preprocess, "zero_map_piece_fallback": part.effective.zero_map_piece,
        "normal_request": block.request.normal, "additional_backread": block.request.additional_backread,
        "draw_request": block.draw, "resident": block.resident, "ready": block.ready,
        "groups_off": block.groups_off,
        "sources": block.request.sources.iter().map(|s| serde_json::json!({
            "map": s.source_map, "collision": s.collision, "connection": s.connection, "mask": s.mask,
        })).collect::<Vec<_>>(),
        "residency_request": format!("{:?}", decision.residency),
        "event_enabled": decision.event_enabled, "group_visible": decision.group_visible,
        "visible": decision.visible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mask(word: u32) -> Mask {
        let mut m = [0; 8];
        m[0] = word;
        m
    }

    fn source() -> SourceMasks {
        SourceMasks {
            primary_draw: mask(0b0011),
            alternative_draw: mask(0b1100),
            draw_selector: mask(0b0101),
            primary_backread: mask(0b1010),
            alternative_backread: mask(0b0101),
            backread_selector: mask(0b0110),
            condition1: 0,
            condition2: 1,
        }
    }

    fn part(kind: &str) -> VisibilityPart {
        let source = source();
        VisibilityPart {
            map: "m11_01_00_00".into(),
            name: "floor".into(),
            entity_id: 1101000,
            entity_groups: [1101500, -1, -1, -1, -1, -1, -1, -1],
            effective: source.effective(kind),
            source,
        }
    }

    #[test]
    fn visibility_waits_for_input_changes_and_includes_new_parts() {
        #[derive(Resource, Default)]
        struct Runs(usize);
        let mut app = App::new();
        let row = part("MapPiece");
        app.init_resource::<AuthoredVisibility>()
            .init_resource::<ArenaVisibility>()
            .init_resource::<TakenVisibility>()
            .init_resource::<Runs>()
            .insert_resource(VisibilityBlocks(BTreeMap::from([(
                row.map.clone(),
                BlockVisibility {
                    resident: true,
                    ready: true,
                    groups_off: true,
                    ..default()
                },
            )])))
            .add_systems(
                Update,
                (write_visibility, |mut runs: ResMut<Runs>| runs.0 += 1)
                    .chain()
                    .run_if(visibility_changed()),
            );
        let first = app
            .world_mut()
            .spawn((row.clone(), Visibility::Hidden))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let runs = app.world().resource::<Runs>().0;
        app.update();
        assert_eq!(app.world().resource::<Runs>().0, runs);
        app.world_mut()
            .resource_mut::<ArenaVisibility>()
            .maps
            .insert(
                row.map.clone(),
                EventState {
                    by_name: BTreeMap::from([(row.name.clone(), false)]),
                    ..default()
                },
            );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(first).unwrap(),
            Visibility::Hidden
        );
        let mut late = row.clone();
        late.name = "late".into();
        let second = app.world_mut().spawn((late, Visibility::Hidden)).id();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(second).unwrap(),
            Visibility::Inherited
        );
        app.world_mut()
            .get_mut::<VisibilityPart>(first)
            .unwrap()
            .name = "renamed".into();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(first).unwrap(),
            Visibility::Inherited
        );
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .get_mut(&row.map)
            .unwrap()
            .ready = false;
        app.update();
        for entity in [first, second] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).unwrap(),
                Visibility::Hidden
            );
        }
        // Removing an optional story resource must also invalidate its former selection.
        app.world_mut()
            .init_resource::<super::super::story::MapStory>();
        app.update();
        let runs = app.world().resource::<Runs>().0;
        app.world_mut()
            .remove_resource::<super::super::story::MapStory>();
        app.update();
        assert_eq!(app.world().resource::<Runs>().0, runs + 1);
    }

    #[test]
    fn loader_selects_each_bit_only_at_the_confirmed_gate() {
        for (condition1, condition2, draw, backread) in [
            (0, 0, 0b0011, 0b1010),
            (0, 1, 0b1001, 0b0011),
            (0, 255, 0b1001, 0b0011),
            (1, 0, 0b0011, 0b1010),
            (1, 1, 0b0011, 0b1010),
        ] {
            let source = SourceMasks {
                condition1,
                condition2,
                ..source()
            };
            for kind in ["MapPiece", "Object", "Collision", "Enemy"] {
                let effective = source.effective(kind);
                let (draw, backread) = if kind == "Enemy" {
                    (0b0011, 0b1010)
                } else {
                    (draw, backread)
                };
                assert_eq!(
                    effective.draw,
                    mask(draw),
                    "{kind} {condition1}/{condition2}"
                );
                assert_eq!(
                    effective.backread,
                    mask(if kind == "Collision" {
                        backread | draw
                    } else {
                        backread
                    })
                );
            }
        }
    }

    #[test]
    fn selectors_apply_to_the_last_word_of_both_256_bit_masks() {
        let mut source = source();
        source.primary_draw[7] = 0xaaaa;
        source.alternative_draw[7] = 0x5555;
        source.draw_selector[7] = 0xf0f0;
        source.primary_backread[7] = 0xcccc;
        source.alternative_backread[7] = 0x3333;
        source.backread_selector[7] = 0xff00;
        let effective = source.effective("MapPiece");
        assert_eq!(effective.draw[7], 0xa5a5);
        assert_eq!(effective.backread[7], 0xcc33);
    }

    #[test]
    fn zero_fallback_is_map_piece_only_and_still_obeys_event_and_readiness() {
        let source = SourceMasks {
            primary_draw: [0; 8],
            condition2: 0,
            ..source()
        };
        let mut p = part("MapPiece");
        p.source = source.clone();
        p.effective = source.effective("MapPiece");
        let mut block = BlockVisibility {
            resident: true,
            ready: true,
            draw: mask(1),
            request: BlockRequest {
                normal: mask(1),
                ..default()
            },
            ..default()
        };
        assert!(decide(&p, &block, true).visible);
        assert!(!decide(&p, &block, false).visible);
        block.ready = false;
        assert!(!decide(&p, &block, true).visible);
        for kind in ["Object", "Collision", "Enemy"] {
            assert_eq!(source.effective(kind).draw, [0; 8], "{kind}");
        }
        block.ready = true;
        block.draw = [0; 8];
        assert!(!decide(&p, &block, true).visible);
    }

    #[test]
    fn backread_is_separate_residency_and_does_not_enable_drawing() {
        let p = part("Object");
        let mut block = BlockVisibility {
            resident: true,
            ready: true,
            request: BlockRequest {
                additional_backread: mask(2),
                ..default()
            },
            ..default()
        };
        let d = decide(&p, &block, true);
        assert_eq!(d.residency, ResidencyRequest::Backread);
        assert!(!d.visible);
        block.request.normal = mask(8);
        block.draw = mask(8);
        assert_eq!(decide(&p, &block, true).residency, ResidencyRequest::Normal);
        assert!(decide(&p, &block, true).visible);
        block.resident = false;
        assert!(!decide(&p, &block, true).visible);
    }

    #[test]
    fn connections_union_target_masks_and_obey_source_and_world_gates() {
        let mut row =
            json!({"display_group": {"condition": 255, "disp_groups": [i32::MIN,0,0,0,0,0,0,0]}});
        let mut expected = mask(0x80000000);
        expected[7] = 0x80000000;
        assert_eq!(display_mask(&row).unwrap(), expected);
        row["display_group"]["condition"] = json!(256);
        assert!(display_mask(&row).is_err());
        let map = VisibilityMap {
            map: "m11_01_00_00".into(),
            collisions: BTreeMap::from([
                ("roof".into(), mask(1)),
                ("stairs".into(), mask(2)),
                ("helper".into(), [0; 8]),
            ]),
            connections: vec![
                Connection {
                    name: "a".into(),
                    source: "roof".into(),
                    targets: vec!["m11_00_00_00".into()],
                    mask: mask(16),
                },
                Connection {
                    name: "b".into(),
                    source: "roof".into(),
                    targets: vec!["m11_00_00_00".into(), "m11_02_00_00".into()],
                    mask: mask(32),
                },
                Connection {
                    name: "c".into(),
                    source: "stairs".into(),
                    targets: vec!["m11_00_00_00".into()],
                    mask: mask(64),
                },
            ],
            zero_primary: default(),
            zero_effective: default(),
            zero_map_piece_fallbacks: 0,
        };
        let requests = map.requests(
            ["roof", "stairs"],
            |_, name| name != "c",
            |target| target != "m11_02_00_00",
        );
        assert_eq!(requests["m11_01_00_00"].normal, mask(3));
        assert_eq!(requests["m11_00_00_00"].normal, mask(48));
        assert_eq!(requests["m11_00_00_00"].sources.len(), 2);
        assert!(!requests.contains_key("m11_02_00_00"));
        let mut requests = map.requests(["roof"], |_, _| true, |_| true);
        let second = map.requests(["stairs"], |_, _| true, |_| true);
        for (map, request) in second {
            requests.entry(map).or_default().merge(request);
        }
        assert_eq!(requests["m11_00_00_00"].normal, mask(112));
        let helper = map.requests(["helper"], |_, _| true, |_| true);
        assert_eq!(helper["m11_01_00_00"].normal, [0; 8]);
        assert!(
            map.requests(std::iter::empty::<&str>(), |_, _| true, |_| true)
                .is_empty()
        );
    }

    #[test]
    fn authored_identity_precedence_ignores_sentinel_ids() {
        let mut p = part("MapPiece");
        let mut state = EventState {
            by_entity: BTreeMap::from([(-1, false), (0, false), (1101500, false)]),
            ..default()
        };
        assert_eq!(state.enabled(&p), Some(false));
        state.by_entity.insert(p.entity_id, true);
        assert_eq!(state.enabled(&p), Some(true));
        state.by_name.insert(p.name.clone(), false);
        assert_eq!(state.enabled(&p), Some(false));
        p.entity_id = -1;
        p.entity_groups = [0; 8];
        state.by_name.clear();
        assert_eq!(state.enabled(&p), None);
    }

    #[test]
    fn installed_layouts_keep_zero_parts_and_connections_auditable() {
        let cache = sekiro_sim::cache::root();
        for (map, count, connections, fallbacks) in
            [("m11_00_00_00", 166, 98, 1), ("m11_01_00_00", 221, 298, 0)]
        {
            let layout: Value = serde_json::from_slice(
                &std::fs::read(cache.join(format!("maps/{map}/layout.json"))).unwrap(),
            )
            .unwrap();
            let definition = VisibilityMap::from_layout(map, &layout).unwrap().unwrap();
            assert_eq!(definition.collisions.len(), count);
            assert_eq!(definition.connections.len(), connections);
            let rendered: Vec<_> = layout["parts"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| matches!(p["kind"].as_str(), Some("MapPiece" | "Object")))
                .map(|p| VisibilityPart::from_row(map, p).unwrap().unwrap())
                .collect();
            assert!(!rendered.is_empty());
            println!(
                "{map}: fallback parts {:?}",
                rendered
                    .iter()
                    .filter(|p| p.effective.zero_map_piece)
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
            );
            println!(
                "{map}: zero primary {:?}; zero after preprocessing {:?}; MapPiece all-bit fallbacks {}",
                definition.zero_primary,
                definition.zero_effective,
                definition.zero_map_piece_fallbacks
            );
            assert_eq!(definition.zero_map_piece_fallbacks, fallbacks);
            let msb = sekiro_formats::msb::parse(
                &std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap(),
            )
            .unwrap();
            for collision in msb
                .parts
                .iter()
                .filter(|p| p.kind == sekiro_formats::msb::PartKind::Collision)
            {
                let display = collision.display_group.as_ref().unwrap();
                let mut expected = display.disp_groups.map(|w| w as u32);
                if (0..256).contains(&display.condition) {
                    expected[display.condition as usize / 32] |=
                        1 << (display.condition as usize % 32);
                }
                let requests =
                    definition.requests([collision.name.as_str()], |_, _| false, |_| true);
                assert_eq!(requests[map].normal, expected, "{map} {}", collision.name);
            }
        }
        assert!(
            VisibilityMap::from_layout("m10_00_00_00", &Value::Null)
                .unwrap()
                .is_none()
        );
        assert!(
            VisibilityPart::from_row("m10_00_00_00", &Value::Null)
                .unwrap()
                .is_none()
        );
    }
}
