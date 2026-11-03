//! Real-data Butterfly combat and event hand-over, without external controllers.

use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::duel::Duel;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
#[ignore = "slow: eight simultaneous extracted illusion Lua/HKS actors"]
fn butterfly_illusion_lua_contacts_and_authored_release_pulse() {
    let Some(mut duel) = load_phase("c5090:50900001") else {
        return;
    };
    enter_second_phase(&mut duel);
    duel.enemy_active = true;
    // The actual summon event waits on this boss SpEffect; do not replace its actor enables.
    duel.enemy.tae_runtime.add_timed(200060, f32::INFINITY);
    let mut actions = std::collections::BTreeSet::new();
    let mut attacks = std::collections::BTreeSet::new();
    let mut unknown = std::collections::BTreeSet::new();
    let mut pulse_contact = false;
    let mut shared_release_flag = false;
    let mut release_clips = std::collections::BTreeSet::new();
    let mut release_bullets = std::collections::BTreeSet::new();
    let mut conversion_clips = std::collections::BTreeSet::new();
    for tick in 0..1800 {
        if tick == 900 {
            // Authored release action emits Bullet 50900921 through its TAE.
            duel.force_act = Some(3020);
            duel.enemy_active = true;
        }
        duel.step(&InputFrame::default(), DT);
        let flags = &duel.events.as_ref().unwrap().scripts.flags;
        shared_release_flag |= flags.contains(&19952081) || flags.contains(&19952082);
        let position = duel.player.body.position;
        for actor in &mut duel.butterfly_illusions.as_mut().unwrap().actors {
            for bullet in &actor.duel.bullets.live {
                if bullet.shooter == 1 && (15_000_800..=15_000_822).contains(&bullet.def.id) {
                    release_bullets.insert(bullet.def.id);
                    if bullet.def.id % 10 == 0 && bullet.age <= DT + 0.0001 {
                        release_clips.insert(actor.entity);
                        if let Some(clip) = &actor.report.enemy.animation {
                            conversion_clips.insert(clip.clone());
                        }
                    }
                }
            }
            pulse_contact |= actor.duel.enemy.tae_frame().sp_effects.contains(&3509070);
            // Isolate contact timing from the unhosted event-relative navigation points.
            if tick < 900
                && attacks.is_empty()
                && actor.report.enemy.clip_started
                && actor.report.enemy.animation.as_deref().is_some_and(|clip| {
                    clip.rsplit('_')
                        .next()
                        .and_then(|id| id.parse::<i32>().ok())
                        .is_some_and(|id| (3000..3020).contains(&id))
                })
            {
                actor.duel.enemy.body.position = [position[0], position[1], position[2] - 1.2];
                actor.duel.enemy.body.yaw = std::f32::consts::PI;
            }
            for event in actor.duel.ai.as_mut().unwrap().drain_log() {
                match event {
                    AiEvent::Action(action) => {
                        actions.insert(action);
                    }
                    AiEvent::Unknown(call) => {
                        unknown.insert(call);
                    }
                    AiEvent::Error(error) => panic!("illusion Lua: {error}"),
                    _ => {}
                }
            }
            for (hit, _) in &actor.report.hits {
                if hit.attacker == 1 {
                    attacks.insert(hit.attack.atk_id);
                }
            }
            assert!(actor.report.enemy.behavior.script_errors.is_empty());
        }
    }
    eprintln!("Illusion actions {actions:?}; attacks {attacks:?}; unknown {unknown:?}");
    eprintln!(
        "Release actors {release_clips:?}; clips {conversion_clips:?}; bullets {release_bullets:?}"
    );
    for actor in &duel.butterfly_illusions.as_ref().unwrap().actors {
        eprintln!(
            "{} {:?} {:?} hp {}",
            actor.entity,
            actor.report.enemy.animation,
            actor.duel.enemy.body.position,
            actor.duel.rules.fighters[1].vitality.hp
        );
    }
    assert!(
        duel.butterfly_illusions
            .as_ref()
            .unwrap()
            .actors
            .iter()
            .any(|a| a.duel.enemy_active)
    );
    assert!(!actions.is_empty());
    assert!(!attacks.is_empty(), "no illusion combat contacts");
    assert!(
        pulse_contact,
        "the authored release pulse never reached an illusion"
    );
    assert!(
        shared_release_flag,
        "Butterfly Lua did not publish the shared release flags"
    );
    assert!(!unknown.contains("ai:SetEnableImmediateAction_forGoal"));
    assert!(!unknown.contains("ai:ClearEnableImmediateAction_forGoal"));
    eprintln!("Illusion release clips {release_clips:?}; damaging bullets {release_bullets:?}");
    assert_eq!(
        release_clips.len(),
        8,
        "the release never interrupted illusion combat"
    );
    assert_eq!(
        release_bullets,
        std::collections::BTreeSet::from([
            15000800, 15000801, 15000802, 15000810, 15000811, 15000812, 15000820, 15000821,
            15000822,
        ]),
        "illusion conversion did not emit every installed child family"
    );
}

