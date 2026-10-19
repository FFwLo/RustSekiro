//! Sim tests, part 3: 44 tests, from `hold_art_sheathes_and_releases` to `a_feigning_wolf_is_dropped_by_the_enemy`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

#[test]
pub(crate) fn hold_art_sheathes_and_releases() {
    // spAtkType 104 (5500): HoldStart a104_316500 -> HoldLoop a104_316510 while attack + guard are
    // held; releasing attack draws (GroundSpecialAttackCombo1 a104_316000), or with the 287 unlock
    // (6100) HoldAction a104_317000; releasing guard ends the hold (HoldEnd a104_316520).
    for (weapon, release_attack, want) in [
        (5500, true, ("GroundSpecialAttackCombo1", "a104_316000")),
        (6100, true, ("GroundSpacialAttackHoldAction", "a104_317000")),
        (5500, false, ("GroundSpacialAttackHoldEnd", "a104_316520")),
    ] {
        let mut app = app();
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = weapon;
        app.update();
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
            pad.attack_held = true;
            pad.guard_held = true;
        }
        let mut seen = Vec::<(String, String)>::new();
        for i in 0..240 {
            if i == 150 {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                if release_attack { pad.attack_held = false } else { pad.guard_held = false }
            }
            app.update();
            let (s, anim, _, _) = player_state(&mut app);
            if seen.last() != Some(&(s.clone(), anim.clone())) {
                seen.push((s, anim));
            }
        }
        assert!(seen.iter().any(|(s, _)| s == "GroundSpacialAttackHoldLoop"), "{weapon}: {seen:?}");
        assert!(seen.iter().any(|(s, a)| s == want.0 && a == want.1), "{weapon}: {seen:?}");
    }
}

#[test]
pub(crate) fn guard_tapped_before_the_window_is_dropped_but_held_guard_fires() {
    // Live recording 2026-10-08 (rec_windows.py): out of GroundStep_N (guard flag 117 from frame 12)
    // a guard tap let go at frame 4 never deflected; a press still held when the window opens does.
    for (hold, want) in [(false, false), (true, true)] {
        let mut app = app();
        app.update();
        force_player(&mut app, "GroundStep_N", 1.0);
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Guard);
            pad.guard_held = true;
        }
        let mut deflected = false;
        for i in 0..40 {
            if i == 4 && !hold {
                app.world_mut().resource_mut::<PadInput>().guard_held = false;
            }
            app.update();
            deflected |= player_state(&mut app).0.starts_with("StandToDeflectGuard");
        }
        assert_eq!(deflected, want, "held {hold}: {:?}", player_state(&mut app));
    }
}

#[test]
pub(crate) fn guard_tap_in_a_hard_deflect_recoil_becomes_a_guard_at_ref_412() {
    // Live recording 2026-10-08: StandDeflectHardSmall_R, guard tapped at f2 and let go at f4 ->
    // additive deflect (a000_299060, ref 411 f0-9) -> StandToDeflectGuard when the base's ref 412
    // (f9-10) comes up (HKS line 6161), at f9.4 in the real game.
    let mut app = app();
    app.update();
    force_player(&mut app, "StandDeflectHardSmall_R", 2.0);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Guard);
        pad.guard_held = true;
    }
    let mut seen = Vec::new();
    for i in 0..30 {
        if i == 4 {
            app.world_mut().resource_mut::<PadInput>().guard_held = false;
        }
        app.update();
        let (s, _, t, _) = player_state(&mut app);
        if seen.last().map(|x: &(String, f32)| &x.0) != Some(&s) {
            seen.push((s, t * crate::data::TAE_FPS));
        }
    }
    assert!(seen.iter().any(|(s, _)| s.starts_with("StandToDeflectGuard")), "{seen:?}");
}

#[test]
pub(crate) fn letting_go_of_attack_in_an_art_releases_it() {
    // Ref 222 SP_EF_REF_TAE_ENABLE_SP_ATK_RELEASE (a106_316000 f15-27): attack let go -> Combo1Release
    // a106_316100 (art 5700, spAtkcategory 106). Live: Combo2 let go at f2-3 -> Combo2Release at f6.5.
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5700;
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
        pad.attack_held = true;
        pad.guard_held = true;
    }
    let mut seen = Vec::<(String, String)>::new();
    for i in 0..40 {
        if i == 3 {
            app.world_mut().resource_mut::<PadInput>().attack_held = false;
        }
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        if seen.last() != Some(&(s.clone(), anim.clone())) {
            seen.push((s, anim));
        }
    }
    assert!(seen.iter().any(|(s, a)| s == "GroundSpecialAttackCombo1Release" && a == "a106_316100"), "{seen:?}");
}

/// Art `art` started with attack and guard held throughout: the states it went through.
pub(crate) fn art_held_states(art: i64) -> Vec<(String, String)> {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = art;
    app.update();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
        pad.attack_held = true;
        pad.guard_held = true;
    }
    let mut seen = Vec::<(String, String)>::new();
    for _ in 0..60 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        if seen.last() != Some(&(s.clone(), anim.clone())) {
            seen.push((s, anim));
        }
    }
    seen
}

/// Base Mortal Draw (5700, no ref 283) is always released at ref 222, attack held or not; the
/// Empowered one (7300, resident 140601 -> 283) holds Combo1 (HKS BEH_A_GROUND_SP_ATTACK_RELEASE;
/// round 24: the base art held into the upgrade's moveset).
#[test]
pub(crate) fn base_mortal_draw_is_released_even_when_held() {
    let base = art_held_states(5700);
    assert!(base.iter().any(|(s, _)| s == "GroundSpecialAttackCombo1Release"), "{base:?}");
    let empowered = art_held_states(7300);
    assert!(!empowered.iter().any(|(s, _)| s == "GroundSpecialAttackCombo1Release"), "{empowered:?}");
}

/// Ichimonji's slash adds SpEffect 140210 by TAE 67 (changeStaminaRate 12.5): Wolf's posture drops
/// by 12.5 % of max (it was never applied; round 24).
#[test]
pub(crate) fn ichimonji_recovers_posture() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = 5300;
    app.update();
    let max = {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.posture = a.posture_max * 0.5;
        a.posture_max
    };
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut low = f32::MAX;
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        low = low.min(a.posture);
    }
    // Natural regen lowers it too: look for the refresh itself (12.5 % of max at once).
    let want = format!("refresh: -{:.0} posture", max * 0.125);
    assert!(log_text(&app).contains(&want), "no {want} (posture fell to {low} of {max})
{}", log_text(&app));
}

#[test]
pub(crate) fn flowing_water_cuts_guarded_posture_damage_by_attack_attribute() {
    // Live 2026-10-08: Samurai General slash attacks (staminaPhysicsAttribute 1) cost x0.785 posture on
    // deflect / block, c1180 / c1470 strikes (2) x0.74 = SkillParam 280 "Flowing Water" -> SpEffect 150420
    // (block family, stateInfo 158) / 150421 (deflect family, 204): defSlashStaminaDmgRate 0.8, others
    // 0.75, after the sword's 1 % guard cut, truncated (guard::guarded_posture).
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut config = app.world().resource::<crate::config::GameConfig>().clone();
    let row = crate::guard::GuardRow::of(true, 5000, 90);
    assert!(row.is_some(), "Wolf's guard row 105000090 -> 5000090");
    let posture = |config: &crate::config::GameConfig, attribute: i64, deflect: bool| {
        let skills: Vec<i64> = crate::combat::skill_sp_ids(combat, config).collect();
        crate::guard::guarded_posture(18.0, &crate::guard::GuardDef::player(5000, deflect), row.as_ref(), &skills, deflect, attribute)
    };
    config.player.skills = vec![];
    assert_eq!(posture(&config, 1, false), 17.0, "no skill = level-1 Wolf: 18 x 0.99");
    config.player.skills = vec![280];
    for deflect in [true, false] {
        assert_eq!(posture(&config, 1, deflect), 14.0, "17.82 x 0.8");
        assert_eq!(posture(&config, 2, deflect), 13.0, "17.82 x 0.75");
    }
}

