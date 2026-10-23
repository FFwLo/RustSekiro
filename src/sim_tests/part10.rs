//! Sim tests, part 10: 38 tests, from `an_attack_at_the_run_loops_wrap_keeps_the_momentum_too` to `hit_stop_holds_posture_regen_and_sp_effect_timers`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// The run loop a000_000500 is a looping generator (c0000.hkx MODE_LOOPING; data::loops_c0000):
/// an attack pressed as the loop wraps still carries the run's momentum, because the from
/// generator keeps looping through the blend (actor.rs advance wraps its clock). Presses are
/// spread over one loop length so some land on the wrap.
#[test]
pub(crate) fn an_attack_at_the_run_loops_wrap_keeps_the_momentum_too() {
    fn player_pos(app: &mut App) -> Vec3 {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
    }
    let mut worst = f32::MAX;
    for extra in (0..36).step_by(3) {
        let mut app = app();
        move_enemy_far(&mut app);
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
        for _ in 0..90 + extra {
            app.update();
        }
        let p0 = player_pos(&mut app);
        for _ in 0..6 {
            app.update();
        }
        let run_step = (player_pos(&mut app) - p0).length();
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
        worst = worst.min(d6 / run_step);
        assert!(d6 > 0.55 * run_step && d6 < run_step + 0.03, "press at +{extra}: attack's first 6 ticks {d6:.3} m, the run's {run_step:.3} m");
    }
    eprintln!("worst attack / run ratio over the loop: {worst:.2}");
}

/// The castle Genichiro's last dot (71001000, kept immortal by m11_01 11115810 without a 201000
/// reaction) goes at Wolf's Ninsatsu hit, not on the throw's first frame: 11115820 ("bars == 0"
/// -> Wait 2 -> the Tomoe swap) runs from the hit. Before, the immortality rule zeroed the pending
/// count at once.
#[test]
pub(crate) fn castle_genichiros_last_dot_waits_for_the_hit() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7100", Some(71001000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        world.get_mut::<Enemy>(boss).unwrap().ninsatsu.0 = 1;
    }
    break_and_deathblow_died(&mut app, 1);
    let mut early = Vec::new();
    let mut started = None;
    for f in 0..(60 * 20) {
        let wolf = player_state(&mut app).0;
        if started.is_none() && wolf.starts_with("Deathblow") {
            started = Some(f);
        }
        if let Some(s) = started {
            if f < s + 20 {
                let world = app.world_mut();
                let boss = boss_in_play(world);
                early.push(world.get::<Enemy>(boss).unwrap().ninsatsu_left());
            }
        }
        if enemy_chrs(&mut app) == ["c7110"] {
            break;
        }
        app.update();
    }
    assert!(started.is_some(), "no deathblow");
    assert!(early.iter().all(|n| *n == 1), "the dot went early: {early:?}");
    assert_eq!(enemy_chrs(&mut app), ["c7110"], "no Tomoe swap");
}

/// The Old Dragons of the Tree are 15 c5300 bodies (m25 2500811-825; rows 53000010-12) whose
/// hits feed the shared bar 2500810 (12505803 x15: SpEffect 3530020 on the bar). No event enables
/// them (they stand from the start), so the exporter left them out of the cast and the fight was
/// the bar's dummy alone (tools/boss_scripts.py `linked`).
#[test]
pub(crate) fn the_old_dragons_bar_is_the_scripted_bar_body() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5300", Some(53000000), Vec::new());
    let mut bar = None;
    for _ in 0..600 {
        app.update();
        bar = app.world().resource::<crate::enemy::BossScript>().run.as_ref().and_then(|r| r.bar);
        if bar.is_some() {
            break;
        }
    }
    // m25 12505801: 2003[11](1, 2500810, 0, 905300) "Old Dragons of the Tree" on the shared bar
    // body, whose NpcParam 53000000 has ninsatuNum 0 (so the ninsatsu rule alone showed no bar).
    assert_eq!(bar, Some(2500810), "the script's boss bar");
}

#[test]
pub(crate) fn the_old_dragons_stand_in_their_fight() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5300", Some(53000000), Vec::new());
    for _ in 0..60 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query::<(&crate::enemy::script::Scripted, &Actor, Option<&Visibility>)>();
    let dragons: Vec<i64> = q.iter(world).filter(|(s, a, v)| (2500811..=2500825).contains(&s.0) && a.hp > 0.0 && v.is_none_or(|v| *v != Visibility::Hidden)).map(|(s, ..)| s.0).collect();
    assert_eq!(dragons.len(), 15, "{dragons:?}");
    // The fight's start (12505801 after its action button) turns the bar body 2500810's AI off.
    {
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::script::Scripted, &Enemy)>();
        assert!(q.iter(world).find(|(s, _)| s.0 == 2500810).is_some_and(|(_, e)| e.ai_off), "the bar body fights");
    }
    // A dragon down to 1 HP (4[14] HP <= 1) costs the bar: SpEffect 3530020 on 2500810.
    let bar_hp = |app: &mut App| {
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::script::Scripted, &Actor)>();
        q.iter(world).find(|(s, _)| s.0 == 2500810).map(|(_, a)| a.hp).unwrap()
    };
    let before = bar_hp(&mut app);
    {
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::script::Scripted, &mut Actor)>();
        let (_, mut a) = q.iter_mut(world).find(|(s, _)| s.0 == 2500811).unwrap();
        a.hp = 1.0;
    }
    for _ in 0..60 {
        app.update();
    }
    let after = bar_hp(&mut app);
    assert!(after < before, "bar {before} -> {after}");
}

/// Isshin Ashina's volley (m11_01 11115944): on SpEffect 3540510 (a000_003032 f90) the script
/// fires 254000382 from 33 region spheres 1112962-994, owner the c1000 dummy 1110921 no event
/// enables. Every shot was dropped (a region source, an absent owner).
#[test]
pub(crate) fn isshin_ashinas_volley_fires_from_its_regions() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5430.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5430", Some(54300000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<(&crate::enemy::script::Scripted, &mut Actor)>();
        let (_, mut a) = q.iter_mut(world).find(|(s, _)| s.0 == 1110920).unwrap();
        a.resident.push(3540510);
    }
    let mut most = 0;
    for _ in 0..10 {
        app.update();
        let world = app.world_mut();
        most = most.max(world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).count());
    }
    assert!(most >= 33, "{most} fire spots");
}

/// Round 45: aged, Wolf's deathblow on a broken c1300 is the aging row 11300260 ("aging break",
/// a222_502000 -> 512000), not the usual one; afterwards the aged form comes back as AgingIdle,
/// not W_StandAging again (HKS 381). Before, the aged attack ate it.
#[test]
pub(crate) fn an_aged_wolf_takes_the_aging_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1300", None, Vec::new());
    app.update();
    let wolf = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut st = world.resource_mut::<crate::status::Statuses>();
        for _ in 0..8 {
            st.hit_wolf(&combat, wolf, 0, &[], &[9600], crate::status::Guard::None);
        }
    });
    for _ in 0..60 * 6 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "AgingIdle");
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
        let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
        e.aggressive = false;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -1.5);
        a.yaw = std::f32::consts::PI;
        a.posture = a.posture_max;
        e.on_posture_break(&mut a, &combat, false, 4.0);
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut pa, mut pt) = q.single_mut(world).unwrap();
        pt.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
        pa.yaw = 0.0;
    });
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut anims = Vec::<String>::new();
    let mut states = Vec::<String>::new();
    for _ in 0..60 * 12 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        if anims.last() != Some(&anim) {
            anims.push(anim);
        }
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(anims.iter().any(|k| k == "a222_502000"), "{anims:?} {states:?} {}", log_text(&app));
    assert!(anims.iter().any(|k| k == "a222_512000"), "{anims:?}");
    assert_eq!(states.iter().filter(|s| *s == "StandAging").count(), 0, "{states:?}");
    assert!(states.last().is_some_and(|s| s.starts_with("Aging")), "{states:?}");
}

/// Round 44: aged (9600's curse build-up -> 9620, ref 591) Wolf plays the aged form: W_StandAging
/// then AgingIdle, max HP x0.125 (maxHpRate), attack -> W_AgingGroundAttack; aged, a kill is
/// W_AgingGroundDeathStart -> W_AgingGroundDeathLoop (HKS 365) with no resurrection offered.
/// Before, nothing of it.
#[test]
pub(crate) fn an_aged_wolf_plays_the_aged_form() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1300.json")).exists() {
        return;
    }
    // c1300's data holds 9600 (the build-up row).
    let mut app = app_cfg("c1300", None, Vec::new());
    move_enemy_far(&mut app);
    app.update();
    let (wolf, max0) = {
        let world = app.world_mut();
        let (e, a) = world.query_filtered::<(Entity, &Actor), With<Player>>().single(world).unwrap();
        (e, a.hp_max)
    };
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut st = world.resource_mut::<crate::status::Statuses>();
        for _ in 0..8 {
            st.hit_wolf(&combat, wolf, 0, &[], &[9600], crate::status::Guard::None);
        }
    });
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..60 * 6 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "StandAging"), "{seen:?} {}", log_text(&app));
    assert_eq!(seen.last().map(String::as_str), Some("AgingIdle"), "{seen:?}");
    {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!((a.hp_max - max0 * 0.125).abs() < 0.5, "hp_max {} of {max0}", a.hp_max);
        assert!(a.resident.contains(&9620));
        // 9620's changeStaminaRate -100 each frame: the posture stays drained (round 46).
        assert!(a.posture >= a.posture_max - 0.01, "posture {} of {}", a.posture, a.posture_max);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "AgingGroundAttack");
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..60 * 8 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "AgingGroundDeathStart"), "{seen:?}");
    assert!(seen.iter().any(|s| s == "AgingGroundDeathLoop"), "{seen:?}");
    assert!(!seen.iter().any(|s| s.contains("Revival")), "{seen:?}");
}

