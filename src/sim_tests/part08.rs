//! Sim tests, part 8: 49 tests, from `a_jump_in_the_break_pose_vaults_without_killing` to `a_patrol_point_with_a_zero_region_is_reached_by_is_arrived`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// A jump in the deflect break pose (ThrowParam 11020012 弾き1B_ジャンプ: a201_510120 / ThrowDef12120)
/// vaults round behind the General and leaves him open to the deathblow; it killed him outright
/// as a deathblow before (2026-10-12 bug hunt round 7).
#[test]
pub(crate) fn a_jump_in_the_break_pose_vaults_without_killing() {
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.single_mut(world).unwrap();
        a.posture = a.posture_max - 1.0;
    }
    start_enemy_combo(&mut app, 0);
    let (mut pressed, mut pose, mut vault) = (false, None, false);
    for i in 0..300u32 {
        if !pressed && time_to_enemy_hit(&mut app).is_some_and(|dt| dt <= 3.0 / TAE_FPS) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
            pressed = true;
        }
        app.update();
        let (ps, ..) = player_state(&mut app);
        if ps == "ThrowBreak" && pose.is_none() {
            pose = Some(i);
        }
        if pose.is_some_and(|p| i == p + 15) {
            app.world_mut().resource_mut::<PadInput>().guard_held = false;
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
        vault |= enemy_state(&mut app).0 == "ThrowDef12120";
        if pose.is_some_and(|p| i == p + 60) {
            break;
        }
    }
    assert!(pose.is_some(), "no break pose
{}", log_text(&app));
    assert!(vault, "no ThrowDef12120 vault: {:?}", enemy_state(&mut app));
    let world = app.world_mut();
    let (a, e) = world.query::<(&Actor, &Enemy)>().single(world).unwrap();
    assert!(a.hp > 0.0 && !e.is_dead(), "the vault killed him (hp {})", a.hp);
}

/// The General's neck grab (ThrowParam 21020000: atkSorbDmyId 230, a point in front of him)
/// takes Wolf onto that dummy, wherever the grab capsule touched him (2026-10-12 bug hunt round 7).
#[test]
pub(crate) fn a_grab_pulls_wolf_onto_the_grabbers_dummy() {
    let mut app = app_geom("c1020", None);
    for _ in 0..3 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003012");
    }
    let mut held = None;
    for i in 0..200u32 {
        app.update();
        if held.is_none() && player_state(&mut app).0 == "Grabbed" {
            held = Some(i);
        }
        if held.is_some_and(|h| i == h + 2) {
            break;
        }
    }
    assert!(held.is_some(), "no grab\n{}", log_text(&app));
    let world = app.world_mut();
    let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let (dm, etf) = world.query_filtered::<(&crate::model::Dummies, &Transform), With<Enemy>>().single(world).unwrap();
    let (m, et) = (dm.0[&230], etf.translation);
    let at = world.get::<GlobalTransform>(m).unwrap().translation();
    let off = (wolf - at).with_y(0.0).length();
    // (2 cm with the absorb, two ticks of his anim later; 10-12 cm where the capsule touched.)
    assert!(off < 0.05, "Wolf {off:.2} m off the grab dummy 230 (Wolf {wolf}, dummy {at}, General {et})");
}

/// An enemy grab is a throw pair too: separate() leaves "Grabbed" / ThrowAtk* alone. The Rappa
/// (c1360, ThrowParam 21360000 defSorbDmyId 173: onto Wolf's dummy 0.52 m behind his root) is
/// absorbed there and its anim then rides onto his back, roots ~4 cm apart; the push-out held it
/// at the 0.8 m radii from the next tick on (2026-10-12 bug hunt round 14).
#[test]
pub(crate) fn a_grab_pair_is_not_pushed_apart() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1360.json")).exists() {
        return;
    }
    let mut app = app_geom("c1360", None);
    for _ in 0..3 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        // a000_003016: the grab (judge 180, throwFlag 1).
        e.force_anim(&mut a, "a000_003016");
    }
    let (mut held, mut absorbed, mut closest) = (None, f32::MAX, f32::MAX);
    for i in 0..300u32 {
        app.update();
        let grabbed = player_state(&mut app).0 == "Grabbed";
        if held.is_none() && grabbed {
            held = Some(i);
        }
        let Some(h) = held else { continue };
        if !grabbed || i > h + 30 {
            break;
        }
        let world = app.world_mut();
        let (dm, wt) = world.query_filtered::<(&crate::model::Dummies, &Transform), With<Player>>().single(world).unwrap();
        let (m, wt) = (dm.0[&173], wt.translation);
        let at = world.get::<GlobalTransform>(m).unwrap().translation();
        let et = world.query_filtered::<&Transform, With<Enemy>>().single(world).unwrap().translation;
        if i == h + 2 {
            absorbed = (et - at).with_y(0.0).length();
        }
        closest = closest.min((et - wt).with_y(0.0).length());
    }
    assert!(held.is_some(), "no grab
{}", log_text(&app));
    assert!(absorbed < 0.05, "the absorb left the Rappa {absorbed:.2} m off Wolf's dummy 173");
    assert!(closest < 0.5, "the pair was held {closest:.2} m apart (the body radii are 0.8 m)");
}

/// A guard pressed in the air guard's break pose (AirDeflectGuardBreak, a050_190150) waits for
/// the pose's own flag 117 at frame 69: it does not start a fresh air guard at once (2026-10-12
/// bug hunt round 15: is_air_guard matched the break pose too).
#[test]
pub(crate) fn an_air_guard_break_holds_wolf_until_its_guard_flag() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..40 {
        app.update();
        if player_actor(&mut app).2 {
            break;
        }
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        assert!(a.airborne, "the jump did not leave the ground: {}", a.state);
        a.play("AirDeflectGuardBreak", "a050_190150");
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    for _ in 0..3 {
        app.update();
    }
    let (state, ..) = player_actor(&mut app);
    assert!(!state.starts_with("AirDeflectGuardStart"), "a guard press left the air guard break at once: {state}");
}

/// A jump pressed early in an air slash (AirComboAttack1 frame 5: flag 87 buffers from 3, the
/// jump's 119 opens at 12) kicks when the window opens, as a mashed attack chains; the press
/// was dropped on the spot (2026-10-12 bug hunt round 8).
#[test]
pub(crate) fn an_early_jump_in_an_air_slash_becomes_the_air_kick() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut slash, mut kicked, mut pressed, mut seen) = (None, false, false, Vec::<String>::new());
    for i in 0..200u32 {
        app.update();
        let (state, _, airborne, _) = player_actor(&mut app);
        if seen.last() != Some(&state) {
            seen.push(state.clone());
        }
        if airborne && slash.is_none() && !state.starts_with("AirComboAttack") && i > 6 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        if state == "AirComboAttack1" && slash.is_none() {
            slash = Some(i);
        }
        if !pressed && slash.is_some_and(|s| i == s + 5) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
            pressed = true;
        }
        kicked |= state == "AirKick";
        if kicked || !airborne && slash.is_some() {
            break;
        }
    }
    assert!(slash.is_some(), "no air slash: {seen:?}");
    assert!(kicked, "the early jump press was lost: {seen:?}");
}

/// Regular enemies turn in place with their turn clips (c9997 ExecTurn): an unaware General
/// turned 150 degrees off his home facing turns back (NonBattleAct's Turn / Stay to
/// POINT_INIT_POSE) with a W_Turn*_*180 clip, not a slow spin, and ends facing it again.
#[test]
pub(crate) fn an_enemy_turns_in_place_with_its_turn_clip() {
    let mut app = app_cfg("c1010", None, vec![]);
    let enemy = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Actor, &mut Enemy), With<Enemy>>();
        let (mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        e.make_unaware(&mut a);
        ents[0]
    };
    for _ in 0..30 {
        app.update();
    }
    let home = app.world().get::<Actor>(enemy).unwrap().yaw;
    app.world_mut().get_mut::<Actor>(enemy).unwrap().yaw = home + 150f32.to_radians();
    let mut states = Vec::new();
    let mut faced = None;
    for i in 0..60 * 8 {
        app.update();
        let a = app.world().get::<Actor>(enemy).unwrap();
        if states.last() != Some(&a.state) {
            states.push(a.state.clone());
        }
        let off = ((a.yaw - home + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs();
        if states.iter().any(|s| s.starts_with("Turn")) && !a.state.starts_with("Turn") && off < 0.2 {
            faced = Some(i);
            break;
        }
    }
    assert!(states.iter().any(|s| s.starts_with("Turn") && s.ends_with("180")), "a 180 turn clip played: {states:?}");
    let i = faced.unwrap_or_else(|| panic!("it ends up facing its home way: {states:?}"));
    assert!(i < 60 * 3, "within 3 s ({i} frames)");
}

/// A group enemy whose npc_row is not one of its chr's rows (c1110 11103000) still comes in,
/// on its chr's main row, and its kind is loaded once (2026-10-12 bug hunt round 9: the line-up
/// looked it up by the asked row and dropped it).
#[test]
pub(crate) fn a_group_enemy_with_a_foreign_row_still_spawns() {
    let g = crate::config::GroupEnemy { chr: "c1110".into(), npc_row: Some(11103000), ..Default::default() };
    let mut app = app_cfg("c1010", None, vec![g.clone(), g]);
    app.update();
    let combat = app.world().resource::<Combat>();
    let k = combat.kind_index("c1110", Some(11103000)).expect("found by the asked row");
    assert_ne!(combat.kinds[k].foe.npc_row, 11103000, "on the chr's main row");
    assert_eq!(combat.kinds.iter().filter(|k| k.foe.chr == "c1110").count(), 1);
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<Enemy>>().iter(world).count(), 3);
}

