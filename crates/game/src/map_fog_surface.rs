//! Authored landscape fog cards, independent of the volumetric fog pass.
use std::collections::HashMap;

use bevy::{
    asset::{embedded_asset, embedded_path},
    gltf::GltfMaterialExtras,
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    light::{NotShadowCaster, NotShadowReceiver},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use serde_json::Value;

#[derive(Clone, Debug, ShaderType)]
struct Inputs {
    colors: [Vec4; 4],
    floats: [Vec4; 14],
    vectors: [Vec4; 18],
    uv: [Vec4; 4],
    control: Vec4,
    lighting: LightingInputs,
}

// Six contiguous vec4s retain the shader's existing uniform layout.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
struct LightingInputs {
    sun: Vec4,
    direct: Vec4,
    specular: Vec4,
    ambient: Vec4,
    hemi_axis: Vec4,
    hemi_delta: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
#[bind_group_data(FogKey)]
pub(super) struct FogMaterial {
    #[uniform(0)]
    inputs: Inputs,
    #[texture(1)]
    #[sampler(2)]
    mask: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    flow: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    ground: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    height: Handle<Image>,
    #[storage(9, read_only)]
    irradiance: Handle<bevy::render::storage::ShaderBuffer>,
    #[texture(10, dimension = "3d")]
    #[sampler(11)]
    voxels: Option<Handle<Image>>,
    #[texture(12, dimension = "cube_array")]
    #[sampler(13)]
    diffuse: Option<Handle<Image>>,
    double_sided: bool,
    diagnostic: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct FogKey {
    double_sided: bool,
    diagnostic: bool,
}
impl From<&FogMaterial> for FogKey {
    fn from(m: &FogMaterial) -> Self {
        Self {
            double_sided: m.double_sided,
            diagnostic: m.diagnostic,
        }
    }
}

impl Material for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(embedded_path!("map_fog_surface.wesl"))
            .with_source("embedded")
            .into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.diagnostic {
            AlphaMode::Mask(0.05)
        } else {
            AlphaMode::Blend
        }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = if key.bind_group_data.double_sided {
            None
        } else {
            Some(Face::Back)
        };
        if key.bind_group_data.diagnostic {
            descriptor
                .fragment
                .as_mut()
                .unwrap()
                .shader_defs
                .push("FOG_DIAGNOSTIC".into());
        }
        Ok(())
    }
}

#[derive(Resource)]
struct Settings {
    prefix: String,
    enabled: bool,
    empty_gi: Handle<bevy::render::storage::ShaderBuffer>,
    materials: HashMap<AssetId<StandardMaterial>, Handle<FogMaterial>>,
    definitions: HashMap<AssetId<StandardMaterial>, Option<Value>>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FogConversion;

pub(crate) fn install(app: &mut App, prefix: &str) {
    embedded_asset!(app, "map_fog_surface.wesl");
    let empty_gi = app
        .world_mut()
        .resource_mut::<Assets<bevy::render::storage::ShaderBuffer>>()
        .add(bevy::render::storage::ShaderBuffer::from(vec![0u32; 4]));
    app.add_plugins(MaterialPlugin::<FogMaterial>::default())
        .insert_resource(Settings {
            prefix: prefix.to_owned(),
            enabled: !std::env::args().any(|a| a == "--no-fog-surfaces"),
            empty_gi,
            materials: HashMap::new(),
            definitions: HashMap::new(),
        })
        // Replace before the ordinary map material wrapper consumes these entities.
        .add_systems(PreUpdate, replace.in_set(FogConversion))
        .add_systems(
            PostUpdate,
            update_lighting.after(crate::lighting::apply_lighting),
        );
}

fn inputs(data: &Value) -> Inputs {
    let family = data["family"].as_str().unwrap_or("CloudFog");
    let params = &data["params"];
    let prefix = format!("M_LandScape__{family}_snp_0_");
    let read = |name: &str| -> Vec4 {
        let v = params[name].as_array();
        Vec4::from_array(std::array::from_fn(|i| {
            v.and_then(|v| v.get(i))
                .and_then(Value::as_f64)
                .unwrap_or(0.) as f32
        }))
    };
    let mut result = Inputs {
        colors: std::array::from_fn(|i| {
            let name = format!("{prefix}color_{i}");
            let c = read(&name);
            let intensity = params[format!("{name}-Intensity")][0]
                .as_f64()
                .unwrap_or(1.) as f32;
            // SAT colour upload 1403dc4b0 converts RGB with gamma 2.2, then
            // applies intensity to RGB only; alpha is independent.
            (c.xyz().powf(2.2) * intensity).extend(c.w)
        }),
        floats: std::array::from_fn(|i| read(&format!("{prefix}float_{i}"))),
        vectors: [Vec4::ZERO; 18],
        uv: [Vec4::new(1., 1., 0., 0.); 4],
        control: Vec4::new(
            f32::from(family == "GroundFog"),
            params["GXFT_SATStaticSwitch"][0].as_f64().unwrap_or(0.) as f32,
            0.,
            0.,
        ),
        lighting: LightingInputs {
            sun: Vec4::Y,
            direct: Vec4::ZERO,
            specular: Vec4::ZERO,
            ambient: Vec4::ZERO,
            hemi_axis: Vec4::ZERO,
            hemi_delta: Vec4::ZERO,
        },
    };
    // float2 and float3 slots overlap numerically, but are separate native registers.
    for i in 0..6 {
        result.vectors[i] = read(&format!("{prefix}float3_{i}"));
    }
    for i in 0..12 {
        result.vectors[i + 6] = read(&format!("{prefix}float2_{i}"));
    }
    result.colors[1] = read("g_DiffuseMapColor");
    result.colors[1] = Vec4::new(
        result.colors[1].x.powi(2),
        result.colors[1].y.powi(2),
        result.colors[1].z.powi(2),
        1.,
    );
    result
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn replace(
    mut commands: Commands,
    assets: Res<AssetServer>,
    standard: Res<Assets<StandardMaterial>>,
    mut fog: ResMut<Assets<FogMaterial>>,
    mut settings: ResMut<Settings>,
    meshes: Query<(
        Entity,
        &MeshMaterial3d<StandardMaterial>,
        &GltfMaterialExtras,
    )>,
    irradiance: Option<Res<crate::lighting::MapIrradiance>>,
    mut changed: MessageReader<AssetEvent<StandardMaterial>>,
) {
    for event in changed.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            settings.definitions.remove(id);
            settings.materials.remove(id);
        }
    }
    for (entity, original, extras) in &meshes {
        // Most meshes are ordinary surfaces. Parse each shared material once rather
        // than parsing every material's complete layer metadata for every instance.
        let definition = settings
            .definitions
            .entry(original.0.id())
            .or_insert_with(|| {
                let mut extras = serde_json::from_str::<Value>(&extras.value).ok()?;
                let data = extras["sekiro_fog_surface"].take();
                (data["version"].as_u64() == Some(1)).then_some(data)
            })
            .clone();
        let Some(data) = definition else {
            continue;
        };
        if !standard.contains(&original.0) {
            continue;
        }
        if !settings.enabled {
            commands
                .entity(entity)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(Visibility::Hidden);
            continue;
        }
        let prefix = settings.prefix.clone();
        let empty_gi = settings.empty_gi.clone();
        let handle = settings
            .materials
            .entry(original.0.id())
            .or_insert_with(|| {
                let mut inputs = inputs(&data);
                let textures = data["textures"].as_array();
                let slots = if inputs.control.x > 0. {
                    [0, 2, 3, 4]
                } else {
                    [1, 7, 1, 7]
                };
                info!(
                    "map fog surface material: family={}, slots={slots:?}",
                    data["family"].as_str().unwrap_or_default()
                );
                let images: [Handle<Image>; 4] = std::array::from_fn(|i| {
                    let row = textures
                        .and_then(|t| t.iter().find(|t| t["slot"].as_u64() == Some(slots[i])));
                    let Some(row) = row else {
                        return Handle::default();
                    };
                    let Some(uri) = row["uri"].as_str() else {
                        return Handle::default();
                    };
                    inputs.uv[i] = Vec4::new(
                        row["scale"][0].as_f64().unwrap_or(1.) as f32,
                        row["scale"][1].as_f64().unwrap_or(1.) as f32,
                        row["uv"].as_f64().unwrap_or(0.) as f32,
                        0.,
                    );
                    // Every bank is relative to this part's source block, including neighbours.
                    let path = crate::map::world::material_texture_path(&assets, &original.0, uri)
                        .unwrap_or_else(|| format!("{prefix}models/{uri}"));
                    let srgb =
                        inputs.control.x > 0. && (i == 1 || (i == 2 && inputs.control.y < 0.5));
                    assets
                        .load_builder()
                        .with_settings(move |s: &mut ImageLoaderSettings| {
                            s.is_srgb = srgb;
                            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                                address_mode_u: ImageAddressMode::Repeat,
                                address_mode_v: ImageAddressMode::Repeat,
                                ..ImageSamplerDescriptor::linear()
                            });
                        })
                        .load(path)
                });
                fog.add(FogMaterial {
                    inputs,
                    mask: images[0].clone(),
                    flow: images[1].clone(),
                    ground: images[2].clone(),
                    height: images[3].clone(),
                    irradiance: irradiance.as_ref().map_or(empty_gi, |i| i.buffer.clone()),
                    voxels: irradiance.as_ref().map(|i| i.voxels.clone()),
                    diffuse: irradiance.as_ref().map(|i| i.diffuse.clone()),
                    double_sided: standard.get(&original.0).is_some_and(|m| m.double_sided),
                    diagnostic: false,
                })
            })
            .clone();
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((MeshMaterial3d(handle), NotShadowCaster, NotShadowReceiver));
    }
}

