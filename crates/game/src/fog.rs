//! Shader-derived global fog, integrated in HDR before bloom and tone mapping.
use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::camera::{Exposure, visibility::NoFrustumCulling};
use bevy::core_pipeline::{
    FullscreenShader,
    schedule::{Core3d, Core3dSystems},
};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::renderer::RenderContext;
use bevy::render::view::ViewDepthStencilTexture;
use bevy::render::{
    RenderApp, RenderStartup,
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_resource::{binding_types::*, *},
    renderer::ViewQuery,
    view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};
use sekiro_formats::gparam::Gparam;

#[derive(Resource, Default)]
pub(crate) struct FogSelection(pub i32);

#[derive(Component, Clone, ExtractComponent, ShaderType, Debug, PartialEq)]
#[extract_app(bevy::render::RenderApp)]
pub struct GameFog {
    // fixed density, height coefficient, height density, affine height offset.
    density: Vec4,
    // near depth, far depth, sky start, directional start.
    depths: Vec4,
    sky: Vec4,
    direct: Vec4,
    opposite: Vec4,
    direction: Vec4,
    // RGB extinction; w is the signed world-height axis.
    extinction: Vec4,
    // NearOffset, slice count, sample offset, analytical continuation enabled.
    slices: Vec4,
    // Ring count, polygon sector angle, first vertex azimuth, unused.
    background_control: Vec4,
    // Native camera-centred sky outline: polygon apothem and world Y.
    background: [Vec4; 16],
    // Native scene radiance to the camera's HDR colour space.
    radiance_scale: Vec4,
}

/// Only retire depth sampling when this effect introduced the usage bit.
#[derive(Component)]
struct FogDepthSampling;