/// The duel line-up faces Wolf's start (the arena export's enemy faces +Z, map.rs player_start),
/// the main enemy and the group alike (2026-10-12 bug hunt round 10: they faced -Z, away).
#[test]
pub(crate) fn the_line_up_faces_wolf() {
    let g = crate::config::GroupEnemy { chr: "c1010".into(), ..Default::default() };
    let mut app = app_cfg("c1010", None, vec![g]);
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let mut n = 0;
    for (t, a) in world.query_filtered::<(&Transform, &Actor), With<Enemy>>().iter(world) {
        let to = (wolf - t.translation).with_y(0.0).normalize();
        assert!(a.forward().dot(to) > 0.9, "facing Wolf: {} vs {to}", a.forward());
        n += 1;
    }
    assert_eq!(n, 2);
}

/// The 危 kanji is the warning bullet's hit (Bullet 10200982: 2 m, 0.1 s, SpEffect 211000), not
/// the TAE start for any enemy anywhere: the General's perilous thrust raises it with Wolf at
/// hand, not from 8 m away (round 31).
#[test]
pub(crate) fn the_perilous_kanji_needs_its_warning_bullet_to_reach_wolf() {
    let raised = |z: f32| {
        let mut app = app();
        app.update();
        app.init_resource::<crate::hud::Perilous>();
        {
            let world = app.world_mut();
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            t.translation.z = z;
            e.force_anim(&mut a, PERILOUS_THRUST);
        }
        for _ in 0..30 {
            app.update();
        }
        app.world().resource::<crate::hud::Perilous>().kind == 982
    };
    assert!(raised(1.5), "no kanji at 1.5 m");
    assert!(!raised(8.0), "the kanji went up 8 m away");
}

/// Every warning bullet carries SpEffect 211000, not only the 980 / 982 / 983 suffixes: Isshin's
/// a100_003016 fires Bullet 54000986 (sweep) and the Way of Tomoe's a000_003043 fires 71100986,
/// so their kanji went up in the game and not here (boss bug hunt 2026-10-14).
#[test]
pub(crate) fn the_perilous_kanji_comes_with_every_warning_bullet() {
    for (chr, anim, warn) in [("c5400", "a100_003016", 986), ("c7110", "a000_003043", 986)] {
        if !std::path::Path::new(&format!("{}/extracted/enemies/{chr}.json", env!("CARGO_MANIFEST_DIR"))).exists() {
            continue;
        }
        let mut app = app_with(chr);
        // The arena's map switch (map.rs) puts Wolf on its start spot a few frames in.
        for _ in 0..10 {
            app.update();
        }
        app.init_resource::<crate::hud::Perilous>();
        {
            let world = app.world_mut();
            // The arena rows stand Wolf well away (Isshin's 15 m): he steps up in front of the boss.
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
            let (mut e, mut a, t) = q.single_mut(world).unwrap();
            let front = t.translation + a.forward() * 1.5;
            e.force_anim(&mut a, anim);
            world.query_filtered::<&mut Transform, With<Player>>().single_mut(world).unwrap().translation = front;
        }
        let mut raised = false;
        for _ in 0..120 {
            app.update();
            raised |= app.world().resource::<crate::hud::Perilous>().kind == warn;
        }
        assert!(raised, "{chr} {anim}: no kanji for its {warn} bullet");
    }
}

/// The kanji is the game's effect: SpEffect 211000's SpEffectVfx 48100 plays 600035 on Wolf's
/// dummy 220 (the HUD text only stands in without it).
#[test]
pub(crate) fn the_perilous_kanji_is_its_effect_on_wolf() {
    let mut app = app();
    app.update();
    app.init_resource::<crate::hud::Perilous>();
    app.init_resource::<crate::fxr::FxrLib>();
    assert!(app.world_mut().resource_mut::<crate::fxr::FxrLib>().has(600035), "fxr 600035 (the kanji) is extracted");
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
        let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
        t.translation.z = 1.5;
        e.force_anim(&mut a, PERILOUS_THRUST);
    }
    let mut played = false;
    for _ in 0..30 {
        app.update();
        let world = app.world_mut();
        played |= world.query::<&crate::fxr::FxEffect>().iter(world).any(|f| f.id == 600035);
    }
    assert!(played, "no 600035 effect");
    let p = app.world().resource::<crate::hud::Perilous>();
    assert_eq!(p.kind, 982);
    assert!(p.timer <= 0.0, "the HUD text showed as well");
}

/// A reset clears the perilous kanji (2026-10-12 bug hunt round 10: it stayed up to 0.8 s over
/// the new line-up).
#[test]
pub(crate) fn a_reset_clears_the_perilous_kanji() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    if app.world().get_resource::<crate::hud::Perilous>().is_none() {
        app.init_resource::<crate::hud::Perilous>();
    }
    app.world_mut().resource_mut::<crate::hud::Perilous>().raise(982, true);
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    app.update();
    assert!(app.world().resource::<crate::hud::Perilous>().timer <= 0.0);
}

/// launchConditionType picks which ends launch HitBulletID (2026-10-12 bug hunt round 10: every
/// end did, so the Genpei arrow 13100520's lct 1 "hit water" child came on every hit).
#[test]
pub(crate) fn a_bullets_child_follows_its_launch_condition() {
    use crate::enemy_bullet::launches;
    // (lct, hit enemy, landed, expired)
    assert!(!launches(1, true, false, false) && !launches(2, false, true, false), "no water in the sim");
    assert!(launches(0, true, false, false) && launches(0, false, false, true));
    assert!(launches(3, false, true, false) && !launches(3, true, false, false));
    assert!(launches(4, true, false, false) && !launches(4, false, true, false));
    assert!(launches(254, false, false, true) && !launches(254, true, false, false));
    assert!(launches(255, false, true, false) && !launches(255, false, false, true));
}

/// A bullet that touches Wolf inside his iframes (TAE 950 / 951 / 954 type 2, combat.rs dodged)
/// passes through: the gecko's poison cloud (11000021, build-up only, no damage) puts nothing on
/// him then, but does when he is not dodging (2026-10-12 bug hunt round 10).
#[test]
pub(crate) fn a_dodged_bullet_passes_through_wolf() {
    let run = |dodge: bool| -> bool {
        let mut app = app_cfg("c1100", None, Vec::new());
        app.update();
        let world = app.world_mut();
        let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
        let combat = world.resource::<Combat>();
        let spec = combat.kind(0).data.bullet_rows.get("11000021").expect("gecko poison row").clone();
        // One of Wolf's anims with a long full-iframe window.
        let iframe = combat
            .player
            .anims
            .iter()
            .flat_map(|(k, a)| a.events.iter().filter(|e| e.kind == 950 && e.end - e.start > 0.5).map(move |e| (k.clone(), e.start + 0.05)))
            .min_by(|a, b| a.0.cmp(&b.0))
            .expect("an anim with TAE 950");
        let (wolf, at) = {
            let mut q = world.query_filtered::<(Entity, &Transform), With<Player>>();
            let (e, t) = q.single(world).unwrap();
            (e, t.translation)
        };
        crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Z, 0.0, None);
        world.flush();
        // A cloud (initVellocity 0, hitRadius 1, life 2 s) on Wolf.
        for _ in 0..60 {
            if dodge {
                let mut a = app.world_mut().get_mut::<Actor>(wolf).unwrap();
                a.anim = iframe.0.clone();
                a.t = iframe.1;
                a.prev_t = iframe.1;
            }
            app.update();
        }
        app.world().resource::<crate::status::Statuses>().0.contains_key(&wolf)
    };
    assert!(run(false), "the spit lands on a Wolf who is not dodging");
    assert!(!run(true), "it passes through his iframes");
}

/// The guard stance (c9997 ExecAIGuard, GOAL_COMMON_Guard's request 9910 = ACTION_TYPE_GUARD):
/// GuardStart -> GuardIdle, guard walks and W_TurnGuard while held, GuardEnd when the request
/// stops; the stance's clips hold ShieldBlock (2026-10-12: 9910 was played as a missing clip,
/// so enemies never took their guard).
#[test]
pub(crate) fn the_guard_goal_holds_the_guard_stance() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    let world = app.world_mut();
    let ent = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Actor, &Enemy)>();
        let (mut a, e) = q.get_mut(world, ent).unwrap();
        let d = combat.data_of(&a);
        let mut cmd = crate::ai::Command { guard: true, ..Default::default() };
        a.procedural("IdleBattle");
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GuardStart");
        crate::enemy::guard_stance(&mut a, d, &cmd, true, true);
        assert_eq!(a.state, "GuardIdle");
        assert!(d.flag(&a.anim, 0.5, crate::player::FLAG_SHIELD_BLOCK), "the stance blocks");
        cmd.mv = Some('L');
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GuardWalkLeft");
        cmd.mv = None;
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GuardIdle");
        let yaw = a.yaw;
        assert!(crate::enemy::start_turn(e, &mut a, d, yaw + 2.0, false));
        assert!(a.state.starts_with("TurnGuard_Left"), "{}", a.state);
        crate::enemy::guard_stance(&mut a, d, &cmd, true, true);
        assert_eq!(a.state, "GuardIdle");
        cmd.guard = false;
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GuardEnd");
    });
}