fn update_lighting(
    lighting: Res<crate::lighting::MapLighting>,
    ambient_light: Res<bevy::light::GlobalAmbientLight>,
    mut materials: ResMut<Assets<FogMaterial>>,
) {
    // Asset changes include same-frame additions, before AssetEvent::Added is published.
    // Do not scan settled materials just because the schedule runs again.
    if !lighting.is_changed() && !ambient_light.is_changed() && !materials.is_changed() {
        return;
    }
    let set =
        crate::lighting::LightSet::from_gparam(&lighting.gparam, lighting.light_set, lighting.hour);
    let rgb = |color: Color| {
        let c = color.to_linear();
        Vec3::new(c.red, c.green, c.blue)
    };
    let direct = rgb(set.sun_color) * set.sun_lux / std::f32::consts::PI;
    let ambient = rgb(set.ambient) * ambient_light.brightness;
    // CloudFog/GroundFog Fwd read FC_DirLightSpc_0 independently of
    // FC_DirLightCol_0. Diffuse W is not a substitute for SpecColor0 W.
    let values = LightingInputs {
        sun: set.sun_toward.extend(0.),
        direct: direct.extend(0.),
        specular: set.direct[2] / std::f32::consts::PI,
        ambient: ambient.extend(0.),
        hemi_axis: set.hemi_axis.extend(0.),
        hemi_delta: (set.hemi_delta * ambient_light.brightness).extend(0.),
    };
    // iter_mut marks every visited asset modified, even when no field changes.
    let changed: Vec<_> = materials
        .iter()
        .filter(|(_, material)| {
            let inputs = &material.inputs;
            inputs.control.w != crate::lighting::ENV_PER_UNIT || inputs.lighting != values
        })
        .map(|(id, _)| id)
        .collect();
    for id in changed {
        let Some(mut material) = materials.get_mut(id) else {
            continue;
        };
        material.inputs.lighting = values;
        material.inputs.control.w = crate::lighting::ENV_PER_UNIT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_materials_upload_only_for_new_materials_or_changed_lighting() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<FogMaterial>()
            .insert_resource(crate::lighting::MapLighting {
                gparam: Default::default(),
                hour: 12.,
                light_set: 0,
                applied: None,
                source: String::new(),
            })
            .init_resource::<bevy::light::GlobalAmbientLight>()
            .add_systems(Update, update_lighting);
        let material = || FogMaterial {
            inputs: inputs(&serde_json::json!({})),
            mask: default(),
            flow: default(),
            ground: default(),
            height: default(),
            irradiance: default(),
            voxels: None,
            diffuse: None,
            double_sided: false,
            diagnostic: false,
        };
        let modified = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Messages<AssetEvent<FogMaterial>>>()
                .drain()
                .filter(|e| matches!(e, AssetEvent::Modified { .. }))
                .count()
        };
        let first = app
            .world_mut()
            .resource_mut::<Assets<FogMaterial>>()
            .add(material());
        app.update();
        assert_eq!(modified(&mut app), 1);
        app.update();
        assert_eq!(modified(&mut app), 0);
        let second = app
            .world_mut()
            .resource_mut::<Assets<FogMaterial>>()
            .add(material());
        app.update();
        assert_eq!(modified(&mut app), 1);
        let before = app
            .world()
            .resource::<Assets<FogMaterial>>()
            .get(&first)
            .unwrap()
            .inputs
            .lighting
            .ambient;
        app.world_mut()
            .resource_mut::<bevy::light::GlobalAmbientLight>()
            .brightness *= 2.;
        app.update();
        assert_eq!(modified(&mut app), 2);
        for handle in [&first, &second] {
            assert_eq!(
                app.world()
                    .resource::<Assets<FogMaterial>>()
                    .get(handle)
                    .unwrap()
                    .inputs
                    .lighting
                    .ambient,
                before * 2.,
            );
        }
        app.update();
        assert_eq!(modified(&mut app), 0);
    }
}
