//! The Ashina Samurai General (c1020) as a sparring partner. Stats come from
//! NpcParam 10219000, attacks and their timing from c1020's TAE and
//! AtkParam_Npc, reactions from the shared NPC behaviour (c9997).
//!
//! Defence follows its real AI (ai/102000_battle.lua, Goal.Parry): when the
//! player's attack notifies the AI (TAE flag 63, a few frames before the hit)
//! the enemy picks deflect (anim 3101) or guard (3100) from its parry rank
//! (resident SpEffect 221000-2), the attack type (SpEffect 109970 = thrust)
//! and its consecutive-guard count (weak clashes 200215/6 count up, strong
//! 200210/1 reset). Offence and movement come from the real battle goal running
//! in the Lua AI runtime (ai.rs): acts, combos, spacing, and Kengeki counters
//! after clashes. Without extracted AI scripts it falls back to fixed combos.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::actor::{Actor, ActorSet, Side};
use crate::ai::{Brain, Snapshot};
use crate::config::GameConfig;
use crate::data::Combat;
use crate::player::Player;

/// Fallback c1020 attack strings (Goal.Act01..07 of 102000_battle.lua: 3000-3002 combo, 3004-3005, 3006, 3010).
const COMBOS: &[&[&str]] = &[
    &["a000_003000", "a000_003001", "a000_003002"],
    &["a000_003004", "a000_003005"],
    &["a000_003006"],
    &["a000_003010"],
];

/// ChrActionFlag 23: "End If AI ComboAttack Queued" (the chain point).
const FLAG_AI_COMBO: i64 = 23;
/// ChrActionFlag 78: "End If AI Move Queued".
const FLAG_AI_MOVE: i64 = 78;
/// NPC AI cancel flags: 23 combo attack, 86 attack, 79 step, 78 move.
const AI_CANCEL_FLAGS: [i64; 4] = [23, 86, 79, 78];
/// ChrActionFlag 63 on the attacker: the AI notification that fires INTERUPT_ParryTiming.
pub const FLAG_AI_NOTIFY: i64 = 63;
/// PC_ATTACK_DIST_STAND (ai_define): standing player attack reach as the AI sees it.
const PC_ATTACK_DIST_STAND: f32 = 3.4;
/// Battle-state SpEffect the engine keeps on NPCs in combat (Goal.Interrupt checks it).
const SP_BATTLE_STATE: i64 = 200004;
/// SpEffect stateInfo 352: posture display "collapsed" (deathblow available).
const STATE_INFO_COLLAPSED: i64 = 352;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Driven by the AI (or the passive / fallback behaviour).
    Ai,
    Broken(f32),
    Dead(f32),
}

/// ChrActionFlag 69 "ThrowType5": on ThrowDef reactions, where a killed defender switches to
/// its ThrowDefDeath anim.
const FLAG_THROW_TYPE5: i64 = 69;

#[derive(Component)]
pub struct Enemy {
    mode: Mode,
    /// Deathblow kill pending: the ThrowDefDeath anim id that follows the ThrowDef reaction.
    throw_death: Option<i64>,
    /// Passive enemies walk up but never attack (toggle with T) - for practising timing at your own pace.
    pub aggressive: bool,
    cooldown: f32,
    rng: u32,
    /// "ConsecutiveGuardCount" and AI timer 13 (1 s) from common_func_NTC.lua.
    guard_count: i32,
    guard_count_timer: f32,
    /// AI_TIMER_PARRY_INTERVAL (0.1 s).
    parry_timer: f32,
    /// Engine-applied clash SpEffects (200200-200216) with remaining time.
    pub clash: Vec<(i64, f32)>,
    last_notify: f32,
    /// EzState id of the AI-requested anim (attack, step) that is playing.
    cur_ez: Option<i64>,
    ez_started: Option<i64>,
    ez_failed: Option<i64>,
    /// An anim the AI did not request replaced its action since the last AI tick.
    interrupted: bool,
    /// Player attack notify seen; the Lua brain runs Goal.Parry on its next tick.
    parry_timing: bool,
    prev_sp: HashSet<i64>,
    /// Active goal chain, for the debug panel.
    pub ai_desc: String,
    /// Fallback combo when no AI scripts are available: (combo, step).
    fallback: Option<(usize, usize)>,
}

