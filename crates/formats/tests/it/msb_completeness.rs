//! Independent raw-table audit of every installed MSBS, without regenerating cache data.

use std::{collections::BTreeMap, path::Path};

use sekiro_formats::msb::{self, PartKind};

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

fn i32_at(data: &[u8], at: usize) -> i32 {
    u32_at(data, at) as i32
}

fn i16_at(data: &[u8], at: usize) -> i16 {
    i16::from_le_bytes(data[at..at + 2].try_into().unwrap())
}

fn offset(data: &[u8], at: usize) -> usize {
    usize::try_from(i64::from_le_bytes(data[at..at + 8].try_into().unwrap())).unwrap()
}

fn string(data: &[u8], at: usize) -> String {
    let units = data[at..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .take_while(|&u| u != 0)
        .collect::<Vec<_>>();
    String::from_utf16(&units).unwrap()
}

fn tables(data: &[u8]) -> BTreeMap<String, Vec<usize>> {
    let mut tables = BTreeMap::new();
    let mut at = 16;
    while at != 0 {
        let count = u32_at(data, at + 4) as usize;
        let entries = (0..count - 1)
            .map(|i| offset(data, at + 16 + i * 8))
            .collect();
        tables.insert(string(data, offset(data, at + 8)), entries);
        at = offset(data, at + 16 + (count - 1) * 8);
    }
    tables
}

fn msb_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            msb_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "msb") {
            out.push(path);
        }
    }
}