#[test]
#[ignore]
pub(crate) fn throw_root_motion_probe() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    for k in ["a000_012000", "a000_012001", "a000_012100", "a000_012101"] {
        let pts: Vec<String> = [0.0f32, 25.0, 61.0, 100.0].iter().map(|f| format!("{:?}", combat.enemy0().root_at(k, f / 60.0).map(|(p, y)| (p.x, p.y, y)))).collect();
        println!("{k} len {:.2}: {}", combat.enemy0().length(k), pts.join(" "));
    }
    for k in ["a201_510000", "a201_500000"] {
        let pts: Vec<String> = [0.0f32, 25.0, 61.0, 100.0].iter().map(|f| format!("{:?}", combat.player.root_at(k, f / 60.0).map(|(p, y)| (p.x, p.y, y)))).collect();
        println!("{k}: {}", pts.join(" "));
    }
}

#[test]
pub(crate) fn killed_mid_jump_falls_and_lands() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut killed = false;
    for i in 0..240 {
        app.update();
        let (_, vy, airborne, pos) = player_actor(&mut app);
        if airborne && vy > 0.0 && !killed {
            // Killed on the way up.
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            q.single_mut(world).unwrap().hp = 0.0;
            killed = true;
        }
        if killed && !airborne {
            assert!(pos.y <= CAPSULE_HALF_HEIGHT + 1e-3, "landed at {}", pos.y);
            return;
        }
        assert!(pos.y < 10.0, "flying away: y {} at frame {i}", pos.y);
    }
    panic!("never landed");
}

/// Mashing jump through a plain jump kicks once: AirKick's SpEffect 100247 (ref 431,
/// SP_EF_REF_TAE_ONLY_ADD_ACTION_INPUT_JUMP) turns the presses into the additive input only
/// (HKS 2814-2823), and the jump lands before that window closes.
#[test]
pub(crate) fn mashed_air_kicks_kick_once_per_jump() {
    let mut app = app();
    move_enemy_far(&mut app);
    let mut kicks = 0;
    let mut prev = String::new();
    let mut left_ground = false;
    for i in 0..150 {
        if i % 3 == 0 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
        app.update();
        let (state, _, airborne, _) = player_actor(&mut app);
        if state == "AirKick" && prev != "AirKick" {
            kicks += 1;
        }
        left_ground |= airborne;
        if left_ground && !airborne {
            break;
        }
        prev = state;
    }
    assert!(left_ground, "never jumped");
    assert_eq!(kicks, 1, "kicks in one jump");
}

/// Wolf's procedural walk (Locomotion, no TAE anim) plays its clip's PlaySound events (the
/// footsteps): over two loops of the walk each fires twice (sound.rs `sound_windows`).
#[test]
pub(crate) fn locomotion_plays_its_clip_sounds() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    let d = &combat.player;
    let mut a = crate::actor::Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0);
    a.procedural("Locomotion");
    a.move_start = d.length(&a.move_clip(true));
    let loop_key = a.move_clip(false);
    let len = d.length(&loop_key);
    let Some(anim) = d.anim(&loop_key) else { return };
    let sounds: Vec<f32> = anim.events.iter().filter(|e| matches!(e.kind, 128..=132) && e.start < len - 0.02).map(|e| e.start).collect();
    println!("{loop_key}: {len:.3} s, sounds at {sounds:?}");
    if sounds.is_empty() || len <= 0.0 {
        return;
    }
    let dt = 1.0 / 60.0;
    a.t = a.move_start - dt * 0.5;
    let mut count = vec![0; sounds.len()];
    while a.t < a.move_start + 2.0 * len - dt * 0.5 {
        a.prev_t = a.t;
        a.t += dt;
        let (clip, windows, _) = crate::sound::sound_windows(&a, d).expect("windows");
        if clip == loop_key {
            for (i, s) in sounds.iter().enumerate() {
                if windows.iter().any(|&(lo, hi)| *s > lo && *s <= hi) {
                    count[i] += 1;
                }
            }
        }
    }
    assert!(count.iter().all(|&c| c == 2), "{sounds:?} fired {count:?}");
}

#[test]
pub(crate) fn the_first_locomotion_step_plays_the_start_clips_sound_at_0() {
    // Round 55: Locomotion starts at t 0 / prev_t 0, and its first window was (0, dt], so an
    // event at 0 of the move start clip (a000_005100 PlaySound 2100) never played.
    let app = app();
    let combat = app.world().resource::<Combat>();
    let d = &combat.player;
    let mut a = crate::actor::Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0);
    a.procedural("Locomotion");
    a.move_start = d.length(&a.move_clip(true));
    if a.move_start <= 0.0 {
        return;
    }
    a.t = 1.0 / 60.0;
    let (clip, windows, _) = crate::sound::sound_windows(&a, d).expect("windows");
    assert_eq!(clip, a.move_clip(true));
    assert!(windows.iter().any(|&(lo, hi)| lo < 0.0 && hi > 0.0), "{windows:?}");
}

#[test]
pub(crate) fn the_deathblow_muffle_ends_on_the_title() {
    // Round 55: deathblow_mix runs in FixedUpdate, frozen off the Playing screen; quitting
    // mid-deathblow kept the muffle on through the menus.
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
        .init_state::<crate::menu::Screen>()
        .init_resource::<crate::sound::DeathblowMix>()
        .add_systems(Update, crate::sound::end_deathblow_mix_off_screen);
    {
        let mut mix = app.world_mut().resource_mut::<crate::sound::DeathblowMix>();
        for _ in 0..30 {
            mix.step(Some((0, [0.3, 2.0, 0.3, 1.0, 0.3, 3.0])), 1.0 / 60.0);
        }
        assert!(!mix.idle());
    }
    app.update();
    assert!(app.world().resource::<crate::sound::DeathblowMix>().idle());
}

#[test]
pub(crate) fn hit_sounds_follow_the_exe_material_lookup() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    // Wolf's sword attack (AtkParam_Pc 5000010: atkMaterial_forSe 0 = Iron group, slash, S) on the
    // General (NpcParam materialSe 114 / 108): HitEffectSeParam 114 Iron_Slash_S = 100000103 ->
    // 'z' sound z100000103; 108 Iron_Slash_S = 108 -> z000000108 (both play).
    let sword = crate::data::Attack { atk_material_se: 0, atk_pow_se: 0, atk_type: 0, ..Default::default() };
    let on_general = crate::sound::hit_sounds(combat, &sword, crate::sound::defender_materials(combat, &crate::actor::Actor::new(crate::actor::Side::Enemy, 1.0, 1.0, 0.0, 0)));
    if combat.foe0().chr == "c1020" {
        assert_eq!(on_general, ["z100000103", "z000000108"]);
    }
    // On Wolf (protector defenseMaterial 113 / 139): row 113 Iron_Slash_S = 13 -> z000000013.
    let on_wolf = crate::sound::hit_sounds(combat, &sword, crate::sound::defender_materials(combat, &crate::actor::Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0)));
    assert_eq!(on_wolf, ["z000000013"]);
}

/// The game's clash effects (vfx::hit_sfx): the General's sword attack (AtkParam_Npc defSfxMaterial
/// 100 / 139, atkMaterial_forSfx 0 = Iron, atkPow_forSfx 2 = L, slash) deflected -> JustGuard row
/// 100 atkIron_1 520 "Jasuga sparks" -> HitEffectSfxParam 520 Slash_L = 252001; guarded -> concept
/// row 100 atkIron_1 10 "spark" -> 201002; Wolf's sword (M) on the General (NpcParam materialSfx
/// 114 / 108) -> 114: 10 -> 201001 and 290 "meat under armour" -> 229001.
#[test]
pub(crate) fn clash_effects_follow_the_hit_effect_tables() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    use crate::vfx::{Kind, hit_sfx};
    let general = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 2, atk_type: 0, def_sfx_material1: 100, def_sfx_material2: 139, ..Default::default() };
    assert_eq!(hit_sfx(combat, &general, Kind::Deflect, [-1, -1]), [252001]);
    assert_eq!(hit_sfx(combat, &general, Kind::Guard, [-1, -1]), [201002]);
    let sword = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 1, atk_type: 0, ..Default::default() };
    if combat.foe0().chr == "c1020" {
        assert_eq!(hit_sfx(combat, &sword, Kind::Hit, crate::vfx::defender_sfx_materials(combat, &crate::actor::Actor::new(crate::actor::Side::Enemy, 1.0, 1.0, 0.0, 0))), [201001, 229001]);
        // HitEffectSfxAngleParam (FUN_14092b6c0): concept 290 has a row, so an Upper attack
        // (atkDir_forSfx 1) turns 229001 to the absolute X -75; concept 10 has none (look-at).
        let upper = crate::data::Attack { atk_dir_sfx: 1, ..sword.clone() };
        let mats = crate::vfx::defender_sfx_materials(combat, &crate::actor::Actor::new(crate::actor::Side::Enemy, 1.0, 1.0, 0.0, 0));
        let turned = crate::vfx::hit_sfx_turned(combat, &upper, Kind::Hit, mats);
        assert_eq!(turned[0], (201001, None));
        assert_eq!(turned[1], (229001, Some(crate::fxr::game_rot([-75.0, 0.0, 0.0, 0.0]))));
        // No direction (0): the row's angles are (0, 0, 0), world-aligned.
        assert_eq!(crate::vfx::hit_sfx_turned(combat, &sword, Kind::Hit, mats)[1], (229001, Some(Quat::IDENTITY)));
    }
    assert_eq!(crate::vfx::hit_sfx_turned(combat, &general, Kind::Guard, [-1, -1]), [(201002, None)]);
    // Wolf's own attacks (AtkParam_Pc defSfxMaterial1/2 255 / 139, defSe the same) take the sword's
    // EquipParamWeapon 5000 materials for 255 (101 / 139; exe FUN_140ba8a50 / FUN_140ba8840), so an
    // enemy guarding or deflecting Wolf gets the sword's sparks and sounds.
    assert_eq!(crate::vfx::guard_materials(combat, [255, 139], false), [101, 139]);
    assert_eq!(crate::vfx::guard_materials(combat, [255, 139], true), [101, 139]);
    let wolf = crate::data::Attack { def_sfx_material1: 255, def_sfx_material2: 139, ..sword.clone() };
    let as_sword = crate::data::Attack { def_sfx_material1: 101, def_sfx_material2: 139, ..sword.clone() };
    assert_eq!(hit_sfx(combat, &wolf, Kind::Guard, [-1, -1]), hit_sfx(combat, &as_sword, Kind::Guard, [-1, -1]));
    assert!(!hit_sfx(combat, &wolf, Kind::Guard, [-1, -1]).is_empty());
}

