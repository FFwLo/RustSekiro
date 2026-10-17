//! The sky domes of the sky library map m89_00_00_99 -> `<extracted>/sky/sky.json` + their
//! textures (`sky/<stem>.dds`, from other/maptex.tpf).
//!
//! common.emevd event 100 shows one of the library's entity groups 8907000..8907070 (2012[1]):
//! m900000 morning, 010 afternoon, 020 evening, 030 night, 040 past night, 050 dawn, 060 the m10
//! map's, 070 the m25 map's. Each piece has a SkyDome material (SkyDome or SkyDome_Ver2) and
//! m900060 / 070 a second Cloud one. Per model: per material, its shader, MTD params and
//! textures and its mesh (positions in game space, both UV sets, indices).
//! Each dome's painted far-mountain range is a separate piece of the same entity group
//! (m89_00_00_99.msb: m900030 and m900130 in 8907030), a ~5 km cylinder with a FarMountain
//! material (m99_farmountain_00_a): exported as the dome's "horizon", with its depth write.
//! After sekiro-rs (MIT / Apache-2.0) crates/extract/src/maps/sky.rs (9ee18b0; the horizon
//! companion after its docs/BACKDROP.md, 2026-10-15).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::{json, Value};

use crate::{flver, msb, mtd};

pub const DOMES: [&str; 8] = ["m900000", "m900010", "m900020", "m900030", "m900040", "m900050", "m900060", "m900070"];

pub fn export(root: &Path) {
    let mtd_dir = root.join("mtd/allmaterialbnd.mtdbnd.d");
    let mtds: HashMap<String, std::path::PathBuf> =
        std::fs::read_dir(&mtd_dir).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().to_lowercase().trim_end_matches(".mtd").to_string(), e.path())).collect();
    let tex: HashMap<String, Vec<u8>> = std::fs::read(root.join("other/maptex.tpf")).map(|d| flver::tpf(&d).into_iter().map(|(n, d)| (n.to_lowercase(), d)).collect()).unwrap_or_default();
    let out = root.join("sky");
    std::fs::create_dir_all(&out).unwrap();
    let library = std::fs::read(root.join("map/mapstudio/m89_00_00_99.msb")).ok().map(|d| msb::read(&d));
    let piece = |id: &str| std::fs::read(root.join(format!("map/m89_00_00_99/m89_00_00_99_{id}.mapbnd.d/m89_00_00_99_{id}.flver"))).ok().map(|d| flver::read(&d));
    let mut models = BTreeMap::new();
    for name in DOMES {
        let Some(f) = piece(name.trim_start_matches('m')) else {
            println!("{name}: no flver");
            continue;
        };
        let mut row = serde_json::Map::new();
        for m in &f.meshes {
            let Some(d) = mtds.get(&m.mtd.to_lowercase()).and_then(|p| mtd::read(&std::fs::read(p).ok()?)) else {
                println!("{name}: no MTD {}", m.mtd);
                continue;
            };
            let key = if d.shader.contains("SkyDome") {
                "dome"
            } else if d.shader.contains("Cloud") {
                "cloud"
            } else {
                continue;
            };
            if row.contains_key(key) {
                continue;
            }
            println!("{name} {key}: {} ({} verts)", d.shader.rsplit(['\\', '/']).next().unwrap_or(""), m.vertices.len());
            row.insert(key.to_string(), material(&d, m, &tex, &out, name));
        }
        // The painted horizon: the other piece of the dome's entity group.
        let parts = library.iter().flat_map(|l| l.parts.iter());
        let groups: Vec<i32> = parts.clone().filter(|p| p.model == name).flat_map(|p| p.groups.clone()).collect();
        for c in parts.filter(|p| p.model != name && p.groups.iter().any(|g| groups.contains(g))).map(|p| p.model.clone()) {
            let Some(f) = piece(c.trim_start_matches('m')) else { continue };
            for m in &f.meshes {
                let Some(d) = mtds.get(&m.mtd.to_lowercase()).and_then(|p| mtd::read(&std::fs::read(p).ok()?)) else { continue };
                if !d.shader.contains("FarMountain") || row.contains_key("horizon") {
                    continue;
                }
                let mut entry = material(&d, m, &tex, &out, name);
                entry["model"] = json!(c);
                let no_depth = d.ints.iter().find(|(k, _)| k == "g_GX_bDisableDepthWrite").and_then(|(_, v)| v.first().copied()).unwrap_or(0);
                entry["depth_write"] = json!(no_depth == 0);
                println!("{name} horizon: {c} {} ({} verts)", m.mtd, m.vertices.len());
                row.insert("horizon".to_string(), entry);
            }
        }
        models.insert(name.to_string(), Value::Object(row));
    }
    std::fs::write(out.join("sky.json"), serde_json::to_string(&json!({"version": 1, "models": models})).unwrap()).unwrap();
    println!("{} sky domes -> {}", models.len(), out.join("sky.json").display());
}

/// A material's row: its shader, MTD params, textures (written from maptex.tpf) and mesh.
fn material(d: &mtd::Mtd, m: &flver::Mesh, tex: &HashMap<String, Vec<u8>>, out: &Path, name: &str) -> Value {
    let mut params = BTreeMap::new();
    for (k, v) in &d.floats {
        params.insert(k.clone(), json!(v));
    }
    for (k, v) in &d.ints {
        params.insert(k.clone(), json!(v));
    }
    let mut textures = BTreeMap::new();
    for t in &d.textures {
        let stem = t.path.rsplit(['\\', '/']).next().unwrap_or("").split('.').next().unwrap_or("").to_lowercase();
        match tex.get(&stem) {
            Some(dds) => {
                std::fs::write(out.join(format!("{stem}.dds")), dds).unwrap();
                textures.insert(t.kind.clone(), format!("sky/{stem}.dds"));
            }
            None if !stem.is_empty() => println!("{name}: texture {stem} not in maptex.tpf"),
            None => {}
        }
    }
    let mesh = json!({
        "pos": m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
        "uv": m.vertices.iter().map(|v| v.uv).collect::<Vec<_>>(),
        "uv2": m.vertices.iter().map(|v| v.uv2).collect::<Vec<_>>(),
        "idx": m.indices,
    });
    json!({"shader": d.shader, "mtd": m.mtd, "params": params, "textures": textures, "mesh": mesh})
}