/// A penetrating bullet launches its child by its whole-life hit record, not by the frame it
/// ends on (2026-10-12 bug hunt round 11: the c1190 cannon blast 11903310, isPenetrate 1, lct
/// 255, never spawned 11903311 after catching Wolf). The gecko's poison cloud (11000021,
/// isPenetrate 1, life 2 s) on Wolf, made launchConditionType 4 "hit enemy": its child comes.
#[test]
pub(crate) fn a_penetrating_bullet_launches_by_its_hit_record() {
    let mut app = app_cfg("c1100", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let mut spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("11000021").expect("gecko poison row").clone();
    assert_eq!(spec.is_penetrate, 1);
    spec.launch_condition_type = 4;
    spec.hit_bullet_id = 11000021;
    let at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Z, 0.0, None);
    world.flush();
    // Past its 2 s life: the first cloud is gone, its child is there.
    for _ in 0..(60 * 2 + 30) {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).count(), 1, "the child cloud");
}

#[test]
pub(crate) fn a_camera_shake_starts_once_per_tae_event() {
    // Frames faster than the fixed step: (prev_t, t) only moves in FixedUpdate, so a shake
    // started in Update was pushed again on every frame between two steps.
    let mut app = app();
    crate::camera::add_shakes(&mut app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 240.0)));
    let (key, start) = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().clone();
        let d = world.resource::<Combat>().data_of(&a);
        let mut found: Vec<(String, f32)> = d
            .anims
            .iter()
            .flat_map(|(k, an)| an.events.iter().filter(|e| matches!(e.kind, 145 | 146) && e.start > 0.1 && e.arg_i64("RumbleCamID").is_some()).map(move |e| (k.clone(), e.start)))
            .collect();
        found.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)));
        found.into_iter().next().expect("a Wolf anim with a RumbleCam event")
    };
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.play("Attack", &key);
        a.prev_t = start - 0.06;
        a.t = start - 0.06;
        world.resource_mut::<crate::camera::CameraShake>().0.clear();
    }
    let mut pushed = 0;
    for _ in 0..40 {
        app.update();
        let world = app.world_mut();
        let mut shake = world.resource_mut::<crate::camera::CameraShake>();
        pushed += shake.0.len();
        shake.0.clear();
    }
    let events = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().clone();
        let d = world.resource::<Combat>().data_of(&a);
        d.anim(&key).unwrap().events.iter().filter(|e| matches!(e.kind, 145 | 146) && (e.start - start).abs() < 1e-4 && e.arg_i64("RumbleCamID").is_some()).count()
    };
    assert_eq!(pushed, events, "{key} at {start}");
}

/// A deflect that empties the posture puts the enemy in its break pose (ThrowDef12100), and
/// attack takes it to the deathblow: the 忍殺 mark shows there, on the same enemy.
#[test]
pub(crate) fn the_deathblow_mark_shows_on_a_break_pose() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.single_mut(world).unwrap();
        a.posture = a.posture_max - 1.0;
    }
    start_enemy_combo(&mut app, 0);
    let mut pressed = false;
    for _ in 0..300u32 {
        if !pressed && time_to_enemy_hit(&mut app).is_some_and(|dt| dt <= 3.0 / TAE_FPS) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
            pressed = true;
        }
        app.update();
        if player_state(&mut app).0 == "ThrowBreak" {
            break;
        }
    }
    assert_eq!(enemy_state(&mut app).0, "ThrowDef12100");
    // The HUD reads GlobalTransforms (no propagation in the sim app).
    {
        let world = app.world_mut();
        let mut q = world.query::<(&Transform, &mut GlobalTransform)>();
        for (t, mut g) in q.iter_mut(world) {
            *g = GlobalTransform::from(*t);
        }
    }
    let enemy = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap()
    };
    let target = app
        .world_mut()
        .run_system_once(
            |combat: Res<Combat>, wolf: Query<(&Actor, &GlobalTransform), With<Player>>, enemies: crate::hud::MarkEnemies, dummies: Query<&GlobalTransform>| {
                let (wa, wg) = wolf.single().unwrap();
                crate::hud::deathblow_mark_target(&combat, wa, wg.translation(), &enemies, &dummies, None)
            },
        )
        .unwrap();
    assert_eq!(target, Some(enemy), "no mark on the break pose");
}

/// One Mind (6100: art variation 5005) fires its own BehaviorParam_PC rows' bullets (105005210 ->
/// Bullet 500510 ...), never the equipped prosthetic's: with the Loaded Umbrella (76000) on, its
/// a104_317000 judge 210 went through the umbrella's variation to 760010 "Iron fan unleashed slash".
#[test]
pub(crate) fn an_art_fires_its_own_bullets_not_the_prosthetics() {
    let mut app = app();
    {
        let world = app.world_mut();
        let mut config = world.resource_mut::<crate::config::GameConfig>();
        config.player.combat_art = 6100;
        config.player.prosthetics = vec![76000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = 10;
    }
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
        pad.attack_held = true;
        pad.guard_held = true;
    }
    let mut anims = Vec::<String>::new();
    let mut bullets = std::collections::BTreeSet::<String>::new();
    for i in 0..400 {
        if i == 150 {
            app.world_mut().resource_mut::<PadInput>().attack_held = false;
        }
        app.update();
        let a = player_state(&mut app).1;
        if anims.last() != Some(&a) {
            anims.push(a);
        }
        let world = app.world_mut();
        for n in world.query_filtered::<&Name, With<crate::prosthetic::Projectile>>().iter(world) {
            bullets.insert(n.as_str().to_string());
        }
    }
    assert!(anims.iter().any(|a| a == "a104_317000"), "{anims:?}");
    let combat = app.world().resource::<Combat>();
    // Its rows' bullets and their Bullet param children (HitBulletID / intervalCreateBulletId).
    let d = &combat.player;
    let mut own: Vec<i64> = (0..1000).filter_map(|j| d.behavior(5005, j)).map(|b| b.ref_id).collect();
    let mut i = 0;
    while i < own.len() {
        if let Some(r) = d.bullet_rows.get(&own[i].to_string()) {
            for c in [r.hit_bullet_id, r.interval_create_bullet_id] {
                if c > 0 && !own.contains(&c) {
                    own.push(c);
                }
            }
        }
        i += 1;
    }
    let own: Vec<String> = own.iter().map(|id| format!("Bullet {id}")).collect();
    assert!(bullets.iter().all(|b| own.contains(b)), "not One Mind's bullets: {bullets:?}");
}

/// The general stance (9920, ACTION_TYPE_GENERAL_STANCE; c1010): W_GeneralStanceStart401060 ->
/// GeneralStanceIdle, runs / walks front in it, W_GeneralStanceTrun_*, and its end clip when the
/// request stops; asked for the guard instead, it ends the general stance first.
#[test]
pub(crate) fn the_general_stance_runs_like_the_guard() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    let world = app.world_mut();
    let ent = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Actor, &Enemy)>();
        let (mut a, e) = q.get_mut(world, ent).unwrap();
        let d = combat.data_of(&a);
        let mut cmd = crate::ai::Command { general: true, ..Default::default() };
        a.procedural("IdleBattle");
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GeneralStanceStart401060");
        crate::enemy::guard_stance(&mut a, d, &cmd, true, true);
        assert_eq!(a.state, "GeneralStanceIdle");
        cmd.mv = Some('F');
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GeneralStanceRunFront");
        cmd.mv = None;
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GeneralStanceIdle");
        let yaw = a.yaw;
        assert!(crate::enemy::start_turn(e, &mut a, d, yaw - 3.0, false));
        assert_eq!(a.state, "GeneralStanceTrun_Right180");
        crate::enemy::guard_stance(&mut a, d, &cmd, true, true);
        assert_eq!(a.state, "GeneralStanceIdle");
        cmd = crate::ai::Command { guard: true, ..Default::default() };
        crate::enemy::guard_stance(&mut a, d, &cmd, false, true);
        assert_eq!(a.state, "GeneralStanceEnd601040", "the guard asked for: the general stance ends first");
    });
}

