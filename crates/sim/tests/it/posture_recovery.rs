//! Posture recovery on a guarding enemy under continuous pressure, and the engine's one
//! "no recovery" flag (`act(110)`). Skips when `cache/` has not been extracted.

use std::path::PathBuf;

use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::tae::TaeFrame;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn cache() -> Option<PathBuf> {
    let c = sekiro_sim::cache::root();
    if !c.join("anim/c5060/tae.json").exists()
        || !c.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists()
    {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    Some(c)
}

/// What one pressure run saw.
struct Run {
    /// Guarded hits that landed on the enemy.
    guarded_hits: usize,
    /// Lowest posture the enemy reached.
    lowest: i32,
    /// Posture just after each guarded hit.
    after_hits: Vec<i32>,
    /// The speed (points per second) shown while the enemy stood in its guard `a000_003100`.
    guard_rate: f32,
}

/// Wolf slashes every `period` steps for `steps` steps at an enemy forced into its guard
/// (`act` 3100), keeping Wolf healthy. Logs the enemy's posture over time.
fn pressure(duel: &mut Duel, label: &str, period: usize, steps: usize) -> Run {
    duel.force_act = Some(3100);
    let mut run = Run {
        guarded_hits: 0,
        lowest: i32::MAX,
        after_hits: Vec::new(),
        guard_rate: 0.0,
    };
    for i in 0..steps {
        let wolf = &mut duel.rules.fighters[0];
        wolf.vitality.hp = wolf.vitality.max_hp;
        wolf.posture.remaining = wolf.posture.max;
        let attack = i > 0 && i % period == 0;
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack,
                    lock_on: i < 3,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        let enemy = &duel.rules.fighters[1];
        let clip = duel
            .enemy
            .behavior
            .runtime
            .main_clip()
            .map(|c| c.animation.clone())
            .unwrap_or_default();
        let guarded = r
            .hits
            .iter()
            .filter(|(h, res)| h.attacker == 0 && res.guarded)
            .count();
        run.guarded_hits += guarded;
        run.lowest = run.lowest.min(enemy.posture.remaining);
        if guarded > 0 {
            run.after_hits.push(enemy.posture.remaining);
        }
        if clip == "a000_003100" && enemy.recovery_rate > 0.0 {
            run.guard_rate = run.guard_rate.max(enemy.recovery_rate);
        }
        if guarded > 0 || i % 60 == 0 {
            eprintln!(
                "{label} period {period}: t={:.2} posture={}/{} speed={:.1}/s guarded={guarded} clip={clip}",
                i as f32 * DT,
                enemy.posture.remaining,
                enemy.posture.max,
                enemy.recovery_rate,
            );
        }
    }
    run
}

/// Wolf's slash costs a guarding Owl 14 or 15 of 660 posture, and the Owl's guard animation
/// `a000_003100` carries neither TAE 225 nor a StaminaControlParam type, so it recovers at his
/// full `staminaRecoverBaseVel` of 60 per second: a slash every 0.6 s is refunded within about
/// 0.25 s of each hit and posture never builds. Only a much faster cadence builds it.
#[test]
fn guarding_owl_under_pressure() {
    let Some(cache) = cache() else { return };
    let mut results = Vec::new();
    for period in [36, 24, 18, 12] {
        let mut owl = Duel::load_with(&cache, 1.6, EnemySpec::owl()).unwrap();
        let run = pressure(&mut owl, "owl", period, 600);
        eprintln!(
            "owl period {period}: {} guarded hits, lowest posture {}, after hits {:?}, guard speed {}",
            run.guarded_hits, run.lowest, run.after_hits, run.guard_rate
        );
        results.push((period, run));
    }
    let (_, slow) = &results[0];
    assert!(slow.guarded_hits >= 5, "{} guarded hits", slow.guarded_hits);
    // Data: NpcParam 50600000 staminaRecoverBaseVel 60, no 225 / 960 on the guard clip.
    assert!((slow.guard_rate - 60.0).abs() < 0.5, "{}", slow.guard_rate);
    // A slash every 0.6 s: every hit is refunded, posture stays within 40 of full.
    assert!(slow.lowest >= 660 - 40, "lowest {}", slow.lowest);
}

/// The soldier's guard `a000_003100` has TAE 225 at 25 %, so he recovers at 5 of 20 per second
/// and three guarded slashes 0.6 s apart break his 75 posture.
#[test]
fn guarding_soldier_under_pressure_breaks() {
    let Some(cache) = cache() else { return };
    let mut soldier = Duel::load(&cache, 1.6).unwrap();
    let run = pressure(&mut soldier, "soldier", 36, 600);
    eprintln!(
        "soldier: {} guarded hits, lowest posture {}, after hits {:?}",
        run.guarded_hits, run.lowest, run.after_hits
    );
    assert!(run.guarded_hits >= 2);
    assert!(
        run.after_hits.windows(2).take(1).all(|w| w[1] < w[0]),
        "{:?}",
        run.after_hits
    );
    // The third guarded slash is a deflect, which empties the meter without a break report.
    assert!(
        run.lowest <= 0,
        "the soldier posture never emptied: {}",
        run.lowest
    );
}

#[test]
fn guarding_owl_at_castle_roof_progression() {
    let Some(cache) = cache() else { return };
    let mut owl = Duel::load_with(&cache, 1.6, EnemySpec::owl()).unwrap();
    owl.rules
        .set_attack_power(sekiro_sim::progression::AttackPower::CASTLE_ROOF)
        .unwrap();
    let run = pressure(&mut owl, "owl AP6", 36, 600);
    eprintln!(
        "owl AP6: {} guarded hits, lowest {}, after hits {:?}, guard rate {}",
        run.guarded_hits, run.lowest, run.after_hits, run.guard_rate
    );
    assert!(run.guarded_hits >= 5);
    assert_eq!(run.guard_rate, 60.0);
    assert_eq!(run.guarded_hits, 11);
    // Root blending shifts hit arrival by a tick; the recovery rate remains 60/s.
    assert_eq!(run.lowest, 514);
    assert_eq!(
        run.after_hits,
        [628, 611, 613, 560, 561, 566, 567, 514, 515, 524, 525]
    );
}

/// `act(110)` (`SetStaminaRecoveryDisabled`) skips the whole recovery update: no gain, no carry.
/// No shipped script calls it, so this only covers the engine rule.
#[test]
fn recovery_disabled_flag_skips_the_update() {
    let Some(cache) = cache() else { return };
    let mut duel = Duel::load_with(&cache, 4.0, EnemySpec::owl()).unwrap();
    duel.use_simple_ai();
    duel.enemy_active = false;
    let rules = &mut duel.rules;
    let frame = TaeFrame::default();
    rules.fighters[1].posture.remaining = 100;
    rules.recover_with_state(1, &frame, false, DT);
    let rate = rules.fighters[1].recovery_rate;
    assert_eq!(rate, 60.0);
    let before = rules.fighters[1].posture.remaining;
    for _ in 0..120 {
        rules.recover_with_state(1, &frame, true, DT);
    }
    assert_eq!(rules.fighters[1].posture.remaining, before);
    assert_eq!(
        rules.fighters[1].recovery_rate, rate,
        "the speed display is not updated"
    );
    for _ in 0..60 {
        rules.recover_with_state(1, &frame, false, DT);
    }
    assert_eq!(rules.fighters[1].posture.remaining, before + 60);
}
