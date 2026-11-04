//! Grass clumps are baked into shared meshes per authored terrain surface.
//! Surfaces stay children of their terrain part, including its visibility gates.
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result, ensure};
use bevy::gltf::{
    GltfExtras, GltfMaterialExtras, GltfMaterialName, GltfMesh, GltfMeshExtras, GltfNode,
};
use bevy::prelude::*;
use serde_json::Value;

use crate::map::MsbPart;
use crate::map::world::BlockPart;

#[derive(Component)]
struct GrassNoShadow;

/// A part root whose grass is in place (or that has none), so it is not visited again.
#[derive(Component)]
struct GrassAttached;

#[derive(Resource, Default)]
struct Grass {
    blocks: Vec<Block>,
}

struct Block {
    id: String,
    manifest: Value,
    prototypes: BTreeMap<String, Prototype>,
    /// Bound surface names by part index, in manifest order.
    parts: HashMap<usize, Vec<String>>,
    merged: HashMap<String, Vec<Primitive>>,
}

struct Prototype {
    gltf: Handle<Gltf>,
    path: String,
    roots: Vec<usize>,
    primitives: Option<Vec<Primitive>>,
    barrier: Option<Entity>,
}

#[derive(Clone)]
struct Primitive {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    extras: Option<GltfExtras>,
    mesh_extras: Option<GltfExtras>,
    material_extras: Option<GltfExtras>,
    material_name: Option<String>,
}

pub(crate) fn install(app: &mut App, dir: &Path, prefix: &str) -> Result<()> {
    if std::env::var_os("SEKIRO_NO_GRASS").is_some() {
        return Ok(());
    }
    let maps = dir.parent().context("map directory")?;
    let primary = dir.file_name().and_then(|s| s.to_str()).context("map ID")?;
    if !matches!(primary, "m11_00_00_00" | "m11_01_00_00") {
        return Ok(());
    }
    let mut grass = Grass::default();
    for id in ["m11_00_00_00", "m11_01_00_00"] {
        let path = maps.join(id).join("grass/grass.json");
        if !path.is_file() {
            continue;
        }
        let manifest: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            manifest["version"] == 1 && manifest["map"] == id,
            "unsupported grass manifest"
        );
        let block_prefix = crate::map::world::block_prefix(prefix, primary, id);
        let assets = app.world().resource::<AssetServer>();
        let mut prototypes = BTreeMap::new();
        for model in manifest["models"].as_array().context("grass models")? {
            let model = model.as_str().context("grass model name")?;
            let path = format!("{block_prefix}grass/models/{model}.glb");
            let bytes = std::fs::read(maps.join(id).join(format!("grass/models/{model}.glb")))?;
            ensure!(
                bytes.len() >= 20 && &bytes[..4] == b"glTF",
                "invalid grass GLB"
            );
            let length = u32::from_le_bytes(bytes[12..16].try_into()?) as usize;
            let doc: Value =
                serde_json::from_slice(bytes.get(20..20 + length).context("grass GLB JSON")?)?;
            let roots = doc["scenes"][0]["nodes"]
                .as_array()
                .context("grass scene roots")?
                .iter()
                .map(|n| n.as_u64().map(|n| n as usize).context("grass node index"))
                .collect::<Result<_>>()?;
            let gltf = assets
                .load_builder()
                .override_unapproved()
                .load(path.clone());
            prototypes.insert(
                model.to_owned(),
                Prototype {
                    gltf,
                    path,
                    roots,
                    primitives: None,
                    barrier: None,
                },
            );
        }
        let mut parts: HashMap<usize, Vec<String>> = HashMap::new();
        for binding in manifest["bindings"].as_array().into_iter().flatten() {
            let part = binding["part_index"].as_u64().context("grass part index")?;
            let surface = binding["surface"]
                .as_str()
                .context("grass binding surface")?;
            ensure!(
                manifest["surfaces"][surface]["model"]
                    .as_str()
                    .is_some_and(|m| prototypes.contains_key(m)),
                "grass surface {surface:?} has no model"
            );
            parts
                .entry(part as usize)
                .or_default()
                .push(surface.to_owned());
        }
        grass.blocks.push(Block {
            id: id.to_owned(),
            manifest,
            prototypes,
            parts,
            merged: HashMap::new(),
        });
    }
    app.insert_resource(grass)
        .add_message::<crate::map::world::BlockFreed>()
        .add_systems(PreUpdate, (release, instantiate).chain())
        .add_systems(PostUpdate, enforce_shadow_flags);
    Ok(())
}

