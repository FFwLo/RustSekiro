//! Headless simulation of real combat exchanges: the game's own plugins run
//! under MinimalPlugins with a fixed 60 Hz clock and scripted input, so timing
//! rules can be verified end to end without a window. Needs extracted/combat_data.json.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

use crate::actor::{Actor, ActorPlugin};
use crate::combat::CombatPlugin;
use crate::config::ConfigPlugin;
use crate::data::{Combat, DataPlugin, TAE_FPS};
use crate::enemy::{Enemy, EnemyPlugin};
use crate::hud::CombatLog;
use crate::player::{Action, PadInput, Player, PlayerPlugin, CAPSULE_HALF_HEIGHT};

fn app() -> App {
    sim_tests_app()
}

fn sim_tests_app() -> App {
    app_with("c1020")
}

/// The sim with a given enemy (config.toml's enemy.chr is overridden so tests are stable).
fn app_with(chr: &str) -> App {
    app_cfg(chr, None, Vec::new())
}

/// The sim with an enemy (NpcParam row) and more enemies at once (config enemy.group).
fn app_cfg(chr: &str, npc_row: Option<i64>, group: Vec<crate::config::GroupEnemy>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .add_plugins(ConfigPlugin);
    let several = !group.is_empty();
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.enemy.chr = chr.to_string();
        config.enemy.npc_row = npc_row;
        config.enemy.group = group;
    }
    app.add_plugins((DataPlugin, ActorPlugin, CombatPlugin, PlayerPlugin, EnemyPlugin, crate::prosthetic::ProstheticPlugin, crate::status::StatusPlugin, crate::enemy_bullet::EnemyBulletPlugin));
    app.finish();
    app.cleanup();
    app.update();
    // Face off 2.4 m apart: player at the origin facing +Z, enemy facing -Z.
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
    for (mut a, mut t) in q.iter_mut(world) {
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
    }
    // (A group keeps its own spots.)
    let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Enemy>>();
    for (mut a, mut t) in q.iter_mut(world) {
        if several {
            continue;
        }
        a.yaw = 0.0;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 2.4);
    }
    app
}

fn enemy_state(app: &mut App) -> (String, String, f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
    let a = q.single(world).unwrap();
    (a.state.clone(), a.anim.clone(), a.t)
}

fn player_state(app: &mut App) -> (String, String, f32, f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Player>>();
    let a = q.single(world).unwrap();
    (a.state.clone(), a.anim.clone(), a.t, a.posture)
}

fn log_text(app: &App) -> String {
    app.world().resource::<CombatLog>().lines.iter().map(|l| l.0.clone()).collect::<Vec<_>>().join("\n")
}

fn start_enemy_combo(app: &mut App, combo: usize) {
    let world = app.world_mut();
    let mut q = world.query::<(&mut Enemy, &mut Actor)>();
    let (mut e, mut a) = q.single_mut(world).unwrap();
    e.force_combo(&mut a, combo);
}

/// Seconds until the first damaging hitbox of the enemy's current anim starts.
fn time_to_enemy_hit(app: &mut App) -> Option<f32> {
    let (_, anim, t) = enemy_state(app);
    let combat = app.world().resource::<Combat>();
    combat
        .enemy0()
        .attack_windows(&anim)
        .iter()
        .filter(|(_, atk, _)| atk.atk_phys > 0.0)
        .map(|(e, _, _)| e.start - t)
        .filter(|d| *d > -0.001)
        .reduce(f32::min)
}

/// Runs an enemy attack and presses guard `lead` seconds before its first hit
/// (pressing `extra_presses` additional times first, 7 TAE frames apart: mashing speed,
/// since presses inside a guard start's frames 0-6 merge into one buffered press).
fn guard_against_attack(lead: f32, extra_presses: usize) -> String {
    guard_against_attack_facing(lead, extra_presses, false)
}

fn guard_against_attack_facing(lead: f32, extra_presses: usize, facing_away: bool) -> String {
    let mut app = app();
    if facing_away {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().yaw = 0.0;
    }
    start_enemy_combo(&mut app, 0);
    let mut presses_left = extra_presses + 1;
    let mut next_press_frame = 0;
    for frame in 0..240 {
        if let Some(dt) = time_to_enemy_hit(&mut app) {
            let press_at = lead + extra_presses as f32 * (7.0 / TAE_FPS);
            if presses_left > 0 && dt <= press_at && frame >= next_press_frame {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.press(Action::Guard);
                pad.guard_held = true;
                presses_left -= 1;
                next_press_frame = frame + 14;
            }
        }
        app.update();
        if log_text(&app).contains("DEFLECT") || log_text(&app).contains("blocked") || log_text(&app).contains("hit:") {
            break;
        }
    }
    let (es, _, _) = enemy_state(&mut app);
    let (ps, _, _, _) = player_state(&mut app);
    format!("{}\nenemy={es} player={ps}", log_text(&app))
}

#[test]
fn timed_guard_deflects_and_bounces_the_enemy() {
    // 2 TAE frames before the hit: inside the 6-frame window of StandToDeflectGuard.
    let out = guard_against_attack(2.0 / TAE_FPS, 0);
    assert!(out.contains("YOU DEFLECT"), "{out}");
    // Combo 1 hit 1 has justDeflectedAction 1 -> full bound right.
    assert!(out.contains("enemy=AttackBoundEnemy1_Right"), "{out}");
    assert!(out.contains("player=StandDeflectHard"), "{out}");
}

#[test]
fn late_guard_only_blocks() {
    // 10 TAE frames before the hit: past the 6-frame deflect window, still guarding.
    let out = guard_against_attack(10.0 / TAE_FPS, 0);
    assert!(out.contains("blocked"), "{out}");
    assert!(out.contains("player=StandDeflectEasy"), "{out}");
}

#[test]
fn spammed_guard_loses_the_deflect() {
    // Fourth press in quick succession: StandToDeflectGuard4 has no deflect window.
    let out = guard_against_attack(1.0 / TAE_FPS, 3);
    assert!(!out.contains("YOU DEFLECT"), "{out}");
}

#[test]
fn no_guard_gets_hit() {
    let mut app = app();
    start_enemy_combo(&mut app, 0);
    for _ in 0..240 {
        app.update();
        if log_text(&app).contains("hit:") {
            break;
        }
    }
    let out = log_text(&app);
    assert!(out.contains("enemy hit"), "{out}");
}

#[test]
fn enemy_guards_the_first_player_attack() {
    let mut app = app();
    // Quick slash: press and release attack.
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        if log_text(&app).contains("enemy blocked") || log_text(&app).contains("ENEMY DEFLECT") || log_text(&app).contains("you hit") {
            break;
        }
    }
    let out = log_text(&app);
    // Consecutive-guard count starts at 0, so Goal.Parry picks guard (3100), not deflect.
    assert!(out.contains("enemy blocked"), "{out}\nenemy={:?}", enemy_state(&mut app));
}

#[test]
fn vertical_jump_follows_the_exe_arc() {
    let mut app = app();
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut peak, mut launch, mut land) = (0.0f32, None, None);
    for i in 0..300 {
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(world).unwrap();
        peak = peak.max(t.translation.y - CAPSULE_HALF_HEIGHT);
        if a.airborne && launch.is_none() {
            launch = Some(i);
        }
        if launch.is_some() && !a.airborne && land.is_none() {
            land = Some(i);
            // c0000_transition.lua W_LandGroundJump (vertical): a000_201040, as in the recordings.
            assert_eq!(a.state, "LandVerticalGroundJump");
        }
        if let Some(l) = land {
            // The land anim (0.67 s) ends back in idle - not stuck in a jump pose.
            if a.state == "StandIdle" {
                assert!(i - l <= 45, "idle {} frames after landing", i - l);
                break;
            }
            assert!(i - l < 60, "still {} {} frames after landing", a.state, i - l);
        }
    }
    // ChrPhysicsVelocityChangeParam 100: 9.64 m/s up; Normal fall 13.72 + world gravity 9.8 m/s^2.
    // Live game (5 recorded vertical jumps): rise 1.98 m, 0.82 s in the air.
    let air = (land.expect("landed") - launch.expect("launched")) as f32 / 60.0;
    assert!((peak - 1.98).abs() < 0.05, "peak {peak}");
    assert!((air - 0.82).abs() < 0.05, "air {air}");
}

#[test]
fn aggressive_enemy_runs_its_real_ai() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Enemy>();
        q.single_mut(world).unwrap().aggressive = true;
    }
    let mut attacked = None;
    for i in 0..600 {
        app.update();
        let (state, anim, _) = enemy_state(&mut app);
        if state.starts_with("Ez") && anim.starts_with("a000_0030") {
            attacked = Some((i, anim));
            break;
        }
    }
    let world = app.world_mut();
    let mut q = world.query::<&Enemy>();
    let desc = q.single(world).unwrap().ai_desc.clone();
    assert!(attacked.is_some(), "no AI attack in 10 s; goals: {desc}; enemy={:?}", enemy_state(&mut app));
}

/// Plays one enemy attack anim and performs `act` (with `stick`) `lead` seconds
/// before its first damaging hit. Returns the log plus both states.
fn defend_against(anim: &str, lead: f32, act: Action, stick: Vec2) -> String {
    defend_against_with("c1020", anim, lead, act, stick)
}

