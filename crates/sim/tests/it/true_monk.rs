use sekiro_sim::Buttons;
use sekiro_sim::{
    InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
};

const DT: f32 = 1.0 / 60.0;

fn load() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5000/tae.json",
        "anim/c0000/tae.json",
        "maps/m25_00_00_00/collision.bin",
        "raw/event/m25_00_00_00.emevd",
        "raw/script/m25_00_00_00.luabnd.d/500000_battle.lua",
        "raw/chr/c5005.chrbnd.d/c5005.flver",
    ] {
        if !cache.join(path).exists() {
            eprintln!("skipping True Monk: missing {path}");
            return None;
        }
    }
    let spec = ArenaSpec::by_name("fountainhead-bridge").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

#[test]
#[ignore = "slow: three installed-data phases and simultaneous phantom attacks"]
fn true_monk_deathblows_drive_all_three_phases() {
    let Some(mut duel) = load() else { return };
    duel.enemy_active = false;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), DT);
    }
    for mark in (1..=3).rev() {
        let p = duel.enemy.body.position;
        duel.player.body.position = [p[0], p[1], p[2] + 1.6];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut broke = false;
        let mut pair = None;
        let mut clips = std::collections::BTreeSet::new();
        for i in 0..1800 {
            if !broke {
                duel.rules.fighters[1].posture.remaining = 1;
            }
            let report = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        attack: if broke {
                            pair.is_none() && i % 10 == 0
                        } else {
                            i % 40 == 0
                        },
                        lock_on: i == 2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(
                report.enemy.behavior.script_errors.is_empty(),
                "{:?}",
                report.enemy.behavior.script_errors
            );
            broke |= report
                .hits
                .iter()
                .any(|(h, r)| h.attacker == 0 && r.posture_broke);
            pair = pair.or(report.deathblow.started);
            if let Some(c) = duel.enemy.behavior.runtime.main_clip() {
                clips.insert(c.animation.clone());
            }
            if pair.is_some()
                && duel.deathblow.is_none()
                && duel.rules.fighters[1].vitality.deathblows_left == mark - 1
            {
                break;
            }
        }
        for _ in 0..600 {
            duel.step(&InputFrame::default(), DT);
            if let Some(c) = duel.enemy.behavior.runtime.main_clip() {
                clips.insert(c.animation.clone());
            }
        }
        eprintln!(
            "True Monk mark {mark}: pair {pair:?}, clips {clips:?}, hp {:?}, commands {:?}, flags {:?}",
            duel.rules.fighters[1].vitality,
            duel.ai.as_ref().unwrap().state.event_requests,
            duel.events.as_ref().unwrap().scripts.flags
        );
        assert!(broke && pair.is_some());
        assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, mark - 1);
        if mark > 1 {
            let expected = if mark == 3 { (1916, 672) } else { (2714, 840) };
            assert_eq!(
                (
                    duel.rules.fighters[1].vitality.max_hp,
                    duel.rules.fighters[1].posture.max
                ),
                expected
            );
            assert_eq!(
                duel.rules.fighters[1].vitality.hp,
                duel.rules.fighters[1].vitality.max_hp
            );
            assert!(!duel.rules.fighters[1].dead());
            assert!(clips.contains("a000_012000"));
            if mark == 2 {
                assert!(clips.contains("a000_020015"));
            }
            duel.enemy_active = true;
            if mark == 3 {
                let cache = sekiro_sim::cache::root();
                let spot = sekiro_sim::events::warp_point(&cache, 25, 0, 2_502_856).unwrap();
                duel.player.body.position = spot.position;
                duel.player.body.yaw = spot.yaw;
            }
            let mut contacts = 0;
            let mut phantom_acts = 0;
            let mut phantom_contacts = 0;
            for _ in 0..3600 {
                let report = duel.step(&InputFrame::default(), DT);
                contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
                for event in duel.ai.as_mut().unwrap().drain_log() {
                    if let AiEvent::Error(e) = event {
                        panic!("{e}");
                    }
                }
                for actor in &mut duel.monk_phantoms.as_mut().unwrap().actors {
                    assert!(actor.report.enemy.behavior.script_errors.is_empty());
                    phantom_contacts += actor
                        .report
                        .hits
                        .iter()
                        .filter(|(h, _)| h.attacker == 1)
                        .count();
                    for event in actor.duel.ai.as_mut().unwrap().drain_log() {
                        match event {
                            AiEvent::Action(a) if (3000..3100).contains(&a) => phantom_acts += 1,
                            AiEvent::Error(e) => panic!("phantom {}: {e}", actor.entity),
                            AiEvent::Unknown(n) => eprintln!("phantom unknown {n}"),
                            _ => {}
                        }
                    }
                }
                if contacts >= 2 && (mark == 2 || (phantom_acts >= 3 && phantom_contacts >= 1)) {
                    break;
                }
            }
            eprintln!(
                "True Monk phase after mark {mark}: {contacts} contacts, {phantom_acts} phantom acts"
            );
            for actor in &duel.monk_phantoms.as_ref().unwrap().actors {
                eprintln!(
                    "phantom {}: {:?}, {:?}, pos {:?}",
                    actor.entity,
                    actor.duel.ai.as_ref().unwrap().state.event_requests,
                    actor.report.enemy.animation,
                    actor.duel.enemy.body.position
                );
            }
            assert!(contacts >= 2);
            if mark == 3 {
                assert!(phantom_acts >= 3);
                assert!(phantom_contacts >= 1);
            }
            duel.enemy_active = false;
            duel.force_act = None;
        } else {
            assert!(clips.contains("a100_013700"));
            assert!(clips.contains("a100_020200"));
        }
    }
    assert!(duel.events.as_ref().unwrap().scripts.flag(9309));
    assert!(duel.events.as_ref().unwrap().scripts.flag(12_505_956));
    assert_eq!(
        duel.events
            .as_ref()
            .unwrap()
            .scripts
            .cast_scene
            .enabled
            .get(&2_500_852),
        Some(&true)
    );
}

