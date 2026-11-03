use sekiro_sim::{Buttons, InputFrame, duel::Duel, lua_ai::AiEvent, registry::Registry};

#[test]
fn castle_genichiro_attacks_idle_wolf() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/skeleton.bin").exists() {
        return;
    }
    let registry = Registry::load(&cache).unwrap();
    let spec = registry.resolve("c7100:71001000").unwrap();
    assert_eq!(
        registry.display_name(&spec.chr, spec.npc_param),
        Some("Genichiro Ashina")
    );
    let mut duel = Duel::load_with(&cache, 6.0, spec).unwrap();
    assert_eq!(duel.rules.fighters[1].vitality.max_hp, 1598);
    assert_eq!(duel.rules.fighters[1].posture.max, 660);
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
    assert!(duel.ai.is_some());
    duel.god = true;
    let mut attacks = 0;
    let mut hits = 0;
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        hits += report
            .hits
            .iter()
            .filter(|(hit, _)| hit.attacker == 1)
            .count();
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(name) => eprintln!("unknown: {name}"),
                _ => {}
            }
        }
        if attacks >= 5 {
            break;
        }
    }
    eprintln!("Genichiro: {attacks} attacks, {hits} melee contacts");
    assert!(attacks >= 5);
}

#[test]
fn castle_genichiro_loses_two_marks_and_reaches_map_handover() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/skeleton.bin").exists() {
        return;
    }
    let spec = Registry::load(&cache)
        .unwrap()
        .resolve("c7100:71001000")
        .unwrap();
    let mut duel = Duel::load_with(&cache, 1.6, spec).unwrap();
    duel.god = true;
    for _ in 0..150 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(907100));
    for marks in [1, 0] {
        take_mark(&mut duel);
        assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, marks);
        for _ in 0..600 {
            duel.step(&InputFrame::default(), 1.0 / 60.0);
            if marks == 0 && duel.events.as_ref().unwrap().scripts.flag(11115802) {
                break;
            }
        }
        if marks == 1 {
            assert_eq!(duel.rules.fighters[1].vitality.hp, 1598);
            assert!(!duel.events.as_ref().unwrap().scripts.flag(11115802));
            duel.enemy_active = true;
            let mut attacks = 0;
            for _ in 0..3600 {
                let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
                assert!(report.enemy.behavior.script_errors.is_empty());
                for event in duel.ai.as_mut().unwrap().drain_log() {
                    match event {
                        AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                        AiEvent::Error(e) => panic!("{e}"),
                        _ => {}
                    }
                }
                if attacks >= 5 {
                    break;
                }
            }
            assert_eq!(duel.ai.as_ref().unwrap().state.ninsatsu, (1, 2));
            assert!(attacks >= 5, "no attacks after the first mark: {attacks}");
            eprintln!("Genichiro after first mark: {attacks} attacks");
        }
    }
    assert!(duel.events.as_ref().unwrap().scripts.flag(11115802));
    assert_eq!(duel.spec.chr, "c7110");
    assert_eq!(duel.spec.npc_param, 71100000);
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(907110));
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
    assert!(duel.enemy_active);
    assert!(!duel.events.as_ref().unwrap().defeated);
    let mut attacks = 0;
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(report.enemy.behavior.script_errors.is_empty());
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        if attacks >= 5 {
            break;
        }
    }
    assert!(attacks >= 5, "Tomoe did not attack: {attacks}");
    eprintln!("Castle Tomoe: {attacks} attacks");
    take_mark(&mut duel);
    for _ in 0..600 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        if duel.events.as_ref().unwrap().defeated {
            break;
        }
    }
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 0);
    assert!(duel.events.as_ref().unwrap().defeated);
    duel.reset().unwrap();
    assert_eq!(duel.spec.chr, "c7100");
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
}

fn take_mark(duel: &mut Duel) {
    duel.enemy_active = false;
    for _ in 0..360 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let p = duel.player.body.position;
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let mut broke = false;
    let mut started = false;
    for i in 0..1500 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        let frame = InputFrame {
            buttons: Buttons {
                attack: if broke {
                    !started && i % 10 == 0
                } else {
                    i % 40 == 0
                },
                lock_on: (2..4).contains(&i),
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let report = duel.step(&frame, 1.0 / 60.0);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        broke |= report
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= report.deathblow.started.is_some();
        if started && duel.deathblow.is_none() {
            break;
        }
    }
    assert!(started, "deathblow not started");
}

#[test]
fn castle_genichiro_arrows_use_the_shared_bullet_runtime() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/skeleton.bin").exists() {
        return;
    }
    let spec = Registry::load(&cache)
        .unwrap()
        .resolve("c7100:71001000")
        .unwrap();
    let mut duel = Duel::load_with(&cache, 8.0, spec).unwrap();
    duel.enemy_active = false;
    duel.force_act = Some(3008);
    duel.god = true;
    let mut missiles = std::collections::BTreeSet::new();
    let mut contacts = 0;
    for _ in 0..1200 {
        let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(report.enemy.behavior.script_errors.is_empty());
        for bullet in &duel.bullets.live {
            if bullet.look() == sekiro_sim::bullets::BulletLook::Missile {
                missiles.insert(bullet.def.id);
            }
        }
        contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
    }
    eprintln!("Genichiro arrows: {missiles:?}, {contacts} contacts");
    assert!(!missiles.is_empty());
    assert!(contacts > 0);
}
