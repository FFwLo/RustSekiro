//! The Grappling Hook against the exe (row E7): tools/emu-cases `wire` on the 2026-11-04 snapshot
//! (General Naomori's arena). Before each whole chr frame FUN_140a4b570 the wire point search runs
//! (the singleton [0x143d8e058]'s task FUN_140efaf80 -> FUN_140efd8e0), the camera turned at the
//! hook; then the press. `wire_frames.json`: per scenario and frame the search's result (state,
//! hook, camera), Wolf's TAE clips, position, yaw and velocity. The test skips when the JSON (or
//! the map export extracted/map_edges_m11_00_00_00.json) is missing.

use super::*;
use crate::traversal::{GrapplePoint, GrappleTarget};

type Json = serde_json::Value;

fn load(name: &str) -> Option<Json> {
    let text = std::fs::read_to_string(crate::paths::emu().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn v3(x: &Json) -> Vec3 {
    Vec3::new(x[0].as_f64().unwrap() as f32, x[1].as_f64().unwrap() as f32, x[2].as_f64().unwrap() as f32)
}

/// Game space -> Bevy (X mirrored).
fn bevy(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, v.z)
}

/// One scenario run in the sim: Wolf at the exe's start (feet at the origin), facing its yaw, the
/// point as the wire aim, the press on the exe's frames. Per tick: (state, anim, t, feet, yaw).
fn run_sim(s: &Json, point: GrapplePoint) -> Vec<(String, String, f32, Vec3, f32)> {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let w = app.world_mut();
        let mut q = w.query_filtered::<(&mut Transform, &mut Actor), With<Player>>();
        let (mut t, mut a) = q.single_mut(w).unwrap();
        a.yaw = -(s["yaw0"].as_f64().unwrap() as f32);
        t.rotation = Quat::from_rotation_y(a.yaw);
    }
    let target = GrappleTarget { index: 0, point, dist: 0.0, in_action_range: true, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None, host: false });
    let frames = s["frames"].as_array().unwrap();
    let mut out = Vec::new();
    for f in frames {
        if f["arms"].as_array().is_some_and(|a| !a.is_empty()) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
        }
        app.update();
        let w = app.world_mut();
        let mut q = w.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(w).unwrap();
        out.push((a.state.clone(), a.anim.clone(), a.t, t.translation - Vec3::Y * CAPSULE_HALF_HEIGHT, a.yaw));
    }
    out
}

/// The frames a TAE clip ran in the exe (its id in the frame's `tae` log).
fn tae_frames(frames: &[Json], id: u64) -> Vec<usize> {
    frames.iter().enumerate().filter(|(_, f)| f["log"].as_array().unwrap().iter().any(|l| l["at"] == "tae" && l["id"] == id)).map(|(i, _)| i).collect()
}

/// RSekiro takes a TAE physics event (920, 904) in the decide after the clock crossed it, the
/// update after the exe's (which crosses and steps in one frame): from the launch on the sim is one
/// frame later. gap: structural (decide before advance), shared by every 920 launch.
const SIM_LAG: usize = 1;

