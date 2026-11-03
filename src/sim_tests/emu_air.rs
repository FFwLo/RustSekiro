//! Wolf in the air against the exe: tools/emu-cases `air` on the 2026-11-03 snapshot (whole
//! character frames FUN_140a4b570 at 1/60 s with the pad driven; the physics step FUN_140bccd50
//! -> FUN_140bacc10 / FUN_140bac120, the NoGravity read FUN_140bcc420, the land check
//! FUN_140b97a30 -> FUN_140b97b30). `air_jump.json`: jumps (still, stick forward / side / half /
//! back, off a run) and walking / running off the arena's edge; `air_art.json`: arts from the
//! ground, an air slash and a jump kick; `air_ng_launch.json`: SetNoGravity forced around a jump's
//! launch; `air_drops_*.json`: drops from 1 to 80 m. Each test skips when its JSON is missing.

use super::*;
use crate::data::FALL_TYPES;
use crate::player::{air_frame, air_step, fall_hp_loss, LAUNCH_TIME};

type Json = serde_json::Value;

fn load(name: &str) -> Option<Json> {
    let text = std::fs::read_to_string(crate::paths::emu().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn v3(x: &Json) -> Vec3 {
    Vec3::new(x[0].as_f64().unwrap() as f32, x[1].as_f64().unwrap() as f32, x[2].as_f64().unwrap() as f32)
}

fn logs<'a>(f: &'a Json, at: &str) -> Vec<&'a Json> {
    f["log"].as_array().unwrap().iter().filter(|l| l["at"] == at).collect()
}

fn scenarios(v: &Json) -> &Vec<Json> {
    v["scenarios"].as_array().unwrap()
}

/// The frame the jump launches (TAE 201011's first window), if any.
fn launch_frame(s: &Json) -> Option<usize> {
    s["frames"].as_array().unwrap().iter().position(|f| logs(f, "tae").iter().any(|l| l["id"] == 201011 && l["prev"].as_f64() == Some(0.0)))
}

/// Every exe air step (the fall module ran: its input velocity +0x160 after the launch / ground
/// hand-over, and the body airborne after it) gives the same velocity from `player::air_step` on
/// the frame's world stick (FUN_1409efea0 +0xc0) and fall type: 873 frames in 17 runs (jumps,
/// stick in four directions and half, a run jump, two walk-offs, ground arts falling off the
/// cliff, an air slash, a jump kick, NoGravity with the launch timer running), within 1e-4 m/s.
#[test]
pub(crate) fn air_steps_match_the_exe() {
    let dt = 1.0 / 60.0;
    let (mut n, mut worst, mut bad) = (0, 0.0f32, Vec::new());
    for name in ["air_jump.json", "air_art.json", "air_ng_launch.json"] {
        let Some(v) = load(name) else { return };
        for s in scenarios(&v) {
            assert_eq!(s["stopped"], Json::Null, "{name} {} stopped", s["name"]);
            for (i, f) in s["frames"].as_array().unwrap().iter().enumerate() {
                let pre = logs(f, "fall_pre");
                if pre.is_empty() || f["a2"][0] != 0 {
                    continue;
                }
                let input = v3(&pre[0]["in"]);
                let stick = v3(&f["world_stick"]);
                let ft = FALL_TYPES[(f["fall_type"].as_u64().unwrap() as usize).min(FALL_TYPES.len() - 1)];
                let (h, vy) = air_step(input.with_y(0.0), input.y, if ft.controllable { stick } else { Vec3::ZERO }, ft, dt);
                let exe = v3(&f["vel"]);
                let err = (h.with_y(vy) - exe).abs().max_element();
                worst = worst.max(err);
                n += 1;
                if err > 1e-4 {
                    bad.push(format!("{} f{i}: in {input} stick {stick} -> exe {exe}, ours {}", s["name"], h.with_y(vy)));
                }
            }
        }
    }
    assert!(n >= 870, "only {n} air steps");
    assert!(bad.is_empty(), "{} of {n} air steps differ (worst {worst}):\n{}", bad.len(), bad.join("\n"));
}

