//! The HKS env surface (src/hks_env.rs) against the exe: tools/emu-cases `hks_env` runs the env
//! dispatcher FUN_140b36410 on Wolf from the 2026-11-03 snapshot (standing idle in General
//! Naomori's arena, AI off) for every env id and argument list in Wolf's scripts.

use super::*;
use crate::hks_env::HksEnv;

/// Answers that differ on purpose, (id, first argument): why.
const KNOWN: &[(i32, i64, &str)] = &[];

/// Every env answer Sv1 gives for idle Wolf equals the exe's; the field pokes the generator made
/// (one handler field written, the id run again) all moved the answer as the doc comments say.
#[test]
pub(crate) fn hks_env_matches_the_exe() {
    let path = crate::paths::root().join("extracted/emu/hks_env.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();

    for p in v["pokes"].as_array().unwrap() {
        assert_eq!(p["after"], p["expect"], "exe poke {}: {p}", p["what"]);
    }

    let mut app = app();
    for _ in 0..60 {
        app.update();
    }
    // The snapshot Wolf's equipment (its SpEffect list: 140701 is art weapon 7400's
    // residentSpEffectId, 127300 Loaded Axe 73000's).
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.player.combat_art = 7400;
        config.player.prosthetics = vec![73000];
    }
    let mut env = HksEnv::from_world(app.world_mut());
    // The save's HP (548 of 1120 in the snapshot): GetHP is the current HP (data module +0x130).
    let hp = v["env"].as_array().unwrap().iter().find(|e| e["id"] == 1000).and_then(|e| e["cases"][0]["ret"].as_f64()).unwrap();
    env.fields.insert("data+0x130".into(), hp);

    let (mut total, mut answered, mut compared) = (0u64, 0u64, 0usize);
    let mut diffs = Vec::new();
    let mut known_seen = Vec::new();
    for e in v["env"].as_array().unwrap() {
        total += e["count"].as_u64().unwrap();
        let Some(id) = e["id"].as_i64() else { continue };
        let mut any = false;
        for c in e["cases"].as_array().unwrap() {
            let Some(exe) = c["ret"].as_i64() else { continue };
            let args: Vec<i64> = c["args"].as_array().unwrap().iter().map(|a| a.as_f64().unwrap() as i64).collect();
            let Some(ours) = env.env(id as i32, &args) else { continue };
            any = true;
            compared += 1;
            let first = args.first().copied().unwrap_or(0);
            if let Some(k) = KNOWN.iter().find(|k| k.0 as i64 == id && k.1 == first) {
                assert_ne!(ours as i64, exe, "env {id}({first}) now matches; drop it from KNOWN ({})", k.2);
                known_seen.push((id, first));
            } else if ours as i64 != exe {
                diffs.push(format!("env {id}{args:?}: Sv1 {ours}, exe {exe}"));
            }
        }
        if any {
            answered += e["count"].as_u64().unwrap();
        }
    }
    println!(
        "hks env: Sv1 answers {answered} of {total} calls ({:.1} %), {compared} cases compared, {} known differences",
        100.0 * answered as f64 / total as f64,
        known_seen.len()
    );
    assert_eq!(known_seen.len(), KNOWN.len(), "a KNOWN entry no longer runs");
    assert!(answered >= 1045, "Sv1 answered fewer env calls than on 2026-11-03 (1,045 of 1,605)");
    assert!(diffs.is_empty(), "Sv1's env answers differ from the exe (idle Wolf):\n{}", diffs.join("\n"));
}