fn transform(row: &Value) -> Result<Transform> {
    Ok(Transform {
        translation: Vec3::from_array(serde_json::from_value(row["translation"].clone())?),
        rotation: Quat::from_array(serde_json::from_value(row["rotation"].clone())?),
        scale: Vec3::from_array(serde_json::from_value(row["scale"].clone())?),
    })
}

fn plan(world: &World, prototype: &Prototype) -> Option<Vec<Primitive>> {
    let assets = world.resource::<AssetServer>();
    if !assets.is_loaded_with_dependencies(prototype.gltf.id()) {
        return None;
    }
    let gltf = world.resource::<Assets<Gltf>>().get(&prototype.gltf)?;
    let nodes = world.resource::<Assets<GltfNode>>();
    let meshes = world.resource::<Assets<GltfMesh>>();
    let mut output = Vec::new();
    #[allow(clippy::too_many_arguments)]
    fn visit(
        world: &World,
        gltf: &Gltf,
        nodes: &Assets<GltfNode>,
        meshes: &Assets<GltfMesh>,
        handle: &Handle<GltfNode>,
        parent: Mat4,
        path: &str,
        out: &mut Vec<Primitive>,
    ) -> Option<()> {
        let node = nodes.get(handle)?;
        let matrix = parent * node.transform.to_matrix();
        if let Some(handle) = &node.mesh {
            let mesh = meshes.get(handle)?;
            for p in &mesh.primitives {
                let index = gltf
                    .materials
                    .iter()
                    .position(|m| Some(m) == p.material.as_ref())?;
                let material: Handle<StandardMaterial> = world
                    .resource::<AssetServer>()
                    .load_builder()
                    .override_unapproved()
                    .load(format!("{path}#Material{index}/std"));
                if !world
                    .resource::<Assets<StandardMaterial>>()
                    .contains(&material)
                {
                    return None;
                }
                out.push(Primitive {
                    mesh: p.mesh.clone(),
                    material,
                    transform: Transform::from_matrix(matrix),
                    extras: p.extras.clone(),
                    mesh_extras: mesh.extras.clone(),
                    material_extras: p.material_extras.clone(),
                    material_name: p.material.as_ref().and_then(|m| {
                        gltf.named_materials
                            .iter()
                            .find(|(_, h)| h.id() == m.id())
                            .map(|(n, _)| n.to_string())
                    }),
                });
            }
        }
        for child in &node.children {
            visit(world, gltf, nodes, meshes, child, matrix, path, out)?;
        }
        Some(())
    }
    // Exported clumps use the first authored detail scene, just like LOD0 map geometry.
    for &root in &prototype.roots {
        visit(
            world,
            gltf,
            nodes,
            meshes,
            gltf.nodes.get(root)?,
            Mat4::IDENTITY,
            &prototype.path,
            &mut output,
        )?;
    }
    (!output.is_empty()).then_some(output)
}

/// Tagged part roots whose grass is not in place yet.
type Unattached = QueryState<
    (Entity, Option<&'static BlockPart>, Option<&'static MsbPart>),
    (Without<GrassAttached>, Or<(With<BlockPart>, With<MsbPart>)>),
>;

