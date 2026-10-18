//! The game's sky: one dome of the sky library m89_00_00_99 around the camera at infinite depth
//! (tools/sekiro-extract `sky` -> extracted/sky/sky.json), in place of the GI probe shown as the
//! sky box (a capture from inside the level: nearby scenery blown up behind the world).
//!
//! common.emevd event 100 shows one library group by the story time (flags 9800-9803: morning,
//! afternoon, evening, night = m900000 / 010 / 020 / 030, with past night 040 and dawn 050), and
//! fixed ones on m10 (060) and m25 (070). Here the hour picks the nearest of the six keys, as
//! the light set and probe do. Adapted from sekiro-rs (MIT / Apache-2.0) crates/game/src/sky.rs
//! (9ee18b0): the selection, the material inputs and the radiance bridge (the probes' scale).
//! 2026-10-13: the Ver2 math checked against the game's own shader (shaderbdle
//! M[Sky]_SkyDome_Ver2_{0,1}_Fwd.ppo, D3DDisassemble): colours are rgb^2.2 x Intensity (was gamma
//! 2), switch 1's sun lobe uses the MTD's float3_6 (not the scene sun); kb/map.md "Sky domes".
//! `SHINOBI_MAP_PROBE_SKY=1` keeps the probe sky box instead.
//! The painted far-mountain range: the dome's "horizon" companion (m900130 for m900030; the
//! extractor finds it by the m89 MSB's entity groups), drawn with the FarMountain material's own
//! colours (sekiro-rs docs/BACKDROP.md, 2026-10-15; adopted 2026-10-17).
//! gap: the castle's late-story fire sky (Cloud_Fire) and the debug 8907015 are not chosen.

use bevy::asset::embedded_path;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use serde_json::Value;
use std::collections::BTreeMap;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "sky.wgsl");
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default()).init_resource::<SkyChoice>().add_systems(Update, (load_library, update_sky).chain());
    }
}

