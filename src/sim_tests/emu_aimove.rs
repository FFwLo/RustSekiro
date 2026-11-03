//! How the enemy AI's goals become moves, against the exe (topic aimove).

use crate::stealth::SearchRing;

/// The sight search scheduler FUN_140614be0 (run in tools/emu-cases `aimove_sched` on the
/// snapshot's ring DAT_143d547d8 +0x28) on 400 rings: bucket counts 0-4, runs of full buckets,
/// cursors, intervals 0.2-0.4 s, cap 3, every 25th with the no-limit byte (+0x69) set. The
/// stretched interval and the bucket it took must match `SearchRing::place`. Skipped when
/// extracted/emu/aimove_sched.json is missing.
#[test]
fn search_ring_matches_the_exe() {
    let Ok(text) = std::fs::read_to_string(crate::paths::emu().join("aimove_sched.json")) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["snapshot"]["cap"][0].as_u64(), Some(3), "the snapshot's cap");
    let mut n = 0;
    for c in v["cases"].as_array().unwrap() {
        let counts: Vec<u8> = c["counts"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u8).collect();
        let mut ring = SearchRing { counts: counts.clone().try_into().unwrap(), cursor: c["cursor"].as_u64().unwrap() as usize, cap: 3 };
        let secs = c["secs"].as_f64().unwrap() as f32;
        let want = c["xmm0"].as_f64().unwrap() as f32;
        if c["nolimit"] == 1 {
            // +0x69 set (0 in the game): the bucket is bumped and the interval kept.
            assert_eq!(want, secs);
            continue;
        }
        let got = ring.place(secs);
        assert!((got - want).abs() < 1e-6, "{c}: {got} vs {want}");
        let bumped: Vec<usize> = (0..100).filter(|&k| ring.counts[k] != counts[k]).collect();
        let exe: Vec<usize> = c["bumped"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
        assert_eq!(bumped, exe, "{c}");
        n += 1;
    }
    assert!(n > 300);
}

/// The ring in play: one enemy's searches never stretch, a fourth search on a frame three
/// others already took moves one bucket (1/30 s) on.
#[test]
fn search_ring_stretches_only_crowded_frames() {
    let mut ring = SearchRing::default();
    for _ in 0..1000 {
        assert_eq!(ring.place(0.3), 0.3);
        for _ in 0..18 {
            ring.advance();
        }
    }
    let mut ring = SearchRing::default();
    let got: Vec<f32> = (0..4).map(|_| ring.place(0.3)).collect();
    assert_eq!(&got[..3], &[0.3; 3]);
    assert!((got[3] - (0.3 + 1.0 / 30.0)).abs() < 1e-6);
}

/// The aimove frame runs (tools/emu-cases `aimove`: whole world frames, the General's AI on,
/// Wolf 3-16 m away in front, beside, behind, walking off; the forced attack 3000), or None.
fn frame_runs() -> Option<Vec<serde_json::Value>> {
    let files = std::fs::read_dir(crate::paths::emu()).ok()?;
    let mut runs: Vec<serde_json::Value> = Vec::new();
    for f in files.flatten() {
        let n = f.file_name().to_string_lossy().to_string();
        if n.starts_with("aimove_frames_") && n.ends_with(".json") {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
            runs.extend(v["scenarios"].as_array().unwrap().iter().cloned());
        }
    }
    (!runs.is_empty()).then_some(runs)
}

