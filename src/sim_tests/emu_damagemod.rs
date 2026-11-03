//! Damage-side module ticks and the hit's own gates against the exe (topic damagemod).

use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let path = crate::paths::root().join("extracted/emu").join(name);
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn f(v: &Value, k: &str) -> f32 {
    v[k].as_f64().unwrap_or_else(|| panic!("{k}")) as f32
}

/// The exe's pool (SuperArmor module +0x18) as a SuperArmor.
fn pool_of(v: &Value, max: f32, delay: f32) -> crate::actor::SuperArmor {
    crate::actor::SuperArmor {
        max,
        cur: f(v, "cur"),
        bonus: f(v, "bonus"),
        timer: f(v, "timer"),
        broken: v["broken"].as_i64() == Some(1),
        delay,
        was_broken: v["was_broken"].as_i64() == Some(1),
        last_full: f(v, "last_full"),
    }
}

fn same(sa: &crate::actor::SuperArmor, v: &Value) -> bool {
    (sa.cur - f(v, "cur")).abs() < 1e-3
        && (sa.timer - f(v, "timer")).abs() < 1e-5
        && sa.broken == (v["broken"].as_i64() == Some(1))
        && sa.was_broken == (v["was_broken"].as_i64() == Some(1))
        && (sa.last_full - f(v, "last_full")).abs() < 1e-3
}

/// The super-armour pool frame by frame (tools/emu-cases damagemod: FUN_140bdbcb0 ->
/// FUN_140bdb690 after hits FUN_140bdb470 on the General with superArmorDurability 200, delay
/// vfunc 0x2d8 7 s; 9 scenarios, 1,760 ticks at 1/60 and 1/30 s: the timer's run-out, two hits,
/// breaks with and without the hold flag, a break inside the timer, a hit on a broken pool, a
/// changed full size) against actor::SuperArmor::tick / hit. Then whole chr frames
/// FUN_140a4b570 (ChrIns vf25 FUN_140a05060 ticks the pool) with a hit stop: the timer runs on
/// the hit-stop clock (chr+0x108; actor::HitStop), held while the stop holds, and a break refills
/// on the second frame whatever the clock. Skipped when extracted/emu/damagemod_sa.json is missing.
#[test]
pub(crate) fn super_armour_ticks_match_the_exe() {
    let Some(v) = load("damagemod_sa.json") else { return };
    let mut bad = Vec::new();
    let mut ticks = 0;
    for c in v["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (max, delay, dt) = (f(c, "max"), f(c, "delay"), f(c, "dt"));
        // The drain's rates: FUN_140bfdd20 (saReceiveDamageRate product) and FUN_140bfd040 when >= 0.
        let (r1, r2) = (f(c, "rate_dd20"), f(c, "rate_d040"));
        let rate = r1 * if r2 >= 0.0 { r2 } else { 1.0 };
        let mut sa = pool_of(&c["start"], max, delay);
        for (i, fr) in c["frames"].as_array().unwrap().iter().enumerate() {
            if !fr["hit"].is_null() {
                // The generator's amounts per scenario, in order of the frames that carry a hit.
                let amount = hit_amount(name, i);
                sa.hit(amount * rate);
                if !same(&sa, &fr["hit"]) {
                    bad.push(format!("{name} frame {i} hit: ours {sa:?}, exe {}", fr["hit"]));
                }
            }
            sa.tick(dt, fr["reacting"].as_bool().unwrap(), false, 0.0);
            ticks += 1;
            if !same(&sa, &fr["after"]) {
                bad.push(format!("{name} frame {i}: ours {sa:?}, exe {}", fr["after"]));
                sa = pool_of(&fr["after"], max, delay);
            }
        }
    }
    assert!(ticks > 1500, "{ticks} ticks");
    // Whole frames: the hit stop's clock drives the pool's timer.
    if let Some(fv) = load("damagemod_frames.json") {
        for s in fv["scenarios"].as_array().unwrap() {
            let (dt, stop, amount, max) = (f(s, "dt"), f(s, "hit_stop"), f(s, "amount"), f(s, "max"));
            let mut hs = crate::actor::HitStop::default();
            hs.start(stop);
            let mut sa = crate::actor::SuperArmor::new(max, 0.4);
            sa.hit(amount * 0.6);
            for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
                let clock = hs.split(dt);
                if (clock - f(fr, "chr_dt")).abs() > 1e-6 {
                    bad.push(format!("frames dt {dt} stop {stop} amount {amount} frame {i}: clock ours {clock}, exe {}", fr["chr_dt"]));
                }
                sa.tick(clock, false, false, 0.0);
                if !same(&sa, &fr["pool"]) {
                    bad.push(format!("frames dt {dt} stop {stop} amount {amount} frame {i}: ours {sa:?}, exe {}", fr["pool"]));
                }
                // The toughness module (NpcParam toughness 0 on every row) and the damage module's
                // per-frame byte stay 0.
                assert_eq!(f(&fr["toughness"], "cur"), 0.0);
                assert_eq!(fr["damage_c8"].as_i64(), Some(0));
                assert!(fr["stopped"].is_null());
            }
        }
    }
    assert!(bad.is_empty(), "{} differences:\n{}", bad.len(), bad[..bad.len().min(30)].join("\n"));
}

/// The amounts the generator's scenarios hit with (tools/emu-cases/src/damagemod.rs `sa`).
fn hit_amount(name: &str, frame: usize) -> f32 {
    match (name, frame) {
        ("break" | "break_held", 0) => 400.0,
        ("break_in_timer", 0) => 150.0,
        ("break_in_timer", 30) => 200.0,
        ("hit_while_broken", 0) => 400.0,
        ("hit_while_broken", 1) => 50.0,
        _ => 60.0,
    }
}

