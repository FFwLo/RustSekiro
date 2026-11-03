//! Unweighted map-object vertices are stored in their node's frame (normal W names the node).
//! The FLVER header bounds are the game's model-space bounds, so exported geometry must fit them.

use super::*;
use rayon::prelude::*;
use sekiro_formats::msb;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAPS: [&str; 2] = ["m11_00_00_00", "m11_01_00_00"];
/// Header bounds are stored as f32 from the authoring tool; 1 cm covers their rounding.
const TOLERANCE: f32 = 0.01;

fn cache() -> PathBuf {
    sekiro_formats::cache::root()
}

/// Every map piece and object model placed in `id`, with its FLVER path.
fn placed_models(cache: &Path, id: &str) -> BTreeMap<String, PathBuf> {
    let msb = super::super::read_msb(cache, id).unwrap();
    let raw = cache.join("raw");
    let mut out = BTreeMap::new();
    for p in &msb.parts {
        let Some(m) = msb.model(p) else { continue };
        let path = match p.kind {
            msb::PartKind::MapPiece => super::super::piece_flver(&raw, id, &m.name),
            msb::PartKind::Object => super::super::object_flver(&raw, &m.name),
            _ => continue,
        };
        if path.is_file() {
            out.insert(m.name.clone(), path);
        }
    }
    out
}

fn bounds_error(lo: Vec3, hi: Vec3, f: &flver::Flver) -> f32 {
    let (a, b) = (Vec3::from(f.bbox_min), Vec3::from(f.bbox_max));
    (lo - a)
        .abs()
        .max_element()
        .max((hi - b).abs().max_element())
}

#[test]
fn placed_models_fit_their_header_bounds_in_model_space() {
    let cache = cache();
    if !cache.join("raw/map/mapstudio/m11_00_00_00.msb").is_file() {
        eprintln!("installed-map fixture unavailable");
        return;
    }
    let mut checked = 0;
    let mut moved = 0;
    for id in MAPS {
        let models = placed_models(&cache, id);
        let results: Vec<_> = models
            .par_iter()
            .map(|(name, path)| {
                let f = flver::parse(&std::fs::read(path).unwrap()).unwrap();
                let world = flver::world_matrices(&f.nodes);
                let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
                let mut any_moved = false;
                for mesh in &f.meshes {
                    let v = mesh.model_space_vertices(&world);
                    any_moved |= matches!(v, std::borrow::Cow::Owned(_));
                    for p in &v.positions {
                        lo = lo.min(Vec3::from(*p));
                        hi = hi.max(Vec3::from(*p));
                    }
                }
                let empty = lo.x > hi.x;
                (name.clone(), empty, any_moved, bounds_error(lo, hi, &f))
            })
            .collect();
        for (name, empty, any_moved, error) in results {
            if empty {
                continue;
            }
            checked += 1;
            moved += usize::from(any_moved);
            assert!(
                error <= TOLERANCE,
                "{id} {name}: model-space vertices miss the header bounds by {error} m"
            );
        }
    }
    // Both maps place hundreds of node-bound objects; a regression to raw positions fails above.
    assert!(checked > 1000, "only {checked} models checked");
    assert!(moved > 100, "only {moved} models have node-bound vertices");
}

#[test]
fn outskirts_bamboo_exports_leaves_on_the_stalks() {
    let raw = cache().join("raw");
    if !raw.join("obj/o112440.objbnd.d/o112440.flver").is_file() {
        eprintln!("installed-object fixture unavailable");
        return;
    }
    let mtds = super::super::load_mtds(&raw).unwrap();
    // The clumps around the Ashina Outskirts start house.
    for model in ["o112400", "o112410", "o112420", "o112430", "o112440"] {
        let f =
            flver::parse(&std::fs::read(super::super::object_flver(&raw, model)).unwrap()).unwrap();
        let doc = lod_doc(&build(&f, &mtds, &|_| None).unwrap().glb);
        let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
        for primitive in doc["meshes"][0]["primitives"].as_array().unwrap() {
            let accessor =
                &doc["accessors"][primitive["attributes"]["POSITION"].as_u64().unwrap() as usize];
            let v = |k: &str| {
                let a = accessor[k].as_array().unwrap();
                Vec3::new(
                    a[0].as_f64().unwrap() as f32,
                    a[1].as_f64().unwrap() as f32,
                    a[2].as_f64().unwrap() as f32,
                )
            };
            lo = lo.min(v("min"));
            hi = hi.max(v("max"));
        }
        // Export mirrors x; the header is in FromSoftware space.
        let header_lo = Vec3::new(-f.bbox_max[0], f.bbox_min[1], f.bbox_min[2]);
        let header_hi = Vec3::new(-f.bbox_min[0], f.bbox_max[1], f.bbox_max[2]);
        let error = (lo - header_lo)
            .abs()
            .max_element()
            .max((hi - header_hi).abs().max_element());
        assert!(
            error <= TOLERANCE,
            "{model}: LOD0 spans {lo}..{hi}, header {header_lo}..{header_hi}"
        );
        // Stalks stand: nothing reaches more than 5 m sideways or dips below the root.
        let reach = [lo.x, lo.z, hi.x, hi.z]
            .into_iter()
            .fold(0f32, |m, c| m.max(c.abs()));
        assert!(lo.y > -0.1 && reach < 5.0, "{model}: LOD0 spans {lo}..{hi}");
        // Leaf clusters hang in the crown, not at the root.
        let world = flver::world_matrices(&f.nodes);
        let leaves = f
            .meshes
            .iter()
            .filter(|m| f.materials[m.material].mtd.contains("Pivot_Parent_Child"))
            .flat_map(|m| m.model_space_vertices(&world).positions.clone())
            .collect::<Vec<_>>();
        assert!(!leaves.is_empty(), "{model}: no leaf clusters");
        let mean_height = leaves.iter().map(|p| p[1]).sum::<f32>() / leaves.len() as f32;
        assert!(mean_height > 4.0, "{model}: leaves sit {mean_height} m up");
    }
}

fn lod_doc(bytes: &[u8]) -> Value {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}