fn release_depth_sampling(
    event: On<Remove<GameFog>>,
    mut commands: Commands,
    mut cameras: Query<&mut Camera3d, With<FogDepthSampling>>,
) {
    if let Ok(mut camera) = cameras.get_mut(event.entity) {
        let mut depth = TextureUsages::from(camera.depth_texture_usages);
        depth.remove(TextureUsages::TEXTURE_BINDING);
        camera.depth_texture_usages = depth.into();
        commands.entity(event.entity).remove::<FogDepthSampling>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sekiro_formats::gparam::{Entry, Group, Param, Value};

    fn fixture(params: Vec<(&str, i32, f32, Value)>) -> Gparam {
        let mut group = Group {
            name: "VolumetricFog ParamEditor".into(),
            display_name: String::new(),
            params: Vec::new(),
        };
        for (name, id, time, value) in params {
            if let Some(param) = group.params.iter_mut().find(|p| p.name == name) {
                param.entries.push(Entry { id, time, value });
            } else {
                group.params.push(Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry { id, time, value }],
                });
            }
        }
        Gparam {
            groups: vec![group],
        }
    }

    #[test]
    fn global_enable_is_independent_of_analytical_continuation_and_discrete() {
        for (enable, analytical, hour, expected) in [
            (None, false, 0., Some(0.)),
            (Some(false), true, 0., None),
            (Some(false), false, 5., None),
            (Some(false), false, 8., Some(0.)),
            (Some(true), false, 0., Some(0.)),
            (Some(true), true, 0., Some(1.)),
        ] {
            let mut values = vec![(
                "VolumetricFog EnableAnalyticalFog",
                0,
                0.,
                Value::Bool(analytical),
            )];
            if let Some(enable) = enable {
                values.extend([
                    ("VolumetricFog Enable", 0, 0., Value::Bool(enable)),
                    ("VolumetricFog Enable", 0, 12., Value::Bool(true)),
                ]);
            }
            let fog = GameFog::from_gparam(&fixture(values), 100, hour, Vec3::Y);
            assert_eq!(fog.map(|fog| fog.slices.w), expected);
        }
    }

    #[test]
    fn omitted_analytical_switch_preserves_the_native_finite_volume() {
        let g = fixture(vec![(
            "VolumetricFog Global FixedDensity",
            0,
            0.,
            Value::Float(0.003),
        )]);
        let fog = GameFog::from_gparam(&g, 0, 22., Vec3::Y).unwrap();
        assert_eq!(fog.slices.w, 0.);
        assert_eq!(fog.density.x, 0.003);
        assert_eq!(fog.slices, Vec4::new(1., 32., 0., 0.));
        for map in ["m11_00", "m11_01"] {
            let path =
                sekiro_sim::cache::root().join(format!("raw/param/drawparam/{map}_0000.gparam"));
            if !path.is_file() {
                continue;
            }
            let g = sekiro_formats::gparam::parse(&std::fs::read(path).unwrap()).unwrap();
            assert!(fog_param_exact(&g, "EnableAnalyticalFog").is_none());
            let fog = GameFog::from_gparam(&g, 0, 22., Vec3::Y).unwrap();
            assert_eq!(fog.slices.w, 0.);
        }
    }

    fn light_fixture(angles: &[(i32, [f32; 2])]) -> Gparam {
        Gparam {
            groups: vec![Group {
                name: "LightSet ParamEditor".into(),
                display_name: String::new(),
                params: vec![Param {
                    name: "Directional Light Angle0".into(),
                    display_name: String::new(),
                    entries: angles
                        .iter()
                        .map(|&(id, angle)| Entry {
                            id,
                            time: 0.,
                            value: Value::Vec2(angle),
                        })
                        .collect(),
                }],
            }],
        }
    }

    #[test]
    fn fog_resolves_integer_bank_and_light_without_aliasing_the_collision_sun() {
        let mut g = fixture(vec![
            (
                "VolumetricFog DirLight LightBankId",
                0,
                0.,
                Value::Int(111_010_000),
            ),
            (
                "VolumetricFog DirLight LightBankId",
                0,
                12.,
                Value::Int(111_020_000),
            ),
        ]);
        g.groups
            .extend(light_fixture(&[(0, [-0.4, 0.2]), (100, [-1., 2.])]).groups);
        let mut banks = FogBanks::default();
        banks.0.insert(
            "m11_02_0000".into(),
            Some(light_fixture(&[(0, [-0.8, -0.2]), (42, [-0.7, 1.])])),
        );
        assert_eq!(
            fog_integer(&g, "DirLight LightBankId", 100, 5.),
            Some(111_010_000)
        );
        let local = banks.direction(&g, "m11_01_0750", 100, 0.);
        assert_eq!(local, super::super::toward(&[-0.4, 0.2]));
        assert_ne!(
            local,
            super::super::LightSet::from_gparam(&g, 100, 5.).sun_toward
        );
        let external = banks.direction(&g, "m11_01_0750", 100, 12.);
        assert_eq!(external, super::super::toward(&[-0.8, -0.2]));
        let midpoint = banks.direction(&g, "m11_01_0750", 100, 6.);
        assert!(midpoint.distance((local + external).normalize()) < 0.000001);
        g.groups[0].params.push(Param {
            name: "VolumetricFog DirLight LightId".into(),
            display_name: String::new(),
            entries: vec![Entry {
                id: 0,
                time: 0.,
                value: Value::Int(42),
            }],
        });
        assert_eq!(
            banks.direction(&g, "m11_01_0750", 100, 12.),
            super::super::toward(&[-0.7, 1.])
        );
        let exact = fixture(vec![(
            "DirLight LightBankId",
            0,
            0.,
            Value::Int(111_010_001),
        )]);
        assert_eq!(
            fog_integer(&exact, "DirLight LightBankId", 0, 0.),
            Some(111_010_001)
        );
        assert!(
            FogBanks::light(&light_fixture(&[(43, [0.4, 0.2])]), 43, 0.)
                .unwrap()
                .y
                < 0.
        );
    }

    #[test]
    fn sky_swaps_update_fog_in_the_same_frame_without_reinserting_diagnostic_fog() {
        #[derive(Component)]
        struct TestDome;
        #[derive(Component)]
        struct Diagnostic;
        #[derive(Resource)]
        struct NextDome(Handle<Mesh>);
        fn swap(next: Res<NextDome>, mut domes: Query<&mut Mesh3d, With<TestDome>>) {
            for mut mesh in &mut domes {
                mesh.0 = next.0.clone();
            }
        }
        fn diagnostics(mut commands: Commands, cameras: Query<Entity, With<Diagnostic>>) {
            for entity in &cameras {
                commands.entity(entity).remove::<GameFog>();
            }
        }
        let mesh = |height: f32| {
            let positions: Vec<_> = [-1000., height]
                .into_iter()
                .flat_map(|y| {
                    (0..16).map(move |i| {
                        let angle = i as f32 * std::f32::consts::TAU / 16.;
                        [1000. * angle.cos(), y, 1000. * angle.sin()]
                    })
                })
                .collect();
            let mut mesh = Mesh::new(
                bevy::mesh::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0., 0.]; positions.len()]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh
        };
        let mut app = App::new();
        app.insert_resource(super::super::MapLighting {
            gparam: Gparam::default(),
            hour: 12.,
            light_set: 0,
            applied: None,
            source: String::new(),
        })
        .init_resource::<FogSelection>()
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, (configure, swap, diagnostics).chain())
        .add_systems(PostUpdate, update_background);
        let first = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(mesh(1000.));
        let second = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(mesh(1300.));
        app.insert_resource(NextDome(first.clone()));
        app.world_mut().spawn((
            TestDome,
            Mesh3d(first),
            Visibility::Inherited,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        let diagnostic = app
            .world_mut()
            .spawn((Camera3d::default(), Diagnostic))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<GameFog>(camera).unwrap().background[1].y,
            1000.
        );
        app.insert_resource(NextDome(second));
        app.update();
        assert_eq!(
            app.world().get::<GameFog>(camera).unwrap().background[1].y,
            1300.
        );
        assert!(app.world().get::<GameFog>(diagnostic).is_none());
        let scale = app.world().get::<GameFog>(camera).unwrap().radiance_scale.x;
        app.world_mut().entity_mut(camera).insert(Exposure {
            ev100: Exposure::default().ev100 + 1.,
        });
        app.update();
        assert!(
            (app.world().get::<GameFog>(camera).unwrap().radiance_scale.x - scale * 0.5).abs()
                < 0.00001
        );
    }

    #[test]
    fn installed_fog_banks_and_enabled_authored_densities() {
        let cache = sekiro_sim::cache::root();
        if !cache
            .join("raw/param/drawparam/m15_00_0000.gparam")
            .is_file()
        {
            return;
        }
        let mut banks = FogBanks::default();
        for map in [
            "m10_00", "m11_00", "m11_01", "m11_02", "m13_00", "m15_00", "m17_00", "m20_00",
            "m25_00",
        ] {
            let selection =
                crate::lighting_selection::select(&cache, &format!("{map}_00_00"), None, None)
                    .unwrap();
            let bytes = std::fs::read(
                cache.join(format!("raw/param/drawparam/{}.gparam", selection.source)),
            )
            .unwrap();
            let g = sekiro_formats::gparam::parse(&bytes).unwrap();
            let direction = banks.direction(&g, &selection.source, 100, selection.hour);
            let fog = GameFog::from_gparam(&g, 100, selection.hour, direction).unwrap();
            println!(
                "FOG_NATIVE {map}: source={}, hour={}, bank={:?}, density={:?}, depths={:?}, sky={:?}, direct={:?}, opposite={:?}, direction={:?}, analytical={}",
                selection.source,
                selection.hour,
                fog_integer(&g, "DirLight LightBankId", 100, selection.hour),
                fog.density,
                fog.depths,
                fog.sky,
                fog.direct,
                fog.opposite,
                fog.direction,
                fog.slices.w
            );
            if map == "m15_00" {
                assert!((fog.density.x - 0.007).abs() < 0.000001);
                assert_eq!(fog.slices.w, 0.);
            }
            if map == "m10_00" {
                assert!((fog.density.x - 0.0032).abs() < 0.000001);
                assert_eq!(fog.slices.w, 0.);
            }
        }
    }

    #[test]
    fn native_sky_profile_intersections_match_the_authored_triangles() {
        let dir = sekiro_sim::cache::root().join("maps/m89_00_00_99");
        let Ok(bytes) = std::fs::read(dir.join("sky.json")) else {
            return;
        };
        let library: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for (name, row) in library["models"].as_object().unwrap() {
            if !row["shader"]
                .as_str()
                .unwrap_or_default()
                .contains("SkyDome")
            {
                continue;
            }
            let bytes = std::fs::read(dir.join(format!("models/{name}.glb"))).unwrap();
            let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + length]).unwrap();
            let binary = &bytes[28 + length..];
            let primitive = json["meshes"][0]["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["material"] == row["material_index"])
                .unwrap();
            let accessor = |index: usize| {
                let accessor = &json["accessors"][index];
                let view = &json["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
                let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
                    + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
                (accessor, view, offset)
            };
            let (a, v, offset) =
                accessor(primitive["attributes"]["POSITION"].as_u64().unwrap() as usize);
            let positions: Vec<[f32; 3]> = (0..a["count"].as_u64().unwrap() as usize)
                .map(|i| {
                    std::array::from_fn(|axis| {
                        let offset =
                            offset + i * v["byteStride"].as_u64().unwrap_or(12) as usize + axis * 4;
                        f32::from_le_bytes(binary[offset..offset + 4].try_into().unwrap())
                    })
                })
                .collect();
            let background = Background::from_positions(&positions)
                .unwrap_or_else(|| panic!("unsupported native sky outline {name}"));
            let (a, _, offset) = accessor(primitive["indices"].as_u64().unwrap() as usize);
            let size = if a["componentType"] == 5123 { 2 } else { 4 };
            let indices: Vec<usize> = (0..a["count"].as_u64().unwrap() as usize)
                .map(|i| {
                    let offset = offset + i * size;
                    if size == 2 {
                        u16::from_le_bytes(binary[offset..offset + 2].try_into().unwrap()) as usize
                    } else {
                        u32::from_le_bytes(binary[offset..offset + 4].try_into().unwrap()) as usize
                    }
                })
                .collect();
            for ray in [
                Vec3::X,
                Vec3::Z,
                Vec3::Y,
                Vec3::new(-0.4, 0.7, 1.).normalize(),
                Vec3::new(0.8, -0.1, -0.2).normalize(),
            ] {
                let mesh_distance = indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|triangle| {
                        let a = Vec3::from_array(positions[triangle[0]]);
                        let b = Vec3::from_array(positions[triangle[1]]);
                        let c = Vec3::from_array(positions[triangle[2]]);
                        let edge = b - a;
                        let edge2 = c - a;
                        let cross = ray.cross(edge2);
                        let determinant = edge.dot(cross);
                        if determinant.abs() < 0.000001 {
                            return None;
                        }
                        let u = (-a).dot(cross) / determinant;
                        let q = (-a).cross(edge);
                        let v = ray.dot(q) / determinant;
                        let t = edge2.dot(q) / determinant;
                        (u >= -0.00001 && v >= -0.00001 && u + v <= 1.00001 && t > 0.).then_some(t)
                    })
                    .min_by(f32::total_cmp)
                    .unwrap();
                let angle = ray.z.atan2(ray.x);
                let step = background.control.y;
                let normal = background.control.z
                    + (((angle - background.control.z) / step).floor() + 0.5) * step;
                let radial = ray.xz().length() * (angle - normal).cos();
                let profile_distance = background.rings[..background.control.x as usize]
                    .windows(2)
                    .filter_map(|ring| {
                        let a = ring[0].xy();
                        let delta = ring[1].xy() - a;
                        let denominator = radial * delta.y - ray.y * delta.x;
                        if denominator.abs() < 0.000001 {
                            return None;
                        }
                        let t = (a.x * delta.y - a.y * delta.x) / denominator;
                        let u =
                            (Vec2::new(radial, ray.y) * t - a).dot(delta) / delta.length_squared();
                        (t > 0. && (-0.00001..=1.00001).contains(&u)).then_some(t)
                    })
                    .min_by(f32::total_cmp)
                    .unwrap();
                assert!(
                    (mesh_distance - profile_distance).abs() < 0.05 + 0.00002 * mesh_distance,
                    "{name} {ray:?}: mesh={mesh_distance}, profile={profile_distance}"
                );
                println!(
                    "FOG_BACKGROUND {name} {ray:?}: mesh={mesh_distance:.4}, profile={profile_distance:.4}"
                );
            }
        }
    }

    #[test]
    fn signed_height_and_scaled_depths_use_executable_units() {
        let g = Gparam {
            groups: vec![Group {
                name: "VolumetricFog".into(),
                display_name: String::new(),
                params: [
                    ("GameScale", 2.),
                    ("NearDepth", 3.),
                    ("FarDepth", 1.),
                    ("NearOffset", 4.),
                    ("FixedDensity", 0.4),
                    ("HeightFogDistribution", -0.8),
                    ("HeightFogDensity Scale", 0.6),
                    ("HeightFogOffset", 10.),
                    ("SkyLight StartDist", 5.),
                    ("DirLight StartDist", 6.),
                    ("DirLight AngleClip", 0.25),
                ]
                .into_iter()
                .map(|(name, value)| Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry {
                        id: 0,
                        time: 12.,
                        value: Value::Float(value),
                    }],
                })
                .collect(),
            }],
        };
        let f = GameFog::from_gparam(&g, 0, 12., Vec3::Y).unwrap();
        assert_eq!(f.depths, Vec4::new(6., 6.2, 10., 12.));
        assert_eq!(f.density, Vec4::new(0.2, 0.4, 0.15, 10.));
        assert_eq!(f.extinction.w, -1.);
        assert_eq!(f.direction.w, 4.);
        assert_eq!(f.slices.x, 8.);
        // Raw world Y=14 gives flipped height -4, not a GS-scaled offset.
        assert_eq!(f.extinction.w * 14. + f.density.w, -4.);
    }

    #[test]
    fn configuring_new_cameras_preserves_msaa_and_enables_depth_sampling() {
        let mut app = App::new();
        app.insert_resource(super::super::MapLighting {
            gparam: Gparam::default(),
            hour: 12.,
            light_set: 0,
            applied: None,
            source: String::new(),
        })
        .init_resource::<FogSelection>()
        .add_systems(Update, configure)
        .add_observer(release_depth_sampling);
        let first = app
            .world_mut()
            .spawn((Camera3d::default(), Msaa::Sample4))
            .id();
        app.update();
        let second = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        for entity in [first, second] {
            assert!(app.world().get::<GameFog>(entity).is_some());
            let usages: TextureUsages = app
                .world()
                .get::<Camera3d>(entity)
                .unwrap()
                .depth_texture_usages
                .into();
            assert!(usages.contains(TextureUsages::TEXTURE_BINDING));
        }
        assert_eq!(*app.world().get::<Msaa>(first).unwrap(), Msaa::Sample4);
        app.world_mut().entity_mut(first).remove::<GameFog>();
        app.update();
        assert!(
            !TextureUsages::from(
                app.world()
                    .get::<Camera3d>(first)
                    .unwrap()
                    .depth_texture_usages
            )
            .contains(TextureUsages::TEXTURE_BINDING)
        );
        // Enter a bank with fog disabled: retire only the depth usage owned by fog.
        let third = app
            .world_mut()
            .spawn(Camera3d {
                depth_texture_usages: (TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC)
                    .into(),
                ..default()
            })
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<super::super::MapLighting>()
            .gparam = fixture(vec![("VolumetricFog Enable", 0, 0., Value::Bool(false))]);
        app.world_mut()
            .resource_mut::<super::super::MapLighting>()
            .source = "fog disabled".into();
        app.update();
        for entity in [first, second] {
            assert!(app.world().get::<GameFog>(entity).is_none());
            let usages = TextureUsages::from(
                app.world()
                    .get::<Camera3d>(entity)
                    .unwrap()
                    .depth_texture_usages,
            );
            assert!(!usages.contains(TextureUsages::TEXTURE_BINDING));
        }
        let usages = TextureUsages::from(
            app.world()
                .get::<Camera3d>(third)
                .unwrap()
                .depth_texture_usages,
        );
        assert!(usages.contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC));
    }

    #[test]
    fn authored_zero_density_and_channel_extinction_survive_upload() {
        let g = Gparam {
            groups: vec![Group {
                name: "VolumetricFog ParamEditor".into(),
                display_name: String::new(),
                params: [
                    ("Global FixedDensity", Value::Float(0.)),
                    ("Global HeightFogDensity Scale", Value::Float(0.)),
                    ("Global AbsorptionScale", Value::Vec3([2., 0., 0.5])),
                    ("SkyLight StartDist", Value::Float(30.)),
                    ("DirLight StartDist", Value::Float(70.)),
                ]
                .into_iter()
                .map(|(name, value)| Param {
                    name: name.into(),
                    display_name: String::new(),
                    entries: vec![Entry {
                        id: 0,
                        time: 12.,
                        value,
                    }],
                })
                .collect(),
            }],
        };
        let f = GameFog::from_gparam(&g, 100, 12., Vec3::Y).unwrap();
        assert_eq!(f.density.x, 0.);
        assert_eq!(f.density.z, 0.);
        assert_eq!(f.extinction.xyz(), Vec3::new(3., 1., 1.5));
        assert_eq!(f.depths.zw(), Vec2::new(30., 70.));
    }

    #[test]
    fn castle_upload_uses_real_hour_rows_and_separate_directional_colours() {
        let path = sekiro_sim::cache::root().join("raw/param/drawparam/m11_01_0000.gparam");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let g = sekiro_formats::gparam::parse(&bytes).unwrap();
        let noon = GameFog::from_gparam(&g, 0, 12., Vec3::Y).unwrap();
        let dusk = GameFog::from_gparam(&g, 0, 18., Vec3::Y).unwrap();
        assert_eq!(noon.depths.y, 1000.);
        assert_eq!(noon.extinction.w, 1.);
        assert!((noon.density.x - 0.0006).abs() < 0.000001);
        assert!((noon.density.z - 0.05).abs() < 0.000001);
        assert_ne!(noon.direct.xyz(), noon.opposite.xyz());
        assert_ne!(noon.direct.xyz(), dusk.direct.xyz());
    }
}

