//! The SpEffect list tick against the user's own sekiro.exe, run in tools/emu-cases `speffect` on
//! the Naomori-arena snapshot (docs/kb/speffect.md 2026-11-03): the add FUN_1409e9c00 and the
//! character tick FUN_140a02a10 (FUN_140a03340's list tick, then the HP module) at 30 and 60 fps.
//! Skips when extracted/emu/speffect_ticks.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::root().join(format!("extracted/emu/{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// The runs `status` covers: (name, enemy). Burn and terror on the General are left out (gap: his
/// burn build-up is not taken in the emulator, terror resist 999); the items, buffs and arts are
/// rows RSekiro does not run through `Status` (gap, kb).
const RUNS: &[(&str, bool)] = &[
    ("burn_9100", false),
    ("burn_again", false),
    ("burn_then_9105", false),
    ("poison_9045", false),
    ("poison_9045", true),
    ("poison_and_burn", false),
    ("terror_9200", false),
    ("floor_4011", false),
    ("floor_4011", true),
    ("stop_109450", false),
    ("stop_109450", true),
];

/// Frame by frame: the build-up gauges after each add (Wolf wears Hard mode's 8610: x1.5 burn,
/// x1.2 terror), HP after each tick and the added effects' list (id, time left, expired flag):
/// `Statuses::build_up` / `start` / `tick_effects` against the exe.
#[test]
pub(crate) fn speffect_ticks_match_the_exe() {
    let Some(v) = emu_json("speffect_ticks") else { return };
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let e = Entity::PLACEHOLDER;
    let mut bad: Vec<String> = Vec::new();
    let mut frames = 0;
    for run in v["runs"].as_array().unwrap() {
        let name = run["name"].as_str().unwrap();
        let enemy = run["enemy"].as_bool().unwrap();
        if !RUNS.contains(&(name, enemy)) {
            continue;
        }
        let fps = run["fps"].as_i64().unwrap();
        let dt = run["dt"].as_f64().unwrap() as f32;
        let start = &run["start"];
        let hp_max = start["hp_max"].as_f64().unwrap() as f32;
        let mut hp = start["hp"].as_f64().unwrap() as f32;
        let base: Vec<i64> = start["sp"].as_array().unwrap().iter().map(|s| s[0].as_i64().unwrap()).collect();
        let worn: Vec<i64> = if enemy { Vec::new() } else { base.clone() };
        // The resist module's max per exe type (+0x24: poison, disease/terror, blood/burn, curse,
        // freeze) in Sv1's order (poison, burn, terror, curse, freeze).
        let r = |i: usize| start["resist"][9 + i].as_f64().unwrap() as f32;
        let resist = [r(0), r(2), r(1), r(3), r(4)];
        let mut s = crate::status::Statuses::default();
        let mut stop = Vec::new();
        let adds = run["adds"].as_array().unwrap();
        // Only rows Sv1 has (Wolf's export and the loaded enemy kinds'); terror 9200 and the
        // floor's 4011 are in neither here (their kinds' 9222 / 4001 take the same path).
        let has = |id: i64| combat.player.sp_effects.contains_key(&id.to_string()) || combat.kinds.iter().any(|k| k.data.sp_effects.contains_key(&id.to_string()));
        if !adds.iter().all(|a| has(a[1].as_i64().unwrap())) {
            continue;
        }
        let tag = format!("{name} {} {fps}fps", if enemy { "enemy" } else { "wolf" });
        for (i, f) in run["frames"].as_array().unwrap().iter().enumerate() {
            for (k, a) in adds.iter().filter(|a| a[0].as_u64() == Some(i as u64)).enumerate() {
                let id = a[1].as_i64().unwrap();
                if crate::status::Statuses::is_build_up(&combat, id) {
                    s.build_up_for(&combat, e, &worn, id, resist);
                } else {
                    s.start_for(&combat, e, id);
                }
                // The exe's left per type after this add = resist - Sv1's gauge.
                let after = &f["added"][k]["after"]["resist"];
                let st = s.0.entry(e).or_default();
                for (ty, (exe_i, gauge)) in [(0, st.poison), (2, st.burn), (1, st.terror)].into_iter().enumerate() {
                    let exe = after[4 + exe_i].as_f64().unwrap() as f32;
                    let ours = [resist[0], resist[1], resist[2]][ty] - gauge;
                    if exe != ours {
                        bad.push(format!("{tag} f{i} add {id}: type {ty} left exe {exe} ours {ours}"));
                    }
                }
            }
            let st = s.0.entry(e).or_default();
            let (delta, _) = crate::status::tick_effects(&combat, st, hp_max, dt, &mut stop);
            hp = (hp + delta as f32).clamp(0.0, hp_max);
            frames += 1;
            let exe_hp = f["hp"].as_f64().unwrap() as f32;
            // Wolf's terror kill: the exe's snapshot keeps him at 1 (no-death flag), Sv1 at 0.
            if exe_hp != hp && !(exe_hp == 1.0 && hp == 0.0) {
                bad.push(format!("{tag} f{i}: hp exe {exe_hp} ours {hp}"));
            }
            let exe: Vec<(i64, f32, bool)> = f["sp"].as_array().unwrap().iter().filter(|x| !base.contains(&x[0].as_i64().unwrap())).map(|x| (x[0].as_i64().unwrap(), x[1].as_f64().unwrap() as f32, x[4].as_i64() == Some(1))).collect();
            let ours: Vec<(i64, f32, bool)> = st.effects.iter().map(|a| (a.id, a.left, a.expired)).collect();
            let same = exe.len() == ours.len() && exe.iter().zip(&ours).all(|(x, o)| x.0 == o.0 && (x.1 - o.1).abs() < 1e-4 && x.2 == o.2);
            if !same {
                bad.push(format!("{tag} f{i}: list exe {exe:?} ours {ours:?}"));
            }
        }
    }
    assert!(frames > 0, "no runs");
    // The first three per run.
    let mut shown: Vec<String> = Vec::new();
    for b in &bad {
        let run = b.split(" f").next().unwrap_or_default();
        if shown.iter().filter(|s| s.starts_with(run)).count() < 3 {
            shown.push(b.clone());
        }
    }
    assert!(bad.is_empty(), "{} differ in {frames} frames:\n{}", bad.len(), shown.join("\n"));
}

/// The gourd 3000 on Wolf (Knowledge of Medicine LV5, x1.5): the exe heals (int)(1120 x 40 % x
/// 1.5) = 672, capped at max; `player::estus_heal`.
#[test]
pub(crate) fn gourd_heal_matches_the_exe() {
    let Some(v) = emu_json("speffect_ticks") else { return };
    for run in v["runs"].as_array().unwrap().iter().filter(|r| r["name"] == "gourd_3000") {
        let max = run["start"]["hp_max"].as_f64().unwrap() as f32;
        let hp = run["start"]["hp"].as_f64().unwrap() as f32;
        let exe = run["frames"][0]["hp"].as_f64().unwrap() as f32;
        // Wolf's worn 150204 carries the LV5 correction (changeHpEstusFlaskCorrectRate 1.5).
        let ours = (hp + crate::player::estus_heal(max, 40.0, 1.5)).min(max);
        assert_eq!(ours, exe, "{} fps", run["fps"]);
        let lowered = crate::player::estus_heal(1001.0, 40.0, 1.5);
        assert_eq!(lowered, 600.0, "whole points");
    }
}
