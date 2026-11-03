//! Optional installed-data coverage; proprietary fixtures are never checked in.
use std::{collections::BTreeMap, path::PathBuf};

#[test]
fn installed_fxr_coverage() {
    let root = std::env::var_os("SEKIRO_FXR_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| sekiro_formats::cache::root().join("raw/sfx"));
    if !root.is_dir() {
        eprintln!("FXR cache unavailable: {}", root.display());
        return;
    }
    let mut bundles = BTreeMap::new();
    let mut actions = BTreeMap::<u16, usize>::new();
    let mut properties = BTreeMap::<u32, usize>::new();
    let mut curve_payloads = BTreeMap::<u32, usize>::new();
    let mut sampled_properties = 0usize;
    let mut unsupported_sampling = BTreeMap::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let mut count = 0;
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_none_or(|e| e != "fxr") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let effect = sekiro_formats::fxr::parse(&bytes)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let name = format!("f{:09}.fxr", effect.id);
            assert_eq!(path.file_name().unwrap().to_string_lossy(), name);
            for action in &effect.actions {
                *actions.entry(action.id()).or_default() += 1;
            }
            for property in &effect.properties {
                *properties.entry(property.tag & 0xffff).or_default() += 1;
                // Explicit linear policy checks the sampling utility, not native semantics.
                match property.compile_unmodified(sekiro_formats::fxr::Interpolation::Linear) {
                    Ok(compiled) => {
                        for argument in [0.0, 0.25, 0.5, 1.0] {
                            let sample = compiled.sample(argument).unwrap();
                            assert!(sample.iter().all(|v| v.is_finite()));
                        }
                        sampled_properties += 1;
                    }
                    Err(error) => {
                        *unsupported_sampling
                            .entry(format!("{error:?}"))
                            .or_insert(0usize) += 1;
                    }
                }
                if matches!((property.tag >> 4) & 255, 3..=6) {
                    assert!(
                        property.curve().is_some(),
                        "{}: curve {:#x}",
                        path.display(),
                        property.tag
                    );
                    *curve_payloads.entry(property.tag & 0xffff).or_default() += 1;
                }
            }
            // Every proper truncation must fail even if its referenced tables ended earlier.
            assert!(sekiro_formats::fxr::parse(&bytes[..143]).is_err());
            count += 1;
        }
        bundles.insert(entry.file_name().to_string_lossy().into_owned(), count);
    }
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ffxbnd") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let files = sekiro_formats::bnd4::parse(&bytes).unwrap();
        let mut count = 0;
        for file in files.iter().filter(|f| f.file_name().ends_with(".fxr")) {
            let bytes = file.contents(None).unwrap();
            sekiro_formats::fxr::parse(&bytes)
                .unwrap_or_else(|e| panic!("{} / {}: {e}", path.display(), file.file_name()));
            count += 1;
        }
        let unpacked = format!("{}.d", path.file_name().unwrap().to_string_lossy());
        assert_eq!(
            bundles.get(&unpacked),
            Some(&count),
            "binder and unpacked FXR counts differ"
        );
    }
    eprintln!("FXR files by bundle: {bundles:?}");
    eprintln!("FXR action records by id (raw semantics): {actions:?}");
    eprintln!("FXR property evaluator records: {properties:?}");
    eprintln!("FXR structured curve payloads: {curve_payloads:?}");
    eprintln!("FXR sampleable properties with explicit linear policy: {sampled_properties}");
    eprintln!("FXR unsupported sampling: {unsupported_sampling:?}");
    assert!(bundles.values().sum::<usize>() > 0);
}