impl Enemy {
    pub fn is_broken(&self) -> bool {
        matches!(self.mode, Mode::Broken(_))
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.mode, Mode::Dead(_))
    }

    /// Killed by a deathblow: the ThrowParam defender anim ThrowDef<def> plays out the pair with
    /// Wolf (its root motion: the General slides 2 m back), then, with HP at 0, its ChrActionFlag
    /// 69 (ThrowType5) switches to ThrowDefDeath<def + 1> (c9997 HKS: env(276) ->
    /// W_ThrowDefDeath). Live (rec_20261008_054259): 12000 for 111 frames (flag at TAE 55), then
    /// 12001. Without a ThrowDef anim: ThrowDefDeath directly, else a procedural death.
    pub fn deathblow(&mut self, a: &mut Actor, combat: &Combat, def_anim: i64) {
        a.hp = 0.0;
        self.cur_ez = None;
        a.move_vel = Vec3::ZERO;
        let (react, death) = (format!("ThrowDef{def_anim}"), format!("ThrowDefDeath{}", def_anim + 1));
        let has = |s: &str| combat.enemy.anim_key(s).is_some();
        self.throw_death = None;
        if has(&react) && has(&death) && a.play_state(&combat.enemy, &react) {
            self.throw_death = Some(def_anim + 1);
        } else if !([death, react].iter().any(|s| has(s) && a.play_state(&combat.enemy, s))) {
            a.procedural("Dead");
        }
        let len = combat.enemy.length(&a.anim);
        self.mode = Mode::Dead(len + 2.0);
    }

    /// Posture broken: a deflect/guard break keeps its AttackBoundEmptyStamina /
    /// GuardBreak anim, a hit break plays TrunkCollapseFront (Back from behind) as in
    /// c9997 HKS ExecDamageBreak. The deathblow window lasts until the break anim
    /// shows the collapsed posture (stateInfo 352: frames 0-75 of 90); `fallback` if none.
    pub fn on_posture_break(&mut self, a: &mut Actor, combat: &Combat, from_behind: bool, fallback: f32) {
        if matches!(self.mode, Mode::Dead(_)) {
            return;
        }
        let d = &combat.enemy;
        if !a.state.starts_with("AttackBoundEmptyStamina") && !a.state.starts_with("GuardBreak") {
            let s = if from_behind && d.anim_key("TrunkCollapseBack").is_some() { "TrunkCollapseBack" } else { "TrunkCollapseFront" };
            if !a.play_state(d, s) {
                a.procedural("PostureBroken");
            }
        }
        // Window = SpEffect stateInfo 352 ("trunk display: collapsed", 220420, frames 0-75 of
        // every break anim: the deathblow indicator); else the anim's AI attack flag 86.
        let anim = d.anim(&a.anim);
        let collapse_end = anim.and_then(|an| {
            an.events
                .iter()
                .filter(|e| matches!(e.kind, 67 | 401))
                .filter(|e| d.sp_effects.get(&e.arg_i64("SpEffectID").unwrap_or(0).to_string()).is_some_and(|s| s.state_info == STATE_INFO_COLLAPSED))
                .map(|e| e.end)
                .reduce(f32::max)
        });
        let ai_flag = anim.and_then(|an| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(86)).map(|e| e.start).reduce(f32::min));
        let window = collapse_end.or(ai_flag).map(|t| (t - a.t).max(0.5)).unwrap_or(fallback);
        self.mode = Mode::Broken(window);
        a.move_vel = Vec3::ZERO;
        self.cur_ez = None;
    }

    /// Plays a reaction state; the AI replans (Kengeki) and acts again at the
    /// reaction's AI cancel flags.
    pub fn react(&mut self, a: &mut Actor, combat: &Combat, state: &str) {
        if matches!(self.mode, Mode::Dead(_) | Mode::Broken(_)) {
            return;
        }
        if a.play_state(&combat.enemy, state) {
            self.interrupt();
            a.move_vel = Vec3::ZERO;
        }
    }

    fn interrupt(&mut self) {
        self.interrupted = true;
        self.cur_ez = None;
        self.fallback = None;
    }

    /// Engine clash result SpEffect (lasts 0.1 s); also drives the guard count.
    pub fn add_clash(&mut self, id: i64) {
        self.clash.push((id, 0.1));
        match id {
            200215 | 200216 => {
                self.guard_count = if self.guard_count_timer <= 0.0 { 1 } else { self.guard_count + 1 };
                self.guard_count_timer = 1.0;
            }
            200210 | 200211 => {
                self.guard_count = 0;
                self.guard_count_timer = 0.0;
            }
            _ => {}
        }
    }

    /// Test hook: start a specific combo immediately.
    #[cfg(test)]
    pub fn force_combo(&mut self, a: &mut Actor, combo: usize) {
        a.play(&format!("Combo{combo}.0"), COMBOS[combo][0]);
        self.cur_ez = ez_of(COMBOS[combo][0]);
        self.fallback = Some((combo, 0));
    }

    /// Test hook: play one attack anim as if the AI had requested it.
    #[cfg(test)]
    pub fn force_anim(&mut self, a: &mut Actor, key: &str) {
        let ez = ez_of(key);
        a.play(&format!("Ez{}", ez.unwrap_or(0)), key);
        self.cur_ez = ez;
        self.fallback = None;
    }

    /// Playing one of its own attacks (EzState 3000-3099), not a guard/deflect.
    pub fn is_attacking(&self) -> bool {
        matches!(self.cur_ez, Some(ez) if (3000..3100).contains(&ez))
    }

    pub fn guard_count(&self) -> i32 {
        if self.guard_count_timer > 0.0 { self.guard_count } else { 0 }
    }

    fn rand100(&mut self) -> i32 {
        (self.rand() * 100.0) as i32 + 1
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }
}

