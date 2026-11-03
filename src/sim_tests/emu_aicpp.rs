//! The enemy's sight cones against the exe's (topic aicpp).

use bevy::prelude::*;

use crate::stealth::{in_cone_shape, Cone, Seen};

/// The targeting system's sight checks (FUN_140617280 -> FUN_14061bb70 normal / perceive,
/// FUN_1406172f0 -> FUN_14061c2a0 around, run in tools/emu-cases `aicpp_sight` on a memory
/// snapshot of the user's sekiro.exe, ray off) for the six nearest enemies with distinct
/// NpcThinkParam rows, Wolf placed around each one's eye point at 0.5-60 m, 0-180 deg either side
/// and -12..12 m up, with the context's perceive flag, light level (Dark, PitchDark), distance cut
/// (crouch 0.8) and angle factors poked: 52,554 placements against `in_cone_shape` from the exe's
/// eye point and facing. The exe's X is mirrored into Bevy space. Skipped when
/// extracted/emu/aicpp_sight.json is missing.
#[test]
pub(crate) fn sight_cones_match_the_exe() {
    let Ok(text) = std::fs::read_to_string(crate::paths::emu().join("aicpp_sight.json")) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let params: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("json/params/NpcThinkParam.json")).unwrap()).unwrap();
    let row = |id: i64| params["rows"].as_array().unwrap().iter().find(|r| r["id"].as_i64() == Some(id)).cloned().unwrap();
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mirror = |v: &serde_json::Value| Vec3::new(-f(&v[0]), f(&v[1]), f(&v[2]));
    let (mut n, mut bad) = (0, Vec::new());
    for o in v["observers"].as_array().unwrap() {
        let think = row(o["think"].as_i64().unwrap());
        let set = if o["perceive"].as_u64() == Some(1) { "perceive" } else { "normal" };
        let (cone, around) = (Cone::from(&think, set), Cone::from(&think, "around"));
        assert_eq!(cone.disable_dark, o["disable_dark"].as_u64() == Some(1));
        let (eye, fwd) = (mirror(&o["eye"]), mirror(&o["fwd"]));
        let light = f(&o["light_table"][o["light_index"].as_u64().unwrap() as usize]);
        // The exe's light table (DAT_143afb490..8) is what Seen::light documents.
        assert_eq!([f(&o["light_table"][0]), f(&o["light_table"][1]), f(&o["light_table"][2])], [1.0, 0.5, 0.3]);
        assert_eq!(f(&o["ctx50"]), 1.0, "observer vfunc 0xe0 (taken as 1)");
        let ang = [0, 1, 2, 3].map(|i| f(&o["ang"][i]));
        for c in o["cases"].as_array().unwrap() {
            let wolf = Seen { pos: mirror(&c["target"]), radius: f(&o["radius"]), cut: f(&o["cut"]), ang, around_rate: 1.0, light };
            let want = (c["normal"].as_u64().unwrap(), c["around"].as_u64().unwrap());
            let got = (if in_cone_shape(&cone, eye, fwd, &wolf) { 2 } else { 0 }, in_cone_shape(&around, eye, fwd, &wolf) as u64);
            n += 1;
            if got != want && bad.len() < 10 {
                bad.push(format!("{} {} at {:?}: exe {want:?}, ours {got:?}", o["think"], o["variant"], wolf.pos));
            }
        }
    }
    assert_eq!(n, 52_554);
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The Lua `ai:` distance and angle queries (tools/emu-cases `aicpp_query`: GetDist
/// 0x1405e2c10, GetOriginDist FUN_1405e3fb0, GetToTargetAngle FUN_1405e4880, GetDistY
/// FUN_1405e2e70, GetDistYSigned FUN_1405e2f30, IsInsideTarget FUN_1405e56e0) on three enemies
/// with Wolf (TARGET_LOCALPLAYER) placed 0.3-12 m away, 0-180 deg either side, 3 m below to 2 m
/// above: 288 placements against `surface_dist`, `to_target_angle` and `angle_in`. With no
/// target (TARGET_ENE_0 in the snapshot) every query reads -1 and IsInsideTarget false.
#[test]
pub(crate) fn ai_distance_queries_match_the_exe() {
    let Ok(text) = std::fs::read_to_string(crate::paths::emu().join("aicpp_query.json")) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mirror = |v: &serde_json::Value| Vec3::new(-f(&v[0]), f(&v[1]), f(&v[2]));
    let mut n = 0;
    for o in v["observers"].as_array().unwrap() {
        let me = mirror(&o["chr_pos"]);
        // The exe's facing (-sin yaw, -cos yaw), mirrored.
        let yaw = f(&o["chr_yaw"]);
        let fwd = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
        let (own, wolf_r) = (f(&o["map_hit_radius"]), f(&o["wolf_radius"]));
        assert_eq!(wolf_r, crate::player::CAPSULE_RADIUS);
        assert_eq!(own, f(&o["chr_radius"]), "GetMapHitRadius = the physics radius");
        for c in o["cases"].as_array().unwrap() {
            let w = mirror(&c["wolf"]);
            let at = format!("{} wolf {w:?}", o["think"]);
            let (dist, dy) = ((w - me).length(), w.y - me.y);
            let close = |k: &str, ours: f32, tol: f32| assert!((f(&c[k]) - ours).abs() < tol, "{k} {at}: exe {}, ours {ours}", f(&c[k]));
            close("dist_21", crate::ai::surface_dist(dist, dy, wolf_r) - own, 1e-4);
            close("origin_dist_21", dist, 1e-4);
            close("dist_y_21", dy.abs(), 1e-5);
            close("dist_y_signed_21", dy, 1e-5);
            let angle = crate::ai::to_target_angle(fwd, w - me);
            let exe = f(&c["angle_21"]);
            assert!(((exe - angle + 540.0).rem_euclid(360.0) - 180.0).abs() < 1e-3, "angle {at}: exe {exe}, ours {angle}");
            // AI_DIR_TYPE F / B / L / R (1-4) x 90 / 60 deg.
            let inside: Vec<u64> = c["inside_21"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect();
            // Placements exactly on a cone's edge (30 / 45 deg off a centre) fall in or out in the
            // exe one way or the other by f32 rounding, so those are left out.
            let mut i = 0;
            for centre in [0.0f64, 180.0, -90.0, 90.0] {
                for w in [90.0f64, 60.0] {
                    let off = ((exe as f64 - centre + 540.0).rem_euclid(360.0) - 180.0).abs();
                    if (off - w / 2.0).abs() > 1e-3 {
                        assert_eq!(inside[i], crate::ai::angle_in(centre, w, exe as f64) as u64, "IsInsideTarget {at} angle {exe} centre {centre} width {w}");
                    }
                    i += 1;
                }
            }
            for k in ["dist_0", "origin_dist_0", "dist_y_0", "dist_y_signed_0"] {
                assert_eq!(f(&c[k]), -1.0, "{k} without a target");
            }
            n += 1;
        }
    }
    assert_eq!(n, 288);
}
