//! Sim tests, part 9: 47 tests, from `a_blocked_hit_does_not_wake_a_held_enemy` to `an_attack_out_of_a_run_keeps_the_runs_momentum`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// A map-held enemy wakes on Wolf's hit only when it takes HP (EMEVD 4[1] / 3[23] read a damage
/// record written for final HP damage > 0, FUN_1406d8950 from FUN_140b6e150 140b6.c:8033): one
/// already cut (a rival's blow) that blocks Wolf's swing, its HP whole, stays held; it woke
/// before (Wolf's guard reaction + HP below max).
#[test]
pub(crate) fn a_blocked_hit_does_not_wake_a_held_enemy() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let hold = spot.hold.clone().expect("1110236 has its map event's AI hold");
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: Some(10102010), at: None, msb: None };
    let mut app = app_cfg("c1010", Some(10102010), vec![g]);
    let held = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        t.translation = Vec3::new(40.0, CAPSULE_HALF_HEIGHT, 40.0);
        e.make_unaware(&mut a);
        let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
        t.translation = spot.at.with_y(CAPSULE_HALF_HEIGHT);
        a.yaw = spot.yaw;
        e.make_unaware(&mut a);
        // Cut earlier by a rival.
        a.hp -= 10.0;
        world.entity_mut(ents[1]).insert((spot.route.clone(), hold.clone()));
        ents[1]
    };
    for _ in 0..30 {
        app.update();
    }
    // Wolf's swing blocked: his guard reaction, no HP lost.
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.get_mut(world, held).unwrap();
        e.react_from(&mut a, &combat, true, "GuardDamageSmall_RighttoLeft");
    });
    for _ in 0..30 {
        app.update();
    }
    assert!(app.world().get::<crate::enemy::AiHold>(held).is_some(), "a blocked hit woke it");
    // Wolf's cut that takes HP wakes it.
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.get_mut(world, held).unwrap();
        a.hp -= 5.0;
        e.react_from(&mut a, &combat, true, "DamageS_Front");
    });
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get::<crate::enemy::AiHold>(held).is_none(), "Wolf's cut did not wake it");
}

/// A map-held enemy wakes on the SpEffects Wolf's bullets put on it (Enemy::add_timed, e.g. the
/// Finger Whistle's 230518 from bullets 790004-6, zero damage): its wake list has them, but the
/// hold only read the resident and TAE SpEffects (2026-10-12 round 19).
#[test]
pub(crate) fn the_finger_whistle_wakes_a_held_enemy() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let hold = spot.hold.clone().expect("1110236 has its map event's AI hold");
    assert!(hold.sp_effects.contains(&230518), "wake list {:?}", hold.sp_effects);
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: Some(10102010), at: None, msb: None };
    let mut app = app_cfg("c1010", Some(10102010), vec![g]);
    let held = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        t.translation = Vec3::new(40.0, CAPSULE_HALF_HEIGHT, 40.0);
        e.make_unaware(&mut a);
        let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
        t.translation = spot.at.with_y(CAPSULE_HALF_HEIGHT);
        a.yaw = spot.yaw;
        e.make_unaware(&mut a);
        world.entity_mut(ents[1]).insert((spot.route.clone(), hold.clone()));
        ents[1]
    };
    for _ in 0..30 {
        app.update();
    }
    assert!(app.world().get::<crate::enemy::AiHold>(held).is_some(), "woke on its own");
    app.world_mut().get_mut::<Enemy>(held).unwrap().add_timed(230518, 1.0);
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get::<crate::enemy::AiHold>(held).is_none(), "the whistle did not wake it");
}

/// ChrActionFlag 24 on Wolf (GroundWireShoot a000_202000 frames 0-19): an enemy hit takes HP and
/// posture but plays no reaction and no knockback, so the grapple goes on (exe b6bfe0; sekiro-rs
/// fight.rs:660-666). It used to cancel the grapple and push Wolf back (2026-10-12 round 16).
#[test]
pub(crate) fn a_hit_under_flag_24_keeps_wolfs_grapple() {
    let mut app = app();
    app.update();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let d = &combat.player;
            assert!(d.flag("a000_202000", 0.2, crate::combat::FLAG_NO_DAMAGE_MOTION), "the wire shot holds flag 24");
            let mut q = world.query::<(&mut Enemy, &mut Actor)>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003000");
        });
    }
    let before = log_text(&app).matches("hit:").count();
    let mut hit = false;
    let mut hp0 = 0.0;
    for _ in 0..300 {
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            if a.anim != "a000_202000" {
                a.play("GroundWireShoot", "a000_202000");
            }
            a.t = 0.2;
            a.prev_t = 0.2;
            hp0 = a.hp;
        }
        app.update();
        if log_text(&app).matches("hit:").count() > before {
            hit = true;
            break;
        }
    }
    assert!(hit, "no hit: {}", log_text(&app));
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!(a.anim, "a000_202000", "the hit cancelled the grapple ({})", a.state);
    assert!(a.hp < hp0, "HP still lands");
    assert_eq!(a.kb_vel, Vec3::ZERO, "no knockback under flag 24");
}

/// Wolf's cut that plays no reaction (mid-swing: the enemy_mid_swing setup, live) still marks
/// the map AI hold's damage wake: it is set where Wolf's HP damage lands, not only by a reaction
/// (react_from), so a held MSB enemy cut this way wakes (EMEVD 4[1] / 3[23]; 2026-10-12 round 16).
#[test]
pub(crate) fn a_cut_with_no_reaction_still_wakes_a_held_enemy() {
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
        a.t = 7.0 / TAE_FPS;
        a.prev_t = a.t;
        a.yaw = std::f32::consts::PI;
        e.hurt_by_wolf = false;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..40 {
        app.update();
        if log_text(&app).contains("you hit") {
            break;
        }
    }
    assert!(log_text(&app).contains("you hit"), "{}", log_text(&app));
    assert_eq!(enemy_state(&mut app).1, "a000_003000", "it reacted: not the no-reaction case");
    let world = app.world_mut();
    assert!(world.query::<&Enemy>().single(world).unwrap().hurt_by_wolf, "no damage wake");
}

/// A sandbox line-up of kinds that do not exist places nobody: it must not try again (and log)
/// every frame.
#[test]
pub(crate) fn a_lineup_that_places_nobody_is_not_retried() {
    let mut app = app();
    app.world_mut().insert_resource(crate::sandbox::Lineup(vec!["c9999".into()]));
    app.add_systems(Update, crate::sandbox::respawn_lineup);
    app.update();
    assert!(app.world().get_resource::<crate::sandbox::Lineup>().is_none(), "the empty line-up is retried every frame");
    assert_eq!(app.world_mut().query_filtered::<(), With<crate::sandbox::SandboxEnemy>>().iter(app.world()).count(), 0);
}

/// A tool hit on an enemy under ChrActionFlag 24 pushes it nowhere (as a sword hit), and any
/// tool damage wakes a map-held enemy (hurt_by_wolf), reaction or not (round 16, Bosees).
#[test]
pub(crate) fn a_tool_hit_under_flag_24_has_no_knockback_and_wakes_the_hold() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    for _ in 0..5 {
        app.update();
    }
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Actor, &mut Enemy, &Transform), With<Enemy>>();
        let (mut a, mut e, t) = q.single_mut(world).unwrap();
        let d = combat.data_of(&a);
        let mut keys: Vec<&String> = d.anims.keys().collect();
        keys.sort();
        let held = keys.iter().find_map(|k| (0..90).map(|i| i as f32 / 30.0).find(|&t| d.flag(k, t, crate::combat::FLAG_NO_DAMAGE_MOTION)).map(|t| ((*k).clone(), t)));
        let Some((anim, ft)) = held else {
            eprintln!("no flag-24 anim for this enemy: skipped");
            return;
        };
        let at = t.translation;
        let from = at + a.forward() * 3.0;
        e.force_anim(&mut a, &anim);
        a.t = ft;
        a.kb_vel = Vec3::ZERO;
        e.hurt_by_wolf = false;
        crate::prosthetic::tool_hit_push(&combat, &mut a, &mut e, from, at, 1.0, 2, 10.0);
        assert_eq!(a.kb_vel, Vec3::ZERO, "knockback under flag 24 ({anim} {ft:.2})");
        assert!(e.hurt_by_wolf, "tool damage did not wake the hold");
        e.hurt_by_wolf = false;
        crate::prosthetic::tool_hit_push(&combat, &mut a, &mut e, from, at, 1.0, 2, 0.0);
        assert!(!e.hurt_by_wolf, "no damage, no wake");
    });
}

