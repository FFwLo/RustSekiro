//! Camera-centred authored m89 skies, with independent material radiance.
use std::{collections::BTreeMap, path::Path};

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset, embedded_path},
    camera::visibility::NoFrustumCulling,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use serde_json::Value;

// Native sky output is scene radiance, like the diffuse lighting accumulator.
// Share its lux-to-radiance bridge; the shader applies the camera exposure once.
const SKY_RADIANCE_SCALE: f32 = crate::lighting::ENV_PER_UNIT;

#[derive(Resource)]
pub(crate) struct SkyEnabled;

/// Event-selected authored sky, such as the Dragon sanctuary's group 8907090.
#[derive(Resource, Default)]
pub(crate) struct SkyOverride(pub Option<&'static str>);

#[derive(Clone, ShaderType, Debug)]
struct SkyInputs {
    colors: [Vec4; 11],
    floats: [Vec4; 48],
    vectors: [Vec4; 10],
    uv: [Vec4; 10],
    sun: Vec4,
    control: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(SkyKey)]
struct SkyMaterial {
    #[uniform(0)]
    inputs: SkyInputs,
    #[texture(1)]
    #[sampler(2)]
    high: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    low: Handle<Image>,
    write_depth: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SkyKey {
    write_depth: bool,
}

impl From<&SkyMaterial> for SkyKey {
    fn from(material: &SkyMaterial) -> Self {
        Self {
            write_depth: material.write_depth,
        }
    }
}

impl Material for SkyMaterial {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.inputs.control.x > 2.5 {
            AlphaMode::Mask(self.inputs.control.w)
        } else if self.inputs.control.x > 1.5 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        }
    }
    fn vertex_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(embedded_path!("sky.wesl"))
            .with_source("embedded")
            .into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn specialize(
        _: &MaterialPipeline,
        d: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        d.primitive.cull_mode = None;
        if let Some(depth) = &mut d.depth_stencil {
            // FarMountain's authored alpha-tested finite cylinder also occludes
            // landscape cloud cards beyond it. The dome itself remains at infinity.
            depth.depth_write_enabled = Some(key.bind_group_data.write_depth);
            depth.depth_compare = Some(CompareFunction::GreaterEqual);
        }
        Ok(())
    }
}

#[derive(Resource)]
struct Library {
    id: String,
    meshes: BTreeMap<String, Handle<Mesh>>,
    materials: BTreeMap<String, Handle<SkyMaterial>>,
    current: String,
    groups: BTreeMap<i32, String>,
    clouds: BTreeMap<String, CloudLayer>,
    horizons: BTreeMap<String, CloudLayer>,
    missing: Option<String>,
    legacy: bool,
    clock_override: bool,
}

#[derive(Clone, Default)]
struct CloudLayer {
    mesh: Handle<Mesh>,
    material: Handle<SkyMaterial>,
}

/// Comparison control for the pre-correction group, cloud and clock behavior.
pub(crate) fn legacy_selection() -> bool {
    std::env::var_os("SEKIRO_LEGACY_SKY_SELECTION").is_some()
}

fn group_models(msb: &sekiro_formats::msb::Msb, legacy: bool) -> BTreeMap<i32, String> {
    let mut groups = BTreeMap::new();
    for part in &msb.parts {
        if let Some(model) = msb.model(part) {
            for &group in &part.entity_group_ids {
                if group > 0 {
                    // The native selector takes the first part: the dome precedes
                    // its separate FarMountain companion in each installed m89 group.
                    if legacy {
                        groups.insert(group, model.name.clone());
                    } else {
                        groups.entry(group).or_insert_with(|| model.name.clone());
                    }
                }
            }
        }
    }
    groups
}

#[derive(Component)]
struct Dome;
#[derive(Component)]
struct CloudDome;
#[derive(Component)]
struct Horizon;