#[test]
fn true_monk_first_phase_lua_attacks_on_bridge() {
    let Some(mut duel) = load() else { return };
    assert_eq!(duel.spec.npc_param, 50_000_000);
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 3);
    let mut attacks = 0;
    let mut contacts = 0;
    let mut clips = std::collections::BTreeSet::new();
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        if let Some(c) = duel.enemy.behavior.runtime.main_clip() {
            clips.insert(c.animation.clone());
        }
        for event in duel.ai.as_mut().expect("True Monk Lua").drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(name) => eprintln!("True Monk unknown: {name}"),
                _ => {}
            }
        }
        if attacks >= 3 && contacts >= 2 {
            break;
        }
    }
    eprintln!("True Monk: {attacks} attacks, {contacts} contacts, clips {clips:?}");
    assert!(attacks >= 3 && contacts >= 2);
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(905000));
}

#[test]
#[ignore = "slow: four installed-data counter fixtures with the phantom cast"]
fn true_monk_perilous_counters_use_both_authored_sets() {
    for marks in [3, 1] {
        for (act, dodge, jumps) in [(3015, Some(45), &[][..]), (3005, None, &[35, 48][..])] {
            let Some(mut duel) = load() else { return };
            duel.enemy_active = false;
            // Isolate the authored move sets from phase-event arbitration here.
            // The fight-through above proves the native heat-up installs this same effect.
            for _ in 0..180 {
                duel.step(&InputFrame::default(), DT);
            }
            duel.events = None;
            if marks == 1 {
                duel.enemy.tae_runtime.add_timed(200031, f32::INFINITY);
            }
            duel.rules.fighters[1].vitality.deathblows_left = marks;
            for _ in 0..60 {
                duel.step(&InputFrame::default(), DT);
            }
            if marks == 1 {
                assert_eq!(
                    duel.enemy.behavior.runtime.offsets.by_type.get(&15),
                    Some(&100)
                );
            }
            duel.enemy_active = false;
            let p = duel.enemy.body.position;
            duel.player.body.position = [p[0], p[1], p[2] + 2.4];
            duel.player.body.yaw = 0.0;
            duel.enemy.body.yaw = std::f32::consts::PI;
            let mut warning = None;
            let mut mikiri = None;
            let mut kick = false;
            for i in 0..180 {
                duel.force_act = (i < 10).then_some(act);
                let report = duel.step(
                    &InputFrame {
                        move_stick: if dodge == Some(i) {
                            [0.0, 1.0]
                        } else {
                            [0.0; 2]
                        },
                        buttons: Buttons {
                            dodge: dodge == Some(i),
                            jump: jumps.contains(&i),
                            lock_on: i == 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    DT,
                );
                assert!(report.enemy.behavior.script_errors.is_empty());
                assert!(report.player.behavior.script_errors.is_empty());
                warning = warning.or(report.perilous);
                mikiri = mikiri.or(report.mikiri);
                kick |= report.jump_counter;
            }
            eprintln!(
                "True Monk marks {marks}, act {act}: {warning:?}, Mikiri {mikiri:?}, kick {kick}"
            );
            assert!(warning.is_some());
            if dodge.is_some() {
                assert!(mikiri.is_some());
            } else {
                assert!(kick);
            }
        }
    }
}
