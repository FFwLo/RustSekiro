//! Sim tests, part 5: 35 tests, from `a_guarding_enemy_takes_its_npc_posture_rate` to `blue_lightning_leaves_an_air_art_going`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// An enemy that blocks or deflects Wolf's slash takes posture x its NpcParam <attr>StaminaDmgRate
/// too, as on a hit (exe FUN_1408439a0: every outcome x vfunc 0x248's per-attribute rate). The
/// General's row with every <attr>StaminaDmgRate set to 0.5 takes half (floored) of what it takes
/// at 1.0, in a block pose and in a deflect window (2026-10-13 round 28).
#[test]
pub(crate) fn a_guarding_enemy_takes_its_npc_posture_rate() {
    // (posture the enemy gains from Wolf's first slash while held in a block / deflect pose)
    let run = |rate: f64, deflect: bool| -> f32 {
        let mut app = app();
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 2.0;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        }
        for _ in 0..10 {
            app.update();
        }
        let (key, t, row) = {
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
            let combat = world.resource::<Combat>();
            let row = combat.foe_of(a).npc_row.to_string();
            let d = combat.data_of(a);
            let mut found: Vec<(String, f32)> = d
                .anims
                .iter()
                .flat_map(|(k, an)| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(crate::player::FLAG_SHIELD_BLOCK)).map(move |e| (k.clone(), e.start + 0.05)))
                .filter(|(k, t)| d.flag(k, *t, crate::player::FLAG_SHIELD_BLOCK) && d.has_state_info(k, *t, crate::data::STATE_INFO_JUST_GUARD) == deflect)
                .collect();
            found.sort_by(|x, y| x.0.cmp(&y.0));
            let (key, t) = found.into_iter().next().expect("a guard pose");
            (key, t, row)
        };
        {
            let mut combat = app.world_mut().resource_mut::<Combat>();
            if let Some(r) = combat.params["NpcParam"][&row].as_object_mut() {
                for f in (1..=12).filter_map(crate::data::npc_stamina_rate_field) {
                    r.insert(f.to_string(), rate.into());
                }
            }
        }
        let hold = |app: &mut App| {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
            let mut a = q.single_mut(world).unwrap();
            if a.anim != key {
                a.play("Guard", &key);
            }
            a.prev_t = t;
            a.t = t;
            a.posture = 0.0;
        };
        hold(&mut app);
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        for _ in 0..90 {
            hold(&mut app);
            app.update();
            let world = app.world_mut();
            let p = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap().posture;
            if p > 0.0 {
                return p;
            }
        }
        0.0
    };
    for deflect in [false, true] {
        let (whole, half) = (run(1.0, deflect), run(0.5, deflect));
        assert!(whole > 1.0, "deflect {deflect}: no posture at 1.0");
        assert_eq!(half, (whole * 0.5).floor(), "deflect {deflect}: {whole} at 1.0, {half} at 0.5");
    }
}

/// Sen Throw (70400): its throws' judges are gated by the sen-held levels (127060-127063,
/// stateInfo 999-996, set by common.emevd). With 500+ sen the LV3 row's coins fly and hit; with
/// none only LV0 (107004101: no bullet, wepCost 1) - the emblem goes and nothing flies. The bare
/// judge 107004100 (refType 0) never throws the free Shuriken 700000 (round 68: no level was
/// on, so it threw nothing at any sen).
#[test]
pub(crate) fn sen_throw_throws_by_the_sen_held() {
    let (states, _, hp) = use_tool_sen(70400, 20, 0, 150, 500);
    assert!(hp > 0.0, "500 sen: no hit, states {states:?}");
    let (states, spent, hp) = use_tool_sen(70400, 20, 0, 150, 0);
    assert_eq!(hp, 0.0, "0 sen hit: states {states:?}");
    assert!(spent >= 1, "0 sen: spent {spent}");
}

/// The Sabimaru poisons: each cut puts 9004 (poizonAttackPower 31, stateInfo 2) toward the
/// General's resist_poison; full, 9004 hands on to 9045 (replaceSpEffectId), which takes 0.75 %
/// + 12 HP every second for 19.9 s.
#[test]
pub(crate) fn the_sabimaru_poisons() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![75000];
        world.resource_mut::<crate::config::GameConfig>().player.spirit_emblems = 99;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 1.8;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = 99;
    }
    for _ in 0..10 {
        app.update();
    }
    let mut poisoned = None;
    let mut seen: Vec<String> = Vec::new();
    for f in 0..1500 {
        if poisoned.is_none() && f % 4 == 0 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
        }
        // Held in reach: each cut's knockback (1.2 m) would carry him out of it.
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            let mut t = q.single_mut(world).unwrap();
            t.translation.x = 0.0;
            t.translation.z = 1.8;
            let mut q = world.query_filtered::<&mut Transform, (With<Player>, Without<Enemy>)>();
            let mut t = q.single_mut(world).unwrap();
            t.translation.x = 0.0;
            t.translation.z = 0.0;
        }
        app.update();
        let st = player_state(&mut app).0;
        if seen.last() != Some(&st) {
            seen.push(st);
        }
        if poisoned.is_none() && log_text(&app).contains("POISONED") {
            poisoned = Some(enemy_hp(&mut app));
        }
    }
    let hp = poisoned.unwrap_or_else(|| panic!("never poisoned: {seen:?}\n{}", log_text(&app)));
    assert!(enemy_hp(&mut app) < hp && log_text(&app).contains("poison: -"), "{}", log_text(&app));
}


/// Okinaga's spew costs on: HoldStart's consumption dummy 999 (f10) and HoldLoop's (f110 of
/// a072_400300, every 4 s loop); out of emblems, the loop's use check (ref 326, every 10 frames)
/// fails into the clack.
#[test]
pub(crate) fn okinagas_spew_costs_while_held() {
    let (_, tap, _) = use_tool(72200, 30, 0, 200);
    // HoldLoop starts ~3.2 s in (Combo1 f75 + HoldStart); its 999 fires 3.67 s later.
    let (states, held, _) = use_tool_with(72200, 30, 600, 650, 500, Some((140, 190)));
    assert!(held >= tap + 6, "held {held} vs tap {tap}: {states:?}");
    let (states, spent, _) = use_tool_with(72200, 6, 600, 650, 500, Some((140, 190)));
    assert!(spent <= 6 && states.iter().any(|s| s == "SubAttackFailed" || s.ends_with("SubAttackFailed")), "{spent}: {states:?}");
}


