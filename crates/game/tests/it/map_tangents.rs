//! The map export reconstructs glTF U tangents from Sekiro's stored UV bitangents.
//! This checks them against the MikkTSpace tangents Bevy would generate from the same
//! positions, normals and UVs: the directions must agree and, above all, the handedness sign.
//! Skips when `cache/maps/m11_00_00_00` has not been exported.

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use serde_json::Value;

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

impl Glb {
    fn read(path: &PathBuf) -> Self {
        let data = std::fs::read(path).unwrap();
        let json_len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let json = serde_json::from_slice(&data[20..20 + json_len]).unwrap();
        let bin_start = 20 + json_len + 8;
        Self {
            json,
            bin: data[bin_start..].to_vec(),
        }
    }

    /// Floats of an accessor, `n` per element.
    fn floats(&self, accessor: usize, n: usize) -> Vec<Vec<f32>> {
        let a = &self.json["accessors"][accessor];
        let view = &self.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap() as usize;
        (0..count)
            .map(|i| {
                (0..n)
                    .map(|k| {
                        let o = off + (i * n + k) * 4;
                        f32::from_le_bytes(self.bin[o..o + 4].try_into().unwrap())
                    })
                    .collect()
            })
            .collect()
    }

    fn indices(&self, accessor: usize) -> Vec<u32> {
        let a = &self.json["accessors"][accessor];
        let view = &self.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap() as usize;
        (0..count)
            .map(|i| u32::from_le_bytes(self.bin[off + i * 4..off + i * 4 + 4].try_into().unwrap()))
            .collect()
    }
}

#[test]
fn exported_tangents_match_mikktspace() {
    let dir = sekiro_sim::cache::root().join("maps/m11_00_00_00/models");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipping: {} not exported", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "glb"))
        .collect();
    paths.sort();
    let (mut vertices, mut same_sign, mut aligned, mut unreferenced) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut all_vertices, mut all_sign, mut all_aligned) = (0usize, 0usize, 0usize);
    for path in paths.iter().step_by(7) {
        let glb = Glb::read(path);
        for mesh in glb.json["meshes"].as_array().into_iter().flatten() {
            for prim in mesh["primitives"].as_array().into_iter().flatten() {
                let at = &prim["attributes"];
                let (Some(t), Some(uv)) = (at["TANGENT"].as_u64(), at["TEXCOORD_0"].as_u64())
                else {
                    continue;
                };
                let pos = glb.floats(at["POSITION"].as_u64().unwrap() as usize, 3);
                let nrm = glb.floats(at["NORMAL"].as_u64().unwrap() as usize, 3);
                let uvs = glb.floats(uv as usize, 2);
                let ours = glb.floats(t as usize, 4);
                let idx = glb.indices(prim["indices"].as_u64().unwrap() as usize);
                // MikkTSpace writes tangents only for vertices used by this face set.
                // Authored LODs may share a buffer; untouched vertices have zero generated
                // tangents, so comparing them measures buffer sparsity, not handedness.
                let mut referenced = vec![false; ours.len()];
                for &index in &idx {
                    referenced[index as usize] = true;
                }
                let to3 =
                    |v: &Vec<Vec<f32>>| v.iter().map(|x| [x[0], x[1], x[2]]).collect::<Vec<_>>();
                let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, to3(&pos))
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, to3(&nrm))
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_UV_0,
                        uvs.iter().map(|x| [x[0], x[1]]).collect::<Vec<_>>(),
                    )
                    .with_inserted_indices(Indices::U32(idx));
                if m.generate_tangents().is_err() {
                    continue;
                }
                let Some(VertexAttributeValues::Float32x4(bevy)) =
                    m.attribute(Mesh::ATTRIBUTE_TANGENT)
                else {
                    continue;
                };
                for ((a, b), used) in ours.iter().zip(bevy).zip(referenced) {
                    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                    all_vertices += 1;
                    all_sign += (a[3].signum() == b[3].signum()) as usize;
                    all_aligned += (dot > 0.7) as usize;
                    if !used {
                        unreferenced += 1;
                        continue;
                    }
                    vertices += 1;
                    same_sign += (a[3].signum() == b[3].signum()) as usize;
                    aligned += (dot > 0.7) as usize;
                }
            }
        }
    }
    if vertices == 0 {
        eprintln!("skipping: no exported tangents");
        return;
    }
    let sign = same_sign as f32 / vertices as f32;
    let dir = aligned as f32 / vertices as f32;
    eprintln!(
        "{vertices} referenced vertices ({unreferenced} unused): handedness agrees {sign:.3}, direction within 45 deg {dir:.3}"
    );
    eprintln!(
        "old all-buffer comparison: handedness {:.3}, direction {:.3}",
        all_sign as f32 / all_vertices as f32,
        all_aligned as f32 / all_vertices as f32
    );
    assert!(sign > 0.9, "handedness agrees for only {sign:.3}");
    assert!(dir > 0.8, "directions agree for only {dir:.3}");
}