/// A respawn forgets a home its last life moved (a type 5 route's end, Patrol::new_home): the
/// fresh Brain's IsChangeInitialPosition is false, so POINT_INITIAL / the leash must not keep the
/// old pose (2026-10-12 bug hunt round 12).
#[test]
pub(crate) fn a_respawn_forgets_the_route_end_home() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    let marker = Vec3::new(50.0, 0.0, 50.0);
    {
        let world = app.world_mut();
        world.query::<&mut Enemy>().single_mut(world).unwrap().home = Some((marker, 1.0));
    }
    let (_, died) = break_and_deathblow_died(&mut app, 900);
    assert!(died, "the deathblow did not kill");
    for _ in 0..5 {
        app.update();
    }
    let world = app.world_mut();
    let e = world.query::<&Enemy>().single(world).unwrap();
    assert!(e.home.is_none_or(|h| h.0.distance(marker) > 1.0), "kept the old home {:?}", e.home);
}

/// An instant TAE recovery counts once when its start falls on a frame edge: the window is
/// (prev_t, t], plus the anim's very first frame (2026-10-12 bug hunt round 12).
#[test]
pub(crate) fn an_instant_recovery_on_a_frame_edge_counts_once() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    let combat = app.world().resource::<Combat>();
    let d = combat.enemy0();
    let mut keys: Vec<&String> = d.anims.keys().collect();
    keys.sort();
    let mut seen = 0;
    for key in keys {
        let whole = crate::enemy::instant_hp_rate(d, key, -1.0, f32::INFINITY);
        if whole <= 0.0 {
            continue;
        }
        for ev in d.anim(key).unwrap().events.iter().filter(|ev| matches!(ev.kind, 66 | 67)) {
            let s = ev.start;
            let split = crate::enemy::instant_hp_rate(d, key, -1.0, s) + crate::enemy::instant_hp_rate(d, key, s, f32::INFINITY);
            assert!((split - whole).abs() < 1e-3, "{key} at {s}: {split} split, {whole} whole");
            seen += 1;
        }
    }
    assert!(seen > 0, "no c1010 anim with an instant recovery");
}

/// A reset takes Wolf's prosthetic bullets in flight too, and stops their flying effects where
/// they were (as an ended bullet's, prosthetic.rs fly): a shuriken thrown just before the reset
/// hit the new line-up (2026-10-12 bug hunt round 12).
#[test]
pub(crate) fn a_reset_takes_wolfs_bullets_in_flight() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut flying = Vec::new();
    for _ in 0..120 {
        app.update();
        let world = app.world_mut();
        flying = world.query_filtered::<Entity, With<crate::prosthetic::Projectile>>().iter(world).collect();
        if !flying.is_empty() {
            break;
        }
    }
    assert!(!flying.is_empty(), "no shuriken thrown");
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    app.update();
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<crate::prosthetic::Projectile>>().iter(world).count(), 0, "a bullet survived the reset");
    for f in world.query::<&crate::fxr::FxEffect>().iter(world) {
        let on_bullet = matches!(f.anchor, Some(crate::fxr::Anchor::Entity(b)) if flying.contains(&b));
        assert!(!on_bullet, "a bullet's effect still rides on it (stop {})", f.stop);
    }
}

/// The combat arts' TAE 2 bullets are exported at each art level's variation, as their attacks
/// are: One Mind / Ashina Cross a104_317000 judge 210 -> BehaviorParam_PC 105005210 refType 1
/// (2026-10-12 bug hunt round 12: only the sword's v5000 was looked up, so none were found).
#[test]
pub(crate) fn the_combat_arts_bullets_are_exported() {
    let mut app = app();
    app.update();
    let combat = app.world().resource::<Combat>();
    for (var, j) in [(5004, 210), (5005, 210), (5030, 210)] {
        let key = format!("v{var}:{j}");
        let b = combat.player.bullets.get(&key).unwrap_or_else(|| panic!("no {key} bullet"));
        let row = combat.player.behavior(var, j).expect("behavior row");
        assert_eq!(b.bullet_id, row.ref_id, "{key}");
    }
}

/// Runs a deathblow kill, then the frames until the enemy is back (its in-place respawn); Wolf
/// steps 80 m away once it is dead, so the new life does not see him at once.
pub(crate) fn kill_and_respawn(app: &mut App, while_dead: impl Fn(&mut Enemy, &mut Actor)) {
    let (_, died) = break_and_deathblow_died(app, 200);
    assert!(died, "the deathblow did not kill");
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
    }
    for _ in 0..60 * 30 {
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        if !e.is_dead() {
            return;
        }
        while_dead(&mut e, &mut a);
    }
    panic!("no respawn");
}

/// A respawn comes back seen: its idle's opacity, not the death clip's TAE 193 fade to 0 (c1180
/// ThrowDefDeath12001 3.1-3.6 s; the respawn's procedural Approach has no anim to read it from)
/// (2026-10-12 bug hunt round 13).
#[test]
pub(crate) fn a_respawn_comes_back_visible() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    kill_and_respawn(&mut app, |_, a| a.opacity = 0.0);
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
    assert!(a.opacity > 0.99, "opacity {}", a.opacity);
}

/// A vanish cut short does not leave the boss invisible: the Owl's a000_003037 holds TAE 193 at
/// 0 from 0.27 to 0.7 s; a clip without a keyframe after it takes his idle's opacity (1). The
/// user (2026-10-16): the Owl and the Monk went invisible and could not be hit.
#[test]
pub(crate) fn a_vanish_cut_short_comes_back_visible() {
    let mut app = app_cfg("c5060", Some(50600000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query_filtered::<(Entity, &mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
    let (ent, mut e, mut a) = q.single_mut(world).unwrap();
    e.force_anim(&mut a, "a000_003037");
    for _ in 0..30 {
        app.update();
    }
    assert!(app.world().get::<Actor>(ent).unwrap().opacity < 0.01, "not vanished");
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
    let (mut e, mut a) = q.single_mut(world).unwrap();
    e.force_anim(&mut a, "a000_003000");
    for _ in 0..2 {
        app.update();
    }
    let a = app.world().get::<Actor>(ent).unwrap();
    assert!(a.opacity > 0.99, "still invisible in {}: {}", a.anim, a.opacity);
}

/// A respawn starts without the old life's timed SpEffects: the firecracker's cool time (107100,
/// 30 s: "used to it", no stagger) and the timed list ai_hold / sp_self read stayed (2026-10-12
/// bug hunt round 21).
#[test]
pub(crate) fn a_respawn_forgets_the_firecracker_cool_time() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    kill_and_respawn(&mut app, |e, _| {
        e.add_timed(107100, 30.0);
    });
    let world = app.world_mut();
    let e = world.query::<&Enemy>().single(world).unwrap();
    assert!(!e.clash.iter().any(|c| c.0 == 107100), "{:?}", e.clash);
    assert_eq!(e.guard_count(), 0);
}

/// A respawn faces its spawn way, and its home pose with it, not the way it died (a deathblow
/// from behind turned it 180 degrees) (2026-10-12 bug hunt round 13).
#[test]
pub(crate) fn a_respawn_faces_its_start() {
    let mut app = app();
    app.update();
    // Its spawn facing (the line-up's: toward Wolf's start; the test app turns it after).
    let start_yaw = {
        let world = app.world_mut();
        world.query::<&Enemy>().single(world).unwrap().start_yaw
    };
    for _ in 0..30 {
        app.update();
    }
    kill_and_respawn(&mut app, move |_, a| a.yaw = start_yaw + std::f32::consts::PI);
    let world = app.world_mut();
    let (e, a) = world.query::<(&Enemy, &Actor)>().single(world).unwrap();
    let off = |y: f32| ((y - start_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs();
    assert!(off(a.yaw) < 0.05, "yaw {} vs start {start_yaw}", a.yaw);
    assert!(e.home.is_some_and(|h| off(h.1) < 0.05), "home {:?} vs start {start_yaw}", e.home);
}

/// An MSB patroller's respawn starts its map life again: unaware of Wolf (patrol, NonBattleAct,
/// open to the stealth deathblows) and held by its map event's AI hold until it wakes; not still
/// hunting with the old life's battle target (2026-10-12 bug hunt round 13).
#[test]
pub(crate) fn a_patrollers_respawn_is_unaware_and_held_again() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let hold = spot.hold.clone().expect("1110236 has its map event's AI hold");
    let mut app = app_cfg("c1010", Some(10102010), vec![]);
    {
        let world = app.world_mut();
        let ent = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
        world.entity_mut(ent).insert((spot.route.clone(), hold, crate::enemy::StartUnaware));
    }
    for _ in 0..5 {
        app.update();
    }
    {
        let world = app.world_mut();
        let e = world.query::<&Enemy>().single(world).unwrap();
        assert!(e.is_unaware(), "starts unaware");
    }
    // The old life hunting Wolf (react's damaged(): BATTLE, seen, its forget time).
    kill_and_respawn(&mut app, |e, _| e.targeting.damaged(&serde_json::json!({ "SightTargetForgetTime": 60.0 })));
    {
        // On its MSB spot, not 2 m off it along world -Z (the line-up's rule; 2026-10-12 round 16).
        let world = app.world_mut();
        let (t, e) = world.query::<(&Transform, &Enemy)>().single(world).unwrap();
        let (at, spawned) = (t.translation, e.start);
        assert!((at - spawned).with_y(0.0).length() < 0.05, "respawned at {at}, its spot {spawned}");
    }
    // (Its targeting state updates on the next frames; Wolf is 80 m away.)
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    let (ent, e) = world.query::<(Entity, &Enemy)>().single(world).unwrap();
    assert!(e.is_unaware(), "came back hunting (state {})", e.targeting.state);
    assert!(world.get::<crate::enemy::AiHold>(ent).is_some(), "its AI hold is gone");
}

/// The rival snapshot (an NPC on a hostile team as the target) is BATTLE with a normal enemy
/// target, and a changed state only on the frames the rival comes or goes: the real targeting's
/// prev_state (NONE) made IsChangeState true every frame, so _COMMON_AddStateTransitionGoal
/// looped 401040 and never attacked (2026-10-12 bug hunt round 13).
#[test]
pub(crate) fn a_rival_target_changes_state_once() {
    use crate::stealth::{BATTLE, NONE, TYPE_NORMAL_ENEMY};
    let real = crate::stealth::Targeting::new(1);
    assert_eq!(real.state, NONE);
    let r = Some(Entity::PLACEHOLDER);
    let first = crate::enemy::rival_view(&real, r, None);
    assert_eq!((first.state, first.kind), (BATTLE, TYPE_NORMAL_ENEMY));
    assert!(first.changed(), "the rival's first frame");
    let next = crate::enemy::rival_view(&real, r, r);
    assert_eq!((next.state, next.kind), (BATTLE, TYPE_NORMAL_ENEMY));
    assert!(!next.changed(), "every frame after: no change");
    let gone = crate::enemy::rival_view(&real, None, r);
    assert_eq!(gone.state, NONE);
    assert!(gone.changed(), "back to the real target: a change");
    assert!(!crate::enemy::rival_view(&real, None, None).changed());
}


/// A hostile NPC's blow (combat.rs resolve's enemy-vs-enemy contacts) does not make Wolf the
/// battle target, nor wake a map AI hold; Wolf's does (2026-10-12 bug hunt round 14: c1010 team
/// 6 vs c1700 team 24 both turned on Wolf, unseen, at their first exchange).
#[test]
pub(crate) fn a_rivals_blow_does_not_turn_it_on_wolf() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.make_unaware(&mut a);
        e.react_from(&mut a, &combat, false, "DamageS_Front");
        e.targeting.update(combat.param("NpcThinkParam", combat.foe_of(&a).think_id), Vec3::ZERO, Vec3::NEG_Z, &crate::stealth::Seen::new(Vec3::new(0.0, 0.0, 200.0), 0.3, &[]), 1.0 / 60.0);
        assert!(e.is_unaware(), "a rival's blow: state {}", e.targeting.state);
        assert!(!e.hurt_by_wolf);
        e.react_from(&mut a, &combat, true, "DamageS_Front");
        e.targeting.update(combat.param("NpcThinkParam", combat.foe_of(&a).think_id), Vec3::ZERO, Vec3::NEG_Z, &crate::stealth::Seen::new(Vec3::new(0.0, 0.0, 200.0), 0.3, &[]), 1.0 / 60.0);
        assert_eq!(e.targeting.state, crate::stealth::BATTLE, "Wolf's blow");
        assert!(e.hurt_by_wolf);
    });
}