/// Fang and Blade (HKS BEH_A_GROUND_ATTACK, line 3257): the attack button inside a tool move's
/// SP_EF_REF_TAE_ENABLE_SUB_ATTACK_DERIVE_ATTACK window follows up with the tool's slash: the
/// Shuriken's and Spear's W_GroundSubAttackDeriveAttackCombo1, the Sabimaru LV2's directed cut (LV1
/// 75000 carries ref 315 WEP_DISABLE_DERIVE_SUB_ATTACK_COMBO).
#[test]
pub(crate) fn tools_follow_up_with_the_attack_button() {
    for (tool, want) in [(70000, "GroundSubAttackDeriveAttackCombo1"), (78000, "GroundSubAttackDeriveAttackCombo1"), (75100, "GroundSubAttackDeriveDirectivityAttack_V")] {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        }
        for _ in 0..10 {
            app.update();
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
        let mut seen: Vec<String> = Vec::new();
        for f in 0..200 {
            if f > 6 && f % 3 == 0 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert!(seen.iter().any(|s| s == want), "{tool}: {seen:?}");
    }
}


/// The Mist Raven's leap (HKS 536-561): with no stick it is SubAttackJumpStart_V (404010, its root
/// rising 3 m under SetNoGravity), which ends in the air -> AirSubAttackMoveStartToLoop (-> the
/// fall loop on a longer drop) -> the free-fall landing; a stick leap (404011..) runs along the floor and lands at once.
#[test]
pub(crate) fn the_mist_raven_leap_rises_and_falls() {
    fn player_pos(app: &mut App) -> Vec3 {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
    }
    // The attack button in the fall (ref 301) is the follow-up W_AirSubAttackDeriveAttack (HKS 2890).
    for (stick, airborne, attack) in [(Vec2::ZERO, true, false), (Vec2::new(0.0, -1.0), false, false), (Vec2::ZERO, true, true)] {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![74000];
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        }
        for _ in 0..10 {
            app.update();
        }
        {
            let world = app.world_mut();
            let key = crate::player::sub_anim(&world.resource::<Combat>().player, "SubAttackJumpReady", 74).expect("a074 SubAttackJumpReady");
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            q.single_mut(world).unwrap().play("SubAttackJumpReady", &key);
            world.resource_mut::<PadInput>().stick = stick;
        }
        let (mut seen, mut top) = (Vec::<String>::new(), 0.0f32);
        let y0 = player_pos(&mut app).y;
        for _ in 0..400 {
            if attack && seen.last().is_some_and(|s| s == "AirSubAttackMoveStartToLoop") {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            top = top.max(player_pos(&mut app).y - y0);
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        if attack {
            assert!(seen.iter().any(|s| s == "AirSubAttackDeriveAttack"), "{seen:?}");
            assert!(seen.iter().any(|s| s.starts_with("LandAirSubAttackDeriveAttack") || s == "LandFreeFall"), "{seen:?}");
        } else if airborne {
            assert!(top > 2.5, "rose {top}: {seen:?}");
            // 3 m is down inside StartToLoop (1 s); AirSubAttackMoveLoop is for longer drops.
            for want in ["SubAttackJumpStart_V", "AirSubAttackMoveStartToLoop", "LandFreeFall"] {
                assert!(seen.iter().any(|s| s == want), "{want}: {seen:?}");
            }
        } else {
            assert!(seen.iter().any(|s| s == "LandAirSubAttackMove"), "{seen:?}");
            assert!(!seen.iter().any(|s| s.starts_with("AirSubAttackMove")), "{seen:?}");
        }
        assert!((player_pos(&mut app).y - y0).abs() < 0.05, "back on the floor: {seen:?}");
    }
}

/// Tools in the air (BEH_A_AIR_SUB_ATTACK, HKS 3007): pressed during a jump each tool plays its
/// air move, and the move lands into its Land version (ref 201) or the plain landing. One use per
/// jump for 071/072/074/075/077/078/079 (AIR_SUB_ATTACK_COUNT_MAX 1), the Shuriken / Firecracker /
/// Umbrella any number of times.
#[test]
pub(crate) fn tools_work_in_the_air() {
    use crate::prosthetic::hks;
    let none = |_: i64| false;
    assert_eq!(hks::air_press(71, true, 1, false, &none), None);
    assert_eq!(hks::air_press(70, true, 1, false, &none), Some(("AirSubAttackCombo1", false)));
    // Divine Abduction without ref 309: the "071 or 077" branch (HKS 3083), counted (round 49).
    assert_eq!(hks::air_press(77, true, 0, false, &none), Some(("AirSubAttackCombo1", true)));
    // Validate 6220's flamethrower clause (round 50): inside ref 329 only a 072 with ref 325.
    let r329 = |r: i64| r == hks::REF_ONLY_FLAMETHROWER_CANCEL;
    let r329_325 = |r: i64| r == hks::REF_ONLY_FLAMETHROWER_CANCEL || r == hks::REF_WEP_ENABLE_SUB_ATTACK_HOLD;
    assert!(hks::flame_gate(70, &none) && hks::flame_gate(72, &r329_325));
    assert!(!hks::flame_gate(72, &r329) && !hks::flame_gate(70, &r329_325));
    // Round 59 (6220): the umbrella opens from a held button inside ref 311 (100368).
    let r311 = |r: i64| r == hks::REF_ENABLE_PRESS_SUB_ATTACK;
    assert!(hks::held_press(76, true, true, &r311));
    assert!(!hks::held_press(76, false, true, &r311) && !hks::held_press(70, true, true, &r311));
    assert_eq!(hks::air_press(72, false, 0, false, &none), Some(("SubAttackFailedAir", false)));
    // The start lands as the game's misspelt LandAirSubAttacStart (403050) from the same time
    // while ref 201 is up, else from the loop's landing.
    assert_eq!(hks::air_land(73, "AirSubAttackStart", true), Some(("LandAirSubAttacStart", true)));
    assert_eq!(hks::air_land(73, "AirSubAttackStart", false), Some(("LandAirSubAttackLoop", false)));
    assert_eq!(hks::air_land(70, "AirSubAttackLoop", false), Some(("LandAirSubAttackLoop", false)));
    for (tool, want) in [(70000, "AirSubAttackCombo1"), (73000, "AirSubAttackStart"), (74000, "AirSubAttackMoveAtemiReady"), (76000, "AirSubAttackGuardStart"), (79000, "AirSubAttackCombo1")] {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        }
        for _ in 0..10 {
            app.update();
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut seen: Vec<String> = Vec::new();
        let mut pressed = false;
        for _ in 0..300 {
            let (state, _, t, _) = player_state(&mut app);
            if !pressed && state.contains("GroundJumpStart") && t > 0.2 {
                pressed = true;
                app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert!(seen.iter().any(|s| s == want), "{tool}: {seen:?}");
        let landed = seen.iter().position(|s| s.starts_with("LandAirSubAtta") || s == "LandFreeFall");
        assert!(landed.is_some(), "{tool} never landed: {seen:?}");
        assert_eq!(seen.last().map(String::as_str), Some("StandIdle"), "{tool}: {seen:?}");
    }
}

/// Round 26: the air throws fire their TAE 4 BulletBehavior_Midair bullets and pay the emblem:
/// the shuriken's a070_403000 (judge 150 -> Bullet 700050-style row at v7000:150) and the
/// firecracker's a071_403000 (judge 120 -> BehaviorParam_PC 107100120 -> Bullet 710020, wepCost
/// 1); before, the air move played but nothing was thrown or paid.
#[test]
pub(crate) fn an_air_throw_fires_and_pays() {
    for tool in [70000, 71000] {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        }
        for _ in 0..10 {
            app.update();
        }
        let before = {
            let world = app.world_mut();
            world.query::<&Player>().single(world).unwrap().emblems
        };
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut pressed = false;
        let mut thrown = false;
        for _ in 0..200 {
            let (state, _, t, _) = player_state(&mut app);
            if !pressed && state.contains("GroundJumpStart") && t > 0.2 {
                pressed = true;
                app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
            }
            app.update();
            let world = app.world_mut();
            thrown |= world.query::<&crate::prosthetic::Projectile>().iter(world).next().is_some();
        }
        let after = {
            let world = app.world_mut();
            world.query::<&Player>().single(world).unwrap().emblems
        };
        assert!(thrown, "{tool}: no bullet
{}", log_text(&app));
        assert!(after < before, "{tool}: emblems {before} -> {after}");
    }
}

/// Tools out of a sprint and a crouch (BEH_A_GROUND_SUB_ATTACK 3523 by style): the first state each
/// plays. Only 070 / 071 / 076 / 079 have crouch versions (W_CrouchSubAttackCombo / GuardStart /
/// *Moveable); the umbrella closes by W_GroundSubAttackGuardEnd in every style (line 3779); the
/// whistle has no sprint branch (walking: Combo1Move).
#[test]
pub(crate) fn tools_from_a_sprint_and_a_crouch() {
    let cases: &[(i64, &str, &str)] = &[
        (70000, "SprintSubAttack", "CrouchSubAttackCombo1"),
        (71000, "SprintSubAttack", "CrouchSubAttackCombo1"),
        (72000, "SprintSubAttack", "GroundSubAttackCombo1"),
        (73000, "SprintSubAttack", "GroundSubAttackCombo1"),
        (74000, "SprintToSubAttackJumpAtemiReady", "SubAttackJumpAtemiReady"),
        (75000, "SprintSubAttack", "GroundSubAttackCombo1"),
        (76000, "SprintToSubAttackGuardStart", "CrouchSubAttackGuardStart"),
        (77000, "SprintSubAttack", "GroundSubAttackCombo1"),
        (78000, "SprintSubAttack", "GroundSubAttackCombo1"),
        (79000, "GroundSubAttackCombo1Move", "CrouchSubAttackCombo1Moveable"),
    ];
    for &(tool, sprint_want, crouch_want) in cases {
        for (style, want) in [("sprint", sprint_want), ("crouch", crouch_want)] {
            let mut app = app();
            app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
            app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            for _ in 0..5 {
                app.update();
            }
            if style == "sprint" {
                force_player(&mut app, "SprintLoop", 3.0);
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
                app.world_mut().resource_mut::<PadInput>().dodge_held = true;
            } else {
                app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
                for _ in 0..120 {
                    app.update();
                }
            }
            app.update();
            app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
            let mut first = None;
            for _ in 0..10 {
                app.update();
                let (s, k, _, _) = player_state(&mut app);
                if k.starts_with("a07") {
                    first = Some((s, k));
                    break;
                }
            }
            let (s, k) = first.unwrap_or_default();
            assert_eq!(s, want, "{tool} {style} ({k})");
            assert!(k.starts_with(&format!("a{:03}", tool / 1000)), "{tool} {style}: {k}");
        }
    }
}

/// A deathblow gives Wolf back posture and, with the skills, HP (the a20x_510xxx kill frame's
/// one-shot SpEffects): 105050 34 % posture for everyone; Breath of Life: Shadow (80: permit 150300
/// stateInfo 986) adds 150301 +10 % HP; skill 265 (150320, 984) adds 150321 34 % posture.
#[test]
pub(crate) fn deathblows_give_back_posture_and_hp_with_the_skills() {
    for skills in [vec![], vec![80, 265]] {
        let mut app = app();
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.skills = skills.clone();
        for _ in 0..5 {
            app.update();
        }
        let (hp_max, posture_max) = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            let mut a = q.single_mut(world).unwrap();
            a.hp = a.hp_max * 0.5;
            a.posture = a.posture_max * 0.9;
            (a.hp_max, a.posture_max)
        };
        break_and_deathblow(&mut app, 200);
        let (hp, posture) = {
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            (a.hp, a.posture)
        };
        let log = log_text(&app);
        if skills.is_empty() {
            assert!((hp - hp_max * 0.5).abs() < 1.0, "hp {hp}: {log}");
            assert!(log.contains("deathblow: -"), "{log}");
        } else {
            assert!(hp >= hp_max * 0.6 - 1.0, "hp {hp}: {log}");
            assert!(log.contains("deathblow: +"), "{log}");
        }
        // Natural recovery runs too, so only "at least the deathblow's share" is checked.
        let back = posture_max * if skills.is_empty() { 0.34 } else { 0.68 };
        assert!(posture <= posture_max * 0.9 - back + 1.0, "posture {posture} of {posture_max}: {log}");
    }
}

/// After a Todome (Wolf's a000_710205-7 put on 105051 / 150302-150332, effectEndurance -1) the
/// deathblow recoveries are blocked: same spCategory, categoryPriority 2 beats 3. The General's
/// deathblow clip has no release row, so it stays blocked.
#[test]
pub(crate) fn todome_blocks_the_deathblow_recoveries() {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.skills = vec![80, 265];
    for _ in 0..5 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().held = vec![105051, 150302, 150312, 150322, 150332];
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.hp = a.hp_max * 0.5;
        a.posture = a.posture_max * 0.9;
    }
    break_and_deathblow(&mut app, 200);
    let log = log_text(&app);
    assert!(!log.contains("deathblow: +") && !log.contains("deathblow: -"), "{log}");
    let d = &app.world().resource::<Combat>().player;
    for (block, rec) in [(105051, 105050), (150302, 150301)] {
        let (b, r) = (&d.sp_effects[&block.to_string()], &d.sp_effects[&rec.to_string()]);
        assert!(b.sp_category == r.sp_category && b.category_priority < r.category_priority, "{block} vs {rec}");
    }
}

/// The 14 deathblow clips that fire the categories' priority-1 rows at f0 (e.g. a225_510090:
/// 105052 / 150303-150333 "release recovery prohibition") lift the Todome holds, since a row
/// that outranks a held one of its category replaces it (round 30: the holds stayed until a
/// reset).
#[test]
pub(crate) fn a_release_row_lifts_the_todome_recovery_block() {
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..5 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().held = vec![105051, 150302, 150312, 150322, 150332];
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.play("DeathblowTest", "a225_510090");
    }
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    let p = world.query::<&Player>().single(world).unwrap();
    assert!(p.held.is_empty(), "still held: {:?}", p.held);
}

/// Every combat art from idle (HKS BEH_A_GROUND_SP_ATTACK 3388): its opening state and the Spirit
/// Emblems it spends (EquipParamWeapon resourceItemA through its wepCost behaviours). Shadowrush /
/// Shadowfall (109, 2) pay only in the hit jump (judge 215), so a miss is free
/// (`shadowrush_jumps_off_the_hit`).
#[test]
pub(crate) fn every_art_starts_and_costs_its_emblems() {
    let cases: &[(i64, &str, u32)] = &[
        (5100, "GroundSpecialAttackCombo1", 0),   // Whirlwind Slash
        (5200, "GroundSpecialAttackStep_F", 0),   // Nightjar Slash
        (5300, "GroundSpecialAttackCombo1", 0),   // Ichimonji
        (5400, "GroundSpecialAttackCombo1", 2),   // Dragon Flash
        (5500, "GroundSpacialAttackHoldStart", 2), // Ashina Cross
        (5600, "GroundSpecialAttackCombo1", 0),   // Floating Passage
        (5700, "GroundSpecialAttackCombo1", 3),   // Mortal Draw
        (5800, "GroundSpecialAttackJumpReady", 0), // Senpou Leaping Kicks
        (5900, "GroundSpecialAttackCombo1", 0),   // Praying Strikes
        (6000, "GroundSpecialAttackCombo1", 0),   // Shadowrush (paid on a hit)
        (6100, "GroundSpacialAttackHoldStart", 3), // One Mind
        (7000, "GroundSpecialAttackStep_N", 0),   // Nightjar Slash Reversal
        (7100, "GroundSpecialAttackCombo1", 0),   // Ichimonji: Double
        (7200, "GroundSpecialAttackCombo1", 1),   // Spiral Cloud Passage (gate 994)
        (7300, "GroundSpecialAttackCombo1", 3),   // Empowered Mortal Draw
        (7400, "GroundSpecialAttackJumpReady", 0), // High Monk
        (7500, "GroundSpecialAttackCombo1", 0),   // Praying Strikes - Exorcism
        (7600, "GroundSpecialAttackCombo1", 0),   // Shadowfall (paid on a hit)
        (7700, "GroundSpecialAttackJumpReady", 1), // Sakura Dance
    ];
    for &(art, first, cost) in cases {
        // (300: a tapped hold art draws only once HoldStart's ref 222 opens, f21.)
        let (states, spent) = use_art(art, 10, 300);
        assert_eq!(states.first().map(String::as_str), Some(first), "{art}: {states:?}");
        assert_eq!(spent, cost, "{art}: {states:?}");
        assert_eq!(states.last().map(String::as_str), Some("StandIdle"), "{art}: {states:?}");
    }
    // The jump arts leap, fall and land (316700 -> 316710 -> LandGroundSpecialAttackJumpStart).
    let (states, _) = use_art(7400, 10, 240);
    for want in ["GroundSpecialAttackJumpStart", "LandGroundSpecialAttackJumpStart"] {
        assert!(states.iter().any(|s| s == want), "{want}: {states:?}");
    }
    // Sakura Dance without the emblems leaps on its *NoResource clips (a110_316711 / 316721:
    // 316710 / 316720's clips, own TAE events) and pays nothing.
    let (states, spent) = use_art(7700, 0, 240);
    for want in ["GroundSpecialAttackJumpReadyNoResource", "GroundSpecialAttackJumpStartNoResource"] {
        assert!(states.iter().any(|s| s == want), "{want}: {states:?}");
    }
    assert!(states.iter().any(|s| s.starts_with("LandGroundSpecialAttackJump") && s.ends_with("NoResource")), "{states:?}");
    assert_eq!(spent, 0, "{states:?}");
}

/// Shadowrush (6000) thrusts into a target 3 m off: the hit (ref 225, f44-55) jumps off it
/// (GroundSpecialAttackHitJump, a109_316600) and its judge 215 pays the 2 emblems; without them
/// there is no jump. Shadowfall (7600, unlock ref 286) slashes down out of the jump on attack
/// (ref 226) and lands into LandGroundSpecialAttackHitJumpDeriveAction (ref 201).
#[test]
pub(crate) fn shadowrush_jumps_off_the_hit() {
    let run = |art: i64, emblems: u32| {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.combat_art = art;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 3.0;
            let mut q = world.query::<&mut Player>();
            q.single_mut(world).unwrap().emblems = emblems;
        }
        for _ in 0..10 {
            app.update();
        }
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
        }
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..300 {
            if seen.last().is_some_and(|s| s == "GroundSpecialAttackHitJump") {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        let world = app.world_mut();
        let mut q = world.query::<&Player>();
        let left = q.single(world).unwrap().emblems;
        (seen, emblems - left)
    };
    let (states, spent) = run(6000, 10);
    assert!(states.iter().any(|s| s == "GroundSpecialAttackHitJump"), "{states:?}");
    assert!(!states.iter().any(|s| s == "GroundSpecialAttackHitJumpDeriveAction"), "{states:?}");
    assert_eq!(spent, 2, "{states:?}");
    assert_eq!(states.last().map(String::as_str), Some("StandIdle"), "{states:?}");
    let (states, spent) = run(6000, 0);
    assert!(!states.iter().any(|s| s.contains("HitJump")), "{states:?}");
    assert_eq!(spent, 0);
    let (states, spent) = run(7600, 10);
    for want in ["GroundSpecialAttackHitJump", "GroundSpecialAttackHitJumpDeriveAction", "LandGroundSpecialAttackHitJumpDeriveAction"] {
        assert!(states.iter().any(|s| s == want), "{want}: {states:?}");
    }
    assert_eq!(spent, 2, "{states:?}");
}

/// The art pressed in a jump (BEH_A_AIR_SP_ATTACK, `air_art_state` / `air_art_land`): Whirlwind
/// Slash (100) AirSpecialAttack -> LandAirSpecialAttack; Nightjar Slash (101) AirSpecialAttackStart
/// -> its landing; One Mind (104) the air hold, let go -> AirSpacialAttackHoldEnd; Sakura Dance (110)
/// bounces off the floor (ref 288: AirSpecialAttackLandingJumpReady -> ...LandingJumpStart ->
/// LandGroundSpecialAttackJumpAfterJumpStart), once per jump.
#[test]
pub(crate) fn arts_in_the_air() {
    let run_with = |art: i64, release_at: Option<usize>, presses: usize, emblems: u32| {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.combat_art = art;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 30.0;
            let mut q = world.query::<&mut Player>();
            q.single_mut(world).unwrap().emblems = emblems;
        }
        for _ in 0..10 {
            app.update();
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let (mut seen, mut launch, mut pressed): (Vec<String>, Option<usize>, usize) = (Vec::new(), None, 0);
        for f in 0..400 {
            if let Some(l) = launch {
                if pressed < presses && f == l + 8 + pressed * 6 {
                    pressed += 1;
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.press(Action::Attack);
                    pad.press(Action::Guard);
                    pad.attack_held = true;
                    pad.guard_held = true;
                }
                if release_at.is_some_and(|r| f == l + r) || (release_at.is_none() && f == l + 12) {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.attack_held = false;
                    pad.guard_held = false;
                }
            }
            app.update();
            let (state, _, airborne, _) = player_actor(&mut app);
            if airborne && launch.is_none() {
                launch = Some(f);
            }
            if seen.last() != Some(&state) {
                seen.push(state);
            }
        }
        seen
    };
    let run = |art: i64, release_at: Option<usize>, presses: usize| run_with(art, release_at, presses, 10);
    let has = |seen: &[String], want: &str| seen.iter().any(|s| s == want);
    let seen = run(5100, None, 1);
    for want in ["AirSpecialAttack", "LandAirSpecialAttack"] {
        assert!(has(&seen, want), "5100 {want}: {seen:?}");
    }
    let seen = run(5200, None, 1);
    assert!(has(&seen, "AirSpecialAttackStart"), "5200: {seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("LandAirSpecialAttack")), "5200: {seen:?}");
    // (Let go once HoldStart's ref 230 is up, f21-33: the air hold ends only in it.)
    let seen = run(6100, Some(8 + 23), 1);
    assert!(has(&seen, "AirSpecialAttackHoldStart"), "6100: {seen:?}");
    // (HoldEnd, or its landing when it lands in the same step.)
    assert!(seen.iter().any(|s| s.ends_with("AirSpacialAttackHoldEnd")), "6100 HoldEnd: {seen:?}");
    let seen = run(7700, None, 1);
    for want in ["AirSpecialAttackStart", "AirSpecialAttackLandingJumpReady", "AirSpecialAttackLandingJumpStart", "LandGroundSpecialAttackJumpAfterJumpStart"] {
        assert!(has(&seen, want), "7700 {want}: {seen:?}");
    }
    // Without the emblems: the *NoResource air clips (a103 / a110 316201, the art's clip with
    // their own TAE events).
    let seen = run_with(5400, None, 1, 0);
    assert!(has(&seen, "AirSpecialAttackNoResource"), "5400 no emblems: {seen:?}");
    let seen = run_with(7700, None, 1, 0);
    assert!(has(&seen, "AirSpecialAttackStartNoResource"), "7700 no emblems: {seen:?}");
    assert!(seen.iter().any(|s| s.ends_with("NoResource") && s != "AirSpecialAttackStartNoResource"), "7700 no emblems lands: {seen:?}");
    // Once per jump (g_airSpecialAttackCount, also counted by the ground leap).
    assert_eq!(crate::player::air_art_state(110, 0, true, 1, false), None);
    for seen in [run(5100, None, 1), run(7700, None, 1)] {
        assert_eq!(seen.last().map(String::as_str), Some("StandIdle"), "{seen:?}");
    }
}

/// Pressed again in the combo window (ref 223): High Monk's kicks go on out of the leap's landing
/// (HKS 5386, LAND_GROUND_SPECIAL_ATTACK_JUMP_START -> Combo2), One Mind's hold action into its
/// second cut (5379, W_GroundSpacialAttackVariationCombo2).
#[test]
pub(crate) fn arts_combo_out_of_their_jump_and_hold() {
    for (art, want, hold) in [(7400, "GroundSpecialAttackCombo2", false), (6100, "GroundSpacialAttackVariationCombo2", true)] {
        let mut app = app();
        {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.combat_art = art;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            q.single_mut(world).unwrap().translation.z = 30.0;
        }
        for _ in 0..10 {
            app.update();
        }
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
            pad.attack_held = hold;
            pad.guard_held = hold;
        }
        let mut seen: Vec<String> = Vec::new();
        for f in 0..300 {
            if hold && f == 30 {
                app.world_mut().resource_mut::<PadInput>().attack_held = false;
            }
            let s = player_state(&mut app).0;
            if f > 10 && f % 4 == 0 && (s.starts_with("Land") || s.contains("HoldAction")) {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.press(Action::Attack);
                pad.press(Action::Guard);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert!(seen.iter().any(|s| s == want), "{art}: {seen:?}");
    }
}

/// Round 53: Sakura Dance's leap from the ground spends its one air use (HKS 3445): in the leap the
/// counter is already at AIR_SP_ATTACK_COUNT_MAX, so the art pressed again there is refused.
#[test]
pub(crate) fn sakura_dances_ground_leap_counts_its_air_use() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.combat_art = 7700;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
        world.query::<&mut Player>().single_mut(world).unwrap().emblems = 10;
    }
    for _ in 0..10 {
        app.update();
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut count = None;
    for _ in 0..30 {
        app.update();
        if player_state(&mut app).0 == "GroundSpecialAttackJumpReady" {
            count = Some(app.world_mut().query::<&Player>().single(app.world()).unwrap().air_art_count);
            break;
        }
    }
    assert_eq!(count, Some(1));
}

/// One art use from idle: equips `art` with `emblems` Spirit Emblems, presses attack + guard and
/// runs `frames`; returns the states Wolf went through and the emblems spent.
pub(crate) fn use_art(art: i64, emblems: u32, frames: usize) -> (Vec<String>, u32) {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.combat_art = art;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
        let mut q = world.query::<&mut Player>();
        q.single_mut(world).unwrap().emblems = emblems;
    }
    for _ in 0..10 {
        app.update();
    }
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..frames {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    let left = {
        let world = app.world_mut();
        let mut q = world.query::<&Player>();
        q.single(world).unwrap().emblems
    };
    (seen, emblems.saturating_sub(left))
}

/// Dragon Flash (5400, spAtkcategory 103) costs EquipParamWeapon resourceItemA 2, paid by its wepCost
/// behaviour (BehaviorParam_PC 105004220); without the emblems HKS 3455 plays
/// W_GroundSpecialAttackCombo1NoResource (a103_316001) and nothing is paid.
#[test]
pub(crate) fn dragon_flash_costs_two_emblems() {
    let (states, spent) = use_art(5400, 10, 300);
    assert!(states.iter().any(|s| s == "GroundSpecialAttackCombo1"), "{states:?}");
    assert_eq!(spent, 2, "{states:?}");
    // Ashina Cross (5500, 2: 105005200 on the draw) and Mortal Draw (5700, 3: its consumption
    // dummy 105007999, TAE 2 judge 999 at f60).
    for (art, cost) in [(5500, 2), (5700, 3)] {
        let (states, spent) = use_art(art, 10, 300);
        assert_eq!(spent, cost, "{art}: {states:?}");
    }
    let (states, spent) = use_art(5400, 1, 300);
    assert!(states.iter().any(|s| s == "GroundSpecialAttackCombo1NoResource"), "{states:?}");
    assert_eq!(spent, 0);
}

/// Latent skills read from SkillParam -> SpEffectParam: Mikiri Counter posture UP (70: 150400
/// attackHitParryStaminaAttackRate 1.25), deflect posture UP (270: 150410 defStaminaAttackRate
/// 1.25), Knowledge of Medicine (170: 150200 changeHpEstusFlaskCorrectRate 1.1).
#[test]
pub(crate) fn latent_skill_rates() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut config = app.world().resource::<crate::config::GameConfig>().clone();
    config.player.skills = vec![];
    assert_eq!(crate::combat::skill_rate(combat, &config, "defStaminaAttackRate"), 1.0);
    assert_eq!(crate::player::medicine_rate(combat, &config), 1.0);
    config.player.skills = vec![70, 270, 170];
    assert_eq!(crate::combat::skill_rate(combat, &config, "attackHitParryStaminaAttackRate"), 1.25);
    assert_eq!(crate::combat::skill_rate(combat, &config, "defStaminaAttackRate"), 1.25);
    assert!((crate::player::medicine_rate(combat, &config) - 1.1).abs() < 1e-4);
    // Each Knowledge of Medicine adds 150210 accumuVal 1; the stack climbs 150200's
    // accumuOverFireId chain (150201-150204 need 2-5): two skills 1.2, all five 1.5.
    config.player.skills = vec![170, 171];
    assert!((crate::player::medicine_rate(combat, &config) - 1.2).abs() < 1e-4);
    config.player.skills = vec![170, 171, 600, 601, 602];
    assert!((crate::player::medicine_rate(combat, &config) - 1.5).abs() < 1e-4);
    // Covert A / B as typed rows (for the stealth cuts).
    config.player.skills = vec![60, 61];
    let rows = crate::combat::skill_sp_effect_rows(combat, &config);
    assert!(rows.iter().any(|s| s.sight_search_enemy_cut == 20.0), "covert A");
    assert!(rows.iter().any(|s| (s.hearing_search_enemy_rate - 0.5).abs() < 1e-4), "covert B");
}

/// The Finger Whistle is an upper-body action (STATE_TYPE_UPPER_ACTION_ATK): pressed while walking
/// it is W_GroundSubAttackCombo1Move (HKS 3731) and Wolf keeps walking under it.
#[test]
pub(crate) fn the_finger_whistle_plays_while_walking() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![79000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..40 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    app.update();
    let start = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Transform, With<Player>>();
        q.single(world).unwrap().translation
    };
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..30 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    let end = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Transform, With<Player>>();
        q.single(world).unwrap().translation
    };
    assert!(seen.iter().any(|s| s == "GroundSubAttackCombo1Move"), "{seen:?}");
    assert!((end - start).with_y(0.0).length() > 0.5, "stood still: {seen:?} {start} -> {end}");
}

