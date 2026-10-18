//! Sim tests, part 7: 43 tests, from `reigniting_a_burn_refreshes_it` to `no_gourd_while_guarding_and_a_tool_switch_in_the_air`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// A burning Wolf set on fire again (burns have disableBlood 0) keeps one burn, its time started
/// over - not a second affliction whose flames (midst fx) would never be stopped.
#[test]
pub(crate) fn reigniting_a_burn_refreshes_it() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    world.resource_scope(|_, combat: Mut<Combat>| {
        let mut s = Statuses::default();
        let burning = |s: &mut Statuses| (1..=10).find(|_| s.hit_wolf(&combat, wolf, weapon, &[9115], Guard::None).iter().any(|l| l == "BURNING"));
        assert!(burning(&mut s).is_some());
        let first: Vec<(i64, f32)> = s.0[&wolf].effects.iter().map(|a| (a.id, a.left)).collect();
        assert_eq!(first.len(), 1, "{first:?}");
        // Its flames on the body (fx entity stand-in).
        let flames = Entity::PLACEHOLDER;
        let a = &mut s.0.get_mut(&wolf).unwrap().effects[0];
        a.left = 0.5;
        a.fx_mut().push(flames);
        assert!(burning(&mut s).is_some(), "no second ignition");
        let again: Vec<(i64, f32)> = s.0[&wolf].effects.iter().map(|a| (a.id, a.left)).collect();
        assert_eq!(again, first, "the burn was not refreshed in place");
        assert_eq!(s.0.get_mut(&wolf).unwrap().effects[0].fx_mut().as_slice(), &[flames], "the burning flames were dropped unstopped");
    });
}

#[test]
pub(crate) fn the_head_kick_relaunch_goes_straight_up() {
    // Live (30 locked head kicks a000_213115 in the rec_* recordings): Wolf goes straight up after
    // the relaunch; the clip's 0.27 m step back (frames 17-20) never shows, as the air velocity,
    // not the root motion, moves him once he falls free (actor.rs advance).
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 1.0;
        let e = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
        if !world.contains_resource::<crate::camera::LockOn>() {
            world.insert_resource(crate::camera::LockOn::default());
        }
        world.resource_mut::<crate::camera::LockOn>().target = Some(e);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut launch, mut relaunch) = (None, None);
    let mut zs = Vec::new();
    for i in 0..120 {
        if launch.map(|l| i == l + 14).unwrap_or(false) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(world).unwrap();
        if a.airborne && launch.is_none() {
            launch = Some(i);
        }
        if a.state == "AirKickEnemyJumpStart_F_Lock" {
            if relaunch.is_none() && a.vel_y > 5.0 {
                relaunch = Some(i);
            }
            if relaunch.is_some() {
                zs.push(t.translation.z);
            }
        }
    }
    assert!(zs.len() > 10, "no relaunch: {}", log_text(&app));
    let drift = zs.iter().fold(0.0f32, |m, z| m.max((z - zs[0]).abs()));
    assert!(drift < 0.02, "slid {drift} m: {zs:?}");
}

/// Fluid chains (user goal 2026-10-11: deflect / art / tool transitions): an action held down
/// from the start of `from` fires on the first tick its TAE execute flag (115 attack and art,
/// 117 guard, 137 tool) is up - nothing else in the routing holds it back. Returns (tick the
/// flag opened, tick the state changed, the new state).
pub(crate) fn chain_tick(from: &str, then: Action) -> (Option<usize>, Option<usize>, String) {
    let mut app = app();
    move_enemy_far(&mut app);
    let from_action = match from {
        "tool" => Some(Action::Prosthetic),
        "art" => Some(Action::CombatArt),
        "attack" => Some(Action::Attack),
        _ => None,
    };
    if let Some(act) = from_action {
        app.world_mut().resource_mut::<PadInput>().press(act);
        for _ in 0..10 {
            app.update();
            if !crate::player::is_free(&player_state(&mut app).0) {
                break;
            }
        }
        // A tap: the button is let go, so the hold check moves on to the release state.
        let tapped = player_state(&mut app).0;
        for _ in 0..30 {
            app.update();
            if player_state(&mut app).0 != tapped {
                break;
            }
        }
    } else {
        app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            q.single_mut(world).unwrap().play_state(&combat.player, from);
        });
    }
    let start = player_state(&mut app).0;
    let accept = match then {
        Action::Guard => 117,
        Action::Prosthetic => 137,
        _ => 115,
    };
    let (mut open, mut fired) = (None, None);
    for i in 0..120 {
        let (s, anim, t, _) = player_state(&mut app);
        if s != start {
            fired = Some(i - 1);
            return (open, fired, s);
        }
        if open.is_none() && app.world().resource::<Combat>().player.flag(&anim, t, accept) {
            open = Some(i);
        }
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(then);
        pad.guard_held = then == Action::Guard;
        app.update();
    }
    (open, fired, start)
}

/// Every chain fires the tick after its execute flag opens: the HKS reads the flags of the
/// previous frame's TAE update (docs/kb/input-buffer.md), and a press held from the start is
/// latched by 87 / the action's buffer flag before that. Guard out of a deflect recoil is not
/// here: frames 0-9 (ref 228) it plays the additive deflect over the recoil instead.
#[test]
pub(crate) fn deflects_arts_and_tools_chain_on_the_cancel_frame() {
    use Action::*;
    let cases = [
        ("StandDeflectHardSmall_F", Attack, "HardDeflectAtkSmallF"),
        ("StandDeflectHardSmall_F", CombatArt, "GroundSpecialAttackCombo1"),
        ("StandDeflectHardSmall_F", Prosthetic, "GroundSubAttackCombo2"),
        ("StandDeflectHardLarge_F", Attack, "GroundAttackCombo1Reverse"),
        ("StandDeflectHardLarge_F", CombatArt, "GroundSpecialAttackCombo1"),
        ("StandDeflectHardLarge_F", Prosthetic, "GroundSubAttackCombo2"),
        ("tool", CombatArt, "GroundSpecialAttackCombo1"),
        ("tool", Guard, "StandToDeflectGuard"),
        ("art", Guard, "StandToDeflectGuard"),
        ("art", Attack, "GroundAttackCombo1"),
        ("art", Prosthetic, "GroundSubAttackCombo1"),
        ("attack", Guard, "StandToDeflectGuard"),
        ("attack", CombatArt, "GroundSpecialAttackCombo1"),
        ("attack", Prosthetic, "GroundSubAttackCombo2"),
    ];
    for (from, then, want) in cases {
        let (open, fired, to) = chain_tick(from, then);
        let open = open.unwrap_or_else(|| panic!("{from} -> {then:?}: the execute flag never opened"));
        assert_eq!(to, want, "{from} -> {then:?}");
        assert!(fired.is_some_and(|f| f <= open + 1), "{from} -> {then:?}: flag open at tick {open}, fired {fired:?}");
    }
}

/// Grappling Hook (wire.rs): a ridden point 9 m ahead, 3 m up. G -> GroundWireShoot ->
/// GroundWireMoveStart_UpperSlope (hook 16 deg up) -> the TAE 920 launch arrives on the landing
/// spot when the camera slot turns 19 (0.67 s later), and Wolf lands there (LandWireMove) and
/// stays on it.
#[test]
pub(crate) fn a_grapple_lands_on_the_point_in_the_move_start_time() {
    let mut app = app();
    move_enemy_far(&mut app);
    let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 3.5, 9.0), land: Vec3::new(0.0, 3.0, 9.0), fwd: Vec3::NEG_Z, variation: 0 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: true, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut seen = Vec::new();
    let mut launch = None;
    let mut landed = None;
    for i in 0..240 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if seen.last() != Some(&s) {
            seen.push(s.clone());
        }
        let w = app.world_mut();
        let mut q = w.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(w).unwrap();
        if launch.is_none() && a.airborne && a.fall_type == 2 && s.starts_with("GroundWireMoveStart") && a.t > 0.66 {
            launch = Some(i);
        }
        if landed.is_none() && s == "LandWireMove" {
            landed = Some((i, t.translation));
        }
    }
    println!("states {seen:?} launch {launch:?} landed {landed:?}");
    assert!(seen.starts_with(&["GroundWireShoot".to_string(), "GroundWireMoveStart_UpperSlope".to_string()]), "{seen:?}");
    let (li, pos) = landed.expect("never landed");
    let flight = (li - launch.unwrap()) as f32 / 60.0;
    assert!((flight - 0.667).abs() < 0.05, "flight {flight}");
    assert!((pos - (point.land + Vec3::Y * CAPSULE_HALF_HEIGHT)).length() < 0.2, "landed at {pos}");
    // Still on the perch at the end.
    let (_, _, _, _) = player_state(&mut app);
    let w = app.world_mut();
    let mut q = w.query_filtered::<&Transform, With<Player>>();
    let y = q.single(w).unwrap().translation.y;
    assert!(y > 3.0, "fell off the perch: y {y}");
    // A jump in place on the perch lands back on it (round 53: the perch was dropped on leaving
    // the ground, and he landed through it on the ground below).
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut rose = false;
    for _ in 0..180 {
        app.update();
        let w = app.world_mut();
        let mut q = w.query_filtered::<&Actor, With<Player>>();
        rose |= q.single(w).unwrap().airborne;
    }
    let w = app.world_mut();
    let mut q = w.query_filtered::<(&Actor, &Transform), With<Player>>();
    let (a, t) = q.single(w).unwrap();
    assert!(rose, "no jump");
    assert!(!a.airborne && t.translation.y > 3.0, "jumped off the perch to y {}", t.translation.y);
}

