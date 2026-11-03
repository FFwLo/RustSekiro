use std::path::PathBuf;

use sekiro_sim::{
    Buttons, InputFrame,
    arena::{Arena, ArenaSpec},
    duel::Duel,
    lua_ai::AiEvent,
    registry::Registry,
};

const DT: f32 = 1.0 / 60.0;

#[test]
#[ignore = "slow: sanctuary cast and complete first-phase sword replay"]
fn divine_dragon_old_dragons_sword_replay_reaches_native_handover() {
    let Some(cache) = cache() else { return };
    let spec = ArenaSpec::by_name("fountainhead-sanctuary").unwrap();
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut d = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut d);
    d.god = true;
    let mut contacts = 0;
    let mut outgoing = 0;
    let mut revivals = 0;
    let mut last_hp = 1000;
    for tick in 0..24000 {
        if d.spec.chr == "c5200" {
            break;
        }
        let cast = d.old_dragons.as_ref().unwrap();
        let p = glam::Vec3::from_array(d.player.body.position);
        let target = cast
            .actors
            .iter()
            .filter(|a| {
                a.duel.spec.chr == "c5300"
                    && !a.duel.rules.fighters[1].dead()
                    && !a.duel.enemy.tae_frame().sp_effects.contains(&3_530_030)
            })
            .min_by(|a, b| {
                let distance = |a: &sekiro_sim::duel::old_dragons::Actor| {
                    glam::Vec3::from_array(a.duel.enemy.body.position).distance_squared(p)
                };
                distance(a).total_cmp(&distance(b))
            });
        let mut input = InputFrame::default();
        if let Some(target) = target {
            let v = glam::Vec3::from_array(target.duel.enemy.body.position) - p;
            input.camera_yaw = (-v.x).atan2(-v.z);
            input.move_stick = [0.0, if v.x.hypot(v.z) > 1.3 { 1.0 } else { 0.0 }];
            input.buttons = Buttons {
                attack: tick >= 900 && tick % 40 < 3,
                ..Default::default()
            };
        }
        let r = d.step(&input, DT);
        if tick % 600 == 0 {
            eprintln!(
                "tick {tick}: Wolf {:?}, contacts {contacts}, outgoing {outgoing}, selected {:?}",
                d.player.body.position,
                d.old_dragons.as_ref().map(|e| (
                    e.selected,
                    e.actors[e.selected].duel.enemy.body.position,
                    e.actors[e.selected].duel.enemy.state().to_owned(),
                    e.actors[e.selected].duel.rules.fighters[1].vitality.hp
                ))
            );
        }
        contacts += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        outgoing += r.hits.iter().filter(|(h, _)| h.attacker == 0).count();
        if d.spec.chr == "c5200" {
            break;
        }
        let hp = d.rules.fighters[1].vitality.hp;
        if hp != last_hp {
            eprintln!(
                "tick {tick}: Old Dragons bar {hp}, outgoing {outgoing}, contacts {contacts}"
            );
            last_hp = hp;
        }
        let cast = d.old_dragons.as_mut().unwrap();
        revivals = cast.actors.iter().map(|a| a.revivals as usize).sum();
        for a in &mut cast.actors {
            assert!(
                a.report.enemy.behavior.script_errors.is_empty(),
                "{} {:?}",
                a.entity,
                a.report.enemy.behavior.script_errors
            );
            for event in a.duel.ai.as_mut().unwrap().drain_log() {
                if let AiEvent::Error(error) = event {
                    panic!("actor {}: {error}", a.entity);
                }
            }
        }
    }
    eprintln!(
        "handover {}, contacts {contacts}, outgoing {outgoing}, revivals {revivals}, bar {}, counter {}",
        d.spec.chr,
        d.rules.fighters[1].vitality.hp,
        sekiro_sim::event_values::read(&d.events.as_ref().unwrap().scripts.flags, 12_505_828, 5)
    );
    assert!(contacts > 0 && outgoing > 0 && revivals > 0);
    assert_eq!(d.spec.chr, "c5200");
    assert!(d.events.as_ref().unwrap().scripts.flag(12_505_881));
    assert!(
        d.old_dragons.as_ref().unwrap().actors.iter().any(|a| a
            .duel
            .enemy
            .tae_frame()
            .sp_effects
            .contains(&3_530_030)
            && a.duel.rules.fighters[1].vitality.max_hp == 160)
    );
}

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    for path in [
        "anim/c5200/tae.json",
        "anim/c5300/tae.json",
        "anim/c5310/tae.json",
        "anim/c5320/tae.json",
        "anim/c0000/tae.json",
        "maps/m25_00_00_00/collision.bin",
        "raw/event/m25_00_00_00.emevd",
    ] {
        if !cache.join(path).exists() {
            eprintln!("skipping Dragon: missing {path}");
            return None;
        }
    }
    Some(cache)
}

