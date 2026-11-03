//! The deathblow camera: Wolf's deathblow clips drive the camera through their TAE (155 slot
//! 41 with CameraParam 4000, 153 distance overrides, 151 look-at on his camera bone's dummy
//! 142), and it hands back to the normal camera afterwards (`docs/DEATHBLOW.md`, "Camera").
use std::path::{Path, PathBuf};

use glam::Vec3;
use sekiro_sim::camera::CameraDirector;
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::tae::TaeDb;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    let ok = cache.join("anim/c0000/a200_510000.bin").is_file()
        && cache.join("anim/c0000/a221_510000.bin").is_file()
        && cache.join("params/CameraSetParam.json").is_file();
    if !ok {
        eprintln!("skipping: Wolf's deathblow clips or the camera params are not extracted");
    }
    ok.then_some(cache)
}

/// Plays `clip` for `seconds`, then nothing for `after` seconds; returns per step (time,
/// slot, distance, look-at dummy).
fn drive(cache: &Path, clip: &str, seconds: f32, after: f32) -> Vec<(f32, i32, f32, Option<i32>)> {
    let tae = TaeDb::load_from_cache(cache, "c0000");
    let mut cam = CameraDirector::load(cache);
    let mut out = Vec::new();
    let steps = ((seconds + after) / DT) as usize;
    for i in 0..steps {
        let t = i as f32 * DT;
        let playing = (t < seconds).then_some((clip, t));
        cam.update(playing, &tae, DT);
        out.push((t, cam.slot, cam.distance, cam.look_at.map(|l| l.dummy)));
    }
    out
}

#[test]
fn soldier_deathblow_closes_in_and_returns() {
    let Some(cache) = cache() else { return };
    // a200_510000 (2.67 s): slot 41 for 0 to 1.6 s; distance 4.0, 3.0, then 1.5 (0.867 to
    // 1.567 s at rate 1), then back to 4.0 with SlowStart (1.567 to 2.0 s).
    let track = drive(&cache, "a200_510000", 2.67, 4.0);
    let slot_at = |t: f32| track.iter().find(|s| s.0 >= t).unwrap().1;
    assert_eq!(slot_at(0.5), 41);
    assert_eq!(slot_at(2.0), 1);
    let closest = track
        .iter()
        .filter(|s| s.0 < 2.0)
        .map(|s| s.2)
        .fold(f32::MAX, f32::min);
    assert!(closest < 2.0, "closest {closest:.2} m");
    let at = |t: f32| track.iter().find(|s| s.0 >= t).unwrap().2;
    assert!(
        at(1.3) < at(0.5),
        "closing in: {:.2} then {:.2}",
        at(0.5),
        at(1.3)
    );
    // Back to the normal 4.5 m afterwards, no look-at at any point in this clip.
    let end = track.last().unwrap();
    assert!((end.2 - 4.5).abs() < 0.15, "settled at {:.2} m", end.2);
    assert!(track.iter().all(|s| s.3.is_none()));
}

#[test]
fn owl_deathblow_looks_at_wolfs_camera_bone() {
    let Some(cache) = cache() else { return };
    // a221_510000: TAE 151 on dummy 142 from 0 to 2.5 s, slot 41 to 3.6 s, distance 2.0 / 3.5 /
    // 2.0 / 2.5 m.
    let track = drive(&cache, "a221_510000", 5.1, 4.0);
    let look = |t: f32| track.iter().find(|s| s.0 >= t).unwrap().3;
    assert_eq!(look(1.0), Some(142));
    assert_eq!(look(2.6), None);
    let at = |t: f32| track.iter().find(|s| s.0 >= t).unwrap().2;
    assert!(at(2.4) < 3.0, "close during the blow: {:.2}", at(2.4));
    let end = track.last().unwrap();
    assert_eq!(end.1, 1);
    assert!((end.2 - 4.5).abs() < 0.2, "settled at {:.2} m", end.2);
}

