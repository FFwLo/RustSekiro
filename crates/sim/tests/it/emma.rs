use std::collections::BTreeSet;

use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::duel::Duel;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn fight() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c7400/skeleton.bin",
        "anim/c0000/skeleton.bin",
        "maps/m11_01_00_00/collision.bin",
        "raw/script/m11_01_00_00.luabnd.d/740000_battle.lua",
    ] {
        if !cache.join(path).is_file() {
            eprintln!("skipping Emma: missing {}", cache.join(path).display());
            return None;
        }
    }
    let arena = Arena::load(&cache, ArenaSpec::by_name("shura").unwrap(), false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, arena.spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

#[test]
fn emma_lua_attacks_on_the_shura_roof() {
    let Some(mut duel) = fight() else { return };
    assert!(duel.ai.is_some());
    let mut attacks = 0;
    let mut contacts = 0;
    let mut unknown = BTreeSet::new();
    for _ in 0..3600 {
        let r = duel.step(&InputFrame::default(), DT);
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        contacts += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => {
                    attacks += 1;
                }
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(n) => {
                    unknown.insert(n);
                }
                _ => {}
            }
        }
        if attacks >= 5 && contacts >= 2 {
            break;
        }
    }
    eprintln!(
        "Emma: {attacks} attacks, {contacts} contacts, unknown {unknown:?}; HP {}, posture {}, marks {}",
        duel.rules.fighters[1].vitality.max_hp,
        duel.rules.fighters[1].posture.max,
        duel.rules.fighters[1].vitality.deathblows_left
    );
    assert!(attacks >= 5 && contacts >= 2);
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(907400));
    assert!(
        unknown.is_empty(),
        "unanswered Emma AI queries: {unknown:?}"
    );
}

#[test]
fn emma_ai_region_query_tracks_wolf_crossing_an_msb_boundary() {
    use sekiro_sim::lua_ai::lua50::{Host, Value, Vm};
    use sekiro_sim::lua_ai::runtime::ai_obj;
    let Some(mut duel) = fight() else { return };
    let cache = sekiro_sim::cache::root();
    // Find a real volume containing the staged roof start, rather than inventing a boundary.
    let point = duel.player.body.position;
    let region = (1112000..1113000)
        .find(|&id| {
            sekiro_sim::arena_c::regions::contains(&cache, "m11_01_00_00", id, point) == Some(true)
        })
        .expect("the Shura start is inside an authored MSB volume");
    let ai = &mut duel.ai.as_mut().unwrap().state;
    ai.world.target = Some(sekiro_sim::lua_ai::AiActor {
        position: point,
        ..Default::default()
    });
    let Value::Obj(object) = ai_obj() else {
        unreachable!()
    };
    let mut vm = Vm::new();
    let args = [ai_obj(), Value::Num(0.0), Value::Num(f64::from(region))];
    let query = ai
        .call_method(&mut vm, object, "IsInsideTargetRegion", &args)
        .unwrap();
    assert!(matches!(query.as_slice(), [Value::Bool(true)]));
    ai.world.target.as_mut().unwrap().position[0] += 10000.0;
    let query = ai
        .call_method(&mut vm, object, "IsInsideTargetRegion", &args)
        .unwrap();
    assert!(matches!(query.as_slice(), [Value::Bool(false)]));
    eprintln!("Emma Lua region {region}: inside true, outside false");
}

#[test]
fn emma_posture_deathblow_reaches_isshin_through_the_map_script() {
    let Some(mut duel) = fight() else { return };
    duel.enemy_active = false;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), DT);
    }
    let p = duel.player.body.position;
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let mut broke = false;
    let mut started = false;
    for i in 0..1800 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: if broke {
                        !started && i % 10 == 0
                    } else {
                        i % 40 == 0
                    },
                    lock_on: (2..4).contains(&i),
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        // She guards Wolf's slash, so her break is a guard break (1001): the hit report must
        // count it as a posture break like a clean hit's (#192).
        broke |= r
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= r.deathblow.started.is_some();
        if duel.spec.chr == "c5430" {
            break;
        }
    }
    assert!(started, "Emma's real deathblow never started");
    assert_eq!(
        duel.spec.chr,
        "c5430",
        "state {}, events {:?}",
        duel.enemy.state(),
        duel.events.as_ref().unwrap().scripts.log
    );
    assert!(duel.events.as_ref().unwrap().scripts.flag(9315));
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
}

