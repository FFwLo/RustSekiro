//! Where a landed hit's HP gets its rates, against the user's own sekiro.exe (tools/emu-cases
//! `hprate`: the `hits` duels w_open / g_open with the resolve FUN_140b6c880 and its element sum
//! FUN_140840eb0 hooked; docs/kb/damage.md "HP rates and flag factors"). Skips when
//! extracted/emu/hprate_*.json is missing.

use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("hprate_{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn floats(v: &Value) -> Vec<f32> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
}

fn ids(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

const CASES: &[&str] = &[
    "w_ap99", "w_ap1", "w_ap10", "w_ap_none", "w_ap16", "w_ako", "w_ap16_ako", "w_flag41", "w_flag41_arrow", "w_p14", "w_p15",
    "w_p14_p16", "w_sp9600", "g_ap99", "g_ap1", "g_flag41", "g_flag41_arrow",
];

/// Wolf's slash (AtkParam_Pc 5000010, 80 physical) on the General and the General's slash
/// (10200100) on Wolf: Sv1's Attack Power rate (`combat::record_rate` of the growth row's
/// physicsAttackRate, times the quick items' rate as player.rs puts them in `atk_rate`) is the
/// exe's first rate array p5 element for element; the flag factors (`combat::counter_hit`,
/// `combat::sum_flag_factor`) are the exe's p13 and its sum's factor; and the whole HP (the
/// exe's other rate arrays, the General's cuts, taken as they are) is the exe's.
#[test]
pub(crate) fn hp_rates_match_the_exe() {
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0;
    for name in CASES {
        let Some(v) = load(name) else { continue };
        let wolf_attacks = v["duel"].as_str() == Some("w_open");
        let (drop, add, att_add) = (ids(&v["wolf_drop"]), ids(&v["wolf_add"]), ids(&v["att_add"]));
        // Wolf's worn rows: the save's AP 99 unless dropped, the added ones.
        let mut worn: Vec<i64> = if drop.contains(&160099) { Vec::new() } else { vec![160099] };
        worn.extend(add.iter().copied());
        if wolf_attacks {
            worn.extend(att_add.iter().copied());
        }
        let growth = worn.iter().copied().find(|id| (160000..160100).contains(id));
        let items: Vec<(i64, f32)> = worn.iter().copied().filter(|id| !(160000..160100).contains(id)).map(|id| (id, 0.0)).collect();
        for (k, (rec, sum)) in v["records"].as_array().unwrap().iter().zip(v["sums"].as_array().unwrap()).enumerate() {
            let exe_out = v["sum_out"][k].as_f64().unwrap() as f32;
            let rates: Vec<Vec<f32>> = sum["rates"].as_array().unwrap().iter().map(floats).collect();
            let amounts = floats(&sum["atk"]);
            let attacker_wolf = rec["attacker"].as_str() == Some("wolf");
            assert_eq!(attacker_wolf, wolf_attacks, "{name}: attacker {}", rec["attacker"]);
            // p5: Wolf's Attack Power (and his items) as whole percent; an NPC's is 1 (its doping is
            // in the amounts, kb/atkrec.md).
            let ours_ap = if attacker_wolf {
                let r = growth.map_or(1.0, |id| crate::items::param("SpEffectParam", id)["physicsAttackRate"].as_f64().unwrap() as f32);
                crate::combat::record_rate(r * crate::items::wolf_rates(&items)[0])
            } else {
                1.0
            };
            for (e, &x) in rates[0].iter().enumerate() {
                if x != ours_ap {
                    bad.push(format!("{name}: p5[{e}] exe {x}, Sv1 {ours_ap}"));
                }
            }
            let flag41 = v["flag41"].as_bool().unwrap();
            let arrow = !v["arrow"].is_null();
            let p13 = sum["p13"].as_i64() == Some(1);
            if crate::combat::counter_hit(flag41, arrow) != p13 {
                bad.push(format!("{name}: p13 exe {p13}, Sv1 {}", crate::combat::counter_hit(flag41, arrow)));
            }
            let (p14, p15) = (sum["p14"].as_i64() == Some(1), sum["p15"].as_i64() == Some(1));
            let (p16, p17) = (sum["p16"].as_f64().unwrap() as f32, sum["p17"].as_f64().unwrap() as f32);
            // Sv1 only ever passes p14 / p15 off, p16 -1 and p17 1 (the builders clear them; 9600
            // leaves p17 at 1): the poked cases check the factor itself.
            if v["rec"].is_null() && (p14 || p15 || p16 != -1.0 || p17 != 1.0) {
                bad.push(format!("{name}: unpoked p14 {p14} p15 {p15} p16 {p16} p17 {p17}"));
            }
            let s: f32 = (0..5).map(|e| (amounts[e] * ours_ap * rates[1..].iter().map(|r| r[e]).product::<f32>()).max(0.0)).sum();
            let ours = crate::combat::sum_flag_factor(s, crate::combat::counter_hit(flag41, arrow), p14, p15, p16, p17);
            if crate::combat::hit_hp(ours, 1.0) != crate::combat::hit_hp(exe_out, 1.0) {
                bad.push(format!("{name}: HP exe {exe_out}, Sv1 {ours}"));
            }
            n += 1;
        }
    }
    if n == 0 {
        return;
    }
    assert!(bad.is_empty(), "HP rates differ from the exe in {} checks:\n{}", bad.len(), bad.join("\n"));
}