#[test]
pub(crate) fn guard_walk_keeps_facing_and_follows_the_stick() {
    // Not locked on, guard-walking Wolf turns to the stick and walks forward (DeflectGuardMoveF)
    // along it at ~1.5 m/s. (rec_20261008_051450 was read as facing the camera's direction, but
    // that camera trails behind Wolf; the user, playing: he must not face the camera.)
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.guard_held = true;
        pad.press(Action::Guard);
    }
    for _ in 0..30 {
        app.update();
    }
    let yaw0 = {
        let world = app.world_mut();
        world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().yaw
    };
    let start = player_actor(&mut app).3;
    for _ in 0..60 {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.guard_held = true;
        pad.stick = Vec2::new(0.7071, 0.7071);
        drop(pad);
        app.update();
    }
    let (state, _, _, end) = player_actor(&mut app);
    let world = app.world_mut();
    let yaw1 = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().yaw;
    assert!(state.starts_with("DeflectGuardMove"), "{state}");
    let _ = yaw0;
    // Faces the stick.
    let want = crate::player::stick_world(Vec2::new(0.7071, 0.7071), 0.0);
    let facing = Quat::from_rotation_y(yaw1) * Vec3::NEG_Z;
    let off_stick = facing.angle_between(want).to_degrees();
    assert!(off_stick < 3.0, "facing {off_stick:.1} deg off the stick");
    let moved = (end - start).with_y(0.0);
    assert!(moved.length() > 0.8 && moved.length() < 2.2, "moved {:.2} m in 1 s", moved.length());
    let want = crate::player::stick_world(Vec2::new(0.7071, 0.7071), 0.0);
    let off = moved.normalize().angle_between(want).to_degrees();
    assert!(off < 15.0, "moved {off:.0} deg off the stick");
}

#[test]
pub(crate) fn debug_perilous_drill_plays_only_red_attacks() {
    let mut app = app();
    {
        let mut dbg = app.world_mut().resource_mut::<crate::enemy::EnemyDebug>();
        dbg.mode = crate::enemy::AiMode::Perilous;
        dbg.interval = 0.5;
    }
    let mut seen = Vec::new();
    for _ in 0..900 {
        app.update();
        let (state, anim, _) = enemy_state(&mut app);
        if state.starts_with("Ez") && seen.last() != Some(&anim) {
            seen.push(anim);
        }
        if seen.len() >= 3 {
            break;
        }
    }
    let combat = app.world().resource::<Combat>();
    assert!(!seen.is_empty(), "no drill attack");
    for a in &seen {
        assert!(combat.enemy0().perilous_warning(a, -1.0, 1e9).is_some(), "{a} is not perilous");
    }
}

/// With three enemies drilling, each still waits the menu's interval between its attacks: the
/// shared EnemyDebug timer ran down once per enemy each tick (3 enemies: 1.5 s -> 0.5 s)
/// (2026-10-12 bug hunt round 23).
#[test]
pub(crate) fn each_drilling_enemy_waits_the_full_interval() {
    let g = |at: [f32; 2]| crate::config::GroupEnemy { chr: "c1020".into(), npc_row: None, at: Some(at), msb: None };
    let mut app = app_cfg("c1020", None, vec![g([1.8, 1.8]), g([-1.8, 1.8])]);
    {
        let mut dbg = app.world_mut().resource_mut::<crate::enemy::EnemyDebug>();
        dbg.mode = crate::enemy::AiMode::Perilous;
        dbg.interval = 1.5;
    }
    // Per enemy: ticks idle in range since its last attack, and how many attacks it started.
    let mut idle: std::collections::HashMap<Entity, (u32, bool, u32)> = std::collections::HashMap::new();
    let mut waits = Vec::new();
    for _ in 0..60 * 20 {
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &Actor), (With<Enemy>, Without<crate::enemy::Cast>)>();
        for (ent, a) in q.iter(world) {
            let (ticks, was_attacking, attacks) = idle.entry(ent).or_insert((0, false, 0));
            let attacking = a.state.starts_with("Ez");
            if attacking && !*was_attacking {
                // Its first attack comes at once (timer 0); later ones after the wait.
                if *attacks > 0 {
                    waits.push(*ticks);
                }
                *attacks += 1;
                *ticks = 0;
            } else if a.state == "IdleBattle" {
                *ticks += 1;
            }
            *was_attacking = attacking;
        }
    }
    assert!(idle.len() >= 3, "{} enemies", idle.len());
    assert!(waits.len() >= 3, "too few repeat attacks: {waits:?}");
    // 1.5 s = 90 ticks (a tick or two of the attack's own end blend aside).
    assert!(waits.iter().all(|&w| w >= 85), "waits (ticks) {waits:?}");
}

/// Our General's attack choices over 3 simulated minutes (the player stands guarding at 2.4 m),
/// to compare with the live fight (tools: rec_20261008_054259 attack counts).
#[test]
#[ignore]
pub(crate) fn ai_attack_distribution() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    let mut last = String::new();
    let mut starts = Vec::new();
    for i in 0..(60 * 180) {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.guard_held = true;
            if i % 2 == 0 {
                pad.press(Action::Guard);
            }
        }
        app.update();
        let (state, anim, _) = enemy_state(&mut app);
        if state.starts_with("Ez3") && anim != last {
            *counts.entry(anim.clone()).or_default() += 1;
            starts.push(i as f32 / 60.0);
        }
        last = anim;
        // Keep both alive.
        let world = app.world_mut();
        for mut a in world.query::<&mut Actor>().iter_mut(world) {
            a.hp = a.hp_max;
            a.posture = 0.0;
        }
    }
    let gaps: Vec<f32> = starts.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = gaps.iter().sum::<f32>() / gaps.len().max(1) as f32;
    println!("attacks {} in 180 s, mean gap {mean:.2} s", starts.len());
    for (k, n) in &counts {
        println!("  {k} {n}");
    }
}

#[test]
pub(crate) fn guard_and_deflect_sounds_follow_the_exe_tables() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    // A General sword attack: AtkParam_Npc defSeMaterial1/2 = 100 / 139, atkMaterial_forSe 0 (Iron).
    let atk = combat.enemy0().attacks.values().find(|a| a.def_se_material1 == 100 && a.atk_material_se == 0 && a.atk_type == 0).expect("sword attack");
    assert_eq!(crate::sound::guard_sounds(combat, atk, true), ["z199999980"]);
    assert_eq!(crate::sound::guard_sounds(combat, atk, false), ["z200000101"]);
}

