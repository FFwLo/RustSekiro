//! HP and posture amounts against the user's own sekiro.exe, run in tools/emu-cases on the
//! Naomori-arena snapshot (`damage_*` pilots, docs/kb/damage.md 2026-11-03). Each test skips when
//! its extracted/emu/damage_*.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// Posture regen frame by frame (FUN_140a04850 on Wolf and five enemies: TAE 960 types -1..10,
/// TAE 225 percents 0-150, the stop flag, 1/60, 1/30 and 0.0123 s frames, carries, posture from
/// empty to full): `regen_frame` and `Actor::bound_posture` with `stamina_limits` give the exe's
/// posture and carry after every frame, from the exe's base, SpEffect product and row. Also checks
/// Sv1's inputs where it has them: graph 504 at Wolf's level, NpcParam staminaRecoverBaseVel, the
/// exported StaminaControlParam rows and the regen SpEffect rows.
#[test]
pub(crate) fn posture_regen_matches_the_exe() {
    let Some(v) = emu_json("damage_regen") else { return };
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let mut bad: Vec<String> = Vec::new();
    let mut frames = 0;
    for c in v["chars"].as_array().unwrap() {
        let npc = c["npc_param"].as_i64().unwrap();
        let max = c["posture_max"].as_i64().unwrap() as i32;
        let exe_row = c["row"].as_array().unwrap();
        // The exe's row in the export's field names (+4 = staminaMaxRatio, +6 = staminaMinRatio).
        let mut row = serde_json::Map::new();
        for (t, r) in exe_row.iter().enumerate() {
            row.insert(format!("staminaRecoverRatio_forType{t:03}"), r["recover"].clone());
            row.insert(format!("staminaMaxRatio_forType{t:03}"), r["limit_a"].clone());
            row.insert(format!("staminaMinRatio_forType{t:03}"), r["limit_b"].clone());
        }
        let row = serde_json::Value::Object(row);
        // Sv1's exported row, where it has one, must be the exe's.
        let ours = combat.param("StaminaControlParam", c["ctrl_row"].as_i64().unwrap());
        if !ours.is_null() {
            for (k, e) in row.as_object().unwrap() {
                if ours[k].as_i64() != e.as_i64() {
                    bad.push(format!("StaminaControlParam {} {k}: Sv1 {}, exe {e}", c["ctrl_row"], ours[k]));
                }
            }
        }
        let first = &c["runs"][0]["frames"][0];
        let base = first["base"].as_f64().unwrap() as f32;
        if c["player"].as_bool().unwrap() {
            let lvl = c["level"].as_f64().unwrap() as f32;
            let ours = crate::data::calc_correct(combat.param("CalcCorrectGraph", 504), lvl);
            if (ours - base).abs() > 1e-3 {
                bad.push(format!("Wolf's base regen at level {lvl}: Sv1 {ours}, exe {base}"));
            }
        } else {
            let n = combat.param("NpcParam", npc);
            if let Some(ours) = n["staminaRecoverBaseVel"].as_f64() {
                if (ours as f32 - base).abs() > 1e-3 {
                    bad.push(format!("NpcParam {npc} staminaRecoverBaseVel: Sv1 {ours}, exe {base}"));
                }
            }
        }
        for run in c["runs"].as_array().unwrap() {
            let dt = run["dt"].as_f64().unwrap() as f32;
            for f in run["frames"].as_array().unwrap() {
                let g = |k: &str| f[k].as_f64().unwrap() as f32;
                let ty = f["type"].as_i64().unwrap();
                let mut a = Actor::new(crate::actor::Side::Enemy, 1.0, max as f32, 0.0, 0);
                a.posture = (max as f32) - g("posture");
                a.posture_limits = crate::actor::stamina_limits(&row, ty, max).map(|(lo, hi)| (max as f32 - hi as f32, max as f32 - lo as f32));
                let (posture, carry) = if f["no_regen"].as_i64() == Some(1) {
                    (a.posture, g("carry"))
                } else {
                    let ratio = (ty >= 0).then(|| row[format!("staminaRecoverRatio_forType{ty:03}")].as_f64().unwrap() as f32);
                    let (whole, carry) = crate::actor::regen_frame(g("base"), g("speed"), g("tae_pct"), ratio, dt, g("carry"));
                    (a.bound_posture(a.posture - whole as f32), carry)
                };
                let (exe_posture, exe_carry) = (max as f32 - g("out_posture"), g("out_carry"));
                if posture != exe_posture || (carry - exe_carry).abs() > 1e-6 {
                    bad.push(format!(
                        "NpcParam {npc} type {ty} tae {} dt {dt}: posture left {} carry {} -> Sv1 {} / {carry}, exe {} / {exe_carry}",
                        f["tae_pct"], f["posture"], f["carry"], max as f32 - posture, g("out_posture")
                    ));
                }
                frames += 1;
            }
        }
        // The SpEffect rows that move regen, as Sv1 exported them (Wolf's 5221-5223, the enemies'
        // 300600-300602, 7900-7902, the area doping).
        for s in c["sp_regen"].as_array().unwrap() {
            let id = s["id"].as_i64().unwrap();
            let key = id.to_string();
            let Some(ours) = combat.player.sp_effects.get(&key).or_else(|| combat.kinds.iter().find_map(|k| k.data.sp_effects.get(&key))) else { continue };
            let (speed, change, cond) = (s["row_speed"].as_f64().unwrap() as f32, s["change"].as_f64().unwrap() as f32, s["cond_hp"].as_f64().unwrap() as f32);
            if (ours.stamina_recover_speed_rate - speed).abs() > 1e-6 || ours.stamina_recover_change_speed != change || (cond >= 0.0 && ours.condition_hp != cond) {
                bad.push(format!("SpEffect {id}: Sv1 rate {} change {} conditionHp {}, exe {speed} {change} {cond}", ours.stamina_recover_speed_rate, ours.stamina_recover_change_speed, ours.condition_hp));
            }
        }
    }
    assert!(frames > 1000, "only {frames} regen frames");
    let n = bad.len();
    bad.truncate(12);
    assert!(n == 0, "posture regen differs from the exe in {n} of {frames} frames / checks:\n{}", bad.join("\n"));
}