/// The General's turn while his goals run: every frame the exe's turn was not done (the AI
/// angle minus his yaw, M +0x20, beyond what one frame turns), his yaw moved by `ai_turn_rate` x
/// dt, his clip's TAE SetTurnSpeed at that time (run 240, walks 120, battle start 135 / 0,
/// attacks by window). Frames of clips without one (turn clips: root motion), in a
/// FixedRotationDirection (703) window or next to a SetTurnSpeed window edge (the clip time is
/// counted in frames here) are left out. Skipped when no extracted/emu/aimove_frames_*.json.
#[test]
fn general_turn_rates_match_the_exe() {
    let Some(runs) = frame_runs() else { return };
    let app = super::part01::sim_tests_app();
    let combat = app.world().resource::<crate::data::Combat>();
    let d = &combat.kind(0).data;
    let wrap = |x: f32| (x + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let (mut n, mut bad, mut by_rate) = (0, Vec::new(), std::collections::BTreeMap::<i32, usize>::new());
    for sc in &runs {
        let dt = f(&sc["dt"]);
        let fr = sc["frames"].as_array().unwrap();
        let mut start = 0;
        for i in 1..fr.len() {
            let anim = fr[i]["ganim"].as_i64().unwrap();
            if anim != fr[i - 1]["ganim"].as_i64().unwrap() {
                start = i;
                continue;
            }
            let key = format!("a000_{anim:06}");
            let t = (i - start) as f32 * dt;
            let window = |t: f32| d.turn_speed(&key, t, true);
            let Some(rate) = window(t) else { continue };
            if [t - 3.0 * dt, t - 2.0 * dt, t - dt, t + dt].iter().any(|&u| window(u) != Some(rate)) || d.events_at(&key, t).any(|e| e.kind == 703) {
                continue;
            }
            let want = wrap(f(&fr[i - 1]["turn"][1]));
            let moved = wrap(f(&fr[i]["gyaw"]) - f(&fr[i - 1]["gyaw"]));
            if want.abs() <= rate.to_radians() * dt + 0.02 {
                continue;
            }
            n += 1;
            *by_rate.entry(rate as i32).or_default() += 1;
            let ours = crate::enemy::ai_turn_rate(d, &key, t, 135.0).to_radians() * dt * want.signum();
            if (ours - moved).abs() > 0.002 {
                bad.push(format!("{} f{i} {key} t {t:.3}: exe {moved:.4} ours {ours:.4} (asked {want:.3})", sc["name"]));
            }
        }
    }
    println!("{n} turn-limited frames by SetTurnSpeed {by_rate:?}");
    assert!(bad.is_empty(), "{} of {n}:\n{}", bad.len(), bad.join("\n"));
    assert!(n >= 40, "only {n} frames");
}

/// A moving enemy against Wolf (aimove `push_ledge` / `push_ledge_far`: the General's forced
/// attack 3000 lunging at Wolf from 1.6 / 2.6 m, then his 1040 walking in; physics +0xb10 mode 2,
/// his proxy integrated with the clip's velocity every frame; Wolf idle where the snapshot's
/// broadphase has him). The General stops at `actor::contact_flat` of the two proxies (Wolf's
/// lifted 0.4 x 1.5 capsule, the General's NpcParam 0.5 x 2.0 at his feet, keepDistance 0.05:
/// 0.950 m) and Wolf is not shoved, as `separate` gives the one that walked in all the push.
#[test]
fn moving_enemy_stops_against_wolf_like_the_exe() {
    let Some(runs) = frame_runs() else { return };
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mut held = 0;
    for sc in runs.iter().filter(|s| s["name"].as_str().unwrap().starts_with("push_ledge")) {
        let fr = sc["frames"].as_array().unwrap();
        let w0 = &fr[0]["w"];
        for x in fr {
            let (g, w) = (&x["g"], &x["w"]);
            let wolf = crate::actor::ProxyBody { feet: f(&w[1]), lift: crate::actor::PROXY_LIFT, radius: crate::actor::PLAYER_BODY_RADIUS, height: crate::actor::PLAYER_PROXY_HEIGHT };
            let general = crate::actor::ProxyBody { feet: f(&g[1]), lift: 0.0, radius: 0.5, height: 2.0 };
            let min = crate::actor::contact_flat(wolf, general).unwrap();
            let d = (f(&g[0]) - f(&w[0])).hypot(f(&g[2]) - f(&w[2]));
            assert!(d >= min - 0.001, "{} {x}: {d} inside {min}", sc["name"]);
            held += (d < min + 0.004) as usize;
            assert!((f(&w[0]) - f(&w0[0])).hypot(f(&w[2]) - f(&w0[2])) < 1e-4, "Wolf was shoved");
        }
    }
    assert!(held >= 10, "only {held} frames against Wolf");
}
