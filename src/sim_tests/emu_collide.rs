//! What the proxy pilot left open, against the exe (topic collide, docs/kb/movement.md "Bodies,
//! steps and the floor follow against the exe"): tools/emu-cases `collide_push_cases` walks
//! Wolf's proxy into the idle c1150 dog on the Naomori-arena snapshot (his whole chr frame
//! FUN_140a4b570, the dog's first; the requested velocity put in at hknpCharacterProxy::integrate
//! FUN_1418f8b60), `collide_stepwalk` across the arena's steps and down off them.

use super::*;
use crate::actor::{contact_flat, ProxyBody, PLAYER_BODY_RADIUS, PLAYER_PROXY_HEIGHT, PROXY_LIFT};

fn floats(x: &serde_json::Value) -> Vec<f32> {
    x.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect()
}

fn load(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::root().join("extracted/emu").join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// Wolf walking into an idle enemy stops where `actor::contact_flat` says: each frame RSekiro
/// moves Wolf from the exe's last position by the request and pushes him out of the dog along
/// the line between them (`separate`, Wolf the one that walked in), with the exe's heights.
#[test]
pub(crate) fn wolf_stops_against_an_idle_enemy_like_the_exe() {
    let Some(v) = load("collide_push.json") else { return };
    let mut report = Vec::new();
    let mut failures = Vec::new();
    let mut compared = 0;
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        let req = floats(&s["req"]);
        // The exe's proxies: Wolf's capsule 0.4 x 1.5 lifted 0.3 (physics +0x188), the dog's
        // NpcParam 11500000 hitRadius 0.5 / hitHeight 1.0 (proxy +0x90 / +0xa0).
        let (wb, eb) = (floats(&s["wolf_box"]), floats(&s["enemy_box"]));
        assert!((wb[4] - PLAYER_BODY_RADIUS).abs() < 1e-6 && (wb[5] - PLAYER_PROXY_HEIGHT).abs() < 1e-6, "{name}: Wolf's proxy {wb:?}");
        assert!((floats(&s["lift"])[0] - PROXY_LIFT).abs() < 1e-6, "{name}: lift");
        let (er, eh) = (eb[4], eb[5]);
        let mut prev = Vec3::from_slice(&floats(&s["start"]));
        // heading_45's start cast found a ledge 1.7 m over the dog's ground: Wolf fell onto the
        // dog and rolled off him (an airborne contact, not the walk): left out.
        let first = &s["frames"][0];
        if first["wolf"][1].as_f64().unwrap() - first["enemy"][1].as_f64().unwrap() > 0.8 {
            report.push(format!("{name}: starts in the air over the dog, skipped"));
            continue;
        }
        let (mut worst, mut worst_old, mut rest) = (0.0f32, 0.0f32, None);
        for fr in s["frames"].as_array().unwrap() {
            assert_eq!(fr["enemy_mode"].as_u64(), Some(1), "{name}: the dog took a proxy step");
            let exe = Vec3::from_slice(&floats(&fr["wolf"]));
            let dog = Vec3::from_slice(&floats(&fr["enemy"]));
            let wolf = ProxyBody { feet: exe.y, lift: PROXY_LIFT, radius: PLAYER_BODY_RADIUS, height: PLAYER_PROXY_HEIGHT };
            let enemy = ProxyBody { feet: dog.y, lift: 0.0, radius: er, height: eh };
            let push = |min: Option<f32>| {
                let p = Vec2::new(prev.x + req[0] * dt, prev.z + req[1] * dt);
                let d = p - dog.xz();
                match min {
                    Some(m) if d.length() < m => dog.xz() + d.normalize_or(Vec2::X) * m,
                    _ => p,
                }
            };
            let own = push(contact_flat(wolf, enemy));
            // Before: the radii summed (0.9 m) whenever the feet-to-top spans met.
            let old = push(Some(PLAYER_BODY_RADIUS + er));
            let free = Vec2::new(prev.x + req[0] * dt, prev.z + req[1] * dt);
            let flat = (exe.xz() - dog.xz()).length();
            prev = exe;
            // Frames the rock stops or turns (no push either side, the exe off the free move)
            // are the map's, not the bodies': skipped.
            if own == free && (exe.xz() - free).length() > 0.002 {
                continue;
            }
            compared += 1;
            worst = worst.max((own - exe.xz()).length());
            worst_old = worst_old.max((old - exe.xz()).length());
            if let Some(m) = contact_flat(wolf, enemy).filter(|m| flat < m + 0.002) {
                rest = Some((flat, m, exe.y - dog.y));
            }
        }
        report.push(format!("{name}: worst {worst:.4} m (before {worst_old:.4}); in contact (exe flat, RSekiro, dy) {rest:.4?}"));
        // The arrival frame is the worst (4 mm): the exe solves the move against the contact
        // plane, RSekiro pushes the end point out along the line; at rest they agree to 0.3 mm.
        if worst > 0.005 {
            failures.push(format!("{name}: {worst:.4}"));
        }
    }
    println!("collide push: {compared} frames\n{}", report.join("\n"));
    assert!(compared > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The post-step's floor follow: Wolf put h over flat ground stands on it after one frame (physics
/// +0xa2 set) while h is within `map::FLOOR_FOLLOW`, and falls past it (`collide_snap`).
#[test]
pub(crate) fn floor_follow_depth_matches_the_exe() {
    let Some(v) = load("collide_snap.json") else { return };
    let (mut held, mut fell) = (0.0f32, f32::MAX);
    for c in v["cases"].as_array().unwrap() {
        let h = c["h"].as_f64().unwrap() as f32;
        let f = &c["frames"][0];
        if f["a2"].as_u64() == Some(1) {
            assert!(f["y"].as_f64().unwrap().abs() < 0.005, "h {h}: grounded off the floor {f}");
            held = held.max(h);
        } else {
            fell = fell.min(h);
        }
    }
    println!("floor follow: held from {held} m, fell from {fell} m; RSekiro {}", crate::map::FLOOR_FOLLOW);
    assert!(held < fell && (held..fell).contains(&crate::map::FLOOR_FOLLOW), "held {held} fell {fell}");
}

/// Walking off a sharp edge (`collide_stepwalk` down_*): the drop, from the ground profile along
/// the walk, decides it: within FLOOR_FOLLOW he stays grounded the whole way (0.21, 0.29, 0.55 m),
/// past it he falls (0.62, 0.68 m) once 0.15-0.2 m past the edge.
#[test]
pub(crate) fn walking_off_an_edge_falls_past_the_floor_follow_like_the_exe() {
    let Some(v) = load("collide_steps.json") else { return };
    let mut n = 0;
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        if !name.starts_with("down") {
            continue;
        }
        let y0 = s["start"][1].as_f64().unwrap() as f32;
        let prof: Vec<(f32, f32)> = s["profile"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap() as f32, p[1].as_f64().map_or(f32::NEG_INFINITY, |y| y as f32 - y0))).collect();
        // The first sharp drop (over 0.15 m in 0.05 m) and the floor 0.15 m on.
        let Some(k) = (1..prof.len()).find(|&k| prof[k - 1].1 - prof[k].1 > 0.15) else { continue };
        let drop = prof[k - 1].1 - prof[(k + 3).min(prof.len() - 1)].1;
        let fell = s["frames"].as_array().unwrap().iter().find(|f| f["a2"][0].as_u64() == Some(0) && (f["along"].as_f64().unwrap() as f32) < prof[k].0 + 0.6);
        let ours = drop > crate::map::FLOOR_FOLLOW;
        println!("{name}: edge at {:.2} m, drop {drop:.2}: exe {}, RSekiro {}", prof[k].0, if fell.is_some() { "falls" } else { "follows" }, if ours { "falls" } else { "follows" });
        assert_eq!(fell.is_some(), ours, "{name}: drop {drop}");
        n += 1;
    }
    assert!(n > 0);
}