/// SetNoGravity (ChrActionFlag 27, physics +0x92c) forced on four windows around a jump's launch:
/// until the launch frame (ng_launch), 6-16 frames after it (ng_rise), 1-3 after it (ng_timer) and
/// 30-38 after it, falling (ng_fall). From the launch to the landing, `player::air_frame` with the
/// fall module's launch timer (LAUNCH_TIME, set on the launch frame) gives the exe's velocity on
/// every frame: the flag does nothing while the timer runs (16 frames, the launch's own
/// included), then holds (+0x160 zeroed, no air step, the fall top at the held height), and the
/// frame after a hold is a ground step (-0.163 m/s), then the air steps again.
#[test]
pub(crate) fn no_gravity_waits_for_the_launch_timer() {
    let Some(v) = load("air_ng_launch.json") else { return };
    let dt = 1.0 / 60.0;
    let ft = FALL_TYPES[0];
    let mut holds = 0;
    for s in scenarios(&v) {
        let name = s["name"].as_str().unwrap();
        let frames = s["frames"].as_array().unwrap();
        let l = launch_frame(s).expect("no launch");
        let launch = v3(&logs(&frames[l], "fall_pre")[0]["in"]);
        let (mut h, mut vy, mut left, mut held) = (launch.with_y(0.0), launch.y, LAUNCH_TIME, false);
        let mut i = l;
        while i < frames.len() {
            let f = &frames[i];
            // The flag this frame: physics +0x92c as FUN_140bcc420 read it (b92a[0] after it).
            let flag = f["b92a"][0] == 1;
            // The air step ran (the fall module's FUN_140bacc10 was called).
            let stepped = !logs(f, "fall_pre").is_empty();
            if i > l && !stepped && !flag && !held {
                break; // landed
            }
            let (now_held, nh, nvy, _) = air_frame(flag, left, held, h, vy, v3(&f["world_stick"]), ft, dt);
            assert_eq!(now_held, flag && !stepped, "{name} f{i}: held");
            let exe = v3(&f["vel"]);
            if now_held {
                holds += 1;
                let before = &frames[i - 1];
                assert_eq!(f["pos"], before["pos"], "{name} f{i}: the hold keeps the body still");
                if before["b92a"][0] == 1 {
                    // With the flag up the frame before too (+0x92a, FUN_140bcc420), +0x160 reads
                    // zero and the fall top is the held height; a flag new this frame keeps the old
                    // velocity, unused (ng_fall f41: -2.12, no move; zeroed from f42).
                    assert_eq!(exe, Vec3::ZERO, "{name} f{i}");
                    assert_eq!(f["top"], f["pos"][1], "{name} f{i}: the hold sets the fall top");
                } else {
                    assert_eq!(exe, v3(&before["vel"]), "{name} f{i}");
                }
            } else {
                assert!((nh.with_y(nvy) - exe).abs().max_element() < 1e-4, "{name} f{i}: exe {exe}, ours {}", nh.with_y(nvy));
            }
            (h, vy, held) = (nh, nvy, now_held);
            left = (left - dt).max(0.0);
            i += 1;
        }
        assert!(i - l > 40, "{name}: landed {} frames after the launch", i - l);
    }
    assert!(holds >= 10, "{holds} holds");
}

/// Walking or running off the arena's edge: the first air step's input (+0x160) is the last
/// ground frame's displacement over dt and one frame of world gravity (-9.8 dt), which is what
/// the walk-off in player.rs hands over (`Player::ground_vel`, -WORLD_GRAVITY * dt): -1.69 m/s
/// walking, -5.57 m/s running. The fall top is the last grounded height for the whole fall.
#[test]
pub(crate) fn a_walk_off_keeps_the_ground_velocity() {
    let Some(v) = load("air_jump.json") else { return };
    let mut runs = 0;
    for s in scenarios(&v).iter().filter(|s| s["name"].as_str().unwrap().starts_with("edge")) {
        let frames = s["frames"].as_array().unwrap();
        let i = frames.iter().position(|f| !logs(f, "fall_pre").is_empty()).expect("never fell");
        let (g, gg) = (&frames[i - 1], &frames[i - 2]);
        let input = v3(&logs(&frames[i], "fall_pre")[0]["in"]);
        let moved = (v3(&g["pos"]) - v3(&gg["pos"])).with_y(0.0) * 60.0;
        assert!((input.with_y(0.0) - moved).length() < 1e-3, "{}: in {input}, ground move {moved}", s["name"]);
        assert!((input.y + crate::data::WORLD_GRAVITY / 60.0).abs() < 1e-5, "{}: in {input}", s["name"]);
        assert!(moved.length() > 1.5, "{}: {moved}", s["name"]);
        let top = g["top"].as_f64().unwrap();
        for f in frames[i..].iter().take_while(|f| f["a2"][0] == 0) {
            assert_eq!(f["top"].as_f64().unwrap(), top, "{}: the fall top moved", s["name"]);
        }
        runs += 1;
    }
    assert_eq!(runs, 2);
}