/// "a000_003000" -> 3000
fn ez_of(anim: &str) -> Option<i64> {
    anim.get(5..).and_then(|s| s.parse().ok())
}

/// Lua brains per enemy (mlua states are not Send, so this is a non-send resource).
#[derive(Default)]
struct Brains {
    map: HashMap<Entity, Brain>,
    failed: bool,
}

/// Debug-menu control of the enemy's behaviour (debug_menu.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiMode {
    /// The game's own Lua battle AI.
    #[default]
    Real,
    /// Walks into range and waits (deflect practice).
    Passive,
    /// Only perilous (red kanji) attacks: sweeps, thrusts, grabs, picked at random.
    Perilous,
    /// One chosen attack, again and again.
    Repeat,
    /// Stands still.
    Idle,
}

#[derive(Resource)]
pub struct EnemyDebug {
    pub mode: AiMode,
    /// Attack anim for Repeat (a000_003xxx).
    pub attack: String,
    /// Seconds between drill attacks.
    pub interval: f32,
    timer: f32,
}

impl Default for EnemyDebug {
    fn default() -> Self {
        EnemyDebug { mode: AiMode::Real, attack: String::new(), interval: 1.5, timer: 0.0 }
    }
}

/// The enemy's own attacks (EzState 3000-3099 anims with attack windows), sorted, with their
/// perilous warning (TAE 2 BehaviorJudgeID 980 sweep / 982 thrust / 983 grab) if any.
pub fn attack_list(d: &crate::data::CharData) -> Vec<(String, Option<i64>)> {
    let mut v: Vec<(String, Option<i64>)> = d
        .anims
        .keys()
        .filter(|k| k.starts_with("a000_0030") && ez_of(k).is_some_and(|ez| (3000..3100).contains(&ez)))
        .filter(|k| !d.attack_windows(k).is_empty())
        .map(|k| (k.clone(), d.perilous_warning(k, -1.0, 1e9)))
        .collect();
    v.sort();
    v
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(Brains::default())
            .init_resource::<EnemyDebug>()
            .add_systems(Startup, spawn_enemy)
            .add_systems(FixedUpdate, (parry_interrupt, think).chain().in_set(ActorSet::Decide))
            .add_systems(Update, toggle_aggression.run_if(resource_exists::<ButtonInput<KeyCode>>));
    }
}

fn npc_stats(combat: &Combat) -> (f32, f32, f32, i64) {
    let n = combat.param("NpcParam", combat.foe.npc_row);
    let f = |k: &str| n[k].as_f64().unwrap_or(0.0) as f32;
    (f("hp"), f("stamina"), f("staminaRecoverBaseVel"), n["staminaControlParamId"].as_i64().unwrap_or(0))
}