// common.emevd event 100 activates these m89 groups with 2012[1].
// Story time flags 9800..9803 map to the matching authored light/probe hour keys.
fn selection(id: &str, hour: f32) -> &'static str {
    if id.starts_with("m25") {
        return "m900070";
    }
    if id.starts_with("m10") {
        return "m900060";
    }
    let keys = [
        (0., "m900040"),
        (2., "m900050"),
        (6., "m900000"),
        (12., "m900010"),
        (18., "m900020"),
        (22., "m900030"),
    ];
    let hour = hour.rem_euclid(24.);
    keys.into_iter()
        .min_by(|a, b| {
            let distance = |h: f32| {
                let d = (h - hour).abs();
                d.min(24. - d)
            };
            distance(a.0).total_cmp(&distance(b.0))
        })
        .unwrap()
        .1
}

fn inputs(row: &Value) -> SkyInputs {
    let shader = row["shader"].as_str().unwrap_or_default();
    let prefix = if shader.contains("Ver2") {
        "M_Sky__SkyDome_Ver2_snp_0_"
    } else if shader.contains("FarMountain") {
        "M_LandScape__FarMountain_snp_0_"
    } else if shader.contains("M[Sky]_Cloud.") {
        "M_Sky__Cloud_snp_0_"
    } else {
        "M_Sky__SkyDome_snp_0_"
    };
    let values = |name: &str| -> Vec<f32> {
        row["params"][name]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| v.as_f64().unwrap_or(0.) as f32)
            .collect()
    };
    let vector = |name: &str| {
        let v = values(name);
        Vec4::new(
            *v.first().unwrap_or(&0.),
            *v.get(1).unwrap_or(&0.),
            *v.get(2).unwrap_or(&0.),
            *v.get(3).unwrap_or(&0.),
        )
    };
    SkyInputs {
        colors: std::array::from_fn(|i| {
            let slot = if prefix.contains("FarMountain") {
                [10, 11, 12, 13, 8][i.min(4)]
            } else {
                i
            };
            let name = format!("{prefix}color_{slot}");
            let c = vector(&name);
            // SAT colour descriptors request gamma conversion: 1403dc4b0 applies
            // RGB^2.2 before the independent intensity, preserving alpha.
            let intensity = values(&format!("{name}-Intensity"))
                .first()
                .copied()
                .unwrap_or(1.);
            (c.xyz().powf(2.2) * intensity).extend(c.w)
        }),
        floats: std::array::from_fn(|i| vector(&format!("{prefix}float_{i}"))),
        vectors: std::array::from_fn(|i| vector(&format!("{prefix}float3_{i}"))),
        uv: std::array::from_fn(|i| vector(&format!("{prefix}float2_{i}"))),
        sun: Vec4::Y,
        control: Vec4::new(
            if prefix.contains("FarMountain") {
                3.
            } else if prefix.contains("Ver2") {
                1.
            } else if prefix.contains("Cloud_snp") {
                2.
            } else {
                0.
            },
            SKY_RADIANCE_SCALE,
            0.,
            if prefix.contains("FarMountain") {
                values("g_AlphaRef").first().copied().unwrap_or(0.) / 255.
            } else {
                values("GXFT_SATStaticSwitch")
                    .first()
                    .copied()
                    .unwrap_or(0.)
            },
        ),
    }
}