/// The wire flight's fall type (Wire::launch fallType 2: Wire gravity and air control) ends with
/// the landing: walking off the perch afterwards is a plain fall (2026-10-12 bug hunt round 12).
#[test]
pub(crate) fn a_wire_landing_ends_the_wire_fall_type() {
    let mut app = app();
    move_enemy_far(&mut app);
    let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 3.5, 9.0), land: Vec3::new(0.0, 3.0, 9.0), fwd: Vec3::NEG_Z, variation: 0 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: true, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut wired = false;
    for _ in 0..240 {
        app.update();
        let w = app.world_mut();
        let a = w.query_filtered::<&Actor, With<Player>>().single(w).unwrap();
        wired |= a.airborne && a.fall_type == 2;
    }
    assert!(wired, "the grapple never flew on the wire fall type");
    let w = app.world_mut();
    let a = w.query_filtered::<&Actor, With<Player>>().single(w).unwrap();
    assert!(!a.airborne, "still in the air: {}", a.state);
    assert_eq!(a.fall_type, 0, "landed ({}) still on the wire fall type", a.state);
}

/// The Grappling Hook Attack (ThrowKind 180000 / 180100): hooked on the Chained Ogre, the attack
/// in the wire fall takes his ThrowParam 15020170 (Wolf a213_511600, the Ogre ThrowDef13600) and
/// the landing 15020171 (511610 / ThrowDef13610).
#[test]
pub(crate) fn the_grappling_hook_attack_throws_the_ogre() {
    let mut app = app_with("c5020");
    let ogre = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &mut Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (e, mut t) = q.single_mut(world).unwrap();
        t.translation = Vec3::new(0.0, 0.0, 9.0);
        e
    };
    let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 2.0, 9.0), land: Vec3::new(0.0, 0.0, 9.0), fwd: Vec3::NEG_Z, variation: 50200 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: true, ride: false };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: Some((ogre, 300_050_200)) });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut seen = Vec::new();
    let mut foe = Vec::new();
    for _ in 0..600 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if s.starts_with("WireMove") || s.starts_with("GroundWireMoveStart") {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        let (es, _, _) = enemy_state(&mut app);
        if foe.last() != Some(&es) {
            foe.push(es);
        }
    }
    let hp = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
        let a = q.single(world).unwrap();
        (a.hp, a.hp_max)
    };
    println!("wolf {seen:?}\nogre {foe:?} hp {hp:?}");
    // A deathblow (AtkParam_Pc 5000660): one of the Ogre's bars off, HP refilled by 13610's
    // 300610 "HP reset" (on the last bar it would be ThrowDefDeath13611).
    let bars = app.world_mut().query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(app.world()).unwrap().ninsatsu;
    println!("bars {bars:?}");
    assert_eq!(bars.0 + 1, bars.1, "{bars:?}");
    assert!(seen.iter().any(|s| s == "WireThrow"), "{seen:?}");
    assert!(seen.iter().any(|s| s == "WireThrowLand"), "{seen:?}");
    assert!(foe.iter().any(|s| s == "ThrowDef13600"), "{foe:?}");
    assert!(foe.iter().any(|s| s.starts_with("ThrowDef1361")), "{foe:?}");
}

/// Round 37: Gyoubu's broken deathblow on his last bar is the Todome, main + 8 (ThrowParam
/// 15080009: Wolf a225_510090, ThrowDef12090); with bars left the plain 15080001 (a225_510000).
/// His break pose ThrowDef12100 has no ThrowStart2 (67) in its TAE, so an attack in Wolf's
/// a225_510100 goes this way too, not to 15080011 / 0019 (gap: how the exe reaches 0019).
#[test]
pub(crate) fn gyoubus_last_bar_broken_deathblow_is_the_todome() {
    for (bars, want) in [(1, "a225_510090"), (2, "a225_510000")] {
        let mut app = app_with("c5080");
        app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            e.ninsatsu.0 = bars;
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            t.translation.z = 1.8;
        });
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        let mut anims = Vec::new();
        for _ in 0..60 {
            app.update();
            let (s, a, _, _) = player_state(&mut app);
            if anims.last() != Some(&a) {
                anims.push(a);
            }
            if s == "Deathblow" {
                break;
            }
        }
        assert_eq!(anims.last().map(String::as_str), Some(want), "bars {bars}: {anims:?}");
    }
}

/// Gyoubu on his last bar: the Grappling Hook Attack's landing deathblow kills (ThrowDef13610 ->
/// ThrowDefDeath13611 on flag 69) and an attack pressed in the landing takes the Todome
/// (ThrowParam 15080179: Wolf a225_511690, Gyoubu ThrowDef13690).
#[test]
pub(crate) fn the_grappling_hook_attack_todome_on_gyoubus_last_bar() {
    let mut app = app_with("c5080");
    let gyoubu = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &mut Transform, &mut Enemy), Without<crate::enemy::Cast>>();
        let (e, mut t, mut en) = q.single_mut(world).unwrap();
        t.translation = Vec3::new(0.0, 0.0, 9.0);
        en.ninsatsu.0 = 1;
        e
    };
    let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 2.0, 9.0), land: Vec3::new(0.0, 0.0, 9.0), fwd: Vec3::NEG_Z, variation: 80200 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: true, ride: false };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: Some((gyoubu, 300_080_200)) });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut seen = Vec::new();
    let mut foe = Vec::new();
    for _ in 0..900 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if s.starts_with("WireMove") || s.starts_with("GroundWireMoveStart") || s == "WireThrowLand" {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        let (es, _, _) = enemy_state(&mut app);
        if foe.last() != Some(&es) {
            let w = app.world_mut();
            let (en, ea) = w.query_filtered::<(&Enemy, &Actor), Without<crate::enemy::Cast>>().single(w).unwrap();
            println!("{es} bars {:?} hp {} dead {}", en.ninsatsu, ea.hp, en.is_dead());
            foe.push(es);
        }
    }
    println!("wolf {seen:?}\ngyoubu {foe:?}");
    assert!(seen.iter().any(|s| s == "WireThrowTodome"), "{seen:?}");
    assert!(foe.iter().any(|s| s == "ThrowDef13690"), "{foe:?}");
}

/// BEH_R_LAND_WIRE with the stick held: W_StandMoveStartFromFreeFall, straight into moving
/// instead of LandWireMove.
#[test]
pub(crate) fn a_wire_landing_with_the_stick_held_runs_on() {
    let mut app = app();
    move_enemy_far(&mut app);
    let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 0.5, 9.0), land: Vec3::new(0.0, 0.0, 9.0), fwd: Vec3::NEG_Z, variation: 0 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: 9.0, in_action_range: true, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut seen = Vec::new();
    for i in 0..240 {
        if i == 30 {
            app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
        }
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    println!("{seen:?}");
    assert!(!seen.iter().any(|s| s == "LandWireMove"), "{seen:?}");
    let i = seen.iter().position(|s| s.contains("WireMoveStart")).expect("no move start");
    assert_eq!(seen.get(i + 1).map(|s| s.as_str()), Some("Locomotion"), "{seen:?}");
}

