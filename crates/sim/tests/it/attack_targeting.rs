//! How attacks find their target: the TAE 760 lunge, the unlocked attack auto-aim, the
//! soldier's TAE 703 fixed rotation and the parry-timing flag 63 that tells the soldier's AI
//! to parry. Skips when `cache/` has not been extracted.

use sekiro_sim::duel::{Duel, FLAG_PARRY_TIMING};
use sekiro_sim::input::angle_diff;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, Character, InputFrame};
use std::path::PathBuf;

const DT: f32 = 1.0 / 60.0;

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    Some(cache)
}

/// Wolf and an idle soldier `distance` metres in front of him.
fn idle_duel(distance: f32) -> Option<Duel> {
    let mut duel = Duel::load(&cache()?, distance).unwrap();
    duel.use_simple_ai();
    duel.enemy_active = false;
    Some(duel)
}

fn attack(held: bool) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            attack: held,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

/// How far Wolf travels during a held first slash (a050_300000, TAE 760 at 0.6-0.8 s), locked
/// on to the soldier `distance` metres away.
fn locked_slash_travel(distance: f32) -> Option<f32> {
    let mut duel = idle_duel(distance)?;
    let lock = InputFrame {
        buttons: Buttons {
            lock_on: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    };
    duel.step(&lock, DT);
    duel.step(&InputFrame::default(), DT);
    assert!(duel.player.lock_target.is_some());
    let start = duel.player.body.position;
    let mut lunged = false;
    for _ in 0..70 {
        let r = duel.step(&attack(true), DT);
        lunged |= duel.player.tae_frame().reach_target.is_some()
            && r.player.animation.as_deref() == Some("a050_300000");
    }
    assert!(lunged, "the held slash never reached its TAE 760 window");
    let end = duel.player.body.position;
    Some(((end[0] - start[0]).powi(2) + (end[2] - start[2]).powi(2)).sqrt())
}

#[test]
fn locked_slash_lunges_further_at_a_far_target() {
    let Some(far) = locked_slash_travel(4.0) else {
        return;
    };
    let near = locked_slash_travel(1.5).unwrap();
    eprintln!("slash travel: {far:.3} m at 4 m, {near:.3} m at 1.5 m");
    assert!(far > near + 0.2, "far {far} near {near}");
}

/// Wolf facing yaw 0 attacks with the stick pushed along world yaw -40 deg (to his right);
/// the soldier stands 2 m away at `enemy_yaw_deg`. Returns, after 40 steps (still in the
/// swing), Wolf's yaw and the yaw from him to the soldier, in degrees. The stick stays
/// pushed, as the auto-aim measures from it.
fn unlocked_attack_yaw(enemy_yaw_deg: f32) -> Option<(f32, f32)> {
    let mut duel = idle_duel(2.0)?;
    let a = enemy_yaw_deg.to_radians();
    duel.enemy.body.position = [-a.sin() * 2.0, 0.0, -a.cos() * 2.0];
    duel.player.body.yaw = 0.0;
    let stick = (-40.0f32).to_radians();
    for i in 0..40 {
        let input = InputFrame {
            move_stick: [0.0, 1.0],
            camera_yaw: stick,
            ..attack(i < 2)
        };
        duel.step(&input, DT);
        assert!(duel.player.lock_target.is_none());
    }
    let to_enemy = duel.player.yaw_to(duel.enemy.body.position);
    Some((duel.player.body.yaw.to_degrees(), to_enemy.to_degrees()))
}

fn off(a_deg: f32, b_deg: f32) -> f32 {
    angle_diff(a_deg.to_radians(), b_deg.to_radians())
        .to_degrees()
        .abs()
}

#[test]
fn unlocked_attack_homes_in_on_an_enemy_near_the_stick() {
    // 10 deg from the stick: auto-aim picks the soldier and Wolf turns to him (the lunge
    // moves Wolf, so compare with the direction from where he ends up).
    let Some((yaw, to_enemy)) = unlocked_attack_yaw(-50.0) else {
        return;
    };
    eprintln!("enemy at -50 deg: Wolf yaw {yaw:.1}, soldier at {to_enemy:.1}");
    assert!(off(yaw, to_enemy) < 3.0, "yaw {yaw} to soldier {to_enemy}");
    // 60 deg from the stick (20 from the facing): no homing; Wolf follows the stick.
    let (yaw, to_enemy) = unlocked_attack_yaw(20.0).unwrap();
    eprintln!("enemy at +20 deg: Wolf yaw {yaw:.1}, soldier at {to_enemy:.1}");
    assert!(off(yaw, -40.0) < 3.0, "yaw {yaw}");
}

/// Yaw change of each step of the soldier's first slash (a000_003000), asked to face
/// `face_offset` radians off its start yaw, with whether TAE 703 was active.
fn soldier_slash_turns(face_offset: Option<f32>) -> Option<Vec<(bool, f32)>> {
    let mut npc = Character::load_npc(&cache()?, "c1010").unwrap();
    npc.settle(DT, 400);
    let mut steps = Vec::new();
    for i in 0..90 {
        npc.npc.action = if i < 10 { 3000 } else { 0 };
        npc.npc.face_yaw = face_offset.map(|o| npc.body.yaw + o);
        let before = npc.body.yaw;
        let r = npc.tick(&InputFrame::default(), DT);
        if r.animation.as_deref() == Some("a000_003000") {
            steps.push((
                npc.tae_frame().fixed_rotation,
                angle_diff(npc.body.yaw, before),
            ));
        }
    }
    Some(steps)
}

#[test]
fn soldier_does_not_track_during_fixed_rotation() {
    let Some(free) = soldier_slash_turns(None) else {
        return;
    };
    // The target keeps sitting 90 deg to its left, so tracking would turn it every step.
    let tracking = soldier_slash_turns(Some(std::f32::consts::FRAC_PI_2)).unwrap();
    assert_eq!(free.len(), tracking.len());
    let fixed: Vec<_> = free
        .iter()
        .zip(&tracking)
        .filter(|((fixed, _), _)| *fixed)
        .collect();
    assert!(!fixed.is_empty(), "the slash has no TAE 703 window");
    // Inside the window only root motion turns it: the same as without a target.
    for ((_, a), (_, b)) in &fixed {
        assert!((a - b).abs() < 1e-4, "turned {b} vs {a} during TAE 703");
    }
    // Outside it the soldier tracks.
    assert!(
        free.iter()
            .zip(&tracking)
            .any(|((fixed, a), (_, b))| !fixed && (a - b).abs() > 1e-3),
        "the soldier never tracked during its slash"
    );
}

#[test]
fn wolf_attack_flag_63_raises_the_parry_interrupt() {
    let Some(cache) = cache() else {
        return;
    };
    if !cache.join("raw/script/aicommon.luabnd.d").exists() {
        return;
    }
    let mut duel = Duel::load(&cache, 2.5).unwrap();
    assert!(duel.ai.is_some());
    let mut flag_steps = Vec::new();
    let mut interrupt_steps = Vec::new();
    for i in 0..60 {
        duel.step(&attack(i < 2), DT);
        if duel.player.tae_frame().flag(FLAG_PARRY_TIMING) {
            flag_steps.push(i);
        }
        let events = duel.ai.as_mut().unwrap().drain_log();
        if events.contains(&AiEvent::Interrupt(24)) {
            interrupt_steps.push(i);
        }
    }
    eprintln!("flag 63 at {flag_steps:?}, INTERUPT_ParryTiming at {interrupt_steps:?}");
    let first_flag = *flag_steps.first().expect("Wolf's slash has no flag 63");
    // The AI reads the world before Wolf steps, so it hears the flag one step later, and
    // only once per flag window.
    assert_eq!(interrupt_steps, vec![first_flag + 1]);
}
