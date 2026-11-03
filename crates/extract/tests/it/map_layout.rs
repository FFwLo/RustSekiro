#[path = "../../src/maps/layout.rs"]
mod layout;

use sekiro_formats::msb;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

// Authored MSBS scalars are f32; JSON reparsing can differ by one f64 ULP.
// Require exact authored precision, not a tolerance that can conceal an MSB change.
fn same_source_values(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            (a.as_f64().unwrap() as f32).to_bits() == (b.as_f64().unwrap() as f32).to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_source_values(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same_source_values(a, b)))
        }
        _ => actual == expected,
    }
}

// Matrix decomposition can round differently across glam versions.
// Only derived transforms get this tolerance; source_transform stays exact above.
fn same_derived_transform(actual: &Value, expected: &Value) -> bool {
    let (Some(actual), Some(expected)) = (actual.as_array(), expected.as_array()) else {
        return false;
    };
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(a, b)| {
            let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) else {
                return false;
            };
            let tolerance = 8.0 * f64::from(f32::EPSILON) * a.abs().max(b.abs()).max(1.0);
            (a - b).abs() <= tolerance
        })
}

#[test]
fn decomposition_rounding_does_not_relax_authored_precision() {
    let authored = json!([0.8_f32]);
    let rounded = json!([f32::from_bits(0.8_f32.to_bits() + 1)]);
    assert!(!same_source_values(&authored, &rounded));
    assert!(same_derived_transform(&authored, &rounded));
    assert!(!same_derived_transform(&authored, &json!([0.80001])));
}