/// BEH_R_LAND (HKS 2026-2056): a free fall lands into W_StandMoveStartFromFreeFallShortStiff,
/// straight into moving, with the stick held, and into the stiff LandFreeFall only without it
/// (round 31: walking off a ledge with the stick froze him).
#[test]
pub(crate) fn a_free_fall_with_the_stick_held_runs_on() {
    for (held, want) in [(true, "Locomotion"), (false, "LandFreeFall")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "FreeFall"));
                a.airborne = true;
                a.vel_y = -3.0;
                t.translation.y += 0.6;
            });
        }
        let mut landed = None;
        for _ in 0..60 {
            if held {
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
            }
            app.update();
            let (s, ..) = player_state(&mut app);
            if s != "FreeFall" {
                landed = Some(s);
                break;
            }
        }
        assert_eq!(landed.as_deref(), Some(want), "stick held {held}");
    }
}

/// From 20 m (FALL_HEIGHT_LONG_STIFF_LAND, BEH_R_LAND 1908) Wolf lands long-stiff
/// (a000_200021), stick held or not; a short fall is the plain land.
#[test]
pub(crate) fn a_twenty_metre_fall_lands_long_stiff() {
    for (drop, held, want) in [(21.0, true, "a000_200021"), (21.0, false, "a000_200021"), (2.0, false, "a000_200020")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "FreeFall"));
                a.airborne = true;
                a.vel_y = 0.0;
                t.translation.y += drop;
            });
        }
        let mut landed = None;
        for _ in 0..600 {
            if held {
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
            }
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            if a.state != "FreeFall" {
                landed = Some((a.state.clone(), a.anim.clone()));
                break;
            }
        }
        let (state, anim) = landed.expect("landed");
        assert_eq!(state, "LandFreeFall", "drop {drop} held {held}");
        assert_eq!(anim, want, "drop {drop} held {held}");
    }
}

/// A fall past HitMtrlParam fallDeathDist (40 m, the default row) ends in the fall-return death
/// (HKS DAMAGE_TYPE_LAND_DEAD_RETURN, a000_200909), then Wolf stands at the last safe ground a
/// third of his max HP down; a 25 m fall is only the long stiff landing. With a third or less
/// left, the fall kills him (`low`).
#[test]
pub(crate) fn a_forty_metre_fall_returns_wolf_to_safe_ground() {
    for (drop, dies, low) in [(45.0, true, false), (25.0, false, false), (45.0, true, true)] {
        let mut app = app();
        move_enemy_far(&mut app);
        for _ in 0..30 {
            app.update();
        }
        let (start, hp) = {
            let world = app.world_mut();
            let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
            (t.translation, a.hp)
        };
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "FreeFall"));
                a.airborne = true;
                a.vel_y = 0.0;
                if low {
                    a.hp = a.hp_max * 0.3;
                }
                t.translation += Vec3::new(3.0, drop, 0.0);
            });
        }
        let mut saw = false;
        let mut dead = false;
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..900 {
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            saw |= a.state == "LandDeadReturnStart";
            dead |= a.state.contains("Death");
            if seen.last() != Some(&a.state) { seen.push(a.state.clone()); }
        }
        let world = app.world_mut();
        let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        assert_eq!(saw, dies, "drop {drop} {seen:?}");
        let want = if !dies { hp } else if low { 0.0 } else { hp - a.hp_max / 3.0 };
        assert!((a.hp - want).abs() < 0.5, "drop {drop} low {low}: HP {} want {want}", a.hp);
        assert_eq!(dead, low, "drop {drop} low {low}: {seen:?}");
        assert!(!seen.iter().any(|s| s == "GroundFallReturn"), "back standing, no get-up clip: {seen:?}");
        if dies && !low {
            assert!((t.translation - start).length() < 0.5, "back at the safe ground: {:?} vs {:?}", t.translation, start);
        }
    }
}

/// Round 38: BEH_R_LAND 2026 - a jump's fall from 5 m or more lands stiff (_LandFreeFall,
/// a000_200020), not as the jump's own landing; a short one keeps LandVerticalGroundJump.
#[test]
pub(crate) fn a_jump_off_a_five_metre_ledge_lands_stiff() {
    for (drop, want) in [(6.0, "LandFreeFall"), (0.5, "LandVerticalGroundJump")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "VerticalGroundJumpFall"));
                a.airborne = true;
                a.vel_y = 0.0;
                t.translation.y += drop;
            });
        }
        let mut landed = None;
        for _ in 0..600 {
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            if !a.airborne {
                landed = Some(a.state.clone());
                break;
            }
        }
        assert_eq!(landed.as_deref(), Some(want), "drop {drop}");
    }
}

/// Probe (sekiro-rs 8eaf879's bug): strafing right locked on, the lock drops with the stick
/// still held: within 0.5 s Wolf must run where the stick points (camera-relative), not keep
/// the strafe.
#[test]
pub(crate) fn held_stick_realigns_after_the_lock_drops() {
    let mut app = app();
    let enemy = {
        let world = app.world_mut();
        let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
        t.translation.z = -40.0;
        e
    };
    app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
    let hold = |app: &mut App, n: usize| {
        for _ in 0..n {
            app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
            app.update();
        }
    };
    hold(&mut app, 60);
    app.world_mut().resource_mut::<crate::camera::LockOn>().target = None;
    hold(&mut app, 30);
    let world = app.world_mut();
    let cam_yaw = world.query::<&crate::camera::OrbitCamera>().iter(world).next().map_or(0.0, |c| c.yaw);
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    let want = crate::player::stick_world(Vec2::new(1.0, 0.0), cam_yaw);
    let got = a.move_vel.with_y(0.0).normalize_or_zero();
    assert!(got.dot(want) > 0.97, "moving {got:.2}, stick {want:.2}, state {} twist {:.1}", a.state, a.twist.to_degrees());
}

/// A bullet past its range (`dist`) loses damageDamp / staminaDamp / knockbackDamp percent per
/// second (c1060's rows: 100 each, so a second past range they deal nothing).
#[test]
pub(crate) fn a_bullet_past_its_range_weakens() {
    let mut app = app_cfg("c1060", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let combat = world.resource::<Combat>();
    let spec = combat.kind(0).data.bullet_rows.values().find(|s| s.damage_damp > 0.0 && s.attack.as_ref().is_some_and(|a| a.atk_phys > 0.0)).expect("a decaying bullet").clone();
    assert_eq!((spec.damage_damp, spec.stamina_damp, spec.knockback_damp), (100.0, 100.0, 100.0));
    let atk = spec.attack.unwrap();
    assert_eq!(atk.decayed([100.0; 3], 0.0).atk_phys, atk.atk_phys);
    assert!((atk.decayed([100.0; 3], 0.5).atk_phys - atk.atk_phys * 0.5).abs() < 1e-3);
    let gone = atk.decayed([100.0; 3], 1.5);
    assert_eq!((gone.atk_phys, gone.atk_stam), (0.0, 0.0));
}

/// Round 42: Isshin's finisher (ThrowParam 15400590, DiffAng 0-45, diffAngMyToDef 35) by the
/// exe's range check: Wolf behind him facing his way takes it; in front of him, facing him, not
/// (his kneel a100_020200 has no root yaw; the absorb dummy 230 is at his back). Before, only
/// the front within 45 degrees took it. (Since 2026-10-16 from his 21000, after the Todome.)
#[test]
pub(crate) fn isshins_finisher_is_taken_from_behind() {
    let mut app = app_cfg("c5400", Some(54000000), Vec::new());
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&Enemy, &mut Actor, &Transform), Without<crate::enemy::Cast>>();
        let (e, mut a, tf) = q.single_mut(world).unwrap();
        let at = tf.translation;
        a.anim_group = 1;
        // His first kneel (20200, window 105) takes the Todome 15400680 / 681 from anywhere near
        // (rec_20261016_080607: 680 / 681 at 912.2 s, from Wolf's walk-up).
        a.play("Ez20200", "a100_020200");
        a.t = 2.0;
        a.yaw = 0.0;
        let fwd = a.forward();
        let front = at + fwd * 1.5;
        let c = crate::player::deathblow_check(&combat, front, -fwd, false, &a, e, at, &|_| None);
        assert!(c.as_ref().is_some_and(|c| c.in_reach && c.suffix == 681), "Todome: {:?}", c.map(|c| c.suffix));
        // The finisher, from his second (21000, window 104; 11125875 after the Todome's message 20).
        a.play("Event21000", "a100_021000");
        a.t = 2.0;
        assert!(combat.data_of(&a).finisher_window(&a.anim, a.t) == Some(104), "no finisher window on a100_021000 at 2 s");
        let behind = at - fwd * 1.5;
        let c = crate::player::deathblow_check(&combat, behind, fwd, false, &a, e, at, &|_| None);
        assert!(c.as_ref().is_some_and(|c| c.in_reach && c.suffix == 590), "behind: {:?}", c.map(|c| c.suffix));
        assert!(crate::player::deathblow_check(&combat, front, -fwd, false, &a, e, at, &|_| None).is_none(), "taken from the front");
    });
}

