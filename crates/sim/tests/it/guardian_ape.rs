//! Installed-data Guardian Ape fight checks, with scripted Wolf inputs.
use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    enemy::EnemySpec,
    lua_ai::AiEvent,
};

const DT: f32 = 1.0 / 60.0;

fn load() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5100/tae.json",
        "anim/c0000/tae.json",
        "raw/event/m17_00_00_00.emevd",
        "maps/m17_00_00_00/collision.bin",
        "anim/c5100/skeleton.bin",
        "raw/chr/c5100.chrbnd.d/c5100.flver",
        "raw/script/m17_00_00_00.luabnd.d/510000_battle.lua",
        "raw/script/m17_00_00_00.luabnd.d/510000_logic.lua",
        "raw/script/m17_00_00_00.luabnd.d/510010_battle.lua",
        "raw/param/gameparam/gameparam.parambnd.d/SpEffectParam.param",
        "raw/param/gameparam/gameparam.parambnd.d/NpcParam.param",
        "raw/param/gameparam/gameparam.parambnd.d/NpcThinkParam.param",
        "raw/param/gameparam/gameparam.parambnd.d/ThrowParam.param",
    ] {
        if !cache.join(path).is_file() {
            eprintln!("skipping Guardian Ape: missing {path}");
            return None;
        }
    }
    let mut duel = Duel::load_with(
        &cache,
        4.5,
        EnemySpec::from_cache(&cache, "c5100:51000000").unwrap(),
    )
    .unwrap();
    let arena = Arena::load(&cache, ArenaSpec::by_name("watering-hole").unwrap(), false).unwrap();
    arena.place(&mut duel);
    let p = duel.enemy.body.position;
    duel.player.body.position = [p[0], p[1], p[2] + 4.5];
    duel.player.body.yaw = 0.0;
    duel.enemy.body.yaw = std::f32::consts::PI;
    assert!(
        duel.ai.is_some(),
        "Guardian Ape must use its extracted Lua AI"
    );
    duel.god = true;
    Some(duel)
}

#[test]
fn guardian_ape_lua_attacks_wolf() {
    let Some(mut duel) = load() else { return };
    let mut hit = false;
    for _ in 0..1800 {
        let r = duel.step(&InputFrame::default(), DT);
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        hit |= r.hits.iter().any(|(h, _)| h.attacker == 1);
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(name) => eprintln!("unimplemented ape AI query: {name}"),
                _ => {}
            }
        }
        if hit {
            break;
        }
    }
    assert!(hit, "Guardian Ape never hits Wolf with its real Lua AI");
}

