//! Sky-library material inputs, alongside the existing m89 GLBs.
use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result};
use sekiro_formats::{
    dcx::Oodle,
    flver,
    mtd::{Mtd, ParamValue},
};
use serde_json::{Value, json};

pub(super) fn export(raw: &Path, out: &Path, oodle: &'static Oodle) -> Result<()> {
    let mtds = super::load_mtds(raw)?;
    let textures = super::textures::MapTextures::open(raw, oodle).context("sky texture store")?;
    let mut models = BTreeMap::new();
    let msb =
        sekiro_formats::msb::parse(&std::fs::read(raw.join("map/mapstudio/m89_00_00_99.msb"))?)?;
    let layout: Value = serde_json::from_slice(&std::fs::read(out.join("layout.json"))?)?;
    std::fs::create_dir_all(out.join("sky"))?;
    for name in layout["models"]
        .as_object()
        .context("sky library models")?
        .keys()
    {
        if !name.starts_with("m900") {
            continue;
        }
        let f = flver::parse(
            &std::fs::read(super::piece_flver(raw, "m89_00_00_99", name))
                .with_context(|| format!("sky model {name}"))?,
        )?;
        let Some((material_index, m)) = f
            .materials
            .iter()
            .enumerate()
            .filter_map(|(i, m)| mtds.get(&m.mtd).map(|m| (i, m)))
            .find(|(_, m)| m.shader.contains("SkyDome"))
        else {
            continue;
        };
        let mut row = material(m, material_index, &textures, out)?;
        if let Some((index, cloud)) = f
            .materials
            .iter()
            .enumerate()
            .filter_map(|(i, m)| mtds.get(&m.mtd).map(|m| (i, m)))
            .find(|(_, m)| m.shader.ends_with("M[Sky]_Cloud.spx"))
        {
            row["cloud"] = material(cloud, index, &textures, out)?;
        }
        // The m89 event group enables both the dome and its separate painted horizon.
        // Derive the association from the MSB, including special-story variants.
        let groups: Vec<_> = msb
            .parts
            .iter()
            .filter(|p| msb.model(p).is_some_and(|m| m.name == *name))
            .flat_map(|p| p.entity_group_ids.iter().copied().filter(|&id| id > 0))
            .collect();
        for part in &msb.parts {
            if !part.entity_group_ids.iter().any(|id| groups.contains(id)) {
                continue;
            }
            let Some(companion) = msb.model(part).filter(|m| m.name != *name) else {
                continue;
            };
            let far = flver::parse(&std::fs::read(super::piece_flver(
                raw,
                "m89_00_00_99",
                &companion.name,
            ))?)?;
            for (index, m) in far.materials.iter().enumerate() {
                let Some(m) = mtds
                    .get(&m.mtd)
                    .filter(|m| m.shader.contains("FarMountain"))
                else {
                    continue;
                };
                let mut horizon = material(m, index, &textures, out)?;
                horizon["model"] = json!(companion.name);
                horizon["depth_write"] = json!(!matches!(
                    m.param("g_GX_bDisableDepthWrite"),
                    Some(ParamValue::Bool(true))
                ));
                row["horizon"] = horizon;
            }
        }
        models.insert(name.clone(), row);
    }
    std::fs::write(
        out.join("sky.json"),
        serde_json::to_vec_pretty(&json!({"version":1,"models":models}))?,
    )?;
    eprintln!("sky library: {} authored materials", models.len());
    Ok(())
}

fn material(
    m: &Mtd,
    material_index: usize,
    textures: &super::textures::MapTextures,
    out: &Path,
) -> Result<Value> {
    let params: BTreeMap<_, _> = m
        .params
        .iter()
        .filter_map(|p| match &p.value {
            ParamValue::Floats(v) => Some((p.name.clone(), json!(v))),
            ParamValue::Ints(v) => Some((p.name.clone(), json!(v))),
            ParamValue::Bool(_) => None,
        })
        .collect();
    let mut samplers = BTreeMap::new();
    for t in &m.textures {
        let stem = crate::models::textures::texture_stem(&t.path).to_lowercase();
        if stem.is_empty() {
            continue;
        }
        if let Some(dds) = textures.dds(&stem)? {
            std::fs::write(out.join(format!("sky/{stem}.dds")), dds)?;
            samplers.insert(t.kind.clone(), format!("sky/{stem}.dds"));
        }
    }
    Ok(
        json!({"shader":m.shader, "material_index":material_index, "params":params, "textures":samplers}),
    )
}