/// Isshin, the Sword Saint (c5400): a deflect that empties his posture plays the break pose
/// (ThrowParam 15400010: Wolf a242_510100 / ThrowDef12100) and attack takes it to the deathblow,
/// also as Isshin after the first deathblow and his a000_003015 change (the user's report:
/// "the close look for the second but M1 does nothing").
#[test]
pub(crate) fn isshins_break_pose_leads_to_the_deathblow_after_his_change() {
    let mut app = app_cfg("c5400", Some(54000000), Vec::new());
    let seen = break_and_deathblow(&mut app, 400);
    assert!(seen.iter().any(|s| s == "ThrowDef12000"), "{seen:?}");
    let world = app.world_mut();
    world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    let mut fails = Vec::new();
    let mut tries = 0;
    let mut f = 0;
    while f < 1500 && tries < 12 {
        app.update();
        f += 1;
        let (es, _, _) = enemy_state(&mut app);
        if f % 120 != 0 || es.starts_with("Throw") {
            continue;
        }
        tries += 1;
        // combat.rs's breaking deflect, as it plays it.
        let mode = app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let th = combat.throw(combat.foe0().throw_row(10)).unwrap();
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            let mode = format!("{} {} {}", e.mode_name(), a.state, a.anim);
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.2);
            a.yaw = 0.0;
            a.posture = a.posture_max;
            e.react(&mut a, &combat, &format!("ThrowDef{}", th.def_anim));
            e.on_posture_break(&mut a, &combat, false, 4.0);
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut pa, mut pt) = q.single_mut(world).unwrap();
            pt.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
            pa.yaw = std::f32::consts::PI;
            pa.hp = pa.hp_max;
            pa.play("ThrowBreak", &th.atk_anim);
            mode
        });
        let mut ok = false;
        for i in 0..200 {
            if i == 36 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            f += 1;
            let (ps, _, _, _) = player_state(&mut app);
            if ps == "Deathblow" || ps.starts_with("ThrowBreakAttack") || enemy_state(&mut app).0.starts_with("ThrowDef1211") {
                ok = true;
                break;
            }
        }
        if !ok {
            fails.push(format!("{mode} -> enemy {:?} wolf {:?}", enemy_state(&mut app), player_state(&mut app)));
        }
        // Back to the fight.
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut pa = q.single_mut(world).unwrap();
        pa.hp = pa.hp_max;
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().ninsatsu.0 = 2;
    }
    assert!(fails.is_empty(), "{tries} tries: {fails:#?}");
}

/// The armoured club servant (c1180 NpcParam 11807101; common_func 20005410 on m10 1000363):
/// his armour part takes Wolf's hits at x0.35 HP (400250 slash cut 0.5 x body compensation 0.7)
/// and x0.5 posture, as live (348 -> 121, 130 -> 65); the Loaded Spear breaks it (anim 20039,
/// 220061 / 220062 / 400250 -> 230400 / 230401). His plain twin 11807100 has none.
#[test]
pub(crate) fn the_armoured_servant_takes_a_third_and_the_loaded_spear_breaks_it() {
    assert!(crate::enemy::armor_of(11807100).is_none());
    let mut app = app_cfg("c1180", Some(11807101), Vec::new());
    app.update();
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        let armor = e.armor.clone().expect("armour");
        assert!(a.resident.contains(&400_250) && a.resident.contains(&220_062));
        let (hp, posture) = armor.rates(1, 1);
        assert!((hp - 0.35).abs() < 1e-4 && (posture - 0.5).abs() < 1e-4, "{hp} {posture}");
        assert_eq!((348.0 * hp) as i32, 121);
        armor.shatter(&mut e, &mut a, &combat);
        assert!(e.armor.is_none());
        assert!(!a.resident.contains(&400_250) && a.resident.contains(&230_400));
        assert_eq!(a.state, "Event20039");
    });
}

/// HP and posture a shuriken takes off the c1180 `row` turned away from Wolf (no guard).
pub(crate) fn shuriken_on_c1180(row: i64) -> (f32, f32) {
    shuriken_on("c1180", row)
}

/// HP and posture a shuriken takes off the `chr` `row` turned away from Wolf (no guard).
pub(crate) fn shuriken_on(chr: &str, row: i64) -> (f32, f32) {
    tool_on(chr, row, None)
}

/// As shuriken_on, with the tool level `tool` (EquipParamWeapon id) in slot 1.
pub(crate) fn tool_on(chr: &str, row: i64, tool: Option<i64>) -> (f32, f32) {
    let mut app = app_cfg(chr, Some(row), Vec::new());
    if let Some(t) = tool {
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![t];
    }
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform, &mut Enemy), Without<crate::enemy::Cast>>();
        let (mut a, mut t, mut e) = q.single_mut(world).unwrap();
        e.targeting.state = crate::stealth::BATTLE;
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 4.0);
    }
    app.update();
    let before = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, Without<crate::enemy::Cast>>().iter(world).find(|a| a.side == crate::actor::Side::Enemy).unwrap();
        (a.hp, a.posture)
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..60 {
        app.update();
        if log_text(&app).contains("hit: -") {
            break;
        }
    }
    assert!(log_text(&app).contains("hit: -"), "{}", log_text(&app));
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, Without<crate::enemy::Cast>>().iter(world).find(|a| a.side == crate::actor::Side::Enemy).unwrap();
    (before.0 - a.hp, a.posture - before.1)
}

/// Tool bullets scale all five attackBase* elements (2026-10-12 round 22): the Lazulite
/// Shuriken 70500 (physics 0, magic 5, dark 20; bullet 700500 phys / mag / dark corrections)
/// did 0 HP, as bullet damage read only physics / fire / thunder.
#[test]
pub(crate) fn the_lazulite_shuriken_deals_its_magic_and_dark_damage() {
    let (hp, _) = tool_on("c1180", 11807100, Some(70500));
    assert!(hp > 1.0, "Lazulite Shuriken did {hp} HP");
}

/// Round 26: the Kodachi LV3-B poison mist (75300; Bullet 750300-750302 with spEffectId0 9009,
/// AtkParam 7503150 all 0) builds poison on an enemy in it; it was dropped as "deals nothing".
#[test]
pub(crate) fn the_poison_mist_poisons() {
    let mut app = app_cfg("c1020", None, Vec::new());
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![75300];
    let foe = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &mut Actor, &mut Transform, &mut Enemy), Without<crate::enemy::Cast>>();
        let (ent, mut a, mut t, mut e) = q.single_mut(world).unwrap();
        e.targeting.state = crate::stealth::BATTLE;
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 2.0);
        ent
    };
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut poison = 0.0;
    for _ in 0..240 {
        app.update();
        poison = app.world().resource::<crate::status::Statuses>().0.get(&foe).map_or(0.0, |s| s.poison);
        if poison > 0.0 {
            break;
        }
    }
    assert!(poison > 0.0, "{}", log_text(&app));
}

/// Wolf's bullets land on the armour part too (2026-10-12 bug hunt): the shuriken (atkAttribute /
/// staminaPhysicsAttribute 9 Light Shoot) does x0.35 HP (400250 defLightShootDmgRate 0.5 x the
/// part's body compensation 0.7) and x0.5 posture (defLightShootStaminaDmgRate 0.5) on the
/// armoured servant against his plain twin 11807100; no bullet breaks the armour.
#[test]
pub(crate) fn a_shuriken_on_the_armoured_servant_is_cut_like_a_slash() {
    let (hp0, p0) = shuriken_on_c1180(11807100);
    let (hp1, p1) = shuriken_on_c1180(11807101);
    assert!(hp0 > 0.0 && p0 > 0.0, "{hp0} {p0}");
    assert!((hp1 / hp0 - 0.35).abs() < 0.03 && (p1 / p0 - 0.5).abs() < 0.05, "plain {hp0} / {p0}, armoured {hp1} / {p1}");
}

