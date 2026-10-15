//! Sim tests, part 4: 23 tests, from `crouching_cuts_how_far_enemies_see_wolf` to `a_blocked_shuriken_plays_the_guard_reaction`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

#[test]
pub(crate) fn crouching_cuts_how_far_enemies_see_wolf() {
    // C toggles crouch: CrouchStart (a000_216000), then the crouch idle a000_005000, whose TAE
    // carries SpEffect 109200 (sightSearchEnemyCut 20) -> enemies' sight distances x 0.8.
    let mut app = app();
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    app.update();
    assert_eq!(player_state(&mut app).0, "CrouchStart");
    for _ in 0..120 {
        app.update();
    }
    {
        let mut q = app.world_mut().query_filtered::<&Actor, With<Player>>();
        let world = app.world();
        let a = q.single(world).unwrap();
        let combat = world.resource::<crate::data::Combat>();
        assert!(a.crouch && a.state == "StandIdle", "{} {}", a.state, a.crouch);
        let (key, t) = a.shown_clip(&combat.player);
        assert_eq!(key, "a000_005000");
        let sp: Vec<_> = combat.player.sp_effects_at(&key, t).into_iter().map(|(_, e)| e).collect();
        let seen = crate::stealth::Seen::new(Vec3::ZERO, 0.4, &sp);
        assert!((seen.cut - 0.8).abs() < 1e-4, "{}", seen.cut);
    }
    // And C again stands up (CrouchEnd a000_216100).
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    app.update();
    assert_eq!(player_state(&mut app).0, "CrouchEnd");
}

/// An Ashina soldier (c1010 10100000, resident 241000) already in CAUTION that hears a new Wolf
/// sound plays ListenSound a000_000700: the new AiSoundParam id raises INTERUPT_ChangeSoundTarget
/// (46) and common_table_ai_common.lua:247 answers it after a 0-0.3 s wait. The run that first
/// alerted it (1010) goes through its caution start (EndureAttack 1010), not 46; the landing
/// (1310) afterwards is the new id. Once, not every tick (HANDOFF 4.47 round 25).
/// (It was lost before: ConfirmCautionTarget's terminate cleared the sound when the 46 reaction
/// replaced the plan, so the same sound was "new" again next frame and replanned it away;
/// FUN_1405c3770 skips the clear on a 46 frame - round 26.)
#[test]
pub(crate) fn a_cautious_enemy_listens_to_a_new_sound() {
    let mut app = app_cfg("c1010", Some(10100000), Vec::new());
    make_enemy_unaware(&mut app, 1.8, true);
    let (mut listens, mut last, mut seen) = (0, String::new(), Vec::new());
    let mut step = |app: &mut App, listens: &mut u32| {
        app.update();
        let (_, _, anim, desc, _) = enemy_info(app);
        let world = app.world_mut();
        let e = world.query::<&Enemy>().single(world).unwrap();
        if e.targeting.sound_new {
            seen.push(format!("NEW sound {:?}", e.targeting.sound.map(|s| s.id)));
        }
        if anim == "a000_000700" && last != anim {
            *listens += 1;
        }
        let line = format!("{anim} | {desc}");
        if seen.last() != Some(&line) {
            seen.push(line);
        }
        last = anim;
    };
    // Run by: heard (1010), CAUTION. (12 ticks: the run start and its stop carry Wolf about
    // 1.7 m, inside the landing sound's 2 m; 20 ran him out to 3 m once the stop kept the run's
    // momentum through its blend-in.)
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    for _ in 0..12 {
        step(&mut app, &mut listens);
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    for _ in 0..60 * 3 {
        step(&mut app, &mut listens);
    }
    assert_eq!(enemy_info(&mut app).0, crate::stealth::CAUTION);
    assert_eq!(listens, 0, "listened to the run that alerted it");
    // A jump: the landing's 1310 is a new sound id.
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..60 * 3 {
        step(&mut app, &mut listens);
    }
    assert_eq!(listens, 1, "ListenSound after the landing: {seen:#?}");
}

#[test]
pub(crate) fn running_near_an_unaware_enemy_is_heard_but_crouching_is_not() {
    // Run loop a000_000500 emits AiSound 1010 (radius 2 m) for its whole length; the crouch
    // walk 005200 only 1001 (0.25 m). Enemy 1.8 m ahead, back turned; Wolf moves sideways.
    let mut app = app();
    make_enemy_unaware(&mut app, 1.8, true);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    for _ in 0..20 {
        app.update();
    }
    let (state, kind, ..) = enemy_info(&mut app);
    assert_eq!((state, kind), (crate::stealth::CAUTION, crate::stealth::TYPE_SOUND), "running 1.8 m away is heard");

    let mut app = sim_tests_app();
    make_enemy_unaware(&mut app, 1.8, true);
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..90 {
        app.update();
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(1.0, 0.0);
        pad.walk = true;
    }
    for _ in 0..40 {
        app.update();
    }
    let (state, ..) = enemy_info(&mut app);
    assert_eq!(state, crate::stealth::NONE, "crouch-walking is not heard at 1.8 m");
}

/// A cautious enemy walking to a sound it heard (Wolf's AiSound 4, 10 m, at 7 m) stops on
/// entering another AI's trigger region tagged with that sound's serial: interrupt 48 ->
/// InterruptTableLogic_Common (common_table_ai_common.lua:286) -> ConfirmCautionTarget. A region
/// with another serial does not stop it (it confirms only near the sound, as before).
#[test]
pub(crate) fn a_cautious_enemy_stops_in_an_allys_trigger_region() {
    let stop_at = |same: bool| -> Option<f32> {
        let mut app = app();
        make_enemy_unaware(&mut app, 8.0, true);
        for _ in 0..5 {
            app.update();
        }
        let spot = Vec3::new(0.0, 0.9, 1.0);
        let serial = {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut sounds = world.resource_mut::<crate::enemy::AiSounds>();
                sounds.emit(&combat, 4, spot, None, None);
                sounds.0.last().unwrap().serial
            })
        };
        let mut placed = false;
        for _ in 0..300 {
            app.update();
            let (_, _, _, goal, pos) = enemy_info(&mut app);
            if !placed && goal.contains("MoveToSomewhere") && pos.distance(spot) > 5.0 {
                let r = crate::ai::TriggerRegion { owner: 12345, cat: 1000, unit: if same { serial } else { serial + 1 }, at: pos, radius: 5.0, life: 5.0 };
                app.world_mut().resource_mut::<crate::enemy::TriggerRegions>().0.push(r);
                placed = true;
            }
            if placed && goal.contains("ConfirmCautionTarget") {
                return Some(pos.distance(spot));
            }
        }
        assert!(placed, "never walked to the sound: {}", log_text(&app));
        None
    };
    let same = stop_at(true).expect("no stop in the region");
    assert!(same > 4.0, "stopped only at the sound: {same}");
    assert!(stop_at(false).is_none_or(|d| d < 3.5), "another serial stopped it");
}

/// HKS g_paramHkbState: CROUCH_START is STATE_TYPE_STANDBY, so the stick moves Wolf out of the
/// crouch-down at once (it used to hold him for its 0.83 s).
#[test]
pub(crate) fn moving_right_after_crouching_is_not_held() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "CrouchStart");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    for _ in 0..6 {
        app.update();
    }
    let mut q = app.world_mut().query_filtered::<&Actor, With<Player>>();
    let a = q.single(app.world()).unwrap();
    assert_eq!(a.state, "Locomotion");
    assert!(a.crouch, "still crouched");
}

