//! Reusable vertex buffers for FXR batches; materials and sorting remain in Bevy's phases.
use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d};
use bevy::ecs::{
    query::ROQueryItem,
    system::{SystemParamItem, lifetimeless::SRes},
};
use bevy::pbr::{
    RenderMeshInstances, SetMaterialBindGroup, SetMeshBindGroup, SetMeshViewBindGroup,
    SetMeshViewBindingArrayBindGroup, Transmissive3d,
};
use bevy::prelude::*;
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, PhaseItem, RenderCommand, RenderCommandResult,
    SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
};
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::{
    Render, RenderApp, RenderSystems,
    extract_resource::{ExtractResource, ExtractResourcePlugin},
};
use std::{collections::HashMap, sync::Arc};

struct Upload {
    bytes: Vec<u8>,
    vertices: u32,
    center: Vec3,
}

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub(super) struct VertexStreams(HashMap<AssetId<Mesh>, Arc<Upload>>);

impl VertexStreams {
    pub(super) fn publish(&mut self, id: AssetId<Mesh>, mesh: &Mesh) -> bool {
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return false;
        };
        let (min, max) = positions.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(Vec3::from_array(*p)), max.max(Vec3::from_array(*p))),
        );
        self.0.insert(
            id,
            Arc::new(Upload {
                bytes: mesh.create_packed_vertex_buffer_data(),
                vertices: mesh.count_vertices() as u32,
                center: (min + max) * 0.5,
            }),
        );
        true
    }

    #[cfg(test)]
    pub(super) fn versions(&self) -> Vec<usize> {
        self.0.values().map(|u| Arc::as_ptr(u) as usize).collect()
    }
}

struct GpuBatch {
    buffer: Buffer,
    capacity: usize,
    upload: Arc<Upload>,
}

#[derive(Resource, Default)]
struct GpuStreams(HashMap<AssetId<Mesh>, GpuBatch>);

type DrawStream = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetMaterialBindGroup<{ bevy::pbr::MATERIAL_BIND_GROUP_INDEX }>,
    DrawVertices,
);

pub(super) fn install(app: &mut App) {
    app.init_resource::<VertexStreams>()
        .add_plugins(ExtractResourcePlugin::<VertexStreams>::default())
        .add_systems(Last, retire_vertices);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<GpuStreams>()
            .add_render_command::<Transparent3d, DrawStream>()
            .add_render_command::<Transmissive3d, DrawStream>()
            .add_systems(
                Render,
                route_draws
                    .after(RenderSystems::Queue)
                    .before(RenderSystems::PhaseSort),
            )
            .add_systems(
                Render,
                upload_vertices.in_set(RenderSystems::PrepareResources),
            );
    }
}

fn retire_vertices(
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut streams: ResMut<VertexStreams>,
) {
    for event in events.read() {
        if let AssetEvent::Removed { id } = event
            && streams.bypass_change_detection().0.remove(id).is_some()
        {
            streams.set_changed();
        }
    }
}

fn upload_vertices(
    streams: Res<VertexStreams>,
    mut gpu: ResMut<GpuStreams>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let _timer = crate::perf_log::SystemTimer::start("fxr vertex upload");
    gpu.0.retain(|id, _| streams.0.contains_key(id));
    for (&id, upload) in &streams.0 {
        if gpu
            .0
            .get(&id)
            .is_some_and(|b| Arc::ptr_eq(&b.upload, upload))
        {
            continue;
        }
        let capacity = upload.bytes.len().max(4).next_power_of_two();
        if gpu
            .0
            .get(&id)
            .is_none_or(|b| b.capacity < upload.bytes.len())
        {
            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some("FXR streaming vertices"),
                size: capacity as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.0.insert(
                id,
                GpuBatch {
                    buffer,
                    capacity,
                    upload: upload.clone(),
                },
            );
        }
        let batch = gpu.0.get_mut(&id).unwrap();
        queue.write_buffer(&batch.buffer, 0, &upload.bytes);
        batch.upload = upload.clone();
    }
}

fn route_draws(
    streams: Res<VertexStreams>,
    instances: Res<RenderMeshInstances>,
    transparent_draws: Res<DrawFunctions<Transparent3d>>,
    transmissive_draws: Res<DrawFunctions<Transmissive3d>>,
    mut transparent: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut transmissive: ResMut<ViewSortedRenderPhases<Transmissive3d>>,
) {
    if streams.0.is_empty() {
        return;
    }
    let transparent_id = transparent_draws.read().id::<DrawStream>();
    let transmissive_id = transmissive_draws.read().id::<DrawStream>();
    route_phase(&streams, &instances, &mut transparent, |item| {
        item.draw_function = transparent_id;
        &mut item.sorting_info
    });
    route_phase(&streams, &instances, &mut transmissive, |item| {
        item.draw_function = transmissive_id;
        &mut item.sorting_info
    });
}

fn route_phase<P: bevy::render::render_phase::SortedPhaseItem>(
    streams: &VertexStreams,
    instances: &RenderMeshInstances,
    phases: &mut ViewSortedRenderPhases<P>,
    mut route: impl FnMut(&mut P) -> &mut TransparentSortingInfo3d,
) {
    for phase in phases.0.values_mut() {
        for item in phase.items.values_mut() {
            if let Some(upload) = instances
                .mesh_asset_id(item.main_entity())
                .and_then(|id| streams.0.get(&id))
                && let TransparentSortingInfo3d::Sorted { mesh_center, .. } = route(item)
            {
                *mesh_center = upload.center;
            }
        }
    }
}

struct DrawVertices;
impl<P: PhaseItem> RenderCommand<P> for DrawVertices {
    type Param = (SRes<GpuStreams>, SRes<RenderMeshInstances>);
    type ViewQuery = ();
    type ItemQuery = ();
    fn render<'w>(
        item: &P,
        _: ROQueryItem<'w, '_, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (gpu, instances): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(batch) = instances
            .into_inner()
            .mesh_asset_id(item.main_entity())
            .and_then(|id| gpu.into_inner().0.get(&id))
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, batch.buffer.slice(..));
        pass.draw(0..batch.upload.vertices, item.batch_range().clone());
        RenderCommandResult::Success
    }
}