/// HP amounts (FUN_140840eb0 element sum, FUN_140840ce0 attack / defence curve, vfunc 0x228
/// defences): `combat::hit_hp` keeps (int) of the exe's sum and lifts a positive sum under 1 to 1,
/// the curve gives the attack back at the snapshot's globals (all 0), and every one of the
/// snapshot's characters has 0 defence (Sv1 applies none). The sum's flag factors (p13 x1.4, p14
/// x p16 or x1.125, p15 x p16 or x0.8, p17) are `combat::sum_flag_factor` (their callers:
/// emu_hprate.rs). Only non-negative elements are compared (Sv1 has no negative attack; the exe
/// clamps each element at 0).
#[test]
pub(crate) fn hit_hp_matches_the_exe() {
    let Some(v) = emu_json("damage_hp") else { return };
    let mut bad: Vec<String> = Vec::new();
    for g in v["globals"].as_array().unwrap().iter().take(6) {
        if g["f32"].as_f64() != Some(0.0) {
            bad.push(format!("curve global {} is {} (the curve is no longer the identity)", g["at"], g["f32"]));
        }
    }
    for c in v["curve"].as_array().unwrap() {
        let (atk, out) = (c["atk"].as_f64().unwrap(), c["out"].as_f64().unwrap());
        if atk >= 0.0 && atk != out {
            bad.push(format!("curve atk {atk} def {}: exe {out}", c["def"]));
        }
    }
    let mut n = 0;
    for s in v["sums"].as_array().unwrap() {
        let f = |k: &str| -> Vec<f32> { s[k].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect() };
        let (atk, rate) = (f("atk"), f("rate"));
        if atk.iter().any(|a| *a < 0.0) {
            continue;
        }
        // One of the seven rate arrays (slot) holds `rate`, the others 1; then the flag factors
        // (combat::sum_flag_factor, p13 x1.4, p14 / p15 with p16, p17).
        let sum: f32 = (0..5).map(|i| atk[i] * rate[i]).sum();
        let b = |k: &str| s[k].as_i64() == Some(1);
        let fl = |k: &str| s[k].as_f64().unwrap() as f32;
        let sum = crate::combat::sum_flag_factor(sum, b("p13"), b("p14"), b("p15"), fl("p16"), fl("p17"));
        let exe = s["out"].as_f64().unwrap() as f32;
        let ours = crate::combat::hit_hp(sum, 1.0);
        if ours != (exe as i32) as f32 {
            bad.push(format!("sum atk {atk:?} rate {rate:?} slot {}: Sv1 {ours}, exe {exe}", s["slot"]));
        }
        n += 1;
    }
    assert!(n > 50, "only {n} plain sums");
    for d in v["defence"].as_array().unwrap() {
        if d["def"].as_array().unwrap().iter().any(|x| x.as_f64() != Some(0.0)) {
            bad.push(format!("NpcParam {} defence {}", d["npc_param"], d["def"]));
        }
    }
    let n = bad.len();
    bad.truncate(12);
    assert!(n == 0, "HP amounts differ from the exe in {n} checks:\n{}", bad.join("\n"));
}

