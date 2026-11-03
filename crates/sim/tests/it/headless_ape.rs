//! Both installed burrow apes fight one Wolf on the arena's extracted collision.
use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
};
const DT: f32 = 1.0 / 60.0;

fn approach(d: &mut Duel, p: [f32; 3], distance: f32) {
    let floor = d
        .player
        .ground
        .floor(p[0], p[2] + distance, p[1] + 5.0)
        .unwrap();
    d.player.body.position = [p[0], floor, p[2] + distance];
    d.player.body.grounded = true;
    d.player.body.velocity = [0.0; 3];
}

fn load() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5100/tae.json",
        "anim/c0000/tae.json",
        "anim/c5100/skeleton.bin",
        "anim/c0000/skeleton.bin",
        "raw/chr/c5100.chrbnd.d/c5100.flver",
        "maps/m17_00_00_00/collision.bin",
        "raw/event/m17_00_00_00.emevd",
        "raw/script/m17_00_00_00.luabnd.d/510010_battle.lua",
        "raw/script/m17_00_00_00.luabnd.d/510001_battle.lua",
        "raw/script/m17_00_00_00.luabnd.d/510000_logic.lua",
        "raw/param/gameparam/gameparam.parambnd.d/NpcParam.param",
        "raw/param/gameparam/gameparam.parambnd.d/NpcThinkParam.param",
        "raw/param/gameparam/gameparam.parambnd.d/ThrowParam.param",
    ] {
        if !cache.join(path).is_file() {
            eprintln!("skipping Headless Ape: missing {path}");
            return None;
        }
    }
    let spec = ArenaSpec::by_name("apes-burrow").unwrap();
    let mut d = Duel::load_with(&cache, 4.0, spec.enemy_spec()).unwrap();
    Arena::load(&cache, spec, false).unwrap().place(&mut d);
    d.god = true;
    Some(d)
}

