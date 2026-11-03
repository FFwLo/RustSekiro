use std::collections::BTreeSet;

use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::duel::Duel;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::registry::Registry;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn fight() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5400/skeleton.bin",
        "maps/m11_01_00_00/collision.bin",
        "raw/script/m11_01_00_00.luabnd.d/543000_battle.lua",
    ] {
        if !cache.join(path).is_file() {
            eprintln!(
                "skipping Isshin Ashina: missing {}",
                cache.join(path).display()
            );
            return None;
        }
    }
    if sekiro_sim::clips::ClipLibrary::new(cache.join("anim/c0000"))
        .clip("a242_511700")
        .is_none()
    {
        eprintln!("skipping Isshin Ashina: Wolf's aliased a242_511700 is not extracted");
        return None;
    }
    let registry = Registry::load(&cache).unwrap();
    let (spec, placement) = registry.entity("m11_01_00_00", 1110920).unwrap();
    assert_eq!(registry.anim_chr(&spec.chr, spec.npc_param), "c5400");
    let arena = Arena::load(&cache, ArenaSpec::by_name("shura").unwrap(), false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec).unwrap();
    arena.place(&mut duel);
    duel.enemy.body.position = arena
        .floor_at(glam::Vec3::from_array(placement.position))
        .to_array();
    duel.enemy.body.yaw = placement.yaw;
    duel.god = true;
    Some(duel)
}

fn take_mark(duel: &mut Duel) {
    duel.enemy_active = false;
    for _ in 0..300 {
        duel.step(&InputFrame::default(), DT);
    }
    duel.player.body.position = [110.0, 54.15, 238.0];
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [110.0, 54.15, 236.4];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let marks = duel.rules.fighters[1].vitality.deathblows_left;
    let mut broke = false;
    let mut started = false;
    for i in 0..1500 {
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
        broke |= r
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= r.deathblow.started.is_some();
        if started
            && duel.deathblow.is_none()
            && duel.rules.fighters[1].vitality.deathblows_left < marks
        {
            break;
        }
    }
    assert!(
        started,
        "Isshin deathblow never started: {}",
        duel.enemy.state()
    );
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, marks - 1);
}

#[test]
fn isshin_ashina_lua_fights_both_marks_and_finishes_from_map_messages() {
    let Some(mut duel) = fight() else { return };
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
    for phase in [1, 2] {
        duel.enemy_active = true;
        let mut acts = BTreeSet::new();
        let mut contacts = 0;
        let mut unknown = BTreeSet::new();
        for _ in 0..2400 {
            let r = duel.step(&InputFrame::default(), DT);
            assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
            contacts += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
            for event in duel.ai.as_mut().unwrap().drain_log() {
                match event {
                    AiEvent::Action(a) if (3000..3100).contains(&a) => {
                        acts.insert(a);
                    }
                    AiEvent::Error(e) => panic!("{e}"),
                    AiEvent::Unknown(n) => {
                        unknown.insert(n);
                    }
                    _ => {}
                }
            }
            if acts.len() >= 3 && contacts >= 3 {
                break;
            }
        }
        eprintln!(
            "Isshin Ashina phase {phase}: acts {acts:?}, contacts {contacts}, unknown {unknown:?}, HP {}, posture {}, set {}",
            duel.rules.fighters[1].vitality.max_hp,
            duel.rules.fighters[1].posture.max,
            duel.enemy.behavior.runtime.offsets.offset(15)
        );
        assert!(acts.len() >= 3 && contacts >= 3);
        assert!(
            unknown.is_empty(),
            "unanswered Isshin AI queries: {unknown:?}"
        );
        assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(905401));
        take_mark(&mut duel);
        for _ in 0..600 {
            duel.step(&InputFrame::default(), DT);
        }
        if phase == 1 {
            assert!(duel.events.as_ref().unwrap().scripts.flag(11115922));
            assert!(duel.rules.fighters[1].vitality.hp > 0);
        }
    }
    assert!(
        duel.events.as_ref().unwrap().scripts.flag(9316),
        "finishing state {}, events {:?}",
        duel.enemy.state(),
        duel.events.as_ref().unwrap().scripts.log
    );
    assert!(duel.events.as_ref().unwrap().defeated);
}

