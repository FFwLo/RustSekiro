use super::*;

fn document(bytes: &[u8]) -> Value {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

#[test]
fn mountain_retains_native_snow_mask_and_baked_occlusion() {
    let raw = sekiro_formats::cache::root().join("raw");
    let source =
        raw.join("map/m11_00_00_00/m11_00_00_00_925000.mapbnd.d/m11_00_00_00_925000.flver");
    if !source.is_file() {
        return;
    }
    let f = flver::parse(&std::fs::read(source).unwrap()).unwrap();
    let mut mtds = Mtds(HashMap::new());
    for mat in &f.materials {
        let name = format!("{}.mtd", texture_stem(&mat.mtd));
        let m = mtd::parse(
            &std::fs::read(raw.join("mtd/allmaterialbnd.mtdbnd.d").join(&name)).unwrap(),
        )
        .unwrap();
        mtds.0.insert(name.to_ascii_lowercase(), m);
    }
    let choice = choose_material(&f.materials[0], mtds.get(&f.materials[0].mtd));
    let layers = choice
        .layers
        .as_ref()
        .expect("native Mountain must retain both albedos");
    assert_eq!(layers.kind, 3);
    assert!(layers.snow.is_some() && layers.mask.is_some() && layers.detail.is_some());
    let wanted = wanted_textures(&f, &mtds);
    assert!(wanted.iter().any(|(name, _)| name == "m99_snow_00_a"));
    let built = build(&f, &mtds, &|name| Some(format!("tex/{name}.dds"))).unwrap();
    let d = document(&built.glb);
    assert!(d["materials"][0]["extras"]["sekiro_layers"]["snow"]["uri"].is_string());
    let primitive = &d["meshes"][0]["primitives"][0];
    let accessor = &d["accessors"][primitive["attributes"]["COLOR_0"].as_u64().unwrap() as usize];
    let view = &d["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
    let offset = 28
        + u32::from_le_bytes(built.glb[12..16].try_into().unwrap()) as usize
        + view["byteOffset"].as_u64().unwrap_or(0) as usize
        + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
    for (i, color) in f.meshes[0].vertices.colors[0].iter().enumerate() {
        let start = offset + i * 16;
        assert_eq!(
            f32::from_le_bytes(built.glb[start..start + 4].try_into().unwrap()),
            color[0]
        );
    }
}

#[test]
fn detail_scenes_do_not_draw_other_levels_or_motion_blur_together() {
    use flver::face_set_flags::*;
    let mesh = flver::Mesh {
        dynamic: false,
        material: 0,
        node: 7,
        bone_indices: vec![],
        face_sets: [0, LOD1, LOD2, LOD_EX, LOD1 | MOTION_BLUR]
            .into_iter()
            .map(|flags| flver::FaceSet {
                flags,
                triangle_strip: false,
                cull_backfaces: flags != LOD2,
                indices: vec![0, 1, 2],
            })
            .collect(),
        buffers: vec![],
        vertices: flver::Vertices {
            positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [9., 9., 9.]],
            ..Default::default()
        },
    };
    let f = flver::Flver {
        version: 0x2001a,
        bbox_min: [0.; 3],
        bbox_max: [1.; 3],
        dummies: vec![],
        materials: vec![flver::Material {
            name: "surface".into(),
            mtd: "surface.mtd".into(),
            textures: vec![],
            index: 0,
        }],
        nodes: vec![],
        meshes: vec![mesh],
        layouts: vec![],
    };
    let built = build(&f, &Mtds(HashMap::new()), &|_| None).unwrap();
    let doc = document(&built.glb);
    assert_eq!(built.primitives, 4);
    assert_eq!(doc["scene"], 0);
    for level in 0..4 {
        let node = doc["scenes"][level]["nodes"][0].as_u64().unwrap() as usize;
        let mesh = doc["nodes"][node]["mesh"].as_u64().unwrap() as usize;
        let p = doc["meshes"][mesh]["primitives"].as_array().unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0]["extras"]["sekiro_lod"], level);
        assert_eq!(p[0]["extras"]["face_flags"], [0, LOD1, LOD2, LOD_EX][level]);
        let position = p[0]["attributes"]["POSITION"].as_u64().unwrap() as usize;
        assert_eq!(
            doc["accessors"][position]["count"],
            if level == 0 { 4 } else { 3 }
        );
        let material = p[0]["material"].as_u64().unwrap() as usize;
        assert_eq!(doc["materials"][material]["doubleSided"], level == 2);
    }
    assert_eq!(
        doc["meshes"][4]["primitives"][0]["extras"]["face_flags"],
        LOD1 | MOTION_BLUR
    );
}

