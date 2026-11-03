//! Static capture instancing from the same loaded GLTF subassets as the scene loader.
//! Avoids repeatedly reflecting nested WorldAsset graphs for a fixed camera survey.
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::gltf::{
    GltfExtras, GltfMaterialExtras, GltfMaterialName, GltfMesh, GltfMeshExtras, GltfMeshName,
    GltfNode,
};
use bevy::prelude::*;
use serde_json::Value;

#[derive(Component)]
pub(super) struct CaptureScene {
    pub gltf: Handle<Gltf>,
    pub level: usize,
    pub roots: Vec<usize>,
}

#[derive(Clone)]
struct Primitive {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    extras: Option<GltfExtras>,
    mesh_extras: Option<GltfExtras>,
    material_extras: Option<GltfExtras>,
    material_name: Option<String>,
    mesh_name: String,
    name: String,
}
#[derive(Clone)]
struct Node {
    parent: Option<usize>,
    transform: Transform,
    name: String,
    primitives: Vec<Primitive>,
    world_from_scene: Mat4,
}
#[derive(Resource, Default)]
struct Plans(HashMap<(AssetId<Gltf>, usize), Vec<Node>>);

pub(super) fn install(app: &mut App) {
    app.init_resource::<Plans>().add_systems(
        PreUpdate,
        instantiate.before(crate::map_fog_surface::FogConversion),
    );
}

pub(super) fn scene_roots(path: &Path) -> Result<[Vec<usize>; 3]> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0; 20];
    file.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header[12..16].try_into()?) as usize;
    anyhow::ensure!(
        size <= 64 * 1024 * 1024,
        "GLB metadata exceeds capture limit"
    );
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes)?;
    let doc: Value = serde_json::from_slice(&bytes)?;
    Ok(std::array::from_fn(|i| {
        doc["scenes"][i]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect()
    }))
}

fn material_label(
    materials: &[Handle<bevy::gltf::GltfMaterial>],
    material: Option<&Handle<bevy::gltf::GltfMaterial>>,
    inverted: bool,
) -> Result<String> {
    if let Some(material) = material {
        let index = materials
            .iter()
            .position(|m| m.id() == material.id())
            .context("capture material absent from GLTF table")?;
        Ok(GltfAssetLabel::Material {
            index,
            is_scale_inverted: inverted,
        }
        .to_string())
    } else {
        Ok(GltfAssetLabel::DefaultMaterial.to_string())
    }
}

fn plan(world: &World, scene: &CaptureScene) -> Result<Vec<Node>> {
    let gltfs = world.resource::<Assets<Gltf>>();
    let assets = world.resource::<AssetServer>();
    let gltf = gltfs.get(&scene.gltf).context("capture GLTF not ready")?;
    let path = assets
        .get_path(scene.gltf.id())
        .context("capture GLTF has no path")?;
    let mut result = Vec::new();
    fn visit(
        world: &World,
        gltf: &Gltf,
        handle: &Handle<GltfNode>,
        parent: Option<usize>,
        path: &bevy::asset::AssetPath<'_>,
        result: &mut Vec<Node>,
    ) -> Result<()> {
        let node = world
            .resource::<Assets<GltfNode>>()
            .get(handle)
            .context("capture node not ready")?;
        anyhow::ensure!(
            node.skin.is_none(),
            "capture requires static exported map geometry"
        );
        let index = result.len();
        let world_from_scene = parent.map_or(Mat4::IDENTITY, |i| result[i].world_from_scene)
            * node.transform.to_matrix();
        let mut primitives = Vec::new();
        if let Some(mesh) = &node.mesh {
            let mesh = world
                .resource::<Assets<GltfMesh>>()
                .get(mesh)
                .context("capture mesh not ready")?;
            for primitive in &mesh.primitives {
                let assets = world.resource::<AssetServer>();
                // GLTF subasset handles need not have a registered AssetServer path.
                // Their position in the GLTF material table is the authoritative label.
                let label = material_label(
                    &gltf.materials,
                    primitive.material.as_ref(),
                    world_from_scene.determinant() < 0.,
                )?;
                let material: Handle<StandardMaterial> = super::load_map_asset(
                    assets,
                    path.without_label()
                        .clone_owned()
                        .with_label(format!("{label}/std")),
                    true,
                );
                anyhow::ensure!(
                    world
                        .resource::<Assets<StandardMaterial>>()
                        .contains(&material),
                    "capture standard material not ready"
                );
                let material_name = primitive.material.as_ref().and_then(|m| {
                    gltf.named_materials
                        .iter()
                        .find(|(_, h)| h.id() == m.id())
                        .map(|(name, _)| name.to_string())
                });
                primitives.push(Primitive {
                    mesh: primitive.mesh.clone(),
                    material,
                    extras: primitive.extras.clone(),
                    mesh_extras: mesh.extras.clone(),
                    material_extras: primitive.material_extras.clone(),
                    material_name,
                    mesh_name: mesh.name.clone(),
                    name: primitive.name.clone(),
                });
            }
        }
        result.push(Node {
            parent,
            transform: node.transform,
            name: node.name.clone(),
            primitives,
            world_from_scene,
        });
        for child in &node.children {
            visit(world, gltf, child, Some(index), path, result)?;
        }
        Ok(())
    }
    for &index in &scene.roots {
        visit(
            world,
            gltf,
            gltf.nodes.get(index).context("capture scene node index")?,
            None,
            &path,
            &mut result,
        )?;
    }
    Ok(result)
}

