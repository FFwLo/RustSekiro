//! The player (Wolf): input, and the action state machine ported from
//! Sekiro's c0000.hks. The HKS decides *what* to play; the TAE of the playing
//! animation decides *when* each input is accepted and which variant runs:
//!
//! - ChrActionFlag (TAE 0) cancel windows gate inputs: 115 attack, 117 guard,
//!   26/25 step, 119 jump, 11 move (the engine's env(1106) checks these).
//! - SpEffects (TAE 67) carry behaviour refs the HKS reads via env(3036, ref):
//!   203 deflect window, 208 release-attack, 212 guard chain, 214-218 combo
//!   routing, 221 hold-guard cancel, 207 guard can end, 15 step -> sprint.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{wrap_angle, Actor, ActorSet, ShownYaw, Side};
use crate::view::{LockOn, View};
use crate::config::GameConfig;
use crate::data::{CharData, Combat, FALL_TYPES, calc_correct};
use crate::enemy::Enemy;
use crate::combat::CombatLog;

#[derive(Component)]
pub struct Player {
    /// Plunge deathblow in flight: the posture-broken enemy Wolf is falling onto and the
    /// ThrowParam suffix of the landing (151 崩し落下1, 161 蹴り崩し1 out of a head-kick jump).
    plunge: Option<(Entity, i64)>,
    /// A launch homing on a character (ChrPhysicsVelocityChangeParam homingId other than the
    /// wire's: the jump arts' 10010 -> ChrPhysicsHomingParam 20000), until he lands.
    leap_homing: Option<LeapHoming>,
    /// Docked to a map edge: wall hug or ledge hang (docking.rs).
    pub(crate) dock: Option<crate::docking::Dock>,
    /// Wall jumps this flight (WALL_JUMP_COUNT_MAX 1) and the wall-contact side of the ready pose
    /// in progress (docking.rs).
    pub(crate) wall_jumps: u8,
    /// The direction (from Wolf) of the wall the last chained wall jump left.
    pub(crate) last_wall: Option<Vec3>,
    /// At a Sculptor's Idol (idol.rs): which, the clip phase, the menu's command, the checkpoint.
    pub(crate) seat: crate::seat::Seat,
    pub(crate) wall_contact: Option<char>,
    /// HKS MoveSpeedLevel: 0 idle, 1 walk, 2 run (converges at 3/s).
    pub(crate) speed_level: f32,
    /// The pad manipulator's stick gate and yaw request (`Turn`).
    pub(crate) turn: Turn,
    pub(crate) requests: HashMap<Action, f32>,
    /// State after the last decide; a different state at the next decide was set by combat.
    last_state: String,
    /// Healing Gourd uses left (config player.gourd_charges; goods 3000 maxNum 10).
    pub gourd: u32,
    /// Resurrection nodes left (config player.resurrections).
    pub resurrections: u32,
    /// Attack pressed on the death loop before its f0 refresh was read: revive once it is.
    revive_queued: bool,
    /// The resurrection choice is open (menu.rs's death screen shows ": Die" / ": Resurrect").
    pub revive_open: bool,
    /// Die chosen there (guard): no resurrection, the death screen fades out to the final death.
    pub gave_up: bool,
    /// After a resurrection the nodes left are locked until Wolf lands a deathblow (the game's
    /// rule; the user, 2026-10-18: "second ressurection should be locked away in game after you
    /// resurrect until you land a deathblow"). Resting and a reset unlock them too.
    pub revive_locked: bool,
    /// The worn items' maxHpRate product now applied to hp_max (items.rs `max_hp_rate`;
    /// Yashariku's Sugar 3411: 0.5), so it comes off again when they end.
    pub item_hp_rate: f32,
    /// The same for maxStaminaRate on posture_max (Yashariku's Sugar 3411: 0.5).
    pub item_posture_rate: f32,
    /// Instant SpEffects whose looks show once (status.rs `sp_bursts`): the gourd's heal 3000.
    pub sp_bursts: Vec<i64>,
    /// Resurrective Power gathered toward the next used resurrection node (drops.rs).
    pub res_power: u32,
    /// Rested at an idol: the refilled goods (Five-color Rice) are topped up (items.rs).
    pub refill_items: bool,
    /// Seconds in the death / revival-wait loop. Those clips are one frame long and loop, so
    /// their time wraps and their f0-4 TAE (110010 Pretending Dead, iframes) stays on; the revive
    /// choice and the timeout run on this instead of the clip time (round 27).
    death_wait: f32,
    /// Seconds since HP emptied (the exe's PlayerIns +0x2138 / +0x212c pair, `REVIVE_DELAY` /
    /// `REVIVE_WINDOW`); 0 while alive.
    dead_for: f32,
    /// The grab escape's meter while Grabbed (ThrowParam escHp, round 28).
    escape: Option<GrabEscape>,
    /// Attack auto-homing active (HKS g_autoAimFlag) and just started this decide.
    auto_aim: bool,
    auto_aim_fresh: bool,
    /// Spirit Emblems (prosthetic ammunition; config player.spirit_emblems).
    pub emblems: u32,
    /// SpEffects a TAE AddSpEffect put on for good (effectEndurance -1, e.g. the Todome's
    /// "recovery prohibited" 105051 / 150302-150332); they win their spCategory over a
    /// higher-priority one-shot recovery. effectEndurance -1 and saveCategory -1 (not saved, no
    /// event script clears them): they last until Wolf is rebuilt (death respawn, rest, load),
    /// which the fight reset stands in for.
    pub(crate) held: Vec<i64>,
    /// Aged and past W_StandAging: the form comes back as W_AgingIdle after a throw or a
    /// reaction (HKS 381), not with the onset again.
    aged_form: bool,
    /// Locked-on step tilt (HKS _set4DirStepTilt -> act 3025): root-motion yaw offset for the step.
    pub(crate) step_tilt: f32,
    /// A deathblow is in reach of the attack button (decide's throw check, read by hks_wolf).
    pub(crate) deathblow_ready: bool,
    /// The jump-over throw (崩し蹴りジャンプ) is in reach of the jump button.
    pub(crate) jump_throw_ready: bool,
    /// A plunge deathblow is in reach of the air attack.
    pub(crate) plunge_ready: bool,
    /// The jump's root tilt (HKS act 3025 from _setPositioningJumpTilt / _set4DirJumpTilt: the
    /// stick's angle minus its sector's centre), radians, on the ground jumps and the kick jump;
    /// it turns their root motion and TAE 920 launch (round 76: they flew along the sector's
    /// fixed line, up to 22.5 / 45 deg off the stick).
    pub(crate) jump_tilt: f32,
    /// Deathblow start throw in progress: the enemy, the main ThrowParam row suffix and whether
    /// Wolf is behind it (deathblow_check). Round 37: `behind` was re-derived from the suffix
    /// (111 / 21) and missed the boss rows (Todome 181, HU 611 / 619, Gyoubu's 119), so the boss
    /// was turned 180 degrees onto Wolf.
    throw_start: Option<(Entity, i64, bool)>,
    /// The enemy of the running deathblow, until it dies (the kill follow-up anim).
    pub(crate) throw_target: Option<Entity>,
    /// Equipped prosthetic slot (index into config player.prosthetics; Z cycles it).
    pub tool_slot: usize,
    /// The quick-item slot in hand (config player.quick_items; items.rs).
    pub quick_slot: usize,
    /// A quick item being used: its goods and use state, until the state ends.
    pub item_use: Option<(i64, String)>,
    /// TAE 922 ChrPhysicsVelosityScale in progress (`VelScale`).
    vel_scale: Option<VelScale>,
    /// A switch is in progress: when its arm anim ends the new tool unfolds (SubWeaponExpand).
    expand_pending: bool,
    /// The tool group of the last prosthetic use (HKS g_beforeSubAttackType): a combo follow-up
    /// needs the same tool.
    pub sub_cat_before: i64,
    /// HKS g_enableSpAttaclkJump: the art had its emblems when pressed (env(3035) at
    /// BEH_A_GROUND_SP_ATTACK / BEH_A_AIR_SP_ATTACK); gates Shadowrush's hit jump.
    pub art_enable_jump: bool,
    /// SpEffects on Wolf with seconds left (TAE 940 BehaviorParam_AddSpEffect, e.g. Divine
    /// Abduction's 107700: ref 309 USED_TEKIMAWASHI for 3 s); read as env(3036, ref).
    pub timed: Vec<(i64, f32)>,
    /// Grappling Hook move in progress (wire.rs).
    pub(crate) wire: Option<crate::wire::Wire>,
    /// A ridden wire point Wolf landed on (its landing spot): stands as a floor while he is on
    /// it. gap: the objects' own collision (branches, roofs) is not in the map terrain.
    pub(crate) perch: Option<Vec3>,
    /// Attack pressed during the Grappling Hook Attack's landing (the Todome input).
    wire_todome: bool,
    /// The fall top (physics +0x854): the height of the last grounded frame or SetNoGravity hold,
    /// kept through the flight, not the flight's highest point (copied every grounded update,
    ///; FUN_140b97880). The landing reads the fall height from it (env(224),
    /// `FALL_HEIGHT_LONG_STIFF_LAND`, the fall damage).
    pub(crate) air_top: f32,
    /// Wolf's centre at the last fall check (the path tested against the fall-return walls).
    fall_from: Option<Vec3>,
    /// The ground Wolf stood on these last `SAFE_RING` ticks (not water), oldest first: a
    /// fall-return death puts him back at the oldest.
    pub(crate) safe_ring: std::collections::VecDeque<Vec3>,
    /// Swimming or diving (swim.rs).
    pub(crate) swim: Option<crate::swim::Swim>,
    /// Grounded, the horizontal move of the last tick (m/s) and where it was measured from: the
    /// air velocity a walk-off starts with (physics +0x160).
    ground_vel: Vec3,
    ground_pos: Vec3,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Attack,
    Guard,
    Step,
    Jump,
    UseItem,
    /// Combat art (attack + guard together): Whirlwind Slash.
    CombatArt,
    /// Shinobi Prosthetic (Sekiro's R2): Loaded Shuriken.
    Prosthetic,
    /// Crouch toggle (ACTION_ARM_CROUCH, Sekiro's L3).
    Crouch,
    /// Next prosthetic tool (ACTION_ARM_CHANGE_WEAPON_L).
    SwitchTool,
    /// Grappling Hook (ACTION_ARM_WIRE_SHOOT; wire.rs).
    Grapple,
    /// Next quick item (gameAction 13 / 14: C here, the pad's d-pad down; items.rs).
    NextItem,
    /// Talk / examine / wall hug / ledge hang (ACTION_ARM_WALL_HANG, key guide Kc@20; docking.rs).
    Interact,
}

/// Resurrective Power that lights one used resurrection node again (drops, HUD).
pub(crate) const RES_POWER_NODE: u32 = 100;

pub const CAPSULE_RADIUS: f32 = 0.4;
/// How far from a wire perch's landing spot Wolf still stands on it (gap: the object's own
/// collision is not loaded; about the branch / roof-tip's reach).
const PERCH_RADIUS: f32 = 0.6;
const CAPSULE_MIDDLE: f32 = 1.0;
pub const CAPSULE_HALF_HEIGHT: f32 = CAPSULE_RADIUS + CAPSULE_MIDDLE / 2.0;

// ChrActionFlag types (TAE event 0) that open input windows.
/// ChrActionFlag 69 ThrowType5: the grabbed defender's throw-death point (exe env 276).
pub const FLAG_THROW_TYPE5: i64 = 69;
/// ChrActionFlag 70 on the grabber: the escape check pulse (exe env 277, flag bit 1).
pub const FLAG_THROW_ESCAPE_CHECK: i64 = 70;
pub const FLAG_ACCEPT_ATTACK: i64 = 115;
pub const FLAG_ACCEPT_GUARD: i64 = 117;
pub const FLAG_ACCEPT_STEP: i64 = 26;
pub const FLAG_ACCEPT_STEP_ALT: i64 = 25;
pub const FLAG_ACCEPT_JUMP: i64 = 119;
/// ChrActionFlag 67 "ThrowStart2" on a defender's break pose (ThrowDef12100 frames 0-45).
const FLAG_THROW_START2: i64 = 67;
/// EquipParamGoods row of the Healing Gourd.
pub(crate) const GOURD_GOODS: i64 = 3000;
/// PlayerIns +0x2134 (FUN_140a217a0): death -> the resurrection choice opens.
const REVIVE_DELAY: f32 = 1.8;
/// PlayerIns +0x2128 (15.0): how long the choice stays open.
const REVIVE_WINDOW: f32 = 15.0;
/// ChrActionFlag 31 "AnimCancelEnd_UseItem" / 90 "Limit Move Speed To Walk".
const FLAG_ACCEPT_ITEM: i64 = 31;
/// ChrActionFlag 137 "AnimCancelEnd_R2_Prosthetic".
const FLAG_ACCEPT_PROSTHETIC: i64 = 137;
const FLAG_LIMIT_WALK: i64 = 90;
/// SpEffect 3000 (goods 3000, the Healing Gourd): changeHpEstusFlaskRate -40 -> 40 % HP.
const GOURD_SPEFFECT: &str = "3000";

/// The equipped combat art's first anim: EquipParamWeapon[config player.combat_art]
/// spAtkcategory -> group a<cat>, GroundSpecialAttackCombo1 = <group>_316000 (or 316001); the jump
/// arts (107 / 110) have none: their GroundSpecialAttackJumpReady 316700.
fn art_anim(d: &CharData, combat: &Combat, config: &GameConfig) -> Option<String> {
    let cat = combat.param("EquipParamWeapon", config.player.combat_art)["spAtkcategory"].as_i64()?;
    [316000, 316001, 316700].iter().map(|id| format!("a{cat:03}_{id}")).find(|k| d.anim(k).is_some_and(|a| a.duration.is_some()))
}
/// A state's anim id: the state table's, else for SprintSpecialAttackJumpReady (in c0000.hkx, but
/// CMSG gives it no animId) its NoResource twin's 316770.
fn art_state_id(d: &CharData, state: &str) -> Option<String> {
    match d.states.get(state) {
        Some(k) => k.rsplit('_').next().map(str::to_string),
        None => match state {
            "SprintSpecialAttackJumpReady" => Some("316770".to_string()),
            _ => None,
        },
    }
}

/// A state's anim in the art's group: the state's id under a<cat> (a107_316710 ...).
fn art_key(d: &CharData, cat: i64, state: &str) -> Option<String> {
    let id = art_state_id(d, state)?;
    let key = format!("a{cat:03}_{id}");
    d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some(key)
}




/// ChrPhysicsHomingParam's target point on a character: its origin (targetBaseDmyPolyId -1) plus
/// targetOffset in its own space (model forward is -Z, so 20000's (0, 0, -1) is 1 m in front of
/// it). gap: the offset's space is taken from the plunge's dummy-space use (200000300), not
/// traced for the origin case.
fn leap_target(t: &Transform, offset: Vec3) -> Vec3 {
    t.translation + t.rotation * offset
}

/// A launch homing on a character (exe FUN_140bb3580 / FUN_140bb3220, ChrPhysicsHomingParam).
#[derive(Clone, Copy, Debug)]
struct LeapHoming {
    target: Entity,
    row: crate::wire::HomingRow,
    /// fallCorrectionChaseMaxVelocityRate x the planar speed when the homing starts.
    max_speed: f32,
}

/// The horizontal speed that lands a launch (vertical speed `vy`, gravity `g` < 0) on a point
/// `dist` away and `dy` above (exe FUN_140bb3b70, no horizontal acceleration): the later root of
/// dy = vy t + g t^2 / 2, then dist / t. None when the point can't be reached.
fn arrival_speed(vy: f32, dist: f32, dy: f32, g: f32) -> Option<f32> {
    let disc = vy * vy + 2.0 * g * dy;
    if g >= 0.0 || disc < 0.0 {
        return None;
    }
    let t = (-vy - disc.sqrt()) / g;
    (t > 0.0).then(|| dist / t)
}

impl LeapHoming {
    /// initialVelocityCorrection 2 (FUN_140bb3580's second branch): the vertical speed stays the
    /// row's; the horizontal one turns to the target and slows to land on it - but never speeds
    /// up past the row's (needed > current: no correction, the row's launch as it is).
    /// Correction 1 (the wire's arc) is wire.rs's.
    fn launch(a: &mut Actor, pos: Vec3, target: Vec3, g: f32, e: Entity, row: crate::wire::HomingRow) -> Option<Self> {
        if row.initial_correction == 2 {
            let to = (target - pos).with_y(0.0);
            let speed = a.air_base.length();
            if to.length() <= 0.0 {
                a.air_base = Vec3::ZERO;
            } else if let Some(need) = arrival_speed(a.vel_y, to.length(), target.y - pos.y, g).filter(|n| *n > 0.0 && *n <= speed) {
                a.air_base = to.normalize() * need;
                a.yaw = yaw_of(to);
            }
            a.move_vel = a.air_base;
        }
        row.chase.then_some(Self { target: e, row, max_speed: a.air_base.length() * row.chase_speed_rate })
    }

    /// fallCorrectionChase (FUN_140bb3220): each frame the horizontal velocity moves toward the
    /// one that lands on the (moving) target point, by at most chaseMaxAcceleration * dt; without
    /// a solution it accelerates straight at it. Capped at `max_speed`. gap: where the exe
    /// applies the velocity-rate cap is not traced (taken on the result).
    fn chase(&self, a: &mut Actor, pos: Vec3, target: Vec3, g: f32, dt: f32) {
        let to = (target - pos).with_y(0.0);
        let dist = to.length();
        if dist <= 1e-4 {
            return;
        }
        let dir = to / dist;
        let step = self.row.chase_accel * dt;
        a.air_base += match arrival_speed(a.vel_y, dist, target.y - pos.y, g) {
            Some(need) => (dir * need - a.air_base).clamp_length_max(step),
            None => dir * step,
        };
        a.air_base = a.air_base.clamp_length_max(self.max_speed);
    }
}


/// Knowledge of Medicine (SkillParam 170 / 171 / 600-602, each 150200 + 150210): every learned one
/// adds 150210's accumuVal 1; the stacks climb 150200's accumuOverFireId chain (LV2 150201 at
/// accumuOverVal 2, LV3 150202 at 3, ...) and the level's changeHpEstusFlaskCorrectRate (1.1 - 1.5)
/// scales the gourd's heal (sim test `latent_skill_rates`: 1.1, 1.2).
/// The gourd's HP (FUN_140c014f0 for a fired row's changeHpEstusFlaskRate): the rate % times
/// the worn changeHpEstusFlaskCorrectRate product, held within 100 %, then whole points of max
/// HP: (int)(max * rate * 0.01). Emulator (speffect_ticks gourd_3000): Wolf 1120 with Knowledge
/// of Medicine LV5 (1.5) heals 672. Before: the float 672.0 (whole here, but a fraction for other
/// max HPs).
pub(crate) fn estus_heal(hp_max: f32, rate: f32, correct: f32) -> f32 {
    (hp_max * (rate * correct).clamp(-100.0, 100.0) * 0.01).trunc()
}

pub fn medicine_rate(combat: &Combat, config: &GameConfig) -> f32 {
    medicine_row(combat, config).map_or(1.0, |row| row["changeHpEstusFlaskCorrectRate"].as_f64().unwrap_or(1.0) as f32)
}

/// The Knowledge of Medicine level row the learned skills climb to (150200 LV1 .. 150204 LV5).
fn medicine_row<'a>(combat: &'a Combat, config: &'a GameConfig) -> Option<&'a serde_json::Value> {
    let effects: Vec<&serde_json::Value> = crate::combat::skill_sp_effects(combat, config).collect();
    let stacks: i64 = effects.iter().filter_map(|se| se["accumuVal"].as_i64()).filter(|v| *v > 0).sum();
    let mut row = effects.into_iter().find(|se| se["changeHpEstusFlaskCorrectRate"].as_f64().is_some_and(|r| r != 1.0))?;
    for _ in 0..8 {
        let (next, need) = (row["accumuOverFireId"].as_i64().unwrap_or(-1), row["accumuOverVal"].as_i64().unwrap_or(0));
        let up = combat.param("SpEffectParam", next);
        if next <= 0 || need <= 0 || stacks < need || up.is_null() {
            break;
        }
        row = up;
    }
    Some(row)
}

/// SpEffect 8610 "Hard mode" (the Demon Bell / Charmless): def{Poison,Blood}ResistDamageRate 1.5,
/// defDiseaseResistDamageRate 1.2 on Wolf (`player.hard_mode`).
pub const HARD_MODE_SP: i64 = 8610;

/// The rows Wolf wears for the SpEffect list's rules (status.rs: build-up rates and disables, the
/// regens' changeHpEstusFlaskCorrectRate, the buff-duration skill's extendLifeRate): his actor's
/// (`combat::worn_sp`), the learned latent skills' SpEffects with Knowledge of Medicine at its
/// level (150200 -> 150204, the accumulation 150210 left out), and Hard mode's 8610. The exe's
/// list on the snapshot's Wolf holds 8610, 150204, 150600 the same way (speffect_ticks).
pub(crate) fn wolf_worn(combat: &Combat, config: &GameConfig, a: &Actor) -> Vec<i64> {
    let medicine = medicine_row(combat, config).and_then(|r| r["id"].as_i64());
    let mut worn = crate::combat::worn_sp(a, &[]);
    for &skill in &config.player.skills {
        let row = combat.param("SkillParam", skill);
        for id in ["spEffect1", "spEffect2", "spEffect3"].into_iter().filter_map(|f| row[f].as_i64().filter(|&i| i > 0)) {
            let accumulates = combat.param("SpEffectParam", id)["accumuVal"].as_i64().is_some_and(|v| v > 0);
            let id = if medicine.is_some() && combat.param("SpEffectParam", id)["changeHpEstusFlaskCorrectRate"].as_f64().is_some_and(|r| r != 1.0) { medicine.unwrap() } else { id };
            if !accumulates && !worn.contains(&id) {
                worn.push(id);
            }
        }
    }
    if config.player.hard_mode {
        worn.push(HARD_MODE_SP);
    }
    worn
}

/// The equipped art's Spirit Emblem cost (EquipParamWeapon resourceItemA: Dragon Flash 5400 2,
/// Ashina Cross 5500 2, Mortal Draw 5700 3, ...), paid by its wepCost behaviours (prosthetic.rs).
pub fn art_cost(combat: &Combat, config: &GameConfig) -> u32 {
    combat.param("EquipParamWeapon", config.player.combat_art)["resourceItemA"].as_u64().unwrap_or(0) as u32
}

/// The equipped art's spAtkcategory and the unlock ref of its resident SpEffect (0 = none).
fn art_kind(combat: &Combat, config: &GameConfig) -> Option<(i64, i64)> {
    let row = combat.param("EquipParamWeapon", config.player.combat_art);
    let cat = row["spAtkcategory"].as_i64().filter(|&c| c > 0)?;
    let unlock = ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"]
        .iter()
        .filter_map(|f| row[*f].as_i64().filter(|&id| id > 0))
        .map(|id| combat.param("SpEffectParam", id)["behaviorRefId"].as_i64().unwrap_or(0))
        .find(|&r| r > 0)
        .unwrap_or(0);
    Some((cat, unlock))
}

/// ChrActionFlag 27 "SetNoGravity".
pub(crate) const FLAG_NO_GRAVITY: i64 = 27;

/// The fall module's launch timer (FUN_140bb1670, physics +0x1c0 +0x180), set by a TAE 920 launch
/// in the update it enters, before that update's air step: the step reads 0.251 there (0.25 s and
/// a margin), so it runs on 16 updates, the launch's own included (air_ng_launch.json ng_rise).
pub(crate) const LAUNCH_TIME: f32 = 0.251;

/// SetNoGravity holds the body this update: the flag dispatched on the clip's step (the exe's
/// window, `Actor::step`) and no launch timer running (FUN_140bccd50 / FUN_140bb14d0).
pub(crate) fn no_gravity_hold(d: &CharData, a: &Actor) -> bool {
    !a.anim.is_empty() && a.launch_left <= 0.0 && d.flag(&a.anim, a.step(), FLAG_NO_GRAVITY)
}

/// One airborne update of the physics step FUN_140bccd50: (held, air velocity, vertical speed,
/// the gravity it ran). A SetNoGravity `flag` with no launch timer left holds (+0x160 zeroed, no
/// air step); the update after a hold is a ground step (the body read grounded: world gravity
/// only, -0.163 m/s) unless a launch came in; otherwise `air_step`, the stick only for a controllable fall type.
/// air_ng_launch.json: ng_launch / ng_rise / ng_timer / ng_fall (test emu_air).
#[allow(clippy::too_many_arguments)]
pub(crate) fn air_frame(flag: bool, launch_left: f32, was_held: bool, h: Vec3, vy: f32, stick: Vec3, ft: crate::data::FallType, dt: f32) -> (bool, Vec3, f32, f32) {
    if flag && launch_left <= 0.0 {
        return (true, Vec3::ZERO, 0.0, 0.0);
    }
    // Not when a launch (TAE 920) came in after the hold: the launch velocity takes the air step
    // (the head kick's relaunch at the end of its [0, 0.2] flag).
    if was_held && launch_left <= 0.0 {
        return (false, h, -crate::data::WORLD_GRAVITY * dt, -crate::data::WORLD_GRAVITY);
    }
    let (h, vy) = air_step(h, vy, if ft.controllable { stick } else { Vec3::ZERO }, ft, dt);
    (false, h, vy, ft.gravity())
}

/// One air step of the fall module (FUN_140bccd50 -> FUN_140bacc10 / FUN_140bac120, physics
/// +0x160): gravity (world 9.8 + the fall type's verticalAcceleration, fall +0x7c / +0x1c4) on the
/// vertical speed; the horizontal velocity decays along its direction by HorizontalAcceleration
/// (fall +0x78; left alone once slower than one step), then the world stick (FUN_1409efea0 +0xc0)
/// adds StickAcceleration * stick * dt (fall +0x80) split along the velocity (or the stick when
/// still): the across part always steers, the along part brakes in full but speeds up only to
/// StickAccelerationMaxVelocity (+0x84). Checked frame by frame against the exe (test
/// air_steps_match_the_exe). The position takes the mean of the old and new velocity
/// (FUN_140bccd50's half step; actor.rs `fall`).
pub(crate) fn air_step(h: Vec3, vy: f32, stick: Vec3, ft: crate::data::FallType, dt: f32) -> (Vec3, f32) {
    let mut h = h;
    let speed = h.length();
    let decel = ft.h_accel * dt;
    if speed > 0.0 && (decel >= 0.0 || -decel <= speed) {
        h += h / speed * decel;
    }
    if stick != Vec3::ZERO {
        let acc = stick * ft.stick_accel * dt;
        let speed = h.length();
        let dir = if speed > 0.0 { h / speed } else { stick.normalize() };
        let along = dir.dot(acc);
        let par = dir * along;
        h += if along <= 0.0 { par } else { dir * along.min((ft.stick_max_vel - speed).max(0.0)) };
        h += acc - par;
    }
    (h, vy + ft.gravity() * dt)
}
pub const FLAG_ACCEPT_MOVE: i64 = 11;
pub const FLAG_SHIELD_BLOCK: i64 = 3;
pub const FLAG_DISABLE_TURN: i64 = 7;

// SP_EF_REF_* behaviour refs (c0000_define.hks).
pub const REF_ENABLE_SPRINT_ACTION: i64 = 1;
pub const REF_JUST_GUARD: i64 = 203;
pub const REF_GUARD_COMBO: i64 = 212;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_RELEASE: letting go of attack here plays the art's Release
/// (a106_316010 f6-18; live recording: let go at f2-3 -> a106_316110 at f6.5).
pub const REF_SP_ATK_RELEASE: i64 = 222;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_HOLD_RELEASE: a hold art (104) can end its hold here
/// (a104_316500 HoldStart f21-33; the air hold a104_316220 f0-93).
pub const REF_SP_ATK_HOLD_RELEASE: i64 = 230;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO: the art's follow-up window (SpEffect 100252).
#[cfg(test)]
pub const REF_ENABLE_SP_ATK_COMBO: i64 = 223;
/// SP_EF_REF_TAE_ENABLE_ADD_ACTION_INPUT_GUARD_CANCEL.
pub const REF_ADD_GUARD_CANCEL: i64 = 412;
/// SP_EF_REF_TAE_ENABLE_ADD_ACTION_INPUT_GUARD: raised by the additive deflect (a000_299060 f0-9).
pub const REF_ADD_INPUT_GUARD: i64 = 411;
/// SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION: landing keeps the attack going (LandAir* anims).
pub const REF_ORIGINAL_LAND_ACTION: i64 = 201;
/// Fall damage starts here (m): DAT_143b01948 = 20.0, read by FUN_140b97b30 (5 m while aged:
/// `FALL_DAMAGE_FROM_AGED`).
pub(crate) const FALL_DAMAGE_MIN: f32 = 20.0;
/// The share of max HP a lethal landing (fall height >= fallDeathDist) costs before the return
/// (DAT_143b01958 = 0.4 in FUN_140b97b30).
pub(crate) const FALL_RETURN_COST: f32 = 0.4;

/// HP a landing from `drop` metres costs, and whether it is the lethal one (the return death),
/// None under FALL_DAMAGE_MIN, for a full-HP Wolf on the default ground (`dist` = fallDeathDist):
/// from 20 m (int)(maxHP x h / 100), from `dist` (int)(maxHP x 0.4) and the return. The whole
/// rule (HP left, floors, aging, stateInfo 47) is `fall_landing`.
pub(crate) fn fall_hp_loss(hp_max: f32, drop: f32, dist: f32) -> Option<(f32, bool)> {
    if drop < FALL_DAMAGE_MIN {
        return None;
    }
    if drop >= dist {
        return Some(((hp_max * FALL_RETURN_COST).floor(), true));
    }
    // The factor is rounded to f32 before the HP product: 20 m -> 0.19999999 -> 223 of 1120.
    let factor = drop.min(100.0) * 0.01;
    Some(((hp_max * factor).floor(), false))
}

/// What a landing does to Wolf's HP (`fall_landing`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FallLanding {
    /// No fall damage.
    None,
    /// HP after a fall short of the kill (never under 1: the player keeps 1 HP).
    Hurt(f32),
    /// The fall-return death (LandDeadReturnStart, damage type 15): HP after its cost.
    Return(f32),
    /// HP was at or under the return's cost: HP 0 and the ordinary death on the spot.
    Death,
}

/// The landing's fall damage for the player, exe FUN_140b97a30 -> FUN_140b97b30 on the land
/// frame (emulated in extracted/emu/air_falldmg.json, test emu_air
/// `the_fall_landing_matches_the_exe`), fall height = physics +0x854 (the fall top) - y:
/// - a HitMtrlParam disableFallDamage floor (row 40; FUN_140ba8cb0 reads it from physics +0x940):
///   nothing, aged or not;
/// - the kill test FUN_14084e5d0: aged (SpEffect stateInfo 0x155 = 341 worn) from 5 m
///   (DAT_143b01950) on any other floor, deep water too; else from 20 m and the floor's
///   fallDeathDist (40, 80 on the deep-water rows 22 / 36 / 38; no floor: 40);
/// - the factor FUN_140843660: 1 for a kill; else 0 under 20 m (5 m aged, DAT_143b01954), else
///   CalcCorrectGraph 210 (h) / 100 x the floor's fallDamageRate (0.5 on 22 / 36 / 38) x
///   FUN_1408433f0's other factors: graphs 211 / 212 / 213 of the player's game data (flat 100 %
///   on every input, so 1, while game data +0x8fc is above 0: 50 on the snapshot) and the
///   product of the worn SpEffects' fallDamageRate (0 only on 4050 / 4053, stateInfo 47 too);
/// - stateInfo 47 worn (SpEffects 640 / 4050 / 4053): the damage is zeroed before the kill is
///   looked at, so even a lethal drop does nothing;
/// - a kill: HP over (int)(maxHP x 0.4) (DAT_143b01958) pays that and plays the return, else HP 0
///   and the ordinary death (the exe's damage of maxHP x 1);
/// - short of it: HP -= (int)(maxHP x factor); a player emptied that way keeps 1 HP.
pub(crate) fn fall_landing(hp: f32, hp_max: f32, drop: f32, material: Option<u32>, aged: bool, no_fall_damage: bool) -> FallLanding {
    let Some(dist) = crate::terrain::fall_death_dist(material) else { return FallLanding::None };
    let from = if aged { FALL_DAMAGE_FROM_AGED } else { FALL_DAMAGE_MIN };
    let kill = if aged { drop >= FALL_DAMAGE_FROM_AGED } else { drop >= FALL_DAMAGE_MIN && drop >= dist };
    let factor = if kill { 1.0 } else if aged { 0.0 } else { crate::terrain::fall_damage_share(drop, material, from) };
    let damage = (hp_max * factor).floor();
    if no_fall_damage || damage <= 0.0 {
        return FallLanding::None;
    }
    if kill {
        return fall_return_hp(hp, hp_max).map_or(FallLanding::Death, FallLanding::Return);
    }
    FallLanding::Hurt((hp - damage).max(1.0))
}

/// The fall-return death's cost: exe FUN_140b975f0 for the fall out of the map (the fall-return
/// walls, the void drop: damage type 14) and FUN_140b97b30 for the lethal landing (type 15) both
/// take (int)(maxHP x 0.4) (DAT_143b01958) when HP is over it; at or under it HP goes to 0 and
/// Wolf dies (None). Emulated in extracted/emu/air_falldmg.json. Before 2026-11-04 the void fall
/// took 1/3 of max HP after the return (an earlier request, the exe's cost then untraced).
pub(crate) fn fall_return_hp(hp: f32, hp_max: f32) -> Option<f32> {
    let cost = (hp_max * FALL_RETURN_COST).floor();
    (hp > cost).then_some(hp - cost)
}
/// Seconds of the fall-return clip (under the black cut) before Wolf is back on safe ground.
const FALL_RETURN_HOLD: f32 = 1.0;
/// The fall height (m) landings hurt and kill from while aged (exe DAT_143b01950/54: SpEffect
/// stateInfo 0x155 = 341 worn).
const FALL_DAMAGE_FROM_AGED: f32 = 5.0;
/// stateInfo 341: aging (SpEffect 9620).
const STATE_INFO_AGING: i64 = 341;
/// stateInfo 47: no fall damage (SpEffects 640 / 4050 / 4053).
const STATE_INFO_NO_FALL_DAMAGE: i64 = 47;

/// Whether one of Wolf's SpEffects now (residents, TAE, timed) has this stateInfo.
fn wolf_wears(a: &Actor, p: &Player, d: &crate::data::CharData, state_info: i64) -> bool {
    let mut ids = crate::combat::worn_sp(a, &[]);
    ids.extend(p.timed.iter().map(|t| t.0));
    ids.iter().any(|id| d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| crate::data::sp_cycle(*id).map(|c| c.state_info)) == Some(state_info))
}

/// The map's fall-return walls (hit filter 15, under its ravines and off its cliffs): Wolf's body
/// passing one this tick (off the wire; the sim's contact takes any move) starts the fall return
/// at once, as in the game (sekiro-rs sim duel.rs: the wall's fade, then back on safe ground),
/// well before the void drop (`FALL_VOID_DROP`). Checked after the tick's move (any state's), on its whole path.
fn fall_return_walls(combat: Res<crate::data::Combat>, mut log: ResMut<CombatLog>, mut wolf: Query<(&mut Actor, &mut Player, &Transform)>) {
    let d = &combat.player;
    for (mut a, mut p, tf) in &mut wolf {
        let from = p.fall_from.replace(tf.translation);
        if p.wire.is_some() || a.hp <= 0.0 || is_fall_return(&a.state) {
            continue;
        }
        if from.is_some_and(|f| crate::terrain::fall_return_wall(f, tf.translation)) && a.play_state(d, "FallDeadReturnStart") {
            a.hp = fall_return_hp(a.hp, a.hp_max).unwrap_or(0.0);
            a.airborne = false;
            a.vel_y = 0.0;
            info!("fall return: Wolf passed a fall-return wall at {}", tf.translation);
            log.push("fell into the void", Color::srgb(1.0, 0.4, 0.4));
        }
    }
}

/// A fall-return death (W_FallDeadReturnStart / W_LandDeadReturnStart): Wolf out of the map, on
/// his way back to safe ground.
pub fn is_fall_return(state: &str) -> bool {
    matches!(state, "LandDeadReturnStart" | "FallDeadReturnStart")
}
/// The exe's fall-return point (FUN_140a75f00 every frame, CSGame update): a ring of the last 5
/// grounded positions, and CSFallDeadStep::STEP_WarpPlayer (FUN_1411d70e0, via FUN_140a71020)
/// puts Wolf at the oldest, so a ledge or wall he only touched on the way down is not where he
/// comes back (AKJama/sekiro-rs#201). Its spot gate is `safe_spot`. gap: the collision's MSB
/// UnkT17 gate (none set on m11_00) is not modelled.
const SAFE_RING: usize = 5;

/// Whether Wolf's spot can enter the fall-return ring: a walkable floor (the 60 degree slope
/// limit, `Terrain::floor`) right under his feet. The exe's ring takes a frame only while
/// FUN_140a00b40 holds (chr +0x1a12 bit 7): the physics module's ground contact without its slide
/// flag (+0xa48) and a map collision hit under the feet, so resting against a slope too steep to
/// stand on below a cliff is no return point (#201). Without a map every grounded spot is.
fn safe_spot(centre: Vec3, terrain: Option<&crate::terrain::Terrain>) -> bool {
    const GAP: f32 = 0.25;
    let Some(t) = terrain else { return true };
    let feet = centre.y - CAPSULE_HALF_HEIGHT;
    t.floor(centre.x, centre.z, feet + GAP).is_some_and(|y| feet - y <= GAP)
}

#[cfg(test)]
mod safe_spot_tests {
    use super::*;

    /// A 70 degree slope (too steep to stand on) under a cliff is no return point; the walkable
    /// floor above it is, and so is any spot without a map.
    #[test]
    fn a_slope_too_steep_to_stand_on_is_no_return_point() {
        // A flat top at y 0 (x < 0), then a 70 degree face falling toward +x.
        let drop = 10.0;
        let run = drop / 70f32.to_radians().tan();
        let verts = vec![
            Vec3::new(-10.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(-10.0, 0.0, 5.0),
            Vec3::new(run, -drop, -5.0),
            Vec3::new(run, -drop, 5.0),
        ];
        let t = crate::terrain::Terrain::from_tris(verts, [([0, 1, 2], 0), ([0, 2, 3], 0), ([1, 4, 5], 0), ([1, 5, 2], 0)].into_iter());
        let on = |x: f32, y: f32| Vec3::new(x, y + CAPSULE_HALF_HEIGHT, 0.0);
        assert!(safe_spot(on(-3.0, 0.0), Some(&t)), "the cliff top");
        let x = run / 2.0;
        assert!(!safe_spot(on(x, -drop / 2.0), Some(&t)), "half way down the face");
        assert!(safe_spot(on(x, -drop / 2.0), None), "no map");
    }
}
/// Falling this far with no ground below ends in the fall-return death (gap: HitMtrlParam's default
/// fallDeathDist stands in for the exe's void rule).
const FALL_VOID_DROP: f32 = 40.0;
/// A walk-off on the move is walked down for its first 30 cm. HKS BEH_R_FALL (c0000_transition
/// 5685) takes env 200 alone for the unaged Wolf (its env 224 >= 30 cm test sits in the aged
/// branch: Lua `and` before `or`); the exe's env 200 (FUN_140b97970) holds ground contact while
/// a physics probe (P+0x920) reads 1.0 or less. gap: that probe is not identified; this 30 cm
/// (the aged branch's) stands in for it.
pub const FALL_START_DROP: f32 = 0.30;
const REF_MOVING: [i64; 3] = [90, 91, 92];

/// GetLocomotionType() == LOCOMOTION_TYPE_MOVE: the move loop (procedural here) or a clip with a
/// moving ref up (the sprint loop, move starts).
pub(crate) fn moving_locomotion(d: &CharData, a: &Actor) -> bool {
    a.state == "Locomotion" || (!a.anim.is_empty() && REF_MOVING.iter().any(|&r| d.has_ref(&a.anim, a.step(), r)))
}

/// Raw input sampled every frame; presses are latched until FixedUpdate reads them.
#[derive(Resource, Default)]
pub struct PadInput {
    pub stick: Vec2,
    pub walk: bool,
    /// The pad stick's last walk / run choice, for the hysteresis (`stick_runs`).
    pub pad_running: bool,
    pub attack_held: bool,
    pub guard_held: bool,
    pub dodge_held: bool,
    pub prosthetic_held: bool,
    /// The interact / hug / hang button held (env 1108: the air hang catch wants it held).
    pub interact_held: bool,
    /// The grappling hook key held (F): up under water (swim.rs).
    pub grapple_held: bool,
    pub(crate) pressed: Vec<Action>,
    pub reset: bool,
    /// Sheathe / draw the sword (X).
    pub sheathe: bool,
}

impl PadInput {
    /// Queue a button press (used by input reading and by the simulation tests).
    /// This frame's presses not yet taken by player.rs (hks_wolf reads them first).
    pub fn pressed(&self) -> &[Action] {
        &self.pressed
    }

    pub fn press(&mut self, a: Action) {
        self.pressed.push(a);
    }

    /// A press the script took itself (hks_wolf): player.rs does not see it.
    pub fn take(&mut self, a: Action) {
        self.pressed.retain(|&p| p != a);
    }

    /// No movement or presses this frame (a talk holds Wolf, enemy/talk.rs); the fight reset
    /// and the stick's walk / run history stay.
    pub fn hold(&mut self) {
        *self = PadInput { pad_running: self.pad_running, reset: self.reset, ..Default::default() };
    }
}

/// Wolf is flown, not played (ufo.rs): his presses are dropped.
pub(crate) static FLOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Something else drives the pad (autoboss.rs): the keyboard and pad are not read.
#[derive(Resource)]
pub struct ScriptedInput;

impl Player {
    /// A wire move (shoot, flight, landing) is under way.
    pub fn in_wire(&self) -> bool {
        self.wire.is_some()
    }
}

/// The buttons Wolf's decisions read: the frame before's. In the exe (tools/emu-cases
/// `animstart`, kb/frame-coverage.md "One frame's order") the pad is read in ChrIns::vf22
/// (FUN_1407ef120 -> FUN_1407ef960) but the HKS runs inside the Havok behaviour step
/// FUN_141015fa0, after the current clip's TAE update FUN_140b5c920 and its in-step ChrActionFlags
/// (FUN_140b51b70), and the clip it asks for takes its first step [0, dt] in the next frame's
/// step: a press on frame P plays from P + 1 (attack, guard, Loaded Axe, the General's forced
/// 3000 alike). `decide` runs before `advance` and reads the window the last advance took, so it
/// is the exe's HKS of the frame before; it reads the presses and held buttons of the frame before
/// too. The stick stays live for `decide`: the exe's turn and move speed (vf22, before the step)
/// take the frame's. The script's stick reads (MoveSpeedLevel, MoveAngle, the step's, jump's and
/// attack's angle, the run stop) take the frame before's with its buttons (`Turn::hks`): the exe's
/// HKS reads the stick of its own frame (emu `hksstick`).
#[derive(Resource, Default)]
pub(crate) struct HksPad {
    pressed: Vec<Action>,
    held: [bool; 4],
    /// The frame's held buttons while `decide` reads the frame before's.
    live: [bool; 4],
    /// The next tick's buttons count as the frame before's (`prime`).
    primed: bool,
    /// This tick was primed: the script reads this frame's stick too.
    primed_now: bool,
}

impl HksPad {
    /// A test that puts Wolf straight into a state: the buttons it sets for the next tick count
    /// as pressed and held the frame before, so that tick's `decide` reads them.
    pub(crate) fn prime(&mut self) {
        self.primed = true;
    }
}

fn held_of(pad: &PadInput) -> [bool; 4] {
    [pad.attack_held, pad.guard_held, pad.dodge_held, pad.prosthetic_held]
}

fn set_held(pad: &mut PadInput, h: [bool; 4]) {
    [pad.attack_held, pad.guard_held, pad.dodge_held, pad.prosthetic_held] = h;
}

/// Before the script (hks_wolf) and `decide`: the frame's buttons are kept for the next frame, the
/// frame before's are put in.
fn hks_pad_in(mut pad: ResMut<PadInput>, mut late: ResMut<HksPad>) {
    let now = (std::mem::take(&mut pad.pressed), held_of(&pad));
    late.live = now.1;
    late.primed_now = late.primed;
    if std::mem::take(&mut late.primed) {
        pad.pressed = now.0;
        late.held = now.1;
        return;
    }
    pad.pressed = std::mem::replace(&mut late.pressed, now.0);
    let before = std::mem::replace(&mut late.held, now.1);
    set_held(&mut pad, before);
}

/// The pad manipulator (FUN_1407f0a70) runs every frame, whatever Wolf does, in ChrIns::vf22
/// before the HKS: the stick it takes (the 45 deg gate, `Turn::gate`) is the move vector `decide`
/// turns and moves by; the script's MoveSpeedLevel is the length of the one it took the frame
/// before (`Turn::hks`; hks_wolf, exe FUN_140b4afd0 -> +0xbac; emu_turn, emu_hks2 `level`,
/// emu_hksstick).
pub(crate) fn pad_manipulator(pad: Res<PadInput>, lock: Res<LockOn>, late: Res<HksPad>, mut q: Query<&mut Player>) {
    for mut p in &mut q {
        p.turn.new_tick();
        let before = p.turn.taken;
        p.turn.taken = p.turn.gate(pad.stick, lock.target.is_some());
        p.turn.hks = if late.primed_now { p.turn.taken } else { before };
    }
}

/// After `decide`: the held buttons are the frame's again for everything else.
pub(crate) fn hks_pad_out(mut pad: ResMut<PadInput>, late: Res<HksPad>) {
    set_held(&mut pad, late.live);
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PadInput>()
            .init_resource::<HksPad>()
            .add_plugins(crate::view::ViewPlugin)
            .add_systems(Startup, spawn_player.after(crate::place::MapLoad))
            // Input is read right before the frame's fixed steps: Bevy runs them before Update, so a press
            // read in Update reached the simulation one frame (16.7 ms at 60 fps) late.
            .add_systems(RunFixedMainLoop, read_input.run_if(resource_exists::<ButtonInput<KeyCode>>.and_then(not(resource_exists::<ScriptedInput>))).in_set(bevy::app::RunFixedMainLoopSystems::BeforeFixedMainLoop))
            .init_resource::<ActiveThrow>()
            .add_systems(FixedUpdate, hks_pad_in.before(throw_check).before(ActorSet::Decide))
            .add_systems(FixedUpdate, pad_manipulator.after(hks_pad_in).before(throw_check).before(ActorSet::Decide))
            .add_systems(FixedUpdate, (sync_art_variation, decide, record_state).chain().in_set(ActorSet::Decide))
            // The step's one-shot TAE events (401 / 67 SpEffects, 32 weapon style) in the step's tick.
            .add_systems(FixedUpdate, (apply_refresh_effects, tae_one_shots).chain().after(ActorSet::Advance).before(crate::combat::resolve_throws).before(crate::anim::PoseSet))
            .add_systems(FixedUpdate, hks_pad_out.after(ActorSet::Decide).before(ActorSet::Advance))
            .add_systems(FixedUpdate, follow_throw.after(ActorSet::Advance).before(crate::anim::PoseSet))
            .add_systems(FixedUpdate, consume_goods.after(ActorSet::Advance))
            .add_systems(FixedUpdate, crate::docking::hold.after(ActorSet::Advance).before(crate::anim::PoseSet))
            .add_systems(FixedUpdate, crate::swim::hold.after(crate::docking::hold).before(crate::terrain::bodies_on_terrain).before(crate::anim::PoseSet));
        // Bodies on the ground: wall slide and floor follow after the moves (terrain.rs).
        app.add_systems(FixedUpdate, crate::terrain::bodies_on_terrain.after(ActorSet::Advance).before(crate::anim::PoseSet));
        app.add_systems(FixedUpdate, fall_return_walls.after(crate::terrain::bodies_on_terrain).before(crate::anim::PoseSet));
        app.add_systems(FixedUpdate, wolf_no_dead.before(ActorSet::Decide));
        app.add_systems(FixedUpdate, crate::ninjutsu::throw_effects.after(ActorSet::Advance));
        if std::env::var("SHINOBI_MOVE_LOG").is_ok() {
            app.add_systems(FixedUpdate, move_log.after(ActorSet::Advance));
        }
    }
}

/// SHINOBI_MOVE_LOG=<csv>: Wolf's state, clip, clip time, ground speed, height and yaw per
/// fixed tick, for checking movement in the real game build.
fn move_log(q: Query<(&Actor, &Transform), With<Player>>, time: Res<Time>, mut last: Local<Option<Vec3>>, mut out: Local<Option<std::fs::File>>) {
    use std::io::Write;
    let Ok((a, tf)) = q.single() else { return };
    if out.is_none() {
        let path = std::env::var("SHINOBI_MOVE_LOG").unwrap_or_default();
        *out = std::fs::File::create(path).ok();
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "t,state,anim,anim_t,speed,y,yaw_deg,airborne");
        }
    }
    let dt = time.delta_secs().max(1e-6);
    let p = tf.translation;
    let v = last.map_or(0.0, |l| (p - l).with_y(0.0).length() / dt);
    *last = Some(p);
    if let Some(f) = out.as_mut() {
        let _ = writeln!(f, "{:.3},{},{},{:.3},{:.2},{:.2},{:.1},{}", time.elapsed_secs(), a.state, a.anim, a.t, v, p.y, a.yaw.to_degrees(), a.airborne);
    }
}

/// A deathblow in progress: ThrowParam's dummy-poly absorb, applied by follow_throw.
#[derive(Resource, Default)]
pub struct ActiveThrow(pub Option<ThrowHold>);

/// A grab's escape meter (exe throw state +0x1a): ThrowParam escHp, lowered by 1 each
/// selfEscCycleTime window with at least selfEscCycleCnt presses (FUN_140808640 ->
/// FUN_140802c30 -> FUN_140bdf220(-1)).
#[derive(Clone, Copy, Debug)]
struct GrabEscape {
    hp: i64,
    window: f32,
    need: u32,
    presses: u32,
    t: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ThrowHold {
    pub target: Entity,
    pub atk_dmy: i16,
    pub def_dmy: i16,
    /// Wolf's state while the throw lasts ("Deathblow", "Mikiri").
    pub state: &'static str,
    /// ThrowParam adsrobModelPosInterpolationTime: the absorbed model's slide (anim.rs ModelLag).
    pub model_interp: f32,
}

impl ThrowHold {
    pub fn new(target: Entity, th: &crate::data::Throw, state: &'static str) -> Self {
        ThrowHold { target, atk_dmy: th.atk_dmy, def_dmy: th.def_dmy, state, model_interp: th.model_interp }
    }
}


/// HKS _SetJumpDirection sectors (PRM_GROUND_JUMP_*_STICK_RANGE, degrees from the facing, + right):
/// (ready state, raw anim for the side jumps, landing state).
/// The states the jump tilt turns: the positioning ground jumps (Ready / Start / their lands),
/// the kick jump and the step out of a knockdown (_execDamageStep's _set4DirJumpTilt).
fn jump_tilted(state: &str) -> bool {
    (state.contains("GroundJump") && !state.starts_with("Vertical") && !state.starts_with("Sprint")) || state.starts_with("LandGroundPositioningJump") || state.starts_with("AirKickEnemyJump") || state.starts_with("StandStepFromDown_")
}

/// _setPositioningJumpTilt (HKS 5114-5140): the stick angle minus the jump sector's centre
/// (c0000_define 232-239), radians with the step's sign; the lock-on forward jump none.
fn jump_tilt(ready: &str, angle: f32) -> f32 {
    let centre = match ready {
        "LockonForwardGroundJumpReady" => return 0.0,
        "GroundJumpReady_F_R" => 45.0,
        "RightsideGroundJumpReady" => 90.0,
        "GroundJumpReady_B_R" => 135.0,
        "GroundJumpReady_F_L" => -45.0,
        "LeftsideGroundJumpReady" => -90.0,
        "GroundJumpReady_B_L" => -135.0,
        _ if angle > 0.0 => 180.0,
        _ => -180.0,
    };
    (-(angle - centre)).to_radians()
}

fn directional_jump(angle: f32) -> (&'static str, Option<&'static str>, &'static str) {
    match angle {
        a if a.abs() <= 18.75 => ("LockonForwardGroundJumpReady", None, "LandGroundPositioningJump_LockOn_F"),
        a if a > 18.75 && a < 67.5 => ("GroundJumpReady_F_R", None, "LandGroundPositioningJump_F_R"),
        a if (67.5..=112.5).contains(&a) => ("RightsideGroundJumpReady", Some("a000_201103"), "LandGroundPositioningJump_R"),
        a if a > 112.5 && a < 157.5 => ("GroundJumpReady_B_R", None, "LandGroundPositioningJump_B_R"),
        a if a < -18.75 && a > -67.5 => ("GroundJumpReady_F_L", None, "LandGroundPositioningJump_F_L"),
        a if (-112.5..=-67.5).contains(&a) => ("LeftsideGroundJumpReady", Some("a000_201102"), "LandGroundPositioningJump_L"),
        a if a < -112.5 && a > -157.5 => ("GroundJumpReady_B_L", None, "LandGroundPositioningJump_B_L"),
        _ => ("BackwardGroundJumpReady", None, "LandGroundPositioningJump_B"),
    }
}

/// Can Wolf deathblow this enemy, and with which throw: a broken enemy (posture or vitality) from
/// the front (崩し始動 0000 -> 本体 0001; the Ochimusha's 近 0005 -> 0006 when within its Dist) or
/// from behind (崩し背後始動 0110 -> 0111, DiffAng 0-90); an enemy that has not noticed Wolf only
/// from behind (背後始動 0020 -> 背後本体 0021: a20x_500200 -> 510200 [-> 510201], ThrowDef12200;
/// live: c1010 0.29 s + 2.0 s, the General 0.28 s + 1.52 s + 1.82 s). `in_reach`: within the start
/// throw's Dist (else the main throw's).
pub struct DeathblowCheck {
    pub behind: bool,
    /// The main throw's ThrowParam suffix.
    pub suffix: i64,
    pub main: crate::data::Throw,
    pub start: Option<crate::data::Throw>,
    pub in_reach: bool,
}

/// Which enemy a deathblow goes to, from (entity, position, in reach, broken, chr number)
/// candidates. In reach: the exe's input throw (FUN_140be0e50) runs an IChrFinder over
/// WorldChrMan (FUN_140a4c260: its chr sets in order, each set's chrs by index) and stops at the
/// first chr with any passing row (FUN_140bdf930 -> FUN_140bde5c0); the lock target and the
/// distance play no part. The snapshot's order runs by chr number (c1000, c1001, ..., c7600, the
/// players last), so the lowest chr number wins, then the earliest spawned (Entity order).
/// Run with two c1010 and the General in reach (120 placements, the pick never followed the
/// nearest on 48 of the 79 with several: sim_tests/emu_throw2.rs). gap: within one chr number
/// the exe's order (the set's index, the map's load order) is taken as Sv1's spawn order.
/// With `or_broken` (the HUD's 忍殺 mark) and none in reach, a broken one: the locked-on, else
/// the nearest (gap: the mark's own pick is not traced).
pub(crate) fn deathblow_pick(cands: impl IntoIterator<Item = (Entity, Vec3, bool, bool, i64)>, lock: Option<Entity>, wolf: Vec3, or_broken: bool) -> Option<Entity> {
    let cands: Vec<(Entity, Vec3, bool, bool, i64)> = cands.into_iter().collect();
    let first = cands.iter().filter(|c| c.2).min_by_key(|c| (c.4, c.0.index())).map(|c| c.0);
    first.or_else(|| {
        if !or_broken {
            return None;
        }
        cands
            .iter()
            .filter(|c| c.3)
            .min_by(|x, y| {
                let key = |c: &(Entity, Vec3, bool, bool, i64)| (Some(c.0) != lock, c.1.distance(wolf));
                let (kx, ky) = (key(x), key(y));
                kx.0.cmp(&ky.0).then(kx.1.total_cmp(&ky.1))
            })
            .map(|c| c.0)
    })
}

/// A chr's number for deathblow_pick ("c1020" -> 1020).
pub(crate) fn chr_number(combat: &Combat, ea: &Actor) -> i64 {
    combat.foe_of(ea).chr.trim_start_matches('c').parse().unwrap_or(i64::MAX)
}

pub fn deathblow_check(combat: &Combat, wolf: Vec3, wolf_fwd: Vec3, aged: bool, ea: &Actor, enemy: &Enemy, at: Vec3, dmy: &dyn Fn(i16) -> Option<Vec3>) -> Option<DeathblowCheck> {
    // The kind test's own gates (FUN_140be3810): no throw starts while the defender's action
    // flags hold ChrActionFlag 67 ThrowStart2 (+0x70 bit 0x20: ThrowDef12100 / 13100 0-1.5 s) or
    // it is invulnerable (vfunc 0x550 = FUN_1409f3250: TAE 950 / 951, +0x70 bits 1 / 0). The
    // check itself stands (out of reach), so the HUD mark keeps showing on the break pose.
    // gap: the exe's mark display is not traced against these gates.
    let closed = throw_kind_closed(combat, ea);
    deathblow_rows(combat, wolf, wolf_fwd, aged, ea, enemy, at, dmy).map(|mut c| {
        c.in_reach &= !closed;
        c
    })
}

#[allow(clippy::too_many_arguments)]
fn deathblow_rows(combat: &Combat, wolf: Vec3, wolf_fwd: Vec3, aged: bool, ea: &Actor, enemy: &Enemy, at: Vec3, dmy: &dyn Fn(i16) -> Option<Vec3>) -> Option<DeathblowCheck> {
    let d = &combat.player;
    let foe = combat.foe_of(ea);
    // No deathblow while the Owl's phase-2 transition holds its TAE marker 3506080 (a000_020005
    // heat-up, the 021003 kneel loop, the first 0.233 s of 20006-20008); his castle Lua answers
    // an attack then with escape 20007. Ported from AKJama/sekiro-rs b997933 (docs/OWL.md: the
    // user's real-game check). gap: the exe's test of the marker is not traced.
    if combat.data_of(ea).sp_effects_at(&ea.anim, ea.step()).iter().any(|(id, _)| *id == 3_506_080) {
        return None;
    }
    // A boss in its finisher window (data.rs finisher_open): Wolf's last blow, Dist from its
    // judge dummy (judgeRangeBasePosDmyId2) or root, and the row's facings (throw_facing_ok,
    // round 42: Isshin's 15400590 DiffAng 0-45 = Wolf behind him facing his way - his kneel
    // a100_020200 has no root yaw and his absorb dummy 230 is at his back; before, in front
    // within 45 of his facing). gap: the Y range is not checked; from the dragon's dummy 230 (6 m
    // up, by its head) only across the floor, its body not being ground here.
    // Aged (THROWABLE_STATE_ATK_PC_AGING 10, c0000.lua 699; round 45 from Bosees' read of the
    // rows): only the aging rows, c1300's alone - 0260 / 0261 "aging break" (ThrowKind cat 18,
    // a222_502000 -> 512000, ThrowDef14000, DiffAng 90-180), 0270 / 0271 "aging behind" (cat 17,
    // unaware, 502100 -> 512100) and 0280 / 0281 "aging break behind" (cat 19, 502200 -> 512200);
    // no finisher, near, Todome or second-form rows. The categories pass by the enemy's throwable
    // state (throwable_state_allows): a broken one (103) the aging-break 18 / 19, not the unaware 17.
    let state = def_throw_state(combat, ea, enemy);
    // A dying chr passes the exe's kind test in its death anim's 932 103 span (DeathDefault 0-45
    // f: no 950 / 951 / 67 there, defAliveCheck 0 on the break kinds; HP 0 changes nothing,
    // throw_kinds run). gap: Sv1 does not take it - its Mode::Dead enemy (counted, fading) has no
    // way back into a ThrowDef anim - but for the finisher windows.
    if enemy.is_dead() && !matches!(state, 104 | 105) {
        return None;
    }
    // A start row's whole test (FUN_140be3a90): the range (throw_range_ok), the defender's
    // SpEffect condition (throw_speffects_ok) and the sight casts (throw_sight_ok) on the map,
    // from the feet. gap: Wolf's side of the SpEffect pair is taken as met (Actor not passed;
    // no row of the loaded enemies sets _forAtk).
    let npc = combat.npc(ea);
    let (def_h, def_r) = (npc["hitHeight"].as_f64().unwrap_or(2.0) as f32, npc["hitRadius"].as_f64().unwrap_or(0.5) as f32);
    let feet = Vec3::Y * CAPSULE_HALF_HEIGHT;
    let row_ok = |row: i64| {
        throw_range_ok(combat, row, wolf, wolf_fwd, at, ea.forward())
            && throw_speffects_ok(combat, row, &|_| true, &|id| crate::combat::wears_sp(ea, id))
            && throw_sight_ok(combat, row, wolf - feet, WOLF_PHYS_HEIGHT, CAPSULE_RADIUS, at - feet, def_h, def_r, &map_sphere_cast)
    };
    if aged {
        let unaware = enemy.is_unaware() && throwable_state_allows(state, 17);
        let behind = ea.forward().angle_between(wolf_fwd.with_y(0.0).normalize_or_zero()).to_degrees() < 90.0;
        if !throwable_state_allows(state, 18) && !(unaware && behind) {
            return None;
        }
        let (start_sfx, suffix) = if unaware && behind { (270, 271) } else if behind { (280, 281) } else { (260, 261) };
        let main = combat.throw(foe.throw_row(suffix)).filter(|t| d.anim(&t.atk_anim).is_some())?;
        let start = combat.throw(foe.throw_row(start_sfx)).filter(|st| d.anim(&st.atk_anim).is_some());
        let reach_row = if start.is_some() { foe.throw_row(start_sfx) } else { foe.throw_row(suffix) };
        let in_reach = row_ok(reach_row);
        return Some(DeathblowCheck { behind, suffix, main, start, in_reach });
    }
    let usable = |sfx: i64| combat.throw(foe.throw_row(sfx)).is_some_and(|t| d.anim(&t.atk_anim).is_some());
    // The Todome (トドメ始動 / 本体, ThrowParam 0180 / 0181, +500 in the second form: Wolf
    // a2xx_501700 -> 511700, the boss ThrowDef13700) is taken from the boss's kneel, its Event20200
    // with the 105 window (data.rs finisher_window), not from the last mark: in every recording
    // (rec_2026101x_*_boss_*) the last mark is a usual deathblow, the map script then sends the
    // boss to 20200 and Wolf to his Event7102xx, and 0180 / 0181 follow 0.8-1.8 s later (Isshin:
    // 15400680 / 681 at 912.2 s). The start rows' Dist 20 and DiffAng 0-180 reach him anywhere
    // near. The window's state 105 passes the Todome's category 16 (FUN_140be0a40).
    if throwable_state_allows(state, 16) {
        let hu = if ea.anim_group == 1 && usable(681) { 500 } else { 0 };
        if usable(181 + hu) {
            let main = combat.throw(foe.throw_row(181 + hu))?;
            let start = combat.throw(foe.throw_row(180 + hu)).filter(|st| d.anim(&st.atk_anim).is_some());
            let reach = start.as_ref().map_or(main.dist, |st| st.dist);
            return Some(DeathblowCheck { behind: false, suffix: 181 + hu, main, start, in_reach: at.distance(wolf) < reach });
        }
    }
    // The "enemy special state A" rows (throwKind 250000, category 5) pass throwable state 104
    // only; 105 passes the Todome's category 16 and not 5 (FUN_140be0a40 run on the snapshot).
    let finisher = throwable_state_allows(state, 5).then(|| combat.finisher(&foe)).flatten();
    if let Some((id, main)) = finisher {
        let judge = (main.judge_dmy > 0).then(|| dmy(main.judge_dmy)).flatten();
        let base = judge.unwrap_or(at);
        let facing_base = if judge.is_some() { base.with_y(wolf.y) } else { base };
        if !throw_facing_ok(combat, id, wolf, wolf_fwd, facing_base, ea.forward()) || d.anim(&main.atk_anim).is_none() {
            return None;
        }
        let gap = if judge.is_some() { (base - wolf).with_y(0.0).length() } else { base.distance(wolf) };
        let in_reach = gap < main.dist;
        return Some(DeathblowCheck { behind: false, suffix: id - foe.throw_row(0), main, start: None, in_reach });
    }
    // An enemy's own state 101 passes the unaware-behind category 0 (背後始動), the break state
    // 103 the break category 4 and not 0: a broken enemy that never noticed Wolf takes the break
    // rows (as the plunge's `stealth`), and a finisher window (104 / 105) none of the two.
    let unaware = enemy.is_unaware() && throwable_state_allows(state, 0);
    if !throwable_state_allows(state, 4) && !unaware {
        return None;
    }
    // The start rows' DiffAngMin / Max are the facings' yaw difference (FUN_140be0270, see
    // throw_facing_ok): 崩し背後始動 0110 0-90 (Wolf faces the way the enemy does), 崩し始動
    // 0000 90-180. Round 41: by Wolf's place in front of / behind the enemy.
    let behind = ea.forward().angle_between(wolf_fwd.with_y(0.0).normalize_or_zero()).to_degrees() < 90.0;
    if unaware && !behind {
        return None;
    }
    let near = combat.throw(foe.throw_row(5)).filter(|st| !behind && d.anim(&st.atk_anim).is_some() && row_ok(foe.throw_row(5)));
    let (start_sfx, suffix) = if unaware {
        (20, 21)
    } else if behind {
        (110, 111)
    } else if near.is_some() {
        (5, 6)
    } else {
        (0, 1)
    };
    // The last mark of a boss with "崩しトドメ" / "崩し背後トドメ" rows (main + 8: Gyoubu 0009 / 0119,
    // the headless Ape 0509 / 0619 -> ThrowDef12090 / 13290, whose message 30 at 9.5 s is his
    // defeat, m17 11705800) takes those, after the usual start. (The Todome 0180 / 0181 is not
    // taken here but from the boss's kneel, above.)
    let last = !unaware && enemy.ninsatsu.0 <= 1;
    // Its second form ("（HU）" rows, +500: Isshin's spear phase a243_*, the headless Guardian
    // Ape's a235_*, Lady Butterfly's) while it plays its second anim set (a100). gap: the exe's
    // pick of these rows is not traced (by the rows' names and the anim sets).
    // The row's own condition decides (combat::throw_row_for: ThrowParam
    // additionalConditionSpecialEffect_forDef, 200031 = the a100 set on every (HU) row; round 90).
    // Without a matching row the old anim-set rule stands.
    let hu = match crate::combat::throw_row_for(combat, ea, suffix) {
        Some(id) if id == foe.throw_row(suffix + 500) && usable(suffix + 500) => 500,
        Some(_) => 0,
        None => if ea.anim_group == 1 && usable(suffix + 500) { 500 } else { 0 },
    };
    let (start_sfx, mut suffix) = (start_sfx + hu, suffix + hu);
    if last && usable(suffix + 8) {
        suffix += 8;
    }
    let main = combat.throw(foe.throw_row(suffix))?;
    let start = combat.throw(foe.throw_row(start_sfx)).filter(|st| d.anim(&st.atk_anim).is_some());
    // The start row's height window too (upperYRange / lowerYRange: 1.26 m on the General's
    // 0020 / 0110), as the plunge and deflect-pose starts check it (combat.rs throw_in_reach):
    // round 36, a deathblow reached across a 2 m ledge. And its facings (throw_facing_ok:
    // diffAngMyToDef 35 on the General's rows - Wolf must face the enemy; round 41, before any
    // facing took it).
    let reach_row = if start.is_some() { foe.throw_row(start_sfx) } else { foe.throw_row(suffix) };
    let in_reach = row_ok(reach_row);
    Some(DeathblowCheck { behind, suffix, main, start, in_reach })
}

/// The aged STATE_TYPE_STANDBY states (g_paramHkbState): they take every input, as StandIdle.
fn is_aged_standby(s: &str) -> bool {
    matches!(s, "AgingIdle" | "AgingStandMove_Walk_F" | "AgingSprintStart" | "AgingSprintLoop")
}

/// Wolf's aged states (STYLE_TYPE_AGING_STAND / _FALL in g_paramHkbState), but the deaths.
fn is_aging_state(s: &str) -> bool {
    (s.starts_with("Aging") || s == "StandAging" || s == "FreeFallAging" || s.starts_with("LandAging")) && !s.contains("Death")
}

/// The aged form (round 44; the Mist Noble's breath fills the curse gauge -> 9620, ref 591, 30 s).
/// Returns whether it ran the tick.
/// - BEH_R_AGING_DAMAGE (HKS 1043-1053): W_StandAging (a000_210300), in the air W_FreeFallAging;
///   not in a throw (5617: env 206) or a death.
/// - The aged standby (AgingIdle a000_023000, AgingStandMove_Walk_F 023200, AgingSprintStart /
///   Loop 024100 / 024200 while the dodge is held) takes: attack -> W_AgingGroundAttack
///   (BEH_A_AGING_ATTACK, a050_390000), roll / step / guard / tool / grapple / art ->
///   W_AgingGroundAction (BEH_A_AGING_ACTION, 226010). Letting go of the stick: W_AgingStandWalkStop
///   / W_AgingSprintEnd (BEH_A_AGING_GROUND_MOVE_STOP). They move by their root motion and turn by
///   their TAE SetTurnSpeed.
/// - Falling: W_AgingFreeFallStart -> Loop -> W_LandAgingFreeFall (HKS 1744 / 1780).
/// - BEH_R_CURE_AGING (1054-1062, 5624): ref 591 gone and a move / action input -> W_Idle (the
///   input then acts this tick).
/// - The jump (BEH_A_AGING_GROUND_JUMP 4707-4718): W_AgingGroundJumpReady_F with the stick, _V
///   without, into its Start, whose TAE 920 launches (as the main tick's), the fall loop, and
///   W_LandAgingGroundJump (1776).
/// - No aged quick turn: c0000_transition.hks has none (BEH_A_AGING_* are 700-702, 710, 720,
///   721, 152); the walk turns as it goes.
/// - Out of scope: BEH_A_AGING_ITEM_USE 152 (4392-4417) fires only W_AgingItemReturnStatue, the
///   Homeward Idol's travel (ITEM_UNIQUE_RESERVE_1); every other item is refused.
#[allow(clippy::too_many_arguments)]
fn aging_step(p: &mut Player, a: &mut Actor, combat: &Combat, config: &GameConfig, stick: Vec3, pad: &PadInput, pos: &mut Vec3, dt: f32, log: &mut CombatLog) -> bool {
    let d = &combat.player;
    let ended = !a.anim.is_empty() && a.t >= d.length(&a.anim);
    let standby = matches!(a.state.as_str(), "AgingIdle" | "AgingStandMove_Walk_F" | "AgingSprintStart" | "AgingSprintLoop" | "AgingStandWalkStop" | "AgingSprintEnd");
    if a.aged.is_none() {
        if !is_aging_state(&a.state) {
            return false;
        }
        p.aged_form = false;
        if (standby || ended) && (stick != Vec3::ZERO || !p.requests.is_empty()) || ended && !standby {
            a.play("StandIdle", "");
            a.move_vel = Vec3::ZERO;
            log.push("aging wore off", Color::srgb(0.8, 0.8, 0.8));
            return false;
        }
    }
    if a.aged.is_some() && !is_aging_state(&a.state) {
        // The pair states as actor.rs in_throw counts them (round 52: a grab, the kick vault and the
        // Mikiri pose were cut by the onset, the grabber left in ThrowAtk).
        if a.state.starts_with("Throw") || a.state.contains("Deathblow") || crate::actor::in_throw(a) {
            return false;
        }
        a.move_vel = Vec3::ZERO;
        a.air_base = Vec3::ZERO;
        p.speed_level = 0.0;
        p.requests.clear();
        let onset = if a.airborne {
            if a.state == "FreeFall" || p.aged_form { "AgingFreeFallStart" } else { "FreeFallAging" }
        } else if p.aged_form {
            "AgingIdle"
        } else {
            "StandAging"
        };
        a.play_state(d, onset);
        if !p.aged_form {
            log.push("AGED", Color::srgb(0.75, 0.7, 0.6));
        }
        p.aged_form = true;
        return true;
    }
    // The jump's ready into its start; the start's TAE 920 launches (main tick's rule).
    if ended && a.state.starts_with("AgingGroundJumpReady") {
        let start = a.state.replace("Ready", "Start");
        a.play_state(d, &start);
        return true;
    }
    let jumping = a.state.starts_with("AgingGroundJumpStart") || a.state == "AgingGroundJumpFallLoop";
    if a.state.starts_with("AgingGroundJumpStart") && (a.prev_t > 0.0 || !a.vel_change_done) {
        if let Some(vc) = d.velocity_change(&a.anim, a.prev_t, a.t).and_then(|id| combat.velocity_change_row(id)) {
            a.vel_change_done = true;
            let dir = Quat::from_rotation_y(-vc.h_angle.to_radians() + if jump_tilted(&a.state) { p.jump_tilt } else { 0.0 }) * a.forward();
            a.air_base = a.move_vel * vc.h_scale + dir * vc.h_change;
            a.vel_y = a.vel_y * vc.v_scale + vc.v_change;
            a.fall_type = vc.fall_type;
            a.airborne = true;
            a.launch_left = LAUNCH_TIME;
        }
    }
    if a.airborne {
        a.move_vel = a.air_base;
        if ended && matches!(a.state.as_str(), "FreeFallAging" | "AgingFreeFallStart") {
            a.play_state(d, "AgingFreeFallLoop");
        } else if ended && a.state.starts_with("AgingGroundJumpStart") {
            a.play_state(d, "AgingGroundJumpFallLoop");
        }
        // Gravity as the death fall's; the body moves by vel_y elsewhere (movement).
        let g = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)].gravity();
        a.grav_y = g;
        a.vel_y += g * dt;
        let floor = crate::terrain::ground_y(*pos) + CAPSULE_HALF_HEIGHT;
        if pos.y <= floor && a.vel_y < 0.0 {
            pos.y = floor;
            a.airborne = false;
            a.fall_type = 0;
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
            // Aged (stateInfo 341) every landing from 5 m kills (exe FUN_14084e5d0, the main
            // tick's rule): the fall-return death, BEH_R_DEATH before the aged landing, with its
            // HP cost or the ordinary death (`fall_landing`).
            let no_damage = wolf_wears(a, p, d, STATE_INFO_NO_FALL_DAMAGE);
            match fall_landing(a.hp, a.hp_max, p.air_top - pos.y, crate::terrain::floor_material(*pos), true, no_damage) {
                FallLanding::Return(hp) => {
                    a.hp = hp;
                    if a.play_state(d, "LandDeadReturnStart") {
                        log.push("fall death - back to safe ground", Color::srgb(1.0, 0.4, 0.4));
                        return true;
                    }
                }
                FallLanding::Death => {
                    a.hp = 0.0;
                    log.push("fall death", Color::srgb(1.0, 0.4, 0.4));
                    return true;
                }
                FallLanding::None | FallLanding::Hurt(_) => {}
            }
            a.play_state(d, if jumping { "LandAgingGroundJump" } else { "LandAgingFreeFall" });
        }
        return true;
    }
    a.move_vel = Vec3::ZERO;
    let free = standby || ended;
    if ended && !matches!(a.state.as_str(), "AgingIdle" | "AgingStandMove_Walk_F" | "AgingSprintLoop") {
        let next = if a.state == "AgingSprintStart" { "AgingSprintLoop" } else { "AgingIdle" };
        a.play_state(d, next);
    }
    if !free {
        free_turn(p, a, d, config, stick, dt);
        return true;
    }
    let action = [Action::Guard, Action::Step, Action::Prosthetic, Action::Grapple, Action::CombatArt];
    if p.requests.remove(&Action::Attack).is_some() {
        if stick != Vec3::ZERO {
            a.yaw = yaw_of(stick);
        }
        a.play_state(d, "AgingGroundAttack");
    } else if action.iter().any(|k| p.requests.contains_key(k)) {
        for k in action {
            p.requests.remove(&k);
        }
        a.play_state(d, "AgingGroundAction");
    } else if p.requests.remove(&Action::Jump).is_some() {
        if stick != Vec3::ZERO {
            a.yaw = yaw_of(stick);
        }
        a.vel_change_done = false;
        a.play_state(d, if stick != Vec3::ZERO { "AgingGroundJumpReady_F" } else { "AgingGroundJumpReady_V" });
    } else if stick != Vec3::ZERO {
        if pad.dodge_held {
            if !a.state.starts_with("AgingSprint") || a.state == "AgingSprintEnd" || (ended && a.state == "AgingSprintLoop") {
                let s = if a.state == "AgingSprintLoop" { "AgingSprintLoop" } else { "AgingSprintStart" };
                a.play_state(d, s);
            }
        } else if a.state != "AgingStandMove_Walk_F" || ended {
            a.play_state(d, "AgingStandMove_Walk_F");
        }
        free_turn(p, a, d, config, stick, dt);
    } else if a.state == "AgingStandMove_Walk_F" {
        a.play_state(d, "AgingStandWalkStop");
    } else if a.state.starts_with("AgingSprint") && a.state != "AgingSprintEnd" {
        a.play_state(d, "AgingSprintEnd");
    } else if a.state == "AgingIdle" && ended {
        a.play_state(d, "AgingIdle");
    }
    // The other presses (item, crouch, tool switch) have no aged move.
    p.requests.retain(|k, _| matches!(k, Action::Attack));
    p.requests.remove(&Action::Attack);
    true
}

/// The shock a lightning charge enchantment sets off (exe re-scout, round 43): 949x (30 s,
/// motionInterval 0.1) fires its cycleOccurrenceSpEffectId 941x, gated on stateInfo 901 (9480
/// "shock resistance permission", resident on Wolf's protectors), which fires the damage row
/// 943x, gated on the charge's own stateInfo 356 / 151, which fires 9450 / 9451 (refs 593 / 592).
/// So it goes off at once. The damage row's changeHpPoint + changeHpRate % of max HP, and its
/// negative changeStaminaPoint + Rate % as posture; a kill is the electric-shock death
/// (death_hit element 6 / 10, BEH_R_DEATH 888 / 903). Returns BEH_R_SPECIAL_DAMAGE's reaction
/// (1013-1040): W_GroundDamageElectricShock for ref 592, ..Weak for 593.
/// gap: whether a guarded / deflected hit's SpEffects land (as the poison build-up has it).
pub(crate) fn lightning_shock(combat: &Combat, a: &mut Actor, charge: i64) -> Option<&'static str> {
    let row = |id: i64| combat.params.get("SpEffectParam").and_then(|t| t.get(id.to_string()));
    let next = |id: i64| row(id).and_then(|r| r["cycleOccurrenceSpEffectId"].as_i64()).filter(|n| *n > 0);
    let dmg_id = next(next(charge)?)?;
    let dmg = row(dmg_id)?;
    let f = |k: &str| dmg[k].as_f64().unwrap_or(0.0) as f32;
    let strong = next(dmg_id).and_then(row).and_then(|r| r["behaviorRefId"].as_i64()) == Some(592);
    a.hp = (a.hp - f("changeHpPoint") - a.hp_max * f("changeHpRate") / 100.0).max(0.0);
    a.add_posture(-f("changeStaminaPoint") - a.posture_max * f("changeStaminaRate") / 100.0);
    if a.hp <= 0.0 {
        a.death_hit = Some((0, if strong { 6 } else { 10 }));
    }
    Some(if strong { "GroundDamageElectricShock" } else { "GroundDamageElectricShockWeak" })
}

/// BEH_R_DEATH 785-951 (c0000_transition): Wolf's death start for the killing hit's (damage
/// level, element), in the HKS's order, each IndexDeath B but the small blow's and the fall's.
/// - In the air (free fall): lightning (6) W_FallDeathElectroChargeStart, blue lightning (10)
///   ..WeakStart; EXLARGE (4) W_AirDamageDeathLargeBlowStart, EX_BLAST (10)
///   ..ExLargeBlowStart, BREATH (11) ..SpecialLargeBlowStart, UPPER (9) ..LargeUpperStart,
///   FLING (6) ..LargePoundStart; NONE W_FallDeathDmgStartNaturalDeath; else
///   W_FallDeathDmgStart (F / B).
/// - On the ground: lightning W_GroundDeathStartElectricShock, blue lightning ..Weak; EXLARGE
///   W_StandDamageDeathLargeBlowStart, EX_BLAST ..ExLargeBlowStart, BREATH
///   ..SpecialLargeBlowStart, SMALL_BLOW (7) W_StandDamageDeathSmallBlow (F / B by the 4-way
///   angle), UPPER ..LargeUpperStart, FLING W_StandDamageDeathLargePound; else none (fire /
///   terror / poison / the plain start are the caller's).
/// Candidates in order; the caller plays the first with a clip. gap: the air blows' clips
/// (a000_1214xx-1217xx) and FallDeathDmgStart / Loop (1210xx) are not in the game's anibnd,
/// so the air ones take NaturalDeath's fall or the ground blow.
pub(crate) fn death_starts(hit: Option<(i64, i64)>, airborne: bool, back: bool, back_quadrant: bool, aged: bool) -> Vec<String> {
    let Some((level, element)) = hit else {
        // No killing hit (a status tick: damage level NONE). In the air HKS 813-817 takes
        // W_FallDeathDmgStartNaturalDeath (round 60: before, the ground poison / fire / terror
        // start played mid-air). gap: NONE comes before AGING_FALL there; aged keeps its own.
        return match (airborne, aged) {
            (true, true) => vec!["AgingFallDeathStart".to_string()],
            (true, false) => vec!["FallDeathDmgStartNaturalDeath".to_string()],
            _ => Vec::new(),
        };
    };
    let fb = if back { "B" } else { "F" };
    let blow = |pre: &str| match level {
        4 => Some(format!("{pre}DamageDeathLargeBlowStart_{fb}")),
        10 => Some(format!("{pre}DamageDeathExLargeBlowStart_{fb}")),
        11 => Some(format!("{pre}DamageDeathSpecialLargeBlowStart_{fb}")),
        9 => Some(format!("{pre}DamageDeathLargeUpperStart")),
        _ => None,
    };
    let mut v = Vec::new();
    if airborne {
        match (element, level) {
            (6, _) => v.push("FallDeathElectroChargeStart".to_string()),
            (10, _) => v.push("FallDeathElectroChargeWeakStart".to_string()),
            (_, 6) => v.push("AirDamageDeathLargePoundStart".to_string()),
            (_, 0) => v.push("FallDeathDmgStartNaturalDeath".to_string()),
            _ => match blow("Air") {
                Some(s) => v.push(s),
                // Aged in the air (STYLE_TYPE_AGING_FALL, 819): W_AgingFallDeathStart.
                None if aged => v.push("AgingFallDeathStart".to_string()),
                None => v.extend([format!("FallDeathDmgStart{fb}"), "FallDeathDmgStartNaturalDeath".to_string()]),
            },
        }
    }
    match (element, level) {
        (6, _) => v.push("GroundDeathStartElectricShock".to_string()),
        (10, _) => v.push("GroundDeathStartElectricShockWeak".to_string()),
        (_, 7) => v.push(format!("StandDamageDeathSmallBlow_{}", if back_quadrant { "B" } else { "F" })),
        (_, 6) => v.push("StandDamageDeathLargePound".to_string()),
        _ => v.extend(blow("Stand")),
    }
    v
}

/// Any of Wolf's death states (starts, chains, loops), which a new death start must not cut.
fn is_death_state(s: &str) -> bool {
    s.starts_with("GroundDeath")
        || s.starts_with("GroundRevivalStayLoop")
        // The grab deaths only (round 60: "ThrowDef" also took the Ape's non-lethal blow-away,
        // ThrowDefLargeBlow*, and a tick that emptied HP there left Wolf dead with no death).
        || s.starts_with("ThrowDefDeath")
        || s.starts_with("ThrowDefRevival")
        || s.contains("DamageDeath")
        || s.starts_with("FallDeath")
        || s.starts_with("LandFallDeath")
        || s.starts_with("LandDeath")
        || s.starts_with("LandRevivalStayLoop")
        || s.starts_with("AgingGroundDeath")
        || s.starts_with("AgingFallDeath")
        || s == "LandAgingFallDeath"
}

/// The death states that end in a death loop, as (Land set, IndexDeath side): HKS 275 / 328, the
/// ground set -> W_GroundDeathLoop / RevivalStayLoop; 277 / 334, the Land set -> W_LandDeathLoop
/// / RevivalStayLoop. The blows and lightning are IndexDeath B (BEH_R_DEATH), the small blow
/// and the fall their own side, the blue-lightning fall F (DEATH_TYPE_COMMON).
fn death_done(s: &str) -> Option<(bool, &'static str)> {
    let own = if s.ends_with('B') { "B" } else { "F" };
    match s {
        "ThrowDefDeathLargeBlowLand600380" | "StandDamageDeathLargePound" | "GroundDeathStartElectricShock" | "GroundDeathStartElectricShockWeak" | "GroundDeathStartInsane" | "GroundDeathStartPoison" | "GroundDeathStartFire" => Some((false, "B")),
        _ if s.starts_with("StandDamageDeathSmallBlow_") => Some((false, own)),
        _ if s.contains("DamageDeath") && s.contains("Land") => Some((false, "B")),
        "LandFallDeathDmgF" | "LandFallDeathDmgB" => Some((true, own)),
        "LandFallDeathElectroCharge" => Some((true, "B")),
        "LandFallDeathElectroChargeWeak" => Some((true, "F")),
        _ => None,
    }
}

/// The blow / fall death chains (the behaviour graph's own transitions, not in HKS): a Start
/// that has played out goes to its FallLoop while airborne, else to its Land; a FallLoop lands
/// once on the ground. A missing Air clip takes the Stand one. Inference: SpecialLargeBlow (no
/// Land of its own in HKS 275) falls and lands as ExLargeBlow; the fall deaths' loops are
/// FallDeathDmgLoop<F|B> / ..ElectroCharge(Weak)Loop.
fn death_chain_step(a: &mut Actor, d: &CharData) {
    let s = a.state.clone();
    let (base, side) = match s.strip_suffix("_F").or_else(|| s.strip_suffix("_B")) {
        Some(b) => (b, &s[b.len()..]),
        None => (s.as_str(), ""),
    };
    let (fall, land): (Option<String>, String) = if s.contains("DamageDeath") {
        if let Some(rest) = base.strip_suffix("Start") {
            let rest = rest.replace("SpecialLargeBlow", "ExLargeBlow");
            (Some(format!("{rest}FallLoop{side}")), format!("{rest}Land{side}"))
        } else if let Some(rest) = base.strip_suffix("FallLoop") {
            (None, format!("{rest}Land{side}"))
        } else {
            return;
        }
    } else {
        match s.as_str() {
            "FallDeathDmgStartNaturalDeath" | "FallDeathDmgStartB" => (Some("FallDeathDmgLoopB".into()), "LandFallDeathDmgB".into()),
            "FallDeathDmgStartF" => (Some("FallDeathDmgLoopF".into()), "LandFallDeathDmgF".into()),
            "FallDeathDmgLoopB" => (None, "LandFallDeathDmgB".into()),
            "FallDeathDmgLoopF" => (None, "LandFallDeathDmgF".into()),
            "FallDeathElectroChargeStart" => (Some("FallDeathElectroChargeLoop".into()), "LandFallDeathElectroCharge".into()),
            "FallDeathElectroChargeLoop" => (None, "LandFallDeathElectroCharge".into()),
            "FallDeathElectroChargeWeakStart" => (Some("FallDeathElectroChargeWeakLoop".into()), "LandFallDeathElectroChargeWeak".into()),
            "FallDeathElectroChargeWeakLoop" => (None, "LandFallDeathElectroChargeWeak".into()),
            // HKS 1778: aged, W_LandAgingFallDeath.
            "AgingFallDeathStart" => (Some("AgingFallDeathLoop".into()), "LandAgingFallDeath".into()),
            "AgingFallDeathLoop" => (None, "LandAgingFallDeath".into()),
            _ => return,
        }
    };
    let ended = !a.anim.is_empty() && a.t >= d.length(&a.anim);
    let play = |a: &mut Actor, st: &str| a.play_state(d, st) || (st.starts_with("Air") && a.play_state(d, &st.replacen("Air", "Stand", 1)));
    match fall {
        // A loop lands when on the ground; a start once played out (still airborne: falls).
        None if !a.airborne => {
            play(a, &land);
        }
        Some(_) if ended && !a.airborne => {
            play(a, &land);
        }
        Some(f) if ended => {
            play(a, &f);
        }
        _ => {}
    }
}

/// Which ThrowKindParam categories (throwKindCategory) a defender's throwable state lets in:
/// FUN_140be0a40, run on the snapshot for states 0-127 x categories 0-21 / 100 / 101 (the 52 pairs
/// it passes; sim_tests/emu_throw.rs). The kind test FUN_140be3810 reads the state from the
/// defender's throw module (chr +0x1ff8 bag +0x88): +0x7a, written each frame by TAE 932 (unk0),
/// else +0x7b, its own (101 on every enemy of the snapshot, Wolf 1). 101 = an enemy as it
/// stands (behind 0, crouching 1, ...), 103 / 108 the break anims (TrunkCollapse* 0-75, 108 the
/// Large one), 104 / 105 the finisher windows (data.rs finisher_window), 1 / 2 / 4 Wolf's.
pub(crate) fn throwable_state_allows(state: u8, category: i64) -> bool {
    let cats: &[i64] = match state {
        1 | 2 => &[100],
        4 => &[101],
        101 => &[0, 1, 2, 3, 6, 7, 9, 10, 11, 12, 17],
        102 => &[2, 3, 7, 12],
        103 => &[4, 7, 12, 13, 14, 15, 18, 19, 20],
        104 => &[2, 3, 5, 7, 12],
        105 => &[2, 3, 7, 12, 16],
        106 | 109 | 110 => &[12],
        107 => &[8, 12],
        108 => &[4, 7, 12, 13, 14, 15, 18, 19],
        111 => &[12, 21],
        _ => &[],
    };
    cats.contains(&category)
}

/// An enemy's throwable state (throwable_state_allows) from what Sv1 tracks: the TAE 932 finisher
/// windows (104 / 105, data.rs finisher_window), the break window (Mode::Broken: the break anim's
/// TAE 932 103 / 108 span, enemy.rs break_window_end; row E31 brkwin), else its own 101.
/// Round throw2 (2026-11-03): TAE 932 is read in every anim (data.rs throw_state), so its 103 on
/// the death anims (DeathDefault 010000 / 010010 0-45 f), the ThrowDef / LandTrunk /
/// TrunkCollapseAerial spans, 107 (c1060 / c1080 / c1240 / c1250 / c1310 a000_0030xx, 008602)
/// and 111 (c1340 / c1350 a100_0083xx) count as in the exe; a scan of all enemy anims finds no 932
/// of 102 / 106 / 109 / 110 (those states are the table's only), and no snapshot chr holds one
/// as its own +0x7b (101 on every enemy, 255 on c7600).
pub(crate) fn def_throw_state(combat: &Combat, ea: &Actor, enemy: &Enemy) -> u8 {
    match combat.data_of(ea).throw_state(&ea.anim, ea.t) {
        Some(w) => w as u8,
        None if enemy.is_broken() => 103,
        None => 101,
    }
}

/// The kind test's gates past the state table (FUN_140be3810): the defender's action flags
/// (chr +0x1ff8 bag[0] +0x70) bit 0x20, set by ChrActionFlag 67 ThrowStart2 (TAE dispatcher
/// FUN_140b51b70 case 0x43; every ThrowDef anim's first frames), closes every kind; bit 0x10
/// (flag 68, in no enemy anim) the start kinds; vfunc 0x550 (FUN_1409f3250) closes them while
/// the chr is invulnerable: bits 0 / 1 (TAE 951 / 950). gap: its chr flags +0x1a11 & 0x20 /
/// +0x1a13 & 2 (event invincibility) are not tracked.
pub(crate) fn throw_kind_closed(combat: &Combat, ea: &Actor) -> bool {
    if ea.anim.is_empty() {
        return false;
    }
    let d = combat.data_of(ea);
    d.flag(&ea.anim, ea.t, 67) || d.events_at(&ea.anim, ea.t).any(|e| e.kind == 950 || e.kind == 951)
}

/// A ThrowParam row's facing test (the exe's range check FUN_140be0270, round 41 trace; the
/// `_start` columns are read only by a debug drawer and are 0 on every row):
/// - diffAngMyToDef: the angle between the thrower's forward and the 3D direction to the
///   defender, at most this (180 = any);
/// - DiffAngMin / DiffAngMax: |wrap(defender yaw - thrower yaw)| in degrees, inclusive - 0 both
///   face the same way (the thrower behind), 180 face to face.
pub(crate) fn throw_facing_ok(combat: &Combat, row_id: i64, thrower: Vec3, thrower_fwd: Vec3, defender: Vec3, defender_fwd: Vec3) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let f = |k: &str, or: f32| row[k].as_f64().map_or(or, |v| v as f32);
    let to_def = (defender - thrower).normalize_or_zero();
    let my_to_def = f("diffAngMyToDef", 180.0);
    if my_to_def < 180.0 && to_def != Vec3::ZERO && thrower_fwd.angle_between(to_def).to_degrees() > my_to_def {
        return false;
    }
    let yaw = thrower_fwd.with_y(0.0).normalize_or_zero().angle_between(defender_fwd.with_y(0.0).normalize_or_zero()).to_degrees();
    yaw.is_nan() || (f("DiffAngMin", 0.0)..=f("DiffAngMax", 180.0)).contains(&yaw)
}

/// A ThrowParam row's height window (FUN_140be0270): the defender less than upperYRange above
/// and less than lowerYRange below the thrower, both strict (a row with neither set takes no one;
/// only the empty row 0 is such).
pub(crate) fn throw_height_ok(combat: &Combat, row_id: i64, thrower: Vec3, defender: Vec3) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let dy = defender.y - thrower.y;
    dy < f("upperYRange") && dy > -f("lowerYRange")
}

/// A ThrowParam row's whole range test, FUN_140be0270 (run on the snapshot, Wolf -> the General,
/// 3,000 placements: sim_tests/emu_throw.rs): the height window (throw_height_ok), then within
/// Dist in 3D (strict, squared as the exe), then the facings (throw_facing_ok). Positions are
/// the chrs' roots (FUN_140be3290 takes judgeRangeBasePosDmyId1 / 2 instead when set: no row of
/// Wolf's start throws sets them). gap: the row's other tests in FUN_140be3a90 - the
/// additionalConditionSpecialEffect pair, the fall orbit (FUN_140be3ba0, the plunge's own
/// fall_reaches) and the sight cast between the two (FUN_140be3da0) are throw_speffects_ok,
/// throw_fall_orbit_ok and throw_sight_ok.
pub(crate) fn throw_range_ok(combat: &Combat, row_id: i64, thrower: Vec3, thrower_fwd: Vec3, defender: Vec3, defender_fwd: Vec3) -> bool {
    let dist = combat.param("ThrowParam", row_id)["Dist"].as_f64().unwrap_or(0.0) as f32;
    throw_height_ok(combat, row_id, thrower, defender) && thrower.distance_squared(defender) < dist * dist && throw_facing_ok(combat, row_id, thrower, thrower_fwd, defender, defender_fwd)
}

/// A ThrowParam row's SpEffect pair, the first test of FUN_140be3a90: additionalConditionSpecialEffect
/// _forAtk (+0x6c) on the thrower and _forDef (+0x70) on the defender, each -1 or active
/// (FUN_1409f1c80 walks the chr's SpEffect list, chr +0x11d0). Run on the snapshot (General's
/// 11020190, forDef 200030 which he wears; the pair poked to ids worn or not: 16 cases,
/// sim_tests/emu_throw2.rs).
pub(crate) fn throw_speffects_ok(combat: &Combat, row_id: i64, atk_has: &dyn Fn(i64) -> bool, def_has: &dyn Fn(i64) -> bool) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let id = |k: &str| row[k].as_i64().unwrap_or(-1);
    speffect_pair_ok(id("additionalConditionSpecialEffect_forAtk"), id("additionalConditionSpecialEffect_forDef"), atk_has, def_has)
}

pub(crate) fn speffect_pair_ok(atk_id: i64, def_id: i64, atk_has: &dyn Fn(i64) -> bool, def_has: &dyn Fn(i64) -> bool) -> bool {
    (atk_id == -1 || atk_has(atk_id)) && (def_id == -1 || def_has(def_id))
}

/// FUN_140802730: the two sight casts' heights as fractions of each chr's height (DAT_143b015a0,
/// a debug-menu slider; 0.8 / 0.42 in the exe's data and on the snapshot).
pub(crate) const THROW_SIGHT_FRACS: [f32; 2] = [0.8, 0.42];
/// Wolf's physics height (+0xd0, FUN_140bbf730) and radius (FUN_140bbf740) on the snapshot.
pub(crate) const WOLF_PHYS_HEIGHT: f32 = 1.5;

/// A ThrowParam row's sight test, FUN_140be3da0, the last of FUN_140be3a90: unless the row sets
/// isSkipSphereCast (+0x3b bit 2), two sphere casts from the thrower to the defender, one per
/// THROW_SIGHT_FRACS height (each chr's point raised by its height x the fraction), radius =
/// the larger of the two chrs' radii x sphereCastRadiusRateTop (+0x30, the first, upper cast) /
/// Low (+0x32, the lower) / 100 (no cast when 0; FUN_140be3da0 reads +0x30 for cast 0 at 0.8 and
/// +0x32 for cast 1 at 0.42: the two were swapped here, which only the Chained Ogre's grab rows
/// (AtkChrId 5020) 25020000 / 25020100 / 25021000 / 25021100, Top 29 / Low 51, tell apart;
/// kb/params.md). A cast passes with no hit, or a hit (the sphere's centre at contact)
/// farther from the start than the defender's point; both must pass. `cast(start, delta,
/// radius)` sweeps the sphere along delta. Run on the snapshot (FUN_14094d8d0 with filter 0x4c,
/// 1,200 placements Wolf -> the General: the same casts and this rule give the exe's answer on
/// all; sim_tests/emu_throw2.rs). Points are the feet (FUN_1409f0870; the rows'
/// judgeRangeBasePosDmyId are -1 on Wolf's).
#[allow(clippy::too_many_arguments)]
pub(crate) fn throw_sight_ok(combat: &Combat, row_id: i64, thrower: Vec3, thr_height: f32, thr_radius: f32, defender: Vec3, def_height: f32, def_radius: f32, cast: &dyn Fn(Vec3, Vec3, f32) -> Option<Vec3>) -> bool {
    let row = combat.param("ThrowParam", row_id);
    if row["isSkipSphereCast"].as_i64().unwrap_or(0) != 0 {
        return true;
    }
    let rates = ["sphereCastRadiusRateTop", "sphereCastRadiusRateLow"].map(|k| row[k].as_f64().unwrap_or(0.0) as f32);
    (0..2).all(|i| {
        let radius = rates[i] / 100.0 * thr_radius.max(def_radius);
        if radius <= 0.0 {
            return true;
        }
        let start = thrower + Vec3::Y * thr_height * THROW_SIGHT_FRACS[i];
        let delta = defender + Vec3::Y * def_height * THROW_SIGHT_FRACS[i] - start;
        cast(start, delta, radius).is_none_or(|hit| delta.length_squared() < (hit - start).length_squared())
    })
}

/// The sight cast on Sv1's map: the centre ray along `delta` (map::raycast), the sphere's centre
/// put back by its radius from the hit. gap: a ray, not the exe's swept sphere (radius 0.2 m
/// for Wolf -> the General), and only the hit collision (the exe's filter 0x4c not known).
pub(crate) fn map_sphere_cast(start: Vec3, delta: Vec3, radius: f32) -> Option<Vec3> {
    let len = delta.length();
    if len < 1e-5 {
        return None;
    }
    let dir = delta / len;
    crate::terrain::raycast(start, dir, len + radius).map(|d| start + dir * (d - radius).max(0.0))
}

/// The orbit FUN_140be3ba0 predicts (FUN_140baedb0 -> FUN_140bb2850): from `p0` with the chr's
/// velocity `v`, decelerating ORBIT_ACCEL[0] along its horizontal heading and falling
/// ORBIT_ACCEL[1] (DAT_143b09ce8 / ec plus physics +0x1c4, 0 on the snapshot), the point
/// nearest `target` and its time. The target is taken at its full horizontal distance along the
/// heading (the solver works in the heading / up plane): one real root of the cubic -> that time
/// (at least 0); three -> the closest point on the orbit sampled every 1/6 s (16 points) and the
/// later time the fall reaches its height (9999 when it never does); no horizontal speed -> the
/// target's height on the plumb line (or the apex). Checked on 400 runs of FUN_140bb2850
/// (sim_tests/emu_throw2.rs).
pub(crate) const ORBIT_ACCEL: [f32; 2] = [-0.1, -13.72];
pub(crate) fn fall_orbit_closest(p0: Vec3, v: Vec3, target: Vec3) -> (Vec3, f32) {
    let [ah, av] = ORBIT_ACCEL;
    let s = v.with_y(0.0).length();
    let vy = v.y;
    let dy = target.y - p0.y;
    let reach_time = |y: f32| {
        // av t^2 + 2 vy t - 2 y = 0, the later root (FUN_140bb5b50).
        let disc = 4.0 * vy * vy + 8.0 * av * y;
        if disc < 0.0 {
            return 9999.0;
        }
        let r = disc.sqrt();
        ((-2.0 * vy + r) / (2.0 * av)).max((-2.0 * vy - r) / (2.0 * av))
    };
    if s <= 0.0 {
        let top = (vy * vy * -0.5 * av).max(0.0);
        if dy < top {
            return (Vec3::new(p0.x, target.y, p0.z), reach_time(dy));
        }
        return (p0 + Vec3::Y * top, -vy / av);
    }
    let dir = v.with_y(0.0) / s;
    let d = (target - p0).with_y(0.0).length();
    let at = |t: f32| p0 + dir * (t * t * ah * 0.5 + s * t) + Vec3::Y * (t * t * av * 0.5 + vy * t);
    let roots = cubic_real_roots(-0.5 * (av * av + ah * ah), -1.5 * (av * vy + ah * s), dy * av + d * ah - s * s - vy * vy, dy * vy + d * s);
    if roots.len() == 1 {
        let t = if roots[0].is_nan() { 0.0 } else { roots[0].max(0.0) };
        return (at(t), t);
    }
    let pts: Vec<Vec3> = (0..16).map(|k| at(k as f32 / 6.0)).collect();
    let mut best = (f32::MAX, p0);
    for w in pts.windows(2) {
        let ab = w[1] - w[0];
        let u = if ab.length_squared() > 0.0 { ((target - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let q = w[0] + ab * u;
        if q.distance_squared(target) < best.0 {
            best = (q.distance_squared(target), q);
        }
    }
    (best.1, reach_time(best.1.y - p0.y))
}

/// Real roots of a t^3 + b t^2 + c t + d (one, or three when the discriminant allows).
fn cubic_real_roots(a: f32, b: f32, c: f32, d: f32) -> Vec<f32> {
    let (b, c, d) = ((b / a) as f64, (c / a) as f64, (d / a) as f64);
    let p = c - b * b / 3.0;
    let q = 2.0 * b * b * b / 27.0 - b * c / 3.0 + d;
    let disc = (q / 2.0).powi(2) + (p / 3.0).powi(3);
    if disc > 0.0 {
        let s = disc.sqrt();
        return vec![((-q / 2.0 + s).cbrt() + (-q / 2.0 - s).cbrt() - b / 3.0) as f32];
    }
    let r = 2.0 * (-p / 3.0).sqrt();
    let phi = (3.0 * q / (p * r)).clamp(-1.0, 1.0).acos();
    (0..3).map(|k| (r * ((phi - 2.0 * std::f64::consts::PI * k as f64) / 3.0).cos() - b / 3.0) as f32).collect()
}

/// A ThrowParam row's fall-orbit test FUN_140be3ba0, for kinds with doesCheckNormalFallOrbit
/// (the plunge 130000 / 630000, kick 160000 / 660000, stealth plunge 70000 / 570000 ...): the
/// thrower's fall top (physics +0x854, Sv1's air_top) above the defender more than
/// normalFallOrbitCheck_heightLimit, the orbit's nearest point (fall_orbit_closest) within
/// _range of the defender in 3D, reached within _timeLimit (ms, cut to whole seconds by the
/// exe's FD4Time from (int)(ms / 1000)). Checked on 450 placements of Wolf above the General
/// (plunge 11020150, kick 11020160, stealth plunge 11020030; sim_tests/emu_throw2.rs).
pub(crate) fn throw_fall_orbit_ok(combat: &Combat, row_id: i64, thrower: Vec3, fall_top: f32, vel: Vec3, defender: Vec3) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let kind = row["throwKind"].as_i64().unwrap_or(-1);
    if combat.param("ThrowKindParam", kind)["doesCheckNormalFallOrbit"].as_i64().unwrap_or(0) == 0 {
        return true;
    }
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    if fall_top - defender.y <= f("normalFallOrbitCheck_heightLimit") {
        return false;
    }
    let (q, t) = fall_orbit_closest(thrower, vel, defender);
    let limit = (row["normalFallOrbitCheck_timeLimit"].as_i64().unwrap_or(0) / 1000) as f32;
    q.distance(defender) <= f("normalFallOrbitCheck_range") && t <= limit
}

/// The main deathblow throw: Wolf's ThrowParam anim, the enemy's ThrowDef(Death) anim, and the
/// absorb onto the throw dummy (follow_throw). Facing here is the fallback without models.
#[allow(clippy::too_many_arguments)]
fn main_deathblow(
    a: &mut Actor,
    pos: Vec3,
    ea: &mut Actor,
    enemy: &mut Enemy,
    epos: Vec3,
    ee: Entity,
    combat: &Combat,
    th: &crate::data::Throw,
    behind: bool,
    throw: &mut ActiveThrow,
    log: &mut CombatLog,
) {
    let to_enemy = (epos - pos).with_y(0.0).normalize_or_zero();
    a.yaw = yaw_of(to_enemy);
    if combat.player.anim(&th.atk_anim).is_some() {
        a.play("Deathblow", &th.atk_anim);
        a.move_vel = Vec3::ZERO;
    }
    // The defender faces Wolf, or away from him for the behind deathblow.
    ea.yaw = if behind { yaw_of(to_enemy) } else { yaw_of(-to_enemy) };
    enemy.deathblow(ea, combat, th.def_anim);
    throw.0 = Some(ThrowHold::new(ee, th, "Deathblow"));
    log.push("DEATHBLOW", Color::srgb(1.0, 0.2, 0.2));
}

/// Throw absorb (ThrowParam): when a paired throw starts, the defender's root is put on the
/// attacker's dummy poly atkSorbDmyId (front deathblow 266, deflect-break 246, Mikiri 523) or the
/// attacker on the defender's defSorbDmyId (behind 229, plunge landing 233), facing along the
/// dummy's forward. Live (rec_20261008_054259): the General snaps to exactly 1.50 m in front of
/// Wolf, facing him (180.0 deg), on the main throw's first frame; after that only the two anims'
/// root motion moves them (1.50 -> 0.85 -> 2.35 m), so it is applied once, not held.
#[allow(clippy::type_complexity)]
fn follow_throw(
    mut commands: Commands,
    combat: Res<Combat>,
    mut throw: ResMut<ActiveThrow>,
    mut player: Single<(Entity, &mut Actor, &mut Transform, Option<&crate::model::Dummies>, &GlobalTransform), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&mut Actor, &mut Transform, Option<&crate::model::Dummies>, &GlobalTransform), (With<Enemy>, Without<Player>)>,
    globals: Query<&GlobalTransform, (Without<Player>, Without<Enemy>)>,
) {
    let Some(h) = throw.0.as_mut() else { return };
    let (pe, pa, ptf, pdm, pg) = &mut *player;
    let Ok((mut ea, mut etf, edm, eg)) = enemies.get_mut(h.target) else {
        throw.0 = None;
        return;
    };
    if pa.state != h.state {
        throw.0 = None;
        return;
    }
    // The dummy's position and forward (+Z, model_<chr>.dummies.json) in its character's local
    // space (last drawn pose), to be placed on the simulated transform. Id 0 = the root.
    let local = |dm: Option<&crate::model::Dummies>, g: &GlobalTransform, id: i16| -> Option<(Vec3, Vec3)> {
        if id == 0 {
            return Some((Vec3::ZERO, Vec3::Z));
        }
        let dg = globals.get(*dm?.0.get(&id)?).ok()?;
        let inv = g.affine().inverse();
        Some((inv.transform_point3(dg.translation()), inv.transform_vector3(dg.rotation() * Vec3::Z)))
    };
    // The absorbed character moves onto the dummy and faces along its forward (front deathblow:
    // 266 faces back at Wolf; behind: 229 faces the enemy's way; plunge: 233 faces the enemy).
    // Its model slides there from where it stood (ModelLag, adsrobModelPosInterpolationTime).
    // Walls: the move goes through the map collision (Terrain::slide); what a wall stops, the
    // other character gives way instead (its own slide), so the pair keeps its spacing without
    // either ending up in geometry. After AKJama/sekiro-rs docs/DEATHBLOW.md "Walls" (theirs too:
    // the engine's handling is not traced). Returns that shortfall.
    let interp = h.model_interp;
    let slide = |at: Vec3, d: Vec3| -> Vec3 {
        match crate::terrain::terrain() {
            Some(t) => {
                let m = t.slide(at - Vec3::Y * CAPSULE_HALF_HEIGHT, Vec2::new(d.x, d.z));
                Vec3::new(m.x, 0.0, m.y)
            }
            None => d,
        }
    };
    let mut absorb = |e: Entity, tf: &mut Transform, a: &mut Actor, owner: &Transform, (l, f): (Vec3, Vec3)| -> Vec3 {
        let goal = owner.transform_point(l);
        let want = (goal - tf.translation).with_y(0.0);
        let jump = slide(tf.translation, want);
        tf.translation += jump;
        if interp > 0.0 && jump.length_squared() > 1e-6 {
            commands.entity(e).insert(crate::anim::ModelLag { offset: -jump, time: interp, left: interp });
        }
        let dir = (owner.rotation * f).with_y(0.0);
        if dir.length_squared() > 1e-6 {
            let want = yaw_of(dir);
            let diff = (want - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            a.yaw += diff;
            tf.rotation = Quat::from_rotation_y(a.yaw);
        }
        want - jump
    };
    if h.def_dmy != 0 {
        let Some(l) = local(edm, eg, h.def_dmy) else {
            warn!("deathblow: enemy has no dummy poly {} (throw follow off)", h.def_dmy);
            throw.0 = None;
            return;
        };
        let owner = *etf;
        let short = absorb(*pe, ptf, pa, &owner, l);
        if short.length_squared() > 1e-6 {
            let give = slide(etf.translation, -short);
            etf.translation += give;
        }
    } else {
        let Some(l) = local(*pdm, pg, h.atk_dmy) else {
            warn!("deathblow: Wolf has no dummy poly {} (throw follow off)", h.atk_dmy);
            throw.0 = None;
            return;
        };
        // The defender is placed on Wolf as he stands at the throw anim's first frame. This runs a
        // step into the anim, after its root motion already moved / turned Wolf (the vault 511900
        // turns him 180 deg in ~10 frames: absorbed on the turned Wolf, the enemy ended ~10 deg
        // off; live the two line up straight, the enemy turning 3 deg).
        let mut owner = **ptf;
        if let (Some((p0, y0)), Some((p1, y1))) = (combat.player.root_at(&pa.anim, 0.0), combat.player.root_at(&pa.anim, pa.t)) {
            let yaw0 = pa.yaw - (y1 - y0);
            let moved = Quat::from_rotation_y(yaw0 - y0 + pa.root_yaw) * Vec3::new(p1.x - p0.x, 0.0, p1.y - p0.y) * pa.root_scale;
            owner.translation -= moved;
            owner.rotation = Quat::from_rotation_y(yaw0);
        }
        let short = absorb(h.target, &mut etf, &mut ea, &owner, l);
        if short.length_squared() > 1e-6 {
            let give = slide(ptf.translation, -short);
            ptf.translation += give;
        }
    }
    throw.0 = None;
}

/// The equipped art's "has emblems" StateInfo, put on by common.emevd events 9930-9934
/// (`emevd::art_gates`) while Wolf has the art's resident SpEffect and at least the event's
/// emblem count: Dragon Flash 140300 -> 140310 stateInfo 995 (2), Spiral Cloud Passage 140501 ->
/// 140510 994 (1), Mortal Draw 140600 / 140601 -> 140610 993 (3), 100286 (art 6100) -> 140410 990
/// (3). Ashina Cross (5500, 140400) has none. Without the event file: the SpEffect 10 after the
/// resident, at the art's resourceItemA (the old reading from ids and names).
fn art_emblem_gate(combat: &Combat, config: &GameConfig, emblems: u32) -> Option<i64> {
    let resident = combat.param("EquipParamWeapon", config.player.combat_art)["residentSpEffectId"].as_i64().filter(|&r| r > 0)?;
    let gate = match crate::emevd::art_gates() {
        Some(gates) => gates.iter().find(|g| g.residents.contains(&resident) && emblems >= g.emblems)?.gate,
        None => {
            let cost = art_cost(combat, config);
            if cost == 0 || emblems < cost {
                return None;
            }
            resident / 100 * 100 + 10
        }
    };
    combat.param("SpEffectParam", gate)["stateInfo"].as_i64().filter(|&s| s > 0)
}

/// Keeps CharData.art_variation on the equipped art (config player.combat_art, also after F5),
/// and tool_variation / resident_gates on the equipped prosthetic tool level (Z switches it).
fn sync_art_variation(config: Res<GameConfig>, player: Query<&Player>, mut combat: ResMut<Combat>) {
    let slot = player.iter().next().map_or(0, |p| p.tool_slot);
    let tool = equipped_tool(&combat, &config, slot).map(|t| (t.id, t.variation));
    let var = combat.param("EquipParamWeapon", config.player.combat_art)["behaviorVariationId"].as_i64();
    // The resident SpEffects' StateInfos (sound::active_state_infos without the TAE SpEffects).
    let probe = Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0);
    let mut gates = crate::sound_rules::active_state_infos(&combat, &probe, Some(&config), tool.map(|t| t.0));
    // The art's "has emblems" StateInfo (`art_emblem_gate`): Mortal Draw's emblem slash (judges
    // 220-231) and consumption dummy 999 (993), Spiral Cloud Passage's cost 999 (994) fire only with it.
    let emblems = player.iter().next().map_or(0, |p| p.emblems);
    if let Some(gate) = art_emblem_gate(&combat, &config, emblems) {
        gates.push(gate);
    }
    // The timed SpEffects' StateInfos (round 59: the Flame Vent's enchantment 106020 is 62,
    // the Spirit's 106030 64, gating the sword's fire / spirit judges 290 / 295).
    if let Some(p) = player.iter().next() {
        let infos: Vec<i64> = p.timed.iter().filter_map(|(id, _)| combat.player.sp_effects.get(&id.to_string())).map(|s| s.state_info).collect();
        for s in infos {
            if s > 0 && !gates.contains(&s) {
                gates.push(s);
            }
        }
    }
    let tool_var = tool.map(|t| t.1);
    if combat.player.art_variation != var || combat.player.tool_variation != tool_var || combat.player.resident_gates != gates {
        combat.player.art_variation = var;
        combat.player.tool_variation = tool_var;
        combat.player.resident_gates = gates;
    }
}

/// Wolf's facing at the start (and on a retry): toward the enemies' line-up (enemy.rs
/// spawn_lineup: the first enemy at x 0, z -4), the way the game lets him walk into an arena
/// facing the boss.
pub(crate) fn start_yaw() -> f32 {
    yaw_of((crate::place::frame().point(Vec3::new(0.0, 0.0, -4.0)) - (crate::encounter::hooks().player_start)()).with_y(0.0))
}

/// Graphs 500 / 501 / 504 = max HP / max posture / posture regen by level (exe: FUN_140a368f0
/// stores 500 -> PlayerGameData +0x20 and 501 -> +0x3c; 504 in FUN_140850b10's regen caller).
fn level_stats(combat: &Combat, lvl: f32) -> (f32, f32, f32) {
    let g = |id| calc_correct(combat.param("CalcCorrectGraph", id), lvl);
    (g(500), g(501), g(504))
}

fn spawn_player(mut commands: Commands, combat: Res<Combat>, config: Res<GameConfig>) {
    let (hp, posture, regen) = level_stats(&combat, config.posture.player_level);
    let mut actor = Actor::new(Side::Player, hp, posture, regen, 0);
    actor.yaw = start_yaw();
    // Resident SpEffects of the default outfit (EquipParamProtector residentSpEffectId1-3):
    // 5221-5223 cut posture regen to x0.75 / x0.5 / x0.25 at <= 75 / 50 / 25 % HP.
    if let Some(rows) = combat.params.get("EquipParamProtector").and_then(|v| v.as_object()) {
        for r in rows.values() {
            for k in ["residentSpEffectId", "residentSpEffectId2", "residentSpEffectId3"] {
                if let Some(id) = r[k].as_i64().filter(|&v| v > 0) {
                    actor.resident.push(id);
                }
            }
        }
    }
    commands.spawn((
        Player { deathblow_ready: false, jump_throw_ready: false, plunge_ready: false, plunge: None, leap_homing: None, dock: None, wall_jumps: 0, last_wall: None, seat: Default::default(), wall_contact: None, speed_level: 0.0, turn: Turn::default(), requests: HashMap::new(), last_state: String::new(), gourd: config.player.gourd_charges, emblems: config.player.spirit_emblems, auto_aim: false, auto_aim_fresh: false, resurrections: config.player.resurrections, revive_queued: false, death_wait: 0.0, dead_for: 0.0, escape: None, aged_form: false, step_tilt: 0.0, jump_tilt: 0.0, throw_start: None, throw_target: None, tool_slot: 0, quick_slot: 0, item_use: None, vel_scale: None, expand_pending: false, sub_cat_before: 0, art_enable_jump: false, timed: Vec::new(), held: Vec::new(), wire: None, perch: None, wire_todome: false, air_top: 0.0, fall_from: None, safe_ring: Default::default(), swim: None, ground_vel: Vec3::ZERO, ground_pos: Vec3::ZERO, revive_open: false, gave_up: false, revive_locked: false, item_hp_rate: 1.0, item_posture_rate: 1.0, sp_bursts: Vec::new(), res_power: 0, refill_items: false },
        actor,
        Name::new("Player"),
        Transform::from_translation((crate::encounter::hooks().player_start)()),
        Visibility::default(),
    ));
}

/// Capsule + blade for the player (windowed app only; the simulation tests skip it).
pub fn player_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // Runs after the skeletons (world.rs): Wolf with one has his model, no capsule.
    q: Query<Entity, (With<Player>, Without<Mesh3d>, Without<crate::anim::Skeleton>)>,
) {
    for e in &q {
        commands
            .entity(e)
            .insert((
                Mesh3d(meshes.add(Capsule3d::new(CAPSULE_RADIUS, CAPSULE_MIDDLE))),
                MeshMaterial3d(materials.add(Color::srgb(0.85, 0.8, 0.72))),
            ))
            .with_children(|p| crate::model::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.8, 0.85, 0.9)));
    }
}

/// Walk / run by the left stick, with hysteresis (c0000_transition `_SpeedUpdate`): walking,
/// Wolf starts running above PRM_RUN_STICK_LEVEL_WALK_TO_RUN 0.9; running, he keeps running
/// above PRM_RUN_STICK_LEVEL_RUN_TO_WALK 0.7 (c0000_define 205-206; PRM_RUN_STICK_LEVEL 0.75 has
/// no reader). gap: whether the exe rescales the stick level before the HKS reads it.
const RUN_STICK_WALK_TO_RUN: f32 = 0.9;
const RUN_STICK_RUN_TO_WALK: f32 = 0.7;

fn stick_runs(was_running: bool, level: f32) -> bool {
    level > if was_running { RUN_STICK_RUN_TO_WALK } else { RUN_STICK_WALK_TO_RUN }
}

/// Sekiro's pad layout. From DefaultKeyAssignParam00 (pad key ids beside the PC keys of the same
/// game action): attack 2008 = mouse 1, guard 2009 = mouse 2, lock-on 2011 = mouse 3 (middle),
/// so 2008 / 2009 / 2011 are R1 / L1 / R3; the prosthetic (3001) and grapple (3000) are the
/// analog R2 / L2. gap: the face buttons' and d-pad's ids (2000-2007, 2010) are not decoded to
/// buttons (no names in Paramdex SDT); the game's default layout is used: cross jump, circle
/// step / sprint, square item, triangle and d-pad up next tool, L3 crouch.
pub(crate) fn read_pad(pads: &Query<&Gamepad>, enabled: bool, pad: &mut PadInput) {
    if !enabled {
        return;
    }
    let mut level = 0.0f32;
    let mut stick_won = false;
    for g in pads {
        let stick = g.left_stick();
        level = level.max(stick.length());
        if stick.length() > pad.stick.length() {
            pad.stick = stick.clamp_length_max(1.0);
            stick_won = true;
        }
        pad.attack_held |= g.pressed(GamepadButton::RightTrigger);
        pad.guard_held |= g.pressed(GamepadButton::LeftTrigger);
        pad.dodge_held |= g.pressed(GamepadButton::East);
        pad.grapple_held |= g.pressed(GamepadButton::LeftTrigger2);
        pad.prosthetic_held |= g.pressed(GamepadButton::RightTrigger2);
        let mut press = |b: GamepadButton, a: Action| {
            if g.just_pressed(b) {
                pad.pressed.push(a);
            }
        };
        press(GamepadButton::RightTrigger, Action::Attack);
        press(GamepadButton::LeftTrigger, Action::Guard);
        press(GamepadButton::East, Action::Step);
        press(GamepadButton::South, Action::Jump);
        press(GamepadButton::West, Action::UseItem);
        press(GamepadButton::RightTrigger2, Action::Prosthetic);
        press(GamepadButton::LeftThumb, Action::Crouch);
        press(GamepadButton::North, Action::SwitchTool);
        press(GamepadButton::DPadUp, Action::SwitchTool);
        press(GamepadButton::LeftTrigger2, Action::Grapple);
        press(GamepadButton::DPadDown, Action::NextItem);
    }
    // Updated every frame (a released stick drops back to walk) so the hysteresis has a history.
    pad.pad_running = stick_runs(pad.pad_running, level);
    if stick_won {
        pad.walk = !pad.pad_running;
    }
}

pub(crate) fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut pad: ResMut<PadInput>,
    pads: Query<&Gamepad>,
    config: Res<GameConfig>,
) {
    let mut stick = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { stick.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) { stick.y -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { stick.x += 1.0; }
    if keys.pressed(KeyCode::KeyA) { stick.x -= 1.0; }
    pad.stick = stick.normalize_or_zero();
    // Keyboard defaults are the game's own: param DefaultKeyAssignParam00 (keyboardKeyId =
    // DirectInput scan code + 69) per gameAction, whose meaning is the key guide's
    // <?kgiconKc@N?> text (menu.msgbnd キーガイド.fmg): 0 walk Left Alt, 1-4 WASD, 5 step / sprint
    // Shift, 6 jump Space, 7 crouch Q, 12 lock-on middle mouse (camera.rs), 15 prosthetic tool
    // switch Z, 16 attack LMB, 17 prosthetic Left Ctrl, 18 guard RMB, 19 grappling hook F,
    // 20 talk / examine / wall hug / ledge hang E (docking.rs, not built yet), 21 use quick item
    // R, 13 / 14 quick item next / previous X / C, 22 eavesdrop V. J / K stay as extra attack /
    // guard keys.
    pad.walk = keys.pressed(KeyCode::AltLeft);
    let captured = cursor.grab_mode != bevy::window::CursorGrabMode::None;
    // Mouse buttons only count once the mouse is captured (the first click captures it).
    let lmb = captured && mouse.pressed(MouseButton::Left);
    let rmb = captured && mouse.pressed(MouseButton::Right);
    pad.attack_held = lmb || keys.pressed(KeyCode::KeyJ);
    pad.guard_held = rmb || keys.pressed(KeyCode::KeyK);
    pad.dodge_held = keys.pressed(KeyCode::ShiftLeft);
    pad.grapple_held = keys.pressed(KeyCode::KeyF);
    if (captured && mouse.just_pressed(MouseButton::Left)) || keys.just_pressed(KeyCode::KeyJ) {
        pad.pressed.push(Action::Attack);
    }
    if (captured && mouse.just_pressed(MouseButton::Right)) || keys.just_pressed(KeyCode::KeyK) {
        pad.pressed.push(Action::Guard);
    }
    if keys.just_pressed(KeyCode::ShiftLeft) {
        pad.pressed.push(Action::Step);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        pad.pressed.push(Action::UseItem);
    }
    if keys.just_pressed(KeyCode::ControlLeft) {
        pad.pressed.push(Action::Prosthetic);
    }
    pad.prosthetic_held = keys.pressed(KeyCode::ControlLeft);
    if keys.just_pressed(KeyCode::Space) {
        pad.pressed.push(Action::Jump);
    }
    // Sandbox reset (not a game key; R is the game's switch-prosthetic key).
    if keys.just_pressed(KeyCode::Backspace) {
        pad.reset = true;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        pad.sheathe = true;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        pad.pressed.push(Action::Crouch);
    }
    // Talk / examine / wall hug / ledge hang: E (gameAction 20; docking.rs).
    if keys.just_pressed(KeyCode::KeyE) {
        pad.pressed.push(Action::Interact);
    }
    pad.interact_held = keys.pressed(KeyCode::KeyE);
    // Switch prosthetic tool: Z (gameAction 15).
    if keys.just_pressed(KeyCode::KeyZ) {
        pad.pressed.push(Action::SwitchTool);
    }
    // Next quick item: C (gameAction 14's key; 13's X sheathes here).
    if keys.just_pressed(KeyCode::KeyC) {
        pad.pressed.push(Action::NextItem);
    }
    // Grappling Hook: F, the game's key for the pad's L2.
    if keys.just_pressed(KeyCode::KeyF) {
        pad.pressed.push(Action::Grapple);
    }
    read_pad(&pads, config.input.gamepad, &mut pad);
}

/// The equipped prosthetic tool level: config player.prosthetics[slot] (EquipParamWeapon 7xxxx).
/// Lightning Reversal air states: the loop each charge state runs into (c0000 behavior:
/// AirDamageElectroCharge(Weak)Start -> ...Loop, Air(Weak)ElectroChargeDeflect{Easy,Hard} ->
/// ...FallLoop); the loops themselves loop.
/// A grab's blow-away end (c0000_transition 386-399, STATE_TYPE_REACTION_THROW_DEF's end): its
/// fall loop and landing. ThrowId 600030 / 600040 -> W_ThrowDefLargeBlowStart (a000_199201),
/// landing as W_StandDamageLargeBlowLand (1826; DamageDirection B: _B); the Guardian Ape's
/// 600380 its own chain (199231 / 199241 / 199251), at HP <= 0 the Death one (199261 / 199271 /
/// 199281, IndexDeath COMMON_BACK; lands 1805 / 1883). Round 37: both went straight to idle.
fn throw_blow(state: &str) -> Option<(&'static str, &'static str)> {
    match state {
        "ThrowDefLargeBlowStart" | "ThrowDefLargeBlowFallLoop" => Some(("ThrowDefLargeBlowFallLoop", "StandDamageLargeBlowLand_B")),
        "ThrowDefLargeBlowStart600380" | "ThrowDefLargeBlowLoop600380" => Some(("ThrowDefLargeBlowLoop600380", "ThrowDefLargeBlowLand600380")),
        "ThrowDefDeathLargeBlowStart600380" | "ThrowDefDeathLargeBlowLoop600380" => Some(("ThrowDefDeathLargeBlowLoop600380", "ThrowDefDeathLargeBlowLand600380")),
        _ => None,
    }
}

/// The grab's end state for Wolf's grab clip (`a{group}_{ThrowId}`): the blow-away start of
/// `throw_blow`, else None (back to idle).
pub(crate) fn grab_end_state(anim: &str, hp: f32) -> Option<&'static str> {
    match anim.get(5..).and_then(|n| n.parse::<i64>().ok())? {
        600030 | 600040 => Some("ThrowDefLargeBlowStart"),
        600380 if hp <= 0.0 => Some("ThrowDefDeathLargeBlowStart600380"),
        600380 => Some("ThrowDefLargeBlowStart600380"),
        _ => None,
    }
}

/// combat.rs's grab end: the blow-away start where the throw has one, else idle.
pub(crate) fn grab_end(a: &mut Actor, d: &CharData) {
    if !grab_end_state(&a.anim, a.hp).is_some_and(|s| a.play_state(d, s)) {
        a.play("StandIdle", "");
    }
}

/// One step of a `throw_blow` chain: the start into its loop, the loop looping, and the landing
/// once on the ground after the start. Returns whether `a` is in one.
fn throw_blow_step(a: &mut Actor, d: &CharData) -> bool {
    let Some((lp, land)) = throw_blow(&a.state) else {
        return false;
    };
    let len = d.length(&a.anim);
    let ended = a.anim.is_empty() || a.t >= len;
    if !a.airborne && (a.state == lp || ended) {
        a.play_state(d, land);
    } else if ended && a.state == lp {
        a.t %= len.max(1e-3);
    } else if ended && !a.play_state(d, lp) {
        a.t = len;
    }
    true
}

pub fn equipped_tool<'a>(combat: &'a Combat, config: &GameConfig, slot: usize) -> Option<&'a crate::data::Prosthetic> {
    let tools = &config.player.prosthetics;
    let id = *tools.get(slot % tools.len().max(1))?;
    combat.player.prosthetics.iter().find(|t| t.id == id)
}

/// A prosthetic tool's own clip (groups a070..a079).
pub fn is_tool_anim(anim: &str) -> bool {
    anim.get(1..4).and_then(|g| g.parse::<i64>().ok()).is_some_and(|g| (70..=79).contains(&g))
}

/// A prosthetic state's anim in a tool's own group: the state map holds the Shuriken's
/// (a070_<id>); the tool plays a0<group>_<id> (HKS: offsetType 14 = the left-hand weapon's
/// wepmotionCategory). None when that tool has no such anim.
pub fn tool_anim(d: &CharData, state: &str, group: i64) -> Option<String> {
    let key = d.anim_key(state)?;
    let k = format!("a{group:03}{}", key.get(4..)?);
    d.anim(&k).is_some_and(|a| a.duration.is_some()).then_some(k)
}

/// A prosthetic state's anim for the equipped tool group: `tool_anim`, else the state's own anim
/// (the shared a070 clips, e.g. AddSubAttackFailed), else W_SubAttackFailed's clip: its CMSG is
/// animId 400900 offsetType 11 and only a070_400900 ships it.
pub fn sub_anim(d: &CharData, state: &str, group: i64) -> Option<String> {
    if state == "SubAttackFailed" {
        return d.anim("a070_400900").is_some_and(|a| a.duration.is_some()).then(|| "a070_400900".to_string());
    }
    // W_SubAttackFailedAir likewise ships only as a070_403900.
    if state == "SubAttackFailedAir" {
        return d.anim("a070_403900").is_some_and(|a| a.duration.is_some()).then(|| "a070_403900".to_string());
    }
    // The flame's hold loop shares its clip with HoldMove (CMSG animId 400300 for both).
    let state = if state == "GroundSubAttackHoldLoop" { "GroundSubAttackHoldMove" } else { state };
    // The whistle's walking W_GroundSubAttackCombo1Move / LockOnMove have no clip of their own (CMSG
    // animId 0): the standing *Moveable clip on the upper body over the walk (HKS 3731: StartTime_02).
    let moveable = format!("{state}able");
    let state = if state.ends_with("SubAttackCombo1Move") || state.ends_with("SubAttackLockOnMove") { moveable.as_str() } else { state };
    tool_anim(d, state, group).or_else(|| {
        let k = d.anim_key(state)?;
        (group == 70 && d.anim(k).is_some_and(|a| a.duration.is_some())).then(|| k.to_string())
    })
}

/// States that keep Wolf crouched: crouch idle / locomotion and its own starts, stops and turns.
pub(crate) fn crouch_ok(state: &str) -> bool {
    matches!(state, "StandIdle" | "Locomotion")
        || (state.starts_with("Crouch") && !state.starts_with("CrouchAttackToStand"))
        || state.starts_with("SprintToCrouch")
}

/// Standby states: any action may start (the run/walk stops carry no cancel flags but are
/// standby in the HKS, so input interrupts them).
pub(crate) fn is_free(state: &str) -> bool {
    // CrouchStart too: HKS g_paramHkbState [HKB_STATE_CROUCH_START] = STATE_TYPE_STANDBY, like
    // StandIdle (Wolf can move or act at once); CrouchEnd is STATE_TYPE_ACTION.
    matches!(state, "StandIdle" | "Locomotion" | "CrouchStart")
        || is_move_quick_turn(state)
        || state.starts_with("StandRunStop")
        || state.starts_with("StandWalkStop_")
        || state.starts_with("CrouchRunStop_")
        || state.starts_with("CrouchWalkStop_")
        || is_guard_idle(state)
}

/// Guarding with no action: idle or guard walk (DeflectGuardMoveF/B/L/R).
fn is_guard_idle(state: &str) -> bool {
    state == "DeflectGuardIdle" || state.starts_with("DeflectGuardMove")
}

/// WalkTwist convergence: SpinJointBehavior constant-rate mode, WalkTwist +0x70 = 0.1 rad per
/// 1/30 s frame (exe FUN_1407f65a0 / FUN_14072fdb0).
const WALK_TWIST_RATE: f32 = 0.1 * 30.0;

/// HKS STATE_TYPE_UPPER_ACTION / _ATK / _ITEM_USE states (c0000_cmsg g_paramHkbState: the drinks
/// and pills on the walk, the tools' *Moveable / *Move, the guard's run-out): drawn over the
/// walk/run by anim.rs (c0000.hkx StandMoveUpper_SM), moved by lower_body_move; the crouched ones
/// take the crouch walk (Actor::move_clip). Ours names the guard's run-out variants
/// DeflectGuardToStandMove<speed>.
pub(crate) fn is_upper_action(state: &str) -> bool {
    state.starts_with("DeflectGuardToStandMove") || state_type(state).contains("STATE_TYPE_UPPER_ACTION")
}

/// The move's stop by the move direction (BEH_A_GROUND_MOVE_STOP: W_StandRunStop /
/// W_StandWalkStop; crouched W_CrouchRunStop / W_CrouchWalkStop, CrouchRunStop_F 5600,
/// CrouchWalkStop_F 5300): the stop clips carry their own slide (root motion).
pub(crate) fn play_move_stop(d: &CharData, a: &mut Actor, p: &mut Player, run: bool) -> bool {
    let dir = ["F", "B", "L", "R"][a.move_dir.min(3) as usize];
    let (stop, fallback) = match (a.crouch, run) {
        (true, true) => (format!("CrouchRunStop_{dir}"), "CrouchRunStop_F"),
        (true, false) => (format!("CrouchWalkStop_{dir}"), "CrouchWalkStop_F"),
        (false, true) => (format!("StandRunStop{dir}"), "StandRunStopF"),
        (false, false) => (format!("StandWalkStop_{dir}"), "StandWalkStop_F"),
    };
    if a.play_state(d, &stop) || a.play_state(d, fallback) {
        a.move_vel = Vec3::ZERO;
        p.speed_level = 0.0;
        return true;
    }
    false
}

/// HKS env 1105: a STATE_TYPE_STANDBY* state (c0000 onUpdate calls act(9100) in each; the
/// sprint's walk-out StandMoveStartFromSprint / LoopFromSprint and SprintLoop are ones, so letting
/// go there stops: BEH_A_GROUND_MOVE_STOP W_SprintStopReady).
pub(crate) fn is_standby(state: &str) -> bool {
    is_free(state) || state_type(state).contains("STATE_TYPE_STANDBY")
}

/// A state's entry in the script's g_paramHkbState table (c0000_cmsg), by our state names
/// (HKB_STATE_ITEM_GOURD_DRINK_MOVE -> ItemGourdDrinkMove; SprintStartFromStep_B ->
/// SprintStartFromStep): its style and state types.
fn state_type(state: &str) -> &'static str {
    static MAP: std::sync::OnceLock<std::collections::HashMap<String, String>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let src = std::fs::read_to_string(crate::paths::extracted().join("hks_src/c0000_cmsg.lua")).unwrap_or_default();
        let Some(table) = src.lines().find(|l| l.starts_with("g_paramHkbState")) else {
            return Default::default();
        };
        table
            .split("[HKB_STATE_")
            .skip(1)
            .filter_map(|e| {
                let (name, rest) = e.split_once(']')?;
                let name = name.split('_').map(|w| w[..1].to_string() + &w[1..].to_lowercase()).collect();
                Some((name, rest.get(..rest.find('}')?)?.to_string()))
            })
            .collect()
    })
    .get(state)
    .or_else(|| MAP.get()?.get(state.rsplit_once('_')?.0))
    .map_or("", String::as_str)
}

/// A STYLE_TYPE_SPRINT state (SprintLoop, SprintStartFromStep_*, the sprint jump / wire / sub
/// attack): the sprint faces the stick, locked on or not.
pub(crate) fn is_sprint_style(state: &str) -> bool {
    state_type(state).contains("STYLE_TYPE_SPRINT")
}


/// A behaviorRefId on Wolf: his anim's SpEffects or a timed one (bullets, items).
/// SP_EF_REF_IN_NON_COMBAT_AREA (c0000_define 748; SpEffect 100298 / 100308 / 108900 carry it).
const SP_REF_IN_NON_COMBAT_AREA: i64 = 600;

/// The weapon state the area wants (HKS BEH_A_GROUND_JUMP 6148, BEH_A_WALL_JUMP 6023,
/// BEH_A_ITEM_USE 6240, BEH_A_ADD_SUB_WEAPON_CHANGE 6394): sheathed (env 207 ARM_STYLE_SAFE)
/// inside a non-combat area, drawn outside it. Otherwise these actions are refused.
/// gap: only the drawn sword inside a non-combat area refuses here. Sheathed outside one is the
/// X sheathe, which the game does not have (its BEH_R_NON_COMBAT_AREA_LEAVE draws at once), so
/// refusing there only swallowed jumps.
fn arm_style_matches(d: &CharData, a: &Actor, p: &Player) -> bool {
    a.sheathed || !wolf_ref(d, a, p, SP_REF_IN_NON_COMBAT_AREA)
}

pub(crate) fn wolf_ref(d: &CharData, a: &Actor, p: &Player, behavior_ref: i64) -> bool {
    sp_ref_active(d, a, behavior_ref)
        || p.timed.iter().any(|(id, _)| d.sp_effects.get(&id.to_string()).map_or_else(|| crate::data::sp_effect_behavior_ref(*id), |s| s.behavior_ref_id) == behavior_ref)
        // The lasting rows (a TAE row's replaceSpEffectId after its window: actor::run_sp_effect_chains).
        || a.sp_chained.iter().any(|(id, _)| crate::data::sp_effect_lingering_ref(*id) == behavior_ref)
}

/// SP_EF_REF_TAE_MOVING_RUN / DISABLE_PRESS_STEP (c0000_define.lua 604 / 726): BEH_A_SPRINT.
const SP_REF_TAE_MOVING_RUN: i64 = 91;
const SP_REF_TAE_DISABLE_PRESS_STEP: i64 = 427;
const SP_REF_KICK_ENEMY_JUMP: i64 = 204;
const SP_REF_SP_ATK_HIT_JUMP: i64 = 225;

/// env(3036, ref): is an SpEffect with this behaviorRefId applied by the TAE of the clip on screen?
fn sp_ref_active(d: &CharData, a: &Actor, behavior_ref: i64) -> bool {
    // The clip on screen's TAE: the anim's in this update (`Actor::step`: the event's window is
    // seen on the update its end falls in too, the recordings' ref 231 cut-off), the procedural
    // walk / run / idle clips' at their time and one TAE frame after (the walk's
    // SP_EF_REF_TAE_MOVING_WALK 90 is what the script's GetLocomotionType reads).
    // The additive layer's TAE as well (AddHardDeflectGuard's ref 411, HKS 6161).
    let has = |key: &str, at: crate::tae_window::At| d.sp_effects_at(key, at).iter().any(|(_, s)| s.behavior_ref_id == behavior_ref);
    // Seen one TAE frame after its window too (hks_wolf env 3036: the recordings' ref 231 cut-off,
    // DeflectGuardToStand* frames 0-9: Combo1Reverse up to 19 ticks after the release, Combo1
    // from 20; the step window alone cuts at 18). gap: where the exe keeps the row on is not traced.
    let lag = 1.0 / crate::data::TAE_FPS;
    let shown = if a.anim.is_empty() {
        let (key, t) = a.shown_clip(d);
        has(&key, t.into()) || (t >= lag && has(&key, (t - lag).into()))
    } else {
        has(&a.anim, a.step()) || (a.t >= lag && has(&a.anim, (a.t - lag).into()))
    };
    shown || (!a.add_anim.is_empty() && has(&a.add_anim, a.add_t.into())) || legs_clip(d, a).is_some_and(|(k, lt)| has(&k, lt.into()))
}

/// Under an upper action with the legs moving, the legs' locomotion clip (c0000.hkx
/// StandMoveOverwrite) and its time. Its TAE counts for env 3036 too: the run's
/// SP_EF_REF_TAE_MOVING_RUN 91 is what GetLocomotionType reads, so the script's move start
/// (BEH_A_GROUND_MOVE_START: W_StandMoveLowerStart) fires once, not every frame.
/// IsMoveAcceptable (env 2000, exe FUN_140b36410 0x7d0): flag 11 set by this frame's TAE
/// (ChrCtrl+0x88 bit 2) and latched from the last frame (+0x9c bit 4, FUN_1407d8ce0 before the
/// graph step), so it opens a frame after the window does (recordings: the step's frame-18
/// window, tick 36, W_StandMoveStart at tick 37).
pub fn accept_move(d: &CharData, a: &Actor) -> bool {
    let ((now, t), (last, lt)) = (a.shown_clip(d), a.shown_clip_at(d, a.prev_t));
    d.flag(&now, t, FLAG_ACCEPT_MOVE) && d.flag(&last, lt, FLAG_ACCEPT_MOVE)
}

pub(crate) fn legs_clip(d: &CharData, a: &Actor) -> Option<(String, f32)> {
    if !is_upper_action(&a.state) || Vec2::new(a.move_vel.x, a.move_vel.z).length() <= 0.05 {
        return None;
    }
    let (legs, t, _) = a.locomotion_clip_at(a.t);
    let len = d.length(&legs);
    Some((legs, if len > 0.0 { t.rem_euclid(len) } else { t }))
}

/// Plunge deathblow start (ThrowParam 崩し落下0, suffix 150: Dist 20 m, defender within upperYRange
/// 3 m above / lowerYRange 15 m below, normalFallOrbitCheck (throw_fall_orbit_ok): the fall top
/// over heightLimit 3 m above the defender, the predicted orbit within range 1.5 m of it inside
/// timeLimit 2000 ms).
/// Plays the throw's attacker anim (a20x_511400 = 510300's clip) and the defender's ThrowDef13400.
/// Out of a head-kick jump (AirKickEnemyJumpStart*) it is 蹴り崩し0 (suffix 160: a20x_511500,
/// ThrowDef13500; heightLimit -10000 = no height check), landing on 蹴り崩し1 (161). Live
/// (rec_c1020_c_20261009): the kick broke the General, an air attack 0.33 s into 213115 started
/// a201_511500 (0.72 s) -> 511510 (1.68 s) -> 511511 with ThrowDef13500 -> 13510 -> 13511.
/// The Grappling Hook Attack's air throw on the hooked enemy (ThrowParam <chr>0170): within the
/// row's Dist and y range, and the fall orbit when its kind checks one (throw_fall_orbit_ok).
fn wire_throw(
    combat: &Combat,
    a: &Actor,
    pos: Vec3,
    target: Entity,
    enemies: &Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
) -> Option<crate::data::Throw> {
    let (_, ea, _, etf) = enemies.get(target).ok()?;
    let row_id = combat.foe_of(ea).throw_row(170);
    let th = combat.throw(row_id)?;
    combat.player.anim(&th.atk_anim)?;
    let row = combat.param("ThrowParam", row_id);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let dy = etf.translation.y - pos.y;
    if etf.translation.distance(pos) >= th.dist || dy >= f("upperYRange") || dy <= -f("lowerYRange") {
        return None;
    }
    // The row's fall orbit only for kinds with doesCheckNormalFallOrbit (the wire's 170000 has
    // none: the exe takes it at any height in the window).
    throw_fall_orbit_ok(combat, row_id, pos, pos.y, a.air_base.with_y(a.vel_y), etf.translation).then_some(th)
}

/// The plunge `ea` (at `epos`) would take from Wolf falling at `pos` now: its throw, landing
/// row suffix and whether it is the stealth one. Shared by `start_plunge` and the HUD's
/// deathblow mark (the user, 2026-10-18: "no indicator, sometimes works sometimes doesn't").
pub(crate) fn plunge_on(combat: &Combat, a: &Actor, pos: Vec3, ea: &Actor, enemy: &crate::enemy::Enemy, epos: Vec3) -> Option<(crate::data::Throw, i64, bool)> {
    plunge_from(combat, a, pos, pos.y, ea, enemy, epos)
}

/// `plunge_on` with the fall top the orbit test reads (physics +0x854, `Player::air_top`; the
/// HUD's mark goes by Wolf's height now).
pub(crate) fn plunge_from(combat: &Combat, a: &Actor, pos: Vec3, fall_top: f32, ea: &Actor, enemy: &crate::enemy::Enemy, epos: Vec3) -> Option<(crate::data::Throw, i64, bool)> {
    let kick = a.state.starts_with("AirKickEnemyJumpStart");
    if a.vel_y >= 0.0 && !kick {
        return None;
    }
    let stealth = !enemy.is_broken() && enemy.is_unaware();
    let (start, land) = if kick {
        (160, 161)
    } else if stealth {
        (30, 31)
    } else {
        (150, 151)
    };
    if !(enemy.is_broken() || stealth) || enemy.is_dead() {
        return None;
    }
    // Each enemy kind's own ThrowParam rows.
    let row_id = combat.foe_of(ea).throw_row(start);
    let row = combat.param("ThrowParam", row_id);
    let th = combat.throw(row_id)?;
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let (upper, lower) = (f("upperYRange"), f("lowerYRange"));
    let dy = epos.y - pos.y;
    if epos.distance(pos) >= th.dist || dy >= upper || dy <= -lower {
        return None;
    }
    // The exe's orbit test (FUN_140be3ba0, throw_fall_orbit_ok): the fall top above the enemy
    // over heightLimit, the predicted orbit's nearest point within range in time. Wolf's
    // velocity: his air base and fall speed. gap: the exe's is the proxy's (FUN_140bbf7d0).
    if !throw_fall_orbit_ok(combat, row_id, pos, fall_top.max(pos.y), a.air_base.with_y(a.vel_y), epos) || combat.player.anim(&th.atk_anim).is_none() {
        return None;
    }
    Some((th, land, stealth))
}

fn start_plunge(
    p: &mut Player,
    a: &mut Actor,
    combat: &Combat,
    pos: Vec3,
    enemies: &mut Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    log: &mut CombatLog,
) -> bool {
    // An enemy that has not noticed Wolf takes the stealth plunge 落下0 / 落下1 (0030 / 0031:
    // a20x_510300 -> 510310, ThrowDef12300 -> 12310): live (General) 0.54 s + 1.69 s + 1.83 s.
    // The row is chosen per target (2026-10-12 bug hunt round 16: one broken enemy anywhere
    // used to turn off the stealth plunge on every unaware one).
    for (e, mut ea, mut enemy, etf) in enemies.iter_mut() {
        let Some((th, land, stealth)) = plunge_from(combat, a, pos, p.air_top, &ea, &enemy, etf.translation) else { continue };
        a.play("PlungeDeathblow", &th.atk_anim);
        p.plunge = Some((e, land));
        if enemy.is_broken() {
            // A broken enemy takes no reaction (react returns for Mode::Broken): it plays its
            // ThrowDef pose (13400: 3 s, over the fall's 2 s timeLimit) and is held broken while
            // it lasts, as the kick jump's 13900 (2026-10-12 bug hunt round 15: it stayed in its
            // collapse and its Broken window could run out mid-fall).
            let ed = combat.data_of(&ea);
            if ea.play_state(ed, &format!("ThrowDef{}", th.def_anim)) {
                enemy.on_posture_break(&mut ea, combat, false);
            }
        } else {
            enemy.react(&mut ea, combat, &format!("ThrowDef{}", th.def_anim));
            if stealth {
                // Held in its ThrowDef reaction (no AI) until the landing throw.
                enemy.on_posture_break(&mut ea, combat, false);
            }
        }
        log.push("plunge", Color::srgb(1.0, 0.5, 0.3));
        return true;
    }
    false
}

/// HKS _UpdateAutoAim: the homing target is refreshed for this long after an attack starts.
const AUTO_AIM_TIME: f32 = 1.0 / 6.0;

/// HKS _StartAutoAim on an attack transition: pick the homing target now (so the swing's
/// opening facing uses it) and keep refreshing it for AUTO_AIM_TIME.
fn start_auto_aim(
    p: &mut Player,
    a: &mut Actor,
    combat: &Combat,
    pos: Vec3,
    stick: Vec3,
    enemies: &Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
) {
    p.auto_aim = true;
    p.auto_aim_fresh = true;
    let dir = if stick != Vec3::ZERO { stick } else { a.forward() };
    a.homing = auto_homing_target(combat, pos, dir, enemies.iter().map(|(e, ea, en, t)| (e, t.translation, ea.hp > 0.0 && !en.is_dead())));
}

/// FUN_140b30fa0 / FUN_1409c58e0 / FUN_140b2ddc0: from the eye point (feet + 1.5 m), the
/// nearest live enemy with 3D distance <= closeMaxRadius, height within -closeMinHeight ..
/// +closeMaxHeight, and within closeAngRange of the stick (or facing). LockCamParam row 0.
/// gap: the darkness variants (_forD / _forPD) and the line-of-sight ray are not modelled.
fn auto_homing_target(combat: &Combat, pos: Vec3, dir: Vec3, enemies: impl Iterator<Item = (Entity, Vec3, bool)>) -> Option<Entity> {
    let row = combat.param("LockCamParam", 0);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let (max_r, min_h, max_h, ang) = (f("closeMaxRadius"), f("closeMinHeight"), f("closeMaxHeight"), f("closeAngRange"));
    let eye = pos - Vec3::Y * CAPSULE_HALF_HEIGHT + Vec3::Y * 1.5;
    let dir = dir.with_y(0.0).normalize_or_zero();
    enemies
        .filter(|&(_, _, alive)| alive)
        .filter_map(|(e, t, _)| {
            // Candidate point: the enemy's lock point at the same 1.5 m height.
            let c = t - Vec3::Y * CAPSULE_HALF_HEIGHT + Vec3::Y * 1.5;
            let v = c - eye;
            let d = v.length();
            let flat = v.with_y(0.0).normalize_or_zero();
            let ok = d <= max_r && v.y >= -min_h && v.y <= max_h && flat.angle_between(dir).to_degrees() <= ang;
            ok.then_some((e, d))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(e, _)| e)
}

/// Remembers the state decide left the player in (see the ActionRequest buffer).
fn record_state(mut player: Single<(&mut Player, &mut Actor)>) {
    let (p, a) = &mut *player;
    a.attack_hit = false;
    if p.last_state != a.state {
        if !keeps_buffer(&a.state) {
            p.requests.clear();
        }
        p.last_state = a.state.clone();
        // The attack that started auto-homing keeps it; leaving it ends it (FUN_140b2d9d0 clears +0x38).
        if p.auto_aim_fresh {
            p.auto_aim_fresh = false;
        } else {
            p.auto_aim = false;
            a.homing = None;
        }
    }
}


/// HKS FireEvent(state) runs ResetRequest -> act(9101) -> FUN_140b2afd0: every pending
/// request is dropped on a state change. FireEventNoReset (locomotion, quick turns, falls
/// and landings) and graph-driven returns to idle keep them.
fn keeps_buffer(state: &str) -> bool {
    // The c0000_transition.lua FireEventNoReset targets Wolf can reach here.
    // Round 62: the art releases too (HKS 3467-3475, 3493).
    const NO_RESET: [&str; 17] = [
        "GroundAttackCombo1Release",
        "GroundAttackCombo1ReverseRelease",
        "GroundAttackCombo2Release",
        "GroundAttackCombo2ReverseRelease",
        "DeflectGuardAttackRelease",
        "GroundSpecialAttackCombo1Release",
        "StandDeflectEasyMinimum",
        "StandDeflectHardMinimum",
        "AirDeflectGuardEnd",
        "EasyDeflectedL",
        "EasyDeflectedR",
        "ThrowDefLargeBlowStart",
        "GroundSpecialAttackCombo2Release",
        "GroundSpecialAttackCombo1ReleaseNoResource",
        "SprintSpecialAttackRelease",
        "SprintSpecialAttackReleaseNoResource",
        "GroundSpacialAttackHoldEnd",
    ];
    is_free(state)
        || state.contains("Fall")
        || state.contains("Land")
        || state.starts_with("StandMove")
        || NO_RESET.contains(&state)
        || state.starts_with("HardDeflectAtkRelease")
        || state.starts_with("EasyDeflectedAtkRelease")
        || state.starts_with("DeflectGuardToStand")
        || state.starts_with("SprintQuickTurn") && state != "SprintQuickTurnReady"
        || state.starts_with("SprintStopQuickTurn")
        || state.starts_with("SprintStartFromStep")
}

/// ChrActionFlags that open an action's input buffer (exe: the TAE ChrActionFlag handler
/// FUN_140b51b70 sets accept bits +0xd0 via FUN_140b2be00). 87 AttackAction_Complex opens
/// every action; 1 attack/sub-attack (bits 0,1,25,26), 9 and 150 guard (2), 25 SpMove/
/// Backstep/Rolling (5,13,14), 151 jump (4), 30 item (7), 136 shinobi tool (18).
const FLAG_BUFFER_ALL: i64 = 87;

fn buffer_flags(action: Action) -> &'static [i64] {
    match action {
        Action::Attack | Action::CombatArt => &[1],
        Action::Guard => &[9, 150],
        Action::Step => &[25],
        Action::Jump => &[151],
        Action::UseItem => &[30],
        Action::Prosthetic => &[136],
        // gap: the ChrActionFlags behind ACTION_ARM_CROUCH (bit 15) are not traced; the crouch
        // toggle is only taken from standby (decide).
        Action::Crouch | Action::SwitchTool | Action::NextItem => &[],
        // gap: the accept / execute bits of ACTION_ARM_WIRE_SHOOT are not traced; taken as the
        // jump's (151 / 119): the wire moves open their cancels together with the jump's.
        Action::Grapple => &[151],
        // Read raw by decide's docking (docking.rs), not buffered.
        Action::Interact => &[],
    }
}

/// Is a press latched right now? (pending |= pressed & acceptMask in FUN_140b2c190.) A
/// press outside every accept window is dropped, not queued.
pub(crate) fn buffers(d: &CharData, a: &Actor, action: Action) -> bool {
    // A break pose reads attack / jump for its throw directly (decide, "ThrowBreak").
    if a.state == "ThrowBreak" && matches!(action, Action::Attack | Action::Jump) {
        return true;
    }
    // The walk-up to a kneeling boss reads attack for the Todome (todome_input): its clip has no
    // accept flag, so the press was dropped (the user, 2026-10-16: Isshin's close-up, "i can't
    // press the attack button"). Kept from its start (TAE 931 unk0 9, 0-5 s) until the 930 window
    // takes it (rec_20261016_080607: the Todome 0.77 s into a000_710206).
    if action == Action::Attack && a.anim.starts_with("a000_7102") && d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 931 && e.dispatched(a.step()))) {
        return true;
    }
    // The aged standby states (g_paramHkbState STATE_TYPE_STANDBY: AgingIdle, the walk, the
    // sprint start / loop) take every input as StandIdle does.
    if a.airborne || is_free(&a.state) || is_aged_standby(&a.state) || a.anim.is_empty() || sprint_window(d, a) {
        return true;
    }
    d.flag(&a.anim, a.step(), FLAG_BUFFER_ALL) || buffer_flags(action).iter().any(|&f| d.flag(&a.anim, a.step(), f))
}

/// A jump press now would be a wall jump (BEH_A_WALL_JUMP, before the kick): in the air with a
/// wall at the chest, once per flight; chained (the parkour map's lifted limit), each jump needs
/// another wall, more than 90 deg from the last one's direction (spamming jump climbed one flat
/// wall).
pub(crate) fn wall_jump_open(d: &CharData, a: &Actor, p: &Player, feet: Vec3) -> bool {
    use crate::docking as dk;
    let air_ok = a.state == "FreeFall" || a.state.contains("GroundJump") || a.state.starts_with("SprintJump") || a.state.starts_with("WallJumpStart");
    if !(air_ok && a.airborne && p.dock.is_none() && p.wire.is_none() && p.plunge.is_none() && p.wall_jumps < dk::wall_jump_limit() && arm_style_matches(d, a, p)) {
        return false;
    }
    let chest = feet + Vec3::Y * 1.2;
    let new_wall = match (p.last_wall, dk::wall_contact_dir(chest, a.yaw)) {
        (Some(last), Some(now)) => last.angle_between(now) > std::f32::consts::FRAC_PI_2,
        _ => true,
    };
    new_wall && dk::wall_contact(chest, a.yaw).is_some()
}

/// 崩し蹴りジャンプ (ThrowParam 0200): a broken enemy facing Wolf within the row's Dist ->
/// (enemy, its position, Wolf's start throw anim).
fn jump_over<'a>(combat: &Combat, wolf: Vec3, enemies: impl Iterator<Item = (Entity, &'a Actor, &'a Enemy, &'a Transform)>) -> Option<(Entity, Vec3, String)> {
    let d = &combat.player;
    enemies.into_iter().find_map(|(ee, ea, en, etf)| {
        // Each enemy kind's own 0200 row.
        let st = combat.throw(combat.foe_of(ea).throw_row(200)).filter(|st| d.anim(&st.atk_anim).is_some())?;
        let to_wolf = (wolf - etf.translation).with_y(0.0).normalize_or_zero();
        let facing = ea.forward().angle_between(to_wolf).to_degrees() < 90.0;
        (en.is_broken() && facing && etf.translation.distance(wolf) <= st.dist).then(|| (ee, etf.translation, st.atk_anim.clone()))
    })
}

/// The exe's throw check on the attack button (ThrowParam) goes before the script: while a
/// deathblow is in reach, hks_wolf leaves the attack press to decide's deathblow (the jump's
/// to the jump-over throw).
pub(crate) fn throw_check(
    combat: Res<Combat>,
    mut wolf: Single<(&Actor, &mut Player, &Transform)>,
    enemies: Query<(Entity, &Actor, &Enemy, &Transform), Without<Player>>,
    enemy_dummies: Query<&crate::model::Dummies, With<Enemy>>,
    globals: Query<&GlobalTransform>,
) {
    let (a, p, tf) = &mut *wolf;
    let dmy_of = |ee: Entity| move |id: i16| enemy_dummies.get(ee).ok().and_then(|dm| dm.0.get(&id)).and_then(|m| globals.get(*m).ok()).map(|g| g.translation());
    p.deathblow_ready = enemies.iter().any(|(ee, ea, en, etf)| {
        etf.translation.distance(tf.translation) < 25.0 && deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), ea, en, etf.translation, &dmy_of(ee)).is_some_and(|db| db.in_reach)
    });
    p.jump_throw_ready = jump_over(&combat, tf.translation, enemies.iter()).is_some();
    p.plunge_ready = a.airborne && enemies.iter().any(|(_, ea, en, etf)| plunge_from(&combat, a, tf.translation, p.air_top, ea, en, etf.translation).is_some());
}

/// The art's states a press starts (hks_wolf::art_press_event's).
fn is_art_press(state: &str) -> bool {
    ["GroundSpecialAttack", "GroundSpacialAttack", "SprintSpecialAttack"].iter().any(|k| state.starts_with(k))
        && !["Release", "HoldEnd", "HoldLoop", "HoldAction", "HoldMove", "HoldQuickTurn", "HitJump"].iter().any(|k| state.contains(k))
}

/// The ground jump's first states (hks_wolf's jump events): the ready (or the fast jump's start)
/// of the vertical, forward, lock-on, sprint, storm and swamp jumps.
fn is_ground_jump_press(state: &str) -> bool {
    !state.starts_with("Aging") && !state.contains("SpecialAttack") && !state.contains("SubAttack")
        && (state.contains("GroundJumpReady") || state.contains("GroundJumpStart") || state.starts_with("SprintJump") || state.starts_with("GroundStormJump") || state.starts_with("SwampJump"))
        && !state.contains("Back")
}

/// The prosthetic press's states (hks_wolf::sub_press_event's).
fn is_sub_press(state: &str) -> bool {
    (state.contains("SubAttack") && !["Add", "Land", "Hang", "Cover"].iter().any(|k| state.starts_with(k))
        && !["Release", "DeriveAttack", "DeriveDirectivity", "Move", "Loop", "End", "Deflect", "Action"].iter().any(|k| state.contains(k)))
        || matches!(state, "AirSubAttackMoveReady" | "AirSubAttackMoveAtemiReady")
}

/// An art's opening state (BEH_A_GROUND_SP_ATTACK), not a combo.
fn is_art_opener(state: &str) -> bool {
    is_art_press(state) && (state.contains("Combo1") || state.contains("Step") || state.contains("JumpReady") || state.contains("HoldStart") || state.starts_with("SprintSpecialAttack"))
}

/// The attack button's states (BEH_A_GROUND_ATTACK), not their releases.
fn is_ground_attack(state: &str) -> bool {
    !state.contains("Release")
        && ["GroundAttackCombo", "GroundStepAttack_", "SprintAttack_", "CrouchAttackTo", "HardDeflectAtk", "EasyDeflectedAtk", "DeflectGuardAttack"].iter().any(|k| state.starts_with(k))
}

/// Turning rate [deg/s] where the turn controller clamps (locked on, actions): the anim's TAE
/// SetTurnSpeed (e.g. SprintLoop 360) when it has one, else the engine default, config
/// movement.turn_speed = 720 (exe: the turn controller's +0x28 = 720.0 in FUN_1407da540;
/// FUN_1407daa20 uses the TAE value when >= 0). Live, locked on in the run a000_000500 (no TAE
/// 224) the facing turns 12 deg a frame (rec_20261016_094717 ticks 357-363). Free turning from
/// the stick has no default rate: `free_turn`.
fn turn_rate(d: &CharData, a: &Actor, config: &GameConfig, locked: bool) -> f32 {
    if a.anim.is_empty() { None } else { d.turn_speed(&a.anim, a.step(), locked) }.unwrap_or(config.movement.turn_speed)
}

fn is_guard_start(state: &str) -> bool {
    state.starts_with("StandToDeflectGuard") || state == "SprintToDeflectGuard"
}

/// Execute flags: a pending request fires while one is up (requested |= pending & the
/// execute mask +0xe0, set by FUN_140b2bdc0): 115 attack (bits 0,25), 117 guard (2),
/// 26 step (5,13,14), 119 jump (4), 31 item (7), 137 shinobi tool (18).
pub(crate) fn accept_flag(action: Action) -> &'static [i64] {
    match action {
        Action::Attack => &[FLAG_ACCEPT_ATTACK],
        Action::Guard => &[FLAG_ACCEPT_GUARD],
        Action::Step => &[FLAG_ACCEPT_STEP, FLAG_ACCEPT_STEP_ALT],
        Action::Jump => &[FLAG_ACCEPT_JUMP],
        Action::UseItem => &[FLAG_ACCEPT_ITEM],
        Action::CombatArt => &[FLAG_ACCEPT_ATTACK],
        Action::Prosthetic => &[FLAG_ACCEPT_PROSTHETIC],
        Action::Grapple => &[FLAG_ACCEPT_JUMP],
        Action::Crouch | Action::SwitchTool | Action::Interact | Action::NextItem => &[],
    }
}

/// The engine's env(1106, button): is this input accepted right now?
/// The STYLE_TYPE_GROUND_GUARD states (c0000_cmsg g_paramHkbState): the guard idle / move /
/// turns, the guard starts, the easy deflects and their recoils.
/// A buffered guard press still counts: the button held, or pressed this frame (it is still
/// down when it executes).
pub(crate) fn guard_press_live(p: &Player, held: bool, dt: f32) -> bool {
    held || p.requests.get(&Action::Guard).is_some_and(|&age| age <= dt * 1.5)
}

pub(crate) fn is_guard_style(s: &str) -> bool {
    s.starts_with("DeflectGuard")
        || s.contains("ToDeflectGuard")
        || s.starts_with("StandDeflectEasy")
        || s.starts_with("LandAirDeflectEasy")
        || matches!(s, "StandDeflectGuardFromDamage" | "EasyDeflectDmg" | "LandAirDeflectGuard")
}

pub(crate) fn accepts(d: &CharData, a: &Actor, action: Action) -> bool {
    if a.airborne {
        return false;
    }
    // g_behaviorTable (c0000_transition 6450): BEH_A_ITEM_USE and BEH_A_CROUCH_START are off in
    // the guard style (3) - with the guard held Wolf neither drinks nor crouches (round 54).
    if matches!(action, Action::UseItem | Action::Crouch) && is_guard_style(&a.state) {
        return false;
    }
    if is_free(&a.state) || is_aged_standby(&a.state) || sprint_window(d, a) {
        return true;
    }
    // The HKS sees the flags the previous frame's TAE update set (live: buffered inputs fire one
    // game frame later than a fresh press, docs/kb/input-buffer.md), so the execute window is
    // read at the previous step's time.
    !a.anim.is_empty() && accept_flag(action).iter().any(|&f| d.flag(&a.anim, a.prev_t, f))
}

/// Wolf's state for an EzState request (W_Event<id>, c0000.hks 264): Event<id>, or for the walk-ups
/// 710200 / 710201 / 710203 / 710204, which have no state of that name in c0000.hkx, the state
/// playing that clip (statemap_c0000: Event390010 / 390011 / 390013 / 390014 -> a000_710200 / 201 /
/// 203 / 204). The recordings show those clips on the request: the True Monk a000_710200, the Owl
/// Father 710201, the Ape 710203, Genichiro (castle) 710204 (rec_2026101x_*_boss_*).
pub fn event_state(d: &CharData, id: i64) -> String {
    let own = format!("Event{id}");
    if d.anim_key(&own).is_some() || !(710_200..=710_204).contains(&id) {
        return own;
    }
    format!("Event{}", id - 710_200 + 390_010)
}

/// Wolf's walk-up to a kneeling boss (a000_710200-710207, the map script's request with the
/// boss's 20200) takes the attack for the Todome while its TAE 930 (unk0 1, 1.0-5.0 s on
/// a000_710200-710207) is on: the boss's own 20200 window (932 / 105) is the same 1.0-5.0 s, and the
/// recordings start the Todome 0.8-1.8 s into the walk-up (data.rs finisher_window). gap: what
/// 930 is in the exe is not traced.
fn todome_input(d: &CharData, a: &Actor) -> bool {
    a.anim.starts_with("a000_7102") && d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 930 && e.arg_i64("unk0") == Some(1) && e.dispatched(a.step())))
}

/// Ref 1 SP_EF_REF_ENABLE_SPRINT_ACTION (SprintStartFromStep_F, SprintLoop, SprintToDeflectGuard
/// 6-15): sprinting is a free state for actions - SprintLoop has no ChrActionFlags at all, and
/// the HKS picks SprintAttack / SprintToDeflectGuard from this ref. The sprint's 100304 ends
/// into 100303, ref 1 for 0.2 s more (`sp_chained`): the recordings' SprintAttack out of the
/// first 12-15 ticks of StandMoveStartFromSprint, whose own TAE has no ref 1.
fn sprint_window(d: &CharData, a: &Actor) -> bool {
    (!a.anim.is_empty() && d.has_ref(&a.anim, a.step(), REF_ENABLE_SPRINT_ACTION))
        || a.sp_chained.iter().any(|(id, _)| crate::data::sp_effect_lingering_ref(*id) == REF_ENABLE_SPRINT_ACTION)
}

pub(crate) fn stick_world(stick: Vec2, cam_yaw: f32) -> Vec3 {
    let rot = Quat::from_rotation_y(cam_yaw);
    (rot * Vec3::NEG_Z * stick.y + rot * Vec3::X * stick.x).normalize_or_zero()
}

/// TAE 922 ChrPhysicsVelosityScale, exe FUN_140b537a0 -> FUN_140bb1980 (args: s32 row, then u8
/// horizontal curve, u8 its exponent, u8 vertical curve, u8 its exponent; the template's
/// unk1..unk4, a missing one read as 0): two CSEasingValue interpolators 0 -> 1 over the event
/// (end - start), started at the time since its start; each tick (FUN_140ba.. velocity pass,
/// "+0xd9 active") velocity = start + (target - start) * ease, horizontal on the first curve, vertical
/// on the second, until both reach 1. Target: the tick's own velocity times the
/// ChrPhysicsVelocityChangeParam row's horizontal / vertical scale. gap: where the exe applies
/// the row's scales to the target is not traced (the target read is the tick's velocity,
/// +0x140); scaling it is the reading that uses the row.
struct VelScale {
    /// The anim whose event this is: the scaling ends with it.
    anim: String,
    /// The row's change (horizontalVelocityChange along horizontalVelocityAngle,
    /// verticalVelocityChange), added to the scaled target.
    add: (Vec3, f32),
    v0: Vec3,
    vy0: f32,
    /// What last tick's blend took off the free velocity (horizontal, vertical).
    delta: (Vec3, f32),
    scale: (f32, f32),
    curves: [(u8, u8); 2],
    elapsed: f32,
    duration: f32,
}

/// CSEasingValue curves (exe table 0x142bccfa0, three functions per type, names at 0x143b1d4b0):
/// 0 Linear (FUN_1411a0b90: x), 1 EaseIn (0bd0: x^n), 2 EaseOut (0d70: 1 - (1 - x)^n), 3 EaseInOut
/// (0fb0: EaseIn on the first half, EaseOut on the second, each squeezed to half), n = the exponent.
fn ease((kind, n): (u8, u8), x: f32) -> f32 {
    let n = n as f32;
    let ease_in = |x: f32| x.powf(n);
    let ease_out = |x: f32| 1.0 - (1.0 - x).powf(n);
    match kind {
        1 => ease_in(x),
        2 => ease_out(x),
        3 if x < 0.5 => ease_in(2.0 * x) * 0.5,
        3 => ease_out(2.0 * x - 1.0) * 0.5 + 0.5,
        _ => x,
    }
}

pub(crate) fn yaw_of(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

/// Wolf's turn from the move stick as the exe runs it each frame (PlayerIns update FUN_140a37560,
/// emulated on a snapshot of the game: tools/emu-cases `turn`, src/sim_tests/emu_turn.rs):
/// - the pad manipulator (PlayerCtrl's FUN_1407f0a70 -> FUN_1407f0f50) takes the stick only when
///   last frame's raw stick (+0x340) was off centre too and within 45 deg of it (DAT_143289178,
///   the cosine against the normalised dot product); otherwise the move vector (+0x10, +0xc0) is 0
///   this frame: a press from rest counts one frame late, and a flick of more than 45 deg in one
///   frame reads as a one-frame let-go (MoveSpeedLevel 0). No dead zone: 0.05 is taken as is;
/// - MoveSpeedLevel (behaviour variable 2, FUN_140b4afd0 -> +0xbac) is the taken stick's length,
///   not rescaled (1.2 reads 1.2; sprinting 2.0);
/// - its yaw request (+0x24) is the stick's yaw (the camera yaw +0x94 plus the stick's angle)
///   minus the facing, wrapped to [-pi, pi);
/// - the next frame the turn controller (PlayerCtrl +0x250 BehaviorTurn, FUN_1407da6d0 ->
///   FUN_1407dac80) adds that request to the physics yaw: whole when no rate is set (the player's
///   BehaviorTurn +0x70 bit 0, FUN_1407da5f0, with no TAE 224 SetTurnSpeed and the module +0x16c
///   rate < 0), else clamped to rate * dt (FUN_1407dab40). So free in the run loop the facing
///   snaps to the stick a frame after it moves (live: rec_20261015_191200 tick 687 request 0.855,
///   tick 688 the yaw +49 deg) and the drawn facing (`ShownYaw`) turns after it at 792 deg/s.
/// gap: the module +0x16c rate and the PlayerCtrl +0x259 gate are not ported; the camera-yaw hold
/// while the stick is held (+0x424 / +0x428, option DAT_143d5b49c) is not ported.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Turn {
    /// PadManipulator +0x340: last frame's raw stick.
    pub last_stick: Vec2,
    /// PadManipulator +0x24: the request made this frame, applied the next.
    pub request: Option<f32>,
    /// Last frame's request, for this frame's turn.
    pub pending: Option<f32>,
    /// The stick the manipulator took this frame (`pad_manipulator`): the move vector `decide`
    /// turns and moves by.
    pub taken: Vec2,
    /// The stick the script (hks_wolf) reads: the one taken the frame before, as the script
    /// stands for the exe's HKS of the frame before and that HKS reads its own frame's stick
    /// (behaviour +0xbac / +0xbb0 and the angle / level pairs from +0xb30, written by
    /// FUN_140b4afd0 in vf22 before the Havok step; emu `hksstick`, kb/frame-coverage.md "The
    /// HKS's stick").
    pub hks: Vec2,
}

impl Turn {
    /// A new frame: last frame's request is the one to apply.
    pub fn new_tick(&mut self) {
        self.pending = self.request.take();
    }

    /// The stick the manipulator takes this frame: free (FUN_1407f0f50) through the 45 deg
    /// gate; locked on (`locked`: the lock module hands a lock point, FUN_140b2dab0, and
    /// FUN_1407f0a70 runs the lock-on manipulator FUN_1407f1710) as it is, the same frame: that
    /// path stores +0x340 and turns the raw stick by the camera yaw with no gate (emulated with a
    /// lock point, sim test `lock_manipulator_matches_the_exe`: a press from rest moves that
    /// frame, a 90 deg flick is no let-go).
    pub fn gate(&mut self, raw: Vec2, locked: bool) -> Vec2 {
        let last = std::mem::replace(&mut self.last_stick, raw);
        if locked {
            return raw;
        }
        let near = last.length() > 0.0 && raw.length() > 0.0 && last.normalize().dot(raw.normalize()) >= std::f32::consts::FRAC_PI_4.cos();
        if near { raw } else { Vec2::ZERO }
    }

    /// The turn controller's step: last frame's request onto `yaw` (whole, or clamped to `rate`
    /// deg/s), then this frame's request toward `target` (None: the stick is centred, no request).
    pub fn step(&mut self, yaw: f32, target: Option<f32>, rate: Option<f32>, dt: f32) -> f32 {
        let want = self.pending.take().unwrap_or(0.0);
        let yaw = yaw + rate.map_or(want, |r| want.clamp(-r.to_radians() * dt, r.to_radians() * dt));
        self.request = target.map(|t| wrap_angle(t - yaw));
        yaw
    }

    /// `step` on `yaw`, a turn engaging the drawn facing (`ShownYaw`) at 1.1x the rate in force
    /// (`rate`, else `default_rate`).
    pub fn turn(&mut self, yaw: &mut f32, shown: &mut Option<ShownYaw>, target: Option<f32>, rate: Option<f32>, default_rate: f32, dt: f32) {
        let before = *yaw;
        *yaw = self.step(before, target, rate, dt);
        if *yaw != before && shown.is_none() {
            *shown = Some(ShownYaw { yaw: before, ..default() });
        }
        if let Some(s) = shown.as_mut() {
            s.rate = rate.unwrap_or(default_rate);
        }
    }
}

/// FUN_1407daa20's pick for a free turn: the shown clip's TAE 224 SetTurnSpeed (the move starts
/// a000_000160-163 / 460-463 360 deg/s over 0.2 s, the sprint 180 / 360), None when it has none.
fn free_turn_rate(d: &CharData, a: &Actor) -> Option<f32> {
    let (key, t) = a.shown_clip(d);
    d.turn_speed(&key, t, false)
}

/// Turns Wolf toward the (world) stick through the turn controller (`Turn`), drawing the turn
/// through `ShownYaw` at 1.1x the rate in force (the TAE's, else movement.turn_speed 720).
fn free_turn(p: &mut Player, a: &mut Actor, d: &CharData, config: &GameConfig, stick: Vec3, dt: f32) {
    let rate = free_turn_rate(d, a);
    p.turn.turn(&mut a.yaw, &mut a.shown, (stick != Vec3::ZERO).then(|| yaw_of(stick)), rate, config.movement.turn_speed, dt);
}

/// Locked on and standing (`decide`): the turn to the target, at the clip's TAE 224 rate when
/// it has one, else whole the next frame.
fn locked_turn(p: &mut Player, a: &mut Actor, d: &CharData, config: &GameConfig, target_dir: Vec3, dt: f32) {
    let rate = if a.anim.is_empty() { None } else { d.turn_speed(&a.anim, a.t, true) };
    p.turn.turn(&mut a.yaw, &mut a.shown, (target_dir != Vec3::ZERO).then(|| yaw_of(target_dir)), rate, config.movement.turn_speed, dt);
}

fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    yaw + diff.clamp(-max_step, max_step)
}

/// A one-shot TAE event (a motionInterval 999 recovery) entering in this tick: the exe's window
/// (prev_t + 5e-7, t + 5e-7], every start up to t on the first step (`tae_window::starts`).
/// Windows touch end to end, so a start on a tick boundary is in exactly one. The old
/// `prev_t - 1e-4` slack overlapped the previous window and gave back the deathblow posture /
/// Breath of Life twice (2026-10-12 bug hunt round 12).
fn one_shot_starts(start: f32, prev_t: f32, t: f32) -> bool {
    crate::tae_window::starts(start, prev_t, t)
}


/// ConsumeCurrentGoods (TAE 65, drink frame 21 / repeat frame 9): one charge, the heal, on the
/// tick whose step enters the event. The exe's handler FUN_140b57740 runs in the frame the
/// behaviour step takes that window (tools/emu-cases items: a000_250000's 0.7 s event on frame 42
/// of a press on frame 0); `decide` reads the window a tick later, so the use is after `advance`.
fn consume_goods(combat: Res<Combat>, config: Res<GameConfig>, mut log: ResMut<CombatLog>, mut player: Single<(&mut Player, &mut Actor)>) {
    let (p, a) = &mut *player;
    let d = &combat.player;
    if !matches!(a.state.trim_start_matches("Crouch"), "ItemGourdDrink" | "ItemGourdDrinkRepeat" | "ItemGourdDrinkMove" | "ItemGourdDrinkRepeatMove") {
        return;
    }
    let consumed = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 65 && one_shot_starts(e.start, a.prev_t, a.t)));
    if consumed && p.gourd > 0 {
        p.gourd -= 1;
        let rate = d.sp_effects.get(GOURD_SPEFFECT).map_or(40.0, |s| -s.change_hp_estus_flask_rate);
        let heal = estus_heal(a.hp_max, rate, medicine_rate(&combat, &config));
        a.hp = (a.hp + heal).min(a.hp_max);
        p.sp_bursts.push(GOURD_GOODS);
        log.push(format!("gourd: +{heal:.0} HP ({} left)", p.gourd), Color::srgb(0.5, 1.0, 0.6));
    }
}

/// The instant "Refresh" SpEffects Wolf's death and revival anims add (TAE 401 at f0,
/// motionInterval 0, effectEndurance 0): the death loop a000_110010 cures every ailment (8002,
/// stateInfo 13) and empties the poison / burn gauges (8003 poizonAttackPower / 8004 registBlood
/// -99999); GroundRevival_F a000_110030's 8001 "full posture" (changeStaminaRate 100). Before,
/// he came back still poisoned or burning with his posture as it was (2026-10-12 bug hunt round 15).
fn apply_refresh_effects(
    combat: Res<Combat>,
    statuses: Option<ResMut<crate::status::Statuses>>,
    mut log: ResMut<CombatLog>,
    mut q: Query<(Entity, &mut Actor, &mut Player)>,
) {
    let mut statuses = statuses;
    for (e, mut a, mut p) in &mut q {
        if a.anim.is_empty() || a.prev_t >= a.t {
            continue;
        }
        let d = crate::actor::data_for(&combat, &a);
        let Some(an) = d.anim(&a.anim) else { continue };
        let (mut cure, mut posture) = (false, 0.0f32);
        // The spCategories the refresh rows and their replacements take (8002 194 -> 8007 193
        // "Recovery sealed"): Wolf's timed rows in them go (round 66: the Owl's seal 3506090,
        // category 193, outlived the death loop's cure).
        let mut cats: Vec<i64> = Vec::new();
        // 401 and 67 (AddSpEffect): Ichimonji's 140210 (changeStaminaRate 12.5, one-shot) comes by
        // TAE 67 at the slash and was never applied (round 24).
        for ev in an.events.iter().filter(|ev| (ev.kind == 401 || ev.kind == 67) && one_shot_starts(ev.start, a.prev_t, a.t)) {
            let Some(s) = ev.arg_i64("SpEffectID").and_then(|id| d.sp_effects.get(&id.to_string())) else { continue };
            if s.motion_interval != 0.0 || s.effect_endurance != 0.0 {
                continue;
            }
            cure |= s.state_info == 13 || s.poizon_attack_power < 0.0 || s.regist_blood < 0.0;
            let mut id = ev.arg_i64("SpEffectID").unwrap_or(0);
            for _ in 0..4 {
                let Some(c) = crate::data::sp_cycle(id) else { break };
                if c.category > 0 {
                    cats.push(c.category);
                }
                id = c.replace;
            }
            posture += s.change_stamina_rate.max(0.0);
        }
        if cure {
            if let Some(st) = statuses.as_deref_mut() {
                st.clear(e);
            }
        }
        if !cats.is_empty() {
            p.timed.retain(|(id, _)| crate::data::sp_cycle(*id).is_none_or(|c| !cats.contains(&c.category)));
        }
        if posture > 0.0 && a.posture > 0.0 {
            let back = (a.posture_max * posture / 100.0).min(a.posture);
            a.posture -= back;
            log.push(format!("refresh: -{back:.0} posture"), Color::srgb(0.9, 0.8, 0.4));
        }
    }
}
/// The one-shot TAE events of the step that just ran (row E4, kb/frame-coverage.md "One-shot TAE
/// events' frame"): the exe dispatches them inside the chr frame's Havok step (TAE update
/// FUN_140b5c920 -> FUN_140b59170: 32 FUN_140b509b0, 67 FUN_140b56500, 401 FUN_140b56750, the
/// add FUN_1409e9c00 on the update that enters the event), so they land in the frame whose step
/// enters them, before that frame's HKS. Sv1 ran them in the next tick's `decide`, a tick late;
/// now after `advance`, on the window it just took (`apply_refresh_effects` the same).
fn tae_one_shots(combat: Res<Combat>, config: Res<GameConfig>, mut log: ResMut<CombatLog>, mut player: Single<(&mut Player, &mut Actor), Without<Enemy>>) {
    if FLOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let d = &combat.player;
    let (p, a) = &mut *player;
    // One-shot recoveries from the TAE's AddSpEffect (motionInterval 999: applied once): the
    // resurrection's 110015 "HP Half Recovery" (changeHpRate -50 -> +50 % of max HP, frame 0) and
    // the deathblow's (a20x_510xxx at the kill frame): 105050 posture 34 % for everyone, and the
    // skills' 150301 / 150311 HP 10 % and 150321 / 150331 posture 34 %, which work only while the
    // skill's permit stateInfo (986 / 987 / 984 / 985, invocationConditionsStateChange1) is on Wolf.
    // "Recovery prohibited by the Todome" (とどめスルーによる回復禁止: 105051, 150302 / 150312 /
    // 150322 / 150332): Wolf's Todome anims a000_710205-7 add them (TAE 401, f0, effectEndurance
    // -1, kept in `Player.held`); each shares its recovery's spCategory (179 / 175-178) at
    // categoryPriority 2 vs 3, and the lower priority wins in a category, so after a Todome these
    // recoveries do nothing.
    if a.prev_t < a.t {
        let states: Vec<i64> = crate::combat::skill_sp_effects(&combat, &config).filter_map(|se| se["stateInfo"].as_i64()).filter(|&s| s > 0).collect();
        let (mut heal, mut posture) = (0.0f32, 0.0f32);
        if let Some(an) = d.anim(&a.anim) {
            let fired: Vec<(i64, &crate::data::SpEffect)> = an
                .events
                .iter()
                .filter(|e| matches!(e.kind, 67 | 401) && one_shot_starts(e.start, a.prev_t, a.t))
                .filter_map(|e| {
                    let id = e.arg_i64("SpEffectID").unwrap_or(0);
                    Some((id, d.sp_effects.get(&id.to_string())?))
                })
                .collect();
            // A fired row that outranks a held one of its category replaces it: every deathblow
            // clip's f0 105052 / 150303 / 150313 / 150323 / 150333 (priority 1, "release recovery
            // prohibition") lift the 105051 / 1503x2 (priority 2, endurance -1) the Event7102xx
            // clips put on; they stayed until a reset (round 30).
            for &(_, s) in &fired {
                if s.sp_category > 0 {
                    p.held.retain(|h| d.sp_effects.get(&h.to_string()).is_none_or(|h| h.sp_category != s.sp_category || s.category_priority >= h.category_priority));
                }
            }
            for &(id, s) in &fired {
                if s.effect_endurance < 0.0 && s.sp_category > 0 && !p.held.contains(&id) {
                    p.held.push(id);
                }
            }
            let blocked = |s: &crate::data::SpEffect| {
                s.sp_category > 0
                    && p.held.iter().filter_map(|h| d.sp_effects.get(&h.to_string())).any(|h| h.sp_category == s.sp_category && h.category_priority < s.category_priority)
            };
            for s in fired.iter().map(|f| f.1).filter(|s| s.motion_interval >= 999.0 && !blocked(s)) {
                let conds = [s.invocation_conditions_state_change1, s.invocation_conditions_state_change2, s.invocation_conditions_state_change3];
                if conds.iter().any(|&c| c > 0) && !conds.iter().any(|c| *c > 0 && states.contains(c)) {
                    continue;
                }
                heal += (-s.change_hp_rate).max(0.0);
                posture += s.change_stamina_rate.max(0.0);
            }
        }
        if heal > 0.0 {
            let gain = (a.hp_max * heal / 100.0).min(a.hp_max - a.hp);
            a.hp += gain;
            let what = if a.state.contains("Revival_") { "resurrected" } else { "deathblow" };
            log.push(format!("{what}: +{:.0} HP", a.hp_max * heal / 100.0), Color::srgb(1.0, 0.5, 0.5));
        }
        if posture > 0.0 && a.posture > 0.0 {
            let back = (a.posture_max * posture / 100.0).min(a.posture);
            a.posture -= back;
            log.push(format!("deathblow: -{back:.0} posture"), Color::srgb(0.9, 0.8, 0.4));
        }
    }
    // Weapon style (TAE 32 SetWeaponStyle): "0: None" = sheathed (the blade rests in the scabbard,
    // WepAbsorpPosParam rightHang_0), "1: Right Weapon One-Handed" = in the hand.
    if let Some(an) = d.anim(&a.anim).filter(|_| a.prev_t < a.t) {
        let style = an.events.iter().filter(|e| e.kind == 32 && one_shot_starts(e.start, a.prev_t, a.t)).filter_map(|e| e.args.get("WeaponStyle")?.as_str()).last();
        if let Some(style) = style {
            a.sheathed = style.starts_with('0');
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn decide(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    lock: Res<LockOn>,
    mut pad: ResMut<PadInput>,
    mut log: ResMut<CombatLog>,
    view: Res<View>,
    mut player: Single<(Entity, &mut Player, &mut Actor, &mut Transform), Without<Enemy>>,
    mut enemies: Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    enemy_dummies: Query<&crate::model::Dummies, With<Enemy>>,
    globals: Query<&GlobalTransform>,
    mut throw: ResMut<ActiveThrow>,
    mut fight: Option<ResMut<crate::enemy::FightReset>>,
    mut snap: Option<ResMut<crate::view::CameraSnap>>,
    (wire_aim, grapples, ledges, mut used): (Option<Res<crate::wire::WireAim>>, Option<Res<crate::enemy::grapple::EnemyGrapples>>, Option<Res<crate::traversal::LedgeEdges>>, Option<ResMut<crate::items::Used>>),
) {
    // UFO (ufo.rs): Wolf is flown, not played; presses dropped.
    if FLOWN.load(std::sync::atomic::Ordering::Relaxed) {
        pad.hold();
        return;
    }
    let dt = time.delta_secs();
    let d = &combat.player;
    let (pe, p, a, tf) = &mut *player;
    // The stick the pad manipulator took this frame (`pad_manipulator`).
    let gated_stick = p.turn.taken;
    // The last ground's height (`air_top`, where the fall height starts) on every grounded tick,
    // before the states that return early (aging, hangs, throws) skip the movement's update.
    if !a.airborne {
        p.air_top = tf.translation.y;
    }
    // The quick items' worn effects (items.rs): Wolf's damage rates and the regeneration.
    let [atk, stam, cut, posture_cut] = crate::items::wolf_rates(&p.timed);
    // And his Attack Power (combat::growth_sp): every hit's HP and posture damage. The HP rate
    // rides the hit record as whole percent of its product with the items' (combat::record_rate:
    // AP 16's 4.45 -> 4.44, with Ako's Sugar 4.45 x 1.125 -> 5.00).
    let growth = crate::items::param("SpEffectParam", crate::combat::growth_sp(&config));
    let rate = |k: &str| growth[k].as_f64().unwrap_or(1.0) as f32;
    (a.atk_rate, a.stam_atk_rate, a.dmg_cut, a.posture_cut) = (crate::combat::record_rate(rate("physicsAttackRate") * atk), stam * rate("staminaAttackRate"), cut, posture_cut);
    a.item_sp.clear();
    a.item_sp.extend(p.timed.iter().map(|t| t.0));
    // Their maxHpRate (Yashariku's Sugar halves the max HP for its 30 s; the user, 2026-10-18:
    // "make yashariku's sugar limit max hp to half hp of wolf"): hp_max rescaled when it
    // changes, HP kept under it and not given back after (bCurrHPIndependeMaxHP 1).
    let hp_rate = crate::items::max_hp_rate(&a.item_sp);
    if (hp_rate - p.item_hp_rate).abs() > 1e-6 {
        a.hp_max *= hp_rate / p.item_hp_rate;
        a.hp = a.hp.min(a.hp_max);
        p.item_hp_rate = hp_rate;
    }
    // Its maxStaminaRate 0.5 halves the max posture the same way: the stamina left (max - the
    // posture taken) is kept under the new max and not given back after.
    let st_rate = crate::items::max_stamina_rate(&a.item_sp);
    if (st_rate - p.item_posture_rate).abs() > 1e-6 {
        let left = (a.posture_max - a.posture).max(0.0);
        a.posture_max *= st_rate / p.item_posture_rate;
        a.posture = (a.posture_max - left.min(a.posture_max)).max(0.0);
        p.item_posture_rate = st_rate;
    }
    let regen_dt = a.clock(dt);
    crate::items::tick_regen(&p.timed, a, regen_dt);
    // Next quick item (C / d-pad down): the next filled slot.
    if p.requests.remove(&Action::NextItem).is_some() {
        let next = crate::items::next_slot(&config, p.quick_slot);
        if next != p.quick_slot {
            p.quick_slot = next;
            let id = crate::items::in_slot(&config, next);
            let name = crate::items::param("EquipParamGoods", id)["name"].as_str().unwrap_or("").to_string();
            log.push(format!("quick item: {}", if name.is_empty() { id.to_string() } else { name }), Color::srgb(0.85, 0.85, 0.75));
        }
    }
    // A quick item in use: spent and in effect at its ConsumeCurrentGoods (TAE 65).
    if let Some((goods, state)) = p.item_use.clone() {
        if a.state.trim_start_matches("Crouch") != state {
            p.item_use = None;
        // At frame 0 too (Bite Down's, a000_250400): on the clip's first step.
        } else if d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 65 && ((e.start > a.prev_t && e.start <= a.t) || (e.start == 0.0 && a.prev_t == 0.0 && a.t > 0.0)))) {
            let extend = crate::items::extend_rate(crate::combat::skill_sp_ids(&combat, &config));
            for sp in crate::items::effects_of(goods) {
                crate::items::wear(&mut p.timed, sp, extend);
            }
            if let Some(u) = used.as_deref_mut() {
                u.0.push(goods);
            }
            // The Homeward Idol: back to the last idol prayed at (idol.rs).
            if goods == crate::items::HOMEWARD_IDOL {
                if let Some((at, yaw)) = p.seat.checkpoint.and_then(crate::encounter::world().respawn_at) {
                    tf.translation = at + Vec3::Y * CAPSULE_HALF_HEIGHT;
                    a.yaw = yaw;
                    if let Some(s) = snap.as_deref_mut() {
                        s.0 = true;
                    }
                }
            }
            log.push(format!("used goods {goods}"), Color::srgb(0.5, 1.0, 0.6));
        }
    }
    // A wire on an enemy follows him (wire.rs track): the shoot's snapshot left Wolf flying to
    // where Gyoubu / the Ape stood a second before.
    if let (Some(w), Some(gs)) = (p.wire.as_mut(), grapples.as_deref()) {
        if let Some(g) = w.enemy.and_then(|e| gs.0.iter().find(|g| g.entity == e)) {
            w.track(g, tf.translation);
        }
    }
    // The Lightning Reversal charge wears off (effectEndurance 30 s), or the release's
    // SpEffects 9505 / 9506 ("discharge parent", replaced by 9507 / 9508) discharge it.
    // A lightning hit on the ground: its charge's shock (round 43).
    if let Some(charge) = a.shock.take().filter(|_| !a.airborne && a.hp > 0.0) {
        if let Some(state) = lightning_shock(&combat, a, charge) {
            log.push("SHOCKED - lightning on the ground", Color::srgb(0.6, 0.8, 1.0));
            if a.hp > 0.0 {
                a.play_state(d, state);
            }
        }
    }
    if let Some((id, left)) = a.electro {
        let discharged = !a.anim.is_empty() && d.sp_effects_at(&a.anim, a.step()).iter().any(|(id, _)| matches!(id, 9505 | 9506));
        let sp_dt = a.clock(dt);
        a.electro = (left > sp_dt && !discharged && a.hp > 0.0).then_some((id, left - sp_dt));
    }
    // Set again by the guard walk below when it applies; a locked-on step keeps its tilt.
    a.root_yaw = if a.state.starts_with("GroundStep_") {
        p.step_tilt
    } else if jump_tilted(&a.state) {
        p.jump_tilt
    } else {
        0.0
    };
    // WalkTwist (exe FUN_1407f6ff0): last tick's target eases in; locomotion / the guard walk set
    // the next one below, anything else lets it return to 0.
    let step = WALK_TWIST_RATE * dt;
    a.twist += (a.twist_target - a.twist).clamp(-step, step);
    a.twist_target = 0.0;

    // ActionRequest buffer (exe: SprjChrActionRequestModule, FUN_140b2c190). A press is
    // latched only while its accept bit is set (TAE accept flags, see buffers()) and stays
    // pending, with no timer, until the action fires on its execute flag (accept_flag) or a
    // state change resets it (keeps_buffer). config input.buffer > 0 adds a timed expiry
    // for experiments.
    let entered = a.state != p.last_state;
    let prev_state = p.last_state.clone();
    if entered {
        if !keeps_buffer(&a.state) {
            p.requests.clear();
        }
        // Combat changed the state; record_state then only sees decide's own changes.
        p.last_state = a.state.clone();
    }
    // Raw press (death screen choices are not TAE-buffered).
    let attack_pressed = pad.pressed.contains(&Action::Attack);
    let guard_pressed = pad.pressed.contains(&Action::Guard);
    // Docking reads its presses raw (docking.rs: the hug / hang button, the climb and the drop).
    let interact_pressed = pad.pressed.contains(&Action::Interact);
    let dock_jump = pad.pressed.contains(&Action::Jump);
    let dock_step = pad.pressed.contains(&Action::Step);
    let item_pressed = pad.pressed.contains(&Action::UseItem);
    // Any other action stands Wolf up. Steps, guards and hit reactions from a crouch are the
    // standing ones in the HKS too (_fireGroundStep -> W_GroundStep, W_StandToDeflectGuard; the
    // Crouch{Damage,Death}* states only under a forced crouch, g_forceCrouch). The attack has
    // its own (attack_route: CrouchAttackToStand), the gourd too (CrouchItemGourdDrink*).
    if a.crouch && !crouch_ok(&a.state) {
        a.crouch = false;
    }
    // Swimming (swim.rs), Ctrl and F are the dive and the rise: no tool, no grappling hook.
    let swim_dive = pad.pressed.contains(&Action::Prosthetic);
    if p.swim.is_some() {
        pad.pressed.retain(|act| !matches!(act, Action::Prosthetic | Action::Grapple));
    }
    let presses = pad.pressed.len() as u32;
    for act in pad.pressed.drain(..) {
        if buffers(d, a, act) {
            p.requests.insert(act, 0.0);
        }
    }
    // Grab escape (kb/enemy.md "Grab escape"; round 28). The row Wolf is held by (defAnimId /
    // defAnimOffset of his clip, AtkChrId of the grabber) gives escHp; presses in each
    // selfEscCycleTime window (read as ms) take 1 off it. Once it is 0, the grabber's next TAE
    // ChrActionFlag 70 pulse (c1500 4120: every 15 frames) with Wolf in throw state 3/4 is
    // env 277 (FUN_140b39e90): BEH_R_THROW_ESCAPE -> W_ThrowEscape<ThrowId>
    // (ThrowEscape600350 / 600370 = the grab clip + 2: a223_600352 / 600372). gaps: which buttons
    // FUN_140807eb0 counts (here any press; its stick shake is not modelled). The damage path
    // (FUN_140b68ff0: a hit while the victim's TAE flag 68 bit is set takes the damage off escHp)
    // never applies: no Wolf clip carries flag 68 (checked over all of c0000's anims, round 31).
    if a.state == "Grabbed" && a.hp > 0.0 {
        let grabber = enemies.iter().filter(|(_, ea, ..)| ea.state.starts_with("ThrowAtk")).min_by(|x, y| x.3.translation.distance(tf.translation).total_cmp(&y.3.translation.distance(tf.translation))).map(|(_, ea, ..)| (ea.anim.clone(), ea.t, combat.foe_of(ea).chr_num, ea.kind));
        if p.escape.is_none() {
            let (group, id) = (a.anim.get(1..4).and_then(|g| g.parse::<i64>().ok()), a.anim.get(5..).and_then(|n| n.parse::<i64>().ok()));
            let row = combat.params.get("ThrowParam").and_then(|t| t.as_object()).and_then(|rows| {
                rows.values().find(|r| {
                    r["defAnimId"].as_i64() == id && r["defAnimOffset"].as_i64() == group && grabber.as_ref().is_none_or(|g| r["AtkChrId"].as_i64() == Some(g.2))
                })
            });
            if let Some(r) = row.filter(|r| r["escHp"].as_i64().unwrap_or(0) > 0) {
                p.escape = Some(GrabEscape {
                    hp: r["escHp"].as_i64().unwrap_or(0),
                    window: r["selfEscCycleTime"].as_f64().unwrap_or(100.0) as f32 / 1000.0,
                    need: r["selfEscCycleCnt"].as_u64().unwrap_or(1).max(1) as u32,
                    presses: 0,
                    t: 0.0,
                });
            }
        }
        if let Some(esc) = p.escape.as_mut() {
            esc.presses += presses;
            esc.t += dt;
            if esc.t >= esc.window {
                if esc.presses >= esc.need {
                    esc.hp -= 1;
                }
                esc.presses = 0;
                esc.t = 0.0;
            }
            let pulse = grabber.as_ref().is_some_and(|(anim, t, _, kind)| !anim.is_empty() && combat.kind(*kind).data.flag(anim, *t, FLAG_THROW_ESCAPE_CHECK));
            if esc.hp <= 0 && pulse {
                let key = a.anim.get(5..).and_then(|n| n.parse::<i64>().ok()).map(|n| format!("{}_{:06}", &a.anim[..4], n + 2));
                if let Some(k) = key.filter(|k| d.anim(k).is_some()) {
                    a.play("ThrowEscape", &k);
                    log.push("escaped the grab", Color::srgb(0.6, 1.0, 0.6));
                }
                p.escape = None;
            }
        }
    } else {
        p.escape = None;
    }
    let buffer = config.input.buffer;
    p.requests.retain(|_, age| {
        *age += dt;
        buffer <= 0.0 || *age <= buffer
    });

    if pad.reset {
        pad.reset = false;
        // The whole fight starts over (the enemies' starting line-up).
        if let Some(f) = fight.as_mut() {
            f.0 = true;
        }
        // Drawn at the start spot at once: no fade from the old pose, no deathblow model slide
        // carried over (2026-10-12 bug hunt round 17).
        commands.entity(*pe).insert(crate::anim::SnapPose);
        p.gourd = config.player.gourd_charges;
        p.emblems = config.player.spirit_emblems;
        p.resurrections = config.player.resurrections;
        p.revive_locked = false;
        p.held.clear();
        // A reset mid-jump, mid-plunge or crouched starts the fight standing on the ground:
        // the air state, knockback and queued presses go too (2026-10-12 bug hunt).
        p.requests.clear();
        p.plunge = None;
        p.perch = None;
        p.vel_scale = None;
        p.timed.clear();
        a.airborne = false;
        a.fall_type = 0;
        // Lightning Reversal's charge goes with the air state (it was kept for up to 30 s and
        // fired at the next fight's line-up; 2026-10-12 bug hunt round 13).
        a.electro = None;
        // Aging goes with the statuses (Statuses::clear_all); its row off the body before
        // hp_max is set again.
        if let Some(id) = a.aged.take() {
            a.resident.retain(|r| *r != id);
        }
        // The aged form's "back as AgingIdle" flag, and a death pick no death consumed (a grab
        // death, god mode): the next life ages with W_StandAging and dies its own death (round 52).
        p.aged_form = false;
        a.death_ref = 0;
        a.death_hit = None;
        a.shock = None;
        a.vel_y = 0.0;
        a.air_base = Vec3::ZERO;
        a.move_vel = Vec3::ZERO;
        a.kb_cont = 0.0;
        a.kb_dec = 0.0;
        a.kb_vel = Vec3::ZERO;
        a.crouch = false;
        // The old fight's clock state (round 58; enemies get a fresh Actor): a running hit stop
        // froze Wolf at the start spot, a deflect's additive window stayed live, the last clip's
        // TAE SpEffects (tae_prev) handed on to their replacements on the reset tick (109450 ->
        // 109451, regen off), and the chained / tool buffs carried over.
        a.hit_stop = Default::default();
        a.add_anim.clear();
        a.add_t = 0.0;
        a.tae_prev.clear();
        a.sp_chained.clear();
        a.attack_hit = false;
        a.regen_carry = 0.0;
        a.since_posture_damage = 99.0;
        p.revive_queued = false;
        p.death_wait = 0.0;
        p.dead_for = 0.0;
        // config posture.player_level after F5 (it was read only at spawn; round 24).
        (a.hp_max, a.posture_max, a.posture_regen) = level_stats(&combat, config.posture.player_level);
        p.item_hp_rate = 1.0;
        a.hp = a.hp_max;
        a.posture = 0.0;
        a.play("StandIdle", "");
        tf.translation = (crate::encounter::hooks().player_start)();
        a.yaw = start_yaw();
        // Back at the last Sculptor's Idol prayed at, standing before it (idol.rs).
        let checkpoint = p.seat.checkpoint;
        p.seat = crate::seat::Seat { checkpoint, ..Default::default() };
        if let Some((at, yaw)) = checkpoint.and_then(crate::encounter::world().respawn_at) {
            tf.translation = at + Vec3::Y * CAPSULE_HALF_HEIGHT;
            a.yaw = yaw;
        }
        if let Some(mut s) = snap {
            s.0 = true;
        }
    }
    // Sheathe / draw (X): HKS BEH_R_NON_COMBAT_AREA_ENTER / LEAVE, standing or moving variant
    // (GroundNonCombatArea[Move]Enter a000_700500 / 700501, Leave 700510 / 700511). Sheathed, an
    // attack / guard / art / prosthetic press draws the sword instead (Sekiro has no sword
    // actions while sheathed).
    let sword_press = [Action::Attack, Action::Guard, Action::CombatArt, Action::Prosthetic].iter().any(|k| p.requests.contains_key(k));
    let sheathe = std::mem::take(&mut pad.sheathe);
    if a.hp > 0.0 && !a.airborne && (sheathe || a.sheathed && sword_press) && (is_free(&a.state) || (!a.anim.is_empty() && accepts(d, a, Action::Attack))) {
        let moving = a.state == "Locomotion";
        let state = match (a.sheathed, moving) {
            (false, false) => "GroundNonCombatAreaEnter",
            (false, true) => "GroundNonCombatAreaMoveEnter",
            (true, false) => "GroundNonCombatAreaLeave",
            (true, true) => "GroundNonCombatAreaMoveLeave",
        };
        if a.play_state(d, state) {
            a.move_vel = Vec3::ZERO;
            p.requests.clear();
            return;
        }
    }
    // The crouch (BEH_A_CROUCH_START / _END, the sprint's W_SprintToCrouchReady) is the script's:
    // hks_wolf::take_crouch.
    if a.sheathed {
        for k in [Action::Attack, Action::Guard, Action::CombatArt, Action::Prosthetic] {
            p.requests.remove(&k);
        }
    }
    // Killed in a grab: exe env 206 (any throw state) blocks BEH_R_DEATH, so the grab clip runs on;
    // the damage that empties HP mid-throw sets throw state 6 (FUN_140b6f690 -> FUN_1407fa400),
    // and at the grab clip's own ChrActionFlag 69 (env 276 = flag 69 and state 6)
    // W_ThrowDefDeathStart<ThrowId> plays the clip + 1 (a210_600000 -> 600001, a223_600140 ->
    // 600141 ...), which ends into the death / revival-wait loop as GroundDeathStart does
    // (HKS 326-333). Before, Wolf snapped to GroundDeathStart_F mid-throw (round 27).
    if a.hp <= 0.0 && a.state == "Grabbed" {
        if !a.anim.is_empty() && d.flag(&a.anim, a.step(), FLAG_THROW_TYPE5) {
            let death = a.anim.get(5..).and_then(|n| n.parse::<i64>().ok()).map(|n| format!("{}_{:06}", &a.anim[..4], n + 1));
            if let Some(k) = death.filter(|k| d.anim(k).is_some()) {
                a.play("ThrowDefDeathStart", &k);
            }
        }
        if a.state == "Grabbed" && (a.anim.is_empty() || a.t < d.length(&a.anim)) {
            a.move_vel = Vec3::ZERO;
            return;
        }
        // The Guardian Ape's 600380 has no death start: its grab ends in the Death blow-away.
        if a.state == "Grabbed" {
            if let Some(st) = grab_end_state(&a.anim, a.hp).filter(|s| s.starts_with("ThrowDefDeath")) {
                a.play_state(d, st);
            }
        }
    }
    // The fall-return deaths (HKS BEH_R_DEATH 769-776): the screen cuts to black (menu.rs
    // `fall_cut`) for `FALL_RETURN_HOLD` of the clip, then Wolf stands at the last safe ground.
    // They keep the resurrections; both took (int)(40 %) of max HP when they started
    // (`fall_return_hp`). A void fall that emptied HP (the exe's ordinary death in the air, type
    // 6) dies here after the cut (gap: the sim has no death in the void), resurrections as usual. Standing, not the get-up GroundFallReturn (a000_711210: the user,
    // "why does he have animation when he returns").
    if is_fall_return(&a.state) {
        a.move_vel = Vec3::ZERO;
        if a.anim.is_empty() || a.t >= d.length(&a.anim).min(FALL_RETURN_HOLD) {
            if let Some(&safe) = p.safe_ring.front() {
                tf.translation = safe;
            }
            a.airborne = false;
            a.vel_y = 0.0;
            p.air_top = tf.translation.y;
            a.play("StandIdle", "");
        }
        return;
    }
    if a.state == "GroundFallReturn" {
        a.move_vel = Vec3::ZERO;
        if a.anim.is_empty() || a.t >= d.length(&a.anim) {
            a.play("StandIdle", "");
        }
        return;
    }
    // BEH_R_DEATH fires whenever HP is out (HKS 5605), the revival too (round 60: a tick that
    // emptied the revived 1 HP before 110015's +50 % let Wolf get up at 0 and act; the revive
    // tick itself sets 1 HP).
    if a.hp <= 0.0 {
        // The resurrection choice (round 66, static): at death FUN_140a2cc30
        // arms +0x2138 = 1.8 s (+0x2134) when a resurrection is allowed and no stateInfo 341
        // (aging) / 359 is worn; FUN_140a36a60 counts it down (held at 0
        // while stateInfo 351 is worn), then opens +0x212c = 15 s (+0x2128); its end is the
        // final death (FUN_1409eda50). The HKS revives from the death starts themselves too
        // (c0000_transition.lua 274-277), not only the stay loops. Before: the choice opened at
        // the loop (1.6-3.3 s late) and closed 6 s into it.
        let holding = wolf_wears(a, p, d, 351);
        if !(holding && p.dead_for < REVIVE_DELAY) {
            p.dead_for += dt;
        }
        // GroundDeathStart (100 frames), then the choice: attack resurrects while a node is left
        // (GroundRevival_F), else back to the start position after the death loop.
        // BEH_R_DEATH (c0000_transition 974-983): killed from behind (damage_angle_FB == B) is
        // DEATH_TYPE_COMMON_BACK, the _B start / loops (a000_110001 / 110011 / 110021), face down
        // (round 33: always _F). gap: the blow / upper / fling deaths (918-951) have no exported
        // clips; they take this plain death too.
        let side = if a.hit_from_back { "B" } else { "F" };
        throw_blow_step(a, d);
        // BEH_R_DEATH 966-971: killed by terror (ref 590) -> W_GroundDeathStartInsane (a000_191600),
        // by a poison tick (ref 500, damage level none) -> W_GroundDeathStartPoison (191400), both
        // IndexDeath B. Round 38: they took the plain start. Round 40: a killing fire hit
        // (combat.rs) -> W_GroundDeathStartFire (191200). gap: a burn tick's element (env 285).
        // Round 40: the killing hit's level / element first (death_starts: the blows, upper,
        // fling, lightning; before, all took the plain start).
        // A death that plays its own anim (a grab's ThrowDefDeath*) spends the pick too; it was
        // left for the next life's plain-hit death (round 52).
        if is_death_state(&a.state) {
            a.death_ref = 0;
            a.death_hit = None;
        }
        // A throw's blow-away (ThrowDefLargeBlow*: env 206, a throw state, blocks BEH_R_DEATH)
        // plays out; its landing's end takes the death start.
        let in_blow = a.state.starts_with("ThrowDef") && !a.anim.is_empty() && a.t < d.length(&a.anim);
        if !is_death_state(&a.state) && !in_blow {
            let hit = a.death_hit.take();
            // Fire / terror / poison are the ground branch's (HKS 960-972), not the air one's.
            let special = match std::mem::take(&mut a.death_ref) {
                _ if a.airborne => None,
                crate::status::REF_ENABLE_INSANE => Some("GroundDeathStartInsane"),
                crate::status::REF_POISON => Some("GroundDeathStartPoison"),
                crate::status::REF_FIRE_ADD_DAMAGE_BURN => Some("GroundDeathStartFire"),
                _ => None,
            };
            let by_hit = death_starts(hit, a.airborne, a.hit_from_back, a.hit_back_quadrant, a.aged.is_some());
            // Aged on the ground (BEH_R_DEATH 970, STYLE_TYPE_AGING_STAND, after fire / terror /
            // poison): W_AgingGroundDeathStart (a000_192500).
            let aged = (a.aged.is_some() && !a.airborne).then_some("AgingGroundDeathStart");
            if !by_hit.iter().any(|s| a.play_state(d, s)) && !special.is_some_and(|s| a.play_state(d, s)) && !aged.is_some_and(|s| a.play_state(d, s)) && !a.play_state(d, &format!("GroundDeathStart_{side}")) {
                a.procedural("Dead");
            }
        }
        death_chain_step(a, d);
        // The Ape's Death blow-away landing is a death start too (HKS 275 / 328), as are the
        // blow / fall deaths' ends (death_done).
        let start_done = (a.state.starts_with("GroundDeathStart") || a.state == "ThrowDefDeathStart" || death_done(&a.state).is_some() || matches!(a.state.as_str(), "AgingGroundDeathStart" | "LandAgingFallDeath"))
            && !a.anim.is_empty()
            && a.t >= d.length(&a.anim);
        // c0000_transition.lua 328-333: the death start ends in W_GroundDeathLoop when the death
        // is final (env 3028), else in W_GroundRevivalStayLoop (a000_110020), which wears 110010
        // "Pretending Dead" (sightSearchEnemyCut 100; common_common_logic_func.lua:16-22: the
        // enemy clears its targets) - the plain loop has none of it (round 27). gap: env(3028)
        // read as "no resurrection left".
        // Killed in a grab: the loops are the throw's own (354-356: W_ThrowDefDeathLoop /
        // W_ThrowDefRevivalStayLoop .. Selector_ThrowId, CMSGs a000_<ThrowId>_DeathLoop /
        // _RevivalStayLoop, offsetType 17 = the grab's group: a221_610111 / 610112 ...), the
        // ThrowId being the start clip's id - 1. Round 37: they fell back to the plain ground loops.
        let throw_loop = (start_done && a.state == "ThrowDefDeathStart")
            .then(|| a.anim.get(5..).and_then(|n| n.parse::<i64>().ok()))
            .flatten()
            .map(|n| (n - 1, if p.resurrections > 0 { "RevivalStayLoop" } else { "DeathLoop" }))
            .and_then(|(id, kind)| Some((format!("ThrowDef{kind}"), d.anim_key(&format!("a000_{id}_{kind}"))?.to_string())))
            .filter(|(_, k)| d.anim(k).is_some());
        if let Some((state, key)) = throw_loop {
            a.play(&state, &key);
            p.death_wait = 0.0;
        } else if start_done && matches!(a.state.as_str(), "AgingGroundDeathStart" | "LandAgingFallDeath") {
            // HKS 365: the aged deaths end in W_AgingGroundDeathLoop, never the revival stay loop.
            a.play_state(d, "AgingGroundDeathLoop");
            p.death_wait = 0.0;
        } else if start_done {
            let (land, side) = death_done(&a.state).unwrap_or((false, if a.state.ends_with("_B") { "B" } else { "F" }));
            let kind = if p.resurrections > 0 { "RevivalStayLoop" } else { "DeathLoop" };
            if !(land && a.play_state(d, &format!("Land{kind}_{side}"))) {
                a.play_state(d, &format!("Ground{kind}_{side}"));
            }
            p.death_wait = 0.0;
        }
        a.move_vel = Vec3::ZERO;
        // Killed in the air (e.g. hit as a jump starts): the body still falls and lands
        // instead of keeping its launch velocity. The floor counts the perch he is over, as the
        // air code's (round 60: the body fell through it), and a body carried off a ledge by
        // the killing blow's knockback or the clip's root motion falls (round 60: it floated;
        // the walk-off check is past this branch's return). Not in a grab's deaths (the
        // thrower places him) or a SetNoGravity window.
        let mut floor = crate::terrain::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT;
        if let Some(spot) = p.perch.filter(|s| (tf.translation - *s).with_y(0.0).length() < PERCH_RADIUS && tf.translation.y >= s.y + CAPSULE_HALF_HEIGHT - 0.3) {
            floor = floor.max(spot.y + CAPSULE_HALF_HEIGHT);
        }
        let held = a.state.starts_with("ThrowDef") || a.state == "Grabbed" || (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_NO_GRAVITY));
        if !a.airborne && !held && tf.translation.y > floor + 0.05 {
            a.airborne = true;
            a.fall_type = 0;
            a.vel_y = 0.0;
        }
        if a.airborne {
            let g = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)].gravity();
            a.grav_y = g;
            a.vel_y += g * dt;
            a.air_base = Vec3::ZERO;
            if tf.translation.y <= floor && a.vel_y < 0.0 {
                tf.translation.y = floor;
                a.airborne = false;
                a.fall_type = 0;
                a.vel_y = 0.0;
            }
        }
        // The choice opens once the loop has advanced past its f0 refresh events (8002-8009 cure,
        // applied by apply_refresh_effects after the step that enters them): a press on the loop's
        // first two ticks revived Wolf still poisoned / burning (2026-10-12 round 23).
        let in_loop = matches!(
            a.state.as_str(),
            "GroundDeathLoop_F" | "GroundDeathLoop_B" | "GroundRevivalStayLoop_F" | "GroundRevivalStayLoop_B" | "LandDeathLoop_F" | "LandDeathLoop_B" | "LandRevivalStayLoop_F" | "LandRevivalStayLoop_B" | "ThrowDefDeathLoop" | "ThrowDefRevivalStayLoop" | "AgingGroundDeathLoop"
        );
        if in_loop && !start_done {
            p.death_wait += dt;
            let len = d.length(&a.anim);
            if len > 0.0 && a.t >= len {
                a.t = a.t.rem_euclid(len);
                a.prev_t = a.t;
            }
        }
        let loop_ready = in_loop && (a.prev_t > 0.0 || p.death_wait > 0.1);
        // HKS 275 / 277 / 285: the death starts W_GroundRevival / W_LandRevival /
        // W_ThrowDefRevival take (the blow and fall deaths' lands among them, death_done).
        let start_revivable = !start_done && (a.state.starts_with("GroundDeathStart") || a.state.starts_with("LandDeathStart") || a.state == "ThrowDefDeathStart" || death_done(&a.state).is_some());
        let can_revive = !p.gave_up && !p.revive_locked && p.resurrections > 0 && a.state != "AgingGroundDeathLoop";
        let choosing = if can_revive { p.dead_for >= REVIVE_DELAY && (loop_ready || start_revivable) } else { loop_ready };
        if in_loop && attack_pressed && !choosing {
            p.revive_queued = true;
        }
        // The queued press is spent whenever the choice opens (round 52: `||` skipped the take on a
        // fresh press, and the leftover revived the next death on its first tick, a second node).
        let queued = choosing && std::mem::take(&mut p.revive_queued);
        p.revive_open = choosing && can_revive;
        // Die (guard; the game's ": Die" beside ": Resurrect" under 死): the node is kept, the
        // death screen fades to black and the final death follows (menu.rs watch_fight).
        if p.revive_open && guard_pressed && !attack_pressed && !queued {
            p.gave_up = true;
            p.revive_open = false;
            log.push("death", Color::srgb(1.0, 0.4, 0.4));
            return;
        }
        if p.gave_up {
            return;
        }
        if choosing && (attack_pressed || queued) && can_revive {
            p.resurrections -= 1;
            p.revive_locked = true;
            a.hp = 1.0; // the revival anim's SpEffect brings it to half
            // The same IndexDeath side: GroundRevival_B (a000_110031) gets up from face down; out
            // of a grab's loop the throw's own W_ThrowDefRevival (285: a000_<ThrowId>_Revival,
            // the loop clip's id + 1: a221_610112 -> 610113).
            let throw_revival = matches!(a.state.as_str(), "ThrowDefRevivalStayLoop" | "ThrowDefDeathLoop" | "ThrowDefDeathStart")
                .then(|| a.anim.get(5..).and_then(|n| n.parse::<i64>().ok()))
                .flatten()
                .map(|n| format!("{}_{:06}", &a.anim[..4], n / 10 * 10 + 3))
                .filter(|k| d.anim(k).is_some());
            if let Some(k) = throw_revival {
                a.play("GroundRevival_Throw", &k);
            } else {
                // Out of the Land loops W_LandRevival (HKS 277: a000_112050 / 112051). A death
                // start gets up on its own IndexDeath side (death_done).
                let side = death_done(&a.state).map_or(if a.state.ends_with("_B") { "B" } else { "F" }, |x| x.1);
                if !(a.state.starts_with("Land") && a.play_state(d, &format!("LandRevival_{side}"))) {
                    a.play_state(d, &format!("GroundRevival_{side}"));
                }
            }
            p.revive_open = false;
            log.push(format!("resurrection ({} left)", p.resurrections), Color::srgb(1.0, 0.5, 0.5));
            return;
        }
        let over = if can_revive { p.dead_for > REVIVE_DELAY + REVIVE_WINDOW } else { loop_ready && p.death_wait > 1.5 };
        if over {
            pad.reset = true;
            log.push("revived", Color::WHITE);
        }
        return;
    }

    p.dead_for = 0.0;
    p.revive_open = false;
    p.gave_up = false;
    // A deathblow landed: the resurrection unlocks.
    if p.revive_locked && a.state == "Deathblow" {
        p.revive_locked = false;
        log.push("resurrection unlocked", Color::srgb(1.0, 0.5, 0.5));
    }
    // Left above the floor by a clip's vertical root motion (the vault 511900) once its
    // SetNoGravity ends: fall and land (live: FreeFall 200010 -> LandFreeFall 200020).
    // A wire perch holds him while he stays on it (`Player::perch`).
    // Kept through a jump or an air reaction on it (round 53: dropped the moment Wolf left the
    // ground, so he landed through the perch on the ground below); leaving it sideways drops it.
    p.perch = p.perch.filter(|spot| (tf.translation - *spot).with_y(0.0).length() < PERCH_RADIUS);
    let perched = p.perch.is_some_and(|spot| tf.translation.y <= spot.y + CAPSULE_HALF_HEIGHT + 0.05);
    // Not in a wire shot / move start: its launch fires on the tick SetNoGravity ends, after this
    // check, and a grapple from a perch fell off it instead (2026-10-12 bug hunt round 20).
    let wiring = p.wire.is_some() && crate::wire::is_wire_state(&a.state);
    // Docked at an edge (docking.rs: hanging, or climbing up off it) is held, not over a floor.
    // Swimming (swim.rs) is held by the water too.
    let docked = p.dock.is_some() || p.swim.is_some() || crate::docking::is_jump_ready(&a.state) || a.state.starts_with("WallJumpReady_");
    // Not in a deathblow either: the throw plays out and only its end falls (HKS
    // c0000_transition.hks FireStateEndEvent: HKB_STATE_THROW_ATK_TO_FREE_FALL -> W_FreeFall on
    // IsAnimEnd). The Demon of Hatred's leap (a240_500000 / 510000) left Wolf above his field's
    // uneven ground and the fall cut the deathblow, so the Demon never fell (2026-10-17).
    let throwing = crate::actor::in_throw(&a) || a.state == "DeathblowStart";
    // Over a drop his capsule's rim still rests on, he stands (terrain.rs `rim_floor`).
    let on_rim = || crate::terrain::rim_floor(tf.translation).is_some_and(|h| tf.translation.y <= h + CAPSULE_HALF_HEIGHT + 0.05);
    if !a.airborne && !perched && !wiring && !docked && !throwing && tf.translation.y > crate::terrain::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT + 0.05 && !on_rim() && !no_gravity_hold(d, a) {
        a.airborne = true;
        // A plain fall (fallType 0), not the last wire flight's (2026-10-12 bug hunt round 12).
        a.fall_type = 0;
        // The exe's air velocity (physics +0x160) is the last grounded frame's move: the walk /
        // run's horizontal velocity, and one frame of world gravity down (the ground hold, -9.8
        // dt); the first air step adds the fall's. tools/emu-cases `air` edge_run: off the cliff
        // at a run Wolf keeps -5.56 m/s (decaying 0.1 m/s^2), y velocity -0.163 -> -0.555.
        // Before: straight down from rest.
        a.vel_y = -crate::data::WORLD_GRAVITY * dt;
        a.air_base = p.ground_vel;
        // BEH_R_FALL (W_SelfFallStart, then FreeFall) is the script's (hks_wolf); on the move
        // the move state goes on until FALL_START_DROP (the air branch, env 200).
    }
    if !a.airborne && dt > 0.0 {
        // A warp (a reset, a placement) is not a move.
        let moved = (tf.translation - p.ground_pos).with_y(0.0);
        p.ground_vel = if moved.length() < 1.0 { moved / dt } else { Vec3::ZERO };
    }
    p.ground_pos = tf.translation;
    if !a.airborne {
        a.ng_held = false;
    }

    let cam_pitch = view.orbit.map_or(0.26, |o| o.1);
    let cam_yaw = view.orbit.map_or(0.0, |o| o.0);
    // The manipulator's move vector (`Turn::gate`, this frame's) drives the walk / run and the
    // free turn; the HKS's own reads (a step's, jump's or attack's direction, the starts and
    // stops) take the stick it took the frame before, with the frame before's buttons
    // (`Turn::hks`; emu `hksstick`: the exe's HKS reads its own frame's stick, and `decide` is
    // the exe's HKS of the frame before).
    let stick = stick_world(p.turn.hks, cam_yaw);
    let move_stick = stick_world(gated_stick, cam_yaw);
    // Aged, an attack in reach of an aging deathblow row takes the deathblow below.
    // Only where the attack is taken (an aged standby): pressed mid-onset, mid-attack or in an aged
    // fall it skipped aging_step every tick and the plain tick ran the aged clips (round 52).
    let aged_deathblow = a.aged.is_some()
        && p.requests.contains_key(&Action::Attack)
        && accepts(d, a, Action::Attack)
        && enemies.iter().any(|(_, ea, en, etf)| deathblow_check(&combat, tf.translation, a.forward(), true, ea, en, etf.translation, &|_| None).is_some_and(|db| db.in_reach));
    if (a.aged.is_some() || is_aging_state(&a.state)) && !aged_deathblow && aging_step(p, a, &combat, &config, stick, &pad, &mut tf.translation, dt, &mut log) {
        return;
    }
    // Attack auto-homing (CSChrAutoHomingModule): while not locked on, an attack's first 1/6 s
    // (HKS _UpdateAutoAim -> act(156)) picks the nearest enemy inside LockCamParam's melee
    // auto-capture box; FUN_1409efdb0 then gives attacks the lock point or this target.
    if lock.target.is_none() && p.auto_aim && a.t < AUTO_AIM_TIME {
        let dir = if stick != Vec3::ZERO { stick } else { a.forward() };
        a.homing = auto_homing_target(&combat, tf.translation, dir, enemies.iter().map(|(e, ea, en, t)| (e, t.translation, ea.hp > 0.0 && !en.is_dead())));
    }
    let target_pos = lock
        .target
        .or(a.homing)
        .and_then(|e| enemies.get(e).ok())
        .map(|(_, _, _, t)| t.translation);
    let target_dir = target_pos.map(|tp| (tp - tf.translation).with_y(0.0).normalize_or_zero());
    let len = d.length(&a.anim);
    let ended = !a.anim.is_empty() && a.t >= len;

    // Kill follow-up (HKS BEH_R_THROW_KILL -> W_ThrowKill<env(273)>): when the thrown enemy dies
    // (its ThrowDef -> ThrowDefDeath switch, flag 69) Wolf goes on with the throw anim's id + 1 if
    // the set has one (only the a201 set: 510001, 510111, 510201, 511201, 511411, 511511).
    // Live (rec_c1020_d_20261009): a201_510000 for 1.85 s, then 510001 on the frame the General
    // goes 12000 -> 12001.
    // The Grappling Hook Attack's Todome (ThrowKind 180900 "[PC] 崩しワイヤートドメ": derived from
    // 180100, attack button, defAliveCheck 2; ThrowParam <chr>0179, only Gyoubu's 15080179 -
    // a225_511690 / ThrowDef13690, Dist 5): when the landing's deathblow took the last bar (the
    // defender's ThrowDef13610 -> ThrowDefDeath13611 switch on its flag 69, frame 55, after
    // 511610's TAE 304 at 52). gap: the exe's input window for it - 511610 has no buffer flag,
    // so an attack pressed anywhere in the landing counts.
    if a.state == "WireThrowLand" && attack_pressed {
        p.wire_todome = true;
    }
    if let (Some(ee), "WireThrowLand") = (p.throw_target, a.state.as_str()) {
        // Gyoubu has no ThrowDefDeath (a major boss: Event20200 after the reaction), so the
        // dead defender's flag 69 itself.
        let dying = enemies.get(ee).map(|(_, ea, en, _)| {
            ea.state.starts_with("ThrowDefDeath") || (en.is_dead() && !ea.anim.is_empty() && combat.data_of(ea).flag(&ea.anim, ea.step(), 69))
        });
        if dying.is_err() {
            p.throw_target = None;
        } else if dying == Ok(true) {
            p.throw_target = None;
            let todome = enemies.get_mut(ee).ok().filter(|_| p.wire_todome).and_then(|q| Some((combat.throw(combat.foe_of(&q.1).throw_row(179))?, q)));
            if let Some((th, (_, mut ea, mut enemy, _))) = todome.filter(|(th, q)| d.anim(&th.atk_anim).is_some() && q.3.translation.distance(tf.translation) <= th.dist) {
                a.play("WireThrowTodome", &th.atk_anim);
                a.move_vel = Vec3::ZERO;
                enemy.deathblow(&mut ea, &combat, th.def_anim);
                throw.0 = Some(ThrowHold::new(ee, &th, "WireThrowTodome"));
                log.push("todome", Color::srgb(1.0, 0.3, 0.3));
            }
        }
    } else if let Some(ee) = p.throw_target {
        // The kill follow-up is the script's (hks_wolf: BEH_R_THROW_KILL on env 274 with the
        // partner's death, W_ThrowKill<env 273>); the partner is kept for it while the deathblow
        // lasts.
        if a.state != "Deathblow" || enemies.get(ee).is_err() {
            p.throw_target = None;
        }
    }

    // Deathblow start throw (ThrowParam 0000 崩し始動 a201_500000 / 0110 崩し背後始動): Wolf steps
    // in; the start anim's CommonBehavior (judge 600 at TAE frame 11; behind: 640 at 7) runs the main throw. Live
    // (rec_20261008_054259): 0.4 s of start, then the main anims begin together.
    if a.state == "DeathblowStart" {
        if let Some((ee, suffix, behind)) = p.throw_start {
            let judged = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 5 && e.start <= a.t));
            if judged || ended {
                p.throw_start = None;
                let ok = enemies.get_mut(ee).ok().and_then(|q| Some((combat.throw(combat.foe_of(&q.1).throw_row(suffix))?, q)));
                if let Some((th, (_, mut ea, mut enemy, etf))) = ok {
                    if suffix == 201 {
                        // The vault itself is no kill: the enemy plays ThrowDef13900 and stays
                        // open to the deathblow while it lasts.
                        if d.anim(&th.atk_anim).is_some() {
                            a.play("BreakKickJump", &th.atk_anim);
                            a.move_vel = Vec3::ZERO;
                        }
                        let ed = combat.data_of(&ea);
                        if ea.play_state(ed, &format!("ThrowDef{}", th.def_anim)) {
                            enemy.on_posture_break(&mut ea, &combat, false);
                        }
                        throw.0 = Some(ThrowHold::new(ee, &th, "BreakKickJump"));
                        log.push("kick jump over", Color::srgb(0.8, 0.9, 1.0));
                    } else {
                        main_deathblow(a, tf.translation, &mut ea, &mut enemy, etf.translation, ee, &combat, &th, behind, &mut throw, &mut log);
                        p.throw_target = Some(ee);
                    }
                }
            }
        }
        // The main throw could not start (its enemy despawned in a boss swap, no row 0201 for
        // the vault, no clip for Wolf): back to idle. Before, nothing left DeathblowStart and
        // Wolf stood frozen, every input ignored (round 52).
        if a.state == "DeathblowStart" && (p.throw_start.is_none() || ended) {
            p.throw_start = None;
            a.play_state(d, "StandIdle");
        }
        return;
    }

    // Break throws: a deflect (弾き0, suffix 0010: a20x_510100 / ThrowDef12100) or a Mikiri (見切り崩し,
    // 0120: a20x_511100 / ThrowDef13100) that empties the enemy's posture plays a pose pair
    // (combat.rs). While the defender's anim has ChrActionFlag 67 ThrowStart2 (0-45 of
    // ThrowDef12100), attack takes it to the deathblow (弾き1 0011 / 見切り崩し攻撃 0121) and jump
    // to the jumping variant (0012 / 0122) - before Wolf's own cancel flags (frame 30): live
    // c1020 510100 0.61 s -> 510110 -> 510111 (ThrowDef12100 -> 12110 -> 12111); c1010 510100
    // 0.61-0.79 s -> 510110, 0.47-0.59 s -> 510120; 511100 0.55-0.61 s -> 511110.
    if a.state == "ThrowBreak" {
        let (atk_sfx, jump_sfx) = match a.anim.get(5..) {
            Some("510100") => (11, 12),
            Some("511100") => (121, 122),
            _ => (0, 0),
        };
        // The break pose's own partner (the ThrowHold combat.rs set up), not the first broken
        // enemy in its window: with two broken in a group fight the scan could pick the other
        // (round 65). The scan stays for a pose without a hold.
        let open = |(_, ea, en, _): &(Entity, &Actor, &Enemy, &Transform)| en.is_broken() && !ea.anim.is_empty() && combat.data_of(ea).flag(&ea.anim, ea.step(), FLAG_THROW_START2);
        let held = throw.0.as_ref().filter(|h| h.state == "ThrowBreak").map(|h| h.target);
        let target = match held {
            Some(h) => enemies.get(h).ok().filter(|q| open(q)).map(|(e, ..)| e),
            None => enemies.iter().find(|q| open(q)).map(|(e, ..)| e),
        };
        if target.is_none() && p.requests.contains_key(&Action::Attack) {
            info!("deathblow refused: wolf in the break pose {} t {:.2}, no enemy in its ThrowStart2 window (broken: {})", a.anim, a.t, enemies.iter().any(|q| q.2.is_broken()));
        }
        let pick = if target.is_some() && p.requests.remove(&Action::Attack).is_some() {
            Some(atk_sfx)
        } else if target.is_some() && p.requests.remove(&Action::Jump).is_some() {
            Some(jump_sfx)
        } else {
            None
        };
        if let (Some(sfx), Some(ee)) = (pick, target) {
            // On the last deathblow the break pose's attack takes its Todome row, main + 8 as
            // deathblow_check's "崩しトドメ" (Gyoubu's 15080019 "1A_攻撃トドメ": a225_510190 /
            // ThrowDef12190, the last-bar 0011). Round 37: the plain 0011 always played. gap: Gyoubu's
            // ThrowDef12100 has no ThrowStart2 (67), so this path never finds him in the sim; the
            // exe's pick of 0019 is not traced (by the row's name; Gyoubu is the only boss with one).
            let sfx = enemies
                .get(ee)
                .ok()
                .filter(|q| sfx == atk_sfx && q.2.ninsatsu.0 <= 1)
                .and_then(|q| combat.throw(combat.foe_of(q.1).throw_row(sfx + 8)))
                .filter(|t| d.anim(&t.atk_anim).is_some())
                .map_or(sfx, |_| sfx + 8);
            // A second form's (HU) follow-up rows (15400511 / 512, 15090511 / 512, 15400621 / 622;
            // round 89) by their forDef condition; the plain row when none matches.
            let ok = enemies.get_mut(ee).ok().and_then(|q| {
                let row = crate::combat::throw_row_for(&combat, &q.1, sfx).unwrap_or_else(|| combat.foe_of(&q.1).throw_row(sfx));
                Some((combat.throw(row)?, q))
            });
            if let Some((th, (_, mut ea, mut enemy, etf))) = ok {
                if sfx == jump_sfx && d.anim(&th.atk_anim).is_some() {
                    // 弾き1B_ジャンプ / 見切り崩しジャンプ (ThrowKind 20110 弾き1_裏回り): a vault round
                    // behind it, no kill (a201_510120 has no ThrowAttackBehavior; ThrowDef12120 /
                    // 13120 keep the deathblow mark, as 13900): the enemy stays open, as the 0201 kick
                    // jump. It was run as a deathblow and killed a regular enemy outright.
                    a.play("BreakKickJump", &th.atk_anim);
                    a.move_vel = Vec3::ZERO;
                    let ed = combat.data_of(&ea);
                    if ea.play_state(ed, &format!("ThrowDef{}", th.def_anim)) {
                        enemy.on_posture_break(&mut ea, &combat, false);
                    }
                    throw.0 = Some(ThrowHold::new(ee, &th, "BreakKickJump"));
                    log.push("jump round behind", Color::srgb(0.8, 0.9, 1.0));
                    return;
                }
                if d.anim(&th.atk_anim).is_some() {
                    main_deathblow(a, tf.translation, &mut ea, &mut enemy, etf.translation, ee, &combat, &th, false, &mut throw, &mut log);
                    p.throw_target = Some(ee);
                    return;
                }
            }
        }
    }

    // 崩し蹴りジャンプ (ThrowParam 0200 -> 0201, both enemies on the a200 set): jump in front of a
    // broken enemy (Dist 3.0 / 3.1, DiffAng 90-180 = it faces Wolf) vaults over it: a200_501900
    // (CommonBehavior 740 at frame 8 runs the main throw) -> 511900 (no gravity frames 0-27),
    // enemy ThrowDef13900. Live: c1010 0.30 s + 1.07 s, then a free-fall landing behind him; the
    // General 501900 -> 511900 -> attack -> the behind deathblow.
    if p.requests.contains_key(&Action::Jump) && accepts(d, a, Action::Jump) {
        if let Some((ee, epos, anim)) = jump_over(&combat, tf.translation, enemies.iter().map(|(ee, ea, en, etf)| (ee, &*ea, &*en, etf))) {
            p.requests.remove(&Action::Jump);
            a.yaw = yaw_of((epos - tf.translation).with_y(0.0));
            a.move_vel = Vec3::ZERO;
            a.play("DeathblowStart", &anim);
            p.throw_start = Some((ee, 201, false));
            return;
        }
    }

    // Deathblow: attack near a posture-broken enemy. Start rows ("崩し始動" 0000, DiffAng 90-180 =
    // facing the enemy; "崩し背後始動" 0110, DiffAng 0-90 = behind it) carry the reach (Dist) and
    // lead to the main throws ("崩し本体" 0001; "崩し背後本体" 0111, a20x_511200 = 510200's clip,
    // isTurnAtker). The Ochimusha also has 崩し始動（近）0005 (Dist 1.2) -> 崩し本体（近）0006
    // (a200_502500 -> 512500), taken first when that close. Live (rec_c1010_20261009): near 3x
    // (0.30 s + 1.83 s), far 1x (0.30 s + 2.00 s).
    // Attack with a broken enemy around that does not become a deathblow: why, in the log (the
    // user's report on Isshin: the mark up, M1 does nothing).
    if p.requests.contains_key(&Action::Attack) && enemies.iter().any(|(_, _, en, _)| en.is_broken()) {
        let dmy_of = |ee: Entity| move |id: i16| enemy_dummies.get(ee).ok().and_then(|dm| dm.0.get(&id)).and_then(|m| globals.get(*m).ok()).map(|g| g.translation());
        for (ee, ea, en, etf) in enemies.iter().filter(|q| q.2.is_broken()) {
            let db = deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), ea, en, etf.translation, &dmy_of(ee));
            if !db.as_ref().is_some_and(|db| db.in_reach) || !accepts(d, a, Action::Attack) {
                info!(
                    "deathblow refused: wolf {} {} t {:.2} accepts {} | enemy {} {} t {:.2} dist {:.2} (y {:.2}) check {}",
                    a.state, a.anim, a.t, accepts(d, a, Action::Attack), ea.state, ea.anim, ea.t,
                    etf.translation.distance(tf.translation), etf.translation.y - tf.translation.y,
                    db.map_or("none".to_string(), |db| format!("row {} reach {} start {:?}", db.suffix, db.in_reach, db.start.map(|s| s.dist))),
                );
            }
        }
    }
    if p.requests.contains_key(&Action::Attack) && (accepts(d, a, Action::Attack) || todome_input(d, a)) {
        let dmy_of = |ee: Entity| move |id: i16| enemy_dummies.get(ee).ok().and_then(|dm| dm.0.get(&id)).and_then(|m| globals.get(*m).ok()).map(|g| g.translation());
        // Several enemies open at once: deathblow_pick (the exe's chr order: lowest chr number).
        let pick = deathblow_pick(
            enemies.iter().filter_map(|(ee, ea, en, etf)| {
                deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), ea, en, etf.translation, &dmy_of(ee)).map(|db| (ee, etf.translation, db.in_reach, en.is_broken(), chr_number(&combat, ea)))
            }),
            lock.target,
            tf.translation,
            false,
        );
        if let Some((ee, mut ea, mut enemy, etf)) = pick.and_then(|e| enemies.get_mut(e).ok()) {
            if let Some(db) = deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), &ea, &enemy, etf.translation, &dmy_of(ee)) {
                let (behind, mut suffix, mut th, start) = (db.behind, db.suffix, db.main, db.start);
                p.requests.remove(&Action::Attack);
                // The worn ninjutsu's twin of a killing behind deathblow (ninjutsu.rs), paid now.
                let foe = combat.foe_of(&ea);
                let ninjutsu = config.player.ninjutsu;
                let kills = enemy.ninsatsu.0 <= 1 && a.aged.is_none();
                if let Some((s, t)) = crate::ninjutsu::pick(&combat, ninjutsu, p.emblems, config.player.infinite_emblems, |s| foe.throw_row(s), suffix, kills) {
                    (suffix, th) = (s, t);
                    if !config.player.infinite_emblems {
                        p.emblems -= crate::ninjutsu::cost(ninjutsu);
                    }
                    log.push(format!("NINJUTSU ({} emblems)", p.emblems), Color::srgb(0.9, 0.3, 0.3));
                }
                // isTurnAtker: Wolf turns to the enemy at once (live: exactly).
                a.yaw = yaw_of((etf.translation - tf.translation).with_y(0.0));
                a.move_vel = Vec3::ZERO;
                if let Some(st) = start {
                    a.play("DeathblowStart", &st.atk_anim);
                    p.throw_start = Some((ee, suffix, behind));
                    return;
                }
                main_deathblow(a, tf.translation, &mut ea, &mut enemy, etf.translation, ee, &combat, &th, behind, &mut throw, &mut log);
                p.throw_target = Some(ee);
                return;
            }
        }
    }

    // Facing helper for action starts: auto-aim at the lock target, else snap to the stick.
    let aim = |a: &mut Actor| {
        if let Some(dir) = target_dir {
            a.yaw = yaw_of(dir);
        } else if stick != Vec3::ZERO {
            a.yaw = yaw_of(stick);
        }
    };
    // Entered a deflect start the script picked (hks_wolf::take_press): it faces the target or
    // the stick. The sprint deflect turns to the lock target over its first half second instead
    // (sprint_deflect_turn below).
    if entered && (is_guard_start(&a.state) || a.state.starts_with("StandDeflectGuardFromDamage")) && a.state != "SprintToDeflectGuard" && a.t < 0.1 {
        aim(a);
        a.move_vel = Vec3::ZERO;
    }
    // Entered an attack the script picked: the homing target (_StartAutoAim), facing, stop.
    if entered && (is_ground_attack(&a.state) || is_art_press(&a.state)) && a.t < 0.1 {
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        aim(a);
        a.move_vel = Vec3::ZERO;
        // An art's opener: whether it had its emblems (g_enableSpAttaclkJump = env(3035), the
        // hit jump's gate). The air uses (g_airSpecialAttackCount) are the script's.
        if is_art_opener(&a.state) {
            p.art_enable_jump = p.emblems >= art_cost(&combat, &config);
        }
    }
    // Entered the guard's end over the walk / run: the walk or run underneath (the guard walk is
    // the walk speed level).
    if entered && a.state.starts_with("DeflectGuardToStandMove") {
        a.move_index = if a.state.ends_with("Walk") { 0 } else { 1 };
        if p.speed_level < 1.0 && stick != Vec3::ZERO {
            p.speed_level = 1.0;
        }
    }
    // Entered an air art the script picked (BEH_A_AIR_SP_ATTACK): the homing target and the
    // emblem state for the hit jump.
    if entered && a.t < 0.1 && a.state.starts_with("AirSpecialAttack") && !a.state.contains("Loop") {
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        p.art_enable_jump = p.emblems >= art_cost(&combat, &config);
    }
    if entered && a.t < 0.1 && a.state.starts_with("AirStormJump") {
        log.push(format!("storm jump ({})", a.state), Color::srgb(0.7, 0.9, 1.0));
    }
    // Entered a ground jump the script picked (BEH_A_GROUND_JUMP; hks_wolf takes the press):
    // the launch, landing and tilt bookkeeping player.rs's jump block did. Not its own chain
    // (ready -> start at the clip's end).
    if entered && a.t < 0.1 && is_ground_jump_press(&a.state) && !prev_state.contains("JumpReady") {
        let sprint = a.state.starts_with("Sprint");
        let storm = a.state.contains("Storm");
        p.jump_tilt = 0.0;
        if storm {
            log.push(format!("storm jump ({})", a.state), Color::srgb(0.7, 0.9, 1.0));
        } else if stick != Vec3::ZERO && (sprint || lock.target.is_none()) {
            a.yaw = yaw_of(stick);
        } else if stick != Vec3::ZERO {
            let local = Quat::from_rotation_y(-a.yaw) * stick;
            let angle = local.x.atan2(-local.z).to_degrees();
            let (ready, _, _) = directional_jump(angle);
            if ready == a.state {
                p.jump_tilt = jump_tilt(ready, angle);
            }
        }
        a.move_vel = Vec3::ZERO;
    }
    // Entered a prosthetic move the script picked on the press: facing, stop, the tool's
    // category for the next combo check (_isSubAttackCombo).
    if entered && a.t < 0.1 && is_sub_press(&a.state) {
        aim(a);
        a.move_vel = Vec3::ZERO;
        p.sub_cat_before = equipped_tool(&combat, &config, p.tool_slot).map_or(70, |t| t.group);
        if a.state.starts_with("SubAttackFailed") {
            log.push("no spirit emblems", Color::srgb(0.7, 0.7, 0.7));
        }
    }

    // Grappling Hook (wire.rs; docs/kb/grapple.md). A reaction or anything else that takes Wolf
    // out of the wire states ends the move.
    if p.wire.is_some() && !crate::wire::is_wire_state(&a.state) {
        p.wire = None;
    }
    if let Some(w) = p.wire.as_mut() {
        w.turn(a, tf.translation, dt);
        // 905 WireDisableFalling: the air shoots hang in the air.
        if !a.anim.is_empty() && d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 905 && e.dispatched(a.step()))) {
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
        }
        // The shoot and its end into the move start are the graph's (hks_wolf: the script's
        // W_*WireShoot on env 3000, graph_chains for the end).
        if entered && crate::wire::is_shoot(&a.state) {
            log.push("grapple", Color::srgb(0.6, 0.9, 0.6));
        }
    }
    // A grapple press the script did not take (BEH_A_*_WIRE_SHOOT): in its window, or with no
    // point in the action range, the request is dropped (the script's ResetRequest).
    if p.requests.contains_key(&Action::Grapple) {
        let ok = if a.airborne {
            matches!(a.state.as_str(), "FreeFall" | "WireMove") || a.state.ends_with("GroundJumpFall") || (!a.anim.is_empty() && d.flag(&a.anim, a.step(), FLAG_ACCEPT_JUMP))
        } else {
            is_free(&a.state) || accepts(d, a, Action::Grapple)
        };
        let target = wire_aim.as_ref().and_then(|w| w.target).filter(|t| t.in_action_range);
        if ok || target.is_none() {
            p.requests.remove(&Action::Grapple);
        }
    }

    // Wall hug / ledge hang (docking.rs, docs/kb/docking.md): held, nothing else runs.
    // At a Sculptor's Idol (idol.rs): E kneels; while kneeling only its clips run.
    // An item on the ground in reach (items.rs): E takes it, before the idol and the wall.
    let world = crate::encounter::world();
    let picked = interact_pressed && !a.airborne && is_free(&a.state) && (world.take_item)(tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT);
    let interact_pressed = interact_pressed && !picked;
    // The host's idols and NPCs (encounter.rs WorldHooks: this game's Sculptor, then its idols).
    let at_idol = world.interactions.iter().any(|f| f(p, a, d, tf, interact_pressed, ended, &config));
    // Praying or resting at an idol brings the enemies back (the fight's line-up).
    if std::mem::take(&mut p.seat.respawn) {
        if let Some(f) = fight.as_mut() {
            f.0 = true;
        }
    }
    if at_idol {
        return;
    }
    // In deep water (swim.rs): the swim and the dive, nothing else.
    let mibu = crate::items::count(&config, crate::swim::MIBU_BREATHING) > 0;
    crate::swim::air(a, d, ended);
    // In the water (swim.rs): the quick item, if it has a water clip (the gourd, the Homeward
    // Idol); its TAE 65 is spent as on land (the gourd below, the others by `item_use` above).
    if let Some(under) = p.swim.as_ref().map(|s| s.diving).filter(|_| item_pressed) {
        p.requests.remove(&Action::UseItem);
        let goods = crate::items::in_slot(&config, p.quick_slot);
        let dry = if goods == GOURD_GOODS { Some("ItemGourdDrink") } else { crate::items::use_state(crate::items::param("EquipParamGoods", goods)["goodsUseAnim"].as_i64().unwrap_or(255)) };
        let have = if goods == GOURD_GOODS { p.gourd > 0 } else { goods > 0 && crate::items::count(&config, goods) > 0 };
        match dry.and_then(|s| crate::swim::item_state(s, under)) {
            Some(state) if have && !a.state.contains("Item") && a.play_state(d, &state) => {
                if goods != GOURD_GOODS {
                    p.item_use = Some((goods, state));
                }
                return;
            }
            Some(_) => {}
            None => log.push("this item cannot be used in the water", Color::srgb(0.8, 0.8, 0.8)),
        }
    }
    if crate::swim::is_water_drink(&a.state) {
        let consumed = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 65 && e.start > a.prev_t && e.start <= a.t));
        if consumed && p.gourd > 0 {
            p.gourd -= 1;
            let rate = d.sp_effects.get(GOURD_SPEFFECT).map_or(40.0, |s| -s.change_hp_estus_flask_rate);
            let heal = a.hp_max * rate / 100.0 * medicine_rate(&combat, &config);
            a.hp = (a.hp + heal).min(a.hp_max);
            p.sp_bursts.push(GOURD_GOODS);
            log.push(format!("gourd: +{heal:.0} HP ({} left)", p.gourd), Color::srgb(0.5, 1.0, 0.6));
        }
    }
    let swim = crate::swim::Controls { stick, rise_held: pad.grapple_held, sink_held: pad.prosthetic_held, dive_pressed: swim_dive, fast_held: pad.dodge_held, jump_pressed: dock_jump, pitch: cam_pitch, attack_pressed, attack_held: pad.attack_held };
    match crate::swim::tick(p, a, d, tf, swim, ended, mibu, dt, &mut log) {
        crate::swim::Step::Held => return,
        // Out of the water: the swim jump's Start (swim.rs), launched by its row 221 along the
        // facing (fallType 1) - onto the pool's edge.
        crate::swim::Step::Jump => {
            p.requests.remove(&Action::Jump);
            let (h, v, fall) = combat.velocity_change_row(crate::swim::JUMP_VELOCITY_ROW).map_or((9.15, 6.82, 1), |r| (r.h_change, r.v_change, 1));
            a.airborne = true;
            a.fall_type = fall;
            a.vel_y = v;
            a.air_base = a.forward() * h;
            a.play_state(d, crate::swim::JUMP_START);
            log.push("jump out of the water", Color::srgb(0.5, 0.75, 1.0));
            return;
        }
        crate::swim::Step::Free => {}
    }
    if dock_step_tick(p, a, d, tf, ledges.as_deref(), stick, pad.walk, pad.interact_held, interact_pressed, dock_jump, dock_step, ended, &mut log, attack_pressed, &combat, &enemies) {
        return;
    }

    // --- HKS g_behaviorValidateOrder (subset), highest priority first ---

    // Combat art (ACTION_ARM_SPECIAL_ATTACK): attack and guard pressed together, or attack
    // while guard is held. With an art equipped the HKS never reaches DeflectGuardAttack
    // (it needs env(345) == SP_ATK_TYPE_NONE).
    let art = art_anim(d, &combat, &config);
    let art_equipped = art.is_some();
    // A pending guard tap pairs only while it is recent: an old tap left in the buffer (buffer
    // 0 = no timeout) turned the next attack, seconds later, into the art. gap: the game's own
    // pairing window is not traced; 0.1 s (6 frames) taken.
    let guard_recent = p.requests.get(&Action::Guard).is_some_and(|&age| age <= 0.1);
    // Only with an art equipped (HKS 6193: env(345) ~= SP_ATK_TYPE_NONE; round 54: without one
    // the pair became a CombatArt request nothing took, and both the deflect and the slash were
    // lost). gap: 6193 also needs the guard held now (guardCount > 0); a released tap still pairs.
    // Not in the wire flight (round 62): BEH_A_AIR_SP_ATTACK is off in STYLE_TYPE_WIRE_FALL
    // (g_behaviorTable), so there the press stays the wire-fall attack.
    if p.requests.contains_key(&Action::Attack)
        && art_equipped
        && p.wire.is_none()
        && (guard_recent || pad.guard_held)
    {
        p.requests.remove(&Action::Guard);
        p.requests.remove(&Action::Attack);
        p.requests.insert(Action::CombatArt, 0.0);
    }
    // Shadowrush / Shadowfall (109): BEH_R_GROUND_SP_ATTACK_HIT_JUMP (HKS 5750) - the thrust
    // connected (env 2004) while SP_EF_REF_TAE_ENABLE_SP_ATK_HIT_JUMP (225: 100266, a109_316000
    // f44-55) is up and the art had its emblems at the start (g_enableSpAttaclkJump = env(3035)) ->
    // W_GroundSpecialAttackHitJump (a109_316600: no gravity f0-12, TAE 920 row 10000 at f12;
    // judge 215 = BehaviorParam_PC 105010215 wepCost 1 pays the art's 2 emblems).
    if a.attack_hit && sp_ref_active(d, a, SP_REF_SP_ATK_HIT_JUMP) && p.art_enable_jump {
        if let Some(k) = art_kind(&combat, &config).filter(|(c, _)| *c == 109).and_then(|(c, _)| art_key(d, c, "GroundSpecialAttackHitJump")) {
            a.play("GroundSpecialAttackHitJump", &k);
            log.push("hit jump", Color::srgb(0.8, 0.9, 1.0));
            return;
        }
    }

    // Sakura Dance's bounce: the landing ready (a110_316210) ends into the next leap
    // (AirSpecialAttackLandingJumpStart, a110_316260: TAE 920 row 10030 at f0).
    if ended && a.state.starts_with("AirSpecialAttackLandingJumpReady") {
        let nr = if a.state.ends_with("NoResource") { "NoResource" } else { "" };
        let want = format!("AirSpecialAttackLandingJumpStart{nr}");
        if let Some(k) = art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, &want)) {
            a.play(&want, &k);
            return;
        }
    }

    // The quick item (the gourd's drink, repeat and empty shakes, the other goods' use states,
    // crouched or moving) is the script's: hks_wolf::take_item.
    // The drink: ConsumeCurrentGoods (TAE 65) is `consume_goods`, after `advance`.
    if matches!(a.state.trim_start_matches("Crouch"), "ItemGourdDrink" | "ItemGourdDrinkRepeat" | "ItemGourdDrinkMove" | "ItemGourdDrinkRepeatMove") {
        // Flag 90: may walk while drinking (the Move ones walk on the legs: lower_body_move).
        if !is_upper_action(&a.state) && !a.anim.is_empty() && d.flag(&a.anim, a.step(), FLAG_LIMIT_WALK) {
            let walk = d.root_velocity(if a.crouch { "a000_005200" } else { "a000_000200" }).length();
            a.move_vel = stick * walk;
        }
    }

    // HKS _execDamageStep (5062-5078), first in BEH_A_GROUND_STEP / BEH_A_GROUND_JUMP: a step or
    // jump out of a large hit while SP_EF_REF_TAE_ENABLE_HIT_JUMP_CANCEL (502: 100344 on
    // StandDamageLarge_F/B 0.5-2.0 s) is up is W_StandStepFromDamage (_F/_B: a000_206100 / 206101,
    // by the hit's side); out of a knockdown / its get-up while DOWN_JUMP_CANCEL (504: 100346, 53
    // anims) is up, W_StandStepFromDown by the prone side and _set4DirJumpDir (stick angle from
    // the facing, +-45 / 135; no stick = N), a000_2061xx (round 31: a plain jump or step played).
    // _set4DirJumpTilt (act 3025, HKS 5159): the stick angle minus the direction's centre
    // (PRM_4DIR_JUMP_ANGLE_CENTER F 0 / L -90 / R 90 / B 180, c0000_define 248-251), as the
    // positioning jumps' tilt (atlas movement.damage-step). The script's: hks_wolf takes the
    // press, the side and direction by the graph's selectors.

    // The ground jump (BEH_A_GROUND_JUMP: the storm, sprint, swamp, vertical, forward and
    // lock-on directional jumps; atlas movement.jump-weapon-state) is the script's:
    // hks_wolf takes the press, the bookkeeping runs on entering (above). gap: g_forceCrouch
    // (low ceilings) is not answered.

    // The step (_fireGroundStep: GroundStep_F / _B / _L / _R locked on, _F along the stick or _N
    // free) is the script's: hks_wolf::take_press.

    // Round 63: g_behaviorValidateOrder (HKS 6449) - item, jump, step, then the tool switch,
    // the art, its releases, the attack, its release, the tool, the deflect. Before, the art,
    // its hold, the switch and the tool came before item / jump / step (a buffered step lost
    // to an art press; One Mind's hold ate jump / step at its flags 119 / 26, frames 12-33).

    // Prosthetic switch (HKS BEH_A_ADD_SUB_WEAPON_CHANGE: the additive W_AddSubWeaponChange,
    // a000_412090, over whatever Wolf does), then, back in StandIdle, BEH_ADD_R_SUB_WEAPON_EXPAND:
    // the new tool unfolds (SubWeaponExpand, a0<group>_412000). gap: the expand's walk / run /
    // crouch variants are not played.
    // Not while the last switch still plays (validate 6394: IsNodeActive("AddSubWeaponChange_CMSG")
    // == FALSE; its TAE also holds ref 320 100359 for 0-0.3 s). Before, each press restarted it
    // and cycled on (round 49). A refused press is dropped, as the HKS's validate leaves it.
    // In the air too: g_behaviorTable[600] is on in FREE_FALL / WIRE_FALL (styles 7 / 8; round 54:
    // the press was taken and dropped mid-jump).
    let switching = d.anim_key("AddSubWeaponChange").is_some_and(|k| a.add_anim == k);
    // HKS 6393: not while ref 316 SP_EF_REF_TAE_ENABLE_SUB_GUARD_RELEASE_CONNECT is up (the
    // umbrella's guard / release clips: a mid-clip switch resolved its later judges and costs
    // with the new tool) nor before the last change's expand (g_EndSubWeaponChange) (round 68).
    if p.requests.remove(&Action::SwitchTool).is_some() && config.player.prosthetics.len() > 1 && !switching && !sp_ref_active(d, a, 316) && !p.expand_pending && arm_style_matches(d, a, p) {
        p.tool_slot = (p.tool_slot + 1) % config.player.prosthetics.len();
        if let Some(k) = d.anim_key("AddSubWeaponChange") {
            a.add_anim = k.to_string();
            a.add_t = 0.0;
        }
        p.expand_pending = true;
        if let Some(t) = equipped_tool(&combat, &config, p.tool_slot) {
            log.push(combat.weapon_name(t.id), Color::srgb(0.85, 0.85, 0.75));
        }
    }
    if p.expand_pending && a.add_anim.is_empty() && a.state == "StandIdle" {
        p.expand_pending = false;
        // Crouched (HKB_STATE_CROUCH_IDLE, BEH_R_SUB_WEAPON_EXPAND 5745): the crouch expand
        // (round 69: the standing one stood him up mid-crouch).
        let tool = equipped_tool(&combat, &config, p.tool_slot);
        let crouched = a.crouch.then(|| tool.and_then(|t| tool_anim(d, "CrouchSubWeaponExpand1", t.group)).map(|k| ("CrouchSubWeaponExpand1", k))).flatten();
        if let Some((st, k)) = crouched.or_else(|| tool.and_then(|t| tool_anim(d, "SubWeaponExpand", t.group)).map(|k| ("SubWeaponExpand", k))) {
            a.play(st, &k);
            return;
        }
    }

    // The art's opener and its combos (BEH_A_GROUND_SP_ATTACK, _FireSpAttackCombo) are the
    // script's: hks_wolf::art_press_event; aimed on entering (above).

    // The art let go (BEH_A_GROUND_SP_ATTACK_RELEASE / _GUARD_RELEASE: Combo1 -> Combo1Release,
    // the hold's draw or end, the forced releases of base Mortal Draw / Praying Strikes) and the
    // hold's loop are the script's (hks_wolf; the graph's clip-end transitions).

    // The attack button (BEH_A_GROUND_ATTACK: the combo refs, the step / sprint / crouch / deflect
    // follow-ups, the art's combos first) is the script's: hks_wolf::take_press; aimed on
    // entering (above).

    // Attack let go while ref 208 is up (BEH_A_GROUND_ATTACK_RELEASE): the release is the
    // script's (hks_wolf, W_*Release); held on, the attack plays out.

    // Shinobi Prosthetic (prosthetic::hks, the HKS sub-attack branches): the press picks the equipped
    // tool's state, the release window lets go, and the hold / guard / leap states chain at their
    // ends. Bullets and the Spirit Emblem cost come from the anims' TAE (prosthetic.rs).
    {
        use crate::prosthetic::hks;
        let tool = equipped_tool(&combat, &config, p.tool_slot);
        let cat = tool.map_or(70, |t| t.group);
        let resident_ref = tool.and_then(|t| d.sp_effects.get(&t.resident.to_string())).map_or(0, |s| s.behavior_ref_id);
        let timed_refs: Vec<i64> = p.timed.iter().filter_map(|(id, _)| d.sp_effects.get(&id.to_string())).map(|s| s.behavior_ref_id).collect();
        let refs = |r: i64| (resident_ref == r && r != 0) || sp_ref_active(d, a, r) || timed_refs.contains(&r);
        // env(3035, ACTION_ARM_SHINOBI_WEP_ACTION): the exe (env case 0xbdb -> PlayerIns vtable
        // +0x2b0 = FUN_140a26010 -> FUN_14084dd10) compares the tool's EquipParamWeapon
        // resourceItemA / B / C (+0x248..0x24a) with the goods 1000 + 1001 (Spirit Emblems) held;
        // B / C are 0 in every row. Every other ACTION_ARM is always enabled.
        let enable_action = p.emblems >= tool.map_or(1, |t| t.emblems.max(1));
        let style = if a.state.starts_with("Sprint") || sprint_window(d, a) {
            hks::Style::Sprint
        } else if a.crouch {
            hks::Style::Crouch
        } else {
            hks::Style::Stand
        };
        let held = pad.prosthetic_held;
        let in_sub = d.anim_key(&a.state).is_some_and(|k| k.starts_with("a070")) || a.anim.get(..4).is_some_and(|g| g.starts_with("a07"));
        let held_press = in_sub && hks::held_press(cat, held, enable_action, &refs);
        // The press is the script's (hks_wolf::sub_press_event); the held repeat stays here.
        if held_press && hks::flame_gate(cat, &refs) {
            p.requests.remove(&Action::Prosthetic);
            let enable_combo = refs(hks::REF_SUB_ATTACK_COMBO) && p.sub_cat_before == cat;
            p.sub_cat_before = cat;
            let state = hks::press(&hks::Press {
                cat,
                state: &a.state,
                style,
                enable_action,
                enable_combo,
                refs: &refs,
                locked: lock.target.is_some(),
                moving: stick != Vec3::ZERO,
            });
            if state != a.state {
                if state.ends_with("SubAttackFailed") {
                    log.push("no spirit emblems", Color::srgb(0.7, 0.7, 0.7));
                }
                if let Some(k) = sub_anim(d, state, cat) {
                    aim(a);
                    a.move_vel = Vec3::ZERO;
                    a.play(state, &k);
                    return;
                }
            }
        }
        // The attack button out of a tool move (its derives) and the let-go inside ref 302 are
        // the script's (hks_wolf: sub_derive_event, the sub releases).
        // State ends (HKS FireStateEndEvent: the held loops and their ends, the Mist Raven's leap
        // by the stick, the falls) are the graph's clip ends (hks_wolf::graph_chains). The flat
        // leaps (404011-404017) end on the floor; the upward one (V 404010: its root rises 3 m)
        // ends in the air, which the script reads (env 248 -> W_LandAirSubAttackMove, else
        // W_AirSubAttackMoveStartToLoop, HKS 557).
        if a.state == "AirSubAttackMoveStartToLoop" && !a.airborne && tf.translation.y > crate::terrain::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT + 0.05 {
            a.airborne = true;
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
        }
    }

    // BEH_A_DEFLECT_GUARD_START, first branches (c0000_transition.lua:4438-4442), the script's
    // (hks_wolf): ref 228 TAE_ENABLE_ADD_JUST_DEFLECT without 412 -> W_AddHardDeflectGuard, an
    // additive deflect window (a000_299060) over the recoil, which keeps playing; ref 503
    // TAE_ENABLE_HIT_DEFLECT_CANCEL (StandDamageMiddle/Large from frame 12) ->
    // StandDeflectGuardFromDamage by the reaction's DamageDirection.

    // The deflect start (W_StandToDeflectGuard / 1Reverse / SprintToDeflectGuard, HKS 4443-4458),
    // the spam chain (BEH_A_DEFLECT_GUARD_CONTINUE, 2 -> 3 -> 4 on ref 212) and the held-button
    // start (ref 221) are the script's: hks_wolf::take_press; aimed on entering (above).
    // A buffered guard press only counts while guard is still held (live recording 2026-10-08:
    // taps let go before the window opened were dropped out of GroundStep_N, HardDeflectedL,
    // GroundAttackCombo1Release and Combo3; attack presses survive being let go): the window
    // reached with the tap let go drops it (the script's start wants env 1108 held too).
    // HKS 6161: ADD_ACTION_INPUT_GUARD (411, the additive deflect) with the base's
    // ADD_ACTION_INPUT_GUARD_CANCEL (412) starts the guard even let go - live:
    // StandDeflectHardSmall tap at f1.5-3.4 -> StandToDeflectGuard f9.4.
    let add_guard = !a.add_anim.is_empty()
        && d.has_ref(&a.add_anim, a.add_t, REF_ADD_INPUT_GUARD)
        && !a.anim.is_empty()
        && d.has_ref(&a.anim, a.step(), REF_ADD_GUARD_CANCEL);
    if !add_guard && !guard_press_live(p, pad.guard_held, dt) && p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) {
        p.requests.remove(&Action::Guard);
    }

    // Deflect end (BEH_A_DEFLECT_GUARD_END, 6176: every guard-style state with ref 207 up; round
    // 61: the easy recoils and LandAirDeflect* played on and snapped to idle) is the script's
    // (hks_wolf): DeflectGuardToStand, or over the walk / run DeflectGuardToStandMoveWalk / Run
    // (hkx selector on MoveSpeedIndex), an upper-body state (is_upper_action).

    // Sprint (BEH_A_SPRINT / _QUICK_TURN / _STOP) is the script's (hks_wolf): the held step's
    // ref 15 window goes to SprintStartFromStep (graph selector by step direction), the loop,
    // the quick turn by stick side and the stop. Here only the steering stays.
    let moving = stick != Vec3::ZERO;
    // HKS BEH_A_SPRINT (6113-6116, then _fireGroundStep at 2761-2771): dodge held while running
    // (ref 91 SP_EF_REF_TAE_MOVING_RUN on the run loop's clip, 100001; not ref 427
    // DISABLE_PRESS_STEP, speed level > 0) steps, and the held step goes on into the sprint.
    // Holding dodge and then pushing the stick only ran (round 31).
    if a.state == "Locomotion" && pad.dodge_held && moving && p.speed_level > 0.0 && !p.requests.contains_key(&Action::Step) {
        let (key, t) = a.shown_clip(d);
        if d.has_ref(&key, t, SP_REF_TAE_MOVING_RUN) && !d.has_ref(&key, t, SP_REF_TAE_DISABLE_PRESS_STEP) {
            p.requests.insert(Action::Step, 0.0);
        }
    }
    if (a.state.starts_with("SprintStartFromStep") || a.state == "SprintLoop") && pad.dodge_held && moving {
        if lock.target.is_some() {
            a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, true).to_radians() * dt);
        } else {
            free_turn(p, a, d, &config, move_stick, dt);
        }
        return;
    }

    // TAE 920 ChrPhysicsVelocityChange: new velocity = current * scale + change. Each event fires
    // once as the clock crosses it (Spiral Cloud's 316710 has two: f0 and f27); the flag only keeps
    // a frame-0 one from repeating while the clock stands at 0.
    if a.prev_t > 0.0 || !a.vel_change_done {
        if let Some(vc) = d.velocity_change(&a.anim, a.prev_t, a.t).and_then(|id| combat.velocity_change_row(id)) {
            a.vel_change_done = true;
            // A new launch replaces any scaling still running from the anim before.
            p.vel_scale = None;
            // Facing = 0 deg; positive angles taken as to the right.
            let dir = Quat::from_rotation_y(-vc.h_angle.to_radians() + if jump_tilted(&a.state) { p.jump_tilt } else { 0.0 }) * a.forward();
            a.air_base = a.move_vel * vc.h_scale + dir * vc.h_change;
            a.vel_y = a.vel_y * vc.v_scale + vc.v_change;
            a.fall_type = vc.fall_type;
            a.airborne = true;
            a.launch_left = LAUNCH_TIME;
            a.move_vel = a.air_base;
            p.leap_homing = None;
            // Row 1100's homing (ChrPhysicsHomingParam 300000000): the wire's guaranteed arrival.
            if vc.homing_id == crate::wire::WIRE_HOMING {
                if let Some(w) = p.wire.as_mut() {
                    w.launch(d, a, tf.translation);
                }
            }
            // Any other row with a target type homes on that character (the jump arts' 10010 ->
            // 20000: Senpou Leaping Kicks / High Monk land in front of the lock target).
            else if vc.homing_id != 0 && vc.homing_target_type == 1 {
                let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
                p.leap_homing = lock.target.zip(crate::wire::homing_row(vc.homing_id)).and_then(|(target, row)| {
                    let (_, _, _, t) = enemies.get(target).ok()?;
                    // Already nearer the target than its homing point (the lock-on forward jump,
                    // 10000, off a body-to-body stand): no homing. The live head kicks off such a
                    // jump (rec_20261016_* boss fights, 19, sim test
                    // a_forward_jump_over_a_sweep_stops_at_the_body_and_the_kick_lands) hold at the
                    // contact distance; homing on the point 1 m out pulled Wolf back off the head.
                    // gap: the exe's test for this is not traced.
                    let point = leap_target(t, row.offset);
                    if (point - tf.translation).with_y(0.0).dot((t.translation - tf.translation).with_y(0.0)) <= 0.0 {
                        return None;
                    }
                    LeapHoming::launch(a, tf.translation, leap_target(t, row.offset), ft.gravity(), target, row)
                });
            }
        }
    }
    // TAE 904 WireSlideToStartPoint (wire.rs): the 922 blend with row 1101 and the exe's fixed
    // curves (horizontal Linear 1, vertical EaseIn 10) over the event.
    if let Some(e) = (!a.anim.is_empty()).then(|| d.anim(&a.anim)).flatten().and_then(|an| an.events.iter().find(|e| e.kind == 904 && e.start >= a.prev_t && e.start < a.t.max(a.prev_t + 1e-6))) {
        if let Some(vc) = combat.velocity_change_row(1101) {
            a.airborne = true;
            a.fall_type = vc.fall_type;
            p.vel_scale = Some(VelScale {
                anim: a.anim.clone(),
                add: (a.forward() * vc.h_change, vc.v_change),
                v0: a.air_base,
                vy0: a.vel_y,
                delta: (Vec3::ZERO, 0.0),
                scale: (vc.h_scale, vc.v_scale),
                curves: [(0, 1), (1, 10)],
                elapsed: 0.0,
                duration: e.end - e.start,
            });
        }
    }

    // TAE 922 ChrPhysicsVelosityScale (`VelScale`): over the event the air velocity blends from
    // its value at the start to the scaled one plus the row's change, along the event's ease
    // curves. A row with an upward change launches from the ground: the storm jumps (a000_2014x1,
    // no 920) rise only by theirs (980: +25 m/s, fallType 1). gap: that 922 applies the change
    // is read from those anims (nothing else lifts them), not traced in the exe.
    let launch = !a.airborne && d.velocity_scale(&a.anim, a.prev_t, a.t).and_then(|e| combat.velocity_change_row(e.arg_i64("ChrPhysicsVelocityParam ID")?)).is_some_and(|vc| vc.v_change > 0.0);
    if launch {
        a.airborne = true;
        a.vel_y = 0.0;
        a.air_base = Vec3::ZERO;
    }
    if a.airborne {
        if let Some((e, vc)) = d.velocity_scale(&a.anim, a.prev_t, a.t).and_then(|e| Some((e, combat.velocity_change_row(e.arg_i64("ChrPhysicsVelocityParam ID")?)?))) {
            let arg = |k: &str| e.arg_i64(k).unwrap_or(0) as u8;
            let dir = Quat::from_rotation_y(-vc.h_angle.to_radians() + if jump_tilted(&a.state) { p.jump_tilt } else { 0.0 }) * a.forward();
            if vc.v_change != 0.0 || vc.h_change != 0.0 {
                a.fall_type = vc.fall_type;
            }
            p.vel_scale = Some(VelScale {
                anim: a.anim.clone(),
                add: (dir * vc.h_change, vc.v_change),
                v0: a.air_base,
                vy0: a.vel_y,
                delta: (Vec3::ZERO, 0.0),
                scale: (vc.h_scale, vc.v_scale),
                curves: [(arg("unk1"), arg("unk2")), (arg("unk3"), arg("unk4"))],
                elapsed: 0.0,
                duration: e.end - e.start,
            });
        }
    }
    if let Some(vs) = p.vel_scale.as_mut().filter(|vs| a.airborne && vs.anim == a.anim) {
        // gap: the exe's clock for the 922 blend (FUN_140bb1980) is not traced; the actor clock
        // is taken because the clip and the event's duration run on it (round 81).
        vs.elapsed += a.clock(dt);
        let x = if vs.duration > 0.0 { (vs.elapsed / vs.duration).clamp(0.0, 1.0) } else { 1.0 };
        // Undo last tick's blend to get the free velocity, then blend toward its scaled value.
        let (free, free_y) = (a.air_base + vs.delta.0, a.vel_y + vs.delta.1);
        let h = vs.v0.lerp(free * vs.scale.0 + vs.add.0, ease(vs.curves[0], x));
        let y = vs.vy0 + (free_y * vs.scale.1 + vs.add.1 - vs.vy0) * ease(vs.curves[1], x);
        vs.delta = (free - h, free_y - y);
        a.air_base = h;
        a.move_vel = h;
        a.vel_y = y;
        if x >= 1.0 {
            p.vel_scale = None;
        }
    } else {
        p.vel_scale = None;
    }

    // Jumps: Ready -> Start (TAE 920 launches) -> Fall loop -> land. The graph takes the ground,
    // sprint and storm jumps on at their clip ends (hks_wolf::graph_chains). Side jumps have no
    // behaviour-graph clip (CMSG anim 0): their raw anims a000_20111x.
    if ended && a.state == "LeftsideGroundJumpReady" {
        a.play("LeftsideGroundJumpStart", "a000_201112");
        return;
    }
    if ended && a.state == "RightsideGroundJumpReady" {
        a.play("RightsideGroundJumpStart", "a000_201113");
        return;
    }
    // The fall top (physics +0x854, FUN_140bc3470): the height of the last grounded frame (and of
    // a SetNoGravity hold, below), kept through the flight; not the flight's highest point. A jump
    // landing where it left falls 0 m (tools/emu-cases `air`: +0x854 stays at the take-off over
    // the whole jump). The HKS fall height env(224) and the fall damage read top - y.
    if !a.airborne {
        p.air_top = tf.translation.y;
    }
    if !a.airborne {
        // HitMtrlParam 20-24 / 35-38 / 45-47 are water, swamp and underwater floors (not a safe spot).
        let water = crate::terrain::floor_material(tf.translation).is_some_and(|m| matches!(m, 20..=24 | 35..=38 | 45..=47));
        if !water && a.state != "Grabbed" && a.hp > 0.0 && safe_spot(tf.translation, crate::terrain::terrain().as_deref()) {
            p.safe_ring.push_back(tf.translation);
            if p.safe_ring.len() > SAFE_RING {
                p.safe_ring.pop_front();
            }
        }
    } else if p.air_top - tf.translation.y >= FALL_VOID_DROP && crate::terrain::floor_material(tf.translation).is_none() && crate::terrain::terrain().is_some() {
        // Falling with no ground below: HKS DAMAGE_TYPE_FALL_DEAD_RETURN -> W_FallDeadReturnStart
        // (a000_199120), then back at the safe spot. gap: the exe's kill height / return volume is
        // not traced; the default fallDeathDist (40 m) stands in. Its HP cost: `fall_return_hp`.
        if a.play_state(d, "FallDeadReturnStart") {
            a.hp = fall_return_hp(a.hp, a.hp_max).unwrap_or(0.0);
            a.airborne = false;
            a.vel_y = 0.0;
            log.push("fell into the void", Color::srgb(1.0, 0.4, 0.4));
            return;
        }
    }
    // BEH_R_FALL on the move: W_SelfFallStart once he has fallen FALL_START_DROP (env 224 is the
    // height fallen, Smithbox HKS list "GetFallHeight"); a shallower drop lands still moving.
    // gap: the crouch (W_SelfCrouchFreeFallStart) and sub-guard falls have no clip of their own here.
    if a.airborne {
        // Air actions open on the jump/air anims' own cancel flags. The fall loops (FreeFall,
        // *GroundJumpFall: STATE_TYPE_STANDBY in g_paramHkbState) carry none and take every input,
        // as the standby states on the ground (`is_free`).
        let air_ok = |a: &Actor, f: i64| a.state == "FreeFall" || a.state.ends_with("GroundJumpFall") || (!a.anim.is_empty() && d.flag(&a.anim, a.step(), f));
        if let Some(th) = p.wire.and_then(|w| w.enemy).filter(|_| p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK)).and_then(|e| wire_throw(&combat, a, tf.translation, e, &enemies)) {
            // The Grappling Hook Attack (崩しワイヤー, ThrowKind 180000 "[PC] Breaking wire 0":
            // acceptableButton 0 = attack, doesCheckNormalFallOrbit 1, unlock type 5 = always):
            // ThrowParam <chr>0170 in the wire fall at the hooked enemy - Wolf a<offset>_511600,
            // the enemy ThrowDef13600; the landing takes 180100 (0171: 511610 / 13610).
            p.requests.remove(&Action::Attack);
            a.play("WireThrow", &th.atk_anim);
            if let Some(Ok((_, mut ea, mut enemy, _))) = p.wire.and_then(|w| w.enemy).map(|e| enemies.get_mut(e)) {
                enemy.react(&mut ea, &combat, &format!("ThrowDef{}", th.def_anim));
            }
            log.push("grappling hook attack", Color::srgb(1.0, 0.5, 0.3));
        } else if p.wire.is_some() && p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK) {
            // BEH_A_AIR_ATTACK in the wire-fall style (HKS 2877): W_WireFallAttackCombo1 with
            // ACTION_UNLOCK_TYPE_WIRE_MOVE_ATTACK (14), else the additive attack (gap: not played).
            p.requests.remove(&Action::Attack);
            // HKS 2872-2876: a lightning charge throws it back first (round 62: the wire-fall
            // attack was taken and the charge kept).
            if let Some((id, _)) = a.electro {
                a.play_state(d, if id == 9495 { "AirElectroReceiveAttack" } else { "AirWeakElectroReceiveAttack" });
            } else if crate::combat::action_unlocked(&combat, &config, 14) {
                a.play_state(d, "WireFallAttackCombo1");
            }
        } else if p.wire.is_some() && p.requests.contains_key(&Action::Guard) && (air_ok(a, FLAG_ACCEPT_GUARD) || a.state == "WireMove") {
            // BEH_A_AIR_DEFLECT_START in the wire-fall style (HKS 2965): W_WireMoveDeflectGuardStart.
            p.requests.remove(&Action::Guard);
            a.play_state(d, "WireMoveDeflectGuardStart");
        // Round 63: g_behaviorValidateOrder (HKS 6449) - the kick (and storm jump) first, then
        // the art, the air attacks, the tool, the deflect last; before, guard, art, tool, kick,
        // attack (a pending guard beat a kick press when both opened at AirComboAttack's f12).
        } else if p.requests.contains_key(&Action::Attack)
            && (air_ok(a, FLAG_ACCEPT_ATTACK) || (a.state.starts_with("AirKickEnemyJumpStart") && air_ok(a, FLAG_BUFFER_ALL)))
            && start_plunge(p, a, &combat, tf.translation, &mut enemies, &mut log)
        {
            // Out of a head-kick jump the kick-down throw is checked from its buffer flag 87
            // (frame 9), not the air attack's 115 (frame 21): live 0.33 s into 213115.
            p.requests.remove(&Action::Attack);
        }
        // The kick and the storm jump, the art, the air attacks (the lightning thrown back, the
        // Mist Raven's and Shadowfall's derives), the tool and the deflect are the script's
        // (hks_wolf, BEH_A_AIR_*).
        // BEH_R_ENEMY_JUMP: env(2004) - the kick's own attack (TAE 307 PCBehavior 901 ->
        // AtkParam_Pc 901, frames 9-21) connected - while AirKick's SpEffect 100334
        // (behaviorRefId 204, frames 9-21) is up -> AirKickEnemyJumpStart (no gravity frames
        // 0-6, then TAE 920 row 2101 relaunches). Also how sweeps are jumped (AirKick has
        // sweep i-frames).
        if a.state == "AirKick" && a.attack_hit && sp_ref_active(d, a, SP_REF_KICK_ENEMY_JUMP) {
            // HKS _set4DirJumpDir on the stick (vs facing, + right): within +-45 deg forward (or no
            // stick: vertical), -135..-45 left, 45..135 right, else back; locked on, forward and
            // vertical become FORWARD_LOCKON. Live: locked kicks play _F_Lock (a000_213115),
            // unlocked neutral ones _N (213114).
            let local = Quat::from_rotation_y(-a.yaw) * stick;
            let angle = local.x.atan2(-local.z).to_degrees();
            let dir = if stick == Vec3::ZERO || angle.abs() < 45.0 {
                match (lock.target.is_some(), stick == Vec3::ZERO) {
                    (true, _) => "F_Lock",
                    (false, true) => "N",
                    (false, false) => "F",
                }
            } else if (-135.0..-45.0).contains(&angle) {
                "L"
            } else if (45.0..135.0).contains(&angle) {
                "R"
            } else {
                "B"
            };
            // _set4DirJumpTilt (HKS 5159-5175), before the lock-on rewrite: F / F_Lock tilt by the
            // angle, L / R by angle -+ 90, B by angle -+ 180; the vertical (N) none.
            let tilt = match dir {
                "N" => 0.0,
                "F" | "F_Lock" if stick == Vec3::ZERO => 0.0,
                "F" | "F_Lock" => angle,
                "L" => angle + 90.0,
                "R" => angle - 90.0,
                _ if angle > 0.0 => angle - 180.0,
                _ => angle + 180.0,
            };
            p.jump_tilt = (-tilt).to_radians();
            a.play_state(d, &format!("AirKickEnemyJumpStart_{dir}"));
            a.air_base = Vec3::ZERO;
            a.vel_y = 0.0;
            log.push("kick jump", Color::srgb(0.8, 0.9, 1.0));
        }
        // BEH_A_AIR_SP_ATTACK_RELEASE / _GUARD_RELEASE (HKS 2941-2955) -> W_AirSpacialAttackHoldEnd
        // (a104_316240): attack let go while 222 is up, or guard while 230 is (c0000_transition
        // .lua 6048-6058). The air hold anims have 230 only, so only guard ends it there.
        let air_release = (!pad.attack_held && d.has_ref(&a.anim, a.step(), REF_SP_ATK_RELEASE)) || (!pad.guard_held && d.has_ref(&a.anim, a.step(), REF_SP_ATK_HOLD_RELEASE));
        if matches!(a.state.as_str(), "AirSpecialAttackHoldStart" | "AirSpecialAttackHoldLoop") && air_release {
            if let Some(k) = art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, "AirSpacialAttackHoldEnd")) {
                a.play("AirSpacialAttackHoldEnd", &k);
            }
        }
        // BEH_A_AIR_DEFLECT_END (c0000_transition.lua 6066: the guard let go, the start too while
        // ref 207 is up) is the script's (hks_wolf, W_AirDeflectGuardEnd).

        let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
        // ChrActionFlag 27 SetNoGravity (FUN_140b51b70 case 0x1b sets physics +0x92c on every
        // update its window overlaps, the one its end falls in too; FUN_140bcc420 turns it into
        // +0x92a). The physics step FUN_140bccd50 then skips the air step and holds: the air
        // velocity +0x160 is zeroed and the fall top +0x854 is the held height. Except while the
        // fall module's launch timer runs (LAUNCH_TIME from a TAE 920, FUN_140bb14d0): a launch in the
        // flag's last update (the head kick's [0, 0.2] flag and its 920 at 0.2, the wire's 2020x0)
        // flies on (tools/emu-cases `air` ng_launch / ng_rise, test
        // no_gravity_waits_for_the_launch_timer). gap: a flag new this update keeps +0x160 in
        // the exe (zeroed the update after); unused, as the body does not move either way.
        let flag = !a.anim.is_empty() && d.flag(&a.anim, a.step(), FLAG_NO_GRAVITY);
        let before = a.air_base;
        let (held, h, vy, g) = air_frame(flag, a.launch_left, a.ng_held, a.air_base, a.vel_y, stick, ft, dt);
        (a.air_base, a.vel_y, a.grav_y) = (h, vy, g);
        if held {
            p.air_top = tf.translation.y;
        }
        a.ng_held = held;
        a.launch_left = (a.launch_left - dt).max(0.0);
        // Plunge deathblow: ChrPhysicsHomingParam 200000300 (TAE 920 row 8000 on the plunge anim):
        // fallCorrectionGuaranteeArrival 1 - the horizontal velocity is re-aimed every frame so
        // the fall ends on the target - and fallCorrectionTurn 1 (face it); no chase acceleration.
        // Target: the defender's targetBaseDmyPolyId 233 (on its model root) + targetOffset
        // (0, 0, -0.3) in that dummy's space; the enemy's position without a model (tests).
        if a.state == "PlungeDeathblow" {
            let dmy = p.plunge.and_then(|(e, _)| enemy_dummies.get(e).ok()).and_then(|dm| dm.0.get(&233)).and_then(|&de| globals.get(de).ok());
            let tp = match dmy {
                Some(g) => Some(g.transform_point(Vec3::new(0.0, 0.0, -0.3))),
                None => p.plunge.and_then(|(e, _)| enemies.get(e).ok()).map(|(_, _, _, t)| t.translation),
            };
            if let Some(tp) = tp {
                let g = ft.gravity();
                let h = tf.translation.y - CAPSULE_HALF_HEIGHT - crate::terrain::ground_y(tf.translation);
                // Time to the floor: h + vy t + g t^2 / 2 = 0 (g < 0).
                let disc = a.vel_y * a.vel_y - 2.0 * g * h;
                let t_land = if g < 0.0 && disc >= 0.0 { (-a.vel_y - disc.sqrt()) / g } else { 0.0 };
                let to = (tp - tf.translation).with_y(0.0);
                if t_land > 1e-3 {
                    a.air_base = to / t_land;
                }
                if to.length_squared() > 1e-4 {
                    a.yaw = yaw_of(to);
                }
            }
        }
        if let Some(h) = p.leap_homing {
            if let Ok((_, _, _, t)) = enemies.get(h.target) {
                h.chase(a, tf.translation, leap_target(t, h.row.offset), ft.gravity(), dt);
            }
        }
        if let Some(w) = p.wire.as_mut() {
            // The guaranteed arrival puts him on the spot this step (wire.rs `steer`): the fall
            // step below (actor.rs `fall`) still moves him by this step's vertical speed, so he is
            // put that much over it and ends the step on it, as the exe's position does.
            if let Some(at) = w.steer(a, tf.translation, dt) {
                tf.translation = at - Vec3::Y * (a.vel_y - 0.5 * a.grav_y * dt) * dt;
            }
        }
        a.move_vel = a.air_base;
        // The wire's flight moves by the mean of the step's old and new horizontal velocity, as
        // the exe's physics step does (FUN_140bccd50's half step; tools/emu-cases `wire`: 0.2516 m
        // on the launch step from 15.19 -> 15.02 m/s, the arc's analytic distance at the flight
        // time). gap: the other air flights still move by the new velocity (not compared here).
        if p.wire.is_some_and(|w| w.flight.is_some()) {
            a.move_vel = (before + a.air_base) * 0.5;
        }
        let mut floor = crate::terrain::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT;
        // A ridden point is a floor once the arc is down to it (`Player::perch`).
        let ride_spot = p.wire.filter(|w| w.ride && w.flight.is_none()).map(|w| w.land);
        if let Some(spot) = ride_spot.filter(|s| (tf.translation - *s).with_y(0.0).length() < PERCH_RADIUS) {
            floor = floor.max(spot.y + CAPSULE_HALF_HEIGHT);
        }
        // The perch he jumped from is a floor too while he is over it and not already below it.
        if let Some(spot) = p.perch.filter(|s| (tf.translation - *s).with_y(0.0).length() < PERCH_RADIUS && tf.translation.y >= s.y + CAPSULE_HALF_HEIGHT - 0.3) {
            floor = floor.max(spot.y + CAPSULE_HALF_HEIGHT);
        }
        // A TAE 922 launch still easing in (the storm jumps) is lifting off, not landing.
        let lifting = p.vel_scale.as_ref().is_some_and(|vs| vs.add.1 > 0.0 && vs.anim == a.anim);
        if tf.translation.y <= floor && a.vel_y < 0.0 && !lifting {
            tf.translation.y = floor;
            a.airborne = false;
            p.leap_homing = None;
            // The fall type lasts one flight: Wire::launch's fallType 2 (gravity -9.8 - 9.8,
            // stick 6 m/s) is not the next walk-off's (0, kb/movement.md -23.52).
            a.fall_type = 0;
            a.vel_y = 0.0;
            a.move_vel = Vec3::ZERO;
            a.air_base = Vec3::ZERO;
            // Down before BEH_R_FALL fired (under FALL_START_DROP): still the move state, and
            // BEH_R_LAND is only for the air styles (g_behaviorTable column 7), so no landing.
            if moving_locomotion(d, a) {
                return;
            }
            // BEH_R_LAND_WIRE (HKS 2076, the wire-fall style): out of the wire air attack
            // W_LandWireFallAttackCombo1 at the same time, out of the wire guard
            // W_LandAirDeflectGuard; with the stick held (MoveSpeedLevel > 0)
            // W_StandMoveStartFromFreeFall = straight into moving (as BEH_R_LAND here); else
            // W_LandWireMove. gap: the crouch landings (W_CrouchMoveStartFromFreeFall /
            // W_LandCrouchFreeFall on TAE ref ENABLE_ADD_ACTION_INPUT_CROUCH) are not taken (nor in
            // the plain landing).
            if let Some(w) = p.wire.take() {
                if ride_spot.is_some_and(|spot| (tf.translation - spot).with_y(0.0).length() < PERCH_RADIUS) {
                    p.perch = Some(w.land);
                }
                // Breaking wire 1 (ThrowKind 180100, throwCheckSituation 1: on the landing, no
                // button): ThrowParam <chr>0171, Wolf 511610 / the enemy ThrowDef13610, held as a
                // pair (ThrowHold). Its hit is a deathblow: 511610 TAE 304 judge 680 = AtkParam_Pc
                // 5000680 "右手刀_忍殺本体_ワイヤー" (throwFlag 2, atkPhysCorrection 60000); 13610 ends in
                // SpEffect 300610 "HP reset" (changeHpRate -100) and 13611 is its death - one
                // life bar off, or the kill on the last (Enemy::deathblow; Bosses' reading).
                if a.state == "WireThrow" {
                    let pair = w.enemy.and_then(|e| Some((e, combat.throw(combat.foe_of(&enemies.get(e).ok()?.1).throw_row(171))?)));
                    if let Some((target, th)) = pair.filter(|(_, th)| d.anim(&th.atk_anim).is_some()) {
                        if let Ok((_, mut ea, mut enemy, _)) = enemies.get_mut(target) {
                            a.play("WireThrowLand", &th.atk_anim);
                            p.wire_todome = false;
                            enemy.deathblow(&mut ea, &combat, th.def_anim);
                            throw.0 = Some(ThrowHold::new(target, &th, "WireThrowLand"));
                            p.throw_target = Some(target);
                            return;
                        }
                    }
                }
                let landed = match a.state.as_str() {
                    "WireFallAttackCombo1" => a.continue_state(d, "LandWireFallAttackCombo1"),
                    "WireMoveDeflectGuardStart" | "WireMoveDeflectGuardLoop" => a.play_state(d, "LandAirDeflectGuard"),
                    _ if stick != Vec3::ZERO => {
                        p.speed_level = if pad.walk { 1.0 } else { 2.0 };
                        a.procedural("Locomotion");
                        true
                    }
                    _ => a.play_state(d, "LandWireMove"),
                };
                if landed {
                    return;
                }
            }
            // Plunge lands: ThrowParam 崩し落下1 (suffix 151): Wolf a20x_511410 (510310's clip),
            // the enemy ThrowDefDeath13411; 蹴り崩し1 (161): a20x_511510, ThrowDef13510.
            if a.state == "PlungeDeathblow" {
                let plunge = p.plunge.take();
                let kind_foe = |t: Entity| enemies.get(t).ok().map(|q| combat.foe_of(q.1));
                if let Some((target, th)) = plunge.and_then(|(t, s)| Some((t, combat.throw(kind_foe(t)?.throw_row(s))?))) {
                    // Only onto an enemy still open to it (held broken by its ThrowDef pose).
                    if let Ok((_, mut ea, mut enemy, etf)) = enemies.get_mut(target).map_err(|_| ()).and_then(|q| if q.2.is_broken() { Ok(q) } else { Err(()) }) {
                        if d.anim(&th.atk_anim).is_some() {
                            a.play("Deathblow", &th.atk_anim);
                        }
                        ea.yaw = yaw_of((tf.translation - etf.translation).with_y(0.0));
                        enemy.deathblow(&mut ea, &combat, th.def_anim);
                        throw.0 = Some(ThrowHold::new(target, &th, "Deathblow"));
                        p.throw_target = Some(target);
                        log.push("DEATHBLOW (plunge)", Color::srgb(1.0, 0.2, 0.2));
                        return;
                    }
                }
            }
            // Landing from a height the ground's HitMtrlParam fallDeathDist (map::fall_death_dist)
            // kills: HKS DAMAGE_TYPE_LAND_DEAD_RETURN -> W_LandDeadReturnStart (a000_200909), then
            // back at the safe spot. The HP rule (FUN_140b97a30 -> FUN_140b97b30, emulated in
            // extracted/emu/air_falldmg.json, test emu_air): `fall_landing` (from 20 m, from 5 m
            // while stateInfo 341, aging, is worn; nothing while stateInfo 47 is). Short of the
            // kill no reaction of its own: the landing below plays as usual.
            let landing_drop = p.air_top - tf.translation.y;
            let floor = crate::terrain::floor_material(tf.translation);
            let aged = wolf_wears(a, p, d, STATE_INFO_AGING);
            let no_damage = wolf_wears(a, p, d, STATE_INFO_NO_FALL_DAMAGE);
            match fall_landing(a.hp, a.hp_max, landing_drop, floor, aged, no_damage) {
                FallLanding::None => {}
                FallLanding::Hurt(hp) => {
                    log.push(format!("fall: -{:.0} HP", a.hp - hp), Color::srgb(1.0, 0.4, 0.4));
                    a.hp = hp;
                }
                FallLanding::Return(hp) => {
                    a.hp = hp;
                    if a.play_state(d, "LandDeadReturnStart") {
                        log.push("fall death - back to safe ground", Color::srgb(1.0, 0.4, 0.4));
                        return;
                    }
                }
                FallLanding::Death => {
                    // HP at or under the return's cost: the ordinary death where he landed.
                    a.hp = 0.0;
                    log.push("fall death", Color::srgb(1.0, 0.4, 0.4));
                    return;
                }
            }
            // Charged (HKS 1870-1875): landing in the charge states sets it off (the script lands
            // in W_LandAirDamageElectroCharge(Weak)). gap: the charge SpEffect is dropped on
            // landing (its own end is not traced); how the exe holds the shock off until the
            // landing is not traced.
            if let Some((id, _)) = a.electro.take().filter(|_| a.state.starts_with("AirDamageElectroCharge")) {
                lightning_shock(&combat, a, id);
                log.push("SHOCKED - landed charged", Color::srgb(0.6, 0.8, 1.0));
            }
            // The landing itself is the script's (BEH_R_LAND, hks_wolf::land_script on this frame:
            // the stiffness by the fall height, the Land versions while ref 201 is up, the jump's
            // own lands, the stick's move start). BEH_R_LAND 2027 / 2044: the dodge pressed or held
            // steps out of it (_fireGroundStep), which the script takes as the press.
            if pad.dodge_held && !a.sheathed {
                p.requests.entry(Action::Step).or_insert(0.0);
            }
            // The jump arts' lands the script does not make (BEH_R_LAND held off by ref 103
            // SP_EF_REF_TAE_NO_LAND_ANIM, Sakura Dance's leap without emblems f0-36: gap, env 3008
            // not traced): the leap while ref 201 is up into LandGroundSpecialAttackJumpStart from
            // the same time, the fall loop into LandGroundSpecialAttackJumpFallLoop (HKS 2016-2025).
            if a.state.starts_with("GroundSpecialAttackJump") {
                let no_res = if a.state.ends_with("NoResource") { "NoResource" } else { "" };
                let ref_201 = !a.anim.is_empty() && d.has_ref(&a.anim, a.step(), REF_ORIGINAL_LAND_ACTION);
                let land = if a.state.starts_with("GroundSpecialAttackJumpStart") { ref_201.then(|| format!("LandGroundSpecialAttackJumpStart{no_res}")) } else { Some(format!("LandGroundSpecialAttackJumpFallLoop{no_res}")) };
                if let Some((land, k)) = land.and_then(|l| art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, &l)).map(|k| (l, k))) {
                    let t = if land.contains("Start") { a.t } else { 0.0 };
                    a.play(&land, &k);
                    a.start_at(t);
                    return;
                }
            }
            // A blow-away the graph is not in (the hit did not reach the script as a blow: gap)
            // lands in its Land anim (StandDamage*Blow* / Upper*).
            let falling_reaction = |st: &str| {
                st.contains("BlowStart") || st.contains("BlowFallLoop") || st.contains("UpperStart") || st.contains("UpperFallLoop")
                    || (st.starts_with("AirDamage") && (st.contains("Start") || st.contains("FallLoop")))
            };
            let blow_land = throw_blow(&a.state)
                .map(|(_, land)| land.to_string())
                .or_else(|| falling_reaction(&a.state).then(|| a.state.replace("FallLoop", "Land").replace("Start", "Land")));
            if blow_land.is_some_and(|land| a.play_state(d, &land)) {
                return;
            }
            // Without the script (SHINOBI_HKS=0) the plain landing.
            if stick != Vec3::ZERO {
                p.speed_level = if pad.walk { 1.0 } else { 2.0 };
                a.procedural("Locomotion");
            } else {
                a.play_state(d, "LandFreeFall");
            }
            return;
        }
        if ended && a.state.starts_with("GroundSpecialAttackJump") {
            // The jump art's fall loop (316730) loops until the landing; the graph takes the
            // ready into the leap and the leap into the loop (hks_wolf::graph_chains).
            a.t %= len.max(1e-3);
        } else if ended && crate::wire::is_wire_state(&a.state) {
            // The wire moves fall on in WireMove (a000_202020, loops) until BEH_R_LAND_WIRE; the
            // wire guard holds its loop while guard is held, then W_WireMoveDeflectGuardEnd
            // (HKS 2978).
            match a.state.as_str() {
                "WireMove" => a.t %= len.max(1e-3),
                "WireMoveDeflectGuardStart" | "WireMoveDeflectGuardLoop" if pad.guard_held => {
                    if a.state == "WireMoveDeflectGuardLoop" {
                        a.t %= len.max(1e-3);
                    } else {
                        a.play_state(d, "WireMoveDeflectGuardLoop");
                    }
                }
                "WireMoveDeflectGuardStart" | "WireMoveDeflectGuardLoop" => {
                    a.play_state(d, "WireMoveDeflectGuardEnd");
                }
                _ => {
                    if !a.play_state(d, "WireMove") {
                        a.t = len;
                    }
                }
            }
        } else if ended {
            // The air clip ends are the graph's (hks_wolf: its end transitions into the fall
            // loops, the air art's loop, the guard's loop or end, the blows' fall loops; a loop
            // on its own clip wraps). Left here: the walk-off's move loop over its first
            // FALL_START_DROP, the exe's grab blow-away, else the last frame holds till the
            // landing (SprintJumpStart has no fall CMSG; a side jump's start holds).
            if moving_locomotion(d, a) {
                a.t %= len.max(1e-3);
            } else if throw_blow(&a.state).is_some() {
                throw_blow_step(a, d);
            } else {
                a.t = len;
            }
        }
        return;
    }

    // A grab's blow-away on the ground (its start ended without a lift, or its loop touched down).
    if !a.airborne && throw_blow(&a.state).is_some() {
        throw_blow_step(a, d);
        return;
    }
    // The clip ends are the graph's (hks_wolf: the guard's idle, the move out of an upper
    // action, the knockdowns' get-ups by Selector_ProneDirection); the exe's own states (throws,
    // deathblows, events, ...) end in the idle.
    if ended {
        a.play("StandIdle", "");
    }

    // Locked-on idle turn in place (HKS BEH_A_GROUND_QUICK_TURN / _GroundQuickTurn): standing
    // still with the target more than 60 deg off the facing -> StandQuickTurn{Right,Left}{90,180}
    // (180 beyond 120 deg); the clip's root motion does the turning.
    // The lock target only: target_dir also carries the attack's auto-homing (a.homing, cleared
    // after decide), which turned an unlocked Wolf after a homed swing (round 53; as the step's
    // lock_dir, round 31).
    if a.state == "StandIdle" && stick == Vec3::ZERO {
        if let Some(td) = lock.target.and(target_dir) {
            let ang = a.forward().angle_between(td).to_degrees();
            if ang > 60.0 {
                let side = if td.dot(a.forward().cross(Vec3::Y)) > 0.0 { "Right" } else { "Left" };
                let size = if ang > 120.0 { "180" } else { "90" };
                let style = if a.crouch { "Crouch" } else { "Stand" };
                if a.play_state(d, &format!("{style}QuickTurn{side}{size}")) {
                    return;
                }
            }
        }
    }

    // Movement: free states, or the TAE move window (as the script sees it: accept_move).
    let move_ok = is_free(&a.state) || (!a.anim.is_empty() && accept_move(d, a));
    // The in-place quick turns (Stand / Crouch / DeflectGuard QuickTurn*, a000_000010-13 and
    // kin) have no root motion and no TAE 224: the turn controller turns Wolf to the target at
    // its default rate (FUN_1407daa20: 720 deg/s when no TAE / module rate is set). Without it
    // the clip played, Wolf never turned, and the >60 deg check replayed it forever (Wolf
    // stepping in place with no input). gap: the exe's turn request for these states.
    if a.state.contains("QuickTurn") && !a.state.starts_with("Sprint") && !a.anim.is_empty() && d.root_at(&a.anim, a.t).is_none() {
        if let Some(td) = lock.target.and(target_dir) {
            let rate = d.turn_speed(&a.anim, a.step(), true).unwrap_or(config.movement.turn_speed);
            a.yaw = turn_toward(a.yaw, yaw_of(td), rate.to_radians() * dt);
        }
    }
    // SprintToDeflectGuard turns to the lock target at the TAE rate (180 / 360 deg/s, then the
    // default 720) except in the Disable Turning flag 7 windows (live, 7 deflects; see the guard
    // start above).
    if a.state == "SprintToDeflectGuard" && !a.anim.is_empty() && !d.flag(&a.anim, a.step(), FLAG_DISABLE_TURN) {
        if let Some(td) = lock.target.and(target_dir) {
            let rate = turn_rate(d, a, &config, true);
            a.yaw = turn_toward(a.yaw, yaw_of(td), rate.to_radians() * dt);
        }
    }
    if is_guard_idle(&a.state) {
        // Guard walk: DeflectGuardMoveF/B/L/R (root motion 2.0 m / 40 frames). Wolf keeps his
        // facing (guard idle TAE SetTurnSpeed 0; live, rec_20261008_051450: the facing holds while
        // he walks any way) - locked on he faces the target. The stick's angle to the facing picks
        // the clip as HKS _MoveDirectionUpdate does: |a| < 55 F, > 125 B, else L (a < 0) / R.
        a.move_vel = Vec3::ZERO;
        if let Some(td) = target_dir {
            let rate = turn_rate(d, a, &config, true);
            let rate = if rate > 0.0 { rate } else { config.movement.turn_speed };
            a.yaw = turn_toward(a.yaw, yaw_of(td), rate.to_radians() * dt);
        } else if moving {
            // Free (not locked on) guard walk turns to the stick and walks forward, as the free
            // walk does; the strafes are the lock-on's (the user, twice: "Wolf always faces the
            // front of the camera when he guards"; before, it faced the camera's direction).
            // Standing, the facing holds (guard idle's TAE SetTurnSpeed 0).
            a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, false).to_radians() * dt);
        }
        let local = Quat::from_rotation_y(-a.yaw) * stick;
        let angle = local.x.atan2(-local.z).to_degrees();
        let (want, clip_angle) = if !moving {
            ("DeflectGuardIdle", angle)
        } else if angle.abs() < 55.0 {
            ("DeflectGuardMoveF", 0.0)
        } else if angle.abs() > 125.0 {
            ("DeflectGuardMoveB", 180.0)
        } else if angle < 0.0 {
            ("DeflectGuardMoveL", -90.0)
        } else {
            ("DeflectGuardMoveR", 90.0)
        };
        // Move along the stick (live: 50 deg off forward while the F clip plays).
        let off = (angle - clip_angle + 180.0).rem_euclid(360.0) - 180.0;
        a.root_yaw = (-off).to_radians();
        if moving {
            // The legs turn with it (WalkTwist), so they step along the stick.
            a.twist_target = off.to_radians();
        }
        if a.state != want || ended {
            a.play_state(d, want);
            // The four directions are layers of the one DeflectGuardMove state (c0000.hkx
            // DeflectGuard_SM id 30, an hkbLayerGenerator), not states: a direction change is
            // no transition and keeps the full step (no blend-in).
            a.blend_from = None;
            a.blend_len = 0.0;
        }
        return;
    }
    if is_upper_action(&a.state) {
        // StandMoveOverwrite: the legs run the walk/run (StandMoveLower_SM carries the motion).
        lower_body_move(p, a, d, &config, stick, move_stick, target_dir, pad.walk, dt);
    } else if move_ok && (moving || a.state == "Locomotion" || is_move_quick_turn(&a.state)) {
        locomotion(p, a, d, &config, stick, move_stick, target_dir, pad.walk, dt);
    } else if let Some(td) = lock.target.and(target_dir).filter(|_| move_ok && a.state == "StandIdle") {
        // Locked on and standing: the lock-on manipulator (FUN_1407f1710) asks every frame for
        // the turn to the target (+0x24 = its yaw from Wolf minus his, wrapped) and BehaviorTurn
        // applies it the next frame, whole when the clip has no TAE 224 rate, the drawn facing
        // following at 1.1x (emulated: a target 90 deg off snaps the physics yaw in one frame,
        // the drawn yaw turns 0.2304 rad a frame; sim test `lock_manipulator_matches_the_exe`).
        // Before, a locked Wolf standing still within 60 deg of the target never turned to it
        // (past 60 deg the quick turns above take over, with their own turn). StandIdle only:
        // the guard idle and the quick turns turn on their own. gap: the exe aims at the lock
        // point (lock module +0x38), here the target's root.
        locked_turn(p, a, d, &config, td, dt);
    } else if !move_ok {
        a.move_vel = Vec3::ZERO;
        p.speed_level = 0.0;
        // Turning during actions follows TAE SetTurnSpeed unless turning is disabled.
        if !a.anim.is_empty() && !d.flag(&a.anim, a.step(), FLAG_DISABLE_TURN) {
            if let Some(speed) = d.turn_speed(&a.anim, a.step(), lock.target.is_some()) {
                let want = target_dir.or((stick != Vec3::ZERO).then_some(stick));
                if let Some(w) = want {
                    a.yaw = turn_toward(a.yaw, yaw_of(w), speed.to_radians() * dt);
                }
            }
        }
    }
}

/// The hug / hang deathblows (ThrowParam "[PC] wall-mounted" / "hanging", per enemy: suffix
/// 0060 -> 0061 hugging with the enemy on Wolf's left (Wolf a20x_500600 -> 510600, the enemy
/// ThrowDef12600), 0070 -> 0071 on his right (500700 -> 510700, 12700), hanging 0080 -> 0081
/// with the enemy facing away (500800 -> 510800, 12800) and 0130 -> 0131 facing Wolf (501000
/// -> 511000, 13000)). The start row's reach (Dist), height window and facings decide, as the
/// other deathblows' (throw_height_ok, throw_facing_ok); the enemy must not have found Wolf (or be
/// posture-broken). Returns the enemy, the start and main suffixes and whether it faces away.
/// gap: the exe's own pick between rows; the nearest that fits is taken.
#[allow(clippy::type_complexity)]
fn dock_deathblow(combat: &Combat, dock: &crate::docking::Dock, wolf: Vec3, wolf_fwd: Vec3, enemies: &Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>) -> Option<(Entity, i64, i64, bool)> {
    let wolf_right = wolf_fwd.cross(Vec3::Y);
    enemies
        .iter()
        .filter(|(_, ea, en, _)| ea.hp > 0.0 && !en.is_dead() && (en.is_unaware() || en.is_broken()))
        .filter_map(|(ee, ea, _, etf)| {
            let foe = combat.foe_of(ea);
            let to = etf.translation - wolf;
            let behind = ea.forward().dot(to.with_y(0.0).normalize_or_zero()) > 0.0;
            let pairs: Vec<(i64, i64)> = if dock.hug() {
                if to.dot(wolf_right) < 0.0 { vec![(60, 61)] } else { vec![(70, 71)] }
            } else {
                vec![(80, 81), (130, 131)]
            };
            pairs.into_iter().find_map(|(start, main)| {
                let id = foe.throw_row(start);
                let th = combat.throw(id)?;
                let reach = to.with_y(0.0).length() <= th.dist;
                let ok = reach && throw_height_ok(combat, id, wolf, etf.translation) && throw_facing_ok(combat, id, wolf, wolf_fwd, etf.translation, ea.forward());
                ok.then_some((ee, start, main, behind, to.length()))
            })
        })
        .min_by(|x, y| x.4.total_cmp(&y.4))
        .map(|(ee, s, m, b, _)| (ee, s, m, b))
}

/// Wall hug and ledge hang (docking.rs): starts them on the button, runs the docked states and
/// their ends. True while it owns Wolf this tick (decide returns).
#[allow(clippy::too_many_arguments)]
fn dock_step_tick(
    p: &mut Player,
    a: &mut Actor,
    d: &CharData,
    tf: &Transform,
    ledges: Option<&crate::traversal::LedgeEdges>,
    stick: Vec3,
    walk: bool,
    interact_held: bool,
    interact: bool,
    jump: bool,
    step: bool,
    ended: bool,
    log: &mut CombatLog,
    attack: bool,
    combat: &Combat,
    enemies: &Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
) -> bool {
    use crate::docking as dk;
    let edges: &[crate::traversal::LedgeEdge] = ledges.map(|l| l.0.as_slice()).unwrap_or(&[]);
    let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    // Wall jumps (BEH_A_WALL_JUMP): the count comes back on the ground.
    if !a.airborne && p.dock.is_none() && !a.state.starts_with("WallJump") && !dk::is_jump_ready(&a.state) {
        p.wall_jumps = 0;
        p.last_wall = None;
    }
    // A ready pose (in the air, or out of a hug / hang) ends into its take-off; the take-off into
    // the jump's fall loop (its TAE 920 launched it).
    if let Some(contact) = p.wall_contact {
        if (a.state.starts_with("WallJumpReady_") || dk::is_jump_ready(&a.state)) && ended {
            let ty = dk::jump_type(contact, stick, a.yaw);
            p.dock = None;
            a.docked = false;
            p.wall_contact = None;
            // The take-off's TAE 920 launches along the facing (ChrPhysicsVelocityChangeParam 1001:
            // 6.4 m/s at angle 0, 9.69 up): a horizontal one turns Wolf to the stick first (HKS
            // _SetJumpDirection), or he jumped into the wall.
            if ty != 'N' && stick.length_squared() > 0.01 {
                a.yaw = yaw_of(stick);
            }
            if a.play_state(d, &dk::wall_jump_start(contact, ty)) {
                log.push(format!("wall jump {contact}_{ty}"), Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        }
    }
    if a.state.starts_with("WallJumpStart_") && ended {
        let vertical = a.state.ends_with("_N");
        a.play_state(d, if vertical { "VerticalGroundJumpFall" } else { "ForwardGroundJumpFall" });
        return true;
    }
    if a.state.starts_with("WallJumpReady_") || dk::is_jump_ready(&a.state) {
        return true;
    }
    // Jump in the air with a wall at the chest, once per flight.
    if jump && wall_jump_open(d, a, p, feet) {
        {
            let wall_dir = dk::wall_contact_dir(feet + Vec3::Y * 1.2, a.yaw);
            if let Some(angle) = dk::wall_contact(feet + Vec3::Y * 1.2, a.yaw) {
                p.last_wall = wall_dir;
                let contact = dk::contact_type(angle);
                if a.play_state(d, &format!("WallJumpReady_{contact}")) {
                    p.wall_jumps += 1;
                    p.wall_contact = Some(contact);
                    a.vel_y = a.vel_y.max(0.0);
                    log.push("wall contact", Color::srgb(0.6, 0.8, 0.9));
                    return true;
                }
            }
        }
    }
    // The drop's push-off runs into the fall (HangToFreeFall, 0.33 s).
    if a.state.starts_with("Hang") && a.state.ends_with("ToFreeFall") {
        if ended {
            a.play_state(d, "FreeFall");
        }
        return !ended;
    }
    if let Some(dock) = p.dock.clone() {
        let st = a.state.clone();
        let climbing = st.starts_with("HangToStand") || st.starts_with("HangFootHoldToStand");
        if climbing {
            // Off the edge once the clip's motion is done; the clip plays on (its stand-up was
            // cut to a bare idle at 0.77 of its 1.4 s before) and ends or is cancelled as any
            // standing clip.
            if dk::start_done(d, a, ended) {
                p.dock = None;
                a.docked = false;
                if ended {
                    a.play("StandIdle", "");
                }
                return false;
            }
            return true;
        }
        // A hang deathblow keeps Wolf at the edge (the enemy is pulled down past him), then
        // back to hanging.
        if !dock.hug() && (st == "Deathblow" || st == "DeathblowStart") {
            if st == "Deathblow" && ended {
                dk::play(a, d, if dock.edge.variation == 1 { "HangFootHoldIdle" } else { "HangIdle" });
            }
            return true;
        }
        if !dk::is_docked_state(&st) {
            // Something else (a hit, a death) took Wolf off the edge.
            p.dock = None;
            return false;
        }
        if dk::is_start(&st) {
            if dk::start_done(d, a, ended) {
                let next = if dock.hug() { None } else { dk::hang_start_next(&st) };
                match next {
                    Some(n) => {
                        if let Some(dd) = p.dock.as_mut() {
                            dd.from = (feet, a.yaw);
                        }
                        dk::play(a, d, &n);
                    }
                    None => {
                        if let Some(n) = dk::held_state(&dock, a, Vec3::ZERO, walk, true) {
                            dk::play(a, d, &n);
                        }
                    }
                }
            }
            return true;
        }
        // The hug / hang deathblows (dock_deathblow): the start throw, then decide's
        // DeathblowStart runs the main one at the start anim's judge. The wall ones only from the
        // peek: _UpdateNextThrowInfo sets ThrowGrabType COVER_LOOK and the WALL_L / R_NINSATSU
        // SpEffects (109304 / 109305) only in STYLE_TYPE_COVER_LOOK; a plain hug grabs as standing
        // (atlas movement.docking-deathblows). gap: the stand deathblows from a plain hug.
        if attack && !dk::is_start(&st) && (!dock.hug() || dk::is_peek(&st)) {
            let fwd = Quat::from_rotation_y(a.yaw) * Vec3::NEG_Z;
            // Hanging, the reach and height are taken from the hands on the lip (the rows' 1.35 m
            // height window could never reach an enemy on the top from the hanging body 1.6 m
            // under it). gap: the exe measures from judgeRangeBasePosDmyId1 (not traced).
            let from = if dock.hug() { tf.translation } else { dock.point(dock.s) + Vec3::Y * CAPSULE_HALF_HEIGHT };
            if let Some((ee, start, main, behind)) = dock_deathblow(combat, &dock, from, fwd, enemies) {
                let foe_start = enemies.get(ee).ok().and_then(|q| combat.throw(combat.foe_of(&q.1).throw_row(start)));
                if let Some(th) = foe_start.filter(|th| d.anim(&th.atk_anim).is_some()) {
                    a.play("DeathblowStart", &th.atk_anim);
                    a.move_vel = Vec3::ZERO;
                    p.throw_start = Some((ee, main, behind));
                    if dock.hug() {
                        p.dock = None;
                        a.docked = false;
                    }
                    log.push(if dock.hug() { "wall deathblow" } else { "ledge deathblow" }, Color::srgb(1.0, 0.2, 0.2));
                    return true;
                }
            }
        }
        if dock.hug() {
            if jump && a.play_state(d, "CoverToWallJumpReady") {
                p.dock = None;
                a.docked = false;
                p.wall_contact = Some('B');
                p.wall_jumps += 1;
                return true;
            }
            if interact || step {
                p.dock = None;
                a.play_state(d, if dk::is_peek(&a.state) { "CoverEndFromCoverLook" } else { "CoverEnd" });
                log.push("wall hug end", Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        } else {
            let foot = dock.edge.variation == 1;
            // Jump: the kick off the wall (BEH_A_HANG_END_CONNECT comes before BEH_A_HANG_CLIMB in
            // g_behaviorValidateOrder, so the jump never climbs; HKS 3171: HangJumpReady, held at
            // the edge until it ends, then WallJumpStart_F_<type> by the stick, and
            // g_wallJumpCount = WALL_JUMP_COUNT_MAX). Atlas movement.hang-climb-drop.
            if jump && a.play_state(d, if foot { "HangFootHoldJumpReady" } else { "HangJumpReady" }) {
                p.wall_contact = Some('F');
                p.wall_jumps = dk::wall_jump_limit();
                return true;
            }
            // The hang button (ACTION_ARM_WALL_HANG) climbs. gap: env 3050 (room to stand up).
            if interact {
                if let Some(dd) = p.dock.as_mut() {
                    dd.climb_from = None;
                }
                a.play_state(d, if foot { "HangFootHoldToStand" } else { "HangToStand" });
                log.push("climb up", Color::srgb(0.6, 0.8, 0.9));
                dk::debug_dock(&format!("{} (climb, s {:.2} of {:.2})", a.state, dock.s, dock.len));
                return true;
            }
            if step {
                p.dock = None;
                a.docked = false;
                a.airborne = true;
                a.fall_type = 0;
                a.vel_y = 0.0;
                a.air_base = Vec3::ZERO;
                a.play_state(d, if foot { "HangFootHoldToFreeFall" } else { "HangToFreeFall" });
                log.push("drop", Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        }
        // Hanging at an end with the stick on that way: the next edge carries on (the same face),
        // or the corner move takes Wolf round onto it.
        if !dock.hug() {
            let along = stick.dot(dock.right());
            if along.abs() > 0.3 {
                let right = along > 0.0;
                let toward_b = right == (dock.right().dot(dock.along) > 0.0);
                let at_end = if toward_b { dock.s >= dock.len - dk::END_MARGIN - 0.01 } else { dock.s <= dk::END_MARGIN + 0.01 };
                if at_end {
                    let next = dk::next_hang_edge(edges, &dock, toward_b);
                    dk::debug_dock(&format!("end of a {:.2} m edge: next {:?}", dock.len, next.as_ref().map(|(nd, t)| (t, nd.len, nd.side_from))));
                    if let Some((mut nd, turn)) = next {
                        nd.from = (feet, a.yaw);
                        if turn == dk::Turn::Straight {
                            p.dock = Some(nd);
                        } else {
                            let st = dk::corner_state(turn, right, dock.edge.variation == 1);
                            if a.play_state(d, &st) {
                                p.dock = Some(nd);
                                log.push("hang corner", Color::srgb(0.6, 0.8, 0.9));
                                return true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(n) = dk::held_state(&dock, a, stick, walk, ended) {
            dk::play(a, d, &n);
        }
        return true;
    }
    // Ground starts on the button, from standby: the wall hug (BEH_A_COVER_START), else the
    // hang over the lip Wolf stands at (BEH_A_GROUND_HANG_START).
    if interact && !a.airborne && is_free(&a.state) && p.wire.is_none() {
        if let Some(mut dock) = dk::find_hug(edges, feet, a.yaw) {
            // Stick held: the moving starts by the stick's angle; else the clip whose own turn and
            // path fit (hug_start_best).
            let st = if stick.length_squared() > 0.01 { dk::hug_start_state(&dock, a.yaw, stick) } else { dk::hug_start_best(d, &dock, (feet, a.yaw)) };
            dock.from = (feet, a.yaw);
            if a.play_state(d, st) {
                a.crouch = false;
                p.dock = Some(dock);
                log.push("wall hug", Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        } else if let Some(mut dock) = dk::find_hang_top(edges, feet, a.yaw) {
            let st = dk::hang_start_best(d, &dock, (feet, a.yaw));
            dock.from = (feet, a.yaw);
            if dk::play(a, d, &st) {
                dk::debug_dock(&format!("{st} (ledge hang, {:.2} m edge, side from {})", dock.len, dock.side_from));
                a.crouch = false;
                p.dock = Some(dock);
                log.push("ledge hang", Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        }
    }
    // The air catch (BEH_A_AIR_HANG_START): falling past a lip with the button held (env 1108).
    // gap: the TAE grab window (SP_EF_REF_TAE_ENABLE_AIR_HANG_GRAB) that catches without it.
    // The air catch: the button held (env 1108), or inside a jump's TAE grab window
    // (SpEffect ref 5 SP_EF_REF_TAE_ENABLE_AIR_HANG_GRAB) without it.
    let grab_window = !a.anim.is_empty() && d.has_ref(&a.anim, a.step(), dk::REF_ENABLE_AIR_HANG_GRAB);
    if (interact_held || interact || grab_window) && a.airborne && a.vel_y <= 0.0 && p.wire.is_none() && p.plunge.is_none() {
        if let Some((mut dock, long)) = dk::find_hang_air(edges, feet, a.yaw) {
            let st = dk::air_hang_state(&dock, a.yaw, long);
            dock.from = (feet, a.yaw);
            if dk::play(a, d, &st) {
                p.dock = Some(dock);
                a.airborne = false;
                a.vel_y = 0.0;
                log.push("ledge catch", Color::srgb(0.6, 0.8, 0.9));
                return true;
            }
        }
    }
    false
}

/// Walk/run with speeds taken from the walk (a000_0001xx) and run (a000_0004xx)
/// clips' root motion, blended by the HKS speed level.
#[allow(clippy::too_many_arguments)]
fn locomotion(
    p: &mut Player,
    a: &mut Actor,
    d: &CharData,
    config: &GameConfig,
    stick: Vec3,
    turn: Vec3,
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    // The 180 turns, the game's own clips (c0000.hkx *_F_CMSG_Anime; the user: "turning around
    // animations when idle and walking"):
    // - running: W_StandMoveQuickTurn{Left,Right}180 (StandRunQuickTurn*180, a000_000442 / 443);
    // - from a standstill: BEH_A_QUICK_TURN_MOVE_START -> W_StandQuickTurnMoveStart*180, the run
    //   start (StandQuickTurnRunStart*180, a000_000432 / 433) or with walk held the walk start
    //   (StandQuickTurnWalkStart*180, a000_000132 / 133);
    // - walking: the walk's quick turn (StandWalkQuickTurn*180, a000_000142 / 143).
    // Their roots carry no turn and run on along the new heading, so the heading takes the stick
    // at once. The run clips' hips spin from about 100 deg off it to straight by frame 16 (0.53 s);
    // the walk clips only lean 50 deg into the turn, so the model swings round over the clip's
    // blend from the old facing (anim.rs Skeleton::yaw_off). Turned, he moves on in the loop; let
    // go mid-turn, the stop.
    if let Some((walk, _)) = quick_turn_kind(&a.state) {
        // Locked on, the turn is the target's (hks_wolf: the script's in-move 180 with the stick
        // at rest), so letting go does not stop it.
        if stick == Vec3::ZERO && target_dir.is_none() && a.play_state(d, if walk { "StandWalkStop_F" } else { "StandRunStopF" }) {
            a.move_vel = Vec3::ZERO;
            p.speed_level = 0.0;
            return;
        }
        if a.t < if walk { WALK_QUICK_TURN_TURNED } else { QUICK_TURN_TURNED } {
            a.move_vel = Vec3::ZERO;
            return;
        }
        // The graph plays the turn's clip out before the loop (the turn-and-go is a move start,
        // W_StandQuickTurnMoveStart*180; the in-move W_StandMoveQuickTurn*180 too), so
        // Locomotion shows the rest of it.
        let start = Some((a.anim.clone(), a.t));
        a.procedural("Locomotion");
        a.move_start = 0.0;
        if let Some((clip, t)) = start {
            a.move_start = d.length(&clip);
            a.move_start_clip = Some(clip);
            a.start_at(t);
        }
        p.speed_level = if walk { p.speed_level.max(1.0) } else { p.speed_level.max(2.0) };
        move_velocity(p, a, d, config, stick, turn, target_dir, dt);
        return;
    }
    let behind = stick != Vec3::ZERO && a.forward().angle_between(stick).to_degrees() > QUICK_TURN_ANGLE;
    // Sheathed, the a010 copy where the CMSG has one (offsetType 11's idle category).
    if target_dir.is_none() && !a.crouch && behind {
        let standing = a.state == "StandIdle" || (a.state == "Locomotion" && p.speed_level <= 0.3);
        // On the move the 180 turn opens only in a stop's first 9 frames (BEH_A_STAND_MOVE_QUICK_TURN:
        // TAE ref 14 ENABLE_GROUND_QUICK_TURN, on StandWalkStop_F / StandRunStopF alone): let go,
        // then the stick back. Thrown straight back, the run turns at the exe's 720 deg/s.
        let (shown, st) = a.shown_clip(d);
        let stop_turn = d.has_ref(&shown, st, REF_ENABLE_GROUND_QUICK_TURN);
        let kind = if standing {
            Some(if walk_held { "StandQuickTurnWalkStart" } else { "StandQuickTurnRunStart" })
        } else if stop_turn && a.state.starts_with("StandWalkStop") {
            Some("StandWalkQuickTurn")
        } else if stop_turn && a.state.starts_with("StandRunStop") {
            Some("StandRunQuickTurn")
        } else {
            None
        };
        if let Some(kind) = kind {
            // BEH_A_QUICK_TURN_MOVE_START: Left only where TwistLowerRootAngle (minus the turn,
            // hks_wolf move_vars) is above 0, so a full reverse turns right.
            // In the move, BEH_A_STAND_MOVE_QUICK_TURN: Right where TurnAngle > 0.
            let turn = stick.dot(a.forward().cross(Vec3::Y)).atan2(stick.dot(a.forward()));
            let right = if standing { -turn <= 0.0 } else { turn > 0.0 };
            let side = if right { "Right" } else { "Left" };
            if let Some(mut key) = d.anim_key(&format!("{kind}{side}180_F_CMSG_Anime")).map(str::to_string) {
                let sheathed = format!("a010{}", &key[4..]);
                if a.sheathed && key.starts_with("a000") && d.anim(&sheathed).is_some() {
                    key = sheathed;
                }
                a.yaw = yaw_of(stick);
                a.play(&format!("{kind}{side}180"), &key);
                a.move_vel = Vec3::ZERO;
                return;
            }
        }
    }
    // Stick released: the stop anims carry their own slide (root motion), in the move direction
    // (HKS BEH_A_GROUND_MOVE_STOP: W_StandRunStop / W_StandWalkStop by MoveDirection).
    if stick == Vec3::ZERO && a.state == "Locomotion" && p.speed_level > 0.3 {
        // The run stop where the clip on screen is a run (TAE ref 91 MOVING_RUN, as the script's
        // BEH_A_GROUND_MOVE_STOP reads it), not by the speed level, which lags behind the clip.
        let run = {
            let (key, t) = a.shown_clip(d);
            d.has_ref(&key, t, SP_REF_TAE_MOVING_RUN)
        };
        if play_move_stop(d, a, p, run) {
            return;
        }
    }
    if p.speed_level <= 0.0 && stick == Vec3::ZERO {
        a.move_vel = Vec3::ZERO;
        if a.state == "Locomotion" {
            a.play("StandIdle", "");
        }
        return;
    }
    if stick != Vec3::ZERO {
        // HKS _SpeedUpdate: MoveSpeedIndex from the stick level (walk button = walk).
        a.move_index = if walk_held { 0 } else { 1 };
    }
    // A move from a standstill or out of an action plays the move start first (W_StandMoveStart).
    let fresh = a.state != "Locomotion" || !a.anim.is_empty();
    a.procedural("Locomotion");
    if fresh {
        a.move_start = d.length(&a.move_clip(true));
    }
    move_velocity(p, a, d, config, stick, turn, target_dir, dt);
}

/// Upper-body states (HKS STATE_TYPE_UPPER_ACTION, c0000.hkx StandMoveOverwrite): the action's
/// clip plays above the pelvis while the legs keep walking / running. The speed level converges
/// as in locomotion; letting go of the stick slows the legs to a stop under the action.
#[allow(clippy::too_many_arguments)]
fn lower_body_move(
    p: &mut Player,
    a: &mut Actor,
    d: &CharData,
    config: &GameConfig,
    stick: Vec3,
    turn: Vec3,
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    if stick != Vec3::ZERO {
        a.move_index = if walk_held { 0 } else { 1 };
    }
    move_velocity(p, a, d, config, stick, turn, target_dir, dt);
}

/// The 180 turns: the stick more than this (deg) from the facing (BEH_A_QUICK_TURN_MOVE_START /
/// BEH_A_STAND_MOVE_QUICK_TURN: |TwistLowerRootAngle| / |TurnAngle| > 135).
const QUICK_TURN_ANGLE: f32 = 135.0;
/// SP_EF_REF_TAE_ENABLE_GROUND_QUICK_TURN (c0000_define 594): the stops' first 9 frames.
const REF_ENABLE_GROUND_QUICK_TURN: i64 = 14;
/// When the quick turn's pose faces the run (the hips straight, frame 16 at 30 fps): the clip hands
/// over to the run loop. gap: the hkb StandMoveQuickTurn*_to_StandMoveLoop condition is not read.
const QUICK_TURN_TURNED: f32 = 0.53;

pub(crate) fn is_move_quick_turn(state: &str) -> bool {
    quick_turn_kind(state).is_some()
}

/// A 180 quick turn's kind: (a walk one, its pose spins round itself - the run ones).
pub(crate) fn quick_turn_kind(state: &str) -> Option<(bool, bool)> {
    if state.starts_with("StandRunQuickTurn") || state.starts_with("StandQuickTurnRunStart") {
        Some((false, true))
    } else if state.starts_with("StandWalkQuickTurn") || state.starts_with("StandQuickTurnWalkStart") {
        Some((true, false))
    } else {
        None
    }
}

/// When a walk quick turn hands over to the walk loop (s): its lean is out by about frame 15.
const WALK_QUICK_TURN_TURNED: f32 = 0.5;

/// HKS GetMoveSpeed: ConvergeValue(level, speed, inc 3, dec 3), inc 6 toward level 2.
fn converge_speed(p: &mut Player, stick: Vec3, walk_held: bool, dt: f32) {
    let target_level = if stick == Vec3::ZERO { 0.0 } else if walk_held { 1.0 } else { 2.0 };
    let inc = if target_level >= 2.0 { 6.0 } else { 3.0 };
    p.speed_level = if p.speed_level < target_level {
        (p.speed_level + inc * dt).min(target_level)
    } else {
        (p.speed_level - 3.0 * dt).max(target_level)
    };
}

/// Walk/run velocity for the current speed level, turning toward the stick (free) or facing
/// the target (locked on, strafing).
fn move_velocity(p: &mut Player, a: &mut Actor, d: &CharData, config: &GameConfig, stick: Vec3, turn: Vec3, target_dir: Option<Vec3>, dt: f32) {
    // Locked on: the stick relative to the facing picks the directional clip; free: forward.
    let local = match target_dir {
        Some(td) => {
            // Locked on, Wolf turns to the target at the turn controller's rate (TAE SetTurnSpeed
            // with IsLockOnCheck, else the exe default 720 deg/s, FUN_1407daa20), not instantly:
            // the directional clip is chosen from the stick against the facing so far.
            a.yaw = turn_toward(a.yaw, yaw_of(td), turn_rate(d, a, config, true).to_radians() * dt);
            Quat::from_rotation_y(-a.yaw) * stick
        }
        None => {
            // The turn is the manipulator's (vf22, this frame's stick), the clip's the HKS's.
            free_turn(p, a, d, config, turn, dt);
            Vec3::NEG_Z
        }
    };
    // Move direction class: HKS _MoveDirectionUpdate cones (|a| < 55 F, > 125 B, else L / R) with
    // the exe WalkTwist classifier's hysteresis (FUN_1407f7550: 60 deg cones -+5, so the current
    // class holds out to 65 deg / in to 115 deg) - no flicker at a cone edge.
    if target_dir.is_some() && stick != Vec3::ZERO {
        let angle = local.x.atan2(-local.z).to_degrees();
        let side = if angle < 0.0 { 2 } else { 3 };
        let (front, back) = match a.move_dir {
            0 => (65.0, 125.0),
            1 => (55.0, 115.0),
            _ => (55.0, 125.0),
        };
        a.move_dir = if angle.abs() < front { 0 } else if angle.abs() > back { 1 } else { side };
        // WalkTwist target: the move direction relative to the clip's (F 0, B 180, L -90, R 90).
        let clip_angle = [0.0, 180.0, -90.0, 90.0][a.move_dir as usize];
        a.twist_target = ((angle - clip_angle + 180.0).rem_euclid(360.0) - 180.0).to_radians();
    } else if target_dir.is_none() {
        a.move_dir = 0;
    }
    // Speed = the shown clip's root motion at its current time (Havok: the move layers'
    // MotionSelector plays the same clip with useMotion), so the planted foot stays put - the
    // move starts accelerate, the loops surge with each stride. Letting go of the stick scales it
    // down with the speed level until the stop anim / idle takes over.
    let (key, ct, looping) = a.locomotion_clip_at(a.t);
    let clip_speed = d.root_velocity_at(&key, ct, looping).length();
    let speed = if stick == Vec3::ZERO { clip_speed * p.speed_level.min(1.0) } else { clip_speed };
    // Stick released: on along the shown clip's direction (F / B / L / R from the facing), not
    // the facing, which locked on points at the target (2026-10-12 bug hunt round 20: a legs-only
    // back or strafe move slid Wolf toward the enemy).
    let dir = if stick == Vec3::ZERO {
        let clip_angle = [0.0f32, 180.0, -90.0, 90.0][(a.move_dir as usize).min(3)].to_radians();
        Quat::from_rotation_y(a.yaw) * Vec3::new(clip_angle.sin(), 0.0, -clip_angle.cos())
    } else if target_dir.is_some() {
        stick
    } else {
        a.forward()
    };
    a.move_vel = dir * speed;
}

#[cfg(test)]
mod tests {
    /// Every whole 1/30 s TAE frame start fires on exactly one tick of the f32 clock that
    /// actor::advance runs (t += the 60 Hz fixed step), e.g. a200_510110's kill-frame recoveries
    /// at 0.36666667 (round 12: the slack fired 38 of 170 player recoveries twice).
    #[test]
    fn the_plunge_fall_check_works_below_world_zero() {
        // Wolf 4 m over an enemy 10 m below the export origin, falling 1 m/s (fall_orbit_closest:
        // straight down to its height 0.3 m off, in 0.69 s; heightLimit 3, range 1.5, 2 s).
        use bevy::math::Vec3;
        let target = Vec3::new(0.0, -10.0, 0.0);
        let (q, t) = super::fall_orbit_closest(target + Vec3::new(0.0, 4.0, 0.3), Vec3::new(0.0, -1.0, 0.0), target);
        assert!(q.distance(target) <= 1.5 && t <= 2.0, "{q} {t}");
        // Out of range sideways: no plunge.
        let (q, _) = super::fall_orbit_closest(target + Vec3::new(3.0, 4.0, 0.0), Vec3::new(0.0, -1.0, 0.0), target);
        assert!(q.distance(target) > 1.5);
    }

    #[test]
    fn one_shot_recoveries_fire_once() {
        let dt = std::time::Duration::from_secs_f64(1.0 / 60.0).as_secs_f32();
        for frame in 0..120 {
            let start = frame as f32 / 30.0;
            let (mut prev_t, mut t, mut fired) = (0.0f32, 0.0f32, 0);
            for _ in 0..300 {
                prev_t = t;
                t += dt;
                fired += super::one_shot_starts(start, prev_t, t) as u32;
            }
            assert_eq!(fired, 1, "frame {frame} ({start} s) fired {fired} times");
        }
    }

    #[test]
    fn velosity_scale_ease_curves() {
        // The args every 922 event carries: 3 / 3 (EaseInOut, cube).
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(super::ease((3, 3), 0.25), 0.0625));
        assert!(close(super::ease((3, 3), 0.5), 0.5));
        assert!(close(super::ease((3, 3), 0.75), 0.9375));
        assert!(close(super::ease((2, 3), 0.5), 0.875));
        assert!(close(super::ease((1, 3), 0.5), 0.125));
        assert!(close(super::ease((0, 3), 0.3), 0.3));
    }

    /// A pad drives PadInput: R1 attack, cross jump, a half-tilted left stick walks.
    #[test]
    fn gamepad_feeds_pad_input() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::*;
        let mut world = World::new();
        let mut g = Gamepad::default();
        g.digital_mut().press(GamepadButton::RightTrigger);
        g.digital_mut().press(GamepadButton::South);
        g.analog_mut().set(GamepadAxis::LeftStickY, 0.5);
        world.spawn(g);
        let pad = world
            .run_system_once(|pads: Query<&Gamepad>| {
                let mut pad = super::PadInput::default();
                super::read_pad(&pads, true, &mut pad);
                pad
            })
            .unwrap();
        assert!(pad.attack_held);
        assert!(pad.pressed.contains(&super::Action::Attack) && pad.pressed.contains(&super::Action::Jump));
        assert!((pad.stick.y - 0.5).abs() < 1e-4 && pad.walk, "{:?}", pad.stick);
    }

    /// Walk to run above 0.9, run back to walk only below 0.7 (c0000_transition `_SpeedUpdate`).
    #[test]
    fn walk_and_run_switch_with_hysteresis() {
        assert!(!super::stick_runs(false, 0.8), "walking at 0.8 stays a walk");
        assert!(super::stick_runs(false, 0.95));
        assert!(super::stick_runs(true, 0.8), "running at 0.8 stays a run");
        assert!(!super::stick_runs(true, 0.6));
    }
}

/// EMEVD 2004[12] Set Character Immortality on Wolf (enemy::script::WolfImmortal, e.g. the
/// prologue Genichiro 11125812): the exe's NoDead clamp in SetHp (FUN_140bd64e0) keeps HP at 1 when
/// a hit or tick would empty it from above 0. Run before Decide, so every damage since the last tick
/// (Resolve, status ticks) is clamped before decide reads HP. The killing hit's death pick is dropped as the god cheat does (round
/// 73, for Bosees' scripted loss).
fn wolf_no_dead(immortal: Option<Res<crate::enemy::script::WolfImmortal>>, mut q: Query<&mut Actor, With<Player>>, mut last: Local<f32>) {
    let Ok(mut a) = q.single_mut() else { return };
    if let Some(hp) = no_dead_clamp(immortal.is_some_and(|i| i.0), *last, a.hp) {
        a.hp = hp;
        a.death_ref = 0;
        a.death_hit = None;
    }
    *last = a.hp;
}

/// SetHp's NoDead rule (FUN_140bd64e0): immortal, a new HP below 1 from an old one above 0 is 1.
pub(crate) fn no_dead_clamp(immortal: bool, old: f32, new: f32) -> Option<f32> {
    (immortal && new < 1.0 && old > 0.0).then_some(1.0)
}
