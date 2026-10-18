//! The game's global fog in place of Bevy's DistanceFog: a constant density plus the height fog
//! (HeightFogDistribution / Offset / Density Scale), lit by the sky colour and the direct /
//! opposite colours by the angle to the sun, integrated in HDR before the post passes
//! (height_fog.wgsl).
//!
//! Adapted from sekiro-rs (MIT / Apache-2.0) crates/game/src/fog.rs + fog.wgsl (a936191). Its
//! upload constants are provisional; here the exe's own packing is used, from its
//! docs/SHADER-INPUTS.md (884ea0b, functions 39e7e0 / 39fa70 / 39f000): lengths times GameScale,
//! per-length densities and HeightFogDistribution over it, sH = HeightFogDensity Scale / GS / 2,
//! h = world Y - HeightFogOffset in metres (not scaled), ExtinctionScale = 1 + AbsorptionScale,
//! colours at gamma 2.2 times W, the depth slices geometric between Near' and Far'.
//! gap: the fog's local lights, its shadowed (sun-occluded) variant and the transparent
//! effects' own depth are not drawn: particles take the fog of what is behind them.

use bevy::core_pipeline::{schedule::Core3d, Core3dSystems, FullscreenShader};
use bevy::prelude::*;
use bevy::render::{
    extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin},
    render_resource::{binding_types::*, *},
    renderer::{RenderContext, ViewQuery},
    view::{ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
    RenderApp, RenderStartup,
};

/// The fog values the pass reads (height_fog.wgsl `GameFog`), in metres and scene radiance.
#[derive(Component, Clone, ExtractComponent, ShaderType, Debug, PartialEq, Default)]
pub struct GameFog {
    /// FixedDensity', kH, sH, the height offset: the arena origin's map Y - HeightFogOffset.
    pub density: Vec4,
    /// Near', Far', SkyLight StartDist', DirLight StartDist'.
    pub depths: Vec4,
    /// SkyColor, SkyLight HeightAtten.
    pub sky: Vec4,
    /// DirectColor, DirLight HeightAtten.
    pub direct: Vec4,
    /// OppositeColor, the height sign (-1 for a negative HeightFogDistribution).
    pub opposite: Vec4,
    /// Toward the fog's light, DirLightAngleInvCos.
    pub direction: Vec4,
    /// ExtinctionScale, the slice count.
    pub extinction: Vec4,
    /// NearOffset', analytical continuation on (1) / off (0).
    pub extra: Vec4,
}

/// gap: VolumeDepth (the slice count, clamped 4..128 by the exe) is not in the exported draw
/// params; the clamp's top.
const SLICES: f32 = 128.0;
/// gap: DirLightAngleInvCos's AngleClip floor is not recovered (sekiro-rs fog.rs uses 0.01).
const ANGLE_CLIP: f32 = 0.01;

impl GameFog {
    /// The fog of the map's light set at its hour. `toward_sun`: the fog light's direction
    /// (gap: DirLight LightBankId's light is not resolved; the light set's sun, Angle0);
    /// `radiance`: scene radiance to Bevy's (map.rs ENV_PER_UNIT, the probes' bridge).
    /// None when VolumetricFog Enable is off.
    pub fn from_lighting(l: &crate::map::MapLighting, id: i32, toward_sun: Vec3, radiance: f32) -> Option<Self> {
        let get = |name: &str, or: f32| l.at("VolumetricFog", name, id).and_then(|v| v.first().copied()).filter(|v| v.is_finite()).unwrap_or(or);
        if get("Enable", 1.0) < 0.5 {
            return None;
        }
        let colour = |name: &str| {
            let v = l.at("VolumetricFog", name, id).unwrap_or_default();
            let (c, w) = crate::map::fog_colour(&v);
            c.to_linear().to_vec3() * w.max(0.0) * radiance
        };
        let gs = get("GameScale", 1.0).max(f32::MIN_POSITIVE);
        let (near, far) = (get("NearDepth", 0.01), get("FarDepth", 400.0));
        let dist = get("HeightFogDistribution", 0.0);
        let sign = if dist < 0.0 { -1.0 } else { 1.0 };
        let toward = toward_sun.normalize_or(Vec3::Y);
        // gap: AbsorptionScale, both HeightAttens, NearOffset and EnableAnalyticalFog are not in
        // the exported draw params (0, 0, 0, on).
        let absorption = l.at("VolumetricFog", "AbsorptionScale", id).filter(|v| v.len() >= 3).map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
        Some(GameFog {
            density: Vec4::new(
                (get("FixedDensity", 0.0) / gs).max(0.0),
                (dist.abs() / gs).max(1.18e-38),
                0.5 * get("HeightFogDensity Scale", 0.0).max(0.0) / gs,
                l.map_origin.map_or(0.0, |o| o.y) - get("HeightFogOffset", 0.0),
            ),
            depths: Vec4::new(gs * near, gs * far.max(near + 0.1), gs * get("SkyLight StartDist", 0.0).max(0.0), gs * get("DirLight StartDist", 0.0).max(0.0)),
            sky: colour("SkyColor").extend(get("SkyLight HeightAtten", 0.0).max(0.0)),
            direct: colour("DirectColor").extend(get("DirLight HeightAtten", 0.0).max(0.0)),
            opposite: colour("OppositeColor").extend(sign),
            direction: toward.extend(1.0 / (sign * toward.y).max(ANGLE_CLIP)),
            extinction: (Vec3::ONE + absorption.max(Vec3::ZERO)).extend(SLICES),
            extra: Vec4::new(gs * get("NearOffset", 0.0), get("EnableAnalyticalFog", 1.0).min(1.0).max(0.0), 0.0, 0.0),
        })
    }
}

