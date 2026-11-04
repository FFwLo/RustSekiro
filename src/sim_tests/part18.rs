//! Sim tests, part 18: the engine states the graph plays (row P13, docs/kb/behaviour-graph.md
//! "Engine states from the graph"): the grapple shot and its move start, the deathblow's throw
//! pair and its kill follow-up. Each passes with the graph leading (player.rs's hand-written path
//! for it is gone); without the script (no extracted c0000.hkx) they skip.

use super::*;

fn graph(app: &App) -> Option<(String, String)> {
    app.world().get_non_send::<crate::hks_wolf::WolfScript>().map(|w| w.graph_main())
}

/// Grapple pressed with a point in range: the script's BEH_A_GROUND_WIRE_SHOOT (env 3000
/// IsWirePossible, ACTION_ARM_WIRE_SHOOT) fires W_GroundWireShoot, and at the shoot's end the
/// graph's selector on WireMoveStartIndex (from WireDramaticAngle, > 50 deg: VERTICAL) picks the
/// move start: a hook almost straight up is GroundWireMoveStart_Up's clip, a low one 9 m ahead
/// _UpperSlope's. Wolf's state follows the graph's at each step.
#[test]
pub(crate) fn the_grapple_shot_and_its_move_start_are_the_graphs() {
    for (hook, want) in [(Vec3::new(0.0, 12.0, 3.0), "GroundWireMoveStart_Up"), (Vec3::new(0.0, 3.5, 9.0), "GroundWireMoveStart_UpperSlope")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        if graph(&app).is_none() {
            return;
        }
        let (seen, _) = grapple_to(&mut app, hook, hook - Vec3::Y * 0.5, 60);
        // (the press reaches the script the tick after: StandIdle first)
        let shot = seen.iter().position(|s| s == "GroundWireShoot").unwrap_or_else(|| panic!("no shoot: {seen:?}"));
        assert_eq!(seen.get(shot + 1).map(String::as_str), Some(want), "hook {hook}: {seen:?}");
    }
}

/// A grapple with no point in range does nothing (env 3000 false: the script never shoots).
#[test]
pub(crate) fn a_grapple_with_no_point_in_range_stays_put() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.update();
    if graph(&app).is_none() {
        return;
    }
    let point = crate::traversal::GrapplePoint { hook: Vec3::new(0.0, 3.5, 9.0), land: Vec3::new(0.0, 3.0, 9.0), fwd: Vec3::NEG_Z, variation: 0 };
    let target = crate::traversal::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: false, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None, host: false });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    for _ in 0..30 {
        app.update();
        let (s, ..) = player_state(&mut app);
        assert!(!crate::wire::is_wire_state(&s), "{s}");
    }
    assert!(!graph(&app).unwrap().0.contains("Wire"));
}

/// The deathblow: the exe's throw check takes the attack press before the script (the graph
/// never starts the combo), the engine fires the throw pair into the graph (W_ThrowAtk500000,
/// then W_ThrowAtk510000), and as the General dies the kill follow-up is the script's
/// BEH_R_THROW_KILL (env 274 with env 273 = atkAnimId + 1): W_ThrowKill510001, its clip
/// a201_510001 on Wolf.
#[test]
pub(crate) fn the_deathblow_kill_follow_up_is_the_scripts_throw_kill() {
    let mut app = app();
    app.update();
    if graph(&app).is_none() {
        return;
    }
    break_enemy_at(&mut app, 1.8);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut states = Vec::new();
    let mut kill = None;
    for i in 0..400u32 {
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        let (gs, gc) = graph(&app).unwrap();
        if states.last() != Some(&gs) {
            states.push(gs.clone());
        }
        if pa == "a201_510001" {
            kill = Some(i);
            assert_eq!((gs.as_str(), gc.as_str()), ("ThrowKill510001", "a201_510001"));
            break;
        }
    }
    assert!(kill.is_some(), "no kill: graph {states:?}");
    assert!(!states.iter().any(|s| s.starts_with("GroundAttackCombo")), "the script took the deathblow's press: {states:?}");
    let start = states.iter().position(|s| s == "ThrowAtk500000");
    let main = states.iter().position(|s| s == "ThrowAtk510000");
    assert!(start.is_some() && main > start, "throw pair not in the graph: {states:?}");
    // The kill's end: Wolf stands, and the graph with him (player.rs's end, synced as W_Idle).
    for _ in 0..400 {
        app.update();
        let (s, ..) = player_state(&mut app);
        if s != "Deathblow" {
            assert_eq!((s.as_str(), graph(&app).unwrap().0.as_str()), ("StandIdle", "StandIdle"));
            return;
        }
    }
    panic!("the kill never ended");
}
