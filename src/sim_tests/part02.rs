//! Sim tests, part 2: 45 tests, from `locked_slash_lunges_toward_a_far_target` to `a_jump_press_leaves_the_hold_art`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// Largest root-motion scale seen during a held Combo1, optionally locked on to the enemy
/// (2.4 m ahead, or 30 m away when `far`).
pub(crate) fn slash_root_scale(locked: bool, far: bool) -> f32 {
    let mut app = app();
    if far {
        move_enemy_far(&mut app);
    }
    if !app.world().contains_resource::<crate::camera::LockOn>() {
        app.world_mut().insert_resource(crate::camera::LockOn::default());
    }
    if locked {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<Enemy>>().single(world).unwrap();
        world.resource_mut::<crate::camera::LockOn>().target = Some(e);
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.attack_held = true;
    }
    let mut best = 0.0f32;
    for _ in 0..50 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.state == "GroundAttackCombo1" {
            best = best.max(a.root_scale);
        }
    }
    best
}

#[test]
pub(crate) fn locked_slash_lunges_toward_a_far_target() {
    // TAE 760 on Combo1 (frames 18-24): ReferenceDist 1.58, range 0.8-2.4. Wolf has already closed
    // ~0.5 m by frame 18, so ~1.87 m / 1.58.
    let locked = slash_root_scale(true, false);
    assert!(locked > 1.05 && locked <= 2.4 / 1.58 + 1e-3, "locked {locked}");
    // Unlocked: attack auto-homing picks the enemy inside LockCamParam's 3 m / 30 deg capture box.
    let homing = slash_root_scale(false, false);
    assert!((homing - locked).abs() < 1e-3, "homing {homing} vs locked {locked}");
    // Nothing in range: no target, no boost.
    let free = slash_root_scale(false, true);
    assert!((free - 1.0).abs() < 1e-4, "free {free}");
}


#[test]
pub(crate) fn a_deflect_facing_away_is_still_hit() {
    // Guard arc (ShieldBlock ArgB 90 = front half) gates deflects too: a perfect press with
    // the attacker behind does nothing.
    let out = guard_against_attack_facing(2.0 / TAE_FPS, 0, true);
    assert!(out.contains("hit:") && !out.contains("DEFLECT"), "{out}");
}

#[test]
pub(crate) fn hit_in_the_air_uses_the_air_reactions() {
    let mut app = app();
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    for _ in 0..20 {
        app.update();
    }
    assert!(player_actor(&mut app).2, "not airborne");
    let s = take_hit(&mut app, "a000_003000");
    let airborne = player_actor(&mut app).2;
    assert!(s.starts_with("AirDamage") || !airborne, "{s}\n{}", log_text(&app));
    assert!(s.starts_with("AirDamage"), "landed before the hit: {s}\n{}", log_text(&app));
}

/// Round 27: Wolf killed in a grab plays the grab clip on to its ChrActionFlag 69 (frame 70 of
/// a210_600000), then W_ThrowDefDeathStart's clip + 1 (a210_600001), then the revival wait - not
/// GroundDeathStart_F mid-throw (exe env 206 / 276; HKS 326-333).
#[test]
pub(crate) fn a_lethal_grab_plays_its_throw_death() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.play("Grabbed", "a210_600000");
        a.t = 60.0 / TAE_FPS;
        a.prev_t = a.t;
        a.hp = 0.0;
    }
    let mut seen = Vec::<String>::new();
    for _ in 0..60 * 4 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        let s = format!("{s} {anim}");
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(!seen.iter().any(|s| s.starts_with("GroundDeathStart_F")), "{seen:?}");
    let death = seen.iter().position(|s| s == "ThrowDefDeathStart a210_600001");
    assert!(death.is_some(), "{seen:?}");
    // Round 37: the throw's own loop (W_ThrowDefRevivalStayLoop600000: a210_610002), not the
    // ground one, and attack gets up by its W_ThrowDefRevival (a210_610003).
    assert!(seen[death.unwrap()..].iter().any(|s| s == "ThrowDefRevivalStayLoop a210_610002"), "{seen:?}");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..10 {
        app.update();
    }
    let (s, anim, _, _) = player_state(&mut app);
    assert_eq!((s.as_str(), anim.as_str()), ("GroundRevival_Throw", "a210_610003"));
}

/// Round 37: with no resurrection left a grab's death ends in its W_ThrowDefDeathLoop600000
/// (a210_610001), which is the death screen's loop too.
#[test]
pub(crate) fn a_final_grab_death_loops_in_the_throws_own_death_loop() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Player), With<Player>>();
        let (mut a, mut p) = q.single_mut(world).unwrap();
        p.resurrections = 0;
        a.play("Grabbed", "a210_600000");
        a.t = 60.0 / TAE_FPS;
        a.prev_t = a.t;
        a.hp = 0.0;
    }
    let mut seen = Vec::<String>::new();
    for _ in 0..60 * 4 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        let s = format!("{s} {anim}");
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert_eq!(seen.last().map(String::as_str), Some("ThrowDefDeathLoop a210_610001"), "{seen:?}");
}

/// Round 37: a grab with a blow-away end (c0000_transition 386-399) throws Wolf into its own
/// chain instead of idle: the Guardian Ape's 600380 -> ThrowDefLargeBlowStart600380 (a000_199231)
/// -> its landing (199251) -> StandDamageLargeDown face down; at HP 0 the Death chain (199261 ...)
/// whose landing ends in the face-down revival-wait loop. 600030 -> the generic
/// ThrowDefLargeBlowStart (199201), landing as StandDamageLargeBlowLand_B.
#[test]
pub(crate) fn grabs_with_a_blow_away_end_throw_wolf_down() {
    for (grab, hp, want) in [
        ("a230_600380", 100.0, ["ThrowDefLargeBlowStart600380", "ThrowDefLargeBlowLand600380", "StandDamageLargeDown_FaceDown"]),
        ("a230_600380", 0.0, ["ThrowDefDeathLargeBlowStart600380", "ThrowDefDeathLargeBlowLand600380", "GroundRevivalStayLoop_B"]),
        ("a213_600030", 100.0, ["ThrowDefLargeBlowStart", "StandDamageLargeBlowLand_B", "StandDamageLargeDown_FaceDown"]),
    ] {
        let mut app = app();
        move_enemy_far(&mut app);
        // The whole grab plays: the script's own grab end sets the landing's side
        // (Selector_ProneDirection, c0000_transition 386-395).
        let len = {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
                a.play("Grabbed", grab);
                a.hp = hp;
                combat.player.length(grab)
            })
        };
        let mut seen = Vec::<String>::new();
        for _ in 0..(60.0 * (len + 6.0)) as usize {
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        let mut at = 0;
        for w in want {
            match seen[at..].iter().position(|s| s == w) {
                Some(i) => at += i,
                None => panic!("{grab} hp {hp}: no {w} in order: {seen:?}"),
            }
        }
    }
}

