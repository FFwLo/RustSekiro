//! Guard and deflect resolution against the exe (topic guard).

/// The game's guard resolution (FUN_140b6c880 resolve, FUN_140b6e6a0 apply, FUN_140b698d0
/// attacker side, FUN_140842fb0 flick defence; run in tools/emu-cases guard.rs on the memory
/// snapshot, Wolf against the General c1020 and back) against combat.rs: guard_verdict on
/// attack_flick / guard_flick, the guard rows behavior_ref / atk_guard pick and the arc of
/// guard_half_angle. 90 cases: five attacks (a slash, the thrust, the sweep, guardAtkRate 100
/// and 20) guarded or not, in the deflect window or not, the arc in steps, the spam chain,
/// posture running out, the flick SpEffects 1130 / 900001, Wolf's attacks on the General (a
/// deathblow start's flick 90) and the umbrella's guard row 107600100 (guardAttribute 1).
/// Skipped when extracted/emu/guard_resolution.json is missing.
#[test]
pub(crate) fn guard_resolution_matches_the_exe() {
    use crate::combat::{attack_flick, guard_flick, guard_verdict, Verdict};
    let path = crate::paths::emu().join("guard_resolution.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 90);
    // The arcs: Wolf's katana guardAngle 0 (the front half), the General's NpcParam guardAngle.
    let wolf = crate::actor::Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
    let wolf_arc = crate::combat::guard_half_angle(&wolf);
    assert_eq!(wolf_arc, 90.0);
    let mut general = crate::actor::Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 0.0, 0);
    general.guard_angle = 60.0; // NpcParam guardAngle of every c1020 row
    let general_arc = crate::combat::guard_half_angle(&general);
    let i = |c: &serde_json::Value, k: &str| c[k].as_i64().unwrap_or_else(|| panic!("{k}"));
    let mut bad = Vec::new();
    for c in cases {
        let wolf_defends = c["wolf_defends"].as_bool().unwrap();
        let (pc_atk, pc_def) = (!wolf_defends, wolf_defends);
        let atk = i(c, "atk");
        let ag = crate::data::atk_guard(pc_atk, atk).unwrap_or_else(|| panic!("AtkParam {atk}"));
        // The guard row: Wolf's judge 90 with the katana's variation 5000, the General's 900.
        let row_id = i(c, "guard_row");
        if row_id != 107600100 {
            assert_eq!(row_id, if wolf_defends { 100_000_000 + 5000 * 1000 + 90 } else { 200_000_000 + 10200 * 1000 + 900 });
        }
        let row = crate::data::behavior_ref(pc_def, row_id).and_then(|r| crate::data::atk_guard(pc_def, r));
        assert!(row.is_some(), "guard row {row_id}");
        let mut worn: Vec<i64> = c["extra"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        if c["window"].as_bool().unwrap() {
            worn.push(if wolf_defends { 105010 } else { 200220 });
        }
        if i(c, "spam") > 0 {
            worn.push(105019 + i(c, "spam"));
        }
        let fd = guard_flick(row.as_ref(), pc_def, 5000, worn);
        let fa = attack_flick(&ag, pc_atk, 5000);
        let arc = if wolf_defends { wolf_arc } else { general_arc };
        let guarded = c["guard"].as_bool().unwrap() && (c["angle"].as_f64().unwrap() as f32) < arc;
        let ours = guard_verdict(guarded, fd, fa, row.map_or(0, |g| g.guard_attribute), &ag);
        let exe = match (i(c, "guarded"), i(c, "deflected")) {
            (_, 1) => Verdict::Deflect,
            (1, _) => Verdict::Block,
            _ => Verdict::Hit,
        };
        let at = format!("atk {atk} guard {} window {} angle {} extra {} row {row_id}", c["guard"], c["window"], c["angle"], c["extra"]);
        if (fd, fa) != (i(c, "flick_def") as i32, i(c, "flick_atk") as i32) {
            bad.push(format!("{at}: flick ours {fd}/{fa}, exe {}/{}", c["flick_def"], c["flick_atk"]));
        }
        if ours != exe {
            bad.push(format!("{at}: ours {ours:?}, exe {exe:?}"));
        }
        // A block that empties the guard breaks it and bounces nobody (reaction 99999); a
        // deflect never sets the break, even when it empties posture.
        let gb = i(c, "guard_break") == 1;
        assert_eq!(gb, exe == Verdict::Block && i(c, "def_posture") <= i(c, "def_posture_dmg"), "{at}: guard break");
        let want = match exe {
            Verdict::Deflect => 1000,
            Verdict::Block if !gb => 1017,
            _ => 99999,
        };
        assert_eq!(i(c, "att_reaction"), want, "{at}: attacker reaction");
    }
    assert!(bad.is_empty(), "guard resolution differs from the exe:\n{}", bad.join("\n"));
}