pub struct HeightFogPlugin;

impl Plugin for HeightFogPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "height_fog.wgsl");
        app.add_plugins((ExtractComponentPlugin::<GameFog>::default(), UniformComponentPlugin::<GameFog>::default()))
            .add_systems(Update, depth_binding);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, init_pipeline)
                .add_systems(Core3d, fog.after(Core3dSystems::MainPass).before(Core3dSystems::EarlyPostProcess));
        }
    }
}

/// The depth usage this module added (not one another system set).
#[derive(Component)]
struct FogDepthBinding;

/// Lets a fogged camera's depth buffer be sampled (keeping the usages others added), and takes
/// that back when the fog goes: a switch from a fogged map (the castle top) to one without
/// (Hirata, VolumetricFog off) kept the sampled depth, and with the sun's shadows on the Vulkan
/// driver lost the device 20-30 s later (2026-10-17; a fresh start on Hirata never did).
fn depth_binding(mut commands: Commands, mut cams: Query<(Entity, &mut Camera3d, Has<GameFog>, Has<FogDepthBinding>)>) {
    for (e, mut c3, fogged, ours) in &mut cams {
        let usage = TextureUsages::from(c3.depth_texture_usages);
        if fogged && !usage.contains(TextureUsages::TEXTURE_BINDING) {
            c3.depth_texture_usages = (usage | TextureUsages::TEXTURE_BINDING).into();
            commands.entity(e).insert(FogDepthBinding);
        } else if !fogged && ours {
            c3.depth_texture_usages = (usage - TextureUsages::TEXTURE_BINDING).into();
            commands.entity(e).remove::<FogDepthBinding>();
        }
    }
}

#[derive(Resource)]
struct FogPipeline(Vec<(BindGroupLayoutDescriptor, CachedRenderPipelineId)>);

fn init_pipeline(mut commands: Commands, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let variants = [false, true]
        .into_iter()
        .map(|multisampled| {
            let layout = BindGroupLayoutDescriptor::new(
                "game fog",
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::FRAGMENT,
                    (
                        texture_2d(TextureSampleType::Float { filterable: false }),
                        if multisampled { texture_depth_2d_multisampled() } else { texture_depth_2d() },
                        uniform_buffer::<ViewUniform>(true),
                        uniform_buffer::<GameFog>(true),
                    ),
                ),
            );
            let pipeline = cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("game fog".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen.to_vertex_state(),
                fragment: Some(FragmentState {
                    shader: assets.load("embedded://sv1/height_fog.wgsl"),
                    shader_defs: if multisampled { vec!["MULTISAMPLED_DEPTH".into()] } else { vec![] },
                    targets: vec![Some(ColorTargetState { format: TextureFormat::Rgba16Float, blend: None, write_mask: ColorWrites::ALL })],
                    ..default()
                }),
                ..default()
            });
            (layout, pipeline)
        })
        .collect();
    commands.insert_resource(FogPipeline(variants));
}

