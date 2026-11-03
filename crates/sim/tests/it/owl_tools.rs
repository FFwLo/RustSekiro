//! The Great Shinobi Owl's tools against Wolf: firecrackers, the explosive slash, poison balls,
//! the smoke screen and the recovery-sealing powder, each forced with the repeat brain
//! (`BrainKind::Repeat`, the game's `--force-act`). Values are the data's (`docs/OWL.md`,
//! "Tools"). Skips when `cache/` lacks him.

use std::collections::BTreeSet;

use sekiro_sim::brain::BrainKind;
use sekiro_sim::bullets::BulletLook;
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

/// SpEffects of the tools.
const POISONED: i32 = 9050;
const RECOVERY_SEALED: i32 = 3506090;

fn load(distance: f32, act: i32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Owl is not extracted");
        return None;
    }
    let mut duel = Duel::load_with(&cache, distance, EnemySpec::owl()).unwrap();
    duel.set_brain(BrainKind::Repeat(act).build("c5060"), false)
        .unwrap();
    Some(duel)
}

/// Keeps Wolf alive and unbroken, so the Owl keeps fighting.
fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

/// Steps with Wolf idle until the Owl starts `clip`; false if he never does.
fn until_clip(duel: &mut Duel, clip: &str) -> bool {
    for _ in 0..3000 {
        keep_wolf_fresh(duel);
        let r = duel.step(&InputFrame::default(), DT);
        if r.enemy.animation.as_deref() == Some(clip) {
            return true;
        }
    }
    false
}

/// What happened in the steps after a tool started.
#[derive(Debug, Default)]
struct Window {
    /// Owl's hits on Wolf: (AtkParam row, HP, posture, guarded).
    hits: Vec<(i32, i32, i32, bool)>,
    /// Wolf's behaviour states.
    states: BTreeSet<String>,
    /// Bullet looks seen in flight, and the largest radius of each.
    looks: Vec<(BulletLook, f32)>,
    /// Wolf's SpEffects.
    effects: BTreeSet<i32>,
    max_poison: f32,
    hp_lost: i32,
    /// Steps Wolf was locked on.
    locked: usize,
}

impl Window {
    fn hits_by(&self, atk: i32) -> Vec<(i32, i32, i32, bool)> {
        self.hits.iter().copied().filter(|h| h.0 == atk).collect()
    }

    fn radius(&self, look: BulletLook) -> f32 {
        self.looks
            .iter()
            .filter(|(l, _)| *l == look)
            .map(|(_, r)| *r)
            .fold(0.0, f32::max)
    }
}

/// Runs `steps` steps with `input`, keeping Wolf's posture full (HP is measured).
fn watch(duel: &mut Duel, steps: usize, input: impl Fn(usize) -> InputFrame) -> Window {
    let mut w = Window::default();
    for i in 0..steps {
        let hp = duel.rules.fighters[0].vitality.hp;
        let r = duel.step(&input(i), DT);
        assert!(r.player.behavior.script_errors.is_empty());
        assert!(r.enemy.behavior.script_errors.is_empty());
        w.hp_lost += hp - duel.rules.fighters[0].vitality.hp;
        for b in &duel.bullets.live {
            match w.looks.iter_mut().find(|(l, _)| *l == b.look()) {
                Some((_, r)) => *r = r.max(b.radius),
                None => w.looks.push((b.look(), b.radius)),
            }
        }
        for (h, res) in &r.hits {
            if h.attacker == 1 {
                w.hits.push((
                    h.attack.atk_id,
                    res.hp_damage,
                    res.posture_damage,
                    res.guarded,
                ));
            }
        }
        w.states
            .extend(r.player.behavior.state_path.iter().cloned());
        w.effects
            .extend(duel.player.tae_frame().sp_effects.iter().copied());
        w.max_poison = w.max_poison.max(duel.rules.fighters[0].status.poison.value);
        w.locked += usize::from(r.locked_on);
        let wolf = &mut duel.rules.fighters[0];
        wolf.posture.remaining = wolf.posture.max;
    }
    w
}

fn idle(_: usize) -> InputFrame {
    InputFrame::default()
}

fn guard(_: usize) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            guard: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

/// Wolf turns and runs away from the Owl (he starts facing him).
fn flee(_: usize) -> InputFrame {
    InputFrame {
        move_stick: [0.0, -1.0],
        ..InputFrame::default()
    }
}

