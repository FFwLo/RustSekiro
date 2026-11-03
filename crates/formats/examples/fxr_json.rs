//! Converts installed FXRs to the named tree (`fxr_json`) for inspection.
//!
//!   fxr_json <file.fxr>      prints the JSON
//!   fxr_json --export <fxr dir> <out dir>   writes <id>.json for every effect
//!   fxr_json --audit <dir>   counts layout mismatches, unknown actions and skipped properties
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: fxr_json <file.fxr> | --audit <dir>")?;
    if first == "--audit" {
        let dir = PathBuf::from(args.next().ok_or("dir")?);
        let (mut files, mut mismatches, mut skipped) = (0, 0, 0);
        let mut unknown = std::collections::BTreeMap::<u16, usize>::new();
        let mut mods = std::collections::BTreeMap::<(u32, u32, usize), usize>::new();
        let mut bad = std::collections::BTreeMap::<u16, usize>::new();
        for entry in std::fs::read_dir(dir)?.flatten() {
            if entry.path().extension().is_none_or(|e| e != "fxr") {
                continue;
            }
            let Ok(fxr) = sekiro_formats::fxr::parse(&std::fs::read(entry.path())?) else {
                continue;
            };
            for m in &fxr.modifiers {
                *mods
                    .entry((m.tag, m.evaluator, m.values.len()))
                    .or_default() += 1;
            }
            let (_, report) = sekiro_formats::fxr_json::to_json(&fxr);
            files += 1;
            skipped += report.skipped_properties;
            mismatches += report.layout_mismatches.len();
            for a in report.unknown_actions {
                *unknown.entry(a).or_default() += 1;
            }
            for a in report.layout_mismatches {
                *bad.entry(a).or_default() += 1;
            }
        }
        println!("{files} files, {mismatches} layout mismatches, {skipped} skipped properties");
        println!("unknown actions: {unknown:?}");
        println!("mismatching actions: {bad:?}");
        println!("modifiers (tag, words): {mods:x?}");
        return Ok(());
    }
    if first == "--export" {
        let dir = PathBuf::from(args.next().ok_or("in dir")?);
        let out = PathBuf::from(args.next().ok_or("out dir")?);
        std::fs::create_dir_all(&out)?;
        for entry in std::fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().is_none_or(|e| e != "fxr") {
                continue;
            }
            let Ok(fxr) = sekiro_formats::fxr::parse(&std::fs::read(&path)?) else {
                continue;
            };
            let id: u32 = stem.trim_start_matches('f').parse().unwrap_or(0);
            let (value, _) = sekiro_formats::fxr_json::to_json(&fxr);
            std::fs::write(
                out.join(format!("{id}.json")),
                serde_json::to_string(&value)?,
            )?;
        }
        return Ok(());
    }
    let fxr = sekiro_formats::fxr::parse(&std::fs::read(PathBuf::from(first))?)?;
    let (value, report) = sekiro_formats::fxr_json::to_json(&fxr);
    println!("{}", serde_json::to_string_pretty(&value)?);
    eprintln!("{report:?}");
    Ok(())
}