fn spawn_enemy(mut commands: Commands, combat: Res<Combat>, config: Res<GameConfig>) {
    let (hp, posture, regen, ctrl) = npc_stats(&combat);
    let mut actor = Actor::new(Side::Enemy, hp, posture, regen, ctrl);
    let npc = combat.param("NpcParam", combat.foe.npc_row);
    actor.resident = (0..32).filter_map(|i| npc[format!("spEffectID{i}")].as_i64()).filter(|&v| v > 0).collect();
    actor.posture_debt = -(npc["maxDebtStamina"].as_f64().unwrap_or(0.0) as f32).min(0.0);
    actor.guard_angle = npc["guardAngle"].as_f64().unwrap_or(0.0) as f32;
    // Area scaling: HP / posture now, posture regen through the resident product,
    // damage and posture damage in combat.
    if let Some(dope) = combat.enemy.sp_effects.get(&config.enemy.area_doping.to_string()) {
        actor.hp_max *= dope.max_hp_rate;
        actor.hp = actor.hp_max;
        actor.posture_max *= dope.max_stamina_rate;
        actor.atk_rate = dope.physics_attack_power_rate;
        actor.stam_atk_rate = dope.stamina_attack_rate;
        actor.resident.push(config.enemy.area_doping);
    }
    commands.spawn((
        Enemy {
            mode: Mode::Ai,
            aggressive: false,
            cooldown: 1.5,
            rng: 0x9E37_79B9,
            guard_count: 0,
            guard_count_timer: 0.0,
            parry_timer: 0.0,
            clash: Vec::new(),
            last_notify: -1.0,
            cur_ez: None,
            throw_death: None,
            ez_started: None,
            ez_failed: None,
            interrupted: false,
            parry_timing: false,
            prev_sp: HashSet::new(),
            ai_desc: String::new(),
            fallback: None,
        },
        actor,
        Name::new("Samurai General"),
        Transform::from_xyz(0.0, crate::player::CAPSULE_HALF_HEIGHT + 0.05, -4.0),
        Visibility::default(),
    ));
}

/// Capsule + blade for the enemy (windowed app only).
pub fn enemy_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<Entity, (With<Enemy>, Without<Mesh3d>)>,
) {
    for e in &q {
        commands
            .entity(e)
            .insert((Mesh3d(meshes.add(Capsule3d::new(0.45, 1.1))), MeshMaterial3d(materials.add(Color::srgb(0.45, 0.2, 0.18)))))
            .with_children(|p| crate::world::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.9, 0.75, 0.6)));
    }
}

fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    yaw + diff.clamp(-max_step, max_step)
}

fn in_front(from: &Actor, from_pos: Vec3, to_pos: Vec3, half_angle_deg: f32) -> bool {
    let dir = (to_pos - from_pos).with_y(0.0).normalize_or_zero();
    from.forward().angle_between(dir).to_degrees() <= half_angle_deg
}

