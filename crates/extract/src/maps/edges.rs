//! Export action edges with exactly the collision mesh's mirror and part placement.
use anyhow::{Context, Result};
use glam::Vec3;
use sekiro_formats::{bxf4, dcx::Oodle, edge, msb};
use serde_json::json;
use std::path::Path;

pub(super) fn export(cache: &Path, id: &str, msb: &msb::Msb, oodle: &Oodle) -> Result<bool> {
    let base = cache.join(format!("raw/map/{id}/{id}"));
    let bhd = std::fs::read(base.with_extension("edgebhd"))?;
    let bdt = std::fs::read(base.with_extension("edgebdt"))?;
    let mut rows = Vec::new();
    let mut counts = [0usize; 4];
    let mut grapple = Vec::new();
    for file in bxf4::parse(&bhd, &bdt)? {
        let data = file.contents(Some(oodle))?;
        let edges = edge::parse(&data).with_context(|| file.name.clone())?;
        let model = format!("h{:06}", edges.owner);
        if !msb.parts.iter().any(|p| {
            p.kind == msb::PartKind::Collision && msb.model(p).is_some_and(|m| m.name == model)
        }) {
            for e in edges.edges.iter().filter(|e| e.kind() == 1) {
                // Retain unplaced owners for census without inventing a world transform.
                grapple.push(json!({ "position": null, "local_position": e.start, "variation": e.variation(), "model": model, "source_part": model }));
            }
        }
        for part in msb.parts.iter().filter(|p| {
            p.kind == msb::PartKind::Collision && msb.model(p).is_some_and(|m| m.name == model)
        }) {
            let matrix = super::mirror(part.matrix());
            let transform = |p: [f32; 3]| {
                matrix
                    .transform_point3(Vec3::new(-p[0], p[1], p[2]))
                    .to_array()
            };
            for e in &edges.edges {
                if e.kind() == 1 {
                    grapple.push(json!({ "position": transform(e.start), "variation": e.variation(), "model": model, "source_part": part.name }));
                }
                if (e.kind() as usize) < counts.len() {
                    counts[e.kind() as usize] += 1;
                }
                rows.push(json!({ "kind": e.kind(), "variation": e.variation(), "start": transform(e.start), "end": transform(e.end), "normal": null, "source_part": part.name, "chain": e.chain, "flags": e.flags }));
            }
        }
    }
    for part in msb.parts.iter().filter(|p| p.kind == msb::PartKind::Object) {
        let Some(model) = msb.model(part) else {
            continue;
        };
        let number = model.name.trim_start_matches('o');
        let path = cache.join(format!("raw/obj/{}.objbnd.d/e{number}.edge", model.name));
        if !path.exists() {
            continue;
        }
        let data = std::fs::read(&path)?;
        let edges = edge::parse(&data).with_context(|| path.display().to_string())?;
        let matrix = super::mirror(part.matrix());
        for e in edges.edges.iter().filter(|e| e.kind() == 1) {
            let p = matrix.transform_point3(Vec3::new(-e.start[0], e.start[1], e.start[2]));
            grapple.push(json!({ "position": p.to_array(), "variation": e.variation(), "model": model.name, "source_part": part.name }));
        }
    }
    let out = cache.join("maps").join(id);
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("edges.json"), serde_json::to_vec_pretty(&rows)?)?;
    std::fs::write(
        out.join("grapple.json"),
        serde_json::to_vec_pretty(&grapple)?,
    )?;
    eprintln!(
        "{id}: {} grapple records ({} placed points)",
        grapple.len(),
        grapple.iter().filter(|r| !r["position"].is_null()).count()
    );
    eprintln!(
        "{id}: {} placed edges (grapple {}, hang {}, cover {})",
        rows.len(),
        counts[1],
        counts[2],
        counts[3]
    );
    Ok(std::env::var_os("SEKIRO_EDGES_ONLY").is_some())
}