/// Skipped (ViewQuery) on views without a [`GameFog`].
fn fog(
    view: ViewQuery<(&ViewTarget, &ViewDepthTexture, &Msaa, &ViewUniformOffset, &DynamicUniformIndex<GameFog>)>,
    pipeline: Res<FogPipeline>,
    cache: Res<PipelineCache>,
    views: Res<ViewUniforms>,
    uniforms: Res<ComponentUniforms<GameFog>>,
    mut ctx: RenderContext,
) {
    let (target, depth, msaa, offset, index) = view.into_inner();
    let (layout, id) = &pipeline.0[usize::from(msaa.samples() > 1)];
    let (Some(pso), Some(view_binding), Some(fog_binding)) = (cache.get_render_pipeline(*id), views.uniforms.binding(), uniforms.uniforms().binding()) else {
        return;
    };
    // The frame the fog comes or goes, its depth usage (depth_binding) is a frame behind.
    if target.main_texture_format() != TextureFormat::Rgba16Float || !depth.texture.usage().contains(TextureUsages::TEXTURE_BINDING) {
        return;
    }
    let post = target.post_process_write();
    let bindings = ctx.render_device().create_bind_group("game fog", &cache.get_bind_group_layout(layout), &BindGroupEntries::sequential((post.source, depth.view(), view_binding, fog_binding)));
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("game fog"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Load, store: StoreOp::Store } })],
        ..default()
    });
    pass.set_render_pipeline(pso);
    pass.set_bind_group(0, &bindings, &[offset.offset, index.index()]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn castle(hour: f32) -> Option<crate::map::MapLighting> {
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("map_boss_71001000.json")).ok()?).ok()?;
        Some(crate::map::MapLighting {
            drawparam: j["drawparam"].clone(),
            light_set: 0,
            hour,
            applied: false,
            env: Vec::new(),
            arena_yaw: 0.0,
            luts: Vec::new(),
            stem: String::new(),
            sound_space: 0,
            source_map: String::new(),
            map_origin: Some(Vec3::new(0.0, 54.0, 0.0)),
        })
    }

    /// The exe's packing (SHADER-INPUTS.md 39e7e0 / 39fa70): densities and the height
    /// distribution over GameScale, lengths times it, sH halved, the offset in plain metres
    /// against the map height.
    #[test]
    fn the_fog_takes_the_exes_game_scale_packing() {
        let Some(l) = castle(0.0) else {
            eprintln!("no castle export: skipped");
            return;
        };
        // Castle id 0 at 0 h: GameScale 12, FixedDensity 0, HeightFogDistribution 0.3, Density
        // Scale 0.075, Offset -100, Near 0.01, Far 200.
        let f = GameFog::from_lighting(&l, 0, Vec3::Y, 1.0).expect("the castle's fog is on");
        assert_eq!(f.density.x, 0.0);
        assert!((f.density.y - 0.3 / 12.0).abs() < 1e-6, "{}", f.density.y);
        assert!((f.density.z - 0.5 * 0.075 / 12.0).abs() < 1e-6, "{}", f.density.z);
        assert!((f.density.w - (54.0 + 100.0)).abs() < 1e-4, "{}", f.density.w);
        assert!((f.depths.x - 0.12).abs() < 1e-5 && (f.depths.y - 2400.0).abs() < 1e-2, "{:?}", f.depths);
        assert_eq!(f.extinction.xyz(), Vec3::ONE);
        assert_eq!(f.opposite.w, 1.0);
    }

    /// The fog's height profile and its far integral agree: the shader's primitive is the
    /// profile's integral (above, below and across the reference plane).
    #[test]
    fn the_height_integral_matches_the_profile() {
        let profile = |q: f32| if q >= 0.0 { (-q).exp() } else { 2.0 - q.exp() };
        let primitive = |h: f32, k: f32| if h >= 0.0 { -(-k * h).exp() / k } else { 2.0 * h - (k * h).exp() / k };
        let k = 0.025;
        for (h0, h1) in [(10.0f32, 90.0f32), (-60.0, -5.0), (-40.0, 70.0)] {
            let n = 20000;
            let num: f32 = (0..n).map(|i| profile(k * (h0 + (h1 - h0) * (i as f32 + 0.5) / n as f32))).sum::<f32>() * (h1 - h0) / n as f32;
            let exact = primitive(h1, k) - primitive(h0, k);
            assert!((num - exact).abs() < 1e-3 * exact.abs().max(1.0), "{h0}..{h1}: {num} vs {exact}");
        }
    }
}