#[test]
pub(crate) fn an_attack_from_a_crouch_stands_wolf_up() {
    // HKS BEH_A_GROUND_ATTACK in STYLE_TYPE_CROUCH: W_CrouchAttackToStand (a050_301000); Wolf is
    // standing after it.
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..40 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().attack_held = true;
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "CrouchAttackToStand");
    for _ in 0..5 {
        app.update();
    }
    let mut q = app.world_mut().query_filtered::<&Actor, With<Player>>();
    assert!(!q.single(app.world()).unwrap().crouch, "standing");
}

/// HKS BEH_A_SPRINT: dodge held from a stand, then the stick pushed: once the run loop (ref 91)
/// plays, the held dodge steps and the step goes on into the sprint (round 31: he only ran).
#[test]
pub(crate) fn holding_dodge_then_running_sprints() {
    let mut app = app();
    move_enemy_far(&mut app);
    let mut states = Vec::new();
    for i in 0..150 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.dodge_held = true;
            if i >= 10 {
                pad.stick = Vec2::new(0.0, 1.0);
            }
        }
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s == "SprintStartFromStep_F" || s == "SprintLoop"), "{states:?}");
}

/// HKS _execDamageStep: a step out of StandDamageLarge_F inside ref 502 (100344, 0.5-2.0 s) is
/// StandStepFromDamage_F (a000_206100); out of the face-down get-up inside ref 504 (100346) with
/// the stick back, StandStepFromDown_FaceDown_B (206121) (round 31: plain steps).
#[test]
pub(crate) fn a_step_out_of_a_large_hit_is_the_damage_step() {
    for (from, t, stick, want) in [
        ("StandDamageLarge_F", 0.6f32, Vec2::ZERO, "StandStepFromDamage_F"),
        ("StandDamageLargeDownWakeUp_FaceDown", 0.3, Vec2::new(0.0, -1.0), "StandStepFromDown_FaceDown_B"),
    ] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
                a.yaw = 0.0;
                assert!(a.play_state(&combat.player, from), "{from}");
                (a.t, a.prev_t) = (t, t);
            });
        }
        let mut states = Vec::new();
        for i in 0..4 {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = stick;
                if i == 0 {
                    pad.press(Action::Step);
                }
            }
            app.update();
            let s = player_actor(&mut app).0;
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        assert!(states.iter().any(|s| s == want), "{from}: {states:?}");
    }
}

#[test]
pub(crate) fn crouch_while_sprinting_is_the_slide() {
    // HKS BEH_A_CROUCH_START with ref 1 up: W_SprintToCrouchReady (a000_216010), then
    // SprintToCrouchLeft / Right (216020 / 216021), crouched.
    let mut app = app();
    force_player(&mut app, "SprintLoop", 3.0);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        pad.dodge_held = true;
        pad.press(Action::Crouch);
    }
    app.update();
    assert_eq!(player_state(&mut app).0, "SprintToCrouchReady");
    let mut seen = Vec::new();
    for _ in 0..60 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s.starts_with("SprintToCrouchLeft") || s.starts_with("SprintToCrouchRight")), "{seen:?}");
}

/// cargo test --release run_deflect_trace -- --ignored --nocapture : Wolf runs at the enemy,
/// then deflects; prints his state and position per frame.
#[test]
#[ignore]
pub(crate) fn run_deflect_trace() {
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Transform)>();
        let (_, mut t) = q.single_mut(world).unwrap();
        t.translation.z = -12.0;
        // SHINOBI_SPRINT_SIDE: the enemy 3 m to the right of the run instead of ahead.
        if std::env::var("SHINOBI_SPRINT_SIDE").is_ok() {
            t.translation = Vec3::new(3.0, t.translation.y, -4.0);
        }
    }
    let sprint = std::env::var("SHINOBI_SPRINT").is_ok();
    // SHINOBI_SPRINT_LOCK: locked on to the enemy.
    if std::env::var("SHINOBI_SPRINT_LOCK").is_ok() {
        if !app.world().contains_resource::<crate::camera::LockOn>() {
            app.world_mut().insert_resource(crate::camera::LockOn::default());
        }
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
        world.resource_mut::<crate::camera::LockOn>().target = Some(e);
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        if sprint {
            pad.dodge_held = true;
            pad.press(Action::Step);
        }
    }
    for f in 0..90 {
        if f == 40 {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Guard);
            pad.guard_held = true;
        }
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(world).unwrap();
        if f >= 30 {
            println!("{f:3} {:28} {:12} t {:.2} x {:.3} z {:.3} vel {:.2} yaw {:.0}", a.state, a.anim, a.t, t.translation.x, t.translation.z, a.move_vel.length(), a.yaw.to_degrees());
        }
    }
}

/// cargo test --release plunge_idle_trace -- --ignored --nocapture : c1010 stealth plunge, then
/// Wolf stands still; prints his state / clip / flags.
#[test]
#[ignore]
pub(crate) fn plunge_idle_trace() {
    // SHINOBI_PLUNGE_CHR: the enemy (default c1010).
    let mut app = app_with(&std::env::var("SHINOBI_PLUNGE_CHR").unwrap_or_else(|_| "c1010".into()));
    // SHINOBI_PLUNGE=broken: the debug menu's set-up (posture broken, enemy facing Wolf).
    if std::env::var("SHINOBI_PLUNGE").as_deref() == Ok("broken") {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let wolf = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).map(|(a, t)| (a.yaw, a.forward(), t.translation)).unwrap();
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<Player>>();
            for (mut e, mut a, mut tf) in q.iter_mut(world) {
                tf.translation = wolf.2 + wolf.1 * 1.2;
                a.yaw = wolf.0 + std::f32::consts::PI;
                a.posture = a.posture_max;
                e.on_posture_break(&mut a, &combat, false, 6.0);
            }
        });
    } else {
        make_enemy_unaware(&mut app, 1.2, true);
    }
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            a.play_state(&combat.player, "VerticalGroundJumpStart");
            a.t = 0.5;
            a.prev_t = 0.5;
            a.airborne = true;
            a.vel_y = -1.0;
            t.translation.y += 3.0;
            t.translation.z += 0.6;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut last = String::new();
    let mut last_e = String::new();
    for f in 0..1200 {
        app.update();
        let (st, kind, anim, desc, pos) = enemy_info(&mut app);
        let eyaw = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&Actor, With<Enemy>>();
            q.single(world).map_or(0.0, |a| a.yaw.to_degrees())
        };
        let eline = format!("ENEMY state {st} kind {kind} anim {anim} | {desc} pos ({:.1},{:.1}) yaw {:.0}", pos.x, pos.z, eyaw);
        if eline != last_e {
            println!("{f:4} {eline}");
            last_e = eline;
        }
        let mut q = app.world_mut().query_filtered::<(&Actor, &Transform), With<Player>>();
        let world = app.world();
        let (a, t) = q.single(world).unwrap();
        let combat = world.resource::<Combat>();
        let (clip, ct) = a.shown_clip(&combat.player);
        let line = format!("{} anim {} clip {} sheathed {} crouch {} air {} y {:.2}", a.state, a.anim, clip, a.sheathed, a.crouch, a.airborne, t.translation.y);
        if line != last {
            println!("{f:4} t {ct:.2} {line}");
            last = line;
        }
    }
}