/// Goal.Parry (102000_battle.lua), run when the player's attack notifies the AI. With the
/// real AI on, the notify is forwarded to the Lua brain (its own Goal.Parry); this Rust
/// port only drives the passive practice partner.
fn parry_interrupt(
    time: Res<Time>,
    combat: Res<Combat>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor, &Transform), Without<Player>>,
    brains: NonSend<Brains>,
) {
    let dt = time.delta_secs();
    let (pa, ptf) = *player;
    let pd = &combat.player;
    for (mut e, mut a, tf) in &mut q {
        e.parry_timer -= dt;
        e.guard_count_timer -= dt;
        e.clash.retain_mut(|c| {
            c.1 -= dt;
            c.1 > 0.0
        });
        if pa.anim.is_empty() || matches!(e.mode, Mode::Dead(_) | Mode::Broken(_)) || a.hp <= 0.0 {
            continue;
        }
        // Rising edge of the notify flag on the player's current anim.
        let notify = pd.events_at(&pa.anim, pa.t).find(|ev| ev.kind == 0 && ev.flag_type() == Some(FLAG_AI_NOTIFY)).map(|ev| ev.start);
        let Some(start) = notify else { continue };
        if (start - e.last_notify).abs() < 1e-4 && pa.t - start < 0.2 {
            continue;
        }
        e.last_notify = start;
        // With the real AI running, the script's own Goal.Parry decides (Lua interrupt).
        if e.aggressive && !brains.failed {
            e.parry_timing = true;
            continue;
        }
        // Goal.Interrupt gates: battle state (SpEffect 200004, implied here), parry interval timer.
        if e.parry_timer > 0.0 || e.is_attacking() {
            continue;
        }
        let rank = if a.resident.contains(&221000) { 0 } else if a.resident.contains(&221001) { 1 } else if a.resident.contains(&221002) { 2 } else { -1 };
        if rank < 0 {
            continue;
        }
        e.parry_timer = 0.1;
        let dist = tf.translation.distance(ptf.translation);
        let player_refs: Vec<i64> = pd.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id).collect();
        let thrust = player_refs.contains(&109970);
        let r = e.rand100();
        let facing = in_front(&a, tf.translation, ptf.translation, 90.0) && in_front(pa, ptf.translation, tf.translation, 90.0);
        // Common_Parry (common_common_func_NTC.lua) for enemies whose script uses it.
        if let Some((guard_mult, step_prob, step_type, rush_anim)) = combat.foe.common_parry {
            let step = if step_type == 1 { "a000_005201" } else { "a000_005211" };
            let choice = if facing && dist <= PC_ATTACK_DIST_STAND {
                if player_refs.contains(&109990) {
                    Some(rush_anim) // rush attack: EndureAttack(rushAnim)
                } else if thrust {
                    match rank {
                        0 => Some("a000_003101"),
                        1 if r <= 50 => Some("a000_003101"),
                        _ => None,
                    }
                } else if player_refs.contains(&109980) && step_type != -1 && rank == 0 {
                    Some(step)
                } else if r <= e.guard_count() * guard_mult {
                    Some("a000_003101")
                } else {
                    Some("a000_003100")
                }
            } else if facing && dist <= PC_ATTACK_DIST_STAND + 1.0 && step_type != -1 && e.rand100() <= step_prob {
                Some(step) // just out of reach: back-step away
            } else {
                None
            };
            if let Some(anim) = choice {
                let label = match anim {
                    "a000_003101" => "Deflect(3101)",
                    "a000_003100" => "Guard(3100)",
                    _ => "Parry",
                };
                a.play(label, anim);
                a.yaw = f32::atan2(-(ptf.translation - tf.translation).x, -(ptf.translation - tf.translation).z);
                a.move_vel = Vec3::ZERO;
                e.interrupt();
            }
            continue;
        }
        if !(facing && dist <= PC_ATTACK_DIST_STAND) {
            continue;
        }
        let choice = if thrust {
            // Rank 0 always deflects a thrust; rank 1 does half the time.
            if rank == 0 || (rank == 1 && r <= 50) { Some("a000_003101") } else { None }
        } else if player_refs.contains(&109980) {
            Some("a000_003100")
        } else if r <= e.guard_count() * 50 {
            Some("a000_003101")
        } else {
            Some("a000_003100")
        };
        if let Some(anim) = choice {
            let label = if anim.ends_with("101") { "Deflect(3101)" } else { "Guard(3100)" };
            a.play(label, anim);
            a.yaw = f32::atan2(-(ptf.translation - tf.translation).x, -(ptf.translation - tf.translation).z);
            a.move_vel = Vec3::ZERO;
            e.interrupt();
        }
    }
}

/// Idle / locomotion states: any AI action may start.
fn is_free_state(state: &str) -> bool {
    matches!(
        state,
        "IdleBattle" | "WalkFrontBattle" | "WalkBackBattle" | "WalkLeftBattle" | "WalkRightBattle" | "RunFrontBattle" | "Approach" | "StandIdle"
    )
}