pub(crate) fn wolf_skeleton(app: &mut App) -> (Entity, f32, Vec3, bool) {
    let world = app.world_mut();
    let (e, skel, lag) = world.query_filtered::<(Entity, &crate::anim::Skeleton, Option<&crate::anim::ModelLag>), With<Player>>().single(world).unwrap();
    let (root, blend) = (skel.root, skel.blend);
    let has_lag = lag.is_some();
    let r = world.get::<Transform>(root).unwrap().translation;
    (e, blend, r, has_lag)
}

/// Wolf's fight reset ends a running deathblow model slide and snaps his pose: the slide's
/// world offset used to go on past the reset, his model drawn off the start spot and sliding
/// back, and he faded in from the old pose (2026-10-12 bug hunt round 17).
#[test]
pub(crate) fn a_reset_snaps_wolfs_model_onto_the_start() {
    let mut app = app_geom("c1020", None);
    move_enemy_far(&mut app);
    let (wolf, _, _, _) = wolf_skeleton(&mut app);
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        world.get_mut::<Actor>(wolf).unwrap().play_state(&combat.player, "StandDamageMiddle_F");
    });
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().entity_mut(wolf).insert(crate::anim::ModelLag { offset: Vec3::new(1.0, 0.0, 0.0), time: 0.5, left: 0.5 });
    app.update();
    app.world_mut().resource_mut::<PadInput>().reset = true;
    for _ in 0..2 {
        app.update();
    }
    let (_, blend, root, lag) = wolf_skeleton(&mut app);
    assert!(!lag, "the model slide went on past the reset");
    assert!((root - Vec3::new(0.0, -crate::player::CAPSULE_HALF_HEIGHT, 0.0)).length() < 1e-4, "root off its rest after the reset: {root}");
    assert!(blend >= 1.0, "Wolf faded in from the old pose after the reset: blend {blend}");
}

/// The model slide's end puts the root back at rest even when frames are drawn between ticks:
/// the drawn root was read back as the sim pose and kept a part of the last step's offset
/// (2026-10-12 bug hunt round 17).
#[test]
pub(crate) fn the_model_slide_ends_at_rest_when_drawn_between_ticks() {
    let mut app = app_geom("c1020", None);
    move_enemy_far(&mut app);
    app.insert_resource(crate::interp::DrawAlpha(0.5));
    let (wolf, _, _, _) = wolf_skeleton(&mut app);
    app.world_mut().entity_mut(wolf).insert(crate::anim::ModelLag { offset: Vec3::new(1.0, 0.0, 0.0), time: 0.11, left: 0.11 });
    for _ in 0..30 {
        app.update();
    }
    let (_, _, root, lag) = wolf_skeleton(&mut app);
    assert!(!lag);
    assert!((root - Vec3::new(0.0, -crate::player::CAPSULE_HALF_HEIGHT, 0.0)).length() < 1e-4, "root kept an offset: {root}");
}

/// A second burn row (9117 then 9118, both spCategory 10004) replaces the first: one burn, the
/// new row, the old flames stopped - not two burns ticking with two flames (round 17).
#[test]
pub(crate) fn a_second_burn_row_replaces_the_first() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1700", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    world.resource_scope(|_, combat: Mut<Combat>| {
        let mut s = Statuses::default();
        let burning = |s: &mut Statuses, id: i64| (1..=10).find(|_| s.hit_wolf(&combat, wolf, weapon, &[], &[id], Guard::None).iter().any(|l| l == "BURNING"));
        assert!(burning(&mut s, 9117).is_some());
        let flames = Entity::PLACEHOLDER;
        s.0.get_mut(&wolf).unwrap().effects[0].fx_mut().push(flames);
        assert!(burning(&mut s, 9118).is_some());
        let ids: Vec<i64> = s.0[&wolf].effects.iter().map(|a| a.id).collect();
        assert_eq!(ids, vec![9118], "the burns stacked");
        assert_eq!(s.stopping(), &[flames], "the old flames were not stopped");
    });
}

/// A respawned enemy snaps to its new pose (anim::SnapPose): no fade-in from its death pose over
/// the idle's Blend length, no deathblow ModelLag slide (2026-10-12 round 17).
#[test]
pub(crate) fn a_respawn_snaps_to_its_new_pose() {
    let mut app = app_geom("c1010", None);
    for _ in 0..10 {
        app.update();
    }
    // (Returns on the respawn's tick.)
    kill_and_respawn(&mut app, |_, _| {});
    let world = app.world_mut();
    let (ent, sk) = world.query_filtered::<(Entity, &crate::anim::Skeleton), (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
    assert_eq!(sk.blend, 1.0, "fading in from the death pose (shown {})", sk.shown);
    assert!(world.get::<crate::anim::ModelLag>(ent).is_none(), "kept its deathblow slide");
}

/// The camera's snap and follow focus on Wolf from his feet, not his capsule's middle: a snap
/// at the follow's own target leaves nothing to sink (round 18).
#[test]
pub(crate) fn the_camera_snap_focus_is_measured_from_the_feet() {
    let at = Vec3::new(1.0, crate::player::CAPSULE_HALF_HEIGHT, 2.0); // standing on y = 0
    let f = crate::camera::chr_focus(at, 1.5);
    assert!((f - Vec3::new(1.0, 1.5, 2.0)).length() < 1e-6, "{f}");
}

/// The firecracker's burst SpEffects (230110 / 230111) land on the enemy it hits: they wake a
/// map-held enemy, and the burst does no HP damage (round 19).
#[test]
pub(crate) fn a_firecracker_burst_puts_its_sp_effect_on_the_enemy() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![71000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut landed = false;
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let e = world.query::<&Enemy>().single(world).unwrap();
        if e.clash.iter().any(|c| c.0 == 230110 || c.0 == 230111) {
            landed = true;
            break;
        }
    }
    assert!(landed, "the burst's SpEffect never reached the enemy");
}

/// The lock-on toggles on and off with the middle mouse button (and Q) (user, 2026-10-12: "i
/// can't remove the lock in").
#[test]
pub(crate) fn the_middle_mouse_button_releases_the_lock() {
    use bevy::input::mouse::AccumulatedMouseMotion;
    let mut app = app();
    app.init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AccumulatedMouseMotion>()
        .add_systems(Update, crate::camera::toggle_lock);
    if !app.world().contains_resource::<crate::camera::LockOn>() {
        app.world_mut().insert_resource(crate::camera::LockOn::default());
    }
    for _ in 0..5 {
        app.update();
    }
    let click = |app: &mut App| {
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Middle);
        app.update();
        let mut m = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        m.release(MouseButton::Middle);
        m.clear();
        for _ in 0..5 {
            app.update();
        }
        app.world().resource::<crate::camera::LockOn>().target
    };
    assert!(click(&mut app).is_some(), "the first click did not lock on");
    assert!(click(&mut app).is_none(), "the second click did not let go");
}

/// Locked on, a legs-only move (an upper action over the walk / run legs) with the stick let go
/// keeps going the way its legs clip goes, not toward the target: the facing was used, so a
/// back or strafe move slid Wolf onto the enemy (2026-10-12 bug hunt round 20).
#[test]
pub(crate) fn a_released_locked_on_back_move_does_not_slide_toward_the_target() {
    let mut app = app();
    if !app.world().contains_resource::<crate::camera::LockOn>() {
        app.world_mut().insert_resource(crate::camera::LockOn::default());
    }
    let enemy = {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
        world.resource_mut::<crate::camera::LockOn>().target = Some(e);
        e
    };
    let dist = |app: &mut App| {
        let world = app.world_mut();
        let w = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let e = world.get::<Transform>(enemy).unwrap().translation;
        (w - e).with_y(0.0).length()
    };
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Player, &mut Actor), With<Player>>();
        let (mut p, mut a) = q.single_mut(world).unwrap();
        a.play_state(&combat.player, "DeflectGuardToStandMoveRun");
        a.move_dir = 1;
        p.speed_level = 2.0;
    });
    let before = dist(&mut app);
    for _ in 0..20 {
        app.update();
    }
    let after = dist(&mut app);
    assert!(after > before - 0.05, "slid toward the target: {before} -> {after}");
}