/// cargo test --release crouch_drift_trace -- --ignored --nocapture
#[test]
#[ignore]
pub(crate) fn crouch_drift_trace() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for f in 0..240 {
        // The stick held forward from SHINOBI_CROUCH_MOVE_AT (frame, default 90 = after CrouchStart).
        if f == std::env::var("SHINOBI_CROUCH_MOVE_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(90) {
            app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
        }
        app.update();
        let mut q = app.world_mut().query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(app.world()).unwrap();
        if f % 6 == 0 {
            println!("{f:3} {:14} {:12} pos ({:.3},{:.3}) vel {:.3} crouch {}", a.state, a.anim, t.translation.x, t.translation.z, a.move_vel.length(), a.crouch);
        }
    }
}

/// The behind deathblow's absorb (ThrowParam 11010111, atkSorbDmyId 249) snaps the enemy's
/// capsule onto Wolf's dummy, and its model slides there over adsrobModelPosInterpolationTime
/// (0.5 s) instead of jumping.
#[test]
pub(crate) fn an_absorbed_enemy_model_slides_onto_the_throw_dummy() {
    let mut app = app_geom("c1010", None);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, true, 4.0);
            t.translation.z = 1.8;
            a.yaw = std::f32::consts::PI;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut seen, mut mid) = (None, 0.0f32);
    for f in 0..300 {
        app.update();
        let world = app.world_mut();
        let lag = world.query_filtered::<&crate::anim::ModelLag, With<Enemy>>().iter(world).next().map(|l| (l.offset.length(), l.time));
        let root = world.query_filtered::<&crate::anim::Skeleton, With<Enemy>>().iter(world).next().map(|s| s.root).unwrap();
        let off = world.get::<Transform>(root).unwrap().translation.with_y(0.0).length();
        if let (None, Some((jump, time))) = (seen, lag) {
            assert!((time - 0.5).abs() < 1e-4 && jump > 0.05, "jump {jump} time {time}");
            seen = Some(f);
        }
        if let Some(s) = seen {
            if f - s > 32 {
                assert!(off < 1e-4, "model still {off} m off the capsule after 0.53 s");
                break;
            }
            if f - s == 15 {
                mid = off;
            }
        }
    }
    assert!(seen.is_some(), "no absorb\n{}", log_text(&app));
    assert!(mid > 0.02, "no slide halfway: {mid} m");
}

/// cargo test --release behind_throw_live_trace -- --ignored --nocapture : the c1010 behind
/// deathblow (a200_511200 / ThrowDef13200) in the game's own conventions (x mirrored, yaw = pi -
/// ours), one line per 1/30 s like tools/rec_throw.py on rec_c1010_20261009 (tick 1985...).
#[test]
#[ignore]
pub(crate) fn behind_throw_live_trace() {
    let mut app = app_geom("c1010", None);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, true, 4.0);
            t.translation.z = 1.8;
            a.yaw = std::f32::consts::PI;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut n = None;
    for f in 0..400 {
        app.update();
        let world = app.world_mut();
        let w = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).map(|(a, t)| (a.anim.clone(), a.yaw, t.translation)).unwrap();
        let e = world.query_filtered::<(&Actor, &Transform), With<Enemy>>().single(world).map(|(a, t)| (a.anim.clone(), a.yaw, t.translation)).unwrap();
        if n.is_none() && (w.0.ends_with("_511200") || std::env::var("SHINOBI_THROW_EVERY").is_ok() && w.0.ends_with("_501200")) {
            n = Some(f);
        }
        let Some(s) = n else { continue };
        let every = if std::env::var("SHINOBI_THROW_EVERY").is_ok() { 1 } else { 2 };
        if (f - s) % every != 0 || f - s > 150 {
            continue;
        }
        let native = |y: f32| std::f32::consts::PI - y;
        let (dx, dz) = (-(e.2.x - w.2.x), e.2.z - w.2.z);
        let wrap = |d: f32| (d + 180.0).rem_euclid(360.0) - 180.0;
        let ang = wrap((dx.atan2(dz) - native(w.1)).to_degrees());
        let dyaw = wrap((native(e.1) - native(w.1)).to_degrees());
        println!("{:4} wolf {} enemy {} dist {:5.2} at {:7.1} deg  dyaw {:7.1}", (f - s) / 2, w.0, e.0, dx.hypot(dz), ang, dyaw);
    }
}

/// Z cycles the prosthetic slots (HKS BEH_A_ADD_SUB_WEAPON_CHANGE: the additive a000_412090), and
/// back in StandIdle the new tool unfolds in its own group (SubWeaponExpand: a071_412000 for the
/// Shinobi Firecracker); the prosthetic button then plays that tool's anims.
#[test]
pub(crate) fn switching_prosthetics_plays_the_next_tool() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000, 71000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::SwitchTool);
    app.update();
    {
        let world = app.world_mut();
        let mut q = world.query::<(&Player, &Actor)>();
        let (p, a) = q.single(world).unwrap();
        assert_eq!(p.tool_slot, 1);
        assert_eq!(a.add_anim, "a000_412090");
    }
    // A second press while the switch still plays is refused (validate 6394, round 49).
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::SwitchTool);
    app.update();
    assert_eq!(app.world_mut().query::<&Player>().single(app.world()).unwrap().tool_slot, 1);
    let mut expanded = false;
    for _ in 0..240 {
        app.update();
        let (s, k, ..) = player_state(&mut app);
        if s == "SubWeaponExpand" {
            assert_eq!(k, "a071_412000");
            expanded = true;
            break;
        }
    }
    assert!(expanded, "no SubWeaponExpand after the switch");
    for _ in 0..240 {
        app.update();
        if player_state(&mut app).0 == "StandIdle" {
            break;
        }
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    app.update();
    let (s, k, ..) = player_state(&mut app);
    assert_eq!(s, "GroundSubAttackCombo1");
    assert!(k.starts_with("a071_"), "{k}");
}

/// Shinobi Firecracker LV1 (71000, forced release: SpEffect 127100 ref 314): a071_400100's TAE 2
/// judges 150 / 160 fire Bullets 710050 / 710060 (3-way fans), whose sparks land as 2 m bursts
/// (710003, SpEffect 230110 "vs. non-special character"); the General is no beast, so c9997's
/// GetSpDamage gives SP_DAMAGE_BURST -> W_AssassinationBloodReaction. He has no cool time: his
/// a000_020110 adds only 8300, no SP_EFFECT_REF_NO_BURST_REACTION (1000044) row, so a second
/// firecracker staggers him again (round 30; the bullet's 107100 is only the gate for the
/// enemies' own cool-time rows).
#[test]
pub(crate) fn firecracker_staggers_the_general_every_time() {
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
    let mut staggered = false;
    for _ in 0..90 {
        app.update();
        if enemy_state(&mut app).0 == "AssassinationBloodReaction" {
            staggered = true;
            break;
        }
    }
    assert!(staggered, "no burst reaction; enemy {:?}, player {:?}", enemy_state(&mut app), player_state(&mut app));
    // Again once both are free: the cool time holds.
    for _ in 0..400 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut again = false;
    for _ in 0..90 {
        app.update();
        if enemy_state(&mut app).0 == "AssassinationBloodReaction" {
            again = true;
            break;
        }
    }
    assert!(again, "no second burst reaction; enemy {:?}", enemy_state(&mut app));
}