/// Manual test #1 (2026-10-13): the F1 menu's infinite Spirit Emblems tops Wolf's emblems back up
/// every tick, for testing the arts and tools.
#[test]
pub(crate) fn the_infinite_emblems_cheat_refills_them() {
    let mut app = app_cfg("c1000", None, Vec::new());
    app.init_resource::<crate::debug_menu::DebugMenu>().add_systems(FixedUpdate, crate::debug_menu::player_cheats);
    app.update();
    app.world_mut().resource_mut::<crate::debug_menu::DebugMenu>().infinite_emblems = true;
    app.world_mut().query::<&mut Player>().single_mut(app.world_mut()).unwrap().emblems = 0;
    app.update();
    app.update();
    let n = app.world_mut().query::<&Player>().single(app.world()).unwrap().emblems;
    assert!(n >= 15, "emblems {n}");
}

/// Round 46: aged, a jump press plays BEH_A_AGING_GROUND_JUMP: W_AgingGroundJumpReady_V (no
/// stick) into its Start, whose TAE 920 lifts Wolf off the ground, the fall loop, and lands as
/// W_LandAgingGroundJump (HKS 1776), not the free-fall landing. Before, the press did nothing.
#[test]
pub(crate) fn an_aged_wolf_jumps() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1300", None, Vec::new());
    move_enemy_far(&mut app);
    app.update();
    let wolf = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut st = world.resource_mut::<crate::status::Statuses>();
        for _ in 0..8 {
            st.hit_wolf(&combat, wolf, 0, &[], &[9600], crate::status::Guard::None);
        }
    });
    for _ in 0..60 * 6 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "AgingIdle");
    let y0 = app.world_mut().query_filtered::<&Transform, With<Player>>().single(app.world()).unwrap().translation.y;
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut seen: Vec<String> = Vec::new();
    let mut top = y0;
    for _ in 0..60 * 4 {
        app.update();
        let s = player_state(&mut app).0;
        top = top.max(app.world_mut().query_filtered::<&Transform, With<Player>>().single(app.world()).unwrap().translation.y);
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert_eq!(seen.first().map(String::as_str), Some("AgingGroundJumpReady_V"), "{seen:?} {}", log_text(&app));
    assert!(seen.iter().any(|s| s == "AgingGroundJumpStart_V"), "{seen:?}");
    assert!(top > y0 + 0.2, "rose {} {seen:?}", top - y0);
    assert!(seen.iter().any(|s| s == "LandAgingGroundJump"), "{seen:?}");
    assert!(!seen.iter().any(|s| s == "LandAgingFreeFall"), "{seen:?}");
    assert!(seen.last().is_some_and(|s| s.starts_with("Aging")), "{seen:?}");
}

/// Fountainhead's strong noble c1300 (NpcParam 13000001) breathes Bullet 13000200, "Breathe in
/// life": SpEffect 9600, registCurse 40 (stateInfo 116, the exe's "curse" slot), against Wolf's
/// 200, recovering ResistRecoverPoint_Curse_Player 20 a second. Full, it applies its
/// cycleOccurrence 9620 "Aging_invoking" (30 s, behaviorRefId 591). The AI sees it on Wolf
/// (130000_battle.lua: HasSpecialEffectId(TARGET_ENE_0, 9620)) and stops breathing: it strikes
/// (Act03) or grabs (Act02).
#[test]
pub(crate) fn the_nobles_breath_ages_wolf() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1300.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1300", None, Vec::new());
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut aged_at = None;
    let mut after = std::collections::BTreeSet::new();
    for f in 0..(60 * 40) {
        app.update();
        let wolf = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
        let aged = app.world().resource::<crate::status::Statuses>().0.get(&wolf).is_some_and(|s| s.effects.iter().any(|a| a.id == 9620));
        if aged && aged_at.is_none() {
            aged_at = Some(f);
        }
        if aged_at.is_some() {
            after.insert(enemy_state(&mut app).1);
        }
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.hp = a.hp_max;
        a.posture = 0.0;
    }
    assert!(aged_at.is_some(), "never aged: {}", log_text(&app));
    // (Not 3003: the breath combo's ComboFinal, Act01.)
    assert!(after.contains("a000_003007") || after.contains("a000_003004"), "{after:?}");
}

/// The Finger Whistle maddens a beast (prosthetic::madden): its bullets' 230518 cycles to 230510,
/// whose gate stateInfo 959 the dog wears (230500, NpcParam spEffectID9), then to 230515
/// COMMON_SP_EFFECT_CONFUSE, endurance -1, changeTeamType 29 (TEAM_TYPE_Indiscriminate): the
/// dog's team is 29 and it is hostile to the soldier. The soldier (no 230500) is not maddened.
#[test]
pub(crate) fn the_finger_whistle_maddens_a_dog_not_a_soldier() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1150.json")).exists() {
        return;
    }
    let group = vec![
        crate::config::GroupEnemy { chr: "c1150".into(), npc_row: Some(11500000), at: Some([0.0, 2.0]), msb: None },
        crate::config::GroupEnemy { chr: "c1010".into(), npc_row: None, at: Some([1.5, 2.5]), msb: None },
    ];
    let mut app = app_cfg("c1150", Some(11500000), group);
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![79000];
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        world.query::<&mut Player>().single_mut(world).unwrap().emblems = 10;
    }
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..240 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Enemy>>();
    let combat = world.resource::<Combat>();
    let teams: Vec<(String, i64, bool)> = q.iter(world).map(|a| (combat.foe_of(a).chr.clone(), crate::combat::team_of(combat, a), a.sp_chained.iter().any(|c| c.0 == 230515))).collect();
    let dog = teams.iter().find(|t| t.0 == "c1150").unwrap();
    let soldier = teams.iter().find(|t| t.0 == "c1010").unwrap();
    assert!(dog.2 && dog.1 == 29, "{teams:?}");
    assert!(!soldier.2 && soldier.1 != 29, "{teams:?}");
    assert!(crate::combat::teams_hostile(dog.1, soldier.1));
}

/// A maddened dog (team 29) turns on the soldier next to it: with the AI on, the soldier loses HP
/// to it while Wolf stands back out of reach.
#[test]
pub(crate) fn a_maddened_dog_bites_the_soldier() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1150.json")).exists() {
        return;
    }
    let group = vec![
        crate::config::GroupEnemy { chr: "c1150".into(), npc_row: Some(11500000), at: Some([0.0, 6.0]), msb: None },
        crate::config::GroupEnemy { chr: "c1010".into(), npc_row: None, at: Some([1.5, 7.0]), msb: None },
    ];
    let mut app = app_cfg("c1150", Some(11500000), group);
    let (dog, soldier) = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &Actor, &mut Enemy)>();
        let combat_chr = |a: &Actor, w: &World| w.resource::<Combat>().foe_of(a).chr.clone();
        let ents: Vec<(Entity, String)> = q.iter(world).map(|(x, a, _)| (x, combat_chr(a, world))).collect();
        let dog = ents.iter().find(|x| x.1 == "c1150").unwrap().0;
        let soldier = ents.iter().find(|x| x.1 == "c1010").unwrap().0;
        (dog, soldier)
    };
    for x in [dog, soldier] {
        app.world_mut().get_mut::<Enemy>(x).unwrap().aggressive = true;
    }
    {
        let world = app.world_mut();
        let mut a = world.get_mut::<Actor>(dog).unwrap();
        a.sp_chained.push((230515, f32::INFINITY));
    }
    let hp0 = app.world().get::<Actor>(soldier).unwrap().hp;
    for _ in 0..(60 * 20) {
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        a.hp = a.hp_max;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -30.0);
    }
    let hp = app.world().get::<Actor>(soldier).unwrap().hp;
    assert!(hp < hp0, "the soldier kept {hp} / {hp0}: {}", log_text(&app));
}