/// Read the existing GLB mesh without spawning its ordinary, lit scene materials.
fn mesh(path: &Path, material_index: usize) -> Result<Mesh> {
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(bytes.get(..4) == Some(b"glTF"), "invalid sky GLB");
    let word = |offset| -> Result<usize> {
        Ok(u32::from_le_bytes(
            bytes
                .get(offset..offset + 4)
                .context("truncated sky GLB")?
                .try_into()?,
        ) as usize)
    };
    let len = word(12)?;
    let json: Value = serde_json::from_slice(bytes.get(20..20 + len).context("GLB JSON")?)?;
    let bin = bytes.get(28 + len..).context("GLB binary")?;
    let p = json["meshes"][0]["primitives"]
        .as_array()
        .context("sky primitives")?
        .iter()
        .find(|p| p["material"].as_u64() == Some(material_index as u64))
        .context("sky dome primitive")?;
    let accessor = |index: &Value, components: usize| -> Result<Vec<Vec<f32>>> {
        let a = &json["accessors"][index.as_u64().context("accessor index")? as usize];
        let v = &json["bufferViews"][a["bufferView"].as_u64().context("buffer view")? as usize];
        anyhow::ensure!(a["componentType"] == 5126, "sky attribute must be float");
        let offset = v["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = v["byteStride"]
            .as_u64()
            .map_or(components * 4, |v| v as usize);
        (0..a["count"].as_u64().context("attribute count")? as usize)
            .map(|i| {
                (0..components)
                    .map(|c| {
                        let o = offset + i * stride + c * 4;
                        Ok(f32::from_le_bytes(
                            bin.get(o..o + 4).context("attribute bytes")?.try_into()?,
                        ))
                    })
                    .collect()
            })
            .collect()
    };
    let positions: Vec<[f32; 3]> = accessor(&p["attributes"]["POSITION"], 3)?
        .into_iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    let uv: Vec<[f32; 2]> = accessor(&p["attributes"]["TEXCOORD_0"], 2)?
        .into_iter()
        .map(|v| [v[0], v[1]])
        .collect();
    let uv_b = if p["attributes"]["TEXCOORD_1"].is_null() {
        uv.clone()
    } else {
        accessor(&p["attributes"]["TEXCOORD_1"], 2)?
            .into_iter()
            .map(|v| [v[0], v[1]])
            .collect()
    };
    let a = &json["accessors"][p["indices"].as_u64().context("indices")? as usize];
    let v = &json["bufferViews"][a["bufferView"].as_u64().context("index view")? as usize];
    let offset = v["byteOffset"].as_u64().unwrap_or(0) as usize
        + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let size = match a["componentType"].as_u64() {
        Some(5123) => 2,
        Some(5125) => 4,
        _ => anyhow::bail!("sky indices type"),
    };
    let indices: Vec<u32> = (0..a["count"].as_u64().context("index count")? as usize)
        .map(|i| {
            let o = offset + i * size;
            let b = bin.get(o..o + size).context("index bytes")?;
            Ok(if size == 2 {
                u16::from_le_bytes(b.try_into()?) as u32
            } else {
                u32::from_le_bytes(b.try_into()?)
            })
        })
        .collect::<Result<_>>()?;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv_b);
    mesh.insert_indices(Indices::U32(indices));
    Ok(mesh)
}

fn texture_slots(
    app: &mut App,
    dir: &Path,
    row: &Value,
    images: &mut BTreeMap<String, Handle<Image>>,
) -> Result<Vec<Handle<Image>>> {
    let mut slots = Vec::new();
    for path in row["textures"]
        .as_object()
        .context("sky textures")?
        .values()
    {
        let path = path.as_str().context("sky texture path")?;
        let srgb = row["shader"]
            .as_str()
            .is_some_and(|s| s.contains("FarMountain"));
        let key = format!("{srgb}:{path}");
        let image = if let Some(image) = images.get(&key) {
            image.clone()
        } else {
            let mut image = Image::from_buffer(
                &std::fs::read(dir.join(path))?,
                bevy::image::ImageType::Extension("dds"),
                bevy::image::CompressedImageFormats::all(),
                srgb,
                ImageSampler::default(),
                RenderAssetUsages::default(),
            )?;
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..default()
            });
            let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
            images.insert(key, image.clone());
            image
        };
        slots.push(image);
    }
    anyhow::ensure!(!slots.is_empty(), "authored sky has no exported textures");
    Ok(slots)
}

