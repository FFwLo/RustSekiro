use bevy::shader::ShaderDefVal;

#[test]
fn snow_coverage_matches_native_height_and_wet_operations() {
    use std::borrow::Cow;
    use wgpu::util::DeviceExt;

    // Native Ground Gbuf 16/18 and Multiple Gbuf 20: fixed instruction-derived
    // samples guard the order of wet exclusion, height subtraction and saturation.
    let production = include_str!("../../src/map_material.wesl");
    let coverage = production.split("fn snow_coverage(").nth(1).unwrap();
    let coverage = coverage.split("// FLV:").next().unwrap();
    let normal = production.split("fn layer_world_normal(").nth(1).unwrap();
    let normal = normal.split("fn has(").next().unwrap();
    let source = format!(
        r#"
fn saturate(x: f32) -> f32 {{ return clamp(x, 0.0, 1.0); }}
fn snow_coverage({coverage}
fn layer_world_normal({normal}
@group(0) @binding(0) var<storage, read_write> result: array<f32, 6>;
@compute @workgroup_size(1)
fn main() {{
    let height = (dot(vec3(0.2, 0.6, 0.4), vec3(0.0, 0.5, 0.7)) + 0.01) * 0.5;
    result[0] = snow_coverage(0.85, 0.4, 0.0, height, 3.0);
    result[1] = snow_coverage(0.85, 0.4, 0.8, height, 3.0);
    result[2] = snow_coverage(0.85, 0.4, 1.25, height, 3.0);
    result[3] = snow_coverage(1.4, 1.0, 0.5, 0.2, 0.0);
    result[4] = snow_coverage(-0.2, 1.0, 1.5, 0.05, 0.0);
    let n = layer_world_normal(vec3(0.6, 0.8, 1.2), vec3(0.0, 0.8, 0.0), vec4(0.7, 0.0, 0.0, 1.0), true);
    result[5] = snow_coverage(n.y, 1.0, 0.0, 0.2, 0.0);
}}
"#
    );
    let instance = wgpu::Instance::default();
    let adapter = bevy::tasks::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) =
        bevy::tasks::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("native snow coverage"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let output = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 24],
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 24,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 24);
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| sender.send(r).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    receiver.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    for (i, expected) in [0.5812, 0.0, 0.0, 0.64, 0.055, 0.7218655]
        .into_iter()
        .enumerate()
    {
        let actual = f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert!(
            (actual - expected).abs() < 0.000002,
            "sample {i}: {actual} vs {expected}"
        );
    }
}

#[test]
fn layered_map_forward_depth_and_motion_shaders_validate() {
    let (mut cache, mut assets) = super::shader_support::cache();
    super::shader_support::register(
        &mut cache,
        &mut assets,
        "map_coverage",
        include_str!("../../src/map_coverage.wesl"),
    );
    super::shader_support::register(
        &mut cache,
        &mut assets,
        "map_bindings",
        include_str!("../../src/map_bindings.wesl"),
    );
    for (name, source, prepass) in [
        (
            "map_material",
            include_str!("../../src/map_material.wesl"),
            false,
        ),
        (
            "map_prepass",
            include_str!("../../src/map_prepass.wesl"),
            true,
        ),
        ("map_sweep", include_str!("../../src/map_sweep.wesl"), false),
        (
            "map_fog_surface",
            include_str!("../../src/map_fog_surface.wesl"),
            false,
        ),
    ] {
        let id = super::shader_support::register(&mut cache, &mut assets, name, source);
        if name == "map_sweep" {
            // Collision has positions/normals but no UVs, tangents or colour streams.
            let mut defs: Vec<ShaderDefVal> = [
                "VERTEX_NORMALS",
                "VERTEX_OUTPUT_INSTANCE_INDEX",
                "SHADOW_FILTER_METHOD_HARDWARE_2X2",
            ]
            .into_iter()
            .map(Into::into)
            .collect();
            for (key, n) in [
                ("MATERIAL_BIND_GROUP", 3),
                ("MAX_DIRECTIONAL_LIGHTS", 10),
                ("MAX_CASCADES_PER_LIGHT", 4),
                ("MAX_RECT_LIGHTS", 64),
                ("AVAILABLE_STORAGE_BUFFER_BINDINGS", 8),
                ("MSAA_SAMPLES", 1),
            ] {
                defs.push(ShaderDefVal::UInt(key.into(), n));
            }
            cache
                .get(0, id, &defs)
                .unwrap_or_else(|e| panic!("sweep collision shader: {e:?}"));
        }
        for bindless in [false, true] {
            for game in [false, true] {
                for msaa in [1, 4] {
                    let mut defs: Vec<ShaderDefVal> = [
                        "VERTEX_UVS",
                        "VERTEX_UVS_A",
                        "VERTEX_UVS_B",
                        "VERTEX_NORMALS",
                        "VERTEX_TANGENTS",
                        "VERTEX_COLORS",
                        "VERTEX_OUTPUT_INSTANCE_INDEX",
                        "CAMERA_FADE",
                        "SHADOW_FILTER_METHOD_HARDWARE_2X2",
                    ]
                    .into_iter()
                    .map(Into::into)
                    .collect();
                    for (key, n) in [
                        ("MATERIAL_BIND_GROUP", 3),
                        ("MAX_DIRECTIONAL_LIGHTS", 10),
                        ("MAX_CASCADES_PER_LIGHT", 4),
                        ("MAX_RECT_LIGHTS", 64),
                        ("AVAILABLE_STORAGE_BUFFER_BINDINGS", 8),
                        ("MSAA_SAMPLES", msaa),
                    ] {
                        defs.push(ShaderDefVal::UInt(key.into(), n));
                    }
                    if msaa > 1 {
                        defs.push("MULTISAMPLED".into());
                    }
                    if bindless {
                        defs.push("BINDLESS".into());
                    }
                    if game {
                        defs.push("GAME_MATERIAL".into());
                        if !prepass {
                            defs.push("GAME_LAYERS".into());
                        }
                    }
                    if prepass {
                        defs.extend(
                            [
                                "PREPASS_PIPELINE",
                                "PREPASS_FRAGMENT",
                                "MOTION_VECTOR_PREPASS",
                                "NORMAL_PREPASS",
                                "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
                            ]
                            .into_iter()
                            .map(Into::into),
                        );
                    }
                    if name == "map_fog_surface" {
                        defs.push("DEPTH_PREPASS".into());
                        defs.push("ENVIRONMENT_MAP".into());
                    }
                    let native_variants: &[bool] = if name == "map_material" {
                        &[false, true]
                    } else {
                        &[false]
                    };
                    for native_direct in native_variants {
                        let mut variant = defs.clone();
                        if *native_direct {
                            variant.push("NATIVE_DIRECT".into());
                        }
                        cache.get(0, id, &variant).unwrap_or_else(|e| {
                            panic!("{name}: bindless={bindless}, game={game}, msaa={msaa}, native_direct={native_direct}: {e:?}")
                        });
                    }
                    if name == "map_fog_surface" {
                        defs.retain(|d| !matches!(d, ShaderDefVal::Bool(name, _) if name == "DEPTH_PREPASS"));
                        defs.push("FOG_DIAGNOSTIC".into());
                        cache
                            .get(0, id, &defs)
                            .unwrap_or_else(|e| panic!("fog surface sweep diagnostic: {e:?}"));
                    }
                }
            }
        }
    }
}