/// The rows of an exported param (extracted/json/params).
fn param(name: &str) -> std::collections::HashMap<i64, Value> {
    let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
    let v: Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let rows: Vec<Value> = match v["rows"].clone() {
        Value::Array(a) => a,
        Value::Object(o) => o.into_iter().map(|(_, r)| r).collect(),
        _ => Vec::new(),
    };
    rows.into_iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect()
}

/// The hit's five amounts (FUN_14083ddc0 -> the owner's vfunc 0x1d8 = FUN_14083dea0 for an
/// enemy; tools/emu-cases damagemod_amounts, the attack's rates 1, 154 AtkParam_Npc rows: the
/// General's, rows with no element damage and rows with one element only) are each the row's
/// element value, so a hit starts no hit stop exactly when Attack::amounts_zero holds.
/// Skipped when extracted/emu/damagemod_amounts.json is missing.
#[test]
pub(crate) fn zero_amount_hits_match_the_exe() {
    let Some(v) = load("damagemod_amounts.json") else { return };
    let rows = param("AtkParam_Npc");
    if rows.is_empty() {
        return;
    }
    let mut bad = Vec::new();
    let mut n = 0;
    for c in v["cases"].as_array().unwrap().iter().filter(|c| c["pc"] == false) {
        let id = c["id"].as_i64().unwrap();
        let atk: crate::data::Attack = serde_json::from_value(rows[&id].clone()).unwrap_or_else(|e| panic!("AtkParam_Npc {id}: {e}"));
        let exe: Vec<f32> = c["amounts"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
        let ours = [atk.atk_phys, atk.atk_mag, atk.atk_fire, atk.atk_thun, atk.atk_dark];
        if exe != ours || exe.iter().all(|a| *a <= 0.0) != atk.amounts_zero(false) {
            bad.push(format!("{id}: exe {exe:?}, ours {ours:?}"));
        }
        n += 1;
    }
    assert!(n > 100, "{n} rows");
    assert!(bad.is_empty(), "amounts differ:\n{}", bad.join("\n"));
    // And Sv1 starts no hit stop for them.
    let zero = crate::data::Attack { hit_stop_time: 0.1, hit_stop_time_defencer: 0.1, ..Default::default() };
    assert!(zero.amounts_zero(false) && zero.amounts_zero(true));
    let slash = crate::data::Attack { atk_phys: 100.0, ..zero.clone() };
    assert!(!slash.amounts_zero(false));
}

/// The guard arc with the Loaded Umbrella in Wolf's hand (tools/emu-cases damagemod_guard:
/// Wolf's vfunc 0x338 answering EquipParamWeapon 76000, judge 100 -> 107600100, the General's
/// slash 10200100 from 0 to 180 deg, the katana alongside) against combat::in_guard_arc, and the
/// parts guard (FUN_140b6c880 -> FUN_1407dceb0 body part -> NpcParam partsAtkParamId1-8,
/// FUN_1410c66a0; the hit +0x1ce): never set on the General, whose body parts map to no guard
/// part even with his parts rows set to a guard row; no NpcParam row names a parts row that
/// could guard (guardBreakRate above an attack's guardAtkRate), so Sv1 has nothing to port.
/// Skipped when extracted/emu/damagemod_guard.json is missing.
#[test]
pub(crate) fn umbrella_arc_and_parts_guard_match_the_exe() {
    let Some(v) = load("damagemod_guard.json") else { return };
    let mut bad = Vec::new();
    for c in v["cases"].as_array().unwrap() {
        assert!(c["ran"].as_bool().unwrap());
        assert_eq!(c["parts_guard"].as_i64(), Some(0), "{c}");
        assert_eq!(c["part_of"].as_i64(), Some(0), "{c}");
        if !c["wolf_defends"].as_bool().unwrap() {
            continue;
        }
        let umbrella = c["weapon"].as_i64() == Some(76000);
        let angle = c["angle"].as_f64().unwrap() as f32;
        // Wolf's guard angle: vfunc 0x258 = the weapon in hand's guardAngle.
        let mut wolf = crate::actor::Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
        if umbrella {
            // An umbrella guard state (prosthetic::hks::sub_guard): the open umbrella in hand.
            wolf.state = "GroundSubAttackGuardAction".into();
        }
        let arc = crate::combat::guard_angle(&wolf);
        assert_eq!(c["guard_angle"].as_i64(), Some(arc as i64));
        // The attack's direction turned `angle` from straight into Wolf's face (yaw 0, facing -Z).
        let face = wolf.forward();
        let (s, co) = angle.to_radians().sin_cos();
        let dir = bevy::prelude::Vec3::new(-(face.x * co - face.z * s), 0.0, -(face.x * s + face.z * co));
        let ours = crate::combat::in_guard_arc(face, dir, arc);
        let exe = c["guarded"].as_i64() == Some(1) || c["deflected"].as_i64() == Some(1);
        if ours != exe {
            bad.push(format!("umbrella {umbrella} angle {angle} window {}: ours {ours}, exe {exe}", c["window"]));
        }
    }
    assert!(bad.is_empty(), "guard arc differs:\n{}", bad.join("\n"));
    // No NpcParam row's parts AtkParam can out-guard an attack (guardBreakRate 0 or no row).
    let (npc, atk) = (param("NpcParam"), param("AtkParam_Npc"));
    for (id, r) in &npc {
        for k in 1..=8 {
            let p = r[format!("partsAtkParamId{k}").as_str()].as_i64().unwrap_or(-1);
            if p >= 0 {
                let gb = atk.get(&p).and_then(|a| a["guardBreakRate"].as_i64()).unwrap_or(0);
                assert_eq!(gb, 0, "NpcParam {id} part {k}: AtkParam {p} guardBreakRate {gb}");
            }
        }
    }
}