/// A worn SpEffect shifts an NPC's behaviour judge (data::shift_judge, exe FUN_140a12b80): the
/// Shura samurai's enchantment (a000_003014 adds 3107010 poison +5 and 3107011 flame +7, each
/// gated on its resident 3107040 / 3107041) turns his combo's judge 100 (AtkParam 10700100, no
/// SpEffect) into 105 (10700105, 9013 poison) or 107 (10700107, 9119 burn). Unenchanted, the
/// same swing builds up nothing.
#[test]
pub(crate) fn the_shura_samurais_enchanted_blade_poisons_or_burns() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1070.json")).exists() {
        return;
    }
    let swing = |row: i64, enchant: Option<i64>| -> (f32, f32) {
        let mut app = app_cfg("c1070", Some(row), Vec::new());
        app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
        app.update();
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.2);
            if let Some(id) = enchant {
                a.sp_chained.push((id, 15.0));
            }
            a.play("Attack3000", "a000_003000");
            a.t = 1.0;
            a.prev_t = 1.0;
        }
        for _ in 0..20 {
            app.update();
        }
        let world = app.world_mut();
        let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        let s = world.resource::<crate::status::Statuses>().0.get(&wolf).map_or((0.0, 0.0), |s| (s.poison, s.burn));
        s
    };
    assert_eq!(swing(10700000, None), (0.0, 0.0));
    let poison = swing(10700000, Some(3107010));
    assert!(poison.0 > 0.0 && poison.1 == 0.0, "{poison:?}");
    let burn = swing(10701000, Some(3107011));
    assert!(burn.1 > 0.0 && burn.0 == 0.0, "{burn:?}");
    // The poison enchantment on the flame samurai (gate 950, he wears 951) shifts nothing.
    assert_eq!(swing(10701000, Some(3107010)), (0.0, 0.0));
    assert_eq!(crate::data::shift_judge(700, &[3145040]), 701);
    assert_eq!(crate::data::shift_judge(10, &[3145040]), 10);
}

/// The super-armour pool (actor::SuperArmor, exe CSSuperArmorDurability): the armoured
/// servant 11800000 (superArmorDurability 240, recover correction 4.0 -> 25 s) wears 5366
/// "damage anim change at SA" (stateInfo 120, every dmgLv 2 = middle... -> the HKS level 8
/// minimum for a small hit) only while the pool holds. Wolf's katana hits drain 40 x
/// atkSuperArmorCorrection / 100; empty, it breaks: that hit staggers him and the next tick
/// refills it (the hold, TAE ChrActionFlag 71, is in no anim); untouched for 25 s, a drained
/// pool is full again.
#[test]
pub(crate) fn the_armoured_servants_super_armour_breaks_and_refills() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1180.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1180", Some(11800000), Vec::new());
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    let world = app.world_mut();
    let ent = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let level = |world: &World| {
        let a = world.get::<Actor>(ent).unwrap();
        crate::combat::damage_motion_level(world.resource::<Combat>().data_of(a), a, 1)
    };
    {
        let a = world.get::<Actor>(ent).unwrap();
        assert!(a.resident.contains(&5366));
        assert_eq!(a.super_armor.max, 240.0);
        assert_eq!(a.super_armor.delay, 25.0);
    }
    assert_eq!(level(world), 8, "small -> minimum under super armour");
    let atk = world.resource::<Combat>().player.attacks.values().map(|k| k.atk_param_id).filter(|id| crate::data::super_armor_damage(true, *id) > 0.0).min().expect("a katana row with SA damage");
    let per = crate::data::super_armor_damage(true, atk);
    let mut hits = 0;
    while !crate::combat::super_armor_hit(&mut world.get_mut::<Actor>(ent).unwrap(), &[], true, atk) {
        hits += 1;
        assert!(hits < 100);
    }
    assert_eq!(hits + 1, (240.0 / per).ceil() as i32, "{per} a hit");
    assert_eq!(level(world), 1, "broken: the hit's own level");
    // In a damage reaction, the next tick refills it all the same.
    world.resource_scope(|w, combat: Mut<Combat>| {
        let mut q = w.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.get_mut(w, ent).unwrap();
        e.react(&mut a, &combat, "DamageSmall_Front");
        assert!(a.state.starts_with("Damage"), "no reaction: {}", a.state);
    });
    app.update();
    let world = app.world_mut();
    let a = world.get::<Actor>(ent).unwrap();
    assert!(a.state.starts_with("Damage"), "reaction over: {}", a.state);
    assert!(a.super_armor.holds(), "held broken in the reaction");
    let mut a = world.get_mut::<Actor>(ent).unwrap();
    a.super_armor.hit(1000.0);
    // Held broken through the reaction, refilled once it ends.
    a.super_armor.tick(1.0, true, false, 0.0);
    assert!(a.super_armor.broken);
    a.super_armor.tick(1.0 / 60.0, false, false, 0.0);
    assert!(a.super_armor.holds() && a.super_armor.cur == 240.0);
    // A partial drain snaps back after the 25 s wait, re-armed by every hit.
    crate::combat::super_armor_hit(&mut a, &[], true, atk);
    a.super_armor.tick(24.0, false, false, 0.0);
    assert!(a.super_armor.cur < 240.0);
    a.super_armor.tick(1.1, false, false, 0.0);
    assert_eq!(a.super_armor.cur, 240.0);
}

/// Deathblows a boss row until its map script handles the defeat (Handle Boss Defeat, after
/// which the duel starts over): (deathblows, the script's flags on the frame before, the states
/// seen).
pub(crate) fn deathblow_to_defeat(chr: &str, row: i64) -> (usize, Vec<i64>, Vec<String>) {
    let mut app = app_cfg(chr, Some(row), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    let mut seen = Vec::new();
    let mut flags = Vec::new();
    // The Folding Screen Monkeys: the bar holder 2000800 is a stand-in; the win (m20 12005800)
    // waits on the four c1260, not on the c1250 monkeys among them.
    super::part01::BOSS_CHR.with(|c| c.set((row == 12600050).then_some("c1260")));
    super::part01::BOSS_DONE.with(|d| d.borrow_mut().clear());
    // The win stays (no restart 5 s after it while a deathblow still plays: the Monkeys' last
    // catch wins at once, the bar holder's HP 22 -> 0 by 3126010).
    app.world_mut().resource_mut::<crate::enemy::AutoRestart>().0 = false;
    // (Its cast comes in a few frames after the bar holder.)
    for _ in 0..60 {
        if super::part01::try_boss_in_play(app.world_mut()).is_some() {
            break;
        }
        app.update();
    }
    for n in 1..=5 {
        if let Some(b) = super::part01::try_boss_in_play(app.world_mut()) {
            seen.extend(break_and_deathblow(&mut app, 600));
            if row == 12600050 {
                super::part01::BOSS_DONE.with(|d| d.borrow_mut().push(b));
            }
        }
        // The Owl (Father)'s second phase (m11_00 11005912: his first ThrowDef12000's message 20,
        // then his SpEffect 3506022 from the step his AI takes, a000_005201-5203, sets 11005902,
        // which his kneel 11005920 needs; 11005912 sets 200051 first). The sim's boss is
        // passive: the step is played here.
        let mut stepped = false;
        for _ in 0..600 {
            if row == 50601010 && !stepped {
                let w = app.world_mut();
                let b = boss_in_play(w);
                let mut a = w.get_mut::<Actor>(b).unwrap();
                if a.resident.contains(&200051) {
                    a.play("Ez5201", "a000_005201");
                    stepped = true;
                }
            }
            // The Demon of Hatred's HU after his first deathblow (702000_battle.lua Act31 at
            // 3702001 and 2 left: a000_020001), whose 3702050 his kneel 11105921 needs.
            if row == 70200000 && !stepped {
                let w = app.world_mut();
                let b = boss_in_play(w);
                let left = w.get::<Enemy>(b).unwrap().ninsatsu_left();
                let mut a = w.get_mut::<Actor>(b).unwrap();
                if left == 2 && !a.resident.contains(&3_702_050) && !a.state.starts_with("ThrowDef") {
                    a.play("Ez20001", "a000_020001");
                    stepped = true;
                }
            }
            let bs = app.world().resource::<crate::enemy::BossScript>();
            if bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) {
                return (n, flags, seen);
            }
            flags = bs.run.as_ref().map(|r| r.flags_on()).unwrap_or_default();
            app.update();
        }
    }
    (usize::MAX, flags, seen)
}

/// Lady Butterfly's second body 50900001 started alone (the picker): her defeat event 11005800
/// waits on her first body 1000800 dead (group -1), and that one is never on the field;
/// `dead_at_start` counts it dead, so her deathblows end the fight.
#[test]
pub(crate) fn butterflys_second_body_alone_reaches_her_defeat() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5090.json")).exists() {
        return;
    }
    let (n, flags, seen) = deathblow_to_defeat("c5090", 50900001);
    assert!(n != usize::MAX, "no defeat: flags {flags:?} states {seen:?}");
}

