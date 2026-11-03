//! Real TAE launches, bullet flight and Wolf's installed guard scripts.

use sekiro_sim::{
    Buttons, InputFrame, brain::BrainKind, duel::Duel, enemy::EnemySpec, registry::Registry,
};

const DT: f32 = 1.0 / 60.0;

fn projectile_guard(chr: &str, act: i32, atk_id: i32, deflect: bool, posture: i32) {
    let cache = sekiro_sim::cache::root();
    if !cache.join(format!("anim/{chr}/skeleton.bin")).exists() {
        eprintln!("skipping: {chr} is not extracted");
        return;
    }
    let spec = if chr == "c5060" {
        EnemySpec::owl()
    } else {
        Registry::load(&cache)
            .unwrap()
            .resolve("c7100:71001000")
            .unwrap()
    };
    let mut duel = Duel::load_with(&cache, 6.0, spec).unwrap();
    duel.set_brain(BrainKind::Repeat(act).build(chr), false)
        .unwrap();
    let mut pressed = !deflect;
    for tick in 0..1800 {
        // Press just before the airborne projectile reaches Wolf, rather than mocking a hit.
        if deflect
            && duel.bullets.live.iter().any(|b| {
                b.def.attack.as_ref().is_some_and(|a| a.atk_id == atk_id)
                    && (b.position - glam::Vec3::from_array(duel.player.body.position)).length()
                        < 2.5
            })
        {
            pressed = true;
        }
        let report = duel.step(
            &InputFrame {
                buttons: Buttons {
                    guard: pressed,
                    lock_on: tick == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        for (hit, result) in &report.hits {
            if hit.attack.atk_id != atk_id {
                continue;
            }
            eprintln!("{chr} act {act} tick {tick}: {result:?}");
            assert!(result.guarded);
            assert_eq!(result.deflected, deflect);
            assert_eq!(result.hp_damage, 0);
            // Includes Wolf's native 1-percent guard cut bias and integer truncation.
            assert_eq!(result.posture_damage, posture);
            assert_eq!(result.attacker_posture_damage, 0);
            assert!(result.attacker.is_none(), "projectile recoiled its shooter");
            assert!(duel.enemy.pending_damage.is_none());
            // The same attack resolved as a character contact still produces melee recoil.
            // Nonzero feedback belongs to that contacting attacker, never the distant shooter.
            let mut contact = hit.clone();
            if chr == "c5060" && deflect {
                contact.attack = duel.fighters[1].bullets[&131].attack.clone().unwrap();
                assert_eq!(
                    contact
                        .attack
                        .profile
                        .as_ref()
                        .unwrap()
                        .attacker_deflected_stam,
                    30
                );
            } else {
                let profile = contact.attack.profile.as_mut().unwrap();
                profile.attacker_deflected_stam = 30;
                profile.attacker_blocked_stam = 30;
            }
            let before = duel.rules.fighters[1].posture.remaining;
            let projectile = duel.rules.resolve_bullet(
                &contact,
                [&duel.player, &duel.enemy],
                [&duel.fighters[0], &duel.fighters[1]],
            );
            assert!(projectile.attacker.is_none());
            assert_eq!(projectile.attacker_posture_damage, 0);
            assert_eq!(duel.rules.fighters[1].posture.remaining, before);
            // Even an already empty shooter meter must not become a projectile break.
            duel.rules.fighters[1].posture.remaining = 0;
            let empty = duel.rules.resolve_bullet(
                &contact,
                [&duel.player, &duel.enemy],
                [&duel.fighters[0], &duel.fighters[1]],
            );
            assert!(!empty.posture_broke);
            assert!(empty.attacker.is_none());
            duel.rules.fighters[1].posture.remaining = before;
            let melee = duel.rules.resolve(
                &contact,
                [&duel.player, &duel.enemy],
                [&duel.fighters[0], &duel.fighters[1]],
            );
            assert!(melee.attacker.is_some());
            assert!(melee.attacker_posture_damage > 0);
            let clip = report.enemy.animation.clone();
            let next = duel.step(&InputFrame::default(), DT);
            assert_eq!(
                next.enemy.animation, clip,
                "contact interrupted the thrower's clip"
            );
            assert!(
                next.player
                    .behavior
                    .state_path
                    .iter()
                    .any(|s| s.contains("Deflect"))
            );
            return;
        }
    }
    panic!("{chr} act {act} never landed AtkParam {atk_id}");
}

#[test]
fn owl_shuriken_deflect_keeps_throwing() {
    projectile_guard("c5060", 3014, 50600150, true, 5);
}

#[test]
fn owl_shuriken_block_keeps_throwing() {
    projectile_guard("c5060", 3014, 50600150, false, 11);
}

#[test]
fn genichiro_arrow_deflect_keeps_shooting() {
    projectile_guard("c7100", 3009, 71000500, true, 8);
}

#[test]
fn genichiro_arrow_block_keeps_shooting() {
    projectile_guard("c7100", 3009, 71000500, false, 17);
}