/// A new lock takes only an enemy in front (camera.rs acquirable: ChrFrontRange 5 m / 30 deg,
/// CamFrontNear 8 m / 60 x 30 deg, CamFrontFar 45 x 30 deg): one behind the camera, though
/// nearer, is not taken.
#[test]
pub(crate) fn a_new_lock_takes_an_enemy_in_front_not_the_nearest_behind() {
    let cam = GlobalTransform::from(Transform::from_xyz(0.0, 2.0, -4.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y));
    let (wolf, fwd) = (Vec3::ZERO, Vec3::Z);
    assert!(!crate::camera::acquirable(&cam, wolf, fwd, Vec3::new(0.0, 0.0, -6.0)), "behind the camera");
    assert!(crate::camera::acquirable(&cam, wolf, fwd, Vec3::new(0.0, 0.0, 10.0)), "ahead");
    // Off the camera's front but close in front of Wolf: ChrFrontRange.
    let side = GlobalTransform::from(Transform::from_xyz(-4.0, 2.0, 0.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y));
    assert!(crate::camera::acquirable(&side, wolf, fwd, Vec3::new(0.3, 0.0, 3.0)));
    assert!(!crate::camera::acquirable(&side, wolf, fwd, Vec3::new(-4.0, 0.0, -3.0)), "behind Wolf and off camera");
}

/// The switch's 30 deg up / down is from the camera's front, not the horizontal: a camera
/// looking 40 deg down at an enemy straight ahead can switch to it.
#[test]
pub(crate) fn the_switch_pitch_is_from_the_camera_front() {
    let target = Vec3::new(0.0, 0.0, 6.0);
    let dir = Vec3::new(0.0, -40f32.to_radians().sin(), 40f32.to_radians().cos());
    let cam = GlobalTransform::from(Transform::from_translation(target - dir * 6.0).looking_to(dir, Vec3::Y));
    assert!(crate::camera::switchable(&cam, target));
    assert!(!crate::camera::switchable(&cam, target + Vec3::Y * 6.0), "45 deg above the front");
}

/// Covert A (skill 60 -> 150000: sightSearchEnemyCut 20) works only crouched / wall-hugging /
/// hanging (its invocationConditionsStateChange1 949, the stateInfo of 109200 / 109210 / 109220 /
/// 109221): standing Wolf is seen at full distance (2026-10-12 bug hunt round 14).
#[test]
pub(crate) fn covert_a_cuts_sight_only_crouched() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.skills = vec![60];
    }
    app.update();
    let cut = |app: &mut App| {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        let combat = world.resource::<Combat>();
        let config = world.resource::<crate::config::GameConfig>();
        let skills = crate::combat::skill_sp_effect_rows(combat, config);
        assert!(skills.iter().any(|s| s.invocation_conditions_state_change1 == 949), "150000's condition");
        crate::enemy::wolf_seen(combat, a, Vec3::ZERO, &skills).cut
    };
    let standing = cut(&mut app);
    assert!((standing - 1.0).abs() < 1e-4, "standing: {standing}");
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..120 {
        app.update();
    }
    // Crouched: 109200's 20 % and Covert A's 20 %.
    let crouched = cut(&mut app);
    assert!((crouched - 0.64).abs() < 1e-4, "crouched: {crouched}");
}

/// A hold art lets go only in its TAE windows (c0000_transition.lua 6198-6209 / 6048-6058):
/// Ashina Cross tapped (attack up at once) still draws, but not before HoldStart's ref 222 opens
/// (a104_316500 f21); in the air (a104_316200 / 316220: 230 only) letting go of attack keeps the
/// hold, and only guard ends it.
#[test]
pub(crate) fn hold_arts_release_only_in_their_windows() {
    let mut ground = app();
    ground.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5500;
    ground.update();
    {
        let mut pad = ground.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
        pad.attack_held = true;
        pad.guard_held = true;
    }
    let (mut hold, mut draw) = (None, None);
    for i in 0..200 {
        if i == 3 {
            ground.world_mut().resource_mut::<PadInput>().attack_held = false;
        }
        ground.update();
        let s = player_state(&mut ground).0;
        if s == "GroundSpacialAttackHoldStart" && hold.is_none() {
            hold = Some(i);
        }
        if s == "GroundSpecialAttackCombo1" && draw.is_none() {
            draw = Some(i);
        }
    }
    let (hold, draw) = (hold.expect("no hold"), draw.expect("no draw"));
    assert!(draw - hold >= 40, "drew {} ticks into the hold (window from f21 = 42)", draw - hold);

    // The air hold (One Mind's 104 group): attack let go -> still holding.
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.combat_art = 6100;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = 10;
    }
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut seen = Vec::<String>::new();
    let (mut launch, mut held_from) = (None, None);
    for f in 0..200 {
        app.update();
        let (s, _, airborne, _) = player_actor(&mut app);
        if airborne && launch.is_none() {
            launch = Some(f);
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
            pad.attack_held = true;
            pad.guard_held = true;
        }
        if s == "AirSpecialAttackHoldStart" && held_from.is_none() {
            held_from = Some(f);
            app.world_mut().resource_mut::<PadInput>().attack_held = false;
        }
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(held_from.is_some(), "{seen:?}");
    assert!(!seen.iter().any(|s| s.ends_with("AirSpacialAttackHoldEnd")), "letting go of attack ended the air hold: {seen:?}");
}

/// Wolf's hits take the enemy's NpcParam per-attribute rates (2026-10-12 bug hunt round 14): the
/// dogs (c1150 / c1151), c1250 and c1260 have lightShootDamageRate 2.0 and
/// lightShootStaminaDmgRate 8.0, so a shuriken (atkAttribute / staminaPhysicsAttribute 9 Light
/// Shoot) does twice its HP damage and 8x its posture damage to c1260 12600000 (249 HP; the dogs
/// die of one) next to the plain servant c1180 11807100 (light shoot rates 1.0).
#[test]
pub(crate) fn a_shuriken_takes_the_light_shoot_rates() {
    let (hp0, p0) = shuriken_on("c1180", 11807100);
    let (hp1, p1) = shuriken_on("c1260", 12600000);
    assert!(hp0 > 0.0 && p0 > 0.0, "{hp0} {p0}");
    assert!((hp1 / hp0 - 2.0).abs() < 0.05 && (p1 / p0 - 8.0).abs() < 0.2, "c1180 {hp0} / {p0}, c1260 {hp1} / {p1}");
}