/// Round 60: HP emptied during the Ape's non-lethal blow-away (a tick) lets it land, then
/// dies; before, "ThrowDef*" counted as a death state and Wolf stayed dead with no death.
/// Killed in the air by no hit (a status tick): W_FallDeathDmgStartNaturalDeath (HKS 813-817).
#[test]
pub(crate) fn a_tick_kill_in_the_ape_blow_away_still_dies() {
    assert_eq!(crate::player::death_starts(None, true, false, false, false), ["FallDeathDmgStartNaturalDeath"]);
    assert!(crate::player::death_starts(None, false, false, false, false).is_empty());
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            a.play("Grabbed", "a230_600380");
            a.t = (combat.player.length("a230_600380") - 0.02).max(0.0);
            a.prev_t = a.t;
            a.hp = 100.0;
        });
    }
    let mut seen = Vec::<String>::new();
    for i in 0..60 * 8 {
        app.update();
        let s = player_state(&mut app).0;
        if s == "ThrowDefLargeBlowStart600380" && i < 30 {
            app.world_mut().query_filtered::<&mut Actor, With<Player>>().single_mut(app.world_mut()).unwrap().hp = 0.0;
        }
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "ThrowDefLargeBlowStart600380"), "{seen:?}");
    assert!(seen.iter().any(|s| s.contains("RevivalStayLoop") || s.contains("DeathLoop")), "never died: {seen:?}");
}

/// Round 60: HP emptied during the revival anim starts the death again (HKS 5605), not after it.
#[test]
pub(crate) fn a_kill_during_the_revival_dies_at_once() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().query_filtered::<&mut Actor, With<Player>>().single_mut(app.world_mut()).unwrap().hp = 0.0;
    for _ in 0..300 {
        app.update();
        if player_state(&mut app).0 == "GroundRevivalStayLoop_F" {
            break;
        }
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..30 {
        app.update();
        if player_state(&mut app).0.starts_with("GroundRevival_") {
            break;
        }
    }
    assert!(player_state(&mut app).0.starts_with("GroundRevival_"), "{:?}", player_state(&mut app));
    app.update();
    app.world_mut().query_filtered::<&mut Actor, With<Player>>().single_mut(app.world_mut()).unwrap().hp = 0.0;
    app.update();
    app.update();
    let s = player_state(&mut app).0;
    assert!(s.starts_with("GroundDeathStart"), "{s}");
}

/// Round 60: a dead Wolf left above the floor (knocked off a ledge) falls to it.
#[test]
pub(crate) fn a_dead_wolf_above_the_floor_falls() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.update();
    let floor = {
        let world = app.world_mut();
        let (mut a, mut tf) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
        let floor = crate::map::ground_y(tf.translation) + crate::player::CAPSULE_HALF_HEIGHT;
        tf.translation.y = floor + 2.0;
        a.hp = 0.0;
        floor
    };
    for _ in 0..120 {
        app.update();
    }
    let y = app.world_mut().query_filtered::<&Transform, With<Player>>().single(app.world()).unwrap().translation.y;
    assert!((y - floor).abs() < 0.05, "the body hangs at {y} over the floor {floor}");
}

#[test]
pub(crate) fn resurrection_returns_wolf_at_half_hp() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    for _ in 0..300 {
        app.update();
        if player_state(&mut app).0 == "GroundRevivalStayLoop_F" {
            break;
        }
    }
    assert_eq!(player_state(&mut app).0, "GroundRevivalStayLoop_F");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..10 {
        app.update();
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!(a.state, "GroundRevival_F");
    // SpEffect 110015 changeHpRate -50: half of max HP (+1 from the revival kick-off).
    assert!((a.hp - (a.hp_max * 0.5 + 1.0)).abs() < 1.0, "hp {} / {}", a.hp, a.hp_max);
}

/// A small-blow posture break lands face down only from the back sector (HKS 1678-1684, the
/// 4-way damage_angle): hit from behind the shoulder (its _B clip) it is still face up. The
/// script picks the side from the hit (env 222), as combat.rs hands it over.
#[test]
pub(crate) fn a_break_small_blow_from_the_side_lands_face_up() {
    for (back_sector, want) in [(false, "StandDamageBreakDown_FaceUp"), (true, "StandDamageBreakDown_FaceDown")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "StandDamageBreakSmallBlow_B"));
                a.hit_from_back = true;
                a.hit_back_quadrant = back_sector;
                // DAMAGE_TYPE_DAMAGEBREAK, DAMAGE_LEVEL_SMALL_BLOW; DAMAGE_DIR_BACK or LEFT.
                a.damage_env = Some(crate::enemy::DamageEnv { kind: 1027, level: 7, dir: if back_sector { 3 } else { 0 }, back_half: true, ..crate::enemy::DamageEnv::NONE });
            });
        }
        let mut next = None;
        for _ in 0..400 {
            app.update();
            let s = player_state(&mut app).0;
            if s != "StandDamageBreakSmallBlow_B" {
                next = Some(s);
                break;
            }
        }
        assert_eq!(next.as_deref(), Some(want), "back sector {back_sector}");
    }
}

/// Killed from behind, Wolf dies face down (BEH_R_DEATH 974-983: DEATH_TYPE_COMMON_BACK):
/// Round 40: BEH_R_DEATH 785-951 - the killing hit's damage level / element picks Wolf's death:
/// EXLARGE on the ground the large blow-away (StandDamageDeathLargeBlowStart_F -> .. Land_F,
/// then the _B stay loop: IndexDeath B), SMALL_BLOW from behind StandDamageDeathSmallBlow_B,
/// lightning GroundDeathStartElectricShock, and a plain hit in the air the fall death
/// (FallDeathDmgStartNaturalDeath -> LandFallDeathDmgB -> LandRevivalStayLoop_B, whose revival
/// is LandRevival_B). Before, all took GroundDeathStart.
#[test]
pub(crate) fn a_killing_hit_picks_the_death_by_its_level() {
    let cases: [((i64, i64), bool, bool, &[&str], &str); 4] = [
        ((4, 0), false, false, &["StandDamageDeathLargeBlowStart_F", "StandDamageDeathLargeBlowLand_F"], "GroundRevivalStayLoop_B"),
        ((7, 0), false, true, &["StandDamageDeathSmallBlow_B"], "GroundRevivalStayLoop_B"),
        ((2, 6), false, false, &["GroundDeathStartElectricShock"], "GroundRevivalStayLoop_B"),
        ((0, 0), true, false, &["FallDeathDmgStartNaturalDeath", "LandFallDeathDmgB"], "LandRevivalStayLoop_B"),
    ];
    for (hit, air, back, want, stay) in cases {
        let mut app = app();
        move_enemy_far(&mut app);
        {
            let world = app.world_mut();
            let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
            a.hp = 0.0;
            a.death_hit = Some(hit);
            a.hit_back_quadrant = back;
            if air {
                a.airborne = true;
                a.vel_y = 0.0;
                t.translation.y += 3.0;
            }
        }
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..60 * 12 {
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s.clone());
            }
            if s == stay {
                break;
            }
        }
        for w in want {
            assert!(seen.iter().any(|s| s == w), "{hit:?}: no {w} in {seen:?}");
        }
        assert_eq!(seen.last().map(String::as_str), Some(stay), "{hit:?}: {seen:?}");
        if air {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            for _ in 0..10 {
                app.update();
            }
            assert_eq!(player_state(&mut app).0, "LandRevival_B");
        }
    }
}

