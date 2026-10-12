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
    app_build(chr, npc_row, group, false)
}

/// The sim with the characters' real geometry (anim.rs skeletons, model.rs GeometryPlugin):
/// hits test the AtkParam capsule between dummy polys against the ragdoll hurt capsules, as in
/// the game, instead of the reach approximation.
fn app_geom(chr: &str, npc_row: Option<i64>) -> App {
    app_build(chr, npc_row, Vec::new(), true)
}

fn app_build(chr: &str, npc_row: Option<i64>, group: Vec<crate::config::GroupEnemy>, geometry: bool) -> App {
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
    if geometry {
        app.add_plugins((bevy::transform::TransformPlugin, crate::anim::AnimPlugin, crate::model::GeometryPlugin));
    }
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
fn enemy_grabs_go_through_the_throw_kind_table() {
    // ThrowKindParam "[Enemy] Throw n" (throwCheckSituation 4) matched on AtkParam throwTypeId:
    // the zombies' knife stab 0, eye gouge 1, restraint 2 -> ThrowAtk4100 / 4110 / 4120.
    let app = app_with("c1500");
    let combat = app.world().resource::<Combat>();
    let foe = &combat.kind(0).foe;
    let ids: Vec<i64> = (0..3).filter_map(|t| combat.enemy_grab(foe, t)).map(|t| t.atk_anim_id).collect();
    assert_eq!(ids, vec![4100, 4110, 4120]);
    assert!(combat.enemy_grab(foe, 4).is_none());
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

/// Crouched, the gourd is the crouch drink (HKS STYLE_TYPE_CROUCH: W_CrouchItemGourdDrink,
/// a000_250010): Wolf stays crouched and it heals the same.
#[test]
fn a_crouched_drink_keeps_wolf_crouched() {
    let mut app = app();
    move_enemy_far(&mut app);
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..40 {
        app.update();
    }
    let max = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        assert!(a.crouch);
        a.hp = 20.0;
        a.hp_max
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(player_state(&mut app).0, "CrouchItemGourdDrink");
    for _ in 0..90 {
        app.update();
    }
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Actor, &Player), With<Player>>();
    let (a, p) = q.single(world).unwrap();
    assert!(a.crouch, "standing after the drink: {}", a.state);
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

/// Wolf's procedural walk (Locomotion, no TAE anim) plays its clip's PlaySound events (the
/// footsteps): over two loops of the walk each fires twice (sound.rs `sound_windows`).
#[test]
fn locomotion_plays_its_clip_sounds() {
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

/// A cross-fade runs on the character's own clock: hit stop freezes it with the clip (sekiro-rs
/// fd53f99; it ran on the frame time, finishing the fade while the pose stood still).
#[test]
fn hit_stop_freezes_the_cross_fade() {
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

fn enemy_broken(app: &mut App) -> bool {
    let world = app.world_mut();
    world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap().is_broken()
}

/// A plunge onto a broken enemy plays its defender pose ThrowDef13400 and holds it broken through
/// the fall: its Broken window had 0.3 s left, which ran out mid-fall, and it stood up in its
/// collapse instead (react does nothing on a broken enemy; 2026-10-12 bug hunt round 15).
#[test]
fn a_plunge_holds_the_broken_enemy_in_its_pose() {
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


/// Wolf falling onto the enemy from `above` m, with both of them `dy` m off the sim floor.
fn start_falling_onto(app: &mut App, above: f32, dy: f32) {
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
fn a_broken_enemy_elsewhere_keeps_the_stealth_plunge() {
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
fn an_attack_from_a_crouch_stands_wolf_up() {
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

/// The behind deathblow's absorb (ThrowParam 11010111, atkSorbDmyId 249) snaps the enemy's
/// capsule onto Wolf's dummy, and its model slides there over adsrobModelPosInterpolationTime
/// (0.5 s) instead of jumping.
#[test]
fn an_absorbed_enemy_model_slides_onto_the_throw_dummy() {
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
fn behind_throw_live_trace() {
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
/// The characters that do not fight (merchants, nuns, handmaidens, Hanbei, scripted set pieces,
/// HANDOFF 4.11 / PLAN "fighters only"): the smoke test only asks that they stand - spawn, idle,
/// stay alive on their feet and never panic. c1300 waits for its Aging status.
const NON_FIGHTERS: &[&str] = &["c1012", "c1110", "c1111", "c1260", "c1300", "c1320", "c1460", "c5410", "c7200", "c7300", "c7420", "c7430", "c7440", "c7450"];

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
            let npc = combat.param("NpcParam", foe.npc_row);
            let can_blow = combat.throw(foe.throw_row(0)).is_some();
            let fights = foe.think_id > 3 && npc["hp"].as_i64().unwrap_or(0) < 9999;
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

/// A guarding General blocks a shuriken, but the Flame Vent (AtkParam disableGuard /
/// disableJustGuard 1) goes through his guard and hurts him.
#[test]
fn the_flame_vent_goes_through_a_guard() {
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

/// Sen Throw (70400) has its own BehaviorParam_PC rows, 107004100 / 107004150, which fire nothing
/// (refType 0): no free Shuriken LV1 (the bare judge's 700000) flies.
#[test]
fn sen_throw_throws_no_free_shuriken() {
    let (states, spent, hp) = use_tool(70400, 20, 0, 150);
    assert!(!states.is_empty());
    assert_eq!((spent, hp), (0, 0.0), "states {states:?}");
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
fn the_deathblow_mark_is_on_the_enemy_the_deathblow_takes() {
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
fn a_reset_brings_the_sandbox_line_up_back() {
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
fn the_invaders_fight_ashina() {
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

/// Wolf's first slash with the real geometry against the General at `dist` m: the TAE frame of
/// the first contact (his AtkParam capsule between the sword's dummy polys against the
/// General's ragdoll capsules), None for a miss.
fn geom_first_hit_frame(dist: f32) -> Option<(String, f32)> {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.z = dist;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    // (The hit can swap the state the same frame: a deflect.)
    let mut last = (String::new(), 0.0);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.attack_hit {
            return Some(last);
        }
        last = (a.anim.clone(), a.t * TAE_FPS);
    }
    None
}

/// Wolf's push-back after his first slash connects on `chr` at 2 m: the knockback the enemy's
/// NpcParam knockbackRate_vsPlayer_* cut refused (TAE flag 72 KnockbackValue, combat::recoil).
fn wolf_recoil_on(chr: &str) -> Option<Vec3> {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.z = 2.0;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if a.attack_hit {
            return Some(a.kb_vel);
        }
    }
    None
}

#[test]
fn wolf_bounces_off_an_enemy_that_cuts_his_knockback() {
    // c1180 (club samurai, cut 75 %): pushed back (-Z, away from the enemy at +Z) over 0.09 s.
    let v = wolf_recoil_on("c1180").expect("the slash connects");
    assert!(v.z < -1.0 && v.x.abs() < v.z.abs() * 0.3, "recoil {v}");
    // c1020 (cut 0): the General takes it all, Wolf stays.
    let v = wolf_recoil_on("c1020").expect("the slash connects");
    assert!(v.length() < 1e-4, "recoil {v}");
}

/// Wolf's first slash on `chr` standing at `at`: (his recoil velocity, his forward, the enemy's
/// root minus his) on the frame it connects.
fn wolf_recoil_at(chr: &str, at: Vec3) -> Option<(Vec3, Vec3, Vec3)> {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.x = at.x;
            t.translation.z = at.z;
        }
    }
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        let (a, wt) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        if a.attack_hit {
            let (v, f, w) = (a.kb_vel, a.forward(), wt.translation);
            let et = world.query_filtered::<&Transform, With<Enemy>>().iter(world).next().unwrap().translation;
            return Some((v, f, et - w));
        }
    }
    None
}

/// FUN_140b6bfe0's facing factor: a push square to the attacker's facing is whole, one at an angle
/// is times cos^2, never below 0.4 (and 0.4 behind him). Wolf's recoil off c1180 (cut 75 %) is
/// taken from the scaled distance, so a slash at an angle bounces him back less.
#[test]
fn knockback_scales_with_the_attackers_facing() {
    use crate::combat::knockback_facing;
    let fwd = Vec3::NEG_Z;
    assert!((knockback_facing(Vec3::NEG_Z * 3.0, fwd) - 1.0).abs() < 1e-6);
    let a = 40f32.to_radians();
    let k = knockback_facing(Vec3::new(a.sin(), 0.0, -a.cos()), fwd);
    assert!((k - a.cos().powi(2)).abs() < 1e-5, "{k}");
    assert_eq!(knockback_facing(Vec3::X, fwd), 0.4, "side on: cos^2 0 floored");
    assert_eq!(knockback_facing(Vec3::Z, fwd), 0.4, "behind");

    let (square, f0, d0) = wolf_recoil_at("c1180", Vec3::new(0.0, 0.0, 2.0)).expect("the square slash connects");
    let (angled, f1, d1) = wolf_recoil_at("c1180", Vec3::new(-0.8, 0.0, 1.8)).expect("the angled slash connects");
    let (k0, k1) = (knockback_facing(d0, f0), knockback_facing(d1, f1));
    assert!(k0 > 0.95 && k1 < 0.9, "facing factors {k0} {k1}");
    let ratio = angled.length() / square.length();
    assert!((ratio - k1 / k0).abs() < 0.03, "recoil {square} vs {angled}: ratio {ratio}, expected {}", k1 / k0);
}

/// AtkParam hitSourceType (Paramdex enum ATK_PARAM_HIT_SOURCE: 0 weapon, 1 body - which model's
/// dummy ids the shapes name) and hti*_Priority ("the higher one wins when two touch at once")
/// are not read by combat.rs: the merged dummy map (weapon ids over body ids) and "any shape
/// connects" give the same hits on this data. Every NPC row is 0, every priority is 0, and no
/// body-source PC row names an id Wolf's sword (WP_A_0300) has; only the wire rows' 30133 is
/// also on the grapple model (WP_A_9500 133).
#[test]
fn hit_source_and_priority_change_no_hit_on_this_data() {
    let rows = |name: &str| -> Vec<serde_json::Value> {
        let t = std::fs::read_to_string(crate::paths::extracted().join(format!("json/params/{name}.json"))).unwrap();
        serde_json::from_str::<serde_json::Value>(&t).unwrap()["rows"].as_array().unwrap().clone()
    };
    // The dummy ids of an exported model (model.rs `load`: the last section, u32 count then
    // 18-byte records led by the i16 id).
    let model_ids = |file: &str| -> std::collections::HashSet<i64> {
        let d = std::fs::read(crate::paths::extracted().join(file)).unwrap();
        (1..1000usize)
            .find_map(|n| {
                let o = d.len().checked_sub(4 + 18 * n)?;
                (u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as usize == n)
                    .then(|| (0..n).map(|i| i16::from_le_bytes([d[o + 4 + 18 * i], d[o + 5 + 18 * i]]) as i64).collect())
            })
            .unwrap()
    };
    let sword = model_ids("model_c0000_wp_a_0300.bin");
    assert!(sword.contains(&100) && sword.contains(&120), "Kusabimaru's blade dummies");
    let prio = |r: &serde_json::Value| (0..16).all(|i| r[format!("hti{i}_Priority")].as_i64().unwrap_or(0) == 0);
    let npc = rows("AtkParam_Npc");
    assert!(npc.len() > 2000 && npc.iter().all(|r| r["hitSourceType"].as_i64() == Some(0) && prio(r)));
    let pc = rows("AtkParam_Pc");
    assert!(pc.iter().all(prio));
    let body: Vec<_> = pc.iter().filter(|r| r["hitSourceType"].as_i64() == Some(1)).collect();
    assert_eq!(body.len(), 54);
    for r in body {
        for i in 0..16 {
            for k in ["DmyPoly1", "DmyPoly2"] {
                let id = r[format!("hit{i}_{k}")].as_i64().unwrap_or(-1);
                if id > 0 {
                    assert!(!sword.contains(&(id % 10000)), "row {} hit{i} {id} is on the sword", r["id"]);
                }
            }
        }
    }
}

/// A knockback covers the distance it asks for (v = dist / (cont + dec / 2), held then falling
/// linearly), whatever the frame step: 1 m with KnockBackParam row 0 damage_S-like times
/// (2026-10-12 bug hunt: whole steps at the start speed went 1.18 m).
#[test]
fn a_knockback_covers_exactly_its_distance() {
    for (cont, dec) in [(0.1f32, 0.2f32), (0.033, 0.05), (0.0, 0.3), (0.25, 0.0)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        let start = {
            let world = app.world_mut();
            let (mut a, t) = world.query_filtered::<(&mut Actor, &Transform), With<Player>>().single_mut(world).unwrap();
            a.knockback(Vec3::X, 1.0, cont, dec);
            t.translation
        };
        for _ in 0..40 {
            app.update();
        }
        let world = app.world_mut();
        let t = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let moved = (t - start).with_y(0.0).length();
        assert!((moved - 1.0).abs() < 0.02, "cont {cont} dec {dec}: moved {moved}");
    }
}

/// Wolf's first attack-window frames (TAE, 30 fps) for the clip comparison in kb/damage.md
/// "Reach and first-hit vs the clips": Combo1, Ichimonji (art 5300) and Mortal Draw (5500),
/// with the art's variation judges as in play. `cargo test wolf_first_hit_frames -- --ignored
/// --nocapture` prints them.
#[test]
#[ignore]
fn wolf_first_hit_frames() {
    let mut app = app();
    app.update();
    let mut combat = app.world_mut().resource_mut::<Combat>();
    for (art, anim) in [(5000, "a050_300100"), (5300, "a102_316000"), (5500, "a104_316000"), (5500, "a104_316010")] {
        let var = combat.param("EquipParamWeapon", art)["behaviorVariationId"].as_i64();
        combat.player.art_variation = var;
        // Every StateInfo gate the anim's events name opened (Mortal Draw's emblem slash).
        let gates: Vec<i64> = combat.player.anim(anim).into_iter().flat_map(|a| a.events.iter()).filter_map(|e| e.arg_i64("StateInfo")).filter(|g| *g > 0).collect();
        combat.player.resident_gates = gates;
        let w: Vec<_> = combat
            .player
            .attack_windows(anim)
            .into_iter()
            .map(|(e, atk, j)| format!("j{j} f{:.1}-{:.1} r{:.2}", e.start * TAE_FPS, e.end * TAE_FPS, atk.hit0_radius))
            .collect();
        println!("{art} {anim}: {w:?}");
    }
}

/// MSB patrol routes (tools/patrols.py -> patrols.json, enemy.rs msb_patrol): the gate map's
/// Ochimusha c1010_0051 (entity 1110248) stands 23 m off the export origin, turned into the arena
/// like the map pieces; c1010_0046 (1110236, PatrolType 5) walks its three points in order and
/// stays at the last, the route stepping on each MoveToSomewhere(POINT_MOVE_POINT) success
/// (common_non_battle_act.lua NonBattleAct_Common). A point is reached on entering its region
/// (a 1.2 m sphere: IsInsideMovePoint, FUN_1405caa00 -> FUN_1409825b0), not on top of it.
#[test]
fn an_msb_enemy_walks_its_patrol_route() {
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
        if let Some((cur, prev)) = world.non_send_resource::<crate::enemy::Brains>().map.get(&patroller).and_then(|b| b.patrol_points()) {
            if prev >= 0 && reached.last() != Some(&prev) {
                let d = (pos - points[prev as usize]).with_y(0.0).length();
                reached.push(prev);
                assert!(d < 1.2 && d > 0.9, "stepped at point {prev} {d:.2} m away (its region's edge: 1.2 m)");
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

/// The gate's c1010 1110236 waits with its AI off (its map event, common_func 20005110) until
/// Wolf walks into "Event_巡回起動領域_地下牢前の橋" (a box 37 x 81 x 13 m turned -48°), or
/// until it is hurt; then it walks its route.
#[test]
fn an_msb_enemy_waits_for_wolf_in_its_wake_region() {
    let Some(spot) = crate::enemy::msb_patrol_on("m11_01_00_00", 1110236) else { return };
    let hold = spot.hold.clone().expect("1110236 has its map event's AI hold");
    let region = hold.region.clone().expect("its wake region");
    assert_eq!((region.shape, region.size.len()), (5, 3));
    // The box's far corner is inside, a point just past its end is not.
    let inside = region.at + region.x * 18.0 + region.z * 40.0 + Vec3::Y;
    assert!(region.contains(inside) && !region.contains(region.at + region.z * 41.0 + Vec3::Y));
    assert!(!region.contains(region.at + region.x * 2.0 - Vec3::Y), "below its base");
    for wake_by_hurt in [false, true] {
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
        let start = spot.at.with_y(0.0);
        assert!(!region.contains(Vec3::new(0.0, 0.0, 80.0)));
        for _ in 0..60 * 5 {
            app.update();
        }
        let pos = app.world().get::<Transform>(held).unwrap().translation;
        assert!((pos.with_y(0.0) - start).length() < 0.1, "held: it stays put ({pos})");
        assert!(app.world().get::<crate::enemy::AiHold>(held).is_some());
        {
            let world = app.world_mut();
            if wake_by_hurt {
                world.get_mut::<Actor>(held).unwrap().hp -= 1.0;
                world.get_mut::<Enemy>(held).unwrap().hurt_by_wolf = true;
            } else {
                let mut q = world.query_filtered::<&mut Transform, With<Player>>();
                // Inside the region on the flat sim floor (its base is 5.3 m below Wolf's start).
                let p = region.at + region.x * 12.0 + region.z * 30.0;
                q.single_mut(world).unwrap().translation = p.with_y(CAPSULE_HALF_HEIGHT);
            }
        }
        let mut moved = 0.0f32;
        for _ in 0..60 * 20 {
            app.update();
            let pos = app.world().get::<Transform>(held).unwrap().translation;
            moved = moved.max((pos.with_y(0.0) - start).length());
        }
        assert!(app.world().get::<crate::enemy::AiHold>(held).is_none(), "woken (hurt: {wake_by_hurt})");
        assert!(moved > 1.0, "it moves once woken (hurt: {wake_by_hurt}): {moved:.2} m");
    }
}

/// AI pacing for the comparison with the live recordings (kb/enemy.md "AI pacing"): the enemy
/// (`SHINOBI_PACE=<chr>[:<NpcParam row>]`, default c1010 and c1020) fights a Wolf who only holds
/// his guard (HP and posture refilled each tick) for 90 s; prints every attack start (time,
/// anim, distance) and the gaps. `cargo test ai_pacing -- --ignored --nocapture`.
#[test]
#[ignore]
fn ai_pacing() {
    let runs: Vec<(String, Option<i64>)> = match std::env::var("SHINOBI_PACE") {
        Ok(v) => v.split(',').map(|c| {
            let (chr, row) = c.split_once(':').map_or((c, None), |(c, r)| (c, r.parse().ok()));
            (chr.to_string(), row)
        }).collect(),
        Err(_) => vec![("c1010".into(), None), ("c1020".into(), None)],
    };
    for (chr, row) in runs {
        let mut app = app_cfg(&chr, row, Vec::new());
        {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
        }
        app.world_mut().resource_mut::<PadInput>().guard_held = true;
        let mut attacks: Vec<(f32, String, f32, f32)> = Vec::new(); // (start, anim, distance, end)
        let mut cur: Option<String> = None;
        let fps = 60.0;
        for frame in 0..(90 * 60) {
            app.update();
            let t = frame as f32 / fps;
            let world = app.world_mut();
            {
                let mut q = world.query_filtered::<&mut Actor, With<Player>>();
                let mut w = q.single_mut(world).unwrap();
                w.hp = w.hp_max;
                w.posture = 0.0;
            }
            let wp = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let (anim, ep) = {
                let mut q = world.query_filtered::<(&Actor, &Transform), (With<Enemy>, Without<crate::enemy::Cast>)>();
                let (a, t) = q.single(world).unwrap();
                (a.anim.clone(), t.translation)
            };
            let kind = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().kind;
            let combat = world.resource::<Combat>();
            let is_attack = !combat.kind(kind).data.attack_windows(&anim).is_empty();
            if cur.as_deref() != Some(anim.as_str()) {
                if let Some(last) = attacks.last_mut().filter(|l| l.3 < 0.0) {
                    last.3 = t;
                }
                if is_attack {
                    attacks.push((t, anim.clone(), (ep - wp).with_y(0.0).length(), -1.0));
                }
                cur = Some(anim);
            }
        }
        let n = attacks.len();
        let gaps: Vec<f32> = attacks.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let idle: Vec<f32> = attacks.windows(2).map(|w| (w[1].0 - w[0].3).max(0.0)).collect();
        let combos = idle.iter().filter(|g| **g < 0.2).count();
        let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let median = |v: &[f32]| {
            let mut v = v.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.get(v.len() / 2).copied().unwrap_or(0.0)
        };
        let mut by: std::collections::BTreeMap<String, usize> = Default::default();
        for a in &attacks {
            *by.entry(a.1.clone()).or_default() += 1;
        }
        let mut d: Vec<f32> = attacks.iter().map(|a| a.2).collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "{chr} {row:?}: {n} attacks in 90 s ({:.1}/min), start-to-start mean {:.2} / median {:.2} s, end-to-start mean {:.2} / median {:.2} s, chained {combos}/{}, distance min {:.1} median {:.1} max {:.1}",
            n as f32 / 1.5,
            mean(&gaps),
            median(&gaps),
            mean(&idle),
            median(&idle),
            idle.len(),
            d.first().copied().unwrap_or(0.0),
            d.get(d.len() / 2).copied().unwrap_or(0.0),
            d.last().copied().unwrap_or(0.0)
        );
        println!("  {by:?}");
    }
}

/// A fight reset (Wolf's death, Retry) respawns the line-up: the new enemies get their skeleton
/// and model geometry (dummy polys) like the first ones.
#[test]
fn enemies_respawned_by_a_fight_reset_get_their_bodies() {
    let mut app = app_geom("c1020", None);
    let bodies = |app: &mut App| {
        let world = app.world_mut();
        let n = world.query_filtered::<Entity, With<Enemy>>().iter(world).count();
        let skel = world.query_filtered::<Entity, (With<Enemy>, With<crate::anim::Skeleton>)>().iter(world).count();
        let dmy = world.query_filtered::<Entity, (With<Enemy>, With<crate::model::Dummies>)>().iter(world).count();
        (n, skel, dmy)
    };
    let first = bodies(&mut app);
    assert!(first.0 > 0 && first.1 == first.0 && first.2 == first.0, "{first:?}");
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    for _ in 0..5 {
        app.update();
    }
    let again = bodies(&mut app);
    assert_eq!(again, first, "(enemies, with skeleton, with dummies) after the reset");
}

/// The encounter picker (C2): SpawnEncounter swaps the fight without a relaunch - the General
/// becomes the Ochimusha (own anims, model, AI), then Genichiro brings his boss events' Tomoe
/// kind and his script; the list knows him as a boss.
#[test]
fn spawn_encounter_swaps_the_fight_in_place() {
    let list = crate::data::encounter_list();
    let genichiro = list.iter().find(|e| e.npc_row == 71001000 && e.boss).expect("Genichiro is a boss encounter");
    let mut app = app_geom("c1020", None);
    let enemy = |app: &mut App| {
        let world = app.world_mut();
        let combat = world.resource::<Combat>().kinds.iter().map(|k| k.foe.chr.clone()).collect::<Vec<_>>();
        let mut q = world.query_filtered::<(&Actor, Has<crate::anim::Skeleton>, Has<crate::model::Dummies>), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (a, skel, dmy) = q.iter(world).next().unwrap();
        (combat[a.kind].clone(), skel && dmy, combat)
    };
    assert_eq!(enemy(&mut app).0, "c1020");
    let wolf_clips = app.world().resource::<crate::anim::AnimLib>().player.clips.len();
    app.world_mut().write_message(crate::enemy::SpawnEncounter { chr: "c1010".into(), ..Default::default() });
    for _ in 0..5 {
        app.update();
    }
    let (chr, body, kinds) = enemy(&mut app);
    assert_eq!((chr.as_str(), body), ("c1010", true), "kinds {kinds:?}");
    app.world_mut().write_message(crate::enemy::SpawnEncounter { chr: "c7100".into(), npc_row: Some(genichiro.npc_row), ..Default::default() });
    for _ in 0..5 {
        app.update();
    }
    let (chr, body, kinds) = enemy(&mut app);
    assert_eq!((chr.as_str(), body), ("c7100", true));
    assert!(kinds.iter().any(|k| k == "c7110"), "Tomoe's kind from his events: {kinds:?}");
    // Wolf's clips against him (anim_c0000_c7100.bin: his deathblows) came in.
    let lib = app.world().resource::<crate::anim::AnimLib>();
    let wolf_vs = crate::paths::root().join("extracted/anim_c0000_c7100.bin");
    assert!(!wolf_vs.exists() || lib.player.clips.len() > wolf_clips, "Wolf's clips against him");
    assert!(app.world().resource::<crate::enemy::BossScript>().run.is_some(), "his boss script runs");
}

#[test]
#[ignore]
fn geometry_probe() {
    let mut app = app_geom("c1020", None);
    let world = app.world_mut();
    let n_dmy: Vec<usize> = world.query::<&crate::model::Dummies>().iter(world).map(|d| d.0.len()).collect();
    let n_hurt: Vec<usize> = world.query::<&crate::anim::Hurtboxes>().iter(world).map(|h| h.0.len()).collect();
    println!("dummies {n_dmy:?} hurtboxes {n_hurt:?}");
    for d in [1.0, 2.4, 3.6, 4.0, 4.4, 4.8, 5.2, 6.0] {
        println!("dist {d}: first hit frame {:?}", geom_first_hit_frame(d));
    }
}

/// An enemy's attack `key` at Wolf (idle) `dist` m away with the real geometry: the TAE frame
/// of the first contact.
fn geom_enemy_hit_frame(chr: &str, key: &str, dist: f32) -> Option<f32> {
    let mut app = app_geom(chr, None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Transform, &mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
        for (mut t, mut e, mut a) in q.iter_mut(world) {
            t.translation.z = dist;
            e.force_anim(&mut a, key);
        }
    }
    let mut last = 0.0;
    for _ in 0..240 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
        if a.attack_hit {
            return Some(last);
        }
        if a.anim != key {
            return None;
        }
        last = a.t * TAE_FPS;
    }
    None
}

/// B1 table (docs/kb/damage.md): the first attack of six common enemies with the real
/// geometry: its TAE hit window, the frame it first touches Wolf at 1.5 m and its reach.
#[test]
#[ignore]
fn enemy_reach_table() {
    for chr in std::env::var("SHINOBI_ENEMIES").unwrap_or("c1020,c1040,c1010,c1150,c7100,c5000".into()).split(',') {
        let app = app_geom(chr, None);
        let combat = app.world().resource::<Combat>();
        let d = combat.enemy0();
        let mut keys: Vec<&String> = d.anims.keys().filter(|k| k.contains("_0030")).collect();
        keys.sort();
        let Some((key, start)) = keys.iter().find_map(|k| {
            d.attack_windows(k).iter().filter(|(_, a, _)| a.atk_phys > 0.0).map(|(e, _, _)| e.start).reduce(f32::min).map(|s| ((*k).clone(), s))
        }) else {
            println!("{chr}: no attack");
            continue;
        };
        drop(app);
        let close = geom_enemy_hit_frame(chr, &key, 1.5);
        let mut reach = None;
        let mut dist = 1.0;
        while dist <= 9.0 {
            if geom_enemy_hit_frame(chr, &key, dist).is_some() {
                reach = Some(dist);
            } else if reach.is_some() {
                break;
            }
            dist += 0.25;
        }
        println!("{chr} {key}: window from f{:.1}, contact at 1.5 m f{close:?}, reach {reach:?} m", start * TAE_FPS);
    }
}

/// Wolf's first slash (a050_300100, AttackBehavior from TAE frame 7) with the game's geometry:
/// it touches the General on its first hit frame up close, still reaches him at 4.8 m with its
/// step-in and misses at 6 m.
#[test]
fn wolfs_first_slash_hits_on_its_tae_frame_with_the_game_geometry() {
    let close = geom_first_hit_frame(2.4).expect("hits at 2.4 m");
    assert_eq!(close.0, "a050_300100");
    assert!((6.5..=7.5).contains(&close.1), "first contact at TAE frame {}", close.1);
    assert!(geom_first_hit_frame(4.8).is_some(), "reaches 4.8 m");
    assert!(geom_first_hit_frame(6.0).is_none(), "misses at 6 m");
}

/// The General's first attack (a000_003000, window from frame 18) touches Wolf 1-2 frames into
/// it at 1.5 m (the sword still has to come down) and misses at 5 m.
#[test]
fn the_generals_slash_touches_wolf_inside_its_window() {
    let f = geom_enemy_hit_frame("c1020", "a000_003000", 1.5).expect("hits at 1.5 m");
    assert!((18.0..=20.5).contains(&f), "first contact at TAE frame {f}");
    assert!(geom_enemy_hit_frame("c1020", "a000_003000", 5.0).is_none());
}

/// A one-dummy attack is a sphere (AtkParam DmyPoly2 -1, Paramdex SDT "-1だと球になる"): the
/// dog's leaping bite (c1150 AtkParam 100: dummy 10, radius 0.4) bites at 1.5 m and misses at
/// 5 m (it used to fall back to the reach approximation and reach 8 m).
#[test]
fn a_one_dummy_attack_is_a_sphere() {
    assert!(geom_enemy_hit_frame("c1150", "a000_003000", 1.5).is_some());
    assert!(geom_enemy_hit_frame("c1150", "a000_003000", 5.0).is_none());
}

/// A boss with deathblows left gets its vitality back from its ThrowDef reaction's own TAE
/// (AddSpEffect 300610 "HP reset", changeHpRate -100): not at the deathblow, at that frame.
#[test]
fn a_boss_refills_at_its_reactions_hp_reset_frame() {
    let mut app = app_with("c7100");
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        a.hp = a.hp_max * 0.3;
    }
    break_and_deathblow(&mut app, 1);
    let mut low_in_throw_def = false;
    for _ in 0..600 {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        if a.state.starts_with("ThrowDef") && a.hp < a.hp_max {
            low_in_throw_def = true;
        }
        if a.hp >= a.hp_max {
            let combat = world.resource::<Combat>();
            let ev = combat.data_of(a).anim(&a.anim).and_then(|an| an.events.iter().find(|e| e.kind == 67 && e.arg_i64("SpEffectID") == Some(300610)).map(|e| e.start));
            let ev = ev.unwrap_or_else(|| panic!("refilled in {} without 300610", a.anim));
            assert!(low_in_throw_def, "refilled before its reaction");
            assert!((a.t - ev).abs() <= 1.0 / 60.0 + 1e-3, "refilled at {:.3} s, 300610 at {ev:.3} s ({})", a.t, a.anim);
            return;
        }
    }
    panic!("never refilled");
}


/// The picker's names are the game's own (the health bars' NPC名 text, tools/npc_names.py); a
/// name two rows share gets the place.
#[test]
fn encounters_carry_the_health_bar_names() {
    if !crate::paths::root().join("extracted/enemies/npc_names.json").exists() {
        return;
    }
    let list = crate::data::encounter_list();
    let name = |row: i64| list.iter().find(|e| e.npc_row == row).map(|e| e.name.clone()).unwrap_or_default();
    assert_eq!(name(71001000), "Genichiro Ashina (Ashina Castle)");
    assert!(name(51000000).starts_with("Guardian Ape"), "{}", name(51000000));
    assert_ne!(name(51000000), name(51000100));
    assert!(name(10201000).starts_with("Leader Shigenori Yamauchi"), "{}", name(10201000));
    for e in list.iter().filter(|e| e.bar) {
        println!("{} {} {}{}", e.chr, e.npc_row, e.name, if e.boss { " [boss]" } else { "" });
    }
}

/// An enemy under a "hearing 0" SpEffect (220601 General purpose: sight 0 hearing 0, on c1020's
/// TAE) hears nothing: the listener's hearing rate (exe FUN_140bfe590) is 0.
#[test]
fn a_deafened_enemy_hears_nothing() {
    let mut app = app_with("c1020");
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<&mut Actor, With<Enemy>>();
        let mut a = q.iter_mut(world).next().unwrap();
        a.resident.clear();
        assert_eq!(crate::enemy::hearing_rate(&combat, &a), 1.0);
        a.resident.push(220601);
        assert_eq!(crate::enemy::hearing_rate(&combat, &a), 0.0);
    });
}


/// The bandit's oil pot (Bullet 15500700, AtkParam decalId1 / 2 50010 / 50011) leaves both its
/// stains where it lands.
#[test]
fn the_oil_pot_leaves_its_stains_where_it_lands() {
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("15500700").expect("oil pot row").clone();
    let at = Vec3::new(6.0, 1.5, 6.0);
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, at, Vec3::NEG_Y, 0.0, None);
    world.flush();
    let mut cursor = world.resource::<Messages<crate::gore::PlaceStain>>().get_cursor();
    let mut placed = Vec::new();
    for _ in 0..(60 * 3) {
        app.update();
        let m = app.world().resource::<Messages<crate::gore::PlaceStain>>();
        placed.extend(cursor.read(m).map(|p| (p.id, p.at)));
    }
    let ids: Vec<i64> = placed.iter().map(|p| p.0).collect();
    assert_eq!(ids, vec![50010, 50011], "{placed:?}");
    assert!(placed.iter().all(|p| p.1.xz().distance(at.xz()) < 0.5 && p.1.y < 0.1), "{placed:?}");
}

/// The Divine Dragon's warning (Bullet 52000980, FollowType 3 "follow target", hitRadius 10,
/// AtkParam 8 dummy): fired 40 m away it rides on Wolf and puts SpEffect 211000 ("dangerous enemy
/// attack VFX") on him, with no damage.
#[test]
fn the_divine_dragons_warning_follows_wolf() {
    let mut app = app_cfg("c5200", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let shooter = world.query_filtered::<Entity, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap();
    let spec = world.resource::<Combat>().kind(0).data.bullet_rows.get("52000980").expect("warning row").clone();
    assert_eq!(spec.follow_type, 3);
    let wolf_at = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    crate::enemy_bullet::spawn_row(&mut world.commands(), &spec, shooter, 0, wolf_at + Vec3::new(40.0, 10.0, 0.0), Vec3::NEG_Z, 0.0, None);
    world.flush();
    let mut seen = false;
    for _ in 0..3 {
        app.update();
        let world = app.world_mut();
        seen |= world.query::<&Player>().single(world).unwrap().timed.iter().any(|t| t.0 == 211000);
    }
    assert!(seen, "no 211000 on Wolf");
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp, hp);
}

/// The Chained Ogre's grab a100_003005 opens the grapple (TAE 66 SpEffect 3502020
/// chrWireVariationNo 50200, hook dummy 650, landing 260) only in its window 0.63-1.07 s
/// (kb/grapple.md section 4); the hook sits on his choker, high above his root.
#[test]
fn the_ogres_grab_opens_the_grapple_in_its_window() {
    let mut app = app_geom("c5020", None);
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
        let mut a = q.single_mut(world).unwrap();
        a.play("Ez3005", "a100_003005");
        for (t, open) in [(0.3, false), (0.8, true), (1.2, false)] {
            a.t = t;
            a.prev_t = t;
            let got = crate::enemy::grapple::wire_effect(&combat, &a);
            assert_eq!(got.is_some(), open, "t {t}: {got:?}");
        }
        a.t = 0.8;
        a.prev_t = 0.79;
    });
    app.update();
    let world = app.world_mut();
    let root = world.query_filtered::<&Transform, (With<Enemy>, Without<crate::enemy::Cast>)>().single(world).unwrap().translation;
    let g = world.resource::<crate::enemy::grapple::EnemyGrapples>().0.clone();
    assert_eq!(g.len(), 1, "{g:?}");
    assert_eq!((g[0].variation, g[0].homing_row, g[0].sp_effect), (50200, 300050200, 3502020));
    // The body dummy, high on him (bent into the grab at 0.8 s: 2.3 m off the floor), not the
    // no-model fallback two metres over the root.
    assert!(g[0].hook.y - root.y > 1.0 && g[0].hook.distance(root + Vec3::Y * 2.0) > 1e-3, "hook {:?} root {root:?}", g[0].hook);
    // The Grappling Hook Attack on him: ThrowParam 15020170 (Wolf a213_511600, his 13600) and
    // 15020171 (511610 / 13610).
    let combat = app.world().resource::<Combat>();
    let foe = &combat.kind(0).foe;
    let rows: Vec<(i64, String, i64)> = combat.wire_throws(foe).into_iter().map(|(id, t)| (id, t.atk_anim, t.def_anim)).collect();
    assert_eq!(rows, vec![(15020170, "a213_511600".to_string(), 13600), (15020171, "a213_511610".to_string(), 13610)]);
}

/// Wolf's hook in the Chained Ogre mid-grab (a100_003005 holds 510010, ENABLE_WIRE_DAMAGE0):
/// c9997 HKS SP_DAMAGE_WIRE -> DamageWire0 (a000_008910). Out of the window it does nothing.
#[test]
fn the_hook_in_the_ogres_grab_staggers_him() {
    let mut app = app_with("c5020");
    let set = |app: &mut App, t: f32| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(Entity, &mut Actor), (With<Enemy>, Without<crate::enemy::Cast>)>();
        let (ent, mut a) = q.single_mut(world).unwrap();
        a.play("Ez3005", "a100_003005");
        a.t = t;
        a.prev_t = t - 0.01;
        ent
    };
    let ent = set(&mut app, 0.3);
    app.world_mut().write_message(crate::enemy::grapple::WireHit(ent));
    app.update();
    assert_ne!(enemy_state(&mut app).0, "DamageWire0", "outside the window");
    let ent = set(&mut app, 0.8);
    app.world_mut().write_message(crate::enemy::grapple::WireHit(ent));
    app.update();
    assert_eq!(enemy_state(&mut app).0, "DamageWire0");
}

/// The Ochimusha's ledge-hang and wall-hug deathblows (ThrowKind 110000 / 110010 hanging,
/// 90000 / 90010 left wall, 100000 / 100010 right): start rows without a defender anim, body rows
/// with ThrowDef12800 / 13000 / 12600 / 12700, all in his anim set.
#[test]
fn the_ochimusha_has_his_hang_and_hug_deathblows() {
    let app = app_with("c1010");
    let combat = app.world().resource::<Combat>();
    let foe = &combat.kind(0).foe;
    let body = |kinds: &[i64]| -> Vec<(i64, i64)> { combat.throws_of_kinds(foe, kinds).into_iter().map(|(id, t)| (id, t.def_anim)).collect() };
    assert_eq!(body(&[110_010]), vec![(11010081, 12800), (11010131, 13000)]);
    assert_eq!(body(&[90_010, 100_010]), vec![(11010061, 12600), (11010071, 12700)]);
    let d = &combat.kind(0).data;
    for def in [12600, 12700, 12800, 13000] {
        assert!(d.anim_key(&format!("ThrowDef{def}")).is_some(), "ThrowDef{def}");
    }
}

/// The Owl's phase-2 heat-up (a000_020005, TAE marker 3506080) offers no deathblow even with
/// his posture broken (sekiro-rs b997933).
#[test]
fn the_owl_has_no_deathblow_during_his_heat_up() {
    let mut app = app_with("c5060");
    let world = app.world_mut();
    let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    world.resource_scope(|world, combat: Mut<Combat>| {
        let mut q = world.query_filtered::<(&Enemy, &mut Actor, &Transform), Without<crate::enemy::Cast>>();
        let (e, mut a, tf) = q.single_mut(world).unwrap();
        let at = tf.translation;
        a.play("Ez20005", "a000_020005");
        a.t = 2.0;
        assert!(crate::player::deathblow_check(&combat, wolf, &a, e, at, &|_| None).is_none());
    });
}

/// The bandit's torch (SpEffect 9115, registBlood 62) builds up on Wolf against his resist 200
/// (status::WOLF_RESIST): burning on the 4th clean hit, the 5th through his guard (sword
/// bloodGuardResist 33 %), the 7th through deflects (bloodJustGuardResist 50 %); then he burns.
#[test]
fn the_bandits_torch_sets_wolf_on_fire() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1550", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    let hp = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp;
    world.resource_scope(|world, combat: Mut<Combat>| {
        assert!(Statuses::is_build_up(&combat, 9115), "9115 not found as a build-up");
        for (guard, hits) in [(Guard::None, 4), (Guard::Block, 5), (Guard::Deflect, 7)] {
            let mut s = Statuses::default();
            let n = (1..=10).find(|_| s.hit_wolf(&combat, wolf, weapon, &[9115], guard).iter().any(|l| l == "BURNING"));
            assert_eq!(n, Some(hits), "{guard:?}");
        }
        let mut st = world.resource_mut::<Statuses>();
        for _ in 0..4 {
            st.hit_wolf(&combat, wolf, weapon, &[9115], Guard::None);
        }
    });
    for _ in 0..(60 * 3) {
        app.update();
    }
    let after = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().hp;
    assert!(after < hp, "no burn damage: {hp} -> {after}");
}

/// A burning Wolf set on fire again (burns have disableBlood 0) keeps one burn, its time started
/// over - not a second affliction whose flames (midst fx) would never be stopped.
#[test]
fn reigniting_a_burn_refreshes_it() {
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
fn the_head_kick_relaunch_goes_straight_up() {
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
fn chain_tick(from: &str, then: Action) -> (Option<usize>, Option<usize>, String) {
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
fn deflects_arts_and_tools_chain_on_the_cancel_frame() {
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
fn a_grapple_lands_on_the_point_in_the_move_start_time() {
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
}

/// The wire flight's fall type (Wire::launch fallType 2: Wire gravity and air control) ends with
/// the landing: walking off the perch afterwards is a plain fall (2026-10-12 bug hunt round 12).
#[test]
fn a_wire_landing_ends_the_wire_fall_type() {
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
fn the_grappling_hook_attack_throws_the_ogre() {
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

/// Gyoubu on his last bar: the Grappling Hook Attack's landing deathblow kills (ThrowDef13610 ->
/// ThrowDefDeath13611 on flag 69) and an attack pressed in the landing takes the Todome
/// (ThrowParam 15080179: Wolf a225_511690, Gyoubu ThrowDef13690).
#[test]
fn the_grappling_hook_attack_todome_on_gyoubus_last_bar() {
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
fn a_wire_landing_with_the_stick_held_runs_on() {
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

/// Probe (sekiro-rs 8eaf879's bug): strafing right locked on, the lock drops with the stick
/// still held: within 0.5 s Wolf must run where the stick points (camera-relative), not keep
/// the strafe.
#[test]
fn held_stick_realigns_after_the_lock_drops() {
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
fn a_bullet_past_its_range_weakens() {
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

/// Isshin, the Sword Saint (c5400): a deflect that empties his posture plays the break pose
/// (ThrowParam 15400010: Wolf a242_510100 / ThrowDef12100) and attack takes it to the deathblow,
/// also as Isshin after the first deathblow and his a000_003015 change (the user's report:
/// "the close look for the second but M1 does nothing").
#[test]
fn isshins_break_pose_leads_to_the_deathblow_after_his_change() {
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
fn the_armoured_servant_takes_a_third_and_the_loaded_spear_breaks_it() {
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
fn shuriken_on_c1180(row: i64) -> (f32, f32) {
    shuriken_on("c1180", row)
}

/// HP and posture a shuriken takes off the `chr` `row` turned away from Wolf (no guard).
fn shuriken_on(chr: &str, row: i64) -> (f32, f32) {
    let mut app = app_cfg(chr, Some(row), Vec::new());
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

/// Wolf's bullets land on the armour part too (2026-10-12 bug hunt): the shuriken (atkAttribute /
/// staminaPhysicsAttribute 9 Light Shoot) does x0.35 HP (400250 defLightShootDmgRate 0.5 x the
/// part's body compensation 0.7) and x0.5 posture (defLightShootStaminaDmgRate 0.5) on the
/// armoured servant against his plain twin 11807100; no bullet breaks the armour.
#[test]
fn a_shuriken_on_the_armoured_servant_is_cut_like_a_slash() {
    let (hp0, p0) = shuriken_on_c1180(11807100);
    let (hp1, p1) = shuriken_on_c1180(11807101);
    assert!(hp0 > 0.0 && p0 > 0.0, "{hp0} {p0}");
    assert!((hp1 / hp0 - 0.35).abs() < 0.03 && (p1 / p0 - 0.5).abs() < 0.05, "plain {hp0} / {p0}, armoured {hp1} / {p1}");
}

/// A fight reset (R, the menu, or the one after Wolf's death) ends his poison: the Statuses
/// entry of his entity, which lives on across resets, is cleared (2026-10-12 bug hunt).
#[test]
fn a_fight_reset_clears_wolfs_poison() {
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
fn a_fight_reset_drops_the_lightning_charge() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().electro = Some((9490, 30.0));
    }
    app.world_mut().resource_mut::<PadInput>().reset = true;
    for _ in 0..3 {
        app.update();
    }
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!(a.electro.is_none(), "Wolf kept the lightning charge through the reset: {:?}", a.electro);
}

/// Death and resurrection refresh Wolf as the game's anims say: the death loop (a000_110010,
/// TAE 401 8002-8009) cures his ailments and empties the build-up gauges, the revival
/// (a000_110030, 8001 changeStaminaRate 100) restores his posture (2026-10-12 bug hunt round 15:
/// he came back poisoned with his posture as it was).
#[test]
fn death_and_resurrection_refresh_wolf() {
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
        if player_actor(&mut app).0 == "GroundDeathLoop_F" {
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

/// One swing hits every enemy it reaches: AttackBehavior windows are done per defender
/// (Actor.hits_done), so two soldiers side by side both take the slash (2026-10-12 bug hunt).
#[test]
fn one_slash_hits_two_enemies_side_by_side() {
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
fn a_doped_enemy_respawns_with_its_full_bar() {
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
fn a_respawned_enemy_starts_with_a_fresh_brain() {
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
fn a_dropped_guard_tap_does_not_turn_the_next_attack_into_the_art() {
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
fn a_reset_mid_jump_lands_wolf_at_the_start() {
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
fn a_respawned_enemy_gets_its_weapons_and_armour_back() {
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
fn a_draw_mask_change_is_kept_when_a_frame_runs_two_ticks() {
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
struct DummyProbe {
    fixed: std::collections::HashMap<Entity, Vec3>,
    worst: f32,
}

fn probe_dummies_fixed(mut probe: ResMut<DummyProbe>, wolf: Query<(&Transform, &crate::model::Dummies), With<Player>>, g: Query<&GlobalTransform>) {
    probe.fixed = wolf
        .iter()
        .flat_map(|(tf, d)| d.0.values().map(move |&m| (tf.translation, m)))
        .filter_map(|(at, m)| Some((m, g.get(m).ok()?.translation())).filter(|(_, p)| p.distance(at) < 3.0))
        .collect();
}

fn probe_dummies_drawn(mut probe: ResMut<DummyProbe>, g: Query<&GlobalTransform>) {
    let probe = &mut *probe;
    for (m, p) in &probe.fixed {
        if let Ok(now) = g.get(*m) {
            probe.worst = probe.worst.max(now.translation().distance(*p));
        }
    }
}

/// Wolf's dummies after `ticks` updates: how far the hit test's view (FixedUpdate) ever was from
/// the drawn one of the same tick.
fn dummy_lag(app: &mut App, ticks: usize) -> f32 {
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
fn the_hit_test_sees_this_ticks_pose() {
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
fn the_hit_test_sees_weapon_parts_where_tae_715_puts_them() {
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
fn mortal_draw_reaches_an_enemy_four_metres_away() {
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
fn a_respawned_enemy_is_not_still_poisoned() {
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

/// A jump in the deflect break pose (ThrowParam 11020012 弾き1B_ジャンプ: a201_510120 / ThrowDef12120)
/// vaults round behind the General and leaves him open to the deathblow; it killed him outright
/// as a deathblow before (2026-10-12 bug hunt round 7).
#[test]
fn a_jump_in_the_break_pose_vaults_without_killing() {
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
fn a_grab_pulls_wolf_onto_the_grabbers_dummy() {
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
fn a_grab_pair_is_not_pushed_apart() {
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
fn an_air_guard_break_holds_wolf_until_its_guard_flag() {
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
fn an_early_jump_in_an_air_slash_becomes_the_air_kick() {
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
fn an_enemy_turns_in_place_with_its_turn_clip() {
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
fn a_group_enemy_with_a_foreign_row_still_spawns() {
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
fn the_line_up_faces_wolf() {
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

/// A reset clears the perilous kanji (2026-10-12 bug hunt round 10: it stayed up to 0.8 s over
/// the new line-up).
#[test]
fn a_reset_clears_the_perilous_kanji() {
    let mut app = app_cfg("c1010", None, vec![]);
    app.update();
    if app.world().get_resource::<crate::hud::Perilous>().is_none() {
        app.init_resource::<crate::hud::Perilous>();
    }
    app.world_mut().resource_mut::<crate::hud::Perilous>().raise(982);
    app.world_mut().resource_mut::<crate::enemy::FightReset>().0 = true;
    app.update();
    assert!(app.world().resource::<crate::hud::Perilous>().timer <= 0.0);
}

/// launchConditionType picks which ends launch HitBulletID (2026-10-12 bug hunt round 10: every
/// end did, so the Genpei arrow 13100520's lct 1 "hit water" child came on every hit).
#[test]
fn a_bullets_child_follows_its_launch_condition() {
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
fn a_dodged_bullet_passes_through_wolf() {
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
fn the_guard_goal_holds_the_guard_stance() {
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
fn a_penetrating_bullet_launches_by_its_hit_record() {
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
fn a_camera_shake_starts_once_per_tae_event() {
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
fn the_deathblow_mark_shows_on_a_break_pose() {
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
fn an_art_fires_its_own_bullets_not_the_prosthetics() {
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
fn the_general_stance_runs_like_the_guard() {
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
fn a_respawn_forgets_the_route_end_home() {
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
fn an_instant_recovery_on_a_frame_edge_counts_once() {
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
fn a_reset_takes_wolfs_bullets_in_flight() {
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
fn the_combat_arts_bullets_are_exported() {
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
fn kill_and_respawn(app: &mut App, while_dead: impl Fn(&mut Enemy, &mut Actor)) {
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
fn a_respawn_comes_back_visible() {
    let mut app = app();
    for _ in 0..30 {
        app.update();
    }
    kill_and_respawn(&mut app, |_, a| a.opacity = 0.0);
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
    assert!(a.opacity > 0.99, "opacity {}", a.opacity);
}

/// A respawn faces its spawn way, and its home pose with it, not the way it died (a deathblow
/// from behind turned it 180 degrees) (2026-10-12 bug hunt round 13).
#[test]
fn a_respawn_faces_its_start() {
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
fn a_patrollers_respawn_is_unaware_and_held_again() {
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
fn a_rival_target_changes_state_once() {
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
fn a_rivals_blow_does_not_turn_it_on_wolf() {
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
fn a_new_lock_takes_an_enemy_in_front_not_the_nearest_behind() {
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
fn the_switch_pitch_is_from_the_camera_front() {
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
fn covert_a_cuts_sight_only_crouched() {
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
fn hold_arts_release_only_in_their_windows() {
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
fn a_shuriken_takes_the_light_shoot_rates() {
    let (hp0, p0) = shuriken_on("c1180", 11807100);
    let (hp1, p1) = shuriken_on("c1260", 12600000);
    assert!(hp0 > 0.0 && p0 > 0.0, "{hp0} {p0}");
    assert!((hp1 / hp0 - 2.0).abs() < 0.05 && (p1 / p0 - 8.0).abs() < 0.2, "c1180 {hp0} / {p0}, c1260 {hp1} / {p1}");
}

/// Every StaminaControlParam row an exported NpcParam names is exported (2026-10-12 bug hunt round
/// 14: only 0 / 1000000 / 1000100 / 1000200 were, so c1180's 1070000 regen ratios read as 100 %).
#[test]
fn the_enemies_posture_regen_rows_are_exported() {
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
fn a_patroller_plays_its_route_end_anim_once() {
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
fn msb_region_shapes_follow_the_shape_type() {
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
fn super_armour_swaps_the_damage_level_in_its_window() {
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

/// A deflected or blocked bullet is repelled itself: its distant shooter takes no posture,
/// recoil or bound reaction, even with an attacker-side posture base and reaction on the row
/// (sekiro-rs a8351b9 fight.rs:644, docs/PROJECTILE-GUARD.md; inferred from the rows).
#[test]
fn a_guarded_bullet_leaves_its_shooter_alone() {
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
    }
}

/// c9997 IsDamagedAerial: an enemy hit in the air plays its aerial reactions (c1060 has 8140 /
/// 8141 / 8150): middle / large / push / fling -> DamageAerialFront / Back, blow / upper / ex
/// blast -> DamageAerialBlow; small blow keeps DamageBlow (ExecDamageBlow comes first); a small
/// hit, or a chr with no aerial anims (c1010), has none (the additive flinch). On the ground the
/// usual table (sekiro-rs c0f97ee character.rs:1045-1074).
#[test]
fn an_enemy_hit_in_the_air_takes_the_aerial_reactions() {
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
fn a_tool_hit_in_the_air_takes_the_aerial_reaction() {
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

/// c9997 ExecGuardBlock picks the guard clip by refs, not by the deflect itself: c1180's shield
/// 400230 (stateInfo 158, no ref) deflects but plays GuardDamageSmall / Large (it has no
/// JustGuardDamage clip; the old pick played nothing); ref 1000090 (200220) plays
/// JustGuardDamage, 1000100 (200230) GuardDamageSpecial (c1010 a000_008502 on Common_Parry
/// a000_003102), 1000105 the additive flinch; a missing clip falls to the next one.
#[test]
fn an_enemys_guard_clip_follows_exec_guard_block() {
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
fn a_broken_wolf_is_knocked_down_only_by_the_lighter_levels() {
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

/// INTERUPT_UseItem fires for a crouched drink too (CrouchItemGourdDrink).
#[test]
fn a_crouched_drink_raises_use_item() {
    let mut a = Actor::new(crate::actor::Side::Player, 100.0, 100.0, 0.0, 0);
    a.prev_t = 0.0;
    a.t = 0.016;
    for (state, want) in [("ItemGourdDrink", true), ("CrouchItemGourdDrink", true), ("CrouchIdle", false)] {
        a.state = state.into();
        assert_eq!(crate::enemy::wolf_starts_an_item(&a), want, "{state}");
    }
}

/// c9997 ExecGuardBlock's EXLARGE (a blow / ex blast on a weak guard: guard_damage_table row 4
/// guardLevel 1, row 10 guardLevels 1-3) has no guard clip: ExecDamageLargeBlow plays
/// DamageLargeBlow, in the air DamageAerialBlow (c1060 has both). It played GuardDamageLarge.
#[test]
fn a_blow_on_a_weak_guard_launches_the_enemy() {
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
fn the_general_stance_attacks_start_the_stance_first() {
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
fn a_patrol_point_with_a_zero_region_is_reached_by_is_arrived() {
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
        if let Some((cur, prev)) = world.non_send_resource::<crate::enemy::Brains>().map.get(&patroller).and_then(|b| b.patrol_points()) {
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

/// A map-held enemy wakes on Wolf's hit only when it takes HP (EMEVD 4[1] / 3[23] read a damage
/// record written for final HP damage > 0, FUN_1406d8950 from FUN_140b6e150 140b6.c:8033): one
/// already cut (a rival's blow) that blocks Wolf's swing, its HP whole, stays held; it woke
/// before (Wolf's guard reaction + HP below max).
#[test]
fn a_blocked_hit_does_not_wake_a_held_enemy() {
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
fn the_finger_whistle_wakes_a_held_enemy() {
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
fn a_hit_under_flag_24_keeps_wolfs_grapple() {
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
fn a_cut_with_no_reaction_still_wakes_a_held_enemy() {
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
fn a_lineup_that_places_nobody_is_not_retried() {
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
fn a_tool_hit_under_flag_24_has_no_knockback_and_wakes_the_hold() {
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

fn wolf_skeleton(app: &mut App) -> (Entity, f32, Vec3, bool) {
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
fn a_reset_snaps_wolfs_model_onto_the_start() {
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
fn the_model_slide_ends_at_rest_when_drawn_between_ticks() {
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
fn a_second_burn_row_replaces_the_first() {
    use crate::status::{Guard, Statuses};
    let mut app = app_cfg("c1700", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let wolf = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    let weapon = world.resource::<crate::config::GameConfig>().player.combat_art;
    world.resource_scope(|_, combat: Mut<Combat>| {
        let mut s = Statuses::default();
        let burning = |s: &mut Statuses, id: i64| (1..=10).find(|_| s.hit_wolf(&combat, wolf, weapon, &[id], Guard::None).iter().any(|l| l == "BURNING"));
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
fn a_respawn_snaps_to_its_new_pose() {
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
fn the_camera_snap_focus_is_measured_from_the_feet() {
    let at = Vec3::new(1.0, crate::player::CAPSULE_HALF_HEIGHT, 2.0); // standing on y = 0
    let f = crate::camera::chr_focus(at, 1.5);
    assert!((f - Vec3::new(1.0, 1.5, 2.0)).length() < 1e-6, "{f}");
}

/// The firecracker's burst SpEffects (230110 / 230111) land on the enemy it hits: they wake a
/// map-held enemy, and the burst does no HP damage (round 19).
#[test]
fn a_firecracker_burst_puts_its_sp_effect_on_the_enemy() {
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
fn the_middle_mouse_button_releases_the_lock() {
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
fn a_released_locked_on_back_move_does_not_slide_toward_the_target() {
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

/// Lands Wolf on a ridden grapple point from where he stands; returns the states and where he
/// stands after `ticks`.
fn grapple_to(app: &mut App, hook: Vec3, land: Vec3, ticks: usize) -> (Vec<String>, Vec3) {
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
fn a_grapple_from_a_perch_flies_to_the_next_point() {
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
fn a_grapple_to_a_high_point_lands_on_it() {
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
fn a_fans_pellets_hit_wolf_once() {
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
            left = Some(world.query::<&crate::enemy_bullet::EnemyBullet>().iter(world).count());
            break;
        }
    }
    let left = left.expect("no pellet hit Wolf");
    assert_eq!(left, 4, "pellets still flying after the first hit");
}