#[test]
#[ignore = "slow: sanctuary cast loading and authored Dragon/tree Lua replay"]
fn divine_dragon_retry_runs_its_lua_and_authored_bullets() {
    let Some(cache) = cache() else { return };
    let arena_spec = ArenaSpec::by_name("fountainhead-sanctuary").unwrap();
    let arena = Arena::load(&cache, arena_spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, arena_spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.player.wire =
        sekiro_sim::wire::Wire::load(&cache, &cache.join(format!("maps/{}", arena_spec.map)))
            .unwrap();
    duel.god = true;
    duel.events
        .as_mut()
        .unwrap()
        .scripts
        .set_flag(12_505_882, true);
    let mut attacks = 0;
    let mut fired = 0;
    let mut contacts = 0;
    let mut clips = std::collections::BTreeSet::new();
    let mut wire_points = 0;
    let mut charged = false;
    let mut grappled = false;
    let mut returned = false;
    let mut raised = std::collections::BTreeSet::new();
    for tick in 0..3600 {
        let mut input = InputFrame::default();
        if let Some(point) = duel
            .player
            .wire
            .points
            .iter()
            .find(|p| p.source_part.starts_with("sanctuary:") && p.position.is_some())
            .and_then(|p| p.position)
        {
            wire_points += 1;
            // Aim the camera at the tree's hook as a player does: turned to it and pitched so
            // it sits at the centre of the screen (the search only sees hooks in view).
            let focus = glam::Vec3::from_array(duel.player.body.position)
                + glam::Vec3::Y * duel.player.wire.rig.focus_height;
            let v = glam::Vec3::from_array(point) - focus;
            input.camera_yaw = (-v.x).atan2(-v.z);
            input.camera_pitch = -v.y.atan2(v.x.hypot(v.z));
            input.buttons.grapple = tick % 40 < 3;
        }
        input.buttons.attack = duel.player.tae_frame().sp_effects.contains(&9495);
        let before = duel.bullets.live.len();
        let report = duel.step(&input, DT);
        charged |= duel.player.tae_frame().sp_effects.contains(&9495);
        grappled |= duel.player.wire.active.is_some();
        returned |= report.other_counter;
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        if duel.spec.chr != "c5200" {
            continue;
        }
        fired += duel.bullets.live.len().saturating_sub(before);
        contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        if let Some(c) = report.enemy.animation {
            clips.insert(c);
        }
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(n) => eprintln!("Dragon unknown {n}"),
                _ => {}
            }
        }
        for actor in duel
            .old_dragons
            .as_mut()
            .unwrap()
            .actors
            .iter_mut()
            .filter(|a| (2_500_880..=2_500_884).contains(&a.entity))
        {
            assert!(actor.report.enemy.behavior.script_errors.is_empty());
            if actor
                .duel
                .enemy
                .tae_frame()
                .sp_effects
                .iter()
                .any(|id| matches!(id, 5025..=5027))
            {
                raised.insert(actor.entity);
            }
            for event in actor.duel.ai.as_mut().unwrap().drain_log() {
                if let AiEvent::Error(error) = &event {
                    panic!("tree {}: {error}", actor.entity);
                }
                if matches!(
                    event,
                    AiEvent::Error(_) | AiEvent::Unknown(_) | AiEvent::Action(_)
                ) {
                    eprintln!("tree {} {event:?}", actor.entity);
                }
            }
        }
        if tick % 600 == 0 {
            let trees: Vec<_> = duel
                .old_dragons
                .as_ref()
                .unwrap()
                .actors
                .iter()
                .filter(|a| (2_500_880..=2_500_884).contains(&a.entity))
                .map(|a| {
                    (
                        a.entity,
                        a.duel.enemy.state().to_owned(),
                        a.duel.enemy.tae_frame().sp_effects.clone(),
                        a.duel.enemy_active,
                        a.duel.ai.as_ref().unwrap().state.event_requests.clone(),
                    )
                })
                .collect();
            eprintln!(
                "Dragon tick {tick}, flags888 {}, flags881 {}, trees {trees:?}, wire {wire_points}, grapple {grappled}, charge {charged}, returned {returned}",
                duel.events.as_ref().unwrap().scripts.flag(12_505_888),
                duel.events.as_ref().unwrap().scripts.flag(12_505_881)
            );
        }
        if attacks >= 3
            && fired >= 1
            && contacts >= 1
            && wire_points > 0
            && grappled
            && charged
            && returned
        {
            break;
        }
    }
    eprintln!(
        "Dragon: {attacks} attacks, {fired} bullets, {contacts} contacts, clips {clips:?}, wire {wire_points}, grapple {grappled}, charge {charged}, returned {returned}, HP {}",
        duel.rules.fighters[1].vitality.hp
    );
    assert_eq!(duel.spec.chr, "c5200");
    assert!(attacks >= 3 && fired >= 1 && contacts >= 1);
    assert_eq!(
        raised.len(),
        5,
        "all five helpers must run their native rise/hold"
    );
    assert!(
        duel.events
            .as_ref()
            .unwrap()
            .scripts
            .cast_scene
            .attachments
            .len()
            >= 30
    );
}