#[test]
fn isshin_ashina_fire_windup_fires_from_the_maps_original_points() {
    let Some(mut duel) = fight() else { return };
    let cache = sekiro_sim::cache::root();
    duel.enemy_active = false;
    for _ in 0..180 {
        duel.enemy_active = false;
        duel.step(&InputFrame::default(), DT);
    }
    let point = sekiro_sim::events::warp_point(&cache, 11, 1, 1112962).unwrap();
    let floor = duel
        .player
        .ground
        .floor(
            point.position[0],
            point.position[2],
            duel.player.body.position[1] + 5.0,
        )
        .expect("the authored fire point has a playable roof floor");
    duel.player.body.position = [point.position[0], floor, point.position[2]];
    duel.enemy_active = false;
    duel.rules.fighters[1].vitality.deathblows_left = 1;
    let mut parents = BTreeSet::new();
    let mut child_contact = false;
    for i in 0..600 {
        duel.force_act = (i < 10).then_some(3032);
        let report = duel.step(&InputFrame::default(), DT);
        assert!(report.enemy.behavior.script_errors.is_empty(), "{report:?}");
        for bullet in &duel.bullets.live {
            if bullet.event_owner == Some(sekiro_sim::isshin_fire::OWNER) {
                if bullet.def.id == 54000382 {
                    parents.insert(bullet.serial);
                }
                assert_eq!(bullet.shooter, 1);
            }
        }
        child_contact |= report
            .hits
            .iter()
            .any(|(hit, result)| hit.attacker == 1 && hit.judge == 382 && result.hp_damage > 0);
    }
    eprintln!(
        "Isshin One Mind: {} event-shot parents, child damage {child_contact}",
        parents.len()
    );
    assert_eq!(parents.len(), 33);
    assert!(
        child_contact,
        "the authored fire never damaged Wolf on its source point"
    );
    let emitter = duel
        .isshin_fire
        .as_ref()
        .expect("the real c1000 source is loaded");
    assert_eq!(emitter.fighter.vitality.max_hp, 9999);
    assert_eq!(duel.rules.fighters[1].vitality.max_hp, 1600);
}

#[test]
fn isshin_ashina_perilous_counters_and_grab_use_the_installed_throw_pairs() {
    let Some(mut duel) = fight() else { return };
    for (act, counter_tick) in [(3026, Some(26)), (3005, Some(45)), (3104, None)] {
        duel.reset().unwrap();
        duel.events = None;
        duel.enemy_active = false;
        duel.player.body.position = [110.0, 54.15, 238.0];
        duel.player.body.yaw = 0.0;
        duel.enemy.body.position = [110.0, 54.15, 236.4];
        duel.enemy.body.yaw = std::f32::consts::PI;
        let mut mikiri = None;
        let mut grab = None;
        let mut damage = 0;
        let mut incoming = 0;
        for i in 0..420 {
            duel.force_act = (i < 10).then_some(act);
            let pressed = counter_tick == Some(i);
            let report = duel.step(
                &InputFrame {
                    move_stick: if act == 3026 && pressed {
                        [0.0, 1.0]
                    } else {
                        [0.0; 2]
                    },
                    buttons: Buttons {
                        lock_on: i == 1,
                        dodge: act == 3026 && pressed,
                        jump: act == 3005 && pressed,
                        guard: act == 3104,
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
            mikiri = mikiri.or(report.mikiri);
            grab = grab.or(report.grab.started);
            damage += report.grab.hits.iter().map(|(_, hp, _)| hp).sum::<i32>();
            incoming += report
                .hits
                .iter()
                .filter(|(hit, result)| hit.attacker == 1 && result.hp_damage > 0)
                .count();
        }
        eprintln!(
            "Isshin {act}: Mikiri {mikiri:?}, grab {grab:?}, throw damage {damage}, incoming {incoming}"
        );
        match act {
            3026 => {
                assert!(mikiri.is_some());
                assert_eq!(incoming, 0);
            }
            3005 => assert_eq!(incoming, 0),
            _ => assert!(grab.is_some() && damage > 0),
        }
    }
}