/// Posture an unguarded hit takes (FUN_140844070): (int)(staminaAttack x the attacker's rate x
/// the defender's per-attribute rate, vfunc 0x248), on Wolf and five enemies over 13 attributes,
/// five values and two rates. Sv1: `npc_attribute_rates`' NpcParam <attr>StaminaDmgRate and the
/// floor in combat.rs resolve. The defenders' SpEffect rates are all 1 here (the c1180 servants'
/// 0.6 / 1.35 come from NpcParam). gap: the guarded side (FUN_1408439a0) on Wolf came out as
/// (int)(value x rate x 0.99) with an empty weapon-info block and is not compared; enemies there
/// had no guard behaviour id (0) - which outcome and guard rate a hit gets is the guard pilot's.
#[test]
pub(crate) fn hit_posture_matches_the_exe() {
    let Some(v) = emu_json("damage_posture") else { return };
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    // The default scene's export lacks some snapshot rows (the c1180 servant 11800000; Sv1 spawns
    // 11807100 there, with the same rates): those come from the full extracted NpcParam.
    let full: serde_json::Value = std::fs::read_to_string(crate::paths::extracted().join("json/params/NpcParam.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let full_row = |id: i64| full["rows"].as_array().and_then(|rows| rows.iter().find(|r| r["id"].as_i64() == Some(id))).cloned().unwrap_or_default();
    let mut bad: Vec<String> = Vec::new();
    let mut n = 0;
    for d in v["defenders"].as_array().unwrap() {
        let npc = d["npc_param"].as_i64().unwrap();
        let player = d["player"].as_bool().unwrap();
        let row = if combat.param("NpcParam", npc).is_null() { full_row(npc) } else { combat.param("NpcParam", npc).clone() };
        for c in d["cases"].as_array().unwrap() {
            let Some(exe) = c["hit"].as_i64() else { continue };
            let attr = c["attr"].as_i64().unwrap();
            let rate = if player { 1.0 } else { crate::data::npc_stamina_rate_field(attr).and_then(|f| row[f].as_f64()).unwrap_or(1.0) as f32 };
            let exe_rate = c["attr_rate"].as_f64().unwrap() as f32;
            let value = c["value"].as_f64().unwrap() as f32 + 2.0;
            let ours = (value * c["rate"].as_f64().unwrap() as f32 * rate).floor() as i64;
            if ours != exe || (rate - exe_rate).abs() > 1e-6 {
                bad.push(format!("NpcParam {npc} attr {attr} value {value} rate {}: Sv1 {ours} (x{rate}), exe {exe} (x{exe_rate})", c["rate"]));
            }
            n += 1;
        }
    }
    assert!(n > 1000, "only {n} posture hits");
    let n2 = bad.len();
    bad.truncate(12);
    assert!(n2 == 0, "posture amounts differ from the exe in {n2} of {n} hits:\n{}", bad.join("\n"));
}
