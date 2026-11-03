use std::collections::BTreeSet;

use sekiro_sim::{
    InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
};

const DT: f32 = 1.0 / 60.0;

fn load(name: &str) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5060/tae.json",
        "anim/c5070/tae.json",
        "maps/m10_00_00_00/collision.bin",
        "raw/script/m10_00_00_00.luabnd.d/506000_battle.lua",
    ] {
        if !cache.join(path).exists() {
            eprintln!("skipping Father: missing {path}");
            return None;
        }
    }
    let spec = ArenaSpec::by_name(name).unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

#[test]
fn owl_father_native_lua_fights_in_hirata() {
    for name in ["hirata-father", "hirata-inner-father"] {
        let Some(mut duel) = load(name) else { return };
        duel.set_brain(Box::new(sekiro_sim::brain::ScriptedBrain::default()), false)
            .unwrap();
        assert!(
            duel.boss_brain.is_none(),
            "Father must retain his native battle goal"
        );
        let mut actions = BTreeSet::new();
        let mut unknown = BTreeSet::new();
        let mut contacts = 0;
        let mut clips = BTreeSet::new();
        for _ in 0..1800 {
            let report = duel.step(&InputFrame::default(), DT);
            assert!(
                report.enemy.behavior.script_errors.is_empty(),
                "{:?}",
                report.enemy.behavior.script_errors
            );
            assert!(
                report.player.behavior.script_errors.is_empty(),
                "{:?}",
                report.player.behavior.script_errors
            );
            contacts += report
                .hits
                .iter()
                .filter(|(hit, _)| hit.attacker == 1)
                .count();
            if let Some(clip) = report.enemy.animation {
                clips.insert(clip);
            }
            for event in duel.ai.as_mut().expect("native Lua AI").drain_log() {
                match event {
                    AiEvent::Action(action) if (3000..3100).contains(&action) => {
                        actions.insert(action);
                    }
                    AiEvent::Error(error) => panic!("{name}: {error}"),
                    AiEvent::Unknown(call) => {
                        unknown.insert(call);
                    }
                    _ => {}
                }
            }
        }
        eprintln!(
            "{name}: actions {actions:?}, contacts {contacts}, unknown {unknown:?}, clips {clips:?}"
        );
        assert!(actions.len() >= 3, "{name}: {actions:?}");
        assert!(contacts > 0, "{name}: no real attack contacts");
        assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
        assert!(duel.events.as_ref().unwrap().boss_bar.is_some());
    }
}

#[test]
#[ignore = "slow: two installed-data Father phases with native Lua and paired executions"]
fn owl_father_both_marks_reach_map_defeat() {
    for (name, flag) in [("hirata-father", 9317), ("hirata-inner-father", 9346)] {
        both_marks(name, flag);
    }
}