fn instantiate(world: &mut World, mut roots: Local<Unattached>) {
    let _timer = crate::perf_log::SystemTimer::start("grass_instantiate");
    let Some(grass) = world.get_resource::<Grass>() else {
        return;
    };
    // Connected blocks tag their roots with BlockPart; arena and explicitly disconnected
    // loaders tag theirs with MsbPart. Both name the MSB part the grass manifest binds.
    let parents: Vec<_> = roots
        .iter(world)
        .map(|(entity, block_part, msb_part)| {
            let (block, part_index) = block_part
                .map(|p| (&p.block, p.part_index))
                .or(msb_part.map(|p| (&p.block, p.part_index)))
                .expect("filtered to tagged roots");
            let block = grass.blocks.iter().position(|b| &b.id == block);
            (entity, block, part_index)
        })
        .collect();
    if parents.is_empty() {
        return;
    }
    let mut grass = world.remove_resource::<Grass>().expect("checked above");
    for (parent, block, part_index) in parents {
        if let Some(block) = block
            && !attach(world, &mut grass.blocks[block], parent, part_index)
        {
            continue;
        }
        world.entity_mut(parent).insert(GrassAttached);
    }
    world.insert_resource(grass);
}

/// Forget loading barriers under a freed block's despawned parts. Its merged clumps stay: merging
/// a block's grass again took 1.8 s in one frame when the block was requested again (#271).
fn release(mut grass: ResMut<Grass>, mut freed: MessageReader<crate::map::world::BlockFreed>) {
    for crate::map::world::BlockFreed(id) in freed.read() {
        for block in grass.blocks.iter_mut().filter(|b| &b.id == id) {
            for prototype in block.prototypes.values_mut() {
                // A loading barrier under a despawned part went with it.
                prototype.barrier = None;
            }
        }
    }
}

/// Spawns the clumps bound to `part_index` under `parent`, or returns false while a
/// prototype is still loading.
fn attach(world: &mut World, block: &mut Block, parent: Entity, part_index: usize) -> bool {
    let Some(surfaces) = block.parts.get(&part_index) else {
        return true;
    };
    for name in surfaces {
        let model = block.manifest["surfaces"][name.as_str()]["model"]
            .as_str()
            .unwrap();
        let prototype = block.prototypes.get_mut(model).unwrap();
        if prototype.primitives.is_none() {
            prototype.primitives = plan(world, prototype);
            if prototype.primitives.is_none() {
                // The ordinary map readiness walk waits for this nested scene as well.
                // It is removed before diagnostic geometry is enumerated.
                if prototype.barrier.is_none() {
                    let handle = world
                        .resource::<AssetServer>()
                        .load_builder()
                        .override_unapproved()
                        .load(GltfAssetLabel::Scene(0).from_asset(prototype.path.clone()));
                    prototype.barrier = Some(
                        world
                            .spawn((
                                WorldAssetRoot(handle),
                                Transform::IDENTITY,
                                Visibility::Hidden,
                                ChildOf(parent),
                            ))
                            .id(),
                    );
                }
                return false;
            }
            if let Some(barrier) = prototype.barrier.take() {
                world.despawn(barrier);
            }
        }
    }
    for name in surfaces {
        let surface = &block.manifest["surfaces"][name.as_str()];
        let prototype = &block.prototypes[surface["model"].as_str().unwrap()];
        let cast_shadow = surface["params"]["castShadow"].as_u64().unwrap_or(0) != 0;
        if !block.merged.contains_key(name) {
            let _timer = crate::perf_log::SystemTimer::start("grass merge");
            let mut baked = Vec::new();
            for primitive in prototype.primitives.as_ref().unwrap() {
                let opaque = world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&primitive.material)
                    .is_some_and(|m| crate::map::opaque(m.alpha_mode));
                if !opaque {
                    // Transparent clumps retain their individual depth-sort positions.
                    for row in surface["placements"].as_array().into_iter().flatten() {
                        let mut instance = primitive.clone();
                        instance.transform = transform(row)
                            .expect("validated grass placement")
                            .mul_transform(primitive.transform);
                        baked.push(instance);
                    }
                    continue;
                }
                let Some(source) = world.resource::<Assets<Mesh>>().get(&primitive.mesh) else {
                    return false;
                };
                let placements =
                    surface["placements"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|row| {
                            transform(row)
                                .expect("validated grass placement")
                                .mul_transform(primitive.transform)
                        });
                let Some(mut mesh) = merge_placements(source, placements) else {
                    continue;
                };
                mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
                let mut primitive = primitive.clone();
                primitive.mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
                primitive.transform = Transform::IDENTITY;
                baked.push(primitive);
            }
            block.merged.insert(name.clone(), baked);
        }
        for primitive in &block.merged[name] {
            let mut entity = world.spawn((
                Mesh3d(primitive.mesh.clone()),
                MeshMaterial3d(primitive.material.clone()),
                primitive.transform,
                Visibility::Inherited,
                ChildOf(parent),
            ));
            if !cast_shadow {
                entity.insert((GrassNoShadow, bevy::light::NotShadowCaster));
            }
            if let Some(extras) = &primitive.extras {
                entity.insert(extras.clone());
            }
            if let Some(extras) = &primitive.mesh_extras {
                entity.insert(GltfMeshExtras {
                    value: extras.value.clone(),
                });
            }
            if let Some(extras) = &primitive.material_extras {
                entity.insert(GltfMaterialExtras {
                    value: extras.value.clone(),
                });
            }
            if let Some(name) = &primitive.material_name {
                entity.insert(GltfMaterialName(name.clone()));
            }
        }
    }
    true
}