#[test]
fn connection_inventory_matching_preserves_retail_signed_fields() {
    let inventory: Vec<_> = [
        "m11_00_00_00",
        "m11_01_00_00",
        "m11_02_00_00",
        "m13_00_00_00",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for (bytes, expected) in [
        ([11, 1, 255, 255], vec!["m11_01_00_00"]),
        ([11, 255, 255, 255], vec!["m11_00_00_00"]),
        ([255; 4], vec![]),
        ([13, 0, 0, 0], vec!["m13_00_00_00"]),
        ([12, 0, 0, 0], vec![]),
    ] {
        assert_eq!(layout::target_blocks(bytes, &inventory), expected);
    }
}

#[test]
fn installed_layout_preserves_all_parts_and_matches_job_a_audit() {
    let cache = sekiro_formats::cache::root();
    if !cache.join("raw/map/mapstudio/m11_00_00_00.msb").exists() {
        eprintln!("installed-map fixture unavailable");
        return;
    }
    let inventory = layout::map_inventory(&cache.join("raw")).unwrap();
    for (id, expected) in [
        ("m11_00_00_00", [4498, 1509, 144, 10, 166, 19, 44, 99]),
        ("m11_01_00_00", [3636, 1674, 284, 11, 221, 10, 30, 298]),
    ] {
        let msb =
            msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{id}.msb"))).unwrap())
                .unwrap();
        let mut exported =
            json!({"pieces": [], "objects": [], "enemies": [], "players": [], "collisions": []});
        layout::extend(
            &mut exported,
            &msb,
            &BTreeMap::new(),
            &HashMap::new(),
            &inventory,
        );
        let kinds = [
            "MapPiece",
            "Object",
            "Enemy",
            "Player",
            "Collision",
            "DummyObject",
            "DummyEnemy",
            "ConnectCollision",
        ];
        for (kind, count) in kinds.into_iter().zip(expected) {
            assert_eq!(exported["part_counts"][kind], count, "{id} {kind}");
        }
        assert_eq!(exported["parts"].as_array().unwrap().len(), msb.parts.len());
        assert_eq!(
            exported["connections"].as_array().unwrap().len(),
            expected[7]
        );
        for (index, (row, part)) in exported["parts"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&msb.parts)
            .enumerate()
        {
            assert_eq!(row["part_index"], index);
            assert_eq!(row["name"], part.name);
            assert_eq!(row["entity_id"], part.entity_id);
            assert_eq!(row["entity_group_ids"], json!(part.entity_group_ids));
            assert_eq!(row["masks"], json!(part.masks));
            assert_eq!(row["disp_groups"], json!(part.disp_groups));
            assert_eq!(row["lod_param_id"], part.lod_param_id);
            assert!(row["render_status"]["reason"].is_string());
            if let Some(display) = &part.display_data {
                assert_eq!(
                    row["display_data"]["collision_mask"],
                    json!(display.collision_mask)
                );
                assert_eq!(row["display_data"]["condition1"], display.condition1);
                assert_eq!(row["display_data"]["condition2"], display.condition2);
            } else {
                assert!(row["display_data"].is_null());
            }
            if let Some(group) = &part.display_group {
                assert_eq!(row["display_group"]["condition"], group.condition);
                assert_eq!(
                    row["display_group"]["disp_groups"],
                    json!(group.disp_groups)
                );
            }
            if let Some((collision_index, map_id)) = part.connect {
                assert_eq!(row["connection"]["collision_index"], collision_index);
                assert_eq!(row["connection"]["map_id"], json!(map_id));
                assert_eq!(
                    row["connection"]["source_collision"]["name"].as_str(),
                    msb.collision_by_index(collision_index)
                        .map(|c| c.name.as_str())
                );
            }
            if let Some(object) = &part.object {
                for (reference, index) in row["object_references"].as_array().unwrap().iter().zip([
                    object.obj_part_index1,
                    object.obj_part_index2,
                    object.obj_part_index3,
                ]) {
                    assert_eq!(reference["part_index"], index);
                    assert_eq!(
                        reference["name"].as_str(),
                        msb.part_by_index(index).map(|p| p.name.as_str())
                    );
                }
            }
        }
        // The actual cache export is a separate E2E check after running the CLI.
        let on_disk: Value = serde_json::from_slice(
            &std::fs::read(cache.join(format!("maps/{id}/layout.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            on_disk["layout_version"],
            layout::VERSION,
            "re-export {id} before running this installed-data check"
        );
        assert_eq!(on_disk["part_counts"], exported["part_counts"]);
        for (actual, expected) in on_disk["parts"]
            .as_array()
            .unwrap()
            .iter()
            .zip(exported["parts"].as_array().unwrap())
        {
            for key in [
                "part_index",
                "kind",
                "name",
                "model",
                "model_index",
                "source_transform",
                "translation",
                "rotation",
                "scale",
                "entity_id",
                "entity_group_ids",
                "lod_param_id",
                "masks",
                "disp_groups",
                "display_data",
                "display_group",
                "entity_data",
                "gparam",
                "scene_gparam",
                "grass_config",
                "enemy",
                "collision",
                "object",
                "object_references",
                "enemy_collision",
                "connection",
            ] {
                assert!(
                    if matches!(key, "translation" | "rotation" | "scale") {
                        same_derived_transform(&actual[key], &expected[key])
                    } else {
                        same_source_values(&actual[key], &expected[key])
                    },
                    "{id} {} {key}: {} != {}",
                    actual["name"],
                    actual[key],
                    expected[key]
                );
            }
        }
        assert_eq!(
            on_disk["pieces"].as_array().unwrap().len(),
            // Four previously empty fog models in Outskirts and one in Castle now render.
            if id == "m11_00_00_00" { 4487 } else { 3624 }
        );
        assert_eq!(on_disk["parts"].as_array().unwrap().len(), msb.parts.len());
        assert_eq!(
            on_disk["connections"],
            json!(
                on_disk["parts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|p| p["connection"].is_object())
                    .collect::<Vec<_>>()
            )
        );
        println!(
            "{id}: {} parts, counts {}, {} connections",
            msb.parts.len(),
            on_disk["part_counts"],
            expected[7]
        );
    }
}