/// GroundDeathStart_B, the _B stay loop, and the revival gets up from it (GroundRevival_B).
#[test]
pub(crate) fn a_death_from_behind_falls_face_down() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.hp = 0.0;
        a.hit_from_back = true;
    }
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..300 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s.clone());
        }
        if s == "GroundRevivalStayLoop_B" {
            break;
        }
    }
    assert!(seen.iter().any(|s| s == "GroundDeathStart_B"), "{seen:?}");
    assert_eq!(seen.last().map(String::as_str), Some("GroundRevivalStayLoop_B"), "{seen:?}");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "GroundRevival_B");
}

#[test]
pub(crate) fn ochimusha_runs_its_own_ai_and_attacks() {
    // c1010 with its decompiled 101000_battle.lua: an attack anim (a000_003xxx) within 10 s
    // that lands on (or is guarded by) the idle player.
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
    assert_eq!(app.world().resource::<Combat>().foe0().chr, "c1010");
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut attacked = None;
    for i in 0..900 {
        app.update();
        let (state, anim, _) = enemy_state(&mut app);
        if attacked.is_none() && state.starts_with("Ez") && anim.starts_with("a000_003") {
            attacked = Some((i, anim));
        }
        if log_text(&app).contains("enemy hit:") {
            break;
        }
    }
    let world = app.world_mut();
    let desc = world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().ai_desc.clone();
    assert!(attacked.is_some(), "no c1010 AI attack in 15 s; goals: {desc}; enemy={:?}", enemy_state(&mut app));
    // The real script ran (not the Rust fallback combos): its battle goal is in the chain.
    assert!(desc.contains("Ochimusha") || desc.contains("101000"), "goals: {desc}");
    assert!(log_text(&app).contains("enemy hit:"), "attack never landed: {attacked:?}\n{}", log_text(&app));
}

#[test]
pub(crate) fn ochimusha_deathblow_uses_its_own_throw_rows() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
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
    // Start throw a200_500000 first, then the main throw.
    for _ in 0..60 {
        app.update();
        if player_state(&mut app).0 == "Deathblow" {
            break;
        }
    }
    let (es, _, _) = enemy_state(&mut app);
    let (ps, pa, _, _) = player_state(&mut app);
    // ThrowParam 11010001 (崩し本体（遠） PC -> 落武者): atkAnimOffset 200 -> player a200_510000
    // (the General's rows use a201), enemy ThrowDef 12000.
    assert_eq!((ps.as_str(), pa.as_str()), ("Deathblow", "a200_510000"), "{}", log_text(&app));
    assert!(es.starts_with("ThrowDef"), "{es}");
}

/// The castle Owl's (50600000, think 50601000 -> 506001) deflected warp slash (a000_003035,
/// judge 220: BehaviorParam 250601220 category 7, justDeflectedAction 2) bounds into
/// AttackBoundEnemy2_Left a000_008603 (c9997 ExecAttackJustBound on damage type 1009), whose TAE
/// 200226 makes Kengeki_Activate pick Kengeki09 / 10: the 3090 sweep (perilous) or 3091. Before,
/// every bound was Enemy1 (200200/1) and the Owl's perilous never came (0 % in the traces).
#[test]
pub(crate) fn the_owls_deflected_warp_slash_bounds_into_its_sweep() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5060.json")).exists() {
        return;
    }
    assert_eq!(crate::data::behavior_category(250601220), 7);
    assert_eq!(crate::data::behavior_category(250601500), 6);
    let mut app = app_with("c5060");
    for _ in 0..30 {
        app.update();
    }
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let bt = world.get::<Transform>(boss).unwrap().translation;
        let at = wolf + (bt - wolf).with_y(0.0).normalize_or_zero() * 2.0;
        world.get_mut::<Transform>(boss).unwrap().translation = at;
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().yaw = crate::combat::yaw_to(wolf, at);
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.get_mut(world, boss).unwrap();
        a.yaw = crate::combat::yaw_to(at, wolf);
        e.aggressive = true;
        e.force_anim(&mut a, "a000_003035");
    }
    let mut pressed = false;
    let mut bound = None;
    for f in 0..240 {
        if let Some(dt) = time_to_enemy_hit(&mut app) {
            if !pressed && dt <= 2.0 / TAE_FPS {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.press(Action::Guard);
                pad.guard_held = true;
                pressed = true;
            }
        }
        app.update();
        let world = app.world_mut();
        let boss = boss_in_play(world);
        let s = world.get::<Actor>(boss).unwrap().state.clone();
        if std::env::var("OWL_DBG").is_ok() {
            let (ws, wa, wt, _) = player_state(&mut app);
            let (es, ea, et) = enemy_state(&mut app);
            eprintln!("f{f} wolf {ws} {wa} {wt:.2} | boss {es} {ea} {et:.2} | hit in {:?}", time_to_enemy_hit(&mut app));
        }
        if s.starts_with("AttackBound") {
            bound = Some((f, s));
            break;
        }
    }
    let out = log_text(&app);
    assert!(out.contains("YOU DEFLECT"), "{out}");
    let (f0, state) = bound.expect("a bound reaction");
    assert_eq!(state, "AttackBoundEnemy2_Left", "{out}");
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        let e = world.get::<Enemy>(boss).unwrap();
        assert!(!e.clash.iter().any(|c| c.0 == 200200 || c.0 == 200201), "the engine row hides the clip's 200226: {:?}", e.clash);
    }
    let mut next = None;
    for f in f0..(f0 + 60 * 5) {
        app.update();
        let world = app.world_mut();
        let boss = boss_in_play(world);
        let a = world.get::<Actor>(boss).unwrap();
        if std::env::var("OWL_DBG").is_ok() && (f - f0) % 6 == 0 {
            let e = world.get::<Enemy>(boss).unwrap();
            eprintln!("f{f} boss {} {} {:.2} | {} | clash {:?}", a.state, a.anim, a.t, e.ai_desc, e.clash);
        }
        if a.anim == "a000_003090" || a.anim == "a000_003091" {
            next = Some(a.anim.clone());
            break;
        }
    }
    assert!(next.is_some(), "no Kengeki09 / 10 answer after the special bound:\n{}", log_text(&app));
}

#[test]
pub(crate) fn ochimusha_attack_can_be_deflected() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
    }
    let mut pressed = false;
    for _ in 0..240 {
        if let Some(dt) = time_to_enemy_hit(&mut app) {
            if !pressed && dt <= 2.0 / TAE_FPS {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.press(Action::Guard);
                pad.guard_held = true;
                pressed = true;
            }
        }
        app.update();
        if ["DEFLECT", "blocked", "hit:"].iter().any(|k| log_text(&app).contains(k)) {
            break;
        }
    }
    let (es, _, _) = enemy_state(&mut app);
    assert!(log_text(&app).contains("YOU DEFLECT"), "{}\nenemy={es}", log_text(&app));
}