impl GameFog {
    pub fn from_gparam(g: &Gparam, id: i32, hour: f32, sun: Vec3) -> Option<Self> {
        let get = |name: &str, fallback: f32| {
            g.param("VolumetricFog", name)
                .and_then(|p| p.at(id, hour))
                .and_then(|v| v.first().copied())
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
        };
        // Renderer enable and analytical continuation are independent discrete switches.
        if fog_param_exact(g, "Enable")
            .and_then(|p| p.nearest(id, hour))
            .and_then(|v| v.first().copied())
            .unwrap_or(1.)
            < 0.5
        {
            return None;
        }
        let color = |name: &str, fallback: Vec3| {
            let v = g.param("VolumetricFog", name).and_then(|p| p.at(id, hour));
            let rgb = v
                .as_ref()
                .filter(|v| v.len() >= 3)
                .map_or(fallback, |v| Vec3::new(v[0], v[1], v[2]));
            // Verified byte quantization and gamma 2.2 upload (39e3d0).
            super::fog_colour(rgb, v.and_then(|v| v.get(3).copied()).unwrap_or(1.))
        };
        // Verified CPU snapshot transforms (39e7e0, 39fa70); raw offset is metres.
        let gs = get("GameScale", 1.).max(f32::MIN_POSITIVE);
        let near = get("NearDepth", 0.01);
        let distribution = get("HeightFogDistribution", 0.1) / gs;
        let sign = if distribution < 0. { -1. } else { 1. };
        Some(Self {
            density: Vec4::new(
                get("FixedDensity", 0.0015) / gs,
                distribution.abs().max(f32::MIN_POSITIVE),
                0.5 * get("HeightFogDensity Scale", 0.25) / gs,
                -sign * get("HeightFogOffset", -150.),
            ),
            depths: Vec4::new(
                gs * near,
                gs * get("FarDepth", 400.).max(near + 0.1),
                gs * get("SkyLight StartDist", 0.),
                gs * get("DirLight StartDist", 0.),
            ),
            sky: color("SkyColor", Vec3::new(0.49, 0.55, 0.65))
                .extend(get("SkyLight HeightAtten", 0.).max(0.)),
            direct: color("DirectColor", Vec3::splat(0.5))
                .extend(get("DirLight HeightAtten", 0.).max(0.)),
            opposite: color("OppositeColor", Vec3::splat(0.15)).extend(0.),
            // The caller supplies the resolved native bank/light direction.
            direction: sun.extend(1. / (sign * sun.y).max(get("AngleClip", 0.01))),
            extinction: (Vec3::ONE
                + g.param("VolumetricFog", "AbsorptionScale")
                    .and_then(|p| p.at(id, hour))
                    .filter(|v| v.len() >= 3)
                    .map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2])))
            .extend(sign),
            slices: Vec4::new(
                // Constructor 1402a0740: +0x14 = 1, +0x08 = 32, +0x24 = 0.
                // The base Ashina GPARAMs omit these sampling controls.
                gs * get("NearOffset", 1.),
                get("VolumeDepth", 32.).clamp(4., 128.),
                get("VolumeSliceSampleOffset", 0.),
                fog_param_exact(g, "EnableAnalyticalFog")
                    .and_then(|p| p.nearest(id, hour))
                    .and_then(|v| v.first().copied())
                    // 1402a0740 initializes blended-record byte +0x20 to false.
                    .unwrap_or(0.),
            ),
            background_control: Vec4::ZERO,
            background: [Vec4::ZERO; 16],
            radiance_scale: Vec4::X,
        })
    }
}