#[test]
fn lower_detail_compaction_keeps_triangle_values_and_parallel_vertex_streams() {
    let original = flver::Vertices {
        positions: (0..4).map(|i| [i as f32, 2., 3.]).collect(),
        uvs: vec![(0..4).map(|i| [i as f32, 7.]).collect()],
        blends: vec![(0..4).map(|i| [i, 8, 9, 10]).collect()],
        normal_w: vec![10, 11, 12, 13],
        ..Default::default()
    };
    let original_indices = [3, 1, 2];
    let mut remapped = original_indices;
    let compact = compact_vertices(&original, &mut remapped);
    assert_eq!(compact.len(), 3);
    for (&before, &after) in original_indices.iter().zip(&remapped) {
        let (before, after) = (before as usize, after as usize);
        assert_eq!(compact.positions[after], original.positions[before]);
        assert_eq!(compact.uvs[0][after], original.uvs[0][before]);
        assert_eq!(compact.blends[0][after], original.blends[0][before]);
        assert_eq!(compact.normal_w[after], original.normal_w[before]);
    }
}

#[test]
fn alternate_levels_reuse_legacy_materials_and_append_culling_variants() {
    use flver::face_set_flags::*;
    let mesh = |material, faces: &[(u32, bool)]| flver::Mesh {
        dynamic: false,
        material,
        node: -1,
        bone_indices: vec![],
        buffers: vec![],
        face_sets: faces
            .iter()
            .map(|&(flags, cull_backfaces)| flver::FaceSet {
                flags,
                cull_backfaces,
                triangle_strip: false,
                indices: vec![0, 1, 2],
            })
            .collect(),
        vertices: flver::Vertices {
            positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            ..Default::default()
        },
    };
    let f = flver::Flver {
        version: 0x2001a,
        bbox_min: [0.; 3],
        bbox_max: [1.; 3],
        dummies: vec![],
        nodes: vec![],
        layouts: vec![],
        materials: ["wood", "snow"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| flver::Material {
                name: name.into(),
                mtd: format!("{name}.mtd"),
                textures: vec![],
                index: index as i32,
            })
            .collect(),
        meshes: vec![
            mesh(0, &[(LOD1, true), (0, true), (LOD2, false)]),
            mesh(1, &[(0, true), (LOD1, true)]),
        ],
    };
    let built = build(&f, &Mtds(HashMap::new()), &|_| None).unwrap();
    let doc = document(&built.glb);
    assert_eq!(doc["materials"].as_array().unwrap().len(), 3);
    for level in 0..2 {
        let node = doc["scenes"][level]["nodes"][0].as_u64().unwrap() as usize;
        let mesh = doc["nodes"][node]["mesh"].as_u64().unwrap() as usize;
        for (native, primitive) in doc["meshes"][mesh]["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert_eq!(primitive["material"], native);
            assert_eq!(primitive["extras"]["source_material"], native);
        }
    }
    assert_eq!(doc["meshes"][2]["primitives"][0]["material"], 2);
    assert_eq!(doc["materials"][2]["doubleSided"], true);
}

