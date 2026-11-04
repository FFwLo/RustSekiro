//! The free camera's swing while Wolf runs with the move stick held sideways, against the exe
//! (E32, docs/kb/camera.md "Free camera swing after a sideways run"): tools/emu-cases `camswing`
//! runs the game's own loop on the Naomori-arena snapshot, Wolf's whole chr frame FUN_140a4b570
//! with his pad written (so the pad manipulator turns the stick into a move from the camera and
//! his locomotion moves him) and then the follow camera FUN_140734790 -> FUN_14073c260, for up to
//! 480 frames. RSekiro's fulcrum chain (`drag_fulcrum`, the yaw read back from it, `place_fulcrum`)
//! is checked frame by frame from the exe's state, and carried on its own over the whole run on
//! the exe's focus: the camera has to swing as far as the exe's, with no factor on the swing.
//! Skipped when the JSON is missing.

use super::*;

#[test]
pub(crate) fn free_camera_swing_on_a_sideways_run_matches_the_exe() {
    use crate::camera::*;
    use std::f32::consts::{PI, TAU};
    let names = ["run_right", "run_left", "sprint_right", "walk_right", "run_fwd_right", "run_right_30"];
    let runs: Vec<(String, serde_json::Value)> = names
        .iter()
        .filter_map(|n| {
            let text = std::fs::read_to_string(crate::paths::emu().join(format!("camswing_{n}.json"))).ok()?;
            Some((n.to_string(), serde_json::from_str(&text).unwrap()))
        })
        .collect();
    if runs.is_empty() {
        return;
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
    let wrap = |x: f32| (x + PI).rem_euclid(TAU) - PI;
    let mut report = Vec::new();
    let mut checked = 0;
    for (name, v) in &runs {
        let dt = v["dt"].as_f64().unwrap() as f32;
        let frames = v["frames"].as_array().unwrap();
        let floats = |fr: &serde_json::Value, k: &str| fr[k].as_array().unwrap().iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>();
        let f = |x: &[f32], o: usize| x[o / 4];
        let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
        // RSekiro's own fulcrum and yaw, carried from the exe's first frame.
        let first = floats(&frames[0], "before");
        let mut own_fulcrum = v3(&first, 0xe0);
        let (mut worst_step, mut worst_run) = (0.0f32, 0.0f32);
        let (mut exe_turned, mut own_turned) = (0.0f32, 0.0f32);
        let mut last = (f(&first, 0x174), f(&first, 0x174));
        let mut checked_here = 0;
        for (i, fr) in frames.iter().enumerate() {
            let (b, a) = (floats(fr, "before"), floats(fr, "after"));
            let focus = v3(&a, 0xc0);
            // Up to the first wall: the diagonal run reaches the arena's wall, where the orbit
            // cast cuts +0xf0 short of the camera distance +0x214 (camwall covers the walls).
            if (v3(&a, 0xf0) - focus).length() < f(&a, 0x214) - 1e-3 || (v3(&a, 0x110) - v3(&a, 0x100)).length() > 1e-4 {
                println!("{name}: a wall from frame {i}");
                break;
            }
            // The right stick is let go, so nothing turns the camera but the fulcrum.
            assert_eq!(f(&a, 0x194), 0.0, "{name} frame {i}: +0x194");
            let moved = focus - v3(&b, 0xc0);
            // This frame from the exe's state: the yaw read back from the dragged fulcrum.
            let arm = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp) - focus;
            let yaw = f32::atan2(arm.x, arm.z);
            worst_step = worst_step.max(wrap(yaw - (f(&a, 0x174) - PI)).abs());
            // The fulcrum put back on the arm from the exe's angles.
            let rotation = Quat::from_euler(EulerRot::YXZ, f(&a, 0x174) - PI, -f(&a, 0x170), 0.0);
            let right = v3(&b, 0x10);
            let sideways = (moved.x * right.x + moved.z * right.z).abs();
            let cam_dist = (v3(&b, 0x40) - focus).length();
            let placed = place_fulcrum(focus, rotation * Vec3::Z, cam_dist, sideways, dt, fp);
            worst_step = worst_step.max((placed - v3(&a, 0xe0)).length());
            // The same chain on RSekiro's own fulcrum over the whole run.
            let own_arm = drag_fulcrum(own_fulcrum, moved, 0.0, fp) - focus;
            let own_yaw = f32::atan2(own_arm.x, own_arm.z);
            let own_rot = Quat::from_euler(EulerRot::YXZ, own_yaw, -f(&a, 0x170), 0.0);
            own_fulcrum = place_fulcrum(focus, own_rot * Vec3::Z, cam_dist, sideways, dt, fp);
            worst_run = worst_run.max(wrap(own_yaw - (f(&a, 0x174) - PI)).abs());
            exe_turned += wrap(f(&a, 0x174) - last.0);
            own_turned += wrap(own_yaw + PI - last.1);
            last = (f(&a, 0x174), own_yaw + PI);
            checked += 1;
            checked_here += 1;
        }
        let secs = checked_here as f32 * dt;
        println!("{name}: exe turned {:.1} deg/s, RSekiro {:.1} deg/s over {secs:.1} s; worst step {worst_step:.2e}, worst carried yaw {worst_run:.2e} rad", exe_turned.to_degrees() / secs, own_turned.to_degrees() / secs);
        if worst_step > 1e-4 {
            report.push(format!("{name}: a frame from the exe's state off by {worst_step:.2e}"));
        }
        if worst_run > 2e-3 || (own_turned - exe_turned).abs() > 0.01 * exe_turned.abs().max(1.0) {
            report.push(format!("{name}: carried over the run the yaw is off by up to {worst_run:.2e} rad, turned {own_turned:.3} vs the exe's {exe_turned:.3} rad"));
        }
    }
    assert!(checked >= 300, "{checked} frames");
    assert!(report.is_empty(), "the free camera's swing differs from the exe:\n{}", report.join("\n"));
}
