//! The lock-on camera and the pad's mapping onto Wolf's manipulator, checked against the user's
//! own sekiro.exe run in tools/emu-cases (`lockcam`, `lockcam_probe`; docs/kb/emulator-pilot.md).

use super::*;

/// The game's follow camera locked on (FUN_1409c5f20 hands it the lock target entry, then
/// FUN_140734790 -> FUN_14073c260 with the lock flag +0x2a0), run on a memory snapshot for 16
/// scenarios: targets ahead, to the side, behind, above, below, close, circling, passing over
/// Wolf's head, running off, the snapshot's own target 29 m off, Wolf strafing, 30 fps, the
/// stick held (it does nothing while locked), the lock let go, and a lock point that jumps on
/// its target (the 0.5 s average). Frame by frame from the exe's state before each frame:
/// the lock point (`LockPointAverage`), the angles (`lock_chase`), the focus follow, the
/// fulcrum, the orbit point, the position chase at the lock rates and the safe margin; after the
/// lock is let go, the free camera's yaw read back from the dragged fulcrum. Skipped when
/// extracted/emu/lockcam_frames.json is missing.
#[test]
pub(crate) fn lock_camera_matches_the_exe() {
    use crate::camera::*;
    use std::f32::consts::PI;
    let path = crate::paths::emu().join("lockcam_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut app = app();
    let combat = app.world_mut().remove_resource::<crate::data::Combat>().unwrap();
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
    let lock = LockChaseParams {
        tgt_pos_rate: p("lockTgtPosRate"),
        rate_x: p("lockRotChaseRateX"),
        rate_y: p("lockRotChaseRateY"),
        play_x: p("lockRotChasePlayAngX"),
        play_y: p("lockRotChasePlayAngY"),
        at_lock: (p("rotRangeXAtLockMin"), p("rotRangeXAtLockMax")),
        lerp_height: (p("rotRangeLerpBeginHeight"), p("rotRangeLerpEndHeight")),
    };
    let at_lock = PosChase {
        xz: p("targetChaseRateAtLockXZ"),
        y: p("targetChaseRateAtLockY"),
        ang_min: p("targetChaseRateAngAtLockMin"),
        ang_max: p("targetChaseRateAngAtLockMax"),
        dist_xz: p("targetChaseDistRateXZ"),
        dist_y: p("targetChaseDistRateY"),
    };
    let free = PosChase {
        xz: p("targetChaseRateXZForNormal"),
        y: p("targetChaseRateY"),
        ang_min: p("targetChaseRateAngMin"),
        ang_max: p("targetChaseRateAngMax"),
        ..at_lock
    };
    let mut worst: std::collections::BTreeMap<String, (f32, String, usize)> = Default::default();
    let mut check = |what: &str, at: String, err: f32, tol: f32| {
        let e = worst.entry(what.to_string()).or_insert((0.0, String::new(), 0));
        let err = if err.is_nan() { f32::INFINITY } else { err };
        if err > tol {
            e.2 += 1;
            if err > e.0 {
                (e.0, e.1) = (err, at);
            }
        }
    };
    let (mut locked, mut released, mut walled) = (0, 0, Vec::new());
    let scenarios = v["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 16);
    for s in scenarios {
        let dt = s["dt"].as_f64().unwrap() as f32;
        let mut avg = LockPointAverage::default();
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{} frame {i}", s["name"].as_str().unwrap());
            let floats = |k: &str| fr[k].as_array().map(|a| a.iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>());
            let (b, a) = (floats("before").unwrap(), floats("after").unwrap());
            let f = |x: &[f32], o: usize| x[o / 4];
            let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
            // The cast hit the ground or a wall (+0x110 off the chased +0x100): the exe's wall
            // handling, not ported here.
            if (v3(&a, 0x110) - v3(&a, 0x100)).length() > 1e-4 {
                walled.push(at);
                break;
            }
            let focus = v3(&a, 0xc0);
            let moved = focus - v3(&b, 0xc0);
            let mut fc = FocusChase { vel: v3(&b, 0x1b0), last_target: v3(&b, 0x1c0), out: v3(&b, 0x1d0), clock: f(&b, 0x410) };
            let wolf = Vec3::from_slice(&floats("wolf").unwrap());
            check("focus follow (+0x90)", at.clone(), (fc.update(wolf, Vec3::new(f(&b, 0x220), f(&b, 0x224), f(&b, 0x220)), dt) - v3(&a, 0x90)).length(), 1e-4);
            let fulcrum = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp);
            let limits = (f(&a, 0x230), f(&a, 0x234));
            let Some(target) = floats("target") else {
                // Let go: the free camera's yaw from the dragged fulcrum again (+0x194 falls
                // from 1 over transChaseByControlEndReleaseTime).
                avg.clear();
                released += 1;
                let arm = fulcrum - focus;
                check("released: yaw (+0x174)", at.clone(), wrap_pi(f32::atan2(arm.x, arm.z) - (f(&a, 0x174) - PI)).abs(), 1e-4);
                let want = (f(&b, 0x194) - dt / p("transChaseByControlEndReleaseTime")).max(0.0);
                check("released: +0x194", at.clone(), (want - f(&a, 0x194)).abs(), 1e-5);
                let pos = chase_position(v3(&b, 0x100), v3(&a, 0xf0), focus, PosChase { xz: f(&a, 0x228), ..free }, Some(limits), f(&a, 0x150), dt);
                check("released: position chase (+0x100)", at.clone(), (pos - v3(&a, 0x100)).length(), 1e-4);
                continue;
            };
            locked += 1;
            check("locked: +0x194 = 1", at.clone(), (f(&a, 0x194) - 1.0).abs(), 0.0);
            // The lock point: the target point averaged over 0.5 s in the target's frame (its
            // rotation stays the snapshot's in every scenario, so the identity stands for it).
            let frame_t = Vec3::from_slice(&floats("target_frame").unwrap());
            let lp = avg.update(Entity::PLACEHOLDER, (frame_t, Quat::IDENTITY), Vec3::from_slice(&target), dt);
            check("lock point (+0x280)", at.clone(), (lp - v3(&a, 0x280)).length(), 1e-4);
            let (pitch, yaw) = lock_chase(focus, fulcrum, v3(&a, 0x280), f(&a, 0x50) * f(&a, 0x278) * 0.5, limits, lock, dt);
            check("lock pitch (+0x170)", at.clone(), (pitch - f(&a, 0x170)).abs(), 1e-5);
            check("lock yaw (+0x174)", at.clone(), wrap_pi(yaw - (f(&a, 0x174) - PI)).abs(), 1e-5);
            // The fulcrum put back on the arm, the orbit point, the chase at the lock rates
            // (angle factor 1: AngAtLock 0 / 0) and the safe margin.
            let rotation = Quat::from_euler(EulerRot::YXZ, f(&a, 0x174) - PI, -f(&a, 0x170), 0.0);
            let right = v3(&b, 0x10);
            let sideways = (moved.x * right.x + moved.z * right.z).abs();
            let cam_dist = (v3(&b, 0x40) - focus).length();
            check("fulcrum (+0xe0)", at.clone(), (place_fulcrum(focus, rotation * Vec3::Z, cam_dist, sideways, dt, fp) - v3(&a, 0xe0)).length(), 1e-4);
            check("orbit point (+0xf0)", at.clone(), (focus + rotation * Vec3::Z * f(&a, 0x214) - v3(&a, 0xf0)).length(), 1e-4);
            // The exe applies these rates once a frame with no time factor: at 30 fps 0.4 a
            // frame (`per_frame`; Sv1 closed 0.64 before).
            let pos = chase_position(v3(&b, 0x100), v3(&a, 0xf0), focus, at_lock, Some(limits), f(&a, 0x150), dt);
            check("locked: position chase (+0x100)", at.clone(), (pos - v3(&a, 0x100)).length(), 1e-4);
            let d = (v3(&a, 0x110) - focus).length();
            let cam = focus + (v3(&a, 0x110) - focus) * safe_margin(d, p("camSafeMarginRate"), p("camSafeMarginMax")) / d;
            check("camera (+0x40, safe margin)", at.clone(), (cam - v3(&a, 0x40)).length(), 1e-4);
        }
    }
    assert!(locked >= 1400 && released >= 60 && walled.is_empty(), "{locked} locked / {released} released frames checked; walls at {walled:?}");
    let report: Vec<String> = worst.iter().filter(|(_, (_, _, n))| *n > 0).map(|(k, (e, at, n))| format!("{k}: {n} frames off, at most {e:.6} at {at}")).collect();
    assert!(report.is_empty(), "the lock camera differs from the exe:\n{}", report.join("\n"));
}

