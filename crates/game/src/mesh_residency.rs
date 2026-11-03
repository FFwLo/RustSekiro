//! Upload reusable source meshes only while an entity references them.
//! CPU sources remain available for streamed instances and static-batch fallback.
use bevy::asset::RenderAssetUsages;
use bevy::platform::hash::FixedHasher;
use bevy::prelude::*;
use bevy::render::mesh::{MeshMetadataFallbackMesh, RenderMesh};
use bevy::render::render_asset::{AssetExtractionSystems, ExtractedAssets, RenderAssets};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

/// Meshes retired from the GPU or still re-uploading, shared with the main world.
#[derive(Resource, Clone, Default)]
pub(crate) struct OffGpuMeshes(pub(crate) Arc<Mutex<HashSet<AssetId<Mesh>>>>);

fn publish_off_gpu(
    off_gpu: Res<OffGpuMeshes>,
    mut resident: ResMut<Resident>,
    meshes: Res<RenderAssets<RenderMesh>>,
) {
    let resident = &mut *resident;
    let before = resident.resuming.len();
    resident.resuming.retain(|id| meshes.get(*id).is_none());
    if resident.publish || resident.resuming.len() != before {
        resident.publish = false;
        let mut off_gpu = off_gpu.0.lock().unwrap();
        off_gpu.clear();
        off_gpu.extend(resident.suspended.iter().chain(&resident.resuming).copied());
    }
}

#[derive(Resource, Default)]
struct SharedMeshes {
    buckets: HashMap<u64, Vec<AssetId<Mesh>>>,
    aliases: HashMap<AssetId<Mesh>, Handle<Mesh>>,
    seen: HashSet<AssetId<Mesh>>,
}

#[derive(Resource, Default)]
pub(crate) struct MeshRefs {
    entities: HashMap<Entity, AssetId<Mesh>>,
    counts: HashMap<AssetId<Mesh>, u32>,
    revision: u64,
}

impl MeshRefs {
    pub(crate) fn contains(&self, id: AssetId<Mesh>) -> bool {
        self.counts.contains_key(&id)
    }

    /// Changes whenever a mesh gains its first or loses its last entity.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    fn remove(&mut self, entity: Entity) {
        if let Some(id) = self.entities.remove(&entity) {
            let count = self.counts.get_mut(&id).unwrap();
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&id);
                self.revision += 1;
            }
        }
    }
}

#[derive(Resource, Default)]
struct Resident {
    revision: Option<u64>,
    referenced: HashSet<AssetId<Mesh>>,
    suspended: HashSet<AssetId<Mesh>>,
    resuming: HashSet<AssetId<Mesh>>,
    retirement_started: bool,
    publish: bool,
}

pub(crate) fn install(app: &mut App) {
    let off_gpu = OffGpuMeshes::default();
    app.insert_resource(off_gpu.clone())
        .init_resource::<MeshRefs>()
        .init_resource::<SharedMeshes>()
        .add_systems(Update, share_sources)
        .add_systems(Last, track);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(off_gpu)
            .init_resource::<Resident>()
            .add_systems(ExtractSchedule, extract.after(AssetExtractionSystems))
            .add_systems(Render, publish_off_gpu.in_set(RenderSystems::PostCleanup));
    }
}

fn share_sources(
    assets: Res<Assets<Mesh>>,
    server: Res<AssetServer>,
    mut state: ResMut<SharedMeshes>,
    mut changed: Query<&mut Mesh3d, Changed<Mesh3d>>,
    mut events: MessageReader<bevy::asset::AssetEvent<Mesh>>,
) {
    for event in events.read() {
        if let bevy::asset::AssetEvent::Unused { id } = event {
            state.aliases.remove(id);
            state.seen.remove(id);
        }
    }
    for mut handle in &mut changed {
        let id = handle.id();
        if let Some(canonical) = state.aliases.get(&id) {
            handle.0 = canonical.clone();
            continue;
        }
        if state.seen.contains(&id) || !crate::map::world::asset_under(&server, id, "maps") {
            continue;
        }
        let Some(mesh) = assets.get(id) else {
            continue;
        };
        if !mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD)
            || mesh.try_attributes().is_err()
            || mesh.contains_attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
            || mesh.morph_targets().is_some()
        {
            continue;
        }
        let mut hash = FixedHasher.build_hasher();
        mesh.primitive_topology().hash(&mut hash);
        for (attribute, values) in mesh.attributes() {
            attribute.id.hash(&mut hash);
            attribute.format.hash(&mut hash);
            values.get_bytes().hash(&mut hash);
        }
        mesh.get_index_buffer_bytes().hash(&mut hash);
        let key = hash.finish();
        let bucket = state.buckets.entry(key).or_default();
        bucket.retain(|candidate| assets.contains(*candidate));
        let canonical = bucket
            .iter()
            .copied()
            .find(|candidate| assets.get(*candidate) == Some(mesh));
        if let Some(canonical) = canonical.and_then(|id| server.get_id_handle(id)) {
            state.aliases.insert(id, canonical.clone());
            handle.0 = canonical;
        } else {
            bucket.push(id);
            state.seen.insert(id);
        }
    }
}

