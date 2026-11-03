use std::path::PathBuf;

use sekiro_formats::emevd::{Emevd, Event, Instruction, Parameter, RestBehavior};
use sekiro_sim::world_events::{self, FirstVisit, PartKind};

fn cache() -> PathBuf {
    crate::arena_c_support::cache()
}

#[test]
fn installed_first_visit_selects_normal_floors_and_pre_invasion_groups() {
    for (map, groups) in [
        (
            "m11_00_00_00",
            vec![
                (1106600, true, PartKind::Object),
                (1106610, false, PartKind::Object),
                (1106640, false, PartKind::Object),
                (1107600, true, PartKind::MapPiece),
                (1107610, false, PartKind::MapPiece),
            ],
        ),
        (
            "m11_01_00_00",
            vec![
                (1116200, true, PartKind::Object),
                (1116201, true, PartKind::Object),
                (1116203, false, PartKind::Object),
                (1116204, false, PartKind::Object),
                (1117201, true, PartKind::MapPiece),
                (1117203, false, PartKind::MapPiece),
                (1117204, false, PartKind::MapPiece),
                (1117210, false, PartKind::MapPiece),
                (1117551, false, PartKind::MapPiece),
                (1116650, false, PartKind::Object),
                (1116651, false, PartKind::Object),
            ],
        ),
    ] {
        let state = world_events::first_visit(&cache(), map).unwrap().unwrap();
        println!(
            "{map}: started {}, ended {}, writes {}, unsupported {}, omitted {}",
            state.started,
            state.ended,
            state.visibility.len(),
            state.unsupported.len(),
            state.omitted.len()
        );
        assert!(
            !state.flags.contains(&8306),
            "{map}: prologue fixture enabled 8306"
        );
        assert!(
            !state.flags.contains(&8302),
            "{map}: first-visit fixture enabled 8302"
        );
        assert!(
            !state.flags.contains(&8301),
            "{map}: first-visit fixture enabled 8301"
        );
        for (target, enabled, kind) in groups {
            let write = state
                .visibility
                .iter()
                .find(|v| v.kind == kind && v.target == target)
                .unwrap_or_else(|| panic!("missing {map} {target}"));
            assert_eq!(
                write.enabled, enabled,
                "{map} {target}, event {}",
                write.event
            );
        }
        let layout: serde_json::Value = serde_json::from_slice(
            &std::fs::read(cache().join(format!("maps/{map}/layout.json"))).unwrap(),
        )
        .unwrap();
        let parts = state.parts(&layout);
        let audit = cache().parent().unwrap().join("target/alt-mc/story");
        std::fs::create_dir_all(&audit).unwrap();
        std::fs::write(
            audit.join(format!("{map}.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "map": map, "flags": state.flags, "writes": state.visibility,
                "parts": parts, "unsupported": state.unsupported, "omitted": state.omitted,
                "started": state.started, "ended": state.ended
            }))
            .unwrap(),
        )
        .unwrap();
        for kind in [PartKind::Object, PartKind::MapPiece, PartKind::Collision] {
            println!(
                "{map} {kind:?}: {} enabled / {} disabled",
                parts.iter().filter(|p| p.kind == kind && p.enabled).count(),
                parts
                    .iter()
                    .filter(|p| p.kind == kind && !p.enabled)
                    .count()
            );
        }
        if map == "m11_01_00_00" {
            for (name, enabled) in [("m015070_4000", true), ("m015072_4000", false)] {
                assert_eq!(
                    parts.iter().find(|p| p.name == name).unwrap().enabled,
                    enabled,
                    "{name}"
                );
            }
            for (entity, enabled) in [
                (1111610, true),
                (1111730, false),
                (1111952, false),
                (1111713, true),
            ] {
                assert_eq!(
                    parts
                        .iter()
                        .find(|p| p.entity_id == entity)
                        .unwrap()
                        .enabled,
                    enabled,
                    "{entity}"
                );
            }
            let restored = parts.iter().find(|p| p.entity_id == 1111610).unwrap();
            assert!(
                restored
                    .source
                    .as_ref()
                    .is_some_and(|v| v.event == 11115615 && v.enabled)
            );
            // Later invasion + cutscene must actually reverse the same floor selection.
            let mut burned = FirstVisit::for_map(map).unwrap();
            burned.flags.extend([8301, 11115921]);
            let later_state = world_events::evaluate(&cache(), map, burned).unwrap();
            assert!(
                later_state
                    .visibility
                    .iter()
                    .find(|v| v.kind == PartKind::Object && v.target == 1116650)
                    .unwrap()
                    .enabled
            );
            let later = later_state.parts(&layout);
            for (name, enabled) in [("m015070_4000", false), ("m015072_4000", true)] {
                assert_eq!(
                    later.iter().find(|p| p.name == name).unwrap().enabled,
                    enabled,
                    "later {name}"
                );
            }
        }
        assert!(
            parts
                .iter()
                .any(|p| p.kind == PartKind::Object && p.source.is_none() && p.enabled)
        );
    }
    assert!(
        world_events::first_visit(&cache(), "m10_00_00_00")
            .unwrap()
            .is_none()
    );
}

