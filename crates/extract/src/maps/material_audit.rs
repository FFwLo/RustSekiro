//! Cache-relative material/model provenance, independent of runtime rendering policy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use sekiro_formats::{bnd4, dcx::Oodle, flver, msb};
use serde_json::{Value, json};

use super::{MapTextures, Mtds, object_flver, piece_flver, pieces, read_msb};
use crate::models::textures::texture_stem;

/// Validate cached DDS files too: earlier exports may have stored a wrong-name substitute.
/// Set SEKIRO_MAP_AUDIT_ALL to audit every cached MSB; only the requested map is repaired.
pub fn export(
    cache: &Path,
    id: &str,
    oodle: &'static Oodle,
    mtds: &Mtds,
    store: &MapTextures,
) -> Result<()> {
    let mut maps = vec![id.to_owned()];
    if std::env::var_os("SEKIRO_MAP_AUDIT_ALL").is_some() {
        for entry in std::fs::read_dir(cache.join("raw/map/mapstudio"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "msb") {
                let name = path
                    .file_stem()
                    .context("MSB name")?
                    .to_string_lossy()
                    .into_owned();
                if !maps.contains(&name) {
                    maps.push(name);
                }
            }
        }
        // The two reported maps precede the remaining maps in the aggregate inventory.
        maps.sort_by_key(|name| {
            (
                match name.as_str() {
                    "m11_00_00_00" => 0,
                    "m11_01_00_00" => 1,
                    _ => 2,
                },
                name.clone(),
            )
        });
    }
    let mut summaries = Vec::new();
    for map in maps {
        summaries.push(audit_map(cache, &map, map == id, oodle, mtds, store)?);
    }
    if std::env::var_os("SEKIRO_MAP_AUDIT_ALL").is_some() {
        std::fs::write(
            cache.join("maps/material-audit.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1, "maps": summaries,
            }))?,
        )?;
    }
    Ok(())
}