/// Round 62: in the wire flight (STYLE_TYPE_WIRE_FALL) attack + guard is the wire-fall attack,
/// not an art (BEH_A_AIR_SP_ATTACK is off there), and a lightning charge throws it back first
/// (HKS 2872-2876).
#[test]
pub(crate) fn wire_flight_attacks_are_no_art_and_throw_lightning_back() {
    for charged in [false, true] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.unlock_all_moves = true;
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 7100;
        let point = crate::map::GrapplePoint { hook: Vec3::new(0.0, 6.5, 12.0), land: Vec3::new(0.0, 6.0, 12.0), fwd: Vec3::NEG_Z, variation: 0 };
        let target = crate::map::GrappleTarget { index: 0, point, dist: 12.0, in_action_range: true, ride: true };
        app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None });
        app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
        let mut flying = false;
        let mut path = Vec::<String>::new();
        for _ in 0..300 {
            app.update();
            let st = player_state(&mut app).0;
            if path.last() != Some(&st) {
                path.push(st.clone());
            }
            let air = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().airborne;
            if air && crate::wire::is_wire_state(&st) {
                flying = true;
                break;
            }
        }
        assert!(flying, "no WireMove: {path:?}");
        if charged {
            app.world_mut().query_filtered::<&mut Actor, With<Player>>().single_mut(app.world_mut()).unwrap().electro = Some((9495, 10.0));
        }
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
            pad.guard_held = true;
        }
        let mut seen = Vec::<String>::new();
        for _ in 0..150 {
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert!(!seen.iter().any(|s| s.contains("SpecialAttack")), "an art in the wire flight: {seen:?}");
        let want = if charged { "AirElectroReceiveAttack" } else { "WireFallAttackCombo1" };
        assert!(seen.iter().any(|s| s == want), "charged {charged}: no {want}: {seen:?}");
    }
}

/// Round 63 (g_behaviorValidateOrder, HKS 6449): pending guard and jump presses in an air attack
/// both open at its frame 12 - the kick is validated first, the air deflect last.
#[test]
pub(crate) fn a_pending_kick_beats_a_pending_air_guard() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.unlock_all_moves = true;
    for _ in 0..5 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..30 {
        app.update();
        if app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().airborne {
            break;
        }
    }
    for _ in 0..6 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..10 {
        app.update();
        if player_state(&mut app).0 == "AirComboAttack1" {
            break;
        }
    }
    assert_eq!(player_state(&mut app).0, "AirComboAttack1");
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
        pad.press(Action::Jump);
    }
    let mut seen = Vec::<String>::new();
    for _ in 0..30 {
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    let kick = seen.iter().position(|s| s == "AirKick");
    let guard = seen.iter().position(|s| s.starts_with("AirDeflectGuard"));
    assert!(kick.is_some() && guard.is_none_or(|g| g > kick.unwrap()), "{seen:?}");
}

/// Lands Wolf on a ridden grapple point from where he stands; returns the states and where he
/// stands after `ticks`.
pub(crate) fn grapple_to(app: &mut App, hook: Vec3, land: Vec3, ticks: usize) -> (Vec<String>, Vec3) {
    let point = crate::map::GrapplePoint { hook, land, fwd: Vec3::NEG_Z, variation: 0 };
    let target = crate::map::GrappleTarget { index: 0, point, dist: hook.length(), in_action_range: true, ride: true };
    app.insert_resource(crate::wire::WireAim { target: Some(target), enemy: None });
    app.world_mut().resource_mut::<PadInput>().press(Action::Grapple);
    let mut seen = Vec::new();
    for _ in 0..ticks {
        app.update();
        let s = player_state(app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    let w = app.world_mut();
    let pos = w.query_filtered::<&Transform, With<Player>>().single(w).unwrap().translation;
    (seen, pos)
}

/// A grapple from a perch flies: the walk-off fall check fired on the move start's launch tick
/// (SetNoGravity just ended, the perch cleared at the shot) and dropped him (2026-10-12 bug hunt
/// round 20).
#[test]
pub(crate) fn a_grapple_from_a_perch_flies_to_the_next_point() {
    let mut app = app();
    move_enemy_far(&mut app);
    let (_, first) = grapple_to(&mut app, Vec3::new(0.0, 3.5, 9.0), Vec3::new(0.0, 3.0, 9.0), 240);
    assert!(first.y > 3.0, "not on the first perch: {first}");
    let (seen, second) = grapple_to(&mut app, Vec3::new(0.0, 4.5, 18.0), Vec3::new(0.0, 4.0, 18.0), 240);
    assert!(!seen.iter().any(|s| s == "FreeFall"), "fell off the perch: {seen:?}");
    let want = Vec3::new(0.0, 4.0 + CAPSULE_HALF_HEIGHT, 18.0);
    assert!((second - want).length() < 0.3, "not on the second perch: {second} {seen:?}");
}

/// The arc to a high ridden point ends on its landing spot: it can arrive still rising, and the
/// flight's horizontal speed carried him past the 0.6 m perch (2026-10-12 bug hunt round 20).
#[test]
pub(crate) fn a_grapple_to_a_high_point_lands_on_it() {
    let mut app = app();
    move_enemy_far(&mut app);
    let (seen, pos) = grapple_to(&mut app, Vec3::new(0.0, 6.5, 8.0), Vec3::new(0.0, 6.0, 8.0), 240);
    let want = Vec3::new(0.0, 6.0 + CAPSULE_HALF_HEIGHT, 8.0);
    assert!((pos - want).length() < 0.3, "missed the high perch: {pos} {seen:?}");
}

/// isUseSharedHitList: a fan's pellets share one hit list, so one pellet hits Wolf and the rest fly
/// through him, as prosthetic.rs SharedHits does for his own (c1190 11902100: 5 pellets, 4 deg
/// apart, not penetrating). Every pellet landed its own hit and was used up on him (2026-10-12
/// round 19).
#[test]
pub(crate) fn a_fans_pellets_hit_wolf_once() {
    let mut app = app_cfg("c1190", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("11902100").expect("c1190 pellet row").clone();
    assert_ne!(spec.is_use_shared_hit_list, 0);
    assert_eq!((spec.num_shoot, spec.is_penetrate), (5, 0));
    let at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let hp0 = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at + Vec3::new(0.0, 0.3, 2.0), Vec3::NEG_Z, 0.0, None);
    world.flush();
    let mut left = None;
    for _ in 0..10 {
        app.update();
        let world = app.world_mut();
        let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
        if hp < hp0 {
            // The pellets (a hit one ends and its HitBulletID child comes).
            left = Some(world.query_filtered::<&Name, With<crate::enemy_bullet::EnemyBullet>>().iter(world).filter(|n| n.as_str() == "Enemy bullet 11902100").count());
            break;
        }
    }
    let left = left.expect("no pellet hit Wolf");
    assert_eq!(left, 4, "pellets still flying after the first hit");
}

/// Enemy bullets pass through Wolf's corpse: in the death loop the pellets were used up on him
/// and their build-up / timed SpEffects landed after the loop's cure (2026-10-13 round 23).
#[test]
pub(crate) fn enemy_bullets_pass_through_wolfs_corpse() {
    // The c1190 pellets flying each tick, at Wolf alive (the control: one hits him and ends) and
    // dead.
    let run = |dead: bool| -> Vec<usize> {
        let mut app = app_cfg("c1190", None, Vec::new());
        app.update();
        let world = app.world_mut();
        let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
        let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("11902100").expect("c1190 pellet row").clone();
        if dead {
            world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
            // Into the death loop, past its TAE 950 iframes (f0-4).
            let mut looping = false;
            let mut in_loop = 0;
            for _ in 0..60 * 15 {
                app.update();
                let world = app.world_mut();
                let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
                // (The loop clip is one frame long and its time wraps: count ticks in it.)
                if a.state == "GroundRevivalStayLoop_F" {
                    in_loop += 1;
                }
                if in_loop > 18 {
                    looping = true;
                    break;
                }
            }
            assert!(looping, "Wolf never reached the death loop");
        }
        let world = app.world_mut();
        // Where he lies now (the death clip moves him).
        let at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at + Vec3::new(0.0, 0.3, 2.0), Vec3::NEG_Z, 0.0, None);
        world.flush();
        (0..10)
            .map(|_| {
                app.update();
                let world = app.world_mut();
                world.query_filtered::<&Name, With<crate::enemy_bullet::EnemyBullet>>().iter(world).filter(|n| n.as_str() == "Enemy bullet 11902100").count()
            })
            .collect()
    };
    let alive = run(false);
    let dead = run(true);
    let hit_tick = alive.iter().position(|&n| n == 4).expect(&format!("no pellet hit the living Wolf: {alive:?}"));
    assert_eq!(dead[hit_tick], 5, "a pellet was used up on the corpse: alive {alive:?}, dead {dead:?}");
}

/// Spawns enemy bullet `row` of the first `kind` enemy at Wolf + `off`, flying along `dir`.
pub(crate) fn shoot_at_wolf(app: &mut App, row: &str, off: Vec3, dir: Vec3) {
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get(row).expect("bullet row").clone();
    let at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at + off, dir, 0.0, None);
    world.flush();
}

