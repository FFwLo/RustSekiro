use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::progression::AttackPower;
use sekiro_sim::{Buttons, InputFrame};

#[test]
fn slash_damage_by_attack_power() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists() {
        eprintln!("skipping: cache not extracted");
        return;
    }
    for spec in [EnemySpec::soldier(), EnemySpec::owl()] {
        for ap in [1, 5, 10] {
            let mut duel = Duel::load_with(&cache, 1.6, spec.clone()).unwrap();
            if spec.chr == "c5060" {
                duel.force_act = Some(3100);
            } else {
                duel.use_simple_ai();
                duel.enemy_active = false;
            }
            // Avoid keep_hp clipping the AP10 slash to this soldier's 195 HP.
            duel.rules.fighters[1].vitality.hp = 10000;
            duel.rules.fighters[1].vitality.max_hp = 10000;
            duel.rules
                .set_attack_power(AttackPower::new(ap).unwrap())
                .unwrap();
            let mut found = false;
            for step in 0..180 {
                let r = duel.step(
                    &InputFrame {
                        buttons: Buttons {
                            attack: step == 30,
                            lock_on: step < 3,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    1.0 / 60.0,
                );
                if let Some((_, hit)) = r.hits.iter().find(|(h, _)| h.attacker == 0) {
                    eprintln!(
                        "{} AP {ap}: HP {} posture {} guarded {}",
                        spec.chr, hit.hp_damage, hit.posture_damage, hit.guarded
                    );
                    assert_eq!(hit.guarded, spec.chr == "c5060");
                    let expected = match (spec.chr.as_str(), ap) {
                        ("c1010", 1) => (80, 30),
                        ("c1010", 5) => (160, 60),
                        ("c1010", 10) => (260, 97),
                        ("c5060", 1) => (0, 15),
                        ("c5060", 5) => (0, 30),
                        ("c5060", 10) => (0, 48),
                        _ => unreachable!(),
                    };
                    assert_eq!((hit.hp_damage, hit.posture_damage), expected);
                    let hp = duel.rules.fighters[0].vitality.max_hp;
                    let posture = duel.rules.fighters[0].posture.max;
                    duel.rules.fighters.iter_mut().for_each(|f| f.reset());
                    duel.rules
                        .set_attack_power(AttackPower::new(ap).unwrap())
                        .unwrap();
                    let residents = duel.rules.fighters[0].resident();
                    assert_eq!(
                        residents
                            .iter()
                            .filter(|id| (160001..=160099).contains(*id))
                            .count(),
                        1
                    );
                    assert_eq!((hp, posture), (320, 120));
                    found = true;
                    break;
                }
            }
            assert!(found, "slash must connect");
        }
    }
}

#[test]
fn every_attack_power_uses_the_extracted_growth_effect() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists() {
        return;
    }
    let mut duel = Duel::load(&cache, 1.6).unwrap();
    for ap in 1..=99 {
        let power = AttackPower::new(ap).unwrap();
        duel.rules.set_attack_power(power).unwrap();
        let effect = duel
            .rules
            .active_effects(0, &duel.player)
            .into_iter()
            .find(|e| e.id == power.effect_id())
            .unwrap();
        let x = f32::from(ap);
        let expected = match ap {
            1..=14 => 1.0 + 0.25 * (x - 1.0),
            15..=27 => 4.25 + 0.1 * (x - 14.0),
            28..=51 => 5.55 + 0.05 * (x - 27.0),
            52 => 6.775,
            _ => 6.75 + 0.01 * (x - 51.0),
        };
        for rate in effect
            .hp_attack_rates
            .into_iter()
            .chain([effect.stamina_attack_rate])
        {
            assert!(
                (rate - expected).abs() < 0.00001,
                "AP {ap}: {rate} vs {expected}"
            );
        }
    }
}

#[test]
fn settings_are_validated_and_override_presets() {
    assert_eq!(AttackPower::from_options(None, None).unwrap().value(), 1);
    assert_eq!(
        AttackPower::from_options(None, Some("castle-roof"))
            .unwrap()
            .value(),
        6
    );
    assert_eq!(
        AttackPower::from_options(Some("10"), Some("castle-roof"))
            .unwrap()
            .value(),
        10
    );
    for value in ["0", "100", "-1", "1.5", "oops"] {
        assert!(AttackPower::from_options(Some(value), None).is_err());
    }
    assert!(AttackPower::from_options(None, Some("unknown")).is_err());
}
