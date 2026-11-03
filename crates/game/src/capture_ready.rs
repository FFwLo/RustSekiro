//! Late mesh checks: meshes extracted before they reached the GPU are specialized again.
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};

/// Mesh entities extracted before their mesh reached the GPU.
///
/// Bevy 0.19's material specialization (`specialize_material_meshes` in bevy_pbr) skips an
/// entity whose mesh asset is not prepared yet without queueing a retry, unlike a missing
/// material, and specializes it again only when the entity changes. With the per-frame upload
/// budget (`crate::map::upload_budget`) a new entity can be extracted before its mesh is
/// prepared and then stays undrawn. The render world reports meshes that were not prepared
/// when first seen and later are, and [`watch_meshes`] marks their entities changed so Bevy
/// specializes them again. This runs for the whole session, so meshes streamed or levels of
/// detail spawned during play are covered too.
///
/// It also counts the meshes the world is still loading, for the play gate
/// ([`LateMeshes::loading`]). Bevy marks `Mesh3d` changed every frame for an entity whose mesh
/// or material asset is rewritten (effect batches, cloth, the sky's animated materials), and a
/// rewritten mesh leaves the render world until it is prepared again, so the gate counts only
/// meshes never seen prepared before and skips [`RuntimeMesh`] entities.
#[derive(Clone, Default, Resource)]
pub(crate) struct LateMeshes(Arc<Mutex<LateState>>);

/// A mesh entity that keeps appearing during play (an effect batch), so the play gate does not
/// wait for its mesh. It is still specialized again when its mesh arrives late.
#[derive(Clone, Copy, Component, Default)]
pub(crate) struct RuntimeMesh;

#[derive(Default)]
struct LateState {
    /// Main to render: meshes of entities that appeared since the last check.
    new: Vec<AssetId<Mesh>>,
    /// Render: meshes not prepared when first checked.
    waiting: HashSet<AssetId<Mesh>>,
    /// Render to main: checked meshes that were prepared in time, and waiting ones now prepared.
    on_time: Vec<AssetId<Mesh>>,
    ready: Vec<AssetId<Mesh>>,
    /// Reported meshes of the world's own entities not yet seen prepared: what the play gate
    /// waits for.
    loading: HashSet<AssetId<Mesh>>,
    /// Meshes of the world's own entities seen prepared at least once.
    loaded: HashSet<AssetId<Mesh>>,
}

impl LateState {
    /// Main world: an entity's mesh appeared or changed. `runtime` entities never hold the gate.
    fn report(&mut self, id: AssetId<Mesh>, runtime: bool) {
        self.new.push(id);
        if !runtime && !self.loaded.contains(&id) {
            self.loading.insert(id);
        }
    }

    /// Render world: sort newly reported meshes into prepared and waiting, and report waiting
    /// meshes that have been prepared since.
    fn check(&mut self, prepared: impl Fn(AssetId<Mesh>) -> bool) {
        if self.new.is_empty() && self.waiting.is_empty() {
            return;
        }
        for id in std::mem::take(&mut self.new) {
            if prepared(id) {
                self.on_time.push(id);
                self.mark_loaded(id);
            } else {
                self.waiting.insert(id);
            }
        }
        let mut ready = Vec::new();
        self.waiting.retain(|id| {
            let done = prepared(*id);
            if done {
                ready.push(*id);
            }
            !done
        });
        for id in ready {
            self.mark_loaded(id);
            self.ready.push(id);
        }
    }

    fn mark_loaded(&mut self, id: AssetId<Mesh>) {
        if self.loading.remove(&id) {
            self.loaded.insert(id);
        }
    }

    /// Main world: forget meshes whose asset was removed, so one removed before it was
    /// prepared does not wait forever.
    fn forget_removed(&mut self, exists: impl Fn(AssetId<Mesh>) -> bool) {
        self.waiting.retain(|id| exists(*id));
        self.loading.retain(|id| exists(*id));
        self.loaded.retain(|id| exists(*id));
    }
}

/// Main world: report new mesh entities and mark entities changed once their late mesh is
/// prepared. Entities are kept only until the render world has checked their mesh; a mesh
/// handle swapped on an existing entity counts as new too.
///
/// Mesh residency ([`crate::mesh_residency`]) is installed only with a map. Without it (the
/// flat-floor fight of `--play` with no `--map`) nothing retires meshes from the GPU, so every
/// mesh asset still in the main world counts as referenced.
#[allow(clippy::type_complexity)]
fn watch_meshes(
    late: Res<LateMeshes>,
    assets: Res<Assets<Mesh>>,
    refs: Option<Res<crate::mesh_residency::MeshRefs>>,
    mut meshes: ParamSet<(
        Query<(Entity, &Mesh3d, Has<RuntimeMesh>), Changed<Mesh3d>>,
        Query<&mut Mesh3d>,
    )>,
    mut unchecked: Local<std::collections::HashMap<AssetId<Mesh>, Vec<Entity>>>,
    mut refs_seen: Local<Option<Option<u64>>>,
    mut asset_events: MessageReader<bevy::asset::AssetEvent<Mesh>>,
) {
    // Only a removed asset or a mesh no entity uses any more can retire a mesh. Cloth, effect
    // batches and the sky rewrite meshes every frame (`Modified`), which retires nothing, and
    // pruning on those rescanned every loaded mesh each frame (#70).
    let revision = refs.as_ref().map(|r| r.revision());
    let retired = mesh_retired(asset_events.read(), &mut refs_seen, revision);
    if unchecked.is_empty() && meshes.p0().is_empty() && !retired {
        return;
    }
    let referenced =
        |id: AssetId<Mesh>| assets.contains(id) && refs.as_ref().is_none_or(|r| r.contains(id));
    let (on_time, ready) = {
        let mut state = late.0.lock().unwrap();
        for (entity, mesh, runtime) in &meshes.p0() {
            // Placeholder handles (`Mesh3d::default()`) never load; a mesh that is not in the
            // main world has nothing to upload from it.
            if !assets.contains(mesh.id()) {
                continue;
            }
            state.report(mesh.id(), runtime);
            unchecked.entry(mesh.id()).or_default().push(entity);
        }
        // A static merge can retire the last entity using a source before its upload.
        // CPU fallback assets still exist, but are no longer part of the loading gate.
        if retired {
            state.forget_removed(referenced);
        }
        (
            std::mem::take(&mut state.on_time),
            std::mem::take(&mut state.ready),
        )
    };
    for id in on_time {
        unchecked.remove(&id);
    }
    for id in ready {
        for entity in unchecked.remove(&id).unwrap_or_default() {
            if let Ok(mut mesh) = meshes.p1().get_mut(entity) {
                mesh.set_changed();
            }
        }
    }
    // A mesh asset removed before the render world prepared it never reports back.
    if retired {
        unchecked.retain(|id, _| referenced(*id));
    }
}