pub(super) fn install(app: &mut App, cache: &Path, id: &str, hour: f32) -> Result<()> {
    if std::env::args().any(|a| a == "--no-sky-dome" || a == "--probe-sky") {
        return Ok(());
    }
    let dir = cache.join("maps/m89_00_00_99");
    let Ok(bytes) = std::fs::read(dir.join("sky.json")) else {
        warn!("sky.json missing; run SEKIRO_MAP_SKY_ONLY=1 sekiro-extract map m89_00_00_99");
        return Ok(());
    };
    let json: Value = serde_json::from_slice(&bytes)?;
    embedded_asset!(app, "sky.wesl");
    app.add_plugins(MaterialPlugin::<SkyMaterial>::default());
    let mut library = Library {
        id: id.into(),
        meshes: BTreeMap::new(),
        materials: BTreeMap::new(),
        current: selection(id, hour).into(),
        groups: BTreeMap::new(),
        clouds: BTreeMap::new(),
        horizons: BTreeMap::new(),
        missing: None,
        legacy: legacy_selection(),
        clock_override: std::env::args().any(|a| a == "--hour"),
    };
    let msb = sekiro_formats::msb::parse(&std::fs::read(
        cache.join("raw/map/mapstudio/m89_00_00_99.msb"),
    )?)?;
    library.groups = group_models(&msb, library.legacy);
    let mut images = BTreeMap::<String, Handle<Image>>::new();
    for name in json["models"]
        .as_object()
        .context("sky models")?
        .keys()
        .map(String::as_str)
    {
        let row = &json["models"][name];
        if name == "m900075" && row.is_null() {
            warn!("silvergrass sky m900075 is not extracted in maps/m89_00_00_99/sky.json");
            continue;
        }
        if name == "m900085" && row.is_null() {
            warn!("illusion temple sky m900085 is not extracted in maps/m89_00_00_99/sky.json");
            continue;
        }
        anyhow::ensure!(!row.is_null(), "missing sky material {name}");
        let slots = texture_slots(app, &dir, row, &mut images)?;
        let value = inputs(row);
        let material = app
            .world_mut()
            .resource_mut::<Assets<SkyMaterial>>()
            .add(SkyMaterial {
                inputs: value.clone(),
                high: slots[0].clone(),
                low: slots.last().unwrap().clone(),
                write_depth: false,
            });
        library.materials.insert(name.into(), material);
        library.meshes.insert(
            name.into(),
            app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh(
                &dir.join(format!("models/{name}.glb")),
                row["material_index"].as_u64().unwrap_or(0) as usize,
            )?),
        );
        let cloud = &row["cloud"];
        if !cloud.is_null() {
            let slots = texture_slots(app, &dir, cloud, &mut images)?;
            let mut cloud_inputs = inputs(cloud);
            cloud_inputs.control.y = value.control.y;
            let material = app
                .world_mut()
                .resource_mut::<Assets<SkyMaterial>>()
                .add(SkyMaterial {
                    inputs: cloud_inputs,
                    high: slots[0].clone(),
                    low: slots.last().unwrap().clone(),
                    write_depth: false,
                });
            let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh(
                &dir.join(format!("models/{name}.glb")),
                cloud["material_index"]
                    .as_u64()
                    .context("cloud material index")? as usize,
            )?);
            library
                .clouds
                .insert(name.into(), CloudLayer { mesh, material });
        }
        let horizon = &row["horizon"];
        if !horizon.is_null() {
            let slots = texture_slots(app, &dir, horizon, &mut images)?;
            let material = app
                .world_mut()
                .resource_mut::<Assets<SkyMaterial>>()
                .add(SkyMaterial {
                    inputs: inputs(horizon),
                    high: slots[0].clone(),
                    low: slots[0].clone(),
                    write_depth: horizon["depth_write"].as_bool().unwrap_or(false),
                });
            let model = horizon["model"].as_str().context("horizon model")?;
            let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh(
                &dir.join(format!("models/{model}.glb")),
                horizon["material_index"]
                    .as_u64()
                    .context("horizon material")? as usize,
            )?);
            library
                .horizons
                .insert(name.into(), CloudLayer { mesh, material });
        }
    }
    if let Some(name) = app
        .world()
        .resource::<crate::lighting_selection::LightingSelection>()
        .initial
        .sky
        .as_ref()
    {
        if library.meshes.contains_key(name) {
            library.current.clone_from(name);
        } else {
            warn!(
                "incomplete sky fidelity: event-selected sky {name} is not exported; retaining {}",
                library.current
            );
            library.missing = Some(name.clone());
        }
    }
    app.world_mut().spawn((
        Dome,
        Mesh3d(library.meshes[&library.current].clone()),
        MeshMaterial3d(library.materials[&library.current].clone()),
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    let cloud = library.clouds.get(&library.current).cloned();
    if cloud.is_some() || !library.legacy {
        let visibility = if cloud.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let cloud = cloud.unwrap_or_default();
        app.world_mut().spawn((
            CloudDome,
            Mesh3d(cloud.mesh),
            MeshMaterial3d(cloud.material),
            Transform::default(),
            visibility,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    let horizon = library.horizons.get(&library.current).cloned();
    let visibility = if horizon.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if horizon.is_none() {
        warn!(
            "sky {} has no exported FarMountain companion; refresh the m89 sky export",
            library.current
        );
    }
    let horizon = horizon.unwrap_or_default();
    app.world_mut().spawn((
        Horizon,
        Mesh3d(horizon.mesh),
        MeshMaterial3d(horizon.material),
        Transform::default(),
        visibility,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    info!("authored sky {} for {id} at {hour}h", library.current);
    app.insert_resource(library)
        .init_resource::<SkyOverride>()
        .insert_resource(SkyEnabled)
        .add_systems(Update, update);
    Ok(())
}

type Clouds<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Mesh3d,
        &'static mut MeshMaterial3d<SkyMaterial>,
        &'static mut Visibility,
    ),
    (With<CloudDome>, Without<Dome>, Without<Horizon>),
>;

type Horizons<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Mesh3d,
        &'static mut MeshMaterial3d<SkyMaterial>,
        &'static mut Visibility,
    ),
    (With<Horizon>, Without<Dome>, Without<CloudDome>),
>;

#[allow(clippy::too_many_arguments)]
fn update(
    sky_override: Res<SkyOverride>,
    lighting: Res<crate::lighting::MapLighting>,
    time: Res<Time>,
    mut library: ResMut<Library>,
    mut materials: ResMut<Assets<SkyMaterial>>,
    mut domes: Query<(&mut Mesh3d, &mut MeshMaterial3d<SkyMaterial>), With<Dome>>,
    mut clouds: Clouds<'_, '_>,
    mut horizons: Horizons<'_, '_>,
    scene: Option<Res<crate::arena_b::Scene>>,
    story: Res<crate::lighting_selection::LightingSelection>,
    map_scene: Res<crate::lighting_selection::MapEventScene>,
) {
    let scene = scene.as_deref().map_or(&map_scene.0, |s| &s.0);
    let event_sky = {
        scene
            .enabled_order
            .iter()
            .rev()
            .filter(|_| library.legacy || !library.clock_override)
            .filter(|group| scene.enabled.get(group) == Some(&true))
            .filter_map(|group| library.groups.get(group))
            .find(|name| !library.legacy || library.meshes.contains_key(*name))
            .map(String::as_str)
    };
    let initial_sky = (!library.legacy || lighting.hour == story.initial.hour)
        .then_some(story.initial.sky.as_deref())
        .flatten();
    let selected = sky_override
        .0
        .filter(|name| !library.legacy || library.meshes.contains_key(*name))
        .or(event_sky)
        .or(initial_sky.filter(|name| !library.legacy || library.meshes.contains_key(*name)))
        .unwrap_or_else(|| selection(&library.id, lighting.hour));
    let selected = selected.to_owned();
    let selected = if library.meshes.contains_key(&selected) {
        library.missing = None;
        selected
    } else {
        if library.missing.as_deref() != Some(&selected) {
            warn!(
                "incomplete sky fidelity: event-selected sky {selected} is not exported; retaining {}",
                library.current
            );
            library.missing = Some(selected);
        }
        library.current.clone()
    };
    if library.current != selected {
        for (mut mesh, mut material) in &mut domes {
            mesh.0 = library.meshes[&selected].clone();
            material.0 = library.materials[&selected].clone();
        }
        library.current.clone_from(&selected);
    }
    let set =
        crate::lighting::LightSet::from_gparam(&lighting.gparam, lighting.light_set, lighting.hour);
    if let Some(mut material) = materials.get_mut(&library.materials[&selected]) {
        material.inputs.sun = set.sun_toward.extend(0.);
        material.inputs.control.z = time.elapsed_secs();
    }
    for (mut mesh, mut handle, mut visibility) in &mut clouds {
        if library.legacy {
            *visibility = if sky_override.0.is_some() {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        } else if let Some(cloud) = library.clouds.get(&selected) {
            if mesh.0 != cloud.mesh {
                mesh.0 = cloud.mesh.clone();
            }
            if handle.0 != cloud.material {
                handle.0 = cloud.material.clone();
            }
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.inputs.sun = set.sun_toward.extend(0.);
            material.inputs.control.z = time.elapsed_secs();
        }
    }
    for (mut mesh, mut handle, mut visibility) in &mut horizons {
        if let Some(layer) = library.horizons.get(&selected) {
            mesh.0 = layer.mesh.clone();
            handle.0 = layer.material.clone();
            *visibility = Visibility::Inherited;
            if let Some(mut material) = materials.get_mut(&handle.0) {
                material.inputs.sun = set.sun_toward.extend(0.);
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sky_app() -> App {
        let cache = sekiro_sim::cache::root();
        let id = "m11_01_00_00";
        let initial = crate::lighting_selection::select(&cache, id, None, None).unwrap();
        let story =
            crate::lighting_selection::LightingSelection::load(&cache, id, initial, false).unwrap();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<SkyMaterial>::default();
        let mut library = Library {
            id: id.into(),
            meshes: BTreeMap::new(),
            materials: BTreeMap::new(),
            current: story.initial.sky.clone().unwrap(),
            groups: BTreeMap::new(),
            clouds: BTreeMap::new(),
            horizons: BTreeMap::new(),
            missing: None,
            legacy: false,
            clock_override: false,
        };
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache.join("raw/map/mapstudio/m89_00_00_99.msb")).unwrap(),
        )
        .unwrap();
        library.groups = group_models(&msb, false);
        let json: Value = serde_json::from_slice(
            &std::fs::read(cache.join("maps/m89_00_00_99/sky.json")).unwrap(),
        )
        .unwrap();
        for (name, row) in json["models"].as_object().unwrap() {
            if row.is_null() {
                continue;
            }
            let material = |row| SkyMaterial {
                inputs: inputs(row),
                high: default(),
                low: default(),
                write_depth: row["depth_write"].as_bool().unwrap_or(false),
            };
            let geometry = || {
                Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
            };
            library.meshes.insert(name.clone(), meshes.add(geometry()));
            library
                .materials
                .insert(name.clone(), materials.add(material(row)));
            if !row["cloud"].is_null() {
                library.clouds.insert(
                    name.clone(),
                    CloudLayer {
                        mesh: meshes.add(geometry()),
                        material: materials.add(material(&row["cloud"])),
                    },
                );
            }
            if !row["horizon"].is_null() {
                library.horizons.insert(
                    name.clone(),
                    CloudLayer {
                        mesh: meshes.add(geometry()),
                        material: materials.add(material(&row["horizon"])),
                    },
                );
            }
        }
        let mut app = App::new();
        app.world_mut().spawn((
            Dome,
            Mesh3d(library.meshes[&library.current].clone()),
            MeshMaterial3d(library.materials[&library.current].clone()),
        ));
        app.world_mut().spawn((
            CloudDome,
            Mesh3d::default(),
            MeshMaterial3d::<SkyMaterial>::default(),
            Visibility::Hidden,
        ));
        app.world_mut().spawn((
            Horizon,
            Mesh3d::default(),
            MeshMaterial3d::<SkyMaterial>::default(),
            Visibility::Hidden,
        ));
        app.insert_resource(crate::lighting::MapLighting {
            gparam: default(),
            hour: story.initial.hour,
            light_set: 0,
            applied: None,
            source: String::new(),
        })
        .insert_resource(story)
        .insert_resource(library)
        .insert_resource(materials)
        .insert_resource(Time::<()>::default())
        .init_resource::<SkyOverride>()
        .init_resource::<crate::lighting_selection::MapEventScene>()
        .add_systems(Update, update);
        app
    }

    #[test]
    fn event_sky_keeps_its_native_painted_horizon_companion() {
        let mut app = sky_app();
        let cache = sekiro_sim::cache::root();
        let json: Value = serde_json::from_slice(
            &std::fs::read(cache.join("maps/m89_00_00_99/sky.json")).unwrap(),
        )
        .unwrap();
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache.join("raw/map/mapstudio/m89_00_00_99.msb")).unwrap(),
        )
        .unwrap();
        for (dome, group) in [
            ("m900000", 8907000),
            ("m900030", 8907030),
            ("m900080", 8907080),
            ("m900095", 8907100),
        ] {
            let row = &json["models"][dome]["horizon"];
            let companion = row["model"].as_str().expect("refresh m89 sky export");
            assert!(msb.parts.iter().any(|p| p.entity_group_ids.contains(&group)
                && msb.model(p).is_some_and(|m| m.name == companion)));
            assert!(row["shader"].as_str().unwrap().contains("FarMountain"));
            let geometry = mesh(
                &cache.join(format!("maps/m89_00_00_99/models/{companion}.glb")),
                row["material_index"].as_u64().unwrap() as usize,
            )
            .unwrap();
            assert!(geometry.count_vertices() > 0);
            app.world_mut().resource_mut::<SkyOverride>().0 = Some(dome);
            app.update();
            let world = app.world_mut();
            let mut query = world.query_filtered::<(&Mesh3d, &MeshMaterial3d<SkyMaterial>, &Visibility),With<Horizon>>();
            let (mesh, material, visibility) = query.single(world).unwrap();
            let expected = &world.resource::<Library>().horizons[dome];
            assert_eq!(mesh.0, expected.mesh);
            assert_eq!(material.0, expected.material);
            assert_eq!(*visibility, Visibility::Inherited);
            assert_eq!(
                world
                    .resource::<Assets<SkyMaterial>>()
                    .get(&material.0)
                    .unwrap()
                    .inputs
                    .control
                    .x,
                3.
            );
        }
    }

    #[test]
    fn native_group_mapping_keeps_all_exported_domes() {
        let app = sky_app();
        let library = app.world().resource::<Library>();
        let cache = sekiro_sim::cache::root();
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache.join("raw/map/mapstudio/m89_00_00_99.msb")).unwrap(),
        )
        .unwrap();
        let before = group_models(&msb, true);
        let mut fixed = 0;
        for (&group, name) in &library.groups {
            if library.meshes.contains_key(name) {
                println!("sky group {group}: {} -> {name}", before[&group]);
                assert!(!library.meshes.contains_key(&before[&group]));
                fixed += 1;
            }
        }
        assert_eq!(fixed, 13);
        assert_eq!(library.groups[&8907080], "m900080");
        assert_eq!(library.groups[&8907100], "m900095");
        assert_eq!(library.groups[&8907085], "m900085");
        assert!(!library.meshes.contains_key("m900085"));
    }

    #[test]
    fn event_sky_survives_clock_changes_and_missing_exports() {
        let mut app = sky_app();
        app.world_mut()
            .resource_mut::<crate::lighting::MapLighting>()
            .hour = 22.;
        app.update();
        assert_eq!(app.world().resource::<Library>().current, "m900000");
        for (group, expected, missing) in [
            (8907080, "m900080", None),
            (8907085, "m900080", Some("m900085")),
            (8907100, "m900095", None),
        ] {
            let mut scene = app
                .world_mut()
                .resource_mut::<crate::lighting_selection::MapEventScene>();
            scene.0.enabled.insert(group, true);
            scene.0.enabled_order.push(group);
            app.update();
            let library = app.world().resource::<Library>();
            assert_eq!(library.current, expected);
            assert_eq!(library.missing.as_deref(), missing);
        }
    }

    #[test]
    fn explicit_hour_keeps_its_diagnostic_sky_despite_story_events() {
        let mut app = sky_app();
        app.world_mut().resource_mut::<Library>().clock_override = true;
        app.world_mut()
            .resource_mut::<crate::lighting_selection::LightingSelection>()
            .initial
            .sky = None;
        app.world_mut()
            .resource_mut::<crate::lighting::MapLighting>()
            .hour = 22.;
        let mut scene = app
            .world_mut()
            .resource_mut::<crate::lighting_selection::MapEventScene>();
        scene.0.enabled.insert(8907080, true);
        scene.0.enabled_order.push(8907080);
        app.update();
        assert_eq!(app.world().resource::<Library>().current, "m900030");
    }

    #[test]
    fn sky_switches_matching_exported_cloud_mesh_and_material() {
        let mut app = sky_app();
        for name in ["m900060", "m900070", "m900000"] {
            app.world_mut().resource_mut::<SkyOverride>().0 = Some(name);
            app.update();
            let world = app.world_mut();
            let mut query = world.query_filtered::<(&Mesh3d, &MeshMaterial3d<SkyMaterial>, &Visibility), With<CloudDome>>();
            let (mesh, material, visibility) = query.single(world).unwrap();
            if let Some(layer) = world.resource::<Library>().clouds.get(name) {
                assert_eq!(mesh.0, layer.mesh);
                assert_eq!(material.0, layer.material);
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(
                    world
                        .resource::<Assets<SkyMaterial>>()
                        .get(&material.0)
                        .unwrap()
                        .alpha_mode(),
                    AlphaMode::Blend
                );
            } else {
                assert_eq!(*visibility, Visibility::Hidden);
            }
        }
    }
    #[test]
    fn sky_radiance_uses_the_same_bridge_as_white_diffuse_sunlight() {
        let exposure = bevy::camera::Exposure::default().exposure();
        // Native white sky radiance and a white Lambert surface at normal incidence
        // have equal native scene values. Their Bevy HDR values must still match.
        let diffuse = crate::lighting::SCENE_TO_BEVY * exposure / std::f32::consts::PI;
        for shader in ["SkyDome_Ver2", "SkyDome", "M[Sky]_Cloud.spx"] {
            let row = serde_json::json!({"shader":shader, "params":{}});
            let hdr = inputs(&row).control.y * exposure;
            assert!(
                (hdr - diffuse).abs() < 0.00001,
                "{shader}: {hdr} != {diffuse}"
            );
        }
    }
    #[test]
    fn native_common_event_and_msb_agree_on_selected_groups() {
        use sekiro_formats::{emevd::Emevd, msb};
        let raw = sekiro_sim::cache::root().join("raw");
        let Ok(bytes) = std::fs::read(raw.join("event/common.emevd")) else {
            return;
        };
        let events = Emevd::parse(&bytes).unwrap();
        let event = events.event(100).unwrap();
        let map = msb::parse(&std::fs::read(raw.join("map/mapstudio/m89_00_00_99.msb")).unwrap())
            .unwrap();
        for (model, group) in [
            ("m900000", 8907000),
            ("m900010", 8907010),
            ("m900020", 8907020),
            ("m900030", 8907030),
            ("m900040", 8907040),
            ("m900050", 8907050),
            ("m900060", 8907060),
            ("m900070", 8907070),
        ] {
            assert!(
                map.parts
                    .iter()
                    .any(|p| map.model(p).is_some_and(|m| m.name == model)
                        && p.entity_group_ids.contains(&group))
            );
            for state in [0, 1] {
                assert!(event.instructions.iter().any(|i| i.code() == (2012, 1)
                    && i.i32_at(0) == Some(group)
                    && i.u8_at(4) == Some(state)));
            }
        }
        assert!(!SkyMaterial::enable_prepass());
        assert!(!SkyMaterial::enable_shadows());
    }
    #[test]
    fn hours_wrap_and_special_maps_keep_their_authored_environment() {
        for (h, n) in [
            (6., "m900000"),
            (12., "m900010"),
            (18., "m900020"),
            (0., "m900040"),
            (24., "m900040"),
            (-6., "m900020"),
        ] {
            assert_eq!(selection("m11_00_00_00", h), n);
            assert_eq!(selection("m11_01_00_00", h), n);
        }
        assert_eq!(selection("m25_00_00_00", 12.), "m900070");
        assert_eq!(selection("m10_00_00_00", 12.), "m900060");
    }
    #[test]
    fn library_meshes_and_material_inputs_are_valid() {
        let dir = sekiro_sim::cache::root().join("maps/m89_00_00_99");
        let Ok(bytes) = std::fs::read(dir.join("sky.json")) else {
            return;
        };
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        for name in [
            "m900000", "m900010", "m900020", "m900040", "m900060", "m900070",
        ] {
            let mesh = mesh(
                &dir.join(format!("models/{name}.glb")),
                json["models"][name]["material_index"].as_u64().unwrap_or(0) as usize,
            )
            .unwrap();
            assert!(mesh.count_vertices() > 0);
            assert!(!mesh.indices().unwrap().is_empty());
            assert!(
                inputs(&json["models"][name])
                    .colors
                    .iter()
                    .all(|v| v.is_finite())
            );
        }
    }
}
