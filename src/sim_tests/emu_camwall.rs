//! The follow camera at walls and the ground against the exe (topic camwall,
//! docs/kb/camera.md "Follow camera at walls and the ground"): tools/emu-cases `camwall` runs
//! FUN_140734790 -> FUN_14073c260 on the Naomori-arena snapshot while the stick pitches the
//! camera to the ground or swings it through the wall about 9 m along +X, and records every
//! sphere cast the update makes (start, vector, radius, hit, fraction). Each frame here starts
//! from the exe's state before it and feeds RSekiro's collision block the exe's own cast
//! results, so RSekiro's map collision stays its own.

use super::*;

/// Return addresses of the casts in FUN_14073c260 (1.6.0): the fulcrum, orbit point and final
/// casts (FUN_14094dfc0 while ChrExFollowCam +0x3ad is set, as in the snapshot).
const FULCRUM_CAST: &str = "0x14073f5c8";
const ORBIT_CAST: &str = "0x14073f9ca";
const FINAL_CAST: &str = "0x1407412b9";

#[test]
pub(crate) fn follow_camera_at_walls_matches_the_exe() {
    use crate::camera::*;
    use std::f32::consts::{PI, TAU};
    let path = crate::paths::emu().join("camwall_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    // E18 (emu-cases `camleft`): Wolf walked into the wall with the arm swung into it, so the
    // focus is pinned to the wall: the fulcrum cast stops at its 1 cm floor and the final cast
    // is down to ~1.5e-5 m. Same casts, same checks.
    let mut pinned = 0;
    if let Some(left) = std::fs::read_to_string(crate::paths::emu().join("camleft_frames.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        let pins: Vec<serde_json::Value> = left["scenarios"].as_array().unwrap().iter().filter(|s| s["name"].as_str().is_some_and(|n| n.starts_with("pin_"))).cloned().collect();
        pinned = pins.len();
        v["scenarios"].as_array_mut().unwrap().extend(pins);
    }
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
    let radius = p("camCastSphereRadius");
    let hit_time = p("targetChaseDistRateTime");
    let normal = Vec2::new(p("targetChaseDistRateXZ"), p("targetChaseDistRateY"));
    let on_wall = Vec2::new(p("targetChaseDistRateXZ_ForHitWall"), p("targetChaseDistRateY_ForHitWall"));
    let wrap = |x: f32| (x + PI).rem_euclid(TAU) - PI;
    let mut worst: std::collections::BTreeMap<String, (f32, String)> = Default::default();
    let mut check = |what: &str, at: &str, err: f32, tol: f32| {
        let e = worst.entry(what.to_string()).or_insert((0.0, String::new()));
        let err = if err.is_nan() { f32::INFINITY } else { err };
        if err > tol && err > e.0 {
            *e = (err, at.to_string());
        }
    };
    let (mut checked, mut cut, mut old_err) = (0, [0usize; 3], 0.0f32);
    let scenarios = v["scenarios"].as_array().unwrap();
    for s in scenarios {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{name} frame {i}");
            let floats = |x: &serde_json::Value| x.as_array().unwrap().iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>();
            // camleft's frames have no mouse.
            let mouse = if fr["mouse"].is_array() { floats(&fr["mouse"]) } else { vec![0.0; 2] };
            let (b, a, stick) = (floats(&fr["before"]), floats(&fr["after"]), floats(&fr["stick"]));
            let f = |x: &[f32], o: usize| x[o / 4];
            let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
            let casts = fr["casts"].as_array().unwrap();
            let cast = |site: &str| casts.iter().find(|c| c["site"] == site);
            let frac = |site: &str| cast(site).and_then(|c| c["frac"].as_f64()).map(|x| x as f32);
            let ends = |site: &str| cast(site).map(|c| (Vec3::from_slice(&floats(&c["start"])), Vec3::from_slice(&floats(&c["dir"])), c["radius"].as_f64().unwrap() as f32));
            if s["escape"].as_bool() == Some(true) {
                // The escape-wall option on (not the user's setting): no fulcrum or orbit cast.
                check("escape-wall option: no fulcrum / orbit cast", &at, (cast(FULCRUM_CAST).is_some() || cast(ORBIT_CAST).is_some()) as u8 as f32, 0.0);
                continue;
            }
            checked += 1;
            let focus = v3(&a, 0xc0);
            let moved = focus - v3(&b, 0xc0);
            // Yaw and pitch read back from last frame's fulcrum (+0xe0, pulled in by its cast),
            // dragged by the focus's move, then turned by the stick (as follow_camera_matches_the_exe).
            let fast = f(&a, 0x160) != 0.0;
            let speed = f(&b, 0x270) * 10.0;
            let rates = stick_rate(&combat, speed, f(&a, 0x160).abs());
            let side = if fast { stick[1].signum() } else { stick[1] };
            let turn = Vec2::new(side * rates.x * dt - mouse[1].to_radians(), stick[0] * rates.y * dt - mouse[0].to_radians());
            let arm = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp) - focus;
            let limits = (f(&a, 0x230), f(&a, 0x234));
            // Not on the frame Wolf is put by the wall (a 6 m jump: the exe's angles there come
            // from the 1/30 s focus steps and its focus pull-in, neither part of this block).
            if moved.length() < 1.0 {
                check("yaw (+0x174) from the cut fulcrum", &at, wrap(f32::atan2(arm.x, arm.z) + turn.x - (f(&a, 0x174) - PI)).abs(), 1e-4);
                let resync = f32::atan2(arm.y, arm.with_y(0.0).length());
                let pitch = if fast { direct_pitch(resync, stick[0], limits, p("directPitchAng"), p("directPitchRate"), dt) } else { resync + turn.y };
                check("pitch (+0x170) from the cut fulcrum", &at, (pitch.clamp(limits.0, limits.1) - f(&a, 0x170)).abs(), 1e-4);
            }
            // The fulcrum placed on the arm, then its cast (radius camCastSphereRadius from the
            // focus) and the cut (1 cm at least).
            let (yaw, pitch) = (f(&a, 0x174) - PI, f(&a, 0x170));
            let rotation = Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0);
            let right = v3(&b, 0x10);
            let sideways = (moved.x * right.x + moved.z * right.z).abs();
            let placed = place_fulcrum(focus, rotation * Vec3::Z, (v3(&b, 0x40) - focus).length(), sideways, dt, fp);
            let Some((st, d, rad)) = ends(FULCRUM_CAST) else { panic!("{at}: no fulcrum cast") };
            check("fulcrum cast (focus -> placed fulcrum, radius)", &at, (st - focus).length().max((st + d - placed).length()).max((rad - radius).abs()), 1e-4);
            let fulcrum = cast_cut(focus, placed - focus, frac(FULCRUM_CAST), FULCRUM_CUT_MIN);
            check("fulcrum cut (+0xe0)", &at, (fulcrum - v3(&a, 0xe0)).length(), 1e-4);
            // The orbit point: +0x214 along the fulcrum's direction, then its cast and cut.
            let raw = focus + (fulcrum - focus).normalize() * f(&a, 0x214);
            let Some((st, d, _)) = ends(ORBIT_CAST) else { panic!("{at}: no orbit cast") };
            check("orbit cast (focus -> orbit point)", &at, (st - focus).length().max((st + d - raw).length()), 1e-4);
            let raw = cast_cut(focus, raw - focus, frac(ORBIT_CAST), CAST_CUT_MIN);
            check("orbit point cut (+0xf0)", &at, (raw - v3(&a, 0xf0)).length(), 1e-4);
            // The hit-wall distance rates and clock, the position chase from the cut point and
            // the distance chase from last frame's cut camera (+0x110).
            let (dist_rate, clock) = hit_wall_rates(f(&b, 0x1e4), hit_time, normal, on_wall, dt);
            check("hit-wall distance rates (+0x1e8 / +0x1ec)", &at, (dist_rate - Vec2::new(f(&a, 0x1e8), f(&a, 0x1ec))).abs().max_element(), 1e-5);
            let r = PosChase { xz: f(&a, 0x228), dist_xz: dist_rate.x, dist_y: dist_rate.y, ..chase };
            let last = (v3(&b, 0x110) - focus).length();
            let pos = chase_position_from(v3(&b, 0x100), last, raw, focus, r, Some(limits), f(&a, 0x150), dt);
            // The exe applies the chase rates once a frame (no dt): at 30 fps 0.4 a frame
            // (`per_frame`; before, RSekiro took them per 1/60 s and closed 0.64).
            check("position chase (+0x100)", &at, (pos - v3(&a, 0x100)).length(), 1e-4);
            if (dt - 1.0 / 60.0).abs() < 1e-6 {
                // Before this port: toward the uncut orbit point, distance from +0x100, rate 0.4.
                let uncut = focus + (fulcrum - focus).normalize() * f(&a, 0x214);
                let old = chase_position(v3(&b, 0x100), uncut, focus, PosChase { xz: f(&a, 0x228), ..chase }, Some(limits), f(&a, 0x150), dt);
                old_err = old_err.max((old - v3(&a, 0x100)).length());
            }
            let pos = v3(&a, 0x100);
            // The final cast from the focus to the chased position; a hit restarts the clock.
            let Some((st, d, _)) = ends(FINAL_CAST) else { panic!("{at}: no final cast") };
            check("final cast (focus -> +0x100)", &at, (st - focus).length().max((st + d - pos).length()), 1e-4);
            let hit = frac(FINAL_CAST);
            let clock = if cast(FINAL_CAST).is_some_and(|c| c["hit"].as_u64().unwrap_or(0) != 0) { hit_time } else { clock };
            check("hit-wall clock (+0x1e4)", &at, (clock - f(&a, 0x1e4)).abs(), 1e-5);
            let post = cast_cut(focus, pos - focus, hit, CAST_CUT_MIN);
            check("camera cut (+0x110)", &at, (post - v3(&a, 0x110)).length(), 1e-4);
            let dd = (post - focus).length();
            let cam = focus + (post - focus) * safe_margin(dd, p("camSafeMarginRate"), p("camSafeMarginMax")) / dd;
            check("camera (+0x40, safe margin)", &at, (cam - v3(&a, 0x40)).length(), 1e-4);
            for (k, site) in [FULCRUM_CAST, ORBIT_CAST, FINAL_CAST].iter().enumerate() {
                cut[k] += frac(site).is_some() as usize;
            }
        }
    }
    println!("camwall: {checked} frames ({pinned} camleft pin scenarios), cuts (fulcrum, orbit, final) {cut:?}; the old chase was off by up to {old_err:.3} m");
    // Enough frames where each cast cut something.
    assert!(checked >= 1500 && cut[0] >= 1 && cut[1] >= 400 && cut[2] >= 100, "{checked} frames, cuts (fulcrum, orbit, final) {cut:?}");
    let report: Vec<String> = worst.iter().filter(|(_, (e, _))| *e > 0.0).map(|(k, (e, at))| format!("{k}: off by {e:.6} at {at}")).collect();
    assert!(report.is_empty(), "the camera at walls differs from the exe:\n{}", report.join("\n"));
}
