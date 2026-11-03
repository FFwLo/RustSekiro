//! Cache-backed checks for independent blend streams and Hirata's split probe containers.
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec3;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use sekiro_formats::{flver, tpf};

#[test]
fn packed_blends_do_not_displace_texture_uvs() {
    let path = sekiro_sim::cache::root()
        .join("raw/map/m11_00_00_00/m11_00_00_00_000003.mapbnd.d/m11_00_00_00_000003.flver");
    if !path.is_file() {
        return;
    }
    let model = flver::parse(&std::fs::read(path).unwrap()).unwrap();
    let vertices = &model.meshes[0].vertices;
    assert_eq!(vertices.uvs.len(), 2);
    assert_eq!(vertices.blends.len(), 6);
    for channel in &vertices.blends {
        assert_eq!(channel.len(), vertices.len());
    }
    for (i, blend) in vertices.blends.iter().enumerate() {
        println!(
            "blend {i}: red {}..{}",
            blend.iter().map(|b| b[0]).min().unwrap(),
            blend.iter().map(|b| b[0]).max().unwrap()
        );
    }
    let path = sekiro_sim::cache::root()
        .join("raw/map/m17_00_00_00/m17_00_00_00_000064.mapbnd.d/m17_00_00_00_000064.flver");
    if path.is_file() {
        let model = flver::parse(&std::fs::read(path).unwrap()).unwrap();
        let mesh = &model.meshes[0];
        let vertices = &mesh.vertices;
        let normals: Vec<[f32; 3]> = vertices
            .normals
            .iter()
            .map(|n| Vec3::new(-n[0], n[1], n[2]).normalize().to_array())
            .collect();
        let mut generated = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vertices
                    .positions
                    .iter()
                    .map(|p| [-p[0], p[1], p[2]])
                    .collect::<Vec<_>>(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vertices.uvs[0].clone())
            .with_inserted_indices(Indices::U32(mesh.lod0().unwrap().triangles(vertices.len())));
        generated.generate_tangents().unwrap();
        if let Some(VertexAttributeValues::Float32x4(tangents)) =
            generated.attribute(Mesh::ATTRIBUTE_TANGENT)
        {
            let mut aligned = 0;
            let mut sign = 0;
            for ((stored, normal), generated) in
                vertices.tangents[0].iter().zip(&normals).zip(tangents)
            {
                let w = if stored[3] < 0. { 1. } else { -1. };
                let tangent =
                    (Vec3::from(*normal).cross(Vec3::new(-stored[0], stored[1], stored[2])) * w)
                        .normalize();
                aligned += usize::from(
                    tangent.dot(Vec3::new(generated[0], generated[1], generated[2])) > 0.7,
                );
                sign += usize::from(w == generated[3]);
            }
            println!(
                "SnowGround tangent agreement {aligned}/{} sign {sign}",
                vertices.len()
            );
            assert!(aligned as f32 / vertices.len() as f32 > 0.9);
            assert!(sign as f32 / vertices.len() as f32 > 0.9);
        }
        for (i, blend) in model.meshes[0].vertices.blends.iter().enumerate() {
            println!(
                "SnowGround blend {i}: {}..{}",
                blend.iter().map(|b| b[0]).min().unwrap(),
                blend.iter().map(|b| b[0]).max().unwrap()
            );
        }
    }
}

#[test]
fn hirata_cubes_live_in_second_container() {
    let dir = sekiro_sim::cache::root().join("raw/map/m10_00_00_00");
    let path = dir.join("m10_00_00_00_envmap_00_second.tpf");
    if !path.is_file() {
        return;
    }
    let data = std::fs::read(path).unwrap();
    let textures = tpf::parse(&data).unwrap();
    let cubes: Vec<_> = textures
        .iter()
        .filter(|t| t.name.contains("GILM"))
        .collect();
    assert!(!cubes.is_empty());
    assert!(cubes.iter().all(|t| t.header().unwrap().cubemap));
    println!("Hirata: {} GILM cubes in second container", cubes.len());
    let data = std::fs::read(dir.join("m10_00_00_00_envmap_00.tpf")).unwrap();
    assert!(
        !tpf::parse(&data)
            .unwrap()
            .iter()
            .any(|t| t.header().unwrap().cubemap)
    );
}