/// Crouched, the whistle is CrouchSubAttackCombo1Moveable (also STATE_TYPE_UPPER_ACTION_ATK): Wolf
/// keeps crouch-walking under it and stays crouched.
#[test]
pub(crate) fn the_finger_whistle_plays_while_crouch_walking() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![79000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..60 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..40 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    app.update();
    let pos = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
    };
    let start = pos(&mut app);
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..30 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    let end = pos(&mut app);
    let crouched = {
        let world = app.world_mut();
        world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().crouch
    };
    assert!(seen.iter().any(|s| s == "CrouchSubAttackCombo1Moveable"), "{seen:?}");
    assert!((end - start).with_y(0.0).length() > 0.3, "stood still: {seen:?} {start} -> {end}");
    assert!(crouched, "{seen:?}");
}

/// The enemy chrs in play (sorted).
/// The enemies in the fight (not those a map script disabled).
pub(crate) fn enemy_chrs(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let kinds: Vec<String> = world.resource::<Combat>().kinds.iter().map(|k| k.foe.chr.clone()).collect();
    let mut v: Vec<String> = world.query::<(&Actor, &Enemy)>().iter(world).filter(|(_, e)| !e.is_disabled()).map(|(a, _)| kinds[a.kind].clone()).collect();
    v.sort();
    v
}

