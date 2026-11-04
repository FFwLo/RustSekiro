//! A bullet's flight and hit timing against the user's own sekiro.exe: tools/emu-cases `bullets`
//! runs whole world frames on the Naomori-arena snapshot (the bullet task FUN_140a74560 ->
//! BulletMan step FUN_140b1c050, the hit manager step FUN_1409a0dc0, the WorldChrMan tasks, the
//! General's and Wolf's chr frames) and throws the shuriken's Bullet 700000 from Wolf's hand at
//! the General (AI off). kb/frame-coverage.md "A bullet's hit timing". Skips when
//! extracted/emu/bullets_*.json is missing.

use super::*;
use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("bullets_{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn v3(v: &Value) -> Vec3 {
    let a = v.as_array().unwrap();
    Vec3::new(a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32, a[2].as_f64().unwrap() as f32)
}

/// What one run shows, frame k after the spawn (k = 1 is the bullet task after it): the first
/// bullet's metres from where it was launched while it flies, the frames whose hit took posture
/// or HP, the first frame the General's reaction clip has stepped, the frame the first bullet
/// ends in.
#[derive(Debug, PartialEq)]
struct Run {
    flown: Vec<f32>,
    hits: Vec<usize>,
    reaction: Option<usize>,
    gone: Option<usize>,
}

/// The exe's run and how far from its launch point the first bullet stopped on the General.
fn exe_run(v: &Value) -> (Run, f32) {
    let spawn = v["spawn_frame"].as_u64().unwrap() as usize;
    let frames = v["frames"].as_array().unwrap();
    let bullet = v["bullet"].as_i64().unwrap();
    let slot = |k: usize| frames[spawn + k]["slots"].as_array().unwrap().iter().find(|s| s["bullet"].as_i64() == Some(bullet)).cloned();
    // Launched from its first position (the first update shifts it 9 mm off the hand: gap).
    let start = v3(&slot(1).unwrap()["pos"]);
    let anim0 = frames[spawn]["general"]["tae"].as_array().and_then(|a| a.first()).map(|c| c[0].as_i64().unwrap());
    let mut run = Run { flown: Vec::new(), hits: Vec::new(), reaction: None, gone: None };
    let mut stop = f32::NAN;
    for k in 1..frames.len() - spawn {
        let fr = &frames[spawn + k];
        // Flags bit 4: it ended in that frame (its slot is freed in the next bullet step).
        match slot(k) {
            Some(s) if run.gone.is_none() && s["flags"].as_u64().unwrap() & 4 == 0 => run.flown.push((v3(&s["pos"]) - start).length()),
            Some(s) if run.gone.is_none() => {
                run.gone = Some(k);
                stop = (v3(&s["pos"]) - start).length();
            }
            None if run.gone.is_none() => run.gone = Some(k),
            _ => {}
        }
        if fr["hits"].as_array().unwrap().iter().any(|h| h["hp_dmg"].as_i64().unwrap_or(0) > 0 || h["posture_dmg"].as_i64().unwrap_or(0) > 0) {
            run.hits.push(k);
        }
        if run.reaction.is_none() {
            let stepped = fr["general"]["tae"].as_array().unwrap().iter().any(|c| {
                let id = c[0].as_i64().unwrap();
                Some(id) != anim0 && !(790000..800000).contains(&id) && (c[2].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-3
            });
            if stepped {
                run.reaction = Some(k);
            }
        }
    }
    (run, stop)
}

/// Ours: the General where app_geom puts him (AI off, Wolf far behind), Bullet 700000 thrown
/// level at his chest (1.5 m) from `reach` metres before the point where it first touches his
/// hurt capsules (our shapes and pose are not the exe's: the snapshot's General was hit 1.7 -
/// 2.1 m up on a slightly rising throw, so the run is placed by that distance), spawned as if at
/// the end of the spawn frame (prosthetic::spawn_row), then `frames` ticks.
fn sim_run(bullet: i64, reach: f32, frames: usize) -> Run {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -40.0);
        let e = boss_in_play(world);
        world.get_mut::<Enemy>(e).unwrap().ai_off = true;
    }
    // Let the poses settle before the throw.
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    let spec = world.resource::<Combat>().player.bullet_rows.get(&bullet.to_string()).expect("Bullet row").clone();
    let e = boss_in_play(world);
    let yaw = world.get::<Actor>(e).unwrap().yaw;
    let feet = world.get::<Transform>(e).unwrap().translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    let f = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    // Where a level line at 1.5 m from 10 m ahead first touches him.
    let (p0, p1) = (feet + f * 10.0 + Vec3::Y * 1.5, feet - f * 2.0 + Vec3::Y * 1.5);
    let hurt = world.get::<crate::anim::Hurtboxes>(e).expect("hurt capsules").0.clone();
    let s = hurt
        .iter()
        .filter_map(|&(a, b, r)| {
            let (pa, pb) = (world.get::<GlobalTransform>(a)?.translation(), world.get::<GlobalTransform>(b)?.translation());
            (crate::combat::segment_distance(p0, p1, pa, pb) <= spec.hit_radius + r).then(|| crate::prosthetic::first_touch(p0, p1, pa, pb, spec.hit_radius + r))
        })
        .fold(1.0f32, f32::min);
    let touch = p0 + (p1 - p0) * s;
    let at = touch + f * reach;
    crate::prosthetic::spawn_row(&mut world.commands(), &spec, at, -f, 9000, 0, -1);
    world.flush();
    let (mut posture, mut hp, anim0) = {
        let a = world.get::<Actor>(e).unwrap();
        (a.posture, a.hp, a.anim.clone())
    };
    let mut run = Run { flown: Vec::new(), hits: Vec::new(), reaction: None, gone: None };
    let mut first: Option<Entity> = None;
    for k in 1..=frames {
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::prosthetic::Projectile, &Transform)>();
        let live: Vec<(Entity, i64, Vec3)> = q.iter(world).map(|(e, p, t)| (e, p.bullet_id(), t.translation)).collect();
        if first.is_none() {
            first = live.iter().find(|b| b.1 == bullet).map(|b| b.0);
        }
        // Ours despawns a bullet in the tick it ends.
        match live.iter().find(|b| Some(b.0) == first) {
            Some(b) if run.gone.is_none() => run.flown.push((b.2 - at).length()),
            None if run.gone.is_none() => run.gone = Some(k),
            _ => {}
        }
        let a = world.get::<Actor>(e).unwrap();
        if a.posture > posture + 1e-3 || a.hp < hp - 1e-3 {
            run.hits.push(k);
        }
        (posture, hp) = (a.posture, a.hp);
        if run.reaction.is_none() && a.anim != anim0 && (a.t - 1.0 / 60.0).abs() < 1e-3 {
            run.reaction = Some(k);
        }
    }
    run
}

