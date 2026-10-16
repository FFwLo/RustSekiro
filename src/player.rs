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

use crate::actor::{Actor, ActorSet, Side};
use crate::camera::{LockOn, OrbitCamera};
use crate::config::GameConfig;
use crate::data::{CharData, Combat, FALL_TYPES, calc_correct};
use crate::enemy::Enemy;
use crate::hud::CombatLog;

#[derive(Component)]
pub struct Player {
    /// Plunge deathblow in flight: the posture-broken enemy Wolf is falling onto and the
    /// ThrowParam suffix of the landing (151 崩し落下1, 161 蹴り崩し1 out of a head-kick jump).
    plunge: Option<(Entity, i64)>,
    /// A launch homing on a character (ChrPhysicsVelocityChangeParam homingId other than the
    /// wire's: the jump arts' 10010 -> ChrPhysicsHomingParam 20000), until he lands.
    leap_homing: Option<LeapHoming>,
    /// HKS MoveSpeedLevel: 0 idle, 1 walk, 2 run (converges at 3/s).
    pub(crate) speed_level: f32,
    requests: HashMap<Action, f32>,
    /// State after the last decide; a different state at the next decide was set by combat.
    last_state: String,
    /// Healing Gourd uses left (config player.gourd_charges; goods 3000 maxNum 10).
    pub gourd: u32,
    /// Resurrection nodes left (config player.resurrections).
    pub resurrections: u32,
    /// Attack pressed on the death loop before its f0 refresh was read: revive once it is.
    revive_queued: bool,
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
    /// Last ground jump was forward (HKS Selector_GroundJumpType): picks its land anim.
    jump_forward: bool,
    /// Landing of a directional (positioning) jump, which takes priority over moving on.
    jump_land: Option<&'static str>,
    /// HKS EnablePositioningJumpLand (c0000_transition.lua 2800): set by every non-sprint ground
    /// jump, vertical too; BEH_R_LAND 2026 takes the stiff landing from 3 m with it (round 76:
    /// only the locked-on jumps had it, via jump_land).
    positioning_land: bool,
    /// Aged and past W_StandAging: the form comes back as W_AgingIdle after a throw or a
    /// reaction (HKS 381), not with the onset again.
    aged_form: bool,
    /// Locked-on step tilt (HKS _set4DirStepTilt -> act 3025): root-motion yaw offset for the step.
    step_tilt: f32,
    /// The jump's root tilt (HKS act 3025 from _setPositioningJumpTilt / _set4DirJumpTilt: the
    /// stick's angle minus its sector's centre), radians, on the ground jumps and the kick jump;
    /// it turns their root motion and TAE 920 launch (round 76: they flew along the sector's
    /// fixed line, up to 22.5 / 45 deg off the stick).
    jump_tilt: f32,
    /// Deathblow start throw in progress: the enemy, the main ThrowParam row suffix and whether
    /// Wolf is behind it (deathblow_check). Round 37: `behind` was re-derived from the suffix
    /// (111 / 21) and missed the boss rows (Todome 181, HU 611 / 619, Gyoubu's 119), so the boss
    /// was turned 180 degrees onto Wolf.
    throw_start: Option<(Entity, i64, bool)>,
    /// The enemy of the running deathblow, until it dies (the kill follow-up anim).
    pub(crate) throw_target: Option<Entity>,
    /// Equipped prosthetic slot (index into config player.prosthetics; Z cycles it).
    pub tool_slot: usize,
    /// TAE 922 ChrPhysicsVelosityScale in progress (`VelScale`).
    vel_scale: Option<VelScale>,
    /// A switch is in progress: when its arm anim ends the new tool unfolds (SubWeaponExpand).
    expand_pending: bool,
    /// The tool group of the last prosthetic use (HKS g_beforeSubAttackType): a combo follow-up
    /// needs the same tool.
    pub sub_cat_before: i64,
    /// HKS g_airSubAttackCount: tool uses this jump (hks::AIR_SUB_ATTACK_COUNT_MAX), 0 on the ground.
    pub air_sub_count: u32,
    /// HKS g_airSpecialAttackCount: Sakura Dance (110) uses this jump (AIR_SP_ATTACK_COUNT_MAX 1),
    /// also counted by its ground leap; 0 on the ground except in the jump readies (_LandReset).
    pub air_art_count: u32,
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
    perch: Option<Vec3>,
    /// Attack pressed during the Grappling Hook Attack's landing (the Todome input).
    wire_todome: bool,
    /// The highest point of the current flight (on the ground: the current height). The landing
    /// reads the fall height from it (env(224), `FALL_HEIGHT_LONG_STIFF_LAND`).
    air_top: f32,
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
}

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
const GOURD_GOODS: i64 = 3000;
/// PlayerIns +0x2134 (FUN_140a217a0, 140a2.c:4246): death -> the resurrection choice opens.
const REVIVE_DELAY: f32 = 1.8;
/// PlayerIns +0x2128 (140a2.c:4244, 15.0): how long the choice stays open.
const REVIVE_WINDOW: f32 = 15.0;
/// ChrActionFlag 31 "AnimCancelEnd_UseItem" / 90 "Limit Move Speed To Walk".
const FLAG_ACCEPT_ITEM: i64 = 31;
/// ChrActionFlag 137 "AnimCancelEnd_R2_Prosthetic".
const FLAG_ACCEPT_PROSTHETIC: i64 = 137;
const FLAG_LIMIT_WALK: i64 = 90;
/// SpEffect 3000 (goods 3000, the Healing Gourd): changeHpEstusFlaskRate -40 -> 40 % HP.
const GOURD_SPEFFECT: &str = "3000";
/// Whirlwind Slash: weapon 5100 (spAtkcategory 100 -> anim group a100, behavior
/// variation 5001 -> judges 200/201, dark damage 125 %), GroundSpecialAttackCombo1 = 316000.
const ART_WHIRLWIND: &str = "a100_316000";

/// The equipped combat art's first anim: EquipParamWeapon[config player.combat_art]
/// spAtkcategory -> group a<cat>, GroundSpecialAttackCombo1 = <group>_316000 (or 316001); the jump
/// arts (107 / 110) have none: their GroundSpecialAttackJumpReady 316700.
fn art_anim(d: &CharData, combat: &Combat, config: &GameConfig) -> Option<String> {
    let cat = combat.param("EquipParamWeapon", config.player.combat_art)["spAtkcategory"].as_i64()?;
    [316000, 316001, 316700].iter().map(|id| format!("a{cat:03}_{id}")).find(|k| d.anim(k).is_some_and(|a| a.duration.is_some()))
}
/// The art combo's next state and clip when its window (ref 223 SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO)
/// is up and the equipped art has one (`art_combo_next`), in the art's anim group.
fn art_combo_pick(d: &CharData, a: &Actor, combat: &Combat, config: &GameConfig, emblems: u32) -> Option<(&'static str, String)> {
    if a.anim.is_empty() || !d.has_ref(&a.anim, a.t, REF_ENABLE_SP_ATK_COMBO) {
        return None;
    }
    let (cat, unlock) = art_kind(combat, config)?;
    let state = art_combo_next(&a.state, cat, emblems >= art_cost(combat, config), |r| r == unlock)?;
    let id = d.states.get(state)?.rsplit('_').next()?.to_string();
    let key = format!("a{cat:03}_{id}");
    d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
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

/// HKS _FireSpAttackCombo: the art state that follows `state` when the art is pressed again
/// inside SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO (223). `cat` = spAtkcategory (SP_ATK_TYPE_1xx);
/// `unlocked(ref)` = the SP_EF_REF_WEP_SP_ATK_UNLOCK_* ref is up (the upgraded art's resident
/// SpEffect, e.g. Ichimonji: Double 7100 -> 140201 -> ref 281).
fn art_combo_next(state: &str, cat: i64, enable: bool, unlocked: impl Fn(i64) -> bool) -> Option<&'static str> {
    const UNLOCK_102_COMBO: i64 = 281;
    const UNLOCK_105_COMBO: i64 = 282;
    const UNLOCK_107_COMBO: i64 = 284;
    const UNLOCK_108_FINISH: i64 = 285;
    match state {
        // Line 5379: One Mind's hold action -> its second cut (W_GroundSpacialAttackVariationCombo2,
        // *NoResource without the emblems).
        "GroundSpacialAttackHoldAction" | "GroundSpacialAttackHoldActionNoResource" if cat == 104 => {
            Some(if enable { "GroundSpacialAttackVariationCombo2" } else { "GroundSpacialAttackVariationCombo2NoResource" })
        }
        // Line 5406: out of the hold's end, the draw.
        "GroundSpacialAttackHoldEnd" => Some("GroundSpecialAttackCombo1"),
        // Line 5386: also out of the jump arts' landings (High Monk's kicks go on).
        "GroundSpecialAttackCombo1"
        | "GroundSpecialAttackCombo1Release"
        | "SprintSpecialAttack"
        | "SprintSpecialAttackRelease"
        | "LandAirSpecialAttack"
        | "LandAirSpecialAttackStart"
        | "LandAirSpecialAttackLoop"
        | "LandGroundSpecialAttackJumpStart"
        | "LandGroundSpecialAttackJumpFallLoop" => match cat {
            102 if !unlocked(UNLOCK_102_COMBO) => None,
            107 if !unlocked(UNLOCK_107_COMBO) => Some("GroundSpecialAttackCombo2Finish"),
            108 if !unlocked(UNLOCK_108_FINISH) => Some("GroundSpecialAttackCombo2Finish"),
            _ => Some("GroundSpecialAttackCombo2"),
        },
        "GroundSpecialAttackCombo2" => match cat {
            105 if !unlocked(UNLOCK_105_COMBO) => Some("GroundSpacialAttackVariationCombo3"),
            107 if !unlocked(UNLOCK_107_COMBO) => None,
            108 if !unlocked(UNLOCK_108_FINISH) => None,
            _ => Some("GroundSpecialAttackCombo3"),
        },
        "GroundSpecialAttackCombo3" => Some("GroundSpecialAttackCombo4"),
        "GroundSpecialAttackCombo4" => Some("GroundSpecialAttackCombo5"),
        _ => None,
    }
}

/// HKS BEH_A_AIR_SP_ATTACK (2906-2940): the art pressed in the air -> (state, counted). `enable` =
/// env(3035) (the emblems), `count` = g_airSpecialAttackCount, `ref_226` = the hit jump's derive
/// window. Sakura Dance (110) once per jump (else W_AddActionInputSpacialAttack, an additive: none
/// here). Without ACTION_UNLOCK_TYPE_AIR_SP_ATTACK (25, line 2915) the caller drops the press.
pub(crate) fn air_art_state(cat: i64, unlock: i64, enable: bool, count: u32, ref_226: bool) -> Option<(&'static str, bool)> {
    const AIR_SP_ATTACK_COUNT_MAX: u32 = 1;
    Some(match cat {
        110 if count >= AIR_SP_ATTACK_COUNT_MAX => return None,
        101 | 107 | 108 => ("AirSpecialAttackStart", false),
        110 => (if enable { "AirSpecialAttackStart" } else { "AirSpecialAttackStartNoResource" }, true),
        109 if unlock == SP_REF_UNLOCK_109_FALL_ATTACK && ref_226 => ("GroundSpecialAttackHitJumpDeriveAction", false),
        104 => ("AirSpecialAttackHoldStart", false),
        103 if !enable => ("AirSpecialAttackNoResource", false),
        _ => ("AirSpecialAttack", false),
    })
}