fn load() -> Option<Duel> {
    load_phase("c5090:50900000")
}

fn enter_second_phase(duel: &mut Duel) {
    let cache = sekiro_sim::cache::root();
    // Wolf has to enter the map's actual gate region before Lua can request the descent.
    duel.player.body.position = sekiro_sim::events::warp_point(&cache, 10, 0, 1002830)
        .unwrap()
        .position;
    let point = &mut duel.player.body.position;
    point[1] = duel
        .player
        .ground
        .floor(point[0], point[2], point[1] + 10.0)
        .expect("the entrance-region fixture needs the actual hall floor");
    duel.player.body.velocity = [0.0; 3];
    duel.player.body.grounded = true;
    duel.player.body.fall_top = point[1];
    let mut entrance = false;
    let mut descent = false;
    for _ in 0..1200 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(report.enemy.behavior.script_errors.is_empty());
        entrance |= report.enemy.animation.as_deref() == Some("a100_021000");
        descent |= report.enemy.animation.as_deref() == Some("a100_003290");
        if duel.events.as_ref().unwrap().scripts.flag(11005802) {
            break;
        }
    }
    assert!(
        entrance && descent,
        "native phase-two entrance/descent clips did not play"
    );
    assert!(duel.events.as_ref().unwrap().scripts.flag(11005802));
    assert!(!duel.rules.fighters[1].resident().contains(&3509210));
    assert_eq!(
        duel.ai.as_ref().unwrap().state.event_requests.get(&0),
        Some(&-1)
    );
}

fn load_phase(selection: &str) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5090/tae.json",
        "anim/c1500/tae.json",
        "anim/c1500/skeleton.bin",
        "anim/c0000/tae.json",
        "maps/m10_00_00_00/collision.bin",
        "raw/event/m10_00_00_00.emevd",
        "params/NpcParam.json",
        "raw/script/m10_00_00_00.luabnd.d/509000_battle.lua",
        "raw/script/m10_00_00_00.luabnd.d/151010_battle.lua",
        "raw/script/aicommon.luabnd.d/ai_define.lua",
    ] {
        if !cache.join(path).is_file() {
            eprintln!("skipping Butterfly: missing {}", cache.join(path).display());
            return None;
        }
    }
    let spec = ArenaSpec::by_name("hirata-hall").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let enemy = sekiro_sim::enemy::EnemySpec::from_cache(&cache, selection).unwrap();
    let mut duel = Duel::load_with(&cache, 1.6, enemy).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

fn face_off(duel: &mut Duel) {
    duel.enemy_active = false;
    for _ in 0..240 {
        duel.step(&InputFrame::default(), DT);
    }
    let p = duel.player.body.position;
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
    duel.enemy.body.yaw = std::f32::consts::PI;
}

