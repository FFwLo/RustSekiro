//! Real silvergrass Lua combat, mark consumption and map-script phase boundaries.
use std::collections::BTreeSet;

use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
};

const DT: f32 = 1.0 / 60.0;

fn fight(name: &str) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5400/skeleton.bin",
        "anim/c7110/skeleton.bin",
        "maps/m11_02_00_00/collision.bin",
        "raw/event/m11_02_00_00.emevd",
        "raw/script/m11_02_00_00.luabnd.d/540000_battle.lua",
    ] {
        if !cache.join(path).is_file() {
            eprintln!(
                "skipping Sword Saint: missing {}",
                cache.join(path).display()
            );
            return None;
        }
    }
    let arena = Arena::load(&cache, ArenaSpec::by_name(name).unwrap(), false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, arena.spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    Some(duel)
}

fn combat(duel: &mut Duel, phase: usize) {
    duel.enemy_active = true;
    let mut acts = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut contacts = 0;
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(report.enemy.behavior.script_errors.is_empty(), "{report:?}");
        contacts += report
            .hits
            .iter()
            .filter(|(hit, _)| hit.attacker == 1)
            .count();
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3200).contains(&a) => {
                    acts.insert(a);
                }
                AiEvent::Unknown(n) => {
                    unknown.insert(n);
                }
                AiEvent::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        if acts.len() >= 3 && contacts >= 3 {
            break;
        }
    }
    eprintln!(
        "Sword Saint phase {phase}: acts {acts:?}, contacts {contacts}, unknown {unknown:?}, HP {}, posture {}, marks {}, set {}, state {}, positions {:?}/{:?}",
        duel.rules.fighters[1].vitality.max_hp,
        duel.rules.fighters[1].posture.max,
        duel.rules.fighters[1].vitality.deathblows_left,
        duel.enemy.behavior.runtime.offsets.offset(15),
        duel.enemy.state(),
        duel.player.body.position,
        duel.enemy.body.position
    );
    assert!(acts.len() >= 3 && contacts >= 3);
    assert!(
        unknown.is_empty(),
        "unanswered Sword Saint AI queries: {unknown:?}"
    );
}

fn take_mark(duel: &mut Duel) {
    duel.enemy_active = false;
    for _ in 0..300 {
        duel.step(&InputFrame::default(), DT);
    }
    // Short paired-throw fixture on the arena floor, using real Wolf sword input.
    // Only posture is reduced; no HP, marks, effects or event flags are injected.
    let cache = sekiro_sim::cache::root();
    let p = sekiro_sim::events::warp_point(&cache, 11, 2, 1122800)
        .unwrap()
        .position;
    duel.player.body.position = p;
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let marks = duel.rules.fighters[1].vitality.deathblows_left;
    let chr = duel.spec.chr.clone();
    let mut broke = false;
    let mut started = false;
    for i in 0..1500 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        let report = duel.step(
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
        assert!(
            report.player.behavior.script_errors.is_empty(),
            "{report:?}"
        );
        assert!(report.enemy.behavior.script_errors.is_empty(), "{report:?}");
        broke |= report
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= report.deathblow.started.is_some();
        if duel.spec.chr != chr {
            assert!(started, "hand-over without a paired deathblow");
            return;
        }
        if started
            && duel.deathblow.is_none()
            && duel.rules.fighters[1].vitality.deathblows_left < marks
        {
            break;
        }
    }
    assert!(
        started,
        "{} deathblow never started: {}",
        duel.spec.chr,
        duel.enemy.state()
    );
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, marks - 1);
}