pub(crate) fn track(
    changed: Query<(Entity, &Mesh3d), Changed<Mesh3d>>,
    mut removed: RemovedComponents<Mesh3d>,
    mut refs: ResMut<MeshRefs>,
) {
    for entity in removed.read() {
        refs.remove(entity);
    }
    for (entity, mesh) in &changed {
        let id = mesh.id();
        if refs.entities.get(&entity) == Some(&id) {
            continue;
        }
        refs.remove(entity);
        refs.entities.insert(entity, id);
        let count = refs.counts.entry(id).or_default();
        *count += 1;
        if *count == 1 {
            refs.revision += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn extract(
    refs: Extract<Res<MeshRefs>>,
    assets: Extract<Res<Assets<Mesh>>>,
    mut events: Extract<MessageReader<bevy::asset::AssetEvent<Mesh>>>,
    fallback: Res<MeshMetadataFallbackMesh>,
    mut resident: ResMut<Resident>,
    mut extracted: ResMut<ExtractedAssets<RenderMesh>>,
    warmup: Option<Res<crate::pipeline_warmup::PipelineWarmup>>,
    prepared: Res<RenderAssets<RenderMesh>>,
) {
    let ready = warmup.is_none_or(|warmup| warmup.ready());
    let changed = resident.revision != Some(refs.revision);
    let asset_event = asset_set_changed(events.read());
    let suspended = resident.suspended.len();
    if changed {
        resident.revision = Some(refs.revision);
        let mut referenced: HashSet<_> = refs.counts.keys().copied().collect();
        referenced.insert(fallback.0.id());
        let retired: Vec<_> = resident
            .referenced
            .difference(&referenced)
            .copied()
            .collect();
        for id in retired {
            if ready
                && assets
                    .get(id)
                    .is_some_and(|m| m.asset_usage.contains(RenderAssetUsages::MAIN_WORLD))
            {
                extracted.removed.insert(id);
                resident.suspended.insert(id);
            }
        }
        resident.referenced = referenced;
    }
    let start_retirement = ready && !resident.retirement_started;
    if start_retirement {
        for (id, _) in prepared.iter() {
            if !resident.referenced.contains(&id)
                && assets
                    .get(id)
                    .is_some_and(|mesh| mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD))
            {
                extracted.removed.insert(id);
                resident.suspended.insert(id);
            }
        }
        resident.retirement_started = true;
    }
    reconcile(
        &assets,
        &mut resident,
        &mut extracted,
        changed || asset_event || start_retirement,
        ready,
    );
    resident.publish |= changed || start_retirement || resident.suspended.len() != suspended;
}

/// Whether any mesh asset was added, removed or dropped. Meshes rewritten every frame (cloth,
/// effect batches, the sky) only send `Modified`, which neither adds nor removes an asset, so
/// they need no reconcile pass over the suspended set (#70). Reads every event, so none carry
/// over to the next frame.
fn asset_set_changed<'a>(events: impl Iterator<Item = &'a bevy::asset::AssetEvent<Mesh>>) -> bool {
    events
        .filter(|event| !matches!(event, bevy::asset::AssetEvent::Modified { .. }))
        .count()
        > 0
}

fn reconcile(
    assets: &Assets<Mesh>,
    resident: &mut Resident,
    extracted: &mut ExtractedAssets<RenderMesh>,
    changed: bool,
    ready: bool,
) {
    // Retire GLTF primitives that have not been instantiated yet. Their main-world data
    // is still owned by the GLTF, so it can be extracted when a later LOD needs it.
    extracted.extracted.retain(|(id, mesh)| {
        let keep = !ready
            || !mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD)
            || resident.referenced.contains(id);
        if !keep {
            resident.suspended.insert(*id);
            extracted.added.remove(id);
            extracted.modified.remove(id);
            extracted.removed.insert(*id);
        }
        keep
    });
    if !changed {
        return;
    }
    let resume: Vec<_> = resident
        .suspended
        .intersection(&resident.referenced)
        .copied()
        .collect();
    for id in resume {
        resident.suspended.remove(&id);
        if !extracted.added.contains(&id)
            && let Some(mesh) = assets.get(id)
            && mesh.try_attributes().is_ok()
        {
            extracted.extracted.push((id, mesh.clone()));
            extracted.added.insert(id);
            resident.resuming.insert(id);
        }
        extracted.removed.remove(&id);
    }
    resident.suspended.retain(|id| assets.contains(*id));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #70: meshes rewritten every frame must not trigger the reconcile pass.
    #[test]
    fn rewritten_meshes_do_not_reconcile() {
        use bevy::asset::AssetEvent;
        let id = AssetId::<Mesh>::Uuid {
            uuid: bevy::asset::uuid::Uuid::from_u128(1),
        };
        let rewrites = [AssetEvent::Modified { id }, AssetEvent::Modified { id }];
        assert!(!asset_set_changed(rewrites.iter()));
        assert!(!asset_set_changed([].iter()));
        for event in [
            AssetEvent::Added { id },
            AssetEvent::Removed { id },
            AssetEvent::Unused { id },
        ] {
            assert!(asset_set_changed(
                [AssetEvent::Modified { id }, event].iter()
            ));
        }
    }

    #[test]
    fn references_change_revision_only_when_membership_changes() {
        let mut app = App::new();
        app.init_resource::<MeshRefs>().add_systems(Last, track);
        let mesh = Handle::<Mesh>::default();
        let first = app.world_mut().spawn(Mesh3d(mesh.clone())).id();
        app.update();
        let revision = app.world().resource::<MeshRefs>().revision;
        let second = app.world_mut().spawn(Mesh3d(mesh.clone())).id();
        app.update();
        assert_eq!(app.world().resource::<MeshRefs>().revision, revision);
        app.world_mut().entity_mut(first).remove::<Mesh3d>();
        app.update();
        assert_eq!(app.world().resource::<MeshRefs>().revision, revision);
        assert!(app.world().resource::<MeshRefs>().contains(mesh.id()));
        app.world_mut().despawn(second);
        app.update();
        assert_eq!(app.world().resource::<MeshRefs>().revision, revision + 1);
        assert!(!app.world().resource::<MeshRefs>().contains(mesh.id()));
    }

    #[test]
    fn merged_sources_release_gpu_buffers_and_can_be_restored() {
        let mut assets = Assets::<Mesh>::default();
        let handle = assets.add(Mesh::from(Triangle3d::new(Vec3::ZERO, Vec3::X, Vec3::Y)));
        let id = handle.id();
        let mut resident = Resident::default();
        let mut extracted = ExtractedAssets::<RenderMesh>::default();
        extracted
            .extracted
            .push((id, assets.get(id).unwrap().clone()));
        extracted.added.insert(id);
        reconcile(&assets, &mut resident, &mut extracted, true, false);
        assert_eq!(
            extracted.extracted.len(),
            1,
            "warmup retains unused layouts"
        );
        reconcile(&assets, &mut resident, &mut extracted, true, true);
        assert!(extracted.extracted.is_empty());
        assert!(extracted.removed.contains(&id));
        assert!(!extracted.added.contains(&id));
        resident.referenced.insert(id);
        reconcile(&assets, &mut resident, &mut extracted, true, true);
        assert!(!extracted.removed.contains(&id));
        assert_eq!(extracted.extracted[0].1.count_vertices(), 3);
        assert!(extracted.added.contains(&id));
    }
}
