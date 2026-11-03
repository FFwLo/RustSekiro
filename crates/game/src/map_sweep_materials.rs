//! Audit every resident block, including pieces hidden in the normal pixel pass.
use super::*;
use std::collections::BTreeSet;

pub(super) fn snapshot(world: &mut World) -> Value {
    let sources: Vec<_> = world
        .query_filtered::<Entity, With<Mesh3d>>()
        .iter(world)
        .filter(|e| map_root(world, *e))
        .collect();
    let assets = world.resource::<AssetServer>();
    let mut blocks = BTreeMap::<String, BTreeMap<String, Value>>::new();
    for entity in sources {
        let Some(key) = map_material::sweep_key(world, entity) else {
            continue;
        };
        let mut root = entity;
        let owner = loop {
            if let Some(part) = world.get::<super::super::world::BlockPart>(root) {
                break Some(part);
            }
            let Some(parent) = world.get::<ChildOf>(root) else {
                break None;
            };
            root = parent.parent();
        };
        let block = owner.map_or_else(|| "primary".to_string(), |part| part.block.clone());
        let key = format!("{key:?}");
        let records = blocks.entry(block).or_default();
        if let Some(record) = records.get_mut(&key) {
            record["mesh_instances"] = json!(record["mesh_instances"].as_u64().unwrap() + 1);
            continue;
        }
        let Some(material) = map_material::sweep_base(world, entity) else {
            continue;
        };
        let textures: BTreeSet<_> = material
            .textures
            .iter()
            .map(|h| {
                let path = assets.get_path(h.id()).map(|p| p.to_string());
                let failed = matches!(assets.get_load_state(h.id()), Some(LoadState::Failed(_)));
                (format!("{:?}", h.id()), path, failed)
            })
            .collect();
        let source = world
            .get::<map_material::MaterialOrigin>(entity)
            .map(|p| p.0.to_string());
        records.insert(key, json!({"source":source,"textures":textures,"missing":material.missing,"pending":material.pending,"mesh_instances":1}));
    }
    json!(blocks)
}

pub(super) fn failed_requests(audit: &Value) -> usize {
    let mut failures = BTreeSet::new();
    for records in audit.as_object().into_iter().flat_map(|b| b.values()) {
        for record in records.as_object().into_iter().flat_map(|m| m.values()) {
            for texture in record["textures"].as_array().into_iter().flatten() {
                if texture[2] == true {
                    failures.insert(texture[1].to_string());
                }
            }
        }
    }
    failures.len()
}

pub(super) fn summary(audit: &Value) -> Value {
    let blocks: BTreeMap<_, _> = audit
        .as_object()
        .into_iter()
        .flat_map(|b| b.iter())
        .map(|(block, records)| {
            let materials: Vec<_> = records
                .as_object()
                .into_iter()
                .flat_map(|r| r.values())
                .collect();
            let failed = failed_requests(&json!({block:records}));
            let missing = materials.iter().filter(|r| r["missing"] == true).count();
            let meshes: u64 = materials.iter().filter_map(|r| r["mesh_instances"].as_u64()).sum();
            let untextured_meshes: u64 = materials.iter().filter(|r| r["missing"] == true).filter_map(|r| r["mesh_instances"].as_u64()).sum();
            (block, json!({"materials":materials.len(), "untextured_materials":missing, "mesh_instances":meshes, "untextured_mesh_instances":untextured_meshes, "failed_texture_requests":failed}))
        })
        .collect();
    json!(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_includes_hidden_neighbour_materials() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        for block in ["m11_00_00_00", "m11_01_00_00"] {
            let root = app
                .world_mut()
                .spawn((
                    super::super::super::world::BlockPart {
                        block: block.to_string(),
                        part_index: 0,
                    },
                    DrawGroups([0; 8]),
                    Visibility::Hidden,
                ))
                .id();
            for _ in 0..2 {
                app.world_mut().spawn((
                    Mesh3d::default(),
                    MeshMaterial3d(handle.clone()),
                    ChildOf(root),
                ));
            }
        }
        let audit = summary(&snapshot(app.world_mut()));
        for block in ["m11_00_00_00", "m11_01_00_00"] {
            assert_eq!(audit[block]["materials"], 1);
            assert_eq!(audit[block]["untextured_materials"], 1);
            assert_eq!(audit[block]["mesh_instances"], 2);
            assert_eq!(audit[block]["untextured_mesh_instances"], 2);
        }
    }

    #[test]
    fn failed_neighbour_declarations_fail_even_without_visible_pixels() {
        let mut audit = json!({
            "m11_00_00_00":{"primary":{"textures":[["base","primary/tex/base.dds",false]]}},
            "m11_01_00_00":{
                "hidden":{"textures":[["layer","wrong/tex/layer.dds",true]]},
                "occluded":{"textures":[["layer","wrong/tex/layer.dds",true]]}
            }
        });
        assert_eq!(failed_requests(&audit), 1);
        assert_eq!(
            summary(&audit)["m11_01_00_00"]["failed_texture_requests"],
            1
        );
        audit["m11_01_00_00"]["occluded"]["textures"][0][2] = json!(false);
        assert_eq!(failed_requests(&audit), 1);
        audit["m11_01_00_00"]["hidden"]["textures"][0][2] = json!(false);
        assert_eq!(failed_requests(&audit), 0);
    }
}
