//! Sim tests, part 1: 49 tests, from `timed_guard_deflects_and_bounces_the_enemy` to `posture_break_staggers_then_a_second_hit_knocks_wolf_down`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;


pub(crate) fn app() -> App {
    sim_tests_app()
}

pub(crate) fn sim_tests_app() -> App {
    app_with("c1020")
}

/// The sim with a given enemy (config.toml's enemy.chr is overridden so tests are stable).
pub(crate) fn app_with(chr: &str) -> App {
    app_cfg(chr, None, Vec::new())
}

/// The sim with an enemy (NpcParam row) and more enemies at once (config enemy.group).
pub(crate) fn app_cfg(chr: &str, npc_row: Option<i64>, group: Vec<crate::config::GroupEnemy>) -> App {
    app_build(chr, npc_row, group, false)
}

/// The sim with the characters' real geometry (anim.rs skeletons, model.rs GeometryPlugin):
/// hits test the AtkParam capsule between dummy polys against the ragdoll hurt capsules, as in
/// the game, instead of the reach approximation.
pub(crate) fn app_geom(chr: &str, npc_row: Option<i64>) -> App {
    app_build(chr, npc_row, Vec::new(), true)
}

pub(crate) fn app_build(chr: &str, npc_row: Option<i64>, group: Vec<crate::config::GroupEnemy>, geometry: bool) -> App {
    // The game's idols and Sculptor (the host's hooks; IdolPlugin sets them in the game). No items
    // on the ground: the pickup list is global and other tests fill it.
    crate::encounter::set_world(crate::encounter::WorldHooks { take_item: |_| false, ..crate::idol::world_hooks() });
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .add_plugins(ConfigPlugin);
    let several = !group.is_empty();
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.enemy.chr = chr.to_string();
        config.enemy.npc_row = npc_row;
        // A fresh Wolf (Vitality 1), whatever config.toml plays at.
        config.posture.player_level = 1.0;
        config.enemy.group = group;
    }
    app.add_plugins((DataPlugin, ActorPlugin, CombatPlugin, PlayerPlugin, EnemyPlugin, crate::prosthetic::ProstheticPlugin, crate::status::StatusPlugin, crate::enemy_bullet::EnemyBulletPlugin, crate::hks_wolf::HksWolfPlugin, crate::hks_enemy::HksEnemyPlugin));
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

pub(crate) fn enemy_state(app: &mut App) -> (String, String, f32) {
    let world = app.world_mut();
    let e = boss_in_play(world);
    let a = world.get::<Actor>(e).unwrap();
    (a.state.clone(), a.anim.clone(), a.t)
}