pub(crate) fn bullets_named(app: &mut App, row: &str) -> Vec<Vec3> {
    let world = app.world_mut();
    let name = format!("Enemy bullet {row}");
    world
        .query_filtered::<(&Name, &Transform), With<crate::enemy_bullet::EnemyBullet>>()
        .iter(world)
        .filter(|(n, _)| n.as_str() == name)
        .map(|(_, t)| t.translation)
        .collect()
}

/// A carrier bullet with no damage and no SpEffects of its own still touches Wolf: c1100's
/// poison spit 11000020 (opposeTarget 1, HitBulletID 11000021) ends on him and its poison cloud
/// comes there. It flew through him and the cloud came where it landed (2026-10-13 round 29).
#[test]
pub(crate) fn a_poison_spit_ends_on_wolf_and_its_cloud_comes_there() {
    let mut app = app_cfg("c1100", None, Vec::new());
    app.update();
    shoot_at_wolf(&mut app, "11000020", Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z);
    for _ in 0..20 {
        app.update();
    }
    let world = app.world_mut();
    let at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    assert!(bullets_named(&mut app, "11000020").is_empty(), "the spit flew on past Wolf");
    let clouds = bullets_named(&mut app, "11000021");
    assert!(
        clouds.iter().any(|c| (c.xz() - at.xz()).length() < 1.0),
        "no poison cloud at Wolf ({at}): {clouds:?}"
    );
}

/// Wolf's burn build-up plus poison build-up (each hit of a build-up bullet raises one).
pub(crate) fn wolf_build_up(app: &mut App) -> f32 {
    let world = app.world_mut();
    let we = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    world.resource::<crate::status::Statuses>().0.get(&we).map_or(0.0, |s| s.burn + s.poison)
}

/// R36 C: a bullet's hit takes its SpEffects from the Bullet row alone (exe FUN_1410b6880 fills
/// the slots from it; the AtkParam's are not read). The Demon of Hatred's fire 70200125 carries
/// burn 9116 on the Bullet and on its AtkParam 70200300: one build-up, not two.
#[test]
pub(crate) fn a_bullet_burns_once_not_through_its_atk_param_too() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7020.json")).exists() {
        return;
    }
    let once = {
        let mut app = app_cfg("c7020", None, Vec::new());
        app.update();
        let world = app.world_mut();
        let we = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let art = world.resource::<crate::config::GameConfig>().player.combat_art;
            world.resource_mut::<crate::status::Statuses>().hit_wolf(&combat, we, art, &[], &[9116], crate::status::Guard::None);
        });
        wolf_build_up(&mut app)
    };
    assert!(once > 0.0, "9116 builds burn on Wolf");
    let mut app = app_cfg("c7020", None, Vec::new());
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    {
        let combat = app.world().resource::<Combat>();
        let spec = combat.kind(0).data.bullet_rows.get("70200125").expect("bullet 70200125");
        assert!(spec.attack.as_ref().is_some_and(|a| a.sp_effects().contains(&9116)), "its AtkParam carries 9116 too");
    }
    shoot_at_wolf(&mut app, "70200125", Vec3::new(0.0, 0.0, 1.5), Vec3::NEG_Z);
    let mut got = 0.0;
    for _ in 0..30 {
        app.update();
        got = wolf_build_up(&mut app).max(got);
    }
    assert_eq!(got, once, "one burn build-up per bullet hit");
}

/// The Demon of Hatred's foot checks (resident 3702080 / 3702081, stateInfo 275: BehaviorParam
/// 1802 every 0.3 s -> Bullet 70200950, 2.5 m) mark Wolf next to him with 3702085 for 0.3 s; his
/// AI's stomp (Act19 -> 3046) waits for it (702000_battle.lua). Nothing fired it before.
#[test]
pub(crate) fn the_demons_feet_mark_wolf_beside_them() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7020.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7020", None, Vec::new());
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    let marked = |app: &mut App| {
        let w = app.world_mut();
        w.query::<&Player>().single(w).unwrap().timed.iter().any(|t| t.0 == 3_702_085)
    };
    // No model here: his feet dummies (13 / 16) stand in as markers, the bodies' push keeps
    // his root past the 2.5 m.
    let foot = |app: &mut App, d: f32| {
        let w = app.world_mut();
        let wolf = w.query_filtered::<&Transform, With<Player>>().single(w).unwrap().translation;
        let at = GlobalTransform::from_translation(wolf + Vec3::new(0.0, -0.8, -d));
        let marker = w.spawn(at).id();
        let e = w.query_filtered::<Entity, With<Enemy>>().single(w).unwrap();
        w.entity_mut(e).insert(crate::model::Dummies([(13, marker), (16, marker)].into_iter().collect()));
    };
    foot(&mut app, 1.0);
    let mut seen = 0;
    for _ in 0..60 {
        app.update();
        seen += marked(&mut app) as u32;
    }
    assert!(seen > 30, "marked {seen} of 60 frames beside the feet");
    foot(&mut app, 8.0);
    for _ in 0..30 {
        app.update();
    }
    assert!(!marked(&mut app), "still marked 8 m away");
}

/// A penetrating bullet's hit record runs out after dmgHitRecordLifeTime: c1030's fire 10300240
/// (life 5 s, record 0.5 s) burns Wolf standing in it again and again. It hit him once in its 5 s
/// (2026-10-13 round 29).
#[test]
pub(crate) fn a_fire_pool_burns_wolf_again_after_its_hit_record() {
    let mut app = app_cfg("c1030", None, Vec::new());
    app.update();
    shoot_at_wolf(&mut app, "10300240", Vec3::new(0.0, 0.3, 0.0), Vec3::NEG_Z);
    let mut rises = 0;
    let mut last = wolf_build_up(&mut app);
    for _ in 0..60 * 2 {
        app.update();
        let now = wolf_build_up(&mut app);
        if now > last + 1e-3 {
            rises += 1;
        }
        last = now;
    }
    assert!(rises >= 3, "the fire hit Wolf {rises} time(s) in 2 s (record 0.5 s)");
}