/// Several enemies of different kinds at once (config enemy.group): each with its own chr, rows
/// and data, all on the enemy side (no hits between them), all after Wolf.
#[test]
pub(crate) fn group_of_different_enemies_fights_together() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1010.json")).exists() {
        return;
    }
    let g = |chr: &str, at: [f32; 2]| crate::config::GroupEnemy { chr: chr.into(), npc_row: None, at: Some(at), msb: None };
    let mut app = app_cfg("c1020", None, vec![g("c1010", [2.5, 3.0]), g("c1020", [-2.5, 3.0])]);
    assert_eq!(enemy_chrs(&mut app), ["c1010", "c1020", "c1020"]);
    {
        let world = app.world_mut();
        for mut e in world.query::<&mut Enemy>().iter_mut(world) {
            e.aggressive = true;
        }
    }
    // Wolf stands still; only he gets hurt.
    let mut attacked = 0;
    for _ in 0..900 {
        app.update();
        let world = app.world_mut();
        attacked += world.query_filtered::<&Actor, With<Enemy>>().iter(world).filter(|a| a.anim.get(5..8) == Some("003")).count();
        for a in world.query_filtered::<&Actor, With<Enemy>>().iter(world) {
            assert_eq!(a.hp, a.hp_max, "an enemy hurt another: {} {}", a.state, a.anim);
        }
    }
    assert!(attacked > 0, "nobody attacked");
}