fn fog_param_exact<'a>(g: &'a Gparam, name: &str) -> Option<&'a sekiro_formats::gparam::Param> {
    g.groups
        .iter()
        .filter(|group| group.name.starts_with("VolumetricFog"))
        .flat_map(|group| &group.params)
        .find(|param| {
            let canonical = param
                .name
                .strip_prefix("VolumetricFog ")
                .unwrap_or(&param.name);
            canonical.strip_prefix("Global ").unwrap_or(canonical) == name
        })
}

// Read identifiers directly from their integer entries, without interpolation or f32 rounding.
fn fog_integer(g: &Gparam, name: &str, id: i32, hour: f32) -> Option<i32> {
    let param = fog_param_exact(g, name)?;
    let row = if param.entries.iter().any(|entry| entry.id == id) {
        id
    } else {
        0
    };
    let distance = |time: f32| {
        let d = (hour.rem_euclid(24.) - time.rem_euclid(24.)).abs();
        d.min(24. - d)
    };
    match &param
        .entries
        .iter()
        .filter(|entry| entry.id == row)
        .min_by(|a, b| distance(a.time).total_cmp(&distance(b.time)))?
        .value
    {
        sekiro_formats::gparam::Value::Int(value) => Some(*value),
        _ => None,
    }
}

fn bank_map(bank: i32) -> Option<String> {
    // Native map-bank producer f47270/f472a0: ((area + 100) * 100 + block) * 10000.
    let area = bank / 1_000_000 - 100;
    let block = bank / 10_000 % 100;
    ((0..100).contains(&area) && bank % 10_000 == 0).then(|| format!("m{area:02}_{block:02}"))
}

