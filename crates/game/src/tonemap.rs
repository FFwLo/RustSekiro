//! Recovered ordinary Yebis post operators and executable-derived CPU packing.
use std::collections::HashMap;

use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::core_pipeline::{
    FullscreenShader,
    schedule::{Core3d, Core3dSystems},
};
use bevy::prelude::*;
use bevy::render::{
    Render, RenderApp, RenderStartup, RenderSystems,
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_resource::{binding_types::*, *},
    renderer::ViewQuery,
    view::ViewTarget,
};

use bevy::render::renderer::RenderContext;

// Native renderer-global quality scale is unresolved; constructor G is one.
pub const BLOOM_QUALITY_SCALE: f32 = 1.0;
pub const OUTPUT_GAMMA: f32 = 2.2;
// The native dt unit is presumed seconds; keep the assumption explicit.
const NATIVE_DELTA_UNITS_PER_SECOND: f32 = 1.0;

#[derive(Component, Clone, ExtractComponent, ShaderType, Debug)]
#[extract_app(bevy::render::RenderApp)]
pub struct GamePost {
    // Verified 250060: 2^Exposure, shipped inverse PreExposure=1, MiddleGray, auto.
    pub exposure: Vec4,
    // Adaptation min/max EV, bright/dark base-10 rate arguments per frame.
    pub adaptation: Vec4,
    // Threshold, final colour gain, radius scale, glare luminance.
    pub bloom: Vec4,
    // Table power, ordinary output exponent, mode code, center weight.
    pub transfer: Vec4,
    // Bloom saturation interpolation, bypass adaptation, grading, hold meter.
    pub controls: Vec4,
    // LightAdapt, DarkAdapt, FilmicContrast, ReinhardContrast.
    pub curve: Vec4,
    // Reinhard white balance, bloom level count, authored Smooth, reserved.
    pub reinhard: Vec4,
    // Native tone correction before output power and the display-space LUT.
    pub colour: Mat4,
}

pub struct NativePostPlugin;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct NativePost;
impl Plugin for NativePostPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tonemap.wesl");
        embedded_asset!(app, "exposure.wesl");
        app.add_plugins((
            ExtractComponentPlugin::<GamePost>::default(),
            UniformComponentPlugin::<GamePost>::default(),
        ))
        .add_systems(Update, update_rates.after(super::apply_post_effects));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<PostViews>()
                .add_systems(RenderStartup, init_pipeline)
                .add_systems(Render, prune_views.in_set(RenderSystems::PrepareResources))
                .add_systems(
                    Core3d,
                    native_post
                        .in_set(Core3dSystems::PostProcess)
                        .in_set(NativePost),
                );
        }
    }
}

fn update_rates(time: Res<Time>, mut cameras: Query<&mut GamePost>) {
    for mut post in &mut cameras {
        post.adaptation.z = time.delta_secs() * NATIVE_DELTA_UNITS_PER_SECOND * post.curve.x;
        post.adaptation.w = time.delta_secs() * NATIVE_DELTA_UNITS_PER_SECOND * post.curve.y;
    }
}

#[derive(Resource)]
struct PostPipeline {
    layout: BindGroupLayoutDescriptor,
    meter_layout: BindGroupLayoutDescriptor,
    meter: CachedComputePipelineId,
    passes: [CachedRenderPipelineId; 6],
    sampler: Sampler,
}

fn init_pipeline(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    cache: Res<PipelineCache>,
    device: Res<bevy::render::renderer::RenderDevice>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "native post",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<GamePost>(true),
                storage_buffer_read_only::<Vec4>(false),
            ),
        ),
    );
    let meter_layout = BindGroupLayoutDescriptor::new(
        "native metering",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                uniform_buffer::<GamePost>(true),
                storage_buffer::<Vec4>(false),
            ),
        ),
    );
    let meter = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("Sekiro log luminance adaptation".into()),
        layout: vec![meter_layout.clone()],
        shader: load_embedded_asset!(assets.as_ref(), "exposure.wesl"),
        ..default()
    });
    let passes = ["BRIGHT", "BLUR_H", "BLUR_V", "TONE", "COPY", "UPSAMPLE"].map(|variant| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("Sekiro {variant}").into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: load_embedded_asset!(assets.as_ref(), "tonemap.wesl"),
                shader_defs: vec![variant.into()],
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        })
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    commands.insert_resource(PostPipeline {
        layout,
        meter_layout,
        meter,
        passes,
        sampler,
    });
}

struct ViewBuffers {
    size: UVec2,
    state: Buffer,
    // Each level has extraction/vertical output and horizontal scratch.
    levels: Vec<(Texture, TextureView, Texture, TextureView)>,
}

#[derive(Resource, Default)]
struct PostViews(HashMap<Entity, ViewBuffers>);

fn prune_views(mut buffers: ResMut<PostViews>, views: Query<(), With<GamePost>>) {
    buffers.0.retain(|entity, _| views.contains(*entity));
}