/// hitRadius grows to hitRadiusMax over spreadTime: c1250's 12500100 (0.2 -> 1.2 m over 0.8 s)
/// rising 0.9 m beside Wolf (it climbs ~1.7 m) catches him once it has spread. It stayed 0.2 m and missed
/// (2026-10-13 round 29).
#[test]
pub(crate) fn a_spreading_bullet_catches_wolf_beside_its_path() {
    let mut app = app_cfg("c1250", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let hp0 = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    shoot_at_wolf(&mut app, "12500100", Vec3::new(0.9, -0.6, 0.5), Vec3::NEG_Z);
    let mut hit = false;
    for _ in 0..50 {
        app.update();
        let world = app.world_mut();
        let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
        if hp < hp0 {
            hit = true;
            break;
        }
    }
    assert!(hit, "the spreading bullet missed Wolf 0.9 m beside its path");
}

/// Round 39: a blinding hit (AtkParam atkBehaviorId 17 at damage level none: the Yasha apes'
/// sand 12500100) plays W_StandDamageBlinding (a000_192600, BEH_R_DAMAGE 1155); it had no
/// reaction.
#[test]
pub(crate) fn the_apes_sand_blinds_wolf() {
    let mut app = app_cfg("c1250", None, Vec::new());
    app.update();
    shoot_at_wolf(&mut app, "12500100", Vec3::new(0.0, -0.6, 0.5), Vec3::NEG_Z);
    let mut seen = Vec::<String>::new();
    for _ in 0..50 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "StandDamageBlinding"), "{seen:?}
{}", log_text(&app));
}

/// Round 39: the Divine Dragon's atkBehaviorId bullets (BEH_R_SPECIAL_DAMAGE 1005-1012): the
/// lightning 52000820 (18 SXL_BLAST) blasts Wolf into AirDamageElectroReceiveExplosionStart, the
/// strong wind 52000830 (19 STORM_BACK_JUMP) into GroundStormJumpBackReady -> its Start.
#[test]
pub(crate) fn the_divine_dragons_blast_and_wind_throw_wolf() {
    for (row, wants) in [
        ("52000820", ["AirDamageElectroReceiveExplosionStart", "AirDamageElectroReceiveExplosionLand"]),
        ("52000830", ["GroundStormJumpBackReady", "GroundStormJumpBackStart"]),
    ] {
        let mut app = app_cfg("c5200", None, Vec::new());
        app.update();
        shoot_at_wolf(&mut app, row, Vec3::new(0.0, -0.6, 0.5), Vec3::NEG_Z);
        let mut seen = Vec::<String>::new();
        for _ in 0..60 * 6 {
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        for want in wants {
            assert!(seen.iter().any(|s| s == want), "{row}: no {want} in {seen:?}");
        }
    }
}

/// Round 40: BEH_R_DEATH 962 - killed by a fire hit (spAttribute 2, level LARGE: c1030's
/// 10300200, atkFire 320) Wolf dies in W_GroundDeathStartFire (a000_191200), face down.
#[test]
pub(crate) fn a_killing_fire_hit_is_the_fire_death() {
    let mut app = app_cfg("c1030", None, Vec::new());
    app.update();
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 1.0;
    }
    shoot_at_wolf(&mut app, "10300200", Vec3::new(0.0, -0.6, 0.5), Vec3::NEG_Z);
    let mut seen = Vec::<String>::new();
    for _ in 0..60 * 7 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "GroundDeathStartFire"), "{seen:?}
{}", log_text(&app));
    assert!(seen.iter().any(|s| s.ends_with("Loop_B")), "{seen:?}");
}

/// A timed SpEffect Wolf's tool puts on an enemy counts in its hit rates: the Firecracker LV3-B
/// burst's 9800 (2 s) makes it take x1.125 HP and posture. Enemy.clash was never read by the
/// rates (2026-10-13 round 30).
#[test]
pub(crate) fn a_firecracker_burst_makes_the_enemy_take_more() {
    let mut app = app();
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        let atk = crate::data::Attack { atk_attribute: 1, stamina_physics_attribute: 1, ..Default::default() };
        let (hp0, posture0, _) = crate::combat::armor_rates(&combat, &mut e, &mut a, &atk, None);
        e.add_timed(9800, 2.0);
        let (hp1, posture1, _) = crate::combat::armor_rates(&combat, &mut e, &mut a, &atk, None);
        assert!((hp1 / hp0 - 1.125).abs() < 1e-4, "HP rate {hp0} -> {hp1}");
        assert!((posture1 / posture0 - 1.125).abs() < 1e-4, "posture rate {posture0} -> {posture1}");
    });
}

/// An MSB enemy runs its own MSB ThinkParamID (Part.Enemy +0x8, patrols.json "think"), not the
/// roster's first placement of its NpcParam row: 1110252 (c1010, NpcParam 10102210) has
/// NpcThinkParam 10102200 (battle goal 101020). It ran the roster's pick (2026-10-12 round 20).
#[test]
pub(crate) fn an_msb_enemy_runs_its_own_think_row() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110252) else { return };
    assert_eq!(spot.think, Some(10102200));
    let mut app = app_cfg("c1010", Some(10102210), Vec::new());
    app.update();
    let world = app.world_mut();
    let combat = world.resource::<Combat>();
    let think = crate::enemy::msb_think(combat, &spot).expect("c1010 has NpcThinkParam 10102200");
    let foe = combat.kind(0).foe.clone();
    assert_eq!(foe.npc_row, 10102210);
    let mut e = world.query::<&mut Enemy>().single_mut(world).unwrap();
    e.think_override = Some(think);
    assert_eq!(e.think_id(&foe), 10102200);
}

/// A two-blade swing (c1700 a000_003076: judges 160 and 161 both at 1.033-1.1 s) deflected on
/// time is two deflects: the second window of the tick meets the guard the first one met, not
/// the StandDeflectHard the first just started (no deflect window: it became a block, extra
/// posture and StandDeflectEasy over the deflect) (2026-10-12 round 20).
#[test]
pub(crate) fn both_blades_of_a_two_blade_swing_are_deflected() {
    let out = defend_against_with("c1700", "a000_003076", 2.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("YOU DEFLECT") && !out.contains("blocked"), "{out}");
    assert!(out.contains("player=StandDeflectHard"), "{out}");
}