/// A fight reset (R, the menu, or the one after Wolf's death) ends his poison: the Statuses
/// entry of his entity, which lives on across resets, is cleared (2026-10-12 bug hunt).
#[test]
pub(crate) fn a_fight_reset_clears_wolfs_poison() {
    let mut app = app();
    let wolf = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Player>>().single(world).unwrap()
    };
    app.world_mut().resource_mut::<crate::status::Statuses>().0.insert(wolf, {
        let mut s = crate::status::Status::default();
        s.poison = 50.0;
        s
    });
    app.world_mut().resource_mut::<PadInput>().reset = true;
    for _ in 0..3 {
        app.update();
    }
    assert!(!app.world().resource::<crate::status::Statuses>().0.contains_key(&wolf), "Wolf kept his poison gauge through the reset");
}

/// R / Retry / an encounter swap drop Lightning Reversal's charge (Actor.electro): the reset puts
/// Wolf on the ground without the landing that takes it (2026-10-12 bug hunt round 13).
#[test]
pub(crate) fn a_fight_reset_drops_the_lightning_charge() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().electro = Some((9492, 30.0));
    }
    app.world_mut().resource_mut::<PadInput>().reset = true;
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(a.electro.is_none(), "Wolf kept the lightning charge through the reset: {:?}", a.electro);
}

/// The enemies see Wolf's revival chain: a000_110030's TAE 110031 (0.3 s) is replaced by 110032
/// (2.5 s), which common_table_ai_common.lua:178-200 answers. Only the TAE-window 110031 reached
/// their snapshot (2026-10-13 round 30).
#[test]
pub(crate) fn the_enemy_sees_wolfs_revival_chain() {
    let mut app = app_cfg("c1010", Some(10100000), Vec::new());
    app.update();
    let world = app.world_mut();
    world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    let mut looped = false;
    for _ in 0..400 {
        app.update();
        if player_actor(&mut app).0 == "GroundRevivalStayLoop_F" {
            looped = true;
            break;
        }
    }
    assert!(looped, "never reached the death loop: {}", player_actor(&mut app).0);
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = false;
    for _ in 0..60 * 3 {
        app.update();
        let world = app.world_mut();
        seen |= world.query::<&Enemy>().iter(world).any(|e| e.prev_sp.contains(&110032));
    }
    assert!(seen, "no enemy saw 110032 after the revival");
}

/// Death and resurrection refresh Wolf as the game's anims say: the death loop (a000_110010,
/// TAE 401 8002-8009) cures his ailments and empties the build-up gauges, the revival
/// (a000_110030, 8001 changeStaminaRate 100) restores his posture (2026-10-12 bug hunt round 15:
/// he came back poisoned with his posture as it was).
#[test]
pub(crate) fn death_and_resurrection_refresh_wolf() {
    let mut app = app();
    move_enemy_far(&mut app);
    let wolf = {
        let world = app.world_mut();
        let (e, mut a) = world.query_filtered::<(Entity, &mut Actor), With<Player>>().single_mut(world).unwrap();
        a.posture = a.posture_max * 0.6;
        a.hp = 0.0;
        e
    };
    app.world_mut().resource_mut::<crate::status::Statuses>().0.insert(wolf, {
        let mut s = crate::status::Status::default();
        s.poison = 50.0;
        s
    });
    let mut looped = false;
    for _ in 0..400 {
        app.update();
        if player_actor(&mut app).0 == "GroundRevivalStayLoop_F" {
            looped = true;
            break;
        }
    }
    assert!(looped, "never reached the death loop: {}", player_actor(&mut app).0);
    app.update();
    assert!(!app.world().resource::<crate::status::Statuses>().0.contains_key(&wolf), "the death loop kept Wolf's poison gauge");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut revived = false;
    for _ in 0..10 {
        app.update();
        revived |= player_actor(&mut app).0 == "GroundRevival_F";
    }
    assert!(revived, "no resurrection: {}", player_actor(&mut app).0);
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(a.posture < a.posture_max * 0.05, "the revival kept posture {} of {}", a.posture, a.posture_max);
}

/// Round 66: the resurrection choice opens 1.8 s after HP empties (PlayerIns +0x2134,
/// FUN_140a36a60), in the death start itself (HKS 274-275), not after it. Mashing Attack from the
/// death revives then, out of GroundDeathStart_F (the loop's f0 cure 8002-8009 is skipped, as in
/// the HKS).
#[test]
pub(crate) fn mashing_attack_through_death_revives_at_1_8_s() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    let mut from = None;
    for f in 0..400 {
        let before = player_actor(&mut app).0;
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        app.update();
        if player_actor(&mut app).0 == "GroundRevival_F" {
            from = Some((f, before));
            break;
        }
    }
    let (f, before) = from.expect("no resurrection");
    assert_eq!(before, "GroundDeathStart_F");
    assert!((105..=112).contains(&f), "revived at frame {f}");
}

/// Round 23 guard: a press waiting for the stay loop is taken only after the loop's f0 refresh
/// events (8002-8009 cure, the poison gauge emptied).
#[test]
pub(crate) fn a_revive_in_the_stay_loop_still_cures_the_poison() {
    let mut app = app();
    move_enemy_far(&mut app);
    let wolf = {
        let world = app.world_mut();
        let (e, mut a) = world.query_filtered::<(Entity, &mut Actor), With<Player>>().single_mut(world).unwrap();
        a.hp = 0.0;
        e
    };
    app.world_mut().resource_mut::<crate::status::Statuses>().0.insert(wolf, {
        let mut s = crate::status::Status::default();
        s.poison = 50.0;
        s
    });
    let mut in_loop = false;
    for _ in 0..400 {
        in_loop = player_actor(&mut app).0.starts_with("GroundRevivalStayLoop");
        if in_loop {
            break;
        }
        app.update();
    }
    assert!(in_loop, "no stay loop: {}", player_actor(&mut app).0);
    let mut revived = false;
    for _ in 0..60 {
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        app.update();
        if player_actor(&mut app).0 == "GroundRevival_F" {
            revived = true;
            break;
        }
    }
    assert!(revived, "no resurrection: {}", player_actor(&mut app).0);
    assert!(!app.world().resource::<crate::status::Statuses>().0.contains_key(&wolf), "revived with the poison gauge");
}

/// One swing hits every enemy it reaches: AttackBehavior windows are done per defender
/// (Actor.hits_done), so two soldiers side by side both take the slash (2026-10-12 bug hunt).
#[test]
pub(crate) fn one_slash_hits_two_enemies_side_by_side() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1010.json")).exists() {
        return;
    }
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: None, at: Some([0.5, 1.6]), msb: None };
    let mut app = app_cfg("c1010", None, vec![g]);
    {
        let world = app.world_mut();
        // Both just in front of Wolf, turned away so neither guards, and fighting him (so the
        // slash is no stealth deathblow).
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform, &mut Enemy), With<Enemy>>();
        for (i, (mut a, mut t, mut e)) in q.iter_mut(world).enumerate() {
            e.targeting.state = crate::stealth::BATTLE;
            a.yaw = std::f32::consts::PI;
            t.translation = Vec3::new(if i == 0 { -0.35 } else { 0.35 }, CAPSULE_HALF_HEIGHT, 1.5);
        }
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..60 {
        app.update();
    }
    let world = app.world_mut();
    let hurt: Vec<bool> = world.query_filtered::<&Actor, With<Enemy>>().iter(world).map(|a| a.hp < a.hp_max).collect();
    assert_eq!(hurt, [true, true], "{}", log_text(&app));
}

/// A doped regular enemy (area_doping 7013, x2 HP) comes back from death with its whole bar:
/// the respawn takes hp_max, which carries the doping, not the raw NpcParam hp.
#[test]
pub(crate) fn a_doped_enemy_respawns_with_its_full_bar() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().enemy.area_doping = 7013;
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    for _ in 0..3 {
        app.update();
    }
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
    assert!(a.hp > 0.0 && (a.hp - a.hp_max).abs() < 1e-3, "respawned at {} of {}", a.hp, a.hp_max);
    // SpEffect 7013 maxHpRate / maxStaminaRate 2 (SpEffectParam; the export lacked the rates, so
    // they read 1 and doping never scaled the bars: 2026-10-12 round 18).
    let (hp_max, posture_max) = (a.hp_max, a.posture_max);
    let combat = app.world().resource::<Combat>();
    let n = combat.param("NpcParam", combat.kind(0).foe.npc_row);
    let f = |k: &str| n[k].as_f64().unwrap() as f32;
    assert!((hp_max - f("hp") * 2.0).abs() < 1e-3, "hp_max {hp_max} vs NpcParam hp {}", f("hp"));
    assert!((posture_max - f("stamina") * 2.0).abs() < 1e-3, "posture_max {posture_max} vs stamina {}", f("stamina"));
}

