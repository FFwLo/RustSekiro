//! Display-space grading after tone mapping, before UI compositing.
use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::core_pipeline::{
    FullscreenShader,
    schedule::{Core3d, Core3dSystems},
};
use bevy::prelude::*;
use bevy::render::{
    RenderApp, RenderStartup,
    extract_component::{ExtractComponent, ExtractComponentPlugin},
    render_asset::RenderAssets,
    render_resource::{binding_types::*, *},
    renderer::ViewQuery,
    texture::GpuImage,
    view::ViewTarget,
};

use bevy::render::renderer::RenderContext;

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub struct GradingLut(pub Handle<Image>);

pub struct GradingPlugin;
impl Plugin for GradingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "grading_lut.wesl");
        app.add_plugins(ExtractComponentPlugin::<GradingLut>::default());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, init_pipeline)
                .add_systems(
                    Core3d,
                    grade
                        .after(super::tonemap::NativePost)
                        .in_set(Core3dSystems::PostProcess),
                );
        }
    }
}

#[derive(Resource)]
struct GradingPipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedRenderPipelineId,
}

fn init_pipeline(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "display grading",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Float { filterable: false }),
            ),
        ),
    );
    let pipeline = cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("Sekiro display LUT".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: load_embedded_asset!(assets.as_ref(), "grading_lut.wesl"),
            targets: vec![Some(ColorTargetState {
                // HDR views keep their half-float ping-pong buffers after tonemapping.
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    });
    commands.insert_resource(GradingPipeline { layout, pipeline });
}

fn grade(
    view: ViewQuery<(&ViewTarget, &GradingLut)>,
    pipeline: Res<GradingPipeline>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    mut ctx: RenderContext,
) {
    let (target, lut) = view.into_inner();
    // Do not advance the ping-pong target unless every prerequisite is ready.
    let (Some(pso), Some(lut)) = (
        cache.get_render_pipeline(pipeline.pipeline),
        images.get(&lut.0),
    ) else {
        return;
    };
    if target.main_texture_format() != TextureFormat::Rgba16Float {
        return;
    }
    let post = target.post_process_write();
    let bindings = ctx.render_device().create_bind_group(
        "display grading",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((post.source, &lut.texture_view)),
    );
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("display grading"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })],
        ..default()
    });
    pass.set_render_pipeline(pso);
    pass.set_bind_group(0, &bindings, &[]);
    pass.draw(0..3, 0..1);
}
