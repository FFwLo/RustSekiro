//! The HKS engine surface in poses other than idle and the act ids (src/hks_env.rs) against the
//! exe: tools/emu-cases `hks2` on the 2026-11-03 snapshot (Wolf in General Naomori's arena, AI
//! live). `hks2_env.json`: the env dispatcher FUN_140b36410 with the handler fields written;
//! `hks2_act.json`: the act dispatcher FUN_140b32410 and what it wrote in Wolf's modules.

use std::collections::{BTreeSet, HashMap};

use crate::hks_env::HksEnv;

/// The mirror of the exe's fields from a JSON object of `name: value` (the generator's names).
fn fields(idle: &serde_json::Value) -> HashMap<String, f64> {
    idle.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap())).collect()
}

/// Every env pose (a handler's fields written on the snapshot's Wolf, the id run) answers the
/// same from the ported rule (`HksEnv::env` on the same fields): 47 ids, 223 poses.
#[test]
pub(crate) fn hks_env_poses_match_the_exe() {
    let path = crate::paths::root().join("extracted/emu/hks2_env.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut idle = fields(&v["idle"]);
    idle.insert("pos_y".into(), v["pos_y"].as_f64().unwrap());
    idle.insert("fall_death_drop".into(), v["fall_death_drop"].as_f64().unwrap());
    assert_eq!(v["fall_death_drop"].as_f64().unwrap() as f32, crate::hks_env::FALL_DEATH_DROP, "the snapshot's fall death drop moved");

    let (mut diffs, mut ids) = (Vec::new(), BTreeSet::new());
    let cases = v["cases"].as_array().unwrap();
    for c in cases {
        assert_eq!(c["stopped"], false, "exe pose stopped: {c}");
        let id = c["id"].as_i64().unwrap() as i32;
        let args: Vec<i64> = c["args"].as_array().unwrap().iter().map(|a| a.as_f64().unwrap() as i64).collect();
        let mut env = HksEnv { fields: idle.clone(), ..Default::default() };
        // The exe's idle answer from the idle fields too.
        let idle_ours = env.env(id, &args);
        env.fields.extend(fields(&c["set"]));
        let ours = env.env(id, &args);
        let exe = c["ret"].as_i64().unwrap() as i32;
        let exe_idle = c["idle_ret"].as_i64().unwrap() as i32;
        ids.insert(id);
        if ours != Some(exe) || idle_ours != Some(exe_idle) {
            diffs.push(format!("env {id}{args:?} {} {}: Sv1 {ours:?} (idle {idle_ours:?}), exe {exe} (idle {exe_idle})", c["what"], c["set"]));
        }
    }
    println!("hks env poses: {} poses over {} ids", cases.len(), ids.len());
    assert!(diffs.is_empty(), "the ported env rules differ from the exe:\n{}", diffs.join("\n"));
}

/// Exe effects of an act Sv1 does not mirror, (act id, field): why.
const ACT_GAPS: &[(i64, &str, &str)] = &[
    (3023, "phys+0x3c0", "SetMovePrecalculation's physics side (the anim) for types 2-5"),
    (3023, "phys+0x3d0", "SetMovePrecalculation's fraction from the anim's root motion"),
    (3023, "phys+0x3e0", "SetMovePrecalculation's fraction from the anim's root motion"),
];

/// Every act Wolf's scripts call, run on the snapshot's Wolf (the bit words it sets cleared first):
/// the fields the exe wrote equal what `HksEnv::act` writes, and Sv1 writes none the exe left; an
/// AddSpEffect is seen by GetSpEffectID after it.
#[test]
pub(crate) fn hks_act_matches_the_exe() {
    let root = crate::paths::root().join("extracted/emu");
    let (Ok(env_text), Ok(text)) = (std::fs::read_to_string(root.join("hks2_env.json")), std::fs::read_to_string(root.join("hks2_act.json"))) else { return };
    let idle = fields(&serde_json::from_str::<serde_json::Value>(&env_text).unwrap()["idle"]);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();

    let (mut diffs, mut ported, mut run, mut gaps_seen) = (Vec::new(), BTreeSet::new(), 0, BTreeSet::new());
    for c in v["cases"].as_array().unwrap() {
        let Some(args) = c["args"].as_array() else { continue };
        assert!(c.get("stopped").is_none(), "exe act stopped: {c}");
        run += 1;
        let id = c["id"].as_i64().unwrap();
        let args: Vec<f64> = args.iter().map(|a| a.as_f64().unwrap()).collect();
        let mut env = HksEnv { fields: idle.clone(), ..Default::default() };
        env.fields.extend(fields(&c["pre"]));
        let before = env.fields.clone();
        if env.act(id as i32, &args) {
            ported.insert(id);
        }
        let exe: HashMap<String, f64> = c["fields"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v[1].as_f64().unwrap())).collect();
        // Changed beyond f32 rounding (an already unit hit direction normalises to itself).
        let moved = |k: &String, v: &f64| before.get(k).is_none_or(|b| (b - v).abs() > 1e-6 * b.abs().max(1.0));
        let ours: HashMap<String, f64> = env.fields.iter().filter(|(k, v)| moved(k, v)).map(|(k, v)| (k.clone(), *v)).collect();
        for name in exe.keys().chain(ours.keys()).collect::<BTreeSet<_>>() {
            let (e, o) = (exe.get(name).copied(), ours.get(name).copied());
            let same = match (e, o) {
                (Some(e), Some(o)) => (e - o).abs() <= 1e-5 * e.abs().max(1.0),
                _ => false,
            };
            if same {
                continue;
            }
            if let Some(g) = ACT_GAPS.iter().find(|g| g.0 == id && g.1 == name) {
                assert!(o.is_none(), "act {id} {name} is now written by Sv1; drop it from ACT_GAPS ({})", g.2);
                gaps_seen.insert((id, name.clone()));
                continue;
            }
            diffs.push(format!("act {id}{args:?} (pre {}): {name} exe {e:?}, Sv1 {o:?}", c["pre"]));
        }
        if let Some(sp) = c.get("env_after_1116") {
            let ours = env.env(crate::hks_env::GET_SP_EFFECT_ID, &[args[0] as i64]);
            if Some(sp.as_i64().unwrap() as i32) != ours && id == crate::hks_env::ACT_ADD_SP_EFFECT as i64 {
                diffs.push(format!("act {id}{args:?}: GetSpEffectID after it exe {sp}, Sv1 {ours:?}"));
            }
        }
    }
    println!("hks acts: {run} cases run, {} ids ported, {} gap fields", ported.len(), gaps_seen.len());
    assert!(diffs.is_empty(), "Sv1's act effects differ from the exe:\n{}", diffs.join("\n"));
}