fn fog_bounds(
    param: &sekiro_formats::gparam::Param,
    id: i32,
    hour: f32,
) -> Option<(
    &sekiro_formats::gparam::Entry,
    &sekiro_formats::gparam::Entry,
    f32,
)> {
    let mut entries: Vec<_> = param
        .entries
        .iter()
        .filter(|entry| entry.id == id)
        .collect();
    if entries.is_empty() && id != 0 {
        return fog_bounds(param, 0, hour);
    }
    if entries.is_empty() {
        return None;
    }
    entries.sort_by(|a, b| a.time.total_cmp(&b.time));
    let hour = hour.rem_euclid(24.);
    let (a, b) = match entries.iter().position(|entry| entry.time > hour) {
        Some(0) | None => (entries[entries.len() - 1], entries[0]),
        Some(i) => (entries[i - 1], entries[i]),
    };
    let span = (b.time - a.time).rem_euclid(24.);
    let blend = if span > 0. {
        (hour - a.time).rem_euclid(24.) / span
    } else {
        0.
    };
    Some((a, b, blend))
}

#[derive(Default)]
struct FogBanks(std::collections::BTreeMap<String, Option<Gparam>>);

impl FogBanks {
    fn direction(&mut self, g: &Gparam, source: &str, id: i32, hour: f32) -> Vec3 {
        let pair = fog_param_exact(g, "DirLight LightBankId")
            .and_then(|param| fog_bounds(param, id, hour));
        let Some((a, b, blend)) = pair else {
            return self.resolve(g, source, 1, 0, hour);
        };
        let integer = |entry: &sekiro_formats::gparam::Entry| match &entry.value {
            sekiro_formats::gparam::Value::Int(value) => *value,
            _ => 1,
        };
        let bank_a = integer(a);
        let bank_b = integer(b);
        let light_a = fog_integer(g, "DirLight LightId", id, a.time).unwrap_or(0);
        let light_b = fog_integer(g, "DirLight LightId", id, b.time).unwrap_or(0);
        let direction = self.resolve(g, source, bank_a, light_a, hour);
        if blend == 0. || (bank_a, light_a) == (bank_b, light_b) {
            return direction;
        }
        let other = self.resolve(g, source, bank_b, light_b, hour);
        // 169080 -> 30d730/30d3e0 blends resolved directions by a spherical rotation.
        // Identifiers themselves are never interpolated.
        let rotation = Quat::from_rotation_arc(direction.normalize(), other.normalize());
        let result = Quat::IDENTITY.slerp(rotation, blend) * direction;
        info!(
            "fog light blend: ({bank_a},{light_a}) -> ({bank_b},{light_b}), factor={blend}, direction={result:?}"
        );
        result
    }