/// Two broken Generals: the first in query order far off (out of reach), the second in reach in
/// front of Wolf. The 忍殺 mark sits on the one in reach, and the deathblow takes that one too
/// (hud.rs deathblow_mark_target and the attack share player.rs deathblow_pick).
#[test]
pub(crate) fn the_deathblow_mark_is_on_the_enemy_the_deathblow_takes() {
    use bevy::ecs::system::RunSystemOnce;
    let g = crate::config::GroupEnemy { chr: "c1020".into(), npc_row: None, at: Some([0.0, 6.0]), msb: None };
    let mut app = app_cfg("c1020", None, vec![g]);
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    let order: Vec<Entity> = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Enemy>>().iter(world).collect()
    };
    assert_eq!(order.len(), 2);
    let (far, near) = (order[0], order[1]);
    // Ahead of Wolf (+Z of his real start), facing him (yaw 0 = -Z, Actor::forward); the HUD
    // reads GlobalTransforms (no propagation in the sim app).
    let place = |app: &mut App| {
        let world = app.world_mut();
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        for (e, dz) in [(far, 8.0), (near, 1.2)] {
            let at = wolf + Vec3::Z * dz;
            world.get_mut::<Actor>(e).unwrap().yaw = 0.0;
            world.get_mut::<Transform>(e).unwrap().translation = at;
            *world.get_mut::<GlobalTransform>(e).unwrap() = GlobalTransform::from_translation(at);
        }
        *world.query_filtered::<&mut GlobalTransform, With<Player>>().single_mut(world).unwrap() = GlobalTransform::from_translation(wolf);
    };
    place(&mut app);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            for e in [far, near] {
                let mut q = world.query::<(&mut Actor, &mut Enemy)>();
                let (mut a, mut en) = q.get_mut(world, e).unwrap();
                en.on_posture_break(&mut a, &combat, false, 3.0);
            }
        });
    }
    app.update();
    place(&mut app);
    let target = app
        .world_mut()
        .run_system_once(
            |combat: Res<Combat>, wolf: Query<(&Actor, &GlobalTransform), With<Player>>, enemies: crate::hud::MarkEnemies, dummies: Query<&GlobalTransform>| {
                let (wa, wg) = wolf.single().unwrap();
                crate::hud::deathblow_mark_target(&combat, wa, wg.translation(), &enemies, &dummies, None)
            },
        )
        .unwrap();
    assert_eq!(target, Some(near), "the mark is on the far enemy (far {far:?}, near {near:?})");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut thrown = None;
    for _ in 0..30 {
        app.update();
        let world = app.world_mut();
        for e in [far, near] {
            if world.get::<Actor>(e).unwrap().state.starts_with("ThrowDef") {
                thrown.get_or_insert(e);
            }
        }
    }
    assert_eq!(thrown, Some(near), "the deathblow took another enemy");
}

/// A fight reset despawns every enemy and brings back the config's line-up; the sandbox's own
/// line-up (SHINOBI_SANDBOX_ENEMIES) stands again too (sandbox.rs respawn_lineup).
#[test]
pub(crate) fn a_reset_brings_the_sandbox_line_up_back() {
    let mut app = app();
    app.add_systems(Update, crate::sandbox::respawn_lineup);
    app.world_mut().insert_resource(crate::sandbox::Lineup(vec!["c1020".into()]));
    let lineup = |app: &mut App| -> Vec<Entity> {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<crate::sandbox::SandboxEnemy>>().iter(world).collect()
    };
    for _ in 0..3 {
        app.update();
    }
    let before = lineup(&mut app);
    assert_eq!(before.len(), 1, "no line-up to begin with");
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    for _ in 0..5 {
        app.update();
    }
    let after = lineup(&mut app);
    assert_eq!(after.len(), 1, "the reset lost the sandbox line-up");
    assert_ne!(after, before, "the old line-up was not reset");
}

/// NpcParam teamType: an Interior Ministry samurai of the invasion (c1700 17001000, team 24
/// "Enemy 2") and an Ashina soldier (c1010, team 6) fight each other while Wolf is far off
/// (combat::teams_hostile).
#[test]
pub(crate) fn the_invaders_fight_ashina() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1700.json")).exists() {
        return;
    }
    let g = crate::config::GroupEnemy { chr: "c1700".into(), npc_row: Some(17001000), at: Some([0.0, 6.0]), msb: None };
    let mut app = app_cfg("c1010", None, vec![g]);
    {
        let world = app.world_mut();
        for mut e in world.query::<&mut Enemy>().iter_mut(world) {
            e.aggressive = true;
        }
        let mut tf = world.query_filtered::<&mut Transform, With<Player>>().single_mut(world).unwrap();
        tf.translation.z = -60.0;
    }
    let mut hurt = false;
    for _ in 0..(60 * 30) {
        app.update();
        let world = app.world_mut();
        hurt |= world.query_filtered::<&Actor, With<Enemy>>().iter(world).any(|a| a.hp < a.hp_max);
        if hurt {
            break;
        }
    }
    assert!(hurt, "they did not fight:
{}", log_text(&app));
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Player>>();
    let wolf = q.single(world).unwrap();
    assert_eq!(wolf.hp, wolf.hp_max, "Wolf was not in it");
}

/// Genichiro (c7100 71001000) at 0 health bars: after his death his map event (m11_01 11115820)
/// disables him and enables the Way of Tomoe (c7110 71100000) where the MSB has him, and warps
/// Wolf to the cutscene's spot.
#[test]
pub(crate) fn genichiro_becomes_way_of_tomoe() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7100", Some(71001000), Vec::new());
    assert!(app.world().resource::<Combat>().kind_index("c7110", Some(71100000)).is_some());
    let wolf_at = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
    };
    let before = wolf_at(&mut app);
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let (mut e, mut a) = world.query::<(&mut Enemy, &mut Actor)>().single_mut(world).unwrap();
            e.ninsatsu.0 = 1;
            e.deathblow(&mut a, &combat, 12000);
        });
    }
    let mut swapped = false;
    for _ in 0..1200 {
        app.update();
        if enemy_chrs(&mut app) == ["c7110"] {
            swapped = true;
            break;
        }
    }
    assert!(swapped, "no Tomoe: {:?}", enemy_chrs(&mut app));
    app.update();
    let after = wolf_at(&mut app);
    assert!(before.distance(after) > 1.0, "Wolf not warped: {before} -> {after}");
    // Wolf's reset (R, death, debug menu) starts the fight over: Genichiro comes back.
    app.world_mut().resource_mut::<crate::player::PadInput>().reset = true;
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(enemy_chrs(&mut app), ["c7100"]);
}

/// The reservoir's Way of Tomoe (71101000) hands over to Isshin: 11125830 enables Isshin 1120860
/// and sets Tomoe's 9311 in one go. A 93xx flag with the boss at 0 bars counted as the defeat,
/// so the fight ended (Won / restart) instead of going on with Isshin.
#[test]
pub(crate) fn reservoir_tomoe_hands_over_to_isshin() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5400.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7110", Some(71101000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    break_and_deathblow(&mut app, 600);
    let mut isshin = false;
    for _ in 0..1800 {
        app.update();
        if enemy_chrs(&mut app).iter().any(|c| c == "c5400") {
            isshin = true;
            break;
        }
    }
    assert!(isshin, "no Isshin: {:?}", enemy_chrs(&mut app));
    for _ in 0..60 * 6 {
        app.update();
    }
    let bs = app.world().resource::<crate::enemy::BossScript>();
    assert!(bs.defeats == 0 && bs.run.as_ref().is_some_and(|r| r.defeated.is_none()), "the fight ended at the hand-over");
    assert!(enemy_chrs(&mut app).iter().any(|c| c == "c5400"));
}

