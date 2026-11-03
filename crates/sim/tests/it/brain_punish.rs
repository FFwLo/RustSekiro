//! Offline fixtures are built from installed data, never copied into source.
use rayon::prelude::*;
use sekiro_sim::brain::{BossActions, ScriptedBrain, moves};
use sekiro_sim::{InputFrame, duel::Duel, enemy::EnemySpec};

#[test]
fn real_break_recovery_and_boss_punish_sheets() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/tae.json").exists() || !cache.join("anim/c5060/tae.json").exists() {
        eprintln!("skipping: real Genichiro/Owl fixtures are not extracted");
        return;
    }
    let registry = sekiro_sim::registry::Registry::load(&cache).unwrap();
    [
        registry.resolve("c7100:71001000").unwrap(),
        EnemySpec::owl(),
    ]
    .into_par_iter()
    .for_each(|spec| {
        let mut d = Duel::load_with(&cache, 20.0, spec).unwrap();
        d.player.pending_damage = Some(sekiro_sim::player::IncomingDamage {
            damage_type: 1001,
            ..Default::default()
        });
        d.step(&InputFrame::default(), 1.0 / 60.0);
        let c = d.player.behavior.runtime.main_clip().unwrap();
        assert_eq!(c.animation, "a050_190100");
        let window = moves::recovery_window(&d.player, &c.animation).unwrap();
        assert!((window - 1.3).abs() < 0.001);
        for i in 1..90 {
            d.step(
                &InputFrame {
                    buttons: sekiro_sim::Buttons {
                        dodge: i % 2 == 0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            if i < 77 {
                assert_eq!(
                    d.player.behavior.runtime.main_clip().unwrap().animation,
                    "a050_190100"
                );
            }
        }
        assert_ne!(
            d.player.behavior.runtime.main_clip().unwrap().animation,
            "a050_190100"
        );
        let actions = BossActions::for_chr(&d.spec.chr).unwrap();
        d.set_brain(
            Box::new(ScriptedBrain::for_actions(actions).with_move_selection()),
            false,
        )
        .unwrap();
        let driver = d.boss_brain.as_ref().unwrap();
        let sheet = &driver.move_sheet;
        for (bucket, distance) in [("near", 1.5), ("far", 8.0)] {
            let list = moves::viable_punishes(
                sheet,
                actions,
                [distance, 0.0],
                [0.0; 2],
                window,
                &Default::default(),
            );
            assert!(!list.is_empty(), "{} {bucket}", d.spec.chr);
            assert!(list.iter().all(|p| p.first + moves::PUNISH_MARGIN < window));
            assert!(list.iter().all(|p| {
                sheet[&p.id]
                    .reachable_time([distance, 0.0], [0.0; 2])
                    .is_some()
            }));
            if bucket == "far" {
                assert!(list.iter().any(|p| !sheet[&p.id].projectiles.is_empty()));
            }
            let expected = match (d.spec.chr.as_str(), bucket) {
                ("c7100", "near") => "3000",
                ("c7100", _) => "3006",
                ("c5060", "near") => "3000+3001",
                _ => "3009",
            };
            assert!(list.iter().any(|p| p.id == expected), "missing {expected}");
            let fastest = list.iter().map(|p| p.first).min_by(f32::total_cmp).unwrap();
            let slower = list.iter().find(|p| p.first > fastest + 0.01).unwrap();
            let late = sheet.iter().find(|(id, row)| {
                actions.parse(id).is_ok()
                    && row
                        .reachable_time([distance, 0.0], [0.0; 2])
                        .is_some_and(|t| t + moves::PUNISH_MARGIN >= window)
            });
            let mut preferences = Vec::new();
            if let Some((id, _)) = late {
                preferences.push(sekiro_sim::brain::coach::Avoid {
                    choice: id.clone(),
                    reason: "late option must be ignored".into(),
                });
            }
            preferences.push(sekiro_sim::brain::coach::Avoid {
                choice: slower.id.clone(),
                reason: "follow-up pressure".into(),
            });
            let options: Vec<_> = list.iter().collect();
            let mut rng = sekiro_sim::brain::sampler::Rng::new(91);
            assert_eq!(
                moves::choose_punish(&options, &preferences, &mut rng)
                    .unwrap()
                    .id,
                slower.id
            );
            for _ in 0..100 {
                let picked = moves::choose_punish(&options, &[], &mut rng).unwrap();
                assert!(list.iter().any(|p| p.id == picked.id));
            }
            for (id, row) in sheet {
                if row
                    .reachable_time([distance, 0.0], [0.0; 2])
                    .is_some_and(|t| t + moves::PUNISH_MARGIN >= window)
                {
                    assert!(!list.iter().any(|p| &p.id == id), "late {id}");
                }
            }
            eprintln!(
                "{} {bucket} {} options: {:?}",
                d.spec.chr,
                list.len(),
                list.iter()
                    .take(4)
                    .map(|p| (&p.id, p.first, p.hits, p.window, p.stun_lock))
                    .collect::<Vec<_>>()
            );
            assert!(
                moves::viable_punishes(
                    sheet,
                    actions,
                    [distance, 0.0],
                    [0.0; 2],
                    0.01,
                    &Default::default()
                )
                .is_empty()
            );
        }
        let mut input = driver.coach_input(&Default::default());
        let bytes = input.fit(sekiro_sim::brain::coach::INPUT_BUDGET);
        assert!(bytes <= sekiro_sim::brain::coach::INPUT_BUDGET);
        eprintln!(
            "{} coach {bytes} bytes; raw move rows left {}",
            d.spec.chr,
            input.move_sheet.len()
        );
        assert!(input.viable_punishes.values().all(|p| !p.is_empty()));
        for (bucket, distance) in [("near", 1.5), ("mid", 4.0), ("far", 8.0)] {
            assert_eq!(
                input.viable_punishes[bucket],
                moves::viable_punishes(
                    sheet,
                    actions,
                    [distance, 0.0],
                    [0.0; 2],
                    window,
                    &Default::default()
                )
            );
        }
        let mut mature = input.clone();
        for option in mature.viable_punishes.values_mut().flatten() {
            option.evidence = moves::PunishEvidence {
                uses: 10_000,
                landed: 8_000,
                converted: 1_000,
                window_hp: 16_000_000.0,
                window_posture: 4_000_000.0,
            };
            option.conversion_rate = option.evidence.conversion_rate();
        }
        let full_lists = mature.viable_punishes.clone();
        assert!(
            mature.fit(sekiro_sim::brain::coach::INPUT_BUDGET)
                <= sekiro_sim::brain::coach::INPUT_BUDGET
        );
        assert_eq!(
            mature.viable_punishes, full_lists,
            "budget must trim other context, not viable punishes"
        );
        d.player.body.position = [
            d.enemy.body.position[0],
            d.enemy.body.position[1],
            d.enemy.body.position[2] + 1.5,
        ];
        let positions = [d.player.body.position, d.enemy.body.position];
        d.force_act = Some(3001);
        d.boss_brain.as_mut().unwrap().seed(91);
        d.boss_brain.as_mut().unwrap().game_plan = Some(sekiro_sim::brain::GamePlan {
            punish_preferences: std::collections::BTreeMap::from([(
                "near".into(),
                vec![sekiro_sim::brain::coach::Avoid {
                    choice: "3001".into(),
                    reason: "Repeat the known close-range slash after a break".into(),
                }],
            )]),
            ..Default::default()
        });
        let mut broke = false;
        for tick in 0..480 {
            if !broke {
                d.rules.fighters[0].posture.remaining = 1;
                d.player.body.position = positions[0];
                d.enemy.body.position = positions[1];
            }
            let report = d.step(
                &InputFrame {
                    buttons: sekiro_sim::Buttons {
                        guard: tick != 1 && tick != 3,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            broke |= report
                .hits
                .iter()
                .any(|(h, r)| h.defender == 0 && r.posture_broke);
            if broke {
                d.force_act = None;
            }
        }
        assert!(
            broke,
            "boss must break Wolf through an actual guarded contact"
        );
        let driver = d.boss_brain.as_mut().unwrap();
        driver.end_fight(sekiro_sim::brain::FightEnd::Unfinished);
        let outcomes = driver
            .memory
            .punishes
            .get(&d.spec.chr)
            .expect("actual break response must record punish evidence");
        assert!(
            outcomes
                .values()
                .flat_map(|m| m.values())
                .any(|e| e.uses > 0)
        );
        eprintln!("{} real punish outcomes {outcomes:?}", d.spec.chr);
    });
}
