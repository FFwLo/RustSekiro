//! `sekiro-extract params`: decodes the unpacked PARAM binders in cache/raw with the
//! Paramdex defs in cache/refs and writes one JSON file per table to cache/params.

use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use rayon::prelude::*;
use sekiro_formats::param::{ParamIssue, ParamSet};
use sekiro_formats::paramdef::ParamDefs;
use serde::Serialize;

/// Binder folders to decode: (label, folder under cache/raw, output folder under cache/params).
const SETS: &[(&str, &str, &str)] = &[
    ("gameparam", "param/gameparam/gameparam.parambnd.d", ""),
    (
        "graphicsconfig",
        "param/graphicsconfig/graphicsconfig.parambnd.d",
        "graphicsconfig",
    ),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TableSummary {
    table: String,
    param_type: String,
    def: String,
    rows: usize,
    row_size: usize,
    data_version: u16,
    duplicate_ids: usize,
    named_rows: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetSummary {
    name: &'static str,
    source: String,
    output: String,
    tables: Vec<TableSummary>,
    issues: Vec<ParamIssue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    paramdex_defs: usize,
    def_errors: Vec<DefError>,
    sets: Vec<SetSummary>,
}

#[derive(Serialize)]
struct DefError {
    file: String,
    error: String,
}

pub fn export(cache: &Path) -> Result<()> {
    let paramdex = cache.join("refs/paramdex");
    let defs = ParamDefs::load_dir(&paramdex.join("Defs"))
        .context("cache/refs/paramdex/Defs missing: run tools/fetch-refs.ps1")?;
    let names = paramdex.join("Names");
    let names = names.is_dir().then_some(names.as_path());
    let out_root = cache.join("params");

    let mut summary = Summary {
        paramdex_defs: defs.len(),
        def_errors: defs
            .errors()
            .iter()
            .map(|(file, error)| DefError {
                file: file.clone(),
                error: error.clone(),
            })
            .collect(),
        sets: Vec::new(),
    };
    for err in &summary.def_errors {
        eprintln!("bad def {}: {}", err.file, err.error);
    }

    for &(label, raw, sub) in SETS {
        let dir = cache.join("raw").join(raw);
        if !dir.is_dir() {
            eprintln!(
                "skipping {label}: {} missing (run sekiro-extract unpack)",
                dir.display()
            );
            continue;
        }
        let set = ParamSet::load_with(&dir, &defs, names)?;
        let out = out_root.join(sub);
        std::fs::create_dir_all(&out)?;
        let tables: Vec<_> = set.tables().collect();
        tables.par_iter().try_for_each(|table| -> Result<()> {
            let path = out.join(format!("{}.json", table.name()));
            let mut w = BufWriter::new(std::fs::File::create(&path)?);
            serde_json::to_writer(&mut w, table)?;
            w.flush()?;
            Ok(())
        })?;
        for issue in set.issues() {
            eprintln!("{label}/{}: {:?}", issue.table, issue.kind);
        }
        let failed = set.issues().iter().filter(|i| i.is_fatal()).count();
        eprintln!(
            "{label}: wrote {} tables to {} ({failed} failed)",
            set.len(),
            out.display()
        );
        summary.sets.push(SetSummary {
            name: label,
            source: dir.display().to_string(),
            output: out.display().to_string(),
            tables: set
                .tables()
                .map(|t| TableSummary {
                    table: t.name().to_string(),
                    param_type: t.def().param_type.clone(),
                    def: t.def().source.clone(),
                    rows: t.len(),
                    row_size: t.def().size,
                    data_version: t.data_version(),
                    duplicate_ids: t.duplicate_ids(),
                    named_rows: t.rows().filter(|r| r.name().is_some()).count(),
                })
                .collect(),
            issues: set.issues().to_vec(),
        });
    }

    std::fs::create_dir_all(&out_root)?;
    let path = out_root.join("_summary.json");
    let mut w = BufWriter::new(std::fs::File::create(&path)?);
    serde_json::to_writer_pretty(&mut w, &summary)?;
    w.flush()?;
    eprintln!("summary: {}", path.display());
    Ok(())
}