#[test]
fn old_dragon_placed_combat_row_runs_its_own_lua() {
    let Some(cache) = cache() else { return };
    let arena_spec = ArenaSpec::by_name("fountainhead-sanctuary").unwrap();
    let arena = Arena::load(&cache, arena_spec, false).unwrap();
    let registry = Registry::load(&cache).unwrap();
    let (mut spec, spot) = registry.entity(arena_spec.map, 2_500_811).unwrap();
    spec.starts_in_battle = true;
    let mut duel = Duel::load_with(&cache, 2.4, spec).unwrap();
    arena.place(&mut duel);
    duel.enemy.body.position = spot.position;
    duel.enemy.body.yaw = std::f32::consts::PI;
    duel.player.body.position = [spot.position[0], spot.position[1], spot.position[2] + 2.4];
    duel.player.body.yaw = 0.0;
    duel.god = true;
    // Event 12505801 supplies command 30 to the Old Dragons' placed group 2505830.
    duel.ai.as_mut().unwrap().state.event_requests.insert(0, 30);
    let mut attacks = 0;
    let mut contacts = 0;
    for _ in 0..1800 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        contacts += report.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        for event in duel.ai.as_mut().unwrap().drain_log() {
            match event {
                AiEvent::Action(a) if (3000..3100).contains(&a) => attacks += 1,
                AiEvent::Error(e) => panic!("{e}"),
                AiEvent::Unknown(n) => eprintln!("Old Dragon unknown {n}"),
                _ => {}
            }
        }
        if attacks >= 2 && contacts > 0 {
            break;
        }
    }
    eprintln!("Old Dragon: {attacks} attacks, {contacts} contacts");
    assert!(attacks >= 2 && contacts > 0);
}

#[test]
fn divine_dragon_collapse_eye_throw_plays_native_pair_and_defeat() {
    let Some(cache) = cache() else { return };
    let registry = Registry::load(&cache).unwrap();
    let (spec, spot) = registry.entity("m25_00_00_00", 2_500_800).unwrap();
    let mut d = Duel::load_with(&cache, 6.0, spec).unwrap();
    d.enemy.body.position = spot.position;
    d.enemy.body.yaw = spot.yaw;
    d.enemy_active = false;
    d.god = true;
    // Isolate the authored collapse opening; the complete lightning replay is separate.
    d.enemy.behavior.runtime.fire_event("W_Event21000");
    for _ in 0..30 {
        d.step(&InputFrame::default(), DT);
    }
    eprintln!(
        "Dragon collapse: {}, unlocks {:?}, eye {:?}",
        d.enemy.state(),
        d.enemy.tae_frame().action_unlocks,
        d.dragon_eye()
    );
    assert!(d.enemy.tae_frame().action_unlocks.contains(&104));
    let eye = d.dragon_eye().unwrap();
    d.player.body.position = eye.position;
    d.player.body.yaw = eye.yaw;
    d.player.body.gravity_enabled = false;
    d.player.body.grounded = true;
    let mut pair = None;
    let mut player_clip = false;
    let mut defender_clip = false;
    for tick in 0..1800 {
        let r = d.step(
            &InputFrame {
                buttons: Buttons {
                    attack: tick < 3,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        pair = pair.or(r.deathblow.started);
        player_clip |= r.player.animation.as_deref() == Some("a233_510900");
        defender_clip |= r.enemy.animation.as_deref() == Some("a000_012900");
        if d.events.as_ref().unwrap().defeated {
            break;
        }
    }
    eprintln!(
        "Dragon pair {pair:?}, player {player_clip}, defender {defender_clip}, flag9310 {}",
        d.events.as_ref().unwrap().scripts.flag(9310)
    );
    assert_eq!(pair, Some((15_200_090, 15_200_090)));
    assert!(player_clip && defender_clip);
    assert!(d.events.as_ref().unwrap().defeated);
    assert!(d.events.as_ref().unwrap().scripts.flag(9310));
}