/// Each jump's fall top (physics +0x854, GetFallHeight) stays at the take-off height through
/// the whole flight, apex included; RSekiro's `Player::air_top` likewise stays at the last
/// grounded height of a jump (before: the flight's highest point).
#[test]
pub(crate) fn the_fall_top_stays_at_take_off() {
    if let Some(v) = load("air_jump.json") {
        for s in scenarios(&v).iter().filter(|s| s["name"].as_str().unwrap().contains("jump")) {
            let frames = s["frames"].as_array().unwrap();
            let Some(l) = launch_frame(s) else { continue };
            let top = frames[l]["top"].as_f64().unwrap();
            assert_eq!(top, frames[l - 1]["pos"][1].as_f64().unwrap(), "{}", s["name"]);
            for f in frames[l..].iter().take_while(|f| f["a2"][0] == 0 || f["a2"][1] == 0) {
                assert_eq!(f["top"].as_f64().unwrap(), top, "{}: the fall top moved", s["name"]);
            }
        }
    }
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..10 {
        app.update();
    }
    let start = {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation.y
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut flown, mut peak) = (0, start);
    for _ in 0..120 {
        app.update();
        let world = app.world_mut();
        let (a, p, t) = world.query::<(&Actor, &Player, &Transform)>().single(world).unwrap();
        if a.airborne {
            flown += 1;
            peak = peak.max(t.translation.y);
            assert!((p.air_top - start).abs() < 1e-3, "fall top {} vs take-off {start}", p.air_top);
        }
    }
    assert!(flown > 40 && peak > start + 1.5, "flown {flown}, peak {}", peak - start);
}

/// Drops from 1 to 80 m (the fall top +0x854 put that far over the ground): under 20 m no HP is
/// lost; from 20 m (the long stiff landing, a000_200021) the loss is (int)(maxHP x h / 100):
/// 20 m 223, 22 m 246, 25 m 280, 30 m 335, 35 m 392, 39.9 m 446 of 1120; from fallDeathDist (40 m,
/// the default HitMtrlParam row) it is (int)(maxHP x 0.4) = 448 and the land is the fall-return
/// death (a000_200909), at 40, 45, 60 and 80 m alike. `player::fall_hp_loss` gives each, and the
/// landing anim follows the same heights (200020 under 20 m). The mortal runs (debug no-death
/// flags cleared) lose the same.
#[test]
pub(crate) fn fall_damage_matches_the_exe() {
    let dist = crate::terrain::fall_death_dist(None).unwrap();
    let mut n = 0;
    for name in ["air_drops_1_2_2.9_3_4.9_5_10_15_19.9_20_22.json", "air_drops_25_30_35_39.9_40_45_60_80.json"] {
        let Some(v) = load(name) else { return };
        for d in v["drops"].as_array().unwrap() {
            let h = d["height"].as_f64().unwrap() as f32;
            let frames = d["frames"].as_array().unwrap();
            let last = frames.last().unwrap();
            let (hp0, hp1) = (d["hp0"][0].as_f64().unwrap() as f32, last["hp"][0].as_f64().unwrap() as f32);
            let hp_max = last["hp"][1].as_f64().unwrap() as f32;
            let landed = frames.iter().map(|f| f["anim"].as_i64().unwrap()).last().unwrap();
            let ours = fall_hp_loss(hp_max, h, dist);
            assert_eq!(ours.map_or(0.0, |(loss, _)| loss), hp0 - hp1, "{h} m: HP {hp0} -> {hp1}");
            let lethal = ours.is_some_and(|(_, lethal)| lethal);
            let want = if lethal { 200909 } else if h >= crate::player::FALL_DAMAGE_MIN { 200021 } else { 200020 };
            assert_eq!(landed, want, "{h} m: landing anim");
            n += 1;
        }
    }
    assert_eq!(n, 24);
}

/// The land continuation of an air slash (HKS StartTime: 50308000 landing at 0.733 s ->
/// 50308050) dispatches its first TAE window from the start time on, (t, t + dt), not (0, t):
/// the exe's selector runs a preUpdate before the first update, which `Actor::start_at`
/// (prev_t = t) mirrors.
#[test]
pub(crate) fn a_land_continuation_starts_its_window_at_the_start_time() {
    let Some(v) = load("air_art.json") else { return };
    let mut seen = 0;
    for s in scenarios(&v) {
        let frames = s["frames"].as_array().unwrap();
        let Some(i) = frames.iter().position(|f| logs(f, "tae").iter().any(|l| l["id"] == 50308050)) else { continue };
        let first = logs(&frames[i], "tae").into_iter().find(|l| l["id"] == 50308050).unwrap();
        let air = logs(&frames[i - 1], "tae").into_iter().find(|l| l["id"] == 50308000).unwrap();
        let (prev, cur, air_t) = (first["prev"].as_f64().unwrap(), first["cur"].as_f64().unwrap(), air["cur"].as_f64().unwrap());
        assert!(prev > 0.7 && (prev - air_t).abs() < 1e-3, "{}: first window ({prev}, {cur}) after the air clip at {air_t}", s["name"]);
        assert!((cur - prev - 1.0 / 60.0).abs() < 1e-5);
        seen += 1;
    }
    assert!(seen >= 2, "{seen} continuations");
    let mut a = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
    a.start_at(0.733);
    assert_eq!((a.prev_t, a.t), (0.733, 0.733));
}