#[derive(Clone, ShaderType, Debug)]
pub(crate) struct SkyInputs {
    colors: [Vec4; 11],
    floats: [Vec4; 48],
    vectors: [Vec4; 10],
    uv: [Vec4; 10],
    sun: Vec4,
    /// Kind (0 SkyDome, 1 Ver2, 2 Cloud, 3 FarMountain), radiance scale, time,
    /// GXFT_SATStaticSwitch (FarMountain: its g_AlphaRef / 255).
    control: Vec4,
    /// The game's world +X in arena space (the Ver2 clouds' flow axis, sky.wgsl `flow`).
    flow_x: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct SkyMaterial {
    #[uniform(0)]
    inputs: SkyInputs,
    #[texture(1)]
    #[sampler(2)]
    high: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    low: Handle<Image>,
}

impl Material for SkyMaterial {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn alpha_mode(&self) -> AlphaMode {
        // The FarMountain range is alpha-tested (discard below g_AlphaRef), not blended.
        if self.inputs.control.x > 1.5 && self.inputs.control.x < 2.5 { AlphaMode::Blend } else { AlphaMode::Opaque }
    }
    fn vertex_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(embedded_path!("sky.wgsl")).with_source("embedded").into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn specialize(_: &MaterialPipeline, d: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        d.primitive.cull_mode = None;
        // The opaque dome writes its own depth (it is drawn at its real distance, 3-6.4 km), so the
        // fog pass fogs it at that view depth, as the game's sky shader samples the froxel fog at
        // its v1.w (docs/kb/sky_shader.md section 6). The blended Cloud layer writes none.
        let opaque = d.fragment.as_ref().is_none_or(|f| f.targets.iter().flatten().all(|t| t.blend.is_none()));
        if let Some(depth) = &mut d.depth_stencil {
            depth.depth_write_enabled = Some(opaque);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        Ok(())
    }
}

/// The dome for a map at an hour: m10's and m25's own, else the nearest of the six story-time
/// keys (common.emevd event 100's groups, the light set's hours).
/// Every dome of the library with its story time (common.emevd event 100's groups; m10's and
/// m25's own), for the debug menu's sky choice.
pub const DOMES: [(&str, &str); 8] = [
    ("m900000", "morning"),
    ("m900010", "afternoon"),
    ("m900020", "evening"),
    ("m900030", "night"),
    ("m900040", "past night"),
    ("m900050", "dawn"),
    ("m900060", "Hirata Estate"),
    ("m900070", "Fountainhead Palace"),
];

/// Debug menu: a dome shown in place of the map's and hour's own (None = `selection`).
#[derive(Resource, Default)]
pub struct SkyChoice(pub Option<&'static str>);

pub(crate) fn selection(source_map: &str, hour: f32) -> &'static str {
    if source_map.starts_with("m25") {
        return "m900070";
    }
    if source_map.starts_with("m10") {
        return "m900060";
    }
    let keys = [(0.0, "m900040"), (2.0, "m900050"), (6.0, "m900000"), (12.0, "m900010"), (18.0, "m900020"), (22.0, "m900030")];
    let hour = hour.rem_euclid(24.0);
    let d = |h: f32| (h - hour).abs().min(24.0 - (h - hour).abs());
    keys.into_iter().min_by(|a, b| d(a.0).total_cmp(&d(b.0))).unwrap().1
}

/// A material's MTD params as the shader's inputs (sekiro-rs sky.rs `inputs`).
pub(crate) fn inputs(row: &Value, radiance: f32) -> SkyInputs {
    let shader = row["shader"].as_str().unwrap_or_default();
    let (prefix, kind) = if shader.contains("FarMountain") {
        ("M_LandScape__FarMountain_snp_0_", 3.0)
    } else if shader.contains("Ver2") {
        ("M_Sky__SkyDome_Ver2_snp_0_", 1.0)
    } else if shader.contains("M[Sky]_Cloud.") {
        ("M_Sky__Cloud_snp_0_", 2.0)
    } else {
        ("M_Sky__SkyDome_snp_0_", 0.0)
    };
    let values = |name: &str| -> Vec<f32> { row["params"][name].as_array().into_iter().flatten().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect() };
    let vector = |name: &str| {
        let v = values(name);
        Vec4::new(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0), v.get(3).copied().unwrap_or(0.0))
    };
    SkyInputs {
        colors: std::array::from_fn(|i| {
            // FarMountain's colours 10, 11, 12, 13 and 8 in slots 0-4 (sky.wgsl's terms).
            let slot = if kind == 3.0 { [10, 11, 12, 13, 8][i.min(4)] } else { i };
            let name = format!("{prefix}color_{slot}");
            let c = vector(&name).truncate();
            let intensity = values(&format!("{name}-Intensity")).first().copied().unwrap_or(1.0);
            // GXMaterialParam_HDRColor (FUN_1403dc4b0): rgb^2.2 x Intensity; the metaparam flag
            // byte +0x2E that turns the power on is 1 for every colour of shaderbdle (2026-10-13).
            (c.powf(2.2) * intensity).extend(1.0)
        }),
        floats: std::array::from_fn(|i| vector(&format!("{prefix}float_{i}"))),
        vectors: std::array::from_fn(|i| vector(&format!("{prefix}float3_{i}"))),
        uv: std::array::from_fn(|i| vector(&format!("{prefix}float2_{i}"))),
        sun: Vec4::Y,
        control: Vec4::new(kind, radiance, 0.0, if kind == 3.0 { values("g_AlphaRef").first().copied().unwrap_or(0.0) / 255.0 } else { values("GXFT_SATStaticSwitch").first().copied().unwrap_or(0.0) }),
        flow_x: Vec4::new(-1.0, 0.0, 0.0, 0.0),
    }
}

/// A dome mesh from sky.json in the arena: game space mirrored in X and turned by the arena's
/// yaw (as the map pieces; fog_wall.rs `map_to_arena` without the origin, the dome being
/// camera-centred).
pub(crate) fn dome_mesh(m: &Value, arena_yaw: f32) -> Option<Mesh> {
    let (s, c) = arena_yaw.sin_cos();
    let f = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
    let pos: Vec<[f32; 3]> = m["pos"].as_array()?.iter().map(|p| [-(f(&p[0]) * c + f(&p[2]) * s), f(&p[1]), -f(&p[0]) * s + f(&p[2]) * c]).collect();
    let uv = |k: &str| -> Option<Vec<[f32; 2]>> { Some(m[k].as_array()?.iter().map(|p| [f(&p[0]), f(&p[1])]).collect()) };
    let idx: Vec<u32> = m["idx"].as_array()?.iter().filter_map(|i| i.as_u64().map(|i| i as u32)).collect();
    if pos.is_empty() || idx.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv("uv")?);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv("uv2")?);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

#[derive(Resource)]
struct Library {
    json: Value,
    current: String,
    yaw: f32,
    images: BTreeMap<String, Handle<Image>>,
}

#[derive(Component)]
struct Dome;

