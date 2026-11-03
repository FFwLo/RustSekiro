//! Wolf's turn and move request from the move stick against the exe's.

use bevy::prelude::*;

use crate::actor::{wrap_angle, ShownYaw};
use crate::player::{stick_world, yaw_of, Turn};

/// The game's PlayerIns update (FUN_140a37560, run in tools/emu-cases `turn` on a memory snapshot
/// of the user's sekiro.exe, Wolf standing, the camera yaw in the manipulator 0: 14 scenarios of
/// 20-50 frames - the stick ahead, to each side, back, 135 deg back left, a half diagonal, its
/// length 0 .. 1.2, swept round at 10 / 40 / 60 deg a frame, a flick then let go, 30 fps, a camera
/// yaw of 1 rad, a TAE SetTurnSpeed of 360) against `Turn` (the manipulator's 45 deg gate and yaw
/// request, the turn controller's whole or clamped step) and `ShownYaw` (the delay controller the
/// player's GetRotation reads), frame by frame. The exe's pad stick is (forward, side), its side
/// +1 the world's -X: our (-side, forward). Skipped when extracted/emu/turn_frames.json is missing.
#[test]
pub(crate) fn wolf_turn_matches_the_exe() {
    let path = crate::paths::root().join("extracted/emu/turn_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let scenarios = v["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 14);
    let mut worst: std::collections::BTreeMap<&str, (f32, String)> = Default::default();
    let mut check = |what: &'static str, at: &str, err: f32| {
        let e = worst.entry(what).or_insert((0.0, String::new()));
        if err > 1e-4 && err > e.0 {
            *e = (err, at.to_string());
        }
    };
    let mut frames = 0;
    for s in scenarios {
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let (dt, cam_yaw, tae) = (f(&s["dt"]), f(&s["cam_yaw"]), f(&s["tae_turn"]));
        let rate = (tae >= 0.0).then_some(tae);
        let (mut turn, mut yaw, mut shown) = (Turn::default(), 0.0f32, None::<ShownYaw>);
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let at = format!("{} frame {i}", s["name"].as_str().unwrap());
            let raw = Vec2::new(-f(&fr["stick"][1]), f(&fr["stick"][0]));
            turn.new_tick();
            let taken = turn.gate(raw);
            let world = stick_world(taken, cam_yaw) * taken.length();
            turn.turn(&mut yaw, &mut shown, (world != Vec3::ZERO).then(|| yaw_of(world)), rate, 720.0, dt);
            let drawn = ShownYaw::draw(&mut shown, yaw, dt);
            check("MoveSpeedLevel (+0xbac)", &at, (taken.length() - f(&fr["level"])).abs());
            check("move vector (+0xc0)", &at, (world - Vec3::new(f(&fr["world"][0]), 0.0, f(&fr["world"][1]))).length());
            check("yaw request (+0x24)", &at, wrap_angle(turn.request.unwrap_or(0.0) - f(&fr["request"])).abs());
            check("physics yaw (+0x74)", &at, wrap_angle(yaw - f(&fr["yaw"])).abs());
            check("drawn yaw (delay +0x24)", &at, wrap_angle(drawn - f(&fr["shown"])).abs());
            check("drawn turn speed (delay +0x30)", &at, (shown.map_or(0.0, |s| s.speed) - f(&fr["speed"])).abs());
            frames += 1;
        }
    }
    assert_eq!(frames, 440);
    let report: Vec<String> = worst.iter().filter(|(_, (e, _))| *e > 0.0).map(|(k, (e, at))| format!("{k}: off by {e} at {at}")).collect();
    assert!(report.is_empty(), "Wolf's turn differs from the exe:\n{}", report.join("\n"));
}
