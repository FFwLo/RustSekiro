//! BTL sidecar export, preserving bank identity and all undecoded bytes.
use anyhow::{Context, Result};
use sekiro_formats::btl;
use serde_json::json;
use std::path::Path;

pub(super) fn export(raw: &Path, out: &Path, id: &str) -> Result<()> {
    let mut paths = std::fs::read_dir(raw.join("map").join(id))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "btl"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut banks = Vec::new();
    let mut count = 0;
    for path in paths {
        let lights =
            btl::parse(&std::fs::read(&path)?).with_context(|| path.display().to_string())?;
        count += lights.len();
        let rows = lights
            .into_iter()
            .map(|l| {
                if l.kind > 1 {
                    eprintln!("unknown BTL light kind {}: {}", l.kind, l.name);
                }
                json!({ "name": l.name, "kind": l.kind, "flag_1c": l.flag_1c,
                "color": l.color, "intensity": l.intensity,
                "specular_color": l.specular_color, "specular_intensity": l.specular_intensity,
                "cast_shadows": l.cast_shadows, "cone_angle": l.cone_angle,
                "near_clip": l.near_clip, "sharpness": l.sharpness, "width": l.width,
                "position": [-l.position[0], l.position[1], l.position[2]],
                "rotation": l.rotation, "range": l.range, "raw": l.raw })
            })
            .collect::<Vec<_>>();
        banks
            .push(json!({ "source": path.file_name().unwrap().to_string_lossy(), "lights": rows }));
    }
    std::fs::write(
        out.join("lights.json"),
        serde_json::to_vec_pretty(&json!({ "version": 1, "banks": banks }))?,
    )?;
    eprintln!("BTL {id}: {count} lights in {} banks", banks.len());
    Ok(())
}