/// The prologue Genichiro (71000000) is a scripted loss: with Wolf in region 1122802, 11125812
/// makes him immortal (2004[12] on 10000; WolfImmortal for the player side), and 11125800 ends
/// the fight at Wolf's 1 HP there (9300 with Genichiro still up): the duel starts over. An
/// overkill on the immortal Wolf stops at 1 HP (player.rs wolf_no_dead, round 73).
#[test]
pub(crate) fn the_prologue_genichiro_is_a_scripted_loss() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7100.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7100", Some(71000000), Vec::new());
    app.update();
    let spot = app.world().resource::<crate::enemy::BossScript>().run.as_ref().unwrap().place_of(1122802).unwrap().0;
    let mut immortal = false;
    let mut reset = false;
    let mut low = f32::MAX;
    for i in 0..600 {
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Transform, &mut Actor), With<Player>>();
            let (mut tf, mut a) = q.single_mut(world).unwrap();
            tf.translation = spot;
            if i == 120 {
                a.hp -= 10000.0;
            }
        }
        app.update();
        immortal |= app.world().resource::<crate::enemy::script::WolfImmortal>().0;
        {
            let world = app.world_mut();
            low = low.min(world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp);
        }
        if app.world().resource::<crate::enemy::BossScript>().run.as_ref().is_none_or(|r| r.flag(9300)) {
            reset = true;
            break;
        }
    }
    assert!(immortal, "Wolf never immortal in 1122802");
    assert!(reset, "no 9300");
    assert!(low >= 1.0, "Wolf went to {low} HP");
    // The duel starts over: a fresh script run (9300 off again).
    for _ in 0..10 {
        app.update();
    }
    assert!(app.world().resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| !r.flag(9300)), "no scripted loss");
}

/// Emma (74000010) is followed by Isshin Ashina: after her defeat event 11115900 (flag), 11115930
/// enables him (1110920, c5430). The exporter left him out of her cast (his entry waits on a
/// flag, not on a condition on her), and her handled defeat ended the fight.
#[test]
pub(crate) fn isshin_ashina_follows_emma() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5430.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7400", Some(74000010), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    let mut isshin = false;
    'blows: for _ in 0..4 {
        break_and_deathblow(&mut app, 600);
        for _ in 0..60 * 8 {
            app.update();
            if enemy_chrs(&mut app).iter().any(|c| c == "c5430") {
                isshin = true;
                break 'blows;
            }
        }
    }
    let flags = app.world().resource::<crate::enemy::BossScript>().run.as_ref().map(|r| r.flags_on()).unwrap_or_default();
    assert!(isshin, "no Isshin Ashina: {:?} flags {flags:?}", enemy_chrs(&mut app));
    for _ in 0..60 * 6 {
        app.update();
    }
    assert!(enemy_chrs(&mut app).iter().any(|c| c == "c5430"), "the fight ended: {:?}", enemy_chrs(&mut app));
    // His entrance 11115930 unloads Emma (2004[29] Set Character Backread State 1110900, 1):
    // she leaves the field, so his deathblows are his.
    let world = app.world_mut();
    let combat = world.resource::<Combat>().kind_index("c7400", Some(74000010));
    let mut q = world.query::<(&Enemy, &Actor)>();
    let emma: Vec<bool> = q.iter(world).filter(|(_, a)| Some(a.kind) == combat).map(|(e, _)| e.disabled).collect();
    assert!(!emma.is_empty() && emma.iter().all(|d| *d), "Emma still on the field: {emma:?}");
}

/// The Corrupted Monk (c5000 50000000) at 2 health bars enables her three phantoms (c5005,
/// m25 12505961). They stay out of the fight until she has SpEffect 5031 (her Attack3032 /
/// 3033), then come in one after another (1 / 2 / 3.5 s, plus 1 s when she sent message 50)
/// where the event's region checks put them, and leave on their message 70 or when it ends.
/// The Monk's phantoms are seen only through their a200 attacks: TAE 193 SetOpacityKeyframe
/// fades them in at the start and out at the end (Actor::opacity, model.rs update_opacity), and
/// they stay at 0 until the next one.
#[test]
pub(crate) fn monk_phantoms_fade_in_and_out() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5005.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5000", Some(50000000), Vec::new());
    {
        let world = app.world_mut();
        let mut e = world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap();
        e.ninsatsu.0 = 2;
        e.msg = Some(50);
        e.aggressive = true;
    }
    for _ in 0..5 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single_mut(world).unwrap();
        a.resident.push(5031);
    }
    // (anim, opacity) of the phantoms in the fight, every frame.
    let mut seen: Vec<(String, f32)> = Vec::new();
    for _ in 0..(60 * 12) {
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::Phantom, &Actor)>();
        seen.extend(q.iter(world).filter(|(p, _)| !p.parked()).map(|(_, a)| (a.anim.clone(), a.opacity)));
    }
    let kinds: std::collections::BTreeSet<&String> = seen.iter().map(|o| &o.0).collect();
    let fading = |k: &str| seen.iter().any(|(a, o)| a == k && *o > 0.05 && *o < 0.95);
    let attack = kinds.iter().find(|k| k.starts_with("a200_003")).unwrap_or_else(|| panic!("no a200 attack: {kinds:?}"));
    assert!(fading(attack), "{attack} does not fade: {kinds:?}");
    // Before its first attack it walks unseen (its a200 idle put it at 0).
    let first = seen.iter().position(|(a, _)| a.starts_with("a200_003")).unwrap();
    assert!(seen[..first].iter().filter(|(a, _)| !a.is_empty()).all(|(_, o)| *o < 0.01), "seen before attacking: {:?}", &seen[..first.min(20)]);
}

#[test]
pub(crate) fn monk_phantoms_come_and_go() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5005.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5000", Some(50000000), Vec::new());
    {
        let world = app.world_mut();
        let mut e = world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap();
        e.ninsatsu.0 = 2;
        // Her 3032 sends event message 50 with 5031: the event's first 1 s wait runs.
        e.msg = Some(50);
    }
    for _ in 0..5 {
        app.update();
    }
    let phantoms = |app: &mut App| {
        let world = app.world_mut();
        let mut v: Vec<bool> = world.query::<&crate::enemy::Phantom>().iter(world).map(|p| p.parked()).collect();
        v.sort();
        v
    };
    assert_eq!(phantoms(&mut app), [true, true, true]);
    let set_sp = |app: &mut App, on: bool| {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single_mut(world).unwrap();
        a.resident.retain(|&id| id != 5031);
        if on {
            a.resident.push(5031);
        }
    };
    set_sp(&mut app, true);
    for _ in 0..(60 * 2 + 10) {
        app.update();
    }
    assert_eq!(phantoms(&mut app), [false, true, true], "the first one at 2 s");
    // Where it came in: the event's region checks (m25 2502856-9, strips across the bridge in
    // the boss frame). Wolf 8 m ahead of the Monk's start is in strip 4 (2502859) only, so the
    // event falls through to label 1: the points 17.4 m behind her (2502860-2).
    let active_z = |app: &mut App| {
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::Phantom, &Transform)>();
        q.iter(world).find(|(p, _)| !p.parked()).map(|(_, tf)| tf.translation.z).unwrap()
    };
    let z = active_z(&mut app);
    assert!(z < -4.0 - 14.0, "strip 4 -> points behind the Monk: z {z}");
    for _ in 0..(60 * 3) {
        app.update();
    }
    assert_eq!(phantoms(&mut app), [false, false, false], "all three by 4.5 s");
    set_sp(&mut app, false);
    for _ in 0..(60 * 8) {
        app.update();
    }
    assert_eq!(phantoms(&mut app), [true, true, true], "they leave");
    // Wolf in strip 2 (2502857, 15 m behind her start): the other group, 2 m ahead of her.
    // Straight into 3033 (5031 without message 50): no first wait, the first one at 1 s.
    {
        let world = app.world_mut();
        let mut tf = world.query_filtered::<&mut Transform, With<Player>>().single_mut(world).unwrap();
        tf.translation.z = -4.0 - 15.0;
        let mut e = world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap();
        e.msg = None;
    }
    set_sp(&mut app, true);
    for _ in 0..(60 + 10) {
        app.update();
    }
    assert_eq!(phantoms(&mut app), [false, true, true], "the first one at 1 s");
    let z = active_z(&mut app);
    assert!(z > -4.0 - 6.0, "strip 2 -> points ahead of the Monk: z {z}");
    // Its own event message 70 (the end of its a200 attacks) ends its turn: it leaves.
    let first = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::enemy::Phantom, &mut Enemy)>();
        let (ent, _, mut e) = q.iter_mut(world).find(|(_, p, _)| !p.parked()).unwrap();
        e.msg = Some(70);
        ent
    };
    let mut left = false;
    for _ in 0..(60 * 8) {
        app.update();
        if app.world().get::<crate::enemy::Phantom>(first).is_some_and(|p| p.parked()) {
            left = true;
            break;
        }
    }
    assert!(left, "message 70 did not end its turn");
}