/// An enemy with its own cool time: c1070's a000_020110 adds 251070 (ref 1000044 NO_BURST_REACTION,
/// 15 s, invocationConditionsStateChange1 976 = the bullet's 107100) for 0.1 s of TAE. It is kept
/// its 15 s, and while it is on, a burst gives no reaction (c9997.lua 873).
#[test]
pub(crate) fn an_enemys_own_burst_cool_time_blocks_the_next_firecracker() {
    let mut app = app_with("c1070");
    for _ in 0..5 {
        app.update();
    }
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.add_timed(107100, 30.0);
        e.react(&mut a, &combat, "AssassinationBloodReaction");
        crate::prosthetic::add_reaction_cool_time(&combat, &a, &mut e, 1000044);
        let cool = e.clash.iter().find(|c| c.0 == 251070).map(|c| c.1);
        assert_eq!(cool, Some(15.0), "{:?}", e.clash);
        assert!(crate::prosthetic::enemy_refs(&combat, &a, &e).contains(&1000044));
        // Without the bullet's 976 the row stays off.
        e.clash.clear();
        crate::prosthetic::add_reaction_cool_time(&combat, &a, &mut e, 1000044);
        assert!(!e.clash.iter().any(|c| c.0 == 251070), "{:?}", e.clash);
    });
}

/// Every exported enemy (extracted/enemies/roster.json, `sekiro-extract npcs`): loads as itself,
/// its real AI scripts load and attack the idle player within 20 s, and a posture-broken enemy
/// takes Wolf's deathblow. Prints one line per enemy; run with
/// `cargo test --release all_enemies_smoke -- --ignored --nocapture`.
/// The characters that do not fight (merchants, nuns, handmaidens, Hanbei, scripted set pieces,
/// HANDOFF 4.11 / PLAN "fighters only"): the smoke test only asks that they stand - spawn, idle,
/// stay alive on their feet and never panic. c1300 waits for its Aging status.
pub(crate) const NON_FIGHTERS: &[&str] = &["c1012", "c1110", "c1111", "c1260", "c1300", "c1320", "c1460", "c5410", "c7200", "c7300", "c7420", "c7430", "c7440", "c7450"];

