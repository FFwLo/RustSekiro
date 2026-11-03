//! The attack record a TAE behaviour event builds, against the exe (topic atkrec).
//!
//! tools/emu-cases `atkrec`: the exe's own FUN_14083d220 / FUN_14084fc50 (the attack info),
//! PlayerIns vfunc 0x1f8 (the behaviour's posture cost), FUN_14099fc00 -> FUN_1410b6880 (one hit
//! entry per AtkParam shape) on the snapshot's Wolf (right hand: the art's virtual weapon 7400,
//! left: the Loaded Axe 73000) and the General; vfunc 0x1c0 / 0x1c8 for every judge; and whole
//! chr frames with the TAE events hooked. Skipped when extracted/emu/atkrec_*.json is missing.

use crate::data::Attack;
use serde_json::Value;
use std::collections::HashMap;

fn load(name: &str) -> Option<Value> {
    let path = crate::paths::root().join("extracted/emu").join(name);
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn param(name: &str) -> HashMap<i64, Value> {
    let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
    let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return HashMap::new() };
    let rows: Vec<Value> = match &v["rows"] {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.values().cloned().collect(),
        _ => Vec::new(),
    };
    rows.into_iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(1.0)
}

/// The hit shapes as RSekiro takes them from an AtkParam row (the export's `hits`: hit0..hit15
/// with DmyPoly1 > 0; Attack::shapes): (dummy 1, dummy 2, radius).
fn our_shapes(row: &Value) -> Vec<(i64, i64, f32)> {
    (0..16)
        .filter(|i| row[format!("hit{i}_DmyPoly1")].as_i64().unwrap_or(-1) > 0)
        .map(|i| (row[format!("hit{i}_DmyPoly1")].as_i64().unwrap(), row[format!("hit{i}_DmyPoly2")].as_i64().unwrap(), f(&row[format!("hit{i}_Radius")])))
        .collect()
}

