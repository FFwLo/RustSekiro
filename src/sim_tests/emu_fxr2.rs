//! The FXR particle runtime against the user's own sekiro.exe (docs/kb/fxr.md, 2026-11-03 fxr2):
//! tools/emu-cases `fxr2` runs the exe's particle pool update FUN_141dd6f60 once a frame (2.5 s at
//! 30 and at 60 fps) on every particle config of the deflect sparks 252001, the hit sparks
//! 201001 / 201002, the blood 220502 and the firecracker 300071 (73 configs, emitters 300 / 399,
//! movements 55 / 60 / 65 / 105), with the real emitter and movement objects, and records every
//! particle each frame. Here RSekiro's emitter (`periodic_cap`, `periodic_emit`, the one-time
//! emitter), duration (`particle_life`) and movement (`move_particle`) step the same particles from
//! the exe's birth states. Skips when extracted/emu/fxr2_frames.json is missing.

use bevy::prelude::Vec3;
use serde_json::Value;

fn emu_json(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn v3(v: &Value) -> Vec3 {
    let a: Vec<f32> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect();
    Vec3::new(a[0], a[1], a[2])
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

/// Per frame: the emitter's count (new particles = age 0), its cap and next time; every particle's
/// death frame, and its position, direction and speed each frame it lives.
#[test]
fn fxr_particle_frames_match_the_exe() {
    use crate::fxr::{move_particle, particle_life, periodic_cap, periodic_emit, Motion, Periodic};
    let Some(v) = emu_json("fxr2_frames") else { return };
    let (mut frames, mut moves, mut bad) = (0, 0, Vec::new());
    for c in v["cases"].as_array().unwrap() {
        let cfg = &c["config"];
        let path = format!("{} {} fps", c["path"].as_str().unwrap(), c["fps"]);
        let dt = f(&c["dt"]);
        let nr = c["node_random"].as_u64().unwrap() as u32;
        let em = &cfg["emitter"];
        let one_time = em["type"].as_i64() == Some(399);
        let cap = periodic_cap(cfg, nr);
        if !one_time && cap != c["emitter_cap"].as_i64().unwrap() {
            bad.push(format!("{path}: cap exe {}, ours {cap}", c["emitter_cap"]));
        }
        let mut st = Periodic::default();
        // RSekiro's particles, by their random: (motion, age, scaled age, scale, end, life)
        let mut ours: Vec<(u32, Motion, f32, f32, f32, f32, f32)> = Vec::new();
        for (k, fr) in c["frames"].as_array().unwrap().iter().enumerate() {
            frames += 1;
            let t = f(&fr["t"]);
            let ps = fr["particles"].as_array().unwrap();
            // ageing and deaths, then the emission, then the move of the older ones
            for p in &mut ours {
                p.2 += dt;
                p.3 += dt * p.4;
            }
            ours.retain(|p| !(p.6 >= 0.0 && p.3 >= p.5));
            let live = ours.len() as i64;
            let n = if one_time { (k == 0 && live == 0) as i64 } else { periodic_emit(em, &mut st, t, nr, live, cap) };
            let born: Vec<&Value> = ps.iter().filter(|p| f(&p["age"]) == 0.0).collect();
            if n != born.len() as i64 {
                bad.push(format!("{path} frame {k}: exe made {}, ours {n}", born.len()));
                break;
            }
            if !one_time && (st.next - f(&fr["emitter_next"])).abs() > 1e-6 {
                bad.push(format!("{path} frame {k}: next emission exe {}, ours {}", fr["emitter_next"], st.next));
            }
            let pm = &cfg["particleMovement"];
            for p in &mut ours {
                moves += 1;
                move_particle(pm, &mut p.1, p.2, dt, p.0, Vec3::NEG_Y);
            }
            for b in born {
                let random = b["random"].as_u64().unwrap() as u32;
                let Some((life, scale, end)) = particle_life(&cfg["particleAttributes"], t, random) else {
                    bad.push(format!("{path} frame {k}: ours makes no particle"));
                    continue;
                };
                let speed = crate::fxr::eval_prop(&cfg["particleModifier"]["speed"], t, random)[0];
                let speed = if cfg["particleModifier"].get("speed").is_some() { speed } else { 0.0 };
                if (speed - f(&b["speed"])).abs() > 1e-4 * speed.abs().max(1.0) || (scale - f(&b["scale"])).abs() > 1e-6 {
                    bad.push(format!("{path} frame {k}: birth speed / scale exe {} {}, ours {speed} {scale}", b["speed"], b["scale"]));
                }
                ours.push((random, Motion { pos: v3(&b["pos"]), dir: v3(&b["dir"]), speed: f(&b["speed"]) }, 0.0, 0.0, scale, end, life));
            }
            if ours.len() != ps.len() {
                bad.push(format!("{path} frame {k}: exe has {} particles, ours {}", ps.len(), ours.len()));
                break;
            }
            for p in &ours {
                let Some(e) = ps.iter().find(|q| q["random"].as_u64() == Some(p.0 as u64)) else {
                    bad.push(format!("{path} frame {k}: particle {:#x} gone in the exe", p.0));
                    continue;
                };
                let (pos, dir, speed) = (v3(&e["pos"]), v3(&e["dir"]), f(&e["speed"]));
                let tol = 1e-3 * pos.length().max(1.0);
                if p.1.pos.distance(pos) > tol || p.1.dir.distance(dir) > 1e-3 || (p.1.speed - speed).abs() > 1e-3 * speed.abs().max(1.0) || (p.2 - f(&e["age"]) / f(&e["scale"])).abs() > 1e-5 {
                    bad.push(format!("{path} frame {k} particle {:#x}: exe pos {pos} dir {dir} speed {speed} age {}, ours {} {} {} {}", p.0, e["age"], p.1.pos, p.1.dir, p.1.speed, p.2));
                }
            }
        }
    }
    assert!(frames > 10_000, "only {frames} frames");
    assert!(bad.is_empty(), "{} differences ({frames} frames, {moves} moves):\n{}", bad.len(), bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
}

/// InitialDirection 0-6 (`fxr::direction_base`) against the exe's frames (FUN_141db46a0): a point
/// shape under a node turned Ry(0.7) Rx(0.4), the new particle's direction in the world
/// (attachment 0) and in the node's frame (attachment 1).
#[test]
fn fxr_initial_directions_match_the_exe() {
    let Some(v) = emu_json("fxr2_frames") else { return };
    let Some(dirs) = v["directions"].as_array() else { return };
    let m: Vec<f32> = v["node_matrix"].as_array().unwrap().iter().map(f).collect();
    // the node's axes are the matrix's columns; as rows for `rows_quat`
    let node = crate::fxr::rows_quat([[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]]);
    let mut bad = Vec::new();
    for d in dirs {
        let (mode, attach) = (d["mode"].as_i64().unwrap(), d["attachment"].as_i64().unwrap());
        let local = crate::fxr::direction_base(mode, node, None) * Vec3::Z;
        let ours = if attach == 0 { node * local } else { local };
        let e = v3(&d["dir"]);
        let want = Vec3::new(-e.x, e.y, e.z);
        if ours.distance(want) > 1e-4 {
            bad.push(format!("mode {mode} attachment {attach}: exe {want}, ours {ours}"));
        }
    }
    assert_eq!(dirs.len(), 14);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