thread_local! {
    /// boss_in_play's chr filter for this test's thread (None: every chr).
    pub(crate) static BOSS_CHR: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
    /// Cast members already deathblown, passed over (the caught monkeys).
    pub(crate) static BOSS_DONE: std::cell::RefCell<Vec<Entity>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The enemy the fight is about: the one character (the map script's cast aside), or, once the
/// script has swapped it out (Lady Butterfly's 1000800 -> 1000810 at her change, m10 11005812),
/// the enabled one with the largest HP pool. A main that is only a stand-in (Enemy::stand_in)
/// is not it; with BOSS_CHR set, only that chr counts (the Folding Screen Monkeys' four c1260,
/// not the ordinary c1250 monkeys of the same fight, whose larger HP would come first); those in
/// BOSS_DONE are passed over.
pub(crate) fn boss_in_play(world: &mut World) -> Entity {
    try_boss_in_play(world).expect("an enemy")
}

/// boss_in_play, None when no one is left (all four monkeys caught).
pub(crate) fn try_boss_in_play(world: &mut World) -> Option<Entity> {
    let only = BOSS_CHR.with(|c| c.get());
    let mut q = world.query_filtered::<(Entity, &Enemy, &Actor, Has<crate::enemy::Cast>), With<Enemy>>();
    let all: Vec<(Entity, bool, f32, bool, usize)> = q.iter(world).map(|(ent, e, a, cast)| (ent, e.disabled || e.stand_in || BOSS_DONE.with(|d| d.borrow().contains(&ent)), a.hp_max, cast, a.kind)).collect();
    let combat = world.resource::<Combat>();
    let all: Vec<(Entity, bool, f32, bool)> = all.into_iter().filter(|c| only.is_none_or(|chr| combat.kind(c.4).foe.chr == chr)).map(|c| (c.0, c.1, c.2, c.3)).collect();
    if let Some(main) = all.iter().find(|c| !c.3 && !c.1) {
        return Some(main.0);
    }
    all.iter().filter(|c| !c.1).max_by(|x, y| x.2.total_cmp(&y.2)).or(all.iter().find(|c| !c.3)).map(|c| c.0)
}

pub(crate) fn player_state(app: &mut App) -> (String, String, f32, f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Actor, With<Player>>();
    let a = q.single(world).unwrap();
    (a.state.clone(), a.anim.clone(), a.t, a.posture)
}

pub(crate) fn log_text(app: &App) -> String {
    app.world().resource::<CombatLog>().lines.iter().map(|l| l.0.clone()).collect::<Vec<_>>().join("\n")
}

pub(crate) fn start_enemy_combo(app: &mut App, combo: usize) {
    let world = app.world_mut();
    let mut q = world.query::<(&mut Enemy, &mut Actor)>();
    let (mut e, mut a) = q.single_mut(world).unwrap();
    e.force_combo(&mut a, combo);
}

/// Seconds until the first damaging hitbox of the enemy's current anim starts.
pub(crate) fn time_to_enemy_hit(app: &mut App) -> Option<f32> {
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
pub(crate) fn guard_against_attack(lead: f32, extra_presses: usize) -> String {
    guard_against_attack_facing(lead, extra_presses, false)
}

pub(crate) fn guard_against_attack_facing(lead: f32, extra_presses: usize, facing_away: bool) -> String {
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
pub(crate) fn timed_guard_deflects_and_bounces_the_enemy() {
    // 2 TAE frames before the hit: inside the 6-frame window of StandToDeflectGuard.
    let out = guard_against_attack(2.0 / TAE_FPS, 0);
    assert!(out.contains("YOU DEFLECT"), "{out}");
    // Combo 1 hit 1 has justDeflectedAction 1 -> full bound right.
    assert!(out.contains("enemy=AttackBoundEnemy1_Right"), "{out}");
    assert!(out.contains("player=StandDeflectHard"), "{out}");
}

#[test]
pub(crate) fn late_guard_only_blocks() {
    // 10 TAE frames before the hit: past the 6-frame deflect window, still guarding.
    let out = guard_against_attack(10.0 / TAE_FPS, 0);
    assert!(out.contains("blocked"), "{out}");
    assert!(out.contains("player=StandDeflectEasy"), "{out}");
}

#[test]
pub(crate) fn spammed_guard_loses_the_deflect() {
    // Fourth press in quick succession: StandToDeflectGuard4 has no deflect window.
    let out = guard_against_attack(1.0 / TAE_FPS, 3);
    assert!(!out.contains("YOU DEFLECT"), "{out}");
}

#[test]
pub(crate) fn no_guard_gets_hit() {
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
pub(crate) fn enemy_guards_the_first_player_attack() {
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
pub(crate) fn vertical_jump_follows_the_exe_arc() {
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
pub(crate) fn aggressive_enemy_runs_its_real_ai() {
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
pub(crate) fn defend_against(anim: &str, lead: f32, act: Action, stick: Vec2) -> String {
    defend_against_with("c1020", anim, lead, act, stick)
}

pub(crate) fn defend_against_with(chr: &str, anim: &str, lead: f32, act: Action, stick: Vec2) -> String {
    defend_against_locked(chr, anim, lead, act, stick, false)
}

/// As `defend_against_with`, locked on to the enemy when `locked` (the 4-way steps).
pub(crate) fn defend_against_locked(chr: &str, anim: &str, lead: f32, act: Action, stick: Vec2, locked: bool) -> String {
    let mut app = app_with(chr);
    {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &mut Enemy, &mut Actor)>();
        let (entity, mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, anim);
        if locked {
            world.resource_mut::<crate::camera::LockOn>().target = Some(entity);
        }
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

/// Wolf's own spCategory 1001 SpEffects turn a hit's reaction level as an enemy's do: in the
/// knock-down wake-up (a000_100310, 100306: small / middle / large / push -> minimum) the General's
/// slash lands (HP) but does not re-stagger him (it played the damage state: a stunlock)
/// (2026-10-13 round 29).
#[test]
pub(crate) fn wolfs_wake_up_super_armour_keeps_him_from_restaggering() {
    let mut app = app_with("c1020");
    let key = "a000_100310";
    let t = {
        let combat = app.world().resource::<Combat>();
        let d = &combat.player;
        (0..34)
            .map(|f| (f as f32 + 0.5) / TAE_FPS)
            .find(|&t| d.sp_effects_at(key, t).iter().any(|(id, _)| *id == 100306) && !d.events_at(key, t).any(|e| matches!(e.kind, 950 | 951 | 954)))
            .expect("a 100306 frame without iframes")
    };
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
    }
    let hold = |app: &mut App| {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        if a.anim != key {
            a.play("WakeUp", key);
        }
        a.prev_t = t;
        a.t = t;
    };
    let hp0 = app.world_mut().query_filtered::<&Actor, With<Player>>().single(app.world()).unwrap().hp;
    for _ in 0..300 {
        hold(&mut app);
        app.update();
        if log_text(&app).contains("hit:") {
            let log = log_text(&app);
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            assert!(a.hp < hp0, "the slash did no damage");
            assert_eq!((a.anim.as_str(), a.state.as_str()), (key, "WakeUp"), "re-staggered: {log}");
            return;
        }
    }
    panic!("the General's slash never hit: {}", log_text(&app));
}

/// a000_003010 "Advance thrust" (AtkParam 200: thrust, disableGuard 1, disableJustGuard 0).
pub(crate) const PERILOUS_THRUST: &str = "a000_003010";
/// a000_003004 "Lower Nagibashi" sweep (AtkParam 140: disableGuard 1, disableJustGuard 1).
pub(crate) const PERILOUS_SWEEP: &str = "a000_003004";

#[test]
pub(crate) fn perilous_thrust_goes_through_a_block() {
    let out = defend_against(PERILOUS_THRUST, 12.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("enemy hit") && !out.contains("blocked"), "{out}");
}

#[test]
pub(crate) fn perilous_thrust_can_still_be_deflected() {
    let out = defend_against(PERILOUS_THRUST, 2.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("YOU DEFLECT"), "{out}");
}

#[test]
pub(crate) fn perilous_sweep_cannot_be_deflected() {
    let out = defend_against(PERILOUS_SWEEP, 2.0 / TAE_FPS, Action::Guard, Vec2::ZERO);
    assert!(out.contains("enemy hit") && !out.contains("DEFLECT"), "{out}");
}

#[test]
pub(crate) fn stepping_into_a_perilous_thrust_is_a_mikiri() {
    // Forward step toward the enemy (+Z): GroundStep_F carries MikiriCounter (flag 125) on frames 0-12.
    let out = defend_against(PERILOUS_THRUST, 5.0 / TAE_FPS, Action::Step, Vec2::new(0.0, -1.0));
    assert!(out.contains("MIKIRI COUNTER"), "{out}");
    assert!(out.contains("enemy=ThrowDef13800") && out.contains("player=Mikiri"), "{out}");
}

/// A step's TAE 954 IFrames dodge by the attack's AtkParam trackType (FUN_140b68a50 bit test,
/// 140b6.c:4234-4241): the side / back steps are types 6 / 7 / 2, the jump start type 1. A side
/// step through the General's perilous sweep (judge 140, trackType 1) is hit; a back step through
/// his perilous thrust (150, trackType 6) passes; a jump clears the sweep. Round 31: type 2 was
/// "everything", so a step dodged the sweep too.
#[test]
pub(crate) fn a_side_step_does_not_dodge_a_sweep() {
    let out = defend_against_locked("c1020", PERILOUS_SWEEP, 2.0 / TAE_FPS, Action::Step, Vec2::new(1.0, 0.0), true);
    assert!(out.contains("enemy hit"), "{out}");
}

#[test]
pub(crate) fn a_back_step_dodges_a_perilous_thrust() {
    // Locked on, stick back (unlocked every step is GroundStep_F: the Mikiri step, and its
    // IFrames 7 / 2 do not cover a thrust).
    let out = defend_against_locked("c1020", PERILOUS_THRUST, 2.0 / TAE_FPS, Action::Step, Vec2::new(0.0, 1.0), true);
    assert!(out.contains("player=GroundStep_B") || !out.contains("player=GroundStep_F"), "{out}");
    assert!(!out.contains("enemy hit") && !out.contains("MIKIRI"), "{out}");
}

#[test]
pub(crate) fn a_jump_clears_a_sweep() {
    let out = defend_against(PERILOUS_SWEEP, 3.0 / TAE_FPS, Action::Jump, Vec2::ZERO);
    assert!(!out.contains("enemy hit"), "{out}");
}

/// cargo test ai_trace -- --ignored --nocapture : prints 20 s of enemy AI decisions.
#[test]
#[ignore]
pub(crate) fn ai_trace() {
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
pub(crate) fn deathblow_plays_the_throw_pair() {
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

/// The deathblow start commits the kill: nothing re-checks the enemy's break once the throw pair
/// is made (the judge FUN_140be2d60 runs at pair time only; Wolf's W_ThrowKill needs env 274 and
/// his own ref 211, c0000_transition.lua:5591-5595; c9997 ThrowDefCommonFunction leaves only on
/// death / damage / fall / anim end). A press in the last frame of the break window still ends in
/// ThrowDefDeath12001: the window stops counting once the start throw holds the enemy (the HANDOFF
/// OPEN "deathblow start commits without re-checking": as the game).
#[test]
pub(crate) fn a_deathblow_pressed_at_the_end_of_the_break_still_kills() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
            let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, false, 4.0);
            // One frame of the window left.
            e.set_broken_left(1.0 / 60.0);
            t.translation.z = 1.8;
        });
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut seen = Vec::<String>::new();
    for _ in 0..360u32 {
        app.update();
        let es = enemy_state(&mut app).0;
        if seen.last() != Some(&es) {
            seen.push(es.clone());
        }
        if es == "ThrowDefDeath12001" {
            return;
        }
    }
    panic!("the kill did not land: enemy {seen:?}, log {}", log_text(&app));
}

#[test]
pub(crate) fn an_enemy_plays_its_own_anim_set() {
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
pub(crate) fn isshins_3015_switches_him_to_his_second_set() {
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
pub(crate) fn break_and_deathblow(app: &mut App, frames: u32) -> Vec<String> {
    break_and_deathblow_died(app, frames).0
}

/// As `break_and_deathblow`, also whether it was dead at some point (it respawns after a while).
pub(crate) fn break_and_deathblow_died(app: &mut App, frames: u32) -> (Vec<String>, bool) {
    {
        let world = app.world_mut();
        let boss = boss_in_play(world);
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query::<(&mut Enemy, &mut Actor, &mut Transform)>();
            let (mut e, mut a, mut t) = q.get_mut(world, boss).unwrap();
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
    for f in 0..frames {
        // A boss kneeling in its 20200 takes the Todome on attack (player.rs deathblow_check),
        // as the player presses it.
        if f % 20 == 0 && enemy_state(app).0 == "Event20200" {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        app.update();
        let s = enemy_state(app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
        let world = app.world_mut();
        let boss = boss_in_play(world);
        died |= world.get::<Enemy>(boss).unwrap().is_dead();
        // (A map script's Handle Boss Defeat: Isshin is defeated, not dead.)
        died |= world.resource::<crate::enemy::BossScript>().run.as_ref().is_some_and(|r| r.defeated.is_some());
    }
    (states, died)
}

#[test]
pub(crate) fn the_corrupted_monks_last_deathblow_is_her_todome() {
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

/// The Demon of Hatred (70200000, three bars): m11_00 11105910 keeps him immortal, so his third
/// deathblow's ThrowDef12000 leaves him at 0 bars, not dead; 11105921 (3702050 "HU後状態" from his
/// 20001, 0 bars, Wolf's message 10) kneels him in 20200 for the Todome 17020180 (Wolf
/// a240_501700 -> 511700, his ThrowDef13700), as rec_20261016_072651 (4 deathblows, the last one
/// the Todome).
#[test]
pub(crate) fn the_demon_of_hatreds_last_deathblow_is_his_todome() {
    let mut app = app_cfg("c7020", Some(70200000), Vec::new());
    let mut all = Vec::new();
    for _ in 0..2 {
        let seen = break_and_deathblow(&mut app, 900);
        let w = app.world_mut();
        let boss = boss_in_play(w);
        // (His AI's Act45 plays the 20001 that leaves it, which this deathblow loop gives no
        // time for.)
        w.get_mut::<Actor>(boss).unwrap().resident.push(3_702_050);
        let (e, a) = (w.get::<Enemy>(boss).unwrap(), w.get::<Actor>(boss).unwrap());
        all.push(format!("{seen:?} bars {} hp {}", e.ninsatsu_left(), a.hp));
    }
    let (seen, died) = break_and_deathblow_died(&mut app, 1500);
    all.push(format!("{seen:?}"));
    assert!(seen.iter().any(|s| s == "ThrowDef13700"), "{all:?}");
    assert!(died, "{all:?}");
}

/// m25 without Wolf's Todome message (he drops his Todome before its TAE 936): her Todome's
/// ThrowDef13700 still kills her (it sends 10 at its start, ending her immortality: 12505965)
/// and 12505950 handles her defeat; 12505964's 20021 does not play on her dead (it would cut
/// every Todome short: her a100_013700 has no 3500010).
#[test]
pub(crate) fn the_corrupted_monk_without_the_todome_message_still_dies() {
    let mut app = app_with("c5000");
    for _ in 0..2 {
        break_and_deathblow(&mut app, 500);
    }
    break_and_deathblow(&mut app, 30);
    let mut boss = Vec::<String>::new();
    let mut defeated = false;
    for f in 0..900 {
        // Her Todome, from her kneel (20200).
        if f % 20 == 0 && enemy_state(&mut app).0 == "Event20200" {
            app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        }
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            if a.anim.ends_with("511700") {
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
pub(crate) fn isshins_last_deathblow_ends_the_fight_with_wolfs_event_710206() {
    // m11_02 11125874: 0 health bars and Wolf's event message 10 (his deathblow's TAE 936) -> Isshin EzState 20200, Wolf EzState 710206 (c0000.hkx Event710206_CMSG).
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
    // His last mark is his usual deathblow (15400500 / 501 -> ThrowDef12000), not the Todome: rec_20261016_080607 a100_012000 -> 020200.
    assert!(boss[..i].iter().any(|s| s == "ThrowDef12000") && !boss.iter().any(|s| s == "ThrowDef13700") || i == 0, "boss {boss:?}");
    assert!(wolf.iter().any(|s| s == "Event710206"), "wolf {wolf:?} boss {boss:?}");
}

#[test]
pub(crate) fn the_corrupted_monks_phase_events_run_on_her_deathblows() {
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
pub(crate) fn the_demon_of_hatreds_last_phase_swaps_its_sp_effects() {
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
pub(crate) fn a_boss_takes_one_deathblow_per_red_dot() {
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
pub(crate) fn deathblow_pair_moves_by_root_motion_like_the_live_game() {
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


pub(crate) fn player_actor(app: &mut App) -> (String, f32, bool, Vec3) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
    let (a, t) = q.single(world).unwrap();
    (a.state.clone(), a.vel_y, a.airborne, t.translation)
}

pub(crate) fn move_enemy_far(app: &mut App) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
    q.single_mut(world).unwrap().translation.z = 30.0;
}

#[test]
pub(crate) fn guard_walk_uses_the_guard_move_anims() {
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
    // Stick toward +Z = toward the camera (camera yaw 0 looks along -Z): the free guard walk turns
    // to the stick and walks forward (2026-10-17, the user: before, it faced the camera's
    // direction and walked backward).
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    for _ in 0..60 {
        app.update();
    }
    let (state, _, _, p1) = player_actor(&mut app);
    assert_eq!(state, "DeflectGuardMoveF");
    // DeflectGuardMoveF root motion: 2.0 m over 40 frames = 1.5 m/s, less the turn.
    let moved = (p1 - p0).with_y(0.0).length();
    assert!(moved > 0.9 && moved < 1.8, "moved {moved}");
    assert!(p1.z > p0.z + 0.8, "{p0:?} -> {p1:?}");
}

#[test]
pub(crate) fn guarding_in_the_air_and_landing_in_guard() {
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
pub(crate) fn kicking_off_the_enemy_relaunches() {
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
pub(crate) fn enemy_grabs_go_through_the_throw_kind_table() {
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
pub(crate) fn neck_grab_holds_then_throws() {
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

/// Atlas armor.flag73-parry-gate: an enemy swing has no armour of its own. The General's
/// a000_003000 (ChrActionFlag 73 at frames 1-9 and 54-80, nothing else) flinches from Wolf's
/// slash mid-swing; his 3003 swings on through frames 5-24, where ChrActionFlag 24 and the
/// spCategory 1001 SpEffect 5350 hold.
#[test]
pub(crate) fn enemy_mid_swing_takes_the_hit_without_flinching() {
    for (anim, ez, armoured) in [("a000_003003", "Ez3003", true), ("a000_003000", "Ez3000", false)] {
        let mut app = app();
        {
            let world = app.world_mut();
            let mut q = world.query::<(&mut Enemy, &mut Actor)>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, anim);
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
        let (state, got, t) = enemy_state(&mut app);
        assert!(log_text(&app).contains("you hit"), "{anim}: {}", log_text(&app));
        if armoured {
            assert!(t * TAE_FPS <= 24.0, "{anim}: the hit landed after the armour, frame {:.1}", t * TAE_FPS);
            assert_eq!((state.as_str(), got.as_str()), (ez, anim), "{}", log_text(&app));
        } else {
            assert!(state.starts_with("Damage"), "{anim} flinches: {state} {got} {}", log_text(&app));
        }
    }
}

#[test]
pub(crate) fn low_hp_slows_player_posture_regen() {
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
pub(crate) fn duel_trace() {
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
        // While Wolf is down the enemy idles by design (110010 Pretending Dead in his revival
        // wait, round 27): not a stall.
        let wolf_down = {
            let world = app.world_mut();
            world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp <= 0.0
        };
        if wolf_down {
            since_change = 0;
        } else if es == last_state {
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
pub(crate) fn attacking_out_of_a_deflect_uses_the_counter_slash() {
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
pub(crate) fn heavy_thrust_blows_the_player_away_and_lands() {
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
pub(crate) fn releasing_a_run_plays_the_run_stop() {
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
pub(crate) fn gourd_heals_forty_percent_at_the_consume_frame() {
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

/// Round 26: a second drink press is the Repeat only inside a000_250000's
/// SP_EF_REF_TAE_ENABLE_ITEM_REPEAT window (100315, 1.1-1.3 s); later in the accept window
/// (flag 31 to 2.1 s) it restarts the full drink (c0000_transition.lua 3993-4008).
#[test]
pub(crate) fn a_late_second_drink_is_a_fresh_drink() {
    let second = |at: usize| {
        let mut app = app();
        move_enemy_far(&mut app);
        app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
        for _ in 0..at {
            app.update();
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
        let mut states = Vec::new();
        for _ in 0..4 {
            app.update();
            let (s, _, _, _) = player_state(&mut app);
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        states
    };
    let early = second(70);
    assert!(early.iter().any(|s| s == "ItemGourdDrinkRepeat"), "{early:?}");
    let late = second(95);
    assert!(!late.iter().any(|s| s == "ItemGourdDrinkRepeat"), "{late:?}");
}

/// Drinking while walking is W_ItemGourdDrinkMove (HKS 3993-4009): the drink on the upper body
/// while the legs walk on (is_upper_action), and it still heals (round 32: the standing clip
/// slid along). Standing still it is the plain drink.
#[test]
pub(crate) fn drinking_while_walking_is_the_moving_drink() {
    for (walking, want) in [(true, "ItemGourdDrinkMove"), (false, "ItemGourdDrink")] {
        let mut app = app();
        move_enemy_far(&mut app);
        let gourd = {
            let world = app.world_mut();
            world.query::<&Player>().single(world).unwrap().gourd
        };
        for _ in 0..40 {
            if walking {
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
            }
            app.update();
        }
        let start = {
            let world = app.world_mut();
            world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
        };
        app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
        let mut seen = Vec::new();
        for _ in 0..60 {
            if walking {
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
            }
            app.update();
            let (s, ..) = player_state(&mut app);
            if seen.last() != Some(&s) {
                seen.push(s);
            }
        }
        assert!(seen.iter().any(|s| s == want), "walking {walking}: {seen:?}");
        let world = app.world_mut();
        let end = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
        let left = world.query::<&Player>().single(world).unwrap().gourd;
        assert_eq!(left, gourd - 1, "walking {walking}: the drink heals");
        if walking {
            assert!((end - start).with_y(0.0).length() > 1.0, "the legs walk on: moved {}", (end - start).length());
        }
    }
}

/// Pressing again in the repeat window after the last charge is
/// W_ItemGourdDrinkRepeatFailed (a000_250006, 1.83 s; c0000_transition.lua 4024-4033), not the
/// 2.33 s empty shake; later, the shake (round 30).
#[test]
pub(crate) fn an_empty_repeat_press_plays_the_repeat_failed_clip() {
    for (at, want) in [(70usize, "ItemGourdDrinkRepeatFailed"), (150, "ItemGourdDrinkFailed")] {
        let mut app = app();
        move_enemy_far(&mut app);
        {
            let world = app.world_mut();
            world.query::<&mut Player>().single_mut(world).unwrap().gourd = 1;
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
        for _ in 0..at {
            app.update();
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
        let mut states = Vec::new();
        for _ in 0..4 {
            app.update();
            let (s, ..) = player_state(&mut app);
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        assert!(states.iter().any(|s| s == want), "press at {at}: {states:?}");
    }
}

/// Crouched, the gourd is the crouch drink (HKS STYLE_TYPE_CROUCH: W_CrouchItemGourdDrink,
/// a000_250010): Wolf stays crouched and it heals the same.
#[test]
pub(crate) fn a_crouched_drink_keeps_wolf_crouched() {
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
pub(crate) fn attack_plus_guard_is_whirlwind_slash_and_hits_twice() {
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
pub(crate) fn quick_shuriken_throw_reaches_the_enemy() {
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

/// Round 59: Wolf's bullets respect the enemy's TAE 950 / 951 invulnerability as the sword
/// does (a shuriken hit and interrupted a knock-down before).
#[test]
pub(crate) fn a_shuriken_passes_an_invulnerable_enemy() {
    let mut app = app();
    // An enemy anim with a long TAE 950 window, held in its middle.
    let (anim, t) = {
        let combat = app.world().resource::<Combat>();
        let d = combat.enemy0();
        let mut best: Option<(String, f32, f32)> = None;
        for (k, a) in &d.anims {
            for e in a.events.iter().filter(|e| e.kind == 950 || e.kind == 951) {
                if best.as_ref().is_none_or(|b| e.end - e.start > b.2 - b.1) {
                    best = Some((k.clone(), e.start, e.end));
                }
            }
        }
        let (k, s, e) = best.expect("no TAE 950 / 951 anim");
        (k, (s + e) * 0.5)
    };
    let hold = |app: &mut App| {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
        let mut a = q.single_mut(world).unwrap();
        if a.anim != anim {
            a.play("Down", &anim);
        }
        a.t = t;
        a.prev_t = t;
        a.hp
    };
    let hp0 = hold(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    for _ in 0..60 {
        hold(&mut app);
        app.update();
    }
    let out = log_text(&app);
    assert!(out.contains("Loaded Shuriken ("), "{out}");
    assert!(!out.contains("hit: -"), "{anim} t {t}: {out}");
    assert_eq!(hold(&mut app), hp0);
}

/// Round 59: the Flame Vent follow-up's Bullet 720040 puts its spEffectIDForShooter 106020
/// "Flame enchantment" (stateInfo 62) on Wolf, and that opens the sword's fire bullets (a050
/// BulletBehavior judge 295 gated on StateInfo 62).
#[test]
pub(crate) fn the_flame_vent_enchants_the_sword() {
    let mut app = app();
    {
        let combat = app.world().resource::<Combat>();
        let Some(row) = combat.player.bullet_rows.get("720040") else { return };
        assert_eq!(row.sp_effect_id_for_shooter, 106020);
        assert!(!combat.player.resident_gates.contains(&62));
    }
    {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.push((106020, 7.5));
    }
    app.update();
    assert!(app.world().resource::<Combat>().player.resident_gates.contains(&62), "the enchantment opens no fire judges");
}

#[test]
pub(crate) fn timed_step_dodges_through_the_slash() {
    // Neutral step 1 frame before the 6-frame hit window: GroundStep_N IFrames (General)
    // frames 0-9 outlast it. (Step too early and the still-active hitbox lands after them.)
    let out = defend_against("a000_003000", 1.0 / TAE_FPS, Action::Step, Vec2::ZERO);
    assert!(!out.contains("enemy hit"), "{out}");
}

/// Taps attack, then presses it again at `frame` of the tap slash; did Combo2 follow?
pub(crate) fn combo_press_at(frame: f32) -> bool {
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
pub(crate) fn early_mash_is_dropped_but_a_press_in_the_accept_window_chains() {
    // Tap slash a050_300100: accept flag 87 from frame 6, execute (115) from frame 15.
    assert!(!combo_press_at(3.0), "a press before the accept window must not be buffered");
    assert!(combo_press_at(8.0), "a press inside the accept window chains into Combo2");
}

pub(crate) fn set_player_posture(app: &mut App, posture: f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&mut Actor, With<Player>>();
    let mut a = q.single_mut(world).unwrap();
    a.posture = posture;
}

/// Lets the enemy swing `anim` until it connects; returns the player's state after the hit.
pub(crate) fn take_hit(app: &mut App, anim: &str) -> String {
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
pub(crate) fn posture_break_staggers_then_a_second_hit_knocks_wolf_down() {
    let mut app = app();
    let max = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Actor, With<Player>>();
        q.single(world).unwrap().posture_max
    };
    set_player_posture(&mut app, max - 1.0);
    // The second hit lands on a broken Wolf at x1.5 (SpEffect 109400, round 25): 128 + 192 HP
    // would kill a level-1 Wolf, so give him room to show the knock-down.
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(world).unwrap();
        (a.hp_max, a.hp) = (2000.0, 2000.0);
    }
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

/// The upper-action states come from the script's own table (c0000_cmsg g_paramHkbState
/// STATE_TYPE_UPPER_ACTION*): the tools' release-on-the-move too (its legs' run counts for
/// GetLocomotionType, so W_StandMoveLowerStart fires once, not every frame).
#[test]
pub(crate) fn upper_action_states_are_the_scripts() {
    if !crate::paths::extracted().join("hks_src/c0000_cmsg.lua").exists() {
        return;
    }
    for s in ["GroundSubAttackCombo1ReleaseMove", "GroundSubAttackLockOnReleaseMove", "ItemGourdDrinkMove", "CrouchSubAttackCombo1Move", "DeflectGuardToStandMoveRun"] {
        assert!(crate::player::is_upper_action(s), "{s}");
    }
    for s in ["GroundSubAttackCombo1", "StandIdle", "ItemGourdDrink", "Locomotion"] {
        assert!(!crate::player::is_upper_action(s), "{s}");
    }
    // env 1105, the STATE_TYPE_STANDBY* states: the sprint's walk-out stops on letting go.
    for s in ["StandMoveStartFromSprint", "StandMoveLoopFromSprint", "SprintLoop", "StandIdle", "GroundSpacialAttackHoldLoop"] {
        assert!(crate::player::is_standby(s), "{s}");
    }
    for s in ["SprintStartFromStep_F", "GroundStep_F", "GroundAttackCombo1", "ItemGourdDrinkMove"] {
        assert!(!crate::player::is_standby(s), "{s}");
    }
}