#[test]
fn guardian_ape_first_deathblow_reaches_headless_revival() {
    let Some(mut duel) = load() else { return };
    duel.enemy_active = false;
    // The ape's start is in the pond, deep water (hit material 22) where Wolf swims: the
    // water surface helper both used to stand on holds only enemies (hit filter 16,
    // docs/HIT-FILTERS.md). Take the fight to the dry bank about 12 m away.
    let bank = glam::Vec3::from(duel.enemy.body.position) + glam::Vec3::new(7.0, 0.0, -10.0);
    for (c, dz) in [(&mut duel.enemy, 0.0), (&mut duel.player, 4.5)] {
        let (x, z) = (bank.x, bank.z + dz);
        let y = c.ground.floor(x, z, bank.y + 3.0).expect("dry bank");
        c.body.position = [x, y, z];
        c.body.fall_top = y;
        c.body.velocity = [0.0; 3];
        c.body.grounded = true;
    }
    // Open the first mark, then press attack through the normal player input path.
    duel.rules.fighters[1].vitality.hp = 1;
    let mut started = false;
    let mut revived = false;
    for i in 0..2400 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 30 == 0,
                    lock_on: i == 2,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        if let Some(rows) = r.deathblow.started {
            eprintln!("first deathblow {rows:?}");
            started = true;
        }
        if duel.spec.think == 51000100 && duel.rules.fighters[1].vitality.hp > 1 {
            assert!(started);
            assert!(!duel.rules.fighters[1].dead());
            assert!(duel.rules.fighters[1].vitality.hp > 1);
            assert_eq!(duel.rules.fighters[1].vitality.max_hp, 3872);
            assert_eq!(duel.rules.fighters[1].vitality.hp, 3872);
            assert_eq!(duel.rules.fighters[1].posture.max, 1254);
            let mask = duel
                .enemy
                .tae_runtime
                .draw_mask
                .expect("revival has no TAE draw mask");
            assert_eq!(
                (mask[10], mask[11]),
                (0, 0),
                "revival retained the living head group"
            );
            assert_eq!(
                (mask[0], mask[1]),
                (1, 1),
                "revival lost its authored neck/head variants"
            );
            assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 0);
            assert!(i > 1000, "revival skipped the authored wait");
            revived = true;
            break;
        }
    }
    assert!(
        revived,
        "no headless revival: started={started}, vitality={:?}, clip={:?}",
        duel.rules.fighters[1].vitality,
        duel.enemy.behavior.runtime.main_clip()
    );
    // Let the authored get-up finish, then verify the second-phase AI really attacks.
    duel.enemy_active = true;
    let mut headless_hit = false;
    for _ in 0..1800 {
        let r = duel.step(&InputFrame::default(), DT);
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        for event in duel.ai.as_mut().unwrap().drain_log() {
            if let AiEvent::Error(e) = event {
                panic!("{e}");
            }
        }
        headless_hit |= r.hits.iter().any(|(h, _)| h.attacker == 1);
        if headless_hit {
            break;
        }
    }
    assert!(headless_hit, "no headless AI hit after revival");
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(905100));
    assert!(!duel.events.as_ref().unwrap().defeated);
    duel.enemy_active = false;
    // Recover from the hit before offering a new player deathblow.
    for _ in 0..360 {
        duel.step(&InputFrame::default(), DT);
    }
    let p = duel.enemy.body.position;
    duel.player.body.position = [p[0], p[1], p[2] + 4.0];
    duel.player.body.yaw = 0.0;
    duel.enemy.body.yaw = std::f32::consts::PI;
    duel.rules.fighters[1].posture.remaining = 0;
    let mut rows = None;
    let mut headless_pair = false;
    let mut struck = false;
    let mut last_player = None;
    let mut continuation = false;
    let mut continuation_row = false;
    let mut final_message = false;
    let mut ordinary_pairs = 0;
    for i in 0..1200 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 30 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        rows = rows.or(r.deathblow.started);
        if let Some(pair) = r.deathblow.started {
            eprintln!("headless pair at {i}: {pair:?}");
            continuation_row |= pair == (15100509, 15100509);
            ordinary_pairs += usize::from(pair == (15100500, 15100501));
        }
        final_message |= duel.enemy.tae_runtime.event_message == Some(30);
        if i < 180 && r.player.animation != last_player {
            eprintln!(
                "headless tick {i}: Wolf {:?} {:?}, ape {:?}",
                r.player.animation, r.player.behavior.state_path, r.enemy.animation
            );
            last_player = r.player.animation.clone();
        }
        struck |= r.deathblow.hit;
        headless_pair |= r.enemy.animation.as_deref() == Some("a100_012000");
        continuation |= r.enemy.animation.as_deref() == Some("a100_012090");
    }
    assert_eq!(rows, Some((15100500, 15100501)));
    assert!(
        headless_pair,
        "headless deathblow selected the living ape's clip"
    );
    assert!(
        duel.events.as_ref().unwrap().scripts.flag(9304),
        "struck={struck}, vitality={:?}, clip={:?}, marks={:?}",
        duel.rules.fighters[1].vitality,
        duel.enemy.behavior.runtime.main_clip(),
        duel.deathblow
    );
    assert!(
        continuation,
        "the final headless pair never continues to a100_012090"
    );
    assert!(continuation_row && final_message);
    assert_eq!(
        ordinary_pairs, 1,
        "final execution restarted an ordinary headless pair"
    );
}