/// Wolf's PadManipulator (FUN_1407f0f50) run in tools/emu-cases on made-up pad frames: the
/// frame's camera slots +0x28 / +0x2c land in M+0x84 (yaw) / M+0x80 (pitch), the mouse +0x18 /
/// +0x1c in M+0xd4 / M+0xd0, and the move (+0x0 up, +0x4 right) in M+0xc0 as the camera's view /
/// its right (the game's world is left-handed: right = (f.z, 0, -f.x)), the same after the
/// camera turned. The right stick's vector action reaches the slots as yaw = +x, pitch = -y with
/// the snapshot's options (bytes 10 / 11 set; cleared, both flip). Sv1 in its mirrored world:
/// `stick_world` puts stick up along the view and right on the camera's right, `chases_pitch`'s
/// -stick.y is the exe's dot(+0xc0, -view), and the right stick turns the view right (yaw -= x)
/// and looks up on stick up (pitch -= y). Skipped when extracted/emu/lockcam_pad.json is missing.
#[test]
pub(crate) fn pad_mapping_matches_the_exe() {
    let path = crate::paths::emu().join("lockcam_pad.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let v4 = |x: &serde_json::Value| Vec3::new(x[0].as_f64().unwrap() as f32, x[1].as_f64().unwrap() as f32, x[2].as_f64().unwrap() as f32);
    let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-5;
    let case = |cases: &serde_json::Value, slot: u64, value: f64| cases.as_array().unwrap().iter().find(|c| c["slot"].as_u64() == Some(slot) && c["value"].as_f64() == Some(value)).cloned().unwrap();
    let cases = &v["cases"];
    // Right stick and mouse: (pitch, yaw) pairs.
    let (yaw, pitch) = (case(cases, 0x28, 1.0), case(cases, 0x2c, 1.0));
    assert_eq!((yaw["m80"][0].as_f64(), yaw["m80"][1].as_f64()), (Some(0.0), Some(1.0)), "frame +0x28 -> M+0x84");
    assert_eq!((pitch["m80"][0].as_f64(), pitch["m80"][1].as_f64()), (Some(1.0), Some(0.0)), "frame +0x2c -> M+0x80");
    assert_eq!(case(cases, 0x18, 1.0)["md0"][1].as_f64(), Some(1.0), "frame +0x18 -> M+0xd4");
    assert_eq!(case(cases, 0x1c, 1.0)["md0"][0].as_f64(), Some(1.0), "frame +0x1c -> M+0xd0");
    // The move, turned into the camera: up along the view, right on its (left-handed) right.
    let mirror = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
    for (view, cases) in [(v4(&v["camera_view"]), cases), (v4(&v["turned"]["camera_view"]), &v["turned"]["cases"])] {
        let f = view.with_y(0.0).normalize();
        let up = v4(&case(cases, 0x0, 1.0)["mc0"]);
        let right = v4(&case(cases, 0x4, 1.0)["mc0"]);
        assert!(close(up, f), "move up {up} vs the view {f}");
        assert!(close(right, Vec3::new(f.z, 0.0, -f.x)), "move right {right} vs the view {f}");
        // Sv1 (mirrored x): the orbit yaw whose view is the mirrored f.
        let fb = mirror(f);
        let cam_yaw = f32::atan2(-fb.x, -fb.z);
        assert!(close(crate::player::stick_world(Vec2::Y, cam_yaw), mirror(up)), "stick_world up");
        assert!(close(crate::player::stick_world(Vec2::X, cam_yaw), mirror(right)), "stick_world right");
        // chases_pitch's -stick.y = dot(+0xc0, -view).
        assert!((up.dot(-f) - -1.0).abs() < 1e-5 && (right.dot(-f)).abs() < 1e-5);
    }
    for (i, c) in v["cases"].as_array().unwrap().iter().enumerate().filter(|(_, c)| c["slot"].as_u64() == Some(0)) {
        let down = c["value"].as_f64() == Some(-1.0);
        let mc0 = v4(&c["mc0"]);
        let f = v4(&v["camera_view"]).with_y(0.0).normalize();
        assert!((mc0.dot(-f) - if down { 1.0 } else { -1.0 }).abs() < 1e-5, "case {i}");
    }
    // The right stick's action (0.6, 0.8): yaw +x, pitch -y with options 10 / 11 set (the
    // snapshot's), both flipped when cleared.
    let opts = v["options"].as_array().unwrap();
    assert_eq!((opts[10].as_u64(), opts[11].as_u64()), (Some(1), Some(1)));
    let stick = v["stick"].as_array().unwrap();
    let got = |i: usize, k: &str| stick[i][k].as_f64().unwrap() as f32;
    assert!((got(0, "yaw") - 0.6).abs() < 1e-6 && (got(0, "pitch") + 0.8).abs() < 1e-6);
    assert!((got(1, "yaw") + 0.6).abs() < 1e-6 && (got(1, "pitch") - 0.8).abs() < 1e-6);
}
