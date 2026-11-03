//! The enemy targeting system frame by frame against the exe (topic aiframe).

use bevy::prelude::*;

use crate::stealth::{in_cone_shape, in_nose, Cone, Seen, Targeting};

/// SprjTargetingSystem::update (vfunc 0x10 = FUN_14061ff40, run in tools/emu-cases
/// `aiframe_targeting` on a memory snapshot of the user's sekiro.exe) for three enemies
/// (NpcThinkParam 11500001, 12400000, 10100304) and 7-8 scenarios each: seen then lost until
/// the sight and memory slots run out, the around meter filling into an indication target and
/// draining, Wolf flickering in and out of the cone, above the cone (perceive set), FIND beyond
/// BattleStartDist then BATTLE, circling, and the nose sphere (nose_dist poked to 3). Each frame
/// `Targeting::update` runs with the exe's eye, dummy 500 and facing, the sight search forced on
/// the frames the exe searched (its timer reset); state, AI_TARGET_TYPE, the perceive flag, the
/// meter and the slots' forget times must match. Scenarios where the exe's sight ray was blocked
/// by the map while Wolf stood in a cone are left out (no map of the snapshot's area here). Also
/// the `ai:` queries on TARGET_ENE_0 with what the exe held. Skipped when
/// extracted/emu/aiframe_targeting.json is missing.
#[test]
pub(crate) fn targeting_frames_match_the_exe() {
    let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("emu/aiframe_targeting.json")) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let params: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("json/params/NpcThinkParam.json")).unwrap()).unwrap();
    let row = |id: i64| params["rows"].as_array().unwrap().iter().find(|r| r["id"].as_i64() == Some(id)).cloned().unwrap();
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mirror = |v: &serde_json::Value| Vec3::new(-f(&v[0]), f(&v[1]), f(&v[2]));
    let (mut frames, mut queries, mut skipped, mut bad) = (0, 0, Vec::new(), Vec::new());
    for sc in v["scenarios"].as_array().unwrap() {
        let mut think = row(sc["think"].as_i64().unwrap());
        let name = format!("{} {}", sc["think"], sc["scenario"].as_str().unwrap());
        if sc["scenario"] == "nose" {
            think["nose_dist"] = serde_json::json!(3);
        }
        let (eye, head, fwd, me) = (mirror(&sc["eye"]), mirror(&sc["head"]), mirror(&sc["fwd"]), mirror(&sc["chr_pos"]));
        let (radius, own) = (f(&sc["radius"]), f(&sc["own_radius"]));
        let seen = |fr: &serde_json::Value| Seen { pos: mirror(&fr["wolf"]), radius, cut: 1.0, ang: [1.0; 4], around_rate: 1.0, light: 1.0 };
        let cones = ["normal", "around", "perceive"].map(|s| Cone::from(&think, s));
        let list = sc["frames"].as_array().unwrap();
        if list.iter().any(|fr| fr["ray"] == 0 && cones.iter().any(|c| in_cone_shape(c, eye, fwd, &seen(fr)))) {
            skipped.push(name);
            continue;
        }
        let mut t = Targeting::new(1);
        let mut timer = 0.0f32;
        for (i, fr) in list.iter().enumerate() {
            let dt = f(&fr["dt"]);
            let wolf = seen(fr);
            // The exe searched this frame when its timer was reset (FUN_140623ca0).
            let searched = f(&fr["timer"]) > timer - dt + 1e-5;
            timer = f(&fr["timer"]);
            t.search_timer = if searched { dt * 0.5 } else { 1e9 };
            t.update(&think, me, eye, head, fwd, &wolf, dt);
            frames += 1;
            let at = format!("{name} f{i}");
            let nodes: Vec<&serde_json::Value> = fr["nodes"].as_array().unwrap().iter().collect();
            let node = |k: u64| nodes.iter().find(|n| n[0].as_u64() == Some(k)).map(|n| f(&n[1]));
            let exe = (fr["state"].as_u64().unwrap() as u8, fr["kind"].as_u64().unwrap() as u8, fr["flags"].as_u64().unwrap() & 0x10_0000 != 0);
            let ours = (t.state, t.kind, t.perceive);
            let meter = (f(&fr["meter"]), t.meter.map_or(-1.0, |m| m.points));
            let close = |a: Option<f32>, b: Option<f32>| match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() < 2e-3,
                (a, b) => a.is_none() == b.is_none(),
            };
            // Node kinds: 1 the normal target (+0x2c runs once unseen), 12 memory, 13 indication.
            let normal = (node(1).filter(|&x| x > 0.0), t.normal.filter(|n| !n.held).map(|n| n.forget));
            let (memory, indication) = ((node(12), t.memory.map(|m| m.forget)), (node(13), t.indication.map(|m| m.forget)));
            if exe != ours || (meter.0 - meter.1).abs() > 2e-3 || !close(normal.0, normal.1) || !close(memory.0, memory.1) || !close(indication.0, indication.1) {
                if bad.len() < 12 {
                    bad.push(format!("{at}: exe {exe:?} meter {} normal {:?} memory {:?} indication {:?}; ours {ours:?} meter {} normal {:?} memory {:?} indication {:?}", meter.0, normal.0, memory.0, indication.0, meter.1, normal.1, memory.1, indication.1));
                }
                continue;
            }
            // The queries on TARGET_ENE_0: GetDist, GetOriginDist, GetToTargetAngle.
            if let Some(q) = fr["ene0"].as_array() {
                queries += 1;
                let q = [f(&q[0]), f(&q[1]), f(&q[2])];
                let want = match t.target_pos(wolf.pos) {
                    None => [-1.0, -1.0, 0.0],
                    Some(p) => {
                        let r = if t.kind == crate::stealth::TYPE_NORMAL_ENEMY { radius } else { 0.0 };
                        let d = (p - me).length();
                        [crate::ai::surface_dist(d, p.y - me.y, r) - own, d, crate::ai::to_target_angle(fwd, p - me)]
                    }
                };
                let da = ((q[2] - want[2] + 540.0).rem_euclid(360.0) - 180.0).abs();
                if (q[0] - want[0]).abs() > 2e-3 || (q[1] - want[1]).abs() > 2e-3 || da > 0.05 {
                    if bad.len() < 12 {
                        bad.push(format!("{at} TARGET_ENE_0: exe {q:?}, ours {want:?}"));
                    }
                }
            }
        }
    }
    eprintln!("aiframe: {frames} frames, {queries} TARGET_ENE_0 queries, skipped {skipped:?}");
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(skipped.len() <= 8, "skipped (sight ray blocked): {skipped:?}");
    assert!(frames > 5_000 && queries > 1_000, "{frames} frames, {queries} queries, skipped {skipped:?}");
}

