//! Sim test: Wolf's TAE camera events (151 CameraLookAtTarget, 153 CameraModule3, 155
//! SetLockParamID, 156 SetCameraAngle) against the exe's camera frame, emulated on a memory
//! snapshot by tools/emu-cases (`taecam`).

use super::*;

/// The CameraParam fields in row order with their byte offsets (the paramdef, as the exe's
/// blended row at the CameraSetParam manager +8 lays them out): (offset, type, name).
fn camera_param_fields() -> Vec<(usize, String, String)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/sekiro-extract/defs/paramdef/CameraParam.xml");
    let xml = std::fs::read_to_string(path).expect("CameraParam paramdef");
    let mut off = 0;
    let mut out = Vec::new();
    for def in xml.split("Def=\"").skip(1) {
        let def = def.split('"').next().unwrap();
        let def = def.split('=').next().unwrap().trim();
        let (ty, name) = def.split_once(' ').unwrap();
        let (name, n) = match name.split_once('[') {
            Some((n, rest)) => (n, rest.trim_end_matches(']').parse::<usize>().unwrap()),
            None => (name, 1),
        };
        if ty == "dummy8" {
            off += n;
        } else {
            out.push((off, ty.to_string(), name.to_string()));
            off += 4;
        }
    }
    assert_eq!(off, 0x1e0, "CameraParam row size");
    out
}