#[test]
#[ignore]
pub(crate) fn ochimusha_trace() {
    // cargo test ochimusha_trace -- --ignored --nocapture
    let mut app = app_with("c1010");
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut goals = std::collections::BTreeSet::new();
    for _ in 0..2400 {
        app.update();
        let (s, an, _) = enemy_state(&mut app);
        if seen.last().map(|l| l.0 != s).unwrap_or(true) {
            seen.push((s, an));
        }
        let world = app.world_mut();
        let d = world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().ai_desc.clone();
        goals.insert(d);
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 320.0;
    }
    let missing: Vec<_> = seen.iter().filter(|(_, a)| a.is_empty()).map(|(s, _)| s.clone()).collect();
    println!("states: {:?}", seen.iter().map(|(s, a)| format!("{s}({a})")).collect::<Vec<_>>());
    println!("goal chains: {}", goals.len());
    for g in goals.iter().take(25) {
        println!("  {g}");
    }
    println!("procedural/missing: {missing:?}");
}

#[test]
pub(crate) fn ochimusha_guards_the_first_player_attack() {
    // c1010 passive: Common_Parry(50, 25, 0, 3102), guard count 0 -> guard (3100).
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        if ["enemy blocked", "ENEMY DEFLECT", "you hit"].iter().any(|k| log_text(&app).contains(k)) {
            break;
        }
    }
    let out = log_text(&app);
    assert!(out.contains("enemy blocked"), "{out}\nenemy={:?}", enemy_state(&mut app));
}

#[test]
pub(crate) fn air_slashes_chain_and_landing_continues_the_slash() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    // Live game (rec_20261008_041541, attack mashed): slashes start 8, 28 and 47 frames after
    // the launch, then LandAirComboAttack3 (a050_308070) on touchdown.
    let (mut launch, mut seen, mut starts) = (None, Vec::<String>::new(), Vec::new());
    for i in 0..300 {
        let (state, _, airborne, _) = player_actor(&mut app);
        if airborne && launch.is_none() {
            launch = Some(i);
        }
        if let Some(l) = launch {
            if i >= l + 6 && (i - l) % 3 == 0 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            if seen.last() != Some(&state) && state.starts_with("AirComboAttack") {
                starts.push(i - l);
            }
        }
        if seen.last() != Some(&state) {
            seen.push(state.clone());
        }
        if state.starts_with("LandAirComboAttack") {
            break;
        }
        app.update();
    }
    assert_eq!(seen.last().map(String::as_str), Some("LandAirComboAttack3"), "{seen:?}");
    for (got, live) in starts.iter().zip([8, 28, 47]) {
        // Earliest possible here; the live player's presses (and the engine's ~1 frame input delay) add a little.
        assert!(*got as i32 <= live && live - *got as i32 <= 4, "slash starts {starts:?} vs live 8/28/47");
    }
}

#[test]
pub(crate) fn attack_while_holding_guard_is_the_combat_art() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    for _ in 0..60 {
        app.update();
    }
    assert!(player_state(&mut app).0.starts_with("DeflectGuard"), "{:?}", player_state(&mut app));
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::<String>::new();
    for _ in 0..20 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "GroundSpecialAttackCombo1"), "{seen:?}");
    assert!(!seen.iter().any(|s| s == "DeflectGuardAttack"), "{seen:?}");
}

#[test]
pub(crate) fn guard_and_attack_without_an_art_still_slashes() {
    // Round 54: with no art equipped the pair became a CombatArt request nothing took (HKS 6193
    // needs env(345) ~= SP_ATK_TYPE_NONE), and the press was lost.
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 0;
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    for _ in 0..60 {
        app.update();
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::<String>::new();
    for _ in 0..20 {
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s.contains("Attack")), "the press was lost: {seen:?}");
    assert!(!seen.iter().any(|s| s.contains("SpecialAttack")), "{seen:?}");
}

#[test]
pub(crate) fn deathblow_camera_closes_in_then_returns() {
    // a200_510000 TAE 153: 4.0 -> 3.0 -> 1.5 (frames 26-47) -> 4.0 (SlowStart, frames 47-60).
    let mut app = app();
    let combat = app.world_mut().remove_resource::<crate::data::Combat>().unwrap();
    let mut a = Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0);
    a.play("Deathblow", "a200_510000");
    let mut cam = crate::camera::CamState::for_tests(&combat);
    let dt = 1.0 / 60.0;
    let (mut min, mut at_end) = (f32::MAX, 0.0);
    for i in 0..(62 * 2) {
        a.t = i as f32 * dt;
        crate::camera::update_cam_distance(&combat, &a, &mut cam, 4.5, 0.05, dt);
        min = min.min(cam.dist);
        at_end = cam.dist;
    }
    assert!(min < 2.6, "closest {min}");
    assert!(at_end > 3.0, "after the slow return {at_end}");
}

#[test]
pub(crate) fn tae151_look_limits_clamp_camera_angles() {
    // TAE 151 CameraLookAtTarget (e.g. a200_510000, dmy 142): LookUp/DownLimit bound pitch and
    // LookLeft/RightLimit bound yaw around the player -> dummy direction (exe FUN_140742230).
    // Dummy straight ahead (-Z), pitch -40..70, yaw unrestricted; rate 1 = instant chase.
    let ahead = Vec3::new(0.0, 0.0, -1.0);
    let mut look = None;
    let (yaw, pitch) = crate::camera::apply_look_limits(1.2, 100f32.to_radians(), &mut look, ahead, [-40.0, 70.0, -180.0, 180.0], 1.0, 1.0);
    assert!((pitch.to_degrees() - 70.0).abs() < 0.01, "pitch {pitch}");
    assert!((yaw - 1.2).abs() < 0.01, "yaw {yaw}");
    // A narrow yaw cone clamps a sideways camera into it.
    let mut look = None;
    let (yaw, _) = crate::camera::apply_look_limits(90f32.to_radians(), 0.0, &mut look, ahead, [-40.0, 70.0, -45.0, 45.0], 1.0, 1.0);
    assert!((yaw.to_degrees() - 45.0).abs() < 0.01, "yaw {yaw}");
    // A dummy above the player pulls the centre pitch to -atan2(1, 1) = -45 deg, so limits
    // -10..10 clamp a level camera down to -35 deg.
    let mut look = None;
    let (_, pitch) = crate::camera::apply_look_limits(0.0, 0.0, &mut look, Vec3::new(0.0, 1.0, -1.0), [-10.0, 10.0, -180.0, 180.0], 1.0, 1.0);
    assert!((pitch.to_degrees() + 35.0).abs() < 0.01, "pitch {pitch}");
}

#[test]
pub(crate) fn releasing_guard_while_moving_plays_the_move_run_end() {
    // BEH_A_DEFLECT_GUARD_END: a non-zero stick on release picks DeflectGuardToStandMoveRun
    // (the Walk variant when walk is held, the standing end when locked on).
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    for _ in 0..40 {
        app.update();
    }
    // Move the stick and let go of guard on the same frame.
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, -1.0);
        pad.guard_held = false;
    }
    let mut states = Vec::<String>::new();
    for _ in 0..30 {
        app.update();
        let s = player_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s == "DeflectGuardToStandMoveRun"), "{states:?}");
}

