//! Throw and deathblow checks against the exe (topic throw).

use super::*;

/// The game's ThrowParam range test FUN_140be0270 and a row's whole test FUN_140be3a90, run in
/// tools/emu-cases throw.rs on the memory snapshot: Wolf as thrower, the General (c1020) put
/// around him at random (bearing, distance, height, both yaws; 600 placements per row) for his
/// rows 11020000 崩し始動, 11020110 崩し背後始動, 11020020 背後始動, 11020001 崩し本体 and
/// 11020150 崩し落下0. player.rs throw_range_ok must agree on every case. The exe's forward for a
/// yaw y is (-sin y, 0, -cos y), Actor::forward's. Skipped when extracted/emu/throw_ranges.json
/// is missing.
#[test]
pub(crate) fn throw_range_matches_the_exe() {
    let path = crate::paths::emu().join("throw_ranges.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3000);
    let app = app();
    let combat = app.world().resource::<Combat>();
    let vec = |c: &serde_json::Value| Vec3::new(c[0].as_f64().unwrap() as f32, c[1].as_f64().unwrap() as f32, c[2].as_f64().unwrap() as f32);
    let fwd = |y: f32| Vec3::new(-y.sin(), 0.0, -y.cos());
    let (mut bad, mut passed, mut row_only) = (Vec::new(), 0, Vec::new());
    for c in cases {
        let row = c["row"].as_i64().unwrap();
        let (wolf, enemy) = (vec(&c["wolf"]), vec(&c["enemy"]));
        let (wy, ey) = (c["wolf_yaw"].as_f64().unwrap() as f32, c["enemy_yaw"].as_f64().unwrap() as f32);
        let exe = c["range"].as_i64().unwrap() == 1;
        let ours = crate::player::throw_range_ok(combat, row, wolf, fwd(wy), enemy, fwd(ey));
        passed += exe as usize;
        if ours != exe {
            bad.push(format!("row {row} wolf {wolf} yaw {wy} enemy {enemy} yaw {ey}: exe {exe}, ours {ours}"));
        }
        // The whole row test adds the fall orbit (the plunge row fails on a standing Wolf) and the
        // sight cast between the two (FUN_140be3da0: three placements 0.7-0.8 m below Wolf, the
        // General sunk in the arena floor).
        if exe && c["row_ok"].as_i64().unwrap() == 0 && row != 11020150 {
            row_only.push((row, enemy.y - wolf.y));
        }
    }
    assert!(bad.is_empty(), "{} of {} differ:\n{}", bad.len(), cases.len(), bad.join("\n"));
    assert!(passed > 1300, "too few passing placements: {passed}");
    assert_eq!(row_only.len(), 3, "{row_only:?}");
    assert!(row_only.iter().all(|(_, dy)| *dy < -0.6), "{row_only:?}");
}

/// The exe's break is the hit record's flags (FUN_140b6e6a0: +0x1c6 on a hit, +0x1c4 on a block
/// that leave posture < 1 with posture damage), not the value: a deflect may empty the bar
/// (guard resolution run: posture 1 -> 0, no flag) and the next posture damage breaks.
#[test]
pub(crate) fn a_deflect_empties_posture_without_a_break() {
    let mut a = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 0.0, 0);
    a.posture = 90.0;
    a.add_deflect_posture(30.0);
    assert_eq!(a.posture, 100.0);
    assert!(!a.posture_broken());
    a.add_posture(1.0);
    assert!(a.posture_broken());
    // A broken bar stays broken through a deflect's posture.
    a.add_deflect_posture(5.0);
    assert!(a.posture_broken());
}

/// The throwable-state table FUN_140be0a40 (defender state x ThrowKindParam category, states
/// 0-127, categories 0-21 / 100 / 101) and the kind test FUN_140be3810 on Wolf -> the General
/// with the General's TAE 932 state poked (0 = his own 101) and his HP at full or 0, against
/// player.rs throwable_state_allows. The derived kinds (throwKindCheckMethod 1: 40100 / 41100,
/// the main throws) pass only out of their start throw, never here.
#[test]
pub(crate) fn throwable_states_match_the_exe() {
    let path = crate::paths::emu().join("throw_kinds.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let pairs: Vec<(u8, i64)> = v["table"].as_array().unwrap().iter().map(|p| (p[0].as_u64().unwrap() as u8, p[1].as_i64().unwrap())).collect();
    assert_eq!(pairs.len(), 52);
    for s in 0..=127u8 {
        for c in (0..=21).chain([100, 101]) {
            assert_eq!(crate::player::throwable_state_allows(s, c), pairs.contains(&(s, c)), "state {s} category {c}");
        }
    }
    let app = app();
    let combat = app.world().resource::<Combat>();
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 112);
    for c in cases {
        let kind = c["kind"].as_i64().unwrap();
        let row = &combat.params["ThrowKindParam"][kind.to_string()];
        let cat = row["throwKindCategory"].as_i64().unwrap();
        let state = match c["state"].as_u64().unwrap() as u8 {
            0 => 101,
            s => s,
        };
        let ours = row["throwKindCheckMethod"].as_i64() != Some(1) && crate::player::throwable_state_allows(state, cat);
        assert_eq!(ours, c["ok"].as_i64().unwrap() == 1, "kind {kind} (category {cat}) state {state} hp {}", c["hp"]);
    }
}