/// The move stick under a turning camera and with ctrl flags +0x88 bit 18 (tools/emu-cases
/// `hks2_move`, the PlayerIns update as in `wolf_turn_matches_the_exe`): with the snapshot's
/// switches (both debug-menu ones off) the move vector follows the camera yaw of its own frame and
/// bit 18 changes nothing, as Sv1's `Turn` / `stick_world` do. The camera-yaw hold (a debug menu
/// bool, 0x143d5b49c, "don't align the stick's forward with the camera's") and the halving behind
/// bit 18 (only with the debug global 0x143d7a34a) are recorded, not ported.
#[test]
pub(crate) fn move_stick_switches_match_the_exe() {
    use crate::actor::wrap_angle;
    use crate::player::{stick_world, yaw_of, Turn};
    use bevy::prelude::*;
    let path = crate::paths::root().join("extracted/emu/hks2_move.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    // The switches as the game ships them (the snapshot): hold off, bit 18 gated off.
    assert_eq!(v["info"]["cam_hold"], 0, "the camera-yaw hold is on in the snapshot");
    assert_eq!(v["info"]["walk_gate"], 0, "the bit 18 gate is on in the snapshot");
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mut report = Vec::new();
    let mut frames = 0;
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let debug_on = !s["globals"].as_array().unwrap().is_empty();
        let (mut turn, mut yaw, mut shown) = (Turn::default(), 0.0f32, None);
        let mut worst = 0.0f32;
        for fr in s["frames"].as_array().unwrap() {
            let raw = Vec2::new(-f(&fr["stick"][1]), f(&fr["stick"][0]));
            turn.new_tick();
            let taken = turn.gate(raw);
            let world = stick_world(taken, f(&fr["cam"])) * taken.length();
            turn.turn(&mut yaw, &mut shown, (world != Vec3::ZERO).then(|| yaw_of(world)), None, 720.0, 1.0 / 60.0);
            let err = (world - Vec3::new(f(&fr["world"][0]), 0.0, f(&fr["world"][1]))).length()
                + (taken.length() - f(&fr["level"])).abs()
                + wrap_angle(yaw - f(&fr["yaw"])).abs();
            worst = worst.max(err);
            frames += 1;
        }
        // With a debug switch on the exe differs (held camera yaw, halved stick); Sv1 ports neither.
        if debug_on {
            assert!(worst > 0.1, "{name}: the exe with a debug switch on now matches Sv1 (worst {worst})");
        } else if worst > 1e-4 {
            report.push(format!("{name}: off by {worst}"));
        }
    }
    println!("move switches: {frames} frames");
    assert!(report.is_empty(), "the move stick differs from the exe:\n{}", report.join("\n"));
}
