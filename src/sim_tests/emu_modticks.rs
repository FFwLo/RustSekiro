//! Hit stop against the exe (topic modticks): SprjChrHitStopModule's start FUN_140b9dad0 and tick
//! FUN_140b9d1b0 run in tools/emu-cases (`modticks`) on Wolf's module of the 2026-11-03 snapshot,
//! and whole character frames FUN_140a4b570 with a hit stop running (Wolf and the General).

use crate::actor::HitStop;

fn load(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::extracted().join("emu").join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn f(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap() as f32
}

/// The module alone: 22 scenarios (7 lengths, 4 restarts in the hold and the ease, dt 1/60 and
/// 1/30), every frame's state, timer, length and scaled dt.
#[test]
pub(crate) fn hit_stop_module_matches_the_exe() {
    let Some(v) = load("modticks_hitstop.json") else { return };
    // The engine's constants the port takes as consts.
    assert_eq!(f(&v["dbg"]["ease_in_12c"]), crate::actor::HIT_STOP_EASE_IN);
    assert_eq!(f(&v["dbg"]["hold_130"]), crate::actor::HIT_STOP_HOLD);
    assert_eq!(v["dbg"]["switch_11d"], 0);
    let mut bad = Vec::new();
    let mut n = 0;
    for c in v["cases"].as_array().unwrap() {
        let dt = f(&c["dt"]);
        let mut h = HitStop::default();
        h.start(f(&c["length"]));
        let restart = c["restart"].as_array().map(|r| (r[0].as_u64().unwrap() as usize, f(&r[1])));
        for (i, e) in c["frames"].as_array().unwrap().iter().enumerate() {
            if let Some((rf, s)) = restart {
                if rf == i {
                    h.start(s);
                }
            }
            let scaled = h.split(dt);
            let (state, timer, length) = h.module_fields();
            n += 1;
            let exe = (e["state"].as_u64().unwrap() as u8, f(&e["timer"]), f(&e["length"]), f(&e["scaled"]));
            // The exe's length stays after the module goes idle; ours resets.
            let length_ok = state == 0 || (length - exe.2).abs() < 1e-7;
            if state != exe.0 || (timer - exe.1).abs() > 1e-7 || !length_ok || (scaled - exe.3).abs() > 1e-7 {
                bad.push(format!("length {} dt {dt} restart {restart:?} frame {i}: Sv1 ({state}, {timer}, {length}, {scaled}), exe {exe:?}", c["length"]));
            }
        }
    }
    assert!(n >= 273, "{n} frames");
    assert!(bad.is_empty(), "{} of {n} frames differ:\n{}", bad.len(), bad[..bad.len().min(12)].join("\n"));
}

/// The defender's start FUN_140b9cfb0 on Wolf and the General: the rule `combat::start_hit_stop`
/// takes for the defender (a player always, an NPC when hitStopType_Defencer FUN_140b9d8c0 is 1;
/// the General's is 0), the length hitStopTime_Defencer as given, and `HitStop::start` leaving
/// the module as the exe does. The exe starts nothing when the hit's five amounts (the damage
/// struct's floats 0-4) are all 0 (gap in Sv1, see combat.rs start_hit_stop).
#[test]
pub(crate) fn hit_stop_defender_start_matches_the_exe() {
    let Some(v) = load("modticks_defender.json") else { return };
    for c in v["cases"].as_array().unwrap() {
        let (who, amount, time) = (c["who"].as_str().unwrap(), f(&c["amount"]), f(&c["time"]));
        assert_eq!(c["mode"], 0);
        let def_on = who == "wolf" || c["def_type"] == 1;
        assert_eq!(c["def_type"], if who == "wolf" { 1 } else { 0 }, "{c}");
        let started = amount > 0.0 && time > 0.0 && def_on;
        assert_eq!(c["started"].as_bool().unwrap(), started, "{c}");
        if started {
            let mut h = HitStop::default();
            h.start(time);
            let (state, timer, length) = h.module_fields();
            let m = &c["module"];
            assert_eq!((state as u64, timer, length), (m["state"].as_u64().unwrap(), f(&m["timer"]), f(&m["length"])), "{c}");
        }
    }
}

/// Whole frames: the clock the pre-update FUN_1409f78a0 leaves in chr+0x108 is the module's tick,
/// and the Havok behaviour (the animation, FUN_140b45440 -> FUN_140fff000) steps by that clock,
/// for Wolf and the General alike, so `actor::advance` runs the clip, TAE and root motion on
/// `HitStop::split`.
#[test]
pub(crate) fn hit_stop_frames_match_the_exe() {
    let Some(v) = load("modticks_frames.json") else { return };
    let mut bad = Vec::new();
    let mut n = 0;
    for s in v["scenarios"].as_array().unwrap() {
        let dt = f(&s["dt"]);
        let mut h = HitStop::default();
        h.start(f(&s["length"]));
        for (i, e) in s["frames"].as_array().unwrap().iter().enumerate() {
            assert!(e["stopped"].is_null(), "{} frame {i} stopped: {}", s["who"], e["stopped"]);
            let clock = h.split(dt);
            n += 1;
            let chr_dt = f(&e["chr_dt"]);
            if (clock - chr_dt).abs() > 1e-7 {
                bad.push(format!("{} dt {dt} length {} frame {i}: Sv1 clock {clock}, exe chr+0x108 {chr_dt}", s["who"], s["length"]));
            }
            for step in e["beh_steps"].as_array().unwrap() {
                if (f(step) - chr_dt).abs() > 1e-7 {
                    bad.push(format!("{} dt {dt} frame {i}: behaviour step {step} is not the clock {chr_dt}", s["who"]));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} of {n} frames differ:\n{}", bad.len(), bad[..bad.len().min(12)].join("\n"));
}