/// A regular enemy that comes back from death gets a new AI: the old Brain (its SetNumber
/// values, timers, observes) goes with the old life (2026-10-12 bug hunt).
#[test]
pub(crate) fn a_respawned_enemy_starts_with_a_fresh_brain() {
    let mut app = app();
    {
        let world = app.world_mut();
        for mut e in world.query::<&mut Enemy>().iter_mut(world) {
            e.aggressive = true;
        }
    }
    for _ in 0..30 {
        app.update();
    }
    let enemy = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap()
    };
    let Some(mut brains) = app.world_mut().get_non_send_mut::<crate::enemy::Brains>() else { return };
    let Some(b) = brains.map.get_mut(&enemy) else {
        eprintln!("no brain built (no Lua AI here): skipped");
        return;
    };
    b.log.push("old life".into());
    drop(brains);
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    let brains = app.world().get_non_send::<crate::enemy::Brains>().unwrap();
    assert!(!brains.map.get(&enemy).is_some_and(|b| b.log.iter().any(|l| l == "old life")), "the respawned enemy kept its old Brain");
}

/// A guard tap let go before its window is dropped, not kept: the next attack, long after, is
/// a plain slash, not the combat art (it paired with the stale tap; 2026-10-12 bug hunt).
#[test]
pub(crate) fn a_dropped_guard_tap_does_not_turn_the_next_attack_into_the_art() {
    let mut app = app();
    app.update();
    force_player(&mut app, "GroundStep_N", 1.0);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    for i in 0..120 {
        if i == 4 {
            app.world_mut().resource_mut::<PadInput>().guard_held = false;
        }
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut states = Vec::new();
    for _ in 0..10 {
        app.update();
        let s = player_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s.starts_with("GroundAttackCombo1")), "{states:?}");
}

/// A fight reset in mid-jump puts Wolf on the ground at the start, not still rising with the
/// jump's speed (2026-10-12 bug hunt).
#[test]
pub(crate) fn a_reset_mid_jump_lands_wolf_at_the_start() {
    let mut app = app();
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..12 {
        app.update();
    }
    {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!(a.airborne, "the jump did not leave the ground: {}", a.state);
    }
    app.world_mut().resource_mut::<PadInput>().reset = true;
    for _ in 0..30 {
        app.update();
    }
    let world = app.world_mut();
    let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
    assert!(!a.airborne && a.vel_y == 0.0, "still airborne after the reset: {} vel_y {}", a.state, a.vel_y);
    assert!((t.translation.y - crate::map::player_start().y).abs() < 0.05, "rose to {}", t.translation.y);
}

/// A regular enemy's new life starts as it spawned: the death anim's TAE 233 hid its sword and
/// sheath (Mask0-9 = 0), and a broken armour (Loaded Spear) is whole again (2026-10-12 bug hunt).
#[test]
pub(crate) fn a_respawned_enemy_gets_its_weapons_and_armour_back() {
    // The mask on the first drawn frame of the new life (later its AI may pick a move whose own
    // TAE 233 hides a part).
    #[derive(Resource, Default)]
    struct FirstLife(bool, Option<usize>);
    fn watch(mut seen: ResMut<FirstLife>, q: Query<(&Enemy, &crate::model::DrawMask)>) {
        for (e, dm) in &q {
            if e.is_dead() {
                seen.0 = true;
            } else if seen.0 && seen.1.is_none() {
                seen.1 = Some(dm.hidden());
            }
        }
    }
    let mut app = app_geom("c1020", None);
    app.init_resource::<FirstLife>()
        .add_systems(FixedUpdate, crate::model::apply_draw_mask_events.after(crate::actor::ActorSet::Advance))
        .add_systems(Last, watch);
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    let hidden = app.world().resource::<FirstLife>().1;
    assert_eq!(hidden, Some(0), "the respawned General hides {hidden:?} groups (None = no respawn)");
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1180.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1180", Some(11801100), Vec::new());
    {
        let world = app.world_mut();
        let mut e = world.query::<&mut Enemy>().single_mut(world).unwrap();
        assert!(e.armor.is_some(), "the servant spawned without its armour");
        e.armor = None;
    }
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    let world = app.world_mut();
    assert!(world.query::<&Enemy>().single(world).unwrap().armor.is_some(), "the respawned servant has no armour");
}

/// TAE 233 is an edge kept until changed: a frame that runs two fixed ticks (under 60 fps) must
/// still apply one that starts in its first tick (2026-10-12 bug hunt round 11: it was read in
/// Update from the last tick's (prev_t, t] only, so a death anim could keep its weapons drawn).
#[test]
pub(crate) fn a_draw_mask_change_is_kept_when_a_frame_runs_two_ticks() {
    let mut app = app_geom("c1020", None);
    app.add_systems(FixedUpdate, crate::model::apply_draw_mask_events.after(crate::actor::ActorSet::Advance));
    let world = app.world_mut();
    let e = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
    // Off the AI so nothing else plays over the clip.
    world.entity_mut(e).remove::<Enemy>();
    // (The new frame length takes effect a frame later.)
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(2.0 / 60.0)));
    app.update();
    // c1020 a000_001040: TAE 233 at 0 s hides Mask6.
    app.world_mut().get_mut::<Actor>(e).unwrap().play("Test", "a000_001040");
    app.update();
    let world = app.world();
    let a = world.get::<Actor>(e).unwrap();
    assert!(a.prev_t > 0.0, "the frame did not run two ticks (prev_t {}, t {})", a.prev_t, a.t);
    let hidden = world.get::<crate::model::DrawMask>(e).unwrap().hidden();
    assert_eq!(hidden, 1, "the TAE 233 at 0 s was missed");
}

/// Where the sim's hit test sees Wolf's dummy polys during a swing (FixedUpdate, before Resolve)
/// and where they are drawn for the same tick (after transform propagation). Only dummies near
/// Wolf: one effect dummy of his (2483) swings metres out and would swamp the numbers.
#[derive(Resource, Default)]
pub(crate) struct DummyProbe {
    fixed: std::collections::HashMap<Entity, Vec3>,
    worst: f32,
}

pub(crate) fn probe_dummies_fixed(mut probe: ResMut<DummyProbe>, wolf: Query<(&Transform, &crate::model::Dummies), With<Player>>, g: Query<&GlobalTransform>) {
    probe.fixed = wolf
        .iter()
        .flat_map(|(tf, d)| d.0.values().map(move |&m| (tf.translation, m)))
        .filter_map(|(at, m)| Some((m, g.get(m).ok()?.translation())).filter(|(_, p)| p.distance(at) < 3.0))
        .collect();
}

pub(crate) fn probe_dummies_drawn(mut probe: ResMut<DummyProbe>, g: Query<&GlobalTransform>) {
    let probe = &mut *probe;
    for (m, p) in &probe.fixed {
        if let Ok(now) = g.get(*m) {
            probe.worst = probe.worst.max(now.translation().distance(*p));
        }
    }
}

/// Wolf's dummies after `ticks` updates: how far the hit test's view (FixedUpdate) ever was from
/// the drawn one of the same tick.
pub(crate) fn dummy_lag(app: &mut App, ticks: usize) -> f32 {
    app.init_resource::<DummyProbe>()
        .add_systems(FixedUpdate, probe_dummies_fixed.after(crate::anim::PoseSet).before(crate::actor::ActorSet::Resolve))
        .add_systems(Last, probe_dummies_drawn);
    for _ in 0..ticks {
        app.update();
    }
    let probe = app.world().resource::<DummyProbe>();
    assert!(!probe.fixed.is_empty(), "Wolf has no dummy polys");
    probe.worst
}

/// The hit test runs on this tick's pose: the dummy polys combat reads in FixedUpdate are where
/// the swing is at the sim's anim time, not where the last drawn frame left them (they lagged
/// one tick at 60 fps and more below it; anim.rs PoseSet).
#[test]
pub(crate) fn the_hit_test_sees_this_ticks_pose() {
    let mut app = app_geom("c1020", None);
    // (The line-up moves the actors after the first tick: settle first.)
    for _ in 0..3 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let worst = dummy_lag(&mut app, 40);
    assert!(worst < 1e-3, "the hit test saw Wolf's dummies {worst} m from this tick's pose");
}

