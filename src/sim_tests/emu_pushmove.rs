//! Characters pushing characters while both move, against the exe (row E14, tools/emu-cases
//! `pushmove`: whole world frames, the General's forced lunge 3000 at Wolf, who stands, walks at
//! him, backs off or crosses; the General's chr frame first, then Wolf's, the broadphase brought
//! up after each). Skipped when extracted/emu/pushmove_*.json are missing.

use bevy::prelude::Vec3;

use crate::actor::{pair_push, ProxyBody, PLAYER_BODY_RADIUS, PLAYER_PROXY_HEIGHT, PROXY_LIFT};

fn run(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("pushmove_{name}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    Some(v["scenarios"][0].clone())
}

fn v3(v: &serde_json::Value) -> Vec3 {
    Vec3::new(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32)
}

/// The General's proxy (NpcParam 10200000 hitRadius 0.5 / hitHeight 2.0: proxy +0x90 / +0xa0)
/// and Wolf's, at rest at their feet.
fn bodies(g: Vec3, w: Vec3) -> (ProxyBody, ProxyBody) {
    (ProxyBody { feet: g.y, lift: 0.0, radius: 0.5, height: 2.0 }, ProxyBody { feet: w.y, lift: 0.0, radius: PLAYER_BODY_RADIUS, height: PLAYER_PROXY_HEIGHT })
}

const LUNGES: [&str; 5] = ["lunge_still", "lunge_in", "lunge_away", "lunge_away_slow", "lunge_side"];

/// A character whose proxy steps is lifted physics +0x188 (0.3 m) over its last feet for its
/// integrate, an enemy as Wolf (the pre-step FUN_140bcac10 for any chr; before, RSekiro lifted
/// only Wolf and kept a moving enemy at his feet).
#[test]
fn a_stepping_enemy_is_lifted_like_wolf() {
    let mut n = 0;
    for name in LUNGES {
        let Some(s) = run(name) else { return };
        let fr = s["frames"].as_array().unwrap();
        for k in 1..fr.len() {
            for (calls, feet) in [(&fr[k]["g_in_g"], &fr[k - 1]["g"]), (&fr[k]["w_in_w"], &fr[k]["w_mid"])] {
                for c in calls.as_array().unwrap() {
                    let lift = v3(&c["at"]).y - v3(feet).y;
                    assert!((lift - PROXY_LIFT).abs() < 2e-3, "{name} f{k}: lifted {lift}");
                    n += 1;
                }
            }
        }
    }
    assert!(n > 200, "only {n} integrates");
}

/// Each step as the exe takes it, through `actor::pair_push`: the General's frame (his request at
/// integrate x dt from his last feet, Wolf where his last frame left him), then Wolf's (his request
/// from there, the General where his step left him). The mover is stopped at the other's body,
/// the other is never shoved: Wolf standing, backing off or crossing keeps his own move to 1e-4,
/// and the stepper's end lies within 3 mm of the exe's on 349 of 350 steps (30 against the
/// other); the one left is the General's 3.9 m/s arrival at a crossing Wolf, 7 mm (Havok slides
/// the request along the contact where `pair_push` pushes out along the line between them).
#[test]
fn pushes_between_moving_characters_match_the_exe() {
    let (mut held, mut steps, mut worst, mut bad) = (0, 0, 0.0f32, Vec::new());
    for name in LUNGES {
        let Some(s) = run(name) else { return };
        let dt = s["dt"].as_f64().unwrap() as f32;
        let fr = s["frames"].as_array().unwrap();
        for k in 1..fr.len() {
            let (g0, w0) = (v3(&fr[k - 1]["g"]), v3(&fr[k]["w_mid"]));
            let (g1, w1) = (v3(&fr[k]["g"]), v3(&fr[k]["w"]));
            // The General's step (when his proxy integrated this frame).
            if let Some(c) = fr[k]["g_in_g"].as_array().unwrap().first() {
                let v = v3(&c["v"]).with_y(0.0);
                let free = g0 + v * dt;
                let (rg, rw) = bodies(free, w0);
                // In his step only he moves.
                let ours = match pair_push(free, w0, rg, rw, 1.0, 0.0, false, false) {
                    Some((dg, dw)) => {
                        assert_eq!(dw, Vec3::ZERO, "{name} f{k}: Wolf shoved by the General's step");
                        held += 1;
                        free + dg
                    }
                    None => free,
                };
                let off = (ours - g1).with_y(0.0).length();
                worst = worst.max(off);
                if off > 0.003 {
                    bad.push(format!("{name} f{k} General {off:.4} (in contact: {})", ours != free));
                }
                steps += 1;
            }
            // Wolf's step against the General where his step left him.
            if let Some(c) = fr[k]["w_in_w"].as_array().unwrap().first() {
                let v = v3(&c["v"]).with_y(0.0);
                let free = w0 + v * dt;
                let (rg, rw) = bodies(g1, free);
                let ours = match pair_push(free, g1, rw, rg, 1.0, 0.0, false, false) {
                    Some((dw, dg)) => {
                        assert_eq!(dg, Vec3::ZERO, "{name} f{k}: the General shoved by Wolf's step");
                        held += 1;
                        free + dw
                    }
                    None => free,
                };
                let off = (ours - w1).with_y(0.0).length();
                worst = worst.max(off);
                if off > 0.003 {
                    bad.push(format!("{name} f{k} Wolf {off:.4} (in contact: {})", ours != free));
                }
                steps += 1;
            }
        }
    }
    println!("{steps} steps, {held} against the other, worst {worst:.4} m; over 3 mm:
{}", bad.join("
"));
    assert!(worst < 0.008 && bad.len() <= 1, "{}", bad.join("
"));
    assert!(held >= 25, "only {held} steps in contact");
}

/// Enemy against enemy: the same lunge at a standing Wolf whose body carries another kind's
/// collision filter info (hknpBody +0x6c). With the c1150 dogs' (0x2000066) the General stops at
/// 0.950 m as against Wolf's own; with the humanoid enemies' (0x2000069, his own proxy body's and
/// every humanoid's in the snapshot) his proxy passes through. So a humanoid's map proxy does not
/// meet another humanoid's: the exe's push between two humanoid enemies is not the proxy (gap: the
/// NpcParam chrHitRadius / chrHitHeight "capsule against characters" is not traced; RSekiro
/// pushes enemy pairs with the proxy rule).
#[test]
fn enemy_bodies_meet_the_proxy_by_their_filter() {
    let Some(dog) = run("lunge_dog_body") else { return };
    let Some(humanoid) = run("lunge_enemy_body") else { return };
    let closest = |s: &serde_json::Value| s["frames"].as_array().unwrap().iter().map(|f| (v3(&f["g"]) - v3(&f["w"])).with_y(0.0).length()).fold(f32::MAX, f32::min);
    assert!((closest(&dog) - 0.95).abs() < 0.002, "dog filter: {}", closest(&dog));
    assert!(closest(&humanoid) < 0.2, "humanoid filter: {}", closest(&humanoid));
}