/// The exe's camera frame with the TAE camera events (FUN_140734790 -> FUN_14073c260, 17
/// scenarios on the snapshot, the singleton DAT_143d5c0d0 filled as the handlers fill it, then
/// its per-frame clear) against the helpers follow_player uses for them:
/// - 153 (`cam_distance_step`), run free from the first frame: +0x218 / +0x214 / +0x21c and the
///   EndInterpolationSpeed time left (singleton +0x7c / +0x80 after the clear);
/// - 155 (`cam_slot_step`, `CamBlend`), run free: the slot, the blend clock and length, every
///   named field of the blended CameraParam, the LockCamParam id, and the distance, FOV (+0x50)
///   and focus height (+0x204) the camera takes from them;
/// - 156 (`set_camera_angle`) each frame from the exe's start angles: +0x174 / +0x170 and +0x150;
/// - 151: the look limits (`look_limits_step`) each frame from the exe's state (+0x230..+0x240),
///   the free camera's clamp into them (`clamp_yaw_exe`, from the resync as in
///   `follow_camera_matches_the_exe`), the focus follow on the dummy at the event's rate and on
///   Wolf at the after-rate (+0x90), the after-rate's countdown (+0x44 / +0x48), and our dummy
///   142's view (`dummy_look`) against the exe's.
/// Exe yaws are compared in the exe's convention (`exe_yaw`). Skipped when
/// extracted/emu/taecam_frames.json is missing.
#[test]
pub(crate) fn tae_camera_events_match_the_exe() {
    use crate::camera::*;
    use std::f32::consts::{PI, TAU};
    let path = crate::paths::emu().join("taecam_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    // Our dummy 142 on Wolf standing still, with the real geometry (checked against the exe's at
    // the end).
    let mut app = app_geom("c1020", None);
    let mut ours = None;
    for _ in 0..120 {
        app.update();
        let world = app.world_mut();
        let found = world.query_filtered::<(&crate::model::Dummies, &Actor), With<Player>>().single(world).ok().map(|(dm, a)| (dm.0[&142], a.yaw));
        if let Some((m, yaw)) = found {
            ours = Some((dummy_look(world.get::<GlobalTransform>(m).unwrap()), yaw));
        }
    }
    let (ours_dummy, wolf_yaw) = ours.expect("Wolf's dummies");
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let fields = camera_param_fields();
    let row0 = combat.param("CameraParam", 0);
    let p = |k: &str| row0[k].as_f64().unwrap_or_else(|| panic!("CameraParam {k}")) as f32;
    let fp = FulcrumParams {
        rate_xz: p("targetTransChaseRateXZ"),
        rate_y: p("targetTransChaseRateY"),
        lr_max_dist: p("fulcrumDistRateByLrMoveMaxDist"),
        lr_min: p("fulcrumDistRateByLrMoveMin"),
        lr_max: p("fulcrumDistRateByLrMoveMax"),
        dist_min: p("fulcrumDistMin"),
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
    let scenarios = v["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 17);
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut exe_dummy = None;
    for s in scenarios {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        let frames = s["frames"].as_array().unwrap();
        let floats = |fr: &serde_json::Value, k: &str| fr[k].as_array().unwrap().iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>();
        let f = |x: &[f32], o: usize| x[o / 4];
        let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
        let bits = |x: f32| x.to_bits() as i32;
        let first = floats(&frames[0], "before");
        // The free-running state (153 / 155).
        let mut cam = CamState::for_tests(&combat);
        cam.dist_raw = f(&first, 0x218);
        cam.dist = f(&first, 0x214);
        cam.dist_from = f(&first, 0x21c);
        // The snapshot's blend is settled (clock = length).
        cam.blend_len = frames[0]["man"][10][1].as_f64().unwrap() as f32;
        cam.blend_t = cam.blend_len;
        let si0 = floats(&frames[0], "single_in");
        (cam.end_time, cam.end_rate) = (f(&si0, 0x7c), f(&si0, 0x80));
        let mut fov = f(&first, 0x50);
        let mut after = (0.0f32, 0.0f32);
        let mut angle_start: Option<((f32, f32), f32)> = None;
        let kind = name.split('_').next().unwrap();
        for (i, fr) in frames.iter().enumerate() {
            let at = format!("{name} frame {i}");
            let (b, a) = (floats(fr, "before"), floats(fr, "after"));
            let (si, so) = (floats(fr, "single_in"), floats(fr, "single_out"));
            let bytes: Vec<u8> = fr["single_bytes"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect();
            let man: Vec<(i32, f32)> = fr["man"].as_array().unwrap().iter().map(|m| (m[0].as_i64().unwrap() as i32, m[1].as_f64().map_or(f32::NAN, |x| x as f32))).collect();
            let blended = floats(fr, "blended");
            let written = bytes[0x9b - 0x84] == 1;
            *counts.entry(kind).or_default() += 1;
            // 153 and 155 both drive the distance; the blend runs in every scenario.
            cam_slot_step(&combat, &mut cam, man[13].0 as i64, dt);
            let blend = cam.blend();
            check("155 slot (+0x3ec)", &at, (cam.slot - man[9].0 as i64).abs() as f32, 0.0);
            check("155 blend length (+0x3f0)", &at, (cam.blend_len - man[10].1).abs(), 1e-6);
            check("155 blend clock (+0x3f4)", &at, (cam.blend_t - man[11].1).abs(), 1e-6);
            let lock_id = cam.lock_row(&combat)["id"].as_i64().unwrap_or(0);
            check("155 LockCamParam id (+0x3cc)", &at, (lock_id - man[1].0 as i64).abs() as f32, 0.0);
            for (off, ty, k) in &fields {
                let ours = blend.get(&combat, k, f64::NAN);
                let exe = if ty == "b32" { bits(blended[off / 4]) as f32 } else { blended[off / 4] };
                check(&format!("155 blended CameraParam {k}"), &at, (ours - exe).abs() / exe.abs().max(1.0), 2e-6);
            }
            let lock = cam.lock_row(&combat);
            let lf = |k: &str| lock[k].as_f64().unwrap() as f32;
            let rate = blend.get(&combat, "lockCamParamLerpRate", 0.0);
            let ev = (kind == "dist" && written).then(|| DistEvent {
                target: f(&si, 0x70),
                start_rate: f(&si, 0x74),
                slow: (f(&si, 0x78) >= 0.0).then_some((f(&si, 0x78), bytes[0] == 1)),
                transition: f(&si, 0x7c),
                end_rate: f(&si, 0x80),
            });
            cam_distance_step(&mut cam, ev, lf("camDistTarget"), rate, dt);
            check("153 distance (+0x218)", &at, (cam.dist_raw - f(&a, 0x218)).abs(), 1e-5);
            check("153 distance used (+0x214)", &at, (cam.dist - f(&a, 0x214)).abs(), 1e-5);
            check("153 SlowStart from (+0x21c)", &at, (cam.dist_from - f(&a, 0x21c)).abs(), 1e-5);
            check("153 end time / rate (singleton +0x7c / +0x80)", &at, (cam.end_time - f(&so, 0x7c)).abs().max((cam.end_rate - f(&so, 0x80)).abs()), 1e-5);
            fov += (lf("camFovY").clamp(0.0, 180.0).to_radians() - fov) * per_frame(rate, dt);
            check("155 FOV (+0x50)", &at, (fov - f(&a, 0x50)).abs(), 1e-6);
            let org_y = blend.get(&combat, "chrOrgOffsetY", 0.0);
            check("155 focus height (+0x204)", &at, (org_y + (lf("chrOrgOffset_Y") - org_y) * rate - f(&a, 0x204)).abs(), 1e-6);
            // 156: from the exe's start angles (+0x1f4 / +0x1f0) and Wolf's yaw at the first frame.
            if kind == "angle" {
                if written {
                    let wolf_yaw = floats(fr, "wolf_rot")[1];
                    let p = f(&si, 0x94);
                    if p == 0.0 || angle_start.is_none() {
                        angle_start = Some(((exe_yaw(f(&a, 0x1f4)), f(&a, 0x1f0)), PI - wolf_yaw));
                    }
                    let (from, chr) = angle_start.unwrap();
                    let interp = bits(f(&si, 0x90)) as i64 - 1;
                    let (yaw, pitch) = set_camera_angle(from, chr, (bytes[0x9a - 0x84] == 1, f(&si, 0x8c)), (bytes[0x99 - 0x84] == 1, f(&si, 0x88)), interp, p);
                    check("156 yaw (+0x174)", &at, wrap(exe_yaw(yaw) - f(&a, 0x174)).abs(), 2e-5);
                    check("156 pitch (+0x170)", &at, (pitch - f(&a, 0x170)).abs(), 2e-5);
                    check("156 position snap (+0x150 = 1, +0x154 = 0)", &at, (f(&a, 0x150) - 1.0).abs().max(f(&a, 0x154).abs()), 0.0);
                } else {
                    angle_start = None;
                }
            }
            if kind != "look" {
                continue;
            }
            // 151: the limits from the exe's state before the frame.
            let event = !f(&si, 0x3c).is_nan();
            let m = floats(fr, "dmy_matrix");
            let view = -Vec3::new(m[8], m[9], m[10]);
            let centre = (-f32::atan2(view.y, view.xz().length()), f32::atan2(view.x, view.z));
            let wolf_rot = floats(fr, "wolf_rot");
            exe_dummy.get_or_insert((centre.0, wrap(centre.1 - wolf_rot[1])));
            let lp = LookParams {
                pitch: (lf("rotRangeMinX").to_radians(), blend.get(&combat, "rotRangeXMax", 0.0)),
                yaw: (blend.get(&combat, "rotRangeYMin", 0.0), blend.get(&combat, "rotRangeYMax", 0.0)),
                base_y: blend.get(&combat, "rotBaseY", 0.0),
            };
            let mut look = [f(&b, 0x230), f(&b, 0x234), f(&b, 0x238), f(&b, 0x23c)];
            let mut look_ref = (f(&b, 0x240) <= TAU).then_some(f(&b, 0x240));
            let blocked = fr["flags_before"][0].as_u64().unwrap() != 0;
            let limits = event.then(|| [f(&si, 0x4c), f(&si, 0x50), f(&si, 0x54), f(&si, 0x58)]);
            look_limits_step(&mut look, &mut look_ref, limits, blocked, Some(centre), (0.0, wolf_rot[1]), lp, f(&b, 0x174), per_frame(rate, dt));
            let lim_err = (0..4).map(|k| (look[k] - a[0x230 / 4 + k]).abs()).fold(0.0, f32::max);
            check("151 look limits (+0x230..+0x23c)", &at, lim_err, 1e-5);
            let ref_err = match look_ref {
                Some(r) => (r - f(&a, 0x240)).abs(),
                None => (f(&a, 0x240) < f32::MAX) as u8 as f32,
            };
            check("151 limits' reference yaw (+0x240)", &at, ref_err, 1e-6);
            // The free camera's turn (as follow_camera_matches_the_exe) clamped into them.
            let focus = v3(&a, 0xc0);
            let moved = focus - v3(&b, 0xc0);
            let stick = floats(fr, "stick");
            let r = Vec2::new(stick[1], stick[0]);
            let speed = f(&b, 0x270) * 10.0;
            let rates = stick_rate(&combat, speed, f(&a, 0x160).abs());
            let fast = f(&a, 0x160) != 0.0;
            let side = if fast { r.x.signum() } else { r.x };
            let turn = Vec2::new(side * rates.x * dt, stick[0] * rates.y * dt);
            let arm = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp) - focus;
            if arm.xz().length() > 1e-4 {
                let yaw = f32::atan2(arm.x, arm.z) + turn.x + PI;
                let lim = (f(&a, 0x238), f(&a, 0x23c));
                check("151 yaw clamped (+0x174)", &at, wrap(clamp_yaw_exe(yaw, lim.0, lim.1) - f(&a, 0x174)).abs(), 1e-4);
                let resync = f32::atan2(arm.y, arm.xz().length());
                let lim = (f(&a, 0x230), f(&a, 0x234));
                let pitch = if fast { direct_pitch(resync, stick[0], lim, p("directPitchAng"), p("directPitchRate"), dt) } else { resync + turn.y };
                let (_, pitch) = apply_look_limits(0.0, pitch, &[lim.0, lim.1, -PI, PI]);
                // Not on a scenario's first frame: the fast turn's first frame from the snapshot's
                // still camera reads its pitch from elsewhere (5.6e-4 rad off, look_cone_stick;
                // the free camera's part, follow_camera_matches_the_exe).
                if i > 0 {
                    check("151 pitch clamped (+0x170)", &at, (pitch - f(&a, 0x170)).abs(), 1e-4);
                }
            }
            // The focus follow: the dummy at min(unk1, 1), else Wolf at unk3 after the event.
            let mut fc = FocusChase { vel: v3(&b, 0x1b0), last_target: v3(&b, 0x1c0), out: v3(&b, 0x1d0), clock: f(&b, 0x410) };
            let normal = Vec3::new(f(&b, 0x220), f(&b, 0x224), f(&b, 0x220));
            let (target, rate) = if event && !blocked {
                after = (f(&si, 0x44), f(&si, 0x48));
                let r = f(&si, 0x40).min(1.0);
                (Vec3::from_slice(&floats(fr, "dmy_pos")) - v3(&a, 0x200), if r > 0.0 { Vec3::splat(r) } else { normal })
            } else {
                (Vec3::from_slice(&floats(fr, "wolf")), if after.1 > 0.0 { Vec3::splat(after.1) } else { normal })
            };
            check("151 focus follow (+0x90)", &at, (fc.update(target, rate, dt) - v3(&a, 0x90)).length(), 1e-4);
            check("151 cast blocked (+0x1a0)", &at, fr["flags_after"][0].as_u64().unwrap() as f32, 0.0);
            after.0 -= dt;
            if after.0 <= 0.0 {
                after = (0.0, 0.0);
            }
            check("151 after-rate time / rate (singleton +0x44 / +0x48)", &at, (after.0 - f(&so, 0x44)).abs().max((after.1 - f(&so, 0x48)).abs()), 1e-5);
        }
    }
    // Our dummy 142 on Wolf standing still: its view against the exe's, relative to his facing.
    let (exe_pitch, exe_yaw_rel) = exe_dummy.unwrap();
    let (pitch, yaw) = ours_dummy;
    check("151 our dummy 142's view (pitch)", "Wolf idle", (pitch - exe_pitch).abs(), 0.05);
    check("151 our dummy 142's view (yaw from Wolf's facing)", "Wolf idle", wrap(yaw - exe_yaw(wolf_yaw) - exe_yaw_rel).abs(), 0.05);
    assert_eq!(counts.values().sum::<usize>(), 2160, "frames: {counts:?}");
    let report: Vec<String> = worst.iter().filter(|(_, (e, _))| *e > 0.0).map(|(k, (e, at))| format!("{k}: off by {e:.7} at {at}")).collect();
    assert!(report.is_empty(), "the TAE camera events differ from the exe:\n{}", report.join("\n"));
}
