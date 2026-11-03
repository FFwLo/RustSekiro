//! The follow camera's last wall cases and the lock-on manipulator against the exe (topic
//! camera2, docs/kb/camera.md "Escape wall, focus pull-in, lock-on at walls, 30 fps"):
//! tools/emu-cases `camera2_walls` runs FUN_140734790 -> FUN_14073c260 on the Naomori-arena
//! snapshot with every cast hooked (the escape-wall option on, Wolf walked and put through the
//! wall about 9 m along +X, the wall at 30 fps, locked on at the wall and the ground);
//! `camera2_manip` runs Wolf's PlayerIns update (FUN_140a37560) with a lock point in his lock
//! module. Each camera frame starts from the exe's state before it and feeds RSekiro's collision
//! block the exe's own cast results. Skipped when the JSON is missing.

use super::*;

const FULCRUM_CAST: &str = "0x14073f5c8";
const ORBIT_CAST: &str = "0x14073f9ca";
const FINAL_CAST: &str = "0x1407412b9";
const WING_CASTS: [&str; 2] = ["0x14073ffe5", "0x140740025"];
const FOCUS_RAY: &str = "0x14073d2ff";
const PULL_CAST: &str = "0x14073d3ad";

#[test]
pub(crate) fn camera_escape_wall_pull_in_and_lock_at_walls_match_the_exe() {
    use crate::camera::*;
    use std::cell::Cell;
    use std::f32::consts::{PI, TAU};
    let path = crate::paths::root().join("extracted/emu/camera2_walls.json");
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
    let lock = LockChaseParams {
        tgt_pos_rate: p("lockTgtPosRate"),
        rate_x: p("lockRotChaseRateX"),
        rate_y: p("lockRotChaseRateY"),
        play_x: p("lockRotChasePlayAngX"),
        play_y: p("lockRotChasePlayAngY"),
        at_lock: (p("rotRangeXAtLockMin"), p("rotRangeXAtLockMax")),
        lerp_height: (p("rotRangeLerpBeginHeight"), p("rotRangeLerpEndHeight")),
    };
    let free = PosChase {
        xz: p("targetChaseRateXZForNormal"),
        y: p("targetChaseRateY"),
        ang_min: p("targetChaseRateAngMin"),
        ang_max: p("targetChaseRateAngMax"),
        dist_xz: p("targetChaseDistRateXZ"),
        dist_y: p("targetChaseDistRateY"),
    };
    let at_lock = PosChase {
        xz: p("targetChaseRateAtLockXZ"),
        y: p("targetChaseRateAtLockY"),
        ang_min: p("targetChaseRateAngAtLockMin"),
        ang_max: p("targetChaseRateAngAtLockMax"),
        ..free
    };
    let row_rates = Vec3::new(p("chrTransChaseRateXZ_ForNormal"), p("chrTransChaseRateY_ForNormal"), p("targetChaseRateXZForNormal"));
    let radius = p("camCastSphereRadius");
    let hit_time = p("targetChaseDistRateTime");
    let normal = Vec2::new(p("targetChaseDistRateXZ"), p("targetChaseDistRateY"));
    let on_wall = Vec2::new(p("targetChaseDistRateXZ_ForHitWall"), p("targetChaseDistRateY_ForHitWall"));
    let wrap = |x: f32| (x + PI).rem_euclid(TAU) - PI;
    let mut worst: std::collections::BTreeMap<String, (f32, String, usize)> = Default::default();
    let mut check = |what: &str, at: &str, err: f32, tol: f32| {
        let e = worst.entry(what.to_string()).or_insert((0.0, String::new(), 0));
        let err = if err.is_nan() { f32::INFINITY } else { err };
        if err > tol {
            e.2 += 1;
            if err > e.0 {
                (e.0, e.1) = (err, at.to_string());
            }
        }
    };
    let (mut checked, mut escaped, mut locked_walled, mut pulled, mut fps30) = (0, 0, 0, 0, 0);
    let mut old_30 = 0.0f32;
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        let escape = s["escape"].as_bool() == Some(true);
        let mut avg = LockPointAverage::default();
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{name} frame {i}");
            let floats = |x: &serde_json::Value| x.as_array().map(|a| a.iter().map(|x| x.as_f64().map_or(f32::NAN, |x| x as f32)).collect::<Vec<_>>());
            let (b, a, stick) = (floats(&fr["before"]).unwrap(), floats(&fr["after"]).unwrap(), floats(&fr["stick"]).unwrap());
            let f = |x: &[f32], o: usize| x[o / 4];
            let v3 = |x: &[f32], o: usize| Vec3::new(x[o / 4], x[o / 4 + 1], x[o / 4 + 2]);
            let casts = fr["casts"].as_array().unwrap();
            let cast = |site: &str| casts.iter().find(|c| c["site"] == site);
            let hit = |site: &str| cast(site).is_some_and(|c| c["hit"].as_u64().unwrap_or(0) != 0);
            let frac = |site: &str| cast(site).and_then(|c| c["frac"].as_f64()).map(|x| x as f32);
            let ends = |site: &str| cast(site).map(|c| (Vec3::from_slice(&floats(&c["start"]).unwrap()), Vec3::from_slice(&floats(&c["dir"]).unwrap()), c["radius"].as_f64().unwrap() as f32));
            checked += 1;
            fps30 += (dt > 1.0 / 45.0) as usize;
            let off = v3(&a, 0x200);
            let wolf = Vec3::from_slice(&floats(&fr["wolf"]).unwrap());
            // The focus follow at last frame's working rates, starting in X / Z from last frame's
            // focus +0x90 (less the offset: after a pull-in the pulled point, else the chase's
            // own output), then the pull-in fed the exe's ray and sphere results (this frame's
            // +0x1d0 keeps the unpulled point).
            let prev = v3(&b, 0x90) - v3(&b, 0x200);
            let out = Vec3::new(prev.x, f(&b, 0x1d4), prev.z);
            let mut fc = FocusChase { vel: v3(&b, 0x1b0), last_target: v3(&b, 0x1c0), out, clock: f(&b, 0x410) };
            let ours = fc.update(wolf, Vec3::new(f(&b, 0x220), f(&b, 0x224), f(&b, 0x220)), dt);
            check("focus follow (+0x1d0)", &at, (ours - v3(&a, 0x1d0)).length(), 1e-4);
            let seen_ray = Cell::new(None);
            let seen_cast = Cell::new(None);
            let ray = |o: Vec3, d: Vec3| {
                seen_ray.set(Some((o, d)));
                hit(FOCUS_RAY)
            };
            let sphere = |o: Vec3, d: Vec3, r: f32| {
                seen_cast.set(Some((o, d, r)));
                frac(PULL_CAST).filter(|_| hit(PULL_CAST))
            };
            let focus = focus_pull_in(v3(&b, 0x90) + off, v3(&a, 0x1d0) + off, wolf + off, crate::player::CAPSULE_RADIUS, ray, sphere);
            check("focus after the pull-in (+0x90)", &at, (focus - off - v3(&a, 0x90)).length(), 1e-4);
            if let (Some((o, d)), Some((eo, ed, _))) = (seen_ray.get(), ends(FOCUS_RAY)) {
                check("pull-in ray (old focus -> chased, + half radius)", &at, (o - eo).length().max((d - ed).length()), 1e-4);
            }
            if hit(FOCUS_RAY) {
                pulled += 1;
                let (Some((o, d, r)), Some((eo, ed, er))) = (seen_cast.get(), ends(PULL_CAST)) else { panic!("{at}: pull-in sphere cast missing") };
                check("pull-in sphere cast (Wolf -> old focus, his radius)", &at, (o - eo).length().max((d - ed).length()).max((r - er).abs()), 1e-4);
            }
            let focus = v3(&a, 0xc0);
            let moved = focus - v3(&b, 0xc0);
            let target = floats(&fr["target"]);
            let limits = (f(&a, 0x230), f(&a, 0x234));
            if let Some(t) = &target {
                // Locked: the lock point and the angles from the dragged fulcrum (`lock_chase`).
                let lp = avg.update(Entity::PLACEHOLDER, (Vec3::from_slice(&floats(&fr["target_frame"]).unwrap()), Quat::IDENTITY), Vec3::from_slice(t), dt);
                check("locked: lock point (+0x280)", &at, (lp - v3(&a, 0x280)).length(), 1e-4);
                let fulcrum = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp);
                let (pitch, yaw) = lock_chase(focus, fulcrum, v3(&a, 0x280), f(&a, 0x50) * f(&a, 0x278) * 0.5, limits, lock, dt);
                check("locked: pitch (+0x170)", &at, (pitch - f(&a, 0x170)).abs(), 1e-4);
                check("locked: yaw (+0x174)", &at, wrap_pi(yaw - (f(&a, 0x174) - PI)).abs(), 1e-4);
            } else if moved.length() < 1.0 && !hit(FOCUS_RAY) {
                // Free: yaw and pitch read back from last frame's fulcrum (as camwall), not on
                // the frames Wolf is put somewhere.
                let speed = f(&b, 0x270) * 10.0;
                let fast = f(&a, 0x160) != 0.0;
                let rates = stick_rate(&combat, speed, f(&a, 0x160).abs());
                let side = if fast { stick[1].signum() } else { stick[1] };
                let turn = Vec2::new(side * rates.x * dt, stick[0] * rates.y * dt);
                let arm = drag_fulcrum(v3(&b, 0xe0), moved, f(&a, 0x194), fp) - focus;
                check("free: yaw (+0x174)", &at, wrap(f32::atan2(arm.x, arm.z) + turn.x - (f(&a, 0x174) - PI)).abs(), 1e-4);
                let resync = f32::atan2(arm.y, arm.with_y(0.0).length());
                let pitch = if fast { direct_pitch(resync, stick[0], limits, p("directPitchAng"), p("directPitchRate"), dt) } else { resync + turn.y };
                check("free: pitch (+0x170)", &at, (pitch.clamp(limits.0, limits.1) - f(&a, 0x170)).abs(), 1e-4);
            }
            // The fulcrum on the arm (its sideways move counted whole a frame, also at 30 fps),
            // then its cast; the orbit point and its cast. The escape-wall option skips both casts.
            let rotation = Quat::from_euler(EulerRot::YXZ, f(&a, 0x174) - PI, -f(&a, 0x170), 0.0);
            let right = v3(&b, 0x10);
            let sideways = (moved.x * right.x + moved.z * right.z).abs();
            let placed = place_fulcrum(focus, rotation * Vec3::Z, (v3(&b, 0x40) - focus).length(), sideways, dt, fp);
            let fulcrum = if escape {
                escaped += 1;
                check("escape wall: no fulcrum / orbit cast", &at, (cast(FULCRUM_CAST).is_some() || cast(ORBIT_CAST).is_some()) as u8 as f32, 0.0);
                placed
            } else {
                let Some((st, d, rad)) = ends(FULCRUM_CAST) else { panic!("{at}: no fulcrum cast") };
                check("fulcrum cast (focus -> placed fulcrum, radius)", &at, (st - focus).length().max((st + d - placed).length()).max((rad - radius).abs()), 1e-4);
                cast_cut(focus, placed - focus, frac(FULCRUM_CAST), FULCRUM_CUT_MIN)
            };
            check("fulcrum (+0xe0)", &at, (fulcrum - v3(&a, 0xe0)).length(), 1e-4);
            let raw = focus + (fulcrum - focus).normalize() * f(&a, 0x214);
            let raw = if escape {
                raw
            } else {
                let Some((st, d, _)) = ends(ORBIT_CAST) else { panic!("{at}: no orbit cast") };
                check("orbit cast (focus -> orbit point)", &at, (st - focus).length().max((st + d - raw).length()), 1e-4);
                cast_cut(focus, raw - focus, frac(ORBIT_CAST), CAST_CUT_MIN)
            };
            check("orbit point (+0xf0)", &at, (raw - v3(&a, 0xf0)).length(), 1e-4);
            // The escape wings from Wolf's point (+0xa0) along the arm, and the working rates.
            let mut wing_hit = false;
            if escape {
                let wings = escape_wings(v3(&a, 0xa0), raw, p("escapeWallWingAngY"), p("escapeWallWingLen"));
                for (k, site) in WING_CASTS.iter().enumerate() {
                    let Some((st, d, r)) = ends(site) else { panic!("{at}: no wing cast {site}") };
                    // The exe's starts carry its f32 rounding of points 200 m out (sd 9e-6 m).
                    check("escape wing start", &at, (st - wings[k].0).length(), 5e-5);
                    check("escape wing cast vector, radius", &at, (d - wings[k].1).length().max((r - p("escapeWallWingRadius")).abs()), 1e-6);
                    wing_hit |= hit(site);
                }
            }
            let escape_rates = Vec3::new(p("chrTransChaseRateXZ_ForEscapeWall"), p("chrTransChaseRateY_ForEscapeWall"), p("targetChaseRateXZForEscapeWall"));
            let rates = working_rates_step(v3(&b, 0x220), row_rates, wing_hit.then_some(escape_rates), dt);
            check("working rates (+0x220 / +0x224 / +0x228)", &at, (rates - v3(&a, 0x220)).abs().max_element(), 1e-6);
            // The position chase once a frame (also at 30 fps), the distance chase from the
            // cut camera +0x110 at the hit-wall rates.
            let (dist_rate, clock) = hit_wall_rates(f(&b, 0x1e4), hit_time, normal, on_wall, dt);
            check("hit-wall distance rates (+0x1e8 / +0x1ec)", &at, (dist_rate - Vec2::new(f(&a, 0x1e8), f(&a, 0x1ec))).abs().max_element(), 1e-5);
            let r = if target.is_some() { at_lock } else { PosChase { xz: rates.z, ..free } };
            let r = PosChase { dist_xz: dist_rate.x, dist_y: dist_rate.y, ..r };
            let last = (v3(&b, 0x110) - focus).length();
            let pos = chase_position_from(v3(&b, 0x100), last, raw, focus, r, Some(limits), f(&a, 0x150), dt);
            check("position chase (+0x100)", &at, (pos - v3(&a, 0x100)).length(), 1e-4);
            if dt > 1.0 / 45.0 {
                // Before: the rates per 1/60 s, 1 - (1 - r)^2 a 30 fps frame.
                let old = chase_position_by(v3(&b, 0x100), last, raw, focus, r, Some(limits), f(&a, 0x150), |x| 1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(2));
                old_30 = old_30.max((old - v3(&a, 0x100)).length());
            }
            let pos = v3(&a, 0x100);
            let Some((st, d, _)) = ends(FINAL_CAST) else { panic!("{at}: no final cast") };
            check("final cast (focus -> +0x100)", &at, (st - focus).length().max((st + d - pos).length()), 1e-4);
            let clock = if hit(FINAL_CAST) { hit_time } else { clock };
            check("hit-wall clock (+0x1e4)", &at, (clock - f(&a, 0x1e4)).abs(), 1e-5);
            let post = cast_cut(focus, pos - focus, frac(FINAL_CAST), CAST_CUT_MIN);
            check("camera cut (+0x110)", &at, (post - v3(&a, 0x110)).length(), 1e-4);
            if target.is_some() && (frac(FULCRUM_CAST).is_some() || frac(ORBIT_CAST).is_some() || frac(FINAL_CAST).is_some()) {
                locked_walled += 1;
            }
            let dd = (post - focus).length();
            let cam = focus + (post - focus) * safe_margin(dd, p("camSafeMarginRate"), p("camSafeMarginMax")) / dd;
            check("camera (+0x40, safe margin)", &at, (cam - v3(&a, 0x40)).length(), 1e-4);
        }
    }
    println!("camera2: {checked} frames ({escaped} escape wall, {pulled} pull-ins, {locked_walled} locked with a cast cut, {fps30} at 30 fps); the old 30 fps chase was off by up to {old_30:.3} m");
    assert!(checked >= 2500 && escaped >= 900 && pulled >= 5 && locked_walled >= 300 && fps30 >= 400, "{checked} frames, {escaped} escape, {pulled} pull-ins, {locked_walled} locked at walls, {fps30} at 30 fps");
    let report: Vec<String> = worst.iter().filter(|(_, (e, _, _))| *e > 0.0).map(|(k, (e, at, n))| format!("{k}: off by {e:.6} at {at} ({n} frames)")).collect();
    assert!(report.is_empty(), "the camera differs from the exe:\n{}", report.join("\n"));
}