/// The beasts' firecracker burst does posture: 710013's AtkParam 7100104 directAtkStamDamage 30
/// times the c1150 dog's NpcParam attriAStaminaDmgRate 3.0 = 90 against its 15 posture, and the
/// break plays W_TrunkCollapseBurst (c9997 ExecDamageBreak) with the deathblow open (round 30;
/// it did no posture and TrunkCollapseBurst was unreachable).
#[test]
pub(crate) fn a_firecracker_breaks_a_dogs_posture() {
    let mut app = app_with("c1150");
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![71000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..90 {
        app.update();
        let (s, ..) = enemy_state(&mut app);
        if s.starts_with("TrunkCollapseBurst") {
            let world = app.world_mut();
            let e = world.query::<&Enemy>().single(world).unwrap();
            assert!(e.is_broken(), "the deathblow is open");
            return;
        }
    }
    panic!("no burst break: enemy {:?}, log {}", enemy_state(&mut app), log_text(&app));
}

/// The Sparking Axe (73200, v7302: spAttribute 2) swung through combat.rs's sword path on the
/// fire-reacting zombie (NpcParam 15000000: 6020) sets it flailing as the Flame Vent does: SP_DAMAGE_FIRE
/// -> W_DamageFire. The sword path played the level reaction (round 30).
#[test]
pub(crate) fn the_sparking_axe_sets_a_zombie_flailing() {
    let mut app = app_cfg("c1500", Some(15000000), Vec::new());
    for sp in app.world_mut().resource_mut::<Combat>().player.sp_effects.values_mut() {
        sp.regist_blood = 0.0;
    }
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![73200];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 1.5;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        (a.hp, a.hp_max, a.posture_max) = (5000.0, 5000.0, 5000.0);
        // Its back to Wolf: Idle still blocks a hit from the front.
        a.yaw += std::f32::consts::PI;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..120 {
        app.update();
        if enemy_state(&mut app).0 == "DamageFire" {
            return;
        }
    }
    panic!("no DamageFire: enemy {:?}, log {}", enemy_state(&mut app), log_text(&app));
}

/// R35: the Sabimaru on an Okami warrior (c1310: resident 9080, ref 1000220 WOMAN). The hit that
/// sets the poison off carries ref 1000032 WOMAN_POISON (the build-up 9003-9009), so c9997
/// GetSpDamage is SP_DAMAGE_POISON_REACTION -> W_SpecialPoisonReaction (a000_020160, rank 4).
/// Only BURNING fed a reaction before. Build-ups raised so the first hit poisons.
#[test]
pub(crate) fn the_sabimaru_poisons_an_okami_warrior_into_her_reaction() {
    let mut app = app_cfg("c1310", None, Vec::new());
    for sp in app.world_mut().resource_mut::<Combat>().player.sp_effects.values_mut() {
        if sp.behavior_ref_id == 1000032 {
            sp.poizon_attack_power = 10000.0;
        }
    }
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![75000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 1.8;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        (a.hp, a.hp_max, a.posture_max) = (5000.0, 5000.0, 5000.0);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..150 {
        app.update();
        if enemy_state(&mut app).0 == "SpecialPoisonReaction" {
            return;
        }
    }
    panic!("no SpecialPoisonReaction: enemy {:?}, log {}", enemy_state(&mut app), log_text(&app));
}

/// Exe SetHP holds HP at 1 only while IsNoDead (deathblows left: NpcParam ninsatuNum > 0, or a
/// stateInfo 143 SpEffect; FUN_140bd64e0 / FUN_140bd5df0), so a regular enemy (ninsatuNum 0) at
/// 0 HP dies outright through c9997 ExecDeath (W_DeathStartDefault: DeathDefault0/1). A
/// two-dot General (10200000, ninsatuNum 2) still opens the deathblow. Every enemy used to open
/// it (the 2026-10-06 rule).
#[test]
pub(crate) fn a_regular_enemy_dies_at_zero_hp_and_a_red_dot_one_opens_the_deathblow() {
    for (chr, row, dies) in [("c1100", None, true), ("c1020", Some(10200000), false)] {
        let mut app = app_cfg(chr, row, Vec::new());
        app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        for _ in 0..5 {
            app.update();
        }
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            a.hp = 0.0;
        }
        for _ in 0..3 {
            app.update();
        }
        let log = log_text(&app);
        let world = app.world_mut();
        let (e, a) = world.query::<(&Enemy, &Actor)>().single(world).unwrap();
        assert_eq!(e.is_dead(), dies, "{chr}: state {} log {log}", a.state);
        if dies {
            assert!(a.state.starts_with("DeathDefault"), "{chr}: {}", a.state);
        } else {
            assert!(e.is_broken(), "{chr}: the deathblow opens, state {}", a.state);
        }
    }
}

/// IsNoDead's TAE input: c1500's a000_020000 holds ChrActionFlag 96 "Set Immortality" (0-3.07 s),
/// so the mortal zombie (15000000, ninsatuNum 0) emptied there keeps 1 HP: no death, no
/// deathblow. Past the flag (idle) the same 0 HP kills it.
#[test]
pub(crate) fn a_zombie_rising_under_flag_96_keeps_1_hp() {
    for (anim, held) in [("a000_020000", true), ("a000_000000", false)] {
        let mut app = app_cfg("c1500", Some(15000000), Vec::new());
        app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        for _ in 0..5 {
            app.update();
        }
        let set = |app: &mut App, hp: bool| {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            if a.anim != anim {
                a.play("Probe", anim);
            }
            (a.prev_t, a.t) = (1.0, 1.0);
            if hp {
                a.hp = 0.0;
            }
        };
        set(&mut app, true);
        for _ in 0..3 {
            app.update();
            if held {
                set(&mut app, false);
            }
        }
        let log = log_text(&app);
        let world = app.world_mut();
        let (e, a) = world.query::<(&Enemy, &Actor)>().single(world).unwrap();
        if held {
            assert!(!e.is_dead() && !e.is_broken() && a.hp == 1.0, "{anim}: hp {} state {} log {log}", a.hp, a.state);
        } else {
            assert!(e.is_dead(), "{anim}: state {} log {log}", a.state);
        }
    }
}

/// R36: the attacks' AI sounds (AtkParam AppearAiSoundId / HitAiSoundId) were never emitted.
/// Wolf's slash (AtkParam_Pc 5000000: 2010 on the swing, 2110 on the hit, opposeTarget only)
/// goes out on his swing; an NPC's hit sound 2000020 (20 m, friendlyTarget, bCallFriend) is
/// heard by an ally within 20 m (it comes to the fight) and not beyond; Wolf's 2110 (4 m, oppose)
/// reaches an enemy at 3 m, and no ally hears an opposeTarget-only sound. A bCallFriend sound is
/// heard only while its owner's AI is in Battle / Find (FUN_140610470).
#[test]
pub(crate) fn attacks_make_ai_sounds_that_call_allies() {
    use crate::enemy::{AiSound, AiSounds, ai_sound_heard};
    assert_eq!(crate::data::atk_ai_sounds(true, 5000000), (2010, 2110));
    assert_eq!(crate::data::atk_ai_sounds(false, 10201010), (2000025, 2000020));
    let mut app = app();
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut swung, mut hit) = (false, false);
    for _ in 0..60 {
        app.update();
        swung |= app.world().resource::<AiSounds>().0.iter().any(|s| s.id == 2010 && s.owner.is_none());
        hit |= app.world().resource::<AiSounds>().0.iter().any(|s| s.id == 2110 && s.owner.is_none());
    }
    assert!(hit, "no hit sound 2110 from Wolf's slash landing (guarded or not)");
    assert!(swung, "no swing sound 2010 from Wolf: {:?}", app.world().resource::<AiSounds>().0);
    let other = app.world_mut().spawn_empty().id();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(Entity, &Actor, &Transform), With<Enemy>>();
        let (me, a, tf) = q.single(world).unwrap();
        let team = crate::combat::team_of(&combat, a);
        let think = combat.param("NpcThinkParam", combat.foe_of(a).think_id);
        let at = tf.translation;
        let npc = |id: i64, d: f32| AiSound { id, pos: at + Vec3::X * d, owner: Some(other), team: Some(team), life: 1.0, serial: 0, owner_alert: true, owner_groups: Vec::new(), friends_only: false };
        let wolf = |id: i64, d: f32| AiSound { id, pos: at + Vec3::X * d, owner: None, team: None, life: 1.0, serial: 0, owner_alert: true, owner_groups: Vec::new(), friends_only: false };
        let ai_sound_heard = |c: &Combat, s: &AiSound, me: Entity, team: i64, a: &Actor, at: Vec3, think: &serde_json::Value| ai_sound_heard(c, s, me, team, a, at, think, &[]);
        assert_eq!(ai_sound_heard(&combat, &npc(2000020, 15.0), me, team, a, at, think), Some((2000020, 1)), "an ally's hit at 15 m");
        assert_eq!(ai_sound_heard(&combat, &npc(2000020, 25.0), me, team, a, at, think), None, "beyond 20 m");
        let calm = AiSound { owner_alert: false, ..npc(2000020, 15.0) };
        assert_eq!(ai_sound_heard(&combat, &calm, me, team, a, at, think), None, "bCallFriend: an owner not in Battle / Find calls no one");
        assert_eq!(ai_sound_heard(&combat, &wolf(2110, 3.0), me, team, a, at, think), Some((2110, 1)), "Wolf's hit at 3 m");
        assert_eq!(ai_sound_heard(&combat, &npc(2110, 3.0), me, team, a, at, think), None, "opposeTarget only: not for an ally");
        let mine = AiSound { owner: Some(me), ..npc(2000020, 1.0) };
        assert_eq!(ai_sound_heard(&combat, &mine, me, team, a, at, think), None, "its own (selfTarget 0)");
        // bRestrictSamePlacementGroup (2000020): an owner in MSB groups reaches only a listener
        // that shares one (FUN_140610810); an owner in none reaches all.
        let grouped = AiSound { owner_groups: vec![2, 5], ..npc(2000020, 15.0) };
        let heard = |mine: &[i64]| crate::enemy::ai_sound_heard(&combat, &grouped, me, team, a, at, think, mine);
        assert_eq!(heard(&[5]), Some((2000020, 1)), "a shared group");
        assert_eq!(heard(&[3]), None, "another group");
        assert_eq!(heard(&[]), None, "a listener in no group");
        assert_eq!(crate::enemy::ai_sound_heard(&combat, &npc(2000020, 15.0), me, team, a, at, think, &[3]), Some((2000020, 1)), "an owner in no group");
        assert_eq!(crate::enemy::ai_sound_heard(&combat, &AiSound { owner_groups: vec![2], ..wolf(2110, 3.0) }, me, team, a, at, think, &[]), Some((2110, 1)), "a row without the restriction");
    });
}