fn model_data(
    raw: &Path,
    map: &str,
    model: &str,
    oodle: &'static Oodle,
) -> Result<(Option<Vec<u8>>, Value)> {
    let path = if model.starts_with('o') {
        object_flver(raw, model)
    } else {
        piece_flver(raw, map, model)
    };
    let relative = |p: &Path| {
        p.strip_prefix(raw)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    if path.is_file() {
        return Ok((
            Some(std::fs::read(&path)?),
            json!({"file": relative(&path)}),
        ));
    }
    let binder = path.parent().context("model parent")?.with_extension("");
    if model.starts_with('o') && binder.is_file() {
        let bytes = std::fs::read(&binder)?;
        let files = bnd4::parse(&bytes)?;
        let names: Vec<_> = files.iter().map(|f| f.file_name()).collect();
        // Recover the requested model from its binder, including compressed members.
        if let Some(file) = files.iter().find(|f| {
            let name = f.file_name().to_ascii_lowercase();
            name == format!("{model}.flver") || name == format!("{model}.flver.dcx")
        }) {
            return Ok((
                Some(file.contents(Some(oodle))?),
                json!({
                    "file": relative(&path), "recovered_from": relative(&binder), "member": file.name,
                }),
            ));
        }
        return Ok((
            None,
            json!({
                "file": relative(&path), "binder": relative(&binder), "members": names,
                "reason": if super::physics_only_members(names.iter().copied()) {
                    "physics_only_binder"
                } else { "missing_render_member" },
            }),
        ));
    }
    Ok((
        None,
        json!({"file": relative(&path), "reason": "model_file_and_binder_absent"}),
    ))
}

fn audit_map(
    cache: &Path,
    id: &str,
    repair: bool,
    oodle: &'static Oodle,
    mtds: &Mtds,
    store: &MapTextures,
) -> Result<Value> {
    let raw = cache.join("raw");
    let out = cache.join("maps").join(id);
    std::fs::create_dir_all(&out)?;
    let msb = read_msb(cache, id)?;
    let layout_path = out.join("layout.json");
    let mut layout: Value = if layout_path.is_file() {
        serde_json::from_slice(&std::fs::read(&layout_path)?)?
    } else {
        json!({"models": {}})
    };
    let mut models = BTreeMap::<String, Vec<String>>::new();
    for part in &msb.parts {
        if matches!(part.kind, msb::PartKind::MapPiece | msb::PartKind::Object)
            && let Some(model) = msb.model(part)
        {
            models
                .entry(model.name.clone())
                .or_default()
                .push(part.name.clone());
        }
    }
    let mut model_rows = Vec::new();
    let mut legacy_names = BTreeSet::new();
    let mut legacy_paths = vec![raw.join("other/maptex.tpf")];
    legacy_paths.extend(
        models
            .keys()
            .filter(|m| m.starts_with('o'))
            .map(|m| object_flver(&raw, m).with_extension("tpf")),
    );
    for path in legacy_paths {
        if path.is_file() {
            let data = std::fs::read(path)?;
            legacy_names.extend(
                sekiro_formats::tpf::parse(&data)?
                    .into_iter()
                    .map(|t| t.name.to_ascii_lowercase()),
            );
        }
    }
    let mut legacy_missing = BTreeSet::new();
    let mut legacy_wrong_first = BTreeSet::new();
    let mut texture_rows = Vec::new();
    let mut materials = Vec::new();
    let mut missing = BTreeSet::new();
    let mut errors = BTreeMap::new();
    let mut checked = BTreeMap::<(String, bool), bool>::new();
    let mut repaired_dds = BTreeSet::new();
    let mut repaired_models = 0usize;
    for (model, instances) in models {
        let loaded = model_data(&raw, id, &model, oodle);
        let (data, provenance) = match loaded {
            Ok(v) => v,
            Err(e) => {
                model_rows.push(json!({"model": model, "instances": instances,
                "classification": "model_read_error", "reason": format!("{e:#}")}));
                continue;
            }
        };
        let Some(data) = data else {
            model_rows.push(json!({"model": model, "instances": instances,
                "classification": provenance["reason"], "source": provenance}));
            continue;
        };
        let f = match flver::parse(&data) {
            Ok(f) => f,
            Err(e) => {
                model_rows.push(json!({"model": model, "instances": instances,
                "classification": "flver_parse_error", "source": provenance, "reason": e.to_string()}));
                continue;
            }
        };
        let wanted: BTreeSet<_> = pieces::wanted_textures(&f, mtds).into_iter().collect();
        for (name, normal) in &wanted {
            let key = (name.clone(), *normal);
            if checked.contains_key(&key) {
                continue;
            }
            let source_name = name.strip_suffix("_map_raw").unwrap_or(name);
            let before = store.legacy_status(source_name, &legacy_names);
            if before == "missing" {
                legacy_missing.insert(name.clone());
            }
            if before == "wrong_first_texture" {
                legacy_wrong_first.insert(name.clone());
            }
            let result = store.dds(source_name).and_then(|d| match d {
                Some(d) if *normal && repair => super::textures::normal_to_bc5(&d).map(Some),
                other => Ok(other),
            });
            let available = match result {
                Ok(Some(dds)) => {
                    if repair {
                        let file_name = if *normal && checked.contains_key(&(name.clone(), false)) {
                            format!("{name}_normal_bc5")
                        } else {
                            name.clone()
                        };
                        let path = out.join(format!("models/tex/{file_name}.dds"));
                        if std::fs::read(&path).ok().as_deref() != Some(dds.as_slice()) {
                            std::fs::create_dir_all(path.parent().context("DDS parent")?)?;
                            std::fs::write(path, dds)?;
                            repaired_dds.insert(name.clone());
                        }
                    }
                    true
                }
                Ok(None) => {
                    missing.insert(name.clone());
                    false
                }
                Err(e) => {
                    missing.insert(name.clone());
                    errors.insert(name.clone(), format!("{e:#}"));
                    false
                }
            };
            checked.insert(key, available);
            texture_rows.push(json!({"export_texture": name, "internal_texture": source_name,
                "convert_normal_to_bc5": normal, "before_fresh_resolution": before,
                "source": store.provenance(source_name)?, "available": available,
                "reason": errors.get(name).map(String::as_str).or(if available { None } else { Some("no exact internal texture name in searched sources") }),
                "rejected_wrong_first_texture": store.rejected_aliases.get(source_name)}));
        }
        for (index, mat) in f.materials.iter().enumerate() {
            let mtd = mtds.get(&mat.mtd);
            let choice = pieces::choose_material(mat, mtd);
            let mut slots = Vec::new();
            let mut params: BTreeSet<&str> =
                mat.textures.iter().map(|t| t.param.as_str()).collect();
            if let Some(mtd) = mtd {
                params.extend(mtd.textures.iter().map(|t| t.kind.as_str()));
            }
            for param in params {
                let flver_path = mat
                    .textures
                    .iter()
                    .find(|t| t.param == param)
                    .map(|t| t.path.as_str())
                    .unwrap_or("");
                let mtd_path = mtd
                    .and_then(|m| m.textures.iter().find(|t| t.kind == param))
                    .map(|t| t.path.as_str())
                    .unwrap_or("");
                let path = if flver_path.is_empty() {
                    mtd_path
                } else {
                    flver_path
                };
                let name = texture_stem(path).to_ascii_lowercase();
                let source = if path.is_empty() {
                    None
                } else {
                    store.provenance(&name)?
                };
                slots.push(json!({"slot": param, "path": path, "texture": name,
                    "path_source": if !flver_path.is_empty() { "FLVER" } else { "MTD" },
                    "fallback": if flver_path.is_empty() && !mtd_path.is_empty() { Some("empty FLVER slot resolved from MTD") } else { None },
                    "source": source,
                    "status": if path.is_empty() { "empty_slot" } else if source.is_some() { "resolved" } else { "missing" },
                    "reason": if path.is_empty() { Some("no path in FLVER or matching MTD slot") }
                        else if source.is_none() && Path::new(path).extension().is_none() && path.contains(['/', '\\']) {
                            Some("authored path has no texture extension and no matching internal texture name")
                        } else if source.is_none() { Some("no exact internal texture name in searched sources") } else { None },
                    "search_scope": "searched_sources (cache-relative paths and archive members)",
                    "rejected_wrong_first_texture": store.rejected_aliases.get(&name),
                }));
            }
            let used = f.meshes.iter().any(|mesh| mesh.material == index);
            let layer_inputs = choice
                .layers
                .as_ref()
                .filter(|l| matches!(l.kind, 1 | 2))
                .map(|layer| {
                    let texture = |t: &Option<pieces::TexRef>| {
                        t.as_ref().map(|t| {
                            json!({"texture": texture_stem(&t.path),
                        "uv": t.uv, "scale": t.scale})
                        })
                    };
                    json!({"kind": layer.kind, "variant": layer.variant,
                    "snow": texture(&layer.snow), "snow_normal": texture(&layer.snow_normal),
                    "height_mask": texture(&layer.mask), "detail": texture(&layer.detail),
                    "wet": texture(&layer.wet), "base_uv": texture(&choice.albedo),
                    "params": layer.params,
                    "snow_vertex": {"packed_stream": 4, "channel": 1},
                    "detail_vertex": {"packed_stream": 5, "channel": 2},
                    "wet_vertex": {"packed_stream": 3, "channel": 0},
                    "snow_height_channels": "linear RGB dot params[13].xyz",
                    "snow_weather_scale": "none in Multiple/MultipleGround pixel inputs"})
                });
            materials.push(json!({"model": model, "material_index": index, "material": mat.name,
                "mtd": mat.mtd, "mtd_found": mtd.is_some(), "mesh_uses_material": used,
                "renderer_skips_material": pieces::skip_material(&choice.mtd),
                "base_colour_fallback": if choice.albedo.is_none() {
                    Some("no selected albedo; exporter uses constant material colour")
                } else if choice.albedo.as_ref().is_some_and(|t| {
                    let name = texture_stem(&t.path).to_ascii_lowercase();
                    checked.get(&(name, false)) == Some(&false)
                }) { Some("selected albedo is missing; exporter uses constant material colour") } else { None },
                "selected_albedo": choice.albedo.as_ref().map(|t| &t.path),
                "selected_normal": choice.normal.as_ref().map(|t| &t.path),
                "layer_inputs": layer_inputs, "slots": slots}));
        }
        let visible = f
            .meshes
            .iter()
            .filter(|mesh| {
                mesh.lod0().is_some() && !pieces::skip_material(&f.materials[mesh.material].mtd)
            })
            .count();
        let classification = if f.meshes.is_empty() {
            "meshless_helper"
        } else if f
            .meshes
            .iter()
            .all(|m| pieces::skip_material(&f.materials[m.material].mtd))
        {
            if f.materials
                .iter()
                .any(|m| m.mtd.to_ascii_lowercase().contains("fog"))
            {
                "fog_renderer_handoff"
            } else {
                "invisible_shadow_helper"
            }
        } else if visible == 0 {
            "no_exportable_lod0"
        } else {
            "render_model"
        };
        let mut export_error = None;
        // A missing member in the expanded cache can be recovered directly from the binder.
        // Rebuild materials with missing DDS references so stale cached images cannot hide failures.
        if repair
            && visible > 0
            && (layout["models"][&model].is_null()
                || !out.join(format!("models/{model}.glb")).is_file()
                || wanted.iter().any(|key| !checked[key])
                || wanted.iter().any(|(name, _)| repaired_dds.contains(name)))
        {
            let uri = |name: &str| {
                checked
                    .iter()
                    .any(|((n, _), ok)| n == name && *ok)
                    .then(|| format!("tex/{name}.dds"))
            };
            match pieces::build(&f, mtds, &uri) {
                Ok(built) if built.primitives > 0 => {
                    std::fs::create_dir_all(out.join("models"))?;
                    std::fs::write(out.join(format!("models/{model}.glb")), &built.glb)?;
                    layout["models"][&model] = json!({"glb": format!("models/{model}.glb"),
                        "triangles": built.triangles, "primitives": built.primitives,
                        "min": built.bbox_min, "max": built.bbox_max});
                    repaired_models += 1;
                }
                Ok(_) => export_error = Some("geometry builder returned no primitives".to_owned()),
                Err(e) => export_error = Some(format!("{e:#}")),
            }
        }
        model_rows.push(json!({"model": model, "instances": instances, "source": provenance,
            "classification": classification, "meshes": f.meshes.len(), "lod0_visible_meshes": visible,
            "materials": f.materials.iter().map(|m| &m.mtd).collect::<Vec<_>>(),
            "has_exported_glb": !layout["models"][&model].is_null() && out.join(format!("models/{model}.glb")).is_file(), "export_error": export_error}));
    }
    let role_conflicts: BTreeSet<String> = checked
        .iter()
        .filter(|((name, normal), available)| {
            *normal && **available && checked.get(&(name.clone(), false)) == Some(&true)
        })
        .map(|((name, _), _)| name.clone())
        .collect();
    if repair {
        for name in &role_conflicts {
            let dds = store.dds(name)?.context("resolved texture disappeared")?;
            for (file_name, bytes) in [
                (name.clone(), dds.clone()),
                (
                    format!("{name}_normal_bc5"),
                    super::textures::normal_to_bc5(&dds)?,
                ),
            ] {
                let path = out.join(format!("models/tex/{file_name}.dds"));
                if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                    std::fs::write(path, bytes)?;
                    repaired_dds.insert(name.clone());
                }
            }
        }
        for row in &model_rows {
            if row["has_exported_glb"] == true {
                let model = row["model"].as_str().context("model name")?;
                let path = out.join(format!("models/{model}.glb"));
                let glb = std::fs::read(&path)?;
                if let Some(patched) = separate_normal_images(&glb, &role_conflicts)? {
                    std::fs::write(path, patched)?;
                }
            }
        }
        std::fs::write(layout_path, serde_json::to_vec_pretty(&layout)?)?;
    }
    let mut classes = BTreeMap::<String, usize>::new();
    for row in &model_rows {
        if row["model"].as_str().is_some_and(|m| m.starts_with('o'))
            && row["has_exported_glb"] != true
        {
            *classes
                .entry(
                    row["classification"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                )
                .or_default() += row["instances"].as_array().map_or(0, Vec::len);
        }
    }
    let summary = json!({"map": id, "missing_export_textures": missing.len(),
        "before_fresh_missing_textures": legacy_missing.len(), "before_wrong_first_textures": legacy_wrong_first.len(),
        "missing_object_instances_by_cause": classes, "repaired_cached_dds": repaired_dds.len(),
        "rebuilt_models": repaired_models});
    let report = json!({"schema_version": 1, "summary": summary, "assets_repaired": repair, "searched_sources": store.searched,
        "source_errors": store.source_errors,
        "texture_role_conflicts": role_conflicts.iter().map(|name| json!({"texture": name,
            "reason": "same authored texture used as colour and normal; separate DDS payloads required",
            "colour_uri": format!("tex/{name}.dds"), "normal_uri": format!("tex/{name}_normal_bc5.dds")})).collect::<Vec<_>>(),
        "legacy_missing_export_textures": legacy_missing, "legacy_wrong_first_export_textures": legacy_wrong_first,
        "repaired_cached_dds": repaired_dds,
        "missing_export_textures": missing, "texture_errors": errors,
        "export_textures": texture_rows,
        "models": model_rows, "materials": materials});
    std::fs::write(
        out.join("material-audit.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    eprintln!("  material audit: {summary}");
    Ok(summary)
}

/// The geometry builder shares images by name; split only conflicting normal uses.
/// Preserve the binary chunk and every material property, including texture transforms.
fn separate_normal_images(glb: &[u8], conflicts: &BTreeSet<String>) -> Result<Option<Vec<u8>>> {
    if conflicts.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(
        glb.get(..4) == Some(b"glTF") && glb.len() >= 20,
        "invalid GLB header"
    );
    let json_len = u32::from_le_bytes(glb[12..16].try_into()?) as usize;
    let json_end = 20usize
        .checked_add(json_len)
        .context("GLB JSON length overflow")?;
    let mut doc: Value =
        serde_json::from_slice(glb.get(20..json_end).context("truncated GLB JSON")?)?;
    let mut changed = false;
    for material in 0..doc["materials"].as_array().map_or(0, Vec::len) {
        let Some(index) = doc["materials"][material]["normalTexture"]["index"].as_u64() else {
            continue;
        };
        let mut texture = doc["textures"][index as usize].clone();
        let Some(source) = texture["source"].as_u64() else {
            continue;
        };
        let mut image = doc["images"][source as usize].clone();
        let Some(uri) = image["uri"].as_str() else {
            continue;
        };
        let Some(name) = uri
            .strip_prefix("tex/")
            .and_then(|s| s.strip_suffix(".dds"))
        else {
            continue;
        };
        if !conflicts.contains(name) {
            continue;
        }
        image["uri"] = json!(format!("tex/{name}_normal_bc5.dds"));
        let images = doc["images"].as_array_mut().context("GLB images")?;
        texture["source"] = json!(images.len());
        images.push(image);
        let textures = doc["textures"].as_array_mut().context("GLB textures")?;
        let new_index = textures.len();
        textures.push(texture);
        doc["materials"][material]["normalTexture"]["index"] = json!(new_index);
        changed = true;
    }
    if !changed {
        return Ok(None);
    }
    let mut json = serde_json::to_vec(&doc)?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 20 + json.len() + glb.len() - json_end;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&glb[..8]);
    out.extend_from_slice(&u32::try_from(total)?.to_le_bytes());
    out.extend_from_slice(&u32::try_from(json.len())?.to_le_bytes());
    out.extend_from_slice(&glb[16..20]);
    out.extend_from_slice(&json);
    out.extend_from_slice(&glb[json_end..]);
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_colour_and_normal_images_split_without_changing_geometry() {
        let doc = json!({"images": [{"uri": "tex/shared.dds"}], "textures": [{"source": 0}],
            "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}},
                "normalTexture": {"index": 0, "scale": 0.5}}]});
        let mut json = serde_json::to_vec(&doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let tail = [4u8, 0, 0, 0, b'B', b'I', b'N', 0, 1, 2, 3, 4];
        let mut glb = b"glTF\x02\0\0\0".to_vec();
        glb.extend_from_slice(&((20 + json.len() + tail.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&tail);
        let patched = separate_normal_images(&glb, &BTreeSet::from(["shared".into()]))
            .unwrap()
            .unwrap();
        let len = u32::from_le_bytes(patched[12..16].try_into().unwrap()) as usize;
        let doc: Value = serde_json::from_slice(&patched[20..20 + len]).unwrap();
        assert_eq!(doc["images"][0]["uri"], "tex/shared.dds");
        assert_eq!(doc["images"][1]["uri"], "tex/shared_normal_bc5.dds");
        assert_eq!(doc["materials"][0]["normalTexture"]["scale"], 0.5);
        assert_eq!(
            doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
            0
        );
        assert_eq!(&patched[20 + len..], &tail);
        assert!(
            separate_normal_images(&patched, &BTreeSet::from(["shared".into()]))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ashina_omissions_are_authored_empty_flvers_not_export_failures() {
        let cache = sekiro_formats::cache::root();
        for model in ["o110000", "o111690", "o111751"] {
            let path = object_flver(&cache.join("raw"), model);
            if !path.is_file() {
                continue;
            }
            let bytes = std::fs::read(path).unwrap();
            let f = flver::parse(&bytes).unwrap();
            assert!(
                f.meshes.is_empty(),
                "{model} contains authored render geometry"
            );
        }
    }
}
