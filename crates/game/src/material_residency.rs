//! GLTF StandardMaterials are source descriptions for map wrappers, rather than extra draws.
//! Prepare only descriptors referenced by actual StandardMaterial mesh entities.
use bevy::prelude::*;
use bevy::render::erased_render_asset::ExtractedAssets;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::collections::HashSet;

#[derive(Resource, Default)]
struct UsedStandardMaterials {
    used: HashSet<AssetId<StandardMaterial>>,
    retired: Vec<AssetId<StandardMaterial>>,
    newly_used: Vec<(AssetId<StandardMaterial>, StandardMaterial)>,
}

#[derive(Resource, Default)]
struct UsageRevision(u64);

#[allow(clippy::type_complexity)]
fn track_usage(
    changed: Query<
        (),
        (
            With<Mesh3d>,
            With<MeshMaterial3d<StandardMaterial>>,
            Or<(Changed<MeshMaterial3d<StandardMaterial>>, Added<Mesh3d>)>,
        ),
    >,
    mut materials: RemovedComponents<MeshMaterial3d<StandardMaterial>>,
    mut meshes: RemovedComponents<Mesh3d>,
    standard: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
    mut revision: ResMut<UsageRevision>,
) {
    let removed = materials.read().count()
        + meshes
            .read()
            .filter(|entity| standard.contains(*entity))
            .count();
    if !changed.is_empty() || removed > 0 {
        revision.0 += 1;
    }
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<UsageRevision>()
        .add_systems(Last, track_usage);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<UsedStandardMaterials>()
            .add_systems(ExtractSchedule, extract_used)
            .add_systems(
                Render,
                filter_unused
                    .after(RenderSystems::ExtractCommands)
                    .before(RenderSystems::PrepareAssets),
            );
    }
}

fn extract_used(
    meshes: Extract<Query<&MeshMaterial3d<StandardMaterial>, With<Mesh3d>>>,
    assets: Extract<Res<Assets<StandardMaterial>>>,
    revision: Extract<Res<UsageRevision>>,
    mut previous: Local<Option<u64>>,
    mut used: ResMut<UsedStandardMaterials>,
) {
    if *previous == Some(revision.0) {
        return;
    }
    *previous = Some(revision.0);
    let current: HashSet<_> = meshes.iter().map(|m| m.0.id()).collect();
    used.newly_used = current
        .difference(&used.used)
        .filter_map(|id| assets.get(*id).map(|asset| (*id, asset.clone())))
        .collect();
    used.retired = used.used.difference(&current).copied().collect();
    used.used = current;
}

fn filter_unused(
    mut used: ResMut<UsedStandardMaterials>,
    mut extracted: ResMut<ExtractedAssets<MeshMaterial3d<StandardMaterial>>>,
) {
    if extracted.extracted.is_empty()
        && extracted.added.is_empty()
        && extracted.modified.is_empty()
        && used.newly_used.is_empty()
        && used.retired.is_empty()
    {
        return;
    }
    let _timer = crate::perf_log::SystemTimer::start("source material residency");
    extracted.extracted.retain(|(id, _)| used.used.contains(id));
    extracted.added.retain(|id| used.used.contains(id));
    extracted.modified.retain(|id| used.used.contains(id));
    // Retire former draws even when a GLTF scene continues holding its source handle.
    extracted.removed.extend(used.retired.drain(..));
    let present: HashSet<_> = extracted.extracted.iter().map(|(id, _)| *id).collect();
    for (id, material) in used.newly_used.drain(..) {
        // A newly spawned mesh can reference an old asset, without an Added/Modified event.
        if !present.contains(&id) {
            extracted.extracted.push((id, material));
            extracted.added.insert(id);
        }
    }
}
