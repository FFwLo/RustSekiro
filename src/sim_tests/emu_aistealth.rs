//! The stealth inputs of the sight check against the exe (topic aistealth, row E33).

use bevy::prelude::*;
use serde_json::{json, Value};

use crate::stealth::{geometry_bits, in_cone_shape, in_nose, Cone, Observer, Seen, LIGHT_TABLE};

fn cases() -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join("aistealth.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn v3(v: &Value) -> Vec3 {
    Vec3::new(f(&v[0]), f(&v[1]), f(&v[2]))
}

/// A SpEffectParam row's sight fields, with the harness's overrides (sightCutLimitType,
/// stateInfo; -1 = keep).
fn sp(id: i64, ty: i64, si: i64) -> crate::data::SpEffect {
    let r = crate::items::param("SpEffectParam", id);
    let g = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
    crate::data::SpEffect {
        state_info: if si >= 0 { si } else { r["stateInfo"].as_i64().unwrap_or(0) },
        sight_cut_limit_type: if ty >= 0 { ty } else { r["sightCutLimitType"].as_i64().unwrap_or(0) },
        sight_search_enemy_cut: g("sightSearchEnemyCut"),
        sight_search_upper_angle_cut: g("sightSearchUpperAngleCut"),
        sight_search_bottom_angle_cut: g("sightSearchBottomAngleCut"),
        sight_search_left_angle_cut: g("sightSearchLeftAngleCut"),
        sight_search_right_angle_cut: g("sightSearchRightAngleCut"),
        around_sight_point_add_rate: r["aroundSightPointAddRate"].as_f64().unwrap_or(1.0) as f32,
        ..Default::default()
    }
}

/// FUN_140616f50 (run alone in tools/emu-cases `aistealth_bits` on the user's sekiro.exe): the
/// observer every 2.5 deg round the target at 0.5 / 4 / 15 m and three heights, three facings
/// (one tilted), perceive off / on; `geometry_bits` must give the same bits. Skipped when
/// extracted/emu/aistealth.json is missing.
#[test]
fn geometry_bits_match_the_exe() {
    let Some(v) = cases() else { return };
    let list = v["bits"]["cases"].as_array().unwrap();
    let bad: Vec<_> = list
        .iter()
        .filter(|c| {
            let got = geometry_bits(v3(&c["observer"]), v3(&c["target"]), v3(&c["facing"]), c["perceive"] == 1);
            got as u64 != c["bits"].as_u64().unwrap()
        })
        .collect();
    assert!(list.len() > 7000 && bad.is_empty(), "{} of {} differ: {:?}", bad.len(), list.len(), bad.iter().take(5).collect::<Vec<_>>());
}