fn three_marks(name: &str, defeat_flag: i32) {
    let Some(mut duel) = fight(name) else {
        return;
    };
    if duel.spec.chr == "c7110" {
        duel.set_brain(sekiro_sim::brain::BrainKind::Scripted.build("c7110"), false)
            .unwrap();
        assert!(duel.boss_brain.is_none());
        combat(&mut duel, 1);
        take_mark(&mut duel);
        for _ in 0..900 {
            duel.step(&InputFrame::default(), DT);
        }
        assert_eq!(duel.spec.chr, "c5400");
        assert_eq!(duel.events.as_ref().unwrap().boss_entity, 1120860);
        assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(905400));
    }
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 3);
    if name == "silvergrass-inner-isshin" {
        assert_eq!(duel.spec.display_name(), "Inner Isshin");
    }
    for phase in 2..=4 {
        combat(&mut duel, phase);
        take_mark(&mut duel);
        duel.enemy_active = true;
        for _ in 0..900 {
            duel.step(&InputFrame::default(), DT);
        }
        eprintln!(
            "after phase {phase}: marks {}, HP {}, set {}, state {}, clip {:?}",
            duel.rules.fighters[1].vitality.deathblows_left,
            duel.rules.fighters[1].vitality.hp,
            duel.enemy.behavior.runtime.offsets.offset(15),
            duel.enemy.state(),
            duel.enemy.behavior.runtime.main_clip()
        );
        if phase < 4 {
            assert!(duel.rules.fighters[1].vitality.hp > 0);
            assert!(duel.events.as_ref().unwrap().scripts.flag(11125862));
            if phase == 2 {
                assert_eq!(duel.enemy.behavior.runtime.offsets.offset(15), 100);
            }
        }
    }
    for _ in 0..300 {
        duel.step(&InputFrame::default(), DT);
        if duel.enemy.state() == "Event21000" {
            break;
        }
    }
    assert_eq!(duel.enemy.state(), "Event21000");
    for _ in 0..360 {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(duel.enemy.state(), "Event21000");
    assert!(!duel.events.as_ref().unwrap().defeated);
    let mut execution = None;
    for tick in 0..2400 {
        if execution.is_none() && matches!(duel.enemy.state().as_str(), "Event20200" | "Event21000")
        {
            let p = duel.enemy.body.position;
            let f = sekiro_sim::deathblow::Placement {
                position: p,
                yaw: duel.enemy.body.yaw,
            }
            .forward();
            duel.player.body.position = [p[0] - f[0] * 1.6, p[1], p[2] - f[1] * 1.6];
            duel.player.body.yaw = duel.enemy.body.yaw;
        }
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: execution.is_none() && tick % 2 == 0 && duel.deathblow_available(),
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        execution = execution.or(r.deathblow.started);
        if duel.events.as_ref().unwrap().defeated {
            break;
        }
    }
    assert_eq!(
        execution,
        Some((15400590, 15400590)),
        "final state {}",
        duel.enemy.state()
    );
    assert!(
        duel.events.as_ref().unwrap().scripts.flag(defeat_flag),
        "state {}, events {:?}",
        duel.enemy.state(),
        duel.events.as_ref().unwrap().scripts.log
    );
    assert!(duel.events.as_ref().unwrap().defeated);
}

#[test]
fn sword_saint_four_phases_reach_the_maps_defeat_message() {
    three_marks("silvergrass", 9312);
}

#[test]
fn sword_saint_inner_rematch_uses_its_own_lua_and_defeat_event() {
    three_marks("silvergrass-inner-isshin", 9347);
}

fn spear(duel: &mut Duel) {
    duel.enemy_active = false;
    duel.events = None;
    for i in 0..600 {
        duel.force_act = (i < 10).then_some(3015);
        let r = duel.step(&InputFrame::default(), DT);
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
    }
    assert_eq!(duel.enemy.behavior.runtime.offsets.offset(15), 100);
}

fn close_pair(duel: &mut Duel, distance: f32) {
    let cache = sekiro_sim::cache::root();
    let p = sekiro_sim::events::warp_point(&cache, 11, 2, 1122800)
        .unwrap()
        .position;
    let floor = duel.player.ground.floor(p[0], p[2], p[1] + 5.0).unwrap();
    duel.player.body.position = [p[0], floor, p[2]];
    duel.player.body.yaw = 0.0;
    let enemy_floor = duel
        .enemy
        .ground
        .floor(p[0], p[2] - distance, p[1] + 5.0)
        .unwrap();
    duel.enemy.body.position = [p[0], enemy_floor, p[2] - distance];
    duel.enemy.body.yaw = std::f32::consts::PI;
}

