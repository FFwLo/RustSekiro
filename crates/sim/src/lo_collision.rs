//! The map collision characters and the camera collide with: the low-resolution `l` binder
//! (#189).
//!
//! Every MSB collision part has a model in two hit binders, `h<block>.hkxbhd` (high resolution,
//! exported as `collision.bin`) and `l<block>.hkxbhd` (low resolution, exported per part as
//! `collision_lo.bin` by `sekiro-extract map <block> --lo-collision`). The exe gives an ordinary
//! part's `h` body Havok group 38 and its `l` body group 39 or 40 (`FUN_1409b58b0`); the
//! player's capsule, enemies' capsules and the camera ray collide with 39 and 40 and never with
//! 38, which only projectile rays, foot IK, ragdolls and debris meet (`docs/HIT-FILTERS.md`).
//! Helper parts get their fixed group whichever binder their body comes from, so their `l` copy
//! is the one characters meet too.
//!
//! So every character and camera query here runs on `l`: [`read`] gives a block's `l` mesh with
//! the layout's `collisions` rows re-ranged into it, which the story's part removal
//! ([`crate::arenas_a::remove_hidden_collision`]), helper stripping
//! ([`crate::hit_filter::strip_helpers`]) and the arenas' steps read as they read
//! `collision.bin`'s. A block exported before `collision_lo.bin` existed falls back to
//! `collision.bin`, with one log line per block (`h`'s swim hits carry no water, so it has no
//! swim planes). The draw-group selection reads `l` too (`crates/game/src/map_world.rs`); the
//! `h` mesh stays for drawing (the collision wireframe) and the render sweeps.

use std::path::Path;

use sekiro_formats::hknp::{CollisionMesh, PartCollision};
use serde_json::Value;

/// The high-resolution `h` collision of a map folder.
pub const HIGH_FILE: &str = "collision.bin";
/// The low-resolution `l` collision of a map folder ([`PartCollision`]).
pub const LOW_FILE: &str = "collision_lo.bin";

/// A block's character collision ([`read`]).
#[derive(Debug, Clone)]
pub struct BlockMesh {
    pub mesh: CollisionMesh,
    /// `{"collisions": [...]}`: the layout's collision rows with `triangles` addressing
    /// [`BlockMesh::mesh`]. A part without a model in the binder gets an empty range.
    pub rows: Value,
}

/// The block name of a map folder (`cache/maps/<block>`), for logs.
fn block_of(dir: &Path) -> &str {
    dir.file_name().and_then(|n| n.to_str()).unwrap_or("?")
}

fn read_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Logs, once per block, that `dir`'s block has no `collision_lo.bin`, so characters use
/// `collision.bin`. It suggests the re-export only when the cache has the raw `l` binder
/// (`cache/raw/map/<block>/l<tail>.hkxbhd`) to export from.
fn log_fallback(dir: &Path) {
    static LOGGED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
    let mut logged = LOGGED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if logged.iter().any(|d| d == dir) {
        return;
    }
    logged.push(dir.to_path_buf());
    let block = block_of(dir);
    let binder = block.get(1..).and_then(|tail| {
        let raw = dir.parent()?.parent()?.join("raw/map").join(block);
        Some(raw.join(format!("l{tail}.hkxbhd")))
    });
    let hint = match binder {
        Some(binder) if binder.is_file() => {
            format!("re-export with `sekiro-extract map {block} --lo-collision`")
        }
        _ => format!("the cache has no raw `l` binder for {block} to export it from"),
    };
    eprintln!(
        "collision: {block}: no {LOW_FILE}, characters and the camera use the high-resolution \
         {HIGH_FILE}, whose swim hits carry no water ({hint})"
    );
}

/// `dir`'s `collision_lo.bin`, if it has one.
pub fn read_low(dir: &Path) -> Result<Option<PartCollision>, String> {
    let path = dir.join(LOW_FILE);
    read_file(&path)?
        .map(|bytes| {
            PartCollision::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))
        })
        .transpose()
}

/// `layout`'s `collisions` rows with each part's range in `low` instead of `collision.bin`
/// (an empty range for a part without an `l` model).
pub fn low_rows(layout: &Value, low: &PartCollision) -> Value {
    let ranges: std::collections::HashMap<&str, [u32; 2]> = low
        .parts
        .iter()
        .map(|p| (p.name.as_str(), [p.first, p.count]))
        .collect();
    let rows: Vec<Value> = layout["collisions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let mut row = row.clone();
            let range = row["name"]
                .as_str()
                .and_then(|n| ranges.get(n))
                .copied()
                .unwrap_or([0, 0]);
            row["triangles"] = serde_json::json!(range);
            row
        })
        .collect();
    serde_json::json!({ "collisions": rows })
}