#[test]
#[ignore]
pub(crate) fn all_enemies_smoke() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extracted");
    let Ok(text) = std::fs::read_to_string(root.join("enemies/roster.json")) else { return };
    let roster: serde_json::Value = serde_json::from_str(&text).unwrap();
    let only = std::env::var("SHINOBI_ENEMIES").unwrap_or_default();
    let (mut ok, mut total) = (0, 0);
    for e in roster.as_array().unwrap() {
        let chr = e["chr"].as_str().unwrap();
        if !e["exported"].as_bool().unwrap_or(false) || !root.join(format!("enemies/{chr}.json")).exists() {
            continue;
        }
        if !only.is_empty() && !only.split(',').any(|c| c == chr) {
            continue;
        }
        total += 1;
        let fight = catch_unwind(AssertUnwindSafe(|| {
            let mut app = app_with(chr);
            let foe = app.world().resource::<Combat>().foe0().clone();
            {
                let world = app.world_mut();
                world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
            }
            let mut anims = std::collections::BTreeSet::new();
            let mut first_hit = None;
            let y0 = {
                let world = app.world_mut();
                world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation.y
            };
            let mut enemy_start: Option<Vec3> = None;
            for i in 0..1200 {
                app.update();
                anims.insert(enemy_state(&mut app).1);
                // Wolf closes in like a player (3 m/s to 2.5 m, as fight_until_hits) on an enemy
                // that moved more than 5 m from where it began (the Demon of Hatred's SpinStep
                // 5201 off a standing target), within 20 m. Otherwise he stands (the Divine
                // Dragon's script warps him 53 m off and its lightning finds him there at 18 s). A grab counts as the hit (the
                // ground zombie c1500's restraint 4120 does no damage).
                let grabbed = {
                    let world = app.world_mut();
                    let et = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
                    let (a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                    let to = (et - t.translation).with_y(0.0);
                    let e0 = *enemy_start.get_or_insert(et);
                    if (et - e0).with_y(0.0).length() > 5.0 && (2.5..20.0).contains(&to.length()) {
                        t.translation += to.normalize() * 3.0 / 60.0;
                    }
                    a.state == "Grabbed"
                };
                if first_hit.is_none() && (grabbed || log_text(&app).contains("enemy hit:") || log_text(&app).contains("thrown:")) {
                    first_hit = Some(i as f32 / 60.0);
                    break;
                }
            }
            let failed = app.world().get_non_send_resource::<crate::enemy::Brains>().is_some_and(|b| b.failed);
            // What applies to this row: a deathblow needs its ThrowParam start row (none for the
            // cricket and the phantom monk; the Divine Dragon and the underwater Headless have only
            // scripted / underwater ones); think 0-3 or hp 9999 = dummies, cutscene and conversation
            // NPCs. (ninsatuNum counts a boss's deathblows; regular enemies have 0.)
            let combat = app.world().resource::<Combat>();
            let can_blow = combat.throw(foe.throw_row(0)).is_some();
            let fights = row_fights(combat, &foe);
            let world = app.world_mut();
            let (e, ea, et) = world.query_filtered::<(&Enemy, &Actor, &Transform), Without<crate::enemy::Cast>>().single(world).unwrap();
            let stands = !e.is_dead() && ea.hp > 0.0 && et.translation.is_finite() && (et.translation.y - y0).abs() < 0.5;
            let desc = e.ai_desc.clone();
            (foe, failed, desc, anims.len(), first_hit, can_blow, fights, stands)
        }));
        let blow = catch_unwind(AssertUnwindSafe(|| {
            let mut app = app_with(chr);
            {
                let world = app.world_mut();
                world.resource_scope(|world, combat: Mut<Combat>| {
                    let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
                    let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
                    a.posture = a.posture_max;
                    e.on_posture_break(&mut a, &combat, false, 4.0);
                    t.translation.z = 1.8;
                });
            }
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            for _ in 0..60 {
                app.update();
                if player_state(&mut app).0 == "Deathblow" {
                    break;
                }
            }
            (player_state(&mut app).1, enemy_state(&mut app).0)
        }));
        let name = e["name"].as_str().unwrap_or("");
        let line = match (&fight, &blow) {
            (Ok((foe, failed, desc, n, hit, can_blow, fights, stands)), Ok((pa, es))) => {
                let good = if NON_FIGHTERS.contains(&chr) {
                    foe.chr == chr && *stands
                } else {
                    foe.chr == chr && !failed && (hit.is_some() || !fights) && (es.starts_with("ThrowDef") || !can_blow)
                };
                ok += good as usize;
                let goal: String = desc.chars().take(40).collect();
                format!(
                    "{} {chr} {name:<40} npc {} think {} anim {} | ai {} [{goal}] {n} anims, hit {} | deathblow {pa} / {es}{}{}",
                    if good { "OK  " } else { "FAIL" },
                    foe.npc_row,
                    foe.think_id,
                    foe.anim_chr,
                    if *failed { "FALLBACK" } else { "lua" },
                    hit.map_or("none".to_string(), |t| format!("{t:.1}s")),
                    if *can_blow { "" } else { " (no deathblow)" },
                    if *fights { "" } else { " (non-combatant)" }
                ) + if NON_FIGHTERS.contains(&chr) { " [stands rule]" } else { "" }
            }
            _ => {
                let msg = |r: &Box<dyn std::any::Any + Send>| r.downcast_ref::<String>().cloned().or(r.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                format!(
                    "PANIC {chr} {name:<40} fight: {} | deathblow: {}",
                    fight.as_ref().err().map(msg).unwrap_or("ok".into()),
                    blow.as_ref().err().map(msg).unwrap_or("ok".into())
                )
            }
        };
        println!("{line}");
    }
    println!("{ok} of {total} enemies pass");
}

/// Fights the enemy with Wolf standing (kept alive) until it lands `want` hits or `frames` pass:
/// (hits, distinct attack anims it played).
/// Whether a row fights: think 0-3 or hp 9999 are dummies, cutscene and conversation NPCs, and
/// battle goal 11000 is GOAL_NanimoShinai_11000_Battle ("does nothing": common_011000_battle.lua
/// only waits), the Folding Screen Monkeys' bar holder 12600050 (c1000, think 10000000, hp 100).
pub(crate) fn row_fights(combat: &Combat, foe: &crate::data::Foe) -> bool {
    let npc = combat.param("NpcParam", foe.npc_row);
    let goal = combat.param("NpcThinkParam", foe.think_id)["battleGoalID"].as_i64().unwrap_or(0);
    foe.think_id > 3 && npc["hp"].as_i64().unwrap_or(0) < 9999 && goal != 11000
}

pub(crate) fn fight_until_hits(app: &mut App, frames: u32, want: u32) -> (u32, usize) {
    let mut attacks = std::collections::BTreeSet::new();
    let mut hits = 0;
    let mut last_hit = 0;
    let mut was_grabbed = false;
    let mut was_reacting = false;
    for f in 0..frames {
        // ... and, after 8 s without a hit, swings every 2 s (a channel like the Monk's 3033
        // waits for that).
        if f - last_hit > 480 && f % 120 == 60 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        app.update();
        let anim = enemy_state(app).1;
        if anim.get(4..7).is_some_and(|g| g == "_00") && anim.get(7..8) == Some("3") {
            attacks.insert(anim);
        }
        let world = app.world_mut();
        let et = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        // A hit, or a grab that only holds (the hidden ground zombie c1500's restraint 4120:
        // ThrowParam 21500200 "grab restraint from the ground", no damage of its own).
        let grabbed = a.state == "Grabbed" && !was_grabbed;
        was_grabbed = a.state == "Grabbed";
        // ... or a reaction with no damage (the Divine Dragon's wind 52000830, atkPhys 0, throws
        // him back: GroundStormJumpBackReady).
        let reacting = a.state.contains("Damage") || a.state.contains("StormJumpBack");
        let reacted = reacting && !was_reacting;
        was_reacting = reacting;
        if a.hp < a.hp_max || grabbed || reacted {
            hits += 1;
            last_hit = f;
            a.hp = a.hp_max;
        }
        a.posture = 0.0;
        // Wolf closes in like a player would (3 m/s up to 2.5 m; after 8 s without a hit right
        // onto it, as onto the hidden ground zombie) and faces it.
        let to = (et - t.translation).with_y(0.0);
        // (Only onto one that has not attacked: the Divine Dragon's smashes 3007 / 3008, AtkParam
        // 120 / 130 from its hand dummies 10-11, land in front of it, not at its centre.)
        let stop = if f - last_hit > 480 && attacks.is_empty() { 0.05 } else { 2.5 };
        // (From far off, as after a boss script's warp to its arena entrance, 70 m from the
        // Divine Dragon: 15 m/s, a player sprinting and grappling in.)
        if to.length() > stop {
            t.translation += to.normalize() * if to.length() > 10.0 { 15.0 } else { 3.0 } / 60.0;
        }
        a.yaw = f32::atan2(-to.x, -to.z);
        if hits >= want {
            break;
        }
    }
    (hits, attacks.len())
}

/// Debug: `SHINOBI_ENEMIES=<chr> SHINOBI_PHASE=<deathblows first>`: the AI plan, state and
/// distance every half second for 20 s after that many deathblows.
#[test]
#[ignore]
pub(crate) fn trace_phase() {
    let chr = std::env::var("SHINOBI_ENEMIES").unwrap_or("c5000".into());
    let n: u32 = std::env::var("SHINOBI_PHASE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let mut app = app_with(&chr);
    for _ in 0..n {
        let seen = break_and_deathblow(&mut app, 900);
        println!("deathblow: {seen:?}");
    }
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    for f in 0..1200 {
        app.update();
        let world = app.world_mut();
        let et = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        let hit = a.hp < a.hp_max;
        a.hp = a.hp_max;
        a.posture = 0.0;
        let to = (et - t.translation).with_y(0.0);
        if to.length() > std::env::var("SHINOBI_STOP").ok().and_then(|v| v.parse().ok()).unwrap_or(2.5) {
            t.translation += to.normalize() * 3.0 / 60.0;
        }
        a.yaw = f32::atan2(-to.x, -to.z);
        if f % std::env::var("SHINOBI_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(30) == 0 || hit {
            let pt = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let (en, ea, et) = world.query_filtered::<(&Enemy, &Actor, &Transform), Without<crate::enemy::Cast>>().single(world).unwrap();
            println!("{:5.1}s {} {} {} t {:.2} d {:.1} at {:.1?} yaw {:.0} [{}] req {:?} res {:?}", f as f32 / 60.0, if hit { "HIT" } else { "   " }, ea.state, ea.anim, ea.t, (et.translation - pt).length(), et.translation, ea.yaw.to_degrees(), en.ai_desc, en.event_req(), ea.resident.len());
        }
    }
    println!("{}", log_text(&app).lines().filter(|l| l.contains("AI")).take(10).collect::<Vec<_>>().join("
"));
}

#[test]
#[ignore]
pub(crate) fn dbg_blow_after_fight() {
    let chr = std::env::var("SHINOBI_ENEMIES").unwrap_or("c1520".into());
    let mut app = app_with(&chr);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    fight_until_hits(&mut app, 1800, 2);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = false;
    }
    for _ in 0..240 {
        app.update();
    }
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.8);
            println!("after break: broken {} hp {}/{}", e.is_broken(), a.hp, a.hp_max);
        });
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
    }
    {
        let world = app.world_mut();
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let (e, ea, et) = world.query_filtered::<(&Enemy, &Actor, &Transform), Without<crate::enemy::Cast>>().single(world).unwrap();
        let c = crate::player::deathblow_check(world.resource::<Combat>(), wolf, (et.translation - wolf).with_y(0.0).normalize_or_zero(), false, ea, e, et.translation, &|_| None);
        println!("check: {:?}", c.map(|c| (c.behind, c.suffix, c.in_reach, c.start.map(|s| s.dist), c.main.dist)));
        let pa = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        println!("wolf hp {} posture {}/{} state {} anim {:?}", pa.hp, pa.posture, pa.posture_max, pa.state, pa.anim);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for f in 0..20 {
        app.update();
        let world = app.world_mut();
        let (e, ea, et) = world.query_filtered::<(&Enemy, &Actor, &Transform), Without<crate::enemy::Cast>>().single(world).unwrap();
        let (eb, es, ey, ep) = (e.is_broken(), ea.state.clone(), ea.yaw, et.translation);
        let (pa, pt) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        println!("{f} enemy broken {eb} {es} yaw {ey:.2} at {ep:?} | wolf {} {} at {:?}", pa.state, pa.anim, pt.translation);
    }
}