#[test]
fn look_limits_clamp_the_camera() {
    let Some(cache) = cache() else { return };
    let tae = TaeDb::load_from_cache(&cache, "c0000");
    let mut cam = CameraDirector::load(&cache);
    cam.update(Some(("a221_510000", 0.5)), &tae, DT);
    // LookUpLimit 0, LookDownLimit 50 around a dummy straight ahead and level: a camera looking
    // up from below (pitch -0.5) is pulled to the lower limit 0, yaw is free (-180..180).
    let (yaw, pitch) = cam.clamp_look(1.0, -0.5, Vec3::NEG_Z, DT);
    assert!((yaw - 1.0).abs() < 1e-4);
    assert!(pitch.abs() < 1e-3, "pitch {pitch}");
    let (_, pitch) = cam.clamp_look(1.0, 1.2, Vec3::NEG_Z, DT);
    assert!((pitch - 50f32.to_radians()).abs() < 1e-3, "pitch {pitch}");
    // No look-at: unchanged.
    cam.update(None, &tae, DT);
    assert_eq!(cam.clamp_look(1.0, -0.5, Vec3::NEG_Z, DT), (1.0, -0.5));
}

type DeathblowFrame = (String, i32, f32, Option<i32>, Option<f32>);

/// Breaks the enemy's posture and starts a deathblow; then steps until the throw is over and
/// the camera has settled. Returns per step: (Wolf's clip, slot, distance, look-at dummy,
/// dummy 142's distance from Wolf's body).
fn duel_deathblow(cache: &Path, spec: EnemySpec) -> Vec<DeathblowFrame> {
    let mut d = Duel::load_with(cache, 1.6, spec).unwrap();
    d.enemy_active = false;
    d.events = None;
    let mut out = Vec::new();
    let mut started = None;
    for i in 0..1800 {
        if started.is_none() {
            let p = &mut d.rules.fighters[1].posture;
            p.remaining = p.remaining.min(1);
        }
        let input = InputFrame {
            buttons: Buttons {
                attack: started.is_none() && i % 20 == 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let r = d.step(&input, DT);
        if r.deathblow.started.is_some() {
            started = Some(i);
        }
        if started.is_none() {
            continue;
        }
        let wolf = Vec3::from_array(d.player.body.position);
        let cam_bone = d
            .dummy_position(0, 142)
            .map(|p| Vec3::from_array(p).distance(wolf));
        let clip = d
            .player
            .behavior
            .runtime
            .main_clip()
            .map(|c| c.animation.clone())
            .unwrap_or_default();
        let c = &d.camera_director;
        out.push((
            clip,
            c.slot,
            c.distance,
            c.look_at.map(|l| l.dummy),
            cam_bone,
        ));
        if d.deathblow.is_none() && !c.active() && out.len() > 60 {
            return out;
        }
    }
    panic!("no deathblow, or the camera never settled");
}

#[test]
fn soldier_deathblow_in_the_duel_drives_the_camera() {
    let Some(cache) = cache() else { return };
    let track = duel_deathblow(&cache, EnemySpec::soldier());
    let body: Vec<_> = track
        .iter()
        .filter(|s| s.0.starts_with("a200_51"))
        .collect();
    assert!(!body.is_empty(), "Wolf never played a body clip");
    assert!(body.iter().any(|s| s.1 == 41), "slot 41 during the throw");
    let closest = body.iter().map(|s| s.2).fold(f32::MAX, f32::min);
    assert!(closest < 2.6, "closest {closest:.2} m");
    // Handed back: default slot, no look-at, normal distance.
    let end = track.last().unwrap();
    assert_eq!((end.1, end.3), (1, None));
    assert!((end.2 - 4.5).abs() < 0.1);
}

#[test]
fn owl_deathblow_in_the_duel_follows_the_camera_bone() {
    let Some(cache) = cache() else { return };
    let track = duel_deathblow(&cache, EnemySpec::owl());
    let looking: Vec<_> = track.iter().filter(|s| s.3 == Some(142)).collect();
    assert!(!looking.is_empty(), "no look-at in the Owl's deathblow");
    assert!(looking.iter().all(|s| s.0 == "a221_510000"));
    // Dummy 142 sits on the animated camera bone `z_dummy_camera_P`: it resolves and moves.
    let d: Vec<f32> = looking.iter().filter_map(|s| s.4).collect();
    assert_eq!(d.len(), looking.len(), "dummy 142 did not resolve");
    let (lo, hi) = d
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    assert!(
        hi - lo > 0.2,
        "the camera bone did not move: {lo:.2}..{hi:.2}"
    );
    let end = track.last().unwrap();
    assert_eq!((end.1, end.3), (1, None));
}
