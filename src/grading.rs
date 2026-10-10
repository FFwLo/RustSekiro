//! The map's colour-grading LUT as a post-process after tonemapping (docs/kb/map.md).
//!
//! The game grades its final image through a Yebis ColorGrading LUT picked by the draw params
//! (`ColorGrading[Yebis]` "LutSourceId" per time of day; `map/m11/m11_cgrading.tpf` ->
//! `map_<id>_lut_<n>.dds`, written by `sekiro-extract map`). `map.rs` puts a
//! [`ColorGradingLut`] on the camera; this pass (Bevy's custom post-processing pattern) runs
//! after tonemapping on the display colour, like the game's (grading_lut.wgsl).

use bevy::core_pipeline::{schedule::Core3d, tonemapping::tonemapping, Core3dSystems, FullscreenShader};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{sampler, texture_2d};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewTarget;
use bevy::render::{RenderApp, RenderStartup};

/// The LUT strip (16x256, loaded with `is_srgb = false`: the bytes are display values).
#[derive(Component, Clone, ExtractComponent)]
pub struct ColorGradingLut(pub Handle<Image>);

pub struct GradingPlugin;

impl Plugin for GradingPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "grading_lut.wgsl");
        app.add_plugins(ExtractComponentPlugin::<ColorGradingLut>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(Core3d, grade.after(tonemapping).in_set(Core3dSystems::PostProcess));
    }
}

#[derive(Resource)]
struct LutPipeline {
    layout: BindGroupLayoutDescriptor,
    screen_sampler: Sampler,
    lut_sampler: Sampler,
    id: CachedRenderPipelineId,
}

fn init_pipeline(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "grading_lut_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );
    let screen_sampler = device.create_sampler(&SamplerDescriptor::default());
    let lut_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("grading_lut_sampler"),
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let id = cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("grading_lut_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: assets.load("embedded://sv1/grading_lut.wgsl"),
            targets: vec![Some(ColorTargetState { format: TextureFormat::Rgba16Float, blend: None, write_mask: ColorWrites::ALL })],
            ..default()
        }),
        ..default()
    });
    commands.insert_resource(LutPipeline { layout, screen_sampler, lut_sampler, id });
}

/// Skipped (ViewQuery) on views without a [`ColorGradingLut`]; waits for the LUT image and
/// the pipeline, and only grades HDR (Rgba16Float) views: the pipeline's target format.
fn grade(
    view: ViewQuery<(&ViewTarget, &ColorGradingLut)>,
    pipeline: Option<Res<LutPipeline>>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    mut ctx: RenderContext,
) {
    let Some(p) = pipeline else { return };
    let (target, lut) = view.into_inner();
    let Some(pipe) = cache.get_render_pipeline(p.id) else { return };
    let Some(lut) = images.get(&lut.0) else { return };
    if target.main_texture_format() != TextureFormat::Rgba16Float {
        return;
    }
    let w = target.post_process_write();
    let bind_group = ctx.render_device().create_bind_group(
        "grading_lut_bind_group",
        &cache.get_bind_group_layout(&p.layout),
        &BindGroupEntries::sequential((w.source, &p.screen_sampler, &lut.texture_view, &p.lut_sampler)),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("grading_lut_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: w.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipe);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}