/// Every exported enemy, every phase: it attacks and lands hits, each deathblow plays its
/// ThrowDef, a boss gets up after each non-final one and fights again, and the last kills.
/// `SHINOBI_ENEMIES=c5000,c7020` limits the run.
#[test]
#[ignore]
pub(crate) fn all_enemies_full() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extracted");
    let Ok(text) = std::fs::read_to_string(root.join("enemies/roster.json")) else { return };
    let roster: serde_json::Value = serde_json::from_str(&text).unwrap();
    let only = std::env::var("SHINOBI_ENEMIES").unwrap_or_default();
    // Every exported chr with its roster row; then the boss fights (boss_scripts.json) whose
    // NpcParam row is another one (Owl (Father) 50601010, the Ape's second fight 51000100, ...).
    // SHINOBI_ENEMIES takes "c5060" (all its cases) or "c5060:50601010".
    let mut cases: Vec<(String, Option<i64>, String)> = Vec::new();
    for e in roster.as_array().unwrap() {
        let chr = e["chr"].as_str().unwrap();
        if e["exported"].as_bool().unwrap_or(false) && root.join(format!("enemies/{chr}.json")).exists() {
            cases.push((chr.to_string(), None, e["name"].as_str().unwrap_or("").to_string()));
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("enemies/boss_scripts.json")) {
        let scripts: serde_json::Value = serde_json::from_str(&text).unwrap();
        for (row, v) in scripts.as_object().unwrap() {
            let boss = &v["chars"][v["boss"].to_string()];
            let (Some(chr), Ok(npc)) = (boss["chr"].as_str(), row.parse::<i64>()) else { continue };
            let default = roster.as_array().unwrap().iter().find(|e| e["chr"] == chr).and_then(|e| e["npc"].as_i64());
            // (A row another fight's script brings in is checked through that fight's hand-over:
            // Lady Butterfly's second body 50900001, Tomoe 71101000.)
            let brought_in = scripts.as_object().unwrap().iter().any(|(r, o)| r != row && o["chars"].as_object().is_some_and(|c| c.values().any(|c| c["npc"].as_i64() == Some(npc))));
            if cases.iter().any(|c| c.0 == chr) && default != Some(npc) && !brought_in {
                cases.push((chr.to_string(), Some(npc), format!("boss row {npc}")));
            }
        }
    }
    let (mut ok, mut total) = (0, 0);
    for (chr, npc, name) in &cases {
        let chr = chr.as_str();
        let id = npc.map_or(chr.to_string(), |n| format!("{chr}:{n}"));
        if !only.is_empty() && !only.split(',').any(|c| c == chr || c == id) {
            continue;
        }
        total += 1;
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut app = app_cfg(chr, *npc, Vec::new());
            // The fighting enemy's numbers: whether it fights, has a deathblow row, its bars.
            let stats = |app: &mut App| {
                let world = app.world_mut();
                let mut q = world.query_filtered::<(&mut Enemy, &Actor), Without<crate::enemy::Cast>>();
                let (mut e, a) = q.single_mut(world).unwrap();
                e.aggressive = true;
                let blows = e.ninsatsu.1;
                let row = a.kind;
                let combat = world.resource::<Combat>();
                let foe = &combat.kind(row).foe;
                (row_fights(combat, foe), combat.throw(foe.throw_row(0)).is_some(), blows)
            };
            let (mut fights, mut can_blow, mut total_blows) = stats(&mut app);
            let mut good = true;
            let mut parts = Vec::new();
            // (A map script can bring it back after its last bar: the Guardian Ape gets up
            // headless, m17 11705810. Up to two more rounds then.)
            // (Without a deathblow row it never gets past its first bar.)
            let mut rounds = if can_blow { total_blows + 2 } else { total_blows };
            let defeats = |app: &App| {
                let bs = app.world().resource::<crate::enemy::BossScript>();
                bs.defeats + bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) as u32
            };
            // The rounds since the current boss came (after a hand-over).
            let mut base = 0;
            let mut phase = 0;
            while phase < rounds {
                let p = phase - base;
                let (hits, atks) = fight_until_hits(&mut app, 1800, 2);
                // (Wolf's swings can land a kneeling boss's finisher here: player.rs.)
                if phase > 0 && defeats(&app) > 0 {
                    good &= p >= total_blows;
                    parts.push("defeated".into());
                    break;
                }
                if fights && hits == 0 {
                    good = false;
                }
                // Settle: it stops attacking and Wolf gets back to idle before the deathblow.
                let set_aggr = |app: &mut App, on: bool| {
                    let world = app.world_mut();
                    world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = on;
                };
                set_aggr(&mut app, false);
                for f in 0..1440 {
                    if f >= 240 && player_state(&mut app).0 != "Grabbed" {
                        break;
                    }
                    app.update();
                    let world = app.world_mut();
                    let mut q = world.query_filtered::<&mut Actor, With<Player>>();
                    let mut a = q.single_mut(world).unwrap();
                    a.hp = a.hp_max;
                    a.posture = 0.0;
                }
                // (The last one: long enough for a map script's defeat, e.g. Isshin's 20200 sends
                // 30 at 11.6 s and m11_02 11125860 waits 5 s more.)
                let (seen, dead) = break_and_deathblow_died(&mut app, if p + 1 >= total_blows { 1500 } else { 900 });
                set_aggr(&mut app, true);
                let blown = seen.iter().any(|s| s.starts_with("ThrowDef"));
                if can_blow && !blown {
                    good = false;
                    let (ps, pa, _, _) = player_state(&mut app);
                    let world = app.world_mut();
                    let combat = world.resource::<Combat>();
                    let rows: Vec<String> = [0i64, 1, 110, 111]
                        .iter()
                        .map(|&sfx| match combat.throw(combat.foe0().throw_row(sfx)) {
                            Some(t) => format!("{sfx}:{}{}", t.atk_anim, if combat.player.anim(&t.atk_anim).is_some() { "" } else { "(missing)" }),
                            None => format!("{sfx}:none"),
                        })
                        .collect();
                    parts.push(format!("NO DEATHBLOW: enemy {seen:?} wolf {ps} {pa} rows {rows:?}"));
                }
                let left = {
                    let world = app.world_mut();
                    world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().ninsatsu.0
                };
                // Not before its last bar; and by the end of the rounds.
                let next = (dead || left == 0) && hand_over(&mut app);
                if can_blow && ((dead && p + 1 < total_blows) || (!dead && !next && phase + 1 == rounds)) {
                    good = false;
                }
                let end: Vec<&String> = seen.iter().filter(|s| s.starts_with("ThrowDef") || s.starts_with("Event") || s.starts_with("Dead")).collect();
                parts.push(format!("hits {hits} ({atks} atk) blow {end:?} left {left}{}", if dead { " DEAD" } else { "" }));
                phase += 1;
                if next {
                    // The next body or boss: the check goes on with it.
                    (fights, can_blow, total_blows) = stats(&mut app);
                    base = phase;
                    rounds = base + if can_blow { total_blows + 2 } else { total_blows };
                    parts.push("-> next".into());
                    continue;
                }
                if dead {
                    break;
                }
            }
            (good, fights, parts.join(" | "))
        }));
        let line = match r {
            Ok((good, fights, parts)) => {
                ok += good as usize;
                format!("{} {id} {name:<34} {}{parts}", if good { "OK  " } else { "FAIL" }, if fights { "" } else { "(non-combatant) " })
            }
            Err(p) => format!("PANIC {id} {name:<34} {}", p.downcast_ref::<String>().cloned().or(p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()),
        };
        println!("{line}");
    }
    println!("{ok} of {total} enemies pass every phase");
}