#[allow(clippy::too_many_arguments)]
fn think(
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform), Without<Player>>,
    mut brains: NonSendMut<Brains>,
    mut log: Option<ResMut<crate::hud::CombatLog>>,
    mut perilous: Option<ResMut<crate::hud::Perilous>>,
    mut debug: Option<ResMut<EnemyDebug>>,
) {
    let dt = time.delta_secs();
    let d = &combat.enemy;
    let (pa, ptf) = *player;
    let npc = combat.param("NpcParam", combat.foe.npc_row);
    let turn_rate = npc["turnVellocity"].as_f64().unwrap_or(135.0) as f32;
    for (entity, mut e, mut a, mut tf) in &mut q {
        let to_player = (ptf.translation - tf.translation).with_y(0.0);
        let dist = to_player.length();
        let want_yaw = f32::atan2(-to_player.x, -to_player.z);
        match e.mode {
            Mode::Dead(t) => {
                // ThrowDef reaction -> its death anim on flag 69 (or when it ends).
                if let Some(death) = e.throw_death {
                    let ended = a.t >= d.length(&a.anim);
                    if (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_THROW_TYPE5)) || ended {
                        e.throw_death = None;
                        a.play_state(d, &format!("ThrowDefDeath{death}"));
                        e.mode = Mode::Dead(d.length(&a.anim) + 2.0);
                    }
                    continue;
                }
                if t - dt <= 0.0 {
                    let (hp, _, _, _) = npc_stats(&combat);
                    a.hp = hp;
                    a.posture = 0.0;
                    tf.translation = Vec3::new(0.0, tf.translation.y, -6.0);
                    e.mode = Mode::Ai;
                    e.cooldown = 2.0;
                    e.interrupt();
                    a.procedural("Approach");
                } else {
                    e.mode = Mode::Dead(t - dt);
                }
                continue;
            }
            Mode::Broken(t) => {
                if t - dt <= 0.0 {
                    // No restore: the collapse SpEffects (220420 / 5359 / 220500) keep
                    // staminaRecoverSpeedRate 1.0 and no c1020 TAE has the ratio-restore event 961
                    // (FUN_140bd62d0), so posture simply regenerated through the collapse
                    // (FUN_140a04850 runs every frame) and the enemy keeps what it recovered.
                    // Emptied vitality: back at the brink, the next hit reopens the deathblow.
                    a.hp = a.hp.max(1.0);
                    e.mode = Mode::Ai;
                    e.cooldown = 0.5;
                    e.interrupt();
                } else {
                    e.mode = Mode::Broken(t - dt);
                }
                continue;
            }
            Mode::Ai => {}
        }

        let len = d.length(&a.anim);
        let ended = !a.anim.is_empty() && a.t >= len;
        let free = a.anim.is_empty() || ended || is_free_state(&a.state);
        // Perilous warning (red kanji) when the attack's TAE fires it.
        if let Some(kind) = d.perilous_warning(&a.anim, a.prev_t, a.t) {
            if let Some(p) = perilous.as_mut() {
                p.raise(kind);
            }
            if let Some(log) = log.as_mut() {
                let what = match kind {
                    980 => "sweep - jump",
                    982 => "thrust - deflect or mikiri (step in)",
                    983 => "grab - dodge",
                    _ => "unblockable",
                };
                log.push(format!("危 perilous {what}"), Color::srgb(1.0, 0.2, 0.1));
            }
        }
        // Track the player while the TAE allows turning (SetTurnSpeed), like NPC homing, except
        // inside TAE 703 FixedRotationDirection (each swing's active frames, e.g. 3000 f22-36).
        let fixed = !a.anim.is_empty()
            && d.events_at(&a.anim, a.t).any(|e| e.kind == 703 && e.args.get("IsEnable").and_then(|v| v.as_bool()).unwrap_or(false));
        if e.cur_ez.is_some() && !ended && !fixed {
            if let Some(speed) = d.turn_speed(&a.anim, a.t, true) {
                a.yaw = turn_toward(a.yaw, want_yaw, speed.to_radians() * dt);
            }
        }

        // Debug-menu drills: perilous-only / one repeated attack / standing still.
        if let Some(dbg) = debug.as_deref_mut() {
            if matches!(dbg.mode, AiMode::Perilous | AiMode::Repeat | AiMode::Idle) {
                drill(&mut e, &mut a, d, &config, dbg, dist, want_yaw, dt, free, ended);
                continue;
            }
        }
        // Passive (deflect practice), or the fallback when the AI scripts are missing.
        if !e.aggressive || brains.failed {
            passive_or_fallback(&mut e, &mut a, d, &config, dist, want_yaw, dt, free, ended);
            continue;
        }

        if !brains.map.contains_key(&entity) {
            let dir = crate::paths::root().join("extracted/ai_src");
            let radius = npc["hitRadius"].as_f64().unwrap_or(0.5) as f32;
            match Brain::load(&dir, combat.foe.battle_goal, combat.foe.think_id, radius, e.rng) {
                Ok(b) => {
                    for l in b.log.iter().filter(|l| !l.starts_with("stub:")) {
                        warn!("AI: {l}");
                    }
                    brains.map.insert(entity, b);
                }
                Err(err) => {
                    warn!("AI scripts unavailable ({err}); using fallback combos");
                    brains.failed = true;
                    continue;
                }
            }
        }

        // World snapshot for the brain.
        let fwd = a.forward();
        let right = fwd.cross(Vec3::Y);
        let dir = to_player.normalize_or_zero();
        let mut sp_self: HashSet<i64> = a.resident.iter().copied().collect();
        sp_self.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        sp_self.extend(e.clash.iter().map(|c| c.0));
        sp_self.insert(SP_BATTLE_STATE);
        let sp_target: HashSet<i64> = combat.player.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id).collect();
        let sp_new: Vec<i64> = sp_self.iter().chain(sp_target.iter()).filter(|id| !e.prev_sp.contains(*id)).copied().collect();
        e.prev_sp = sp_self.iter().chain(sp_target.iter()).copied().collect();
        let playing_ez = e.cur_ez.filter(|ez| format!("a000_{ez:06}") == a.anim);
        let snap = Snapshot {
            dist,
            angle: dir.dot(right).atan2(dir.dot(fwd)).to_degrees(),
            bearing: (-to_player.x).atan2(-to_player.z).to_degrees(),
            target_guard: pa.state.contains("Guard"),
            sp_self,
            sp_target,
            sp_new,
            hp_rate: a.hp / a.hp_max,
            target_hp_rate: pa.hp / pa.hp_max,
            sp: a.posture_max - a.posture,
            sp_rate: (1.0 - a.posture / a.posture_max).max(0.0),
            free,
            flags: AI_CANCEL_FLAGS.iter().copied().filter(|&f| !a.anim.is_empty() && d.flag(&a.anim, a.t, f)).collect(),
            ez: playing_ez,
            t: a.t,
            anim_done: ended,
            ez_started: e.ez_started.take(),
            ez_failed: e.ez_failed.take(),
            interrupted: std::mem::take(&mut e.interrupted),
            parry_timing: std::mem::take(&mut e.parry_timing),
            guard_count: e.guard_count(),
            target_use_item: pa.state.starts_with("ItemGourd") && pa.prev_t == 0.0 && pa.t > 0.0,
        };
        let brain = brains.map.get_mut(&entity).unwrap();
        let cmd = brain.tick(&snap, dt);
        for l in brain.log.drain(..) {
            if !l.starts_with("stub:") {
                warn!("AI: {l}");
            }
        }
        e.ai_desc = brain.describe();

        // Carry out the request.
        let move_ok = free || d.flag(&a.anim, a.t, FLAG_AI_MOVE);
        if let Some(ez) = cmd.anim {
            let key = format!("a000_{ez:06}");
            if d.anim(&key).is_some() {
                a.play(&format!("Ez{ez}"), &key);
                a.move_vel = Vec3::ZERO;
                e.cur_ez = Some(ez);
                e.ez_started = Some(ez);
                if let Some(log) = log.as_mut() {
                    log.push(&format!("enemy {ez}: {}", e.ai_desc), Color::srgb(1.0, 0.7, 0.6));
                }
            } else {
                e.ez_failed = Some(ez);
            }
        } else if let (Some(mv), true) = (cmd.mv, move_ok) {
            let state = match (mv, cmd.walk) {
                ('F', false) => "RunFrontBattle",
                ('F', true) => "WalkFrontBattle",
                ('B', _) => "WalkBackBattle",
                ('L', _) => "WalkLeftBattle",
                _ => "WalkRightBattle",
            };
            if a.state != state || ended {
                a.play_state(d, state);
            }
            e.cur_ez = None;
        } else if free && (ended || a.state.starts_with("Walk") || a.state.starts_with("Run")) {
            a.procedural("IdleBattle");
            a.move_vel = Vec3::ZERO;
            e.cur_ez = None;
        }
        if (cmd.face || cmd.turn) && (free || cmd.mv.is_some()) {
            a.yaw = turn_toward(a.yaw, want_yaw, turn_rate.to_radians() * dt);
        }
    }
}