fn instantiate(world: &mut World) {
    let scenes: Vec<_> = world
        .query::<(Entity, &CaptureScene)>()
        .iter(world)
        .map(|(e, s)| {
            (
                e,
                CaptureScene {
                    gltf: s.gltf.clone(),
                    level: s.level,
                    roots: s.roots.clone(),
                },
            )
        })
        .collect();
    for (entity, scene) in scenes {
        if !world
            .resource::<AssetServer>()
            .is_loaded_with_dependencies(scene.gltf.id())
        {
            continue;
        }
        let key = (scene.gltf.id(), scene.level);
        if !world.resource::<Plans>().0.contains_key(&key) {
            let Ok(plan) = plan(world, &scene) else {
                continue;
            };
            world.resource_mut::<Plans>().0.insert(key, plan);
        }
        let plan = world.resource::<Plans>().0[&key].clone();
        let mut parents = Vec::with_capacity(plan.len());
        for node in plan {
            let parent = node.parent.map_or(entity, |i| parents[i]);
            let node_entity = world
                .spawn((
                    node.transform,
                    Visibility::Inherited,
                    ChildOf(parent),
                    Name::new(node.name),
                ))
                .id();
            parents.push(node_entity);
            for primitive in node.primitives {
                let mut mesh = world.spawn((
                    Mesh3d(primitive.mesh),
                    MeshMaterial3d(primitive.material),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(node_entity),
                    GltfMeshName(primitive.mesh_name),
                    Name::new(primitive.name),
                ));
                if let Some(name) = primitive.material_name {
                    mesh.insert(GltfMaterialName(name));
                }
                if let Some(extras) = primitive.extras {
                    mesh.insert(extras);
                }
                if let Some(extras) = primitive.mesh_extras {
                    mesh.insert(GltfMeshExtras {
                        value: extras.value,
                    });
                }
                if let Some(extras) = primitive.material_extras {
                    mesh.insert(GltfMaterialExtras {
                        value: extras.value,
                    });
                }
            }
        }
        world.entity_mut(entity).remove::<CaptureScene>();
        let mut pending = world.resource_mut::<super::Pending>();
        if pending.ready_roots.insert(entity) {
            pending.ready += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unregistered_subasset_handles_keep_their_material_table_label() {
        let mut assets = Assets::<bevy::gltf::GltfMaterial>::default();
        let first = assets.add(bevy::gltf::GltfMaterial::default());
        let second = assets.add(bevy::gltf::GltfMaterial::default());
        let materials = [first, second.clone()];
        assert_eq!(
            material_label(&materials, Some(&second), false).unwrap(),
            "Material1"
        );
        assert_eq!(
            material_label(&materials, Some(&second), true).unwrap(),
            "Material1 (inverted)"
        );
        let absent = assets.add(bevy::gltf::GltfMaterial::default());
        assert!(material_label(&materials, Some(&absent), false).is_err());
    }
}