/// The Flame Vent on a Villager zombie (c1500, NpcParam SpEffect 6020: ref 1000040
/// FIRE_ACTION_ENABLE): its DAMAGE_ELEMENT_FIRE hit is SP_DAMAGE_FIRE -> W_DamageFire (c9997
/// ExecSpReactionLarge), and the clip's own 3150601 (ref 1000240, 3 s) is kept past its TAE
/// window as the next fire reaction's cool time (round 30: it played the level reaction).
#[test]
pub(crate) fn the_flame_vent_sets_a_zombie_flailing() {
    // NpcParam 15000000 carries 6020 (the default 15000600 does not). The burn build-up is taken
    // off so the hit does not ignite: an igniting hit plays W_FireReaction first (ExecDebuffReaction).
    let mut app = app_cfg("c1500", Some(15000000), Vec::new());
    for sp in app.world_mut().resource_mut::<Combat>().player.sp_effects.values_mut() {
        sp.regist_blood = 0.0;
    }
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![72000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        (a.hp, a.hp_max) = (5000.0, 5000.0);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..120 {
        app.update();
        if enemy_state(&mut app).0 == "DamageFire" {
            return;
        }
    }
    panic!("no DamageFire: enemy {:?}, log {}", enemy_state(&mut app), log_text(&app));
}

/// A hit's reaction clip puts 109450 on Wolf for its TAE window (StandDamageSmall_F a000_100102:
/// 0-0.2 s, staminaRecoverSpeedRate 0), which then becomes its replaceSpEffectId 109451
/// "Stopping core recovery after being hit" (rate 0, 1 s): his posture holds until ~1.2 s and
/// then recovers (round 30; it came back at 0.2 s).
#[test]
pub(crate) fn a_hit_stops_wolfs_posture_regen_a_second_past_its_window() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.update();
    let posture = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().posture
    };
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            assert!(a.play_state(&combat.player, "StandDamageSmall_F"));
            a.posture = a.posture_max * 0.8;
            a.since_posture_damage = 10.0;
        });
    }
    let start = posture(&mut app);
    for _ in 0..60 {
        app.update(); // 1.0 s: the 0.2 s window, then 0.8 s of 109451
    }
    assert_eq!(posture(&mut app), start, "regen came back inside 109451's second");
    for _ in 0..60 {
        app.update();
    }
    assert!(posture(&mut app) < start, "no regen after 109451 ran out");
}

/// An enemy bullet carries its sfxId_Bullet on itself while it flies (as Wolf's prosthetic
/// bullets, prosthetic::spawn_row) and leaves its sfxId_Hit where it lands; the flying one stops
/// there. The oil pot's row with test effect ids.
#[test]
pub(crate) fn enemy_bullets_carry_their_effects() {
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let mut spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("15500700").expect("oil pot row").clone();
    spec.sfx_id_bullet = 600035;
    spec.sfx_id_hit = 600036;
    let at = Vec3::new(6.0, 1.5, 6.0);
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Y, 0.0, None);
    world.flush();
    let world = app.world_mut();
    let flying: Vec<_> = world.query::<&crate::fxr::FxEffect>().iter(world).filter(|f| f.id == 600035).map(|f| matches!(f.anchor, Some(crate::fxr::Anchor::Entity(_)))).collect();
    assert_eq!(flying, vec![true]);
    let mut hit = None;
    for _ in 0..(60 * 3) {
        app.update();
        let world = app.world_mut();
        if let Some(f) = world.query::<&crate::fxr::FxEffect>().iter(world).find(|f| f.id == 600036) {
            hit = Some(f.at.translation);
            break;
        }
    }
    let hit = hit.expect("no hit effect");
    assert!(hit.xz().distance(at.xz()) < 0.5 && hit.y < 0.1, "{hit}");
    let world = app.world_mut();
    assert!(world.query::<&crate::fxr::FxEffect>().iter(world).filter(|f| f.id == 600035).all(|f| f.stop && f.anchor.is_none()));
}

/// The Demon of Hatred's attack a000_003002 puts SpEffect 3702011 (burning body) on him from
/// 0.67 to 3.5 s: its SpEffectVfx 3702003 / 3702004 play 670215 / 670216 while it lasts
/// (ffx.rs tae_sp_vfx), stopped once his anim no longer carries it.

/// The resurrection's heartbeat (status.rs `heartbeat`): while Wolf holds 110091 (Player::held,
/// put on by a000_110020 / 21), its cycle row 110092's SpEffectVfx 5010 sound 200001
/// (c000200001) plays every 2.5 s (110091's motionInterval); it stops once the row is gone.
#[test]
pub(crate) fn the_resurrection_heartbeat_beats_every_two_and_a_half_seconds() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/json/params/SpEffectVfxParam.json")).exists() {
        return;
    }
    let mut app = app();
    app.init_resource::<crate::sound::SoundQueue>();
    app.update();
    let set = |app: &mut App, on: bool| {
        let world = app.world_mut();
        let mut q = world.query::<&mut crate::player::Player>();
        let mut p = q.single_mut(world).unwrap();
        p.held.retain(|id| *id != 110091);
        if on {
            p.held.push(110091);
        }
    };
    let beats = |app: &mut App| -> usize {
        let mut q = app.world_mut().resource_mut::<crate::sound::SoundQueue>();
        let n = q.0.iter().filter(|(k, _)| k == "c000200001").count();
        q.0.clear();
        n
    };
    beats(&mut app);
    // 6 s at 60 fps: beats at 0, 2.5 and 5 s.
    let mut n = 0;
    for _ in 0..360 {
        set(&mut app, true);
        app.update();
        n += beats(&mut app);
    }
    assert_eq!(n, 3, "heartbeats in 6 s");
    let mut after = 0;
    for _ in 0..360 {
        set(&mut app, false);
        app.update();
        after += beats(&mut app);
    }
    assert_eq!(after, 0, "the heart kept beating without 110091");
}

/// The Finger Whistle's madness shows on the beast (status.rs `lasting_glow`): while 230515 is on
/// its `sp_chained`, SpEffectVfx 7900's midst effect 300205 plays (230515 is in no character's
/// combat data: read from SpEffectParam), and it stops when the row goes.
#[test]
pub(crate) fn a_maddened_beast_shows_the_whistle_madness() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/fxr/300205.json")).exists() {
        return;
    }
    let mut app = app();
    app.update();
    let set = |app: &mut App, on: bool| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.iter_mut(world).next().unwrap();
        a.sp_chained.retain(|c| c.0 != 230515);
        // The beast's NpcParam resident 230500 (stateInfo 959) holds the madness's gate.
        if !a.resident.contains(&230500) {
            a.resident.push(230500);
        }
        if on {
            a.sp_chained.push((230515, f32::INFINITY));
        }
    };
    let glow = |app: &mut App| -> Vec<bool> {
        let world = app.world_mut();
        world.query::<&crate::fxr::FxEffect>().iter(world).filter(|f| f.id == 300205).map(|f| f.stop).collect()
    };
    // Stepped until it shows / stops (a frame may run no or several FixedUpdate steps).
    let mut on = Vec::new();
    for _ in 0..20 {
        set(&mut app, true);
        app.update();
        on = glow(&mut app);
        if !on.is_empty() {
            break;
        }
    }
    assert_eq!(on, vec![false], "no madness glow on the maddened beast");
    let mut off = false;
    for _ in 0..20 {
        set(&mut app, false);
        app.update();
        if glow(&mut app).iter().all(|s| *s) {
            off = true;
            break;
        }
    }
    assert!(off, "the madness glow outlived the madness");
}