/// Stopping in the guard's run-out (W_StandMoveableAction) goes on in the standing end at the
/// run-out's time (StartTime_01 = env 3063 1), so it ends soon after: pushing the stick again
/// after that is a plain W_StandMoveStart, as in the recordings (rec 082459 @22340-22362), not
/// a second run-out (W_StandMoveLowerStart) from a standing end that had restarted at 0.
#[test]
pub(crate) fn a_stop_late_in_the_guard_run_out_ends_with_its_clip() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    for _ in 0..40 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    app.world_mut().resource_mut::<PadInput>().guard_held = false;
    let mut log = Vec::<String>::new();
    for i in 0..90 {
        // Let go 0.6 s into the run-out (40 ticks long), push again 0.4 s later.
        app.world_mut().resource_mut::<PadInput>().stick = if (36..60).contains(&i) { Vec2::ZERO } else { Vec2::new(0.0, -1.0) };
        app.update();
        let s = player_state(&mut app).0;
        let ev = app.world().get_non_send::<crate::hks_wolf::WolfScript>().map_or(String::new(), |w| w.fired.join(";"));
        if !ev.is_empty() {
            log.push(format!("{i} {s} {ev}"));
        }
    }
    assert!(log.iter().any(|l| l.contains("W_StandMoveableAction")), "{log:?}");
    let after: Vec<_> = log.iter().skip_while(|l| !l.contains("W_StandMoveableAction")).skip(1).collect();
    assert!(after.first().is_some_and(|l| l.contains("W_StandMoveStart")), "{log:?}");
}

/// Runs a sprint and presses attack the first frame the sprint start is up, steering `stick`.
pub(crate) fn sprint_attack_states(stick: Vec2) -> Vec<String> {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.dodge_held = true;
        pad.stick = Vec2::new(0.0, -1.0);
        pad.press(Action::Step);
    }
    let mut states = Vec::<String>::new();
    let mut pressed = false;
    for _ in 0..240 {
        let s = player_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s.clone());
        }
        if !pressed && s == "SprintStartFromStep_F" {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = stick;
            pad.press(Action::Attack);
            pressed = true;
        }
        app.update();
    }
    states
}

#[test]
pub(crate) fn sprint_attack_selector_follows_the_hks_attack_angle() {
    // c0000_transition.lua:3348: Selector_GroundJumpType = 0 (_L) when
    // (AttackAngle <= 0 and SPRINT_START_FROM_STEP) or (AttackAngle > 0 and not
    // SPRINT_START_FROM_STEP), else 1 (_R); attack_route mirrors this as
    // `left = if from_step { !angle_right } else { angle_right }`.
    // This test drives the SPRINT_START_FROM_STEP branch; attacks out of SprintLoop (ref 1 opens
    // every action, player.rs sprint_window) are covered by attack_out_of_a_sprint_loop_is_a_sprint_attack.
    // Steering left from the sprint start (AttackAngle <= 0) -> selector 0 = SprintAttack_L.
    let left = sprint_attack_states(Vec2::new(1.0, 0.0));
    assert!(left.iter().any(|s| s == "SprintAttack_L"), "{left:?}");
    // Steering right -> selector 1 = SprintAttack_R.
    let right = sprint_attack_states(Vec2::new(-1.0, 0.0));
    assert!(right.iter().any(|s| s == "SprintAttack_R"), "{right:?}");
}

#[test]
pub(crate) fn minimum_level_guarded_hit_keeps_wolf_state() {
    // No c1020 attack is DAMAGE_LEVEL_MINIMUM (8), so force the combo's rows to level 8:
    // combat.rs additive_guard must skip the guard reaction and keep Wolf's current state.
    let mut app = app();
    {
        let mut combat = app.world_mut().resource_mut::<Combat>();
        for atk in combat.kinds[0].data.attacks.values_mut() {
            atk.dmg_level = 8;
            atk.dmg_level_vs_player = 0;
            atk.repel_lost_stam_damage = 0.0; // keep the block from breaking posture
        }
    }
    start_enemy_combo(&mut app, 0);
    let mut pressed = false;
    for _ in 0..240 {
        if !pressed {
            if let Some(dt) = time_to_enemy_hit(&mut app) {
                if dt <= 10.0 / TAE_FPS {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.press(Action::Guard);
                    pad.guard_held = true;
                    pressed = true;
                }
            }
        }
        app.update();
        if log_text(&app).contains("blocked") || log_text(&app).contains("DEFLECT") {
            break;
        }
    }
    let out = log_text(&app);
    let (state, _, _, _) = player_state(&mut app);
    assert!(out.contains("blocked") || out.contains("DEFLECT"), "{out}");
    assert!(!state.starts_with("StandDeflect") && !state.starts_with("AirDeflect"), "level-8 guard must be additive, state {state}\n{out}");
}

/// Puts Wolf into `state` at TAE frame `frame` (30 fps) with the enemy out of the way.
pub(crate) fn force_player(app: &mut App, state: &str, frame: f32) {
    move_enemy_far(app);
    let combat = app.world_mut().remove_resource::<crate::data::Combat>().unwrap();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        assert!(a.play_state(&combat.player, state), "{state}");
        a.t = frame / TAE_FPS;
        a.prev_t = a.t;
    }
    app.world_mut().insert_resource(combat);
}

