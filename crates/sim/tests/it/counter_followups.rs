//! Scripted real-data counter contacts and their follow-up deathblows.

use sekiro_sim::{Buttons, InputFrame, duel::Duel, enemy::EnemySpec};

const DT: f32 = 1.0 / 60.0;

fn load(name: &str, distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/tae.json").is_file()
        || !cache.join("anim/c5060/tae.json").is_file()
        || !cache.join("anim/c1010/tae.json").is_file()
    {
        eprintln!("skipping: counter fixtures are not extracted");
        return None;
    }
    let spec = EnemySpec::from_cache(&cache, name).unwrap();
    let mut duel = Duel::load_with(&cache, distance, spec).unwrap();
    duel.enemy_active = false;
    duel.events = None;
    Some(duel)
}

fn mikiri_deathblow(name: &str, act: i32, dodge: usize, move_first: bool) {
    let Some(mut d) = load(name, 2.0) else { return };
    let mut counter = None;
    let mut started = None;
    let mut cancel_at = None;
    for i in 0..360 {
        d.force_act = (i < 10).then_some(act);
        if counter.is_none() {
            d.rules.fighters[1].posture.remaining = 1;
        }
        let can_cancel = counter.is_some()
            && d.player
                .tae_frame()
                .flag(sekiro_sim::tae::cancel::DEATHBLOW);
        if counter.is_some() && cancel_at.is_none() && !can_cancel {
            assert!(
                !d.deathblow_available(),
                "{name}: counter lock-out ended early"
            );
        }
        if can_cancel && cancel_at.is_none() {
            cancel_at = Some(i);
            assert!(
                d.deathblow_available(),
                "{name}: flag 154 opened but no deathblow prompt"
            );
        }
        let move_now = move_first && cancel_at == Some(i);
        let attack_now = if move_first {
            cancel_at.is_some_and(|at| i == at + 1)
        } else {
            can_cancel
        };
        let r = d.step(
            &InputFrame {
                move_stick: if move_now {
                    [1.0, 0.0]
                } else if i == dodge {
                    [0.0, 1.0]
                } else {
                    [0.0; 2]
                },
                buttons: Buttons {
                    lock_on: i == 1,
                    dodge: i == dodge,
                    attack: attack_now,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty());
        assert!(r.enemy.behavior.script_errors.is_empty());
        if let Some((row, damage)) = r.mikiri {
            assert_eq!(row % 1000, 120);
            assert_eq!(damage, 90);
            assert!(d.rules.deathblow_open(1));
            counter = Some(i);
        }
        if let Some(rows) = r.deathblow.started {
            assert_eq!(
                cancel_at,
                Some(i - usize::from(move_first)),
                "must accept the first cancel-window press"
            );
            started = Some((i, rows));
        }
        if r.deathblow.hit {
            assert!(started.is_some());
            eprintln!("{name}: Mikiri at {counter:?}, deathblow {started:?}");
            return;
        }
    }
    panic!(
        "{name}: counter={counter:?}, deathblow={started:?}, available={}, posture={}, states={}/{}, clips={:?}/{:?}",
        d.deathblow_available(),
        d.rules.fighters[1].posture.remaining,
        d.player.state(),
        d.enemy.state(),
        d.player.behavior.runtime.main_clip(),
        d.enemy.behavior.runtime.main_clip()
    );
}

#[test]
fn deathblow_after_genichiro_mikiri() {
    mikiri_deathblow("c7100:71001000", 3062, 50, false);
}

#[test]
fn deathblow_after_owl_mikiri() {
    mikiri_deathblow("owl", 3033, 75, false);
}

#[test]
fn deathblow_after_soldier_mikiri() {
    mikiri_deathblow("soldier", 3013, 35, false);
}

#[test]
fn deathblow_after_mikiri_move_cancel() {
    mikiri_deathblow("c7100:71001000", 3062, 50, true);
}

fn sweep_stomp(name: &str, act: i32, jumps: [usize; 2], ap: u8) {
    let Some(mut d) = load(name, 2.4) else { return };
    d.rules
        .set_attack_power(sekiro_sim::progression::AttackPower::new(ap).unwrap())
        .unwrap();
    // AtkParam_Pc 901 directAtkStamDamage 10, SpEffect 400200 rate 8;
    // the castle-roof AP6 effect (160006) contributes staminaAttackRate 2.25.
    let expected = if ap == 6 { 180 } else { 80 };
    let mut warning = false;
    for i in 0..170 {
        d.force_act = (i < 10).then_some(act);
        let before = d.rules.fighters[1].posture.remaining;
        let r = d.step(
            &InputFrame {
                buttons: Buttons {
                    lock_on: i == 1,
                    jump: jumps.contains(&i),
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty());
        assert!(r.enemy.behavior.script_errors.is_empty());
        warning |= r.perilous.is_some();
        assert!(
            r.hits.iter().all(|(h, _)| h.attacker != 1),
            "sweep hit Wolf at {i}"
        );
        if let Some((_, result)) = r
            .hits
            .iter()
            .find(|(h, _)| h.attacker == 0 && h.judge == 901)
        {
            assert!(warning, "no perilous warning before stomp");
            assert_eq!(
                result.posture_damage, expected,
                "{name} act {act} AP{ap}: stomp bonus"
            );
            assert_eq!(
                d.rules.fighters[1].posture.remaining,
                (before - expected).max(0)
            );
            assert!(
                r.jump_counter,
                "{name}: stomp was not recognised as a counter"
            );
            eprintln!(
                "{name} act {act} AP{ap}: stomp at {i}, posture {before} -> {}",
                d.rules.fighters[1].posture.remaining
            );
            return;
        }
    }
    panic!("{name} act {act} AP{ap}: stomp never landed");
}

#[test]
fn genichiro_sweep_stomp_bonus() {
    for ap in [1, 6] {
        sweep_stomp("c7100:71001000", 3067, [45, 58], ap);
    }
}

#[test]
fn owl_sweep_stomp_bonus() {
    sweep_stomp("owl", 3004, [25, 38], 1);
}

#[test]
fn soldier_sweep_stomp_bonus() {
    sweep_stomp("soldier", 3020, [45, 58], 1);
}
