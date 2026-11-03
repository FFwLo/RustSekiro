//! Scripted Wolf inputs against the mounted boss's installed Lua, TAE and map events.
use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
};
use std::collections::BTreeSet;

const DT: f32 = 1.0 / 60.0;

fn load() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for file in [
        "anim/c5080/skeleton.bin",
        "anim/c5080/tae.json",
        "anim/c0000/tae.json",
        "raw/script/m11_00_00_00.luabnd.d/508000_battle.lua",
        "maps/m11_00_00_00/collision.bin",
        "raw/event/m11_00_00_00.emevd",
    ] {
        if !cache.join(file).is_file() {
            eprintln!("skipping Gyoubu: missing {file}");
            return None;
        }
    }
    let spec = ArenaSpec::by_name("gyoubu").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 1.6, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

#[test]
fn gyoubu_lua_attacks_in_both_mounted_phases_and_finishes() {
    let Some(mut duel) = load() else { return };
    for marks in [2, 1] {
        assert_eq!(duel.enemy_bar_marks(), marks);
        duel.enemy_active = true;
        let mut actions = BTreeSet::new();
        let mut hits = BTreeSet::new();
        let mut unknown = BTreeSet::new();
        for _ in 0..1800 {
            let report = duel.step(&InputFrame::default(), DT);
            assert!(report.enemy.behavior.script_errors.is_empty());
            for (hit, _) in report.hits {
                if hit.attacker == 1 {
                    hits.insert(hit.attack.atk_id);
                }
            }
            for event in duel.ai.as_mut().expect("Gyoubu Lua").drain_log() {
                match event {
                    AiEvent::Action(action) => {
                        actions.insert(action);
                    }
                    AiEvent::Unknown(call) => {
                        unknown.insert(call);
                    }
                    AiEvent::Error(error) => panic!("Gyoubu Lua: {error}"),
                    _ => {}
                }
            }
        }
        eprintln!("Gyoubu {marks} marks: actions {actions:?}; hits {hits:?}; unknown {unknown:?}");
        assert!(!hits.is_empty(), "mounted phase never contacts Wolf");
        duel.enemy_active = false;
        for _ in 0..240 {
            duel.step(&InputFrame::default(), DT);
        }
        let e = duel.enemy.body.position;
        duel.player.body.position = [e[0], e[1], e[2] + 1.6];
        let p = duel.player.body.position;
        duel.player.body.yaw = 0.0;
        duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut started = None;
        let mut refilled = false;
        let mut broke = false;
        let mut finishing_player_clip = false;
        let mut finishing_boss_clip = false;
        for tick in 0..1800 {
            if !broke {
                duel.rules.fighters[1].posture.remaining = 1;
            }
            let report = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        attack: if broke {
                            started.is_none() && tick % 10 == 0
                        } else {
                            tick % 40 == 0
                        },
                        lock_on: tick == 2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            started = started.or(report.deathblow.started);
            finishing_player_clip |= report.player.animation.as_deref() == Some("a225_511610");
            finishing_boss_clip |= report.enemy.animation.as_deref() == Some("a000_013610");
            broke |= report
                .hits
                .iter()
                .any(|(h, r)| h.attacker == 0 && r.posture_broke);
            assert!(report.enemy.behavior.script_errors.is_empty());
            assert!(report.player.behavior.script_errors.is_empty());
            refilled |= duel.rules.fighters[1].vitality.deathblows_left == 1
                && duel.rules.fighters[1].vitality.hp == duel.rules.fighters[1].vitality.max_hp;
            if marks == 2 && refilled && tick > 600 {
                break;
            }
            if marks == 1 && duel.events.as_ref().unwrap().defeated {
                break;
            }
        }
        eprintln!(
            "Gyoubu {marks} marks: throw {started:?}, refilled {refilled}, vitality {:?}, posture {:?}, positions {:?}/{:?}, clip {:?}, opening {}",
            duel.rules.fighters[1].vitality,
            duel.rules.fighters[1].posture,
            duel.player.body.position,
            duel.enemy.body.position,
            duel.enemy.state(),
            duel.deathblow_available()
        );
        assert_eq!(
            started,
            Some(if marks == 2 {
                (15080000, 15080001)
            } else {
                (15080170, 15080171)
            })
        );
        if marks == 2 {
            assert!(refilled, "mounted get-up did not restore HP");
            assert!(duel.events.as_ref().unwrap().scripts.flag(11105802));
        } else {
            assert!(
                finishing_player_clip && finishing_boss_clip,
                "finishing clips did not play"
            );
            assert!(duel.events.as_ref().unwrap().scripts.flag(9301));
            assert!(duel.events.as_ref().unwrap().defeated);
        }
    }
}
