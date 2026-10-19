//! The maps' authored grass: the clumps tools/map_grass.py lays on each map piece's painted
//! grass surface (its `.grass` table and GrassTypeParam rows) -> extracted/map_grass.json, drawn
//! round the loaded arena with the GrassTypeParam clump models (extracted/objmesh/o007xxx.json,
//! their textures alpha-tested, both sides, X mirrored as every FromSoftware model).
//! After sekiro-rs (MIT / Apache-2.0, AKJama) crates/game/src/map_grass.rs and
//! crates/extract/src/maps/grass.rs (the castle's 6,813 clumps and the Outskirts' 7,740 match).
//! gap: the wind sway (windAmplitude / windCycle), the colour shades, the LOD ranges
//! (GrassLodRangeParam) and the clumps' shadows (castShadow) are not done.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::map::MapLighting;

pub struct MapGrassPlugin;

impl Plugin for MapGrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, place.run_if(resource_exists::<MapLighting>));
    }
}

#[derive(Component)]
struct MapGrass;

/// Clumps farther than this from the arena's origin are left out (the exported arenas reach
/// about 60 m).
const REACH: f32 = 90.0;

/// (model, arena transform) of the loaded map's clumps round the arena.
fn arena_grass(source_map: &str, origin: Vec3, arena_yaw: f32) -> Vec<(String, Transform)> {
    static ALL: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| std::fs::read_to_string(crate::paths::extracted().join("map_grass.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    let mut out = Vec::new();
    for g in all[source_map].as_array().into_iter().flatten() {
        let Some(model) = g["model"].as_str() else { continue };
        let c: Vec<f32> = g["clumps"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
        for k in c.chunks_exact(9) {
            let pos = crate::fog_wall::map_to_arena(origin, arena_yaw, Vec3::new(k[0], k[1], k[2]));
            if (pos - crate::map::frame().offset).xz().length() > REACH {
                continue;
            }
            // The X mirror flips a turn's Y and Z parts; the arena's yaw goes on top.
            let rot = Quat::from_rotation_y(-arena_yaw) * Quat::from_xyzw(k[3], -k[4], -k[5], k[6]).normalize();
            out.push((model.to_string(), Transform::from_translation(pos).with_rotation(rot).with_scale(Vec3::new(k[7], k[8], k[7]))));
        }
    }
    out
}

/// A grass material's albedo texture (map_grass_albedo.json).
fn albedo_of(mtd: &str) -> Option<String> {
    static ALL: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| std::fs::read_to_string(crate::paths::extracted().join("map_grass_albedo.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    all[mtd].as_str().map(str::to_string)
}

/// A clump model's meshes with their alpha-tested materials (None when it is not exported).
fn model_parts(name: &str, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, assets: &AssetServer) -> Option<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>> {
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join(format!("objmesh/{name}.json"))).ok()?).ok()?;
    let floats = |v: &serde_json::Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect::<Vec<f32>>()).unwrap_or_default();
    let mut by_tex: HashMap<String, Handle<StandardMaterial>> = HashMap::new();
    let mut parts = Vec::new();
    for m in json["meshes"].as_array().into_iter().flatten() {
        let (pos, nrm, uv) = (floats(&m["pos"]), floats(&m["normal_vec"]), floats(&m["uv"]));
        let idx: Vec<u32> = m["indices"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|i| i as u32)).collect()).unwrap_or_default();
        let n = pos.len() / 3;
        if n == 0 || idx.len() < 3 {
            continue;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect::<Vec<[f32; 3]>>());
        if nrm.len() == pos.len() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect::<Vec<[f32; 3]>>());
        }
        if uv.len() / 2 == n {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.chunks(2).map(|p| [p[0], p[1]]).collect::<Vec<[f32; 2]>>());
        }
        mesh.insert_indices(Indices::U32(idx));
        // The FLVERs leave the texture to the material (MTD AlbedoMap path, map_grass.py
        // map_grass_albedo.json); a mesh with none (the amsn_light glow cards) is left out.
        let mtd = m["mtd"].as_str().unwrap_or("").rsplit(['\\', '/']).next().unwrap_or("");
        let albedo = match m["albedo"].as_str().filter(|a| !a.is_empty()) {
            Some(a) => a.to_string(),
            None => match albedo_of(mtd) {
                Some(a) => a,
                None => continue,
            },
        };
        let mat = by_tex
            .entry(albedo.clone())
            .or_insert_with(|| {
                let tex = format!("tex/{albedo}.dds");
                let texture = (!albedo.is_empty() && crate::paths::extracted().join(&tex).is_file()).then(|| assets.load(tex));
                mats.add(StandardMaterial { base_color_texture: texture, alpha_mode: AlphaMode::Mask(0.5), double_sided: true, cull_mode: None, perceptual_roughness: 0.9, ..default() })
            })
            .clone();
        parts.push((meshes.add(mesh), mat));
    }
    Some(parts)
}

/// Lays the loaded map's grass (again after a map switch: the stem changes).
#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    lighting: Res<MapLighting>,
    old: Query<Entity, With<MapGrass>>,
    mut shown: Local<Option<String>>,
    mut models: Local<HashMap<String, Option<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    if shown.as_deref() == Some(lighting.stem.as_str()) {
        return;
    }
    *shown = Some(lighting.stem.clone());
    for e in &old {
        commands.entity(e).despawn();
    }
    if std::env::var("SHINOBI_NO_MAP_GRASS").is_ok() {
        return;
    }
    let Some(origin) = lighting.map_origin else { return };
    let clumps = arena_grass(&lighting.source_map, origin, lighting.arena_yaw);
    let mut drawn = 0;
    for (model, at) in &clumps {
        let parts = models.entry(model.clone()).or_insert_with(|| model_parts(model, &mut meshes, &mut mats, &assets));
        let Some(parts) = parts else { continue };
        drawn += 1;
        for (mesh, mat) in parts.iter() {
            commands.spawn((MapGrass, crate::map::MapPiece, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *at, Visibility::default(), bevy::light::NotShadowCaster));
        }
    }
    info!("grass: {drawn} of {} clumps of {} drawn", clumps.len(), lighting.source_map);
}

#[cfg(test)]
mod tests {
    /// sekiro-rs's census (the Outskirts 7,740 clumps of 11 models, the castle 6,813 of 7), and a
    /// clump at the export origin sits at the arena's centre.
    #[test]
    fn the_maps_grass_matches_its_census() {
        let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("map_grass.json")) else {
            eprintln!("no map_grass.json: skipped");
            return;
        };
        let all: serde_json::Value = serde_json::from_str(&text).unwrap();
        let count = |m: &str| all[m].as_array().unwrap().iter().map(|g| g["clumps"].as_array().unwrap().len() / 9).sum::<usize>();
        let models = |m: &str| all[m].as_array().unwrap().iter().map(|g| g["model"].as_str().unwrap()).collect::<std::collections::HashSet<_>>().len();
        assert_eq!((count("m11_00_00_00"), models("m11_00_00_00")), (7740, 11));
        assert_eq!((count("m11_01_00_00"), models("m11_01_00_00")), (6813, 7));
        let c = &all["m11_01_00_00"][0]["clumps"];
        let origin = bevy::prelude::Vec3::new(c[0].as_f64().unwrap() as f32, c[1].as_f64().unwrap() as f32, c[2].as_f64().unwrap() as f32);
        let placed = super::arena_grass("m11_01_00_00", origin, 0.4);
        assert!(placed.iter().any(|(_, t)| t.translation.length() < 1e-3));
    }
}