fn defend_against_with(chr: &str, anim: &str, lead: f32, act: Action, stick: Vec2) -> String {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, anim);
    }
    let mut pressed = false;
    for _ in 0..300 {
        if let Some(dt) = time_to_enemy_hit(&mut app) {
            if !pressed && dt <= lead {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = stick;
                pad.press(act);
                pad.guard_held = act == Action::Guard;
                pressed = true;
            }
        }
        app.update();
        let l = log_text(&app);
        if ["DEFLECT", "blocked", "hit:", "MIKIRI"].iter().any(|k| l.contains(k)) {
            break;
        }
    }
    let (es, _, _) = enemy_state(&mut app);
    let (ps, _, _, _) = player_state(&mut app);
    format!("{}
enemy={es} player={ps}", log_text(&app))
}

/// a000_003010 "Advance thrust" (AtkParam 200: thrust, disableGuard 1, disableJustGuard 0).
const PERILOUS_THRUST: &str = "a000_003010";
/// a000_003004 "Lower Nagibashi" sweep (AtkParam 140: disableGuard 1, disableJustGuard 1).
const PERILOUS_SWEEP: &str = "a000_003004";

#[test]
fn perilous_thrust_goes_through_a_block() {
    let out = defend_against(PERILOUS_THRUST, 12.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("enemy hit") && !out.contains("blocked"), "{out}");
}

#[test]
fn perilous_thrust_can_still_be_deflected() {
    let out = defend_against(PERILOUS_THRUST, 2.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("YOU DEFLECT"), "{out}");
}

#[test]
fn perilous_sweep_cannot_be_deflected() {
    let out = defend_against(PERILOUS_SWEEP, 2.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("enemy hit") && !out.contains("DEFLECT"), "{out}");
}

#[test]
fn stepping_into_a_perilous_thrust_is_a_mikiri() {
    // Forward step toward the enemy (+Z): GroundStep_F carries MikiriCounter (flag 125) on frames 0-12.
    let out = defend_against(PERILOUS_THRUST, 5.0 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0));
    assert!(out.contains("MIKIRI COUNTER"), "{out}");
    assert!(out.contains("enemy=ThrowDef13800") && out.contains("player=Mikiri"), "{out}");
}

/// cargo test ai_trace -- --ignored --nocapture : prints 20 s of enemy AI decisions.
#[test]
#[ignore]
fn ai_trace() {
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Enemy>();
        q.single_mut(world).unwrap().aggressive = true;
    }
    let mut last = String::new();
    for i in 0..1200 {
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(&Enemy, &Actor, &Transform)>();
        let (e, a, tf) = q.single(world).unwrap();
        let line = format!("{} {} | {}", a.state, a.anim, e.ai_desc);
        if line != last {
            println!("{:6.2}s z={:5.2} {line}", i as f32 / 60.0, tf.translation.z);
            last = line;
        }
    }
    println!("{}", log_text(&app));
}

#[test]
fn deathblow_plays_the_throw_pair() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            // Inside ThrowParam 11020001's reach (Dist 2.1 m).
            t.translation.z = 1.8;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    // ThrowParam 11020000 (崩し始動): Wolf's start throw a201_500000 first; its judge 600 starts
    // the main throw. Live (rec_20261008_054259): 24 frames of start.
    let mut start = None;
    let mut main = None;
    for i in 0..60u32 {
        app.update();
        let (ps, pa, _, _) = player_state(&mut app);
        if start.is_none() && ps == "DeathblowStart" {
            assert_eq!(pa, "a201_500000");
            start = Some(i);
        }
        if ps == "Deathblow" {
            main = Some(i);
            break;
        }
    }
    let (start, main) = (start.expect("start throw"), main.expect("main throw"));
    assert!((main - start).abs_diff(24) <= 2, "start throw {} frames (live 24)", main - start);
    let (es, ea, _) = enemy_state(&mut app);
    let (ps, pa, _, _) = player_state(&mut app);
    // ThrowParam 11020001 (崩し): player a201_510000, enemy ThrowDef12000; with HP 0 its flag 69
    // switches to ThrowDefDeath12001. Live (rec_20261008_054259): 111 frames after the main start.
    assert_eq!((ps.as_str(), pa.as_str()), ("Deathblow", "a201_510000"), "{}", log_text(&app));
    assert_eq!(es, "ThrowDef12000", "{ea}");
    let mut switched = None;
    for i in 1..200u32 {
        app.update();
        if enemy_state(&mut app).0 == "ThrowDefDeath12001" {
            switched = Some(i);
            break;
        }
    }
    let switched = switched.expect("ThrowDefDeath12001");
    assert!(switched.abs_diff(111) <= 3, "death anim {switched} frames in (live 111)");
}

#[test]
fn an_enemy_plays_its_own_anim_set() {
    // c1021 (spear General, NpcParam 10210000) has the resident SpEffect 200031 "Anime ID offset
    // [1]" (stateInfo 271): its behavior states (CMSG offsetType 15) play the a100 set.
    let mut app = app_with("c1021");
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..900 {
        app.update();
        let (_, anim, _) = enemy_state(&mut app);
        seen.insert(anim.get(..4).unwrap_or("").to_string());
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
    assert_eq!(a.anim_group, 1);
    assert!(seen.contains("a100"), "anim sets played: {seen:?}
{}", log_text(&app));
}

#[test]
fn isshins_3015_switches_him_to_his_second_set() {
    // c5400 a000_003015 carries SpEffect 200031 (TAE 67): from then on its states play a100.
    let mut app = app_with("c5400");
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.single_mut(world).unwrap();
        assert_eq!(a.anim_group, 0);
        a.play("Ez3015", "a000_003015");
    }
    for _ in 0..400 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
    let mut a = q.single_mut(world).unwrap();
    assert_eq!(a.anim_group, 1);
    a.play("Idle", "a000_000000");
    assert_eq!(a.anim, "a100_000000");
}

/// Breaks the enemy's posture, puts it in front-deathblow reach and presses attack; returns the
/// enemy states seen over `frames` frames.
fn break_and_deathblow(app: &mut App, frames: u32) -> Vec<String> {
    break_and_deathblow_died(app, frames).0
}

/// As `break_and_deathblow`, also whether it was dead at some point (it respawns after a while).
fn break_and_deathblow_died(app: &mut App, frames: u32) -> (Vec<String>, bool) {
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 1.8);
            // Facing Wolf (a kneeling boss's finisher is from its front).
            a.yaw = 0.0;
        });
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        a.yaw = std::f32::consts::PI;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut states = Vec::<String>::new();
    let mut died = false;
    for _ in 0..frames {
        app.update();
        let s = enemy_state(app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
        let world = app.world_mut();
        died |= world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().is_dead();
        // (A map script's Handle Boss Defeat: Isshin is defeated, not dead.)
        died |= world.resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some());
    }
    (states, died)
}

#[test]
fn the_corrupted_monks_last_deathblow_is_her_todome() {
    // c5000 (NpcParam 50000000, ninsatuNum 3): two deathblows she gets up from, then the Todome
    // (ThrowParam 15000180 / 181: Wolf a211_501700 -> 511700, her ThrowDef13700) and Event20200.
    // Both of hers exist only in the a100 set (a000_013700 / 020200 are empty placeholders).
    let mut app = app_with("c5000");
    for _ in 0..2 {
        let seen = break_and_deathblow(&mut app, 500);
        assert!(seen.iter().any(|s| s == "ThrowDef12000"), "{seen:?}
{}", log_text(&app));
    }
    // (Her death comes on Wolf's Todome event message, m25 12505963; she respawns after it.)
    let (seen, died) = break_and_deathblow_died(&mut app, 600);
    let (ps, pa, _, _) = player_state(&mut app);
    assert!(seen.iter().any(|s| s == "ThrowDef13700"), "{seen:?} player {ps} {pa}
{}", log_text(&app));
    assert!(seen.iter().any(|s| s == "Event20200"), "{seen:?}");
    assert!(died, "{seen:?}");
}

/// m25 without Wolf's Todome message (he drops his Todome before its TAE 936): her Todome's
/// ThrowDef13700 still kills her (it sends 10 at its start, ending her immortality: 12505965)
/// and 12505950 handles her defeat; 12505964's 20021 does not play on her dead (it would cut
/// every Todome short: her a100_013700 has no 3500010).
#[test]
fn the_corrupted_monk_without_the_todome_message_still_dies() {
    let mut app = app_with("c5000");
    for _ in 0..2 {
        break_and_deathblow(&mut app, 500);
    }
    break_and_deathblow(&mut app, 30);
    let mut boss = Vec::<String>::new();
    let mut defeated = false;
    for _ in 0..900 {
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            if a.anim.ends_with("511700") || a.anim.ends_with("501700") {
                a.play("StandIdle", "a000_000000");
            }
        }
        app.update();
        let s = enemy_state(&mut app).0;
        if boss.last() != Some(&s) {
            boss.push(s);
        }
        defeated |= app.world().resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some());
    }
    assert!(!boss.iter().any(|s| s == "Event20021"), "boss {boss:?}");
    assert!(defeated, "boss {boss:?}");
}