#[test]
fn firecrackers_burst_in_front_of_him_and_stagger_wolf() {
    let Some(mut duel) = load(4.0, 3010) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003010"));
    keep_wolf_fresh(&mut duel);
    // Wolf guards: the bang is unblockable (AtkParam 50600160 disables guard and deflect for
    // guard attribute 0, Wolf's katana).
    let w = watch(&mut duel, 150, guard);
    eprintln!("{w:#?}");
    // Five firecrackers fly (190), lie lit (195) and burst (200, 1.8 m) with flames (201).
    assert!(w.radius(BulletLook::Carrier) > 0.0);
    assert!((w.radius(BulletLook::Blast) - 1.8).abs() < 1e-4);
    assert!((w.radius(BulletLook::Flame) - 1.8).abs() < 1e-4);
    // The five share a hit record: one hit of 24 physical + 24 fire, 12 posture.
    let bangs = w.hits_by(50_600_160);
    assert_eq!(bangs.len(), 1, "{bangs:?}");
    assert_eq!(bangs[0], (50_600_160, 48, 12, false));
    // dmgLevel 3: the large stagger.
    assert!(w.states.contains("StandDamageLarge"), "{:?}", w.states);
}

#[test]
fn running_from_firecrackers_avoids_them() {
    let Some(mut duel) = load(4.0, 3010) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003010"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 150, flee);
    assert!(
        w.radius(BulletLook::Blast) > 0.0,
        "the firecrackers never burst"
    );
    assert!(w.hits_by(50_600_160).is_empty(), "{:?}", w.hits);
}

#[test]
fn explosive_slash_cuts_with_fire_and_its_flames_hit_nobody() {
    let Some(mut duel) = load(4.0, 3017) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003017"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 240, idle);
    // The slash (judge 199, AtkParam 50601199): 104 physical + 104 fire, 60 posture, a blow-away.
    let slash = w.hits_by(50_601_199);
    assert_eq!(slash.len(), 1, "{:?}", w.hits);
    assert_eq!(slash[0], (50_601_199, 208, 60, false));
    assert!(w.states.contains("StandDamageExLargeBlowStart"));
    // Its five powder bullets (250 to 254) end in flames of 1.5 and 2.5 m that use the dummy
    // AtkParam 1 (`opposeTarget` 0): looks only, no hit of their own.
    assert!(
        (w.radius(BulletLook::Flame) - 2.5).abs() < 1e-4,
        "{:?}",
        w.looks
    );
    assert!(w.hits.iter().all(|h| h.0 == 50_601_199), "{:?}", w.hits);
}

#[test]
fn poison_floor_poisons_a_wolf_standing_in_it() {
    let Some(mut duel) = load(4.0, 3030) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003030"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 300, idle);
    // Three rows of poison floor (702 / 732 / 742: 2 m, 18 s) build 30 every 0.15 s on a
    // resistance of 200: poisoned (9050) in about a second.
    assert!(
        (w.radius(BulletLook::Poison) - 2.0).abs() < 1e-4,
        "{:?}",
        w.looks
    );
    assert!(w.effects.contains(&POISONED), "{w:#?}");
    // No hit reaction from the floor itself.
    assert!(
        !w.states.iter().any(|s| s.starts_with("StandDamage")),
        "{:?}",
        w.states
    );
    assert!(w.hits.is_empty(), "{:?}", w.hits);
    // 9050: 0.25 % of 320 + 12 = 12 HP a second while it lasts (39.9 s).
    let wolf = &duel.rules.fighters[0];
    let left = wolf.status.remaining(POISONED).unwrap();
    assert!(left > 30.0 && left < 39.9, "{left}");
    let hp = wolf.vitality.hp;
    let w2 = watch(&mut duel, 300, flee);
    let lost = hp - duel.rules.fighters[0].vitality.hp;
    assert_eq!(lost, 5 * 12, "{w2:?}");
}

#[test]
fn poison_floor_poisons_wolf_without_an_enemy() {
    let Some(mut duel) = load(4.0, 3030) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003030"));
    for _ in 0..180 {
        duel.step(&idle(0), DT);
        if duel
            .bullets
            .live
            .iter()
            .any(|b| b.def.look == BulletLook::Poison)
        {
            break;
        }
    }
    assert!(
        duel.bullets
            .live
            .iter()
            .any(|b| b.def.look == BulletLook::Poison)
    );
    // Leave the authored floor in the world after its owner disappears.
    duel.enemy_present = false;
    duel.rules.fighters[1].vitality.hp = 0;
    let w = watch(&mut duel, 180, idle);
    assert!(w.effects.contains(&POISONED), "{w:#?}");
    assert!(
        w.hits.is_empty(),
        "effect-only contacts must not cause damage reactions"
    );
    assert!(duel.respawn_in.is_none());
}

