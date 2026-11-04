//! `object_collision.bin`: the hit collision placed objects carry in their own binders
//! (`docs/OBJECT-COLLISION.md`).
//!
//! Each placed `Object` part (not `DummyObject`, which the game never instantiates with physics)
//! whose objbnd holds `<model>.hkx` contributes that physics system's static and keyframed
//! bodies, intact (break state 0), placed with the part's MSB transform and mirrored into Bevy
//! space exactly like the map collision. Dynamic bodies are left out: the exe gives them the
//! dynamic object groups (28 to 31), which the player, enemies and the camera do not collide
//! with. The model's `ObjectParam.collisionType` is kept per part; it selects the Havok group
//! of the bodies, which `sekiro_sim::hit_filter` turns into who collides with them.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use sekiro_formats::hknp::{self, ObjectCollision, ObjectCollisionPart};
use sekiro_formats::msb;

/// Writes `cache/maps/<id>/object_collision.bin` from the MSB and `cache/raw/obj`.
pub(super) fn export(cache: &Path, id: &str, msb: &msb::Msb) -> Result<()> {
    let params: serde_json::Value = serde_json::from_slice(
        &std::fs::read(cache.join("params/ObjectParam.json"))
            .context("cache/params/ObjectParam.json (run `sekiro-extract params`)")?,
    )?;
    let collision_type: HashMap<i64, u8> = params["rows"]
        .as_array()
        .context("ObjectParam rows")?
        .iter()
        .filter_map(|r| Some((r["id"].as_i64()?, r["collisionType"].as_u64()? as u8)))
        .collect();
    let mut decoded: HashMap<String, Option<hknp::CollisionMesh>> = HashMap::new();
    let mut out = ObjectCollision::default();
    let (mut placed, mut with_hkx, mut skipped_dynamic, mut other_shapes) = (0, 0, 0, 0);
    for part in msb.parts.iter().filter(|p| p.kind == msb::PartKind::Object) {
        let Some(model) = msb.model(part) else {
            continue;
        };
        placed += 1;
        let name = &model.name;
        if !decoded.contains_key(name) {
            let path = super::object_flver(&cache.join("raw"), name).with_extension("hkx");
            // gap: a binder the reader cannot decode (m20's o205010: "shared vertex out of
            // range") leaves that object without collision instead of stopping the map.
            let mesh = if path.is_file() {
                match hknp::decode_bodies(&std::fs::read(&path)?, None) {
                    Ok((mesh, stats)) => {
                        other_shapes += stats.other_shapes;
                        skipped_dynamic += stats.dynamic;
                        Some(mesh)
                    }
                    Err(e) => {
                        eprintln!("  warning: {}: {e:#}, no collision for {name}", path.display());
                        None
                    }
                }
            } else {
                None
            };
            decoded.insert(name.clone(), mesh);
        }
        let Some(mesh) = &decoded[name] else {
            continue;
        };
        with_hkx += 1;
        if mesh.indices.is_empty() {
            continue;
        }
        let number: i64 = name.trim_start_matches('o').parse().unwrap_or(-1);
        out.parts.push(ObjectCollisionPart {
            name: part.name.clone(),
            entity_id: part.entity_id,
            collision_type: collision_type.get(&number).copied().unwrap_or(0),
            first: out.mesh.triangle_count() as u32,
            count: mesh.triangle_count() as u32,
        });
        super::extend_placed(&mut out.mesh, mesh, part);
    }
    std::fs::write(
        cache.join("maps").join(id).join("object_collision.bin"),
        out.to_bytes()?,
    )?;
    eprintln!(
        "  object collision: {} of {placed} placed objects with a physics binder, {} with \
         static or keyframed bodies, {} triangles ({skipped_dynamic} dynamic bodies and \
         {other_shapes} other shapes left out)",
        with_hkx,
        out.parts.len(),
        out.mesh.triangle_count(),
    );
    Ok(())
}