#[test]
fn isshins_todome_message_ends_the_fight_with_wolfs_event_710206() {
    // m11_02 11125874: 0 health bars and Wolf's event message 10 (his Todome a242_511700's TAE 936
    // at 2.4 s) -> Isshin EzState 20200, Wolf EzState 710206 (c0000.hkx Event710206_CMSG).
    // (11125874 also needs 11125862: 11125872 sets it once he is at 2 bars or fewer with his
    // second set's 200031, from his 3015 spear draw.)
    let mut app = app_with("c5400");
    for _ in 0..2 {
        break_and_deathblow(&mut app, 800);
    }
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        q.single_mut(world).unwrap().play("Ez3015", "a000_003015");
    }
    for _ in 0..400 {
        app.update();
    }
    break_and_deathblow(&mut app, 60);
    let mut wolf = Vec::<String>::new();
    let mut boss = Vec::<String>::new();
    for _ in 0..900 {
        app.update();
        let s = player_state(&mut app).0;
        if wolf.last() != Some(&s) {
            wolf.push(s);
        }
        let s = enemy_state(&mut app).0;
        if boss.last() != Some(&s) {
            boss.push(s);
        }
    }
    let i = boss.iter().position(|s| s == "Event20200").unwrap_or_else(|| panic!("boss {boss:?} wolf {wolf:?}"));
    assert!(boss[..i].iter().any(|s| s == "ThrowDef13700") || i == 0, "boss {boss:?}");
    assert!(wolf.iter().any(|s| s == "Event710206"), "wolf {wolf:?} boss {boss:?}");
}

#[test]
fn the_corrupted_monks_phase_events_run_on_her_deathblows() {
    // m25 event 12505961: 2 health bars left -> AI command 1 (slot 0) and a re-plan; 12505962:
    // 1 left and her ThrowDef no longer holds 3500010 ("event anim transition not possible",
    // frames 0-110 of a000_012000) -> AI command 2 and ForceAnimationPlayback 20010.
    let mut app = app_with("c5000");
    let req = |app: &mut App| {
        let world = app.world_mut();
        let (e, _) = world.query_filtered::<(&Enemy, &Actor), Without<crate::enemy::Cast>>().single(world).unwrap();
        e.event_req().to_vec()
    };
    let first = break_and_deathblow(&mut app, 500);
    assert_eq!(req(&mut app), vec![(0, 1)], "{first:?}");
    assert!(!first.iter().any(|s| s == "Event20010"), "{first:?}");
    let second = break_and_deathblow(&mut app, 500);
    assert_eq!(req(&mut app), vec![(0, 2)], "{second:?}");
    let i = second.iter().position(|s| s == "Event20010").unwrap_or_else(|| panic!("{second:?}"));
    assert!(second[..i].iter().any(|s| s == "ThrowDef12000"), "{second:?}");
}

#[test]
fn the_demon_of_hatreds_last_phase_swaps_its_sp_effects() {
    // m11_00 event 11105912: 1 health bar left -> SetSpEffect 3702005 (its battle script's "after
    // the second deathblow" check), clear 277020 / 277021 (HU1), set 277022 / 277023 (HU2).
    let mut app = app_with("c7020");
    let resident = |app: &mut App| {
        let world = app.world_mut();
        let (_, a) = world.query_filtered::<(&Enemy, &Actor), Without<crate::enemy::Cast>>().single(world).unwrap();
        a.resident.clone()
    };
    // (Its NpcParam residents do not hold the HU1 pair: the arena's start event sets those.)
    let start = resident(&mut app);
    assert!(!start.contains(&3702005), "{start:?}");
    let first = break_and_deathblow(&mut app, 800);
    let left = {
        let world = app.world_mut();
        world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().ninsatsu
    };
    assert_eq!(left, (2, 3), "{first:?}");
    assert!(!resident(&mut app).contains(&3702005));
    let seen = break_and_deathblow(&mut app, 800);
    let now = resident(&mut app);
    assert!(now.contains(&3702005) && now.contains(&277022) && now.contains(&277023), "{now:?} {seen:?}");
    assert!(!now.contains(&277020) && !now.contains(&277021), "{now:?}");
}

#[test]
fn a_boss_takes_one_deathblow_per_red_dot() {
    // Genichiro (c7100, NpcParam 71000000 ninsatuNum 2): the first deathblow plays his ThrowDef
    // and he gets up again (c9997 HKS: not IsThrowSelfDeath -> IdleTransition); the second kills.
    let mut app = app_with("c7100");
    let probe = |app: &mut App| {
        let world = app.world_mut();
        let mut q = world.query::<(&Enemy, &Actor)>();
        let (e, a) = q.single(world).unwrap();
        (e.ninsatsu, e.is_dead(), a.hp, a.hp_max)
    };
    assert_eq!(probe(&mut app).0, (2, 2));
    let first = break_and_deathblow(&mut app, 400);
    let (left, dead, hp, hp_max) = probe(&mut app);
    assert!(first.iter().any(|s| s.starts_with("ThrowDef")), "{first:?}
{}", log_text(&app));
    assert_eq!(left, (1, 2), "{first:?}");
    assert!(!dead && hp == hp_max, "first deathblow: dead {dead}, hp {hp}/{hp_max} {first:?}");
    assert!(!enemy_state(&mut app).0.starts_with("ThrowDef"), "still down after 400 frames: {first:?}");
    // (The last one: his map script (m11_02 11125800) keeps him immortal, so he plays 12000 and
    // the script's flag 9300 ends the fight, then the cutscene.)
    let second = break_and_deathblow(&mut app, 900);
    let bs = app.world().resource::<crate::enemy::BossScript>();
    let defeated = bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some());
    assert!(probe(&mut app).1 || defeated, "second deathblow did not kill: {second:?}
{}", log_text(&app));
}

#[test]
fn deathblow_pair_moves_by_root_motion_like_the_live_game() {
    // After the absorb (enemy 1.50 m in front, facing Wolf) only the anims' root motion moves the
    // pair. Live (rec_20261008_054259, General, a201_510000 / a000_012000): 1.48 m on the first
    // frame, 0.85 m 25 frames in, 2.35 m 61 frames in.
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            t.translation.z = 1.5;
        });
    }
    // The main throw directly (deflect-break row 0010 has no start throw would differ): play it.
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut dist = Vec::new();
    for _ in 0..200 {
        app.update();
        let (ps, _, _, _) = player_state(&mut app);
        if ps == "Deathblow" {
            let world = app.world_mut();
            let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let foe = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
            if dist.is_empty() {
                // No models in tests: put the enemy where the absorb (dummy 266) would.
                let fwd = (foe - wolf).with_y(0.0).normalize();
                let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
                q.single_mut(world).unwrap().translation = wolf + fwd * 1.5;
                dist.push(1.5);
                continue;
            }
            dist.push((foe - wolf).with_y(0.0).length());
            if dist.len() > 62 {
                break;
            }
        }
    }
    println!("deathblow distance: +1 {:.2} +25 {:.2} +61 {:.2}", dist[1], dist[25], dist[61]);
    assert!((dist[25] - 0.85).abs() < 0.2, "+25: {:.2} (live 0.85)", dist[25]);
    assert!((dist[61] - 2.35).abs() < 0.25, "+61: {:.2} (live 2.35)", dist[61]);
}


fn player_actor(app: &mut App) -> (String, f32, bool, Vec3) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
    let (a, t) = q.single(world).unwrap();
    (a.state.clone(), a.vel_y, a.airborne, t.translation)
}

fn move_enemy_far(app: &mut App) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
    q.single_mut(world).unwrap().translation.z = 30.0;
}

#[test]
fn guard_walk_uses_the_guard_move_anims() {
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
    let (_, _, _, p0) = player_actor(&mut app);
    // Stick toward +Z = toward the camera (camera yaw 0 looks along -Z): the free guard walk faces
    // the camera's direction, so this is walking backward.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..60 {
        app.update();
    }
    let (state, _, _, p1) = player_actor(&mut app);
    assert_eq!(state, "DeflectGuardMoveB");
    // DeflectGuardMoveF root motion: 2.0 m over 40 frames = 1.5 m/s.
    let moved = (p1 - p0).with_y(0.0).length();
    assert!((moved - 1.5).abs() < 0.3, "moved {moved}");
}

#[test]
fn guarding_in_the_air_and_landing_in_guard() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut saw_air_guard = false;
    let mut landed = String::new();
    for i in 0..240 {
        if i == 30 {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.press(Action::Guard);
            pad.guard_held = true;
        }
        app.update();
        let (state, _, airborne, _) = player_actor(&mut app);
        saw_air_guard |= state.starts_with("AirDeflectGuard");
        if i > 30 && !airborne {
            landed = state;
            break;
        }
    }
    assert!(saw_air_guard);
    assert_eq!(landed, "LandAirDeflectGuard");
}

#[test]
fn kicking_off_the_enemy_relaunches() {
    let mut app = app();
    {
        // Enemy right in front (1 m).
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 1.0;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut kicked, mut launch) = (false, None);
    let mut max_vy_after = 0.0f32;
    for i in 0..200 {
        // Kick on the way down: the kick's enemy-jump window (frames 9-21) must meet the body.
        // Live game (rec_20261008_054259): the kick-jump hold starts 27 frames after the launch.
        if launch.map(|l| i == l + 14).unwrap_or(false) {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
        app.update();
        let (state, vy, airborne, _) = player_actor(&mut app);
        if airborne && launch.is_none() {
            launch = Some(i);
        }
        // Unlocked, no stick: HKS SELECTOR_GROUND_JUMP_TYPE_VERTICAL -> _N (live a000_213114).
        if state == "AirKickEnemyJumpStart_N" {
            kicked = true;
        }
        if kicked {
            max_vy_after = max_vy_after.max(vy);
        }
    }
    assert!(kicked, "{}", log_text(&app));
    // ChrPhysicsVelocityChangeParam 2101 relaunch.
    assert!(max_vy_after > 5.0, "vy {max_vy_after}");
}


#[test]
fn neck_grab_holds_then_throws() {
    // a000_003012: AtkParam 220 "Neck Grab" (throwFlag 1) -> ThrowAtk4100, judge 230 lands at frame 45.
    let out = defend_against("a000_003012", 0.0, Action::Guard, Vec2::ZERO);
    assert!(out.contains("GRABBED"), "{out}");
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003012");
    }
    let mut thrown = false;
    for _ in 0..400 {
        app.update();
        if log_text(&app).contains("thrown:") {
            thrown = true;
            break;
        }
    }
    assert!(thrown, "{}", log_text(&app));
}

