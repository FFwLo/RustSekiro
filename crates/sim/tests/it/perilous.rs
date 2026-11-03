//! Perilous attacks (`docs/COMBAT-RULES.md`, "Perilous attacks"): the warning kanji from the
//! data, and the jump counter on a perilous sweep (jump over it, kick off the attacker: the
//! sweep's "jump kick effective" SpEffect multiplies the kick's posture damage by 8).
use std::path::{Path, PathBuf};

use glam::Vec3;
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::hits::Perilous;
use sekiro_sim::{Buttons, Character, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
fn forward_step_counters_the_thrust() {
    let Some(cache) = cache() else { return };
    for (spec, act, step, reaction) in [
        (EnemySpec::soldier(), 3013, 35, "ThrowDef13100"),
        (EnemySpec::owl(), 3033, 75, "ThrowDef13800"),
    ] {
        let mut d = Duel::load_with(&cache, 2.0, spec).unwrap();
        d.set_brain(Box::new(sekiro_sim::brain::LuaBrain), false)
            .unwrap();
        d.enemy_active = false;
        let mut countered = false;
        let hp = d.rules.fighters[0].vitality.hp;
        for i in 0..260 {
            d.force_act = (i < 10).then_some(act);
            let input = InputFrame {
                move_stick: if i == step { [0.0, 1.0] } else { [0.0; 2] },
                buttons: Buttons {
                    dodge: i == step,
                    lock_on: i == 1,
                    ..Default::default()
                },
                ..Default::default()
            };
            let r = d.step(&input, DT);
            assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            if let Some((row, damage)) = r.mikiri {
                assert_eq!(damage, 90);
                assert_eq!(row, if act == 3013 { 11010120 } else { 15060190 });
            }
            if d.enemy.state() == reaction && !countered {
                countered = true;
                assert_eq!(
                    d.rules.fighters[1].posture.remaining,
                    (d.rules.fighters[1].posture.max - 90).max(0)
                );
                assert!(d.player.state().starts_with("ThrowAtk511"));
            }
            assert!(
                r.hits.iter().all(|(h, _)| h.attacker != 1),
                "Wolf hit: {act} at {i}"
            );
            assert_eq!(d.rules.fighters[0].vitality.hp, hp);
        }
        assert!(countered, "no counter for {act}");
        let brain = d.boss_brain.as_ref().unwrap();
        assert_eq!(
            brain
                .tally
                .counts(sekiro_sim::brain::tally::Side::Boss, &act.to_string())
                .countered,
            1
        );
        assert_eq!(
            brain
                .tally
                .counts(sekiro_sim::brain::tally::Side::Boss, &act.to_string())
                .whiffed,
            0
        );
        assert!(
            !d.player.state().starts_with("ThrowAtk511"),
            "counter never released"
        );
    }
}

#[test]
fn mikiri_requires_a_timed_forward_step_and_a_thrust() {
    let Some(cache) = cache() else { return };
    use rayon::prelude::*;
    // Each case builds its own duel, so they run side by side.
    let soldier: fn() -> EnemySpec = EnemySpec::soldier;
    let owl: fn() -> EnemySpec = EnemySpec::owl;
    let cases = [
        (soldier, 3013, 0, [0.0, 1.0]),
        (soldier, 3013, 60, [0.0, 1.0]),
        (owl, 3033, 10, [0.0, 1.0]),
        (owl, 3033, 115, [0.0, 1.0]),
        (soldier, 3013, 35, [0.0, -1.0]),
        (soldier, 3013, 35, [1.0, 0.0]),
        (owl, 3033, 75, [0.0, -1.0]),
        (owl, 3033, 75, [-1.0, 0.0]),
        (soldier, 3020, 55, [0.0, 1.0]),
        (owl, 3004, 40, [0.0, 1.0]),
        (soldier, 3000, 35, [0.0, 1.0]),
        (owl, 3000, 35, [0.0, 1.0]),
    ];
    cases.par_iter().for_each(|&(spec, act, step, stick)| {
        let mut d = Duel::load_with(&cache, 2.0, spec()).unwrap();
        d.enemy_active = false;
        for i in 0..150 {
            d.force_act = (i < 10).then_some(act);
            let r = d.step(
                &InputFrame {
                    move_stick: if i == step { stick } else { [0.0; 2] },
                    buttons: Buttons {
                        dodge: i == step,
                        lock_on: i == 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(
                r.mikiri.is_none(),
                "unexpected counter: {act}, {step}, {stick:?}"
            );
            assert!(!d.player.state().starts_with("ThrowAtk511"));
        }
    });
}

#[test]
fn stepping_forward_away_from_an_unlocked_thrust_does_not_counter() {
    let Some(cache) = cache() else { return };
    let mut d = Duel::load_with(&cache, 2.0, EnemySpec::soldier()).unwrap();
    d.enemy_active = false;
    d.player.body.yaw = std::f32::consts::PI;
    for i in 0..100 {
        d.force_act = (i < 10).then_some(3013);
        let r = d.step(
            &InputFrame {
                camera_yaw: std::f32::consts::PI,
                camera_pitch: 0.0,
                move_stick: if i == 35 { [0.0, 1.0] } else { [0.0; 2] },
                buttons: Buttons {
                    dodge: i == 35,
                    ..Default::default()
                },
            },
            DT,
        );
        assert!(r.mikiri.is_none());
    }
}

#[test]
fn forward_step_window_and_counter_dummies_follow_the_data() {
    let Some(cache) = cache() else { return };
    let d = Duel::load_with(&cache, 2.0, EnemySpec::soldier()).unwrap();
    let anim = d.player.tae.anim("a000_213301").unwrap();
    let event = anim
        .events
        .iter()
        .find(|e| e.kind == sekiro_sim::tae::TaeKind::ActionFlag(125))
        .unwrap();
    assert_eq!(event.start, 0.0);
    assert!((event.end - 0.4).abs() < 0.00001);
    for judge in [940, 941] {
        assert!(sekiro_sim::perilous::counter_dummy(
            &d.fighters[1].attacks[&judge]
        ));
    }
    let owl = Duel::load_with(&cache, 2.0, EnemySpec::owl()).unwrap();
    assert!(sekiro_sim::perilous::counter_dummy(
        &owl.fighters[1].attacks[&311]
    ));
    let mut bullets: Vec<_> = owl.fighters[1]
        .bullets
        .values()
        .map(|b| b.as_ref())
        .collect();
    let mut smoke = None;
    while let Some(b) = bullets.pop() {
        if let Some(a) = b.attack.as_ref().filter(|a| a.atk_id == 50600721) {
            smoke = Some(a);
            break;
        }
        if let Some(child) = &b.child {
            bullets.push(child);
        }
        if let Some(interval) = &b.interval {
            bullets.push(&interval.def);
        }
    }
    let smoke = smoke.expect("Owl's blinding bullet is missing");
    assert!(!sekiro_sim::perilous::counter_dummy(smoke));
}

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    let ok = cache.join("anim/c1010/tae.json").is_file()
        && cache.join("anim/c5060/tae.json").is_file()
        && cache.join("raw/action/script/c0000.hks").is_file();
    if !ok {
        eprintln!("skipping: the soldier, the Owl or Wolf's script are not extracted");
    }
    ok.then_some(cache)
}

fn clip(c: &Character) -> (String, f32) {
    c.behavior
        .runtime
        .main_clip()
        .map(|c| (c.animation.clone(), c.time))
        .unwrap_or_default()
}

fn flat_distance(d: &Duel) -> f32 {
    Vec3::from_array(d.player.body.position)
        .with_y(0.0)
        .distance(Vec3::from_array(d.enemy.body.position).with_y(0.0))
}

/// The first perilous warning of a forced act: (enemy clip time, warning).
fn first_warning(
    cache: &Path,
    spec: EnemySpec,
    act: i32,
    distance: f32,
) -> Option<(f32, sekiro_sim::perilous::PerilousWarning)> {
    let mut d = Duel::load_with(cache, distance, spec).unwrap();
    d.enemy_active = false;
    d.force_act = Some(act);
    for _ in 0..120 {
        let r = d.step(&InputFrame::default(), DT);
        if let Some(w) = r.perilous {
            return Some((clip(&d.enemy).1, w));
        }
    }
    None
}

#[test]
fn the_warning_follows_the_dummy_bullet() {
    let Some(cache) = cache() else { return };
    // Soldier low sweep a000_003020: BulletBehavior 980 at 0.2 s, blow AtkParam 10100240.
    let (t, w) = first_warning(&cache, EnemySpec::soldier(), 3020, 2.0).expect("no warning");
    assert!((t - 0.2).abs() < 0.03, "soldier sweep warning at {t:.3} s");
    assert_eq!(
        (w.attacker, w.judge, w.kind),
        (1, 980, Some(Perilous::Sweep))
    );
    // Soldier thrust a000_003013: 980 at 0.0 s, blow 10100230 (deflectable: a thrust).
    let (t, w) = first_warning(&cache, EnemySpec::soldier(), 3013, 2.0).expect("no warning");
    assert!(t < 0.05, "soldier thrust warning at {t:.3} s");
    assert_eq!(w.kind, Some(Perilous::Thrust));
    // The Owl's Shinobi stab a000_003033: 987 at 0.667 s, blow 50600310.
    let (t, w) = first_warning(&cache, EnemySpec::owl(), 3033, 3.0).expect("no warning");
    assert!((t - 0.667).abs() < 0.03, "Owl stab warning at {t:.3} s");
    assert_eq!((w.judge, w.kind), (987, Some(Perilous::Thrust)));
    // The Owl's low sweep a000_003004: 980 at 0.0 s.
    let (_, w) = first_warning(&cache, EnemySpec::owl(), 3004, 3.0).expect("no warning");
    assert_eq!((w.judge, w.kind), (980, Some(Perilous::Sweep)));
    // An ordinary slash has no warning.
    assert!(first_warning(&cache, EnemySpec::soldier(), 3000, 2.0).is_none());
}

#[test]
fn no_warning_beyond_the_bullets_generation_limit() {
    let Some(cache) = cache() else { return };
    // Bullet 10001600 `generationMaxDistWhenOwner` 5 m: the soldier's warning is not made when
    // Wolf (where it would be made) is farther away.
    let mut d = Duel::load_with(&cache, 7.0, EnemySpec::soldier()).unwrap();
    d.enemy_active = false;
    d.force_act = Some(3020);
    for _ in 0..25 {
        let r = d.step(&InputFrame::default(), DT);
        assert!(flat_distance(&d) > 5.5);
        assert!(
            r.perilous.is_none(),
            "warning at {:.2} m",
            flat_distance(&d)
        );
    }
}

/// What happened in one run of a forced act against a Wolf who presses jump at given steps (a
/// press in the air is the air kick).
#[derive(Debug, Default)]
struct Run {
    /// The kick's posture damage on the enemy, the first time it landed.
    kick_posture: Option<i32>,
    counter: bool,
    /// The enemy's attack landed on Wolf.
    wolf_hit: bool,
    kicked_off: bool,
    enemy_clip_after_kick: String,
    broken: bool,
}

/// Steps 170 times. The enemy is forced to `act` from step `act_from` until `act_until` (no new
/// act after that); Wolf locks on and presses jump at the steps in `jumps`.
fn run(
    cache: &Path,
    spec: EnemySpec,
    act: i32,
    distance: f32,
    (act_from, act_until): (usize, usize),
    jumps: &[usize],
) -> Run {
    let mut d = Duel::load_with(cache, distance, spec).unwrap();
    d.set_brain(Box::new(sekiro_sim::brain::LuaBrain), false)
        .unwrap();
    d.enemy_active = false;
    let mut out = Run::default();
    let mut kicked_at = None;
    for i in 0..170 {
        d.force_act = (act_from..act_until).contains(&i).then_some(act);
        let input = InputFrame {
            buttons: Buttons {
                jump: jumps.contains(&i),
                lock_on: i == 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let r = d.step(&input, DT);
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        for (hit, res) in &r.hits {
            if hit.attacker == 0 && hit.judge == 901 && out.kick_posture.is_none() {
                out.kick_posture = Some(res.posture_damage);
                kicked_at = Some(i);
            }
            out.wolf_hit |= hit.attacker == 1;
        }
        out.counter |= r.jump_counter;
        out.kicked_off |= d.player.state().starts_with("AirKickEnemyJumpStart");
        if kicked_at.is_some_and(|k| i == k + 3) {
            out.enemy_clip_after_kick = clip(&d.enemy).0;
        }
        out.broken |= d.rules.fighters[1].posture.remaining < 1;
    }
    if out.counter {
        let mut tally = d.boss_brain.as_ref().unwrap().tally.clone();
        tally.flush(170.0 * DT);
        let counts = tally.counts(sekiro_sim::brain::tally::Side::Boss, &act.to_string());
        assert_eq!(counts.countered, 1);
        assert_eq!(counts.counter_kinds[1], 1);
    }
    out
}

#[test]
fn jump_kick_on_the_soldiers_sweep_breaks_his_posture() {
    let Some(cache) = cache() else { return };
    // Sweep a000_003020: the blade at 1.067 to 1.333 s, SpEffect 400200 ("jump kick
    // effective", defAntiGroundStaminaDmgRate 8) from 0.0 to 2.667 s. Wolf jumps at 0.75 s,
    // is in the air (i-frame type 1) when the blade passes, and his kick (AtkParam_Pc 901,
    // directAtkStamDamage 10, anti-ground) lands at about 1.47 s.
    let r = run(&cache, EnemySpec::soldier(), 3020, 2.4, (0, 999), &[45, 58]);
    assert!(!r.wolf_hit, "the sweep must pass under the jump: {r:?}");
    assert_eq!(r.kick_posture, Some(80), "10 x 8: {r:?}");
    assert!(r.counter, "{r:?}");
    // 80 empties the soldier's 75 posture: he collapses (deathblow open), and Wolf kicks off
    // him (env 2004 with the kick's SpEffect reference 204).
    assert!(r.broken, "{r:?}");
    assert_eq!(r.enemy_clip_after_kick, "a000_008300", "{r:?}");
    assert!(r.kicked_off, "{r:?}");
}

#[test]
fn jump_kick_on_the_owls_sweep_takes_eight_times_the_posture() {
    let Some(cache) = cache() else { return };
    // Owl low sweep a000_003004: blade 0.867 to 1.0 s, SpEffects 400200 and 3506001 from 0.133
    // to 1.567 s.
    let r = run(&cache, EnemySpec::owl(), 3004, 2.4, (0, 999), &[25, 38]);
    assert!(!r.wolf_hit, "{r:?}");
    assert_eq!(r.kick_posture, Some(80), "{r:?}");
    assert!(r.counter && r.kicked_off, "{r:?}");
    assert!(!r.broken, "the Owl has 660 posture");
}

#[test]
fn an_early_or_late_kick_is_an_ordinary_kick() {
    let Some(cache) = cache() else { return };
    // Early: Wolf kicks the standing Owl just before he starts the sweep (the act from step 52,
    // its 400200 window from step 60).
    let early = run(&cache, EnemySpec::owl(), 3004, 1.8, (52, 999), &[0, 13]);
    assert_eq!(early.kick_posture, Some(10), "{early:?}");
    assert!(!early.counter, "{early:?}");
    // Late: the Owl sweeps with Wolf out of reach; once the sweep's 400200 window has closed
    // (1.567 s) Wolf is put in front of him and kicks.
    let mut d = Duel::load_with(&cache, 9.0, EnemySpec::owl()).unwrap();
    d.enemy_active = false;
    // Placed at 1.33 s; the kick lands about 0.55 s later. (Earlier, the Owl is still down
    // from his falling sweep and the kick passes over him.)
    let place = 80;
    let mut late = None;
    for i in 0..170 {
        d.force_act = (i < 10).then_some(3004);
        if i == place {
            let e = Vec3::from_array(d.enemy.body.position);
            let f = Vec3::new(-d.enemy.body.yaw.sin(), 0.0, -d.enemy.body.yaw.cos());
            let p = e + f * 1.8;
            d.player.body.position = p.to_array();
            d.player.body.yaw = d.player.yaw_to(e.to_array());
        }
        let input = InputFrame {
            buttons: Buttons {
                jump: i == place || i == place + 13,
                lock_on: i == 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let r = d.step(&input, DT);
        if let Some((_, res)) = r
            .hits
            .iter()
            .find(|(h, _)| h.attacker == 0 && h.judge == 901)
        {
            assert!(clip(&d.enemy).0 != "a000_003004" || clip(&d.enemy).1 > 1.567);
            late = Some((res.posture_damage, r.jump_counter));
            break;
        }
    }
    let (posture, counter) = late.expect("the late kick never landed");
    assert_eq!(posture, 10);
    assert!(!counter);
}

#[test]
fn an_early_or_late_jump_is_caught_by_the_sweep() {
    let Some(cache) = cache() else { return };
    // Standing still, the soldier's sweep hits Wolf.
    let stand = run(&cache, EnemySpec::soldier(), 3020, 2.0, (0, 999), &[]);
    assert!(stand.wolf_hit);
    // Jumping too late (the blade arrives at step 64; the jump start itself has the sweep
    // i-frames from its first frame) or too early (landed before it) does not help.
    let late = run(&cache, EnemySpec::soldier(), 3020, 2.0, (0, 999), &[65]);
    assert!(late.wolf_hit, "{late:?}");
    let early = run(&cache, EnemySpec::soldier(), 3020, 2.0, (30, 999), &[2]);
    assert!(early.wolf_hit, "{early:?}");
    // In time, he is in the air with the sweep i-frames when it passes.
    let timed = run(&cache, EnemySpec::soldier(), 3020, 2.0, (0, 999), &[45]);
    assert!(!timed.wolf_hit, "{timed:?}");
}
