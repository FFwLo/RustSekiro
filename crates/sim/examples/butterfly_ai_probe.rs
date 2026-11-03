//! Read-only inspection of the installed boss AI's constant tables.
use sekiro_sim::lua_ai::lua50::{Proto, load_chunk};

fn inspect(proto: &Proto, path: &str) {
    println!("{path}: {:?}", proto.consts);
    for (index, child) in proto.protos.iter().enumerate() {
        inspect(child, &format!("{path}/{index}"));
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if let Some(file) = args
        .iter()
        .position(|a| a == "--code")
        .and_then(|i| args.get(i + 1))
    {
        fn code(p: &Proto, path: &str) {
            println!("{path} constants {:?}", p.consts);
            for (pc, &i) in p.code.iter().enumerate() {
                println!(
                    "{pc}: op {} a {} b {} c {} bx {}",
                    i & 63,
                    i >> 24,
                    (i >> 15) & 511,
                    (i >> 6) & 511,
                    (i >> 6) & 262143
                );
            }
            for (index, child) in p.protos.iter().enumerate() {
                code(child, &format!("{path}/{index}"));
            }
        }
        code(&load_chunk(&std::fs::read(file).unwrap()).unwrap(), "root");
        return;
    }
    if args.iter().any(|a| a == "--replay" || a == "--descent") {
        use sekiro_sim::{
            InputFrame,
            arena::{Arena, ArenaSpec},
            duel::Duel,
            enemy::EnemySpec,
        };
        let cache = &sekiro_sim::cache::root();
        let arena = Arena::load(cache, ArenaSpec::by_name("hirata-hall").unwrap(), false).unwrap();
        let mut duel = Duel::load_with(
            cache,
            1.6,
            EnemySpec::from_cache(cache, "c5090:50900001").unwrap(),
        )
        .unwrap();
        arena.place(&mut duel);
        println!(
            "placement {:?}, boss entity {}",
            duel.spec,
            duel.events.as_ref().unwrap().boss_entity
        );
        duel.god = true;
        let descent = args.iter().any(|a| a == "--descent");
        if descent {
            // new() retains the original instruction zero; load() applies the staged join.
            let file = sekiro_formats::emevd::Emevd::parse(
                &std::fs::read(cache.join("raw/event/m10_00_00_00.emevd")).unwrap(),
            )
            .unwrap();
            duel.events.as_mut().unwrap().scripts = sekiro_sim::events::EventScripts::new(
                &file,
                &[sekiro_sim::events::EventEntry {
                    id: 11005812,
                    from_label: None,
                }],
            )
            .unwrap();
            duel.player.body.position = sekiro_sim::events::warp_point(cache, 10, 0, 1002830)
                .unwrap()
                .position;
            let p = &mut duel.player.body.position;
            p[1] = duel.player.ground.floor(p[0], p[2], p[1] + 10.0).unwrap();
            duel.player.body.velocity = [0.0; 3];
            duel.player.body.grounded = true;
            duel.player.body.fall_top = p[1];
        } else {
            duel.enemy.tae_runtime.add_timed(200060, f32::INFINITY);
        }
        for tick in 0..900 {
            if tick == 300 && !descent {
                duel.force_act = Some(3020);
            }
            let report = duel.step(&InputFrame::default(), 1.0 / 60.0);
            if tick % 60 == 0 {
                println!(
                    "tick {tick} boss {:?} effects {:?} flags {:?}",
                    report.enemy.animation,
                    report.enemy.sp_effects,
                    duel.events.as_ref().unwrap().scripts.flags
                );
                for (label, brain) in std::iter::once(("boss", duel.ai.as_ref().unwrap())).chain(
                    duel.butterfly_illusions
                        .as_ref()
                        .unwrap()
                        .actors
                        .iter()
                        .take(1)
                        .map(|a| ("illusion", a.duel.ai.as_ref().unwrap())),
                ) {
                    for g in brain.state.goals.iter().flatten() {
                        println!(
                            "{label} goal {} {:?} started {} elapsed {}",
                            g.kind, g.params, g.started_anim, g.elapsed
                        );
                    }
                }
            }
        }
        println!(
            "event log {:?}; requests {:?}",
            duel.events.as_ref().unwrap().scripts.log,
            duel.ai.as_ref().unwrap().state.event_requests
        );
        return;
    }
    if std::env::args().any(|a| a == "--regions") {
        for id in [
            1002830, 1002860, 1002890, 9992800, 9992810, 9992930, 9992960,
        ] {
            println!(
                "region {id}: {:?}",
                sekiro_sim::events::warp_point(&sekiro_sim::cache::root(), 10, 0, id)
            );
        }
        return;
    }
    if std::env::args().any(|a| a == "--goals") {
        use sekiro_sim::lua_ai::{lua50::Vm, runtime::AiState};
        let mut vm = Vm::new();
        let mut state = AiState::new(Default::default(), Default::default(), 1);
        for name in ["ai_define", "goal_list"] {
            let data =
                std::fs::read(format!("cache/raw/script/aicommon.luabnd.d/{name}.lua")).unwrap();
            vm.exec_chunk(&mut state, load_chunk(&data).unwrap())
                .unwrap();
        }
        for name in [
            "GOAL_COMMON_ToTargetWarp",
            "GOAL_COMMON_ApproachSettingDirection",
        ] {
            let value = vm.get_global(name);
            println!(
                "{name}: {value:?}; host {:?}",
                value
                    .num()
                    .and_then(|n| sekiro_sim::lua_ai::runtime::NativeGoal::for_id(n as i32))
            );
        }
        return;
    }
    for id in [509000, 151010, 508000] {
        let map = if id == 508000 {
            "m11_00_00_00"
        } else {
            "m10_00_00_00"
        };
        let path = format!("cache/raw/script/{map}.luabnd.d/{id}_battle.lua");
        let data = std::fs::read(path).unwrap();
        inspect(&load_chunk(&data).unwrap(), &id.to_string());
    }
}