#[test]
fn all_eight_outskirts_models_keep_separate_authored_detail_meshes() {
    let cache = sekiro_formats::cache::root();
    let raw = cache.join("raw");
    if !raw.join("map/mapstudio/m11_00_00_00.msb").is_file() {
        eprintln!("installed-map fixture unavailable");
        return;
    }
    let mtds = super::super::load_mtds(&raw).unwrap();
    let names = [
        "004100", "004110", "004140", "004200", "004210", "004300", "004410", "004420",
    ];
    let mut lod_only = 0;
    let mut restored = 0;
    for name in names {
        let f = flver::parse(
            &std::fs::read(raw.join(format!(
                "map/m11_00_00_00/m11_00_00_00_{name}.mapbnd.d/m11_00_00_00_{name}.flver"
            )))
            .unwrap(),
        )
        .unwrap();
        let built = build(&f, &mtds, &|_| None).unwrap();
        let doc = document(&built.glb);
        assert_eq!(doc["scenes"].as_array().unwrap().len(), 4);
        let mut exported = 0;
        for (mi, mesh) in f.meshes.iter().enumerate() {
            let only = mesh.lod0().is_none()
                && mesh
                    .face_sets
                    .iter()
                    .any(|face| matches!(face.detail_level(), 1 | 2));
            lod_only += usize::from(only);
            if skip_material(
                &choose_material(
                    &f.materials[mesh.material],
                    mtds.get(&f.materials[mesh.material].mtd),
                )
                .mtd,
            ) {
                continue;
            }
            let mut kept_lod_only = false;
            for (fi, face) in mesh.face_sets.iter().enumerate() {
                let indices = face.triangles(mesh.vertices.len());
                if mesh.vertices.is_empty() || indices.is_empty() {
                    continue;
                }
                let primitive = doc["meshes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|m| m["primitives"].as_array().unwrap())
                    .find(|p| {
                        p["extras"]["source_mesh"] == mi && p["extras"]["source_face_set"] == fi
                    })
                    .unwrap_or_else(|| panic!("m{name} lost mesh {mi} face {fi}"));
                assert_eq!(primitive["extras"]["face_flags"], face.flags);
                assert_eq!(primitive["extras"]["source_node"], mesh.node);
                assert_eq!(
                    doc["accessors"][primitive["indices"].as_u64().unwrap() as usize]["count"],
                    indices.len()
                );
                if face.detail_level() > 0 {
                    let unique: std::collections::HashSet<_> = indices.iter().collect();
                    let position = primitive["attributes"]["POSITION"].as_u64().unwrap() as usize;
                    assert_eq!(doc["accessors"][position]["count"], unique.len());
                }
                if face.flags & flver::face_set_flags::MOTION_BLUR == 0 {
                    let level = face.detail_level();
                    let node = doc["scenes"][level]["nodes"][0].as_u64().unwrap() as usize;
                    let output_mesh = doc["nodes"][node]["mesh"].as_u64().unwrap() as usize;
                    assert!(
                        doc["meshes"][output_mesh]["primitives"]
                            .as_array()
                            .unwrap()
                            .contains(primitive)
                    );
                    kept_lod_only |= only;
                }
                exported += 1;
            }
            restored += usize::from(kept_lod_only);
        }
        println!(
            "m{name}: {} source meshes, {exported} retained face sets",
            f.meshes.len()
        );
    }
    assert_eq!(lod_only, 93);
    assert_eq!(restored, 93);
    let msb = sekiro_formats::msb::parse(
        &std::fs::read(raw.join("map/mapstudio/m11_00_00_00.msb")).unwrap(),
    )
    .unwrap();
    let instances = msb
        .parts
        .iter()
        .filter(|p| p.kind == sekiro_formats::msb::PartKind::MapPiece)
        .filter(|p| {
            msb.model(p)
                .is_some_and(|m| names.iter().any(|n| m.name == format!("m{n}")))
        })
        .count();
    assert_eq!(instances, 154);
}
