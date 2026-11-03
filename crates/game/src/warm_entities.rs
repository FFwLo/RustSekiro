//! Populate per-entity color and cascade caches for resident map placements during loading.
//! Bevy retains these entries while entities are hidden and invalidates actual mesh/material edits.
use bevy::material::key::{ErasedMaterialPipelineKey, ErasedMeshPipelineKey};
use bevy::pbr::{
    LightKeyCache, MeshPipelineKey, PreparedMaterial, ShadowsDepthOnlyDrawFunction,
    ShadowsDrawFunction, SpecializedMaterialPipelineCache, SpecializedShadowMaterialPipelineCache,
    ViewKeyCache,
};
use bevy::prelude::*;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::sync_world::MainEntity;
use bevy::render::{Extract, ExtractSchedule};
use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
};

type PipelineVariant = (
    bool,
    ErasedMaterialPipelineKey,
    bevy::mesh::MeshVertexBufferLayoutRef,
);

#[derive(Clone, Copy)]
struct Resident {
    entity: MainEntity,
    mesh: AssetId<Mesh>,
    material: bevy::asset::UntypedAssetId,
}

#[derive(Resource, Default)]
struct Residents {
    items: Vec<Resident>,
    captured: bool,
}

#[derive(Resource, Default)]
struct Queue {
    work: VecDeque<Resident>,
    started: bool,
    pipelines:
        HashMap<PipelineVariant, Option<bevy::render::render_resource::CachedRenderPipelineId>>,
    completed: usize,
}

pub(super) fn install<M: Material>(render: &mut SubApp) {
    render
        .init_resource::<Residents>()
        .init_resource::<Queue>()
        .add_systems(ExtractSchedule, extract::<M>);
}

fn extract<M: Material>(
    meshes: Extract<Query<(Entity, &Mesh3d, &MeshMaterial3d<M>)>>,
    mut residents: ResMut<Residents>,
    warmup: Res<super::PipelineWarmup>,
) {
    if residents.captured || !warmup.loaded.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    residents.captured = true;
    residents
        .items
        .extend(meshes.iter().map(|(entity, mesh, material)| Resident {
            entity: entity.into(),
            mesh: mesh.0.id(),
            material: material.0.id().untyped(),
        }));
}

pub(super) fn cancel(world: &mut World) {
    let mut queue = world.resource_mut::<Queue>();
    queue.work.clear();
    queue.pipelines.clear();
}

/// The fraction of resident placements warmed, 0 before the entity warmup starts.
pub(super) fn progress(world: &World) -> f32 {
    let queue = world.resource::<Queue>();
    if queue.started {
        crate::play::fraction(queue.completed, queue.completed + queue.work.len())
    } else {
        0.0
    }
}

pub(super) fn warm<M: Material>(world: &mut World, budget: std::time::Duration) -> bool {
    let _timer = crate::perf_log::SystemTimer::start("entity pipeline warmup");
    let mut queue = world.remove_resource::<Queue>().unwrap();
    if !queue.started {
        queue.started = true;
        queue.work = std::mem::take(&mut world.resource_mut::<Residents>().items).into();
    }
    let colors: Vec<_> = world
        .resource::<ViewKeyCache>()
        .iter()
        .map(|(&view, &key)| (view, key))
        .collect();
    let shadows: Vec<_> = world
        .resource::<LightKeyCache>()
        .iter()
        .map(|(&view, &key)| (view, key))
        .collect();
    let depth_clip = world
        .resource::<bevy::pbr::PrepassPipeline>()
        .depth_clip_control_supported;
    let start = std::time::Instant::now();
    while start.elapsed() < budget {
        let Some(resident) = queue.work.pop_front() else {
            break;
        };
        let Some(mesh) = world
            .resource::<RenderAssets<RenderMesh>>()
            .get(resident.mesh)
        else {
            continue;
        };
        if mesh.layout.0.contains(Mesh::ATTRIBUTE_JOINT_INDEX) {
            continue;
        }
        let layout = mesh.layout.clone();
        let bits = MeshPipelineKey::from_bits_retain(mesh.key_bits.bits());
        let Some(material) = world
            .resource::<ErasedRenderAssets<PreparedMaterial>>()
            .get(resident.material)
        else {
            continue;
        };
        let properties = material.properties.clone();
        for (shadow, views) in [(false, &colors), (true, &shadows)] {
            if shadow && !properties.shadows_enabled {
                continue;
            }
            for &(view, view_key) in views {
                let key = super::pass_key(
                    if shadow {
                        super::Pass::Shadow
                    } else {
                        super::Pass::Color
                    },
                    view_key,
                    bits,
                    &layout,
                    &properties,
                );
                let draw = if shadow {
                    let depth_only = !key.intersects(
                        MeshPipelineKey::MAY_DISCARD | MeshPipelineKey::PREPASS_READS_MATERIAL,
                    ) && (depth_clip
                        || !key.contains(MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO));
                    let draw = if depth_only {
                        properties.get_draw_function(ShadowsDepthOnlyDrawFunction)
                    } else {
                        properties.get_draw_function(ShadowsDrawFunction)
                    };
                    let Some(draw) = draw else {
                        continue;
                    };
                    Some((draw, depth_only))
                } else {
                    None
                };
                let erased = ErasedMaterialPipelineKey {
                    type_id: TypeId::of::<M>(),
                    mesh_key: ErasedMeshPipelineKey::new(key),
                    material_key: properties.material_key.clone(),
                };
                let cached = match queue
                    .pipelines
                    .entry((shadow, erased.clone(), layout.clone()))
                {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let Some(result) =
                            super::specialize(world, shadow, erased, &layout, &properties)
                        else {
                            continue;
                        };
                        *entry.insert(result.ok())
                    }
                };
                if let Some(pipeline) = cached {
                    if let Some((draw, depth_only)) = draw {
                        world
                            .resource_mut::<SpecializedShadowMaterialPipelineCache>()
                            .entry(view)
                            .or_default()
                            .insert(resident.entity, (pipeline, draw, depth_only));
                    } else {
                        world
                            .resource_mut::<SpecializedMaterialPipelineCache>()
                            .entry(view)
                            .or_default()
                            .insert(resident.entity, pipeline);
                    }
                }
            }
        }
        queue.completed += 1;
    }
    let ready = queue.work.is_empty();
    if ready {
        queue.pipelines.clear();
        info!(
            "map entity warmup complete: {} resident mesh placements",
            queue.completed
        );
    }
    world.insert_resource(queue);
    ready
}