#[test]
pub(crate) fn locked_on_jumps_go_where_the_stick_points() {
    // HKS _SetJumpDirection: locked on, the stick picks a directional jump whose launch row
    // (ChrPhysicsVelocityChangeParam 111-118, horizontalVelocityAngle 0/+-45/+-90/+-135/180) sends
    // Wolf that way relative to the target. Live lock-on forward jump: rise 1.26 m, 0.65 s.
    for (stick, want_state) in [
        (Vec2::new(0.0, 1.0), "LockonForwardGroundJumpReady"),
        (Vec2::new(0.7071, 0.7071), "GroundJumpReady_F_R"),
        (Vec2::new(-1.0, 0.0), "LeftsideGroundJumpReady"),
        (Vec2::new(0.0, -1.0), "BackwardGroundJumpReady"),
    ] {
        let mut app = app();
        // Enemy ahead of the camera (camera yaw 0 looks along -Z), so stick-forward is toward it.
        let enemy = {
            let world = app.world_mut();
            let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
            t.translation.z = -2.4;
            e
        };
        app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
        // Wolf turns to the lock target (quick turn, then the lock-on turn) before the jump.
        for _ in 0..90 {
            app.update();
        }
        let start = player_actor(&mut app).3;
        let mut seen_ready = false;
        let mut landed = String::new();
        let mut launched_dir = None;
        for _ in 0..120 {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = stick;
                if !seen_ready {
                    pad.press(Action::Jump);
                }
            }
            app.update();
            let (state, _, airborne, pos) = player_actor(&mut app);
            seen_ready |= state == want_state;
            if state.starts_with("LandGroundPositioningJump") && landed.is_empty() {
                landed = state.clone();
            }
            if airborne && launched_dir.is_none() && (pos - start).with_y(0.0).length() > 0.3 {
                launched_dir = Some((pos - start).with_y(0.0).normalize());
            }
        }
        assert!(seen_ready, "{want_state} not played");
        assert!(!landed.is_empty(), "{want_state}: no positioning landing");
        // Camera yaw 0 in tests: the stick maps to world as stick_world(stick, 0).
        let want = crate::player::stick_world(stick, 0.0);
        let got = launched_dir.expect("launched");
        assert!(got.angle_between(want).to_degrees() < 25.0, "{want_state}: moved {got:?}, stick {want:?}");
    }
}

#[test]
pub(crate) fn locked_on_step_tilts_toward_the_stick() {
    // HKS _set4DirStepTilt: a stick 30 deg right of the target gives GroundStep_F turned 30 deg
    // (outside the +-18.75 no-tilt cone), so the dodge follows the stick.
    let mut app = app();
    let enemy = {
        let world = app.world_mut();
        let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
        t.translation = Vec3::new(0.0, t.translation.y, -6.0);
        e
    };
    app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
    for _ in 0..90 {
        app.update();
    }
    let stick = Vec2::new(30f32.to_radians().sin(), 30f32.to_radians().cos());
    let start = player_actor(&mut app).3;
    let mut stepped = false;
    for i in 0..25 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = stick;
            if i == 0 {
                pad.press(Action::Step);
            }
        }
        app.update();
        stepped |= player_actor(&mut app).0 == "GroundStep_F";
    }
    assert!(stepped, "no GroundStep_F");
    let moved = (player_actor(&mut app).3 - start).with_y(0.0);
    let want = crate::player::stick_world(stick, 0.0);
    let off = moved.normalize().angle_between(want).to_degrees();
    assert!(moved.length() > 0.5 && off < 8.0, "moved {:.2} m, {off:.1} deg off the stick", moved.length());
}

/// Not locked on, an attack's auto-aim homing is no lock (HKS _fireGroundStep: only env(1118)
/// takes the 4-direction step): a dodge with the stick held away from the homed enemy is the free
/// GroundStep_F along the stick, not GroundStep_B facing him (round 31).
#[test]
pub(crate) fn an_unlocked_step_ignores_the_auto_aim_target() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    let enemy = {
        let world = app.world_mut();
        let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
        t.translation = Vec3::new(0.0, t.translation.y, -3.0);
        e
    };
    app.update();
    let stick = Vec2::new(0.0, -1.0);
    let away = crate::player::stick_world(stick, 0.0);
    let enemy_z = -3.0f32;
    assert!(away.z * enemy_z < 0.0, "the stick must point away from the enemy: {away}");
    let mut states = Vec::new();
    for i in 0..10 {
        {
            let world = app.world_mut();
            world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().homing = Some(enemy);
            let mut pad = world.resource_mut::<PadInput>();
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
    assert!(states.iter().any(|s| s == "GroundStep_F"), "{states:?}");
    assert!(!states.iter().any(|s| s == "GroundStep_B"), "{states:?}");
}

#[test]
#[ignore]
pub(crate) fn probe_block_then_move() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut blocked_at = None;
    for i in 0..600 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.guard_held = true;
            if i == 0 {
                pad.press(Action::Guard);
            }
            if blocked_at.is_some_and(|b| i > b + 10) {
                pad.stick = Vec2::new(1.0, 0.0);
            }
        }
        app.update();
        let l = log_text(&app);
        if blocked_at.is_none() && l.contains("blocked") {
            blocked_at = Some(i);
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = false;
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 30.0;
        }
        if let Some(b) = blocked_at {
            if i <= b + 240 && (i - b) % 10 == 0 {
                let (s, _, _, p) = player_actor(&mut app);
                let world = app.world_mut();
                let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
                println!("{:3} {s:28} anim {} t {:.2} pos ({:.2},{:.2})", i - b, a.anim, a.t, p.x, p.z);
            }
            if i > b + 240 {
                break;
            }
        }
    }
}

#[test]
pub(crate) fn guard_release_while_moving_keeps_running() {
    // DeflectGuardToStandMove is an upper-body state (HKS STATE_TYPE_UPPER_ACTION, c0000.hkx
    // StandMoveOverwrite): the legs keep the walk/run under it and it ends into the run loop.
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    let mut released_at = None;
    for i in 0..160 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.guard_held = i < 80;
            if i == 0 {
                pad.press(Action::Guard);
            }
            pad.stick = if i > 40 { Vec2::new(0.0, 1.0) } else { Vec2::ZERO };
        }
        app.update();
        let (s, _, _, p) = player_actor(&mut app);
        if i == 80 {
            assert!(s.starts_with("DeflectGuardToStandMove"), "released into {s}");
            released_at = Some(p);
        }
        if i == 100 {
            let moved = (p - released_at.unwrap()).with_y(0.0).length();
            assert!(moved > 1.0, "moved {moved:.2} m in the first 20 frames of {s}");
        }
    }
    assert_eq!(player_actor(&mut app).0, "Locomotion");
}

#[test]
pub(crate) fn behind_deathblow_root_yaw_is_played_as_stored() {
    // a200_511200's root yaw (Ochimusha behind deathblow) is keyed across the +-pi seam: +90 deg at
    // frame 10, then -180 by frame 15 (-270 deg) in the game's space; ours mirrors X, so -90 / +180.
    // In game (SHINOBI_THROW_TRACE=behind, c1010) the pair then matches rec_c1010_20261009 tick for
    // tick in the game's convention (tools/rec_throw.py: 0.47 m at 175 deg, dyaw -4.5 vs live 0.47 /
    // 176 / -3.8; mid-spin 0.43 m at -24.5 vs -23.9, dyaw 147.6 vs 148.5).
    let app = app_with("c1010");
    let combat = app.world().resource::<Combat>();
    let yaws: Vec<f32> = (0..=20).filter_map(|f| combat.player.root_at("a200_511200", f as f32 / 30.0).map(|(_, y)| y)).collect();
    if yaws.is_empty() {
        return;
    }
    assert!((yaws[10] + std::f32::consts::FRAC_PI_2).abs() < 0.05, "frame 10 at {:.2}", yaws[10]);
    assert!((yaws[15] - std::f32::consts::PI).abs() < 0.05, "frame 15 at {:.2}", yaws[15]);
}

#[test]
pub(crate) fn sheathe_walk_and_draw() {
    // X sheathes (GroundNonCombatAreaEnter, TAE 32 SetWeaponStyle None at frame 14); walking then
    // uses the sheathed a010 clips; an attack press draws the sword (GroundNonCombatAreaLeave,
    // style back at frame 5) instead of attacking.
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    app.world_mut().resource_mut::<PadInput>().sheathe = true;
    app.update();
    assert_eq!(player_actor(&mut app).0, "GroundNonCombatAreaEnter");
    for _ in 0..140 {
        app.update();
    }
    let sheathed = |app: &mut App| {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        (a.sheathed, a.move_clip(false))
    };
    assert!(sheathed(&mut app).0, "not sheathed after the anim");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(player_actor(&mut app).0, "Locomotion");
    assert!(sheathed(&mut app).1.starts_with("a010_"), "{}", sheathed(&mut app).1);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::ZERO;
        pad.press(Action::Attack);
    }
    app.update();
    let s = player_actor(&mut app).0;
    assert!(s.starts_with("GroundNonCombatArea") && s.ends_with("Leave"), "{s}");
    for _ in 0..80 {
        app.update();
    }
    assert!(!sheathed(&mut app).0, "still sheathed after drawing");
}

