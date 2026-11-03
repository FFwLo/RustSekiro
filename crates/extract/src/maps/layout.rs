//! Versioned, complete MSBS placement metadata alongside the legacy viewer projection.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::Result;
use sekiro_formats::msb::{self, PartKind};
use serde_json::{Value, json};

pub const VERSION: u32 = 2;

pub fn transform_json(part: &msb::Part) -> Value {
    let mirror = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0));
    let (s, r, t) = (mirror * part.matrix() * mirror).to_scale_rotation_translation();
    json!({"translation": t.to_array(), "rotation": r.normalize().to_array(), "scale": s.to_array()})
}

pub fn map_inventory(raw: &Path) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(raw.join("map/mapstudio"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "msb")
            && let Some(id) = path.file_stem().and_then(|s| s.to_str())
        {
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    Ok(ids)
}

/// Retail matcher observed in the executable audit: negative block selects block zero.
/// These are inventory matches, not a promise that the block is requested or resident.
pub fn target_blocks(bytes: [u8; 4], inventory: &[String]) -> Vec<String> {
    if bytes == [255; 4] {
        return Vec::new();
    }
    inventory
        .iter()
        .filter(|id| {
            let Some(tail) = id.strip_prefix('m') else {
                return false;
            };
            let fields: Vec<_> = tail
                .split('_')
                .filter_map(|s| s.parse::<u8>().ok())
                .collect();
            fields.len() == 4
                && fields.iter().enumerate().all(|(i, &value)| {
                    if i == 1 && (bytes[i] as i8) < 0 {
                        value == 0
                    } else {
                        (bytes[i] as i8) < 0 || value == bytes[i]
                    }
                })
        })
        .cloned()
        .collect()
}

fn reference(msb: &msb::Msb, index: i32) -> Value {
    json!({"part_index": index, "name": msb.part_by_index(index).map(|p| &p.name)})
}

pub fn extend(
    layout: &mut Value,
    msb: &msb::Msb,
    model_status: &BTreeMap<String, String>,
    ranges: &HashMap<String, [usize; 2]>,
    inventory: &[String],
) {
    let mut counts = BTreeMap::<String, usize>::new();
    let legacy_pieces: BTreeSet<_> = layout["pieces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["name"].as_str())
        .map(str::to_owned)
        .collect();
    let parts: Vec<_> = msb.parts.iter().enumerate().map(|(index, p)| {
        let kind = format!("{:?}", p.kind);
        *counts.entry(kind.clone()).or_default() += 1;
        let model = msb.model(p);
        let status = match p.kind {
            PartKind::Collision => "physics_only",
            PartKind::ConnectCollision => "connection_only",
            PartKind::Player => "player_start",
            PartKind::DummyObject | PartKind::DummyEnemy => "helper_dummy",
            PartKind::Enemy => "character_pipeline",
            PartKind::Other(_) => "unsupported_part_kind",
            _ if model.is_none() => "no_model",
            _ => model.and_then(|m| model_status.get(&m.name)).map(String::as_str).unwrap_or("model_not_exported"),
        };
        let legacy_piece = legacy_pieces.contains(&p.name);
        let reason = if status == "exported" && p.kind == PartKind::MapPiece && !legacy_piece { "legacy_zero_draw_groups" } else { status };
        let mut row = transform_json(p);
        row["part_index"] = json!(index);
        row["kind"] = json!(kind);
        row["name"] = json!(p.name);
        row["id"] = json!(p.id);
        row["model_index"] = json!(p.model_index);
        row["model"] = json!(model.map(|m| &m.name));
        row["sib_path"] = json!(p.sib_path);
        row["source_transform"] = json!({"position": p.position, "rotation": p.rotation, "scale": p.scale});
        row["yaw_degrees"] = json!(-p.rotation[1]);
        row["entity_id"] = json!(p.entity_id);
        row["entity_group_ids"] = json!(p.entity_group_ids);
        row["lod_param_id"] = json!(p.lod_param_id);
        row["is_shadow_only"] = json!(p.is_shadow_only);
        row["masks"] = json!(p.masks);
        row["disp_groups"] = json!(p.disp_groups);
        row["gparam"] = json!(p.gparam);
        row["render_status"] = json!({"asset": status, "reason": reason, "legacy_piece": legacy_piece});
        row["triangles"] = json!(ranges.get(&p.name));
        if let Some((collision_index, map_id)) = p.connect {
            let collision = msb.collision_by_index(collision_index);
            row["connection"] = json!({
                "collision_index": collision_index,
                "source_collision": collision.map(|c| json!({"part_index": msb.parts.iter().position(|p| std::ptr::eq(p, c)), "name": c.name, "model": msb.model(c).map(|m| &m.name)})),
                "map_id": map_id,
                "target_blocks": target_blocks(map_id, inventory),
                "resolution": if collision_index == -1 { "unbound" } else if collision.is_some() { "resolved" } else { "invalid_collision_index" },
            });
        } else { row["connection"] = Value::Null; }
        add_settings(&mut row, p);
        if let Some(object) = &p.object {
            row["object_references"] = json!([reference(msb, object.obj_part_index1), reference(msb, object.obj_part_index2), reference(msb, object.obj_part_index3)]);
        }
        if let Some(enemy) = &p.enemy { row["enemy_collision"] = reference(msb, enemy.collision_part); }
        row
    }).collect();
    layout["layout_version"] = json!(VERSION);
    layout["part_counts"] = json!(counts);
    layout["connections"] = json!(
        parts
            .iter()
            .filter(|p| p["connection"].is_object())
            .collect::<Vec<_>>()
    );
    layout["parts"] = json!(parts);
    // New metadata is optional to the existing loader; preserve its existing field values.
    let by_name: BTreeMap<_, _> = parts
        .iter()
        .filter_map(|p| p["name"].as_str().map(|name| (name, p)))
        .collect();
    for key in ["pieces", "objects", "enemies", "players", "collisions"] {
        if let Some(rows) = layout[key].as_array_mut() {
            for row in rows {
                if let Some(part) = row["name"].as_str().and_then(|name| by_name.get(name)) {
                    for (field, value) in part.as_object().expect("part row").iter() {
                        if row.get(field).is_none() {
                            row[field] = value.clone();
                        }
                    }
                }
            }
        }
    }
    layout["msb_models"] = json!(msb.models.iter().enumerate().map(|(index, m)| json!({"model_index": index, "name": m.name, "kind": format!("{:?}", m.kind), "sib_path": m.sib_path, "instance_count": m.instance_count})).collect::<Vec<_>>());
    layout["model_status"] = json!(model_status);
    layout["map_inventory"] = json!(inventory);
    layout["map_offsets"] = json!(msb.map_offsets.iter().map(|o| json!({"name": o.name, "event_id": o.event_id, "id": o.id, "source_offset": o.source_offset, "part_index": o.part_index, "region_index": o.region_index, "entity_id": o.entity_id, "position": o.position, "rotation_y": o.rotation_y})).collect::<Vec<_>>());
}
fn add_settings(row: &mut Value, p: &msb::Part) {
    row["display_data"] = p.display_data.as_ref().map(|v| json!({"display_groups": v.display_groups, "draw_groups": v.draw_groups, "collision_mask": v.collision_mask, "condition1": v.condition1, "condition2": v.condition2})).unwrap_or(Value::Null);
    row["display_group"] = p.display_group.as_ref().map(|v| json!({"condition": v.condition, "disp_groups": v.disp_groups, "unk24": v.unk24, "unk26": v.unk26})).unwrap_or(Value::Null);
    let v = &p.entity_data;
    row["entity_data"] = json!({"unk_e04": v.unk_e04, "unk_e05": v.unk_e05, "unk_e06": v.unk_e06, "lantern_id": v.lantern_id, "unk_e09": v.unk_e09, "is_point_light_shadow_src": v.is_point_light_shadow_src, "unk_e0b": v.unk_e0b, "is_shadow_src": v.is_shadow_src, "is_static_shadow_src": v.is_static_shadow_src, "is_cascade3_shadow_src": v.is_cascade3_shadow_src, "unk_e0f": v.unk_e0f, "unk_e10": v.unk_e10, "is_shadow_dest": v.is_shadow_dest, "draw_by_reflect_cam": v.draw_by_reflect_cam, "draw_only_reflect_cam": v.draw_only_reflect_cam, "enable_on_above_shadow": v.enable_on_above_shadow, "disable_point_light_effect": v.disable_point_light_effect, "unk_e17": v.unk_e17, "unk_e18": v.unk_e18, "unk_e3c": v.unk_e3c, "unk_e40": v.unk_e40});
    row["enemy"] = p.enemy.as_ref().map(|v| json!({"think_param_id": v.think_param_id, "npc_param_id": v.npc_param_id, "talk_id": v.talk_id, "unk_t10": v.unk_t10, "platoon_id": v.platoon_id, "chara_init_id": v.chara_init_id, "collision_part": v.collision_part, "backup_event_anim_id": v.backup_event_anim_id, "event_flag_id": v.event_flag_id, "walk_route_index": v.walk_route_index, "init_anim_id_type": v.init_anim_id_type, "unk_t24": v.unk_t24, "event_flag_compare_state": v.event_flag_compare_state, "unk_t48": v.unk_t48, "unk_t4c": v.unk_t4c, "unk_t50": v.unk_t50, "unk_t78": v.unk_t78, "unk_t84": v.unk_t84})).unwrap_or(Value::Null);
    row["collision"] = p.collision.as_ref().map(|v| json!({"hit_filter_id": v.hit_filter_id, "sound_space_type": v.sound_space_type, "map_name_id": v.map_name_id, "disable_start": v.disable_start, "play_region_id": v.play_region_id, "lock_cam_param_id": v.lock_cam_param_id, "reflect_plane_height": v.reflect_plane_height, "unk_t17": v.unk_t17, "disable_bonfire_entity_id": v.disable_bonfire_entity_id, "unk_t24": v.unk_t24, "unk_t25": v.unk_t25, "unk_t26": v.unk_t26, "map_visibility": v.map_visibility, "unk_t3c": v.unk_t3c, "unk_t40": v.unk_t40, "unk_t44": v.unk_t44, "unk_t48": v.unk_t48, "unk_t4c": v.unk_t4c, "unk_t50": v.unk_t50, "unk_t54": v.unk_t54})).unwrap_or(Value::Null);
    row["object"] = p.object.as_ref().map(|v| json!({"obj_part_index1": v.obj_part_index1, "obj_part_index2": v.obj_part_index2, "obj_part_index3": v.obj_part_index3, "break_term": v.break_term, "net_sync_type": v.net_sync_type, "unk_t0e": v.unk_t0e, "set_main_obj_structure_booleans": v.set_main_obj_structure_booleans, "anim_id": v.anim_id, "model_sfx_param_relative_id": v.model_sfx_param_relative_id, "unk_t1a": v.unk_t1a})).unwrap_or(Value::Null);
    row["scene_gparam"] = p
        .scene_gparam
        .as_ref()
        .map(|v| json!({"event_ids": v.event_ids, "transition_time": v.transition_time}))
        .unwrap_or(Value::Null);
    row["grass_config"] = p.grass_config.as_ref().map(|v| json!({"unk00": v.unk00, "unk04": v.unk04, "grass_type_param_id": v.grass_type_param_id, "unk0c": v.unk0c, "unk10": v.unk10, "unk14": v.unk14})).unwrap_or(Value::Null);
}