fn ins(bank: i32, id: i32, words: &[i32]) -> Instruction {
    Instruction {
        bank,
        id,
        args: words.iter().flat_map(|v| v.to_le_bytes()).collect(),
        layer: None,
    }
}

fn event(id: i64, instructions: Vec<Instruction>) -> Event {
    Event {
        id,
        instructions,
        rest: RestBehavior::Default,
        parameters: vec![],
    }
}

#[test]
fn constructor_resolves_every_slot_and_orders_entity_group_and_collision_overrides() {
    let mut helper = event(200, vec![ins(2005, 3, &[0, 0])]);
    helper.parameters = vec![Parameter {
        instruction: 0,
        target_start: 0,
        source_start: 0,
        byte_count: 8,
    }];
    let map = Emevd {
        events: vec![
            event(
                0,
                vec![
                    ins(2000, 6, &[200, 700, 0]),
                    ins(2000, 6, &[200, 701, 1]),
                    ins(2000, 0, &[0, 300, 800, 0]),
                    ins(2000, 0, &[1, 300, 801, 1]),
                    ins(2000, 0, &[2, 400]),
                ],
            ),
            {
                let mut local = helper.clone();
                local.id = 300;
                local
            },
            event(
                400,
                vec![
                    ins(1001, 1, &[2]),
                    ins(2005, 3, &[702, 1]),
                    ins(2011, 1, &[900, 0]),
                ],
            ),
        ],
        ..Default::default()
    };
    let common = Emevd {
        events: vec![helper],
        ..Default::default()
    };
    let state = world_events::evaluate_files(
        vec![map, common, Emevd::default()],
        "m11_01_00_00",
        FirstVisit::for_map("m11_01_00_00").unwrap(),
    )
    .unwrap();
    let layout = serde_json::json!({"parts": [
        {"part_index": 0, "name": "group_then_entity", "kind": "Object", "entity_id": 702, "entity_group_ids": [700]},
        {"part_index": 1, "name": "common_other", "kind": "Object", "entity_id": 0, "entity_group_ids": [701]},
        {"part_index": 2, "name": "slot0", "kind": "Object", "entity_id": 800},
        {"part_index": 3, "name": "slot1", "kind": "Object", "entity_id": 801},
        {"part_index": 4, "name": "collision", "kind": "Collision", "entity_id": 900},
        {"part_index": 5, "name": "same_id_different_kind", "kind": "MapPiece", "entity_id": 900}
    ]});
    assert_eq!(
        state
            .parts(&layout)
            .iter()
            .map(|p| p.enabled)
            .collect::<Vec<_>>(),
        [true, true, false, true, false, true]
    );
    assert!(!state.scene.collisions_enabled[&900]);
    assert!(state.unsupported.is_empty());
}

#[test]
fn unknown_world_predicate_blocks_only_its_dependent_restore() {
    let map = Emevd {
        events: vec![event(
            0,
            vec![
                ins(2005, 3, &[700, 0]),
                ins(4, 99, &[0]),
                ins(2005, 3, &[700, 1]),
            ],
        )],
        ..Default::default()
    };
    let state = world_events::evaluate_files(
        vec![map, Emevd::default(), Emevd::default()],
        "m11_01_00_00",
        FirstVisit::for_map("m11_01_00_00").unwrap(),
    )
    .unwrap();
    assert!(!state.visibility[0].enabled);
    assert_eq!(state.unsupported.len(), 1);
    assert_eq!(state.unsupported.first().unwrap().instruction, 1);
}