/// Every StaminaControlParam row an exported NpcParam names is exported (2026-10-12 bug hunt round
/// 14: only 0 / 1000000 / 1000100 / 1000200 were, so c1180's 1070000 regen ratios read as 100 %).
#[test]
pub(crate) fn the_enemies_posture_regen_rows_are_exported() {
    for (chr, row) in [("c1180", 1070000), ("c1060", 1060000)] {
        let mut app = app_cfg(chr, None, vec![]);
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        assert_eq!(a.stamina_ctrl_row, row, "{chr}");
        let combat = world.resource::<Combat>();
        let r = combat.param("StaminaControlParam", row);
        assert!(r.get("staminaRecoverRatio_forType000").is_some(), "{chr}: no StaminaControlParam {row}");
    }
}

/// The route-end anim event (m11_01 11115236 on the patroller 1110236): unaware and inside its
/// region (a 1.2 m sphere at the type 5 route's end), the enemy plays W_Event20004 once, then
/// goes on (no loop: the exe never reads the event's Should Loop byte); alerted first, the event
/// ends without it.
#[test]
pub(crate) fn a_patroller_plays_its_route_end_anim_once() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let re = spot.route_end.clone().expect("1110236 has its route-end anim event");
    assert_eq!((re.anim, re.region.shape), (20004, 2));
    for alerted in [false, true] {
        let mut app = app_cfg("c1010", Some(10102010), vec![]);
        let ent = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Player>>();
            q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
            let ent = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
            world.entity_mut(ent).insert(crate::enemy::StartUnaware);
            ent
        };
        app.update();
        if alerted {
            app.world_mut().get_mut::<Enemy>(ent).unwrap().targeting.damaged(&serde_json::json!({ "SightTargetForgetTime": 60.0 }));
            app.world_mut().get_mut::<Enemy>(ent).unwrap().targeting.state = crate::stealth::CAUTION;
        }
        // The region on its spot.
        let mut region = re.region.clone();
        region.at = app.world().get::<Transform>(ent).unwrap().translation - Vec3::Y * (CAPSULE_HALF_HEIGHT + 0.05);
        app.world_mut().entity_mut(ent).insert(crate::enemy::RouteEndAnim { region, anim: re.anim });
        let mut states = Vec::<String>::new();
        for _ in 0..60 * 8 {
            app.update();
            let s = app.world().get::<Actor>(ent).unwrap().state.clone();
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        let played = states.iter().filter(|s| *s == "Event20004").count();
        assert!(app.world().get::<crate::enemy::RouteEndAnim>(ent).is_none(), "the event is over");
        if alerted {
            assert_eq!(played, 0, "alerted: no anim {states:?}");
        } else {
            assert_eq!(played, 1, "once {states:?}");
            assert_ne!(states.last().map(String::as_str), Some("Event20004"), "it ends {states:?}");
        }
    }
}

/// MSB region shapes by SoulsFormats' ShapeType (1 circle, 2 sphere, 3 cylinder, 4 rectangle,
/// 5 box): the route-end regions are 2 = spheres (one radius), not cylinders (they were read as
/// a zero-height cylinder, so only a point at its exact height was inside).
#[test]
pub(crate) fn msb_region_shapes_follow_the_shape_type() {
    let r = |shape: i64, size: Vec<f32>| crate::enemy::WakeRegion { shape, at: Vec3::ZERO, x: Vec3::X, z: Vec3::Z, size };
    let sphere = r(2, vec![1.2]);
    assert!(sphere.contains(Vec3::new(0.0, 1.0, 0.0)) && sphere.contains(Vec3::new(0.0, -1.0, 0.0)) && !sphere.contains(Vec3::new(1.3, 0.0, 0.0)));
    let circle = r(1, vec![1.2]);
    assert!(circle.contains(Vec3::new(1.0, 50.0, 0.0)) && !circle.contains(Vec3::new(1.3, 0.0, 0.0)));
    let cylinder = r(3, vec![1.0, 2.0]);
    assert!(cylinder.contains(Vec3::new(0.5, 1.5, 0.0)) && !cylinder.contains(Vec3::new(0.5, 2.5, 0.0)) && !cylinder.contains(Vec3::new(0.5, -0.1, 0.0)));
    let rect = r(4, vec![2.0, 4.0]);
    assert!(rect.contains(Vec3::new(0.9, -30.0, 1.9)) && !rect.contains(Vec3::new(1.1, 0.0, 0.0)));
}

/// Super armour (sekiro-rs c0f97ee, docs/COMBAT-RULES.md; exe 84ff70 -> bfd980 -> 10cea20 ->
/// 10b5100): c1010's a000_003010 holds 5358 (spCategory 1001) for 0-0.87 s, whose dmgLv_S / M
/// enum 2 makes a small / middle hit minimum (8: no reaction, an additive flinch) and whose
/// dmgLv_L 5 keeps a large one large; after the window the level stands. It replaced the flag-73
/// proxy (every swing's active part shrugged every hit off).
#[test]
pub(crate) fn super_armour_swaps_the_damage_level_in_its_window() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        let d = combat.data_of(&a);
        a.play("Ez3010", "a000_003010");
        a.t = 0.3;
        assert_eq!(crate::combat::damage_motion_level(d, &a, 2), 8, "middle -> minimum");
        assert_eq!(crate::combat::damage_motion_level(d, &a, 1), 8, "small -> minimum");
        assert_eq!(crate::combat::damage_motion_level(d, &a, 3), 3, "large stays");
        assert_eq!(crate::combat::damage_motion_level(d, &a, 0), 0);
        a.t = 1.2;
        assert_eq!(crate::combat::damage_motion_level(d, &a, 2), 2, "after the window");
    });
}

/// A deflected or blocked bullet is repelled itself (and shows its sfxId_Flick): its distant
/// shooter takes no posture, recoil or bound reaction, even with an attacker-side posture base and reaction on the row
/// (sekiro-rs a8351b9 fight.rs:644, docs/PROJECTILE-GUARD.md; inferred from the rows).
#[test]
pub(crate) fn a_guarded_bullet_leaves_its_shooter_alone() {
    for deflect in [true, false] {
        let mut app = app_cfg("c1060", None, Vec::new());
        app.update();
        let (shooter, atk) = {
            let world = app.world_mut();
            let combat = world.resource::<Combat>();
            let mut atk = combat.kind(0).data.bullet_rows.values().find_map(|s| s.attack.clone().filter(|a| a.atk_phys > 0.0)).expect("a bullet attack");
            atk.repel_lost_stam_damage_attacker = 30.0;
            atk.repel_victory_stam_damage_attacker = 30.0;
            atk.just_deflected_action = 1;
            atk.deflected_action = 1;
            // Its Bullet sfxId_Hit / sfxId_Flick (test ids): the repel one plays.
            atk.bullet_fx = Some((900001, 900002, Vec3::Y, Vec3::NEG_Z));
            let pt = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let pyaw = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().yaw;
            let fwd = Vec3::new(-pyaw.sin(), 0.0, -pyaw.cos());
            let mut q = world.query_filtered::<(Entity, &mut Transform, &mut Actor, &mut Enemy), Without<crate::enemy::Cast>>();
            let (ent, mut t, mut a, mut e) = q.single_mut(world).unwrap();
            t.translation = pt + fwd * 8.0;
            a.posture = 0.0;
            e.make_unaware(&mut a);
            (ent, atk)
        };
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Guard);
            pad.guard_held = true;
        }
        // A deflect lands in the press's window; a block after it.
        for _ in 0..if deflect { 2 } else { 30 } {
            app.update();
        }
        let wolf = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
        app.world_mut().get_mut::<Actor>(shooter).unwrap().bullet_hits.push((wolf, atk.clone()));
        app.update();
        let log = log_text(&app);
        let want = if deflect { "DEFLECT" } else { "blocked" };
        assert!(log.contains(want), "deflect {deflect}: {log}");
        let a = app.world().get::<Actor>(shooter).unwrap();
        assert_eq!(a.posture, 0.0, "deflect {deflect}: the shooter took posture");
        assert!(!a.state.starts_with("AttackBound") && !a.state.starts_with("AttackNoBound"), "deflect {deflect}: {}", a.state);
        let ids: Vec<i64> = app.world_mut().query::<&crate::fxr::FxEffect>().iter(app.world()).map(|f| f.id).filter(|id| *id > 900000).collect();
        assert_eq!(ids, vec![900002], "deflect {deflect}");
    }
}