#[test]
fn sword_saint_thrust_and_sweep_counters_in_both_animation_sets() {
    let Some(mut duel) = fight("silvergrass-isshin") else {
        return;
    };
    for (set, act, counter_tick, thrust) in [
        (0, 3026, 26, true),
        (0, 3005, 45, false),
        (100, 3016, 54, true),
        (100, 3006, 45, false),
    ] {
        duel.reset().unwrap();
        duel.enemy_active = false;
        duel.events = None;
        if set == 100 {
            spear(&mut duel);
        }
        close_pair(&mut duel, 1.6);
        let mut mikiri = None;
        let mut incoming = 0;
        let mut warning = false;
        for tick in 0..300 {
            duel.force_act = (tick < 10).then_some(act);
            let pressed = tick == counter_tick;
            let r = duel.step(
                &InputFrame {
                    move_stick: if pressed && thrust {
                        [0.0, 1.0]
                    } else {
                        [0.0; 2]
                    },
                    buttons: Buttons {
                        lock_on: tick == 1,
                        dodge: pressed && thrust,
                        jump: pressed && !thrust,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DT,
            );
            assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            mikiri = mikiri.or(r.mikiri);
            warning |= r.perilous.is_some();
            incoming += r
                .hits
                .iter()
                .filter(|(h, r)| h.attacker == 1 && r.hp_damage > 0)
                .count();
        }
        eprintln!(
            "Sword Saint set {set} act {act}: warning {warning}, mikiri {mikiri:?}, incoming {incoming}"
        );
        assert!(warning);
        assert_eq!(incoming, 0);
        if thrust {
            assert!(mikiri.is_some());
        }
    }
}

#[test]
fn sword_saint_pistol_and_lightning_use_the_authored_attacks() {
    let Some(mut duel) = fight("silvergrass-isshin") else {
        return;
    };
    spear(&mut duel);
    close_pair(&mut duel, 6.0);
    let mut shots = BTreeSet::new();
    let mut damage = 0;
    for tick in 0..360 {
        duel.force_act = (tick < 10).then_some(3025);
        let r = duel.step(&InputFrame::default(), DT);
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        for bullet in &duel.bullets.live {
            if matches!(bullet.def.id, 54000810 | 54000811) {
                shots.insert(bullet.serial);
            }
        }
        damage += r
            .hits
            .iter()
            .filter(|(h, _)| h.attacker == 1)
            .map(|(_, r)| r.hp_damage)
            .sum::<i32>();
    }
    eprintln!("Sword Saint pistol: {} shots, {damage} damage", shots.len());
    assert!(!shots.is_empty() && damage > 0);
    duel.reset().unwrap();
    spear(&mut duel);
    close_pair(&mut duel, 6.0);
    let mut capture = false;
    let mut jumped = false;
    for tick in 0..300 {
        duel.force_act = (tick < 10).then_some(3056);
        let jump = !jumped
            && duel
                .enemy
                .behavior
                .runtime
                .main_clip()
                .is_some_and(|c| c.animation == "a100_003056" && c.time > 1.3);
        jumped |= jump;
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    jump,
                    lock_on: tick == 1,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        for (h, result) in &r.hits {
            if h.attacker == 1 {
                eprintln!(
                    "lightning tick {tick}: attack {}, hp {}, positions {:?}/{:?}, clip {:?}",
                    h.attack.atk_id,
                    result.hp_damage,
                    duel.player.body.position,
                    duel.enemy.body.position,
                    duel.enemy.behavior.runtime.main_clip()
                );
            }
        }
        capture |= duel.rules.fighters[0]
            .status
            .running
            .iter()
            .any(|e| (9490..=9495).contains(&e.id))
            && !duel.player.body.grounded;
        if capture {
            break;
        }
    }
    eprintln!(
        "lightning jumped {jumped}, positions {:?}/{:?}, clip {:?}",
        duel.player.body.position,
        duel.enemy.body.position,
        duel.enemy.behavior.runtime.main_clip()
    );
    assert!(capture, "Wolf did not capture Isshin's lightning airborne");
    let hp = duel.rules.fighters[1].vitality.hp;
    for tick in 0..180 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: tick % 2 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        if duel.rules.fighters[1].status.has(9445) {
            assert!(duel.rules.fighters[1].vitality.hp < hp);
            return;
        }
    }
    panic!("the lightning reversal did not hit Isshin");
}