/// A boss handing over to its next body or boss (Lady Butterfly's 1000800 -> 1000810, m10
/// 11005812; Genichiro -> Tomoe, m11_01 11115820): the fighting enemy is dead or disabled and its
/// script has enabled one with a boss script of its own row (boss_scripts.json). That one becomes
/// the fighting enemy (loses `Cast`) and the old one gets `Cast`.
pub(crate) fn hand_over(app: &mut App) -> bool {
    let world = app.world_mut();
    let Some((old, done)) = world.query_filtered::<(Entity, &Enemy), Without<crate::enemy::Cast>>().iter(world).next().map(|(x, e)| (x, e.disabled || e.is_dead())) else { return false };
    if !done {
        return false;
    }
    let rows: Vec<(Entity, i64)> = {
        let mut q = world.query_filtered::<(Entity, &Enemy, &Actor), (With<crate::enemy::Cast>, Without<crate::enemy::Phantom>)>();
        let combat = world.resource::<Combat>();
        q.iter(world).filter(|(_, e, _)| !e.disabled && !e.is_dead()).map(|(x, _, a)| (x, combat.foe_of(a).npc_row)).collect()
    };
    let Some(&(next, _)) = rows.iter().find(|(_, row)| crate::enemy::script::scripts().contains_key(row)) else { return false };
    world.entity_mut(old).insert(crate::enemy::Cast);
    world.entity_mut(next).remove::<crate::enemy::Cast>();
    true
}

/// One use of a prosthetic tool level from idle: equips `tool`, presses F (held for `hold` frames),
/// runs `frames` and returns every state Wolf passed through, the Spirit Emblems spent and the
/// enemy's HP lost. The enemy stands idle 2 m in front.
pub(crate) fn use_tool(tool: i64, emblems: u32, hold: usize, frames: usize) -> (Vec<String>, u32, f32) {
    use_tool_sen(tool, emblems, hold, frames, 500)
}