/// Debug drills: walk into range, then every `interval` s one attack - a random perilous one, or
/// the chosen one - played as the AI would request it.
#[allow(clippy::too_many_arguments)]
fn drill(e: &mut Enemy, a: &mut Actor, d: &crate::data::CharData, config: &GameConfig, dbg: &mut EnemyDebug, dist: f32, want_yaw: f32, dt: f32, free: bool, ended: bool) {
    if !free {
        return;
    }
    if ended {
        e.cur_ez = None;
    }
    if dbg.mode == AiMode::Idle {
        a.procedural("IdleBattle");
        a.move_vel = Vec3::ZERO;
        return;
    }
    a.yaw = turn_toward(a.yaw, want_yaw, std::f32::consts::PI * dt);
    if dist > config.enemy.attack_range {
        if a.state != "WalkFrontBattle" || ended {
            a.play_state(d, "WalkFrontBattle");
        }
        return;
    }
    a.procedural("IdleBattle");
    a.move_vel = Vec3::ZERO;
    dbg.timer -= dt;
    if dbg.timer > 0.0 {
        return;
    }
    let key = if dbg.mode == AiMode::Repeat && d.anim(&dbg.attack).is_some() {
        dbg.attack.clone()
    } else {
        let red: Vec<String> = attack_list(d).into_iter().filter(|(_, p)| p.is_some()).map(|(k, _)| k).collect();
        if red.is_empty() {
            return;
        }
        red[(e.rand() * red.len() as f32) as usize % red.len()].clone()
    };
    a.yaw = want_yaw;
    let ez = ez_of(&key);
    a.play(&format!("Ez{}", ez.unwrap_or(0)), &key);
    e.cur_ez = ez;
    e.fallback = None;
    dbg.timer = dbg.interval;
}