/// c9997 IsDamagedAerial: an enemy hit in the air plays its aerial reactions (c1060 has 8140 /
/// 8141 / 8150): middle / large / push / fling -> DamageAerialFront / Back, blow / upper / ex
/// blast -> DamageAerialBlow; small blow keeps DamageBlow (ExecDamageBlow comes first); a small
/// hit, or a chr with no aerial anims (c1010), has none (the additive flinch). On the ground the
/// usual table (sekiro-rs c0f97ee character.rs:1045-1074).
#[test]
pub(crate) fn an_enemy_hit_in_the_air_takes_the_aerial_reactions() {
    for (chr, aerial_anims) in [("c1060", true), ("c1010", false)] {
        let mut app = app_cfg(chr, None, Vec::new());
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
        let combat = world.resource::<Combat>();
        let d = &combat.kind(0).data;
        let at = Vec3::ZERO;
        let front = a.forward() * 2.0;
        let state = |level: i64, from: Vec3, aerial: bool| crate::combat::enemy_damage_state_air(d, level, &a, from, at, aerial);
        let s = |v: &str| Some(v.to_string());
        if aerial_anims {
            assert_eq!(state(2, front, true), s("DamageAerialFront"));
            assert_eq!(state(3, -front, true), s("DamageAerialBack"));
            assert_eq!(state(6, front, true), s("DamageAerialFront"));
            assert_eq!(state(4, front, true), s("DamageAerialBlow"));
            assert_eq!(state(9, -front, true), s("DamageAerialBlow"));
        } else {
            assert_eq!(state(2, front, true), None);
            assert_eq!(state(4, front, true), None);
        }
        assert_eq!(state(1, front, true), None, "{chr}: a small hit in the air");
        assert_eq!(state(7, front, true), s("DamageBlowFront"), "{chr}");
        assert_eq!(state(2, front, false), s("DamageMiddle_Front"), "{chr}: on the ground");
    }
}

/// A tool hit reacts as a sword hit (prosthetic.rs bullet_reaction = combat.rs resolve): an
/// enemy in the air takes c9997's aerial reaction (DamageAerialFront / Back, 8140 / 8141), not
/// the ground flinch.
#[test]
pub(crate) fn a_tool_hit_in_the_air_takes_the_aerial_reaction() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    for _ in 0..5 {
        app.update();
    }
    let react = |app: &mut App, airborne: bool| -> (Option<String>, bool) {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Actor, &Enemy, &Transform), With<Enemy>>();
            let (mut a, e, t) = q.single_mut(world).unwrap();
            a.airborne = airborne;
            let at = t.translation;
            let from = at + a.forward() * 3.0;
            let has = combat.data_of(&a).anim("a000_008140").is_some();
            (crate::prosthetic::bullet_reaction(&combat, &a, e, 2, from, at), has)
        })
    };
    let (ground, _) = react(&mut app, false);
    assert!(ground.as_deref().is_some_and(|s| s.starts_with("DamageMiddle")), "{ground:?}");
    let (air, has) = react(&mut app, true);
    if has {
        assert_eq!(air.as_deref(), Some("DamageAerialFront"));
    } else {
        assert_eq!(air, None, "no aerial anim: the additive flinch");
    }
}

/// R32: a guarded hit with no guard clip for its level goes on through ExecDamage (c9997.lua
/// 2252-2301): c1360 has GuardDamageSmall but no Large, so a large hit on its guard staggers it
/// (DamageLarge_*) instead of vanishing; guardLevel 0 (c1250) is the NONE column, a plain damage
/// reaction. ExecNoSyncAddDamage (2901) flinches minimum / aerial / rank-rejected hits only:
/// level 0 (super armour) has no flinch.
#[test]
pub(crate) fn a_guarded_hit_without_a_guard_clip_takes_its_damage_reaction() {
    use crate::combat::{GuardPick, enemy_guard_block, guard_fall_through, no_sync_add_flinches};
    for (chr, glv) in [("c1360", 4), ("c1250", 0)] {
        let mut app = app_cfg(chr, None, Vec::new());
        app.update();
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&Actor, &Transform), With<Enemy>>();
            let (a, t) = q.single(world).unwrap();
            let d = combat.data_of(a);
            assert_eq!(combat.npc(a)["guardLevel"].as_i64(), Some(glv), "{chr}");
            assert_eq!(enemy_guard_block(d, &[], 3, glv, false, false), GuardPick::None, "{chr}");
            let from = t.translation + a.forward() * 3.0;
            let (state, _) = guard_fall_through(d, a, 3, false, from, t.translation);
            assert_eq!(state.as_deref(), Some("DamageLarge_Front"), "{chr}");
            assert_eq!(guard_fall_through(d, a, 0, false, from, t.translation), (None, false), "{chr}: level 0 does nothing");
        });
    }
    assert!(no_sync_add_flinches(8, false, false) && no_sync_add_flinches(0, true, false) && no_sync_add_flinches(2, false, true));
    assert!(!no_sync_add_flinches(0, false, false), "super armour's level 0 does not flinch");
}

/// c9997 ExecGuardBlock picks the guard clip by refs, not by the deflect itself: c1180's shield
/// 400230 (stateInfo 158, no ref) deflects but plays GuardDamageSmall / Large (it has no
/// JustGuardDamage clip; the old pick played nothing); ref 1000090 (200220) plays
/// JustGuardDamage, 1000100 (200230) GuardDamageSpecial (c1010 a000_008502 on Common_Parry
/// a000_003102), 1000105 the additive flinch; a missing clip falls to the next one.
#[test]
pub(crate) fn an_enemys_guard_clip_follows_exec_guard_block() {
    use crate::combat::{GuardPick, enemy_guard_block};
    let st = |s: &str| GuardPick::State(s.to_string());
    let mut app = app_cfg("c1180", Some(11807400), Vec::new());
    app.update();
    let world = app.world_mut();
    let combat = world.resource::<Combat>();
    let d = &combat.kind(0).data;
    assert_eq!(d.sp_effects.get("400230").map(|s| (s.state_info, s.behavior_ref_id)), Some((158, 0)), "the shield deflects without a ref");
    assert_eq!(enemy_guard_block(d, &[], 2, 4, false, false), st("GuardDamageSmall_RighttoLeft"));
    assert_eq!(enemy_guard_block(d, &[], 3, 4, true, false), st("GuardDamageLarge_LefttoRight"));
    assert_eq!(enemy_guard_block(d, &[1000090], 2, 4, false, false), st("GuardDamageSmall_RighttoLeft"), "no JustGuardDamage clip: the next");
    let mut app = app_cfg("c1010", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let ent = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
    // Common_Parry a000_003102 carries 200230 (ref 1000100) at frames 0-30.
    let refs = world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Actor, &Enemy)>();
        let (mut a, e) = q.get_mut(world, ent).unwrap();
        a.play("Parry", "a000_003102");
        a.t = 0.2;
        crate::prosthetic::enemy_refs(&combat, &a, e)
    });
    assert!(refs.contains(&1000100), "{refs:?}");
    let combat = world.resource::<Combat>();
    let d = &combat.kind(0).data;
    assert_eq!(enemy_guard_block(d, &refs, 3, 4, false, false), st("GuardDamageSpecial_RighttoLeft"));
    assert_eq!(d.anim_key("GuardDamageSpecial_RighttoLeft"), Some("a000_008502"));
    assert_eq!(enemy_guard_block(d, &[1000090], 2, 4, false, false), st("JustGuardDamage_RighttoLeft"));
    assert_eq!(enemy_guard_block(d, &[1000090], 2, 4, true, false), st("JustGuardDanage_LefttoRight"));
    assert_eq!(enemy_guard_block(d, &[1000090, 1000100], 2, 4, false, false), st("JustGuardDamageSpecial_RighttoLeft"));
    assert_eq!(enemy_guard_block(d, &[1000105], 3, 4, false, false), GuardPick::Add);
    assert_eq!(enemy_guard_block(d, &[], 8, 4, false, false), GuardPick::Add);
    assert_eq!(enemy_guard_block(d, &[], 2, 4, false, false), st("GuardDamageSmall_RighttoLeft"));
}

/// c0000_transition.lua:1158: a hit on an already broken Wolf knocks him down only at push /
/// small / middle / large; minimum plays nothing; blow / fling / upper / ex blast / breath take
/// the normal reactions. A hit that breaks him again in StandDamageBreak knocks him down (:1615).
#[test]
pub(crate) fn a_broken_wolf_is_knocked_down_only_by_the_lighter_levels() {
    let mut app = app();
    app.update();
    let combat = app.world().resource::<Combat>();
    let mut a = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
    a.state = "StandDeflectBreak".into();
    a.posture = 0.0;
    let at = Vec3::ZERO;
    let from = a.forward();
    for level in [1, 2, 3, 5] {
        assert!(!crate::combat::broken_takes_normal_damage(level, &a));
        assert_eq!(crate::combat::player_break_state(combat, level, &a, from, at), "StandDamageBreakDamage", "level {level}");
    }
    assert!(!crate::combat::broken_takes_normal_damage(8, &a));
    assert_eq!(crate::combat::player_break_state(combat, 8, &a, from, at), "", "minimum");
    for level in [4, 6, 7, 9, 10, 11] {
        assert!(crate::combat::broken_takes_normal_damage(level, &a), "level {level}");
    }
    // Broken again in StandDamageBreak: any level.
    a.state = "StandDamageBreak_F".into();
    a.posture = a.posture_max;
    assert!(!crate::combat::broken_takes_normal_damage(4, &a));
    assert_eq!(crate::combat::player_break_state(combat, 4, &a, from, at), "StandDamageBreakDamage");
}