    fn resolve(&mut self, g: &Gparam, source: &str, bank: i32, light: i32, hour: f32) -> Vec3 {
        let stem = bank_map(bank).map(|map| format!("{map}_0000"));
        let current = bank_map(bank).is_some_and(|map| source.starts_with(&map));
        let bank_source = if current {
            source
        } else {
            stem.as_deref().unwrap_or("default")
        };
        let direction = if current {
            Self::light(g, light, hour)
        } else {
            self.load(bank_source)
                .and_then(|bank| Self::light(bank, light, hour))
        };
        // 168e10 falls back to bank 1 / light 0, never the selected collision sun.
        let direction =
            direction.or_else(|| self.load("default").and_then(|g| Self::light(g, 0, hour)));
        if direction.is_none() {
            warn!(
                "fog bank {bank}, light {light}: no authored direction in {bank_source} or default bank; using native dummy light"
            );
        }
        // 30e440's dummy vector, mirrored into our world coordinates.
        let axis = f32::from_bits(0x3f350481);
        let direction = direction.unwrap_or(Vec3::new(-axis, axis, 0.));
        info!(
            "fog light: bank={bank}, light={light}, source={bank_source}, direction={direction:?}"
        );
        direction
    }

    fn light(g: &Gparam, id: i32, hour: f32) -> Option<Vec3> {
        let param = g.param("LightSet", "Directional Light Angle0")?;
        if !param.entries.iter().any(|entry| entry.id == id) {
            return None;
        }
        let (a, b, blend) = fog_bounds(param, id, hour)?;
        let toward = |entry: &sekiro_formats::gparam::Entry| {
            let angle = entry.value.floats();
            if angle.len() < 2 || !angle[..2].iter().all(|v| v.is_finite()) {
                return None;
            }
            let direction =
                Quat::from_rotation_y(angle[1]) * Quat::from_rotation_x(angle[0]) * Vec3::Z;
            Some(Vec3::new(-direction.x, direction.y, direction.z))
        };
        // 310630 uploads the linear blend of the two derived LightSet directions.
        Some(toward(a)?.lerp(toward(b)?, blend))
    }

    fn load(&mut self, source: &str) -> Option<&Gparam> {
        self.0
            .entry(source.into())
            .or_insert_with(|| {
                let bytes = std::fs::read(
                    sekiro_sim::cache::root().join(format!("raw/param/drawparam/{source}.gparam")),
                )
                .ok()?;
                sekiro_formats::gparam::parse(&bytes).ok()
            })
            .as_ref()
    }
}

#[derive(Clone)]
struct Background {
    control: Vec4,
    rings: [Vec4; 16],
}

impl Background {
    fn from_positions(positions: &[[f32; 3]]) -> Option<Self> {
        // Native sky meshes are regular polygon rings, including a planar cap in night skies.
        let mut vertices: Vec<_> = positions.iter().map(|&p| Vec3::from_array(p)).collect();
        if vertices.len() < 16 || vertices.iter().any(|p| !p.is_finite()) {
            return None;
        }
        vertices.sort_by(|a, b| a.y.total_cmp(&b.y));
        let mut rings: Vec<(f32, f32)> = Vec::new();
        for vertex in &vertices {
            let radius = vertex.xz().length();
            if let Some(last) = rings.last_mut()
                && (last.1 - vertex.y).abs() < 0.01
            {
                last.0 = last.0.max(radius);
            } else {
                rings.push((radius, vertex.y));
            }
        }
        let top = *rings.last()?;
        if top.0 > 0.02
            && vertices
                .iter()
                .any(|v| (v.y - top.1).abs() < 0.01 && v.xz().length() < 0.02)
        {
            rings.push((0., top.1));
        }
        if rings.len() < 2 || rings.len() > 16 {
            return None;
        }
        let widest = rings.iter().max_by(|a, b| a.0.total_cmp(&b.0))?;
        let mut angles: Vec<_> = vertices
            .iter()
            .filter(|p| (p.y - widest.1).abs() < 0.01 && (p.xz().length() - widest.0).abs() < 0.02)
            .map(|p| p.z.atan2(p.x))
            .collect();
        angles.sort_by(f32::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 0.00001);
        if angles.last()? - angles[0] > std::f32::consts::TAU - 0.00001 {
            angles.pop();
        }
        if angles.len() < 3 {
            return None;
        }
        let step = std::f32::consts::TAU / angles.len() as f32;
        let phase = angles[0];
        if angles
            .iter()
            .enumerate()
            .any(|(i, angle)| (angle - phase - i as f32 * step).abs() > 0.00001)
        {
            return None;
        }
        let mut result = Self {
            control: Vec4::new(rings.len() as f32, step, phase, 0.),
            rings: [Vec4::ZERO; 16],
        };
        for (target, (radius, height)) in result.rings.iter_mut().zip(&rings) {
            *target = Vec4::new(radius * (0.5 * step).cos(), *height, 0., 0.);
        }
        // Validate all rings against the same polygon grid; only cap interiors may have smaller radii.
        for vertex in &vertices {
            let radius = vertex.xz().length();
            let ring = rings
                .iter()
                .find(|(_, height)| (vertex.y - height).abs() < 0.01)?;
            if radius < 0.02 {
                continue;
            }
            let angular_error = ((vertex.z.atan2(vertex.x) - phase) / step).rem_euclid(1.);
            if angular_error.min(1. - angular_error) * step > 0.00001
                || ((radius - ring.0).abs() > 0.02 && (ring.1 - rings.last()?.1).abs() > 0.01)
            {
                return None;
            }
        }
        Some(result)
    }
}