/// Passive practice partner (walks into range and waits), and the stand-in when
/// the AI scripts are missing (aggressive: c1020's combos from its AI acts).
#[allow(clippy::too_many_arguments)]
fn passive_or_fallback(e: &mut Enemy, a: &mut Actor, d: &crate::data::CharData, config: &GameConfig, dist: f32, want_yaw: f32, dt: f32, free: bool, ended: bool) {
    if let Some((combo, step)) = e.fallback {
        let chain_point = d.flag(&a.anim, a.t, FLAG_AI_COMBO) && a.t > 0.2;
        let next = step + 1;
        if (chain_point || ended) && next < COMBOS[combo].len() && dist < config.enemy.attack_range + 2.0 {
            a.play(&format!("Combo{combo}.{next}"), COMBOS[combo][next]);
            e.cur_ez = ez_of(COMBOS[combo][next]);
            e.fallback = Some((combo, next));
        } else if ended {
            e.fallback = None;
            e.cur_ez = None;
            let r = e.rand();
            e.cooldown = config.enemy.cooldown_min + r * (config.enemy.cooldown_max - config.enemy.cooldown_min);
        }
        return;
    }
    if !free {
        return;
    }
    if ended {
        e.cur_ez = None;
    }
    e.cooldown -= dt;
    a.yaw = turn_toward(a.yaw, want_yaw, std::f32::consts::PI * dt);
    if dist > config.enemy.attack_range {
        // Battle walk speed from WalkFrontBattle's root motion.
        if a.state != "WalkFrontBattle" || ended {
            a.play_state(d, "WalkFrontBattle");
        }
    } else {
        a.procedural("IdleBattle");
        a.move_vel = Vec3::ZERO;
        if e.cooldown <= 0.0 && e.aggressive {
            let combo = (e.rand() * COMBOS.len() as f32) as usize % COMBOS.len();
            a.yaw = want_yaw;
            a.play(&format!("Combo{combo}.0"), COMBOS[combo][0]);
            e.cur_ez = ez_of(COMBOS[combo][0]);
            e.fallback = Some((combo, 0));
        }
    }
}

fn toggle_aggression(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Enemy>, mut log: ResMut<crate::hud::CombatLog>) {
    if keys.just_pressed(KeyCode::KeyT) {
        for mut e in &mut q {
            e.aggressive = !e.aggressive;
            log.push(if e.aggressive { "enemy: AGGRESSIVE (real AI)" } else { "enemy: passive" }, Color::WHITE);
        }
    }
}