#[test]
pub(crate) fn locked_on_diagonal_walk_twists_the_legs() {
    // Locked on, stick forward-left: the F clip plays (HKS 55 deg cone) and the exe's WalkTwist
    // turns the legs -45 deg toward the move (0.1 rad per 1/30 s), so they don't slide sideways.
    let mut app = app();
    let enemy = {
        let world = app.world_mut();
        let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
        t.translation.z = -40.0;
        e
    };
    app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
    for _ in 0..60 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = Vec2::new(-0.7071, 0.7071);
            pad.walk = true;
        }
        app.update();
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert_eq!(a.state, "Locomotion");
    // The move's angle from the facing (+ = right) minus the shown clip's direction.
    let local = Quat::from_rotation_y(-a.yaw) * a.move_vel;
    let angle = local.x.atan2(-local.z).to_degrees();
    let clip = [0.0, 180.0, -90.0, 90.0][a.move_dir as usize];
    let want = (angle - clip + 180.0).rem_euclid(360.0) - 180.0;
    assert!(want.abs() > 20.0, "not diagonal: move {angle:.1} deg, clip {clip}");
    assert!((a.twist.to_degrees() - want).abs() < 3.0, "twist {:.1} deg, want {want:.1}", a.twist.to_degrees());
}

/// Breaks the enemy and puts it `z` metres in front of Wolf (who faces +Z).
pub(crate) fn break_enemy_at(app: &mut App, z: f32) {
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
        let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
        a.posture = a.posture_max;
        e.on_posture_break(&mut a, &combat, false, 8.0);
        t.translation.z = z;
    });
}

#[test]
pub(crate) fn general_deathblow_goes_on_to_the_kill_anim_as_he_dies() {
    // Live (rec_c1020_d_20261009): a201_500000 0.40 s -> 510000 1.85 s -> 510001 (4.18 s); the
    // General goes 12000 -> 12001 on that same frame (HKS BEH_R_THROW_KILL).
    let mut app = app();
    break_enemy_at(&mut app, 1.8);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut main, mut kill) = (None, None);
    for i in 0..400u32 {
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        if main.is_none() && pa == "a201_510000" {
            main = Some(i);
        }
        if pa == "a201_510001" {
            kill = Some(i);
            break;
        }
    }
    let (main, kill) = (main.expect("main throw"), kill.unwrap_or_else(|| panic!("no kill anim
{}", log_text(&app))));
    assert!((kill - main).abs_diff(111) <= 3, "kill {} frames after the main throw (live 111)", kill - main);
    assert_eq!(enemy_state(&mut app).0, "ThrowDefDeath12001");
}

#[test]
pub(crate) fn ochimusha_close_deathblow_uses_the_near_rows() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    // ThrowParam 11010005 崩し始動（近）(Dist 1.2) -> 11010006: a200_502500 -> 512500, ThrowDef14500.
    // Live (rec_c1010_20261009): 0.30 s + 1.83 s, 3 of the 4 front deathblows.
    let mut app = app_with("c1010");
    break_enemy_at(&mut app, 1.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::new();
    for _ in 0..120 {
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        if seen.last() != Some(&pa) {
            seen.push(pa.clone());
        }
        if pa == "a200_512500" {
            break;
        }
    }
    assert!(seen.contains(&"a200_502500".to_string()) && seen.contains(&"a200_512500".to_string()), "{seen:?}");
    assert!(enemy_state(&mut app).0.starts_with("ThrowDef14500"), "{}", enemy_state(&mut app).0);
}

#[test]
pub(crate) fn breaking_deflect_plays_the_pose_and_attack_finishes_it() {
    // Live (rec_c1020_c_20261009): the deflect that empties the General's posture goes straight
    // into Wolf a201_510100 / ThrowDef12100; attack 0.61 s later: 510110 / 12110, then the kill
    // 510111 / ThrowDefDeath12111.
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.single_mut(world).unwrap();
        a.posture = a.posture_max - 1.0;
    }
    start_enemy_combo(&mut app, 0);
    let mut pressed = false;
    let mut pose = None;
    for i in 0..300u32 {
        if !pressed {
            if let Some(dt) = time_to_enemy_hit(&mut app) {
                if dt <= 3.0 / TAE_FPS {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.press(Action::Guard);
                    pressed = true;
                }
            }
        }
        app.update();
        let (ps, pa, pt, _) = player_state(&mut app);
        if pose.is_some() && std::env::var("DBG").is_ok() {
            let (es, ea, et) = enemy_state(&mut app);
            println!("{i} {ps} {pa} {pt:.2} | {es} {ea} {et:.2}");
        }
        if ps == "ThrowBreak" && pose.is_none() {
            assert_eq!(pa, "a201_510100");
            assert_eq!(enemy_state(&mut app).0, "ThrowDef12100");
            pose = Some(i);
        }
        if pose.is_some_and(|p| i == p + 36) { // 0.6 s, as live
            app.world_mut().resource_mut::<PadInput>().guard_held = false;
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        if pa == "a201_510111" {
            assert_eq!(enemy_state(&mut app).0, "ThrowDefDeath12111");
            return;
        }
    }
    let (ps, pa, _, _) = player_state(&mut app);
    panic!("pose {pose:?}, player {ps} {pa}, enemy {}
{}", enemy_state(&mut app).0, log_text(&app));
}

#[test]
pub(crate) fn air_attack_out_of_a_head_kick_on_a_broken_enemy_is_the_kick_down() {
    // Live (rec_c1020_c_20261009): the kick breaks the General (TrunkCollapseFront); an air attack
    // 0.33 s into AirKickEnemyJumpStart_F_Lock -> a201_511500 / ThrowDef13500, landing
    // 511510 / 13510, kill 511511 / ThrowDefDeath13511.
    let mut app = app();
    break_enemy_at(&mut app, 0.8);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            a.play_state(&combat.player, "AirKickEnemyJumpStart_N");
            a.airborne = true;
            a.vel_y = 3.0;
            t.translation.y += 1.5;
        });
    }
    let mut seen = Vec::new();
    let mut started = None;
    for i in 0..300 {
        if i == 15 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        if seen.last() != Some(&pa) {
            seen.push(pa.clone());
        }
        if pa == "a201_511500" && started.is_none() {
            started = Some(i);
        }
        if pa == "a201_511511" {
            break;
        }
    }
    // Buffer flag 87 opens at frame 9 of the kick jump (0.30 s); live 0.33 s.
    let started = started.expect("kick-down");
    assert!((18..=22).contains(&started), "kick-down at frame {started}");
    for k in ["a201_511500", "a201_511510", "a201_511511"] {
        assert!(seen.iter().any(|s| s == k), "{k} missing: {seen:?}
{}", log_text(&app));
    }
}

#[test]
pub(crate) fn mikiri_on_the_ochimusha_breaks_him_into_the_mikiri_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    // Live (rec_c1010_20261009, 3 of 3): GroundStep_F into the thrust Attack3013 -> 見切り崩し
    // (ThrowParam 11010120): Wolf a200_511100, ThrowDef13100 (the thrust's parry posture damage
    // empties his posture), then attack -> 511110 / ThrowDef13110.
    let outs: Vec<String> = (2..10).map(|f| defend_against_with("c1010", "a000_003013", f as f32 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0))).collect();
    assert!(outs.iter().any(|o| o.contains("enemy=ThrowDef13100") && o.contains("player=ThrowBreak")), "{outs:#?}");
}

/// The perilous kind is the heralded attack's AtkParam, not its warning judge: the Ochimusha's
/// Mikiri'd thrust a000_003013 fires 980 (logged as a sweep before), the General's sweep and
/// thrust read as such (round 31).
#[test]
pub(crate) fn a_perilous_kind_comes_from_its_attack() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    assert_eq!(crate::enemy::perilous_kind(combat.enemy0(), PERILOUS_SWEEP), "sweep");
    assert_eq!(crate::enemy::perilous_kind(combat.enemy0(), PERILOUS_THRUST), "thrust");
    let Some(ochimusha) = crate::data::EnemyFile::load("c1010") else { return };
    let d: crate::data::CharData = ochimusha.data;
    assert_eq!(d.perilous_warning("a000_003013", -1.0, 1e9), Some(980));
    assert_eq!(crate::enemy::perilous_kind(&d, "a000_003013"), "thrust");
}