#[test]
fn enemy_mid_swing_takes_the_hit_without_flinching() {
    let mut app = app();
    {
        // a000_003000 at frame 7: flag 73 (damage motion) is off from frame 9 until the recovery.
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
        a.t = 7.0 / TAE_FPS;
        a.prev_t = a.t;
        // Swinging away from the player, so only the player's cut lands.
        a.yaw = std::f32::consts::PI;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..40 {
        app.update();
        if log_text(&app).contains("you hit") {
            break;
        }
    }
    let (state, anim, _) = enemy_state(&mut app);
    assert!(log_text(&app).contains("you hit"), "{}", log_text(&app));
    assert_eq!((state.as_str(), anim.as_str()), ("Ez3000", "a000_003000"), "{}", log_text(&app));
}

#[test]
fn low_hp_slows_player_posture_regen() {
    // Default outfit residents 5221-5223 (conditionHp 75/50/25): x0.75 * x0.667 * x0.5 = x0.25 at 20 % HP.
    let regen_over = |hp_frac: f32| {
        let mut app = app();
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            let mut a = q.single_mut(world).unwrap();
            a.hp = a.hp_max * hp_frac;
            a.posture = 100.0;
            a.since_posture_damage = 99.0;
        }
        for _ in 0..120 {
            app.update();
        }
        let (_, _, _, posture) = player_state(&mut app);
        100.0 - posture
    };
    let full = regen_over(1.0);
    let low = regen_over(0.2);
    assert!(full > 0.0, "no regen at full HP");
    assert!((low / full - 0.25).abs() < 0.08, "full {full}, low {low}");
}

/// 90 s against the real AI with a bot that deflects 2 frames before each hit and
/// attacks when the enemy is open; the enemy must never get stuck in one state.
/// `cargo test duel_trace -- --nocapture` prints the outcome.
#[test]
fn duel_trace() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<&mut Enemy>();
        q.single_mut(world).unwrap().aggressive = true;
    }
    let mut last_state = String::new();
    let mut since_change = 0;
    let mut worst_stall = 0;
    let mut pressed_for = String::new();
    for i in 0..(90 * 60) {
        if let Some(dt) = time_to_enemy_hit(&mut app) {
            let (_, anim, _) = enemy_state(&mut app);
            let key = format!("{anim}@{:.2}", dt.max(0.0) + i as f32 / 60.0);
            if dt <= 2.0 / TAE_FPS && dt > 0.0 && !pressed_for.starts_with(&anim) {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.press(Action::Guard);
                pressed_for = key;
            }
        } else {
            pressed_for.clear();
            let (es, _, _) = enemy_state(&mut app);
            if (es.starts_with("AttackBound") || es.starts_with("Trunk") || es.starts_with("Guard")) && i % 20 == 0 {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
        }
        app.update();
        let (es, _, _) = enemy_state(&mut app);
        if es == last_state {
            since_change += 1;
            worst_stall = worst_stall.max(since_change);
        } else {
            since_change = 0;
            last_state = es;
        }
    }
    let world = app.world_mut();
    let mut q = world.query::<&Actor>();
    for a in q.iter(world) {
        println!("{:?} hp {:.0}/{:.0} posture {:.0}/{:.0} state {}", a.side, a.hp, a.hp_max, a.posture, a.posture_max, a.state);
    }
    println!("longest enemy state {:.1} s", worst_stall as f32 / 60.0);
    println!("{}", log_text(&app));
    // Longest legit state: the 140-frame 3014 charge (4.7 s).
    assert!(worst_stall < 6 * 60, "enemy stuck for {:.1} s", worst_stall as f32 / 60.0);
}

#[test]
fn attacking_out_of_a_deflect_uses_the_counter_slash() {
    let mut app = app();
    start_enemy_combo(&mut app, 0);
    let mut deflected = false;
    for _ in 0..240 {
        if !deflected {
            if let Some(dt) = time_to_enemy_hit(&mut app) {
                if dt <= 2.0 / TAE_FPS {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.press(Action::Guard);
                    deflected = true;
                }
            }
        }
        app.update();
        if log_text(&app).contains("YOU DEFLECT") {
            break;
        }
    }
    let (state, _, _, _) = player_state(&mut app);
    assert!(state.starts_with("StandDeflectHard"), "{state}\n{}", log_text(&app));
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = String::new();
    for _ in 0..40 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if s.starts_with("HardDeflectAtk") {
            seen = s;
            break;
        }
    }
    assert!(seen.starts_with("HardDeflectAtkSmall"), "got {:?}", player_state(&mut app));
}

#[test]
fn heavy_thrust_blows_the_player_away_and_lands() {
    // a000_003010 thrust (AtkParam 200, dmgLevel 4): ex-large blow, launched by TAE 920.
    let mut app = app();
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, PERILOUS_THRUST);
    }
    let mut seen = Vec::<String>::new();
    for _ in 0..400 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if seen.last() != Some(&s) {
            seen.push(s.clone());
        }
        if s.contains("BlowLand") {
            break;
        }
    }
    assert!(seen.iter().any(|s| s.contains("BlowStart")), "{seen:?}");
    assert!(seen.iter().any(|s| s.contains("BlowLand")), "{seen:?}");
}


#[test]
fn releasing_a_run_plays_the_run_stop() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..90 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    let mut states = Vec::<String>::new();
    for _ in 0..150 {
        app.update();
        let (s, _, _, _) = player_state(&mut app);
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s == "StandRunStopF"), "{states:?}");
    assert_eq!(states.last().map(|s| s.as_str()), Some("StandIdle"), "{states:?}");
}

#[test]
fn gourd_heals_forty_percent_at_the_consume_frame() {
    let mut app = app();
    move_enemy_far(&mut app);
    let max = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        a.hp = 20.0;
        a.hp_max
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    for _ in 0..90 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Actor, &Player), With<Player>>();
    let (a, p) = q.single(world).unwrap();
    assert!((a.hp - (20.0 + max * 0.4)).abs() < 1.0, "hp {} of {max}", a.hp);
    assert_eq!(p.gourd, 2);
}

#[test]
fn attack_plus_guard_is_whirlwind_slash_and_hits_twice() {
    let mut app = app();
    {
        let mut pad = app.world_mut().resource_mut::<PadInput>();
        pad.press(Action::Attack);
        pad.press(Action::Guard);
    }
    let mut hits = 0;
    let mut seen_art = false;
    let mut last_log = String::new();
    for _ in 0..90 {
        app.update();
        let (s, anim, _, _) = player_state(&mut app);
        seen_art |= s == "GroundSpecialAttackCombo1" && anim == "a100_316000";
        let l = log_text(&app);
        if l != last_log {
            hits += l.matches("you hit").count().saturating_sub(last_log.matches("you hit").count());
            last_log = l;
        }
    }
    assert!(seen_art, "{:?}", player_state(&mut app));
    // Judges 200 (frames 16-20) and 201 (30-36); the enemy may guard instead of being hit.
    assert!(hits + last_log.matches("blocked").count() + last_log.matches("DEFLECT").count() >= 1, "{last_log}");
}


#[test]
fn quick_shuriken_throw_reaches_the_enemy() {
    let mut app = app();
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    let mut out = String::new();
    for _ in 0..60 {
        app.update();
        out = log_text(&app);
        if out.contains("hit: -") || out.contains("blocks the shuriken") || out.contains("deflects the shuriken") {
            break;
        }
    }
    assert!(out.contains("Loaded Shuriken ("), "{out}");
    assert!(out.contains("hit: -") || out.contains("blocks the shuriken") || out.contains("deflects the shuriken"), "{out}");
    let world = app.world_mut();
    let mut q = world.query::<&Player>();
    assert_eq!(q.single(world).unwrap().emblems, 14);
}

#[test]
fn timed_step_dodges_through_the_slash() {
    // Neutral step 1 frame before the 6-frame hit window: GroundStep_N IFrames (General)
    // frames 0-9 outlast it. (Step too early and the still-active hitbox lands after them.)
    let out = defend_against("a000_003000", 1.0 / TAE_FPS, Action::Step, Vec2::ZERO);
    assert!(!out.contains("enemy hit"), "{out}");
}

/// Taps attack, then presses it again at `frame` of the tap slash; did Combo2 follow?
fn combo_press_at(frame: f32) -> bool {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..4 {
        app.update();
        if player_state(&mut app).0.starts_with("GroundAttackCombo1") {
            break;
        }
    }
    assert!(player_state(&mut app).0.starts_with("GroundAttackCombo1"), "{:?}", player_state(&mut app));
    while player_state(&mut app).2 * TAE_FPS < frame {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        if player_state(&mut app).0.starts_with("GroundAttackCombo2") {
            return true;
        }
    }
    false
}

#[test]
fn early_mash_is_dropped_but_a_press_in_the_accept_window_chains() {
    // Tap slash a050_300100: accept flag 87 from frame 6, execute (115) from frame 15.
    assert!(!combo_press_at(3.0), "a press before the accept window must not be buffered");
    assert!(combo_press_at(8.0), "a press inside the accept window chains into Combo2");
}