type BackgroundMeshes<'w, 's> = Query<
    'w,
    's,
    (&'static Mesh3d, &'static Visibility),
    (
        With<NoFrustumCulling>,
        With<NotShadowCaster>,
        With<NotShadowReceiver>,
    ),
>;

pub struct FogPlugin;
impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fog.wesl");
        app.add_plugins((
            ExtractComponentPlugin::<GameFog>::default(),
            UniformComponentPlugin::<GameFog>::default(),
        ))
        .add_observer(release_depth_sampling)
        .add_systems(Update, configure.after(super::apply_lighting))
        .add_systems(PostUpdate, update_background);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .add_systems(RenderStartup, init_pipeline)
                .add_systems(
                    Core3d,
                    fog.before(Core3dSystems::EarlyPostProcess)
                        .after(Core3dSystems::MainPass),
                );
        }
    }
}

fn configure(
    mut commands: Commands,
    lighting: Res<super::MapLighting>,
    cameras: Query<(Entity, &Camera3d)>,
    mut last: Local<Option<(i32, f32, i32, String)>>,
    added: Query<(), Added<Camera3d>>,
    selection: Res<FogSelection>,
    mut banks: Local<FogBanks>,
) {
    let key = (lighting.light_set, lighting.hour);
    let unchanged = last.as_ref().is_some_and(|(set, hour, fog, source)| {
        (*set, *hour, *fog) == (key.0, key.1, selection.0) && source == &lighting.source
    });
    if unchanged && added.is_empty() {
        return;
    }
    if !unchanged {
        *last = Some((key.0, key.1, selection.0, lighting.source.clone()));
    }
    // Preserve the independent MSB FogParamID. No separate Fog formula was recovered.
    // Inspect selected authored rows rather than silently aliasing this ID to LightSetID.
    let legacy: Vec<_> = lighting
        .gparam
        .groups
        .iter()
        .filter(|g| g.name.contains("Fog") && !g.name.contains("VolumetricFog"))
        .flat_map(|g| g.params.iter().map(|p| (&p.name, p.at(selection.0, key.1))))
        .collect();
    info!(
        "MSB FogParamID {}: separate Fog rows {legacy:?}",
        selection.0
    );
    let direction = banks.direction(&lighting.gparam, &lighting.source, key.0, key.1);
    let value = (!std::env::args().any(|a| a == "--no-fog"))
        .then(|| GameFog::from_gparam(&lighting.gparam, key.0, key.1, direction))
        .flatten();
    info!("shader fog: {value:?}");
    for (entity, camera) in &cameras {
        let mut depth = TextureUsages::from(camera.depth_texture_usages);
        if value.is_some() && !depth.contains(TextureUsages::TEXTURE_BINDING) {
            depth.insert(TextureUsages::TEXTURE_BINDING);
            commands.entity(entity).insert(FogDepthSampling);
        }
        // Only new cameras need this; re-inserting Camera3d at every LightSet change (#15)
        // marked the view changed and rebuilt its depth texture.
        if TextureUsages::from(camera.depth_texture_usages) != depth {
            let mut camera = camera.clone();
            camera.depth_texture_usages = depth.into();
            commands.entity(entity).insert(camera);
        }
        if let Some(value) = &value {
            commands.entity(entity).insert(value.clone());
        } else {
            commands.entity(entity).remove::<GameFog>();
        }
    }
}

fn update_background(
    meshes: Option<Res<Assets<Mesh>>>,
    background_meshes: BackgroundMeshes,
    mut backgrounds: Local<
        std::collections::HashMap<bevy::asset::AssetId<Mesh>, Option<Background>>,
    >,
    mut cameras: Query<(&mut GameFog, Option<&Exposure>)>,
) {
    // Sky's positions/UV-only mesh layout and shadow/visibility markers identify its live dome.
    // Follow the selected mesh handle instead of reproducing the sky/event selector here.
    let background = meshes.as_ref().and_then(|meshes| {
        background_meshes
            .iter()
            .filter(|(_, visibility)| **visibility != Visibility::Hidden)
            .filter_map(|(handle, _)| {
                let mesh = meshes.get(&handle.0)?;
                if mesh.contains_attribute(Mesh::ATTRIBUTE_NORMAL)
                    || !mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1)
                {
                    return None;
                }
                let profile = backgrounds
                    .entry(handle.0.id())
                    .or_insert_with(|| {
                        let VertexAttributeValues::Float32x3(positions) =
                            mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
                        else {
                            return None;
                        };
                        Background::from_positions(positions)
                    })
                    .as_ref()?;
                Some((handle.0.id(), profile.clone()))
            })
            .max_by(|a, b| a.1.control.x.total_cmp(&b.1.control.x))
    });
    // Run after Update's sky swaps, without reinserting fog on sweep diagnostic cameras.
    for (mut value, exposure) in &mut cameras {
        // Same radiance bridge as the sky and diffuse accumulator, applied once.
        let radiance_scale = super::ENV_PER_UNIT * exposure.copied().unwrap_or_default().exposure();
        let control = background.as_ref().map_or(Vec4::ZERO, |(_, b)| b.control);
        let rings = background
            .as_ref()
            .map_or([Vec4::ZERO; 16], |(_, b)| b.rings);
        if value.background_control != control
            || value.background != rings
            || value.radiance_scale.x != radiance_scale
        {
            value.background_control = control;
            value.background = rings;
            value.radiance_scale.x = radiance_scale;
            if let Some((id, background)) = &background {
                info!(
                    "fog background: mesh={id:?}, rings={}, sectors={}, outline={:?}",
                    background.control.x,
                    std::f32::consts::TAU / background.control.y,
                    &background.rings[..background.control.x as usize]
                );
            }
            info!("shader fog: {value:?}");
        }
    }
}