#[test]
fn butterfly_lua_uses_planar_distance_and_lands_real_attacks() {
    let Some(mut duel) = load() else { return };
    let mut actions = std::collections::BTreeSet::new();
    let mut hits = std::collections::BTreeSet::new();
    for _ in 0..1800 {
        let r = duel.step(&InputFrame::default(), DT);
        assert!(r.enemy.behavior.script_errors.is_empty());
        for (h, _) in r.hits {
            if h.attacker == 1 {
                hits.insert(h.attack.atk_id);
            }
        }
        for event in duel.ai.as_mut().expect("real Butterfly Lua AI").drain_log() {
            match event {
                AiEvent::Action(action) => {
                    actions.insert(action);
                }
                AiEvent::Error(error) => panic!("Butterfly Lua: {error}"),
                AiEvent::Unknown(call) if call == "ai:GetDistXZ" => {
                    panic!("unhosted planar distance")
                }
                _ => {}
            }
        }
    }
    eprintln!("Butterfly Lua actions {actions:?}; landed AtkParam rows {hits:?}");
    assert!(!actions.is_empty(), "no Lua decisions");
    assert!(
        !hits.is_empty(),
        "no attacks contact Wolf on Hirata collision"
    );
}

#[test]
#[ignore = "slow: eight illusion runtimes and both paired boss executions"]
fn butterfly_deathblows_hand_over_then_finish_the_map_fight() {
    let Some(mut duel) = load() else { return };
    for (row, entity) in [(50_900_000, 1_000_800), (50_900_001, 1_000_810)] {
        assert_eq!(duel.spec.npc_param, row);
        assert_eq!(duel.events.as_ref().unwrap().boss_entity, entity);
        assert_eq!(
            duel.enemy_bar_marks(),
            if row == 50_900_000 { 2 } else { 1 }
        );
        if row == 50_900_001 {
            assert!(duel.rules.fighters[1].resident().contains(&200_051));
            enter_second_phase(&mut duel);
        }
        face_off(&mut duel);
        eprintln!(
            "initial vitality {:?}, posture {:?}, opening={}",
            duel.rules.fighters[1].vitality,
            duel.rules.fighters[1].posture,
            duel.deathblow_available()
        );
        let mut started = None;
        let mut broke = false;
        let mut execution = false;
        let mut defender_execution = false;
        for i in 0..1800 {
            if !broke {
                duel.rules.fighters[1].posture.remaining =
                    duel.rules.fighters[1].posture.remaining.min(2);
            }
            let r = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        attack: i % 20 == 0,
                        lock_on: i == 2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(
                r.player.behavior.script_errors.is_empty(),
                "{:?}",
                r.player.behavior.script_errors
            );
            assert!(
                r.enemy.behavior.script_errors.is_empty(),
                "{:?}",
                r.enemy.behavior.script_errors
            );
            broke |= r
                .hits
                .iter()
                .any(|(h, result)| h.attacker == 0 && result.posture_broke);
            started = started.or(r.deathblow.started);
            if r.deathblow.hit {
                assert_eq!(
                    duel.enemy_bar_marks(),
                    if row == 50_900_000 { 1 } else { 0 }
                );
            }
            execution |= r.player.animation.as_deref() == Some("a000_710205");
            defender_execution |= r.enemy.animation.as_deref() == Some("a100_013700");
            if row == 50_900_000 && duel.spec.npc_param == 50_900_001 {
                break;
            }
            if row == 50_900_001 && duel.events.as_ref().unwrap().defeated {
                break;
            }
        }
        eprintln!(
            "Butterfly row {row}: broke={broke}, throw={started:?}, next={}, clip={:?}",
            duel.spec.npc_param,
            duel.enemy.behavior.runtime.main_clip()
        );
        // The reduced meter exercises the real throw and event paths independently of AI.
        assert!(started.is_some(), "no paired deathblow");
        if row == 50_900_000 {
            assert_eq!(
                duel.spec.npc_param, 50_900_001,
                "no event-driven phase hand-over"
            );
        } else {
            assert!(
                duel.events.as_ref().unwrap().defeated,
                "no map defeat event"
            );
            assert_eq!(started, Some((15_090_680, 15_090_681)));
            assert!(
                defender_execution,
                "finishing defender animation group was lost"
            );
            assert!(
                execution,
                "player execution request was not consumed by HKS"
            );
            assert!(duel.events.as_ref().unwrap().scripts.flag(9302));
        }
    }
}
