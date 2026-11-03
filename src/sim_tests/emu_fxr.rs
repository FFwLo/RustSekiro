//! FXR property evaluation against the user's own sekiro.exe, run in tools/emu-cases `fxr` on the
//! Naomori-arena snapshot (docs/kb/fxr.md 2026-11-03): the exe's property evaluators (table
//! [0x143f136b0]: Constant / Stepped / Linear / Bezier / Hermite, scalar to vec4, loop or not) and
//! its RandomDelta / RandomFraction modifiers (table [0x143f13da0]) on every keyframed or
//! randomised property of the deflect sparks 252001, the hit sparks 201001 / 201002, the blood
//! 220502 and the firecracker 300071, plus one property of every kind in extracted/fxr, at and
//! between the keyframes, before the first and past the last, with eight object randoms.
//! Skips when extracted/emu/fxr_props.json is missing.
//! `fxr_shapes`: the emitter shape (401-405) and direction spread (501 / 503) generators, each
//! run from a chosen state of the exe's FXR RNG (xorshift at FUN_141dc67a0 +4) so the port redraws
//! the same numbers (extracted/emu/fxr_shapes.json).

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// `fxr::eval_prop` (keyframe search, the loop's fmodf, the curves, the exe's modifier random)
/// against the exe's evaluators plus modifiers for each property, argument and object random.
#[test]
fn fxr_properties_match_the_exe() {
    let Some(v) = emu_json("fxr_props") else { return };
    let (mut rows, mut bad) = (0, Vec::new());
    for c in v["cases"].as_array().unwrap() {
        for r in c["rows"].as_array().unwrap() {
            let t = r["t"].as_f64().unwrap() as f32;
            let random = r["random"].as_u64().unwrap() as u32;
            let got = crate::fxr::eval_prop(&c["prop"], t, random);
            let want: Vec<f32> = r["out"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
            rows += 1;
            for (i, w) in want.iter().enumerate() {
                if (got[i] - w).abs() > 1e-4 * w.abs().max(1.0) {
                    bad.push(format!("{} t {t} r {random:#x} [{i}]: exe {w}, ours {}", c["path"].as_str().unwrap(), got[i]));
                    break;
                }
            }
        }
    }
    assert!(rows > 1000, "only {rows} rows");
    assert!(bad.is_empty(), "{} of {rows} differ:\n{}", bad.len(), bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}

/// The modifier random alone, bit for bit: RandomDelta rows with max 1 on a 0 base give
/// `game_random(seed, random)` itself, and the exe's draws over the eight randoms stay in [-1, 1).
#[test]
fn fxr_modifier_random_is_the_exes() {
    let Some(v) = emu_json("fxr_props") else { return };
    let mut n = 0;
    for c in v["cases"].as_array().unwrap() {
        let Some(mods) = c["prop"]["modifiers"].as_array() else { continue };
        if mods.len() != 1 || mods[0]["type"] != "RandomDelta" {
            continue;
        }
        let m = &mods[0];
        let seed0 = m["seed"].as_i64().or_else(|| m["seed"][0].as_i64()).unwrap() as u32;
        let max0 = m["max"].as_f64().or_else(|| m["max"][0].as_f64()).unwrap() as f32;
        if max0 == 0.0 {
            continue;
        }
        for r in c["rows"].as_array().unwrap() {
            let base = r["base"][0].as_f64().unwrap() as f32;
            let out = r["out"][0].as_f64().unwrap() as f32;
            let q = (out - base) / max0;
            assert!((-1.0001..1.0001).contains(&q), "{} draw {q}", c["path"]);
            let ours = crate::fxr::game_random(seed0, r["random"].as_u64().unwrap() as u32);
            assert!((ours - q).abs() < 1e-3, "{} seed {seed0:#x} r {}: exe {q}, ours {ours}", c["path"], r["random"]);
            n += 1;
        }
    }
    assert!(n > 100, "only {n} draws");
}

/// `fxr::shape_point` / `fxr::spread_frame` against the exe's generators, draw for draw: the point
/// and the frame (shapes 403-405, spreads 501-503, 502 with either angle the larger), and the
/// number of RNG draws each took.
#[test]
fn fxr_shapes_and_spreads_match_the_exe() {
    let Some(v) = emu_json("fxr_shapes") else { return };
    let (mut rows, mut bad) = (0, Vec::new());
    for c in v["cases"].as_array().unwrap() {
        let ty = c["type"].as_i64().unwrap();
        let fields: Vec<i64> = c["fields"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        let props: Vec<f32> = c["props"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
        for r in c["rows"].as_array().unwrap() {
            let u32s = |k: &str| -> [u32; 4] { std::array::from_fn(|i| r[k][i].as_u64().unwrap() as u32) };
            let mut d = crate::fxr::ExeRng(u32s("state"));
            let f = |k: &str, i: usize| r[k][i].as_f64().unwrap() as f32;
            let (point, frame) = if ty >= 500 {
                (None, Some(crate::fxr::spread_frame(ty, &props, &mut d)))
            } else {
                let (p, m) = crate::fxr::shape_point(ty, &fields, &props, &mut d);
                (Some(p), if ty >= 403 { Some(m.unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])) } else { None })
            };
            rows += 1;
            let mut err = 0.0f32;
            if let Some(p) = point {
                for i in 0..3 {
                    err = err.max((p[i] - f("point", i)).abs());
                }
            }
            if let Some(m) = frame {
                for i in 0..3 {
                    for j in 0..3 {
                        err = err.max((m[i][j] - f("frame", 4 * i + j)).abs());
                    }
                }
            }
            if err > 1e-4 || d.0 != u32s("after") {
                bad.push(format!("{ty} {fields:?} {props:?}: off by {err}, draws {}", if d.0 == u32s("after") { "same" } else { "differ" }));
            }
        }
    }
    assert!(rows > 1000, "only {rows} rows");
    assert!(bad.is_empty(), "{} of {rows} differ:\n{}", bad.len(), bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}