#[derive(Resource)]
struct FogPipeline(Vec<Option<FogVariant>>);
struct FogVariant {
    layout: BindGroupLayoutDescriptor,
    pipelines: Vec<CachedRenderPipelineId>,
}
fn init_pipeline(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>,
    cache: Res<PipelineCache>,
    adapter: Res<bevy::render::renderer::RenderAdapter>,
) {
    let supported = adapter
        .get_texture_format_features(TextureFormat::Rgba16Float)
        .flags;
    let variants = [1, 2, 4, 8]
        .into_iter()
        .map(|samples| {
            if !supported.sample_count_supported(samples) {
                return None;
            }
            let multisampled = samples > 1;
            let layout = BindGroupLayoutDescriptor::new(
                "game fog",
                &if multisampled {
                    BindGroupLayoutEntries::sequential(
                        ShaderStages::FRAGMENT,
                        (
                            texture_depth_2d_multisampled(),
                            uniform_buffer::<ViewUniform>(true),
                            uniform_buffer::<GameFog>(true),
                        ),
                    )
                    .to_vec()
                } else {
                    BindGroupLayoutEntries::sequential(
                        ShaderStages::FRAGMENT,
                        (
                            texture_2d(TextureSampleType::Float { filterable: false }),
                            texture_depth_2d(),
                            uniform_buffer::<ViewUniform>(true),
                            uniform_buffer::<GameFog>(true),
                        ),
                    )
                    .to_vec()
                },
            );
            let pipelines = (0..if multisampled { 2 } else { 1 })
                .map(|step| {
                    let blend = multisampled.then_some(BlendState {
                        color: BlendComponent {
                            src_factor: if step == 0 {
                                BlendFactor::Zero
                            } else {
                                BlendFactor::One
                            },
                            dst_factor: if step == 0 {
                                BlendFactor::Src
                            } else {
                                BlendFactor::One
                            },
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::Zero,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                    });
                    let mut shader_defs = Vec::new();
                    if multisampled {
                        shader_defs.push("MULTISAMPLED_DEPTH".into());
                        if step == 0 {
                            shader_defs.push("FOG_ATTENUATION".into());
                        }
                    }
                    cache.queue_render_pipeline(RenderPipelineDescriptor {
                        label: Some("Sekiro global fog".into()),
                        layout: vec![layout.clone()],
                        vertex: fullscreen.to_vertex_state(),
                        fragment: Some(FragmentState {
                            shader: load_embedded_asset!(assets.as_ref(), "fog.wesl"),
                            shader_defs,
                            targets: vec![Some(ColorTargetState {
                                format: TextureFormat::Rgba16Float,
                                blend,
                                write_mask: ColorWrites::ALL,
                            })],
                            ..default()
                        }),
                        multisample: MultisampleState {
                            count: samples,
                            ..default()
                        },
                        ..default()
                    })
                })
                .collect();
            Some(FogVariant { layout, pipelines })
        })
        .collect();
    commands.insert_resource(FogPipeline(variants));
}
fn fog(
    view: ViewQuery<(
        &ViewTarget,
        &ViewDepthStencilTexture,
        &Msaa,
        &ViewUniformOffset,
        &DynamicUniformIndex<GameFog>,
    )>,
    pipeline: Res<FogPipeline>,
    cache: Res<PipelineCache>,
    views: Res<ViewUniforms>,
    uniforms: Res<ComponentUniforms<GameFog>>,
    mut ctx: RenderContext,
    mut warned_depth: Local<bool>,
) {
    let (target, depth, msaa, offset, index) = view.into_inner();
    let Some(depth_view) = depth.attachment.depth_stencil_views().depth_only_view() else {
        if !*warned_depth {
            warn!("global fog skipped: depth-only view is unavailable");
            *warned_depth = true;
        }
        return;
    };
    // A newly extracted fog component can precede its rebuilt depth attachment.
    if !depth
        .texture()
        .usage()
        .contains(TextureUsages::TEXTURE_BINDING)
    {
        return;
    }
    let multisampled = msaa.samples() > 1;
    let Some(pipeline) = &pipeline.0[msaa.samples().ilog2() as usize] else {
        return;
    };
    let (Some(view_binding), Some(fog_binding)) =
        (views.uniforms.binding(), uniforms.uniforms().binding())
    else {
        return;
    };
    if target.main_texture_format() != TextureFormat::Rgba16Float {
        return;
    }
    let psos: Vec<_> = pipeline
        .pipelines
        .iter()
        .filter_map(|id| cache.get_render_pipeline(*id))
        .collect();
    if psos.len() != pipeline.pipelines.len() {
        return;
    }
    use bevy::render::diagnostic::RecordDiagnostics;
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "game_fog");
    if multisampled {
        // Preserve per-sample colour/depth correspondence through attenuation and
        // inscattering, then let the existing main attachment resolve the result.
        let bindings = ctx.render_device().create_bind_group(
            "game fog samples",
            &cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::sequential((depth_view, view_binding, fog_binding)),
        );
        for pso in psos {
            let mut attachment = target.get_color_attachment();
            attachment.ops.load = LoadOp::Load;
            let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("game fog samples"),
                color_attachments: &[Some(attachment)],
                ..default()
            });
            pass.set_render_pipeline(pso);
            pass.set_bind_group(0, &bindings, &[offset.offset, index.index()]);
            pass.draw(0..3, 0..1);
        }
        span.end(ctx.command_encoder());
        return;
    }
    let post = target.post_process_write();
    let bindings = ctx.render_device().create_bind_group(
        "game fog",
        &cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((post.source, depth_view, view_binding, fog_binding)),
    );
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("game fog"),
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
    pass.set_render_pipeline(psos[0]);
    pass.set_bind_group(0, &bindings, &[offset.offset, index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}
