//! Layered-map renderer using exported sampler metadata and packed vertex weights.
use std::collections::HashMap;

use bevy::asset::{embedded_asset, embedded_path};
use bevy::gltf::GltfMaterialExtras;

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use serde_json::Value;

#[path = "camera_fade.rs"]
mod camera_fade;

pub(crate) type LayeredMaterial = ExtendedMaterial<StandardMaterial, MapLayers>;

#[path = "map_batches.rs"]
pub(crate) mod batches;

/// Immutable map descriptors can be repeated in different GLBs (and at each LOD).
#[derive(Default)]
struct MaterialInterner {
    buckets: HashMap<u64, Vec<Handle<LayeredMaterial>>>,
}

impl MaterialInterner {
    fn intern(
        &mut self,
        material: LayeredMaterial,
        assets: &mut Assets<LayeredMaterial>,
    ) -> Handle<LayeredMaterial> {
        use std::hash::{Hash, Hasher};
        // Debug includes every descriptor field, including non-reflected cull_mode.
        // Compute this only once per source, and retain neither strings nor duplicate assets.
        // A hash match is always checked against the full descriptor, so collisions are safe.
        let descriptor = format!("{material:?}");
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        descriptor.hash(&mut hash);
        let bucket = self.buckets.entry(hash.finish()).or_default();
        for handle in bucket.iter() {
            if assets
                .get(handle)
                .is_some_and(|other| format!("{other:?}") == descriptor)
            {
                return handle.clone();
            }
        }
        let handle = assets.add(material);
        bucket.push(handle.clone());
        handle
    }

    fn retain(&mut self, live: &std::collections::HashSet<AssetId<LayeredMaterial>>) {
        self.buckets.retain(|_, handles| {
            handles.retain(|h| live.contains(&h.id()));
            !handles.is_empty()
        });
    }
}

