//! Read-only map FX census, including the settled parameter-resolved event writes.
use sekiro_formats::{fxr, fxr_json, msb};
use serde_json::json;
use std::path::Path;

fn main() {
    let cache = std::env::args().nth(1).expect("cache directory");
    let cache = Path::new(&cache);
    let mut maps = Vec::new();
    for map in sekiro_sim::world_events::SUPPORTED_MAPS {
        let msb =
            msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap())
                .unwrap();
        let state = sekiro_sim::world_events::first_visit(cache, map)
            .unwrap()
            .unwrap();
        let regions: Vec<_> = msb.regions.iter().filter(|r| r.effect.is_some()).map(|r| {
            let source = state.effects.iter().rev().find(|w| w.entity == r.entity_id && matches!(w.command, 1 | 2));
            json!({"region":r,"enabled":source.map_or(!r.start_disabled, |w| w.command == 2),"source":source})
        }).collect();
        let catalog = sekiro_sim::map_effects::load(cache, map, &state).unwrap();
        maps.push(json!({"map":map,"sfx":regions,"writes":state.effects,"controls":state.effect_controls,"bonfires":state.bonfires,"visibility":state.visibility,"catalog":catalog}));
    }
    let mut paths = std::collections::BTreeMap::new();
    for dir in std::fs::read_dir(cache.join("raw/sfx")).unwrap().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "fxr") {
                continue;
            }
            if let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.trim_start_matches('f').parse::<i64>().ok())
            {
                paths.entry(id).or_insert(path);
            }
        }
    }
    let mut pending: std::collections::BTreeSet<i64> = maps
        .iter()
        .flat_map(|m| m["catalog"]["placements"].as_array().unwrap())
        .filter_map(|p| p["effect"].as_i64())
        .collect();
    let mut actions = std::collections::BTreeMap::new();
    fn references(v: &serde_json::Value, out: &mut std::collections::BTreeSet<i64>) {
        if let Some(id) = v["sfx"].as_i64() {
            out.insert(id);
        }
        for child in v["nodes"].as_array().into_iter().flatten() {
            references(child, out);
        }
    }
    while let Some(id) = pending.pop_first() {
        if actions.contains_key(&id) {
            continue;
        }
        let Some(path) = paths.get(&id) else {
            continue;
        };
        let f = fxr::parse(&std::fs::read(path).unwrap()).unwrap();
        let (v, report) = fxr_json::to_json(&f);
        references(&v["root"], &mut pending);
        actions.insert(id,json!({"tree":v,"unknown":report.unknown_actions,"mismatches":report.layout_mismatches}));
    }
    println!("{}", json!({"maps":maps,"fxr":actions}));
}