/// The character collision of the map folder `dir` (`cache/maps/<block>`) with its `layout`:
/// `collision_lo.bin` with the rows re-ranged into it, else `collision.bin` with the layout's
/// own rows (logged).
pub fn read(dir: &Path, layout: &Value) -> Result<BlockMesh, String> {
    if let Some(low) = read_low(dir)? {
        let rows = low_rows(layout, &low);
        return Ok(BlockMesh {
            mesh: low.mesh,
            rows,
        });
    }
    log_fallback(dir);
    let path = dir.join(HIGH_FILE);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(BlockMesh {
        mesh: CollisionMesh::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?,
        rows: serde_json::json!({ "collisions": layout["collisions"].clone() }),
    })
}

/// One block's collision as characters collide with it ([`block_collision`]).
#[derive(Debug, Default)]
pub struct BlockCollision {
    /// The walkable mesh, with the story's switched-off parts and the helpers collapsed.
    pub mesh: CollisionMesh,
    /// The helper parts out of the same mesh ([`crate::hit_filter::strip_helpers`]).
    pub helpers: crate::hit_filter::HelperCollision,
    /// Collision parts `switched_off` named.
    pub story_off: usize,
}

/// The map folder `dir`'s character collision ([`read`]) with the parts `switched_off` names
/// (the story, #129) taken out, then the helpers the player does not collide with: one mesh, so
/// the player, enemies and the camera meet the same triangles and helpers.
pub fn block_collision(
    dir: &Path,
    layout: &Value,
    switched_off: impl Fn(&str) -> bool,
) -> Result<BlockCollision, String> {
    let BlockMesh { mut mesh, rows } = read(dir, layout)?;
    // Before the helpers, so a switched-off helper part does not come back as one.
    let story_off = crate::arenas_a::remove_hidden_collision(&mut mesh, &rows, switched_off);
    let helpers = crate::hit_filter::strip_helpers(&mut mesh, &rows);
    Ok(BlockCollision {
        mesh,
        helpers,
        story_off,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sekiro_formats::hknp::PartRange;

    /// The story and helper steps read a part's range from the rows, so on `l` they must take
    /// out that part's `l` triangles, whatever its range in `collision.bin`.
    #[test]
    fn low_rows_address_the_low_mesh() {
        let tri = |y: f32| [[0.0, y, 0.0], [1.0, y, 0.0], [0.0, y, 1.0]];
        let mut mesh = CollisionMesh::default();
        for (i, y) in [0.0, -5.0, 2.0].into_iter().enumerate() {
            mesh.vertices.extend(tri(y));
            let base = 3 * i as u32;
            mesh.indices.extend([base, base + 1, base + 2]);
            mesh.materials.push(1);
        }
        let low = PartCollision {
            parts: vec![
                PartRange {
                    name: "h000100".into(),
                    first: 0,
                    count: 1,
                },
                PartRange {
                    name: "h900000".into(),
                    first: 1,
                    count: 1,
                },
                PartRange {
                    name: "h000200".into(),
                    first: 2,
                    count: 1,
                },
            ],
            mesh,
        };
        // `collision.bin`'s ranges differ; `h900700` has no `l` model.
        let layout = serde_json::json!({"collisions": [
            {"name": "h000100", "hit_filter_id": 8, "triangles": [0, 40]},
            {"name": "h900000", "hit_filter_id": 15, "triangles": [40, 3]},
            {"name": "h000200", "hit_filter_id": 8, "triangles": [43, 9]},
            {"name": "h900700", "hit_filter_id": 8, "triangles": [52, 2]},
        ]});
        let rows = low_rows(&layout, &low);
        let ranges: Vec<_> = rows["collisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r["name"].as_str().unwrap(), r["triangles"].clone()))
            .collect();
        assert_eq!(
            ranges,
            [
                ("h000100", serde_json::json!([0, 1])),
                ("h900000", serde_json::json!([1, 1])),
                ("h000200", serde_json::json!([2, 1])),
                ("h900700", serde_json::json!([0, 0])),
            ]
        );
        let mut mesh = low.mesh.clone();
        assert_eq!(
            crate::arenas_a::remove_hidden_collision(&mut mesh, &rows, |n| n == "h000200"),
            1
        );
        let helpers = crate::hit_filter::strip_helpers(&mut mesh, &rows);
        assert_eq!(&mesh.indices[..3], &[0, 1, 2]);
        assert_eq!(&mesh.indices[3..6], &[3, 3, 3]);
        assert_eq!(&mesh.indices[6..], &[0, 0, 0]);
        assert_eq!(helpers.triggers.len(), 1);
        assert_eq!(helpers.triggers[0].mesh.vertices[0], [0.0, -5.0, 0.0]);
    }
}