/// Manual test #3: every boss row's defeat event must run. boss_scripts.py's world_flags read the
/// world stages and other bosses' defeat flags as on, so Emma's defeat event (m11_01 11115900,
/// sets 9315) ended at once on Isshin Ashina's 9316, the castle-top Genichiro / Way of Tomoe's
/// (11115800, sets 9303) on the ninja-invasion stage 8301, and Isshin Ashina's 11115941 played
/// 20200 over his Todome on 8300 (set only by the m11_01_54_10 variant), before the message 50
/// his 11115920 waits on (FLAGS_OFF). Left out, with their own tests: the Divine Dragon (52000000,
/// lightning reversals) and Isshin the Sword Saint (54000000, the spear draw and Todome message).
#[test]
pub(crate) fn every_boss_row_reaches_its_defeat() {
    let txt = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/boss_scripts.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
    let rows: Vec<(String, i64)> = v.as_object().unwrap().iter().map(|(k, r)| (r["chars"][r["boss"].as_i64().unwrap().to_string()]["chr"].as_str().unwrap_or("").to_string(), k.parse().unwrap())).collect();
    let mut failed = Vec::new();
    for (chr, row) in rows {
        // 71101000 goes on into Isshin the Sword Saint (reservoir_tomoe_hands_over_to_isshin),
        // 74000010 into Isshin Ashina (isshin_ashina_follows_emma).
        // 53000000 (the Old Dragons) hands over to the Divine Dragon (m25 12505885, script.rs
        // hand_over).
        if [52000000, 53000000, 54000000, 71101000, 74000010].contains(&row) || !std::path::Path::new(&format!("{}/extracted/enemies/{chr}.json", env!("CARGO_MANIFEST_DIR"))).exists() {
            continue;
        }
        let (n, flags, mut seen) = deathblow_to_defeat(&chr, row);
        if n == usize::MAX {
            seen.dedup();
            failed.push(format!("{chr} {row}: flags {flags:?} states {seen:?}"));
        }
    }
    assert!(failed.is_empty(), "no defeat: {failed:#?}");
}

/// Manual test #3: Isshin the Sword Saint's phases come from his own AI (540000_battle.lua),
/// not from the map script: at 2 health bars or fewer with his resident 200050 his Act40 draws
/// the spear (3015, whose TAE gives 200031 -> NpcThinkParam battle goal 540010, his second set);
/// at 1 bar, with Number(6) still 0, 540010's Act06 / Act13 opens with the lightning 3056 / 3055.
/// (The sim's enemies are passive by default: these need the real AI.)
#[test]
pub(crate) fn isshins_deathblows_bring_the_spear_then_the_lightning() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5400.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5400", Some(54000000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    let set_ai = |app: &mut App, on: bool| {
        let w = app.world_mut();
        let b = boss_in_play(w);
        w.get_mut::<Enemy>(b).unwrap().aggressive = on;
    };
    // Runs the AI until `want` plays; the states and AI goals seen.
    let run_until = |app: &mut App, want: &[&str]| {
        let mut seen = Vec::<String>::new();
        for _ in 0..600 {
            app.update();
            let w = app.world_mut();
            let b = boss_in_play(w);
            let (e, a) = (w.get::<Enemy>(b).unwrap(), w.get::<Actor>(b).unwrap());
            let line = format!("{} | {}", a.state, e.ai_desc);
            if seen.last() != Some(&line) {
                seen.push(line);
            }
            if want.contains(&a.state.as_str()) {
                return (true, seen);
            }
        }
        (false, seen)
    };
    set_ai(&mut app, true);
    break_and_deathblow(&mut app, 60);
    let (drew, seen) = run_until(&mut app, &["Ez3015"]);
    assert!(drew, "no spear draw at 2 bars: {seen:#?}");
    for _ in 0..200 {
        app.update();
    }
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        let (e, a) = (w.get::<Enemy>(b).unwrap(), w.get::<Actor>(b).unwrap());
        assert_eq!(e.ninsatsu.0, 2);
        assert!(crate::combat::worn_sp(a, &e.clash).contains(&200031), "no 200031 after 3015");
        assert!(e.ai_desc.contains("540010"), "second set's battle goal: {}", e.ai_desc);
    }
    // The deathblow with the AI held (his attacks would knock Wolf out of it).
    set_ai(&mut app, false);
    for _ in 0..120 {
        app.update();
    }
    break_and_deathblow(&mut app, 200);
    set_ai(&mut app, true);
    let (lightning, seen) = run_until(&mut app, &["Ez3055", "Ez3056"]);
    assert!(lightning, "no lightning at 1 bar: {seen:#?}");
    let w = app.world_mut();
    let b = boss_in_play(w);
    assert_eq!(w.get::<Enemy>(b).unwrap().ninsatsu.0, 1);
}

/// Manual test #3: the Guardian Ape's first Todome does not end the fight (m17). He is immortal
/// (11705810) and lies in his Todome reaction a000_013700; its message 20 (6.1 s) and 12 s later
/// he gets up (EzState 20000) as the headless Ape: AI ID 51000100, SpEffects 5304 / 5305 off and
/// 5306 on (11705821). 11705820's 20200 (on Wolf's message 10) does not come: his Todome row
/// gives Wolf a230_511700, which has no TAE 936 (a240 / a242_511700 do), and a 20200 would cut
/// the reaction before its message 20.
#[test]
pub(crate) fn the_guardian_ape_gets_up_headless_after_his_first_todome() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5100.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5100", Some(51000000), Vec::new());
    let mut boss = Vec::<String>::new();
    let mut wolf = Vec::<String>::new();
    let mut headless = false;
    'fight: for _ in 0..4 {
        for _ in 0..300 {
            app.update();
        }
        boss.extend(break_and_deathblow(&mut app, 60));
        for _ in 0..1500 {
            app.update();
            let (s, an, _) = enemy_state(&mut app);
            let s = format!("{s}/{an}");
            if boss.last() != Some(&s) {
                boss.push(s);
            }
            let (s, an, _, _) = player_state(&mut app);
            let s = format!("{s}/{an}");
            if wolf.last() != Some(&s) {
                wolf.push(s);
            }
            let w = app.world_mut();
            let b = boss_in_play(w);
            let (e, a) = (w.get::<Enemy>(b).unwrap(), w.get::<Actor>(b).unwrap());
            if e.think_override == Some(51000100) {
                assert!(a.resident.contains(&5306) && !a.resident.contains(&5304) && !a.resident.contains(&5305), "residents {:?}", a.resident);
                // Two dots, one used (finishers clip 2:48), and full HP.
                assert_eq!(e.ninsatsu, (1, 2));
                assert_eq!(a.hp, a.hp_max);
                headless = true;
                break 'fight;
            }
        }
    }
    assert!(headless, "boss {boss:?} wolf {wolf:?}");
    let i = boss.iter().position(|s| s == "ThrowDef13700/a000_013700").unwrap_or_else(|| panic!("no Todome: {boss:?}"));
    assert_eq!(boss[i + 1..], ["Event20000/a000_020000".to_string()], "boss {boss:?} wolf {wolf:?}");
    assert_eq!(app.world().resource::<crate::enemy::BossScript>().defeats, 0);
    // His rise's TAE 67 rows have effectEndurance -1 and stay after it (FUN_140b56500, nothing
    // removes a TAE effect at the window's end): 275100 max HP / posture x1.1, 3510991 SA +120,
    // 3510110 damage rates.
    let (hp0, sa0) = {
        let w = app.world_mut();
        let b = boss_in_play(w);
        let a = w.get::<Actor>(b).unwrap();
        (a.hp_max, a.super_armor.max)
    };
    for _ in 0..600 {
        app.update();
    }
    let w = app.world_mut();
    let b = boss_in_play(w);
    let a = w.get::<Actor>(b).unwrap();
    assert!(a.state != "Event20000", "still rising");
    for id in [275100, 3510991, 3510110] {
        assert!(a.resident.contains(&id), "{id} not worn: {:?}", a.resident);
    }
    assert!((a.hp_max - hp0 * 1.1).abs() < 1.0, "hp_max {} from {hp0}", a.hp_max);
    assert_eq!(a.super_armor.full(), sa0 + 120.0);
}

/// Manual test #3: Lady Butterfly's change (m10 11005812). Her first part (1000800, one bar) dies
/// on its deathblow; 1 s later it is disabled and the second (1000810) comes in with the forced
/// 21000 and her AI off; once Wolf is in 1002830 and she wears 5028 she gets AI command 10, her
/// AI back and, 3 s on, 11005641 (the music's second part) and her health bar.
#[test]
pub(crate) fn lady_butterfly_changes_into_her_second_part() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5090.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5090", Some(50900000), Vec::new());
    for _ in 0..300 {
        app.update();
    }
    let first = boss_in_play(app.world_mut());
    let mut seen = break_and_deathblow(&mut app, 60);
    let mut changed = false;
    for _ in 0..900 {
        app.update();
        let s = enemy_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        let w = app.world_mut();
        let b = boss_in_play(w);
        let bs = w.resource::<crate::enemy::BossScript>();
        let run = bs.run.as_ref().unwrap();
        if b != first && run.ents.get(&1000810) == Some(&b) && run.flags_on().contains(&11005641) {
            changed = true;
            break;
        }
    }
    assert!(changed, "states {seen:?}");
    assert!(seen.iter().any(|s| s == "Event21000"), "states {seen:?}");
    let w = app.world_mut();
    let b = boss_in_play(w);
    assert_eq!(w.get::<Enemy>(b).unwrap().ninsatsu.0, 1);
    assert_eq!(w.resource::<crate::enemy::BossScript>().defeats, 0);
}