/// Whether a mesh may have been retired since the last call: a mesh asset was removed, or the
/// set of meshes some entity uses changed (`revision`, [`crate::mesh_residency::MeshRefs`];
/// `None` without a map, where only removal retires a mesh). Reads every event, so none carry
/// over to the next frame.
fn mesh_retired<'a>(
    events: impl Iterator<Item = &'a bevy::asset::AssetEvent<Mesh>>,
    seen: &mut Option<Option<u64>>,
    revision: Option<u64>,
) -> bool {
    // Counted rather than `any`, so the reader is drained every frame.
    let removed = events
        .filter(|event| matches!(event, bevy::asset::AssetEvent::Removed { .. }))
        .count()
        > 0;
    let refs_changed = seen.replace(revision) != Some(revision);
    removed || refs_changed
}

/// Render world, after specialization: sort newly reported meshes into prepared and waiting,
/// and report waiting meshes that have been prepared since.
fn check_late_meshes(
    late: Res<LateMeshes>,
    meshes: Res<bevy::render::render_asset::RenderAssets<bevy::render::mesh::RenderMesh>>,
) {
    late.0.lock().unwrap().check(|id| meshes.get(id).is_some());
}

pub fn install(app: &mut App) {
    if app.world().contains_resource::<LateMeshes>() {
        return;
    }
    let late = LateMeshes::default();
    app.insert_resource(late.clone())
        .add_systems(Last, watch_meshes.after(crate::mesh_residency::track));
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(late)
            .add_systems(Render, check_late_meshes.in_set(RenderSystems::PostCleanup));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_waits_for_new_world_meshes_not_rewritten_or_runtime_ones() {
        let id = |i: u128| AssetId::<Mesh>::Uuid {
            uuid: bevy::asset::uuid::Uuid::from_u128(i),
        };
        let (map, effect, removed) = (id(1), id(2), id(3));
        let mut state = LateState::default();
        let mut on_gpu = HashSet::new();
        state.report(map, false);
        state.report(effect, true);
        state.report(removed, false);
        assert_eq!(
            state.loading.len(),
            2,
            "only the world's own meshes hold the gate"
        );
        state.check(|id| on_gpu.contains(&id));
        assert_eq!(state.loading.len(), 2);
        on_gpu.insert(map);
        state.check(|id| on_gpu.contains(&id));
        assert_eq!(state.ready, vec![map], "a late mesh is still reported");
        assert_eq!(state.loading, HashSet::from([removed]));
        state.forget_removed(|id| id != removed);
        assert!(state.loading.is_empty() && !state.waiting.contains(&removed));
        // Bevy marks the entity changed again whenever its mesh or material asset is
        // rewritten, and a rewritten mesh leaves the render world until it is prepared again:
        // every frame for effect batches, cloth and the animated sky. That must not hold the
        // gate, but the entity is still specialized again once its mesh is back.
        on_gpu.remove(&map);
        state.report(map, false);
        state.check(|id| on_gpu.contains(&id));
        assert_eq!(state.loading.len(), 0);
        assert!(state.waiting.contains(&map));
    }

    /// #70: cloth, effect batches and the sky rewrite meshes every frame. Those `Modified`
    /// events must not rescan every loaded mesh; only a removed asset or a mesh losing its last
    /// entity can retire one.
    #[test]
    fn rewritten_meshes_retire_nothing() {
        use bevy::asset::AssetEvent;
        let id = AssetId::<Mesh>::Uuid {
            uuid: bevy::asset::uuid::Uuid::from_u128(1),
        };
        let mut seen = None;
        assert!(
            mesh_retired([].iter(), &mut seen, Some(0)),
            "first frame prunes"
        );
        let rewrites = [AssetEvent::Modified { id }, AssetEvent::Modified { id }];
        for _ in 0..3 {
            assert!(!mesh_retired(rewrites.iter(), &mut seen, Some(0)));
        }
        assert!(mesh_retired(
            [AssetEvent::Modified { id }, AssetEvent::Removed { id }].iter(),
            &mut seen,
            Some(0)
        ));
        assert!(
            mesh_retired(rewrites.iter(), &mut seen, Some(1)),
            "refs changed"
        );
        assert!(!mesh_retired([].iter(), &mut seen, Some(1)));
        // Without a map there is no `MeshRefs`: only a removed asset retires a mesh.
        let mut seen = None;
        assert!(
            mesh_retired([].iter(), &mut seen, None),
            "first frame prunes"
        );
        assert!(!mesh_retired(rewrites.iter(), &mut seen, None));
        assert!(mesh_retired(
            [AssetEvent::Removed { id }].iter(),
            &mut seen,
            None
        ));
    }

}