/// A surface is tied to one terrain part, so baking preserves its exact visibility gate.
/// Use u32 indices before concatenation, including prototypes with more than 65535 vertices
/// after placement. Vertex attributes (normals, tangents, colours and all UVs) stay intact.
fn merge_placements(source: &Mesh, placements: impl Iterator<Item = Transform>) -> Option<Mesh> {
    crate::map::merge_meshes(placements.map(|placement| (source, placement)))
}

fn enforce_shadow_flags(
    mut commands: Commands,
    meshes: Query<Entity, (With<GrassNoShadow>, Without<bevy::light::NotShadowCaster>)>,
) {
    // The `--shadow-casters` budget can restore map casters. Native type flags still win.
    for entity in &meshes {
        commands.entity(entity).insert(bevy::light::NotShadowCaster);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn clumps_share_gpu_assets_and_inherit_each_source_part_once() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Mesh::from(Triangle3d::new(Vec3::ZERO, Vec3::X, Vec3::Y)));
        world.insert_resource(meshes);
        let mut materials = Assets::<StandardMaterial>::default();
        let material = materials.add(StandardMaterial::default());
        world.insert_resource(materials);
        let mut roots: Vec<_> = (0..2)
            .map(|_| {
                world
                    .spawn((
                        BlockPart {
                            block: "test".into(),
                            part_index: 4,
                        },
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id()
            })
            .collect();
        // Legacy loaders tag their roots with the MSB part instead.
        roots.push(
            world
                .spawn((
                    MsbPart {
                        block: "test".into(),
                        part_index: 4,
                    },
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id(),
        );
        let untagged = world.spawn(Transform::default()).id();
        let prototype = Prototype {
            gltf: Handle::default(),
            path: String::new(),
            roots: vec![],
            barrier: None,
            primitives: Some(vec![Primitive {
                mesh,
                material,
                transform: Transform::IDENTITY,
                extras: None,
                mesh_extras: None,
                material_extras: None,
                material_name: None,
            }]),
        };
        let manifest = json!({"bindings":[{"part_index":4,"surface":"ground"}],"surfaces":{"ground":{"model":"tuft","params":{"castShadow":0},"placements":[{"translation":[1.,2.,3.],"rotation":[0.,0.,0.,1.],"scale":[1.,1.,1.]},{"translation":[4.,5.,6.],"rotation":[0.,0.,0.,1.],"scale":[1.,1.,1.]}]}}});
        world.insert_resource(Grass {
            blocks: vec![Block {
                id: "test".into(),
                manifest,
                prototypes: BTreeMap::from([("tuft".into(), prototype)]),
                parts: HashMap::from([(4, vec!["ground".into()])]),
                merged: HashMap::new(),
            }],
        });
        world.run_system_once(instantiate).unwrap();
        world.run_system_once(instantiate).unwrap();
        let children: Vec<_> = world
            .query::<(
                &ChildOf,
                &Mesh3d,
                &MeshMaterial3d<StandardMaterial>,
                &Visibility,
                &Transform,
            )>()
            .iter(&world)
            .collect();
        assert_eq!(children.len(), 3);
        let parents: HashSet<_> = children.iter().map(|c| c.0.parent()).collect();
        assert_eq!(parents, roots.iter().copied().collect());
        assert!(
            children
                .iter()
                .all(|c| *c.3 == Visibility::Inherited && *c.4 == Transform::IDENTITY)
        );
        let merged = world
            .resource::<Assets<Mesh>>()
            .get(&children[0].1.0)
            .unwrap();
        assert_eq!(merged.count_vertices(), 6);
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
            merged.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("positions");
        };
        assert_eq!(positions[0], [1., 2., 3.]);
        assert_eq!(positions[3], [4., 5., 6.]);
        assert_eq!(
            merged.indices().unwrap().iter().collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5]
        );
        assert!(
            children
                .iter()
                .all(|c| c.1.0.id() == children[0].1.0.id() && c.2.0.id() == children[0].2.0.id())
        );
        assert!(
            roots
                .iter()
                .all(|&r| world.get::<GrassAttached>(r).is_some())
        );
        assert!(world.get::<GrassAttached>(untagged).is_none());
        world.despawn(roots[0]);
        assert_eq!(world.query::<&Mesh3d>().iter(&world).count(), 2);
    }

    /// MsbPart tags from the disconnected spawn path bind exactly the parts the replaced
    /// scene path + pose matcher found, for every placed piece and object row.
    #[test]
    fn msb_part_tags_bind_the_parts_scene_and_pose_matching_found() {
        let maps = sekiro_sim::cache::root().join("maps");
        for id in ["m11_00_00_00", "m11_01_00_00"] {
            let read = |name: &str| {
                let bytes = std::fs::read(maps.join(id).join(name)).ok()?;
                Some(serde_json::from_slice::<Value>(&bytes).unwrap())
            };
            let (Some(layout), Some(manifest)) = (read("layout.json"), read("grass/grass.json"))
            else {
                eprintln!("{id}: skipped (not extracted)");
                continue;
            };
            let bindings: Vec<_> = manifest["bindings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    (
                        b["parent_model"]
                            .as_str()
                            .map(|m| format!("models/{m}.glb#")),
                        transform(&b["parent_transform"])
                            .ok()
                            .map(|t| t.to_matrix()),
                        b["part_index"].as_u64().unwrap() as usize,
                    )
                })
                .collect();
            let bound: HashSet<_> = bindings.iter().map(|b| b.2).collect();
            // The replaced matcher: the first binding naming the root's scene at its pose.
            let matched = |scene: &str, pose: Mat4| {
                bindings
                    .iter()
                    .find(|(model, expected, _)| {
                        model.as_ref().is_some_and(|m| scene.contains(m.as_str()))
                            && expected.is_some_and(|e| e.abs_diff_eq(pose, 1e-5))
                    })
                    .map(|b| b.2)
            };
            let mut attached = HashSet::new();
            for row in ["pieces", "objects"]
                .iter()
                .flat_map(|k| layout[*k].as_array().unwrap())
            {
                // The rows the disconnected spawn path places as scene roots.
                let (Some(pose), Some(glb)) = (
                    crate::map::row_transform(row),
                    row["model"]
                        .as_str()
                        .and_then(|m| layout["models"][m]["glb"].as_str()),
                ) else {
                    continue;
                };
                let part = MsbPart::from_row(id, row).expect("v2 rows record their part");
                let tagged = bound.contains(&part.part_index).then_some(part.part_index);
                let scene = format!("{glb}#Scene0");
                assert_eq!(
                    tagged,
                    matched(&scene, pose.to_matrix()),
                    "{id} {}",
                    row["name"]
                );
                attached.extend(tagged);
            }
            assert_eq!(attached, bound, "{id}: every bound part is placed");
        }
    }
}