/// Lady Butterfly (50900000) with her boss script taken back, then `world` marks her an
/// open-world enemy (world.rs WorldChar, her MSB entity 1000800): 30 ticks on, whether a boss
/// script runs and whether she is Scripted.
fn butterfly_script_after(world: bool) -> Option<(bool, bool)> {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5090.json")).exists() {
        return None;
    }
    let mut app = app_cfg("c5090", Some(50900000), Vec::new());
    let w = app.world_mut();
    let b = boss_in_play(w);
    w.resource_mut::<crate::enemy::BossScript>().run = None;
    w.entity_mut(b).remove::<crate::enemy::script::Scripted>();
    if world {
        w.entity_mut(b).insert(crate::enemy::world::WorldChar { map: 0, id: 1000800 });
    }
    for _ in 0..30 {
        app.update();
    }
    let w = app.world();
    Some((w.resource::<crate::enemy::BossScript>().run.is_some(), w.get::<crate::enemy::script::Scripted>(b).is_some()))
}

/// A boss the open world streams in from the MSB (world.rs) gets no boss-arena script: m10's
/// events (11005812 bringing in 1000810, Handle Boss Defeat) already run in the map's world run.
#[test]
pub(crate) fn open_world_boss_starts_no_boss_script() {
    let Some(got) = butterfly_script_after(true) else { return };
    assert_eq!(got, (false, false), "(boss script runs, Scripted)");
    // The arena fight still starts hers.
    assert_eq!(butterfly_script_after(false), Some((true, true)));
}

/// Manual test #5 (timing side): when the Way of Tomoe casts his lightning. 711000_battle.lua
/// forces Act18 (3043, weight 1e12) while Wolf wears 109031 or 110125: his knockdowns and
/// get-ups (StandDamageLarge*, *BlowLand, BreakDown, a000_1003xx-1027xx / 1900xx TAE 67), not
/// his jumps; his Interrupt re-plans on 109031 coming or going. Timer 4 (15 s, set by the cast)
/// and the resident 3711500 (row 71101000, the Sword Saint fight) hold it back. 3043's lightning
/// is its bullet 986 at 1.17 s (TAE BulletBehavior), 3042's 988 at 0.83 s.
#[test]
pub(crate) fn the_way_of_tomoe_answers_a_downed_wolf_with_lightning() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    let mut app = app_with("c7110");
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        w.get_mut::<Enemy>(b).unwrap().aggressive = true;
        w.get_mut::<Transform>(b).unwrap().translation.z = 6.0;
    }
    // Into his opener (Number(7) == 0: Act20, 3008); Wolf goes down while it plays.
    for _ in 0..60 {
        app.update();
    }
    let mut ez = Vec::<String>::new();
    for _ in 0..600 {
        {
            let w = app.world_mut();
            let p = w.query_filtered::<Entity, With<Player>>().single(w).unwrap();
            w.resource_scope(|w, combat: Mut<Combat>| {
                let mut a = w.get_mut::<Actor>(p).unwrap();
                if a.state != "StandDamageLargeDown_FaceUp" || a.t > 1.0 {
                    a.play_state(&combat.player, "StandDamageLargeDown_FaceUp");
                }
            });
        }
        app.update();
        let s = enemy_state(&mut app).0;
        if s.starts_with("Ez") && ez.last() != Some(&s) {
            ez.push(s.clone());
        }
        if s == "Ez3043" {
            break;
        }
    }
    let new: Vec<_> = ez.iter().filter(|s| *s != "Ez3008").collect();
    assert_eq!(new.first().map(|s| s.as_str()), Some("Ez3043"), "attacks {ez:?}");
}

/// Manual test #6 ("double deathblows at the end", Isshin the Sword Saint). As in the game
/// (finishers clip 5:32-5:45), his last-bar Todome (ThrowDef13700) gives way to his kneel
/// (20200, on Wolf's message 10: m11_02 11125874), and the finisher taken from behind
/// (Wolf a243_510900, Isshin ThrowDef12900) ends the fight: a100_012900's message 30 at
/// 11.63 s -> 11125860, Handle Boss Defeat 5 s later. Before, 11125874, re-armed, took Wolf's
/// finisher message 10 at 6.63 s and put Isshin back into 20200, so 12900 never reached its
/// message 30 and he waited for another finisher. c9997.hks ThrowDef12900_onUpdate takes no
/// event transition (script.rs takes_throw_transition).
#[test]
pub(crate) fn isshins_finisher_ends_the_fight_once() {
    isshins_finisher_to_defeat("c5400", 54000000);
}

/// The reservoir run 71101000 (the Way of Tomoe, then Isshin the Sword Saint: 11125830's
/// hand-over, reservoir_tomoe_hands_over_to_isshin) ends as his own row does: Tomoe's deathblow
/// brings Isshin in, and his finisher's message 30 ends the run (11125860, 9312).
#[test]
pub(crate) fn the_reservoir_run_ends_with_isshins_finisher() {
    isshins_finisher_to_defeat("c7110", 71101000);
}

pub(crate) fn isshins_finisher_to_defeat(chr: &str, row: i64) {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5400.json")).exists() {
        return;
    }
    let mut app = app_cfg(chr, Some(row), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    // A row that opens with someone else (Tomoe) goes on until Isshin alone is in.
    for _ in 0..5 {
        if enemy_chrs(&mut app) == ["c5400"] {
            break;
        }
        break_and_deathblow(&mut app, 600);
        for _ in 0..600 {
            app.update();
        }
    }
    assert_eq!(enemy_chrs(&mut app), ["c5400"], "{row}: Isshin never came in");
    {
        let bs = app.world().resource::<crate::enemy::BossScript>();
        assert!(bs.defeats == 0 && bs.run.as_ref().is_some_and(|r| r.defeated.is_none()), "{row}: ended before Isshin");
    }
    let set_ai = |app: &mut App, on: bool| {
        let w = app.world_mut();
        let b = boss_in_play(w);
        w.get_mut::<Enemy>(b).unwrap().aggressive = on;
    };
    // The first deathblow and his spear draw (his second set, which the Todome rows need).
    set_ai(&mut app, true);
    break_and_deathblow(&mut app, 60);
    for _ in 0..400 {
        app.update();
    }
    set_ai(&mut app, false);
    // His spear draw a000_003015 (200031: m11_02 11125872 -> 11125862, which his kneel 11125874
    // needs), if his AI has not drawn it yet.
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        let mut a = w.get_mut::<Actor>(b).unwrap();
        if a.anim_group == 0 {
            a.play("Ez3015", "a000_003015");
        }
    }
    for _ in 0..400 {
        app.update();
    }
    for want in [1u32, 0] {
        for _ in 0..6 {
            for _ in 0..120 {
                app.update();
            }
            let w = app.world_mut();
            let b = boss_in_play(w);
            if w.get::<Enemy>(b).unwrap().ninsatsu.0 <= want {
                break;
            }
            break_and_deathblow(&mut app, 30);
        }
    }
    let mut boss = Vec::<String>::new();
    let mut finisher = false;
    for f in 0..3600 {
        if !finisher && f >= 400 && f % 20 == 0 {
            // From behind him, facing his back.
            let w = app.world_mut();
            let b = boss_in_play(w);
            let (at, fwd, yaw) = {
                let a = w.get::<Actor>(b).unwrap();
                (w.get::<Transform>(b).unwrap().translation, a.forward(), a.yaw)
            };
            let p = w.query_filtered::<Entity, With<Player>>().single(w).unwrap();
            w.get_mut::<Transform>(p).unwrap().translation = at - fwd * 1.3;
            w.get_mut::<Actor>(p).unwrap().yaw = yaw;
            w.resource_mut::<PadInput>().press(Action::Attack);
        }
        app.update();
        let s = enemy_state(&mut app).0;
        finisher |= s == "ThrowDef12900";
        if boss.last() != Some(&s) {
            boss.push(s);
        }
        let bs = app.world().resource::<crate::enemy::BossScript>();
        if bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) {
            break;
        }
    }
    let i = boss.iter().position(|s| s == "ThrowDef12900").unwrap_or_else(|| panic!("no finisher: {boss:?}"));
    assert!(boss[..i].iter().any(|s| s == "Event20200"), "no kneel before the finisher: {boss:?}");
    assert_eq!(boss[i..], ["ThrowDef12900".to_string()], "the finisher was cut: {boss:?}");
    let bs = app.world().resource::<crate::enemy::BossScript>();
    assert!(bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()), "no defeat: {boss:?} run flags {:?}", bs.run.as_ref().map(|r| (r.row, r.flags_on())));
    // Round 93: Won waits for the finisher (menu::finisher_done): ThrowDef12900 to its last frame.
    let (secs, seen) = secs_to_finisher_done(&mut app);
    assert!(seen.iter().any(|s| s == "ThrowDef12900") && secs < 30.0, "{row}: finisher done after {secs:.2} s: {seen:?}");
    let w = app.world_mut();
    let b = boss_in_play(w);
    let a = w.get::<Actor>(b).unwrap();
    assert!(a.anim.is_empty() || a.t >= w.resource::<Combat>().data_of(a).length(&a.anim) - 1e-3, "{row}: {} at {:.2}", a.anim, a.t);
}