fn set_player_posture(app: &mut App, posture: f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Actor, With<Player>>();
    let mut a = q.single_mut(world).unwrap();
    a.posture = posture;
}

/// Lets the enemy swing `anim` until it connects; returns the player's state after the hit.
fn take_hit(app: &mut App, anim: &str) -> String {
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, anim);
    }
    let before = log_text(app).matches("hit:").count();
    for _ in 0..300 {
        app.update();
        if log_text(app).matches("hit:").count() > before {
            break;
        }
    }
    player_state(app).0
}

#[test]
fn posture_break_staggers_then_a_second_hit_knocks_wolf_down() {
    let mut app = app();
    let max = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Actor, With<Player>>();
        q.single(world).unwrap().posture_max
    };
    set_player_posture(&mut app, max - 1.0);
    let s = take_hit(&mut app, "a000_003000");
    assert!(s.starts_with("StandDamageBreak_"), "{s}\n{}", log_text(&app));
    let s = take_hit(&mut app, "a000_003000");
    assert_eq!(s, "StandDamageBreakDamage", "{}", log_text(&app));
    let mut seen = vec![];
    for _ in 0..400 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s.starts_with("StandDamageBreakDown_")), "{seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("StandDamageLargeDownWakeUp_")), "{seen:?}");
}