/// The Mist Raven's leap direction (_SetJumpDirection, PRM_GROUND_JUMP_*_STICK_RANGE): forward is
/// +-18.75 deg, an exact border between the side sectors falls to B, no stick = V.
#[test]
pub(crate) fn mist_raven_leap_sectors_follow_the_hks() {
    use crate::prosthetic::hks::jump_start;
    for (a, want) in [
        (None, "SubAttackJumpStart_V"),
        (Some(18.75), "SubAttackJumpStart_F"),
        (Some(-18.0), "SubAttackJumpStart_F"),
        (Some(20.0), "SubAttackJumpStart_FR"),
        (Some(90.0), "SubAttackJumpStart_R"),
        (Some(67.5), "SubAttackJumpStart_B"),
        (Some(150.0), "SubAttackJumpStart_BR"),
        (Some(180.0), "SubAttackJumpStart_B"),
        (Some(-150.0), "SubAttackJumpStart_BL"),
        (Some(-90.0), "SubAttackJumpStart_L"),
        (Some(-40.0), "SubAttackJumpStart_FL"),
    ] {
        assert_eq!(jump_start(a), want, "{a:?}");
    }
}

/// env(3033) through the data (`combat::action_unlocked`): with `unlock_all_moves` off only the
/// learned skills open their moves (skill 5 -> 200400 -> action26 the slide; 6 -> 200500 ->
/// action25 the air arts); without skill 5 a crouch out of a sprint is not the slide.
#[test]
pub(crate) fn skills_unlock_their_moves() {
    use crate::combat::*;
    let mut app = app();
    {
        let world = app.world_mut();
        let mut config = world.resource_mut::<crate::config::GameConfig>();
        config.player.unlock_all_moves = false;
        config.player.skills = vec![6];
    }
    {
        let world = app.world();
        let (combat, config) = (world.resource::<Combat>(), world.resource::<crate::config::GameConfig>());
        assert!(action_unlocked(combat, config, UNLOCK_AIR_SP_ATTACK));
        assert!(!action_unlocked(combat, config, UNLOCK_SPRINT_TO_CROUCH));
        assert!(!action_unlocked(combat, config, UNLOCK_AIR_DEFLECT_GUARD));
    }
    force_player(&mut app, "SprintLoop", 3.0);
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 1.0);
        pad.dodge_held = true;
        pad.press(Action::Crouch);
    }
    app.update();
    assert_ne!(player_state(&mut app).0, "SprintToCrouchReady");
    // Round 65: it is the plain crouch instead (HKS 4500-4509): moving -> the crouch move loop.
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(a.crouch && a.state == "Locomotion", "state {} crouch {}", a.state, a.crouch);
}

/// R36 B: a blue (weak, spAttribute 10) lightning hit while Wolf's air art holds ref 289
/// (SpEffect 100235) takes c0000_transition's empty branch (1099-1103): no charge, no reaction.
/// We charged him for every airborne 6 / 10 hit. The strong lightning (6) still charges.
#[test]
pub(crate) fn an_air_art_shrugs_off_blue_lightning() {
    let mut app = app();
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        let d = &combat.player;
        let (key, t) = d
            .anims
            .keys()
            .filter(|k| k.starts_with("a110_3162") || k.starts_with("a110_3167"))
            .find_map(|k| (0..120).map(|i| i as f32 / 30.0).find(|t| d.sp_effects_at(k, *t).iter().any(|(id, _)| *id == 100235)).map(|t| (k.clone(), t)))
            .expect("an air art with 100235");
        a.play("AirArt", &key);
        a.t = t;
        let blue = crate::data::Attack { sp_attribute: 10, ..Default::default() };
        let strong = crate::data::Attack { sp_attribute: 6, ..Default::default() };
        assert!(crate::combat::weak_charge_armour(&combat, &a, &blue), "{key} at {t}");
        assert!(!crate::combat::weak_charge_armour(&combat, &a, &strong));
        a.play("Idle", "a000_000000");
        assert!(!crate::combat::weak_charge_armour(&combat, &a, &blue), "not outside the art");
    });
}

/// R36 B in resolve: an enemy's blue lightning (spAttribute 10, charge SpEffect 9490) striking
/// Wolf in an air art under ref 289 (100235) takes c0000_transition's empty branch (1099-1103 in
/// the air, 1157 standing): no charge, no shock, no reaction, the art goes on. The General's
/// attacks are made blue; Wolf is held in the art, on the ground and 0.5 m up.
#[test]
pub(crate) fn blue_lightning_leaves_an_air_art_going() {
    for air in [false, true] {
        blue_lightning_on_an_air_art(air);
    }
}

pub(crate) fn blue_lightning_on_an_air_art(air: bool) {
    let mut app = app();
    for atk in app.world_mut().resource_mut::<Combat>().kinds[0].data.attacks.values_mut() {
        (atk.sp_attribute, atk.sp_effect_id0) = (10, 9490);
    }
    // Wolf's art lands no reaction (attacks carry no armour of their own, so a flinch would end
    // the enemy's combo before its lightning reaches him).
    for atk in app.world_mut().resource_mut::<Combat>().player.attacks.values_mut() {
        atk.dmg_level = 0;
    }
    app.update();
    let (key, t) = {
        let d = &app.world().resource::<Combat>().player;
        let mut keys: Vec<&String> = d.anims.keys().filter(|k| k.starts_with("a110_3162") || k.starts_with("a110_3167")).collect();
        keys.sort();
        keys.into_iter()
            .find_map(|k| {
                // The middle of its 100235 window (the anim moves on before the hit resolves).
                let on: Vec<f32> = (0..120).map(|i| i as f32 / 30.0).filter(|t| d.sp_effects_at(k, *t).iter().any(|(id, _)| *id == 100235)).collect();
                Some((k.clone(), (on.first()? + on.last()?) / 2.0))
            })
            .expect("an air art with 100235")
    };
    let ground = {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation.y
    };
    // Held airborne at ground height, he lands at once (LandVerticalGroundJump), so 0.5 m up.
    let hold = |app: &mut App| {
        let world = app.world_mut();
        let (mut tf, mut a) = world.query_filtered::<(&mut Transform, &mut Actor), With<Player>>().single_mut(world).unwrap();
        if a.anim != key {
            a.play("AirArt", &key);
        }
        (a.prev_t, a.t) = (t, t);
        if air {
            (a.airborne, a.move_vel, tf.translation.y) = (true, Vec3::ZERO, ground + 0.5);
        }
    };
    start_enemy_combo(&mut app, 0);
    for _ in 0..240 {
        hold(&mut app);
        app.update();
        if log_text(&app).contains("enemy hit") {
            let out = log_text(&app);
            assert!(!out.contains("LIGHTNING CAUGHT") && !out.contains("SHOCKED"), "air {air}: {out}");
            assert_eq!(player_state(&mut app).1, key, "air {air}: {out}");
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            assert!(a.electro.is_none() && a.shock.is_none(), "air {air}: {out}");
            return;
        }
    }
    panic!("air {air}: never hit: {}", log_text(&app));
}

#[test]
pub(crate) fn emma_killed_from_her_deflect_break_does_not_get_up() {
    // Emma (c7400, map-scripted) killed out of her deflect break pose: ThrowDef12110 has flag 69
    // (ThrowType5) at TAE 63, where the HKS (c9997 ThrowDefCommonFunction, env 276) goes to
    // ThrowDefDeath12111. The scripted path waited for the 9 s clip to end: she stood up first.
    let mut app = app_cfg("c7400", Some(74000010), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    start_enemy_combo(&mut app, 0);
    let mut pressed = false;
    let mut pose = None;
    let mut killed = None;
    for i in 0..60 * 20u32 {
        if pose.is_none() {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
            for mut a in q.iter_mut(world) {
                a.posture = a.posture_max - 1.0;
            }
        }
        if !pressed {
            if let Some(dt) = time_to_enemy_hit(&mut app) {
                if dt <= 3.0 / TAE_FPS {
                    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
                    pressed = true;
                }
            }
        }
        app.update();
        let ps = player_state(&mut app).0;
        let es = enemy_state(&mut app).0;
        if ps == "ThrowBreak" && pose.is_none() {
            pose = Some(i);
        }
        if pose.is_some_and(|p| i == p + 36) {
            app.world_mut().resource_mut::<PadInput>().guard_held = false;
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        if es == "ThrowDef12110" && killed.is_none() {
            killed = Some(i);
        }
        if es == "ThrowDefDeath12111" {
            let k = killed.expect("12110 first");
            // Flag 69 at 63/30 s; the clip is 9 s.
            assert!(((i - k) as f32 / 60.0) < 3.0, "{} s in 12110", (i - k) as f32 / 60.0);
            return;
        }
    }
    panic!("pose {pose:?} killed {killed:?}, enemy {}", enemy_state(&mut app).0);
}

/// Senpou Leaping Kicks / High Monk locked on a close enemy: the leap's 920 row 10010 homes with
/// ChrPhysicsHomingParam 20000 (initialVelocityCorrection 2 slows it to land 1 m in front of the
/// target), so Wolf lands short of the enemy instead of flying the row's 6.4 m/s over him. Unlocked
/// the leap is the row's (the skills recording: ~5-6 m at ~6.4 m/s with no lock).
#[test]
pub(crate) fn leaping_kicks_land_in_front_of_the_lock_target() {
    for (locked, art) in [(true, 5800), (true, 7400), (false, 5800)] {
        let mut app = app();
        if !app.world().contains_resource::<crate::camera::LockOn>() {
            app.world_mut().insert_resource(crate::camera::LockOn::default());
        }
        let (start, fwd, enemy) = {
            let world = app.world_mut();
            world.resource_mut::<crate::config::GameConfig>().player.combat_art = art;
            world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
            let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
            let (start, fwd) = (t.translation, a.forward());
            let mut q = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>();
            let (e, mut et) = q.single_mut(world).unwrap();
            // Unlocked, off his path: a body in the way now stops a jump (player.rs, the body
            // push in the air), which is not what this measures.
            et.translation = start + if locked { fwd * 3.0 } else { fwd.cross(Vec3::Y).normalize() * 4.0 };
            // Facing Wolf.
            et.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, -fwd);
            if locked {
                world.resource_mut::<crate::camera::LockOn>().target = Some(e);
            }
            (start, fwd, e)
        };
        let _ = enemy;
        for _ in 0..5 {
            app.update();
        }
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Attack);
            pad.press(Action::Guard);
        }
        let mut landed = None;
        let mut was_air = false;
        for _ in 0..200 {
            app.update();
            let world = app.world_mut();
            let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
            if was_air && !a.airborne {
                landed = Some((t.translation - start).dot(fwd));
                break;
            }
            was_air |= a.airborne;
        }
        let got = landed.unwrap_or_else(|| panic!("{art}: never landed"));
        if locked {
            // 3 m to the enemy, 1 m short of it (+ the 0.05 the enemy stands higher): ~2 m.
            assert!((got - 2.0).abs() < 0.35, "{art} locked: landed {got:.2} m ahead");
        } else {
            assert!(got > 3.5, "unlocked: the row's leap, landed {got:.2} m ahead");
        }
    }
}