#[test]
pub(crate) fn guard_during_a_block_recoil_adds_a_deflect_window() {
    // Hard deflect (STYLE_TYPE_STAND): ref 228 -> BEH_A_DEFLECT_GUARD_START's additive
    // AddHardDeflectGuard, the recoil keeps playing.
    let mut app = self::app();
    app.update();
    force_player(&mut app, "StandDeflectHardSmall_F", 2.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.update();
    app.update();
    {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert_eq!(a.state, "StandDeflectHardSmall_F");
        assert!(!a.add_anim.is_empty(), "no additive deflect layer");
    }
    // Round 61: the easy (block) recoil is STYLE_TYPE_GROUND_GUARD, where START never runs:
    // BEH_A_DEFLECT_GUARD_CONTINUE takes ref 227 -> StandToDeflectGuard1Reverse, no layer -
    // and from a tap already let go (CONTINUE needs no hold).
    let mut app = self::app();
    app.update();
    force_player(&mut app, "StandDeflectEasySmall_V1_F", 2.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.world_mut().resource_mut::<PadInput>().guard_held = false;
    app.update();
    app.update();
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!(a.state, "StandToDeflectGuard1Reverse");
    assert!(a.add_anim.is_empty(), "layer {}", a.add_anim);
}

/// Round 61 (HKS 2968-2970): guard in an air easy recoil's ref 228 adds AddHardDeflectGuard and
/// the recoil plays on, as on the ground.
#[test]
pub(crate) fn guard_in_an_air_recoil_adds_a_deflect_window() {
    let mut app = app();
    app.update();
    force_player(&mut app, "AirDeflectEasySmall_L", 2.0);
    {
        let world = app.world_mut();
        let (mut a, mut tf) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
        a.airborne = true;
        tf.translation.y += 3.0;
    }
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.unlock_all_moves = true;
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.world_mut().resource_mut::<PadInput>().guard_held = true;
    app.update();
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!(a.state, "AirDeflectEasySmall_L");
    assert!(!a.add_anim.is_empty(), "no additive deflect layer");
}

/// Round 61: a press in SprintToDeflectGuard (guard style) is CONTINUE, never the sprint deflect
/// again; LandAirDeflectGuard's ref 212 steps the spam chain; letting go in an easy recoil's
/// ref 207 ends the guard (DeflectGuardToStand).
#[test]
pub(crate) fn guard_style_presses_continue_the_guard() {
    let mut app = self::app();
    app.update();
    force_player(&mut app, "SprintToDeflectGuard", 8.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.world_mut().resource_mut::<PadInput>().guard_held = true;
    app.update();
    let s = player_state(&mut app).0;
    assert!(s.starts_with("StandToDeflectGuard"), "{s}");
    let mut app = self::app();
    app.update();
    force_player(&mut app, "LandAirDeflectGuard", 10.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.world_mut().resource_mut::<PadInput>().guard_held = true;
    app.update();
    assert_eq!(player_state(&mut app).0, "StandToDeflectGuard2");
    let mut app = self::app();
    app.update();
    force_player(&mut app, "StandDeflectEasySmall_V1_F", 20.0);
    app.world_mut().resource_mut::<PadInput>().guard_held = false;
    app.update();
    assert_eq!(player_state(&mut app).0, "DeflectGuardToStand");
}

#[test]
pub(crate) fn guard_cancels_a_middle_hit_reaction() {
    // Ref 503 (StandDamageMiddle frames 12-24) -> StandDeflectGuardFromDamage_<dir>.
    let mut app = app();
    app.update();
    force_player(&mut app, "StandDamageMiddle_L", 14.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    let mut seen = String::new();
    for _ in 0..4 {
        app.update();
        seen = player_state(&mut app).0;
        if seen.starts_with("StandDeflectGuardFromDamage") {
            break;
        }
    }
    assert_eq!(seen, "StandDeflectGuardFromDamage_L");
}

#[test]
pub(crate) fn attack_out_of_a_sprint_loop_is_a_sprint_attack() {
    // SprintLoop has no ChrActionFlags; ref 1 opens the actions (HKS SP_EF_REF_ENABLE_SPRINT_ACTION).
    let mut app = app();
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        pad.dodge_held = true;
    }
    force_player(&mut app, "SprintLoop", 3.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::<String>::new();
    for _ in 0..6 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s.starts_with("SprintAttack_")), "{seen:?}");
}

/// Round 26: the sliding sprint deflect only with ref 1 up (c0000_transition.lua 4443-4458):
/// out of SprintLoop it is SprintToDeflectGuard; out of a sprint attack's recovery (a050_302011,
/// guard accepted at frames 33-68, no ref 1) the plain StandToDeflectGuard.
#[test]
pub(crate) fn only_a_running_sprint_gives_the_sliding_deflect() {
    let deflect_from = |state: &str, frame: f32, run: bool| {
        let mut app = app();
        app.update();
        if run {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = Vec2::new(0.0, 1.0);
            pad.dodge_held = true;
        }
        force_player(&mut app, state, frame);
        app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
        let mut seen = Vec::<String>::new();
        for _ in 0..4 {
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        seen
    };
    let run = deflect_from("SprintLoop", 3.0, true);
    assert!(run.iter().any(|s| s == "SprintToDeflectGuard"), "{run:?}");
    let attack = deflect_from("SprintAttack_R", 45.0, false);
    assert!(attack.iter().any(|s| s == "StandToDeflectGuard"), "{attack:?}");
    assert!(!attack.iter().any(|s| s == "SprintToDeflectGuard"), "{attack:?}");
}

/// Round 26: a jump out of a sprint (ref 1) is W_SprintJumpReady a000_201200 -> SprintJumpStart
/// a000_201210, launched by ChrPhysicsVelocityChangeParam 121 (9.15 m/s forward, 9.64 up), not
/// the plain forward jump's row 101 (6.4 m/s); c0000_transition.lua 2784-2789.
#[test]
pub(crate) fn a_sprint_jump_is_the_fast_long_jump() {
    let mut app = app();
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        pad.dodge_held = true;
    }
    // Into the sprint by a forward step, as in play: the landing's Selector_GroundJumpType is
    // the step's (a sprint jump does not set it, HKS _SetJumpDirection is the ground jump's).
    app.world_mut().resource_mut::<PadInput>().press(Action::Step);
    for _ in 0..120 {
        app.update();
        if player_state(&mut app).0 == "SprintLoop" {
            break;
        }
    }
    assert_eq!(player_state(&mut app).0, "SprintLoop");
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut seen = Vec::<String>::new();
    let mut fastest = 0.0f32;
    for _ in 0..40 {
        app.update();
        let s = player_state(&mut app).0;
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        fastest = fastest.max(a.air_base.with_y(0.0).length());
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "SprintJumpReady") && seen.iter().any(|s| s == "SprintJumpStart"), "{seen:?}");
    assert!((fastest - 9.15).abs() < 0.3, "launch {fastest} m/s {seen:?}");
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::ZERO;
        pad.dodge_held = false;
    }
    for _ in 0..240 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "LandForwardGroundJump"), "{seen:?}");
    assert_eq!(seen.last().map(String::as_str), Some("StandIdle"), "{seen:?}");
}

#[test]
pub(crate) fn deathblow_from_behind_uses_the_back_throw() {
    // ThrowParam 11020111 (崩し背後本体): player a201_511200 (imports a201_510200's clip),
    // enemy ThrowDefDeath13201, picked when Wolf stands at the broken enemy's back.
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, true, 4.0);
            t.translation.z = 1.8;
            // Facing away from Wolf (who stands at the origin): forward = +Z.
            a.yaw = std::f32::consts::PI;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    // Start throw 崩し背後始動 (11020110: a201_501200 = a200_500200's clip, judge 640 at frame 7).
    app.update();
    assert_eq!(player_state(&mut app).1, "a201_501200");
    for _ in 0..30 {
        app.update();
        if player_state(&mut app).0 == "Deathblow" {
            break;
        }
    }
    let (es, ea, _) = enemy_state(&mut app);
    let (ps, pa, _, _) = player_state(&mut app);
    assert_eq!((ps.as_str(), pa.as_str()), ("Deathblow", "a201_511200"), "{}", log_text(&app));
    assert_eq!(es, "ThrowDef13200", "{ea}");
    // Round 37: the start throw hands `behind` on to the main one, so the defender keeps his back
    // to Wolf (he used to be turned round to face him).
    let wolf = app.world_mut().query_filtered::<&Transform, With<Player>>().single(app.world()).unwrap().translation;
    let mut q = app.world_mut().query_filtered::<(&Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
    let (a, t) = q.single(app.world()).unwrap();
    let at = t.translation;
    let to_wolf = (wolf - at).with_y(0.0).normalize();
    assert!(a.forward().dot(to_wolf) < 0.0, "yaw {} faces Wolf", a.yaw);
}