#[derive(Component)]
pub(super) struct MaterialOrigin(pub bevy::asset::AssetPath<'static>);

fn release_materials(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut settings: ResMut<LayerSettings>,
) {
    for event in events.read() {
        if let AssetEvent::Removed { id } = event {
            settings.materials.retain(|(source, _), _| source != id);
            let live = settings.materials.values().map(|h| h.id()).collect();
            settings.interned.retain(&live);
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
#[bind_group_data(MapPipelineKey)]
#[data(50, MapLayersUniform, binding_array(101))]
#[bindless(index_table(range(50..81), binding(100)))]
pub(crate) struct MapLayers {
    // xyz: enabled albedo layers; w: mask present.
    enabled: Vec4,
    albedo_enabled: Vec4,
    albedo_uv: Vec4,
    normal_uv: Vec4,
    normal_enabled: Vec4,
    camera_fade: Vec4,
    game: MapGame,
    #[texture(51)]
    #[sampler(52)]
    overlay: Option<Handle<Image>>,
    #[texture(53)]
    #[sampler(54)]
    snow: Option<Handle<Image>>,
    #[texture(55)]
    #[sampler(56)]
    c: Option<Handle<Image>>,
    #[texture(57)]
    #[sampler(58)]
    overlay_normal: Option<Handle<Image>>,
    #[texture(59)]
    #[sampler(60)]
    snow_normal: Option<Handle<Image>>,
    #[texture(61)]
    #[sampler(62)]
    c_normal: Option<Handle<Image>>,
    #[texture(63)]
    #[sampler(64)]
    mask: Option<Handle<Image>>,
    #[storage(65, read_only, binding_array(102))]
    irradiance: Handle<bevy::render::storage::ShaderBuffer>,
    #[texture(66, dimension = "cube_array")]
    #[sampler(67)]
    specular: Option<Handle<Image>>,
    #[texture(68, dimension = "cube_array")]
    #[sampler(69)]
    diffuse: Option<Handle<Image>>,
    #[texture(70, dimension = "3d")]
    #[sampler(71)]
    voxels: Option<Handle<Image>>,
    #[texture(72)]
    #[sampler(73)]
    base_normal: Option<Handle<Image>>,
    #[texture(74)]
    #[sampler(75)]
    detail: Option<Handle<Image>>,
    #[texture(76)]
    #[sampler(77)]
    mask_l: Option<Handle<Image>>,
    #[texture(78)]
    #[sampler(79)]
    wet: Option<Handle<Image>>,
    /// The shared [`MapLightBuffer`], so a LightSet change rewrites one buffer, not every material.
    #[storage(80, read_only, binding_array(103))]
    light: Handle<bevy::render::storage::ShaderBuffer>,
}

#[derive(ShaderType, Reflect, Debug, Clone, Default)]
struct MapGame {
    flags: UVec4,
    p: [Vec4; 16],
    tiling: [Vec4; 12],
}

pub(crate) const ATTR_MAP_BLEND2: bevy::mesh::MeshVertexAttribute =
    bevy::mesh::MeshVertexAttribute::new(
        "MapBlend2",
        0x5ec1_0a01,
        bevy::render::render_resource::VertexFormat::Float32x4,
    );

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MapPipelineKey {
    camera_fade: bool,
    game: bool,
    native_direct: bool,
}

impl From<&MapLayers> for MapPipelineKey {
    fn from(layers: &MapLayers) -> Self {
        static LEGACY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        Self {
            game: layers.game.flags.y != 0,
            native_direct: crate::lighting::native_lights(),
            camera_fade: layers.camera_fade.x > 0.
                || *LEGACY.get_or_init(|| std::env::args().any(|a| a == "--legacy-camera-fade")),
        }
    }
}

// Packed once per material; bindless slabs preserve Bevy's multi-draw batching.
#[derive(Clone, ShaderType)]
struct MapLayersUniform {
    enabled: Vec4,
    albedo_enabled: Vec4,
    albedo_uv: Vec4,
    normal_uv: Vec4,
    normal_enabled: Vec4,
    camera_fade: Vec4,
    game: MapGame,
}

/// Native deferred point/spot falloff for clustered map lights (`direct[12].y`).
/// `SEKIRO_MAP_POINT_LIGHTS_BEVY=1` restores Bevy's inverse-square response for A/B captures.
fn native_point_lights() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SEKIRO_MAP_POINT_LIGHTS_BEVY").is_none_or(|v| v != "1"))
}

/// Scale from the native light accumulation buffer to the sun/fill units (`direct[12].z`).
/// Traced: the light builders (`3892e0` spot, `388130` point) store `c^2 * power * blink *
/// distance fade * 2^(-AntiExposureRate * EV)` with `EV` the scene context float at `+0x458`.
/// Shipped builds keep that EV at 0 (`SC_PreExposure` is `2^EV` at `+0x45c`, also 1) and the
/// FXR 609 `adaptationExponent` is 0 in the effects checked, so the buffer is in the same units
/// as the directional light and the factor is exactly 1.
const NATIVE_LIGHT_BUFFER_SCALE: f32 = 1.0;

/// The remaining, untraced gap between the native curve and the references. The earlier by-eye
/// value 0.25 was fitted at the former 4500 lux per unit bridge; the bridge is now traced
/// (`SCENE_TO_BEVY = pi / exposure`, 0.697 of 4500), so the same total is kept as 0.25 * 4500 /
/// `SCENE_TO_BEVY`. It is not a measurement. The 2026-10-15 brightness audit found the same order
/// of excess (0.15 to 0.35 of the traced native value) on the sun, fill and local directional
/// slots, so this is probably one shared unexplained direct-light factor, see EXPOSURE.md.
const NATIVE_POINT_LIGHT_UNEXPLAINED_RESIDUAL: f32 = 0.25 * 4_500. / crate::lighting::SCENE_TO_BEVY;

fn native_point_light_scale() -> f32 {
    static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| {
        std::env::var("SEKIRO_MAP_POINT_LIGHT_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 0.)
            .unwrap_or(NATIVE_LIGHT_BUFFER_SCALE * NATIVE_POINT_LIGHT_UNEXPLAINED_RESIDUAL)
    })
}

impl From<&MapLayers> for MapLayersUniform {
    fn from(layers: &MapLayers) -> Self {
        Self {
            enabled: layers.enabled,
            albedo_enabled: layers.albedo_enabled,
            albedo_uv: layers.albedo_uv,
            normal_uv: layers.normal_uv,
            normal_enabled: layers.normal_enabled,
            camera_fade: layers.camera_fade,
            game: layers.game.clone(),
        }
    }
}

/// The LightSet values every map material reads. They live in one shared storage buffer: the
/// collision LightSet row changes at doorways (the Outskirts shack, #15), and writing them into
/// each material re-prepared every map material's bind group in one frame.
#[derive(Clone, Debug, Default, PartialEq, ShaderType)]
struct MapLight {
    direct: [Vec4; 13],
    hemi_axis: Vec4,
    hemi_delta: Vec4,
}

impl MapLight {
    fn encoded(&self) -> Vec<u8> {
        let mut encoded = bevy::render::render_resource::encase::StorageBuffer::new(Vec::new());
        encoded.write(self).expect("MapLight storage layout");
        encoded.into_inner()
    }

    /// `follows_scene` selects the collision row (characters, parts authoring their own set)
    /// instead of the map parts' row; see [`crate::lighting::part_light_set`].
    fn new(
        lighting: &crate::lighting::MapLighting,
        ambient: &bevy::light::GlobalAmbientLight,
        follows_scene: bool,
    ) -> Self {
        let set = crate::lighting::LightSet::from_gparam(
            &lighting.gparam,
            crate::lighting::part_light_set(lighting.light_set, follows_scene),
            lighting.hour,
        );
        let mut direct = set.direct;
        direct[12].y = f32::from(native_point_lights() && direct[12].x > 0.);
        direct[12].z = native_point_light_scale();
        let mut hemi_delta =
            (set.hemi_delta * crate::lighting::ENV_PER_UNIT).extend(crate::lighting::ENV_PER_UNIT);
        hemi_delta *= crate::lighting::scale("AMBIENT");
        if crate::lighting::ablate("noamb") {
            hemi_delta = Vec4::ZERO;
        } else if crate::lighting::ablate("nogi") {
            hemi_delta.w = 0.;
        }
        Self {
            direct,
            hemi_axis: set.hemi_axis.extend(f32::from(ambient.brightness > 0.)),
            hemi_delta,
        }
    }
}

/// The two shared light buffers, indexed by whether the surface follows the collision row:
/// map pieces and objects on the ordinary MSB light set, then characters and parts that
/// follow the collision-selected row.
#[derive(Resource)]
struct MapLightBuffer([Handle<bevy::render::storage::ShaderBuffer>; 2]);

impl MapLightBuffer {
    fn get(&self, follows_scene: bool) -> Handle<bevy::render::storage::ShaderBuffer> {
        self.0[follows_scene as usize].clone()
    }
}

impl FromWorld for MapLightBuffer {
    fn from_world(world: &mut World) -> Self {
        let mut buffers = world.resource_mut::<Assets<bevy::render::storage::ShaderBuffer>>();
        Self([(); 2].map(|()| {
            buffers.add(bevy::render::storage::ShaderBuffer::from(
                MapLight::default().encoded(),
            ))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_map_descriptors_share_but_surface_and_lighting_changes_do_not() {
        let mut assets = Assets::<LayeredMaterial>::default();
        let mut interned = MaterialInterner::default();
        let original = LayeredMaterial {
            base: StandardMaterial::default(),
            extension: MapLayers::default(),
        };
        let first = interned.intern(original.clone(), &mut assets);
        assert_eq!(first, interned.intern(original.clone(), &mut assets));
        for material in [
            LayeredMaterial {
                base: StandardMaterial {
                    cull_mode: None,
                    ..original.base.clone()
                },
                ..original.clone()
            },
            LayeredMaterial {
                extension: MapLayers {
                    camera_fade: Vec4::ONE,
                    ..original.extension.clone()
                },
                ..original.clone()
            },
            LayeredMaterial {
                base: StandardMaterial {
                    base_color_texture: Some(Handle::default()),
                    ..original.base.clone()
                },
                ..original.clone()
            },
        ] {
            assert_ne!(first, interned.intern(material, &mut assets));
        }
        assert_eq!(assets.len(), 4);
    }

    #[test]
    fn layered_families_use_native_f0_and_mountain_is_fully_rough() {
        let base = StandardMaterial::default();
        let surface = |kind: u64, variant: u64| {
            native_surface(
                &base,
                &serde_json::json!({"version": 2, "kind": kind, "variant": variant}),
            )
        };
        for kind in 1..=3 {
            let f0 = 0.16 * surface(kind, 0).reflectance.powi(2);
            assert!(
                (f0 - 0.173_205_f32.powi(2)).abs() < 1e-5,
                "kind {kind}: F0 {f0}"
            );
        }
        assert_eq!(surface(3, 0).perceptual_roughness, 1.);
        assert_eq!(
            surface(1, 0).perceptual_roughness,
            base.perceptual_roughness
        );
        assert_eq!(surface(1, 18).reflectance, base.reflectance);
        assert_eq!(surface(2, 18).reflectance, NATIVE_LAYERED_REFLECTANCE);
        assert_eq!(surface(0, 0).reflectance, base.reflectance);
        assert_eq!(
            native_surface(&base, &Value::Null).reflectance,
            base.reflectance
        );
    }

    #[test]
    fn map_extension_preserves_standard_material_bindless_batching() {
        use bevy::render::render_resource::AsBindGroup;
        assert!(<LayeredMaterial as AsBindGroup>::bindless_slot_count().is_some());
    }

    #[test]
    fn hemisphere_wraps_unlayered_map_surfaces_but_not_other_meshes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .init_asset::<LayeredMaterial>()
            .insert_resource(crate::lighting::MapLighting {
                gparam: Default::default(),
                hour: 12.,
                light_set: 0,
                applied: None,
                source: String::new(),
            })
            .insert_resource(bevy::light::GlobalAmbientLight::default())
            .insert_resource(LayerSettings {
                prefix: String::new(),
                primary_texture_prefix: false,
                enabled: true,
                debug: false,
                materials: HashMap::new(),
                interned: MaterialInterner::default(),
                camera_fade: camera_fade::CameraFade::load(std::path::Path::new(""), false),
            })
            .add_systems(Update, (release_materials, replace_materials).chain());
        let original = StandardMaterial {
            unlit: true,
            base_color: Color::srgb(0.2, 0.3, 0.4),
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(original.clone());
        let root = app.world_mut().spawn(super::super::DrawGroups([0; 8])).id();
        let map = app
            .world_mut()
            .spawn((MeshMaterial3d(handle.clone()), ChildOf(root)))
            .id();
        let other = app.world_mut().spawn(MeshMaterial3d(handle.clone())).id();
        app.update();
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(other)
                .unwrap()
                .0,
            handle
        );
        assert!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(map)
                .is_none()
        );
        let wrapped = app
            .world()
            .get::<MeshMaterial3d<LayeredMaterial>>(map)
            .unwrap();
        let material = app
            .world()
            .resource::<Assets<LayeredMaterial>>()
            .get(&wrapped.0)
            .unwrap();
        assert!(material.base.unlit);
        assert_eq!(material.base.base_color, original.base_color);
        assert_eq!(material.extension.albedo_enabled, Vec4::ZERO);
        assert!(!MapPipelineKey::from(&material.extension).camera_fade);
        let mut blind = material.extension.clone();
        blind.camera_fade = Vec4::new(1., 1., 1. / 0.3, 0.);
        assert!(MapPipelineKey::from(&blind).camera_fade);
        assert_eq!(app.world().resource::<LayerSettings>().materials.len(), 1);
        // Removing the source material drops its wrapper.
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .remove(handle.id());
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(app.world().resource::<LayerSettings>().materials.len(), 0);
    }

    #[test]
    fn light_set_change_writes_the_shared_buffer_not_the_materials() {
        use bevy::render::storage::ShaderBuffer;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<LayeredMaterial>()
            .init_asset::<ShaderBuffer>()
            .init_resource::<MapLightBuffer>()
            .insert_resource(crate::lighting::MapLighting {
                gparam: Default::default(),
                hour: 12.,
                light_set: 110,
                applied: None,
                source: String::new(),
            })
            .insert_resource(bevy::light::GlobalAmbientLight::default())
            .add_systems(Update, update_hemisphere);
        let buffers =
            [false, true].map(|scene| app.world().resource::<MapLightBuffer>().get(scene));
        app.world_mut()
            .resource_mut::<Assets<LayeredMaterial>>()
            .add(ExtendedMaterial {
                base: StandardMaterial::default(),
                extension: MapLayers::default(),
            });
        app.update();
        let expected = |app: &App| {
            let lighting = app.world().resource::<crate::lighting::MapLighting>();
            let ambient = app.world().resource::<bevy::light::GlobalAmbientLight>();
            [false, true].map(|scene| MapLight::new(lighting, ambient, scene).encoded())
        };
        let written = |app: &App| {
            buffers.clone().map(|buffer| {
                app.world()
                    .resource::<Assets<ShaderBuffer>>()
                    .get(&buffer)
                    .unwrap()
                    .cast_slice::<u8>()
                    .unwrap()
                    .to_vec()
            })
        };
        assert_eq!(written(&app), expected(&app));
        let modified = |app: &mut App| {
            let mut count = 0;
            app.world_mut()
                .resource_mut::<Messages<AssetEvent<LayeredMaterial>>>()
                .drain()
                .for_each(|e| count += usize::from(matches!(e, AssetEvent::Modified { .. })));
            count
        };
        modified(&mut app);
        // The shack doorway: the collision row flips from 110 inside to 100 outside.
        app.world_mut()
            .resource_mut::<crate::lighting::MapLighting>()
            .light_set = 100;
        app.update();
        assert_eq!(written(&app), expected(&app));
        assert_eq!(
            modified(&mut app),
            0,
            "a LightSet change touched map materials"
        );
    }
}

impl MaterialExtension for MapLayers {
    fn specialize(
        pipeline: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        use bevy::shader::ShaderDefVal;
        let prepass = descriptor.vertex.shader_defs.iter().any(|d| {
            matches!(d,
            ShaderDefVal::Bool(n, true) if n == "PREPASS_PIPELINE")
        });
        if !key.bind_group_data.game && !prepass {
            descriptor.vertex.shader = pipeline.mesh_pipeline.shader.clone();
        }
        if key.bind_group_data.native_direct
            && !prepass
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            fragment.shader_defs.push("NATIVE_DIRECT".into());
        }
        if key.bind_group_data.game {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("GAME_MATERIAL".into());
            }
            if !prepass {
                descriptor.vertex.shader_defs.push("GAME_LAYERS".into());
                if let Some(fragment) = descriptor.fragment.as_mut() {
                    fragment.shader_defs.push("GAME_LAYERS".into());
                }
                let mut attrs = vec![
                    Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                    Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
                ];
                for (a, at) in [
                    (Mesh::ATTRIBUTE_UV_0, 2),
                    (Mesh::ATTRIBUTE_UV_1, 3),
                    (Mesh::ATTRIBUTE_TANGENT, 4),
                    (Mesh::ATTRIBUTE_COLOR, 5),
                ] {
                    if layout.0.contains(a) {
                        attrs.push(a.at_shader_location(at));
                    }
                }
                attrs.push(ATTR_MAP_BLEND2.at_shader_location(8));
                descriptor.vertex.buffers = vec![layout.0.get_layout(&attrs)?];
            }
        }
        if key.bind_group_data.camera_fade
            && (!prepass
                || key.mesh_key.intersects(
                    bevy::pbr::MeshPipelineKey::MOTION_VECTOR_PREPASS
                        | bevy::pbr::MeshPipelineKey::NORMAL_PREPASS,
                ))
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            fragment.shader_defs.push("CAMERA_FADE".into());
        }
        Ok(())
    }

    fn vertex_shader() -> ShaderRef {
        shader_path("map_material.wesl")
    }

    fn fragment_shader() -> ShaderRef {
        shader_path("map_material.wesl")
    }

    fn prepass_fragment_shader() -> ShaderRef {
        shader_path("map_prepass.wesl")
    }
}

fn shader_path(name: &str) -> ShaderRef {
    let path = embedded_path!("map_material.wesl").with_file_name(name);
    ShaderRef::Path(
        bevy::asset::AssetPath::parse(&format!("embedded://{}", path.to_string_lossy()))
            .into_owned(),
    )
}

#[derive(Resource)]
struct LayerSettings {
    prefix: String,
    primary_texture_prefix: bool,
    enabled: bool,
    debug: bool,
    /// Wrappers by source material and whether they read the scene light buffer.
    materials: HashMap<(AssetId<StandardMaterial>, bool), Handle<LayeredMaterial>>,
    interned: MaterialInterner,
    camera_fade: camera_fade::CameraFade,
}

#[derive(bevy::ecs::system::SystemParam)]
struct MapEnvironment<'w, 's> {
    parents: Query<'w, 's, &'static ChildOf>,
    roots: Query<'w, 's, (), With<super::DrawGroups>>,
    scene_parts: Query<'w, 's, (), With<super::FollowsSceneLight>>,
    no_shadow: Query<'w, 's, (), With<super::world::AuthoredNoShadow>>,
    irradiance: Option<Res<'w, crate::lighting::MapIrradiance>>,
    light: Option<Res<'w, MapLightBuffer>>,
}

pub(super) fn install(app: &mut App, prefix: &str, dir: &std::path::Path, args: &[String]) {
    crate::pipeline_warmup::install::<LayeredMaterial>(app, args);
    batches::install(app);
    crate::material_residency::install(app);
    crate::mesh_residency::install(app);
    crate::texture_residency::install(app);
    embedded_asset!(app, "map_material.wesl");
    embedded_asset!(app, "map_prepass.wesl");
    bevy::shader::load_shader_library!(app, "map_bindings.wesl");
    bevy::shader::load_shader_library!(app, "map_coverage.wesl");
    app.add_plugins((
        MaterialPlugin::<LayeredMaterial>::default(),
        crate::split_sun_cascades::SplitSunCascadesPlugin,
    ))
    .init_resource::<MapLightBuffer>()
    .insert_resource(LayerSettings {
        prefix: prefix.to_owned(),
        primary_texture_prefix: args.iter().any(|s| s == "--test-primary-material-prefix"),
        enabled: !args.iter().any(|s| s == "--no-layers"),
        debug: args.iter().any(|s| s == "--show-blend"),
        materials: HashMap::new(),
        interned: MaterialInterner::default(),
        camera_fade: camera_fade::CameraFade::load(
            dir,
            !args.iter().any(|s| s == "--no-camera-fade"),
        ),
    })
    .add_systems(
        Update,
        (release_materials, replace_materials, update_hemisphere)
            .chain()
            .after(crate::lighting::apply_lighting)
            .after(crate::lighting::apply_probes),
    );
}

fn update_hemisphere(
    lighting: Res<crate::lighting::MapLighting>,
    ambient: Res<bevy::light::GlobalAmbientLight>,
    light: Res<MapLightBuffer>,
    mut buffers: ResMut<Assets<bevy::render::storage::ShaderBuffer>>,
    mut last: Local<Option<(i32, f32, bool, String)>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("update_hemisphere");
    let key = (
        lighting.light_set,
        lighting.hour,
        ambient.brightness > 0.,
        lighting.source.clone(),
    );
    if last.as_ref() == Some(&key) {
        return;
    }
    *last = Some(key);
    // Same size every write, so Bevy rewrites the GPU buffer in place and no bind group changes.
    for follows_scene in [false, true] {
        if let Some(mut buffer) = buffers.get_mut(&light.get(follows_scene)) {
            buffer.clear();
            buffer.extend_from_slice(&MapLight::new(&lighting, &ambient, follows_scene).encoded());
        }
    }
}

/// Bevy reflectance giving the native layered-map F0: every M[Multiple], M[MultipleGround] and
/// M[LandScape]_Mountain Gbuf shader writes the constant 0.173205, and F0 = 0.173205^2 = 0.03,
/// so 0.16 * r^2 = 0.03 (#7).
const NATIVE_LAYERED_REFLECTANCE: f32 = 0.433_012_7;

/// The base material for a layered map family, with the native specular constants applied.
fn native_surface(base: &StandardMaterial, data: &Value) -> StandardMaterial {
    let mut base = base.clone();
    if data["version"].as_u64() != Some(2) {
        return base;
    }
    let kind = data["kind"].as_u64().unwrap_or(0);
    // Slot 17 (only Multiple variant 18) writes a textured F0 instead of the constant; the slot
    // is not sampled yet, so it keeps Bevy's default for now (#43).
    let variant = data["variant"].as_u64().unwrap_or(0) as usize;
    let textured_f0 = sekiro_formats::map_layers::slots(kind as u8, variant).contains(&17);
    if (1..=3).contains(&kind) && !textured_f0 {
        base.reflectance = NATIVE_LAYERED_REFLECTANCE;
    }
    // Mountain writes gloss 0 and the shader's Mountain path leaves roughness to the base.
    if kind == 3 {
        base.perceptual_roughness = 1.;
    }
    base
}

#[allow(clippy::type_complexity)]
fn replace_materials(
    mut commands: Commands,
    assets: Res<AssetServer>,
    materials: Res<Assets<StandardMaterial>>,
    mut layered: ResMut<Assets<LayeredMaterial>>,
    meshes: Query<
        (
            Entity,
            &MeshMaterial3d<StandardMaterial>,
            Option<&GltfMaterialExtras>,
            Option<&bevy::gltf::GltfMaterialName>,
        ),
    >,
    mut settings: ResMut<LayerSettings>,
    environment: MapEnvironment,
) {
    let _timer = crate::perf_log::SystemTimer::start("replace_materials");
    let MapEnvironment {
        parents,
        roots,
        scene_parts,
        no_shadow,
        irradiance,
        light,
    } = environment;
    for (entity, original, extras, name) in &meshes {
        // Map descendants share spatial lighting; unrelated FX stay untouched.
        let mut ancestor = entity;
        let mut map_surface = roots.contains(ancestor);
        while !map_surface {
            let Ok(parent) = parents.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
            map_surface = roots.contains(ancestor);
        }
        if !map_surface {
            continue;
        }
        if no_shadow.contains(ancestor) {
            commands
                .entity(entity)
                .insert((super::world::AuthoredNoShadow, bevy::light::NotShadowCaster));
        }
        // All map surfaces need the hemisphere, even without texture layers.
        let Some(base) = materials.get(&original.0) else {
            continue;
        };
        // Parts authoring their own set follow the collision row (#30).
        let follows_scene = scene_parts.contains(ancestor);
        let key = (original.0.id(), follows_scene);
        let handle = if let Some(handle) = settings.materials.get(&key) {
            handle.clone()
        } else {
            let extras = extras
                .and_then(|e| serde_json::from_str::<Value>(&e.value).ok())
                .unwrap_or(Value::Null);
            let data = &extras["sekiro_layers"];
            let camera_fade = settings
                .camera_fade
                .material(name.map_or("", |name| name.0.as_str()));
            let load = |key: &str, srgb: bool| -> Option<Handle<Image>> {
                if !settings.enabled {
                    return None;
                }
                let uri = data[key]["uri"].as_str()?;
                // Exported paths are relative to the owning GLB, including neighbours.
                if !uri.starts_with("tex/") || uri.contains("..") {
                    return None;
                }
                Some(
                    assets
                        .load_builder()
                        .override_unapproved()
                        .with_settings(move |s: &mut ImageLoaderSettings| {
                            s.is_srgb = srgb;
                            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                                address_mode_u: ImageAddressMode::Repeat,
                                address_mode_v: ImageAddressMode::Repeat,
                                ..super::texture_sampler()
                            });
                        })
                        .load(
                            (!settings.primary_texture_prefix)
                                .then(|| {
                                    super::world::material_texture_path(&assets, &original.0, uri)
                                })
                                .flatten()
                                .unwrap_or_else(|| format!("{}models/{uri}", settings.prefix)),
                        ),
                )
            };
            let uv = |key: &str| data[key]["uv"].as_u64().unwrap_or(0) as f32;
            let present = |key: &str| f32::from(data[key]["uri"].is_string() && settings.enabled);
            let mut game = MapGame::default();
            if settings.enabled && data["version"].as_u64() == Some(2) {
                game.flags.y = data["kind"].as_u64().unwrap_or(0) as u32;
                game.flags.z = u32::from(settings.debug);
                // Native cbTextureTilingScale applies each FLVER texture's scale separately.
                for (i, key) in [
                    "base_uv",
                    "base_normal",
                    "overlay",
                    "overlay_normal",
                    "snow",
                    "snow_normal",
                    "c",
                    "c_normal",
                    "mask",
                    "detail",
                    "mask_l",
                    "wet",
                ]
                .into_iter()
                .enumerate()
                {
                    game.tiling[i] = Vec4::new(
                        data[key]["scale"][0].as_f64().unwrap_or(1.) as f32,
                        data[key]["scale"][1].as_f64().unwrap_or(1.) as f32,
                        0.,
                        0.,
                    );
                }
                for (key, bit) in [
                    ("base_normal", 1),
                    ("overlay", 2),
                    ("overlay_normal", 4),
                    ("snow", 8),
                    ("snow_normal", 16),
                    ("c", 32),
                    ("c_normal", 64),
                    ("mask", 128),
                    ("detail", 256),
                    ("mask_l", 512),
                    ("wet", 1024),
                ] {
                    if present(key) > 0. {
                        game.flags.x |= bit;
                    }
                }
                if base.base_color_texture.is_some() {
                    game.flags.x |= 2048;
                }
                for (i, p) in game.p.iter_mut().enumerate() {
                    *p = Vec4::from_array(std::array::from_fn(|c| {
                        data["params"][i][c].as_f64().unwrap_or(0.) as f32
                    }));
                }
            }
            let extension = MapLayers {
                game,
                enabled: Vec4::new(
                    present("overlay").max(present("overlay_normal")),
                    present("snow").max(present("snow_normal")),
                    present("c").max(present("c_normal")),
                    present("mask"),
                ),
                albedo_enabled: Vec4::new(
                    present("overlay"),
                    present("snow"),
                    present("c"),
                    f32::from(data.is_object()),
                ),
                albedo_uv: Vec4::new(uv("overlay"), uv("snow"), uv("c"), uv("mask")),
                normal_uv: Vec4::new(uv("overlay_normal"), uv("snow_normal"), uv("c_normal"), 0.),
                normal_enabled: Vec4::new(
                    present("overlay_normal"),
                    present("snow_normal"),
                    present("c_normal"),
                    f32::from(settings.debug),
                ),
                camera_fade,
                overlay: load("overlay", true),
                snow: load("snow", true),
                c: load("c", true),
                overlay_normal: load("overlay_normal", false),
                snow_normal: load("snow_normal", false),
                c_normal: load("c_normal", false),
                mask: load("mask", false),
                base_normal: load("base_normal", false),
                detail: load("detail", false),
                mask_l: load("mask_l", false),
                wet: load("wet", false),
                irradiance: irradiance
                    .as_ref()
                    .map(|i| i.buffer.clone())
                    .unwrap_or_default(),
                specular: irradiance.as_ref().map(|i| i.specular.clone()),
                diffuse: irradiance.as_ref().map(|i| i.diffuse.clone()),
                voxels: irradiance.as_ref().map(|i| i.voxels.clone()),
                light: light
                    .as_ref()
                    .map(|l| l.get(follows_scene))
                    .unwrap_or_default(),
            };
            let material = ExtendedMaterial {
                base: native_surface(base, data),
                extension,
            };
            let handle = settings.interned.intern(material, &mut layered);
            settings.materials.insert(key, handle.clone());
            handle
        };
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
        if let Some(path) = assets.get_path(original.0.id()) {
            commands
                .entity(entity)
                .insert(MaterialOrigin(path.into_owned()));
        }
    }
}