/// Boss audit against the live recordings of the real game (tools/boss_audit.py, docs/kb/recordings.md):
/// `SHINOBI_AUDIT=<out.tsv> SHINOBI_ENEMIES=<chr> SHINOBI_ROW=<npc row>`. Per phase, the boss
/// fights an unkillable Wolf who walks up to it for SHINOBI_AUDIT_SECS (default 90) s, then is
/// broken and deathblown into the next phase (break_and_deathblow), until its deathblows run out.
/// Writes one line per anim any character starts (`anim phase s who anim`), each hit on Wolf
/// (`hit phase s who anim`) and both fighters' places every second (`pos phase s who x y z`).
/// Real models and dummy polys (app_geom).
#[test]
#[ignore]
pub(crate) fn boss_audit() {
    let out = std::env::var("SHINOBI_AUDIT").expect("SHINOBI_AUDIT=<file>");
    let chr = std::env::var("SHINOBI_ENEMIES").unwrap_or("c7100".into());
    let row = std::env::var("SHINOBI_ROW").ok().and_then(|v| v.parse().ok());
    let secs: f32 = std::env::var("SHINOBI_AUDIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(90.0);
    let mut app = app_geom(&chr, row);
    let mut lines: Vec<String> = Vec::new();
    let mut last: std::collections::HashMap<Entity, String> = Default::default();
    let mut clock = 0.0f32;
    let log = |app: &mut App, lines: &mut Vec<String>, last: &mut std::collections::HashMap<Entity, String>, phase: u32, clock: f32, pos: bool| {
        let world = app.world_mut();
        let mut rows = Vec::new();
        let mut q = world.query::<(Entity, &Actor, &Transform, Has<Player>)>();
        let combat = world.resource::<Combat>();
        for (ent, a, t, wolf) in q.iter(world) {
            let who = if wolf { "wolf".to_string() } else { combat.kind(a.kind).foe.chr.clone() };
            if last.get(&ent) != Some(&a.anim) && !a.anim.is_empty() {
                rows.push(format!("anim\t{phase}\t{clock:.2}\t{who}\t{}", a.anim));
            }
            if pos {
                rows.push(format!("pos\t{phase}\t{clock:.2}\t{who}\t{:.1}\t{:.1}\t{:.1}", t.translation.x, t.translation.y, t.translation.z));
            }
            last.insert(ent, a.anim.clone());
        }
        let mut q = world.query::<(Entity, &crate::enemy_bullet::EnemyBullet)>();
        for (ent, b) in q.iter(world) {
            if last.insert(ent, String::new()).is_some() {
                continue;
            }
            rows.push(format!("bullet\t{phase}\t{clock:.2}\t-\t{}", b.row()));
        }
        lines.extend(rows);
    };
    // Wolf can't die or break (true when he was hit this frame).
    fn keep_wolf(app: &mut App) -> bool {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        let hit = a.hp < a.hp_max;
        a.hp = a.hp_max;
        a.posture = 0.0;
        hit
    }
    // The fight is won: a map script's Handle Boss Defeat, or no boss standing for 15 s (a
    // boss hidden between phases, Lady Butterfly's illusions or Isshin before he comes in, is
    // not a win).
    fn won(app: &mut App, idle: &mut u32) -> bool {
        let world = app.world_mut();
        if world.resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some()) {
            return true;
        }
        let mut q = world.query::<(&Enemy, Has<crate::enemy::Cast>)>();
        let standing = q.iter(world).any(|(e, cast)| !e.disabled && !e.is_dead() && (!cast || e.ninsatsu.1 > 0));
        *idle = if standing { 0 } else { *idle + 1 };
        *idle > 900
    }
    let mut idle = 0u32;
    let mut over = false;
    for phase in 0..12u32 {
        {
            let world = app.world_mut();
            let boss = boss_in_play(world);
            world.get_mut::<Enemy>(boss).unwrap().aggressive = true;
        }
        for f in 0..(secs * 60.0) as u32 {
            app.update();
            clock += 1.0 / 60.0;
            let hit = keep_wolf(&mut app);
            let anim = {
                let world = app.world_mut();
                let boss = boss_in_play(world);
                let et = world.get::<Transform>(boss).unwrap().translation;
                // The boss keeps its health and posture: the phases change only on the forced
                // deathblows below.
                let mut ba = world.get_mut::<Actor>(boss).unwrap();
                ba.hp = ba.hp_max;
                ba.posture = 0.0;
                let anim = ba.anim.clone();
                let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
                let (mut a, mut t) = q.single_mut(world).unwrap();
                let to = (et - t.translation).with_y(0.0);
                // 8 s of every 20 Wolf keeps about 15 m off (the AI's ranged moves: Genichiro's
                // bow 3036 / 3009 and lightning 3044); the rest he walks up.
                let away = f % 1200 >= 720;
                if away && to.length() < 15.0 {
                    t.translation -= to.normalize_or_zero() * 6.0 / 60.0;
                } else if !away && to.length() > 2.5 {
                    t.translation += to.normalize() * if to.length() > 10.0 { 6.0 } else { 3.0 } / 60.0;
                }
                a.yaw = f32::atan2(-to.x, -to.z);
                // Wolf swings every 1.5 s when close: the AI's deflects (3100 / 3101) and
                // backsteps (5201 / 5211, common_func_NTC.lua) answer his attacks.
                if !away && f % 90 == 45 && to.length() < 3.5 {
                    world.resource_mut::<PadInput>().press(Action::Attack);
                }
                anim
            };
            if hit {
                lines.push(format!("hit\t{phase}\t{clock:.2}\tboss\t{anim}"));
            }
            log(&mut app, &mut lines, &mut last, phase, clock, f % 60 == 0);
            if won(&mut app, &mut idle) {
                over = true;
                break;
            }
        }
        if over {
            lines.push(format!("won\t{phase}\t{clock:.2}\tboss\t-"));
            break;
        }
        // break_and_deathblow, logging each frame; again (up to 3 times) when something knocked
        // Wolf out of it (the Demon of Hatred's fire on the floor).
        let (boss, left) = {
            let world = app.world_mut();
            let boss = boss_in_play(world);
            (boss, world.get::<Enemy>(boss).unwrap().ninsatsu.0)
        };
        for _ in 0..3 {
            {
                let world = app.world_mut();
                let mut q = world.query_filtered::<Entity, With<crate::enemy_bullet::EnemyBullet>>();
                for b in q.iter(world).collect::<Vec<_>>() {
                    world.despawn(b);
                }
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
            lines.push(format!("blow\t{phase}\t{clock:.2}\tboss\t-"));
            let mut landed = false;
            for f in 0..600u32 {
                app.update();
                clock += 1.0 / 60.0;
                keep_wolf(&mut app);
                log(&mut app, &mut lines, &mut last, phase, clock, f % 60 == 0);
                let e = app.world().get::<Enemy>(boss).unwrap();
                landed |= e.ninsatsu.0 < left || e.is_dead() || e.disabled;
                if won(&mut app, &mut idle) {
                    over = true;
                    break;
                }
            }
            if landed || over {
                break;
            }
        }
        // The phase change or the hand-over plays out.
        for f in 0..900u32 {
            if over {
                break;
            }
            app.update();
            clock += 1.0 / 60.0;
            keep_wolf(&mut app);
            log(&mut app, &mut lines, &mut last, phase, clock, f % 60 == 0);
            over = won(&mut app, &mut idle);
        }
        if over {
            lines.push(format!("won\t{phase}\t{clock:.2}\tboss\t-"));
            break;
        }
    }
    std::fs::write(&out, lines.join("\n")).unwrap();
}
