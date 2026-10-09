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
    app_with("c1020")
}

/// The sim with a given enemy (config.toml's enemy.chr is overridden so tests are stable).
fn app_with(chr: &str) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .add_plugins(ConfigPlugin);
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.enemy.chr = chr.to_string();
        config.enemy.npc_row = None;
    }
    app.add_plugins((DataPlugin, ActorPlugin, CombatPlugin, PlayerPlugin, EnemyPlugin, crate::prosthetic::ProstheticPlugin));
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
    let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Enemy>>();
    for (mut a, mut t) in q.iter_mut(world) {
        a.yaw = 0.0;
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 2.4);
    }
    app
}

fn enemy_state(app: &mut App) -> (String, String, f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Enemy>>();
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
        .enemy
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
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/ai_src")).exists() {
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
    let mut app = app();
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
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
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
fn deathblow_pair_moves_by_root_motion_like_the_live_game() {
    // After the absorb (enemy 1.50 m in front, facing Wolf) only the anims' root motion moves the
    // pair. Live (rec_20261008_054259, General, a201_510000 / a000_012000): 1.48 m on the first
    // frame, 0.85 m 25 frames in, 2.35 m 61 frames in.
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
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
            let foe = world.query_filtered::<&Transform, With<Enemy>>().single(world).unwrap().translation;
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
        if state == "AirKickEnemyJumpStart_F" {
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
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/ai_src")).exists() {
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
        if out.contains("shuriken hit") || out.contains("blocks the shuriken") || out.contains("deflects the shuriken") {
            break;
        }
    }
    assert!(out.contains("shuriken ("), "{out}");
    assert!(out.contains("shuriken hit") || out.contains("blocks the shuriken") || out.contains("deflects the shuriken"), "{out}");
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
    assert_eq!(app.world().resource::<Combat>().foe.chr, "c1010");
    {
        let world = app.world_mut();
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
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
    let desc = world.query::<&Enemy>().single(world).unwrap().ai_desc.clone();
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
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
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
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
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
        let d = world.query::<&Enemy>().single(world).unwrap().ai_desc.clone();
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
        for atk in combat.enemy.attacks.values_mut() {
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
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, true, 4.0);
            t.translation.z = 1.8;
            // Facing away from Wolf (who stands at the origin): forward = +Z.
            a.yaw = std::f32::consts::PI;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..5 {
        app.update();
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
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
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
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
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
        let pts: Vec<String> = [0.0f32, 25.0, 61.0, 100.0].iter().map(|f| format!("{:?}", combat.enemy.root_at(k, f / 60.0).map(|(p, y)| (p.x, p.y, y)))).collect();
        println!("{k} len {:.2}: {}", combat.enemy.length(k), pts.join(" "));
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
    let on_general = crate::sound::hit_sounds(combat, &sword, crate::sound::defender_materials(combat, crate::actor::Side::Enemy));
    if combat.foe.chr == "c1020" {
        assert_eq!(on_general, ["z100000103", "z000000108"]);
    }
    // On Wolf (protector defenseMaterial 113 / 139): row 113 Iron_Slash_S = 13 -> z000000013.
    let on_wolf = crate::sound::hit_sounds(combat, &sword, crate::sound::defender_materials(combat, crate::actor::Side::Player));
    assert_eq!(on_wolf, ["z000000013"]);
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
        assert!(combat.enemy.perilous_warning(a, -1.0, 1e9).is_some(), "{a} is not perilous");
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
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
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
    let atk = combat.enemy.attacks.values().find(|a| a.def_se_material1 == 100 && a.atk_material_se == 0 && a.atk_type == 0).expect("sword attack");
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
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
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
            world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = false;
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
fn behind_deathblow_turns_with_the_enemy() {
    // a200_511200's root yaw (Ochimusha behind deathblow) is keyed across the +-pi seam; exported
    // the short way it turns +180 deg with the enemy's ThrowDef13200 instead of -270.
    let app = app_with("c1010");
    let combat = app.world().resource::<Combat>();
    let yaws: Vec<f32> = (0..=20).filter_map(|f| combat.player.root_at("a200_511200", f as f32 / 30.0).map(|(_, y)| y)).collect();
    if yaws.is_empty() {
        return;
    }
    assert!(yaws.windows(2).all(|w| w[1] >= w[0] - 1e-3), "not monotonic: {yaws:?}");
    assert!((yaws[20] - std::f32::consts::PI).abs() < 0.05, "ends at {:.2}", yaws[20]);
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
