//! Headless real-data reproduction of the jump/thrust and its learning signal.

use sekiro_sim::brain::{LuaBrain, tally::Side};
use sekiro_sim::{Buttons, InputFrame, duel::Duel, registry::Registry};

#[test]
fn genichiro_mikiri_replay_attributes_the_thrust_and_jump_callback() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/skeleton.bin").exists() {
        eprintln!("skipping: Genichiro is not extracted");
        return;
    }
    let registry = Registry::load(&cache).unwrap();
    for act in [3025, 3076] {
        let spec = registry.resolve("c7100:71001000").unwrap();
        let mut duel = Duel::load_with(&cache, 2.0, spec).unwrap();
        duel.god = true;
        duel.enemy_active = false;
        duel.set_brain(Box::new(LuaBrain), false).unwrap();
        let contact = duel
            .boss_brain
            .as_ref()
            .unwrap()
            .move_sheet
            .get(&act.to_string())
            .and_then(|m| m.hits.first())
            .map_or(1.1, |h| h[0]);
        let mut stepped = false;
        let mut counter = false;
        let mut observed = Vec::new();
        for frame in 0..300 {
            duel.force_act = (frame < 10).then_some(act);
            let clock = duel
                .enemy
                .behavior
                .runtime
                .main_clip()
                .map_or(0.0, |c| c.time);
            // Forward dodge shortly before the thrust's authored contact window.
            let dodge = !stepped && clock >= (contact - 0.2).max(0.0);
            stepped |= dodge;
            let report = duel.step(
                &InputFrame {
                    move_stick: if dodge { [0.0, 1.0] } else { [0.0; 2] },
                    buttons: Buttons {
                        dodge,
                        lock_on: frame == 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            counter |= report.mikiri.is_some();
            if report.enemy.clip_started {
                observed.push(
                    duel.enemy
                        .behavior
                        .runtime
                        .main_clip()
                        .map(|c| c.animation.clone()),
                );
            }
        }
        let driver = duel.boss_brain.as_ref().unwrap();
        let counts = driver.tally.counts(Side::Boss, &act.to_string());
        eprintln!(
            "Genichiro {act}: first contact={contact}, Mikiri={counter}, counts={counts:?}, clips={observed:?}"
        );
        if act == 3025 {
            assert!(counter, "real thrust must be countered");
            assert_eq!(counts.countered, 1);
            assert_eq!(counts.whiffed, 0);
        }
    }
}
