use sekiro_formats::anim::Skeleton;
use sekiro_sim::{Character, InputFrame, registry::Registry};

#[test]
#[ignore = "slow: native arena startup and fixed-input catch with four Lua actors"]
fn folding_screen_monkeys_hidden_target_is_playable_from_arena_start() {
    use sekiro_sim::{
        Buttons,
        arena::{Arena, ArenaSpec},
        duel::Duel,
    };
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1250/skeleton.bin").exists()
        || !cache.join("maps/m20_00_00_00/collision.bin").exists()
    {
        eprintln!("skipping natural monkey catch: donor animations or arena collision absent");
        return;
    }
    let spec = ArenaSpec::by_name("senpou-screens").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 0.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    let mut contacts = 0;
    // The same fixed turn-and-slash replay used in the graphical check, expressed
    // relative to the authored entry yaw. No invisible-target tracking or actor edits.
    let camera_yaw = arena.layout.wolf.yaw + 175.0_f32.to_radians();
    for tick in 0..330 {
        let report = duel.step(
            &InputFrame {
                camera_yaw,
                camera_pitch: 0.0,
                move_stick: if tick < 270 { [0.0, 0.6] } else { [0.0, 0.0] },
                buttons: Buttons {
                    attack: (30..270).contains(&tick) && (tick - 30) % 8 == 0,
                    ..Default::default()
                },
            },
            1.0 / 60.0,
        );
        contacts += report
            .hits
            .iter()
            .filter(|(hit, _)| hit.attacker == 0)
            .count();
        if duel.screen_monkeys.as_ref().unwrap().monkeys[3]
            .duel
            .rules
            .fighters[1]
            .dead()
        {
            break;
        }
    }
    // Allow the screen slot's authored post-death wait to finish.
    for _ in 0..60 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let encounter = duel.screen_monkeys.as_ref().unwrap();
    assert!(
        encounter.monkeys[3].duel.rules.fighters[1].dead(),
        "hidden target survives: Wolf {:?}, target {:?}, contacts {contacts}",
        duel.player.body.position,
        encounter.monkeys[3].duel.enemy.body.position
    );
    assert!(contacts > 0);
    assert!(
        encounter.monkeys[..3]
            .iter()
            .all(|m| !m.duel.rules.fighters[1].dead())
    );
    assert_eq!(duel.rules.fighters[1].vitality.hp, 74);
    assert!(!duel.events.as_ref().unwrap().defeated);
    assert_eq!(
        duel.events
            .as_ref()
            .unwrap()
            .scripts
            .cast_scene
            .enabled
            .get(&2001833),
        Some(&false)
    );
    assert_eq!(
        duel.events
            .as_ref()
            .unwrap()
            .scripts
            .cast_scene
            .enabled
            .get(&2001843),
        Some(&true)
    );
    let start = duel.player.body.position;
    duel.player.body.position[1] -= 100.0;
    duel.player.body.grounded = false;
    duel.step(&InputFrame::default(), 1.0 / 60.0);
    assert!(
        glam::Vec3::from_array(duel.player.body.position).distance(glam::Vec3::from_array(start))
            < 1.0,
        "shared Wolf must return to his last safe arena floor after a void fall: {:?}",
        duel.player.body.position
    );
}