#[test]
pub(crate) fn plunging_onto_a_broken_enemy_is_a_deathblow() {
    // ThrowParam 崩し落下0 (11020150) on the way down, 崩し落下1 (11020151) on landing.
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 8.0);
            t.translation.z = -1.0;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut pressed, mut states) = (false, Vec::<String>::new());
    for _ in 0..200 {
        app.update();
        let (s, vy, airborne, _) = player_actor(&mut app);
        if airborne && vy < -2.0 && !pressed {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            pressed = true;
        }
        if states.last() != Some(&s) {
            states.push(s.clone());
        }
        if s == "Deathblow" {
            break;
        }
    }
    assert!(states.iter().any(|s| s == "PlungeDeathblow"), "{states:?}\n{}", log_text(&app));
    let (ps, pa, _, _) = player_state(&mut app);
    assert_eq!((ps.as_str(), pa.as_str()), ("Deathblow", "a201_511410"), "{states:?}");
    assert_eq!(enemy_state(&mut app).0, "ThrowDef13410");
}

#[test]
pub(crate) fn equipped_combat_art_ichimonji_plays_with_its_own_attack() {
    // config player.combat_art 5300 = Ichimonji: EquipParamWeapon spAtkcategory 102 -> a102_316000,
    // its judges resolve through behavior variation 5003 ("a102:<judge>").
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5300;
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut seen = false;
    for _ in 0..10 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        seen |= s == "GroundSpecialAttackCombo1" && anim == "a102_316000";
    }
    assert!(seen, "{:?}", player_state(&mut app));
    let combat = app.world().resource::<Combat>();
    let w = combat.player.attack_windows("a102_316000");
    assert!(!w.is_empty(), "no Ichimonji hit windows");
    // Resolved through the equipped art's variation (5300 -> 5003), same row as the group's base.
    let art = combat.player.attacks.get("v5003:200").expect("v5003:200 exported");
    assert!(w.iter().any(|(_, a, _)| std::ptr::eq(*a, art)), "uses the art's own AtkParam");
}

/// The enemy's (anim, posture).
pub(crate) fn enemy_anim_posture(app: &mut App) -> (String, f32) {
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    (a.anim.clone(), a.posture)
}

/// Round 28: SpEffect staminaRecoverChangeSpeed adds to the posture regen base (kb/enemy.md,
/// FUN_140a04850): c1010's battle idle a100_400000 wears 300240 (+40/s) on frames 0-123, so with
/// staminaRecoverBaseVel 20 it recovers 60/s there, not 20.
#[test]
pub(crate) fn the_battle_idles_change_speed_adds_to_posture_regen() {
    let mut app = app_cfg("c1010", Some(10100000), Vec::new());
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    move_enemy_far(&mut app);
    app.update();
    let base = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Enemy), Without<crate::enemy::Cast>>();
        let (mut a, mut e) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a100_400000");
        a.posture = a.posture_max * 0.8;
        a.since_posture_damage = 10.0;
        a.posture_regen
    };
    let before = enemy_anim_posture(&mut app).1;
    for _ in 0..30 {
        app.update();
    }
    let (anim, posture) = enemy_anim_posture(&mut app);
    let drop = before - posture;
    assert_eq!(anim, "a100_400000");
    assert!(drop >= (base + 40.0) * 0.5 * 0.85, "base {base}/s, recovered {drop} in 0.5 s");
}

/// Round 51: 300240 (effectEndurance 0.1) is both in the TAE window and in sp_chained over its
/// first 0.1 s; it counts once there (+40/s), not twice (+80/s).
#[test]
pub(crate) fn a_tae_speffect_counts_once_in_posture_regen() {
    let mut app = app_cfg("c1010", Some(10100000), Vec::new());
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    move_enemy_far(&mut app);
    app.update();
    let base = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Enemy), Without<crate::enemy::Cast>>();
        let (mut a, mut e) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a100_400000");
        a.posture = a.posture_max * 0.8;
        a.since_posture_damage = 10.0;
        a.posture_regen
    };
    let before = enemy_anim_posture(&mut app).1;
    for _ in 0..6 {
        app.update();
    }
    let drop = before - enemy_anim_posture(&mut app).1;
    assert!(drop <= (base + 40.0) * 0.1 * 1.2 + 1.0, "base {base}/s, recovered {drop} in 0.1 s");
}

/// Round 28: a combat art's TAE 307 PCBehavior windows resolve to BehaviorParam_PC's own pc<judge>
/// rows (no damage), not the art's attack of the same judge: the air Mortal Draw's contact
/// windows (a106_316200 judge 210) dealt v5007:210's slash every 15 frames.
#[test]
pub(crate) fn an_air_arts_contact_windows_deal_nothing() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5700;
    app.update();
    let combat = app.world().resource::<Combat>();
    let w = combat.player.attack_windows("a106_316200");
    let contact: Vec<_> = w.iter().filter(|(e, _, _)| e.kind == 307).collect();
    assert!(!contact.is_empty(), "no 307 windows in a106_316200");
    for (e, atk, _) in contact {
        assert!(atk.atk_stam == 0.0 && atk.atk_phys_correction == 0.0 && atk.atk_mag_correction == 0.0, "307 at {} hits as an attack", e.start);
    }
}

/// Round 28: a grab with an escape meter (c1500's 4120 restraint, ThrowParam 21500200: escHp 10,
/// selfEscCycleCnt 1, selfEscCycleTime 100) is broken by presses: 10 windows with a press empty
/// it, and the grabber's next flag-70 pulse plays ThrowEscape (a223_600372) and lets the zombie go
/// into its ThrowRelease4122 (c9997 env 275; it held on in the 4120 loop). Without presses Wolf
/// stays held.
#[test]
pub(crate) fn mashing_breaks_the_zombie_restraint() {
    let run = |mash: bool| {
        let mut app = app_cfg("c1500", None, Vec::new());
        {
            let world = app.world_mut();
            let combat = world.remove_resource::<Combat>().unwrap();
            {
                let mut q = world.query_filtered::<(&mut Actor, &mut Enemy, &mut Transform), Without<crate::enemy::Cast>>();
                let (mut ea, mut e, mut et) = q.single_mut(world).unwrap();
                e.force_anim(&mut ea, "a100_004120");
                ea.state = "ThrowAtk4120".into();
                et.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.8);
            }
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            a.play("Grabbed", "a223_600370");
            world.insert_resource(combat);
        }
        let mut seen = Vec::<String>::new();
        for i in 0..60 * 6 {
            if mash && i % 4 == 0 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let (s, anim, _, _) = player_state(&mut app);
            let s = format!("{s} {anim}");
            if seen.last() != Some(&s) {
                seen.push(s);
            }
            let es = format!("enemy {}", enemy_state(&mut app).0);
            if !seen.contains(&es) {
                seen.push(es);
            }
        }
        seen
    };
    let mashed = run(true);
    assert!(mashed.iter().any(|s| s == "ThrowEscape a223_600372"), "{mashed:?}");
    assert!(mashed.iter().any(|s| s == "enemy ThrowRelease4122"), "{mashed:?}");
    let held = run(false);
    assert!(!held.iter().any(|s| s.starts_with("ThrowEscape")), "{held:?}");
}

