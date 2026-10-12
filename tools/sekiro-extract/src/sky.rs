//! The sky domes of the sky library map m89_00_00_99 -> `<extracted>/sky/sky.json` + their
//! textures (`sky/<stem>.dds`, from other/maptex.tpf).
//!
//! common.emevd event 100 shows one of the library's entity groups 8907000..8907070 (2012[1]):
//! m900000 morning, 010 afternoon, 020 evening, 030 night, 040 past night, 050 dawn, 060 the m10
//! map's, 070 the m25 map's. Each piece has a SkyDome material (SkyDome or SkyDome_Ver2) and
//! m900060 / 070 a second Cloud one. Per model: per material, its shader, MTD params and
//! textures and its mesh (positions in game space, both UV sets, indices).
//! After sekiro-rs (MIT / Apache-2.0) crates/extract/src/maps/sky.rs (9ee18b0).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::{json, Value};

use crate::{flver, mtd};

pub const DOMES: [&str; 8] = ["m900000", "m900010", "m900020", "m900030", "m900040", "m900050", "m900060", "m900070"];

pub fn export(root: &Path) {
    let mtd_dir = root.join("mtd/allmaterialbnd.mtdbnd.d");
    let mtds: HashMap<String, std::path::PathBuf> =
        std::fs::read_dir(&mtd_dir).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().to_lowercase().trim_end_matches(".mtd").to_string(), e.path())).collect();
    let tex: HashMap<String, Vec<u8>> = std::fs::read(root.join("other/maptex.tpf")).map(|d| flver::tpf(&d).into_iter().map(|(n, d)| (n.to_lowercase(), d)).collect()).unwrap_or_default();
    let out = root.join("sky");
    std::fs::create_dir_all(&out).unwrap();
    let mut models = BTreeMap::new();
    for name in DOMES {
        let id = name.trim_start_matches('m');
        let path = root.join(format!("map/m89_00_00_99/m89_00_00_99_{id}.mapbnd.d/m89_00_00_99_{id}.flver"));
        let Ok(data) = std::fs::read(&path) else {
            println!("{name}: no {}", path.display());
            continue;
        };
        let f = flver::read(&data);
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
            println!("{name} {key}: {} ({} verts, {} textures)", d.shader.rsplit('\\').next().unwrap_or(""), m.vertices.len(), textures.len());
            row.insert(key.to_string(), json!({"shader": d.shader, "mtd": m.mtd, "params": params, "textures": textures, "mesh": mesh}));
        }
        models.insert(name.to_string(), Value::Object(row));
    }
    std::fs::write(out.join("sky.json"), serde_json::to_string(&json!({"version": 1, "models": models})).unwrap()).unwrap();
    println!("{} sky domes -> {}", models.len(), out.join("sky.json").display());
}