#[test]
fn folding_screen_monkeys_independent_lua_following_and_bell() {
    use sekiro_sim::{
        Buttons,
        arena::{Arena, ArenaSpec},
        duel::Duel,
    };
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1250/skeleton.bin").exists()
        || !cache.join("maps/m20_00_00_00/collision.bin").exists()
    {
        eprintln!("skipping monkey combat: donor animations or arena collision absent");
        return;
    }
    let spec = ArenaSpec::by_name("senpou-screens").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 0.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    let following_area = sekiro_sim::events::warp_point(&cache, 20, 0, 2_002_805).unwrap();
    duel.player.body.position = following_area.position;
    duel.player.body.position[1] += 0.1;
    let start = duel.player.body.position;
    duel.screen_monkeys.as_mut().unwrap().monkeys[3]
        .duel
        .enemy
        .body
        .position = [start[0] + 40.0, start[1], start[2]];
    let mut followed = false;
    for _ in 0..10 {
        let before = duel.events.as_ref().unwrap().scripts.log.len();
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        if duel.events.as_ref().unwrap().scripts.log[before..]
            .iter()
            .any(|event| {
                matches!(
                    event,
                    sekiro_sim::events::EventLog::Command {
                        command: sekiro_sim::events::EventCommand::WarpToDummy {
                            entity: 2_000_804,
                            dummy: 169,
                            ..
                        },
                        ..
                    }
                )
            })
        {
            let point = duel
                .dummy_position(0, 169)
                .expect("authored player following dummy is present");
            let monkey = duel.screen_monkeys.as_ref().unwrap().monkeys[3]
                .duel
                .enemy
                .body
                .position;
            let distance = glam::Vec3::from_array(point).distance(glam::Vec3::from_array(monkey));
            assert!(
                distance < 0.001,
                "map warp must use Wolf's actual dummy, distance {distance}"
            );
            followed = true;
        }
    }
    assert!(
        followed,
        "12005873 must bring a distant hidden monkey to Wolf's authored dummy; Wolf {:?}, event log {:?}",
        duel.player.body.position,
        duel.events.as_ref().unwrap().scripts.log
    );
    let bell_report = duel.step(
        &InputFrame {
            buttons: Buttons {
                action: true,
                ..Default::default()
            },
            ..Default::default()
        },
        1.0 / 60.0,
    );
    assert!(
        !duel.events.as_ref().unwrap().scripts.flag(12_005_826),
        "a distant press must not ring the bell"
    );
    duel.step(&InputFrame::default(), 1.0 / 60.0);
    let bell = &duel.screen_monkeys.as_ref().unwrap().bell;
    duel.player.body.position = [
        bell.point[0],
        bell.point[1] + bell.reach.base,
        bell.point[2],
    ];
    let green_spot = sekiro_sim::events::warp_point(&cache, 20, 0, 2_002_553).unwrap();
    duel.screen_monkeys.as_mut().unwrap().monkeys[1]
        .duel
        .enemy
        .body
        .position = green_spot.position;
    let nearby_bell_report = duel.step(
        &InputFrame {
            buttons: Buttons {
                action: true,
                ..Default::default()
            },
            ..Default::default()
        },
        1.0 / 60.0,
    );
    assert!(
        duel.events.as_ref().unwrap().scripts.flag(12_005_826),
        "nearby action must reach the actual bell event"
    );
    assert_eq!(
        nearby_bell_report.player.animation.as_deref(),
        Some("a000_710700"),
        "bell activation must play ObjActParam's real Wolf clip"
    );
    for _ in 0..240 {
        let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(
            report.player.behavior.script_errors.is_empty(),
            "{:?}",
            report.player.behavior.script_errors
        );
    }
    let encounter = duel.screen_monkeys.as_mut().unwrap();
    for monkey in &mut encounter.monkeys {
        assert!(monkey.report.enemy.behavior.script_errors.is_empty());
        let ai = monkey.duel.ai.as_mut().unwrap();
        let log = ai.drain_log();
        eprintln!(
            "{} HP {} AI time {} position {:?} unknown {:?} log {:?}",
            monkey.entity,
            monkey.duel.rules.fighters[1].vitality.hp,
            ai.state.time,
            monkey.duel.enemy.body.position,
            ai.state.unknown,
            log
        );
        assert!(ai.state.time > 0.0 && ai.state.time <= 4.3);
    }
    eprintln!(
        "Wolf {:?}",
        (duel.player.body.position, duel.player.state())
    );
    assert_ne!(
        bell_report.player.animation.as_deref(),
        Some("a000_710700"),
        "a distant action must not play the bell gesture"
    );
    let events = duel.events.as_ref().unwrap();
    assert_eq!(
        events.scripts.cast_scene.animations.get(&2_000_802),
        Some(&(20_011, false)),
        "the bell stuns the monkey in its authored region"
    );
    assert!(!events.defeated && !events.scripts.flag(9305));
}