#[test]
fn headless_ape_first_mark_refills_from_its_paired_tae() {
    let Some(mut d) = load() else { return };
    for _ in 0..20 {
        d.step(&InputFrame::default(), DT);
    }
    d.enemy_active = false;
    let p = d.enemy.body.position;
    approach(&mut d, p, 4.0);
    d.player.body.yaw = 0.0;
    d.enemy.body.yaw = std::f32::consts::PI;
    d.rules.fighters[1].vitality.hp = 1;
    let mut rows = None;
    let mut refilled = false;
    for i in 0..600 {
        let r = d.step(
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
        if d.rules.fighters[1].vitality.deathblows_left == 1 && d.rules.fighters[1].vitality.hp > 1
        {
            refilled = true;
            break;
        }
    }
    assert_eq!(rows, Some((15100500, 15100501)));
    assert!(
        refilled,
        "headless first-mark refill failed: {:?}",
        d.rules.fighters[1].vitality
    );
    assert_eq!(d.rules.fighters[1].vitality.max_hp, 2240);
    assert!(!d.events.as_ref().unwrap().scripts.flag(9307));
}

#[test]
#[ignore = "slow: two independent Lua apes, summon waits and paired execution on map collision"]
fn headless_ape_both_lua_runtimes_attack_wolf() {
    let Some(mut d) = load() else { return };
    let p = d.enemy.body.position;
    approach(&mut d, p, 4.0);
    let mut main_hit = false;
    for _ in 0..1800 {
        let r = d.step(&InputFrame::default(), DT);
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        main_hit |= r.hits.iter().any(|(h, _)| h.attacker == 1);
        if main_hit {
            break;
        }
    }
    assert!(main_hit, "headless Lua never hits Wolf");
    d.enemy_active = false;
    for _ in 0..360 {
        d.step(&InputFrame::default(), DT);
    }
    let p = d.enemy.body.position;
    approach(&mut d, p, 4.0);
    d.player.body.yaw = 0.0;
    d.enemy.body.yaw = std::f32::consts::PI;
    d.rules.fighters[1].vitality.hp = 1;
    for i in 0..600 {
        d.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 30 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        if d.rules.fighters[1].vitality.deathblows_left == 1 && d.rules.fighters[1].vitality.hp > 1
        {
            break;
        }
    }
    assert_eq!(d.rules.fighters[1].vitality.deathblows_left, 1);
    d.enemy_active = true;
    // Its Lua command must reach 1702871 before the real three-second summon wait.
    let mut summoned = false;
    for _ in 0..2400 {
        d.step(&InputFrame::default(), DT);
        if d.headless_ape.as_ref().unwrap().duel.enemy_active {
            summoned = true;
            break;
        }
    }
    assert!(
        summoned,
        "map never enables the brown ape: position={:?}, request={:?}, goals={:?}",
        d.enemy.body.position,
        d.ai.as_ref().unwrap().state.event_requests,
        d.ai.as_ref()
            .unwrap()
            .state
            .goals
            .iter()
            .flatten()
            .map(|g| (g.kind, &g.params))
            .collect::<Vec<_>>()
    );
    let brown = d.headless_ape.as_mut().unwrap();
    assert_eq!(brown.duel.spec.npc_param, 51_000_010);
    assert_eq!(brown.duel.spec.think, 51_000_011);
    assert_eq!(brown.duel.rules.fighters[1].vitality.max_hp, 1600);
    // Wait for the authored appearance, then approach the independently placed brown ape.
    let mut brown_hit = false;
    for _ in 0..240 {
        d.step(&InputFrame::default(), DT);
        brown_hit |= d
            .headless_ape
            .as_ref()
            .unwrap()
            .report
            .hits
            .iter()
            .any(|(h, _)| h.attacker == 1);
    }
    // Isolate contact behavior on the main placement's open helper floor after the entrance.
    let cache = sekiro_sim::cache::root();
    let arena = Arena::load(&cache, ArenaSpec::by_name("apes-burrow").unwrap(), false).unwrap();
    let p = arena.layout.boss.position.to_array();
    d.headless_ape.as_mut().unwrap().duel.enemy.body.position = p;
    d.headless_ape.as_mut().unwrap().duel.enemy.body.velocity = [0.0; 3];
    d.enemy.body.position = [p[0] + 4.0, p[1], p[2]];
    approach(&mut d, p, 3.0);
    d.enemy_active = true;
    for i in 0..1800 {
        if brown_hit {
            break;
        }
        d.step(
            &InputFrame {
                move_stick: [0.0; 2],
                buttons: Buttons {
                    lock_on: i == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        let brown = d.headless_ape.as_mut().unwrap();
        assert!(
            brown.report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            brown.report
        );
        brown_hit |= brown.report.hits.iter().any(|(h, _)| h.attacker == 1);
        for event in brown.duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(e) => eprintln!("brown AI missing: {e}"),
                e if i < 3 => eprintln!("brown goal: {e:?}"),
                _ => {}
            }
        }
        if brown_hit {
            break;
        }
    }
    assert!(brown_hit, "independent brown Lua never hits Wolf");
    d.enemy_active = false;
    for _ in 0..360 {
        d.step(&InputFrame::default(), DT);
    }
    let brown = d.headless_ape.as_mut().unwrap();
    brown.duel.rules.fighters[1].vitality.hp = 1;
    let p = brown.duel.enemy.body.position;
    brown.duel.enemy.body.yaw = std::f32::consts::PI;
    approach(&mut d, p, 4.0);
    d.player.body.yaw = 0.0;
    d.enemy.body.position = [p[0] + 15.0, p[1], p[2]];
    let mut paired = false;
    for i in 0..900 {
        let r = d.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 30 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        paired |= r.deathblow.started.is_some();
        if d.headless_ape.as_ref().unwrap().duel.rules.fighters[1].dead() {
            break;
        }
    }
    assert!(
        paired && d.headless_ape.as_ref().unwrap().duel.rules.fighters[1].dead(),
        "brown ape execution failed: paired={paired}, vitality={:?}, clip={:?}, Wolf={:?}",
        d.headless_ape.as_ref().unwrap().duel.rules.fighters[1].vitality,
        d.headless_ape.as_ref().unwrap().report.enemy.animation,
        d.player.behavior.runtime.main_clip()
    );
    for _ in 0..360 {
        d.step(&InputFrame::default(), DT);
    }
    let p = d.enemy.body.position;
    approach(&mut d, p, 4.0);
    d.player.body.yaw = 0.0;
    d.enemy.body.yaw = std::f32::consts::PI;
    d.rules.fighters[1].vitality.hp = 1;
    let mut continuation = false;
    let mut message = false;
    for i in 0..1500 {
        let r = d.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 30 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        continuation |= r.deathblow.started == Some((15100509, 15100509));
        message |= d.enemy.tae_runtime.event_message == Some(30);
    }
    assert!(
        continuation && message,
        "burrow finishing continuation failed"
    );
    assert!(d.events.as_ref().unwrap().scripts.flag(9307));
    assert_eq!(d.events.as_ref().unwrap().standing_fog_walls().count(), 0);
}