/// The exe's wire on map points of General Naomori's arena, frame by frame against the sim, from
/// the ground (h100_d12, where the placement left Wolf 6 mm up and the exe shot from the air, is
/// only searched). Per scenario:
/// - the search: `traversal::grapple_target` with the exe's camera on the map export's points picks
///   the exe's hook, in action range exactly when the exe's is (h24_d17: 17 m, no shoot);
/// - GroundWireShoot starts on the exe's frame, turns at its rate and ends facing the exe's yaw
///   (the landing spot, `Wire::turn`) within 0.003 rad;
/// - the move start is the exe's (202012 from 9.7 and 13 m, 202010 upslope from 5 and 6.5 m:
///   `wire::dramatic_angle` from the feet) on the exe's frame;
/// - the flight (the Wire fall braking 10 m/s^2 under 19.6 m/s^2, `wire::arc`) and the guaranteed
///   arrival: on 202012 every flight frame within 12 mm in height and 0.06 m in all of the exe's
///   (SIM_LAG later; the across-ground gap grows to the spots' difference); the
///   arrival on the exe's step (the flight time to the nearest step; 202010 within 2) within
///   0.06 m of its spot (the export's landing spot is
///   not the exe's own, gap). 202010 slides first (904): the arrival only (gap: the slide's
///   height, the exe's ground step, is 0.27 m off at the launch).
/// gap: after the arrival the exe falls 0.36 m more onto the roof through WireFallLandReady
/// (202045) before LandWireMove; the sim stands on the landing spot (no roof collision).
#[test]
pub(crate) fn grapple_frames_match_the_exe() {
    let Some(v) = load("wire_frames.json") else { return };
    let Some(map) = load("../map_edges_m11_00_00_00.json") else { return };
    let points: Vec<GrapplePoint> = map["grapple"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| GrapplePoint { hook: bevy(v3(&p["hook"])), land: bevy(v3(&p["land"])), fwd: bevy(v3(&p["fwd"])), variation: p["var"].as_i64().unwrap_or(0) })
        .collect();
    let mut checked = 0;
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let frames = s["frames"].as_array().unwrap();
        let start = bevy(v3(&s["start"]));
        // The search before the press (frame 2): state 1 in action range, 2 found but out of it.
        let t = &frames[2]["target"];
        let (state, hook) = (t["state"].as_i64().unwrap(), bevy(v3(&t["hook"])));
        let view: Vec<f32> = t["view"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
        let cam_pos = bevy(Vec3::new(view[12], view[13], view[14]));
        let cam_fwd = bevy(Vec3::new(view[8], view[9], view[10])).normalize();
        let pick = crate::traversal::grapple_target(cam_pos, cam_fwd, start + Vec3::Y * CAPSULE_HALF_HEIGHT, None, &points).expect("no point");
        assert!((pick.point.hook - hook).length() < 0.05, "{name}: picked {:?}, exe {hook:?}", pick.point.hook);
        assert_eq!(pick.in_action_range, state == 1, "{name}: in action range");
        let shoot = tae_frames(frames, 202000);
        if state != 1 {
            assert!(shoot.is_empty(), "{name}: the exe shot out of range");
            checked += 1;
            continue;
        }
        if shoot.is_empty() {
            assert!(!tae_frames(frames, 202100).is_empty(), "{name}: no shoot");
            checked += 1;
            continue;
        }
        let point = GrapplePoint { hook: pick.point.hook - start, land: pick.point.land - start, ..pick.point };
        let sim = run_sim(s, point);
        let exe_pos = |i: usize| bevy(v3(&frames[i]["pos"])) - start;
        let exe_yaw = |i: usize| -frames[i]["yaw"].as_f64().unwrap() as f32;
        // The shoot: its first frame, the turn (the exe's yaw is read a frame later), the facing.
        assert_eq!(sim.iter().position(|o| o.0 == "GroundWireShoot"), Some(shoot[0]), "{name}: shoot frame");
        for i in shoot[0]..shoot[0] + 4 {
            assert!((sim[i].4 - exe_yaw(i + 1)).abs() < 0.003, "{name} f{i}: turn {} vs exe {}", sim[i].4, exe_yaw(i + 1));
        }
        let last = frames.len() - 1;
        assert!((sim[last].4 - exe_yaw(last)).abs() < 0.003, "{name}: yaw {} vs exe {}", sim[last].4, exe_yaw(last));
        // The move start.
        let (ms, ms_id) = [202010u64, 202011, 202012].iter().find_map(|&id| tae_frames(frames, id).first().map(|&f| (f, id))).expect("no move start");
        assert_eq!(sim.iter().position(|o| o.0.contains("WireMoveStart")), Some(ms), "{name}: move start frame");
        assert_eq!(sim[ms].1, format!("a000_{ms_id}"), "{name}: move start");
        // The arrival: the exe's last frame before its land-ready pose; the sim is there SIM_LAG
        // later and stays.
        let arrive = tae_frames(frames, 202045)[0] - 1;
        let at = exe_pos(arrive);
        let still = (ms..sim.len()).find(|&i| (sim[i].3 - sim[last].3).with_y(0.0).length() < 1e-4);
        // 202010's slide leaves the sim elsewhere at the launch, so its flight is not the exe's.
        let slack = if ms_id == 202012 { 0 } else { 2 };
        assert!(still.is_some_and(|f| f.abs_diff(arrive + SIM_LAG) <= slack), "{name}: the sim's arrival step {still:?}, the exe's {}", arrive + SIM_LAG);
        let sim_at = sim[arrive + SIM_LAG].3;
        assert!((sim_at - at).with_y(0.0).length() < 0.06 && (sim_at.y - at.y).abs() < 0.01, "{name}: arrival {sim_at:?} vs exe {at:?}");
        assert!((sim[last].3 - sim_at).length() < 1e-3, "{name}: the sim moved after the arrival");
        // The flight, frame by frame (202012: no slide before the launch).
        if ms_id == 202012 {
            let launch = (ms..arrive).find(|&i| exe_pos(i).with_y(0.0).length() > 1e-3).unwrap();
            assert!(sim[launch - 1 + SIM_LAG].3.length() < 1e-3, "{name}: the sim launched early");
            // The launch: the exe's first step's speed, before that step's braking and gravity, is
            // the arc's to the export's landing spot from the start (1 cm/s).
            let vel = v3(&frames[launch]["vel"]);
            let (_, vh, vy) = crate::wire::arc(point.land.with_y(0.0).length(), point.land.y, -10.0, -19.6);
            assert!((vel.with_y(0.0).length() + 10.0 / 60.0 - vh).abs() < 0.01 && (vel.y + 19.6 / 60.0 - vy).abs() < 0.01, "{name}: launch {vel:?}, the arc's {vh} / {vy}");
            for i in launch..arrive {
                let d = (sim[i + SIM_LAG].3 - exe_pos(i)).length();
                assert!(d < 0.06 && (sim[i + SIM_LAG].3.y - exe_pos(i).y).abs() < 0.012, "{name} f{i}: flight {:?} vs exe {:?} ({d:.3} m)", sim[i + SIM_LAG].3, exe_pos(i));
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 8, "scenarios checked");
}