fn both_marks(name: &str, defeat: i32) {
    let Some(mut duel) = load(name) else {
        return;
    };
    duel.enemy_active = false;
    for mark in [2, 1] {
        duel.enemy_active = false;
        if mark == 1 {
            // Phase-two Lua and the bird were exercised above; isolate the final sword pair.
            duel.father_bird.as_mut().unwrap().duel.enemy_active = false;
            for _ in 0..360 {
                duel.step(&InputFrame::default(), DT);
            }
        }
        let p = duel.enemy.body.position;
        duel.player.body.position = [p[0], p[1], p[2] + 1.6];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut broke = false;
        let mut pair = None;
        let mut clips = BTreeSet::new();
        let mut wolf_clips = BTreeSet::new();
        for i in 0..1800 {
            if !broke {
                duel.rules.fighters[1].posture.remaining = 1;
            }
            let report = duel.step(
                &InputFrame {
                    buttons: sekiro_sim::Buttons {
                        attack: if broke {
                            pair.is_none() && i % 10 == 0
                        } else {
                            i % 40 == 0
                        },
                        lock_on: (2..4).contains(&i) && mark == 2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(report.enemy.behavior.script_errors.is_empty());
            if i == 0 {
                assert!(
                    !duel.player.behavior.env.sp_effect_refs.contains(&110005),
                    "Hirata's shallow floor must allow sword combat"
                );
            }
            if let Some(c) = &report.player.animation {
                wolf_clips.insert(c.clone());
            }
            broke |= report
                .hits
                .iter()
                .any(|(h, r)| h.attacker == 0 && r.posture_broke);
            pair = pair.or(report.deathblow.started);
            if let Some(c) = report.enemy.animation {
                clips.insert(c);
            }
            if pair.is_some()
                && duel.deathblow.is_none()
                && duel.rules.fighters[1].vitality.deathblows_left == mark - 1
            {
                break;
            }
        }
        duel.enemy_active = true;
        let mut contacts = 0;
        let mut bird_actions = BTreeSet::new();
        let mut bird_unknown = BTreeSet::new();
        let mut bird_clips = BTreeSet::new();
        for _ in 0..1800 {
            let report = duel.step(&InputFrame::default(), DT);
            contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
            if let Some(c) = report.enemy.animation {
                clips.insert(c);
            }
            for event in duel.ai.as_mut().unwrap().drain_log() {
                if let AiEvent::Error(e) = event {
                    panic!("{e}");
                }
            }
            let bird = duel.father_bird.as_mut().unwrap();
            if let Some(clip) = &bird.report.enemy.animation {
                bird_clips.insert(clip.clone());
            }
            assert!(bird.report.enemy.behavior.script_errors.is_empty());
            for event in bird.duel.ai.as_mut().unwrap().drain_log() {
                match event {
                    AiEvent::Action(a) => {
                        bird_actions.insert(a);
                    }
                    AiEvent::Unknown(call) => {
                        bird_unknown.insert(call);
                    }
                    AiEvent::Error(error) => panic!("bird: {error}"),
                    _ => {}
                }
            }
        }
        eprintln!(
            "Father mark {mark}: broke {broke}, pair {pair:?}, clips {clips:?}, wolf {wolf_clips:?}, positions {:?} {:?}, contacts {contacts}, vitality {:?}, flags {:?}",
            duel.player.body.position,
            duel.enemy.body.position,
            duel.rules.fighters[1].vitality,
            duel.events.as_ref().unwrap().scripts.flags
        );
        let bird = duel.father_bird.as_ref().unwrap();
        eprintln!(
            "Bird enabled {}, active {}, target {}, actions {bird_actions:?}, unknown {bird_unknown:?}, clips {bird_clips:?}, requests {:?}, effects {:?}",
            bird.enabled,
            bird.duel.enemy_active,
            bird.duel.ai.as_ref().unwrap().state.target_state,
            bird.duel.ai.as_ref().unwrap().state.event_requests,
            bird.duel.enemy.tae_frame().sp_effects
        );
        assert!(broke && pair.is_some());
        assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, mark - 1);
        if mark == 2 {
            assert_eq!(
                duel.rules.fighters[1].vitality.hp,
                duel.rules.fighters[1].vitality.max_hp
            );
            assert!(duel.enemy.tae_frame().sp_effects.contains(&200051));
            assert!(contacts > 0);
            assert!(
                clips.contains("a000_003035"),
                "native bird return must play Father's falling attack"
            );
            assert!(
                bird_actions.iter().any(|a| (3000..3100).contains(a)),
                "native bird must execute its own actions"
            );
            assert!(bird_clips.contains("a000_020000"));
            assert!(bird_unknown.is_empty(), "{bird_unknown:?}");
        } else {
            assert!(duel.events.as_ref().unwrap().scripts.flag(defeat), "{name}");
        }
    }
}

#[test]
fn owl_father_thrust_uses_native_mikiri_pair() {
    for name in ["hirata-father", "hirata-inner-father"] {
        let Some(arena_duel) = load(name) else { return };
        let cache = sekiro_sim::cache::root();
        // Isolate the authored attack and counter on flat ground; arena combat is tested above.
        let mut duel = Duel::load_with(&cache, 2.0, arena_duel.spec.clone()).unwrap();
        duel.enemy_active = false;
        let mut counter = None;
        let hp = duel.rules.fighters[0].vitality.hp;
        for i in 0..260 {
            duel.force_act = (i < 10).then_some(3033);
            let report = duel.step(
                &InputFrame {
                    move_stick: if i == 75 { [0.0, 1.0] } else { [0.0; 2] },
                    buttons: sekiro_sim::Buttons {
                        dodge: i == 75,
                        lock_on: i == 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(report.player.behavior.script_errors.is_empty());
            assert!(report.enemy.behavior.script_errors.is_empty());
            counter = counter.or(report.mikiri);
        }
        assert_eq!(counter, Some((15060190, 90)), "{name}");
        assert_eq!(duel.rules.fighters[0].vitality.hp, hp);
    }
}