/// The Villager zombie's bamboo-spear thrust (a200_003011, AtkParam 15000400-402: atkType 0,
/// trackType 6, disableGuard 1, under a 982 warning) is Mikiri'd: the gate read atkType 2 only, so
/// it never was (round 31).
#[test]
pub(crate) fn the_zombies_spear_thrust_can_be_mikirid() {
    let outs: Vec<String> = (2..12).map(|f| defend_against_with("c1500", "a200_003011", f as f32 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0))).collect();
    assert!(outs.iter().any(|o| o.contains("MIKIRI")), "{outs:#?}");
}

/// ThrowParam additionalConditionSpecialEffect_forDef picks the row (combat::throw_row_for): the
/// "（HU）" rows (+500) need the anim set a100 (200031), the plain ones a000 (200030). The spear
/// General c1021 (a100) has only the mikiri 11021690, so its thrust (a100_003015, AtkParam
/// 10201090) is Mikiri'd; Isshin's spear phase and Butterfly's second form take their HU rows.
/// It read 0190 / 0120 / 0010 only, so c1021 and c7410 were never Mikiri'd (bug fixes R89).
#[test]
pub(crate) fn a_second_form_takes_its_hu_throw_rows() {
    let pick = |chr: &str, group: u32, sfx: i64| {
        let mut app = app_with(chr);
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Enemy>>().iter_mut(world).next().unwrap().anim_group = group;
        let a = world.query_filtered::<&Actor, With<Enemy>>().iter(world).next().unwrap();
        crate::combat::throw_row_for(world.resource::<Combat>(), a, sfx)
    };
    assert_eq!(pick("c1021", 1, 190), Some(11021690));
    assert_eq!(pick("c7410", 1, 190), Some(17410690), "the charmed samurai's only mikiri row");
    assert_eq!(pick("c1021", 1, 10), Some(11021010));
    assert_eq!(pick("c1020", 0, 190), Some(11020190));
    assert_eq!(pick("c1020", 1, 190), None, "11020190 needs a000");
    assert_eq!(pick("c5400", 0, 190), Some(15400190));
    assert_eq!((pick("c5400", 1, 190), pick("c5400", 1, 120), pick("c5400", 1, 10)), (Some(15400690), Some(15400620), Some(15400510)));
    assert_eq!((pick("c5090", 0, 10), pick("c5090", 1, 10)), (Some(15090010), Some(15090510)));
    let outs: Vec<String> = (2..12).map(|f| defend_against_with("c1021", "a100_003015", f as f32 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0))).collect();
    assert!(outs.iter().any(|o| o.contains("MIKIRI") && o.contains("player=Mikiri")), "{outs:#?}");
}

#[test]
pub(crate) fn landing_mid_air_deflect_continues_in_its_land_anim() {
    // Live (rec_c1010_b_20261009): AirDeflectHardSmall_R (a050_132102) 0.32 s, then on landing
    // LandAirDeflectHardSmall_B (132112) from the same time (HKS StartTime_00 = env(3063)).
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            a.play_state(&combat.player, "AirDeflectHardSmall_R");
            a.airborne = true;
            a.vel_y = -2.0;
            t.translation.y += 0.5;
        });
    }
    let mut land = None;
    for _ in 0..60 {
        let before = player_state(&mut app);
        app.update();
        let (ps, pa, pt, _) = player_state(&mut app);
        if ps != before.0 {
            land = Some((ps, pa, before.2, pt));
            break;
        }
    }
    let (ps, pa, t_air, t_land) = land.expect("landed");
    assert_eq!((ps.as_str(), pa.as_str()), ("LandAirDeflectHardSmall_B", "a050_132112"));
    assert!((t_land - t_air).abs() < 0.05, "air t {t_air:.2} -> land t {t_land:.2}");
}

/// HKS 1862-1869: the air guard breaks (sword, and the open umbrella's
/// W_AirSubGuardDeflectBreak a076_421400) land into their Land versions from the same time
/// only while ref 201 (SpEffect 100302, 0-2.0 s) is up; later is a plain landing
/// (2026-10-13 round 29 D2).
#[test]
pub(crate) fn an_air_guard_break_lands_into_its_land_version_while_ref_201_is_up() {
    for (state, land, start, lands) in [
        ("AirSubGuardDeflectBreak", "LandAirSubGuardDeflectBreak", 0.5, true),
        ("AirDeflectGuardBreak", "LandAirDeflectGuardBreak", 0.5, true),
        ("AirSubGuardDeflectBreak", "LandAirSubGuardDeflectBreak", 2.3, false),
    ] {
        let mut app = app();
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![76000];
        move_enemy_far(&mut app);
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
                let (mut a, mut t) = q.single_mut(world).unwrap();
                let key = if state.contains("SubGuard") { crate::player::sub_anim(&combat.player, state, 76) } else { combat.player.anim_key(state).map(str::to_string) };
                a.play(state, &key.expect(state));
                a.t = start;
                a.prev_t = start;
                a.airborne = true;
                a.vel_y = -2.0;
                t.translation.y += 0.5;
            });
        }
        let mut landed = None;
        for _ in 0..60 {
            let before = player_state(&mut app);
            app.update();
            let (ps, _, pt, _) = player_state(&mut app);
            if ps != before.0 {
                landed = Some((ps, before.2, pt));
                break;
            }
        }
        let (ps, t_air, t_land) = landed.expect("landed");
        if lands {
            assert_eq!(ps, land, "{state} at {start}");
            let (_, pa, ..) = player_state(&mut app);
            assert!(app.world().resource::<Combat>().player.length(&pa) > 2.0, "{land} plays a real clip ({pa})");
            assert!((t_land - t_air).abs() < 0.05, "{state}: air t {t_air:.2} -> land t {t_land:.2}");
        } else {
            assert_ne!(ps, land, "{state} at {start}: ref 201 is over");
        }
    }
}

#[test]
pub(crate) fn jumping_at_a_broken_enemy_vaults_over_him() {
    // Live (rec_c1020_b_20261009, rec_c1010_20261009): 崩し蹴りジャンプ a200_501900 (0.30 s) ->
    // 511900 (ThrowDef13900); the General then took the behind deathblow.
    let mut app = app();
    break_enemy_at(&mut app, 1.5);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut start, mut main) = (None, None);
    for i in 0..120u32 {
        app.update();
        let (ps, pa, _, _) = player_state(&mut app);
        if start.is_none() && pa == "a200_501900" {
            start = Some(i);
        }
        if ps == "BreakKickJump" {
            main = Some(i);
            assert_eq!(pa, "a200_511900");
            break;
        }
    }
    let (start, main) = (start.expect("start"), main.unwrap_or_else(|| panic!("no vault\n{}", log_text(&app))));
    // CommonBehavior 740 at TAE frame 8 (live 0.30 s ~ 18 frames).
    assert!((main - start).abs_diff(17) <= 2, "start {} frames", main - start);
    assert_eq!(enemy_state(&mut app).0, "ThrowDef13900");
}

#[test]
pub(crate) fn debug_menu_plunge_setup_reaches_the_plunge_deathblow() {
    // The debug menu's "plunge deathblow now": Wolf falling from 3 m, 0.6 m short of the broken
    // enemy, attack pressed at once.
    let mut app = app();
    break_enemy_at(&mut app, 1.2);
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
    let mut seen = Vec::new();
    for _ in 0..200 {
        app.update();
        let (ps, _, _, _) = player_state(&mut app);
        if seen.last() != Some(&ps) {
            seen.push(ps.clone());
        }
    }
    assert!(seen.iter().any(|s| s == "PlungeDeathblow") && seen.iter().any(|s| s == "Deathblow"), "{seen:?}");
}