/// The shuriken thrown at the General from 4, 6 and 8 m. The exe's first bullet task after the
/// spawn only launches it (0 m), then it moves 80 m/s x 1/60 a frame (past `dist` 5 m slowing by
/// accelOutRange), stops where it touches him and lands its hit in that frame's hit step (frame 4
/// from 4 m); his reaction takes its first step the frame after. Its HitBulletID chain (dummy
/// 700001, 0.15 s, then 700002 / 700003 / 700004) lands two more hits 11 frames apart. Before:
/// ours moved on the spawn's own tick and hit two ticks early, its reaction stepped in the hit's
/// next tick, and its dummy ended on touching him (the next hit 2 ticks later, not 11).
#[test]
pub(crate) fn a_thrown_shuriken_flies_and_lands_like_the_exe() {
    let mut compared = 0;
    for name in ["shuriken_4", "shuriken_aside", "shuriken_8"] {
        let Some(v) = load(name) else { continue };
        let (exe, reach) = exe_run(&v);
        let frames = v["frames"].as_array().unwrap().len() - 1 - v["spawn_frame"].as_u64().unwrap() as usize;
        let ours = sim_run(v["bullet"].as_i64().unwrap(), reach, frames);
        println!("{name}: stopped {reach:.3} m out\n  exe  {exe:?}\n  ours {ours:?}");
        assert_eq!(ours.flown.len(), exe.flown.len(), "{name}: frames it flies");
        for (k, (e, o)) in exe.flown.iter().zip(&ours.flown).enumerate() {
            assert!((e - o).abs() < 2e-3, "{name}: frame {} flown exe {e} ours {o}", k + 1);
        }
        assert_eq!(ours.gone, exe.gone, "{name}: the frame the first bullet ends");
        assert_eq!(ours.hits, exe.hits, "{name}: the frames its hits land");
        assert_eq!(ours.reaction, exe.reaction, "{name}: the reaction's first step");
        compared += 1;
    }
    println!("compared {compared} runs");
}