#[test]
fn guardian_ape_sword_guard_deflect_and_dung_shot() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5100/tae.json").is_file() {
        return;
    }
    for (row, act, distance, mode) in [
        (51000100, 3000, 3.5, 0),
        (51000100, 3000, 3.5, 1),
        (51000000, 3014, 10.0, 3),
    ] {
        let mut d = Duel::load_with(
            &cache,
            distance,
            EnemySpec::from_cache(&cache, &format!("c5100:{row}")).unwrap(),
        )
        .unwrap();
        d.events = None;
        d.headless_ape = None;
        d.enemy_active = false;
        d.god = true;
        let mut observed = false;
        for i in 0..400 {
            d.force_act = (i < 10).then_some(act);
            let r = d.step(
                &InputFrame {
                    buttons: Buttons {
                        guard: mode == 0 || (mode == 1 && i >= 65),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            observed |= match mode {
                0 => r.hits.iter().any(|(h, r)| h.attacker == 1 && r.guarded),
                1 => r.hits.iter().any(|(h, r)| h.attacker == 1 && r.deflected),
                _ => d
                    .bullets
                    .live
                    .iter()
                    .any(|b| b.def.id == 51000290 && b.def.sp_effects.contains(&3510121)),
            };
            if observed {
                break;
            }
        }
        assert!(
            observed,
            "ape action {act} mode {mode} never produced its contact"
        );
    }
}

#[test]
fn guardian_ape_scream_builds_terror_and_kills_only_in_range() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5100/tae.json").is_file() {
        return;
    }
    for distance in [4.0, 20.0] {
        let mut d = Duel::load_with(
            &cache,
            distance,
            EnemySpec::from_cache(&cache, "c5100:51000100").unwrap(),
        )
        .unwrap();
        d.events = None;
        d.headless_ape = None;
        d.enemy_active = false;
        let mut buildup = false;
        let mut shot = false;
        for i in 0..360 {
            d.force_act = (i < 10).then_some(3007);
            let r = d.step(&InputFrame::default(), DT);
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            shot |= !d.bullets.live.is_empty();
            buildup |= d.rules.fighters[0].status.terror.value > 0.0;
            if d.rules.fighters[0].dead() {
                break;
            }
        }
        assert!(shot, "scream never produces its TAE bullets");
        assert_eq!(buildup, distance < 10.0);
        assert_eq!(
            d.rules.fighters[0].dead(),
            distance < 10.0,
            "scream at {distance}: {:?}",
            d.rules.fighters[0].status
        );
    }
}

#[test]
fn guardian_ape_headless_sweep_can_be_jump_countered() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5100/tae.json").is_file() {
        return;
    }
    let mut d = Duel::load_with(
        &cache,
        2.8,
        EnemySpec::from_cache(&cache, "c5100:51000100").unwrap(),
    )
    .unwrap();
    d.events = None;
    d.headless_ape = None;
    d.enemy_active = false;
    d.god = true;
    let mut warning = false;
    let mut counter = false;
    let mut swept = false;
    for i in 0..240 {
        d.force_act = (i < 10).then_some(3009);
        let r = d.step(
            &InputFrame {
                buttons: Buttons {
                    lock_on: i == 1,
                    jump: i == 45 || i == 58,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        warning |= r
            .perilous
            .is_some_and(|w| w.kind == Some(sekiro_sim::hits::Perilous::Sweep));
        counter |= r.jump_counter;
        swept |= r
            .hits
            .iter()
            .any(|(h, _)| h.attacker == 1 && h.judge == 560);
    }
    assert!(
        warning && counter,
        "headless sweep never exposes its authored jump counter"
    );
    assert!(!swept, "the sweep hit Wolf despite his jump counter");
}