#[test]
fn folding_screen_monkeys_four_combat_deaths_drive_map_victory() {
    use sekiro_sim::{
        Buttons,
        arena::{Arena, ArenaSpec},
        duel::Duel,
    };
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1250/skeleton.bin").exists()
        || !cache.join("maps/m20_00_00_00/collision.bin").exists()
    {
        eprintln!("skipping monkey victory: donor animations or arena collision absent");
        return;
    }
    let spec = ArenaSpec::by_name("senpou-screens").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 0.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    let spawn = duel.player.body.position;
    for index in 0..4 {
        // A controlled catch on an authored temple floor isolates combat from navigation.
        // No HP, posture, death flags or controller health are changed by the test.
        duel.player.body.position = spawn;
        duel.player.body.yaw = 0.0;
        {
            let monkey = &mut duel.screen_monkeys.as_mut().unwrap().monkeys[index];
            monkey.duel.enemy.body.position = [spawn[0], spawn[1], spawn[2] - 1.1];
            monkey.duel.enemy.body.yaw = std::f32::consts::PI;
        }
        let mut hits = 0;
        for tick in 0..720 {
            let report = duel.step(
                &InputFrame {
                    move_stick: [0.0, 0.5],
                    buttons: Buttons {
                        attack: tick % 8 == 0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            hits += report.hits.iter().filter(|(h, _)| h.attacker == 0).count();
            if duel.screen_monkeys.as_ref().unwrap().monkeys[index]
                .duel
                .rules
                .fighters[1]
                .dead()
            {
                break;
            }
        }
        let monkey = &duel.screen_monkeys.as_ref().unwrap().monkeys[index];
        assert!(
            monkey.duel.rules.fighters[1].dead(),
            "{} was not killed by scripted Wolf contacts, HP {}, hits {hits}",
            monkey.entity,
            monkey.duel.rules.fighters[1].vitality.hp
        );
        assert!(!duel.events.as_ref().unwrap().defeated);
        eprintln!(
            "{} killed by {hits} contacts, controller HP {}",
            monkey.entity, duel.rules.fighters[1].vitality.hp
        );
    }
    for _ in 0..240 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let events = duel.events.as_ref().unwrap();
    assert!(events.defeated && events.scripts.flag(9305));
    assert_eq!(duel.rules.fighters[1].vitality.hp, 0);
}

/// The screen variant ships a skeleton-only binder, so loading its own clip directory
/// would silently leave all four actors motionless even with their models extracted.
#[test]
fn folding_screen_monkeys_load_the_placed_animation_donor() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("maps/m20_00_00_00/layout.json").exists()
        || !cache.join("anim/c1250/skeleton.bin").exists()
    {
        eprintln!("skipping screen monkeys: Senpou layout or c1250 animations not extracted");
        return;
    }
    if cache.join("anim/c1260/skeleton.bin").exists() {
        let donor =
            Skeleton::from_bytes(&std::fs::read(cache.join("anim/c1250/skeleton.bin")).unwrap())
                .unwrap();
        let variant =
            Skeleton::from_bytes(&std::fs::read(cache.join("anim/c1260/skeleton.bin")).unwrap())
                .unwrap();
        let identity = |rig: &Skeleton| {
            rig.bones
                .iter()
                .map(|bone| (bone.name.clone(), bone.parent))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            identity(&donor),
            identity(&variant),
            "variant and donor bone order/hierarchy must agree"
        );
        eprintln!(
            "c1260/c1250: matching {} bone names and parents; bind poses equal: {}",
            donor.bones.len(),
            donor.bones == variant.bones
        );
    }
    let registry = Registry::load(&cache).unwrap();
    for entity in 2_000_801..=2_000_804 {
        let (spec, placement) = registry.entity("m20_00_00_00", entity).unwrap();
        let donor = registry.anim_chr(&spec.chr, spec.npc_param);
        let mut actor = Character::load_npc_with_anim(&cache, &spec.chr, &donor).unwrap();
        actor.body.position = placement.position;
        actor.body.yaw = placement.yaw;
        for _ in 0..60 {
            let step = actor.tick(&InputFrame::default(), 1.0 / 60.0);
            assert!(step.behavior.script_errors.is_empty());
        }
        let playing = actor
            .behavior
            .runtime
            .main_clip()
            .expect("HKS selects a clip");
        assert!(
            actor.clips.clip(&playing.animation).is_some(),
            "{entity}: {}",
            playing.animation
        );
        assert!(
            actor.tae.anim(&playing.animation).is_some(),
            "{entity}: missing TAE for {}",
            playing.animation
        );
        eprintln!(
            "{entity}: {} row {} think {} donor {donor}, {}",
            spec.chr, spec.npc_param, spec.think, playing.animation
        );
    }
}

#[test]
#[ignore = "slow: arena setup, authored apparition spawn and thirty seconds of Lua combat"]
fn folding_screen_monkeys_apparition_runs_its_own_lua_after_a_screen_death() {
    use sekiro_sim::{
        Buttons,
        arena::{Arena, ArenaSpec},
        duel::Duel,
    };
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1250/skeleton.bin").exists()
        || !cache.join("maps/m20_00_00_00/collision.bin").exists()
    {
        eprintln!("skipping apparition: donor animations or arena collision absent");
        return;
    }
    let spec = ArenaSpec::by_name("senpou-screens").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 0.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    let target = duel.screen_monkeys.as_ref().unwrap().monkeys[3]
        .duel
        .enemy
        .body
        .position;
    duel.player.body.position = [target[0], target[1], target[2] + 1.1];
    duel.player.body.yaw = 0.0;
    for tick in 0..120 {
        duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: tick % 8 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            1.0 / 60.0,
        );
    }
    assert!(
        duel.screen_monkeys.as_ref().unwrap().monkeys[3]
            .duel
            .rules
            .fighters[1]
            .dead()
    );
    let trigger = sekiro_sim::events::warp_point(&cache, 20, 0, 2_002_851).unwrap();
    duel.player.body.position = trigger.position;
    duel.player.body.position[1] += 0.5;
    for _ in 0..10 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    assert!(
        duel.screen_monkeys
            .as_ref()
            .unwrap()
            .actor(2_000_805)
            .is_some(),
        "12005890 must enable its authored apparition, Wolf {:?}, controller HP {}, event log {:?}",
        duel.player.body.position,
        duel.rules.fighters[1].vitality.hp,
        duel.events.as_ref().unwrap().scripts.log
    );
    let floor = arena.layout.wolf.position.to_array();
    {
        let actor = duel
            .screen_monkeys
            .as_mut()
            .unwrap()
            .monkeys
            .iter_mut()
            .find(|m| m.entity == 2_000_805)
            .unwrap();
        actor.duel.enemy.body.position = [floor[0], floor[1], floor[2] - 1.1];
        actor.duel.enemy.body.yaw = std::f32::consts::PI;
    }
    duel.player.body.position = floor;
    let mut actions = 0;
    let mut hits = 0;
    for _ in 0..1800 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        let actor = duel
            .screen_monkeys
            .as_mut()
            .unwrap()
            .monkeys
            .iter_mut()
            .find(|m| m.entity == 2_000_805)
            .unwrap();
        hits += actor
            .report
            .hits
            .iter()
            .filter(|(h, _)| h.attacker == 1)
            .count();
        let log = actor.duel.ai.as_mut().unwrap().drain_log();
        if log
            .iter()
            .any(|event| matches!(event, sekiro_sim::lua_ai::AiEvent::Action(3000..=3999)))
        {
            eprintln!(
                "apparition action {:?}, clip {:?}, position {:?}, Wolf {:?}, variation {}, attacks {:?}",
                log,
                actor.report.enemy.animation,
                actor.duel.enemy.body.position,
                duel.player.body.position,
                actor.duel.fighters[1].variation,
                actor.duel.fighters[1].attacks.keys()
            );
        }
        actions += log
            .iter()
            .filter(|event| matches!(event, sekiro_sim::lua_ai::AiEvent::Action(3000..=3999)))
            .count();
    }
    eprintln!("authored apparition: {actions} Lua attack requests and {hits} Wolf contacts");
    assert!(actions > 0 && hits > 0);
}