#[test]
fn emma_perilous_attacks_use_their_authored_contacts_and_throw() {
    let Some(mut duel) = fight() else { return };
    for (act, kind) in [
        (3005, sekiro_sim::hits::Perilous::Thrust),
        (3006, sekiro_sim::hits::Perilous::Sweep),
        (3013, sekiro_sim::hits::Perilous::Thrust),
        (3025, sekiro_sim::hits::Perilous::Grab),
    ] {
        duel.reset().unwrap();
        duel.enemy_active = false;
        duel.events = None;
        duel.player.body.position = [110.0, 54.15, 238.0];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.position = [110.0, 54.15, 236.0];
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut warning = None;
        let mut contacts = 0;
        let mut grab = None;
        let mut throw_damage = 0;
        for i in 0..420 {
            duel.force_act = (i < 10).then_some(act);
            let r = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        guard: act == 3025,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            if let Some(w) = r.perilous {
                warning = w.kind;
            }
            contacts += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
            grab = grab.or(r.grab.started);
            throw_damage += r.grab.hits.iter().map(|(_, hp, _)| hp).sum::<i32>();
        }
        eprintln!(
            "Emma {act}: warning {warning:?}, contacts {contacts}, grab {grab:?}, damage {throw_damage}"
        );
        assert_eq!(warning, Some(kind));
        if act == 3025 {
            assert!(
                grab.is_some() && throw_damage > 0,
                "grab did not play and damage Wolf"
            );
        } else {
            assert!(contacts > 0, "attack did not contact Wolf");
        }
    }
}

#[test]
fn emma_thrusts_accept_mikiri_and_the_sweep_accepts_a_jump() {
    let Some(mut duel) = fight() else { return };
    for (act, counter_tick, sweep) in [(3005, 82, false), (3013, 39, false), (3006, 40, true)] {
        duel.reset().unwrap();
        duel.enemy_active = false;
        duel.events = None;
        duel.player.body.position = [110.0, 54.15, 238.0];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.position = [110.0, 54.15, 236.0];
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut mikiri = None;
        let mut hits = 0;
        for i in 0..220 {
            duel.force_act = (i < 10).then_some(act);
            let r = duel.step(
                &InputFrame {
                    move_stick: if !sweep && i == counter_tick {
                        [0.0, 1.0]
                    } else {
                        [0.0; 2]
                    },
                    buttons: Buttons {
                        lock_on: i == 1,
                        dodge: !sweep && i == counter_tick,
                        jump: sweep && i == counter_tick,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            mikiri = mikiri.or(r.mikiri);
            hits += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        }
        eprintln!("Emma counter {act}: mikiri {mikiri:?}, incoming hits {hits}");
        if !sweep {
            assert!(mikiri.is_some(), "no Mikiri for {act}");
        }
        assert_eq!(hits, 0, "counter input failed for {act}");
    }
}

#[test]
fn emma_lua_defends_against_wolfs_sword_pressure() {
    let Some(mut duel) = fight() else { return };
    duel.events = None;
    duel.player.body.position = [110.0, 54.15, 238.0];
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [110.0, 54.15, 236.4];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let mut guarded = 0;
    let mut deflected = 0;
    for i in 0..600 {
        let r = duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                buttons: Buttons {
                    attack: i % 8 == 0,
                    lock_on: i == 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        for (hit, result) in r.hits.iter().filter(|(h, _)| h.attacker == 0) {
            guarded += usize::from(result.guarded);
            deflected += usize::from(result.deflected);
            assert!(hit.attack.profile.is_some());
        }
    }
    eprintln!("Emma under sword pressure: {guarded} blocks, {deflected} deflects");
    assert!(guarded > 0 && deflected > 0);
}