/// The nose sphere alone (FUN_14061c940, tools/emu-cases `aiframe_ray`) on three enemies with
/// nose_dist 0 / 2 / 5 poked and Wolf's feet placed 0.5-5.5 m from the sight origin in three
/// directions: 189 cases against `in_nose`; and the ray's two ends (ctx +0x30 = dummy 500, ctx
/// +0xa0 = Wolf's feet + 1.5).
#[test]
pub(crate) fn nose_sphere_matches_the_exe() {
    let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("emu/aiframe_ray.json")) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mirror = |v: &serde_json::Value| Vec3::new(-f(&v[0]), f(&v[1]), f(&v[2]));
    let mut n = 0;
    for o in v["observers"].as_array().unwrap() {
        let (eye, radius) = (mirror(&o["eye"]), f(&o["radius"]));
        // ctx +0xa0 (floats 40..43) is ctx +0x90 (36..39) raised by 1.5.
        let ctx = o["ctx"].as_array().unwrap();
        assert!((f(&ctx[41]) - f(&ctx[37]) - crate::stealth::WOLF_SIGHT_HEIGHT).abs() < 1e-4);
        assert_eq!([f(&ctx[12]), f(&ctx[14])], [f(&o["head"][0]), f(&o["head"][2])]);
        for c in o["nose"].as_array().unwrap() {
            let think = serde_json::json!({"nose_dist": c["nose_dist"]});
            let wolf = Seen { pos: mirror(&c["feet"]), radius, cut: 1.0, ang: [1.0; 4], around_rate: 1.0, light: 1.0 };
            assert_eq!(in_nose(&think, eye, &wolf), c["hit"] == 1, "{} {c}", o["think"]);
            n += 1;
        }
    }
    assert_eq!(n, 189);
}