#[test]
fn running_from_the_poison_balls_avoids_the_poison() {
    let Some(mut duel) = load(4.0, 3030) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003030"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 150, flee);
    assert!(w.radius(BulletLook::Poison) > 0.0, "no poison floor");
    assert_eq!(w.max_poison, 0.0);
    assert!(!w.effects.contains(&POISONED));
}

#[test]
fn smoke_screen_blinds_wolf_through_his_guard_and_breaks_the_lock() {
    let Some(mut duel) = load(4.0, 3069) else {
        return;
    };
    // Lock on first.
    duel.step(
        &InputFrame {
            buttons: Buttons {
                lock_on: true,
                ..Buttons::default()
            },
            ..InputFrame::default()
        },
        DT,
    );
    assert!(until_clip(&mut duel, "a000_003069"));
    assert!(
        duel.player.lock_target.is_some(),
        "Wolf lost the lock early"
    );
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 150, guard);
    // The dazzle (721): 5 m for 0.06 s, AtkParam 50600721 with no damage and atkBehaviorId 17,
    // which Wolf's script reads (env 334) as BEHAVIOR_IDENTIFICATION_VALUE_BLINDING.
    assert!(
        (w.radius(BulletLook::Smoke) - 5.0).abs() < 1e-4,
        "{:?}",
        w.looks
    );
    let dazzle = w.hits_by(50_600_721);
    assert_eq!(dazzle, vec![(50_600_721, 0, 0, false)]);
    assert!(w.states.contains("StandDamageBlinding"), "{:?}", w.states);
    // SpEffect 200602 (stateInfo 350, cannot be locked on, 1 s) on the Owl drops the lock.
    assert!(duel.player.lock_target.is_none());
}

#[test]
fn running_from_the_smoke_screen_avoids_the_blinding() {
    let Some(mut duel) = load(4.0, 3069) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003069"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 150, flee);
    assert!(w.radius(BulletLook::Smoke) > 0.0, "no smoke");
    assert!(w.hits_by(50_600_721).is_empty(), "{:?}", w.hits);
    assert!(!w.states.contains("StandDamageBlinding"));
}

#[test]
fn recovery_sealing_powder_seals_wolf_for_thirty_seconds() {
    let Some(mut duel) = load(4.0, 3031) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003031"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 180, idle);
    // The cloud (711) spreads from 0.1 to 3.2 m over 1 s and lives 1.7 s.
    assert!(
        (w.radius(BulletLook::Seal) - 3.2).abs() < 1e-3,
        "{:?}",
        w.looks
    );
    assert!(w.effects.contains(&RECOVERY_SEALED), "{w:#?}");
    assert_eq!(w.hp_lost, 0);
    let left = duel.rules.fighters[0]
        .status
        .remaining(RECOVERY_SEALED)
        .unwrap();
    assert!(left > 27.0 && left <= 30.0, "{left}");
}

#[test]
fn running_from_the_powder_avoids_the_seal() {
    let Some(mut duel) = load(4.0, 3031) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003031"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 180, flee);
    assert!(w.radius(BulletLook::Seal) > 0.0, "no powder cloud");
    assert!(!w.effects.contains(&RECOVERY_SEALED));
}

#[test]
fn shuriken_decay_with_range_and_slow_posture_recovery() {
    let Some(mut duel) = load(9.0, 3009) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003009"));
    keep_wolf_fresh(&mut duel);
    let w = watch(&mut duel, 90, idle);
    let hits = w.hits_by(50_600_150);
    assert!(!hits.is_empty(), "{:?}", w.hits);
    // 32 HP, losing 100 % a second once past its 5 m range (`damageDamp`): a little less at 9 m.
    assert!(hits.iter().all(|h| h.1 > 24 && h.1 < 32), "{hits:?}");
    // Bullet 150's SpEffect 113000: posture recovery -5 per second for 10 s.
    assert!(w.effects.contains(&113_000));
    assert!(duel.rules.fighters[0].status.remaining(113_000).is_some());
}