/// As `use_tool`, carrying `sen` (the Sen Throw's level).
pub(crate) fn use_tool_sen(tool: i64, emblems: u32, hold: usize, frames: usize, sen: u32) -> (Vec<String>, u32, f32) {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.sen = sen;
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = emblems;
    }
    for _ in 0..10 {
        app.update();
    }
    let hp0 = enemy_hp(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    app.world_mut().resource_mut::<PadInput>().prosthetic_held = hold > 0;
    let mut states: Vec<String> = Vec::new();
    for f in 0..frames {
        if f == hold {
            app.world_mut().resource_mut::<PadInput>().prosthetic_held = false;
        }
        app.update();
        let s = player_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let left = {
        let world = app.world_mut();
        let mut q = world.query::<&Player>();
        q.single(world).unwrap().emblems
    };
    (states, emblems - left, hp0 - enemy_hp(&mut app))
}

pub(crate) fn enemy_hp(app: &mut App) -> f32 {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Enemy>>();
    q.single(world).unwrap().hp
}

/// Every tool's level 1 from idle (HKS BEH_A_GROUND_SUB_ATTACK, line 3523): the state it starts,
/// and the Spirit Emblems it costs - EquipParamWeapon resourceItemA, paid by each behaviour whose
/// BehaviorParam_PC row has wepCost (CharData::behavior). Level-1 tools carry ref 314 (always the
/// quick release).
#[test]
pub(crate) fn every_tool_starts_and_costs_its_emblems() {
    // (tool, first state, emblems spent in one use)
    let cases: &[(i64, &str, u32)] = &[
        (70000, "GroundSubAttackCombo1", 1),     // Shuriken: 107000150 (release throw)
        (71000, "GroundSubAttackCombo1", 2),     // Firecracker: 107100150
        (72000, "GroundSubAttackCombo1", 3),     // Flame Vent: 107200110
        (73000, "GroundSubAttackCombo1", 2),     // Axe: 107300102 (single attack)
        (75000, "GroundSubAttackCombo1", 1),     // Sabimaru: 107500100
        (76000, "GroundSubAttackGuardStart", 1), // Umbrella: 107600999 (consumption dummy)
        (77000, "GroundSubAttackCombo1", 3),     // Divine Abduction: 107700100
        (78000, "GroundSubAttackCombo1", 1),     // Spear: 107800105 (single thrust)
        (79000, "GroundSubAttackCombo1Moveable", 3), // Finger Whistle: 107900101
    ];
    let mut bad = Vec::new();
    for &(tool, first, cost) in cases {
        let (states, spent, _) = use_tool(tool, 20, 0, 150);
        let started = states.iter().any(|s| s == first);
        println!("{tool}: {states:?} spent {spent}");
        if !started || spent != cost {
            bad.push(format!("{tool}: want {first} / {cost} emblems, got {states:?} / {spent}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// No Spirit Emblems: the press plays the empty clack (W_SubAttackFailed, a070_400900) and costs
/// nothing; the Sabimaru (75), Divine Abduction (77) and Spear (78) branches check it themselves.
#[test]
pub(crate) fn a_tool_without_emblems_fails() {
    let (states, spent, _) = use_tool(70000, 0, 0, 60);
    assert!(states.iter().any(|s| s == "SubAttackFailed"), "{states:?}");
    assert_eq!(spent, 0);
}

/// The Loaded Axe's swing (a073_400100 judge 102 -> v7300:102) lands on the General standing
/// 2 m away: he guards it (posture) or takes the Axe's own attackBasePhysics 45 x 250 %.
#[test]
pub(crate) fn the_axe_hits() {
    let posture = |app: &mut App| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Actor, With<Enemy>>();
        q.single(world).unwrap().posture
    };
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![73000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    let (hp0, p0) = (enemy_hp(&mut app), posture(&mut app));
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..100 {
        app.update();
    }
    let (hp, p) = (enemy_hp(&mut app), posture(&mut app));
    assert!(hp < hp0 || p > p0, "the axe did not land: {}", log_text(&app));
}

/// Okinaga's Flame Vent (72200, resident ref 325 WEP_ENABLE_SUB_ATTACK_HOLD): held through the
/// spit (ref 321 + combo 300 window), it starts W_GroundSubAttackHoldStart and spews on
/// (HoldLoop) while held; letting go ends it (HoldEnd). The LV1 (72000) can't.
#[test]
pub(crate) fn okinagas_flame_vent_spews_while_held() {
    let (states, _, _) = use_tool(72200, 20, 200, 320);
    for want in ["GroundSubAttackCombo1", "GroundSubAttackHoldStart", "GroundSubAttackHoldLoop", "GroundSubAttackHoldEnd"] {
        assert!(states.iter().any(|s| s == want), "no {want}: {states:?}");
    }
    let (states, _, _) = use_tool(72000, 20, 200, 320);
    assert!(!states.iter().any(|s| s.starts_with("GroundSubAttackHold")), "LV1 held: {states:?}");
}

/// The Loaded Umbrella stays open while held (GuardStart -> GuardLoop) and closes on release
/// (BEH_A_GROUND_SUB_ATTACK_RELEASE -> W_GroundSubAttackGuardEnd).
#[test]
pub(crate) fn the_umbrella_opens_while_held() {
    let (states, _, _) = use_tool(76000, 20, 90, 160);
    for want in ["GroundSubAttackGuardStart", "GroundSubAttackGuardLoop", "GroundSubAttackGuardEnd"] {
        assert!(states.iter().any(|s| s == want), "no {want}: {states:?}");
    }
}


/// The Sabimaru's combo (HKS line 3605): a press inside SP_EF_REF_TAE_ENABLE_SUB_ATTACK_COMBO of
/// GroundSubAttackCombo1 goes on to Combo2 (free: only the odd steps cost).
#[test]
pub(crate) fn the_sabimaru_combos() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![75000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    let mut seen = Vec::new();
    for _ in 0..120 {
        app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "GroundSubAttackCombo2"), "{seen:?}");
}


/// One enemy's first seconds against an idle Wolf: state, anim, AI goals and the brain's log
/// (unknown natives "stub:", script errors). `SHINOBI_ENEMIES=c5100 cargo test --release
/// enemy_trace -- --ignored --nocapture`.
#[test]
#[ignore]
pub(crate) fn enemy_trace() {
    let chr = std::env::var("SHINOBI_ENEMIES").unwrap_or_else(|_| "c1020".into());
    let mut app = app_with(&chr);
    let foe = app.world().resource::<Combat>().foe0().clone();
    println!("{foe:?}");
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut last = String::new();
    for i in 0..600 {
        app.update();
        let (s, an, t) = enemy_state(&mut app);
        let world = app.world_mut();
        let e = world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap();
        let line = format!("{s} {an} | {} | ez {:?} fail {:?} | stealth {}", e.ai_desc, e.cur_ez, e.ez_failed, e.targeting.state);
        if line != last {
            println!("{:5.2}s t{t:.2} {line}", i as f32 / 60.0);
            last = line;
        }
    }
    if let Some(b) = app.world().get_non_send_resource::<crate::enemy::Brains>() {
        for brain in b.map.values() {
            let mut seen = std::collections::BTreeSet::new();
            for l in brain.log.iter().filter(|l| seen.insert((*l).clone())).take(60) {
                println!("log: {l}");
            }
        }
    }
    println!("{}", log_text(&app));
}

/// The Flame Vent sets the General on fire: its flame bullets carry burn build-up (Bullet
/// spEffectId0 9105 / 9109: registBlood, stateInfo 6) against NpcParam resist_blood; burning, he
/// loses changeHpRate % + changeHpPoint HP every motionInterval and a hit makes him flail
/// (c9997 SP_DAMAGE_BURNING -> W_FireReaction).
#[test]
pub(crate) fn the_flame_vent_burns() {
    let mut app = app();
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
    // One LV1 flame (9105: 125) fills 125 of the General's resist_blood 200; the second burns.
    let mut burned_at = None;
    let mut reacted = false;
    for f in 0..480 {
        if f == 0 || f == 200 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
        }
        app.update();
        let log = log_text(&app);
        if burned_at.is_none() && log.contains("BURNING") {
            burned_at = Some((f, enemy_hp(&mut app)));
        }
        reacted |= enemy_state(&mut app).0 == "FireReaction";
    }
    let (_, hp) = burned_at.unwrap_or_else(|| panic!("never caught fire:\n{}", log_text(&app)));
    assert!(enemy_hp(&mut app) < hp, "no burn damage:\n{}", log_text(&app));
    assert!(log_text(&app).contains("burn: -"), "{}", log_text(&app));
    assert!(reacted, "no W_FireReaction");
}

/// R35's guard-skip: a blocked hit that sets the General on fire plays W_FireReaction, not his
/// guard (c9997 ExecDebuffReaction runs before ExecGuardBlock and its TRUE ends ExecDamage).
/// Wolf's sword is given the Flame Vent's burn build-up 9105, raised so the first hit ignites.
#[test]
pub(crate) fn a_blocked_hit_that_ignites_plays_the_fire_reaction() {
    let mut app = app();
    {
        let mut combat = app.world_mut().resource_mut::<Combat>();
        for atk in combat.player.attacks.values_mut() {
            atk.sp_effect_id0 = 9105;
        }
        combat.player.sp_effects.get_mut("9105").expect("9105").regist_blood = 10000.0;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut blocked = None;
    for f in 0..120 {
        app.update();
        if blocked.is_none() && log_text(&app).contains("enemy blocked") {
            blocked = Some(f);
        }
        if blocked.is_some() {
            break;
        }
    }
    let out = log_text(&app);
    assert!(blocked.is_some() && out.contains("BURNING"), "{out}
enemy={:?}", enemy_state(&mut app));
    assert_eq!(enemy_state(&mut app).0, "FireReaction", "{out}");
}

/// A guarding General blocks a shuriken, but the Flame Vent (AtkParam disableGuard /
/// disableJustGuard 1) goes through his guard and hurts him.
#[test]
pub(crate) fn the_flame_vent_goes_through_a_guard() {
    // (tool, the enemy's HP lost while he holds a block pose)
    let run = |tool: i64| -> f32 {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 2.0;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            let mut q = world.query::<&mut Player>();
            q.single_mut(world).unwrap().emblems = 20;
        }
        for _ in 0..10 {
            app.update();
        }
        // A block pose of his: a ShieldBlock flag (3) without the deflect window there.
        let (key, t) = {
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap().clone();
            let combat = world.resource::<Combat>();
            let d = combat.data_of(&a);
            let mut found: Vec<(String, f32)> = d
                .anims
                .iter()
                .flat_map(|(k, an)| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(crate::player::FLAG_SHIELD_BLOCK)).map(move |e| (k.clone(), e.start + 0.05)))
                .filter(|(k, t)| d.flag(k, *t, crate::player::FLAG_SHIELD_BLOCK) && !d.has_state_info(k, *t, crate::data::STATE_INFO_JUST_GUARD))
                .collect();
            found.sort_by(|x, y| x.0.cmp(&y.0));
            found.into_iter().next().expect("a block pose")
        };
        let hold = |app: &mut App| {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
            let mut a = q.single_mut(world).unwrap();
            if a.anim != key {
                a.play("Guard", &key);
            }
            a.prev_t = t;
            a.t = t;
        };
        hold(&mut app);
        let hp0 = enemy_hp(&mut app);
        app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
        for _ in 0..150 {
            hold(&mut app);
            app.update();
        }
        hp0 - enemy_hp(&mut app)
    };
    let shuriken = run(70000);
    let flame = run(72000);
    assert!(shuriken <= 0.0, "the shuriken went through the guard: -{shuriken}");
    assert!(flame > 0.0, "the Flame Vent was blocked");
}

/// A guarding enemy blocks Wolf's shuriken as a sword blow (c9997 ExecGuardBlock): the guard
/// reaction plays and the AI's guard streak counts it (clash 200215 / 200216), round 35 (it
/// stood still and took the raw repelLostStamDamage).
#[test]
pub(crate) fn a_blocked_shuriken_plays_the_guard_reaction() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = 20;
    }
    for _ in 0..10 {
        app.update();
    }
    let (key, t) = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap().clone();
        let combat = world.resource::<Combat>();
        let d = combat.data_of(&a);
        let mut found: Vec<(String, f32)> = d
            .anims
            .iter()
            .flat_map(|(k, an)| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(crate::player::FLAG_SHIELD_BLOCK)).map(move |e| (k.clone(), e.start + 0.05)))
            .filter(|(k, t)| d.flag(k, *t, crate::player::FLAG_SHIELD_BLOCK) && !d.has_state_info(k, *t, crate::data::STATE_INFO_JUST_GUARD))
            .collect();
        found.sort_by(|x, y| x.0.cmp(&y.0));
        found.into_iter().next().expect("a block pose")
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut blocked = false;
    for _ in 0..150 {
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            if a.anim != key {
                a.play("Guard", &key);
            }
            (a.prev_t, a.t) = (t, t);
        }
        app.update();
        if log_text(&app).contains("enemy blocks the shuriken") {
            blocked = true;
            break;
        }
    }
    assert!(blocked, "no block:
{}", log_text(&app));
    let world = app.world_mut();
    let (a, e) = world.query_filtered::<(&Actor, &Enemy), Without<crate::enemy::Cast>>().single(world).unwrap();
    assert!(e.guard_count() >= 1, "the block was not counted as a clash");
    assert!(a.state != "Guard" && a.state.contains("Guard"), "no guard reaction: {}", a.state);
}
