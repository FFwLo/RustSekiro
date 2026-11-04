//! The follow camera's leftovers against the exe (E18, docs/kb/camera.md "Camera leftovers"):
//! tools/emu-cases `camleft` runs FUN_140734790 -> FUN_14073c260 on the Naomori-arena snapshot
//! with every cast hooked: Wolf walked into the wall about 9 m along +X with the arm swung into
//! it (`pin_*`: the fulcrum's 1 cm floor, the final cast at its shortest; those frames run
//! through `follow_camera_at_walls_matches_the_exe`), TAE 152 AttachCameraToNode as its handler
//! writes the camera singleton (`attach_*`, Wolf's dummy 143), and TAE 153 with SlowEnd on and
//! off (`slow_end_*`, `plain_slow_end_*`). Each frame starts from the exe's state before it.
//! Skipped when the JSON is missing.

use super::*;

#[test]
pub(crate) fn camera_leftovers_match_the_exe() {
    use crate::camera::*;
    use std::f32::consts::{PI, TAU};
    let path = crate::paths::emu().join("camleft_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let row = combat.param("CameraParam", 0);
    let p = |k: &str| row[k].as_f64().unwrap_or_else(|| panic!("CameraParam {k}")) as f32;
    let fp = FulcrumParams {
        rate_xz: p("targetTransChaseRateXZ"),
        rate_y: p("targetTransChaseRateY"),
        lr_max_dist: p("fulcrumDistRateByLrMoveMaxDist"),
        lr_min: p("fulcrumDistRateByLrMoveMin"),
        lr_max: p("fulcrumDistRateByLrMoveMax"),
        dist_min: p("fulcrumDistMin"),
    };
    let chase = PosChase {
        xz: p("targetChaseRateXZForNormal"),
        y: p("targetChaseRateY"),
        ang_min: p("targetChaseRateAngMin"),
        ang_max: p("targetChaseRateAngMax"),
        dist_xz: p("targetChaseDistRateXZ"),
        dist_y: p("targetChaseDistRateY"),
    };
    let wrap = |x: f32| (x + PI).rem_euclid(TAU) - PI;
    let mut worst: std::collections::BTreeMap<String, (f32, String)> = Default::default();
    let mut check = |what: &str, at: &str, err: f32, tol: f32| {
        let e = worst.entry(what.to_string()).or_insert((0.0, String::new()));
        let err = if err.is_nan() { f32::INFINITY } else { err };
        if err > tol && err > e.0 {
            *e = (err, at.to_string());
        }
    };
    let floats = |x: &serde_json::Value| x.as_array().unwrap().iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>();
    let f = |x: &[f32], o: usize| x[o / 4];
    let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
    let by_name = |n: &str| v["scenarios"].as_array().unwrap().iter().find(|s| s["name"] == n).unwrap_or_else(|| panic!("no scenario {n}"));
    let (mut attached, mut after_override, mut pinned) = (0, 0, 0);
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        if name.starts_with("pin_") {
            // The fulcrum cast's 1 cm floor: frames where the cast stops under 1 cm.
            for fr in s["frames"].as_array().unwrap() {
                let a = floats(&fr["after"]);
                pinned += ((v3(&a, 0xe0) - v3(&a, 0xc0)).length() < 0.0101) as usize;
            }
            continue;
        }
        if !name.starts_with("attach_") {
            continue;
        }
        // RSekiro's singleton +0x68 / +0x6c (CamState::attach_after) carried over the run.
        let mut after = (0.0f32, 0.0f32);
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{name} frame {i}");
            let (b, a, si, so) = (floats(&fr["before"]), floats(&fr["after"]), floats(&fr["single_in"]), floats(&fr["single_out"]));
            let event = fr["single_in"][0x60 / 4].as_f64().is_some() && i32::from_le_bytes(f(&si, 0x60).to_le_bytes()) >= 0;
            let (blend, unk1, unk2) = (f(&si, 0x64), f(&si, 0x68), f(&si, 0x6c));
            if event {
                after = (unk1, unk2);
            }
            check("+0x6c in (the distance rate the camera reads)", &at, (after.1 - unk2).abs(), 1e-6);
            let focus = v3(&a, 0xc0);
            let node = Vec3::from_slice(&floats(&fr["dmy143"])[..3]);
            if event {
                attached += 1;
                // The fulcrum placed at its distance (no wall here), then put on the node's line.
                let moved = focus - v3(&b, 0xc0);
                let right = v3(&b, 0x10);
                let sideways = (moved.x * right.x + moved.z * right.z).abs();
                let placed = place_fulcrum(focus, Vec3::Z, (v3(&b, 0x40) - focus).length(), sideways, dt, fp);
                let (yaw, pitch, fulcrum) = attach_to_node(focus, placed, node);
                check("152 yaw (+0x174) toward the node", &at, wrap(yaw - (f(&a, 0x174) - PI)).abs(), 1e-4);
                check("152 pitch (+0x170) toward the node", &at, (pitch - f(&a, 0x170)).abs(), 1e-4);
                check("152 fulcrum (+0xe0) on the node's line", &at, (fulcrum - v3(&a, 0xe0)).length(), 1e-4);
                check("152 orbit point (+0xf0) = the node", &at, (node - v3(&a, 0xf0)).length(), 1e-4);
                // The stick held does not turn the camera nor snap the chase (+0x150).
                check("152: no camera control snap (+0x150)", &at, f(&a, 0x150).abs(), 0.0);
            }
            // The position chase: at BlendStrength while the event runs, else the normal one;
            // the distance at +0x6c while set, else the hit-wall rates.
            let dist = if after.1 > 0.0 { (after.1, after.1) } else { (f(&a, 0x1e8), f(&a, 0x1ec)) };
            after_override += (!event && after.1 > 0.0) as usize;
            let last = (v3(&b, 0x110) - focus).length();
            let pos = if event && blend > 0.0 {
                chase_position_attached(v3(&b, 0x100), last, v3(&a, 0xf0), focus, blend, dist, f(&a, 0x150), dt)
            } else {
                let r = PosChase { xz: f(&a, 0x228), dist_xz: dist.0, dist_y: dist.1, ..chase };
                chase_position_from(v3(&b, 0x100), last, v3(&a, 0xf0), focus, r, Some((f(&a, 0x230), f(&a, 0x234))), f(&a, 0x150), dt)
            };
            check("position chase (+0x100) under / after 152", &at, (pos - v3(&a, 0x100)).length(), 1e-4);
            // FUN_140838b50's countdown after the frame.
            after.0 -= dt;
            if after.0 <= 0.0 {
                after = (0.0, 0.0);
            }
            check("+0x68 / +0x6c after the clear", &at, (after.0 - f(&so, 0x68)).abs().max((after.1 - f(&so, 0x6c)).abs()), 1e-5);
        }
    }
    // SlowEnd (+0x85, FUN_140831e70): only copies the final matrix to +0x350.. (with +0x348
    // set); nothing the camera moves differs.
    for (on, off) in [("slow_end_on", "slow_end_off"), ("plain_slow_end_on", "plain_slow_end_off")] {
        for (i, (x, y)) in by_name(on)["frames"].as_array().unwrap().iter().zip(by_name(off)["frames"].as_array().unwrap()).enumerate() {
            let (x, y) = (floats(&x["after"]), floats(&y["after"]));
            for k in (0..x.len()).filter(|&k| !(0x350 / 4..0x390 / 4).contains(&k)) {
                if x[k].to_bits() != y[k].to_bits() && !(x[k].is_nan() && y[k].is_nan()) {
                    check("SlowEnd changes no camera field outside +0x350..+0x390", &format!("{on} frame {i} +{:#x}", k * 4), 1.0, 0.0);
                }
            }
        }
    }
    println!("camleft: {attached} frames under TAE 152, {after_override} after it with the distance rate override, {pinned} pinned frames at the 1 cm floor");
    assert!(attached >= 200 && after_override >= 30 && pinned >= 10, "{attached} / {after_override} / {pinned}");
    let report: Vec<String> = worst.iter().filter(|(_, (e, _))| *e > 0.0).map(|(k, (e, at))| format!("{k}: off by {e:.6} at {at}")).collect();
    assert!(report.is_empty(), "the camera leftovers differ from the exe:\n{}", report.join("\n"));
}