/// Seconds from now until the fight's finisher has played out (menu::finisher_done: Wolf free and
/// every enemy's clip at its end), and the enemy states seen on the way.
pub(crate) fn secs_to_finisher_done(app: &mut App) -> (f32, Vec<String>) {
    let mut seen = Vec::<String>::new();
    for f in 0..(40 * 60) {
        let s = enemy_state(app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
        let w = app.world_mut();
        let wolf = w.query_filtered::<&Actor, With<Player>>().single(w).unwrap().state.clone();
        let mut q = w.query::<(&Enemy, &Actor)>();
        if crate::menu::finisher_done(w.resource::<Combat>(), &wolf, q.iter(w)) {
            return (f as f32 / 60.0, seen);
        }
        app.update();
    }
    (f32::INFINITY, seen)
}

/// Round 93 (the user's bug: "most of the end deathblows are cut short by the title screen"):
/// after the last deathblow the Won screen waits until Wolf is free and the enemy's death /
/// ThrowDef clip has ended. Measured: the Monk's Todome ~19 s (Wolf's 10 s Todome, her Event20200,
/// then her scripted Event20006), the General's ThrowDefDeath12001 ~6 s: both past the old 4 s.
#[test]
pub(crate) fn the_last_deathblow_plays_out_before_won() {
    for chr in ["c5000", "c7100", "c1020"] {
        let mut app = app_with(chr);
        let over = |app: &mut App| {
            let w = app.world_mut();
            let b = boss_in_play(w);
            w.get::<Enemy>(b).unwrap().is_dead() || w.resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some())
        };
        let mut died = false;
        'tries: for _ in 0..4 {
            break_and_deathblow_died(&mut app, 1);
            for _ in 0..600 {
                if over(&mut app) {
                    died = true;
                    break 'tries;
                }
                app.update();
            }
        }
        assert!(died, "{chr}: no last deathblow");
        let (secs, seen) = secs_to_finisher_done(&mut app);
        eprintln!("{chr}: finisher done after {secs:.2} s: {seen:?}");
        assert!(secs.is_finite() && secs < 30.0, "{chr}: never done: {seen:?}");
        let w = app.world_mut();
        let wolf = w.query_filtered::<&Actor, With<Player>>().single(w).unwrap().state.clone();
        assert!(crate::player::is_free(&wolf), "{chr}: Wolf still in {wolf}");
        // Only the characters the ending plays on (menu::finisher_done): Gyoubu's horse / a cast
        // member alive by design keeps its own loop going.
        let mut q = w.query::<(&Enemy, &Actor)>();
        let c = w.resource::<Combat>();
        for (e, a) in q.iter(w).filter(|(e, a)| e.is_dead() || a.state.starts_with("ThrowDef") || a.state.starts_with("Event20200")) {
            assert!(a.anim.is_empty() || a.t >= c.data_of(a).length(&a.anim) - 1e-3, "{chr}: {} cut at {:.2}", a.anim, a.t);
        }
    }
}

/// "Repeat one attack" (SHINOBI_AI_ATTACK=3042 starts in it) plays the chosen attack: the Way
/// of Tomoe's lightning a000_003042 / 003043 (their TAE SpawnOneShotFFX 671026-671029, VFX's
/// sword lightning shots).
#[test]
pub(crate) fn repeat_mode_plays_the_tomoe_lightning() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    for anim in ["a000_003042", "a000_003043"] {
        let mut app = app_with("c7110");
        {
            let mut dbg = app.world_mut().resource_mut::<crate::enemy::EnemyDebug>();
            dbg.mode = crate::enemy::AiMode::Repeat;
            dbg.attack = anim.to_string();
        }
        let w = app.world_mut();
        let b = boss_in_play(w);
        w.get_mut::<Enemy>(b).unwrap().aggressive = true;
        let mut played = 0;
        let mut last = String::new();
        for _ in 0..900 {
            app.update();
            let a = enemy_state(&mut app).1;
            if a == anim && last != anim {
                played += 1;
            }
            last = a;
        }
        assert!(played >= 2, "{anim} played {played} times");
    }
}

/// The stateInfo-155 rows' changeSuperArmorPoint adds to the super-armour pool (FUN_140bdb690,
/// 140bd.c:8889-8900): a rise is added to the pool at once, and the refill fills to the larger
/// max. Isshin's NpcParam pool is 0; the "revived Kensei" row 3540110 (+200), put on by hand
/// here (nothing in Sekiro's data or exe puts it on), gives him one.
#[test]
pub(crate) fn a_super_armour_bonus_grows_the_pool() {
    let mut sa = crate::actor::SuperArmor::new(400.0, 4.0);
    sa.hit(300.0);
    assert_eq!(sa.cur, 100.0);
    sa.tick(0.1, false, false, 120.0);
    assert_eq!((sa.cur, sa.full()), (220.0, 520.0));
    sa.tick(25.0, false, false, 120.0);
    assert_eq!(sa.cur, 520.0);
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5400.json")).exists() {
        return;
    }
    let mut app = app_with("c5400");
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        let mut a = w.get_mut::<Actor>(b).unwrap();
        assert!(!a.super_armor.holds());
        a.resident.push(3540110);
    }
    for _ in 0..3 {
        app.update();
    }
    let w = app.world_mut();
    let b = boss_in_play(w);
    let a = w.get::<Actor>(b).unwrap();
    assert_eq!((a.super_armor.full(), a.super_armor.cur), (200.0, 200.0));
    assert!(a.super_armor.holds());
}

/// The whistle's madness 230515 (effectEndurance -1, gate 959, isWaitModeDelete) lasts while the
/// beast wears its resident 230500 (stateInfo 959) and is deleted the moment that goes
/// (FUN_140c04090, 140c0.c:2641).
#[test]
pub(crate) fn the_whistle_madness_goes_when_its_gate_does() {
    let mut app = app_with("c1000");
    for _ in 0..3 {
        app.update();
    }
    let w = app.world_mut();
    let b = boss_in_play(w);
    {
        let mut a = w.get_mut::<Actor>(b).unwrap();
        if !a.resident.contains(&230500) {
            a.resident.push(230500);
        }
        a.sp_chained.push((230515, f32::INFINITY));
    }
    for _ in 0..30 {
        app.update();
    }
    let w = app.world_mut();
    assert!(w.get::<Actor>(b).unwrap().sp_chained.iter().any(|c| c.0 == 230515), "gone while 959 holds");
    w.get_mut::<Actor>(b).unwrap().resident.retain(|r| *r != 230500);
    for _ in 0..2 {
        app.update();
    }
    let w = app.world_mut();
    assert!(!w.get::<Actor>(b).unwrap().sp_chained.iter().any(|c| c.0 == 230515), "kept without its gate");
}

/// The exe's add rule for lasting rows (FUN_140c02c50): spCategory 100 overwrites the same
/// category and priority (the AI states 200000 / 200001 at 1, the anim sets 200030-200034 at
/// 3) and leaves another priority beside it; category 0 rows stack.
#[test]
pub(crate) fn a_lasting_spfx_overwrites_its_category() {
    let mut app = app_with("c1010");
    app.update();
    let w = app.world_mut();
    let b = boss_in_play(w);
    let mut r = w.resource_scope::<Combat, _>(|w, combat| {
        let mut a = w.get_mut::<Actor>(b).unwrap();
        let d = combat.data_of(&a);
        a.resident = vec![200000, 200030, 6011];
        crate::enemy::wear_for_good(d, &mut a, 200001);
        crate::enemy::wear_for_good(d, &mut a, 200031);
        crate::enemy::wear_for_good(d, &mut a, 6071);
        a.resident.clone()
    });
    r.sort();
    assert_eq!(r, vec![6011, 6071, 200001, 200031]);
}