/// Wolf's PlayerIns update with a lock point (tools/emu-cases `camera2_manip`: the lock module
/// hands it to the manipulator, FUN_1407f0a70 -> FUN_1407f1710): the stick is taken as it is
/// (no 45 deg gate: a press from rest moves that frame, a 90 deg flick is no let-go), turned by
/// the camera yaw as free; standing, the facing turns to the target through the turn controller
/// (the request +0x24 = the target's yaw from Wolf minus his, applied whole the next frame, drawn
/// at 1.1x). After the lock is let go the free gate is back.
#[test]
pub(crate) fn lock_manipulator_matches_the_exe() {
    use crate::actor::{wrap_angle, ShownYaw};
    use crate::player::{stick_world, yaw_of, Turn};
    let path = crate::paths::root().join("extracted/emu/camera2_manip.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut worst: std::collections::BTreeMap<&str, (f32, String)> = Default::default();
    let mut check = |what: &'static str, at: &str, err: f32| {
        let e = worst.entry(what).or_insert((0.0, String::new()));
        if err > 1e-4 && err > e.0 {
            *e = (err, at.to_string());
        }
    };
    let (mut frames, mut turned) = (0, 0);
    for s in v["scenarios"].as_array().unwrap() {
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let dt = f(&s["dt"]);
        let (mut turn, mut yaw, mut shown) = (Turn::default(), 0.0f32, None::<ShownYaw>);
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{} frame {i}", s["name"].as_str().unwrap());
            let raw = Vec2::new(-f(&fr["stick"][1]), f(&fr["stick"][0]));
            let target = fr["target"].as_array().map(|t| Vec3::new(f(&t[0]), f(&t[1]), f(&t[2])));
            turn.new_tick();
            let taken = turn.gate(raw, target.is_some());
            let world = stick_world(taken, 0.0) * taken.length();
            check("move vector (+0xc0)", &at, (world - Vec3::new(f(&fr["mc0"][0]), 0.0, f(&fr["mc0"][2]))).length());
            check("MoveSpeedLevel (+0xbac)", &at, (taken.length() - f(&fr["level"])).abs());
            frames += 1;
            // The facing: standing locked (the branch decide() takes), or free from the stick.
            let walking = target.is_some() && raw != Vec2::ZERO;
            let want = match target {
                Some(t) => {
                    let pos = Vec3::new(f(&fr["pos"][0]), 0.0, f(&fr["pos"][2]));
                    Some(yaw_of((t - pos).with_y(0.0)))
                }
                None => (world != Vec3::ZERO).then(|| yaw_of(world)),
            };
            turn.turn(&mut yaw, &mut shown, want, None, 720.0, dt);
            let drawn = ShownYaw::draw(&mut shown, yaw, dt);
            if target.is_some() {
                turned += !walking as usize;
                check(if walking { "locked walking: yaw request (+0x24)" } else { "locked: yaw request (+0x24)" }, &at, wrap_angle(turn.request.unwrap_or(0.0) - f(&fr["m20"][1])).abs());
            }
            check(if walking { "locked walking: physics yaw" } else { "physics yaw (+0x74)" }, &at, wrap_angle(yaw - f(&fr["yaw"])).abs());
            check(if walking { "locked walking: drawn yaw" } else { "drawn yaw (delay +0x24)" }, &at, wrap_angle(drawn - f(&fr["shown"])).abs());
        }
    }
    println!("lock manipulator: {frames} frames, {turned} standing locked");
    assert!(frames >= 300 && turned >= 90, "{frames} frames, {turned} standing locked");
    let report: Vec<String> = worst.iter().filter(|(_, (e, _))| *e > 0.0).map(|(k, (e, at))| format!("{k}: off by {e} at {at}")).collect();
    assert!(report.is_empty(), "the lock-on manipulator differs from the exe:\n{}", report.join("\n"));
}