/// A cross-fade runs on the character's own clock: hit stop freezes it with the clip (sekiro-rs
/// fd53f99; it ran on the frame time, finishing the fade while the pose stood still).
#[test]
pub(crate) fn hit_stop_freezes_the_cross_fade() {
    let mut app = app_geom("c1020", None);
    move_enemy_far(&mut app);
    for _ in 0..3 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let blend = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&crate::anim::Skeleton, With<Player>>().single(world).unwrap().blend
    };
    let mut fading = false;
    for _ in 0..10 {
        app.update();
        let b = blend(&mut app);
        if b > 0.0 && b < 0.8 {
            fading = true;
            break;
        }
    }
    assert!(fading, "the attack never cross-faded in");
    let before = blend(&mut app);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hit_stop.start(0.1);
    }
    for _ in 0..4 {
        app.update();
    }
    let after = blend(&mut app);
    assert!((after - before).abs() < 1e-4, "the fade went on through the hit stop: {before} -> {after}");
}

pub(crate) fn enemy_broken(app: &mut App) -> bool {
    let world = app.world_mut();
    world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().is_broken()
}

/// A plunge onto a broken enemy plays its defender pose ThrowDef13400 and holds it broken through
/// the fall: its Broken window had 0.3 s left, which ran out mid-fall, and it stood up in its
/// collapse instead (react does nothing on a broken enemy; 2026-10-12 bug hunt round 15).
#[test]
pub(crate) fn a_plunge_holds_the_broken_enemy_in_its_pose() {
    let mut app = app();
    break_enemy_at(&mut app, 1.2);
    // Most of the break window (to its collapse SpEffect 220420, frame 75 = 2.5 s) gone.
    for _ in 0..130 {
        app.update();
    }
    assert!(enemy_broken(&mut app), "the break window ended early");
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
            t.translation.y += 6.0;
            t.translation.z += 0.6;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut seen, mut enemy_seen) = (Vec::new(), Vec::new());
    for _ in 0..200 {
        app.update();
        let ps = player_state(&mut app).0;
        if seen.last() != Some(&ps) {
            seen.push(ps);
        }
        let es = enemy_state(&mut app).0;
        if enemy_seen.last() != Some(&es) {
            enemy_seen.push(es);
        }
    }
    assert!(seen.iter().any(|s| s == "PlungeDeathblow"), "no plunge: {seen:?}");
    assert!(enemy_seen.iter().any(|s| s == "ThrowDef13400"), "the broken enemy never took its plunge pose: {enemy_seen:?}");
    assert!(seen.iter().any(|s| s == "Deathblow"), "no landing deathblow: {seen:?} / {enemy_seen:?}");
}

pub(crate) fn make_enemy_unaware(app: &mut App, z: f32, back_turned: bool) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
    let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
    t.translation.z = z;
    // Wolf faces +Z (yaw PI): back turned = facing +Z too.
    a.yaw = if back_turned { std::f32::consts::PI } else { 0.0 };
    e.make_unaware(&mut a);
}

#[test]
pub(crate) fn attacking_an_unaware_enemy_from_behind_is_the_stealth_deathblow() {
    // Live (rec_c1020_b_20261009): a201_500200 0.28 s -> 510200 1.52 s -> 510201, the General
    // ThrowDef12200 -> ThrowDefDeath12201.
    let mut app = app();
    make_enemy_unaware(&mut app, 1.5, true);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::new();
    for _ in 0..200 {
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        if seen.last() != Some(&pa) {
            seen.push(pa.clone());
        }
        if pa == "a201_510201" {
            break;
        }
    }
    for k in ["a201_500200", "a201_510200", "a201_510201"] {
        assert!(seen.iter().any(|s| s == k), "{k}: {seen:?}
{}", log_text(&app));
    }
    assert_eq!(enemy_state(&mut app).0, "ThrowDefDeath12201");
}

/// The stealth start's height window (ThrowParam 11020020 upper / lowerYRange 1.26 m): no
/// deathblow across a 2 m ledge (round 36).
#[test]
pub(crate) fn a_stealth_deathblow_keeps_to_its_height_window() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    let row = 11020020;
    assert!(crate::player::throw_height_ok(combat, row, Vec3::ZERO, Vec3::new(0.0, 1.0, 1.5)));
    assert!(crate::player::throw_height_ok(combat, row, Vec3::ZERO, Vec3::new(0.0, -1.0, 1.5)));
    assert!(!crate::player::throw_height_ok(combat, row, Vec3::ZERO, Vec3::new(0.0, 2.0, 1.5)));
    assert!(!crate::player::throw_height_ok(combat, row, Vec3::ZERO, Vec3::new(0.0, -2.0, 1.5)));
}

#[test]
pub(crate) fn an_unaware_enemy_notices_wolf_in_front_but_not_behind() {
    // Facing Wolf 3 m away (inside eye_BeginDist_normal 4 m): noticed within 0.5 s.
    let mut app = app();
    make_enemy_unaware(&mut app, 3.0, false);
    for _ in 0..40 {
        app.update();
    }
    // Its logic (_COMMON_AddStateTransitionGoal) plays the alert EzState 1040 =
    // TransToBattleFromDefault (a000_001040), whose TAE sets the battle AI state 200004.
    assert_eq!(enemy_state(&mut app).1, "a000_001040", "{}", log_text(&app));
    // Back turned: still unaware after 3 s.
    let mut app2 = sim_tests_app();
    make_enemy_unaware(&mut app2, 3.0, true);
    for _ in 0..180 {
        app2.update();
    }
    assert_eq!(enemy_state(&mut app2).0, "IdleDefault");
}

#[test]
pub(crate) fn plunging_onto_an_unaware_enemy_is_the_stealth_plunge() {
    // Live (rec_c1020_b_20261009): a201_510300 -> 510310 -> 510311, ThrowDef12300 -> 12310 -> 12311.
    let mut app = app();
    make_enemy_unaware(&mut app, 1.2, true);
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
    let mut seen = Vec::new();
    for _ in 0..300 {
        app.update();
        let (_, pa, _, _) = player_state(&mut app);
        if seen.last() != Some(&pa) {
            seen.push(pa.clone());
        }
        if pa == "a201_510311" {
            break;
        }
    }
    for k in ["a201_510300", "a201_510310", "a201_510311"] {
        assert!(seen.iter().any(|s| s == k), "{k}: {seen:?}
{}", log_text(&app));
    }
}


/// Wolf falling onto the enemy from `above` m, with both of them `dy` m off the sim floor.
pub(crate) fn start_falling_onto(app: &mut App, above: f32, dy: f32) {
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        a.play_state(&combat.player, "VerticalGroundJumpStart");
        a.t = 0.5;
        a.prev_t = 0.5;
        a.airborne = true;
        a.vel_y = -1.0;
        t.translation.y += above + dy;
        t.translation.z += 0.6;
    });
    if dy != 0.0 {
        let mut q = world.query_filtered::<&mut Transform, (With<Enemy>, Without<crate::enemy::Cast>)>();
        for mut t in q.iter_mut(world) {
            t.translation.y += dy;
        }
    }
}

/// The plunge row is chosen per target: a broken enemy elsewhere no longer turns the stealth
/// plunge on an unaware one into nothing (2026-10-12 bug hunt round 16).
#[test]
pub(crate) fn a_broken_enemy_elsewhere_keeps_the_stealth_plunge() {
    let g = crate::config::GroupEnemy { chr: "c1020".into(), npc_row: None, at: Some([0.0, 9.0]), msb: None };
    let mut app = app_cfg("c1020", None, vec![g]);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let mut all: Vec<_> = q.iter_mut(world).collect();
            all.sort_by(|a, b| a.2.translation.z.total_cmp(&b.2.translation.z));
            let mut far = all.pop().unwrap();
            far.1.posture = far.1.posture_max;
            far.0.on_posture_break(&mut far.1, &combat, false, 8.0);
            let near = all.first_mut().unwrap();
            near.2.translation.z = 1.2;
            near.1.yaw = std::f32::consts::PI;
            near.0.make_unaware(&mut near.1);
        });
    }
    start_falling_onto(&mut app, 3.0, 0.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::new();
    for _ in 0..3 {
        app.update();
        let pa = player_state(&mut app).1;
        if seen.last() != Some(&pa) {
            seen.push(pa);
        }
    }
    assert!(seen.iter().any(|s| s == "a201_510300"), "no stealth plunge: {seen:?}");
}

/// Moves Wolf (feet height kept) without touching his state.
pub(crate) fn put_wolf(app: &mut App, x: f32, z: f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Transform, With<Player>>();
    let mut t = q.single_mut(world).unwrap();
    t.translation.x = x;
    t.translation.z = z;
}

pub(crate) fn enemy_info(app: &mut App) -> (u8, u8, String, String, Vec3) {
    let world = app.world_mut();
    let mut q = world.query::<(&Enemy, &Actor, &Transform)>();
    let (e, a, t) = q.single(world).unwrap();
    (e.targeting.state, e.targeting.kind, a.anim.clone(), e.ai_desc.clone(), t.translation)
}