/// TAE 715 OverrideWeaponModelLocation in the sim pose: during Mortal Draw (a106_316000, Model2
/// on body dummy 20 for frames 6-107, else its rest 149) the Model2 dummies the arcs and hit
/// capsules use (12200-12280) are at the hand in FixedUpdate too, not back at the hip.
#[test]
pub(crate) fn the_hit_test_sees_weapon_parts_where_tae_715_puts_them() {
    let mut app = app_geom("c1020", None);
    for _ in 0..3 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.play("GroundSpecialAttackCombo1", "a106_316000");
        a.t = 0.5;
        a.prev_t = 0.5;
    }
    let worst = dummy_lag(&mut app, 20);
    let world = app.world_mut();
    let has_model2 = world.query_filtered::<&crate::model::Dummies, With<Player>>().single(world).unwrap().0.contains_key(&12200);
    assert!(has_model2, "no Model2 dummies");
    assert!(worst < 1e-3, "the hit test saw Wolf's dummies {worst} m from the drawn ones");
}

/// Mortal Draw's slash (AtkParam hits 12220-12250 / 12280 / 12200: Model2 WP_A_0300_2's own
/// dummies, 2-6 m along the long blade) is a real capsule: it reaches an enemy 4 m off, past the
/// 2.5-2.9 m no-model reach approximation it fell back to while the full ids were not looked up.
#[test]
pub(crate) fn mortal_draw_reaches_an_enemy_four_metres_away() {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.combat_art = 5700;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = 10;
    }
    for _ in 0..10 {
        app.update();
    }
    let hp0 = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Transform, &mut Enemy, &Actor), Without<crate::enemy::Cast>>();
        let (mut t, mut e, a) = q.single_mut(world).unwrap();
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 4.0);
        e.targeting.state = crate::stealth::BATTLE;
        a.hp
    };
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut anims = Vec::<String>::new();
    for _ in 0..240 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if anims.last() != Some(&a.anim) {
            anims.push(a.anim.clone());
        }
    }
    let world = app.world_mut();
    let hp = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().hp;
    assert!(anims.iter().any(|a| a.starts_with("a106_")), "no Mortal Draw: {anims:?}");
    assert!(hp < hp0, "the slash missed the enemy 4 m off ({hp0} -> {hp}); {anims:?}\n{}", log_text(&app));
}

/// A respawned enemy starts its new life clean: the Sabimaru poison (9004 -> 9045, 19.9 s) it
/// died with does not keep eating the full bar (2026-10-12 bug hunt round 6).
#[test]
pub(crate) fn a_respawned_enemy_is_not_still_poisoned() {
    let mut app = app();
    app.update();
    app.world_mut().resource_scope(|world, mut st: Mut<crate::status::Statuses>| {
        world.resource_scope(|world, combat: Mut<Combat>| {
            let (ent, a) = world.query_filtered::<(Entity, &Actor), (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
            for _ in 0..40 {
                st.hit(&combat, ent, &a, &[9004], false);
                if st.0.get(&ent).is_some_and(|s| !s.effects.is_empty()) {
                    break;
                }
            }
            assert!(st.0.get(&ent).is_some_and(|s| !s.effects.is_empty()), "no poison");
        });
    });
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    let world = app.world_mut();
    let (ent, a) = world.query_filtered::<(Entity, &Actor), (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    assert!(a.hp >= a.hp_max, "the new life lost {} HP", a.hp_max - a.hp);
    let st = world.resource::<crate::status::Statuses>();
    assert!(st.0.get(&ent).is_none_or(|s| s.effects.is_empty()), "still poisoned after the respawn");
}

/// An enemy wearing NpcParam's 90111 "Poison invalid" (zombies, geckos, monkeys) is never
/// poisoned, and 91011 "fire invalid" never burns (round 57: before, only its afflictions'
/// disable flags counted, and the Sabimaru filled a zombie's gauge).
#[test]
pub(crate) fn a_poison_invalid_resident_keeps_the_poison_off() {
    let mut app = app();
    app.update();
    let hits = |app: &mut App, resident: Vec<i64>| -> bool {
        let world = app.world_mut();
        let ent = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
        world.get_mut::<Actor>(ent).unwrap().resident = resident;
        world.resource_mut::<crate::status::Statuses>().clear(ent);
        world.resource_scope(|world, mut st: Mut<crate::status::Statuses>| {
            let combat = world.resource::<Combat>();
            let a = world.get::<Actor>(ent).unwrap();
            for _ in 0..200 {
                st.hit(combat, ent, a, &[9004], false);
            }
            st.0.get(&ent).is_some_and(|s| !s.effects.is_empty() || s.poison > 0.0)
        })
    };
    assert!(!hits(&mut app, vec![90111]), "poisoned through 90111 Poison invalid");
    assert!(!hits(&mut app, vec![90011]), "poisoned through 90011 Poison & flaming invalid");
    assert!(hits(&mut app, Vec::new()), "no poison without the resident");
}

/// A poisoned enemy despawned mid-fight (a phase swap, Genichiro -> Tomoe) takes its poison
/// effect along: the kept midst loop is stopped, not left at its last spot (round 51).
#[test]
pub(crate) fn a_despawned_enemys_poison_effect_stops() {
    let mut app = app();
    app.update();
    let ent = app.world_mut().query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(app.world()).unwrap();
    app.world_mut().resource_scope(|world, mut st: Mut<crate::status::Statuses>| {
        world.resource_scope(|world, combat: Mut<Combat>| {
            let a = world.get::<Actor>(ent).unwrap().clone();
            for _ in 0..40 {
                st.hit(&combat, ent, &a, &[9004], false);
                if st.0.get(&ent).is_some_and(|s| !s.effects.is_empty()) {
                    break;
                }
            }
        });
    });
    let on_body = |app: &mut App| -> Vec<(Entity, bool)> {
        let world = app.world_mut();
        world
            .query::<(Entity, &crate::fxr::FxEffect)>()
            .iter(world)
            // The held (kept) midst loops; the init kanji (4013) ends by its own age.
            .filter(|(_, f)| f.keep && matches!(f.anchor, Some(crate::fxr::Anchor::Entity(e)) if e == ent))
            .map(|(id, f)| (id, f.stop))
            .collect()
    };
    let mut fx = Vec::new();
    for _ in 0..10 {
        app.update();
        fx = on_body(&mut app);
        if !fx.is_empty() {
            break;
        }
    }
    assert!(fx.iter().any(|(_, stop)| !stop), "no poison effect on the body");
    app.world_mut().despawn(ent);
    app.update();
    app.update();
    let world = app.world_mut();
    for (id, _) in fx {
        assert!(world.get::<crate::fxr::FxEffect>(id).is_none_or(|f| f.stop), "the poison effect outlived its body");
    }
}

/// Round 52: an enemy gone during Wolf's deathblow start (a boss swap despawns it) leaves him
/// free: before, nothing left DeathblowStart and every input was ignored.
#[test]
pub(crate) fn a_deathblow_start_without_its_enemy_returns_to_idle() {
    let mut app = app();
    app.update();
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
            let (mut e, mut a, mut t) = q.get_mut(world, boss).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.8);
            a.yaw = 0.0;
        });
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut started = false;
    for _ in 0..30 {
        app.update();
        if player_state(&mut app).0 == "DeathblowStart" {
            started = true;
            break;
        }
    }
    assert!(started, "no deathblow start");
    let boss = boss_in_play(app.world_mut());
    app.world_mut().despawn(boss);
    let mut seen = Vec::<String>::new();
    for _ in 0..90 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert_ne!(seen.last().map(String::as_str), Some("DeathblowStart"), "{seen:?}");
}

/// Round 52: the fight reset clears what one life left for the next: an unspent death pick (a
/// grab death never takes it); the aged form's private flag is cleared beside it.
#[test]
pub(crate) fn the_reset_clears_the_last_lifes_death_pick() {
    let mut app = app();
    app.update();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.death_ref = crate::status::REF_ENABLE_INSANE;
        a.death_hit = Some((4, 0));
        // Round 58: the old fight's hit stop, additive deflect window and chained SpEffects.
        a.hit_stop.start(5.0);
        a.add_anim = "a000_008500".into();
        a.sp_chained.push((109451, 10.0));
    }
    app.world_mut().resource_mut::<PadInput>().reset = true;
    app.update();
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!((a.death_ref, a.death_hit), (0, None));
    assert!(!a.hit_stop.active(), "hit stop carried into the new fight");
    assert!(a.add_anim.is_empty(), "additive {}", a.add_anim);
    assert!(!a.sp_chained.iter().any(|c| c.0 == 109451), "{:?}", a.sp_chained);
}