/// Wolf wearing none, crouch 109200, wall hug 109210 (sightCutLimitType 2), hang 109220 (1),
/// 109210 made type 1, crouch + wall hug, and crouch made stateInfo 7: the exe's factor functions
/// (FUN_140bfe8a0 / FUN_140bfea80 / bfe6e0 / bfe7c0 / bfe9a0 / FUN_140bfc050) for every bit pair,
/// and the whole target side (FUN_140bd3e40) with the observer (11500001) round Wolf at 6 m,
/// perceive off / on, facing him: the context's cut and angle factors and the normal cone.
#[test]
fn sight_factors_match_the_exe() {
    let Some(v) = cases() else { return };
    let think = crate::data::param_table("NpcThinkParam");
    let mut n = 0;
    for set in v["factors"].as_array().unwrap() {
        let name = set["set"].as_str().unwrap();
        let effects: Vec<crate::data::SpEffect> =
            set["effects"].as_array().unwrap().iter().map(|e| sp(e[0].as_i64().unwrap(), e[1].as_i64().unwrap(), e[2].as_i64().unwrap())).collect();
        let refs: Vec<&crate::data::SpEffect> = effects.iter().collect();
        let mut wolf = Seen::new(Vec3::ZERO, 0.4, &refs);
        for d in set["direct"].as_array().unwrap() {
            // gap: the observer flag (ctx +0x10) is taken as off; only its cases are compared.
            if d["flag"] == 1 {
                continue;
            }
            let bits = (d["bit1"].as_u64().unwrap() | d["bit2"].as_u64().unwrap() << 1) as u8;
            let s = wolf.with_bits(bits);
            let ang: Vec<f32> = d["ang"].as_array().unwrap().iter().map(f).collect();
            assert!((s.cut - f(&d["cut"])).abs() < 1e-6, "{name} {d}: cut {}", s.cut);
            assert!((s.around_rate - f(&d["around_rate"])).abs() < 1e-6, "{name} {d}: rate {}", s.around_rate);
            for k in 0..4 {
                assert!((s.ang[k] - ang[k]).abs() < 1e-6, "{name} {d}: ang {:?}", s.ang);
            }
            n += 1;
        }
        // The whole context, mirrored into Sv1's frame (x negated).
        let m = |p: Vec3| Vec3::new(-p.x, p.y, p.z);
        wolf.pos = m(v3(&set["feet"]));
        wolf.facing = m(v3(&set["facing"]));
        let row = think.get(&set["think"].as_i64().unwrap()).unwrap();
        for c in set["placed"].as_array().unwrap() {
            let eye = m(v3(&c["eye"]));
            let perceive = c["perceive"] == 1;
            let s = wolf.from_eye(eye, perceive);
            let ang: Vec<f32> = c["ang"].as_array().unwrap().iter().map(f).collect();
            assert!((s.cut - f(&c["cut"])).abs() < 1e-6, "{name} {c}: cut {}", s.cut);
            for k in 0..4 {
                assert!((s.ang[k] - ang[k]).abs() < 1e-6, "{name} {c}: ang {:?}", s.ang);
            }
            assert_eq!(c["light_index"], 0, "{name} {c}");
            let cone = Cone::from(row, if perceive { "perceive" } else { "normal" });
            let fwd = (wolf.pos - eye).with_y(0.0).normalize();
            assert_eq!(in_cone_shape(&cone, eye, fwd, &s) as u64 * 2, c["normal"].as_u64().unwrap(), "{name} {c}");
            n += 1;
        }
    }
    assert!(n > 1000, "{n}");
}

/// FUN_140617150 for light index 0 / 1 / 2 with each stateInfo 340 SpEffect put on the observer
/// (11500001, whose own resident one reads Good / Dark / PitchDark -> 0 / 0 / 1) or on Wolf:
/// the observer's remaps them (`Observer`, `Seen::observed_by`), Wolf's do nothing.
#[test]
fn light_remap_matches_the_exe() {
    let Some(v) = cases() else { return };
    let list = v["light"].as_array().unwrap();
    let own: Vec<u64> = list.iter().find(|c| c["sp"] == 0 && c["on"] == "observer").unwrap()["got"].as_array().unwrap().iter().map(|g| g.as_u64().unwrap()).collect();
    for c in list {
        let got: Vec<u64> = c["got"].as_array().unwrap().iter().map(|g| g.as_u64().unwrap()).collect();
        let id = c["sp"].as_i64().unwrap();
        if c["on"] == "wolf" {
            assert_eq!(got, own, "{c}");
            continue;
        }
        if id == 0 {
            continue;
        }
        let o = Observer::from_rows([crate::items::param("SpEffectParam", id)]);
        for (i, g) in got.iter().enumerate() {
            let mut wolf = Seen::new(Vec3::ZERO, 0.4, &[]);
            wolf.light_index = i as u8;
            assert_eq!(wolf.observed_by(&o).light, LIGHT_TABLE[*g as usize], "{c} index {i}");
        }
    }
}

/// FUN_14061c940 with Wolf 1 m from the sight origin (nose_dist 3) and every pair of stateInfo
/// marks (none, 332-335) on Wolf and the observer: smelt unless 333 meets 332 or 335 meets 334.
#[test]
fn nose_marks_match_the_exe() {
    let Some(v) = cases() else { return };
    let think = json!({"nose_dist": 3.0});
    let list = v["nose"].as_array().unwrap();
    for c in list {
        let (w, o) = (c["wolf"].as_i64().unwrap(), c["observer"].as_i64().unwrap());
        let mark = crate::data::SpEffect { state_info: w, around_sight_point_add_rate: 1.0, ..Default::default() };
        let worn: Vec<&crate::data::SpEffect> = if w != 0 { vec![&mark] } else { vec![] };
        let wolf = Seen::new(Vec3::X, 0.4, &worn).observed_by(&Observer::from_rows([&json!({"stateInfo": o})]));
        assert_eq!(in_nose(&think, Vec3::ZERO, &wolf), c["hit"] == 1, "{c}");
    }
    assert_eq!(list.len(), 25);
}
