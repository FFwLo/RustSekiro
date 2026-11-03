use sekiro_sim::{
    Buttons, InputFrame,
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
        "maps/m15_00_00_00/collision.bin",
        "raw/event/m15_00_00_00.emevd",
    ] {
        if !cache.join(path).exists() {
            eprintln!("skipping Monk: missing {path}");
            return None;
        }
    }
    let spec = ArenaSpec::by_name("mibu-bridge").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    let brain = sekiro_sim::brain::BrainKind::Scripted.build(&duel.spec.chr);
    duel.set_brain(brain, false).unwrap();
    assert!(
        duel.boss_brain.is_none(),
        "unlisted scripted plans retain the real Lua AI"
    );
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

#[test]
fn corrupted_monk_ghost_lua_reaches_wolf_in_mibu() {
    let Some(mut duel) = load() else { return };
    assert_eq!(duel.spec.npc_param, 50_001_000);
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
    let mut attacks = 0;
    let mut contacts = 0;
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        for event in duel.ai.as_mut().expect("Monk must run Lua").drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(name) => eprintln!("Monk unknown: {name}"),
                _ => {}
            }
        }
        if attacks >= 5 && contacts >= 2 {
            break;
        }
    }
    eprintln!(
        "Mibu Monk: {attacks} attacks, {contacts} contacts; bar {:?}",
        duel.events.as_ref().unwrap().boss_bar
    );
    assert!(attacks >= 5 && contacts >= 2);
}

#[test]
fn corrupted_monk_ghost_single_mark_reaches_map_defeat() {
    let Some(mut duel) = load() else { return };
    duel.enemy_active = false;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), DT);
    }
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
                    lock_on: (2..4).contains(&i),
                    ..Buttons::default()
                },
                ..InputFrame::default()
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
        if duel.events.as_ref().unwrap().defeated {
            break;
        }
    }
    eprintln!(
        "Mibu finish: pair {pair:?}; clips {clips:?}; marks {}; hp {}",
        duel.rules.fighters[1].vitality.deathblows_left, duel.rules.fighters[1].vitality.hp
    );
    assert!(broke);
    // Map progression is covered here; the renderer replay exposed a missing defender
    // clip for the currently selected pair, so this does not certify execution fidelity.
    assert!(pair.is_some());
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 0);
    assert!(duel.events.as_ref().unwrap().defeated);
    assert!(duel.events.as_ref().unwrap().scripts.flag(9306));
    assert_eq!(
        duel.events.as_ref().unwrap().standing_fog_walls().count(),
        0
    );
}

#[test]
fn corrupted_monk_ghost_naginata_can_be_blocked_and_deflected() {
    for (guard_at, expected_deflect) in [(0, false), (65, true)] {
        let Some(mut duel) = load() else { return };
        duel.god = false;
        duel.immortal = true;
        duel.enemy_active = false;
        for _ in 0..180 {
            duel.step(&InputFrame::default(), DT);
        }
        let p = duel.enemy.body.position;
        duel.player.body.position = [p[0], p[1], p[2] + 2.4];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut contact = None;
        for i in 0..160 {
            duel.force_act = (i < 10).then_some(3003);
            let report = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        guard: i >= guard_at,
                        lock_on: i == 1,
                        ..Buttons::default()
                    },
                    ..InputFrame::default()
                },
                DT,
            );
            assert!(report.enemy.behavior.script_errors.is_empty());
            contact = contact.or_else(|| {
                report
                    .hits
                    .iter()
                    .find(|(h, _)| h.attacker == 1)
                    .map(|(_, r)| *r)
            });
        }
        eprintln!("Mibu guard at {guard_at}: {contact:?}");
        let contact = contact.expect("naginata must reach guarding Wolf");
        assert!(contact.guarded);
        assert!(contact.posture_damage > 0);
        assert_eq!(contact.deflected, expected_deflect);
    }
}

#[test]
fn corrupted_monk_ghost_perilous_counters_use_wolfs_script() {
    for (act, step, jumps) in [(3015, Some(45), &[][..]), (3005, None, &[35, 48][..])] {
        let Some(mut duel) = load() else { return };
        duel.enemy_active = false;
        for _ in 0..180 {
            duel.step(&InputFrame::default(), DT);
        }
        let p = duel.enemy.body.position;
        duel.player.body.position = [p[0], p[1], p[2] + 2.4];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut warning = None;
        let mut mikiri = None;
        let mut kick = None;
        for i in 0..180 {
            duel.force_act = (i < 10).then_some(act);
            let report = duel.step(
                &InputFrame {
                    move_stick: if step == Some(i) {
                        [0.0, 1.0]
                    } else {
                        [0.0; 2]
                    },
                    buttons: Buttons {
                        dodge: step == Some(i),
                        jump: jumps.contains(&i),
                        lock_on: i == 1,
                        ..Buttons::default()
                    },
                    ..InputFrame::default()
                },
                DT,
            );
            assert!(report.enemy.behavior.script_errors.is_empty());
            assert!(report.player.behavior.script_errors.is_empty());
            warning = warning.or(report.perilous);
            mikiri = mikiri.or(report.mikiri);
            if report.jump_counter {
                kick = report
                    .hits
                    .iter()
                    .find(|(h, _)| h.attacker == 0 && h.judge == 901)
                    .map(|(_, r)| r.posture_damage);
            }
        }
        eprintln!("Mibu act {act}: warning {warning:?}, Mikiri {mikiri:?}, jump {kick:?}");
        assert!(warning.is_some());
        if step.is_some() {
            assert!(mikiri.is_some());
        } else {
            assert_eq!(kick, Some(80));
        }
    }
}