/// Reads sky.json once a map's lighting is there.
fn load_library(mut commands: Commands, lighting: Option<Res<crate::map::MapLighting>>, library: Option<Res<Library>>, mut tried: Local<bool>) {
    if library.is_some() || *tried || lighting.is_none() || std::env::var("SHINOBI_MAP_PROBE_SKY").is_ok() {
        return;
    }
    *tried = true;
    let Some(json) = std::fs::read_to_string(crate::paths::extracted().join("sky/sky.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        warn!("sky: no extracted/sky/sky.json (sekiro-extract sky <extracted>): the probe stays the sky");
        return;
    };
    commands.insert_resource(Library { json, current: String::new(), yaw: f32::NAN, images: BTreeMap::new() });
}

/// Puts up (or swaps) the dome for the map and hour, keeps its sun and drift time current, and
/// takes the probe's sky box off the cameras.
#[allow(clippy::too_many_arguments)]
fn update_sky(
    mut commands: Commands,
    lighting: Option<Res<crate::map::MapLighting>>,
    library: Option<ResMut<Library>>,
    time: Res<Time>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
    domes: Query<(Entity, &MeshMaterial3d<SkyMaterial>), With<Dome>>,
    skyboxes: Query<Entity, (With<Camera3d>, With<bevy::core_pipeline::Skybox>)>,
    choice: Res<SkyChoice>,
) {
    let (Some(lighting), Some(mut library)) = (lighting, library) else { return };
    let want = choice.0.unwrap_or_else(|| selection(&lighting.source_map, lighting.hour));
    if library.current != want || library.yaw != lighting.arena_yaw {
        for (e, _) in &domes {
            commands.entity(e).despawn();
        }
        library.current = want.to_string();
        library.yaw = lighting.arena_yaw;
        let row = library.json["models"][want].clone();
        let radiance = crate::map::ENV_PER_UNIT;
        for key in ["dome", "cloud", "horizon"] {
            let m = &row[key];
            let Some(mesh) = m.get("mesh").and_then(|x| dome_mesh(x, lighting.arena_yaw)) else { continue };
            // The texture slots in name order: the first is the high mask, the last the low.
            let slots: Vec<Handle<Image>> = m["textures"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(_, p)| p.as_str())
                .map(|p| {
                    library
                        .images
                        .entry(p.to_string())
                        .or_insert_with(|| {
                            assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| {
                                s.is_srgb = false;
                                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() });
                            }).load(p.to_string())
                        })
                        .clone()
                })
                .collect();
            let (Some(high), Some(low)) = (slots.first().cloned(), slots.last().cloned()) else { continue };
            let material = materials.add(SkyMaterial { inputs: inputs(m, radiance), high, low });
            commands.spawn((Dome, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), NoFrustumCulling, NotShadowCaster, NotShadowReceiver, Name::new(format!("sky {want} {key}"))));
        }
        info!("sky: dome {want} ({}) at {:.1} h", lighting.source_map, lighting.hour);
    }
    let sun = crate::map::sun_toward(&lighting).extend(0.0);
    // Game +X through dome_mesh's mirror and yaw.
    let (ys, yc) = lighting.arena_yaw.sin_cos();
    let flow_x = Vec4::new(-yc, 0.0, -ys, 0.0);
    for (_, h) in &domes {
        if let Some(mut m) = materials.get_mut(&h.0) {
            m.inputs.sun = sun;
            m.inputs.flow_x = flow_x;
            m.inputs.control.z = time.elapsed_secs();
        }
    }
    for e in &skyboxes {
        commands.entity(e).remove::<bevy::core_pipeline::Skybox>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// common.emevd event 100's story-time groups by hour (wrapping), and m10's / m25's own.
    #[test]
    fn the_hour_and_map_pick_the_dome() {
        for (h, n) in [(6.0, "m900000"), (12.0, "m900010"), (18.0, "m900020"), (22.0, "m900030"), (0.0, "m900040"), (24.0, "m900040"), (2.0, "m900050"), (-6.0, "m900020")] {
            assert_eq!(selection("m11_01_00_00", h), n, "{h} h");
        }
        assert_eq!(selection("m25_00_00_00", 12.0), "m900070");
        assert_eq!(selection("m10_00_00_00", 12.0), "m900060");
    }

    /// The exported library: every dome has a mesh and its two masks, and its inputs are finite;
    /// the colours are rgb^2.2 times their Intensity (m900000 color_2: white x30).
    #[test]
    fn the_exported_domes_load() {
        let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("sky/sky.json")) else {
            eprintln!("no sky.json: skipped");
            return;
        };
        let json: Value = serde_json::from_str(&text).unwrap();
        for name in ["m900000", "m900010", "m900020", "m900030", "m900040", "m900050", "m900060", "m900070"] {
            let row = &json["models"][name]["dome"];
            let mesh = dome_mesh(&row["mesh"], 0.7).unwrap_or_else(|| panic!("{name}: no mesh"));
            assert!(mesh.count_vertices() > 100, "{name}");
            assert!(row["textures"].as_object().is_some_and(|t| !t.is_empty()), "{name}: no textures");
            let i = inputs(row, 1.0);
            assert!(i.colors.iter().chain(&i.floats).all(|v| v.is_finite()), "{name}");
        }
        let morning = inputs(&json["models"]["m900000"]["dome"], 1.0);
        assert_eq!(morning.control.x, 1.0, "m900000 is a Ver2 dome");
        assert!((morning.colors[2].x - 30.0).abs() < 1e-4, "{:?}", morning.colors[2]);
        // The night dome's painted range m900130 (FarMountain, alpha ref 64).
        let night = &json["models"]["m900030"]["horizon"];
        assert_eq!(night["model"], "m900130");
        let far = inputs(night, 1.0);
        assert_eq!(far.control.x, 3.0);
        assert!((far.control.w - 64.0 / 255.0).abs() < 1e-6);
        assert!(dome_mesh(&night["mesh"], 0.0).is_some_and(|m| m.count_vertices() == 52));
    }
}