/// HKS BEH_R_LAND (1971-2015) for the air art states -> (land state, keep the time). `ref_201` =
/// SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION, `ref_288` = ..._SP_ATK_110 (Sakura Dance's bounce,
/// a110_316200 f0-37). None = the plain _LandFreeFall.
fn air_art_land(cat: i64, state: &str, ref_201: bool, ref_288: bool) -> Option<(&'static str, bool)> {
    Some(match state {
        "AirSpecialAttack" if ref_201 => ("LandAirSpecialAttack", true),
        "AirSpecialAttackNoResource" if ref_201 => ("LandAirSpecialAttackNoResource", true),
        "AirSpecialAttackStart" if cat == 110 && ref_288 => ("AirSpecialAttackLandingJumpReady", true),
        "AirSpecialAttackLandingJumpStart" if cat == 110 && ref_201 => ("LandGroundSpecialAttackJumpAfterJumpStart", true),
        "AirSpecialAttackStart" if cat == 110 && ref_201 => ("LandGroundSpecialAttackJumpFallLoop", true),
        "AirSpecialAttackStartNoResource" if cat == 110 && ref_288 => ("AirSpecialAttackLandingJumpReadyNoResource", true),
        "AirSpecialAttackLandingJumpStartNoResource" if cat == 110 && ref_201 => ("LandGroundSpecialAttackJumpAfterJumpStartNoResource", true),
        "AirSpecialAttackStartNoResource" if cat == 110 && ref_201 => ("LandGroundSpecialAttackJumpFallLoopNoResource", true),
        "AirSpecialAttackStart" if ref_201 => ("LandAirSpecialAttackStart", true),
        "AirSpecialAttackLoop" => ("LandAirSpecialAttackLoop", false),
        "AirSpecialAttackLoopNoResource" => ("LandAirSpecialAttackLoopNoResource", false),
        "AirSpacialAttackEnd" if ref_201 => ("LandAirSpacialAttackEnd", true),
        "AirSpecialAttackHoldStart" if ref_201 => ("LandAirSpecialAttackHoldStart", true),
        "AirSpecialAttackHoldLoop" => ("LandAirSpecialAttackHoldLoop", false),
        "AirSpacialAttackHoldEnd" if ref_201 => ("LandAirSpacialAttackHoldEnd", true),
        _ => return None,
    })
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

/// HKS BEH_A_GROUND_SP_ATTACK: the art's opening state by type. Out of a sprint (ref 1) the
/// Sprint* variant; 101 steps (_set2DirStepDir: stick within +-90 deg of the facing -> F, no
/// stick -> N, else B; without SP_EF_REF_WEP_SP_ATK_UNLOCK_101_BACK_ATTACK (280) always F);
/// otherwise GroundSpecialAttackCombo1. Candidates in order; the first whose anim exists in the
/// art's group wins; 104 opens its hold (GroundSpacialAttackHoldStart). `enable` = env(3035,
/// ACTION_ARM_SPECIAL_ATTACK): Spirit Emblems held >= the art's resourceItemA (`art_cost`; the exe's
/// FUN_140a26010, as for the tools): without, Dragon Flash (103) plays its *NoResource opening
/// (a103_316001 / 316301).
fn art_start_states(cat: i64, unlock: i64, sprint: bool, stick: Vec3, fwd: Vec3, enable: bool) -> Vec<String> {
    let mut out = Vec::new();
    let pre = if sprint { "Sprint" } else { "Ground" };
    // 107 (Senpou Leaping Kicks / High Monk) and 110 (Sakura Dance) leap first (HKS 3437-3453):
    // W_GroundSpecialAttackJumpReady (Sprint* out of a sprint; 110 without the emblems
    // *NoResource; the sprint one plays 316770, `art_state_id`).
    if cat == 107 || cat == 110 {
        let nr = if cat == 110 && !enable { "NoResource" } else { "" };
        if sprint {
            out.push(format!("SprintSpecialAttackJumpReady{nr}"));
        }
        out.push(format!("GroundSpecialAttackJumpReady{nr}"));
        out.push("GroundSpecialAttackJumpReady".to_string());
        return out;
    }
    if cat == 103 && !enable {
        out.push(if sprint { "SprintSpecialAttackNoResource" } else { "GroundSpecialAttackCombo1NoResource" }.to_string());
    }
    if cat == 101 {
        let dir = if unlock != 280 || (stick != Vec3::ZERO && stick.dot(fwd) > 0.0) {
            "F"
        } else if stick == Vec3::ZERO {
            "N"
        } else {
            "B"
        };
        out.push(format!("{pre}SpecialAttackStep_{dir}"));
    }
    if cat == 104 {
        out.push(if sprint { "SprintSpecialAttackHoldStart" } else { "GroundSpacialAttackHoldStart" }.to_string());
    }
    if sprint && cat != 104 {
        out.push("SprintSpecialAttack".to_string());
    }
    out.push("GroundSpecialAttackCombo1".to_string());
    out
}

/// Knowledge of Medicine (SkillParam 170 / 171 / 600-602, each 150200 + 150210): every learned one
/// adds 150210's accumuVal 1; the stacks climb 150200's accumuOverFireId chain (LV2 150201 at
/// accumuOverVal 2, LV3 150202 at 3, ...) and the level's changeHpEstusFlaskCorrectRate (1.1 - 1.5)
/// scales the gourd's heal (sim test `latent_skill_rates`: 1.1, 1.2).
pub fn medicine_rate(combat: &Combat, config: &GameConfig) -> f32 {
    let effects: Vec<&serde_json::Value> = crate::combat::skill_sp_effects(combat, config).collect();
    let stacks: i64 = effects.iter().filter_map(|se| se["accumuVal"].as_i64()).filter(|v| *v > 0).sum();
    let Some(mut row) = effects.into_iter().find(|se| se["changeHpEstusFlaskCorrectRate"].as_f64().is_some_and(|r| r != 1.0)) else { return 1.0 };
    for _ in 0..8 {
        let (next, need) = (row["accumuOverFireId"].as_i64().unwrap_or(-1), row["accumuOverVal"].as_i64().unwrap_or(0));
        let up = combat.param("SpEffectParam", next);
        if next <= 0 || need <= 0 || stacks < need || up.is_null() {
            break;
        }
        row = up;
    }
    row["changeHpEstusFlaskCorrectRate"].as_f64().unwrap_or(1.0) as f32
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
pub const FLAG_ACCEPT_MOVE: i64 = 11;
pub const FLAG_SHIELD_BLOCK: i64 = 3;
pub const FLAG_DISABLE_TURN: i64 = 7;

// SP_EF_REF_* behaviour refs (c0000_define.hks).
pub const REF_ENABLE_SPRINT_ACTION: i64 = 1;
pub const REF_TRANSITION_SPRINT: i64 = 15;
pub const REF_JUST_GUARD: i64 = 203;
pub const REF_GUARD_CAN_END: i64 = 207;
pub const REF_RELEASE_ATTACK: i64 = 208;
pub const REF_GUARD_COMBO: i64 = 212;
/// In the HKS check order. 231 (COMBO_1_REVERSE) is up after deflects, left swings and
/// DeflectGuardToStand: the next slash then starts from the other side.
pub const REF_COMBO: [(i64, &str); 7] = [
    (214, "GroundAttackCombo1"),
    (215, "GroundAttackCombo2"),
    (231, "GroundAttackCombo1Reverse"),
    (219, "GroundAttackCombo2Reverse"),
    (216, "GroundAttackCombo3"),
    (217, "GroundAttackCombo4"),
    (218, "GroundAttackCombo5"),
];
pub const REF_PRESS_GUARD: i64 = 221;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_RELEASE: letting go of attack here plays the art's Release
/// (a106_316010 f6-18; live recording: let go at f2-3 -> a106_316110 at f6.5).
pub const REF_SP_ATK_RELEASE: i64 = 222;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_HOLD_RELEASE: a hold art (104) can end its hold here
/// (a104_316500 HoldStart f21-33; the air hold a104_316220 f0-93).
pub const REF_SP_ATK_HOLD_RELEASE: i64 = 230;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO: the art's follow-up window (SpEffect 100252).
pub const REF_ENABLE_SP_ATK_COMBO: i64 = 223;
/// SP_EF_REF_TAE_ENABLE_ADD_JUST_DEFLECT / _ADD_ACTION_INPUT_GUARD_CANCEL / _HIT_DEFLECT_CANCEL.
pub const REF_ADD_JUST_DEFLECT: i64 = 228;
pub const REF_ADD_GUARD_CANCEL: i64 = 412;
/// SP_EF_REF_TAE_ENABLE_ADD_ACTION_INPUT_GUARD: raised by the additive deflect (a000_299060 f0-9).
pub const REF_ADD_INPUT_GUARD: i64 = 411;
pub const REF_HIT_DEFLECT_CANCEL: i64 = 503;
/// SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION: landing keeps the attack going (LandAir* anims).
pub const REF_ORIGINAL_LAND_ACTION: i64 = 201;
/// c0000_define FALL_HEIGHT_LONG_STIFF_LAND (m) and the long-stiff LandFreeFall clip.
const FALL_HEIGHT_LONG_STIFF_LAND: f32 = 20.0;
const LAND_LONG_STIFF: &str = "a000_200021";
/// c0000_define 211-212: FALL_HEIGHT_SHORT_STIFF_LAND(_FROM_JUMP), metres.
const FALL_HEIGHT_SHORT_STIFF_LAND: f32 = 5.0;
const FALL_HEIGHT_SHORT_STIFF_LAND_FROM_JUMP: f32 = 3.0;
/// SP_EF_REF_TAE_ENABLE_SUB_ATTACK_DERIVE_ATTACK (c0000_define 666).
pub const REF_SUB_ATTACK_DERIVE_ATTACK: i64 = 301;
/// SP_EF_REF_TAE_ENABLE_DEFLECT_GUARD_ATTACK / _TRANSITION_STEP_ATTACK.
pub const REF_DEFLECT_GUARD_ATTACK: i64 = 213;
pub const REF_STEP_ATTACK: i64 = 224;
pub const REF_GUARD_REVERSE: i64 = 227;

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
    pressed: Vec<Action>,
    pub reset: bool,
    /// Sheathe / draw the sword (X).
    pub sheathe: bool,
}

impl PadInput {
    /// Queue a button press (used by input reading and by the simulation tests).
    pub fn press(&mut self, a: Action) {
        self.pressed.push(a);
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PadInput>()
            .init_resource::<LockOn>()
            .add_systems(Startup, spawn_player)
            .add_systems(Update, read_input.run_if(resource_exists::<ButtonInput<KeyCode>>))
            .init_resource::<ActiveThrow>()
            .add_systems(FixedUpdate, (sync_art_variation, decide, apply_refresh_effects, record_state).chain().in_set(ActorSet::Decide))
            .add_systems(FixedUpdate, follow_throw.after(ActorSet::Advance).before(crate::anim::PoseSet).before(ActorSet::Resolve));
        app.add_systems(FixedUpdate, wolf_no_dead.before(ActorSet::Decide));
        if std::env::var("SHINOBI_MOVE_LOG").is_ok() {
            app.add_systems(FixedUpdate, move_log.after(ActorSet::Resolve));
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
/// selfEscCycleTime window with at least selfEscCycleCnt presses (FUN_140808640 14080.c:2740 ->
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

/// HKS land events of the air deflect reactions (W_LandAirDeflect*): the state whose anim is the
/// air anim's id + 10 (AirDeflectHardSmall_L 132101 -> LandAirDeflectHardSmall_F 132111).
fn land_air_deflect(state: &str) -> Option<&'static str> {
    Some(match state {
        "AirDeflectHard" | "AirDeflectHardSmall_L" => "LandAirDeflectHardSmall_F",
        "AirDeflectHardSmall_R" => "LandAirDeflectHardSmall_B",
        "AirDeflectHardLarge_L" => "LandAirDeflectHardLarge_L",
        "AirDeflectHardLarge_R" => "LandAirDeflectHardLarge_R",
        "AirDeflectHardExLarge" => "LandAirDeflectHardExLarge",
        "AirDeflectEasy" | "AirDeflectEasySmall_L" => "LandAirDeflectEasySmall_L",
        "AirDeflectEasySmall_R" => "LandAirDeflectEasySmall_R",
        "AirDeflectEasyLarge_L" => "LandAirDeflectEasyLarge_L",
        "AirDeflectEasyLarge_R" => "LandAirDeflectEasyLarge_R",
        "AirDeflectEasyExLarge" => "LandAirDeflectEasyExLarge",
        _ => return None,
    })
}

/// The kill follow-up of a throw anim: "a201_510000" -> "a201_510001".
fn kill_anim(anim: &str) -> Option<String> {
    let (group, id) = anim.split_once('_')?;
    Some(format!("{group}_{:06}", id.parse::<u32>().ok()? + 1))
}

/// HKS _SetJumpDirection sectors (PRM_GROUND_JUMP_*_STICK_RANGE, degrees from the facing, + right):
/// (ready state, raw anim for the side jumps, landing state).
/// The states the jump tilt turns: the positioning ground jumps (Ready / Start / their lands) and
/// the kick jump.
fn jump_tilted(state: &str) -> bool {
    (state.contains("GroundJump") && !state.starts_with("Vertical") && !state.starts_with("Sprint")) || state.starts_with("LandGroundPositioningJump") || state.starts_with("AirKickEnemyJump")
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

/// Which enemy a deathblow goes to, from (entity, position, in reach, broken) candidates: among
/// those in reach the locked-on one, else the nearest. With `or_broken` (the HUD's 忍殺 mark) and
/// none in reach, a broken one the same way. One pick for the attack and the mark, so the mark
/// sits on the enemy the deathblow takes.
/// gap: the exe's pick among several throw candidates is not traced.
pub(crate) fn deathblow_pick(cands: impl IntoIterator<Item = (Entity, Vec3, bool, bool)>, lock: Option<Entity>, wolf: Vec3, or_broken: bool) -> Option<Entity> {
    let cands: Vec<(Entity, Vec3, bool, bool)> = cands.into_iter().collect();
    let best = |keep: &dyn Fn(&(Entity, Vec3, bool, bool)) -> bool| {
        cands
            .iter()
            .filter(|c| keep(c))
            .min_by(|x, y| {
                let key = |c: &(Entity, Vec3, bool, bool)| (Some(c.0) != lock, c.1.distance(wolf));
                let (kx, ky) = (key(x), key(y));
                kx.0.cmp(&ky.0).then(kx.1.total_cmp(&ky.1))
            })
            .map(|c| c.0)
    };
    best(&|c| c.2).or_else(|| if or_broken { best(&|c| c.3) } else { None })
}

pub fn deathblow_check(combat: &Combat, wolf: Vec3, wolf_fwd: Vec3, aged: bool, ea: &Actor, enemy: &Enemy, at: Vec3, dmy: &dyn Fn(i16) -> Option<Vec3>) -> Option<DeathblowCheck> {
    let d = &combat.player;
    let foe = combat.foe_of(ea);
    // No deathblow while the Owl's phase-2 transition holds its TAE marker 3506080 (a000_020005
    // heat-up, the 021003 kneel loop, the first 0.233 s of 20006-20008); his castle Lua answers
    // an attack then with escape 20007. Ported from AKJama/sekiro-rs b997933 (docs/OWL.md: the
    // user's real-game check). gap: the exe's test of the marker is not traced.
    if combat.data_of(ea).sp_effects_at(&ea.anim, ea.t).iter().any(|(id, _)| *id == 3_506_080) {
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
    // no finisher, near, Todome or second-form rows. gap: the exe's map from the throwable state
    // to the categories is not traced (by the rows' names).
    if aged {
        let unaware = enemy.is_unaware();
        let behind = ea.forward().angle_between(wolf_fwd.with_y(0.0).normalize_or_zero()).to_degrees() < 90.0;
        if !enemy.is_broken() && !(unaware && behind) {
            return None;
        }
        let (start_sfx, suffix) = if unaware && behind { (270, 271) } else if behind { (280, 281) } else { (260, 261) };
        let main = combat.throw(foe.throw_row(suffix)).filter(|t| d.anim(&t.atk_anim).is_some())?;
        let start = combat.throw(foe.throw_row(start_sfx)).filter(|st| d.anim(&st.atk_anim).is_some());
        let reach_row = if start.is_some() { foe.throw_row(start_sfx) } else { foe.throw_row(suffix) };
        let in_reach = at.distance(wolf) <= start.as_ref().map_or(main.dist, |st| st.dist) && throw_height_ok(combat, reach_row, wolf, at) && throw_facing_ok(combat, reach_row, wolf, wolf_fwd, at, ea.forward());
        return Some(DeathblowCheck { behind, suffix, main, start, in_reach });
    }
    let finisher = combat.data_of(ea).finisher_open(&ea.anim, ea.t).then(|| combat.finisher(&foe)).flatten();
    if let Some((id, main)) = finisher {
        let judge = (main.judge_dmy > 0).then(|| dmy(main.judge_dmy)).flatten();
        let base = judge.unwrap_or(at);
        let facing_base = if judge.is_some() { base.with_y(wolf.y) } else { base };
        if !throw_facing_ok(combat, id, wolf, wolf_fwd, facing_base, ea.forward()) || d.anim(&main.atk_anim).is_none() {
            return None;
        }
        let gap = if judge.is_some() { (base - wolf).with_y(0.0).length() } else { base.distance(wolf) };
        let in_reach = gap <= main.dist;
        return Some(DeathblowCheck { behind: false, suffix: id - foe.throw_row(0), main, start: None, in_reach });
    }
    let unaware = enemy.is_unaware();
    if !enemy.is_broken() && !unaware {
        return None;
    }
    // The start rows' DiffAngMin / Max are the facings' yaw difference (FUN_140be0270, see
    // throw_facing_ok): 崩し背後始動 0110 0-90 (Wolf faces the way the enemy does), 崩し始動
    // 0000 90-180. Round 41: by Wolf's place in front of / behind the enemy.
    let behind = ea.forward().angle_between(wolf_fwd.with_y(0.0).normalize_or_zero()).to_degrees() < 90.0;
    if unaware && !behind {
        return None;
    }
    let dist = at.distance(wolf);
    let near = combat.throw(foe.throw_row(5)).filter(|st| !behind && dist <= st.dist && d.anim(&st.atk_anim).is_some() && throw_facing_ok(combat, foe.throw_row(5), wolf, wolf_fwd, at, ea.forward()));
    let (start_sfx, suffix) = if unaware {
        (20, 21)
    } else if behind {
        (110, 111)
    } else if near.is_some() {
        (5, 6)
    } else {
        (0, 1)
    };
    // A major boss's last deathblow is its Todome (トドメ始動 / 本体, ThrowParam 0180 / 0181: Wolf
    // a2xx_501700 -> 511700, the boss ThrowDef13700); only the bosses with several deathblows have
    // these rows (c5000, c5060, c5100, c5400, c5430, c7020, c7110), the one-bar Guardian Ape and
    // Way of Tomoe too (the Ape's a000_013700 is his 39.5 s fake death: m17 11705821 raises him
    // on its message 20). gap: the exe's choice of the Todome rows is not traced (by the rows'
    // names).
    let todome = !unaware && enemy.ninsatsu.0 <= 1;
    let usable = |sfx: i64| combat.throw(foe.throw_row(sfx)).is_some_and(|t| d.anim(&t.atk_anim).is_some());
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
    let (mut start_sfx, mut suffix) = (start_sfx + hu, suffix + hu);
    if todome {
        if usable(181 + hu) {
            (start_sfx, suffix) = (180 + hu, 181 + hu);
        } else if usable(suffix + 8) {
            // "崩しトドメ" / "崩し背後トドメ" (main + 8: Gyoubu 0009 / 0119, the headless Ape
            // 0509 / 0619 -> ThrowDef12090 / 13290, whose message 30 at 9.5 s is his defeat,
            // m17 11705800), after the usual start.
            suffix += 8;
        }
    }
    let main = combat.throw(foe.throw_row(suffix))?;
    let start = combat.throw(foe.throw_row(start_sfx)).filter(|st| d.anim(&st.atk_anim).is_some());
    // The start row's height window too (upperYRange / lowerYRange: 1.26 m on the General's
    // 0020 / 0110), as the plunge and deflect-pose starts check it (combat.rs throw_in_reach):
    // round 36, a deathblow reached across a 2 m ledge. And its facings (throw_facing_ok:
    // diffAngMyToDef 35 on the General's rows - Wolf must face the enemy; round 41, before any
    // facing took it).
    let reach_row = if start.is_some() { foe.throw_row(start_sfx) } else { foe.throw_row(suffix) };
    let in_reach = dist <= start.as_ref().map_or(main.dist, |st| st.dist) && throw_height_ok(combat, reach_row, wolf, at) && throw_facing_ok(combat, reach_row, wolf, wolf_fwd, at, ea.forward());
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
        let floor = crate::map::ground_y(*pos) + CAPSULE_HALF_HEIGHT;
        if pos.y <= floor && a.vel_y < 0.0 {
            pos.y = floor;
            a.airborne = false;
            a.fall_type = 0;
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
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
    let turn = |a: &mut Actor| {
        if stick != Vec3::ZERO {
            let rate = turn_rate(d, a, config, false);
            a.yaw = turn_toward(a.yaw, yaw_of(stick), rate.to_radians() * dt);
        }
    };
    if !free {
        turn(a);
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
        turn(a);
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

/// A ThrowParam row's height window: the defender at most upperYRange above / lowerYRange below
/// the thrower. A row with neither set has no window.
pub(crate) fn throw_height_ok(combat: &Combat, row_id: i64, thrower: Vec3, defender: Vec3) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let (up, down) = (f("upperYRange"), f("lowerYRange"));
    if up <= 0.0 && down <= 0.0 {
        return true;
    }
    let dy = defender.y - thrower.y;
    dy <= up && dy >= -down
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
        match crate::map::terrain() {
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
    let mut gates = crate::sound::active_state_infos(&combat, &probe, Some(&config), tool.map(|t| t.0));
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
fn start_yaw() -> f32 {
    yaw_of((Vec3::new(0.0, 0.0, -4.0) - crate::map::player_start()).with_y(0.0))
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
        Player { plunge: None, leap_homing: None, speed_level: 0.0, requests: HashMap::new(), last_state: String::new(), gourd: config.player.gourd_charges, emblems: config.player.spirit_emblems, auto_aim: false, auto_aim_fresh: false, resurrections: config.player.resurrections, revive_queued: false, death_wait: 0.0, dead_for: 0.0, escape: None, jump_forward: false, jump_land: None, positioning_land: false, aged_form: false, step_tilt: 0.0, jump_tilt: 0.0, throw_start: None, throw_target: None, tool_slot: 0, vel_scale: None, expand_pending: false, sub_cat_before: 0, air_sub_count: 0, air_art_count: 0, art_enable_jump: false, timed: Vec::new(), held: Vec::new(), wire: None, perch: None, wire_todome: false, air_top: 0.0 },
        actor,
        Name::new("Player"),
        Transform::from_xyz(0.0, CAPSULE_HALF_HEIGHT, 4.0),
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
            .with_children(|p| crate::world::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.8, 0.85, 0.9)));
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
    pad.walk = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::ControlLeft);
    let captured = cursor.grab_mode != bevy::window::CursorGrabMode::None;
    // Mouse buttons only count once the mouse is captured (the first click captures it).
    let lmb = captured && mouse.pressed(MouseButton::Left);
    let rmb = captured && mouse.pressed(MouseButton::Right);
    pad.attack_held = lmb || keys.pressed(KeyCode::KeyJ);
    pad.guard_held = rmb || keys.pressed(KeyCode::KeyK);
    pad.dodge_held = keys.pressed(KeyCode::ShiftLeft);
    if (captured && mouse.just_pressed(MouseButton::Left)) || keys.just_pressed(KeyCode::KeyJ) {
        pad.pressed.push(Action::Attack);
    }
    if (captured && mouse.just_pressed(MouseButton::Right)) || keys.just_pressed(KeyCode::KeyK) {
        pad.pressed.push(Action::Guard);
    }
    if keys.just_pressed(KeyCode::ShiftLeft) {
        pad.pressed.push(Action::Step);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        pad.pressed.push(Action::UseItem);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        pad.pressed.push(Action::Prosthetic);
    }
    pad.prosthetic_held = keys.pressed(KeyCode::KeyF);
    if keys.just_pressed(KeyCode::Space) {
        pad.pressed.push(Action::Jump);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        pad.reset = true;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        pad.sheathe = true;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        pad.pressed.push(Action::Crouch);
    }
    // gap: the game's PC binding for ACTION_ARM_CHANGE_WEAPON_L (DefaultKeyAssignParam's key ids
    // are not decoded); Z is free here.
    if keys.just_pressed(KeyCode::KeyZ) {
        pad.pressed.push(Action::SwitchTool);
    }
    // Grappling Hook: G (gap: the game's PC key, as above); the pad's L2 as in the game.
    if keys.just_pressed(KeyCode::KeyG) {
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

fn electro_loop(state: &str) -> Option<String> {
    if !state.contains("ElectroCharge") || !state.starts_with("Air") {
        return None;
    }
    if state.ends_with("Loop") {
        Some(state.to_string())
    } else if state.starts_with("AirDamage") {
        Some(state.replace("Start", "Loop"))
    } else {
        Some(format!("{state}FallLoop"))
    }
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
fn crouch_ok(state: &str) -> bool {
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

/// HKS STATE_TYPE_UPPER_ACTION states Wolf uses (c0000.hkx StandMoveUpper_SM): drawn over the
/// walk/run by anim.rs, moved by lower_body_move. Also STATE_TYPE_UPPER_ACTION_ATK (c0000_cmsg
/// g_paramHkbState): the Finger Whistle's standing *Moveable / walking *Move states.
/// gap: the crouched ones (CrouchSubAttack*Moveable) keep still legs.
pub(crate) fn is_upper_action(state: &str) -> bool {
    state.starts_with("DeflectGuardToStandMove")
        || matches!(
            state,
            "GroundSubAttackCombo1Moveable" | "GroundSubAttackCombo1Move" | "GroundSubAttackCombo1ReleaseMoveable" | "GroundSubAttackLockOnMoveable" | "GroundSubAttackLockOnMove" | "GroundSubAttackLockOnReleaseMoveable"
        )
        // The crouched whistle (CROUCH_SUB_ATTACK_*_MOVEABLE / _MOVE: STATE_TYPE_UPPER_ACTION_ATK);
        // the legs take the crouch walk (Actor::move_clip).
        || (state.starts_with("CrouchSubAttack") && (state.ends_with("Moveable") || state.ends_with("Move")))
        // The walking gourd drinks (STATE_TYPE_UPPER_ACTION_ITEM_USE, HKB 14120 / 14121).
        || matches!(state, "ItemGourdDrinkMove" | "ItemGourdDrinkRepeatMove")
}

/// The air guard's own states (Start / Loop / End), not its break pose AirDeflectGuardBreak:
/// that one opens the guard only at its flag 117 (frame 69); a press there started a fresh guard
/// at once (2026-10-12 bug hunt round 15).
fn is_air_guard(state: &str) -> bool {
    state.starts_with("AirDeflectGuard") && state != "AirDeflectGuardBreak"
}

/// SpEffect behaviorRefIds the HKS reads with env(3036, ...) (c0000_define.lua SP_EF_REF_*).
const SP_REF_DISABLE_AIR_KICK: i64 = 108;
/// c0000_define SP_EF_REF_TAE_ONLY_ADD_ACTION_INPUT_JUMP (SpEffect 100247).
const SP_REF_TAE_ONLY_ADD_ACTION_INPUT_JUMP: i64 = 431;
/// SP_EF_REF_IN_STORM_JUMP_AREA / _WEAK_AREA (c0000_define.lua 775-776): inside an updraft (the
/// Divine Dragon's bullets 52000660 / 53100640 give 106100, ref 110003; 106101, ref 110004).
pub const SP_REF_IN_STORM_JUMP_AREA: i64 = 110003;
pub const SP_REF_IN_STORM_JUMP_WEAK_AREA: i64 = 110004;
/// SP_EF_REF_TAE_ENABLE_STORM_JUMP (101: 100328 "PC: storm jump transition possible", in the storm
/// jumps from 1.0 s and StormJumpFall).
const SP_REF_ENABLE_STORM_JUMP: i64 = 101;

/// A behaviorRefId on Wolf: his anim's SpEffects or a timed one (bullets, items).
fn wolf_ref(d: &CharData, a: &Actor, p: &Player, behavior_ref: i64) -> bool {
    sp_ref_active(d, a, behavior_ref) || p.timed.iter().any(|(id, _)| d.sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == behavior_ref))
}

/// The storm jump inside an updraft (HKS 2779-2783 on the ground, 2855-2862 in the air): the
/// full area first, then the weak one.
fn storm_jump(d: &CharData, a: &Actor, p: &Player, ground: bool) -> Option<&'static str> {
    if wolf_ref(d, a, p, SP_REF_IN_STORM_JUMP_AREA) {
        Some(if ground { "GroundStormJumpReady" } else { "AirStormJumpStart" })
    } else if wolf_ref(d, a, p, SP_REF_IN_STORM_JUMP_WEAK_AREA) {
        Some(if ground { "GroundStormJumpWeakReady" } else { "AirStormJumpWeakStart" })
    } else {
        None
    }
}
/// SP_EF_REF_TAE_ENABLE_FAST_JUMP (c0000_define.lua:616).
const SP_REF_TAE_ENABLE_FAST_JUMP: i64 = 107;
/// SP_EF_REF_TAE_ENABLE_ITEM_REPEAT (c0000_define.lua:699).
const SP_REF_TAE_ENABLE_ITEM_REPEAT: i64 = 400;
/// SP_EF_REF_TAE_ENABLE_HIT_JUMP_CANCEL / DOWN_JUMP_CANCEL (c0000_define.lua): _execDamageStep.
const SP_REF_TAE_ENABLE_HIT_JUMP_CANCEL: i64 = 502;
/// SP_EF_REF_TAE_MOVING_RUN / DISABLE_PRESS_STEP (c0000_define.lua 604 / 726): BEH_A_SPRINT.
const SP_REF_TAE_MOVING_RUN: i64 = 91;
const SP_REF_TAE_DISABLE_PRESS_STEP: i64 = 427;
const SP_REF_TAE_ENABLE_DOWN_JUMP_CANCEL: i64 = 504;
const SP_REF_KICK_ENEMY_JUMP: i64 = 204;
const SP_REF_SP_ATK_HIT_JUMP: i64 = 225;
const SP_REF_SP_ATK_HIT_JUMP_DERIVE_ACTION: i64 = 226;
/// SP_EF_REF_WEP_SP_ATK_UNLOCK_109_FALL_ATTACK: Shadowfall 7600's resident SpEffect 140901.
const SP_REF_UNLOCK_109_FALL_ATTACK: i64 = 286;
const SP_REF_ORIGINAL_LAND_ACTION_SP_ATK_110: i64 = 288;

/// env(3036, ref): is an SpEffect with this behaviorRefId applied by the current anim's TAE?
fn sp_ref_active(d: &CharData, a: &Actor, behavior_ref: i64) -> bool {
    !a.anim.is_empty() && d.sp_effects_at(&a.anim, a.t).iter().any(|(_, s)| s.behavior_ref_id == behavior_ref)
}

/// Plunge deathblow start (ThrowParam 崩し落下0, suffix 150: Dist 20 m, defender within upperYRange
/// 3 m above / lowerYRange 15 m below, normalFallOrbitCheck: the unsteered fall passes within
/// range 1.5 m of the defender, at most heightLimit 3 m above it, within timeLimit 2000 ms).
/// Plays the throw's attacker anim (a20x_511400 = 510300's clip) and the defender's ThrowDef13400.
/// Out of a head-kick jump (AirKickEnemyJumpStart*) it is 蹴り崩し0 (suffix 160: a20x_511500,
/// ThrowDef13500; heightLimit -10000 = no height check), landing on 蹴り崩し1 (161). Live
/// (rec_c1020_c_20261009): the kick broke the General, an air attack 0.33 s into 213115 started
/// a201_511500 (0.72 s) -> 511510 (1.68 s) -> 511511 with ThrowDef13500 -> 13510 -> 13511.
/// The Grappling Hook Attack's air throw on the hooked enemy (ThrowParam <chr>0170): within the
/// row's Dist and y range, and the normal fall orbit (no steering, 1/60 s steps) passing within
/// normalFallOrbitCheck_range of it before its timeLimit, as `start_plunge` checks.
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
    if etf.translation.distance(pos) > th.dist || dy > f("upperYRange") || dy < -f("lowerYRange") {
        return None;
    }
    let (range, height, time_limit) = (f("normalFallOrbitCheck_range"), f("normalFallOrbitCheck_heightLimit"), f("normalFallOrbitCheck_timeLimit") / 1000.0);
    let g = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)].gravity();
    let (mut q, mut vy) = (pos, a.vel_y);
    for _ in 0..((time_limit * 60.0) as usize).max(1) {
        vy += g / 60.0;
        q += a.air_base / 60.0 + Vec3::Y * vy / 60.0;
        if (q - etf.translation).with_y(0.0).length() <= range && (height < 0.0 || q.y - etf.translation.y <= height) {
            return Some(th);
        }
    }
    None
}

/// The plunge's normal fall orbit (no steering, 1/60 s steps): does it pass within `range` of
/// the target, at most `height` above it, inside `time_limit`?
#[allow(clippy::too_many_arguments)]
fn fall_reaches(pos: Vec3, vel_y: f32, air_base: Vec3, gravity: f32, target: Vec3, range: f32, height: f32, time_limit: f32) -> bool {
    let (mut q, mut vy) = (pos, vel_y);
    for _ in 0..((time_limit * 60.0) as usize).max(1) {
        vy += gravity / 60.0;
        q += air_base / 60.0 + Vec3::Y * vy / 60.0;
        if (q - target).with_y(0.0).length() <= range && (height < 0.0 || q.y - target.y <= height) {
            return true;
        }
        // Past the target's height the fall can no longer reach it (2026-10-12 bug hunt round
        // 16: the cut-off was world y 0.9, so no plunge started on an enemy below about -2.1).
        if q.y < target.y {
            return false;
        }
    }
    false
}

fn start_plunge(
    p: &mut Player,
    a: &mut Actor,
    combat: &Combat,
    pos: Vec3,
    enemies: &mut Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    log: &mut CombatLog,
) -> bool {
    let kick = a.state.starts_with("AirKickEnemyJumpStart");
    if a.vel_y >= 0.0 && !kick {
        return false;
    }
    // An enemy that has not noticed Wolf takes the stealth plunge 落下0 / 落下1 (0030 / 0031:
    // a20x_510300 -> 510310, ThrowDef12300 -> 12310): live (General) 0.54 s + 1.69 s + 1.83 s.
    // The row is chosen per target (2026-10-12 bug hunt round 16: one broken enemy anywhere
    // used to turn off the stealth plunge on every unaware one).
    let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
    for (e, mut ea, mut enemy, etf) in enemies.iter_mut() {
        let stealth = !enemy.is_broken() && enemy.is_unaware();
        let (start, land) = if kick {
            (160, 161)
        } else if stealth {
            (30, 31)
        } else {
            (150, 151)
        };
        // Each enemy kind's own ThrowParam rows.
        let row_id = combat.foe_of(&ea).throw_row(start);
        let row = combat.param("ThrowParam", row_id);
        let Some(th) = combat.throw(row_id) else { continue };
        let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
        let (upper, lower) = (f("upperYRange"), f("lowerYRange"));
        let (range, height, time_limit) = (f("normalFallOrbitCheck_range"), f("normalFallOrbitCheck_heightLimit"), f("normalFallOrbitCheck_timeLimit") / 1000.0);
        if !(enemy.is_broken() || stealth) || etf.translation.distance(pos) > th.dist {
            continue;
        }
        let dy = etf.translation.y - pos.y;
        if dy > upper || dy < -lower {
            continue;
        }
        let hit = fall_reaches(pos, a.vel_y, a.air_base, ft.gravity(), etf.translation, range, height, time_limit);
        if !hit || combat.player.anim(&th.atk_anim).is_none() {
            continue;
        }
        a.play("PlungeDeathblow", &th.atk_anim);
        p.plunge = Some((e, land));
        if enemy.is_broken() {
            // A broken enemy takes no reaction (react returns for Mode::Broken): it plays its
            // ThrowDef pose (13400: 3 s, over the fall's 2 s timeLimit) and is held broken while
            // it lasts, as the kick jump's 13900 (2026-10-12 bug hunt round 15: it stayed in its
            // collapse and its Broken window could run out mid-fall).
            let ed = combat.data_of(&ea);
            if ea.play_state(ed, &format!("ThrowDef{}", th.def_anim)) {
                enemy.on_posture_break(&mut ea, combat, false, 4.0);
            }
        } else {
            enemy.react(&mut ea, combat, &format!("ThrowDef{}", th.def_anim));
            if stealth {
                // Held in its ThrowDef reaction (no AI) until the landing throw.
                enemy.on_posture_break(&mut ea, combat, false, 4.0);
            }
        }
        log.push("plunge", Color::srgb(1.0, 0.5, 0.3));
        return true;
    }
    false
}

/// HKS SPRINT_BRAKE_ANGLE and SP_EF_REF_TAE_ENABLE_SPRINT_QUICK_TURN (c0000_define.lua).
const SPRINT_BRAKE_ANGLE: f32 = 135.0;
const REF_ENABLE_SPRINT_QUICK_TURN: i64 = 2;

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
        Action::Crouch | Action::SwitchTool => &[],
        // gap: the accept / execute bits of ACTION_ARM_WIRE_SHOOT are not traced; taken as the
        // jump's (151 / 119): the wire moves open their cancels together with the jump's.
        Action::Grapple => &[151],
    }
}

/// Is a press latched right now? (pending |= pressed & acceptMask in FUN_140b2c190.) A
/// press outside every accept window is dropped, not queued.
fn buffers(d: &CharData, a: &Actor, action: Action) -> bool {
    // A break pose reads attack / jump for its throw directly (decide, "ThrowBreak").
    if a.state == "ThrowBreak" && matches!(action, Action::Attack | Action::Jump) {
        return true;
    }
    // The aged standby states (g_paramHkbState STATE_TYPE_STANDBY: AgingIdle, the walk, the
    // sprint start / loop) take every input as StandIdle does.
    if a.airborne || is_free(&a.state) || is_aged_standby(&a.state) || a.anim.is_empty() || sprint_window(d, a) {
        return true;
    }
    d.flag(&a.anim, a.t, FLAG_BUFFER_ALL) || buffer_flags(action).iter().any(|&f| d.flag(&a.anim, a.t, f))
}

/// HKS ExecAttack routing (c0000_transition.lua): combo refs 214-219, step attack (ref
/// 224), sprint attack (ref 1), the counter out of a hard deflect (HardDeflectAtk) or
/// from being lightly deflected (EasyDeflectedAtk), the guard attack (ref 213), else
/// GroundAttackCombo1.
fn attack_route(d: &CharData, a: &Actor, stick: Vec3, art_equipped: bool) -> String {
    let has = |r: i64| !a.anim.is_empty() && d.has_ref(&a.anim, a.t, r);
    if let Some((_, s)) = REF_COMBO.iter().find(|(r, _)| has(*r)) {
        return s.to_string();
    }
    if has(REF_STEP_ATTACK) {
        let dir = a.state.rsplit('_').next().filter(|d| matches!(*d, "N" | "F" | "B" | "L" | "R")).unwrap_or("N");
        return format!("GroundStepAttack_{dir}");
    }
    // STYLE_TYPE_CROUCH (HKS BEH_A_GROUND_ATTACK, after the combo / step refs): Wolf stands up
    // attacking (CrouchAttackToStand a050_301000). gap: under a forced crouch (g_forceCrouch:
    // env 3037 / SP_EF_REF_FORCE_CROUCH, low ceilings) it is CrouchAttackToCrouch; not modelled.
    if a.crouch {
        return "CrouchAttackToStand".into();
    }
    if has(REF_ENABLE_SPRINT_ACTION) {
        // Selector_GroundJumpType 0 = SprintAttack_L, 1 = _R (c0000.hkx "SprintAttack Selector").
        // AttackAngle = signed stick angle from the facing (> 0 right): L when steering right
        // out of SprintLoop, or left / straight out of SprintStartFromStep_F.
        let angle_right = stick.dot(a.forward().cross(Vec3::Y)) > 0.0;
        let from_step = a.state == "SprintStartFromStep_F";
        let left = if from_step { !angle_right } else { angle_right };
        return format!("SprintAttack_{}", if left { "L" } else { "R" });
    }
    // Hard deflect reactions: StandDeflectHard{Minimum,Small,Middle}[_L/_R], HardDeflectDmg*.
    if a.state.starts_with("StandDeflectHard") || a.state.starts_with("HardDeflectDmg") || a.state.starts_with("LandAirDeflectHard") {
        let size = if a.state.contains("Middle") || a.state.contains("Large") { "Middle" } else { "Small" };
        let dir = if a.state.ends_with("_L") || a.state.ends_with('L') && a.state.starts_with("HardDeflectDmg") {
            "L"
        } else if a.state.ends_with("_R") || a.state.ends_with('R') && a.state.starts_with("HardDeflectDmg") {
            "R"
        } else {
            "F"
        };
        return format!("HardDeflectAtk{size}{dir}");
    }
    if let Some(side) = a.state.strip_prefix("EasyDeflected").filter(|s| *s == "L" || *s == "R") {
        return format!("EasyDeflectedAtk{side}");
    }
    // Only without a combat art equipped (env(345, HAND_RIGHT) == SP_ATK_TYPE_NONE).
    if has(REF_DEFLECT_GUARD_ATTACK) && !art_equipped {
        return "DeflectGuardAttack".into();
    }
    "GroundAttackCombo1".into()
}

/// The release follow-up of a held attack state (attack let go while ref 208 is up).
fn release_of(state: &str) -> Option<String> {
    Some(match state {
        "GroundAttackCombo1" | "GroundAttackCombo2" | "GroundAttackCombo1Reverse" | "GroundAttackCombo2Reverse"
        | "DeflectGuardAttack" | "CrouchAttackToStand" => format!("{state}Release"),
        s if s.starts_with("HardDeflectAtk") => {
            // HardDeflectAtkSmallL -> HardDeflectAtkReleaseSmall_L
            let rest = &s["HardDeflectAtk".len()..];
            let (size, dir) = rest.split_at(rest.len() - 1);
            format!("HardDeflectAtkRelease{size}_{dir}")
        }
        s if s.starts_with("EasyDeflectedAtk") => format!("EasyDeflectedAtkRelease_{}", &s[s.len() - 1..]),
        s if s.starts_with("GroundStepAttack_") => s.replace("GroundStepAttack_", "GroundStepAttackRelease_"),
        s if s.starts_with("SprintAttack_") => s.replace("SprintAttack_", "SprintAttackRelease_"),
        _ => return None,
    })
}

/// Free turning rate [deg/s]: the anim's TAE SetTurnSpeed (e.g. SprintLoop 360) when it
/// has one, else the engine default, config movement.turn_speed = 720 (exe: the turn
/// controller's +0x28 = 720.0 in FUN_1407da540; FUN_1407daa20 uses the TAE value when >= 0).
fn turn_rate(d: &CharData, a: &Actor, config: &GameConfig, locked: bool) -> f32 {
    if a.anim.is_empty() { None } else { d.turn_speed(&a.anim, a.t, locked) }.unwrap_or(config.movement.turn_speed)
}

fn is_guard_start(state: &str) -> bool {
    state.starts_with("StandToDeflectGuard") || state == "SprintToDeflectGuard"
}

fn is_guard_reaction(state: &str) -> bool {
    state.starts_with("StandDeflect") || state.starts_with("HardDeflectDmg") || state.starts_with("EasyDeflectDmg")
}

/// Execute flags: a pending request fires while one is up (requested |= pending & the
/// execute mask +0xe0, set by FUN_140b2bdc0): 115 attack (bits 0,25), 117 guard (2),
/// 26 step (5,13,14), 119 jump (4), 31 item (7), 137 shinobi tool (18).
fn accept_flag(action: Action) -> &'static [i64] {
    match action {
        Action::Attack => &[FLAG_ACCEPT_ATTACK],
        Action::Guard => &[FLAG_ACCEPT_GUARD],
        Action::Step => &[FLAG_ACCEPT_STEP, FLAG_ACCEPT_STEP_ALT],
        Action::Jump => &[FLAG_ACCEPT_JUMP],
        Action::UseItem => &[FLAG_ACCEPT_ITEM],
        Action::CombatArt => &[FLAG_ACCEPT_ATTACK],
        Action::Prosthetic => &[FLAG_ACCEPT_PROSTHETIC],
        Action::Grapple => &[FLAG_ACCEPT_JUMP],
        Action::Crouch | Action::SwitchTool => &[],
    }
}

/// The engine's env(1106, button): is this input accepted right now?
/// The STYLE_TYPE_GROUND_GUARD states (c0000_cmsg g_paramHkbState): the guard idle / move /
/// turns, the guard starts, the easy deflects and their recoils.
fn is_guard_style(s: &str) -> bool {
    s.starts_with("DeflectGuard")
        || s.contains("ToDeflectGuard")
        || s.starts_with("StandDeflectEasy")
        || s.starts_with("LandAirDeflectEasy")
        || matches!(s, "StandDeflectGuardFromDamage" | "EasyDeflectDmg" | "LandAirDeflectGuard")
}

fn accepts(d: &CharData, a: &Actor, action: Action) -> bool {
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

/// Ref 1 SP_EF_REF_ENABLE_SPRINT_ACTION (SprintStartFromStep_F, SprintLoop, SprintToDeflectGuard
/// 6-15): sprinting is a free state for actions - SprintLoop has no ChrActionFlags at all, and
/// the HKS picks SprintAttack / SprintToDeflectGuard from this ref.
fn sprint_window(d: &CharData, a: &Actor) -> bool {
    !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ENABLE_SPRINT_ACTION)
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

fn yaw_of(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    yaw + diff.clamp(-max_step, max_step)
}

/// A one-shot TAE event (a motionInterval 999 recovery) starting in this tick's (prev_t, t]:
/// windows touch end to end, so a start on a tick boundary is in exactly one; frame 0 counts on
/// the first step (prev_t 0). The old `prev_t - 1e-4` slack overlapped the previous window and
/// gave back the deathblow posture / Breath of Life twice (2026-10-12 bug hunt round 12).
fn one_shot_starts(start: f32, prev_t: f32, t: f32) -> bool {
    (start > prev_t || (prev_t == 0.0 && start == 0.0)) && start <= t
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
#[allow(clippy::too_many_arguments)]
fn decide(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    lock: Res<LockOn>,
    mut pad: ResMut<PadInput>,
    mut log: ResMut<CombatLog>,
    camera: Option<Single<&OrbitCamera>>,
    mut player: Single<(Entity, &mut Player, &mut Actor, &mut Transform), Without<Enemy>>,
    mut enemies: Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    enemy_dummies: Query<&crate::model::Dummies, With<Enemy>>,
    globals: Query<&GlobalTransform>,
    mut throw: ResMut<ActiveThrow>,
    fight: Option<ResMut<crate::enemy::FightReset>>,
    snap: Option<ResMut<crate::camera::CameraSnap>>,
    (wire_aim, grapples): (Option<Res<crate::wire::WireAim>>, Option<Res<crate::enemy::grapple::EnemyGrapples>>),
) {
    let dt = time.delta_secs();
    let d = &combat.player;
    let (pe, p, a, tf) = &mut *player;
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
        let discharged = !a.anim.is_empty() && d.sp_effects_at(&a.anim, a.t).iter().any(|(id, _)| matches!(id, 9505 | 9506));
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
    if a.state != p.last_state {
        if !keeps_buffer(&a.state) {
            p.requests.clear();
        }
        // Combat changed the state; record_state then only sees decide's own changes.
        p.last_state = a.state.clone();
    }
    // Raw press (death screen choices are not TAE-buffered).
    let attack_pressed = pad.pressed.contains(&Action::Attack);
    let crouch_pressed = pad.pressed.contains(&Action::Crouch);
    // Any other action stands Wolf up. Steps, guards and hit reactions from a crouch are the
    // standing ones in the HKS too (_fireGroundStep -> W_GroundStep, W_StandToDeflectGuard; the
    // Crouch{Damage,Death}* states only under a forced crouch, g_forceCrouch). The attack has
    // its own (attack_route: CrouchAttackToStand), the gourd too (CrouchItemGourdDrink*).
    if a.crouch && !crouch_ok(&a.state) {
        a.crouch = false;
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
    // env 277 (FUN_140b39e90 140b3.c:2669): BEH_R_THROW_ESCAPE -> W_ThrowEscape<ThrowId>
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
        if let Some(mut f) = fight {
            f.0 = true;
        }
        // Drawn at the start spot at once: no fade from the old pose, no deathblow model slide
        // carried over (2026-10-12 bug hunt round 17).
        commands.entity(*pe).insert(crate::anim::SnapPose);
        p.gourd = config.player.gourd_charges;
        p.emblems = config.player.spirit_emblems;
        p.resurrections = config.player.resurrections;
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
        a.hp = a.hp_max;
        a.posture = 0.0;
        a.play("StandIdle", "");
        tf.translation = crate::map::player_start();
        a.yaw = start_yaw();
        if let Some(mut s) = snap {
            s.0 = true;
        }
    }
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
        let style = an.events.iter().filter(|e| e.kind == 32 && e.start > a.prev_t && e.start <= a.t).filter_map(|e| e.args.get("WeaponStyle")?.as_str()).last();
        if let Some(style) = style {
            a.sheathed = style.starts_with('0');
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
    // Crouch toggle (HKS BEH_A_CROUCH_START / BEH_A_CROUCH_END on ACTION_ARM_CROUCH): standing
    // still -> W_CrouchStart (CrouchStart a000_216000) / W_CrouchEnd (CrouchEnd a000_216100);
    // moving -> the move loop switches clips (W_CrouchMoveLoop / W_StandMoveLoop). The crouch
    // clips carry SpEffect 109200 "Crouching stealth" (sightSearchEnemyCut 20: enemies see 20 %
    // less far).
    // Sprinting (ref 1 SP_EF_REF_ENABLE_SPRINT_ACTION) -> the slide W_SprintToCrouchReady
    // (a000_216010, its root motion slides), then SprintToCrouchLeft / Right (216020 / 216021).
    // Only with ACTION_UNLOCK_TYPE_SPRINT_TO_CROUCH (26, HKS 4498); else the plain crouch.
    if crouch_pressed
        && a.hp > 0.0
        && !a.airborne
        && !a.crouch
        && sprint_window(d, a)
        && crate::combat::action_unlocked(&combat, &config, crate::combat::UNLOCK_SPRINT_TO_CROUCH)
        && a.play_state(d, "SprintToCrouchReady")
    {
        a.crouch = true;
        p.requests.remove(&Action::Crouch);
        return;
    }
    // Without unlock 26 the sprint's crouch is the plain one (HKS 4500-4509; BEH_A_CROUCH_START
    // is on in the sprint style, g_behaviorTable 6450 column 9): idle -> W_CrouchStart, moving ->
    // straight into W_CrouchMoveLoop (no start clip). Round 65: the press was dropped (SprintLoop
    // is not a free state). AddActionInputBlend / W_AddDisableStep are not modelled.
    if crouch_pressed && a.hp > 0.0 && !a.airborne && !a.crouch && sprint_window(d, a) {
        a.crouch = true;
        if pad.stick != Vec2::ZERO {
            a.procedural("Locomotion");
            a.move_start = 0.0;
        } else if a.play_state(d, "CrouchStart") {
            a.move_vel = Vec3::ZERO;
        }
        p.requests.remove(&Action::Crouch);
        return;
    }
    if crouch_pressed && a.hp > 0.0 && !a.airborne && !is_guard_style(&a.state) && (is_free(&a.state) || matches!(a.state.as_str(), "CrouchStart" | "CrouchEnd")) {
        a.crouch = !a.crouch;
        if a.state != "Locomotion" {
            let st = if a.crouch { "CrouchStart" } else { "CrouchEnd" };
            if a.play_state(d, st) {
                a.move_vel = Vec3::ZERO;
            }
        }
        p.requests.remove(&Action::Crouch);
        return;
    }
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
        if !a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_THROW_TYPE5) {
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
    // BEH_R_DEATH fires whenever HP is out (HKS 5605), the revival too (round 60: a tick that
    // emptied the revived 1 HP before 110015's +50 % let Wolf get up at 0 and act; the revive
    // tick itself sets 1 HP).
    if a.hp <= 0.0 {
        // The resurrection choice (round 66, static): at death FUN_140a2cc30 (140a2.c:8272-8278)
        // arms +0x2138 = 1.8 s (+0x2134) when a resurrection is allowed and no stateInfo 341
        // (aging) / 359 is worn; FUN_140a36a60 (140a3.c:3351-3380) counts it down (held at 0
        // while stateInfo 351 is worn), then opens +0x212c = 15 s (+0x2128); its end is the
        // final death (FUN_1409eda50). The HKS revives from the death starts themselves too
        // (c0000_transition.lua 274-277), not only the stay loops. Before: the choice opened at
        // the loop (1.6-3.3 s late) and closed 6 s into it.
        let holding = {
            let mut ids = crate::combat::worn_sp(a, &[]);
            ids.extend(p.timed.iter().map(|t| t.0));
            ids.iter().any(|id| d.sp_effects.get(&id.to_string()).is_some_and(|s| s.state_info == 351))
        };
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
        let mut floor = crate::map::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT;
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
        // read by apply_refresh_effects after decide with prev_t < t): a press on the loop's
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
        let can_revive = p.resurrections > 0 && a.state != "AgingGroundDeathLoop";
        let choosing = if can_revive { p.dead_for >= REVIVE_DELAY && (loop_ready || start_revivable) } else { loop_ready };
        if in_loop && attack_pressed && !choosing {
            p.revive_queued = true;
        }
        // The queued press is spent whenever the choice opens (round 52: `||` skipped the take on a
        // fresh press, and the leftover revived the next death on its first tick, a second node).
        let queued = choosing && std::mem::take(&mut p.revive_queued);
        if choosing && (attack_pressed || queued) && can_revive {
            p.resurrections -= 1;
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
    // HKS _LandReset: the air tool uses count again once on the ground.
    if !a.airborne {
        p.air_sub_count = 0;
        if !a.state.contains("JumpReady") {
            p.air_art_count = 0;
        }
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
    if !a.airborne && !perched && !wiring && tf.translation.y > crate::map::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT + 0.05 && !(!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_NO_GRAVITY)) {
        a.airborne = true;
        // A plain fall (fallType 0), not the last wire flight's (2026-10-12 bug hunt round 12).
        a.fall_type = 0;
        a.vel_y = 0.0;
        a.air_base = Vec3::ZERO;
        a.play_state(d, "FreeFall");
    }

    let cam_yaw = camera.map_or(0.0, |c| c.yaw);
    let stick = stick_world(pad.stick, cam_yaw);
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
            ea.state.starts_with("ThrowDefDeath") || (en.is_dead() && !ea.anim.is_empty() && combat.data_of(ea).flag(&ea.anim, ea.t, 69))
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
        let dead = enemies.get(ee).map(|(_, ea, _, _)| ea.state.starts_with("ThrowDefDeath"));
        if a.state != "Deathblow" || dead.is_err() {
            p.throw_target = None;
        } else if dead == Ok(true) {
            p.throw_target = None;
            if let Some(kill) = kill_anim(&a.anim).filter(|k| d.anim(k).is_some()) {
                a.play("Deathblow", &kill);
                a.move_vel = Vec3::ZERO;
            }
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
                            enemy.on_posture_break(&mut ea, &combat, false, 4.0);
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
        let open = |(_, ea, en, _): &(Entity, &Actor, &Enemy, &Transform)| en.is_broken() && !ea.anim.is_empty() && combat.data_of(ea).flag(&ea.anim, ea.t, FLAG_THROW_START2);
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
                        enemy.on_posture_break(&mut ea, &combat, false, 4.0);
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
        let over = enemies.iter().find_map(|(ee, ea, en, etf)| {
                // Each enemy kind's own 0200 row.
                let st = combat.throw(combat.foe_of(ea).throw_row(200)).filter(|st| d.anim(&st.atk_anim).is_some())?;
                let to_wolf = (tf.translation - etf.translation).with_y(0.0).normalize_or_zero();
                let facing = ea.forward().angle_between(to_wolf).to_degrees() < 90.0;
                (en.is_broken() && facing && etf.translation.distance(tf.translation) <= st.dist).then(|| (ee, etf.translation, st.atk_anim.clone()))
        });
        if let Some((ee, epos, anim)) = over {
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
    if p.requests.contains_key(&Action::Attack) && accepts(d, a, Action::Attack) {
        let dmy_of = |ee: Entity| move |id: i16| enemy_dummies.get(ee).ok().and_then(|dm| dm.0.get(&id)).and_then(|m| globals.get(*m).ok()).map(|g| g.translation());
        // Several enemies open at once: deathblow_pick (the locked-on one, else the nearest).
        let pick = deathblow_pick(
            enemies.iter().filter_map(|(ee, ea, en, etf)| {
                deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), ea, en, etf.translation, &dmy_of(ee)).map(|db| (ee, etf.translation, db.in_reach, en.is_broken()))
            }),
            lock.target,
            tf.translation,
            false,
        );
        if let Some((ee, mut ea, mut enemy, etf)) = pick.and_then(|e| enemies.get_mut(e).ok()) {
            if let Some(db) = deathblow_check(&combat, tf.translation, a.forward(), a.aged.is_some(), &ea, &enemy, etf.translation, &dmy_of(ee)) {
                let (behind, suffix, th, start) = (db.behind, db.suffix, db.main, db.start);
                p.requests.remove(&Action::Attack);
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

    // Grappling Hook (wire.rs; docs/kb/grapple.md). A reaction or anything else that takes Wolf
    // out of the wire states ends the move.
    if p.wire.is_some() && !crate::wire::is_wire_state(&a.state) {
        p.wire = None;
    }
    if let Some(w) = p.wire.as_mut() {
        w.turn(a, tf.translation, dt);
        // 905 WireDisableFalling: the air shoots hang in the air.
        if !a.anim.is_empty() && d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 905 && e.in_time(a.t))) {
            a.vel_y = 0.0;
            a.air_base = Vec3::ZERO;
            a.move_vel = Vec3::ZERO;
        }
        // The shoot ends into its move start (selector on WireMoveStartIndex).
        if ended && crate::wire::is_shoot(&a.state) {
            let next = crate::wire::move_start(&a.state, w.index);
            a.move_vel = Vec3::ZERO;
            if a.play_state(d, &next) {
                return;
            }
        }
    }
    // BEH_A_GROUND_WIRE_SHOOT / BEH_A_AIR_WIRE_SHOOT (HKS 6251 / 6154): the press with a point in
    // the action range (env IsWirePossible); ACTION_UNLOCK_TYPE_SUB_WEAPONE (4) is always on.
    // Without one the request is dropped (ResetRequest). On the ground from standby or the jump's
    // window (`accept_flag`), in the air from the falls, the wire move or the jump's window.
    if p.requests.contains_key(&Action::Grapple) {
        let in_wire = p.wire.is_some();
        let ok = if a.airborne {
            matches!(a.state.as_str(), "FreeFall" | "WireMove") || a.state.ends_with("GroundJumpFall") || (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_ACCEPT_JUMP))
        } else {
            is_free(&a.state) || accepts(d, a, Action::Grapple)
        };
        let target = wire_aim.as_ref().and_then(|w| w.target).filter(|t| t.in_action_range);
        let enemy = wire_aim.as_ref().and_then(|w| w.enemy);
        if ok || target.is_none() {
            p.requests.remove(&Action::Grapple);
        }
        if let Some(t) = target.filter(|_| ok) {
            let shoot = crate::wire::shoot_state(&a.state, a.airborne, in_wire);
            let wire = crate::wire::Wire::new(&t, enemy, tf.translation);
            if a.play_state(d, shoot) {
                p.wire = Some(wire);
                p.perch = None;
                a.move_vel = Vec3::ZERO;
                log.push("grapple", Color::srgb(0.6, 0.9, 0.6));
                return;
            }
        }
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

    // Jump arts: the ready ends into the leap (TAE 920 launches it; the air branch takes it on).
    // Without the emblems GroundSpecialAttackJumpStartNoResource: only Sakura Dance ships it
    // (a110_316711, 316710's clip with its own TAE events); others fall back to the plain leap.
    if ended && (a.state.starts_with("GroundSpecialAttackJumpReady") || a.state.starts_with("SprintSpecialAttackJumpReady")) {
        if let Some((cat, _)) = art_kind(&combat, &config) {
            let nr = if a.state.ends_with("NoResource") { "NoResource" } else { "" };
            let want = format!("GroundSpecialAttackJumpStart{nr}");
            if let Some((state, k)) = [want.as_str(), "GroundSpecialAttackJumpStart"].into_iter().find_map(|s| art_key(d, cat, s).map(|k| (s, k))) {
                a.play(state, &k);
                return;
            }
        }
    }

    // Healing Gourd: drink (again from a drink: Repeat), or the empty-gourd shake. Crouched, the
    // Crouch ones (HKS STYLE_TYPE_CROUCH: W_CrouchItemGourdDrink / ...Repeat / ...Failed,
    // a000_250010 / 250011 / 250015), and Wolf stays crouched (crouch_ok).
    if p.requests.contains_key(&Action::UseItem) && accepts(d, a, Action::UseItem) {
        p.requests.remove(&Action::UseItem);
        // The repeat window counts only in the drinks (HKS 3986 / 3993: Drink, DrinkMove, Repeat,
        // RepeatMove), not in the empty shakes (round 32).
        let repeat_window = a.state.trim_start_matches("Crouch").starts_with("ItemGourdDrink") && !a.state.ends_with("Failed") && sp_ref_active(d, a, SP_REF_TAE_ENABLE_ITEM_REPEAT);
        // itemUseEnable (env 233) FALSE: a worn SpEffect whose stateInfo is one of goods 3000's
        // useLimitSpEffectType0-4 (910: the Great Shinobi Owl's recovery seal 3506090, 30 s,
        // from his bullets onto p.timed) -> W_ItemFailed / W_CrouchItemFailed (HKS 3940-3951):
        // no drink, no charge (round 66: he drank through it). Aged (341) the HKS resets first.
        // gap: ItemFailed (a000_259000) and CrouchItemFailed are not in the export (no state, no
        // clip): the press is only spent until they are. The exe's goods use check is not traced.
        if a.aged.is_none() {
            let limits = crate::data::goods_use_limits(GOURD_GOODS);
            let worn = crate::combat::worn_sp(a, &p.timed);
            if worn.iter().any(|id| crate::data::sp_cycle(*id).is_some_and(|c| limits.contains(&c.state_info))) {
                if !a.crouch && a.play_state(d, "ItemFailed") {
                    a.move_vel = Vec3::ZERO;
                }
                return;
            }
        }
        let state = if p.gourd == 0 {
            // c0000_transition.lua 4024-4033: empty inside the repeat window,
            // W_ItemGourdDrinkRepeatFailed (a000_250006, 1.83 s), else the 2.33 s shake.
            if repeat_window { "ItemGourdDrinkRepeatFailed" } else { "ItemGourdDrinkFailed" }
        } else if repeat_window {
            // c0000_transition.lua 3986-4008: Repeat only inside the TAE window of
            // SP_EF_REF_TAE_ENABLE_ITEM_REPEAT (100315: a000_250000 1.1-1.3 s, 250001 0.7-0.9 s);
            // a later press starts a fresh drink (round 26).
            "ItemGourdDrinkRepeat"
        } else {
            "ItemGourdDrink"
        };
        // HKS 3993-4009: standing and moving (GetLocomotionType() == LOCOMOTION_TYPE_MOVE), a drink
        // is W_ItemGourdDrinkMove / RepeatMove: STATE_TYPE_UPPER_ACTION_ITEM_USE with no clip of its
        // own (CMSG animId 0), the standing drink on the upper body over the walk (round 32: the
        // standing clip slid along). The crouched branch (3986-3992) never fires a Move.
        // gap: StartTime_02 (env 3063, 2: the legs' loop time) - the legs restart their loop.
        let moving = !a.crouch && stick != Vec3::ZERO && p.speed_level > 0.0 && matches!(state, "ItemGourdDrink" | "ItemGourdDrinkRepeat");
        if moving {
            if let Some(k) = d.anim_key(state).map(str::to_string) {
                a.play(&format!("{state}Move"), &k);
                return;
            }
        }
        let state = if a.crouch { format!("Crouch{state}") } else { state.to_string() };
        a.move_vel = Vec3::ZERO;
        a.play_state(d, &state);
        return;
    }
    // ConsumeCurrentGoods (TAE 65, drink frame 21 / repeat frame 9): one charge, the heal.
    if matches!(a.state.trim_start_matches("Crouch"), "ItemGourdDrink" | "ItemGourdDrinkRepeat" | "ItemGourdDrinkMove" | "ItemGourdDrinkRepeatMove") {
        let consumed = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 65 && e.start > a.prev_t && e.start <= a.t));
        if consumed && p.gourd > 0 {
            p.gourd -= 1;
            let rate = d.sp_effects.get(GOURD_SPEFFECT).map_or(40.0, |s| -s.change_hp_estus_flask_rate);
            let heal = a.hp_max * rate / 100.0 * medicine_rate(&combat, &config);
            a.hp = (a.hp + heal).min(a.hp_max);
            log.push(format!("gourd: +{heal:.0} HP ({} left)", p.gourd), Color::srgb(0.5, 1.0, 0.6));
        }
        // Flag 90: may walk while drinking (the Move ones walk on the legs: lower_body_move).
        if !is_upper_action(&a.state) && !a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_LIMIT_WALK) {
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
    // gap: _set4DirJumpTilt (act 3025) is not applied; the prone side is read from the state.
    for act in [Action::Step, Action::Jump] {
        if !p.requests.contains_key(&act) || a.anim.is_empty() || !accepts(d, a, act) {
            continue;
        }
        let state = if sp_ref_active(d, a, SP_REF_TAE_ENABLE_HIT_JUMP_CANCEL) {
            Some(format!("StandStepFromDamage_{}", if a.state.ends_with("_B") { "B" } else { "F" }))
        } else if sp_ref_active(d, a, SP_REF_TAE_ENABLE_DOWN_JUMP_CANCEL) {
            let break_blow_up = a.state.starts_with("StandDamageBreakSmallBlow") && !a.hit_back_quadrant;
            let prone = if a.state.contains("FaceUp") || a.state == "StandDamageSmallBlow_F" || break_blow_up { "FaceUp" } else { "FaceDown" };
            let dir = if stick == Vec3::ZERO {
                "N"
            } else {
                let local = Quat::from_rotation_y(-a.yaw) * stick;
                match local.x.atan2(-local.z).to_degrees() {
                    x if x > -45.0 && x < 45.0 => "F",
                    x if x > -135.0 && x < -45.0 => "L",
                    x if (45.0..135.0).contains(&x) => "R",
                    _ => "B",
                }
            };
            Some(format!("StandStepFromDown_{prone}_{dir}"))
        } else {
            None
        };
        if let Some(state) = state {
            if a.play_state(d, &state) {
                p.requests.remove(&Action::Step);
                p.requests.remove(&Action::Jump);
                a.move_vel = Vec3::ZERO;
                return;
            }
        }
    }

    // Jump
    if p.requests.contains_key(&Action::Jump) && accepts(d, a, Action::Jump) {
        p.requests.remove(&Action::Jump);
        p.jump_forward = stick != Vec3::ZERO;
        p.jump_land = None;
        p.positioning_land = false;
        p.jump_tilt = 0.0;
        if let Some(state) = storm_jump(d, a, p, true).filter(|s| a.play_state(d, s)) {
            log.push(format!("storm jump ({state})"), Color::srgb(0.7, 0.9, 1.0));
            a.move_vel = Vec3::ZERO;
            return;
        }
        // c0000_transition.lua 2784-2789: with ref 1 up (sprinting) W_SprintJumpReady (a000_201200),
        // or straight W_SprintJumpStart (a000_201210) under ref 107 SP_EF_REF_TAE_ENABLE_FAST_JUMP
        // (100399: LandForwardGroundJump frames 0-6). 201210 launches by
        // ChrPhysicsVelocityChangeParam 121 (9.15 m/s forward, 9.64 up) where the plain forward
        // jump uses 101 (6.4 m/s); it lands as a ground jump (2057-2062: W_LandGroundJump) - round 26.
        if sprint_window(d, a) {
            let state = if sp_ref_active(d, a, SP_REF_TAE_ENABLE_FAST_JUMP) { "SprintJumpStart" } else { "SprintJumpReady" };
            if stick != Vec3::ZERO {
                a.yaw = yaw_of(stick);
            }
            if a.play_state(d, state) {
                p.jump_forward = true;
                a.move_vel = Vec3::ZERO;
                return;
            }
        }
        p.positioning_land = true;
        match (stick != Vec3::ZERO, lock.target.is_some()) {
            (false, _) => {
                a.play_state(d, "VerticalGroundJumpReady");
            }
            // HKS _SetJumpDirection: locked on, the stick's angle to the facing (the target) picks
            // the jump: |a| <= 18.75 lock-on forward, then 45-degree sectors F_R / R / B_R / B and
            // mirrored. Each launches by its own ChrPhysicsVelocityChangeParam row (111-118) and
            // lands in its LandGroundPositioningJump (live: lock-on forward 1.26 m, 0.65 s).
            (true, true) => {
                // No snap to the target: KickAngle / MoveAngle are the stick against the facing
                // as it is (the lock-on turn keeps that near the target).
                let local = Quat::from_rotation_y(-a.yaw) * stick;
                let angle = local.x.atan2(-local.z).to_degrees();
                let (ready, anim, land) = directional_jump(angle);
                p.jump_tilt = jump_tilt(ready, angle);
                let played = match anim {
                    Some(k) if d.anim(k).is_some() => {
                        a.play(ready, k);
                        true
                    }
                    Some(_) => false,
                    None => a.play_state(d, ready),
                };
                if played {
                    p.jump_land = Some(land);
                } else {
                    a.play_state(d, "LockonForwardGroundJumpReady");
                    p.jump_tilt = 0.0;
                    p.jump_land = Some("LandGroundPositioningJump_LockOn_F");
                }
            }
            (true, false) => {
                a.yaw = yaw_of(stick);
                a.play_state(d, "ForwardGroundJumpReady");
            }
        }
        a.move_vel = Vec3::ZERO;
        return;
    }

    // Step (dodge). Locked on: 4 directions relative to the target. Free: forward along the stick, or neutral.
    if p.requests.contains_key(&Action::Step) && accepts(d, a, Action::Step) {
        p.requests.remove(&Action::Step);
        a.move_vel = Vec3::ZERO;
        p.step_tilt = 0.0;
        // Only a real lock-on is env(1118) IsLockedOn (HKS _fireGroundStep 5280-5300): the
        // attack's auto-aim homing is not, so a dodge out of an unlocked swing is the free step
        // (round 31: it took the locked 4-direction branch toward the auto-aim enemy).
        let lock_dir = lock.target.and(target_dir);
        let state = match (lock_dir, stick != Vec3::ZERO) {
            (Some(_), true) => {
                // No snap to the target: KickAngle is the stick against the facing as it is.
                let local = Quat::from_rotation_y(-a.yaw) * stick;
                // HKS _set4DirStepDir (PRM_4DIR_STEP_STICK_RANGE: 45-degree quadrants) and
                // _set4DirStepTilt: the step is turned by the stick's offset from the quadrant
                // centre (none within +-18.75 of forward), so it goes along the stick.
                let angle = local.x.atan2(-local.z).to_degrees();
                let (state, centre) = if angle.abs() < 45.0 {
                    ("GroundStep_F", 0.0)
                } else if angle > -135.0 && angle < -45.0 {
                    ("GroundStep_L", -90.0)
                } else if (45.0..135.0).contains(&angle) {
                    ("GroundStep_R", 90.0)
                } else {
                    ("GroundStep_B", if angle > 0.0 { 180.0 } else { -180.0 })
                };
                let tilt = if state == "GroundStep_F" && angle.abs() < 18.75 { 0.0 } else { angle - centre };
                p.step_tilt = (-tilt).to_radians();
                state
            }
            (Some(_), false) => "GroundStep_N",
            (None, true) => {
                a.yaw = yaw_of(stick);
                "GroundStep_F"
            }
            (None, false) => "GroundStep_N",
        };
        a.play_state(d, state);
        return;
    }

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
    if p.requests.remove(&Action::SwitchTool).is_some() && config.player.prosthetics.len() > 1 && !switching && !sp_ref_active(d, a, 316) && !p.expand_pending {
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

    if p.requests.contains_key(&Action::CombatArt) && accepts(d, a, Action::CombatArt) && art_equipped {
        p.requests.remove(&Action::CombatArt);
        // AttackAngle is measured from the facing before the art turns Wolf.
        let facing = a.forward();
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        aim(a);
        a.move_vel = Vec3::ZERO;
        // Pressed again inside the art's combo window: its next state, in the art's anim group.
        if let Some((state, key)) = art_combo_pick(d, a, &combat, &config, p.emblems) {
            a.play(state, &key);
            return;
        }
        p.art_enable_jump = p.emblems >= art_cost(&combat, &config);
        // Opening state by art type (sprint / step variants), in the art's anim group.
        let start = art_kind(&combat, &config).and_then(|(cat, unlock)| {
            art_start_states(cat, unlock, sprint_window(d, a), stick, facing, p.emblems >= art_cost(&combat, &config)).into_iter().find_map(|state| {
                let id = art_state_id(d, &state)?;
                let key = format!("a{cat:03}_{id}");
                d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
            })
        });
        match start {
            Some((state, key)) => {
                // Sakura Dance's leap from the ground counts as its air use (HKS 3413 / 3445:
                // SP_ATK_TYPE_110 -> g_airSpecialAttackCount + 1, the NoResource leap too); before,
                // only the air branch counted and a second one was taken in the leap (round 53).
                if art_kind(&combat, &config).is_some_and(|(cat, _)| cat == 110) && state.contains("JumpReady") {
                    p.air_art_count += 1;
                }
                a.play(&state, &key)
            }
            None => a.play("GroundSpecialAttackCombo1", art.as_deref().unwrap_or(ART_WHIRLWIND)),
        }
        return;
    }

    // Hold art (spAtkType 104): GroundSpacialAttackHoldStart -> HoldLoop while the art is held.
    // Letting go of attack = BEH_A_GROUND_SP_ATTACK_RELEASE: HoldAction with SP_EF_REF_WEP_SP_ATK_
    // UNLOCK_104_VARIATION_ATTACK (287, art 6100) up, else GroundSpecialAttackCombo1 (the draw
    // slash); letting go of guard = BEH_A_GROUND_SP_ATTACK_GUARD_RELEASE: HoldEnd.
    if matches!(a.state.as_str(), "GroundSpacialAttackHoldStart" | "GroundSpacialAttackHoldLoop" | "SprintSpecialAttackHoldStart") {
        if let Some((cat, unlock)) = art_kind(&combat, &config) {
            // HKS 3476-3485: without the emblems (env 3035) the *NoResource draw.
            let enable = p.emblems >= art_cost(&combat, &config);
            // Each only while its TAE window is up (c0000_transition.lua 6198-6209, checked in
            // that order every tick): the draw with 222 and attack up; HoldEnd with 230 and
            // attack or guard up. Out of them the hold goes on, the buttons re-read next tick.
            let release = !pad.attack_held && d.has_ref(&a.anim, a.t, REF_SP_ATK_RELEASE);
            let hold_release = (!pad.attack_held || !pad.guard_held) && d.has_ref(&a.anim, a.t, REF_SP_ATK_HOLD_RELEASE);
            let next = if release {
                Some(match (unlock == 287, enable) {
                    (true, true) => "GroundSpacialAttackHoldAction",
                    (true, false) => "GroundSpacialAttackHoldActionNoResource",
                    (false, true) => "GroundSpecialAttackCombo1",
                    (false, false) => "GroundSpecialAttackCombo1NoResource",
                })
            } else if hold_release {
                Some("GroundSpacialAttackHoldEnd")
            } else if ended {
                Some("GroundSpacialAttackHoldLoop")
            } else {
                None
            };
            if let Some(state) = next {
                if let Some(key) = d.states.get(state).and_then(|k| k.rsplit('_').next()).map(|id| format!("a{cat:03}_{id}")) {
                    if state != "GroundSpacialAttackHoldEnd" {
                        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
                    }
                    a.play(state, &key);
                }
            }
            return;
        }
    }

    // Art release (HKS BEH_A_GROUND_SP_ATTACK_RELEASE): Combo1 -> Combo1Release, Combo2 ->
    // Combo2Release, SprintSpecialAttack -> SprintSpecialAttackRelease, in the art's anim group.
    // The HKS also forces the release without the art's unlock: base Mortal Draw (106 without
    // ref 283 UNLOCK_106_CHARGE) and base Praying Strikes (108 without 285 UNLOCK_108_FINISH)
    // never hold into Combo1 (c0000_transition.lua BEH_A_GROUND_SP_ATTACK_RELEASE; round 24).
    let forced = matches!(art_kind(&combat, &config), Some((106, u)) if u != 283) || matches!(art_kind(&combat, &config), Some((108, u)) if u != 285);
    if (!pad.attack_held || forced) && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_SP_ATK_RELEASE) {
        let rel = match a.state.as_str() {
            "GroundSpecialAttackCombo1" => Some("GroundSpecialAttackCombo1Release"),
            "GroundSpecialAttackCombo2" => Some("GroundSpecialAttackCombo2Release"),
            "SprintSpecialAttack" => Some("SprintSpecialAttackRelease"),
            "GroundSpecialAttackCombo1NoResource" => Some("GroundSpecialAttackCombo1ReleaseNoResource"),
            "SprintSpecialAttackNoResource" => Some("SprintSpecialAttackReleaseNoResource"),
            _ => None,
        };
        let next = rel.zip(art_kind(&combat, &config)).and_then(|(state, (cat, _))| {
            let id = d.states.get(state)?.rsplit('_').next()?.to_string();
            let key = format!("a{cat:03}_{id}");
            d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
        });
        if let Some((state, key)) = next {
            a.play(state, &key);
            return;
        }
    }

    // Attack: routed by the combo refs active in the current anim.
    if p.requests.contains_key(&Action::Attack) && accepts(d, a, Action::Attack) {
        p.requests.remove(&Action::Attack);
        // BEH_A_GROUND_ATTACK tries _FireSpAttackCombo first (HKS 3256 / 5367): the attack
        // button alone continues an art in its ref 223 window (round 62: it was routed as a
        // plain slash, e.g. Ichimonji: Double's second cut became GroundAttackCombo1Reverse).
        // A combo not unlocked returns FALSE and the plain route goes on.
        if let Some((state, key)) = art_combo_pick(d, a, &combat, &config, p.emblems) {
            start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
            aim(a);
            a.move_vel = Vec3::ZERO;
            a.play(state, &key);
            return;
        }
        let next = attack_route(d, a, stick, art_equipped);
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        aim(a);
        a.move_vel = Vec3::ZERO;
        if !a.play_state(d, &next) {
            a.play_state(d, "GroundAttackCombo1");
        }
        return;
    }

    // Release attack: let go of attack while ref 208 is up (otherwise the hold becomes the charged thrust).
    if !pad.attack_held && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_RELEASE_ATTACK) {
        if let Some(r) = release_of(&a.state).filter(|r| d.anim_key(r).is_some()) {
            // The release clip starts from frame 0: its AI notify (flag 63) is at frame 3 and its
            // hit at frames 7-11, so a quick slash lands 6 + 7 frames after the press.
            a.play_state(d, &r);
            return;
        }
    }

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
        let pressed = p.requests.contains_key(&Action::Prosthetic) && accepts(d, a, Action::Prosthetic);
        let in_sub = d.anim_key(&a.state).is_some_and(|k| k.starts_with("a070")) || a.anim.get(..4).is_some_and(|g| g.starts_with("a07"));
        let held_press = in_sub && hks::held_press(cat, held, enable_action, &refs);
        // Refused, the press stays buffered as the HKS's failed validate leaves it.
        if (pressed || held_press) && hks::flame_gate(cat, &refs) {
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
            if !(held_press && !pressed && state == a.state) {
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
        // The attack button out of a tool move (Fang and Blade etc.): the tool's follow-up.
        if in_sub && !a.anim.is_empty() && p.requests.contains_key(&Action::Attack) {
            let step = (stick != Vec3::ZERO).then(|| {
                let f = a.forward();
                stick.dot(f.cross(Vec3::Y)).atan2(stick.dot(f)).to_degrees()
            });
            let follow = hks::derive_attack(cat, &a.state, a.crouch, p.sub_cat_before == cat, &refs, step)
                .filter(|_| hks::derive_unlock(cat, &a.state, a.crouch).is_none_or(|u| crate::combat::action_unlocked(&combat, &config, u)));
            if let Some((state, k)) = follow.and_then(|s| sub_anim(d, s, cat).map(|k| (s, k))) {
                p.requests.remove(&Action::Attack);
                aim(a);
                a.move_vel = Vec3::ZERO;
                a.play(state, &k);
                return;
            }
        }
        // Let go inside SP_EF_REF_TAE_ENABLE_SUB_ATTACK_RELEASE (302, e.g. SpEffect 100343: Shuriken
        // f9-18, Firecracker f6-15), or a level-1 tool (resident ref 314: always the quick use).
        if in_sub && !a.anim.is_empty() && hks::releases(&refs, held) {
            if let Some(state) = hks::release(cat, &a.state, a.crouch, stick != Vec3::ZERO, lock.target.is_some()) {
                if let Some(k) = sub_anim(d, state, cat).filter(|_| state != a.state) {
                    a.play(state, &k);
                    return;
                }
            }
        }
        // State ends (HKS FireStateEndEvent and the graph's end transitions).
        if ended && in_sub {
            let next = match a.state.as_str() {
                // Line 563: still held -> the flame keeps spewing, else it stops.
                "GroundSubAttackHoldStart" => Some(if held { "GroundSubAttackHoldLoop" } else { "GroundSubAttackHoldEnd" }),
                // The loops (flame, open umbrella) play on while held.
                "GroundSubAttackHoldLoop" if held => Some("GroundSubAttackHoldLoop"),
                // The umbrella open in the air stays open while held (AirSubAttackGuardLoop).
                "AirSubAttackGuardStart" | "AirSubAttackGuardLoop" | "AirSubAttackDeflectEasySmall" => Some(if held { "AirSubAttackGuardLoop" } else { "AirSubAttackGuardEnd" }),
                s if hks::sub_guard(s) && !s.starts_with("GroundSubAttackGuardMoveLoop") => Some(if held { "GroundSubAttackGuardLoop" } else { "GroundSubAttackGuardEnd" }),
                // AirSubAttackStart's graph end transition (SubAttack_SM ev 300) is
                // AirSubAttackLoop: the Firecracker thrown in the air falls on in 403100. The loop
                // (animeEndEventType 3, none) plays on until the landing.
                "AirSubAttackStart" | "AirSubAttackLoop" => Some("AirSubAttackLoop"),
                // Line 536: the Mist Raven's leap goes the stick's way (in the air: line 552).
                "SubAttackJumpReady" | "SubAttackJumpPronReady_F" | "SubAttackJumpPronReady_B" | "AirSubAttackMoveReady" => {
                    let angle = (stick != Vec3::ZERO).then(|| {
                        let f = a.forward();
                        let right = f.cross(Vec3::Y);
                        stick.dot(right).atan2(stick.dot(f)).to_degrees()
                    });
                    Some(if a.state == "AirSubAttackMoveReady" { hks::air_move_start(angle) } else { hks::jump_start(angle) })
                }
                // Line 557: on the floor (env 248) -> W_LandAirSubAttackMove, else
                // W_AirSubAttackMoveStartToLoop. The flat leaps (404011-404017, 5 m along the
                // floor) end on it; the upward one (V 404010: SetNoGravity f0-13, its root rises
                // 3 m) ends in the air and falls.
                s if s.starts_with("SubAttackJumpStart_") || s.starts_with("AirSubAttackMoveStart_") => {
                    if tf.translation.y > crate::map::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT + 0.05 {
                        a.airborne = true;
                        a.vel_y = 0.0;
                        a.air_base = Vec3::ZERO;
                        Some("AirSubAttackMoveStartToLoop")
                    } else {
                        Some("LandAirSubAttackMove")
                    }
                }
                // The graph's end transition: the fall loop plays on until the landing. gap:
                // 419030's TAE 922 ChrPhysicsVelosityScale (7401: scale 0.5, f1-5) is not run; its
                // 920 (7400: +0.6 m/s up) is.
                "AirSubAttackMoveStartToLoop" | "AirSubAttackMoveLoop" => Some("AirSubAttackMoveLoop"),
                // The air follow-up's dive (413000) goes into its falling loop (413100).
                "AirSubAttackDeriveAttack" | "AirSubAttackDeriveAttackLoop" => Some("AirSubAttackDeriveAttackLoop"),
                _ => None,
            };
            if let Some(k) = next.and_then(|s| sub_anim(d, s, cat).map(|k| (s, k))) {
                a.play(k.0, &k.1);
                return;
            }
            // The other air tool moves ending in the air fall: their CMSGs' animeEndEventType 0
            // (FireNextStateEvent; exe FUN_1411e4f20, 1411e.c:6587, fires the event of the
            // state's highest-priority TransitionInfo) leads Master_SM's AirSubAttackCombo1/2/3,
            // Combo1Variation, SpecialEffect, GuardEnd, LockOn, MoveAtemiReady and
            // SubAttackFailedAir to FreeFall (sekiro-extract sms). A tool with no 403100 anim
            // falls the same way.
            if next.is_none() && a.airborne && (a.state.starts_with("AirSubAttack") || a.state == "SubAttackFailedAir") && a.play_state(d, "FreeFall") {
                return;
            }
        }
    }

    // BEH_A_DEFLECT_GUARD_START, first branches (c0000_transition.lua:4438-4442):
    // ref 228 TAE_ENABLE_ADD_JUST_DEFLECT (and not 412 ..._GUARD_CANCEL; StandDeflectEasy* frames
    // 0-9) -> W_AddHardDeflectGuard: an additive deflect window (a000_299060, ref 203 frames 0-9)
    // over the block recoil, which keeps playing. Ref 503 TAE_ENABLE_HIT_DEFLECT_CANCEL
    // (StandDamageMiddle/Large from frame 12) -> StandDeflectGuardFromDamage, its direction the
    // reaction's DamageDirection (hkx selector [0] F [1] B [2] L [3] R).
    // START never runs in guard style (g_behaviorTable row {1,1,0,...}: STYLE_TYPE_GROUND_GUARD
    // takes BEH_A_DEFLECT_GUARD_CONTINUE below), so the easy recoils' ref 228 opens nothing
    // there (round 61: they kept the recoil under an added layer; the game plays
    // StandToDeflectGuard1Reverse). The hard deflects (STYLE_TYPE_STAND) keep it.
    if p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) && !a.anim.is_empty() && !a.airborne && !is_guard_style(&a.state) {
        if d.has_ref(&a.anim, a.t, REF_ADD_JUST_DEFLECT) && !d.has_ref(&a.anim, a.t, REF_ADD_GUARD_CANCEL) {
            if let Some(k) = d.anim_key("AddHardDeflectGuard") {
                p.requests.remove(&Action::Guard);
                a.add_anim = k.to_string();
                a.add_t = 0.0;
                return;
            }
        }
        if d.has_ref(&a.anim, a.t, REF_HIT_DEFLECT_CANCEL) {
            let dir = a.state.rsplit('_').next().filter(|s| matches!(*s, "F" | "B" | "L" | "R")).unwrap_or("F").to_string();
            p.requests.remove(&Action::Guard);
            aim(a);
            a.move_vel = Vec3::ZERO;
            if a.play_state(d, &format!("StandDeflectGuardFromDamage_{dir}")) {
                return;
            }
        }
    }

    // BEH_A_DEFLECT_GUARD_CONTINUE (c0000_transition 4460-4477, validate 6170): a press in a
    // guard-style state (STYLE_TYPE_GROUND_GUARD: the guard starts, the easy recoils,
    // LandAirDeflect*) - the executed press alone, no hold (round 61: a tap let go before the
    // window was dropped there) - steps the spam chain with ref 212 (1 -> 2 -> 3 -> 4; round
    // 61: LandAirDeflectGuard's 212 too), else ref 227's Reverse, else StandToDeflectGuard. Never
    // the sprint deflect (round 61: a press in SprintToDeflectGuard's ref 1 frames restarted it,
    // a fresh 12-frame window and slide, with no spam penalty).
    if is_guard_style(&a.state) && !is_guard_idle(&a.state) && !a.airborne && p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) {
        p.requests.remove(&Action::Guard);
        let has = |r: i64| !a.anim.is_empty() && d.has_ref(&a.anim, a.t, r);
        let next = if has(REF_GUARD_COMBO) {
            match a.state.as_str() {
                "StandToDeflectGuard2" => "StandToDeflectGuard3",
                "StandToDeflectGuard3" | "StandToDeflectGuard4" => "StandToDeflectGuard4",
                _ => "StandToDeflectGuard2",
            }
        } else if has(REF_GUARD_REVERSE) {
            "StandToDeflectGuard1Reverse"
        } else {
            "StandToDeflectGuard"
        };
        aim(a);
        a.move_vel = Vec3::ZERO;
        a.play_state(d, next);
        return;
    }

    // Deflect start: fresh press in a guard window, or holding guard while ref 221 is up.
    // A buffered guard press only fires while guard is still held (live recording 2026-10-08:
    // taps let go before the window opened were dropped out of GroundStep_N, HardDeflectedL,
    // GroundAttackCombo1Release and Combo3; attack presses survive being let go). The spam chain
    // (ref 212) and the additive deflect / guard cancel (228 / 412) above keep released taps.
    // A press made this frame counts as held (the button is still down when it executes).
    let fresh_guard = p.requests.get(&Action::Guard).is_some_and(|&age| age <= dt * 1.5);
    let pressed_guard = p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) && (pad.guard_held || fresh_guard);
    // HKS guard start (c0000_transition.lua:6161): ADD_ACTION_INPUT_GUARD (411, from the additive
    // deflect) together with the base's ADD_ACTION_INPUT_GUARD_CANCEL (412) starts the guard even
    // with the button let go - live: StandDeflectHardSmall tap at f1.5-3.4 -> StandToDeflectGuard f9.4.
    let add_guard = !a.add_anim.is_empty()
        && d.has_ref(&a.add_anim, a.add_t, REF_ADD_INPUT_GUARD)
        && !a.anim.is_empty()
        && d.has_ref(&a.anim, a.t, REF_ADD_GUARD_CANCEL);
    let pressed_guard = pressed_guard || add_guard;
    // The window reached with the tap let go: the game drops it (it stayed queued for good).
    if !pressed_guard && !pad.guard_held && !fresh_guard && p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) {
        p.requests.remove(&Action::Guard);
    }
    let held_guard = pad.guard_held && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_PRESS_GUARD);
    if (pressed_guard || held_guard) && !is_guard_idle(&a.state) && !a.airborne {
        p.requests.remove(&Action::Guard);
        // c0000_transition.lua 4443-4458: W_SprintToDeflectGuard only with ref 1 up, not by the
        // state's name: out of SprintAttack_L/R, a quick turn or SprintStopReady (no ref 1) it is
        // the plain StandToDeflectGuard (0.2 s window, no 3 m slide; round 26).
        let sprinting = sprint_window(d, a);
        let state = if sprinting {
            "SprintToDeflectGuard"
        } else if !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_GUARD_REVERSE) {
            "StandToDeflectGuard1Reverse"
        } else {
            "StandToDeflectGuard"
        };
        // The sprint deflect slides 3.0 m along its facing (a050_203001 root; SetTurnSpeed 180 /
        // 360 deg/s at frames 3-9). Wolf faces the lock-on target (user, from the real game: he
        // deflects toward the enemy), and the slide stops against the enemy's body instead of
        // shoving him (actor::separate). gap: the exe's facing at the guard start (HKS has no
        // _StartAutoAim there) and its proxy push are not traced.
        // gap: HKS BEH_A_DEFLECT_GUARD_START sets no facing (only _StartAutoAim: act 156 stores a
        // homing point, FUN_140b2e090, and turns nothing) and SprintToDeflectGuard's TAE turns at
        // most ~54 deg (no turn f0-3, 180 deg/s f3-6, 360 f6-9). The user's real-game checks say
        // he faces the enemy (2026-10-10; 2026-10-15 "deflect sideways" with the facing kept), so
        // the snap stays until the exe source of that turn is found.
        aim(a);
        a.move_vel = Vec3::ZERO;
        a.play_state(d, state);
        return;
    }

    // Deflect end
    // BEH_A_DEFLECT_GUARD_END (6176) runs in every guard-style state with ref 207 up (round 61:
    // the easy recoils and LandAirDeflect* played on and snapped to idle).
    let can_end = is_guard_idle(&a.state) || (is_guard_style(&a.state) && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_GUARD_CAN_END));
    if can_end && !pad.guard_held {
        // BEH_A_DEFLECT_GUARD_END: idle -> DeflectGuardToStand; moving -> DeflectGuardToStandMove
        // (hkx selector on MoveSpeedIndex: [0] Walk a050_203011, [1] Run a050_203012), an upper-body
        // state over the walk/run (is_upper_action). Ref 220 picks the *Variation ends; no Wolf
        // anim raises it.
        let state = if stick == Vec3::ZERO {
            "DeflectGuardToStand"
        } else if pad.walk {
            "DeflectGuardToStandMoveWalk"
        } else {
            "DeflectGuardToStandMoveRun"
        };
        a.move_index = if pad.walk { 0 } else { 1 };
        if p.speed_level < 1.0 && stick != Vec3::ZERO {
            // The guard walk is the walk speed level.
            p.speed_level = 1.0;
        }
        a.play_state(d, state);
        return;
    }

    // Sprint: hold dodge through the step's ref 15 window, keep holding to keep sprinting.
    let moving = stick != Vec3::ZERO;
    if a.state.starts_with("GroundStep") && d.has_ref(&a.anim, a.t, REF_TRANSITION_SPRINT) && pad.dodge_held && moving {
        a.yaw = yaw_of(stick);
        a.play_state(d, "SprintStartFromStep_F");
        return;
    }
    // HKS BEH_A_SPRINT (6113-6116, then _fireGroundStep at 2761-2771): dodge held while running
    // (ref 91 SP_EF_REF_TAE_MOVING_RUN on the run loop's clip, 100001; not ref 427
    // DISABLE_PRESS_STEP, speed level > 0) steps, and the held step goes on into the sprint above.
    // Holding dodge and then pushing the stick only ran (round 31).
    if a.state == "Locomotion" && pad.dodge_held && moving && p.speed_level > 0.0 && !p.requests.contains_key(&Action::Step) {
        let (key, t) = a.shown_clip(d);
        if d.has_ref(&key, t, SP_REF_TAE_MOVING_RUN) && !d.has_ref(&key, t, SP_REF_TAE_DISABLE_PRESS_STEP) {
            p.requests.insert(Action::Step, 0.0);
        }
    }
    // Sprint quick turn (HKS BEH_A_SPRINT_QUICK_TURN): SprintQuickTurnReady (1 frame) picks
    // Right/Left180 by the side the stick points to; the clip's root motion turns Wolf around.
    if a.state == "SprintQuickTurnReady" {
        let side = if stick.dot(a.forward().cross(Vec3::Y)) > 0.0 { "SprintQuickTurnRight180" } else { "SprintQuickTurnLeft180" };
        a.play_state(d, side);
        return;
    }
    if a.state.starts_with("SprintQuickTurn") && ended {
        a.play_state(d, if pad.dodge_held && moving { "SprintLoop" } else { "SprintStopReady" });
        return;
    }
    if a.state == "SprintStartFromStep_F" || a.state == "SprintLoop" {
        if !(pad.dodge_held && moving) {
            a.play_state(d, "SprintStopReady");
            return;
        }
        // Stick more than SPRINT_BRAKE_ANGLE (135 deg) from the facing while the TAE allows it
        // (SpEffect ref 2 SP_EF_REF_TAE_ENABLE_SPRINT_QUICK_TURN), not locked on.
        if lock.target.is_none()
            && !a.anim.is_empty()
            && d.has_ref(&a.anim, a.t, REF_ENABLE_SPRINT_QUICK_TURN)
            && a.forward().angle_between(stick).to_degrees() > SPRINT_BRAKE_ANGLE
            && a.play_state(d, "SprintQuickTurnReady")
        {
            return;
        }
        a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, lock.target.is_some()).to_radians() * dt);
        if ended {
            a.play_state(d, "SprintLoop");
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

    // Jumps: Ready -> Start (TAE 920 launches) -> Fall loop -> land. The storm jumps'
    // GroundStormJump(Weak|Back)Ready likewise into their Start (TAE 922 row 980 / 981 / 982
    // launches).
    if ended && a.state.starts_with("GroundStormJump") && a.state.ends_with("Ready") {
        let start = a.state.replace("Ready", "Start");
        a.play_state(d, &start);
        return;
    }
    if ended && a.state == "SprintJumpReady" {
        a.play_state(d, "SprintJumpStart");
        return;
    }
    if ended && a.state.contains("GroundJumpReady") {
        // Side jumps have no behaviour-graph clip (CMSG anim 0): their raw anims a000_20111x.
        match a.state.as_str() {
            "LeftsideGroundJumpReady" => a.play("LeftsideGroundJumpStart", "a000_201112"),
            "RightsideGroundJumpReady" => a.play("RightsideGroundJumpStart", "a000_201113"),
            _ => {
                let start = a.state.replace("Ready", "Start");
                a.play_state(d, &start);
            }
        }
        return;
    }
    p.air_top = if a.airborne { p.air_top.max(tf.translation.y) } else { tf.translation.y };
    if a.airborne {
        // Air actions open on the jump/air anims' own cancel flags. The fall loops (FreeFall,
        // *GroundJumpFall: STATE_TYPE_STANDBY in g_paramHkbState) carry none and take every input,
        // as the standby states on the ground (`is_free`).
        let air_ok = |a: &Actor, f: i64| a.state == "FreeFall" || a.state.ends_with("GroundJumpFall") || (!a.anim.is_empty() && d.flag(&a.anim, a.t, f));
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
        } else if p.requests.contains_key(&Action::Jump)
            && (air_ok(a, FLAG_ACCEPT_JUMP) || storm_jump(d, a, p, false).is_some_and(|_| wolf_ref(d, a, p, SP_REF_ENABLE_STORM_JUMP)))
        {
            // BEH_A_AIR_STORM_JUMP first (validated before the kick, HKS g_behaviorValidateOrder):
            // ref 101 up and inside an updraft -> W_AirStormJump(Weak)Start.
            // BEH_A_AIR_KICK: a jump press in the air always kicks (AirKick, a000_213100) unless
            // an SpEffect with behaviorRefId 108 (SP_EF_REF_DISABLE_AIR_KICK) is active. A press
            // before the execute flag 119 (AirComboAttack frame 12, latched by 87 from frame 3)
            // stays pending until it opens, as the air attack's does; it was dropped.
            p.requests.remove(&Action::Jump);
            if let Some(state) = storm_jump(d, a, p, false).filter(|_| wolf_ref(d, a, p, SP_REF_ENABLE_STORM_JUMP)).filter(|s| a.play_state(d, s)) {
                log.push(format!("storm jump ({state})"), Color::srgb(0.7, 0.9, 1.0));
            } else if air_ok(a, FLAG_ACCEPT_JUMP) && !sp_ref_active(d, a, SP_REF_DISABLE_AIR_KICK) && !sp_ref_active(d, a, SP_REF_TAE_ONLY_ADD_ACTION_INPUT_JUMP) {
                a.play_state(d, "AirKick");
            }
            // HKS 2814-2823: under ref 431 (SP_EF_REF_TAE_ONLY_ADD_ACTION_INPUT_JUMP: SpEffect
            // 100247, AirKick frames 12-21) the press only fires W_AddActionInputJump, an additive
            // layer, and no new kick (it chained kicks). gap: that additive clip is not played.
        } else if p.requests.contains_key(&Action::CombatArt) && air_ok(a, FLAG_ACCEPT_ATTACK) && art_equipped && p.wire.is_none() {
            // BEH_A_AIR_SP_ATTACK (`air_art_state`), in the art's group (a1xx_3162xx).
            p.requests.remove(&Action::CombatArt);
            if let Some((cat, unlock)) = art_kind(&combat, &config) {
                let enable = p.emblems >= art_cost(&combat, &config);
                p.art_enable_jump = enable;
                let picked = air_art_state(cat, unlock, enable, p.air_art_count, sp_ref_active(d, a, SP_REF_SP_ATK_HIT_JUMP_DERIVE_ACTION))
                    .filter(|_| crate::combat::action_unlocked(&combat, &config, crate::combat::UNLOCK_AIR_SP_ATTACK));
                match picked.map(|(state, counted)| (state, counted, art_key(d, cat, state))) {
                    Some((state, counted, Some(k))) => {
                        p.air_art_count += counted as u32;
                        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
                        a.play(state, &k);
                    }
                    // The *NoResource air clips ship for 103 / 110 only (a103 / a110 316201, the
                    // art's clip with its own TAE events); the other arts do nothing then.
                    Some((_, _, None)) if !enable => log.push("no spirit emblems", Color::srgb(0.7, 0.7, 0.7)),
                    _ => {}
                }
            }
        } else if p.requests.contains_key(&Action::Attack)
            && (air_ok(a, FLAG_ACCEPT_ATTACK) || (a.state.starts_with("AirKickEnemyJumpStart") && air_ok(a, FLAG_BUFFER_ALL)))
            && start_plunge(p, a, &combat, tf.translation, &mut enemies, &mut log)
        {
            // Out of a head-kick jump the kick-down throw is checked from its buffer flag 87
            // (frame 9), not the air attack's 115 (frame 21): live 0.33 s into 213115.
            p.requests.remove(&Action::Attack);
        } else if let Some(k) = (p.requests.contains_key(&Action::Attack)
            && air_ok(a, FLAG_ACCEPT_ATTACK)
            && matches!(a.state.as_str(), "AirSubAttackMoveStart" | "AirSubAttackMoveStartToLoop" | "AirSubAttackMoveLoop")
            && d.has_ref(&a.anim, a.t, REF_SUB_ATTACK_DERIVE_ATTACK)
            && crate::combat::action_unlocked(&combat, &config, crate::combat::UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_2))
            .then(|| equipped_tool(&combat, &config, p.tool_slot).filter(|t| t.group == 74))
            .flatten()
            .and_then(|_| sub_anim(d, "AirSubAttackDeriveAttack", 74))
        {
            // HKS 2890: the attack button in the Mist Raven's fall while SP_EF_REF_TAE_ENABLE_
            // SUB_ATTACK_DERIVE_ATTACK (301: 100342, 419030 f5-20, all of 419031) is up ->
            // W_AirSubAttackDeriveAttack (a074_413000), with ACTION_UNLOCK_TYPE_SUB_ATTACK_
            // DIRAVE_ATTACK_2 (Fang and Blade).
            p.requests.remove(&Action::Attack);
            a.play("AirSubAttackDeriveAttack", &k);
        } else if let Some(k) = (p.requests.contains_key(&Action::Attack)
            && air_ok(a, FLAG_ACCEPT_ATTACK)
            && sp_ref_active(d, a, SP_REF_SP_ATK_HIT_JUMP_DERIVE_ACTION))
            .then(|| art_kind(&combat, &config).filter(|&(c, unlock)| c == 109 && unlock == SP_REF_UNLOCK_109_FALL_ATTACK))
            .flatten()
            .and_then(|(c, _)| art_key(d, c, "GroundSpecialAttackHitJumpDeriveAction"))
        {
            // HKS 2893 (attack) / 2928 (the art): Shadowfall's spin-slash down out of the hit jump
            // while SP_EF_REF_TAE_ENABLE_SP_ATK_HIT_JUMP_DERIVE_ACTION (226: 100267, a109_316600
            // f24-39) and its unlock ref 286 are up -> W_GroundSpecialAttackHitJumpDeriveAction
            // (a109_316650).
            p.requests.remove(&Action::Attack);
            p.requests.remove(&Action::Guard);
            a.play("GroundSpecialAttackHitJumpDeriveAction", &k);
        } else if let Some((id, _)) = a.electro.filter(|_| p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK)) {
            // HKS 2884: charged (SP_EF_REF_ELECTRO_CHARGE 151 = 9495, SP_EF_REF_WEAK_ELECTRO_CHARGE
            // 356 = 9490) the air attack throws the lightning back: W_AirElectroReceiveAttack
            // (a050_308900: the bolt, BulletBehavior_Midair judge 184 -> Bullet 500184) /
            // W_AirWeakElectroReceiveAttack (a050_308910: AttackBehavior 280 / 281). Their
            // SpEffects 9505 / 9506 (f3-9) discharge him.
            p.requests.remove(&Action::Attack);
            a.play_state(d, if id == 9495 { "AirElectroReceiveAttack" } else { "AirWeakElectroReceiveAttack" });
        } else if p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK) {
            p.requests.remove(&Action::Attack);
            // BEH_A_AIR_ATTACK: refs 214/215/216 pick AirComboAttack1/2/3 (1 -> 2 -> 3 -> 2 ...).
            let has = |r: i64| !a.anim.is_empty() && d.has_ref(&a.anim, a.t, r);
            let next = if has(215) {
                "AirComboAttack2"
            } else if has(216) {
                "AirComboAttack3"
            } else {
                "AirComboAttack1"
            };
            a.play_state(d, next);
        } else if p.requests.contains_key(&Action::Prosthetic) && air_ok(a, FLAG_ACCEPT_PROSTHETIC) {
            // BEH_A_AIR_SUB_ATTACK (HKS 3007, `hks::air_press`): the tool's air move.
            p.requests.remove(&Action::Prosthetic);
            if let Some(tool) = equipped_tool(&combat, &config, p.tool_slot) {
                let resident_ref = d.sp_effects.get(&tool.resident.to_string()).map_or(0, |s| s.behavior_ref_id);
                let timed_refs: Vec<i64> = p.timed.iter().filter_map(|(id, _)| d.sp_effects.get(&id.to_string())).map(|s| s.behavior_ref_id).collect();
                let picked = {
                    let refs = |r: i64| (resident_ref == r && r != 0) || sp_ref_active(d, a, r) || timed_refs.contains(&r);
                    crate::prosthetic::hks::air_press(tool.group, p.emblems >= tool.emblems.max(1), p.air_sub_count, lock.target.is_some(), &refs)
                        .filter(|_| crate::combat::action_unlocked(&combat, &config, crate::combat::UNLOCK_AIR_SUB_ATTACK))
                };
                if let Some((state, k, counted)) = picked.and_then(|(s, c)| sub_anim(d, s, tool.group).map(|k| (s, k, c))) {
                    if state == "SubAttackFailedAir" {
                        log.push("no spirit emblems", Color::srgb(0.7, 0.7, 0.7));
                    }
                    p.air_sub_count += counted as u32;
                    p.sub_cat_before = tool.group;
                    a.play(state, &k);
                }
            }
        } else if p.requests.contains_key(&Action::Guard) && (air_ok(a, FLAG_ACCEPT_GUARD) || is_air_guard(&a.state) || !pad.guard_held) {
            // BEH_A_AIR_DEFLECT_START (HKS 6061): ACTION_UNLOCK_TYPE_AIR_DEFLECT_GUARD. A press
            // before the execute flag (117: AirComboAttack frame 12, AirKick 18) waits for it while
            // held, as the ground guard does; a tap let go before it is dropped.
            p.requests.remove(&Action::Guard);
            if (air_ok(a, FLAG_ACCEPT_GUARD) || is_air_guard(&a.state)) && crate::combat::action_unlocked(&combat, &config, crate::combat::UNLOCK_AIR_DEFLECT_GUARD) {
                // HKS 2968-2970: ref 228 without 412 (the air recoils AirDeflectEasy* / Hard*)
                // adds AddHardDeflectGuard and the recoil plays on, as on the ground (round 61:
                // it was cut into AirDeflectGuardStart, changing the landing).
                let add = !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ADD_JUST_DEFLECT) && !d.has_ref(&a.anim, a.t, REF_ADD_GUARD_CANCEL);
                match d.anim_key("AddHardDeflectGuard").filter(|_| add) {
                    Some(k) => {
                        a.add_anim = k.to_string();
                        a.add_t = 0.0;
                    }
                    None => {
                        a.play_state(d, "AirDeflectGuardStart");
                    }
                }
            }
        }
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
        let air_release = (!pad.attack_held && d.has_ref(&a.anim, a.t, REF_SP_ATK_RELEASE)) || (!pad.guard_held && d.has_ref(&a.anim, a.t, REF_SP_ATK_HOLD_RELEASE));
        if matches!(a.state.as_str(), "AirSpecialAttackHoldStart" | "AirSpecialAttackHoldLoop") && air_release {
            if let Some(k) = art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, "AirSpacialAttackHoldEnd")) {
                a.play("AirSpacialAttackHoldEnd", &k);
            }
        }
        let in_air_guard = is_air_guard(&a.state);
        if a.state == "AirDeflectGuardLoop" && !pad.guard_held {
            a.play_state(d, "AirDeflectGuardEnd");
        }

        let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
        // ChrActionFlag 27 SetNoGravity holds the character in place.
        if !a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_NO_GRAVITY) {
            a.vel_y = 0.0;
            a.grav_y = 0.0;
        } else {
            a.grav_y = ft.gravity();
            a.vel_y += ft.gravity() * dt;
        }
        // One horizontal air velocity (exe FUN_140bac120, fall module). It decays along its
        // direction by HorizontalAcceleration (left alone once slower than one step).
        let speed = a.air_base.length();
        let decel = ft.h_accel * dt;
        if speed > 0.0 && (decel >= 0.0 || -decel <= speed) {
            let step = a.air_base / speed * decel;
            a.air_base += step;
        }
        // Stick: a = StickAcceleration * stick * dt, split along the current velocity (or the
        // stick when still). The across part always steers; the along part brakes in full but
        // only speeds up while under StickAccelerationMaxVelocity, which is all it caps.
        if ft.controllable && stick != Vec3::ZERO {
            let acc = stick * ft.stick_accel * dt;
            let speed = a.air_base.length();
            let dir = if speed > 0.0 { a.air_base / speed } else { stick.normalize() };
            let along = dir.dot(acc);
            let par = dir * along;
            a.air_base += if along <= 0.0 { par } else { dir * along.min((ft.stick_max_vel - speed).max(0.0)) };
            a.air_base += acc - par;
        }
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
                let h = tf.translation.y - CAPSULE_HALF_HEIGHT - crate::map::ground_y(tf.translation);
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
            w.steer(a, tf.translation, dt);
        }
        a.move_vel = a.air_base;
        let mut floor = crate::map::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT;
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
                // pair (ThrowHold). Its hit is a deathblow: 511610 TAE 304 judge 660 = AtkParam_Pc
                // 5000660 "右手刀_忍殺本体_落下" (throwFlag 2, atkPhysCorrection 60000); 13610 ends in
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
            // HKS 1884: the storm jumps and their fall land in W_LandStormJumpFall.
            if (a.state.contains("StormJump") && !a.state.ends_with("Ready")) && a.play_state(d, "LandStormJumpFall") {
                return;
            }
            // Blown away: the matching Land anim (StandDamage*Blow*/Upper*).
            let falling_reaction = |st: &str| {
                st.contains("BlowStart") || st.contains("BlowFallLoop") || st.contains("UpperStart") || st.contains("UpperFallLoop")
                    || (st.starts_with("AirDamage") && (st.contains("Start") || st.contains("FallLoop")))
            };
            // Air posture breaks land into their LandAir versions (W_LandAirDamageBreak /
            // W_LandAirDeflectGuardBreak).
            // Landing mid air-slash while ref 201 (SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION) is
            // up: LandAirComboAttackN continues from the same time (W_LandAirComboAttackN).
            // BEH_R_LAND 1908: from FALL_HEIGHT_LONG_STIFF_LAND (20 m, c0000_define 213; env(224)
            // = the fall height) Wolf lands long-stiff (Selector_LandStiff LONG: LandFreeFall's
            // a000_200021, 2.5 s), after the guard-break / special-attack lands and before the air
            // combo, electro and tool ones. gap: env(224)'s measure in the exe (here: from the
            // flight's highest point); the 3 / 5 m short-stiff lands differ only by the sprint /
            // step out (`_fireGroundStep`), not done.
            let long_fall = p.air_top - tf.translation.y >= FALL_HEIGHT_LONG_STIFF_LAND && d.anim(LAND_LONG_STIFF).is_some();
            // BEH_R_LAND 2026: from 5 m (3 m out of a positioning jump, EnablePositioningJumpLand)
            // the jump's own landings are skipped: stick -> W_StandMoveStartFromFreeFallShortStiff,
            // else _LandFreeFall (Selector_LandStiff NORMAL). Round 38: a jump off a ledge landed
            // as LandVertical / ForwardGroundJump or the positioning land from any height.
            let drop = p.air_top - tf.translation.y;
            let positioning = std::mem::take(&mut p.positioning_land);
            let short_fall = drop >= FALL_HEIGHT_SHORT_STIFF_LAND || (drop >= FALL_HEIGHT_SHORT_STIFF_LAND_FROM_JUMP && positioning);
            if a.state.starts_with("AirComboAttack") && !long_fall && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) {
                let land = format!("Land{}", a.state);
                if a.continue_state(d, &land) {
                    return;
                }
            }
            // Air deflect reactions likewise (HKS W_LandAirDeflect{Easy,Hard}{Small,Large,ExLarge},
            // StartTime = the air anim's time): the land anim is the air one's id + 10. Live
            // (rec_c1010_b_20261009): AirDeflectHardLarge_L -> LandAirDeflectHardLarge_L (132311),
            // AirDeflectHardSmall_R -> LandAirDeflectHardSmall_B (132112); landing after ref 201
            // ended (AirDeflectHard at 0.74 s) is a plain landing.
            if let Some(land) = land_air_deflect(&a.state) {
                if !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) && a.continue_state(d, land) {
                    return;
                }
            }
            // Charged (HKS 1870-1875, before the fall-height rule): landing in the charge states
            // sets it off, W_LandAirDamageElectroCharge(Weak). The HKS goes by the state, so a
            // release that lands before its discharge (9505 / 9506 from 0.1 s) is no shock: it
            // lands into its Land version from the same time while ref 201 is up (HKS 1920-1925).
            // gap: the charge SpEffect is dropped on landing (its own end is not traced).
            let charged = a.electro.take().filter(|_| a.state.starts_with("AirDamageElectroCharge"));
            if let Some((id, _)) = charged {
                let weak = id != 9495;
                // The shock: the charge's chain (lightning_shock; round 43, before a fixed 160 +
                // 10 / 20 %). gap: how the exe holds it off until the landing is not traced.
                lightning_shock(&combat, a, id);
                log.push("SHOCKED - landed charged", Color::srgb(0.6, 0.8, 1.0));
                if a.play_state(d, if weak { "LandAirDamageElectroChargeWeak" } else { "LandAirDamageElectroCharge" }) {
                    return;
                }
            }
            if matches!(a.state.as_str(), "AirElectroReceiveAttack" | "AirWeakElectroReceiveAttack") && !long_fall && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) {
                let land = format!("Land{}", a.state);
                if a.continue_state(d, &land) {
                    return;
                }
            }
            if a.state == "AirDamageBreak" {
                let land = format!("Land{}", a.state);
                if a.play_state(d, &land) {
                    return;
                }
            }
            // The guard breaks (sword and open umbrella) land into their Land versions only while
            // ref 201 is up, from the same time (HKS 1862-1869, W_LandAirDeflectGuardBreak /
            // W_LandAirSubGuardDeflectBreak; SpEffect 100302 at 0-2.0 s); later is a plain landing.
            // The umbrella's clips are the Loaded Umbrella's a076 ones (`sub_anim`).
            // The blinding hit in the air too (HKS 1876-1878, W_LandAirDamageBlinding a000_192611).
            if matches!(a.state.as_str(), "AirDeflectGuardBreak" | "AirSubGuardDeflectBreak" | "AirDamageBlinding")
                && !a.anim.is_empty()
                && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION)
            {
                let land = format!("Land{}", a.state);
                let key = if a.state == "AirSubGuardDeflectBreak" { sub_anim(d, &land, 76) } else { d.anim_key(&land).map(str::to_string) };
                if let Some(k) = key {
                    let t = a.t;
                    a.play(&land, &k);
                    (a.t, a.prev_t) = (t, t);
                    return;
                }
            }
            let blow_land = throw_blow(&a.state)
                .map(|(_, land)| land.to_string())
                .or_else(|| falling_reaction(&a.state).then(|| a.state.replace("FallLoop", "Land").replace("Start", "Land")));
            match blow_land {
                Some(land) if a.play_state(d, &land) => {}
                _ if in_air_guard && pad.guard_held => {
                    a.play_state(d, "LandAirDeflectGuard");
                }
                // c0000_transition.lua BEH_R_LAND: stick held -> straight into moving
                // (W_StandMoveStartFromFreeFallShortStiff); else W_LandGroundJump by jump type
                // (LandVerticalGroundJump a000_201040 / LandForwardGroundJump a000_201045).
                // GroundJumpLandReady (a000_201050) is the pose just before touchdown, not a landing.
                // Directional jumps: W_LandGroundPositioningJump comes before moving on.
                // A free fall lands stiff only without the stick (HKS 2026-2056: stickLevel > 0 ->
                // W_StandMoveStartFromFreeFallShortStiff, the stick arm below; round 31). From 20 m
                // the long-stiff land, stick or not (`long_fall`).
                _ if long_fall => a.play("LandFreeFall", LAND_LONG_STIFF),
                _ if a.state == "FreeFall" && stick == Vec3::ZERO && a.play_state(d, "LandFreeFall") => {}
                // The air tool states land into their Land versions (`hks::air_land`, HKS 1888-1970):
                // while ref 201 is up from the same time, the loops from their start.
                _ if a.state.starts_with("AirSubAttack") && {
                    let cat = equipped_tool(&combat, &config, p.tool_slot).map_or(70, |t| t.group);
                    let ref_201 = !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION);
                    match crate::prosthetic::hks::air_land(cat, &a.state, ref_201).and_then(|(land, keep)| sub_anim(d, land, cat).map(|k| (land, k, keep))) {
                        Some((land, k, keep)) => {
                            let t = if keep { a.t } else { 0.0 };
                            a.play(land, &k);
                            (a.t, a.prev_t) = (t, t);
                            true
                        }
                        None => false,
                    }
                } => {}
                // The jump arts land (HKS 2016-2025): the leap while ref 201 is up into
                // LandGroundSpecialAttackJumpStart from the same time, the fall loop into
                // LandGroundSpecialAttackJumpFallLoop (a107 / a110 316740); the *NoResource lands
                // where the art ships them (a110_316721), else the plain landing.
                _ if a.state.starts_with("GroundSpecialAttackJumpStart") && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) && {
                    let land = format!("LandGroundSpecialAttackJumpStart{}", if a.state.ends_with("NoResource") { "NoResource" } else { "" });
                    match art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, &land)) {
                        Some(k) => {
                            let t = a.t;
                            a.play(&land, &k);
                            (a.t, a.prev_t) = (t, t);
                            true
                        }
                        None => false,
                    }
                } => {}
                _ if a.state.starts_with("GroundSpecialAttackJump") && {
                    let land = format!("LandGroundSpecialAttackJumpFallLoop{}", if a.state.ends_with("NoResource") { "NoResource" } else { "" });
                    match art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, &land)) {
                        Some(k) => {
                            a.play(&land, &k);
                            true
                        }
                        None => stick == Vec3::ZERO && a.play_state(d, "LandFreeFall"),
                    }
                } => {}
                // The air arts land (`air_art_land`).
                _ if (a.state.starts_with("AirSpecialAttack") || a.state.starts_with("AirSpacialAttack")) && {
                    let ref_201 = !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION);
                    let ref_288 = sp_ref_active(d, a, SP_REF_ORIGINAL_LAND_ACTION_SP_ATK_110);
                    let land = art_kind(&combat, &config).and_then(|(c, _)| {
                        let (land, keep) = air_art_land(c, &a.state, ref_201, ref_288)?;
                        Some((land, keep, art_key(d, c, land)?))
                    });
                    match land {
                        Some((land, keep, k)) => {
                            let t = if keep { a.t } else { 0.0 };
                            a.play(land, &k);
                            (a.t, a.prev_t) = (t, t);
                            true
                        }
                        None => stick == Vec3::ZERO && a.play_state(d, "LandFreeFall"),
                    }
                } => {}
                // HKS 2008: the spin-slash while ref 201 is up (100302, f0-45) lands into
                // LandGroundSpecialAttackHitJumpDeriveAction (a109_316660) from the same time; the
                // hit jump itself is not in BEH_R_LAND's list (the plain landing).
                _ if a.state == "GroundSpecialAttackHitJumpDeriveAction" && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) && {
                    match art_kind(&combat, &config).and_then(|(c, _)| art_key(d, c, "LandGroundSpecialAttackHitJumpDeriveAction")) {
                        Some(k) => {
                            let t = a.t;
                            a.play("LandGroundSpecialAttackHitJumpDeriveAction", &k);
                            (a.t, a.prev_t) = (t, t);
                            true
                        }
                        None => false,
                    }
                } => {}
                _ if a.state.starts_with("GroundSpecialAttackHitJump") && stick == Vec3::ZERO && a.play_state(d, "LandFreeFall") => {}
                // The other air tool states (the Mist Raven's fall, the moves out of ref 201) are
                // not in BEH_R_LAND's list: the plain _LandFreeFall.
                _ if (a.state.starts_with("AirSubAttack") || a.state == "SubAttackFailedAir") && stick == Vec3::ZERO && a.play_state(d, "LandFreeFall") => {}
                // The sprint jump too (round 76: SprintJumpStart lasts to the ground, and the 5 m
                // check at 2026 comes before its W_LandGroundJump at 2057).
                _ if short_fall && (a.state.contains("GroundJump") || a.state.starts_with("SprintJump") || a.state == "FreeFall") && {
                    p.jump_land = None;
                    stick == Vec3::ZERO && a.play_state(d, "LandFreeFall")
                } => {}
                _ if p.jump_land.is_some() && a.state.contains("GroundJump") => {
                    let land = p.jump_land.take().unwrap_or("LandGroundPositioningJump_F");
                    a.play_state(d, land);
                }
                _ if stick != Vec3::ZERO => {
                    p.speed_level = if pad.walk { 1.0 } else { 2.0 };
                    a.procedural("Locomotion");
                }
                _ => {
                    a.play_state(d, if p.jump_forward { "LandForwardGroundJump" } else { "LandVerticalGroundJump" });
                }
            }
            return;
        }
        let art_cat = art_kind(&combat, &config).map(|(c, _)| c);
        if ended && a.state.starts_with("GroundSpecialAttackJump") {
            // The jump art's leap ends into its fall loop (316730), which loops until the landing.
            let nr = if a.state.ends_with("NoResource") { "NoResource" } else { "" };
            let fall = format!("GroundSpecialAttackJumpFallLoop{nr}");
            match art_cat.and_then(|c| art_key(d, c, &fall)) {
                Some(k) if a.state != fall => a.play(&fall, &k),
                _ => a.t %= len.max(1e-3),
            }
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
            if a.state == "SprintJumpStart" {
                // No sprint-jump fall CMSG in the behaviour (statemap_c0000.json): its last frame
                // holds until the landing. gap: the graph's blend after 201210 is not traced.
                a.t = len;
            } else if a.state.ends_with("GroundJumpStart") {
                let fall = a.state.replace("Start", "Fall");
                if !a.play_state(d, &fall) {
                    a.t = len;
                }
            } else if a.state.ends_with("GroundJumpFall") || a.state == "FreeFall" {
                a.t %= len.max(1e-3); // loop
            } else if matches!(a.state.as_str(), "AirSpecialAttackStart" | "AirSpecialAttackHoldStart") {
                // The air art's start into its loop (a101 / 107 / 108 / 110 316220, a104's hold).
                let next = a.state.replace("Start", "Loop");
                match art_cat.and_then(|c| art_key(d, c, &next)) {
                    Some(k) => a.play(&next, &k),
                    None => {
                        if !a.play_state(d, "FreeFall") {
                            a.t = len;
                        }
                    }
                }
            } else if matches!(a.state.as_str(), "AirSpecialAttackLoop" | "AirSpecialAttackHoldLoop") {
                a.t %= len.max(1e-3);
            } else if a.state.starts_with("AirSpecialAttack") || a.state.starts_with("AirSpacialAttack") || a.state.starts_with("GroundSpecialAttackHitJump") {
                // STYLE_TYPE_FREE_FALL states: the fall goes on.
                if !a.play_state(d, "FreeFall") {
                    a.t = len;
                }
            } else if (a.state.starts_with("GroundStormJump") || a.state.starts_with("AirStormJump") || a.state == "StormJumpFall") && a.state != "StormJumpFall" {
                // The storm jumps fall on in StormJumpFall (HKS 1884: they land as W_LandStormJumpFall).
                if !a.play_state(d, "StormJumpFall") {
                    a.t = len;
                }
            } else if a.state == "StormJumpFall" {
                a.t %= len.max(1e-3);
            } else if throw_blow(&a.state).is_some() {
                throw_blow_step(a, d);
            } else if let Some(next) = electro_loop(&a.state) {
                // The charge's start into its loop (W_AirDamageElectroCharge(Weak)Loop, the
                // deflects' FallLoop), which loops until he lands or throws it.
                if a.state == next {
                    a.t %= len.max(1e-3);
                } else if !a.play_state(d, &next) {
                    a.t = len;
                }
            } else if a.state.contains("BlowStart") || a.state.contains("UpperStart") || (a.state.starts_with("AirDamage") && a.state.contains("Start")) {
                let fall = a.state.replace("Start", "FallLoop");
                if !a.play_state(d, &fall) {
                    a.t = len;
                }
            } else if a.state.contains("BlowFallLoop") || a.state.contains("UpperFallLoop") || (a.state.starts_with("AirDamage") && a.state.contains("FallLoop")) {
                a.t %= len.max(1e-3);
            } else if matches!(a.state.as_str(), "AirDamageBreak" | "AirDeflectGuardBreak" | "AirSubGuardDeflectBreak") {
                a.t = len; // hold the broken pose until landing
            } else if a.state == "AirDeflectGuardStart" || a.state == "AirDeflectGuardLoop" {
                a.play_state(d, if pad.guard_held { "AirDeflectGuardLoop" } else { "AirDeflectGuardEnd" });
            } else if !a.play_state(d, "ForwardGroundJumpFall") {
                // Air reactions, kick, attack, guard end: fall.
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
    // Anim ended: guard starts and guard reactions return to guard idle if guard is held.
    // Knocked down out of a posture break (behavior graph transitions): BreakDamage and
    // the break blows' landings -> StandDamageBreakDown -> StandDamageLargeDownWakeUp,
    // face up only after a small blow from the front (Selector_ProneDirection).
    if ended {
        // The break small blow's side is the 4-way angle (HKS 1678-1684): face down only from the
        // back sector; a hit from the side lands face up (round 33: any hit behind the shoulder
        // line, its _B state, landed face down).
        let down = if a.state.starts_with("StandDamageBreakSmallBlow") && !a.hit_back_quadrant { "FaceUp" } else { "FaceDown" };
        // Unbroken knockdowns (graph: *BlowLand / LargeUpperLand / LargePound / SmallBlow ->
        // StandDamageLargeDown): prone side per HKS _setProneDir - a small blow from the front
        // lands face up, everything else face down. StandDamageLargeDown plays the get-up clip
        // (a000_10031x, the same one the break chain ends with).
        let normal_down = if a.state == "StandDamageSmallBlow_F" { "FaceUp" } else { "FaceDown" };
        let next = if a.state == "StandDamageBreakDamage"
            || a.state == "StandDamageBreakLargePound"
            || (a.state.starts_with("StandDamageBreak") && (a.state.contains("Land") || a.state.contains("SmallBlow")))
        {
            Some(format!("StandDamageBreakDown_{down}"))
        } else if !a.state.starts_with("StandDamageBreak")
            && (a.state.starts_with("StandDamage") && (a.state.contains("BlowLand") || a.state.contains("UpperLand") || a.state.contains("SmallBlow"))
                || a.state == "StandDamageLargePound")
        {
            Some(format!("StandDamageLargeDown_{normal_down}"))
        } else if (a.state.starts_with("AirDamage") && a.state.contains("Land")) || a.state == "ThrowDefLargeBlowLand600380" {
            // AirDamageLargeLand / LargePoundLand -> StandDamageLargeDown, face down; the Ape's
            // grab landing too (ThrowDefLargeBlowLand600380_to_StandDamageLargeDown).
            Some("StandDamageLargeDown_FaceDown".to_string())
        } else {
            a.state.strip_prefix("StandDamageBreakDown_").map(|dir| format!("StandDamageLargeDownWakeUp_{dir}"))
        };
        if let Some(next) = next {
            if a.play_state(d, &next) {
                return;
            }
        }
    }
    // The slide's end (HKS state-end of HKB_STATE_SPRINT_TO_CROUCH_READY): TurnAngle > 0 ->
    // W_SprintToCrouchLeft, else Right (the stick's angle from the facing, + = left).
    if ended && a.state == "SprintToCrouchReady" {
        let left = stick != Vec3::ZERO && stick.dot(a.forward().cross(Vec3::Y)) < 0.0;
        if a.play_state(d, if left { "SprintToCrouchLeft" } else { "SprintToCrouchRight" }) {
            return;
        }
    }
    if ended {
        let to_guard = (is_guard_start(&a.state) || is_guard_reaction(&a.state) || is_guard_idle(&a.state) || a.state == "LandAirDeflectGuard")
            && pad.guard_held;
        if to_guard {
            a.play_state(d, "DeflectGuardIdle");
        } else if is_upper_action(&a.state) && moving {
            // FireStateEndEvent: an upper action ends into W_StandMoveLoop while moving; the legs'
            // loop keeps its phase (it ran on the action's clock).
            let (t, prev_t) = (a.t, a.prev_t);
            a.procedural("Locomotion");
            (a.t, a.prev_t, a.move_start) = (t, prev_t, 0.0);
        } else {
            a.play("StandIdle", "");
        }
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

    // Movement: free states, or the TAE move window.
    let move_ok = is_free(&a.state) || (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_ACCEPT_MOVE));
    // Moving out of a guard start or a block/deflect reaction with guard held stays in the guard
    // style: HKS BEH_A_GROUND_MOVE_START with STYLE_TYPE_GROUND_GUARD -> W_DeflectGuardMove (not
    // the run - which also stood Wolf still through the reaction's tail before running off).
    if moving && move_ok && pad.guard_held && !a.airborne && (is_guard_start(&a.state) || is_guard_reaction(&a.state)) {
        a.play_state(d, "DeflectGuardIdle");
    }
    // The in-place quick turns (Stand / Crouch / DeflectGuard QuickTurn*, a000_000010-13 and
    // kin) have no root motion and no TAE 224: the turn controller turns Wolf to the target at
    // its default rate (FUN_1407daa20: 720 deg/s when no TAE / module rate is set). Without it
    // the clip played, Wolf never turned, and the >60 deg check replayed it forever (Wolf
    // stepping in place with no input). gap: the exe's turn request for these states.
    if a.state.contains("QuickTurn") && !a.state.starts_with("Sprint") && !a.anim.is_empty() && d.root_at(&a.anim, a.t).is_none() {
        if let Some(td) = lock.target.and(target_dir) {
            let rate = d.turn_speed(&a.anim, a.t, true).unwrap_or(config.movement.turn_speed);
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
            // Free guard walk faces the camera's direction (Sekiro strafes while blocking; live:
            // the facing drifts with the camera while walking, and holds while standing - guard
            // idle's TAE SetTurnSpeed 0).
            let cam_fwd = stick_world(Vec2::new(0.0, 1.0), cam_yaw);
            a.yaw = turn_toward(a.yaw, yaw_of(cam_fwd), turn_rate(d, a, &config, false).to_radians() * dt);
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
        lower_body_move(p, a, d, &config, stick, target_dir, pad.walk, dt);
    } else if move_ok && (moving || a.state == "Locomotion") {
        locomotion(p, a, d, &config, stick, target_dir, pad.walk, dt);
    } else if !move_ok {
        a.move_vel = Vec3::ZERO;
        p.speed_level = 0.0;
        // Turning during actions follows TAE SetTurnSpeed unless turning is disabled.
        if !a.anim.is_empty() && !d.flag(&a.anim, a.t, FLAG_DISABLE_TURN) {
            if let Some(speed) = d.turn_speed(&a.anim, a.t, lock.target.is_some()) {
                let want = target_dir.or((stick != Vec3::ZERO).then_some(stick));
                if let Some(w) = want {
                    a.yaw = turn_toward(a.yaw, yaw_of(w), speed.to_radians() * dt);
                }
            }
        }
    }
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
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    // Stick released: the stop anims carry their own slide (root motion), in the move direction
    // (HKS BEH_A_GROUND_MOVE_STOP: W_StandRunStop / W_StandWalkStop by MoveDirection).
    if stick == Vec3::ZERO && a.state == "Locomotion" && p.speed_level > 0.3 {
        let dir = ["F", "B", "L", "R"][a.move_dir.min(3) as usize];
        let run = p.speed_level > 1.2;
        // Crouched: W_CrouchRunStop / W_CrouchWalkStop (CrouchRunStop_F 5600, CrouchWalkStop_F 5300).
        let (stop, fallback) = match (a.crouch, run) {
            (true, true) => (format!("CrouchRunStop_{dir}"), "CrouchRunStop_F"),
            (true, false) => (format!("CrouchWalkStop_{dir}"), "CrouchWalkStop_F"),
            (false, true) => (format!("StandRunStop{dir}"), "StandRunStopF"),
            (false, false) => (format!("StandWalkStop_{dir}"), "StandWalkStop_F"),
        };
        if a.play_state(d, &stop) || a.play_state(d, fallback) {
            a.move_vel = Vec3::ZERO;
            p.speed_level = 0.0;
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
    move_velocity(p, a, d, config, stick, target_dir, dt);
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
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    if stick != Vec3::ZERO {
        a.move_index = if walk_held { 0 } else { 1 };
    }
    move_velocity(p, a, d, config, stick, target_dir, dt);
}

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
fn move_velocity(p: &Player, a: &mut Actor, d: &CharData, config: &GameConfig, stick: Vec3, target_dir: Option<Vec3>, dt: f32) {
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
            if stick != Vec3::ZERO {
                a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, target_dir.is_some()).to_radians() * dt);
            }
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
        // Wolf 4 m over an enemy 10 m below the export origin, falling (heightLimit 3 m).
        let (target, g) = (bevy::math::Vec3::new(0.0, -10.0, 0.0), -9.8 * 3.0);
        assert!(super::fall_reaches(target + bevy::math::Vec3::new(0.0, 4.0, 0.3), -1.0, bevy::math::Vec3::ZERO, g, target, 1.0, 3.0, 2.0));
        // Out of range sideways: no plunge.
        assert!(!super::fall_reaches(target + bevy::math::Vec3::new(3.0, 4.0, 0.0), -1.0, bevy::math::Vec3::ZERO, g, target, 1.0, 3.0, 2.0));
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