fn put_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_offset(data: &mut [u8], at: usize, value: usize) {
    data[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
}

fn put_string(data: &mut [u8], at: usize, value: &str) {
    for (i, unit) in value.encode_utf16().chain([0]).enumerate() {
        data[at + 2 * i..at + 2 * i + 2].copy_from_slice(&unit.to_le_bytes());
    }
}

#[test]
fn named_metadata_keeps_conditions_signed_masks_and_index_domains_without_cache() {
    // A small original fixture, deliberately placing Collision at full index 1
    // but collision-list index 0; Object references must use the former.
    let mut data = vec![0; 0x1900];
    data[..4].copy_from_slice(b"MSB ");
    put_u32(&mut data, 4, 1);
    put_u32(&mut data, 8, 16);
    put_u32(&mut data, 0x10, 35);
    put_u32(&mut data, 0x14, 6);
    put_offset(&mut data, 0x18, 0x60);
    put_string(&mut data, 0x60, "PARTS_PARAM_ST");
    put_offset(&mut data, 0x48, 0x90);
    put_u32(&mut data, 0x90, 35);
    put_u32(&mut data, 0x94, 2);
    put_offset(&mut data, 0x98, 0xb0);
    put_string(&mut data, 0xb0, "EVENT_PARAM_ST");
    put_offset(&mut data, 0xa0, 0x1800);
    for (i, kind) in [0, 5, 1, 9, 11].into_iter().enumerate() {
        let at = 0x200 + i * 0x400;
        put_offset(&mut data, 0x20 + i * 8, at);
        put_offset(&mut data, at, 0xa0);
        put_string(&mut data, at + 0xa0, &format!("part{i}"));
        put_u32(&mut data, at + 8, kind);
        put_u32(&mut data, at + 16, u32::MAX);
        put_offset(&mut data, at + 24, 0xc0);
        put_u32(&mut data, at + 32, 12f32.to_bits());
        put_offset(&mut data, at + 96, 0x2c0);
        put_offset(&mut data, at + 104, 0x320);
        if matches!(kind, 0 | 1 | 5) {
            put_offset(&mut data, at + 80, 0xe0);
            for word in 0..48 {
                put_u32(&mut data, at + 0xe0 + word * 4, 0x8000_0000 + word as u32);
            }
            data[at + 0xe0 + 192] = 0xa5;
            data[at + 0xe0 + 193] = 0x5a;
        }
        if matches!(kind, 5 | 11) {
            put_offset(&mut data, at + 88, 0x270);
            put_u32(&mut data, at + 0x270, (-123i32) as u32);
            for word in 0..8 {
                put_u32(&mut data, at + 0x274 + word * 4, 0x8000_0010 + word as u32);
            }
            data[at + 0x294..at + 0x296].copy_from_slice(&(-321i16).to_le_bytes());
            data[at + 0x296..at + 0x298].copy_from_slice(&123i16.to_le_bytes());
        }
        if matches!(kind, 1 | 9) {
            put_u32(&mut data, at + 0x328, 0);
            put_u32(&mut data, at + 0x340, 1);
            put_u32(&mut data, at + 0x344, u32::MAX);
            data[at + 0x330..at + 0x332].copy_from_slice(&(-2i16).to_le_bytes());
        }
        if kind == 11 {
            put_u32(&mut data, at + 0x320, 0);
            data[at + 0x324..at + 0x328].copy_from_slice(&[13, 255, 255, 255]);
        }
    }
    let event = 0x1800;
    put_offset(&mut data, event, 0x30);
    put_string(&mut data, event + 0x30, "offset");
    put_u32(&mut data, event + 8, 42);
    put_u32(&mut data, event + 12, 9);
    put_u32(&mut data, event + 16, 7);
    put_offset(&mut data, event + 24, 0x50);
    put_offset(&mut data, event + 32, 0x60);
    put_u32(&mut data, event + 0x50, u32::MAX);
    put_u32(&mut data, event + 0x54, u32::MAX);
    for (i, value) in [90f32, 3., -40., 20.].into_iter().enumerate() {
        put_u32(&mut data, event + 0x60 + i * 4, value.to_bits());
    }
    let parsed = msb::parse(&data).unwrap();
    for part in &parsed.parts[..3] {
        let display = part.display_data.as_ref().unwrap();
        assert_eq!(display.display_groups[7], 0x8000_0007);
        assert_eq!(display.draw_groups[0], 0x8000_0008);
        assert_eq!(display.draw_groups[7], 0x8000_000f);
        assert_eq!(display.collision_mask[0], 0x8000_0010);
        assert_eq!(display.collision_mask[31], 0x8000_002f);
        assert_eq!((display.condition1, display.condition2), (0xa5, 0x5a));
    }
    for i in [1, 4] {
        let group = parsed.parts[i].display_group.as_ref().unwrap();
        assert_eq!(group.condition, -123);
        assert_eq!(group.disp_groups[0], 0x8000_0010u32 as i32);
        assert_eq!((group.unk24, group.unk26), (-321, 123));
    }
    for i in [2, 3] {
        let object = parsed.parts[i].object.as_ref().unwrap();
        assert_eq!(object.anim_id, -2);
        assert_eq!(
            parsed.part_by_index(object.obj_part_index1).unwrap().kind,
            PartKind::MapPiece
        );
        assert_eq!(
            parsed.part_by_index(object.obj_part_index2).unwrap().kind,
            PartKind::Collision
        );
        assert!(parsed.part_by_index(object.obj_part_index3).is_none());
    }
    assert!(parsed.parts[3].display_data.is_none());
    let (index, map_id) = parsed.parts[4].connect.unwrap();
    assert_eq!(map_id, [13, 255, 255, 255]);
    assert_eq!(parsed.collision_by_index(index).unwrap().name, "part1");
    assert!(parsed.collision_by_index(-1).is_none());
    assert!(parsed.collision_by_index(1).is_none());
    let offset = &parsed.map_offsets[0];
    assert_eq!(
        (
            &offset.name,
            offset.event_id,
            offset.id,
            offset.source_offset
        ),
        (&"offset".to_owned(), 42, 7, event)
    );
    assert_eq!(offset.position, [90., 3., -40.]);
    assert_eq!(offset.rotation_y, 20.);
    assert!(
        parsed
            .parts
            .iter()
            .all(|part| part.position == [12., 0., 0.])
    );
}

#[test]
fn every_cached_msbs_preserves_raw_parts_masks_and_references() {
    let dir = sekiro_formats::cache::root().join("raw/map");
    if !dir.exists() {
        eprintln!("skipping MSBS audit: map cache is not extracted");
        return;
    }
    let mut paths = Vec::new();
    msb_files(&dir, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "map cache exists but contains no MSBs");
    let mut total_parts = 0;
    let mut total_connections = 0;
    let mut unbound_connections = 0;
    let mut total_object_refs = 0;
    let mut total_enemy_refs = 0;
    let mut total_patrol_refs = 0;
    let mut total_offsets = 0;
    for path in &paths {
        let map = path.file_stem().unwrap().to_str().unwrap();
        let data = std::fs::read(path).unwrap();
        let tables = tables(&data);
        let raw_parts = &tables["PARTS_PARAM_ST"];
        let raw_patrols = tables["EVENT_PARAM_ST"]
            .iter()
            .copied()
            .filter(|&at| u32_at(&data, at + 12) == 14)
            .collect::<Vec<_>>();
        let raw_collisions = raw_parts
            .iter()
            .copied()
            .filter(|&at| u32_at(&data, at + 8) == 5)
            .collect::<Vec<_>>();
        let parsed = msb::parse(&data).unwrap();
        assert_eq!(parsed.parts.len(), raw_parts.len(), "{map}");
        let mut counts = BTreeMap::new();
        for (&at, part) in raw_parts.iter().zip(&parsed.parts) {
            let kind = u32_at(&data, at + 8);
            *counts.entry(kind).or_insert(0usize) += 1;
            assert_eq!(part.name, string(&data, at + offset(&data, at)));
            assert_eq!(
                part.kind,
                match kind {
                    0 => PartKind::MapPiece,
                    1 => PartKind::Object,
                    2 => PartKind::Enemy,
                    4 => PartKind::Player,
                    5 => PartKind::Collision,
                    9 => PartKind::DummyObject,
                    10 => PartKind::DummyEnemy,
                    11 => PartKind::ConnectCollision,
                    other => PartKind::Other(other),
                }
            );
            let model_index = i32_at(&data, at + 16);
            assert_eq!(part.id, i32_at(&data, at + 12));
            assert_eq!(part.model_index, model_index);
            if model_index != -1 {
                let raw_model = tables["MODEL_PARAM_ST"][model_index as usize];
                assert_eq!(
                    parsed.model(part).unwrap().name,
                    string(&data, raw_model + offset(&data, raw_model))
                );
            }
            for (values, start) in [(part.position, 32), (part.rotation, 44), (part.scale, 56)] {
                for (i, value) in values.into_iter().enumerate() {
                    assert_eq!(value.to_bits(), u32_at(&data, at + start + i * 4));
                }
            }
            let entity = at + offset(&data, at + 96);
            assert_eq!(part.entity_id, i32_at(&data, entity));
            assert_eq!(part.lod_param_id, data[entity + 8]);
            assert_eq!(part.is_shadow_only, data[entity + 18] != 0);
            for (i, &group) in part.entity_group_ids.iter().enumerate() {
                assert_eq!(group, i32_at(&data, entity + 28 + i * 4));
            }
            let flags = &part.entity_data;
            for (value, field) in [
                (flags.unk_e04, 4),
                (flags.unk_e05, 5),
                (flags.unk_e06, 6),
                (flags.lantern_id, 7),
                (flags.unk_e09, 9),
                (flags.unk_e0b, 11),
                (flags.is_static_shadow_src, 13),
                (flags.is_cascade3_shadow_src, 14),
                (flags.unk_e0f, 15),
                (flags.unk_e10, 16),
                (flags.enable_on_above_shadow, 21),
                (flags.unk_e17, 23),
            ] {
                assert_eq!(value, data[entity + field]);
            }
            for (value, field) in [
                (flags.is_point_light_shadow_src, 10),
                (flags.is_shadow_src, 12),
                (flags.is_shadow_dest, 17),
                (flags.draw_by_reflect_cam, 19),
                (flags.draw_only_reflect_cam, 20),
                (flags.disable_point_light_effect, 22),
            ] {
                assert_eq!(value, data[entity + field] != 0);
            }
            for (value, field) in [
                (flags.unk_e18, 24),
                (flags.unk_e3c, 60),
                (flags.unk_e40, 64),
            ] {
                assert_eq!(value, i32_at(&data, entity + field));
            }
            let scene = offset(&data, at + 120);
            assert_eq!(part.scene_gparam.is_some(), scene != 0);
            if let Some(scene_gparam) = &part.scene_gparam {
                assert_eq!(
                    scene_gparam.event_ids,
                    data[at + scene + 60..at + scene + 64]
                        .iter()
                        .map(|&b| b as i8)
                        .collect::<Vec<_>>()[..]
                );
                assert_eq!(
                    scene_gparam.transition_time.to_bits(),
                    u32_at(&data, at + scene + 64)
                );
            }
            let grass = offset(&data, at + 128);
            assert_eq!(part.grass_config.is_some(), grass != 0);
            if let Some(grass_config) = &part.grass_config {
                for (i, value) in [
                    grass_config.unk00,
                    grass_config.unk04,
                    grass_config.grass_type_param_id,
                    grass_config.unk0c,
                    grass_config.unk10,
                    grass_config.unk14,
                ]
                .into_iter()
                .enumerate()
                {
                    assert_eq!(value, i32_at(&data, at + grass + i * 4));
                }
            }
            let display = offset(&data, at + 80);
            assert_eq!(part.display_data.is_some(), display != 0);
            if let Some(display_data) = &part.display_data {
                let words = (0..48)
                    .map(|i| u32_at(&data, at + display + i * 4))
                    .collect::<Vec<_>>();
                assert_eq!(display_data.display_groups, words[..8]);
                assert_eq!(display_data.draw_groups, words[8..16]);
                assert_eq!(display_data.collision_mask, words[16..]);
                assert_eq!(part.masks, words);
                assert_eq!(display_data.condition1, data[at + display + 192]);
                assert_eq!(display_data.condition2, data[at + display + 193]);
            } else {
                assert!(part.masks.is_empty());
            }
            let group = offset(&data, at + 88);
            assert_eq!(part.display_group.is_some(), group != 0);
            if let Some(display_group) = &part.display_group {
                assert_eq!(display_group.condition, i32_at(&data, at + group));
                for i in 0..8 {
                    assert_eq!(
                        display_group.disp_groups[i],
                        i32_at(&data, at + group + 4 + i * 4)
                    );
                    assert_eq!(part.disp_groups[i], u32_at(&data, at + group + 4 + i * 4));
                }
                assert_eq!(
                    display_group.unk24,
                    i16::from_le_bytes(data[at + group + 36..at + group + 38].try_into().unwrap())
                );
                assert_eq!(
                    display_group.unk26,
                    i16::from_le_bytes(data[at + group + 38..at + group + 40].try_into().unwrap())
                );
            } else {
                assert!(part.disp_groups.is_empty());
            }
            let typed = at + offset(&data, at + 104);
            if let Some((index, map_id)) = part.connect {
                total_connections += 1;
                assert_eq!(index, i32_at(&data, typed));
                assert_eq!(map_id, data[typed + 4..typed + 8]);
                let target_name = if index == -1 {
                    unbound_connections += 1;
                    assert!(parsed.collision_by_index(index).is_none());
                    "<unbound (-1)>".to_owned()
                } else {
                    let raw_target = raw_collisions[index as usize];
                    let target_name = string(&data, raw_target + offset(&data, raw_target));
                    let target = parsed.collision_by_index(index).unwrap();
                    assert_eq!(target.name, target_name, "{map}: {}", part.name);
                    assert_eq!(target.kind, PartKind::Collision);
                    target_name
                };
                println!(
                    "connection {map} {} index={index} -> {target_name} map={map_id:?} condition={}",
                    part.name,
                    part.display_group.as_ref().unwrap().condition
                );
            }
            if let Some(object) = &part.object {
                assert_eq!(object.break_term, data[typed + 12]);
                assert_eq!(object.net_sync_type, data[typed + 13] != 0);
                assert_eq!(object.unk_t0e, data[typed + 14]);
                assert_eq!(
                    object.set_main_obj_structure_booleans,
                    data[typed + 15] != 0
                );
                assert_eq!(object.anim_id, i16_at(&data, typed + 16));
                assert_eq!(
                    object.model_sfx_param_relative_id,
                    i16_at(&data, typed + 24)
                );
                assert_eq!(object.unk_t1a, i16_at(&data, typed + 26));
                for (index, field) in [
                    (object.obj_part_index1, 8),
                    (object.obj_part_index2, 32),
                    (object.obj_part_index3, 36),
                ] {
                    assert_eq!(index, i32_at(&data, typed + field));
                    if index != -1 {
                        total_object_refs += 1;
                        let target = parsed.part_by_index(index).unwrap_or_else(|| {
                            panic!(
                                "{map}: {} invalid ObjPartIndex at +{field}: {index}",
                                part.name
                            )
                        });
                        let raw_target = raw_parts[index as usize];
                        assert_eq!(
                            target.name,
                            string(&data, raw_target + offset(&data, raw_target))
                        );
                    }
                }
            }
            assert_eq!(part.object.is_some(), matches!(kind, 1 | 9));
            if let Some(collision) = &part.collision {
                for (value, field) in [
                    (collision.hit_filter_id, 0),
                    (collision.sound_space_type, 1),
                    (collision.unk_t17, 23),
                    (collision.unk_t24, 36),
                    (collision.unk_t25, 37),
                    (collision.unk_t26, 38),
                    (collision.map_visibility, 39),
                ] {
                    assert_eq!(value, data[typed + field]);
                }
                assert_eq!(collision.map_name_id, i16_at(&data, typed + 20));
                assert_eq!(collision.disable_start, data[typed + 22] != 0);
                assert_eq!(collision.lock_cam_param_id, i16_at(&data, typed + 44));
                for (value, field) in [
                    (collision.disable_bonfire_entity_id, 24),
                    (collision.play_region_id, 40),
                    (collision.unk_t3c, 60),
                    (collision.unk_t40, 64),
                    (collision.unk_t4c, 76),
                ] {
                    assert_eq!(value, i32_at(&data, typed + field));
                }
                for (value, field) in [
                    (collision.reflect_plane_height, 4),
                    (collision.unk_t44, 68),
                    (collision.unk_t48, 72),
                    (collision.unk_t50, 80),
                    (collision.unk_t54, 84),
                ] {
                    assert_eq!(value.to_bits(), u32_at(&data, typed + field));
                }
            }
            if let Some(enemy) = &part.enemy {
                assert_eq!(enemy.walk_route_index, i16_at(&data, typed + 32));
                assert_eq!(enemy.init_anim_id_type, i16_at(&data, typed + 34));
                for (value, field) in [
                    (enemy.unk_t10, 16),
                    (enemy.unk_t24, 36),
                    (enemy.event_flag_compare_state, 68),
                    (enemy.unk_t48, 72),
                    (enemy.unk_t4c, 76),
                    (enemy.unk_t50, 80),
                    (enemy.unk_t78, 120),
                ] {
                    assert_eq!(value, i32_at(&data, typed + field));
                }
                assert_eq!(enemy.unk_t84.to_bits(), u32_at(&data, typed + 132));
                if enemy.walk_route_index != -1 {
                    total_patrol_refs += 1;
                    assert!(
                        enemy.walk_route_index >= 0
                            && (enemy.walk_route_index as usize) < raw_patrols.len(),
                        "{map}: {} invalid PatrolInfo reference {}",
                        part.name,
                        enemy.walk_route_index
                    );
                }
                assert_eq!(enemy.collision_part, i32_at(&data, typed + 28));
                if enemy.collision_part != -1 {
                    total_enemy_refs += 1;
                    assert_eq!(
                        parsed.part_by_index(enemy.collision_part).unwrap().kind,
                        PartKind::Collision,
                        "{map}: {}",
                        part.name
                    );
                }
            }
        }
        let raw_offsets = tables["EVENT_PARAM_ST"]
            .iter()
            .copied()
            .filter(|&at| u32_at(&data, at + 12) == 9)
            .collect::<Vec<_>>();
        assert_eq!(parsed.map_offsets.len(), raw_offsets.len());
        for (&at, map_offset) in raw_offsets.iter().zip(&parsed.map_offsets) {
            assert_eq!(map_offset.source_offset, at);
            assert_eq!(map_offset.name, string(&data, at + offset(&data, at)));
            assert_eq!(map_offset.event_id, i32_at(&data, at + 8));
            assert_eq!(map_offset.id, i32_at(&data, at + 16));
            let base = at + offset(&data, at + 24);
            assert_eq!(map_offset.part_index, i32_at(&data, base));
            assert_eq!(map_offset.region_index, i32_at(&data, base + 4));
            assert_eq!(map_offset.entity_id, i32_at(&data, base + 8));
            if map_offset.part_index != -1 {
                assert!(parsed.part_by_index(map_offset.part_index).is_some());
            }
            if map_offset.region_index != -1 {
                assert!((map_offset.region_index as usize) < tables["POINT_PARAM_ST"].len());
            }
            let typed = at + offset(&data, at + 32);
            for i in 0..3 {
                assert_eq!(
                    map_offset.position[i].to_bits(),
                    u32_at(&data, typed + i * 4)
                );
            }
            assert_eq!(map_offset.rotation_y.to_bits(), u32_at(&data, typed + 12));
        }
        if map == "m11_00_00_00" {
            assert_eq!(parsed.map_offsets.len(), 1);
            assert_eq!(parsed.map_offsets[0].position, [90., 3., -40.]);
            assert_eq!(parsed.map_offsets[0].rotation_y, 20.);
            let mismatch = parsed
                .parts
                .iter()
                .find(|p| p.name == "h004490_1000")
                .unwrap();
            assert_eq!(
                mismatch.connect.unwrap().0,
                -1,
                "This row is unbound; indexing a raw Python list with -1 incorrectly selects h900900"
            );
            assert!(parsed.collision_by_index(-1).is_none());
        }
        if map == "m11_01_00_00" {
            assert_eq!(counts.get(&11), Some(&298));
            assert!(parsed.map_offsets.is_empty());
        }
        total_parts += parsed.parts.len();
        total_offsets += parsed.map_offsets.len();
        println!(
            "map {map} raw/parsed part counts={counts:?} MapOffsets={}",
            parsed.map_offsets.len()
        );
    }
    println!(
        "MSBS audit: {} maps, {total_parts} parts, {total_connections} connections ({unbound_connections} explicitly unbound), {total_object_refs} Object references, {total_enemy_refs} Enemy collision references, {total_patrol_refs} PatrolInfo references, {total_offsets} MapOffsets; all non-sentinel references resolved",
        paths.len()
    );
}