/// Every attack row of BehaviorParam_PC on Wolf (both hands) and the General's 210200xxx /
/// 210201xxx, built by the exe: the AtkParam the row refers to (`data::behavior_ref`), the hit
/// shapes (the exe registers shape 0 even when empty, then only shapes with a radius and a
/// dummy; RSekiro's DmyPoly1 > 0), the five amounts (Wolf: `combat::player_amounts` with the
/// hand's weapon; the General: atk* x his info rates +0x70 / +0xa4-0xb0, RSekiro's atk_rate),
/// the posture amounts (entry +0x34 atkStam, +0x38 directAtkStamDamage or atkStam, +0x3c
/// repelLostStamDamage, each x the attacker's staminaAttackRate: Wolf's Attack Power 99 7.23,
/// the General's doping 5.36), the flick (`combat::attack_flick`), the five SpEffects, and the
/// behaviour's posture cost (BehaviorParam stamina; only the unused "egg attack" row 10 has one).
#[test]
pub(crate) fn attack_rows_match_the_exe() {
    let Some(v) = load("atkrec_rows.json") else { return };
    let (pc_rows, npc_rows, pc_beh) = (param("AtkParam_Pc"), param("AtkParam_Npc"), param("BehaviorParam_PC"));
    if pc_rows.is_empty() {
        return;
    }
    let mut bad = Vec::new();
    let mut n = 0;
    let mut posture_rate: HashMap<&str, f32> = HashMap::new();
    for c in v["cases"].as_array().unwrap() {
        let who = c["chr"].as_str().unwrap();
        let pc = who == "wolf";
        let beh = c["row"].as_i64().unwrap();
        assert!(c["stopped"].is_null(), "{who} {beh} stopped {}", c["stopped"]);
        // Cost: PlayerIns::vf63 FUN_140a240e0 = BehaviorParam_PC stamina (x rates that are 1 here).
        if pc {
            let want = pc_beh.get(&beh).and_then(|r| r["stamina"].as_f64()).unwrap_or(0.0) as f32;
            if !close(f(&c["cost"]), want) {
                bad.push(format!("{who} {beh}: cost {} want {want}", c["cost"]));
            }
        }
        let Some(info) = c["info"].as_object() else { continue };
        let atk_id = info["atk"].as_i64().unwrap();
        if crate::data::behavior_ref(pc, beh) != Some(atk_id) {
            bad.push(format!("{who} {beh}: exe AtkParam {atk_id}, ours {:?}", crate::data::behavior_ref(pc, beh)));
        }
        let Some(row) = (if pc { pc_rows.get(&atk_id) } else { npc_rows.get(&atk_id) }) else { continue };
        let atk: Attack = serde_json::from_value(row.clone()).unwrap_or_else(|e| panic!("AtkParam {atk_id}: {e}"));
        let entries = c["entries"].as_array().unwrap();
        let exe: Vec<(i64, i64, f32)> = entries.iter().filter(|e| f(&e["radius"]) > 0.0).map(|e| (e["dmy1"].as_i64().unwrap(), e["dmy2"].as_i64().unwrap(), f(&e["radius"]))).collect();
        if exe != our_shapes(row) {
            bad.push(format!("{who} {atk_id}: shapes exe {exe:?} ours {:?}", our_shapes(row)));
        }
        let Some(e) = entries.first() else { continue };
        n += 1;
        let amounts: Vec<f32> = e["amounts"].as_array().unwrap().iter().map(f).collect();
        let weapon = info["weapon"].as_i64().unwrap();
        let ours = if pc {
            crate::combat::player_amounts(&atk, &crate::data::weapon_row(weapon).unwrap_or_default())
        } else {
            let r = info["rates"].as_array().unwrap();
            [atk.atk_phys * f(&r[0]), atk.atk_mag * f(&r[13]), atk.atk_fire * f(&r[14]), atk.atk_thun * f(&r[15]), atk.atk_dark * f(&r[16])]
        };
        if amounts.iter().zip(ours).any(|(a, b)| !close(*a, b)) {
            bad.push(format!("{who} {atk_id} weapon {weapon}: amounts exe {amounts:?} ours {ours:?}"));
        }
        // The attacker's posture rate, the same on every row of one chr.
        if atk.atk_stam > 0.0 {
            let r = f(&e["f34"]) / atk.atk_stam;
            let first = *posture_rate.entry(if pc { "wolf" } else { "general" }).or_insert(r);
            if !close(r, first) {
                bad.push(format!("{who} {atk_id}: posture rate {r} vs {first}"));
            }
        }
        if let Some(&rate) = posture_rate.get(who) {
            let direct = rate * if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
            if !close(f(&e["f34"]), rate * atk.atk_stam) || !close(f(&e["f38"]), direct) || !close(f(&e["f3c"]), rate * atk.repel_lost_stam_damage) {
                bad.push(format!("{who} {atk_id}: posture exe {} {} {} at rate {rate}", e["f34"], e["f38"], e["f3c"]));
            }
        }
        let flick = crate::combat::attack_flick(&crate::combat::atk_guard_of(&atk, pc), pc, weapon);
        if e["flick"].as_i64() != Some(flick as i64) {
            bad.push(format!("{who} {atk_id}: flick exe {} ours {flick}", e["flick"]));
        }
        let sp: Vec<i64> = e["sp"].as_array().unwrap()[..5].iter().map(|s| s.as_i64().unwrap()).collect();
        if sp != atk.sp_effects() && row["isDisableSpEffect"].as_i64().unwrap_or(0) == 0 {
            bad.push(format!("{who} {atk_id}: SpEffects exe {sp:?} ours {:?}", atk.sp_effects()));
        }
    }
    assert!(n > 600, "{n} rows");
    assert!((posture_rate["wolf"] - 7.23).abs() < 1e-4, "Wolf's staminaAttackRate {}", posture_rate["wolf"]);
    assert!(bad.is_empty(), "{} of {n} differ:\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}

/// PlayerIns vfunc 0x1c0 FUN_140a24560 for judges 0-999 with each hand (right: the art's virtual
/// weapon 7400, variation 5008; left: the Loaded Axe 73000, 7300) against data::pc_behavior_row,
/// vfunc 0x1c8 (the PCBehavior judge is the BehaviorParam_PC row itself, RSekiro's "pc<judge>"),
/// and the General's ChrIns 0x1c0 FUN_140a12b80 (200000000 + variation x 1000 + judge, no
/// fallback, as combat::guard_row and the bullets).
#[test]
pub(crate) fn behaviour_rows_resolve_like_the_exe() {
    let Some(v) = load("atkrec_judges.json") else { return };
    if param("BehaviorParam_PC").is_empty() {
        return;
    }
    let var = |weapon: i64| crate::data::weapon_row(weapon).and_then(|w| w["behaviorVariationId"].as_i64()).unwrap();
    let mut bad = Vec::new();
    for c in v["cases"].as_array().unwrap() {
        let (j, row) = (c["judge"].as_i64().unwrap(), c["row"].as_i64().unwrap());
        let ours = match (c["chr"].as_str().unwrap(), c["via"].as_str().unwrap()) {
            ("wolf", "0x1c0") => crate::data::pc_behavior_row(var(if c["hand"].as_i64() == Some(-2) { 73000 } else { 7400 }), j),
            (_, "0x1c8") => j,
            _ => 200_000_000 + 10200 * 1000 + j,
        };
        if ours != row {
            bad.push(format!("{c}: ours {ours}"));
        }
    }
    assert!(v["cases"].as_array().unwrap().len() >= 6000);
    assert!(bad.is_empty(), "{} differ:\n{}", bad.len(), bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}

/// Whole chr frames (Wolf's attack, combo, art, jump, step, sprint; the General's forced 3000):
/// every AttackBehavior (1) / PCBehavior (307) event the exe ran that issued an attack, and the
/// AtkParam its info got, against the row RSekiro's attack_windows takes for that event: 307 ->
/// "pc<judge>"; 1 on Wolf (Source Default / Force Right Hand = the art's virtual weapon 7400,
/// variation 5008) -> "v5008:<judge>" else "<judge>"; 1 on the General -> his "<judge>".
/// gap: the frame's `anim` is not the TAE key, so this checks the event -> row resolution, not
/// which anim's window the event sits in; the prosthetic run fired no event in the snapshot.
#[test]
pub(crate) fn tae_attack_events_match_the_exe() {
    let Some(v) = load("atkrec_frames.json") else { return };
    let app = super::part01::app_with("c1020");
    let combat = app.world().resource::<crate::data::Combat>();
    let mut bad = Vec::new();
    let mut n = 0;
    for run in v["runs"].as_array().unwrap() {
        let wolf = run["chr"] == "wolf";
        let d = if wolf { &combat.player } else { combat.enemy0() };
        for fr in run["frames"].as_array().unwrap() {
            let log = fr["log"].as_array().unwrap();
            for (i, ev) in log.iter().enumerate().filter(|(_, l)| l.get("ev").is_some()) {
                // The event's own attack info follows it before the next event (none when the
                // event's gate, e.g. the enchantment rows 180-186, keeps it from firing).
                let Some(info) = log[i + 1..].iter().take_while(|l| l.get("ev").is_none()).find_map(|l| l.get("info")) else { continue };
                let atk_id = info["atk"].as_i64().unwrap();
                let (kind, judge) = (ev["ev"].as_i64().unwrap(), ev["judge"].as_i64().unwrap());
                let ours = if kind == 307 {
                    d.attacks.get(&format!("pc{judge}"))
                } else if wolf {
                    assert_ne!(ev["source"].as_i64(), Some(2), "a left-hand event in {}", run["name"]);
                    d.attacks.get(&format!("v5008:{judge}")).or_else(|| d.attacks.get(&judge.to_string()))
                } else {
                    d.attacks.get(&judge.to_string())
                }
                .map(|a| a.atk_param_id);
                n += 1;
                if ours != Some(atk_id) {
                    bad.push(format!("{} f{} ev {kind} judge {judge}: exe AtkParam {atk_id}, ours {ours:?}", run["name"], fr["frame"]));
                }
            }
        }
    }
    assert!(n >= 15, "{n} events");
    assert!(bad.is_empty(), "{} of {n} differ:
{}", bad.len(), bad.iter().take(30).cloned().collect::<Vec<_>>().join("
"));
}