fn native_post(
    view: ViewQuery<(
        Entity,
        &ViewTarget,
        &GamePost,
        &DynamicUniformIndex<GamePost>,
    )>,
    pipeline: Res<PostPipeline>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<GamePost>>,
    mut buffers: ResMut<PostViews>,
    mut ctx: RenderContext,
) {
    let (entity, target, settings, index) = view.into_inner();
    let (Some(meter), Some(binding)) = (
        cache.get_compute_pipeline(pipeline.meter),
        uniforms.uniforms().binding(),
    ) else {
        return;
    };
    let passes: Vec<_> = pipeline
        .passes
        .iter()
        .filter_map(|id| cache.get_render_pipeline(*id))
        .collect();
    if passes.len() != 6 || target.main_texture_format() != TextureFormat::Rgba16Float {
        return;
    }
    let size = UVec2::new(
        target.main_texture().width(),
        target.main_texture().height(),
    );
    let b = buffers
        .0
        .entry(entity)
        .or_insert_with(|| new_buffers(size, settings.reinhard.y as usize, ctx.render_device()));
    if b.size != size || b.levels.len() != settings.reinhard.y as usize {
        let history = b.state.clone();
        *b = new_buffers(size, settings.reinhard.y as usize, ctx.render_device());
        b.state = history;
    }
    let post = target.post_process_write();
    let meter_bindings = ctx.render_device().create_bind_group(
        "native metering",
        &cache.get_bind_group_layout(&pipeline.meter_layout),
        &BindGroupEntries::sequential((post.source, binding.clone(), b.state.as_entire_binding())),
    );
    use bevy::render::diagnostic::RecordDiagnostics;
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "native_post");
    {
        let mut pass = ctx
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("native exposure"),
                ..default()
            });
        pass.set_pipeline(meter);
        pass.set_bind_group(0, &meter_bindings, &[index.index()]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    if settings.bloom.y > 0. {
        for (i, level) in b.levels.iter().enumerate() {
            let source = if i == 0 {
                post.source
            } else {
                &b.levels[i - 1].1
            };
            for (step, source, destination) in [
                (if i == 0 { 0 } else { 4 }, source, &level.1),
                (1, &level.1, &level.3),
                (2, &level.3, &level.1),
            ] {
                let bindings = ctx.render_device().create_bind_group(
                    "native bloom",
                    &cache.get_bind_group_layout(&pipeline.layout),
                    &BindGroupEntries::sequential((
                        source,
                        source,
                        source,
                        source,
                        &pipeline.sampler,
                        binding.clone(),
                        b.state.as_entire_binding(),
                    )),
                );
                draw(
                    &mut ctx,
                    passes[step],
                    &bindings,
                    index.index(),
                    destination,
                );
            }
        }
        // 2536c0 folds the smaller blurred level into the next larger one.
        for i in (1..b.levels.len()).rev() {
            let level = &b.levels[i - 1];
            let bindings = ctx.render_device().create_bind_group(
                "native bloom upsample",
                &cache.get_bind_group_layout(&pipeline.layout),
                &BindGroupEntries::sequential((
                    &level.1,
                    &b.levels[i].1,
                    &level.1,
                    &level.1,
                    &pipeline.sampler,
                    binding.clone(),
                    b.state.as_entire_binding(),
                )),
            );
            draw(&mut ctx, passes[5], &bindings, index.index(), &level.3);
            let level = &mut b.levels[i - 1];
            std::mem::swap(&mut level.1, &mut level.3);
        }
    }
    let bindings = ctx.render_device().create_bind_group(
        "native tone",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            post.source,
            &b.levels[0].1,
            &b.levels[0].1,
            &b.levels[0].1,
            &pipeline.sampler,
            binding,
            b.state.as_entire_binding(),
        )),
    );
    draw(
        &mut ctx,
        passes[3],
        &bindings,
        index.index(),
        post.destination,
    );
    span.end(ctx.command_encoder());
}

fn new_buffers(
    size: UVec2,
    count: usize,
    device: &bevy::render::renderer::RenderDevice,
) -> ViewBuffers {
    let state = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("adapted biased log and gain"),
        contents: &[0; 16],
        usage: BufferUsages::STORAGE,
    });
    let levels = (0..count)
        .map(|i| {
            let divisor = 1 << (i + 1);
            let make = || {
                let texture = device.create_texture(&TextureDescriptor {
                    label: Some("native bloom level"),
                    size: Extent3d {
                        width: size.x.div_ceil(divisor).max(1),
                        height: size.y.div_ceil(divisor).max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&TextureViewDescriptor::default());
                (texture, view)
            };
            let (a, av) = make();
            let (b, bv) = make();
            (a, av, b, bv)
        })
        .collect();
    ViewBuffers {
        size,
        state,
        levels,
    }
}

fn draw(
    ctx: &mut RenderContext,
    pipeline: &RenderPipeline,
    bindings: &BindGroup,
    index: u32,
    destination: &TextureView,
) {
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("native post stage"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })],
        ..default()
    });
    pass.set_render_pipeline(pipeline);
    pass.set_bind_group(0, bindings, &[index]);
    pass.draw(0..3, 0..1);
}