/// Every exported enemy's bulletRows holds its whole Bullet param chain (HitBulletID /
/// intervalCreateBulletId), so enemy_bullet.rs never meets a missing child. The exporter's walk
/// ended on a popped id it had already written and dropped the rest: c1070's 380 lost its
/// interval child 10700383 (spEffectId0 9120, sfx 610742).
#[test]
pub(crate) fn enemy_bullet_rows_hold_their_whole_chain() {
    let dir = crate::paths::extracted();
    let Ok(text) = std::fs::read_to_string(dir.join("json/params/Bullet.json")) else { return };
    let param: serde_json::Value = serde_json::from_str(&text).unwrap();
    let rows: std::collections::HashMap<i64, &serde_json::Value> = param["rows"].as_array().unwrap().iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect();
    let mut missing = Vec::new();
    for f in std::fs::read_dir(dir.join("enemies")).unwrap().flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        if !(name.starts_with('c') && name.ends_with(".json")) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(f.path()).unwrap()) else { continue };
        let Some(have) = v["bulletRows"].as_object() else { continue };
        for id in have.keys().filter_map(|k| k.parse::<i64>().ok()) {
            for k in ["HitBulletID", "intervalCreateBulletId"] {
                let c = rows.get(&id).and_then(|r| r[k].as_i64()).unwrap_or(-1);
                if c > 0 && rows.contains_key(&c) && !have.contains_key(&c.to_string()) {
                    missing.push(format!("{name}: {id} -> {c}"));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:?}");
}

/// Predictive shots (BulletParam predictionShootObserveTime, FUN_140b0acf0 / FUN_140b05b80): the
/// c1190 archer's 11900100 (0.2 s) leads a Wolf running sideways by his average velocity over
/// the last 0.2 s, as far as he gets while the arrow flies to him; standing still, it aims at him.
#[test]
pub(crate) fn a_predictive_arrow_leads_a_running_wolf() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1190.json")).exists() {
        return;
    }
    let app = app_with("c1190");
    let combat = app.world().resource::<Combat>();
    let spec = combat.kinds[0].data.bullets.get("100").expect("c1190 bullet 100");
    assert_eq!(spec.bullet_id, 11900100);
    let window = crate::data::prediction_observe_time(spec.bullet_id);
    assert!((window - 0.2).abs() < 1e-4, "{window}");
    // 1 s still, then 0.5 s at 6 m/s along x: the 0.2 s average is the run alone.
    let mut track = crate::enemy_bullet::TargetTrack::default();
    let mut pos = Vec3::ZERO;
    for i in 0..90 {
        if i >= 60 {
            pos.x += 6.0 / 60.0;
        }
        track.push(pos, 1.0 / 60.0);
    }
    let vel = track.velocity(window);
    assert!((vel.x - 6.0).abs() < 0.05 && vel.y.abs() < 1e-4, "{vel}");
    let (from, target) = (Vec3::new(0.0, 1.0, 20.0), Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(crate::enemy_bullet::predict(spec, from, target, Vec3::ZERO), target);
    let aim = crate::enemy_bullet::predict(spec, from, target, vel);
    // The lead is the run over the arrow's flight time to the led point (1/30 s steps).
    let flight = aim.distance(from) / spec.init_vellocity.clamp(spec.min_vellocity, spec.max_vellocity.max(spec.min_vellocity));
    assert!(aim.x > 0.5 && (aim.x - 6.0 * flight).abs() < 6.0 / 30.0 + 0.05, "aim {aim} flight {flight}");
}

/// Attack-occurrence SpEffects (FUN_140bfc170 -> FUN_140b6e6a0): with the Flame Vent's
/// enchantment 106020 on (stateInfo 62, atkOccurrenceSpEffectId 106021, which lasts 0 s and
/// becomes 9100 "burning damage, PC 25"), each sword hit builds burn on the enemy; the same
/// slash without it builds none.
#[test]
pub(crate) fn an_enchanted_sword_builds_burn() {
    let slash = |enchanted: bool| {
        let mut app = app_cfg("c1020", None, Vec::new());
        let foe = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(Entity, &mut Actor, &mut Transform, &mut Enemy), Without<crate::enemy::Cast>>();
            let (ent, mut a, mut t, mut e) = q.single_mut(world).unwrap();
            e.targeting.state = crate::stealth::BATTLE;
            a.yaw = std::f32::consts::PI;
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.6);
            ent
        };
        app.update();
        let hp0 = app.world().get::<Actor>(foe).unwrap().hp;
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        for _ in 0..60 {
            if enchanted {
                let world = app.world_mut();
                let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
                p.timed.retain(|t| t.0 != 106020);
                p.timed.push((106020, 7.5));
            }
            app.update();
        }
        let hit = app.world().get::<Actor>(foe).unwrap().hp < hp0;
        let burn = app.world().resource::<crate::status::Statuses>().0.get(&foe).map_or(0.0, |s| s.burn);
        (hit, burn, log_text(&app))
    };
    let (hit, burn, log) = slash(false);
    assert!(hit && burn == 0.0, "plain: hit {hit} burn {burn}\n{log}");
    let (hit, burn, log) = slash(true);
    assert!(hit && burn > 0.0, "enchanted: hit {hit} burn {burn}\n{log}");
}

/// FUN_140c06240: an attack-occurrence row counts only for its hit category. Wolf's
/// enchantments are wepParamChange 1 (the sword's), so the Flame Vent's 106020 lands with a
/// sword hit (category 1) and not with a tool bullet (2) or a combat art's bullet (9); the
/// Nightjars' 3145000 (wepParamChange 0) lands with any.
#[test]
pub(crate) fn attack_occurrence_follows_the_hit_category() {
    use crate::data::{atk_occurrence, HIT_CAT_SWORD, HIT_CAT_TOOL};
    assert_eq!(atk_occurrence(&[106020], HIT_CAT_SWORD, false, false).map(|o| o.0), Some(106021));
    assert_eq!(atk_occurrence(&[106020], HIT_CAT_TOOL, false, false), None);
    assert_eq!(atk_occurrence(&[106020], 9, false, false), None);
    assert_eq!(atk_occurrence(&[3145000], HIT_CAT_TOOL, false, false).map(|o| o.0), Some(9125));
    assert_eq!(atk_occurrence(&[3145000], 7, false, false).map(|o| o.0), Some(9125));
    // A later worn row that passes is taken when the first does not.
    assert_eq!(atk_occurrence(&[106020, 3145000], HIT_CAT_TOOL, false, false).map(|o| o.0), Some(9125));
}

/// A bullet issues its own attack as it starts (FUN_140b0fc60 -> FUN_1409a1290's bullet branch):
/// its AtkParam's AppearAiSoundId goes out at the bullet, owned by its shooter. The c1190
/// archer's arrow 11900100 (AtkParam 11900100: 2000025, 10 m) fired 6 m off is heard there, not
/// at the archer; a dummy row (AtkParam 8) makes none.
/// The c1180 servants' armour guard: their idle anims carry ShieldBlock judge 800 under
/// StateInfo 957, which 220062 (put on by the map's armour-part event) opens. Without it there
/// is no guard; with it a sword blow (repel power 30) is blocked by the row 11800800 (repel 20,
/// guard type 1). The arc is NpcParam guardAngle (60), not the judge (combat.rs guard_contact).
#[test]
pub(crate) fn the_armour_guard_follows_its_worn_effect() {
    let mut app = app_with("c1180");
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&mut Actor, &Enemy), Without<crate::enemy::Cast>>();
        let (mut a, e) = q.iter_mut(world).next().unwrap();
        a.resident.retain(|&id| id != 220062);
        a.play("Idle", "a000_000000");
        a.t = 0.5;
        let d = combat.data_of(&a);
        let blow = crate::data::Attack { guard_atk_rate_correction: 300.0, ..Default::default() };
        let guard = |a: &Actor| crate::combat::guard_contact(&combat, d, a, None, ("a000_000000", 0.5), None, true, &blow, 5000);
        assert_eq!(guard(&a).guarded, crate::guard::Guarded::Hit, "without 220062");
        a.sp_chained.push((220062, 1e6));
        let g = guard(&a);
        assert_eq!(g.guarded, crate::guard::Guarded::Block, "with 220062");
        assert_eq!(g.half_angle, 60.0);
        // The block is no guard reaction: 220062's ref 1000105 GUARD_TO_ADD_DAMAGE turns it into
        // the SABlend flinch (c9997 ExecGuardBlock), so he swings on as the game's servants do
        // (live: 0 of their blocks of Wolf's sword played a guard clip). 220062 is in no c1180
        // clip, so its ref comes from SpEffectParam.
        a.resident.push(220062);
        let refs = crate::prosthetic::enemy_refs(&combat, &a, e);
        assert!(refs.contains(&1000105), "{refs:?}");
        assert_eq!(crate::combat::enemy_guard_block(d, &refs, 1, 4, false, false), crate::combat::GuardPick::Add);
    });
}

/// Lady Butterfly's second-form bullets (a000_003010 BulletBehavior 310 / 330, StateInfo 184)
/// fire only while she wears 3509200 "HU for bullet activation" (her a100_021000 TAE 67,
/// endurance -1): a gate put on in play, which the static NpcParam list (`resident_gates`) missed
/// (combat.rs event_gates).
#[test]
pub(crate) fn butterflys_second_form_bullets_need_her_3509200() {
    for worn in [false, true] {
        let mut app = app_with("c5090");
        app.update();
        let shooter = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(Entity, &mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
            let (ent, mut e, mut a) = q.iter_mut(world).next().unwrap();
            e.ai_off = true;
            a.play("Ez3010", "a000_003010");
            a.t = 1.32;
            a.prev_t = 1.32;
            if worn {
                a.sp_chained.push((3509200, 1e6));
            }
            ent
        };
        let mut n = 0;
        for _ in 0..8 {
            app.update();
            let world = app.world_mut();
            n = n.max(world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).filter(|b| b.shooter == shooter).count());
        }
        assert_eq!(n, if worn { 2 } else { 0 }, "3509200 worn: {worn}");
    }
}