/// Largest root-motion scale seen during a held Combo1, optionally locked on to the enemy
/// (2.4 m ahead, or 30 m away when `far`).
fn slash_root_scale(locked: bool, far: bool) -> f32 {
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
fn locked_slash_lunges_toward_a_far_target() {
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
fn a_deflect_facing_away_is_still_hit() {
    // Guard arc (ShieldBlock ArgB 90 = front half) gates deflects too: a perfect press with
    // the attacker behind does nothing.
    let out = guard_against_attack_facing(2.0 / TAE_FPS, 0, true);
    assert!(out.contains("hit:") && !out.contains("DEFLECT"), "{out}");
}

#[test]
fn hit_in_the_air_uses_the_air_reactions() {
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

#[test]
fn resurrection_returns_wolf_at_half_hp() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    for _ in 0..300 {
        app.update();
        if player_state(&mut app).0 == "GroundDeathLoop_F" {
            break;
        }
    }
    assert_eq!(player_state(&mut app).0, "GroundDeathLoop_F");
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

#[test]
fn ochimusha_runs_its_own_ai_and_attacks() {
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
fn ochimusha_deathblow_uses_its_own_throw_rows() {
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

#[test]
fn ochimusha_attack_can_be_deflected() {
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
fn ochimusha_trace() {
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
fn ochimusha_guards_the_first_player_attack() {
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
fn air_slashes_chain_and_landing_continues_the_slash() {
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
fn attack_while_holding_guard_is_the_combat_art() {
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
fn deathblow_camera_closes_in_then_returns() {
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
fn tae151_look_limits_clamp_camera_angles() {
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
fn releasing_guard_while_moving_plays_the_move_run_end() {
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

/// Runs a sprint and presses attack the first frame the sprint start is up, steering `stick`.
fn sprint_attack_states(stick: Vec2) -> Vec<String> {
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
fn sprint_attack_selector_follows_the_hks_attack_angle() {
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
fn minimum_level_guarded_hit_keeps_wolf_state() {
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
fn force_player(app: &mut App, state: &str, frame: f32) {
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
fn guard_during_a_block_recoil_adds_a_deflect_window() {
    // Ref 228 (StandDeflectEasySmall frames 0-9) -> additive AddHardDeflectGuard, recoil keeps playing.
    let mut app = app();
    app.update();
    force_player(&mut app, "StandDeflectEasySmall_V1_F", 2.0);
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    app.update();
    app.update();
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Player>>();
    let a = q.single(world).unwrap();
    assert_eq!(a.state, "StandDeflectEasySmall_V1_F");
    assert!(!a.add_anim.is_empty(), "no additive deflect layer");
}

#[test]
fn guard_cancels_a_middle_hit_reaction() {
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
fn attack_out_of_a_sprint_loop_is_a_sprint_attack() {
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

#[test]
fn deathblow_from_behind_uses_the_back_throw() {
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
}

#[test]
fn plunging_onto_a_broken_enemy_is_a_deathblow() {
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
fn equipped_combat_art_ichimonji_plays_with_its_own_attack() {
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

#[test]
fn ochimusha_plunge_deathblow_uses_its_a200_pair() {
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
fn art_chain(weapon: i64, presses: usize) -> Vec<(String, String)> {
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
            press(&mut app);
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
fn ichimonji_double_follows_up_but_ichimonji_does_not() {
    // HKS _FireSpAttackCombo: spAtkType 102 needs SP_EF_REF_WEP_SP_ATK_UNLOCK_102_COMBO (281),
    // the resident SpEffect 140201 of Ichimonji: Double (7100). Combo2 = a102_316010.
    let double = art_chain(7100, 1);
    assert!(double.iter().any(|(s, a)| s == "GroundSpecialAttackCombo2" && a == "a102_316010"), "{double:?}");
    let single = art_chain(5300, 1);
    assert!(!single.iter().any(|(s, _)| s == "GroundSpecialAttackCombo2"), "{single:?}");
}

#[test]
fn floating_passage_chains_its_strikes() {
    // spAtkcategory 105 (5600): Combo1 -> 2 -> (no unlock 282) VariationCombo3 a105_317020;
    // the upgraded art (7200, unlock 282) goes 2 -> 3 -> 4 -> 5.
    let base = art_chain(5600, 4);
    let states: Vec<&str> = base.iter().map(|(s, _)| s.as_str()).collect();
    assert!(states.contains(&"GroundSpecialAttackCombo2") && states.contains(&"GroundSpacialAttackVariationCombo3"), "{base:?}");
    let up = art_chain(7200, 4);
    assert!(up.iter().any(|(s, a)| s == "GroundSpecialAttackCombo5" && a == "a105_316040"), "{up:?}");
}

#[test]
fn art_out_of_a_sprint_and_nightjar_steps() {
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

#[test]
fn hold_art_sheathes_and_releases() {
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
fn guard_tapped_before_the_window_is_dropped_but_held_guard_fires() {
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
fn guard_tap_in_a_hard_deflect_recoil_becomes_a_guard_at_ref_412() {
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
fn letting_go_of_attack_in_an_art_releases_it() {
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

#[test]
fn flowing_water_cuts_guarded_posture_damage_by_attack_attribute() {
    // Live 2026-10-08: Samurai General slash attacks (staminaPhysicsAttribute 1) cost x0.785 posture on
    // deflect / block, c1180 / c1470 strikes (2) x0.74 = SkillParam 280 "Flowing Water" -> SpEffect 150420
    // (deflect, stateInfo 158) / 150421 (guard, 204): defSlashStaminaDmgRate 0.8, others 0.75 (+ int truncation).
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut config = app.world().resource::<crate::config::GameConfig>().clone();
    config.player.skills = vec![];
    assert_eq!(crate::combat::guard_skill_rate(combat, &config, 1, true), 1.0, "no skill = level-1 Wolf");
    config.player.skills = vec![280];
    for deflect in [true, false] {
        assert!((crate::combat::guard_skill_rate(combat, &config, 1, deflect) - 0.8).abs() < 1e-6);
        assert!((crate::combat::guard_skill_rate(combat, &config, 2, deflect) - 0.75).abs() < 1e-6);
    }
}

#[test]
#[ignore]
fn throw_root_motion_probe() {
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
fn killed_mid_jump_falls_and_lands() {
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

#[test]
fn hit_sounds_follow_the_exe_material_lookup() {
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
fn clash_effects_follow_the_hit_effect_tables() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    use crate::vfx::{Kind, hit_sfx};
    let general = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 2, atk_type: 0, def_sfx_material1: 100, def_sfx_material2: 139, ..Default::default() };
    assert_eq!(hit_sfx(combat, &general, Kind::Deflect, [-1, -1]), [252001]);
    assert_eq!(hit_sfx(combat, &general, Kind::Guard, [-1, -1]), [201002]);
    let sword = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 1, atk_type: 0, ..Default::default() };
    if combat.foe0().chr == "c1020" {
        assert_eq!(hit_sfx(combat, &sword, Kind::Hit, crate::vfx::defender_sfx_materials(combat, &crate::actor::Actor::new(crate::actor::Side::Enemy, 1.0, 1.0, 0.0, 0))), [201001, 229001]);
    }
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
fn guard_walk_keeps_facing_and_follows_the_stick() {
    // Live (rec_20261008_051450, no lock-on): guard-walking, Wolf faces the camera's direction
    // (strafes); a stick 45 deg off it plays DeflectGuardMoveF (|angle| < 55) and moves along the
    // stick at ~1.5 m/s.
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
    // Faces the camera's forward (yaw 0 in tests).
    let off_cam = ((yaw1 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).to_degrees();
    assert!(off_cam.abs() < 1.0, "facing {off_cam:.1} deg off the camera");
    let moved = (end - start).with_y(0.0);
    assert!(moved.length() > 0.8 && moved.length() < 2.2, "moved {:.2} m in 1 s", moved.length());
    let want = crate::player::stick_world(Vec2::new(0.7071, 0.7071), 0.0);
    let off = moved.normalize().angle_between(want).to_degrees();
    assert!(off < 15.0, "moved {off:.0} deg off the stick");
}

#[test]
fn debug_perilous_drill_plays_only_red_attacks() {
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

/// Our General's attack choices over 3 simulated minutes (the player stands guarding at 2.4 m),
/// to compare with the live fight (tools: rec_20261008_054259 attack counts).
#[test]
#[ignore]
fn ai_attack_distribution() {
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
fn guard_and_deflect_sounds_follow_the_exe_tables() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    // A General sword attack: AtkParam_Npc defSeMaterial1/2 = 100 / 139, atkMaterial_forSe 0 (Iron).
    let atk = combat.enemy0().attacks.values().find(|a| a.def_se_material1 == 100 && a.atk_material_se == 0 && a.atk_type == 0).expect("sword attack");
    assert_eq!(crate::sound::guard_sounds(combat, atk, true), ["z199999980"]);
    assert_eq!(crate::sound::guard_sounds(combat, atk, false), ["z200000101"]);
}

#[test]
fn locked_on_jumps_go_where_the_stick_points() {
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
fn locked_on_step_tilts_toward_the_stick() {
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

#[test]
#[ignore]
fn probe_block_then_move() {
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
fn guard_release_while_moving_keeps_running() {
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
fn behind_deathblow_root_yaw_is_played_as_stored() {
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
fn sheathe_walk_and_draw() {
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
fn locked_on_diagonal_walk_twists_the_legs() {
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
fn break_enemy_at(app: &mut App, z: f32) {
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
fn general_deathblow_goes_on_to_the_kill_anim_as_he_dies() {
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
fn ochimusha_close_deathblow_uses_the_near_rows() {
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
fn breaking_deflect_plays_the_pose_and_attack_finishes_it() {
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
fn air_attack_out_of_a_head_kick_on_a_broken_enemy_is_the_kick_down() {
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
fn mikiri_on_the_ochimusha_breaks_him_into_the_mikiri_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    // Live (rec_c1010_20261009, 3 of 3): GroundStep_F into the thrust Attack3013 -> 見切り崩し
    // (ThrowParam 11010120): Wolf a200_511100, ThrowDef13100 (the thrust's parry posture damage
    // empties his posture), then attack -> 511110 / ThrowDef13110.
    let outs: Vec<String> = (2..10).map(|f| defend_against_with("c1010", "a000_003013", f as f32 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0))).collect();
    assert!(outs.iter().any(|o| o.contains("enemy=ThrowDef13100") && o.contains("player=ThrowBreak")), "{outs:#?}");
}

#[test]
fn landing_mid_air_deflect_continues_in_its_land_anim() {
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

#[test]
fn jumping_at_a_broken_enemy_vaults_over_him() {
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
fn debug_menu_plunge_setup_reaches_the_plunge_deathblow() {
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

fn make_enemy_unaware(app: &mut App, z: f32, back_turned: bool) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
    let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
    t.translation.z = z;
    // Wolf faces +Z (yaw PI): back turned = facing +Z too.
    a.yaw = if back_turned { std::f32::consts::PI } else { 0.0 };
    e.make_unaware(&mut a);
}

#[test]
fn attacking_an_unaware_enemy_from_behind_is_the_stealth_deathblow() {
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

#[test]
fn an_unaware_enemy_notices_wolf_in_front_but_not_behind() {
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
fn plunging_onto_an_unaware_enemy_is_the_stealth_plunge() {
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


/// Moves Wolf (feet height kept) without touching his state.
fn put_wolf(app: &mut App, x: f32, z: f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Transform, With<Player>>();
    let mut t = q.single_mut(world).unwrap();
    t.translation.x = x;
    t.translation.z = z;
}

fn enemy_info(app: &mut App) -> (u8, u8, String, String, Vec3) {
    let world = app.world_mut();
    let mut q = world.query::<(&Enemy, &Actor, &Transform)>();
    let (e, a, t) = q.single(world).unwrap();
    (e.targeting.state, e.targeting.kind, a.anim.clone(), e.ai_desc.clone(), t.translation)
}

/// cargo test --release stealth_trace -- --ignored --nocapture : the stealth loop, logged.
/// SHINOBI_STEALTH_TRACE = leave (default: Wolf leaves once it is alerted) | stay | lose
/// (c1010 fights, then Wolf gets out of its sight and is forgotten after SightTargetForgetTime 15 s).
#[test]
#[ignore]
fn stealth_trace() {
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

#[test]
fn crouching_cuts_how_far_enemies_see_wolf() {
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

#[test]
fn running_near_an_unaware_enemy_is_heard_but_crouching_is_not() {
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

/// HKS g_paramHkbState: CROUCH_START is STATE_TYPE_STANDBY, so the stick moves Wolf out of the
/// crouch-down at once (it used to hold him for its 0.83 s).
#[test]
fn moving_right_after_crouching_is_not_held() {
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
fn crouch_while_sprinting_is_the_slide() {
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
fn run_deflect_trace() {
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
fn plunge_idle_trace() {
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
fn crouch_drift_trace() {
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

/// cargo test --release behind_throw_live_trace -- --ignored --nocapture : the c1010 behind
/// deathblow (a200_511200 / ThrowDef13200) in the game's own conventions (x mirrored, yaw = pi -
/// ours), one line per 1/30 s like tools/rec_throw.py on rec_c1010_20261009 (tick 1985...).
#[test]
#[ignore]
fn behind_throw_live_trace() {
    let mut app = app_with("c1010");
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
fn switching_prosthetics_plays_the_next_tool() {
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
/// GetSpDamage gives SP_DAMAGE_BURST -> W_AssassinationBloodReaction. The 30 s cool time (107100)
/// keeps a second firecracker from staggering him again.
#[test]
fn firecracker_staggers_the_general_once_per_cool_time() {
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
    for _ in 0..90 {
        app.update();
        assert_ne!(enemy_state(&mut app).0, "AssassinationBloodReaction", "staggered again inside the cool time");
    }
}

/// Every exported enemy (extracted/enemies/roster.json, `sekiro-extract npcs`): loads as itself,
/// its real AI scripts load and attack the idle player within 20 s, and a posture-broken enemy
/// takes Wolf's deathblow. Prints one line per enemy; run with
/// `cargo test --release all_enemies_smoke -- --ignored --nocapture`.
#[test]
#[ignore]
fn all_enemies_smoke() {
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
            for i in 0..1200 {
                app.update();
                anims.insert(enemy_state(&mut app).1);
                if first_hit.is_none() && (log_text(&app).contains("enemy hit:") || log_text(&app).contains("thrown:")) {
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
            let npc = combat.param("NpcParam", foe.npc_row);
            let can_blow = combat.throw(foe.throw_row(0)).is_some();
            let fights = foe.think_id > 3 && npc["hp"].as_i64().unwrap_or(0) < 9999;
            let world = app.world_mut();
            let desc = world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().ai_desc.clone();
            (foe, failed, desc, anims.len(), first_hit, can_blow, fights)
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
            (Ok((foe, failed, desc, n, hit, can_blow, fights)), Ok((pa, es))) => {
                let good = foe.chr == chr && !failed && (hit.is_some() || !fights) && (es.starts_with("ThrowDef") || !can_blow);
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
                )
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
fn fight_until_hits(app: &mut App, frames: u32, want: u32) -> (u32, usize) {
    let mut attacks = std::collections::BTreeSet::new();
    let mut hits = 0;
    let mut last_hit = 0;
    let mut was_grabbed = false;
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
        if a.hp < a.hp_max || grabbed {
            hits += 1;
            last_hit = f;
            a.hp = a.hp_max;
        }
        a.posture = 0.0;
        // Wolf closes in like a player would (3 m/s up to 2.5 m; after 8 s without a hit right
        // onto it, as onto the hidden ground zombie) and faces it.
        let to = (et - t.translation).with_y(0.0);
        let stop = if f - last_hit > 480 { 0.05 } else { 2.5 };
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
fn trace_phase() {
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
fn dbg_blow_after_fight() {
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
        let c = crate::player::deathblow_check(world.resource::<Combat>(), wolf, ea, e, et.translation, &|_| None);
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
fn all_enemies_full() {
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
                let npc = combat.param("NpcParam", foe.npc_row);
                (foe.think_id > 3 && npc["hp"].as_i64().unwrap_or(0) < 9999, combat.throw(foe.throw_row(0)).is_some(), blows)
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
fn hand_over(app: &mut App) -> bool {
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
fn use_tool(tool: i64, emblems: u32, hold: usize, frames: usize) -> (Vec<String>, u32, f32) {
    let mut app = app();
    {
        let world = app.world_mut();
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

fn enemy_hp(app: &mut App) -> f32 {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Enemy>>();
    q.single(world).unwrap().hp
}

/// Every tool's level 1 from idle (HKS BEH_A_GROUND_SUB_ATTACK, line 3523): the state it starts,
/// and the Spirit Emblems it costs - EquipParamWeapon resourceItemA, paid by each behaviour whose
/// BehaviorParam_PC row has wepCost (CharData::behavior). Level-1 tools carry ref 314 (always the
/// quick release).
#[test]
fn every_tool_starts_and_costs_its_emblems() {
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
fn a_tool_without_emblems_fails() {
    let (states, spent, _) = use_tool(70000, 0, 0, 60);
    assert!(states.iter().any(|s| s == "SubAttackFailed"), "{states:?}");
    assert_eq!(spent, 0);
}

/// The Loaded Axe's swing (a073_400100 judge 102 -> v7300:102) lands on the General standing
/// 2 m away: he guards it (posture) or takes the Axe's own attackBasePhysics 45 x 250 %.
#[test]
fn the_axe_hits() {
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
fn okinagas_flame_vent_spews_while_held() {
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
fn the_umbrella_opens_while_held() {
    let (states, _, _) = use_tool(76000, 20, 90, 160);
    for want in ["GroundSubAttackGuardStart", "GroundSubAttackGuardLoop", "GroundSubAttackGuardEnd"] {
        assert!(states.iter().any(|s| s == want), "no {want}: {states:?}");
    }
}

/// The Sabimaru's combo (HKS line 3605): a press inside SP_EF_REF_TAE_ENABLE_SUB_ATTACK_COMBO of
/// GroundSubAttackCombo1 goes on to Combo2 (free: only the odd steps cost).
#[test]
fn the_sabimaru_combos() {
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
fn enemy_trace() {
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
fn the_flame_vent_burns() {
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

/// The Sabimaru poisons: each cut puts 9004 (poizonAttackPower 31, stateInfo 2) toward the
/// General's resist_poison; full, 9004 hands on to 9045 (replaceSpEffectId), which takes 0.75 %
/// + 12 HP every second for 19.9 s.
#[test]
fn the_sabimaru_poisons() {
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
fn okinagas_spew_costs_while_held() {
    let (_, tap, _) = use_tool(72200, 30, 0, 200);
    // HoldLoop starts ~3.2 s in (Combo1 f75 + HoldStart); its 999 fires 3.67 s later.
    let (states, held, _) = use_tool(72200, 30, 600, 650);
    assert!(held >= tap + 6, "held {held} vs tap {tap}: {states:?}");
    let (states, spent, _) = use_tool(72200, 6, 600, 650);
    assert!(spent <= 6 && states.iter().any(|s| s == "SubAttackFailed" || s.ends_with("SubAttackFailed")), "{spent}: {states:?}");
}


/// Fang and Blade (HKS BEH_A_GROUND_ATTACK, line 3257): the attack button inside a tool move's
/// SP_EF_REF_TAE_ENABLE_SUB_ATTACK_DERIVE_ATTACK window follows up with the tool's slash: the
/// Shuriken's and Spear's W_GroundSubAttackDeriveAttackCombo1, the Sabimaru LV2's directed cut (LV1
/// 75000 carries ref 315 WEP_DISABLE_DERIVE_SUB_ATTACK_COMBO).
#[test]
fn tools_follow_up_with_the_attack_button() {
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
fn the_mist_raven_leap_rises_and_falls() {
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
fn tools_work_in_the_air() {
    use crate::prosthetic::hks;
    let none = |_: i64| false;
    assert_eq!(hks::air_press(71, true, 1, false, &none), None);
    assert_eq!(hks::air_press(70, true, 1, false, &none), Some(("AirSubAttackCombo1", false)));
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

/// Tools out of a sprint and a crouch (BEH_A_GROUND_SUB_ATTACK 3523 by style): the first state each
/// plays. Only 070 / 071 / 076 / 079 have crouch versions (W_CrouchSubAttackCombo / GuardStart /
/// *Moveable); the umbrella closes by W_GroundSubAttackGuardEnd in every style (line 3779); the
/// whistle has no sprint branch (walking: Combo1Move).
#[test]
fn tools_from_a_sprint_and_a_crouch() {
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
fn deathblows_give_back_posture_and_hp_with_the_skills() {
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
/// deathblow recoveries are blocked: same spCategory, categoryPriority 2 beats 3.
#[test]
fn todome_blocks_the_deathblow_recoveries() {
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

/// Every combat art from idle (HKS BEH_A_GROUND_SP_ATTACK 3388): its opening state and the Spirit
/// Emblems it spends (EquipParamWeapon resourceItemA through its wepCost behaviours). Shadowrush /
/// Shadowfall (109, 2) pay only in the hit jump (judge 215), so a miss is free
/// (`shadowrush_jumps_off_the_hit`).
#[test]
fn every_art_starts_and_costs_its_emblems() {
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
        let (states, spent) = use_art(art, 10, 240);
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
fn shadowrush_jumps_off_the_hit() {
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
fn arts_in_the_air() {
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
    let seen = run(6100, Some(30), 1);
    for want in ["AirSpecialAttackHoldStart", "AirSpacialAttackHoldEnd"] {
        assert!(has(&seen, want), "6100 {want}: {seen:?}");
    }
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
fn arts_combo_out_of_their_jump_and_hold() {
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

/// One art use from idle: equips `art` with `emblems` Spirit Emblems, presses attack + guard and
/// runs `frames`; returns the states Wolf went through and the emblems spent.
fn use_art(art: i64, emblems: u32, frames: usize) -> (Vec<String>, u32) {
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
fn dragon_flash_costs_two_emblems() {
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
fn latent_skill_rates() {
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
fn the_finger_whistle_plays_while_walking() {
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
fn the_finger_whistle_plays_while_crouch_walking() {
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
fn enemy_chrs(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let kinds: Vec<String> = world.resource::<Combat>().kinds.iter().map(|k| k.foe.chr.clone()).collect();
    let mut v: Vec<String> = world.query::<(&Actor, &Enemy)>().iter(world).filter(|(_, e)| !e.is_disabled()).map(|(a, _)| kinds[a.kind].clone()).collect();
    v.sort();
    v
}

/// Several enemies of different kinds at once (config enemy.group): each with its own chr, rows
/// and data, all on the enemy side (no hits between them), all after Wolf.
#[test]
fn group_of_different_enemies_fights_together() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1010.json")).exists() {
        return;
    }
    let g = |chr: &str, at: [f32; 2]| crate::config::GroupEnemy { chr: chr.into(), npc_row: None, at: Some(at) };
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

/// NpcParam teamType: an Interior Ministry samurai of the invasion (c1700 17001000, team 24
/// "Enemy 2") and an Ashina soldier (c1010, team 6) fight each other while Wolf is far off
/// (combat::teams_hostile).
#[test]
fn the_invaders_fight_ashina() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1700.json")).exists() {
        return;
    }
    let g = crate::config::GroupEnemy { chr: "c1700".into(), npc_row: Some(17001000), at: Some([0.0, 6.0]) };
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
fn genichiro_becomes_way_of_tomoe() {
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

/// The Corrupted Monk (c5000 50000000) at 2 health bars enables her three phantoms (c5005,
/// m25 12505961). They stay out of the fight until she has SpEffect 5031 (her Attack3032 /
/// 3033), then come in one after another (1 / 2 / 3.5 s, plus 1 s when she sent message 50)
/// where the event's region checks put them, and leave on their message 70 or when it ends.
/// The Monk's phantoms are seen only through their a200 attacks: TAE 193 SetOpacityKeyframe
/// fades them in at the start and out at the end (Actor::opacity, model.rs update_opacity), and
/// they stay at 0 until the next one.
#[test]
fn monk_phantoms_fade_in_and_out() {
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
fn monk_phantoms_come_and_go() {
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
fn mist_raven_leap_sectors_follow_the_hks() {
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
fn skills_unlock_their_moves() {
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
}

/// Genichiro's lightning (c7110 a000_003042 f50 AttackBehavior 361: AtkParam 71100361
/// spAttribute 10) catching Wolf in a jump: he is charged (AirDamageElectroChargeWeakStart,
/// no HP lost), and an attack before landing throws it back (AirWeakElectroReceiveAttack).
#[test]
fn lightning_caught_in_the_air_charges_wolf_and_an_attack_throws_it_back() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c7110.json")).exists() {
        return;
    }
    // The jump's timing against the strike (f50 = 1.67 s): try a few take-offs. The weak charge
    // (a000_102930) lifts by row 1612 (+3.2 m/s, the fall kept at half) and takes the release
    // only from 0.7 s (flag 115), so a catch near the top of the jump lands first.
    let mut tries = vec![];
    for jump_at in [0.95f32, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.45] {
        let mut app = app_with("c7110");
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003042");
        }
        let hp0 = {
            let world = app.world_mut();
            world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp
        };
        let mut jumped = false;
        let mut caught = false;
        for _ in 0..240 {
            let t = enemy_state(&mut app).2;
            if !jumped && t >= jump_at {
                app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
                jumped = true;
            }
            app.update();
            if player_state(&mut app).0.starts_with("AirDamageElectroCharge") {
                caught = true;
                break;
            }
        }
        if !caught {
            tries.push((jump_at, vec![]));
            continue;
        }
        let s = player_state(&mut app).0;
        assert_eq!(s, "AirDamageElectroChargeWeakStart");
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!(a.hp >= hp0 && a.electro.is_some_and(|c| c.0 == 9490), "hp {} -> {}, charge {:?}", hp0, a.hp, a.electro);
        let mut seen = vec![s];
        for i in 0..240 {
            if i % 5 == 0 && !seen.iter().any(|s| s == "AirWeakElectroReceiveAttack") {
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(&mut app).0;
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        if seen.iter().any(|s| s == "AirWeakElectroReceiveAttack") {
            // The release lands into its own Land anim, not the shock (HKS 1920-1925).
            assert!(seen.iter().any(|s| s == "LandAirWeakElectroReceiveAttack") && !log_text(&app).contains("SHOCKED"), "{seen:?}
{}", log_text(&app));
            let world = app.world_mut();
            assert!(world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().electro.is_none(), "still charged: {seen:?}");
            return;
        }
        tries.push((jump_at, seen));
    }
    panic!("no release: {tries:?}");
}

#[test]
fn a_jump_in_an_updraft_is_the_storm_jump() {
    // The Divine Dragon's updraft bullets give Wolf 106100 (ref 110003, 0.5 s): a jump there is
    // W_GroundStormJumpReady -> GroundStormJumpStart, launched by TAE 922 row 980 (+25 m/s); in
    // the air, from 1.0 s (ref 101), a second press is W_AirStormJumpStart; the fall lands in
    // W_LandStormJumpFall.
    let mut app = app();
    let updraft = |app: &mut App| {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.retain(|(id, _)| *id != 106100);
        p.timed.push((106100, 0.5));
    };
    move_enemy_far(&mut app);
    updraft(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let (mut seen, mut top, mut air_jump) = (vec![], 0.0f32, false);
    let y0 = player_actor(&mut app).3.y;
    for i in 0..400 {
        let (s, t) = { let ps = player_state(&mut app); (ps.0, ps.2) };
        if s == "GroundStormJumpStart" && t > 1.1 && !air_jump {
            updraft(&mut app);
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
            air_jump = true;
        }
        if i < 20 {
            updraft(&mut app);
        }
        app.update();
        top = top.max(player_actor(&mut app).3.y - y0);
        if std::env::var("SHINOBI_DBG").is_ok() && i % 4 == 0 {
            let (st, vy, air, pos) = player_actor(&mut app);
            eprintln!("{i} {st} t {:.2} vy {vy:.2} air {air} y {:.2}", player_state(&mut app).2, pos.y - y0);
        }
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    for want in ["GroundStormJumpReady", "GroundStormJumpStart", "AirStormJumpStart", "LandStormJumpFall"] {
        assert!(seen.iter().any(|s| s == want), "no {want}: {seen:?}");
    }
    assert!(top > 8.0, "rose only {top:.1} m: {seen:?}");
}

/// The Divine Dragon fight (m25 events, NpcParam 52000000) up to Wolf being airborne in an
/// updraft once the fight runs (12505888 set).
#[cfg(test)]
fn dragon_app() -> Option<App> {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5310.json")).exists() {
        return None;
    }
    let mut app = app_cfg("c5200", Some(52000000), Vec::new());
    for i in 0..(60 * 7) {
        if i == 2 {
            let world = app.world_mut();
            for mut e in world.query::<&mut Enemy>().iter_mut(world) {
                e.aggressive = true;
            }
        }
        app.update();
    }
    Some(app)
}

#[test]
fn the_divine_dragons_lightning_comes_back_at_it() {
    // m25 12505916: Wolf touching a lightning sign (53100681: 3531061, 2 s) brings the dragon's
    // strong lightning 252000555 (radius 150, AtkParam 52000510 spAttribute 6); caught in the
    // air it charges him (9495), and his release's bolt 500184 puts 3520085 on the dragon, which
    // with a hit on its head (part 2 -> PartBlend_Add02 a000_009511: 3520081) staggers it
    // (12505927 slot 1 -> 20004). At 30 % HP (12505889 -> 12505890) the same brings its collapse
    // (12505887: 20002, then Event21000 with 932 / 104), where Wolf's finisher 15200090 ends the
    // fight (ThrowDef12900; 12505880 Handle Boss Defeat).
    let Some(mut app) = dragon_app() else { return };
    let dragon = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &crate::enemy::script::Scripted)>();
        q.iter(world).find(|(_, s)| s.0 == 2500800).map(|(e, _)| e).unwrap()
    };
    // A "head" capsule (part 2) at its root (no model here).
    let dpos = app.world().get::<Transform>(dragon).unwrap().translation;
    {
        let world = app.world_mut();
        let ma = world.spawn((Transform::from_translation(dpos), GlobalTransform::from_translation(dpos))).id();
        let mb = world.spawn((Transform::from_translation(dpos + Vec3::Y * 4.0), GlobalTransform::from_translation(dpos + Vec3::Y * 4.0))).id();
        world.entity_mut(dragon).insert(crate::anim::Hurtboxes(vec![(ma, mb, 3.0)], vec![2]));
    }
    let set_timed = |app: &mut App, id: i64, secs: f32| {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.retain(|(x, _)| *x != id);
        p.timed.push((id, secs));
    };
    let dragon_anim = |app: &App| app.world().get::<Actor>(dragon).unwrap().anim.clone();
    // Wolf 12 m from it, up in an updraft (both storm jumps), the sign on the way down, the
    // release locked on and facing it. Until the dragon plays one of `want`.
    let reversal = |app: &mut App, want: &[&str]| -> Vec<String> {
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
            let (mut a, mut t) = q.single_mut(world).unwrap();
            t.translation = Vec3::new(dpos.x, t.translation.y, dpos.z + 12.0);
            a.yaw = 0.0;
        }
        set_timed(app, 106100, 0.5);
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut seen: Vec<String> = Vec::new();
        let (mut signed, mut released, mut air_jump) = (false, false, false);
        for i in 0..(60 * 8) {
            if i < 20 {
                set_timed(app, 106100, 0.5);
            }
            let (s, t) = { let ps = player_state(app); (ps.0, ps.2) };
            if s == "GroundStormJumpStart" && t > 1.1 && !air_jump {
                set_timed(app, 106100, 0.5);
                app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
                air_jump = true;
            }
            if s == "StormJumpFall" && !signed {
                set_timed(app, 3531061, 2.0);
                signed = true;
            }
            let charged = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().electro.is_some();
            if charged && !released && i % 5 == 0 {
                app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(dragon);
                let world = app.world_mut();
                let mut q = world.query_filtered::<(&mut Actor, &Transform), With<Player>>();
                let (mut a, t) = q.single_mut(world).unwrap();
                let to = (dpos - t.translation).with_y(0.0);
                a.yaw = f32::atan2(-to.x, -to.z);
                app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
            }
            app.update();
            let s = player_state(app).0;
            if s.contains("ElectroReceiveAttack") {
                released = true;
            }
            if seen.last() != Some(&s) {
                seen.push(s);
            }
            let da = dragon_anim(app);
            if want.iter().any(|w| da.ends_with(w)) {
                return seen;
            }
        }
        let world = app.world_mut();
        let d = world.query::<(&Actor, &Enemy)>().get(world, dragon).map(|(a, e)| format!("{} {} add {} clash {:?}", a.anim, a.hp, a.add_anim, e.clash)).unwrap();
        panic!("no {want:?}: wolf {seen:?} dragon {d}
{}", log_text(app));
    };
    let seen = reversal(&mut app, &["_020004"]);
    assert!(seen.iter().any(|s| s == "AirDamageElectroChargeStart"), "{seen:?}");
    // Let the stagger and Wolf's landing play out.
    for _ in 0..(60 * 6) {
        app.update();
    }
    // At 30 %: the collapse.
    {
        let mut a = app.world_mut().get_mut::<Actor>(dragon).unwrap();
        a.hp = a.hp_max * 0.25;
    }
    // (Its 20001 first: 12505887 on 12505889.)
    let mut t = 0;
    while t < 60 * 2 || dragon_anim(&app).ends_with("_020001") {
        app.update();
        t += 1;
        assert!(t < 60 * 25, "stuck in {}", dragon_anim(&app));
    }
    reversal(&mut app, &["_020002", "_021000"]);
    let mut t = 0;
    while !dragon_anim(&app).ends_with("_021000") {
        app.update();
        t += 1;
        assert!(t < 60 * 15, "no Event21000: {}", dragon_anim(&app));
    }
    // The finisher: next to it (its judge dummy 230 without a model: the root), any side.
    {
        let world = app.world_mut();
        let dpos = world.get::<Transform>(dragon).unwrap().translation;
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        t.translation = Vec3::new(dpos.x + 1.0, t.translation.y, dpos.z);
        a.yaw = f32::atan2(1.0, 0.0);
    }
    for _ in 0..2 {
        app.update();
    }
    {
        let world = app.world_mut();
        let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let mut q = world.query::<(&Actor, &Enemy, &Transform)>();
        let (ea, e, et) = q.get(world, dragon).unwrap();
        let c = crate::player::deathblow_check(world.resource::<Combat>(), wolf, ea, e, et.translation, &|_| None);
        assert!(c.as_ref().is_some_and(|c| c.in_reach), "finisher not open: {} {:?} wolf {wolf} dragon {}", ea.anim, c.map(|c| c.suffix), et.translation);
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut defeated = false;
    for _ in 0..(60 * 20) {
        app.update();
        let bs = app.world().resource::<crate::enemy::BossScript>();
        if bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) {
            defeated = true;
            break;
        }
    }
    assert!(defeated, "not defeated: wolf {:?} dragon {}
{}", player_state(&mut app), dragon_anim(&app), log_text(&app));
}

#[test]
fn dragon_probe() {
    if std::env::var("SHINOBI_DBG").is_err() {
        return;
    }
    let mut app = app_cfg("c5200", None, Vec::new());
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
    }
    let mut last = String::new();
    for f in 0..1800 {
        app.update();
        let world = app.world_mut();
        let n = world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).count();
        let (ea, et) = world.query_filtered::<(&Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).map(|(a, t)| (a.anim.clone(), t.translation)).unwrap();
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
        let (mut a, mut t) = q.single_mut(world).unwrap();
        let hp = a.hp;
        a.hp = a.hp_max;
        let to = (et - t.translation).with_y(0.0);
        if to.length() > 2.5 {
            t.translation += to.normalize() * 3.0 / 60.0;
        }
        a.yaw = f32::atan2(-to.x, -to.z);
        let line = format!("{ea} bullets {n} dist {:.1} wolf {} max {} y {:.1}/{:.1}", to.length(), a.state, a.hp_max, t.translation.y, et.y);
        if line != last || hp < a.hp_max {
            eprintln!("{f} {line} hp {hp}");
            last = line;
        }
    }
    eprintln!("{}", log_text(&app));
}