/// Round 58: a hit stop in the air freezes the clip but not the fall - the exe runs the fall
/// module and Havok on the raw dt (1407e.c:2650), as player.rs's gravity already did.
#[test]
pub(crate) fn a_hit_stop_in_the_air_does_not_freeze_the_fall() {
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..60 {
        app.update();
        if app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().airborne {
            break;
        }
    }
    let y = |app: &mut App| app.world_mut().query_filtered::<&Transform, With<Player>>().single(app.world()).unwrap().translation.y;
    for _ in 0..5 {
        app.update();
    }
    let t0;
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        assert!(a.airborne, "not in the air");
        a.hit_stop.start(0.5);
        t0 = a.t;
    }
    let y0 = y(&mut app);
    app.update();
    app.update();
    let t1 = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().t;
    assert_eq!(t1, t0, "the clip ran in the hold");
    assert!((y(&mut app) - y0).abs() > 1e-3, "the body hung in the air");
}

/// Round 54 (g_behaviorTable, c0000_transition 6450): with the guard held (STYLE_TYPE_GROUND_GUARD)
/// the gourd is refused; a prosthetic switch is taken in the air (FREE_FALL).
#[test]
pub(crate) fn no_gourd_while_guarding_and_a_tool_switch_in_the_air() {
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..10 {
        app.update();
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    let mut guarding = false;
    for _ in 0..40 {
        app.update();
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        if player_state(&mut app).0 == "DeflectGuardIdle" {
            guarding = true;
            break;
        }
    }
    assert!(guarding, "no DeflectGuardIdle: {}", player_state(&mut app).0);
    let gourd0 = app.world_mut().query::<&Player>().single(app.world()).unwrap().gourd;
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    for _ in 0..30 {
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        app.update();
        let s = player_state(&mut app).0;
        assert!(!s.contains("Gourd") && !s.contains("Item"), "drank while guarding: {s}");
    }
    assert_eq!(app.world_mut().query::<&Player>().single(app.world()).unwrap().gourd, gourd0);
    // Let go, jump, switch at the top.
    app.world_mut().resource_mut::<PadInput>().guard_held = false;
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000, 71000];
    for _ in 0..90 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..60 {
        app.update();
        if app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().airborne {
            break;
        }
    }
    assert!(app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().airborne, "no jump");
    app.world_mut().resource_mut::<PadInput>().press(Action::SwitchTool);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&Player>().single(app.world()).unwrap().tool_slot, 1, "no switch in the air");
}

/// Live (rec_20261016_* boss fights, 19 head kicks off a locked forward jump over a perilous
/// sweep): Wolf holds at the contact distance in front of the enemy and the kick lands 18-20 ticks
/// after the press. Before, the jump kept its full speed against the body, so he slid round the
/// enemy's head, flew past and the kick missed (actor.rs separate).
#[test]
pub(crate) fn a_forward_jump_over_a_sweep_stops_at_the_body_and_the_kick_lands() {
    let mut app = app_with("c1020");
    {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &mut Enemy, &mut Actor)>();
        let (entity, mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, PERILOUS_SWEEP);
        world.resource_mut::<crate::camera::LockOn>().target = Some(entity);
    }
    let (mut pressed, mut launch, mut kicked) = (false, None, None);
    let mut far = 0.0f32;
    for i in 0..200 {
        let to = {
            let w = app.world_mut();
            let e = w.query_filtered::<&Transform, With<Enemy>>().single(w).unwrap().translation;
            let t = w.query_filtered::<&Transform, With<Player>>().single(w).unwrap().translation;
            (e - t).with_y(0.0).normalize_or_zero()
        };
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(to.x, -to.z);
        if !pressed && time_to_enemy_hit(&mut app).is_some_and(|dt| dt <= 6.0 / TAE_FPS) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
            pressed = true;
        }
        if launch.map(|l| i == l + 8).unwrap_or(false) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
        app.update();
        let w = app.world_mut();
        let e = w.query_filtered::<&Transform, With<Enemy>>().single(w).unwrap().translation;
        let (a, t) = w.query_filtered::<(&Actor, &Transform), With<Player>>().single(w).unwrap();
        if pressed && a.airborne && launch.is_none() {
            launch = Some(i);
        }
        if launch.is_some() && kicked.is_none() {
            far = far.max((t.translation - e).with_y(0.0).length());
        }
        if kicked.is_none() && a.state.starts_with("AirKickEnemyJump") {
            kicked = Some(i - launch.unwrap() - 8);
        }
    }
    let kicked = kicked.unwrap_or_else(|| panic!("no kick jump: {}", log_text(&app)));
    assert!((16..=22).contains(&kicked), "kick landed {kicked} ticks after the press");
    assert!(far < 1.2, "drifted {far:.2} m from the enemy before the kick");
}

/// Live (rec_2026101x_*): an idle Wolf with an enemy within 1.3 m moved in 32 of 1267 ticks; the
/// enemy stops at the body instead of shoving him. An enemy pushed into Wolf here (as its walk or
/// lunge would) must not slide him in his idle pose (actor.rs separate).
#[test]
pub(crate) fn an_enemy_walking_into_idle_wolf_does_not_slide_him() {
    let mut app = app();
    for _ in 0..10 {
        app.update();
    }
    let start = player_actor(&mut app).3;
    for _ in 0..60 {
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            let mut t = q.single_mut(world).unwrap();
            let to = (start - t.translation).with_y(0.0);
            t.translation += to.normalize_or_zero() * to.length().min(0.05);
        }
        app.update();
    }
    let (state, _, _, pos) = player_actor(&mut app);
    let moved = (pos - start).with_y(0.0).length();
    assert!(moved < 0.05, "Wolf ({state}) slid {moved:.2} m");
}

/// The user's report (2026-10-16): Isshin's last mark taken from the deflect break pose (15400510
/// / 511), then the close-up walk-up, where attack did nothing. Now: his kneel 20200 comes at
/// Wolf's deathblow's own message 10 (a243_510110 TAE 231 at 2.5 s, not at its Ninsatsu hit with
/// an older 10), the red mark shows on his kneel, and attack in the walk-up a000_710206 starts
/// the Todome 15400680 / 681.
#[test]
pub(crate) fn isshins_todome_is_taken_from_the_walk_up() {
    let mut app = app_cfg("c5400", Some(54000000), Vec::new());
    break_and_deathblow(&mut app, 400);
    {
        let w = app.world_mut();
        let mut a = w.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single_mut(w).unwrap();
        a.play("Ez3015", "a000_003015");
    }
    for _ in 0..400 {
        app.update();
    }
    for left in [2u32, 1] {
        app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            e.ninsatsu.0 = left;
            let th = combat.throw(crate::combat::throw_row_for(&combat, &a, 10).unwrap()).unwrap();
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.2);
            a.yaw = 0.0;
            a.posture = a.posture_max;
            e.react(&mut a, &combat, &format!("ThrowDef{}", th.def_anim));
            e.on_posture_break(&mut a, &combat, false, 4.0);
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut pa, mut pt) = q.single_mut(world).unwrap();
            pt.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
            pa.yaw = std::f32::consts::PI;
            pa.play("ThrowBreak", &th.atk_anim);
        });
        let mut kneel_at = None;
        let mut pressed = false;
        let mut todome = false;
        let mut marked = false;
        for i in 0..900 {
            if i == 36 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let (es, _, _) = enemy_state(&mut app);
            let (ps, pa, _, _) = player_state(&mut app);
            if kneel_at.is_none() && es == "Event20200" {
                kneel_at = Some(ps.clone());
                assert_eq!(left, 1, "a kneel with a mark left");
            }
            if !pressed && ps.starts_with("Event7102") && marked {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
                pressed = true;
            }
            // The red mark sits on him while his kneel's window is open (hud.rs).
            if es == "Event20200" && enemy_state(&mut app).2 > 1.2 && !marked {
                use bevy::ecs::system::RunSystemOnce;
                let target = app
                    .world_mut()
                    .run_system_once(|combat: Res<Combat>, wolf: Query<(&Actor, &GlobalTransform), With<Player>>, enemies: crate::hud::MarkEnemies, dummies: Query<&GlobalTransform>| {
                        let (wa, wg) = wolf.single().unwrap();
                        crate::hud::deathblow_mark_target(&combat, wa, wg.translation(), &enemies, &dummies, None)
                    })
                    .unwrap();
                marked = target.is_some();
            }
            todome |= pa.ends_with("511700") && es == "ThrowDef13700";
            if todome {
                break;
            }
        }
        if left == 1 {
            assert!(kneel_at.is_some(), "no kneel");
            assert!(todome, "no Todome from the walk-up");
            assert!(marked, "no red mark on his kneel");
        }
    }
}