/// INTERUPT_UseItem fires for a crouched drink too (CrouchItemGourdDrink), not for the empty
/// gourd's shake (ItemGourdDrinkFailed: HKS acts it on the drink branch only; round 30).
#[test]
pub(crate) fn a_crouched_drink_raises_use_item() {
    let mut a = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
    a.prev_t = 0.0;
    a.t = 0.016;
    for (state, want) in [("ItemGourdDrink", true), ("CrouchItemGourdDrink", true), ("CrouchIdle", false), ("ItemGourdDrinkFailed", false)] {
        a.state = state.into();
        assert_eq!(crate::enemy::wolf_starts_an_item(&a), want, "{state}");
    }
}

/// c9997 ExecGuardBlock's EXLARGE (a blow / ex blast on a weak guard: guard_damage_table row 4
/// guardLevel 1, row 10 guardLevels 1-3) has no guard clip: ExecDamageLargeBlow plays
/// DamageLargeBlow, in the air DamageAerialBlow (c1060 has both). It played GuardDamageLarge.
#[test]
pub(crate) fn a_blow_on_a_weak_guard_launches_the_enemy() {
    use crate::combat::{GuardPick, enemy_guard_block};
    let mut app = app_cfg("c1060", None, Vec::new());
    app.update();
    let combat = app.world().resource::<Combat>();
    let d = &combat.kind(0).data;
    let st = |s: &str| GuardPick::State(s.to_string());
    assert_eq!(enemy_guard_block(d, &[], 4, 1, false, false), st("DamageLargeBlow"));
    assert_eq!(enemy_guard_block(d, &[], 10, 3, false, false), st("DamageLargeBlow"));
    assert_eq!(enemy_guard_block(d, &[], 4, 1, false, true), st("DamageAerialBlow"));
    // A strong guard keeps the guard clip.
    assert_eq!(enemy_guard_block(d, &[], 4, 4, false, false), st("GuardDamageLarge_RighttoLeft"));
}

/// The general stance's own attacks (c9997 ExecAIGeneralStance; 119000 / 132000 / 155060 battle
/// scripts ask AttackTunableSpin 603000-603002; c1190 / c1320 have them): out of the stance the
/// request starts it first (they played the attack at once, standing), in it they play as
/// GeneralStanceAttack<id>, count as attacks (flag-73 windows), and go back to the stance idle.
#[test]
pub(crate) fn the_general_stance_attacks_start_the_stance_first() {
    use crate::enemy::general_stance_play;
    let s = |v: &str| Some(v.to_string());
    assert_eq!(general_stance_play("Idle", false, 603000), s("GeneralStanceStart401060"));
    assert_eq!(general_stance_play("GeneralStanceStart401060", false, 603000), s("GeneralStanceStart401060"), "the start plays out");
    assert_eq!(general_stance_play("GeneralStanceStart401060", true, 603001), s("GeneralStanceAttack603001"));
    assert_eq!(general_stance_play("GeneralStanceIdle", false, 603002), s("GeneralStanceAttack603002"));
    assert_eq!(general_stance_play("GeneralStanceIdle", false, 601040), s("GeneralStanceEnd601040"));
    assert_eq!(general_stance_play("GeneralStanceIdle", false, 401060), s("GeneralStanceEnd601040"));
    assert_eq!(general_stance_play("Idle", false, 3000), None);
    let mut app = app_cfg("c1190", None, vec![]);
    app.update();
    let world = app.world_mut();
    let ent = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query::<(&mut Actor, &mut Enemy)>();
        let (mut a, mut e) = q.get_mut(world, ent).unwrap();
        let d = combat.data_of(&a);
        assert!(a.play_state(d, "GeneralStanceAttack603000"));
        e.cur_ez = Some(603000);
        assert!(e.is_attacking());
        let cmd = crate::ai::Command { general: true, ..Default::default() };
        crate::enemy::guard_stance(&mut a, d, &cmd, true, true);
        assert_eq!(a.state, "GeneralStanceIdle", "the attack ends in the stance");
    });
}

/// IsArrived runs before the region test (FUN_1405caa00 1405c.c:7455; FUN_1405b9070: distance <
/// the chr's radius + the goal's 0): a point whose region is a zero sphere (2 in the game) or a
/// point region (51) is still reached on top of it. It waited for the region forever before.
#[test]
pub(crate) fn a_patrol_point_with_a_zero_region_is_reached_by_is_arrived() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110248) else { return };
    assert!((spot.at - Vec3::new(-11.9, 0.03, -19.78)).length() < 0.1, "{}", spot.at);
    assert_eq!(spot.route.0.points.len(), 1);
    assert!(crate::enemy::msb_patrol_on("m11_01_00_00", 1110414).is_none(), "the General has no route");
    let spot = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236).expect("c1010_0046");
    let mut spot = spot;
    assert_eq!((spot.route.0.kind, spot.route.0.points.len()), (5, 3));
    let radii: Vec<f32> = spot.route.0.areas.iter().map(|a| a.as_ref().map_or(0.0, |a| a.size[0])).collect();
    assert_eq!(radii, [1.2, 1.2, 1.2], "the points' sphere regions");
    // The test arena is flat: the regions stand on its floor.
    for a in spot.route.0.areas.iter_mut().flatten() {
        a.at.y = 0.0;
        a.size[0] = 0.0;
    }
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: Some(10102010), at: None, msb: None };
    let mut app = app_cfg("c1010", Some(10102010), vec![g]);
    let points = spot.route.0.points.clone();
    let patroller = {
        let world = app.world_mut();
        // Wolf far behind the line-up, out of sight; the first enemy far away too.
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 80.0);
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        t.translation = Vec3::new(40.0, CAPSULE_HALF_HEIGHT, 40.0);
        a.yaw = 0.0;
        e.make_unaware(&mut a);
        let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
        t.translation = spot.at.with_y(CAPSULE_HALF_HEIGHT);
        a.yaw = spot.yaw;
        e.make_unaware(&mut a);
        world.entity_mut(ents[1]).insert(spot.route);
        ents[1]
    };
    let mut reached = Vec::new();
    for _ in 0..60 * 40 {
        app.update();
        let world = app.world_mut();
        let pos = world.get::<Transform>(patroller).unwrap().translation;
        if let Some((cur, prev)) = world.non_send::<crate::enemy::Brains>().map.get(&patroller).and_then(|b| b.patrol_points()) {
            if prev >= 0 && reached.last() != Some(&prev) {
                let d = (pos - points[prev as usize]).with_y(0.0).length();
                reached.push(prev);
                assert!(d < 0.9, "stepped at point {prev} {d:.2} m away (its radius)");
            }
            if reached == [0, 1, 2] {
                // A type 5 route ends at its last point: cur -1 (ai.rs Patrol, FUN_1405c2710).
                assert_eq!(cur, -1, "the route is over");
                break;
            }
        }
    }
    assert_eq!(reached, [0, 1, 2], "points reached in order");
}

/// The Owl (Father) talks before he draws (enemy/talk.rs, rec_20261016_074559): Wolf in his talk
/// pose, the Owl's AI off, his IdleUniqueDefault402 gesture at 3.2 s; at 10.1 s the talk's flag
/// 71000130 lets m10 11005910 turn his AI on and Wolf leaves the talk pose.
#[test]
pub(crate) fn the_owl_father_talks_before_the_fight() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5060.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5060", Some(50601010), Vec::new());
    let wolf_state = |app: &mut App| {
        let w = app.world_mut();
        let mut q = w.query_filtered::<&Actor, With<crate::player::Player>>();
        q.single(w).unwrap().state.clone()
    };
    let boss = |app: &mut App| {
        let w = app.world_mut();
        let b = boss_in_play(w);
        (w.get::<Enemy>(b).unwrap().ai_off, w.get::<Actor>(b).unwrap().anim.clone())
    };
    if !crate::enemy::talk::PLAY_TALKS {
        // Skipped: the Owl fights at once and Wolf never takes the talk pose.
        for _ in 0..120 {
            app.update();
            assert_ne!(wolf_state(&mut app), "Event700400");
        }
        assert!(!boss(&mut app).0, "the Owl's AI is still off with the talk skipped");
        return;
    }
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(wolf_state(&mut app), "Event700400");
    assert!(boss(&mut app).0, "the Owl's AI is on during the talk");
    for _ in 0..180 {
        app.update();
    }
    assert_eq!(wolf_state(&mut app), "Event700400", "Wolf left the talk pose");
    let (off, anim) = boss(&mut app);
    assert!(off && anim == "a000_000402", "no gesture: {anim}");
    for _ in 0..360 {
        app.update();
    }
    assert!(!boss(&mut app).0, "the Owl's AI stays off after the talk");
    assert_ne!(wolf_state(&mut app), "Event700400");
}