#[test]
pub(crate) fn a_bullet_makes_its_appear_sound_where_it_starts() {
    let mut app = app_cfg("c1190", None, Vec::new());
    app.update();
    let (shooter, at) = {
        let world = app.world_mut();
        let (e, tf) = world.query_filtered::<(Entity, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
        (e, tf.translation)
    };
    // The archer stands still: its own shot's 2000025 (same owner and id) would replace the record.
    let world = app.world_mut();
    for mut e in world.query::<&mut Enemy>().iter_mut(world) {
        e.ai_off = true;
    }
    let from = at + Vec3::new(6.0, 1.0, 0.0);
    for (row, sound) in [("11900100", true), ("10001600", false)] {
        let world = app.world_mut();
        let old: Vec<Entity> = world.query_filtered::<Entity, With<crate::enemy_bullet::EnemyBullet>>().iter(world).collect();
        for b in old {
            world.despawn(b);
        }
        world.resource_mut::<crate::enemy::AiSounds>().0.clear();
        let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get(row).expect("c1190 row").clone();
        crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, from, Vec3::X, 0.0, None);
        world.flush();
        app.update();
        let sounds = &app.world().resource::<crate::enemy::AiSounds>().0;
        let heard = sounds.iter().find(|s| s.id == 2000025 && s.owner == Some(shooter) && s.pos.distance(from) < 0.01);
        assert_eq!(heard.is_some(), sound, "{row}: {:?}", sounds.iter().map(|s| (s.id, s.pos)).collect::<Vec<_>>());
    }
}

/// A growing bullet re-issues its attack (and so its AppearAiSoundId) every 1/6 s while it grows
/// and once when it has (140b0.c:8408 / 8627): c1190's 11903311 (hitRadius 2.5 -> 3.4 over
/// 0.5 s) issues 2000025 at 0, 1/6, 2/6 and about 0.5 s (3/6 and the final one fall on one frame
/// or two); a bullet that does not grow (11900100) only at its start.
#[test]
pub(crate) fn a_growing_bullet_reissues_its_appear_sound() {
    let mut app = app_cfg("c1190", None, Vec::new());
    app.update();
    let (shooter, at) = {
        let world = app.world_mut();
        let (e, tf) = world.query_filtered::<(Entity, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>().iter(world).next().unwrap();
        (e, tf.translation)
    };
    let world = app.world_mut();
    for mut e in world.query::<&mut Enemy>().iter_mut(world) {
        e.ai_off = true;
    }
    let from = at + Vec3::new(6.0, 1.0, 0.0);
    for (row, want) in [("11903311", 4..=5), ("11900100", 1..=1)] {
        let world = app.world_mut();
        let old: Vec<Entity> = world.query_filtered::<Entity, With<crate::enemy_bullet::EnemyBullet>>().iter(world).collect();
        for b in old {
            world.despawn(b);
        }
        world.resource_mut::<crate::enemy::AiSounds>().0.clear();
        let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get(row).expect("c1190 row").clone();
        crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, from, Vec3::X, 0.0, None);
        world.flush();
        let (mut issues, mut prev) = (0, None::<f32>);
        for _ in 0..60 {
            app.update();
            let life = app.world().resource::<crate::enemy::AiSounds>().0.iter().find(|s| s.id == 2000025 && s.owner == Some(shooter)).map(|s| s.life);
            if life.is_some_and(|l| prev.is_none_or(|p| l > p + 1e-4)) {
                issues += 1;
            }
            prev = life;
        }
        assert!(want.contains(&issues), "{row}: {issues} issues");
    }
}

/// Round 64: c5080's perilous thrust (a000_003009, AtkParam 50800090) has no ThrowParam 0190 row,
/// so it can't be mikiri'd and the step into it costs c5080 no staminaDamageAttackHitParry (90)
/// posture (it did, with Wolf still hit).
#[test]
pub(crate) fn a_thrust_without_a_mikiri_row_costs_no_posture() {
    for f in [3, 6, 9] {
        let mut app = app_with("c5080");
        {
            let world = app.world_mut();
            let mut q = world.query::<(&mut Enemy, &mut Actor)>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003009");
        }
        let posture = |app: &mut App| app.world_mut().query::<(&Enemy, &Actor)>().single(app.world()).unwrap().1.posture;
        let mut pressed = false;
        let mut last = posture(&mut app);
        for _ in 0..300 {
            if let Some(dt) = time_to_enemy_hit(&mut app) {
                if !pressed && dt <= f as f32 / TAE_FPS {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.stick = Vec2::new(0.0, -1.0);
                    pad.press(Action::Step);
                    pressed = true;
                }
            }
            app.update();
            let now = posture(&mut app);
            assert!(now <= last + 1e-3, "frame lead {f}: c5080 posture {last} -> {now}
{}", log_text(&app));
            last = now;
            if log_text(&app).contains("hit:") {
                break;
            }
        }
        assert!(!log_text(&app).contains("MIKIRI"));
    }
}

/// The repeat drill measures range from the enemy's body edge: the Demon of Hatred (c7020,
/// NpcParam hitRadius 2.8) stood 3.2 m off, held by the body push-out, and walked on forever,
/// so SHINOBI_AI_ATTACK=3043 never played his embers (VFX's drill).
#[test]
pub(crate) fn a_big_enemy_reaches_drill_range() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7020.json")).exists() {
        return;
    }
    let mut app = app_cfg("c7020", Some(70200000), Vec::new());
    {
        let mut dbg = app.world_mut().resource_mut::<crate::enemy::EnemyDebug>();
        dbg.mode = crate::enemy::AiMode::Repeat;
        dbg.attack = "a000_003043".to_string();
    }
    // Wolf 12 m off: the Demon has to walk in, and the push-out stops him at 3.2 m.
    app.update();
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        let at = w.get::<Transform>(b).unwrap().translation;
        let mut p = w.query_filtered::<&mut Transform, With<Player>>().single_mut(w).unwrap();
        p.translation = Vec3::new(at.x, p.translation.y, at.z + 12.0);
    }
    let mut seen = Vec::<String>::new();
    for _ in 0..1500 {
        app.update();
        let a = enemy_state(&mut app).1;
        if seen.last() != Some(&a) {
            seen.push(a.clone());
        }
        // A 3043 begun after he walked in (the spawn starts him in range).
        if a == "a000_003043" && seen.iter().any(|s| s == "a000_405000") {
            return;
        }
    }
    panic!("no 3043: {seen:?}");
}

/// Round 64 (R51 question): the exe runs posture regen (FUN_140a04850), status recovery and the
/// SpEffect timers on the character's own FD4Time (chr+0x108, the hit-stop-scaled dt), so a hit
/// stop holds them; before, they ran on real time through the hold.
#[test]
pub(crate) fn hit_stop_holds_posture_regen_and_sp_effect_timers() {
    let run = |stop: bool| {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            let mut a = q.single_mut(world).unwrap();
            a.posture = 100.0;
            a.since_posture_damage = 99.0;
            a.regen_carry = 0.0;
            a.sp_chained.push((-77, 5.0));
            a.electro = Some((9490, 5.0));
            if stop {
                a.hit_stop.start(1.0);
            }
        }
        for _ in 0..8 {
            app.update();
        }
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        let electro = 5.0 - a.electro.map_or(0.0, |e| e.1);
        (100.0 - a.posture, 5.0 - a.sp_chained.iter().find(|c| c.0 == -77).map_or(0.0, |c| c.1), electro)
    };
    let (free_regen, free_timer, free_electro) = run(false);
    let (held_regen, held_timer, held_electro) = run(true);
    // Round 81: the Lightning Reversal charge (9490 / 9495) is an SpEffect timer too.
    assert!(free_electro > 0.1 && held_electro < free_electro * 0.25, "electro {held_electro} vs {free_electro}");
    assert!(free_regen > 2.0 && free_timer > 0.1, "free: regen {free_regen} timer {free_timer}");
    assert!(held_regen < free_regen * 0.25, "regen in hit stop {held_regen} vs {free_regen}");
    assert!(held_timer < free_timer * 0.25, "timer in hit stop {held_timer} vs {free_timer}");
}

/// The Folding Screen Monkeys as the menu picks them (c1260 / 12600050): the bar holder 2000800
/// (a c1000 dummy in the MSB) is a hidden stand-in away from the field, not a fifth monkey one
/// deathblow beat; the four c1260 are the fight.
#[test]
pub(crate) fn the_monkeys_bar_holder_is_off_the_field() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1260.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1260", Some(12600050), Vec::new());
    for _ in 0..60 {
        app.update();
    }
    let w = app.world_mut();
    let mut q = w.query::<(&Enemy, &Transform, &Visibility, Has<crate::enemy::Cast>)>();
    let main: Vec<_> = q.iter(w).filter(|x| !x.3).map(|x| (x.0.stand_in, x.1.translation, *x.2)).collect();
    assert!(main.len() == 1 && main[0].0 && main[0].1.z > 1000.0 && main[0].2 == Visibility::Hidden, "bar holder: {main:?}");
    assert_eq!(q.iter(w).filter(|x| x.3).count(), 4, "the four monkeys");
}