#[test]
pub(crate) fn ochimusha_plunge_deathblow_uses_its_a200_pair() {
    // c1010: ThrowParam 11010150/151, Wolf's a200 group (atkAnimOffset 200).
    let mut app = app_with("c1010");
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 8.0);
            t.translation.z = -1.0;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut pressed = false;
    for _ in 0..200 {
        app.update();
        let (s, vy, airborne, _) = player_actor(&mut app);
        if airborne && vy < -2.0 && !pressed {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            pressed = true;
        }
        if s == "Deathblow" {
            break;
        }
    }
    let (ps, pa, _, _) = player_state(&mut app);
    assert_eq!((ps.as_str(), pa.as_str()), ("Deathblow", "a200_511410"), "{}", log_text(&app));
    assert_eq!(enemy_state(&mut app).0, "ThrowDef13410");
}

/// Equips art `weapon`, starts it, presses it again inside its combo window (ref 223,
/// SpEffect 100252) and returns every (state, anim) seen.
pub(crate) fn art_chain(weapon: i64, presses: usize) -> Vec<(String, String)> {
    art_chain_by(weapon, presses, false)
}

/// As `art_chain`; `attack_only`: the follow-ups are the attack button alone.
pub(crate) fn art_chain_by(weapon: i64, presses: usize, attack_only: bool) -> Vec<(String, String)> {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = weapon;
    let press = |app: &mut App| {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    };
    press(&mut app);
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut done = 0;
    for _ in 0..900 {
        app.update();
        let (s, anim, t, _) = player_state(&mut app);
        if seen.last() != Some(&(s.clone(), anim.clone())) {
            seen.push((s.clone(), anim.clone()));
        }
        let in_window = app.world().resource::<Combat>().player.has_ref(&anim, t, crate::player::REF_ENABLE_SP_ATK_COMBO);
        let pressed_here = seen.len();
        if in_window && done < presses && seen.len() == pressed_here {
            if attack_only {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            } else {
                press(&mut app);
            }
            done += 1;
            // One press per state.
            for _ in 0..40 {
                app.update();
                let (s2, a2, _, _) = player_state(&mut app);
                if s2 != s || a2 != anim {
                    break;
                }
            }
        }
    }
    seen
}

#[test]
pub(crate) fn ichimonji_double_follows_up_but_ichimonji_does_not() {
    // HKS _FireSpAttackCombo: spAtkType 102 needs SP_EF_REF_WEP_SP_ATK_UNLOCK_102_COMBO (281),
    // the resident SpEffect 140201 of Ichimonji: Double (7100). Combo2 = a102_316010.
    let double = art_chain(7100, 1);
    assert!(double.iter().any(|(s, a)| s == "GroundSpecialAttackCombo2" && a == "a102_316010"), "{double:?}");
    let single = art_chain(5300, 1);
    assert!(!single.iter().any(|(s, _)| s == "GroundSpecialAttackCombo2"), "{single:?}");
    // Round 62: the attack button alone continues it too (BEH_A_GROUND_ATTACK tries
    // _FireSpAttackCombo first, HKS 3256), not a plain slash.
    let by_attack = art_chain_by(7100, 1, true);
    assert!(by_attack.iter().any(|(s, a)| s == "GroundSpecialAttackCombo2" && a == "a102_316010"), "{by_attack:?}");
    let single = art_chain_by(5300, 1, true);
    assert!(!single.iter().any(|(s, _)| s == "GroundSpecialAttackCombo2"), "{single:?}");
}

#[test]
pub(crate) fn floating_passage_chains_its_strikes() {
    // spAtkcategory 105 (5600): Combo1 -> 2 -> (no unlock 282) VariationCombo3 a105_317020;
    // the upgraded art (7200, unlock 282) goes 2 -> 3 -> 4 -> 5.
    let base = art_chain(5600, 4);
    let states: Vec<&str> = base.iter().map(|(s, _)| s.as_str()).collect();
    assert!(states.contains(&"GroundSpecialAttackCombo2") && states.contains(&"GroundSpacialAttackVariationCombo3"), "{base:?}");
    let up = art_chain(7200, 4);
    assert!(up.iter().any(|(s, a)| s == "GroundSpecialAttackCombo5" && a == "a105_316040"), "{up:?}");
}

#[test]
pub(crate) fn art_out_of_a_sprint_and_nightjar_steps() {
    // HKS BEH_A_GROUND_SP_ATTACK: in the sprint window (ref 1) -> SprintSpecialAttack (a102_316300
    // for Ichimonji 5300); spAtkType 101 (5200) -> GroundSpecialAttackStep_F a101_316001, and
    // forward even with the stick back unless the 101 back-attack unlock (280, art 7000) is up.
    let mut sprint = app();
    sprint.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5300;
    sprint.update();
    {
        let mut pad = sprint.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        pad.dodge_held = true;
    }
    force_player(&mut sprint, "SprintLoop", 3.0);
    {
        let mut pad = sprint.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut seen = Vec::<(String, String)>::new();
    for _ in 0..6 {
        sprint.update();
        let (s, anim, _, _) = player_state(&mut sprint);
        seen.push((s, anim));
    }
    assert!(seen.iter().any(|(s, a)| s == "SprintSpecialAttack" && a == "a102_316300"), "{seen:?}");

    for (weapon, want) in [(5200, "GroundSpecialAttackStep_F"), (7000, "GroundSpecialAttackStep_B")] {
        let mut app = app();
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = weapon;
        app.update();
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            // Stick away from Wolf's facing (he faces the camera at spawn: stick up = behind him).
            pad.stick = Vec2::new(0.0, 1.0);
            pad.press(Action::Attack);
            pad.press(Action::Guard);
        }
        let mut states = Vec::new();
        for _ in 0..6 {
            app.update();
            states.push(player_state(&mut app).0);
        }
        assert!(states.iter().any(|s| s == want), "{weapon}: {states:?}");
    }
}

/// Round 63 (g_behaviorValidateOrder: GROUND_JUMP before SP_ATTACK_RELEASE): One Mind's
/// HoldStart (a104_316500) opens jump (flag 119) at frame 12; a jump press there jumps instead
/// of being eaten by the hold.
#[test]
pub(crate) fn a_jump_press_leaves_the_hold_art() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5500;
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
        pad.attack_held = true;
        pad.guard_held = true;
    }
    let mut at = None;
    for _ in 0..60 {
        app.update();
        let (s, _, t, _) = player_state(&mut app);
        if s == "GroundSpacialAttackHoldStart" && t >= 14.0 / TAE_FPS {
            at = Some(t);
            break;
        }
    }
    assert!(at.is_some(), "no HoldStart past frame 14: {:?}", player_state(&mut app));
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut seen = Vec::<String>::new();
    for _ in 0..10 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s.contains("Jump")), "{seen:?}");
}
