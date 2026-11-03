//! Native material opt-outs and the camera's authored fade-probe radius.
//!
//! The shader's distance transform is verified; its CPU range producer is not yet recovered.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::Vec4;
use sekiro_formats::mtd::{self, ParamValue};

pub(super) struct CameraFade {
    directory: PathBuf,
    radius: f32,
    materials: HashMap<String, Vec4>,
    enabled: bool,
}

impl CameraFade {
    pub(super) fn load(map_dir: &Path, enabled: bool) -> Self {
        let cache = map_dir.parent().and_then(Path::parent).unwrap_or(map_dir);
        let radius = std::fs::read(cache.join("params/CameraParam.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|json| {
                json["rows"].as_array()?.iter().find(|row| row["id"] == 0)?
                    ["camCastSphereRadiusForFade"]
                    .as_f64()
            })
            .map(|value| value as f32)
            .filter(|value| value.is_finite() && *value > 0.)
            .unwrap_or(0.3);
        Self {
            directory: cache.join("raw/mtd/allmaterialbnd.mtdbnd.d"),
            radius,
            materials: HashMap::new(),
            enabled,
        }
    }

    pub(super) fn material(&mut self, name: &str) -> Vec4 {
        if !self.enabled {
            return Vec4::ZERO;
        }
        let Some((_, name)) = name.rsplit_once(" | ") else {
            return Vec4::ZERO;
        };
        if name.contains(['/', '\\']) || name.contains("..") {
            return Vec4::ZERO;
        }
        *self.materials.entry(name.to_owned()).or_insert_with(|| {
            let enabled = std::fs::read(self.directory.join(format!("{name}.mtd")))
                .ok()
                .and_then(|bytes| mtd::parse(&bytes).ok())
                .is_some_and(|m| {
                    m.param("g_GX_DisableCameraFade") == Some(&ParamValue::Bool(false))
                });
            // Approximation: vanish within the authored probe, restore over one probe radius.
            // Native shader coverage is distance * FC_CameraFadeInvRange - FC_CameraFadeEnd.
            Vec4::new(f32::from(enabled), 1., self.radius.recip(), 0.)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Quat, Vec3};

    fn hanging_blind(dir: &Path) -> sekiro_sim::collision::CollisionWorld {
        let layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("layout.json")).unwrap()).unwrap();
        let part = layout["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|part| part["name"] == "m006672_4001")
            .unwrap();
        let array = |value: &serde_json::Value, i: usize| value[i].as_f64().unwrap() as f32;
        let rotation = Quat::from_array(std::array::from_fn(|i| array(&part["rotation"], i)));
        let translation = Vec3::from_array(std::array::from_fn(|i| array(&part["translation"], i)));
        let scale = Vec3::from_array(std::array::from_fn(|i| array(&part["scale"], i)));
        let bytes = std::fs::read(dir.join("models/m006672.glb")).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let gltf: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        let bin = &bytes[28 + json_len..];
        let primitive = &gltf["meshes"][0]["primitives"][0];
        let accessor = |index: &serde_json::Value, width: usize| {
            let a = &gltf["accessors"][index.as_u64().unwrap() as usize];
            let view = &gltf["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
            let start = view["byteOffset"].as_u64().unwrap_or(0) as usize
                + a["byteOffset"].as_u64().unwrap_or(0) as usize;
            (
                start,
                view["byteStride"].as_u64().unwrap_or(width as u64) as usize,
                a["count"].as_u64().unwrap() as usize,
                a["componentType"].as_u64().unwrap(),
            )
        };
        let (start, stride, count, component) = accessor(&primitive["attributes"]["POSITION"], 12);
        assert_eq!(component, 5126);
        let vertices = (0..count)
            .map(|i| {
                let local = Vec3::from_array(std::array::from_fn(|axis| {
                    let offset = start + i * stride + axis * 4;
                    f32::from_le_bytes(bin[offset..offset + 4].try_into().unwrap())
                }));
                (rotation * (local * scale) + translation).to_array()
            })
            .collect();
        let (start, stride, count, component) = accessor(&primitive["indices"], 4);
        assert_eq!(component, 5125);
        let indices = (0..count)
            .map(|i| {
                let offset = start + i * stride;
                u32::from_le_bytes(bin[offset..offset + 4].try_into().unwrap())
            })
            .collect();
        sekiro_sim::collision::CollisionWorld::from_mesh(&sekiro_formats::hknp::CollisionMesh {
            vertices,
            indices,
            materials: vec![0; count / 3],
        })
    }

    #[test]
    fn hanging_blind_fades_without_adding_decoration_to_camera_collision() {
        let dir = sekiro_sim::cache::root().join("maps/m11_01_00_00");
        let mut fade = CameraFade::load(&dir, true);
        let wood = fade.material("m0 blind | M[Translucency]_e");
        assert_eq!(wood.x, 1.);
        assert_eq!(
            fade.material("m8 timber | M[m11_01]_Wood2_multi_NoDF").x,
            0.
        );
        assert_eq!(fade.material("m8 timber | ../../untrusted").x, 0.);
        assert!((fade.radius - 0.3).abs() < 1e-6);
        for (distance, expected) in [(0.15, 0.), (0.3, 0.), (0.45, 0.5), (0.6, 1.)] {
            let coverage = (distance * wood.z - wood.y).clamp(0., 1.);
            assert!((coverage - expected).abs() < 1e-5);
        }
        let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
        let focus = Vec3::new(
            102.35,
            world.floor(102.35, 239.8, 55.).unwrap() + 1.5,
            239.8,
        );
        let back = Vec3::new(-(std::f32::consts::PI / 6.).cos(), 0.5, 0.);
        assert!(world.raycast(focus, back, 4.8).is_none());
        let obstruction = hanging_blind(&dir).raycast(focus, back, 4.5).unwrap();
        let camera_distance = 4.5 - obstruction;
        assert!(camera_distance > 0. && camera_distance < 2. * fade.radius);
        assert!((camera_distance * wood.z - wood.y).clamp(0., 1.) < 1.);
        assert_eq!(
            CameraFade::load(&dir, false).material("m8 timber | M[m11_01]_Wood2_multi"),
            Vec4::ZERO
        );
    }
}