/// Round 27: with a resurrection left Wolf's death start ends in GroundRevivalStayLoop_F
/// (a000_110020, HKS 328-333), whose 110010 "Pretending Dead" (sightSearchEnemyCut 100) makes a
/// fighting Ashina soldier drop him (common_common_logic_func.lua:16-22); with none left, the
/// plain GroundDeathLoop_F.
/// Ignored (open, Bosees): once Wolf is down the soldier's battle plan never ends - it flickers
/// EndureAttack > CommonAttack(1040) (a000_405000 one tick, none the next) every frame - so the
/// next plan's COMMON_HiPrioritySetup never sees 110010 (HANDOFF 4.49).
#[test]
pub(crate) fn a_feigning_wolf_is_dropped_by_the_enemy() {
    let mut app = app_cfg("c1010", Some(10100000), Vec::new());
    // Facing Wolf 3 m away: it sees him and fights, with its real AI (the test app's enemy is
    // the passive practice partner once in BATTLE, which runs no AI script).
    make_enemy_unaware(&mut app, 3.0, false);
    {
        let world = app.world_mut();
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
    }
    for _ in 0..60 * 2 {
        app.update();
    }
    assert_eq!(enemy_info(&mut app).0, crate::stealth::BATTLE);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    let mut states = Vec::new();
    for _ in 0..60 * 5 {
        app.update();
        let s = player_state(&mut app).0;
        let (st, _, anim, desc, _) = enemy_info(&mut app);
        let s = format!("{s} | enemy {st} {anim} {desc}");
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s.starts_with("GroundRevivalStayLoop_F")), "{states:#?}");
    assert_ne!(enemy_info(&mut app).0, crate::stealth::BATTLE, "still fighting a feigning Wolf: {states:#?}");
}

/// cargo test --release stealth_trace -- --ignored --nocapture : the stealth loop, logged.
/// SHINOBI_STEALTH_TRACE = leave (default: Wolf leaves once it is alerted) | stay | lose
/// (c1010 fights, then Wolf gets out of its sight and is forgotten after SightTargetForgetTime 15 s).
#[test]
#[ignore]
pub(crate) fn stealth_trace() {
    let mode = std::env::var("SHINOBI_STEALTH_TRACE").unwrap_or_else(|_| "leave".into());
    let mut app = if mode == "lose" { app_with("c1010") } else { app() };
    if mode == "lose" {
        // Fighting at 3 m (as spawned in the duel), then Wolf 20 m behind it.
        make_enemy_unaware(&mut app, 3.0, false);
    } else {
        // Enemy 22 m away facing Wolf: outside the normal cone (16 m ahead), inside around (26 m).
        make_enemy_unaware(&mut app, 22.0, false);
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Enemy>();
        q.single_mut(world).unwrap().aggressive = true;
    }
    let mut last = String::new();
    for f in 0..60 * 60 {
        let (state, ..) = enemy_info(&mut app);
        match mode.as_str() {
            "leave" if state == crate::stealth::CAUTION => put_wolf(&mut app, 40.0, -40.0),
            // Out of its 40 m perceive range from then on (no walls to hide behind here).
            "lose" if f >= 120 => put_wolf(&mut app, 0.0, -70.0),
            _ => {}
        }
        app.update();
        let (state, kind, anim, desc, pos) = enemy_info(&mut app);
        let line = format!("state {state} kind {kind} anim {anim} | {desc}");
        if line != last {
            println!("{:6.2}s pos ({:5.1} {:5.1})  {line}", f as f32 / 60.0, pos.x, pos.z);
            last = line;
        }
    }
}

/// Sprinting keeps its speed into SprintLoop: that transition drops the source motion
/// (TaeBlend_NoSrcMotion, blend_src.json), so the loop's root moves at full weight from its
/// first frame. Weighting it by the blend stopped Wolf dead for a moment (8.6 -> 0 m/s).
#[test]
pub(crate) fn sprint_keeps_its_speed_into_the_loop() {
    let mut app = app();
    move_enemy_far(&mut app);
    let mut last = player_actor(&mut app).3;
    let mut min_in_loop = f32::INFINITY;
    let mut min_start = f32::INFINITY;
    let mut seen_loop = false;
    for i in 0..200 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = Vec2::new(0.0, 1.0);
            if i == 90 {
                pad.press(Action::Step);
            }
            pad.dodge_held = i >= 90;
        }
        app.update();
        let (state, _, _, pos) = player_actor(&mut app);
        let v = (pos - last).with_y(0.0).length() * 60.0;
        last = pos;
        if state == "SprintLoop" {
            seen_loop = true;
            min_in_loop = min_in_loop.min(v);
        }
        if state == "SprintStartFromStep_F" {
            min_start = min_start.min(v);
        }
    }
    // Live (Sekiro, move_log 2026-10-15): the step -> SprintStartFromStep hand-over stays at
    // 6.7-8.8 m/s (graph state SprintStartFromStep drops the source motion).
    assert!(min_start > 5.0, "sprint start dipped to {min_start:.2} m/s");
    assert!(seen_loop, "never reached SprintLoop");
    assert!(min_in_loop > 6.0, "sprint loop dipped to {min_in_loop:.2} m/s");
}

/// Live (rec_20261016_094717_lockturn_sprintdeflect, 7 locked sprint deflects): the deflect does not
/// snap to the enemy. Sprinting 90-150 deg off him, the facing is still ~100 deg off at the
/// start, about 75 deg off 0.17 s in and square on by ~0.4 s (TAE turn 0 / 180 / 360, then 720 deg/s).
#[test]
pub(crate) fn a_locked_sprint_deflect_turns_to_the_enemy_instead_of_snapping() {
    for sx in [1.0f32, -0.7] {
        let mut app = app();
        let enemy = {
            let world = app.world_mut();
            let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
            t.translation = Vec3::new(0.0, t.translation.y, -14.0);
            e
        };
        app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
        for _ in 0..90 {
            app.update();
        }
        let mut errs = Vec::new();
        for i in 0..110 {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = Vec2::new(sx, 0.5);
                if i == 0 {
                    pad.press(Action::Step);
                }
                pad.dodge_held = i < 70;
                pad.guard_held = (60..110).contains(&i);
                if i == 60 {
                    pad.press(Action::Guard);
                }
            }
            app.update();
            let world = app.world_mut();
            let et = world.query_filtered::<&Transform, With<Enemy>>().single(world).unwrap().translation;
            let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
            let to = (et - t.translation).with_y(0.0);
            let err = ((a.yaw - (-to.x).atan2(-to.z)).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
            errs.push((a.state.clone(), err));
        }
        let at = |n: usize| errs[60 + n].1.abs();
        assert_eq!(errs[60].0, "SprintToDeflectGuard");
        assert!(at(0) > 70.0, "snapped at the start: {:.0} deg off", at(0));
        assert!(at(10) > 40.0, "already square at +10: {:.0} deg off", at(10));
        assert!(at(30) < 5.0, "still {:.0} deg off at +30", at(30));
    }
}

/// Live (rec_20261016_* W_StandRunStop): after the stick is released the run is down to
/// 0.5 m/s within 3 ticks and Wolf slides 0.15-0.3 m in all (the clip a000_000600 moves 0.3 m).
/// The stop must not carry the run's momentum through its blend (graph state StandRunStop).
#[test]
pub(crate) fn releasing_the_stick_at_a_run_stops_in_about_a_third_of_a_metre() {
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..60 {
        app.update();
    }
    let mut last = player_actor(&mut app).3;
    let mut start = last;
    for i in 0..100 {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = if i < 60 { Vec2::new(0.0, 1.0) } else { Vec2::ZERO };
        }
        app.update();
        let (state, pos) = {
            let w = app.world_mut();
            let (a, t) = w.query_filtered::<(&Actor, &Transform), With<Player>>().single(w).unwrap();
            (a.state.clone(), t.translation)
        };
        if i == 59 {
            start = pos;
        }
        let v = (pos - last).with_y(0.0).length() * 60.0;
        last = pos;
        if i == 65 {
            assert!(state.starts_with("StandRunStop"), "state {state}");
            assert!(v < 1.0, "still {v:.1} m/s 5 ticks after release");
        }
        if i == 99 {
            let slid = (pos - start).with_y(0.0).length();
            assert!(slid < 0.4, "slid {slid:.2} m");
        }
    }
}