/// Manual test #5: Wolf's lightning charge shows on his body (status.rs `electro_glow`): while
/// Actor::electro holds an enemy charge 9492 (no row on Wolf, so his 9490 / SpEffectVfx 6000 stands in) the midst effect 404041 plays, and it stops when the
/// charge is gone.
#[test]
pub(crate) fn wolfs_lightning_charge_glows_while_it_lasts() {
    let mut app = app();
    app.update();
    let set = |app: &mut App, v: Option<(i64, f32)>| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        q.single_mut(world).unwrap().electro = v;
    };
    let glow = |app: &mut App| -> Vec<bool> {
        let world = app.world_mut();
        world.query::<&crate::fxr::FxEffect>().iter(world).filter(|f| f.id == 404041).map(|f| f.stop).collect()
    };
    // Step until the glow shows / stops (a frame under load may run no or several FixedUpdate
    // steps; the charge is set again before each update so nothing in FixedUpdate takes it).
    let mut on = Vec::new();
    for _ in 0..20 {
        set(&mut app, Some((9492, 30.0)));
        app.update();
        on = glow(&mut app);
        if !on.is_empty() {
            break;
        }
    }
    assert_eq!(on, vec![false], "no glow while charged");
    let mut off = false;
    for _ in 0..20 {
        set(&mut app, None);
        app.update();
        if glow(&mut app).iter().all(|s| *s) {
            off = true;
            break;
        }
    }
    assert!(off, "the glow outlived the charge");
}

#[test]
pub(crate) fn the_demons_burning_body_shows_while_his_tae_says_so() {
    let mut app = app_cfg("c7020", None, Vec::new());
    app.add_systems(Update, crate::ffx::tae_sp_vfx);
    app.update();
    let set = |app: &mut App, anim: &str, t: f32| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
        let mut a = q.single_mut(world).unwrap();
        a.play("Attack3002", anim);
        a.t = t;
        a.prev_t = t;
    };
    let burning = |app: &mut App| -> Vec<(i64, bool)> {
        let world = app.world_mut();
        let mut v: Vec<(i64, bool)> = world.query::<&crate::fxr::FxEffect>().iter(world).filter(|f| matches!(f.id, 670215 | 670216)).map(|f| (f.id, f.stop)).collect();
        v.sort();
        v
    };
    set(&mut app, "a000_003002", 1.0);
    app.update();
    assert_eq!(burning(&mut app), vec![(670215, false), (670216, false)]);
    set(&mut app, "a000_003002", 3.9);
    app.update();
    assert!(burning(&mut app).iter().all(|f| f.1), "{:?}", burning(&mut app));
}

/// The Old Dragons of the Tree (m25 12505801, bar 905300 on the shared body 2500810, NpcParam
/// 53000000) come before the Divine Dragon: 12505949 (the shortcut 12505882 waits on, which
/// sets 9318 and skips to the Divine Dragon) starts off for this row (boss_scripts.py
/// FLAGS_OFF). So the tentacle event 12505901 never ends, and 530000_battle.lua's
/// IsEventFlag(12505901) "follow the team leader" mode (Act11, weight 600) stays off. (The
/// body's own attacks are no longer asserted: 12505801 now runs past its action button and turns
/// the bar body's AI off, as in the game; the 15 dragons fight.)
#[test]
pub(crate) fn the_old_dragons_fight_before_the_divine_dragon() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5300", Some(53000000), Vec::new());
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..(60 * 20) {
        app.update();
        seen.insert(enemy_state(&mut app).1);
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.hp = a.hp_max;
        a.posture = 0.0;
    }
    let flags = app.world().resource::<crate::enemy::BossScript>().run.as_ref().unwrap().flags_on();
    assert!(!flags.contains(&12505901) && !flags.contains(&9318), "{flags:?} {seen:?}");
}

/// The Old Dragons' fall (m25 12505885: the bar body 2500810's HP <= 0 -> 9318, AiState 2500800
/// on, SetSp 3520071, WarpCopy) hands over to the Divine Dragon: 9318 is their flag, not a
/// defeat. Before, a 93xx flag with the boss at "0 bars" counted as the defeat, and the body's
/// ninsatuNum is 0, so the fight ended (banner, restart) and the Divine Dragon never came.
#[test]
pub(crate) fn the_old_dragons_fall_hands_over_to_the_divine_dragon() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5300", Some(53000000), Vec::new());
    for _ in 0..120 {
        app.update();
    }
    for _ in 0..(60 * 8) {
        {
            let world = app.world_mut();
            let mut q = world.query::<(&crate::enemy::script::Scripted, &mut Actor)>();
            if let Some((_, mut a)) = q.iter_mut(world).find(|(s, _)| s.0 == 2500810) {
                a.hp = 0.0;
            }
            let mut w = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            w.hp = w.hp_max;
        }
        app.update();
    }
    let bs = app.world().resource::<crate::enemy::BossScript>();
    let run = bs.run.as_ref().expect("the script runs");
    assert!(run.flags_on().contains(&9318), "no 9318: {:?}", run.flags_on());
    assert!(run.defeated.is_none() && bs.defeats == 0, "the Old Dragons' fall ended the fight");
    let world = app.world_mut();
    let mut q = world.query::<(&crate::enemy::script::Scripted, &Enemy)>();
    let dragon = q.iter(world).find(|(s, _)| s.0 == 2500800).map(|(_, e)| (e.disabled, e.ai_off));
    assert_eq!(dragon, Some((false, false)), "the Divine Dragon (disabled, ai_off)");
}

/// Gyoubu's defeat (m11_00 11105800: has 201000 or dead -> IfCharacterDead -> Wait 0.7 -> Handle
/// Boss Defeat) waits for his reaction's ChrActionFlag 12 "Death" (a000_012090 f241 = 8.03 s), as
/// the all-bosses clip's banner 9.5 s after the lunge shows (script.rs snapshot `dead`). Before:
/// dead at the Ninsatsu hit (5.9 s), the banner ~3 s early; and before that on the throw's first
/// frame.
#[test]
pub(crate) fn gyoubus_defeat_waits_for_the_ninsatsu_hit() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5080.json")).exists() {
        return;
    }
    let mut app = app_with("c5080");
    for _ in 0..30 {
        app.update();
    }
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        world.get_mut::<Enemy>(boss).unwrap().ninsatsu.0 = 1;
    }
    break_and_deathblow_died(&mut app, 1);
    let (mut thrown, mut defeated) = (None, None);
    for f in 0..(60 * 20) {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        if thrown.is_none() && world.get::<Enemy>(boss).unwrap().is_dead() {
            thrown = Some(f);
        }
        if world.resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some()) {
            defeated = Some(f);
            break;
        }
        app.update();
    }
    let (Some(t0), Some(t1)) = (thrown, defeated) else { panic!("thrown {thrown:?} defeated {defeated:?}: {}", log_text(&app)) };
    let wait = (t1 - t0) as f32 / 60.0;
    eprintln!("Gyoubu: defeat {wait:.2} s after the throw's start");
    // The reaction's flag 12 at 8.03 s plus the event's 0.7 s (the clip: the banner at 9.5 s).
    assert!((8.5..=9.5).contains(&wait), "defeat {wait:.2} s after the throw's start");
}

/// An attack out of a run keeps the run's momentum through its blend-in: the behaviour graph's
/// TaeBlend transition mixes the from generator's world-from-model in by (1 - w), w the smoothstep
/// over the clip's TAE Blend (actor.rs advance). Live (rec_* recordings): GroundAttackCombo1 out
/// of the run loop a000_000500 moves 0.26-0.45 m in its first 0.1 s against the clip's own
/// 0.094 m. Before, the clip's root alone: Wolf stopped dead on the first attack frame.
#[test]
pub(crate) fn an_attack_out_of_a_run_keeps_the_runs_momentum() {
    fn player_pos(app: &mut App) -> Vec3 {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
    }
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..90 {
        app.update();
    }
    let run_step = {
        let p0 = player_pos(&mut app);
        for _ in 0..6 {
            app.update();
        }
        (player_pos(&mut app) - p0).length()
    };
    assert!(run_step > 0.3, "run step over 6 ticks {run_step}");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut started = None;
    for _ in 0..30 {
        app.update();
        if player_state(&mut app).0.starts_with("GroundAttackCombo1") {
            started = Some(player_pos(&mut app));
            break;
        }
    }
    let p0 = started.unwrap_or_else(|| panic!("no attack: {}", log_text(&app)));
    for _ in 0..6 {
        app.update();
    }
    let d6 = (player_pos(&mut app) - p0).length();
    assert!(d6 > 0.55 * run_step && d6 < run_step + 0.03, "attack's first 6 ticks {d6:.3} m, the run's {run_step:.3} m");
}
