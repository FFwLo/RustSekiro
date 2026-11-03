//! TAE sound events against the decoded sound banks. Skips without `cache/sound`.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use sekiro_sim::tae::{TaeDb, TaeKind, sound_name};

fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

fn known_events() -> HashSet<String> {
    let mut out = HashSet::new();
    for bank in ["main", "c1010", "m11"] {
        let p = cache().join("sound").join(bank).join("events.json");
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let map: BTreeMap<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
        out.extend(map.into_keys());
    }
    out
}

#[test]
fn soldier_and_wolf_sound_events_resolve() {
    let known = known_events();
    if known.is_empty() {
        eprintln!("skipping: run `sekiro-extract sound` first");
        return;
    }
    for chr in ["c1010", "c0000"] {
        let db = TaeDb::load_from_cache(&cache(), chr);
        let mut total = 0;
        let mut found = 0;
        let mut kinds = BTreeMap::new();
        let mut missing = BTreeMap::new();
        for id in db.anim_ids() {
            let name = sekiro_formats::tae::anim_name(id);
            let Some(anim) = db.anim(&name) else { continue };
            for e in anim.events.iter() {
                if let TaeKind::Sound { kind, id } = e.kind {
                    total += 1;
                    *kinds.entry(kind).or_insert(0) += 1;
                    let n = sound_name(kind, id);
                    if known.contains(&n) {
                        found += 1;
                    } else {
                        *missing.entry(n).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut top: Vec<_> = missing.into_iter().collect();
        top.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        eprintln!(
            "{chr}: {total} sound events, {found} resolve; by type {kinds:?}; most missing {:?}",
            &top[..top.len().min(12)]
        );
        assert!(total > 0, "{chr}: no sound events decoded");
    }
    let db = TaeDb::load_from_cache(&cache(), "c1010");
    let slash = db.anim("a000_003000").expect("soldier attack 3000");
    let names: Vec<String> = slash
        .events
        .iter()
        .filter_map(|e| match e.kind {
            TaeKind::Sound { kind, id } => {
                Some(format!("{:.2}s {}", e.start, sound_name(kind, id)))
            }
            _ => None,
        })
        .collect();
    eprintln!("a000_003000 sounds: {names:?}");
    assert!(!names.is_empty());
}

#[test]
fn combat_sounds_come_from_the_hit_effect_params() {
    let Some(sounds) = sekiro_sim::sound::HitSounds::load_from_cache(&cache(), 10_100_000) else {
        return;
    };
    assert_eq!(sounds.materials, [[113, 139], [114, 108]]);
    // Wolf deflects the soldier's slash (AtkParam_Npc 10100100, Iron slash S, defSeMaterial
    // 100 / 139): HitEffectSeJustGuardParam row 100.
    assert_eq!(sounds.lookup(1, 10100100, 0, true, false), ["z199999980"]);
    // Blocked: HitEffectSeParam at the same rows.
    assert_eq!(sounds.lookup(1, 10100100, 0, false, true), ["z200000101"]);
    // A landed slash on Wolf: HitEffectSeParam at his armour materials 113 (139 is empty).
    assert_eq!(sounds.lookup(1, 10100100, 0, false, false), ["z000000013"]);
}
