//! Hit resolution: when an AttackBehavior window (TAE 1) is active and the
//! defender is in reach, decide deflect / block / hit, apply posture, and
//! play each side's reaction.
//!
//! Posture damage, from the exe (Ghidra, Steamless v1.06):
//!   attacker (FUN_140842790): unguarded -> directAtkStamDamage_Attacker,
//!     guarded -> repelVictoryStamDamage_Attacker,
//!     deflected -> repelLostStamDamage_Attacker x product of the defender's
//!     defStaminaAttackRate on stateInfo-204 SpEffects (105020-23, the spam penalty)  [confirmed]
//!   defender (FUN_140847d50 + FUN_1408439a0): unguarded -> directAtkStamDamage,
//!     deflected -> atkStam, guarded -> repelLostStamDamage; then x guard cut / attribute /
//!     SpEffect rates (1.0 for these characters, not modelled).
//!   HP: player = weapon attackBasePhysics x atkPhysCorrection / 100; enemy = atkPhys.
//!
//! Deflect = any SpEffect with stateInfo 158 active on the defender (player 105010, NPC 200220).
//!
//! Reactions, from the HKS (c0000 player, c9997 shared NPC):
//!   player guarding: deflect -> StandDeflectHard{Small|Middle|Large}_{L|R}, block ->
//!     StandDeflectEasy{...}; size from the attack's dmgLevel, side from (just)deflectAction
//!     (1 = L, 2 = R). Deflecting at full posture -> HardDeflectStagger (GUARDATTACKER_STAMZERO).
//!   NPC attacker: deflected -> justDeflectedAction 1/2 = AttackBoundEnemy1_Right/Left,
//!     11-14 = additive flinch (keeps attacking); blocked -> deflectedAction 1/2 =
//!     AttackNoBoundEnemy1_Right/Left; posture emptied by a deflect -> AttackBoundEmptyStamina.
//!   NPC guarding: deflect -> JustGuardDamage, block -> GuardDamage{Small|Large}, emptied -> GuardBreak.
//!   player attacker: deflected -> HardDeflected{R|L}, blocked -> EasyDeflected{R|L}.
//!   NPC clash SpEffects (AI inputs, 0.1 s): got deflected 200200/1 (strong) or 200205/6 (weak),
//!     deflected/guarded the player 200210/1 (strong) or 200215/6 (weak).

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, Side, data_for};
use crate::config::GameConfig;
use crate::data::{Attack, Combat, STATE_INFO_JUST_GUARD, TAE_FPS};
use crate::enemy::Enemy;
use crate::hud::CombatLog;
use crate::anim::Hurtboxes;
use crate::model::Dummies;
use crate::player::FLAG_SHIELD_BLOCK;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatLog>()
            .configure_sets(FixedUpdate, (ActorSet::Decide, ActorSet::Advance, ActorSet::Resolve).chain())
            .add_systems(FixedUpdate, (resolve, resolve_throws).chain().in_set(ActorSet::Resolve))
            .init_resource::<ShowHurtboxes>()
            .add_systems(Update, draw_hitboxes.run_if(resource_exists::<GizmoConfigStore>));
    }
}

enum Outcome {
    Deflect { window: f32, at: f32, rate: f32 },
    Block,
    Hit,
}

/// Starts an additive flinch (anim/actor add layer) by state name, if it has an anim.
/// c9997 ExecNoSyncAddDamage (lines 2899-2935): a hit with no reaction flinches additively,
/// unless the chr lacks the "no additive" anim 9999: PartBlend_Add<part> (anim 9510 + part - 1)
/// for a hit on a part group (env(1120): the hit capsule's, `part`), else SABlend_Add_<dir>.
/// The Divine Dragon's PartBlend anims put 3520080 (hand / body) or 3520081 (head) on it, which
/// its map script reads. gap: env(336) IsPartDamageAdditiveBlendInvalid is not traced.
pub(crate) fn no_sync_add_damage(d: &crate::data::CharData, a: &mut Actor, part: u8, from: Vec3, at: Vec3) {
    let exists = |id: i64| d.anim(&format!("a000_{id:06}")).is_some() || (a.anim_group > 0 && d.anim(&format!("a{:03}_{id:06}", a.anim_group * 100)).is_some());
    if !exists(9999) {
        return;
    }
    if part > 0 && exists(9510 + part as i64 - 1) {
        start_additive(d, a, &format!("PartBlend_Add{part:02}"));
        return;
    }
    let fwd = a.forward();
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
    let dir = if f.abs() >= r.abs() { if f >= 0.0 { "Front" } else { "Back" } } else if r > 0.0 { "Right" } else { "Left" };
    start_additive(d, a, &format!("SABlend_Add_{dir}"));
}

fn start_additive(d: &crate::data::CharData, a: &mut Actor, state: &str) {
    if let Some(k) = d.anim_key(state) {
        a.add_anim = k.to_string();
        a.add_t = 0.0;
    }
}

/// A hit on the open umbrella: the additive flinch (minimum levels, `additive_guard`) or the
/// umbrella's guard reaction, in the umbrella's anims (a076).
fn umbrella_reaction(d: &crate::data::CharData, a: &mut Actor, level: i64, just: bool, attacker_broken: bool, from: Vec3, at: Vec3) {
    use crate::prosthetic::hks;
    // In the air (HKS 657-664 / 1276): W_AirSubAttackDeflect{Hard,Easy}Add for the additive
    // levels, else W_AirSubAttackDeflectEasySmall.
    if a.airborne {
        let (state, add) = if additive_guard(d, a, level) {
            (if just { "AirSubAttackDeflectHardAdd" } else { "AirSubAttackDeflectEasyAdd" }, true)
        } else {
            ("AirSubAttackDeflectEasySmall", false)
        };
        if let Some(k) = crate::player::sub_anim(d, state, 76) {
            if add {
                a.add_anim = k;
                a.add_t = 0.0;
            } else {
                a.play(state, &k);
            }
        }
        return;
    }
    if additive_guard(d, a, level) {
        if let Some(k) = crate::player::sub_anim(d, hks::umbrella_add(just, attacker_broken), 76) {
            a.add_anim = k;
            a.add_t = 0.0;
        }
        return;
    }
    let behind = a.forward().dot((from - at).with_y(0.0).normalize_or_zero()) < 0.0;
    if let Some(s) = hks::umbrella_guard(level, behind) {
        if let Some(k) = crate::player::sub_anim(d, s, 76) {
            a.play(s, &k);
        }
    }
}

/// HKS BEH_ADD_R_GUARD_DAMAGE: a DAMAGE_LEVEL_MINIMUM (8) guard or deflect, or Small / Middle /
/// Large / Push (1, 2, 3, 5) while ref 202 SP_EF_REF_TAE_GUARD_LEVEL_EXCHANGE_MINIMUM is up
/// (SprintToDeflectGuard frames 0-12), is an additive flinch (StandDeflect{Easy,Hard}Minimum,
/// AirDeflect*MinimumAdd via FireEventNoReset): Wolf keeps his current state.
fn additive_guard(d: &crate::data::CharData, a: &Actor, level: i64) -> bool {
    level == 8 || (matches!(level, 1 | 2 | 3 | 5) && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, 202))
}

/// A fair coin for HKS `math.random() > 0.5` picks (xorshift; deterministic per run).
fn coin_flip() -> bool {
    use std::sync::atomic::{AtomicU32, Ordering};
    static STATE: AtomicU32 = AtomicU32::new(0x9E37_79B9);
    let mut x = STATE.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    STATE.store(x, Ordering::Relaxed);
    x & 1 == 1
}

/// HKS damage level -> guard reaction size (c0000_transition.lua deflect selectors).
fn guard_size(level: i64) -> &'static str {
    match level {
        3 | 7 => "Middle",
        4 | 6 | 9 => "Large",
        8 => "Minimum",
        10 | 11 => "ExLarge",
        _ => "Small",
    }
}

/// Pushes the defender away from the attacker: AtkParam distance, cut by the NPC's
/// NpcParam knockbackRate_vsPlayer_* [%], timed by the defender's KnockBackParam row:
/// NpcParam knockbackParamId for NPCs (0 for all 1279 rows), the armour's
/// EquipParamProtector knockbackParamId for Wolf (1 on every outfit piece; its
/// knockBackCutRate_* are 0).
/// Returns the distance the defender's knockback cut took off (FUN_140b6bfe0: knockback distance
/// minus the defender's push), which `recoil` hands back to the attacker.
/// The distance is first scaled by how square the hit is to the attacker's facing
/// (`knockback_facing`); `attacker_forward` None (a prosthetic bullet) leaves it whole.
pub(crate) fn apply_knockback(combat: &Combat, da: &mut Actor, from: Vec3, at: Vec3, attacker_forward: Option<Vec3>, dist: f32, kind: &str, cut_field: &str) -> f32 {
    let dist = dist * attacker_forward.map_or(1.0, |f| knockback_facing(at - from, f));
    let row_id = match da.side {
        Side::Enemy => combat.npc(da)["knockbackParamId"].as_i64().unwrap_or(0),
        Side::Player => combat
            .params
            .get("EquipParamProtector")
            .and_then(|v| v.as_object())
            .and_then(|rows| rows.values().filter_map(|r| r["knockbackParamId"].as_i64()).max())
            .unwrap_or(1),
    };
    let row = combat.param("KnockBackParam", row_id);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    // TAE 226 SetKnockbackPercent on the defender's own anim: its knockback module +0x40
    // (FUN_140ba34a0, percent / 100) multiplies the push distance at knockback start
    // (FUN_140ba37e0). E.g. Wolf's Combo1 0 % at frames 23-25, Combo2 50 %, later swings 30 %.
    let percent = if da.anim.is_empty() {
        1.0
    } else {
        data_for(combat, &da)
            .events_at(&da.anim, da.t)
            .find(|e| e.kind == 226)
            .and_then(|e| e.arg_i64("KnockbackPercent"))
            .map_or(1.0, |p| p as f32 / 100.0)
    };
    let cut = if da.side == Side::Enemy {
        combat.npc(da)[cut_field].as_f64().unwrap_or(0.0) as f32
    } else {
        0.0
    };
    let d = dist * (1.0 - cut / 100.0) * percent;
    da.knockback(at - from, d, f(&format!("{kind}_ContTime")), f(&format!("{kind}_DecTime")));
    dist * cut / 100.0
}

/// FUN_140b6bfe0 (140b6.c:7699-7729): with an attacker, the push distance is times f * f, f the dot
/// of the normalised knockback direction and the attacker's forward axis (FUN_1409eeb40 ->
/// FUN_1407dcc90, its orientation's forward column), but 0.4 when f < 0 or f * f < 0.4: range
/// [0.4, 1]. The leftover handed to the attacker is taken from the scaled distance too.
/// gap: the exe's direction is the attack source's own movement vector when it moves (|v|^2 >
/// 0.0005), else contact point minus the attack origin (FUN_140b11080); here it is defender root
/// minus attacker root.
pub(crate) fn knockback_facing(dir: Vec3, attacker_forward: Vec3) -> f32 {
    let f = dir.normalize_or_zero().dot(attacker_forward.normalize_or_zero());
    if f < 0.0 || f * f < 0.4 { 0.4 } else { f * f }
}

/// The knockback the defender's cut refused goes to the attacker, pushed straight back, when the
/// attacker's anim has ChrActionFlag 72 "KnockbackValue" on (FUN_140b6bfe0: attacker module
/// +0x70 bit 9 and +0xc1c, set by the flag dispatcher case 0x48 from ArgA; then FUN_140ba3420
/// with the reversed knockback direction and FUN_140ba3190(-1, leftover / ArgA, ArgA): the
/// whole leftover over ArgA seconds). Only Wolf's sword attacks carry it (133 anims, ArgA 0.09);
/// with NpcParam knockbackRate_vsPlayer_* 75 (c1180) or 100 (45 rows) Wolf bounces off.
/// gap: the exe starts it one frame later (+0x45), and skips it for hits with the damage struct's
/// +0x164 set (that path defers it through TAE 226) - neither is modelled.
/// Hit stop on a contact (hit, block or deflect alike: the defender's damage module
/// FUN_140b6f690 -> FUN_140b6e5a0 with no outcome test). Attacker (FUN_140b9cd90, read here):
/// AtkParam hitStopTime times a scale from the defender's NpcParam hitStopType (FUN_140b9d850):
/// 0 none, 1 the engine's weak rate 0.5 (FUN_140a51800 +0x124), 2 full, 3 full only under an
/// attacker action state (not identified: none here; no row uses 3); a player defender is full.
/// Defender (FUN_140b9cfb0): hitStopTime_Defencer when its hitStopType_Defencer is 1, a player
/// always. ChrActionFlag 41 on the defender forces both full (FUN_140b60f10) unless the attack
/// is isArrowAtk. Most enemies are 0 / 0: Wolf's hits on them stop nobody. Rules as AKJama/
/// sekiro-rs docs/COMBAT-RULES.md "Hit stop" (MIT / Apache), checked against FUN_140b9cd90.
/// gap: isArrowAtk is not exported, bullets stand for it; a bullet does not stop its distant
/// shooter (sekiro-rs's choice; every NPC bullet row has 0 anyway).
pub(crate) fn start_hit_stop(combat: &Combat, aa: &mut Actor, da: &mut Actor, dd: &crate::data::CharData, atk: &Attack, bullet: bool) {
    let force = !bullet && !da.anim.is_empty() && dd.flag(&da.anim, da.t, 41);
    let (atk_scale, def_on) = match da.side {
        _ if force => (1.0, true),
        Side::Player => (1.0, true),
        Side::Enemy => {
            let npc = combat.npc(da);
            let scale = match npc["hitStopType"].as_i64().unwrap_or(0) {
                1 => 0.5,
                2 => 1.0,
                _ => 0.0,
            };
            (scale, npc["hitStopType_Defencer"].as_i64() == Some(1))
        }
    };
    if !bullet {
        aa.hit_stop.start(atk.hit_stop_time * atk_scale);
    }
    if def_on {
        da.hit_stop.start(atk.hit_stop_time_defencer);
    }
}

pub(crate) fn recoil(combat: &Combat, aa: &mut Actor, from: Vec3, at: Vec3, leftover: f32) {
    if leftover <= 0.0 || aa.anim.is_empty() {
        return;
    }
    let time = data_for(combat, aa)
        .events_at(&aa.anim, aa.t)
        .filter(|e| e.kind == 0 && e.flag_type() == Some(72))
        .filter_map(|e| e.args.get("ArgA").and_then(|v| v.as_f64()).map(|v| v as f32))
        .find(|t| *t > 0.0);
    if let Some(time) = time {
        aa.knockback(from - at, leftover, time, 0.0);
    }
}

/// KnockBackParam columns for a guard reaction size.
pub(crate) fn guard_kb(level: i64) -> &'static str {
    match guard_size(level) {
        "Middle" | "Large" => "guard_L",
        "ExLarge" => "guard_LL",
        _ => "guard_S",
    }
}

/// KnockBackParam columns for a damage reaction.
pub(crate) fn damage_kb(level: i64) -> &'static str {
    match level {
        3 | 4 | 6 | 9 | 10 | 11 => "damage_L",
        2 | 7 => "damage_M",
        8 => "damage_Min",
        _ => "damage_S",
    }
}

/// DEFLECT_DIR: 1 = L, 2 = R.
/// HKS Selector_DeflectDir from the attack's (just) deflect direction: DEFLECT_DIR_L 1 -> L,
/// DEFLECT_DIR_R 2 -> R, anything else (0 / NONE) -> F (front).
fn lr(action: i64) -> &'static str {
    match action {
        1 => "L",
        2 => "R",
        _ => "F",
    }
}

/// Player guard reaction state; falls back to Small when a size has no anim for that side.
fn player_guard_state(combat: &Combat, deflect: bool, level: i64, dir: &str, variant: bool, air: bool) -> String {
    let size = guard_size(level);
    let kind = if deflect { "Hard" } else { "Easy" };
    if air {
        // AirDeflect{Hard,Easy}{Small,Large}_{L,R} / ExLarge / plain (minimum).
        let sized = match size {
            "Minimum" => format!("AirDeflect{kind}"),
            "ExLarge" => format!("AirDeflect{kind}ExLarge"),
            "Small" => format!("AirDeflect{kind}Small_{dir}"),
            _ => format!("AirDeflect{kind}Large_{dir}"),
        };
        return [sized, format!("AirDeflect{kind}Small_{dir}"), format!("AirDeflect{kind}")]
            .into_iter()
            .find(|s| combat.player.anim_key(s).is_some())
            .unwrap_or_default();
    }
    let candidates = match (deflect, size) {
        (_, "Minimum") | (_, "ExLarge") => vec![format!("StandDeflect{kind}{size}")],
        (false, "Small") => vec![format!("StandDeflectEasySmall_V{}_{dir}", if variant { 2 } else { 1 })],
        (false, "Middle") => vec![format!("StandDeflectEasyMiddle_V{}_{dir}", if variant { 2 } else { 1 }), format!("StandDeflectEasyLarge_{dir}")],
        _ => vec![format!("StandDeflect{kind}{size}_{dir}")],
    };
    candidates
        .into_iter()
        .chain(std::iter::once(format!("StandDeflect{kind}Small_{dir}")))
        .chain(std::iter::once(format!("StandDeflectEasySmall_V1_{dir}")))
        .find(|s| combat.player.anim_key(s).is_some())
        .unwrap_or_default()
}

/// Is the defender in a TAE 954 IFrames window that covers this attack? Sweeps are the
/// attacks whose anim raises the 980 perilous-sweep warning.
/// `att` None: a bullet (no attacker anim; never a sweep).
pub(crate) fn dodged(combat: &Combat, def: &Actor, att: Option<&Actor>, atk: &Attack) -> bool {
    if def.anim.is_empty() {
        return false;
    }
    let dd = data_for(combat, &def);
    // Mikiri takes precedence: a perilous thrust into the step's MikiriCounter window.
    if atk.atk_type == 2 && atk.disable_guard == 1 && dd.flag(&def.anim, def.t, FLAG_MIKIRI) {
        return false;
    }
    let is_sweep = || att.is_some_and(|att| data_for(combat, att).anim(&att.anim).is_some_and(|a| a.events.iter().any(|e| e.kind == 2 && e.arg_i64("BehaviorJudgeID") == Some(980))));
    // TAE 950 (exe: action module +0x70 |= 2) on every knock-down, launch, death and revival
    // anim, and 951 (|= 1) on the deathblow throws: full invulnerability windows (no juggling).
    if dd.events_at(&def.anim, def.t).any(|e| e.kind == 950 || e.kind == 951) {
        return true;
    }
    dd.events_at(&def.anim, def.t).filter(|e| e.kind == 954).any(|e| {
        let ty = e.args.get("IFrameType").and_then(|v| v.as_str()).and_then(|t| t.split(':').next()).and_then(|n| n.trim().parse::<i64>().ok());
        match ty {
            Some(2) => true,
            Some(6) => atk.atk_type == 2,
            Some(7) => atk.throw_flag == 1,
            Some(1) => is_sweep(),
            _ => false,
        }
    })
}

/// A ThrowParam row's reach from the thrower at `from` to the other at `to`: within Dist, and
/// the other's height within upperYRange above / lowerYRange below (as the plunge start checks).
fn throw_in_reach(combat: &Combat, row_id: i64, th: &crate::data::Throw, from: Vec3, to: Vec3) -> bool {
    let row = combat.param("ThrowParam", row_id);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let dy = to.y - from.y;
    from.distance(to) <= th.dist && dy <= f("upperYRange") && dy >= -f("lowerYRange")
}

/// AtkParam dummy id -> model dummy entity: 1<model><dummy> addresses a right-hand weapon
/// model's own dummy. Model2's are kept under the full id (model.rs: Mortal Draw 12220 =
/// WP_A_0300_2 dummy 220), Model0's under the bare one (Whirlwind Slash 10120 -> 120), as
/// ffx::dummy_candidates resolves them.
fn dmy_entity(d: &Dummies, id: i64) -> Option<Entity> {
    let full = i16::try_from(id).ok().and_then(|k| d.0.get(&k));
    full.or_else(|| d.0.get(&((if id >= 10000 { id % 10000 } else { id }) as i16))).copied()
}

/// Player damage: the weapon's attackBase* per element (Kusabimaru 5000; a prosthetic's own
/// EquipParamWeapon row for its a070..a079 swings, e.g. the Loaded Axe 73000 attackBasePhysics 45) times the AtkParam element
/// corrections [%], each scaled by the NPC's NpcParam <element>DamageCutRate (c1020: dark
/// 0.6, others 1.0; NpcParam def_* are 0). Combat arts deal "dark" (Whirlwind Slash
/// atkDarkCorrection 125 on attackBaseDark 40).
fn player_attack_damage(combat: &Combat, atk: &Attack, weapon: i64, def: &Actor) -> f32 {
    let w = combat.param("EquipParamWeapon", weapon);
    let npc = combat.npc(def);
    let base = |k: &str| w[k].as_f64().unwrap_or(0.0) as f32;
    let cut = |k: &str| npc[k].as_f64().unwrap_or(1.0) as f32;
    base("attackBasePhysics") * atk.atk_phys_correction / 100.0
        + base("attackBaseMagic") * atk.atk_mag_correction / 100.0 * cut("magicDamageCutRate")
        + base("attackBaseFire") * atk.atk_fire_correction / 100.0 * cut("fireDamageCutRate")
        + base("attackBaseThunder") * atk.atk_thun_correction / 100.0 * cut("thunderDamageCutRate")
        + base("attackBaseDark") * atk.atk_dark_correction / 100.0 * cut("darkDamageCutRate")
}

/// Player hit reaction by HKS damage level (c0000_define DAMAGE_LEVEL_*): 1 small and 2
/// middle in four directions, 3 large front/back, 4 ex-large blow, 6 fling (large blow),
/// 7 small blow, 9 upper (launch), 8 minimum; blows launch through their TAE 920.
fn player_damage_state(combat: &Combat, level: i64, defender: &Actor, from: Vec3, at: Vec3) -> Option<String> {
    // HKS normal damage (c0000_transition.lua ~1162): DamageDirection = 4-way for SMALL / MIDDLE /
    // PUSH, front/back for the rest; NONE (0) and MINIMUM (8) play no reaction.
    let fwd = defender.forward();
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
    let fb = if f >= 0.0 { "F" } else { "B" };
    let dir4 = if f.abs() >= r.abs() { fb } else if r > 0.0 { "R" } else { "L" };
    let candidates = match level {
        0 | 8 => return None,
        1 => vec![format!("StandDamageSmall_{dir4}")],
        2 | 5 => vec![format!("StandDamageMiddle_{dir4}")],
        3 => vec![format!("StandDamageLarge_{fb}")],
        4 => vec![format!("StandDamageLargeBlowStart_{fb}")],
        // EX_BLAST / BREATH: ExLarge and SpecialLarge blows have no clip in c0000_a0xx.
        10 => vec![format!("StandDamageExLargeBlowStart_{fb}"), format!("StandDamageLargeBlowStart_{fb}")],
        11 => vec![format!("StandDamageSpecialLargeBlowStart_{fb}"), format!("StandDamageLargeBlowStart_{fb}")],
        6 => vec!["StandDamageLargePound".to_string()],
        7 => vec![format!("StandDamageSmallBlow_{fb}")],
        9 => vec!["StandDamageLargeUpperStart".to_string()],
        _ => vec![format!("StandDamageSmall_{dir4}")],
    };
    candidates
        .into_iter()
        .chain(std::iter::once("StandDamageLarge_F".to_string()))
        .find(|s| combat.player.anim_key(s).and_then(|k| combat.player.anim(k)).is_some_and(|a| a.duration.is_some()))
}

/// Wolf hit in the air (HKS air damage, c0000_transition.lua ~1106): SMALL / MIDDLE ->
/// AirDamageSmall, LARGE / PUSH -> AirDamageLarge, EXLARGE / SMALL_BLOW -> AirDamageLargeBlow,
/// EX_BLAST -> ExLargeBlow, UPPER -> LargeUpper, FLING -> LargePound, BREATH -> SpecialLargeBlow
/// (front/back). The blow / upper / special air clips do not exist in c0000_a0xx, so those use
/// the AirDamageLarge chain. NONE / MINIMUM: none.
fn player_air_damage_state(combat: &Combat, level: i64, defender: &Actor, from: Vec3, at: Vec3) -> Option<String> {
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let fb = if to.dot(defender.forward()) >= 0.0 { "F" } else { "B" };
    let large = format!("AirDamageLargeStart_{fb}");
    let candidates = match level {
        0 | 8 => return None,
        1 | 2 => vec![format!("AirDamageSmall_{fb}")],
        3 | 5 => vec![large],
        4 | 7 => vec![format!("AirDamageLargeBlowStart_{fb}"), large],
        10 => vec![format!("AirDamageExLargeBlowStart_{fb}"), large],
        9 => vec!["AirDamageLargeUpperStart".to_string(), large],
        6 => vec!["AirDamageLargePoundStart".to_string()],
        11 => vec![format!("AirDamageSpecialLargeBlowStart_{fb}"), large],
        _ => vec![format!("AirDamageSmall_{fb}")],
    };
    candidates.into_iter().find(|s| combat.player.anim_key(s).and_then(|k| combat.player.anim(k)).is_some_and(|a| a.duration.is_some()))
}

/// Wolf's reaction when a hit empties his posture (HKS DAMAGE_TYPE_DAMAGEBREAK,
/// c0000_transition.lua): a blow variant by damage level, else StandDamageBreak_F/B. A hit
/// while he is already broken knocks him down (StandDamageBreakDamage, face down): one that
/// breaks him again in StandDamageBreak (DAMAGE_TYPE_DAMAGEBREAK, c0000_transition.lua:1615), or
/// a push / small / middle / large one in any of the broken states (:1158). Others there take
/// the normal reactions ([`broken_takes_normal_damage`]).
pub(crate) fn player_break_state(combat: &Combat, level: i64, defender: &Actor, from: Vec3, at: Vec3) -> String {
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let fb = if to.dot(defender.forward()) >= 0.0 { "F" } else { "B" };
    let knocked_down = if defender.posture_broken() {
        defender.state.starts_with("StandDamageBreak_")
    } else {
        is_player_broken(&defender.state) && matches!(level, 1 | 2 | 3 | 5)
    };
    let candidates = if knocked_down {
        vec!["StandDamageBreakDamage".to_string()]
    } else if is_player_broken(&defender.state) && !defender.posture_broken() {
        // Minimum: nothing (:1162).
        return "".to_string();
    } else if defender.airborne {
        // BEH_R_AIR_BREAK_DAMAGE: FLING -> AirDamageBreakLargePound, else AirDamageBreak (the
        // air blow / upper / special break clips do not exist); MINIMUM none.
        match level {
            8 => return "".to_string(),
            6 => vec!["AirDamageBreakLargePoundStart".to_string(), "AirDamageBreak".to_string()],
            4 => vec![format!("AirDamageBreakLargeBlowStart_{fb}"), "AirDamageBreak".to_string()],
            9 => vec!["AirDamageBreakLargeUpperStart".to_string(), "AirDamageBreak".to_string()],
            _ => vec!["AirDamageBreak".to_string()],
        }
    } else {
        match level {
            // HKS BEH_R_BREAK_DAMAGE: DAMAGE_LEVEL_MINIMUM plays nothing (c0000_transition.lua:1690).
            8 => return "".to_string(),
            4 => vec![format!("StandDamageBreakLargeBlowStart_{fb}")],
            // EX_BLAST / BREATH: ExLarge and SpecialLarge blows have no clip in c0000_a0xx.
            10 | 11 => vec![format!("StandDamageBreakExLargeBlowStart_{fb}"), format!("StandDamageBreakLargeBlowStart_{fb}")],
            7 => vec![format!("StandDamageBreakSmallBlow_{fb}")],
            9 => vec!["StandDamageBreakLargeUpperStart".to_string()],
            6 => vec!["StandDamageBreakLargePound".to_string()],
            _ => vec![format!("StandDamageBreak_{fb}")],
        }
    };
    candidates
        .into_iter()
        .chain(std::iter::once("StandDamageBreak_F".to_string()))
        .find(|s| combat.player.anim_key(s).and_then(|k| combat.player.anim(k)).is_some_and(|a| a.duration.is_some()))
        .unwrap_or_else(|| "StandDamageBreak_F".to_string())
}

/// HKB_STATE_STAND_DAMAGE_BREAK / STAND_DEFLECT_BREAK: posture broken and still staggered.
pub(crate) fn is_player_broken(state: &str) -> bool {
    matches!(state, "StandDamageBreak_F" | "StandDamageBreak_B" | "StandDeflectBreak" | "LandAirDamageBreak" | "LandAirDeflectGuardBreak")
}

/// A hit that does not break Wolf again while he is broken: push / small / middle / large knock
/// him down and minimum plays nothing (player_break_state); the blow, fling, upper, ex blast
/// and breath levels go on down c0000_transition.lua's damage chain (:1158) to their normal
/// reactions.
pub(crate) fn broken_takes_normal_damage(level: i64, defender: &Actor) -> bool {
    !defender.posture_broken() && is_player_broken(&defender.state) && !matches!(level, 1 | 2 | 3 | 5 | 8)
}

/// ChrActionFlag 24: no damage motion (the exe's b51b70 / bdbb40 publish level 0 and skip the
/// knockback; HP and posture still land). sekiro-rs docs/COMBAT-RULES.md "Super armour".
pub(crate) const FLAG_NO_DAMAGE_MOTION: i64 = 24;
/// ChrActionFlag 73 (SetBool32_0x7C_10): an enemy's attack anim marks where it may flinch (its
/// wind-up and recovery); between them a hit takes no damage motion. Live (the rec in test
/// enemy_mid_swing_takes_the_hit_without_flinching): c1020 hit at a000_003000 frame 9+ swings on.
/// gap: the exe's reader of 0x7C bit 10 is not traced; sekiro-rs has no such gate.
pub(crate) const FLAG_DAMAGE_MOTION: i64 = 73;

/// The HKS damage level an NPC's hit reaction sees (sekiro-rs c0f97ee fight.rs damage_motion_level,
/// docs/COMBAT-RULES.md "Super armour", traced in the v1.6 exe): the active spCategory 1001
/// SpEffect (its anim's TAE ones and residents, HP gate open) with the lowest categoryPriority
/// (bfd980) replaces it by its dmgLv_<level> enum (10cea20), which 10b5100 turns into a level
/// through the table at RVA 2b979e8: enums 1-12 -> 0, 8, 1, 2, 3, 7, 4, 6, 9, 5, 10, 11 (0 keeps
/// it). Level 0 stays (guard contacts never get here). E.g. 5358 (Genichiro / c1010 perilous
/// commitments) makes small / middle hits minimum (8: an additive flinch) and leaves large ones;
/// the residents 5311 / 5320 / 5325 (priority 200). gap: stateInfo 120 effects (5375) need a
/// super-armour meter (NpcParam superArmorDurability, 0 on the rows checked): skipped.
pub(crate) fn damage_motion_level(d: &crate::data::CharData, a: &Actor, level: i64) -> i64 {
    const LEVELS: [i64; 12] = [0, 8, 1, 2, 3, 7, 4, 6, 9, 5, 10, 11];
    if level <= 0 || level >= 12 {
        return level;
    }
    let hp_pct = a.hp / a.hp_max.max(1.0) * 100.0;
    let tae: Vec<&crate::data::SpEffect> = if a.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&a.anim, a.t).into_iter().map(|(_, s)| s).collect() };
    tae.into_iter()
        .chain(a.resident.iter().filter_map(|id| d.sp_effects.get(&id.to_string())))
        .filter(|s| s.sp_category == 1001 && s.state_info != 120 && (s.condition_hp <= 0.0 || hp_pct <= s.condition_hp))
        .min_by_key(|s| s.category_priority)
        .and_then(|s| LEVELS.get((s.dmg_lv(level) - 1).max(-1) as usize).copied().filter(|_| s.dmg_lv(level) > 0))
        .unwrap_or(level)
}

/// NPC damage reaction (c9997.lua ExecDamage*): SMALL / MIDDLE -> Damage{Small,Middle}_<dir>,
/// LARGE / FLING / UPPER -> DamageLarge_<dir>, PUSH -> DamagePushFront/Back, SMALL_BLOW / BREATH
/// -> DamageBlowFront/Back (ExecDamageBlow), BLOW (4) / EX_BLAST -> DamageLargeBlow; MINIMUM (8)
/// and NONE play no reaction. Direction from where the attacker stands (GetDirOfPlayableDamage).
/// c9997 IsDamagedAerial (c9997.lua:788): an enemy off the ground (env 201 false) or under a
/// SpEffect with ref 1000060 (SP_EFFECT_REF_AERIAL_DAMAGE) takes hits in the air.
pub(crate) const SP_REF_AERIAL_DAMAGE: i64 = 1000060;

/// [`SP_REF_AERIAL_DAMAGE`]'s IsDamagedAerial for an enemy at height `y`. gap: env 201 (on the
/// ground) is read as not lifted above where an enemy's SetNoGravity leap began (actor.rs
/// lift_ground), 5 cm.
pub(crate) fn enemy_is_aerial(combat: &Combat, a: &Actor, e: &crate::enemy::Enemy, y: f32) -> bool {
    a.airborne || a.lift_ground.is_some_and(|g| y > g + 0.05) || crate::prosthetic::enemy_refs(combat, a, e).contains(&SP_REF_AERIAL_DAMAGE)
}

/// [`enemy_damage_state`] with c9997 ExecDamage's aerial branches for a hit in the air
/// (`aerial`, IsDamagedAerial; ported after sekiro-rs c0f97ee character.rs:1045-1074):
/// ExecDamageLargeBlow (c9997.lua:2617) blow / upper / ex blast (4 / 9 / 10) -> W_DamageAerialBlow
/// if the chr has anim 8150; ExecDamageBlow (small blow / breath 7 / 11) comes first and is not
/// aerial; ExecDamageDefault (c9997.lua:3140) middle / large / push / fling / small blow
/// (2 / 3 / 5 / 6 / 7) -> W_DamageAerialBack (hit from behind, env 3032, anim 8141) or
/// W_DamageAerialFront (8140). Anything else in the air (a small hit, no aerial anim) has no
/// reaction: ExecNoSyncAddDamage's additive flinch (aerial and damaged).
pub(crate) fn enemy_damage_state_air(d: &crate::data::CharData, level: i64, defender: &Actor, from: Vec3, at: Vec3, aerial: bool) -> Option<String> {
    if !aerial || matches!(level, 7 | 11) {
        return enemy_damage_state(level, defender, from, at);
    }
    // IsExistAnime: the chr's own anim or its anim group's (TAE aliases come exported).
    let exists = |id: i64| d.anim(&format!("a000_{id:06}")).is_some() || (defender.anim_group > 0 && d.anim(&format!("a{:03}_{id:06}", defender.anim_group * 100)).is_some());
    let back = (from - at).with_y(0.0).dot(defender.forward()) < 0.0;
    match level {
        4 | 9 | 10 if exists(8150) => Some("DamageAerialBlow".to_string()),
        2 | 3 | 5 | 6 if back && exists(8141) => Some("DamageAerialBack".to_string()),
        2 | 3 | 5 | 6 if exists(8140) => Some("DamageAerialFront".to_string()),
        _ => None,
    }
}

pub(crate) fn enemy_damage_state(level: i64, defender: &Actor, from: Vec3, at: Vec3) -> Option<String> {
    let fwd = defender.forward();
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
    let fb = if f >= 0.0 { "Front" } else { "Back" };
    let dir = if f.abs() >= r.abs() {
        fb
    } else if r > 0.0 {
        "Right"
    } else {
        "Left"
    };
    Some(match level {
        0 | 8 => return None,
        2 => format!("DamageMiddle_{dir}"),
        3 | 6 | 9 => format!("DamageLarge_{dir}"),
        5 => format!("DamagePush{fb}"),
        7 | 11 => format!("DamageBlow{fb}"),
        4 | 10 => "DamageLargeBlow".to_string(),
        _ => format!("DamageSmall_{dir}"),
    })
}

/// Hurtbox / hitbox overlay (H, debug menu).
#[derive(Resource, Default)]
pub struct ShowHurtboxes(pub bool);

/// Debug (H / the debug menu): active hit capsules (dummy to dummy, hit0_Radius) in red, body
/// capsules in grey.
fn draw_hitboxes(
    combat: Res<Combat>,
    actors: Query<(&Actor, &Transform, Option<&Dummies>, Option<&Hurtboxes>)>,
    tfs: Query<&GlobalTransform>,
    mut gizmos: Gizmos,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut show: ResMut<ShowHurtboxes>,
) {
    // H toggles the hurtbox overlay (also in the debug menu).
    if keys.is_some_and(|k| k.just_pressed(KeyCode::KeyH)) {
        show.0 = !show.0;
    }
    for (a, tf, dummies, hurt) in &actors {
        if show.0 {
            let grey = Color::srgba(0.7, 0.7, 0.7, 0.6);
            match hurt {
                Some(h) => {
                    for &(ma, mb, r) in &h.0 {
                        if let (Ok(pa), Ok(pb)) = (tfs.get(ma), tfs.get(mb)) {
                            gizmos.line(pa.translation(), pb.translation(), grey);
                            gizmos.sphere(Isometry3d::from_translation(pa.translation()), r, grey);
                            gizmos.sphere(Isometry3d::from_translation(pb.translation()), r, grey);
                        }
                    }
                }
                None => {
                    let feet = tf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                    gizmos.line(feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5, grey);
                }
            }
        }
        // The active hit capsules in red: with the overlay too (they read as stray circles in play).
        let (Some(d), false, true) = (dummies, a.anim.is_empty(), show.0) else { continue };
        for (e, atk, _) in data_for(&combat, &a).attack_windows(&a.anim) {
            if !e.in_time(a.t) || (atk.atk_stam <= 0.0 && atk.atk_phys <= 0.0) {
                continue;
            }
            let get = |id: i64| dmy_entity(d, id).and_then(|e| tfs.get(e).ok()).map(|g| g.translation());
            for (d1, d2, r) in atk.shapes() {
                let Some(p1) = get(d1) else { continue };
                let Some(p2) = (if d2 < 0 { Some(p1) } else { get(d2) }) else { continue };
                let red = Color::srgb(1.0, 0.15, 0.1);
                gizmos.line(p1, p2, red);
                gizmos.sphere(Isometry3d::from_translation(p1), r, red);
                gizmos.sphere(Isometry3d::from_translation(p2), r, red);
            }
        }
    }
}

/// Closest distance between segments p1-q1 and p2-q2.
pub(crate) fn segment_distance(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0.0, 0.0)
    } else if a <= 1e-8 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 1e-8 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    ((p1 + d1 * s) - (p2 + d2 * t)).length()
}

/// Fallback hurtbox when a character has no ragdoll capsules (headless tests): a body
/// capsule from feet + 0.3 m to feet + 1.5 m, radius 0.4.
pub(crate) const BODY_RADIUS: f32 = 0.4;
/// ChrActionFlag 125 "MikiriCounter": on in the forward step (GroundStep_F, frames 0-12).
const FLAG_MIKIRI: i64 = 125;
// ThrowParam "見切り PC -> enemy" (mikiri, suffix 0190): player a201_511800, enemy ThrowDef13800.

/// c9997.lua guard_damage_table[damage_level][guard_level]: NONE 0, SMALL 1, LARGE 3, EXLARGE 4,
/// ADD 5 (the raw action).
fn guard_damage_action(level: i64, guard_level: i64) -> u8 {
    const T: [[u8; 5]; 12] = [
        [0, 0, 0, 0, 0],
        [0, 1, 1, 1, 1],
        [0, 3, 1, 1, 1],
        [0, 3, 3, 3, 3],
        [0, 4, 3, 3, 3],
        [0, 1, 1, 1, 3],
        [0, 3, 3, 3, 3],
        [0, 3, 3, 3, 3],
        [0, 5, 5, 5, 5],
        [0, 3, 3, 3, 3],
        [0, 4, 4, 4, 3],
        [0, 3, 3, 3, 3],
    ];
    T[level.clamp(0, 11) as usize][guard_level.clamp(0, 4) as usize]
}

/// SpEffect behaviorRefIds c9997 ExecGuardBlock reads (c9997.lua:563-566).
const SP_REF_ENEMY_JUST_GUARD: i64 = 1000090;
const SP_REF_SPECIAL_GUARD: i64 = 1000100;
const SP_REF_SPECIAL_GUARD2: i64 = 1000101;
const SP_REF_GUARD_TO_ADD_DAMAGE: i64 = 1000105;

/// What a guarding enemy plays for a guarded (blocked or deflected) hit.
#[derive(Debug, PartialEq)]
pub(crate) enum GuardPick {
    State(String),
    /// The additive SABlend_Add flinch.
    Add,
    None,
}

/// c9997 ExecGuardBlock (c9997.lua:2320-2490), for a block and a deflect alike: the
/// guard_damage_table action (ADD, or any with ref 1000105 GUARD_TO_ADD_DAMAGE, flinches
/// additively), then the first clip the chr has of: JustGuardDamageSpecial / Special2 (refs
/// 1000090 ENEMY_JUST_GUARD + 1000100 / 1000101), JustGuardDamage (1000090, SpEffect 200220),
/// GuardDamageSpecial / Special2 (1000100 / 1000101, SpEffects 200230 / 200231), GuardKick (a
/// large guard of a small blow, 7), GuardDamageSmall / Large. None of them: no reaction. A
/// stateInfo 158 SpEffect without ref 1000090 (c1180's shield 400230) deflects but plays the
/// plain guard. `left`: GUARD_DIR_LEFT (*_LefttoRight). EXLARGE (a blow / ex blast on a weak
/// guard) has no guard clip: ExecGuardBlock returns FALSE and ExecDamage goes on to
/// ExecDamageLargeBlow (c9997.lua:2617): W_DamageLargeBlow (8130), in the air (`aerial`,
/// IsDamagedAerial) W_DamageAerialBlow (8150); ExecDamageDefault has no blow branch, so without
/// the clip nothing.
pub(crate) fn enemy_guard_block(d: &crate::data::CharData, refs: &[i64], level: i64, guard_level: i64, left: bool, aerial: bool) -> GuardPick {
    let size = match guard_damage_action(level, guard_level) {
        1 => "Small",
        3 => "Large",
        5 => return GuardPick::Add,
        4 => {
            let blow = if aerial { "DamageAerialBlow" } else { "DamageLargeBlow" };
            let lv_ok = if aerial { matches!(level, 4 | 9 | 10) } else { matches!(level, 4 | 10) };
            return if lv_ok && d.anim_key(blow).is_some() { GuardPick::State(blow.to_string()) } else { GuardPick::None };
        }
        _ => return GuardPick::None,
    };
    if refs.contains(&SP_REF_GUARD_TO_ADD_DAMAGE) {
        return GuardPick::Add;
    }
    let side = if left { "LefttoRight" } else { "RighttoLeft" };
    let (just, sp1, sp2) = (refs.contains(&SP_REF_ENEMY_JUST_GUARD), refs.contains(&SP_REF_SPECIAL_GUARD), refs.contains(&SP_REF_SPECIAL_GUARD2));
    // The game's own typo: W_JustGuardDamage_LefttoRight is the state JustGuardDanage_LefttoRight.
    let just_plain = if left { "JustGuardDanage_LefttoRight".to_string() } else { "JustGuardDamage_RighttoLeft".to_string() };
    let candidates = [
        (just && sp1, format!("JustGuardDamageSpecial_{side}")),
        (just && sp2, format!("JustGuardDamageSpecial2_{side}")),
        (just, just_plain),
        (sp1, format!("GuardDamageSpecial_{side}")),
        (sp2, format!("GuardDamageSpecial2_{side}")),
        (size == "Large" && level == 7, format!("GuardKick_{side}")),
        (true, format!("GuardDamage{size}_{side}")),
    ];
    candidates.into_iter().find(|(ok, s)| *ok && d.anim_key(s).is_some()).map_or(GuardPick::None, |(_, s)| GuardPick::State(s))
}

/// Plays a [`GuardPick`]: GUARD_LEVEL_ADD is W_SABlend_Add_{Front,Left,Right,Back} (anim 9500+),
/// by the hit's direction relative to the enemy.
#[allow(clippy::too_many_arguments)]
fn guard_pick(d: &crate::data::CharData, da: &mut Actor, e: &mut crate::enemy::Enemy, combat: &Combat, by_wolf: bool, pick: GuardPick, from: Vec3, at: Vec3) {
    match pick {
        GuardPick::State(s) => e.react_from(da, combat, by_wolf, &s),
        GuardPick::Add => {
            let fwd = da.forward();
            let to = (from - at).with_y(0.0).normalize_or_zero();
            let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
            let dir = if f.abs() >= r.abs() { if f >= 0.0 { "Front" } else { "Back" } } else if r > 0.0 { "Right" } else { "Left" };
            start_additive(d, da, &format!("SABlend_Add_{dir}"));
        }
        GuardPick::None => {}
    }
}

/// Guard arc half-angle [deg]: the active ShieldBlock flag's ArgB (exe FUN_140b6ab40: blocked
/// when facing . attack dir < cos(ArgB + 180 deg); ArgB 0 acts like 90 = the front half).
pub(crate) fn guard_half_angle(d: &crate::data::CharData, a: &Actor) -> f32 {
    // NPCs: NpcParam guardAngle (c1020 60) - the angle vfunc is per class (EnemyIns vs PlayerIns).
    if a.side == Side::Enemy && a.guard_angle > 0.0 {
        return a.guard_angle;
    }
    d.events_at(&a.anim, a.t)
        .find(|e| e.kind == 0 && e.flag_type() == Some(FLAG_SHIELD_BLOCK))
        .and_then(|e| e.arg_i64("ArgB"))
        .filter(|&b| b != 0)
        .map_or(90.0, |b| b as f32)
}

/// Samples of the attack capsule across one tick (previous position -> current).
const SWEEP_STEPS: usize = 4;

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = (to - from).with_y(0.0);
    f32::atan2(-d.x, -d.z)
}

#[allow(clippy::too_many_arguments)]
/// Posture-damage rate from Wolf's learned latent skills when he guards: each skill's SpEffects
/// (SkillParam spEffect1/2) with stateInfo 158 (deflect) or 204 (block) multiply their
/// def<Attr>StaminaDmgRate for the attack's staminaPhysicsAttribute. Live check 2026-10-08
/// (Flowing Water 280: 150420/150421): slash attacks x0.8, strike x0.75, with int truncation.
pub(crate) fn guard_skill_rate(combat: &Combat, config: &GameConfig, attribute: i64, deflect: bool) -> f32 {
    let Some(field) = crate::data::stamina_rate_field(attribute) else {
        return 1.0;
    };
    let state = if deflect { 158 } else { 204 };
    let mut rate = 1.0;
    for &skill in &config.player.skills {
        let row = combat.param("SkillParam", skill);
        for f in ["spEffect1", "spEffect2"] {
            let Some(id) = row[f].as_i64().filter(|&i| i > 0) else { continue };
            let se = combat.param("SpEffectParam", id);
            if se["stateInfo"].as_i64() == Some(state) {
                rate *= se[field].as_f64().unwrap_or(1.0) as f32;
            }
        }
    }
    rate
}

/// Wolf's hit on an armoured enemy (enemy.rs Armor) lands on the armour part, which passes on
/// x0.35 HP / x0.5 posture (its SpEffects' rates times the part's body compensation); its
/// prosthetic (Loaded Spear, EquipParamWeapon 78xxx = Prosthetic Type 11) breaks it instead and
/// the hit is whole. `weapon` None (a bullet: shuriken, Flame Vent) never breaks it.
/// Both rates also carry the enemy's NpcParam per-attribute rates (`npc_attribute_rates`).
/// Returns (HP rate, posture rate, broke).
pub(crate) fn armor_rates(combat: &Combat, e: &mut crate::enemy::Enemy, da: &mut Actor, atk: &Attack, weapon: Option<i64>) -> (f32, f32, bool) {
    let (npc_hp, npc_posture) = npc_attribute_rates(combat, da, atk);
    let Some(ar) = e.armor.clone() else {
        return (npc_hp, npc_posture, false);
    };
    if ar.prosthetic == 11 && weapon.is_some_and(|w| w / 1000 == 78) {
        ar.shatter(e, da, combat);
        return (npc_hp, npc_posture, true);
    }
    let (hp, posture) = ar.rates(atk.atk_attribute, atk.stamina_physics_attribute);
    (hp * npc_hp, posture * npc_posture, false)
}

/// An NPC's NpcParam rates for a hit's attributes: HP x <attr>DamageCutRate / <attr>DamageRate for
/// its atkAttribute, posture x <attr>StaminaDmgRate for its staminaPhysicsAttribute
/// (data::npc_damage_rate_field; Paramdex calls both final multipliers). The c1150 dogs:
/// lightShootDamageRate 2.0, lightShootStaminaDmgRate 8.0 (a shuriken breaks their 15 posture);
/// the c1180 servants 11807100: thrust 1.35, heavyHit 0.6.
pub(crate) fn npc_attribute_rates(combat: &Combat, da: &Actor, atk: &Attack) -> (f32, f32) {
    if da.side != Side::Enemy {
        return (1.0, 1.0);
    }
    let npc = combat.npc(da);
    let rate = |f: Option<&str>| f.and_then(|f| npc[f].as_f64()).unwrap_or(1.0) as f32;
    (rate(crate::data::npc_damage_rate_field(atk.atk_attribute)), rate(crate::data::npc_stamina_rate_field(atk.stamina_physics_attribute)))
}

/// Posture-damage rate of an enemy defender's own SpEffects (its anim's TAE ones and its
/// residents): their def<Attr>StaminaDmgRate for the attack's staminaPhysicsAttribute, multiplied
/// (as `guard_skill_rate`; Isshin's change a000_003015 holds 3540090 "Revived Kensei posture damage
/// multiplier suppression": x0.3). gap: the exe's sum vs product of several is not traced.
pub(crate) fn def_stamina_rate(combat: &Combat, da: &Actor, attribute: i64) -> f32 {
    if da.side != Side::Enemy {
        return 1.0;
    }
    let pick = |s: &crate::data::SpEffect| s.def_stamina_rate(attribute);
    let d = combat.data_of(da);
    let tae: f32 = if da.anim.is_empty() { 1.0 } else { d.sp_effects_at(&da.anim, da.t).iter().map(|(_, s)| pick(s)).product() };
    let resident: f32 = da.resident.iter().filter_map(|id| d.sp_effects.get(&id.to_string())).map(pick).product();
    tae * resident
}

/// ACTION_UNLOCK_TYPE_* (c0000_define.lua) the HKS asks env(3033) about.
pub(crate) const UNLOCK_AIR_DEFLECT_GUARD: u32 = 15;
pub(crate) const UNLOCK_AIR_SUB_ATTACK: u32 = 16;
pub(crate) const UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_1: u32 = 20;
pub(crate) const UNLOCK_SUB_ATTACK_SHOT_ATTACK: u32 = 21;
pub(crate) const UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_2: u32 = 22;
pub(crate) const UNLOCK_SUB_ATTACK_ENCHANT: u32 = 23;
pub(crate) const UNLOCK_AIR_SP_ATTACK: u32 = 25;
pub(crate) const UNLOCK_SPRINT_TO_CROUCH: u32 = 26;

/// env(3033, ACTION_UNLOCK_TYPE_n): the exe (140b3 env case 0xbd9 -> FUN_1407a7fe0) answers yes
/// when the player holds at least one item whose ActionUnlockParam row has action<n> set (a count
/// per type at PlayerGameData +0x168 +0x90) or a flag bit is set. A skill counts through the
/// virtual weapon it grants: SkillParam.virtualWeaponId -> EquipParamWeapon.actionUnlockParamId ->
/// ActionUnlockParam (Mid-Air Combat Arts: skill 6 -> 200500 -> action25). Wolf always holds the
/// Kusabimaru (goods 2300: action05) and the prosthetic (2310: action04) here. gap: the flag bits'
/// writer is not traced; the other goods (Diving 2420, Night Eyes 2390, ...) are not held.
/// `player.unlock_all_moves` (default) opens every type, and so does data without the
/// ActionUnlockParam table.
pub(crate) fn action_unlocked(combat: &Combat, config: &GameConfig, ty: u32) -> bool {
    if config.player.unlock_all_moves || combat.params.get("ActionUnlockParam").is_none() || matches!(ty, 4 | 5) {
        return true;
    }
    let key = format!("action{ty:02}");
    config.player.skills.iter().any(|&s| {
        let weapon = combat.param("SkillParam", s)["virtualWeaponId"].as_i64().unwrap_or(-1);
        let row = combat.param("EquipParamWeapon", weapon)["actionUnlockParamId"].as_i64().unwrap_or(-1);
        combat.param("ActionUnlockParam", row)[&key].as_i64() == Some(1)
    })
}

/// The SpEffects of Wolf's learned latent skills (SkillParam spEffect1-3, config player.skills).
pub(crate) fn skill_sp_effects<'a>(combat: &'a Combat, config: &'a GameConfig) -> impl Iterator<Item = &'a serde_json::Value> + 'a {
    config.player.skills.iter().flat_map(move |&skill| {
        let row = combat.param("SkillParam", skill);
        ["spEffect1", "spEffect2", "spEffect3"].into_iter().filter_map(move |f| row[f].as_i64().filter(|&i| i > 0)).map(move |id| combat.param("SpEffectParam", id))
    })
}

/// The latent skills' SpEffects as typed rows, for code that takes `SpEffect`s (the stealth sight /
/// hearing cuts: Covert A 60 -> 150000 sightSearchEnemyCut 20, aroundSightPointAddRate 0.5;
/// Covert B 61 -> 150010 hearingSearchEnemyRate 0.5).
pub(crate) fn skill_sp_effect_rows(combat: &Combat, config: &GameConfig) -> Vec<crate::data::SpEffect> {
    skill_sp_effects(combat, config).filter_map(|se| serde_json::from_value(se.clone()).ok()).collect()
}

/// Product of one SpEffectParam rate over the learned latent skills, e.g. Mikiri Counter posture
/// UP (70: 150400 attackHitParryStaminaAttackRate 1.25), deflect posture UP (270: 150410
/// defStaminaAttackRate 1.25).
pub(crate) fn skill_rate(combat: &Combat, config: &GameConfig, field: &str) -> f32 {
    skill_sp_effects(combat, config).map(|se| se[field].as_f64().unwrap_or(1.0) as f32).product()
}

/// NpcParam teamType hostility between two NPCs (both can hurt and target each other). Team 24
/// ("Enemy 2", Paramdex TEAM_TYPE) holds the invaders, and their rows say whom they are for:
/// 11502001 / 14701003 "hostile to the Ashina forces" (葦名勢敵対用), 11502100 "to the monkeys"
/// (猿敵対用), 17000110 "to the fallen samurai" (落武者敵対) - all team 6 ("Enemy") - while
/// 11007200 "does not fight the Lone Shadows" (孤影衆と敵対しない) shares their team 24. Team 29's
/// one row is the Red Ogre "indiscriminate" (50200010 無差別). gap: the exe's team table is not
/// traced; the rule is these names. Disabled (0), friendly NPCs (26) and objects (30) fight none.
pub fn teams_hostile(a: i64, b: i64) -> bool {
    match (a, b) {
        (29, x) | (x, 29) => !matches!(x, 0 | 26 | 30),
        (6, 24) | (24, 6) => true,
        _ => false,
    }
}

/// An NPC's team (NpcParam teamType; 6 "Enemy" without one).
pub fn team_of(combat: &Combat, a: &Actor) -> i64 {
    combat.npc(a)["teamType"].as_i64().unwrap_or(6)
}

fn resolve(
    combat: Res<Combat>,
    config: Res<GameConfig>,
    mut log: ResMut<CombatLog>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Actor, &Transform, Option<&mut Enemy>, Option<&Dummies>, Option<&Hurtboxes>)>,
    dummy_tf: Query<&GlobalTransform>,
    mut sounds: Option<ResMut<crate::sound::SoundQueue>>,
    mut prev_dmy: Local<std::collections::HashMap<Entity, Vec3>>,
    mut throw: Option<ResMut<crate::player::ActiveThrow>>,
    players: Query<&crate::player::Player>,
    mut statuses: ResMut<crate::status::Statuses>,
) {
    // The equipped prosthetic (group, enough Spirit Emblems for a use): the umbrella's guard
    // reactions and the Mist Raven's atemi.
    let equipped = players.iter().next().and_then(|p| crate::player::equipped_tool(&combat, &config, p.tool_slot).map(|t| (t, p.emblems)));
    let tool = equipped.map(|(t, emblems)| (t.group, emblems >= t.emblems.max(1)));
    let tool_weapon = equipped.map(|(t, _)| t.id);
    // Wolf's spot: a posture break faces him (front / back collapse), whichever pair sees it.
    let wolf_at = q.iter().find(|x| x.1.side == Side::Player).map(|x| x.2.translation);
    let mut pairs = q.iter_combinations_mut();
    while let Some([mut x, mut y]) = pairs.fetch_next() {
        for flip in [false, true] {
            let (att, def) = if flip { (&mut y, &mut x) } else { (&mut x, &mut y) };
            let (att_e, ref mut aa, atf, ref mut ae, adummies, _) = *att;
            let (def_e, ref mut da, dtf, ref mut de, _, dhurt) = *def;
            let defender_dead = de.as_ref().is_some_and(|e| e.is_dead());
            // Enemies are on one side, unless their teams are hostile (teams_hostile: the
            // invaders against Ashina). AtkParam friendlyTarget rows are for objects.
            if aa.side == da.side && !(aa.side == Side::Enemy && teams_hostile(team_of(&combat, aa), team_of(&combat, da))) {
                continue;
            }
            if aa.hp <= 0.0 || (da.hp <= 0.0 && da.side == Side::Player) || defender_dead {
                // Arrows that reached a dead / broken shooter's target, or a dead target, are
                // dropped: queued, they landed seconds later once both were up again.
                let alive = aa.hp > 0.0;
                aa.bullet_hits.retain(|(t, _)| alive && *t != def_e);
                continue;
            }
            if aa.anim.is_empty() && aa.bullet_hits.is_empty() {
                continue;
            }
            let ad = data_for(&combat, &aa);
            let mut windows: Vec<(i32, i64, Attack, bool)> = ad
                .attack_windows(&aa.anim)
                .into_iter()
                .filter(|(e, atk, _)| e.in_time(aa.t) && atk.oppose_target != 0 && (atk.atk_stam > 0.0 || atk.npc_damage() > 0.0 || atk.direct_atk_stam_damage > 0.0))
                .map(|(e, atk, j)| ((e.start * TAE_FPS).round() as i32, j, atk.clone(), false))
                .collect();
            // The attacker's bullets that reached this defender (enemy_bullet.rs): already
            // connected, so only the dodge check and the outcome below apply.
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut aa.bullet_hits).into_iter().partition(|(t, _)| *t == def_e);
            aa.bullet_hits = rest;
            windows.extend(mine.into_iter().map(|(_, atk)| (i32::MIN, -1, atk, true)));
            for (start, judge, atk, bullet) in windows {
                if !bullet && aa.hits_done.contains(&(def_e, judge, start)) {
                    continue;
                }
                let to_def = (dtf.translation - atf.translation).with_y(0.0);
                // Hit shapes: every one the row uses (AtkParam hit0..hit15), a capsule between its
                // two dummy polys or a sphere at the first (DmyPoly2 -1), swept from where they were
                // on the previous tick so a fast slash cannot pass through a thin limb capsule
                // between ticks. gap: hitSourceType (which model's dummy ids) and the priority of
                // two shapes touching at once (hti*_Priority) are not read; any shape connects.
                let shapes: Vec<([(Vec3, Vec3); SWEEP_STEPS], f32)> = adummies
                    .map(|d| {
                        atk.shapes()
                            .into_iter()
                            .filter_map(|(d1, d2, r)| {
                                let e1 = dmy_entity(d, d1)?;
                                let e2 = if d2 < 0 { e1 } else { dmy_entity(d, d2)? };
                                let p1 = dummy_tf.get(e1).ok()?.translation();
                                let p2 = dummy_tf.get(e2).ok()?.translation();
                                let (q1, q2) = (prev_dmy.get(&e1).copied().unwrap_or(p1), prev_dmy.get(&e2).copied().unwrap_or(p2));
                                let sweep = std::array::from_fn::<_, SWEEP_STEPS, _>(|i| {
                                    let k = i as f32 / (SWEEP_STEPS - 1) as f32;
                                    (q1.lerp(p1, k), q2.lerp(p2, k))
                                });
                                Some((sweep, r))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let capsule = (!shapes.is_empty()).then_some(&shapes);
                // The part group of the capsule it touched (env(1120)).
                let mut part = 0u8;
                let connects = bullet || match capsule {
                    Some(shapes) => shapes.iter().any(|(sweep, radius)| sweep.iter().any(|&(p1, p2)| match dhurt {
                        // Ragdoll hurtbox capsules (follow the animation).
                        Some(h) => h.0.iter().enumerate().any(|(i, &(ma, mb, r))| match (dummy_tf.get(ma), dummy_tf.get(mb)) {
                            (Ok(a), Ok(b)) => {
                                let touch = segment_distance(p1, p2, a.translation(), b.translation()) <= radius + r;
                                if touch {
                                    part = h.1.get(i).copied().unwrap_or(0);
                                }
                                touch
                            }
                            _ => false,
                        }),
                        None => {
                            let feet = dtf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                            segment_distance(p1, p2, feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5) <= radius + BODY_RADIUS
                        }
                    })),
                    None => {
                        // No model loaded (headless tests): reach approximation.
                        let reach = match aa.side {
                            Side::Player => config.combat.player_reach,
                            Side::Enemy => config.combat.enemy_reach,
                        } + atk.hit0_radius;
                        let in_arc = aa.forward().angle_between(to_def.normalize_or_zero()).to_degrees() <= config.combat.attack_arc / 2.0;
                        to_def.length() <= reach && in_arc
                    }
                };
                if !connects {
                    continue;
                }
                // TAE 954 IFrames on the defender: 2 general (all), 6 thrusts, 7 grabs, 1 sweeps.
                // A dodged hit is not consumed: the hitbox can still land after the window.
                if dodged(&combat, da, Some(aa), &atk) {
                    continue;
                }
                aa.hits_done.push((def_e, judge, start));
                aa.attack_hit = true;

                let dd = data_for(&combat, &da);
                let defending = !da.anim.is_empty();

                // The Mist Raven's atemi (HKS line 1152): a hit inside its SP_EF_REF_TAE_ENABLE_ATEMI_KAWARIMI
                // window (SubAttackJumpAtemiReady f0-21) becomes W_SubAttackJumpReady: the feathers and the
                // leap away instead of the damage.
                if da.side == Side::Player && defending {
                    if let Some((cat, enable)) = tool {
                        let refs = |r: i64| dd.has_ref(&da.anim, da.t, r);
                        if crate::prosthetic::hks::mist_raven_atemi(cat, enable, &refs) {
                            // In the air (line 1095): W_AirSubAttackMoveReady.
                            let ready = if da.airborne { "AirSubAttackMoveReady" } else { "SubAttackJumpReady" };
                            if let Some(k) = crate::player::sub_anim(dd, ready, cat) {
                                da.play(ready, &k);
                                da.move_vel = Vec3::ZERO;
                                log.push("Mist Raven", Color::srgb(0.85, 0.5, 0.5));
                                continue;
                            }
                        }
                    }
                }
                // The open Loaded Umbrella (STATE_TYPE_*_SUB_GUARD): its own guard reactions (HKS
                // 1371-1386, additive 676-690), played in the umbrella's group a076.
                let umbrella = da.side == Side::Player && crate::prosthetic::hks::sub_guard(&da.state);

                // Mikiri counter: a perilous thrust meeting the forward step's MikiriCounter
                // window becomes the mikiri throw (ThrowParam 11020190). The attacker loses
                // staminaDamageAttackHitParry posture.
                if aa.side == Side::Enemy
                    && atk.atk_type == 2
                    && atk.disable_guard == 1
                    && defending
                    && dd.flag(&da.anim, da.t, FLAG_MIKIRI)
                {
                    // When that empties the attacker's posture it is 見切り崩し instead (0120: Wolf
                    // a20x_511100, ThrowDef13100), the break pose player.rs takes on to the deathblow
                    // (live c1010: 511100 0.55-0.61 s -> 511110).
                    aa.add_posture(atk.stamina_damage_attack_hit_parry * skill_rate(&combat, &config, "attackHitParryStaminaAttackRate"));
                    let broken_row = aa.posture_broken().then(|| combat.throw(combat.foe_of(&aa).throw_row(120))).flatten().filter(|th| dd.anim(&th.atk_anim).is_some());
                    let (row, state) = match broken_row {
                        Some(th) => (Some(th), "ThrowBreak"),
                        None => (combat.throw(combat.foe_of(&aa).throw_row(190)), "Mikiri"),
                    };
                    if let Some(th) = row {
                        if dd.anim(&th.atk_anim).is_some() {
                            da.play(state, &th.atk_anim);
                            da.move_vel = Vec3::ZERO;
                            da.yaw = yaw_to(dtf.translation, atf.translation);
                            aa.yaw = yaw_to(atf.translation, dtf.translation);
                            if let Some(e) = ae.as_deref_mut() {
                                e.react_from(aa, &combat, da.side == Side::Player, &format!("ThrowDef{}", th.def_anim));
                            }
                            // ThrowParam 190: the enemy is held on Wolf's dummy 523 (follow_throw).
                            if let Some(t) = throw.as_deref_mut() {
                                t.0 = Some(crate::player::ThrowHold::new(att_e, &th, state));
                            }
                            log.push("MIKIRI COUNTER", Color::srgb(0.6, 0.9, 1.0));
                            continue;
                        }
                    }
                }

                // Grab (AtkParam throwFlag 1, unblockable and undeflectable): contact starts the
                // throw (its ThrowParam row, Combat::enemy_grab: the General's 21020000 -> enemy
                // ThrowAtk4100) holding the player until its ThrowAttackBehavior lands
                // (resolve_throws). The player plays the row's defAnimId in group defAnimOffset
                // (a210_600000; clips in chr/c0000_c<chr>.anibnd, Wolf's per-enemy anims).
                // A grab that is already holding him (its ThrowAtk anim has the hook's own
                // throwFlag 1 hit, e.g. the valley sniper's a100_004100) does not start again.
                if aa.side == Side::Enemy && atk.throw_flag == 1 && (da.state == "Grabbed" || aa.state.starts_with("ThrowAtk") || da.side == Side::Enemy) {
                    // (Grabs are Wolf's: their ThrowParam rows hold him in his a2xx anims.)
                    continue;
                }
                if aa.side == Side::Enemy && atk.throw_flag == 1 && !da.airborne {
                    let row = combat.enemy_grab(combat.foe_of(&aa), atk.throw_type_id);
                    if let Some(e) = ae.as_deref_mut() {
                        e.react_from(aa, &combat, da.side == Side::Player, &format!("ThrowAtk{}", row.as_ref().map_or(4100, |t| t.atk_anim_id)));
                    }
                    aa.yaw = yaw_to(atf.translation, dtf.translation);
                    da.yaw = yaw_to(dtf.translation, atf.translation);
                    // The row's absorb (atkSorbDmyId 230 / 231: Wolf onto the enemy's dummy, the model
                    // sliding over adsrobModelPosInterpolationTime; defSorbDmyId 172 / 173 of c1250 /
                    // c1360: the enemy onto Wolf's), as the deathblows: he was held wherever the
                    // grab capsule touched him. player.rs follow_throw, one step later.
                    if let (Some(th), Some(t)) = (row.as_ref(), throw.as_deref_mut()) {
                        if th.atk_dmy != 0 || th.def_dmy != 0 {
                            t.0 = Some(crate::player::ThrowHold { target: att_e, atk_dmy: th.def_dmy, def_dmy: th.atk_dmy, state: "Grabbed", model_interp: th.model_interp });
                        }
                    }
                    let grabbed = row.map(|t| format!("a{:03}_{:06}", t.def_group, t.def_anim)).filter(|k| dd.anim(k).is_some_and(|a| a.duration.is_some()));
                    match grabbed {
                        Some(k) => da.play("Grabbed", &k),
                        None => da.procedural("Grabbed"),
                    }
                    da.move_vel = Vec3::ZERO;
                    log.push("GRABBED", Color::srgb(1.0, 0.2, 0.1));
                    continue;
                }

                // Perilous attacks (AtkParam disableGuard / disableJustGuard) go through
                // a block, and sweeps/grabs through a deflect too.
                let deflectable = atk.disable_just_guard == 0;
                let blockable = atk.disable_guard == 0;
                // The guard arc gates deflects as well as blocks: an attack from behind lands.
                let in_guard_arc = !da.anim.is_empty()
                    && da.forward().angle_between(-to_def.normalize_or_zero()).to_degrees() <= guard_half_angle(dd, da);
                // The deflect window comes from the base anim or the additive layer
                // (AddHardDeflectGuard, HKS ref 228).
                let (jk, jt) = if !da.add_anim.is_empty() && dd.has_state_info(&da.add_anim, da.add_t, STATE_INFO_JUST_GUARD) {
                    (da.add_anim.clone(), da.add_t)
                } else {
                    (da.anim.clone(), da.t)
                };
                let outcome = if defending && in_guard_arc && deflectable && dd.has_state_info(&jk, jt, STATE_INFO_JUST_GUARD) {
                    let w = dd
                        .events_at(&jk, jt)
                        .find(|e| {
                            e.kind == 67
                                && dd.sp_effects.get(&e.arg_i64("SpEffectID").unwrap_or(0).to_string()).is_some_and(|s| s.state_info == STATE_INFO_JUST_GUARD)
                        })
                        .map(|e| (e.end - e.start, jt - e.start))
                        .unwrap_or((0.0, 0.0));
                    let rate = dd
                        .sp_effects_at(&jk, jt)
                        .iter()
                        .find(|(id, _)| (105020..=105023).contains(id))
                        .map(|(_, s)| if s.def_stamina_attack_rate > 0.0 { s.def_stamina_attack_rate } else { 1.0 })
                        .unwrap_or(1.0);
                    Outcome::Deflect { window: w.0, at: w.1, rate }
                } else if defending
                    && blockable
                    && dd.flag(&da.anim, da.t, FLAG_SHIELD_BLOCK)
                    && in_guard_arc
                {
                    Outcome::Block
                } else {
                    Outcome::Hit
                };

                // Lightning Reversal: an enemy's lightning (spAttribute 6 / 10) catching Wolf in the
                // air charges him instead of hurting him (c0000_transition.lua: free / wire fall +
                // DAMAGE_ELEMENT_LIGHTNING -> W_AirDamageElectroChargeStart, BLUE_LIGHTNING ->
                // ...WeakStart; guarded in the air -> g_AddElectroCharge, W_Air(Weak)ElectroCharge-
                // Deflect{Hard,Easy}). gap: that the charging hit takes no HP or posture is the
                // engine's (not traced in the exe); from the game, where it does not.
                if aa.side == Side::Enemy && da.side == Side::Player && da.airborne && matches!(atk.sp_attribute, 6 | 10) {
                    let strong = atk.sp_attribute == 6;
                    let w = if strong { "" } else { "Weak" };
                    let state = match outcome {
                        Outcome::Deflect { .. } => format!("Air{w}ElectroChargeDeflectHard"),
                        Outcome::Block => format!("Air{w}ElectroChargeDeflectEasy"),
                        Outcome::Hit => format!("AirDamageElectroCharge{w}Start"),
                    };
                    if da.play_state(dd, &state) {
                        // 9495 / 9490: the charge SpEffect, effectEndurance 30 s.
                        da.electro = Some((if strong { 9495 } else { 9490 }, 30.0));
                        da.move_vel = Vec3::ZERO;
                        log.push("LIGHTNING CAUGHT - attack before landing", Color::srgb(0.6, 0.8, 1.0));
                        continue;
                    }
                }
                let level = atk.level(da.side == Side::Player);
                let mid = (atf.translation + dtf.translation) / 2.0 + Vec3::Y * 0.4;
                start_hit_stop(&combat, aa, da, dd, &atk, bullet);
                match outcome {
                    Outcome::Deflect { window, at, rate } => {
                        // The enemy's poison / burn builds up on Wolf, less through his sword.
                        if aa.side == Side::Enemy && da.side == Side::Player {
                            for m in statuses.hit_wolf(&combat, def_e, config.player.combat_art, &atk.sp_effects(), crate::status::Guard::Deflect) {
                                log.push(format!("Wolf: {m}"), Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        let left = apply_knockback(&combat, da, atf.translation, dtf.translation, Some(aa.forward()), atk.knockback_just_guard, guard_kb(level), "knockbackRate_vsPlayer_JustGuard");
                        // A guarded or deflected bullet is repelled itself: its distant shooter takes
                        // no recoil, posture or reaction (sekiro-rs a8351b9 fight.rs:644,
                        // docs/PROJECTILE-GUARD.md: every NPC bullet AtkParam has 0
                        // directAtkStamDamage_Attacker / repelVictoryStamDamage_Attacker and no
                        // guarded reaction). gap: inferred from the rows, the exe's bullet-to-owner
                        // dispatch is not traced.
                        if !bullet {
                            recoil(&combat, aa, atf.translation, dtf.translation, left);
                        }
                        let skill = if da.side == Side::Player { guard_skill_rate(&combat, &config, atk.stamina_physics_attribute, true) } else { def_stamina_rate(&combat, da, atk.stamina_physics_attribute) };
                        // The attacker's area-doping posture rate counts here as on a block or a hit.
                        let pd = (atk.atk_stam * aa.stam_atk_rate * skill).floor();
                        let full = da.posture + pd >= da.posture_max;
                        // A deflect never breaks the defender (HKS: GUARDATTACKER_STAMZERO -> stagger instead).
                        da.posture = (da.posture + pd).min(da.posture_max - 1.0);
                        da.since_posture_damage = 0.0;
                        // Wolf's deflect: his latent skills' defStaminaAttackRate (270: x1.25).
                        let skill_atk = if da.side == Side::Player { skill_rate(&combat, &config, "defStaminaAttackRate") } else { 1.0 };
                        if !bullet {
                            aa.add_posture(atk.repel_lost_stam_damage_attacker * rate * skill_atk);
                        }
                        let attacker_broken = aa.posture_broken();
                        // Defender reaction.
                        match da.side {
                            Side::Player if umbrella => {
                                umbrella_reaction(dd, da, level, true, aa.posture_broken(), atf.translation, dtf.translation);
                            }
                            Side::Player => {
                                let dir = lr(atk.just_deflect_action);
                                if full {
                                    da.play_state(dd, &format!("HardDeflectStagger_{dir}"));
                                } else if !additive_guard(dd, da, level) {
                                    let state = player_guard_state(&combat, true, level, dir, false, da.airborne);
                                    da.play_state(dd, &state);
                                } else {
                                    let flinch = if da.airborne { "AirDeflectHardMinimumAdd" } else { "StandDeflectHardMinimum" };
                                    start_additive(dd, da, flinch);
                                }
                            }
                            Side::Enemy => {
                                // c9997 ExecGuardBlock: GUARD_DIR_RIGHT (1) -> *_RighttoLeft, GUARD_DIR_LEFT (2)
                                // -> *_LefttoRight (the mirror of the player's DEFLECT_DIR_L 1 / R 2).
                                if let Some(e) = de.as_deref_mut() {
                                    let refs = crate::prosthetic::enemy_refs(&combat, da, e);
                                    let glv = combat.npc(&da)["guardLevel"].as_i64().unwrap_or(4);
                                    let pick = enemy_guard_block(dd, &refs, level, glv, atk.just_deflect_action == 2, enemy_is_aerial(&combat, da, e, dtf.translation.y));
                                    guard_pick(dd, da, e, &combat, aa.side == Side::Player, pick, atf.translation, dtf.translation);
                                    e.add_clash(if atk.just_deflect_action == 1 { 200211 } else { 200210 });
                                }
                            }
                        }
                        // Attacker reaction (none for a bullet's shooter).
                        let jd = atk.just_deflected_action;
                        match aa.side {
                            _ if bullet => {}
                            Side::Enemy => {
                                if let Some(e) = ae.as_deref_mut() {
                                    // A deflect that empties the enemy's posture plays the break pose
                                    // pair 弾き0 (ThrowParam 0010: Wolf a20x_510100, enemy ThrowDef12100,
                                    // held on Wolf's dummy atkSorbDmyId); player.rs takes it on to the
                                    // deathblow. Live: the deflect frame goes straight into both. An air
                                    // deflect break keeps AttackBoundEmptyStamina (rec_c1010_b: Wolf
                                    // AirDeflectHardLarge_L, enemy 8650, front deathblow after landing).
                                    // The pair needs the attacker in the row's reach (Dist 4 m, upper /
                                    // lowerYRange), as the plunge and deathblow starts check: one out of
                                    // reach staggers where it stands instead of being pulled onto Wolf's
                                    // dummy.
                                    let row_id = combat.foe_of(&aa).throw_row(10);
                                    let pose = (da.side == Side::Player && !da.airborne)
                                        .then(|| combat.throw(row_id))
                                        .flatten()
                                        .filter(|th| dd.anim(&th.atk_anim).is_some() && throw_in_reach(&combat, row_id, th, dtf.translation, atf.translation));
                                    // Only a pair when the enemy takes its half: one that refuses
                                    // reactions (getting up, held by its map script) left Wolf in the
                                    // pose with nothing to finish (the user's Isshin report: "the close
                                    // look for the second, M1 does nothing").
                                    let def_state = pose.as_ref().map(|th| format!("ThrowDef{}", th.def_anim));
                                    if let (true, Some(ds)) = (attacker_broken, def_state.as_ref()) {
                                        let yaw0 = aa.yaw;
                                        aa.yaw = yaw_to(atf.translation, dtf.translation);
                                        e.react_from(aa, &combat, da.side == Side::Player, ds);
                                        if aa.state != *ds {
                                            aa.yaw = yaw0;
                                        }
                                    }
                                    let pose = pose.filter(|_| def_state.as_ref().is_some_and(|ds| aa.state == *ds));
                                    if let (true, Some(th)) = (attacker_broken, pose) {
                                        da.play("ThrowBreak", &th.atk_anim);
                                        da.move_vel = Vec3::ZERO;
                                        da.yaw = yaw_to(dtf.translation, atf.translation);
                                        if let Some(t) = throw.as_deref_mut() {
                                            t.0 = Some(crate::player::ThrowHold::new(att_e, &th, "ThrowBreak"));
                                        }
                                    } else if attacker_broken {
                                        let s = if jd == 2 { "AttackBoundEmptyStaminaEnemy_Left" } else { "AttackBoundEmptyStaminaEnemy_Right" };
                                        e.react_from(aa, &combat, da.side == Side::Player, s);
                                    } else if jd == 1 || jd == 2 {
                                        e.react_from(aa, &combat, da.side == Side::Player, if jd == 1 { "AttackBoundEnemy1_Right" } else { "AttackBoundEnemy1_Left" });
                                        e.add_clash(if jd == 1 { 200200 } else { 200201 });
                                    } else if (11..=14).contains(&jd) {
                                        // Additive flinch only: the attack carries on (HKS ExecAttackAddJustBound:
                                        // AttackJustGuardBound_Add0N = anim 9600 + N - 1, when the chr has it).
                                        e.add_clash(200205);
                                        start_additive(ad, aa, &format!("AttackJustGuardBound_Add0{}", jd - 10));
                                    }
                                }
                            }
                            Side::Player => {
                                if jd == 1 || jd == 2 {
                                    aa.play_state(ad, if jd == 1 { "HardDeflectedR" } else { "HardDeflectedL" });
                                    aa.move_vel = Vec3::ZERO;
                                }
                            }
                        }
                        log.push(
                            format!(
                                "{} DEFLECT  frame {:.0} of {:.0}-frame window  (attacker posture x{rate})",
                                if da.side == Side::Player { "YOU" } else { "ENEMY" },
                                (at * TAE_FPS).floor() + 1.0,
                                (window * TAE_FPS).round()
                            ),
                            Color::srgb(1.0, 0.85, 0.3),
                        );
                        let sfx = crate::vfx::hit_sfx(&combat, &atk, crate::vfx::Kind::Deflect, [-1, -1]);
                        commands.spawn(crate::vfx::Clash::bundle_fx(mid, (atf.translation - dtf.translation).with_y(0.0), crate::vfx::Kind::Deflect, sfx));
                        if let Some(q) = sounds.as_mut() {
                            for se in crate::sound::guard_sounds(&combat, &atk, true) {
                                q.0.push((se, mid));
                            }
                        }
                    }
                    Outcome::Block => {
                        if aa.side == Side::Enemy && da.side == Side::Player {
                            for m in statuses.hit_wolf(&combat, def_e, config.player.combat_art, &atk.sp_effects(), crate::status::Guard::Block) {
                                log.push(format!("Wolf: {m}"), Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        if let Some(q) = sounds.as_mut() {
                            for se in crate::sound::guard_sounds(&combat, &atk, false) {
                                q.0.push((se, mid));
                            }
                        }
                        if aa.side == Side::Player && da.side == Side::Enemy {
                            let lines = statuses.hit(&combat, def_e, da, &atk.sp_effects(), true);
                            // Catching fire flails him through his guard (c9997 ExecDebuffReaction runs
                            // before ExecGuardBlock; see prosthetic.rs fly_bullets).
                            if lines.iter().any(|l| l == "BURNING") && dd.states.contains_key("FireReaction") && !da.posture_broken() {
                                if let Some(e) = de.as_deref_mut() {
                                    e.react_from(da, &combat, aa.side == Side::Player, "FireReaction");
                                }
                            }
                            for m in lines {
                                log.push(m, Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        let left = apply_knockback(&combat, da, atf.translation, dtf.translation, Some(aa.forward()), atk.knockback_guard, guard_kb(level), "knockbackRate_vsPlayer_Guard");
                        // A blocked bullet leaves its shooter alone too (see the deflect above).
                        if !bullet {
                            recoil(&combat, aa, atf.translation, dtf.translation, left);
                        }
                        let skill = if da.side == Side::Player { guard_skill_rate(&combat, &config, atk.stamina_physics_attribute, false) } else { def_stamina_rate(&combat, da, atk.stamina_physics_attribute) };
                        da.add_posture((atk.repel_lost_stam_damage * aa.stam_atk_rate * skill).floor());
                        if !bullet {
                            aa.add_posture(atk.repel_victory_stam_damage_attacker);
                        }
                        let broken = da.posture_broken();
                        // HKS ExecAttackAddBound: a blocked NPC attack with guard bound type BOUND_ADD01-04
                        // (11-14) flinches additively (AttackGuardBound_Add0N = anim 9700 + N - 1).
                        if aa.side == Side::Enemy && !bullet && (11..=14).contains(&atk.deflect_action) {
                            start_additive(ad, aa, &format!("AttackGuardBound_Add0{}", atk.deflect_action - 10));
                        }
                        match da.side {
                            Side::Player => {
                                if broken {
                                    // HKS DAMAGE_TYPE_GUARDBREAK -> W_StandDeflectBreak (a050_190100), in the
                                    // air W_AirDeflectGuardBreak (a050_190150).
                                    let gb = if da.airborne { "AirDeflectGuardBreak" } else { "StandDeflectBreak" };
                                    da.play_state(dd, gb);
                                    da.posture = 0.0;
                                    log.push("GUARD BROKEN", Color::srgb(1.0, 0.3, 0.2));
                                } else if umbrella {
                                    umbrella_reaction(dd, da, level, false, false, atf.translation, dtf.translation);
                                    log.push(format!("umbrella  (+{:.0} posture)", atk.repel_lost_stam_damage), Color::srgb(0.7, 0.7, 0.8));
                                } else {
                                    // HKS: Selector_DeflectVariation = math.random() > 0.5.
                                    let variant = coin_flip();
                                    if !additive_guard(dd, da, level) {
                                        let state = player_guard_state(&combat, false, level, lr(atk.deflect_action), variant, da.airborne);
                                        da.play_state(dd, &state);
                                    } else {
                                        let flinch = if da.airborne { "AirDeflectEasyMinimumAdd" } else { "StandDeflectEasyMinimum" };
                                    start_additive(dd, da, flinch);
                                    }
                                    log.push(format!("blocked  (+{:.0} posture)", atk.repel_lost_stam_damage), Color::srgb(0.7, 0.7, 0.8));
                                }
                            }
                            Side::Enemy => {
                                if let Some(e) = de.as_deref_mut() {
                                    if broken {
                                        e.react_from(da, &combat, aa.side == Side::Player, if atk.deflect_action == 2 { "GuardBreakLeft" } else { "GuardBreakRight" });
                                    } else {
                                        // guard_damage_table[damage level][NpcParam guardLevel] (c9997.lua).
                                        let refs = crate::prosthetic::enemy_refs(&combat, da, e);
                                        let glv = combat.npc(&da)["guardLevel"].as_i64().unwrap_or(4);
                                        let pick = enemy_guard_block(dd, &refs, level, glv, atk.deflect_action == 2, enemy_is_aerial(&combat, da, e, dtf.translation.y));
                                        guard_pick(dd, da, e, &combat, aa.side == Side::Player, pick, atf.translation, dtf.translation);
                                    }
                                    e.add_clash(if atk.deflect_action == 1 { 200216 } else { 200215 });
                                }
                                log.push(format!("enemy blocked  (+{:.0} posture)", atk.repel_lost_stam_damage), Color::srgb(0.7, 0.7, 0.8));
                            }
                        }
                        let dact = atk.deflected_action;
                        match aa.side {
                            _ if bullet => {}
                            Side::Enemy => {
                                if let Some(e) = ae.as_deref_mut() {
                                    if dact == 1 || dact == 2 {
                                        e.react_from(aa, &combat, da.side == Side::Player, if dact == 1 { "AttackNoBoundEnemy1_Right" } else { "AttackNoBoundEnemy1_Left" });
                                    }
                                }
                            }
                            Side::Player => {
                                if dact == 1 || dact == 2 {
                                    aa.play_state(ad, if dact == 1 { "EasyDeflectedR" } else { "EasyDeflectedL" });
                                    aa.move_vel = Vec3::ZERO;
                                }
                            }
                        }
                        let sfx = crate::vfx::hit_sfx(&combat, &atk, crate::vfx::Kind::Guard, [-1, -1]);
                        commands.spawn(crate::vfx::Clash::bundle_fx(mid, (atf.translation - dtf.translation).with_y(0.0), crate::vfx::Kind::Guard, sfx));
                    }
                    Outcome::Hit => {
                        // ChrActionFlag 24 on the defender: no damage motion and no knockback (nor
                        // the attacker's leftover recoil); HP and posture still land (exe b6bfe0;
                        // sekiro-rs fight.rs:660-666, MIT / Apache-2.0).
                        let no_motion = !da.anim.is_empty() && dd.flag(&da.anim, da.t, FLAG_NO_DAMAGE_MOTION);
                        let left = if no_motion {
                            0.0
                        } else {
                            apply_knockback(&combat, da, atf.translation, dtf.translation, Some(aa.forward()), atk.knockback_hit, damage_kb(level), "knockbackRate_vsPlayer_DirectHit")
                        };
                        if !bullet {
                            recoil(&combat, aa, atf.translation, dtf.translation, left);
                        }
                        let mut ignited = false;
                        if aa.side == Side::Enemy && da.side == Side::Player {
                            for m in statuses.hit_wolf(&combat, def_e, config.player.combat_art, &atk.sp_effects(), crate::status::Guard::None) {
                                log.push(format!("Wolf: {m}"), Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        if aa.side == Side::Player && da.side == Side::Enemy {
                            for m in statuses.hit(&combat, def_e, da, &atk.sp_effects(), false) {
                                ignited |= m == "BURNING";
                                log.push(m, Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        // c9997 GetSpDamage: env(3041) GetStatusActivation STATUS_BURNING (the burn gauge
                        // broke on this hit: SprjChrResistModule +0x4c -> +0x50) with anim 8900 is
                        // SP_DAMAGE_BURNING -> ExecDebuffReaction -> W_FireReaction, rank 3.
                        let burning = ignited
                            && dd.states.contains_key("FireReaction")
                            && crate::prosthetic::rank_allows(crate::prosthetic::transition_rank(&da.state), 3);
                        // NpcParam def_* = 0; its per-attribute rates come with armor_rates below.
                        let weapon = if crate::player::is_tool_anim(&aa.anim) { tool_weapon.unwrap_or(5000) } else { 5000 };
                        let dmg = match aa.side {
                            Side::Player => player_attack_damage(&combat, &atk, weapon, &da),
                            Side::Enemy => atk.npc_damage() * aa.atk_rate,
                        };
                        // An armoured enemy (enemy.rs Armor): Wolf's hit lands on the armour part,
                        // which passes on x0.35 HP / x0.5 posture; its prosthetic (Loaded Spear,
                        // EquipParamWeapon 78xxx = Prosthetic Type 11) breaks it.
                        let (armor_hp, armor_posture) = match de.as_deref_mut() {
                            Some(e) if aa.side == Side::Player => {
                                let (hp, posture, broke) = armor_rates(&combat, e, da, &atk, Some(weapon));
                                if broke {
                                    log.push("armour broken", Color::srgb(0.9, 0.8, 0.5));
                                }
                                (hp, posture)
                            }
                            _ => (1.0, 1.0),
                        };
                        let dmg = dmg * armor_hp;
                        da.hp = (da.hp - dmg).max(0.0);
                        let pd = aa.stam_atk_rate * if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
                        let def_rate = def_stamina_rate(&combat, da, atk.stamina_physics_attribute) * armor_posture;
                        da.add_posture(pd * def_rate);
                        match da.side {
                            Side::Player => {
                                if (da.posture_broken() || is_player_broken(&da.state)) && !broken_takes_normal_damage(level, da) {
                                    let state = player_break_state(&combat, level, da, atf.translation, dtf.translation);
                                    if da.posture_broken() {
                                        log.push("POSTURE BROKEN", Color::srgb(1.0, 0.3, 0.2));
                                    }
                                    if !state.is_empty() {
                                        da.play_state(dd, &state);
                                    }
                                    da.posture = 0.0;
                                } else if no_motion {
                                    // Level 0 (GroundWireShoot a000_202000 frames 0-19, AirWireShoot):
                                    // the grapple goes on.
                                } else {
                                    let state = if da.airborne {
                                        player_air_damage_state(&combat, level, da, atf.translation, dtf.translation)
                                    } else {
                                        player_damage_state(&combat, level, da, atf.translation, dtf.translation)
                                    };
                                    if let Some(state) = state {
                                        da.play_state(dd, &state);
                                    } else if level == 8 && !da.airborne {
                                        // BEH_ADD_R_HIT_DAMAGE: W_AddDamageStart, AddDamageDirection = the hit side.
                                        let fwd = da.forward();
                                        let to = (atf.translation - dtf.translation).with_y(0.0).normalize_or_zero();
                                        let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
                                        let dir = if f.abs() >= r.abs() { if f >= 0.0 { "F" } else { "B" } } else if r > 0.0 { "R" } else { "L" };
                                        start_additive(dd, da, &format!("AddDamageStart_{dir}"));
                                    }
                                }
                                if !no_motion || da.posture_broken() {
                                    da.move_vel = Vec3::ZERO;
                                }
                            }
                            Side::Enemy => {
                                // Super armour: the damage level the HKS sees (env 236) is 0 under
                                // ChrActionFlag 24, else what its spCategory 1001 SpEffects replace it
                                // with (damage_motion_level); minimum (8) / none flinch additively.
                                // NpcParam toughness / superArmorDurability are 0 on the rows checked.
                                if let Some(e) = de.as_deref_mut() {
                                    // Wolf's HP damage: the map AI hold's damage wake (EMEVD 4[1] /
                                    // 3[23]: a record for final HP damage > 0), with or without a
                                    // reaction.
                                    if aa.side == Side::Player && dmg > 0.0 {
                                        e.hurt_by_wolf = true;
                                    }
                                    let level = if no_motion { 0 } else { damage_motion_level(dd, da, level) };
                                    // c9997 GetSpDamage: Wolf's lightning (the weak Lightning Reversal's
                                    // 105000281, spAttribute 6) on one with ref 1000041 (NpcParam 6021) and
                                    // not 1000260 -> W_DamageLightningStart, rank 3 (also mid-swing).
                                    let lightning = aa.side == Side::Player && atk.sp_attribute == 6 && {
                                        let refs = crate::prosthetic::enemy_refs(&combat, da, e);
                                        refs.contains(&1000041) && !refs.contains(&1000260)
                                    } && crate::prosthetic::rank_allows(crate::prosthetic::transition_rank(&da.state), 3);
                                    let aerial = enemy_is_aerial(&combat, da, e, dtf.translation.y);
                                    let reaction = if lightning && !da.posture_broken() {
                                        Some("DamageLightningStart".to_string())
                                    } else if burning && !da.posture_broken() {
                                        Some("FireReaction".to_string())
                                    } else {
                                        let can_flinch = !e.is_attacking() || dd.flag(&da.anim, da.t, FLAG_DAMAGE_MOTION);
                                        (can_flinch && !da.posture_broken()).then(|| enemy_damage_state_air(dd, level, da, atf.translation, dtf.translation, aerial)).flatten()
                                    };
                                    match reaction {
                                        Some(state) => e.react_from(da, &combat, aa.side == Side::Player, &state),
                                        // c9997 ExecNoSyncAddDamage: a hit with no reaction (rejected by the swing,
                                        // or minimum level) flinches additively.
                                        None if !da.posture_broken() => no_sync_add_damage(dd, da, part, atf.translation, dtf.translation),
                                        None => {}
                                    }
                                }
                            }
                        }
                        log.push(
                            format!("{} hit: -{dmg:.0} HP, +{pd:.0} posture", if aa.side == Side::Player { "you" } else { "enemy" }),
                            Color::srgb(1.0, 0.45, 0.4),
                        );
                        let def_mats = crate::vfx::defender_sfx_materials(&combat, da);
                        let sfx = crate::vfx::hit_sfx(&combat, &atk, crate::vfx::Kind::Hit, def_mats);
                        commands.spawn(crate::vfx::Clash::bundle_fx(mid, (atf.translation - dtf.translation).with_y(0.0), crate::vfx::Kind::Hit, sfx));
                        commands.spawn(crate::gore::HitStains { ids: crate::gore::hit_stain_ids(&atk, def_mats[0]), at: mid, dir: dtf.translation - atf.translation });
                        if let Some(q) = sounds.as_mut() {
                            for se in crate::sound::hit_sounds(&combat, &atk, crate::sound::defender_materials(&combat, da)) {
                                q.0.push((se, mid));
                            }
                        }
                    }
                }
            }
        }
        // Posture breaks for an enemy filled up as attacker (deflected) or defender (hit / blocked).
        let positions = [x.2.translation, y.2.translation];
        for (i, (_, a, tf, e, _, _)) in [&mut x, &mut y].into_iter().enumerate() {
            // Vitality emptied works like a posture break in Sekiro: the deathblow opens.
            if a.side == Side::Enemy && (a.posture_broken() || a.hp <= 0.0) {
                if let Some(e) = e {
                    if !e.is_broken() && !e.is_dead() {
                        // Against Wolf, not the pair partner: a break from his shuriken or a
                        // poison tick is seen first by an enemy-enemy pair with several enemies.
                        let other = wolf_at.unwrap_or(positions[1 - i]);
                        let from_behind = a.forward().dot((other - tf.translation).with_y(0.0).normalize_or_zero()) < 0.0;
                        let why = if a.hp <= 0.0 { "VITALITY EMPTY" } else { "POSTURE BROKEN" };
                        e.on_posture_break(a, &combat, from_behind, config.enemy.broken_time);
                        log.push(format!("{why} - deathblow!"), Color::srgb(1.0, 0.15, 0.15));
                    }
                }
            }
        }
    }
    // Remember every dummy's position for next tick's swept capsules.
    prev_dmy.clear();
    for (_, _, _, _, dummies, _) in &q {
        for &m in dummies.into_iter().flat_map(|d| d.0.values()) {
            if let Ok(g) = dummy_tf.get(m) {
                prev_dmy.insert(m, g.translation());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combat() -> Combat {
        let path = crate::paths::root().join("extracted/combat_data.json");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        Combat {
            player: serde_json::from_value(v["player"].clone()).unwrap(),
            kinds: vec![crate::data::EnemyKind { foe: crate::data::Foe::for_chr("c1020"), data: serde_json::from_value(v["enemy"].clone()).unwrap() }],
            params: v["params"].clone(),
            rumble: Default::default(),
            twists: Default::default(),
            names: Default::default(),
        }
    }

    /// A deflect break pairs only in the 弾き0 row's reach (ThrowParam 11020010: Dist 4, Y ranges
    /// 2.4 m): the dog spitting from 8 m or a Nightjar 3 m up on a roof is not pulled onto Wolf
    /// (2026-10-12 bug hunt round 6).
    #[test]
    fn a_deflect_break_pairs_only_in_reach() {
        let c = combat();
        let th = c.throw(11020010).unwrap();
        let wolf = Vec3::ZERO;
        let reach = |at: Vec3| throw_in_reach(&c, 11020010, &th, wolf, at);
        assert!(reach(Vec3::new(0.0, 0.0, 2.5)));
        assert!(!reach(Vec3::new(0.0, 0.0, 8.0)), "pulled from 8 m");
        assert!(!reach(Vec3::new(0.0, 3.0, 1.0)), "pulled off a roof 3 m up");
        assert!(!reach(Vec3::new(0.0, -3.0, 1.0)), "pulled from 3 m below");
    }

    /// The clock a hit stop takes: 0.06 s held, then (elapsed / length)^2 over the length, so
    /// 0.1 s costs about 0.127 s.
    fn clock_lost(h: &mut crate::actor::HitStop) -> f32 {
        let dt = 1.0 / 600.0;
        let mut lost = 0.0;
        for _ in 0..2000 {
            lost += dt - h.split(dt);
        }
        lost
    }

    /// Hit stop as the exe (FUN_140b9cd90 / FUN_140b9cfb0): a defender of NpcParam hitStopType 0
    /// (most enemies, the General too) stops nobody; type 1 halves the attacker's, type 2 the
    /// full time; hitStopType_Defencer 1 stops the defender; an enemy hit on Wolf stops both.
    #[test]
    fn hit_stop_follows_the_defenders_type() {
        let mut c = combat();
        let row = c.kinds[0].foe.npc_row.to_string();
        let atk = Attack { hit_stop_time: 0.1, hit_stop_time_defencer: 0.1, ..Default::default() };
        let run = |c: &Combat, enemy_defends: bool| {
            let mut wolf = Actor::new(Side::Player, 1.0, 1.0, 0.0, 0);
            let mut foe = Actor::new(Side::Enemy, 1.0, 1.0, 0.0, 0);
            if enemy_defends {
                start_hit_stop(c, &mut wolf, &mut foe, &c.kinds[0].data, &atk, false);
            } else {
                start_hit_stop(c, &mut foe, &mut wolf, &c.player, &atk, false);
            }
            let (w, f) = (clock_lost(&mut wolf.hit_stop), clock_lost(&mut foe.hit_stop));
            if enemy_defends { (w, f) } else { (f, w) }
        };
        let set = |c: &mut Combat, k: &str, v: i64| c.params["NpcParam"][&row][k] = v.into();
        set(&mut c, "hitStopType", 0);
        set(&mut c, "hitStopType_Defencer", 0);
        assert_eq!(run(&c, true), (0.0, 0.0));
        set(&mut c, "hitStopType", 2);
        let (atk_lost, def_lost) = run(&c, true);
        assert!((atk_lost - (0.06 + 0.1 * 2.0 / 3.0)).abs() < 0.003 && def_lost == 0.0, "{atk_lost} {def_lost}");
        set(&mut c, "hitStopType", 1);
        let (half, _) = run(&c, true);
        assert!((half - (0.06 + 0.05 * 2.0 / 3.0)).abs() < 0.003, "{half}");
        set(&mut c, "hitStopType_Defencer", 1);
        assert!(run(&c, true).1 > 0.1);
        // An enemy's hit on Wolf: Wolf counts as type 2 and always takes the defender's time.
        let (e, w) = run(&c, false);
        assert!(e > 0.1 && w > 0.1, "{e} {w}");
    }

    #[test]
    fn every_guard_reaction_exists() {
        let c = combat();
        for level in 0..=11 {
            for dir in ["L", "R"] {
                for deflect in [true, false] {
                    let s = player_guard_state(&c, deflect, level, dir, level % 2 == 0, false);
                    assert!(c.player.anim_key(&s).is_some(), "missing player reaction {s} (level {level})");
                }
            }
        }
        for s in [
            "JustGuardDamage_RighttoLeft", "JustGuardDanage_LefttoRight", "GuardDamageSmall_LefttoRight",
            "GuardDamageLarge_RighttoLeft", "AttackBoundEnemy1_Right", "AttackBoundEnemy1_Left",
            "AttackNoBoundEnemy1_Right", "AttackNoBoundEnemy1_Left", "DamageSmall_Front", "WalkFrontBattle",
        ] {
            assert!(c.enemy0().anim_key(s).is_some(), "missing enemy state {s}");
        }
        for s in ["HardDeflectedR", "HardDeflectedL", "EasyDeflectedR", "EasyDeflectedL", "HardDeflectStagger_L"] {
            assert!(c.player.anim_key(s).is_some(), "missing player state {s}");
        }
    }
}

/// Throw damage: a ThrowAttackBehavior (TAE 304) in the thrower's anim lands on the
/// grabbed player (AtkParam throwFlag 2, e.g. judge 230: 192 physical). A grabbed
/// player is released when the thrower is knocked out of its throw anim before that anim's end
/// (staggered, posture-broken; a zombie's bite loop); after a throw that played out, his own
/// clip (the fall and getting up) plays out.
fn resolve_throws(combat: Res<Combat>, mut log: Option<ResMut<CombatLog>>, mut q: Query<&mut Actor>, mut was: Local<Option<(String, f32, usize)>>) {
    // (judge, thrower kind, its atk_rate and stam_atk_rate) of a throw landing this frame.
    let mut lands: Option<(i64, usize, f32, f32)> = None;
    let mut throwing = false;
    let mut now = None;
    for a in &q {
        if a.side != Side::Enemy || !a.state.starts_with("ThrowAtk") {
            continue;
        }
        throwing = true;
        now = Some((a.anim.clone(), a.t, a.kind));
        if let Some(anim) = combat.data_of(a).anim(&a.anim) {
            lands = anim
                .events
                .iter()
                .find(|e| e.kind == 304 && e.start > a.prev_t && e.start <= a.t)
                .and_then(|e| e.arg_i64("BehaviorJudgeID"))
                .map(|j| (j, a.kind, a.atk_rate, a.stam_atk_rate))
                .or(lands);
        }
    }
    // Left the throw anim more than a few frames before its end: knocked out of it.
    let cut = !throwing
        && was.as_ref().is_some_and(|(anim, t, kind)| {
            let d = &combat.kind(*kind).data;
            d.anim(anim).is_some() && *t + 0.1 < d.length(anim)
        });
    *was = now;
    for mut a in &mut q {
        if a.side != Side::Player || a.state != "Grabbed" {
            continue;
        }
        if let Some((atk, rate, stam_rate)) = lands.and_then(|(j, kind, r, sr)| combat.kind(kind).data.attacks.get(&j.to_string()).map(|atk| (atk, r, sr))) {
            // The thrower's area-doping rates, as resolve applies them to a hit.
            let pd = stam_rate * if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
            a.hp = (a.hp - atk.atk_phys * rate).max(0.0);
            a.add_posture(pd);
            // The grabbed clip already contains the throw and the fall; without it, a large hit.
            if a.anim.is_empty() {
                a.play_state(&combat.player, "StandDamageLarge_F");
            }
            if let Some(log) = log.as_mut() {
                log.push(format!("thrown: -{:.0} HP, +{pd:.0} posture", atk.atk_phys), Color::srgb(1.0, 0.45, 0.4));
            }
        } else if cut || (a.anim.is_empty() && !throwing) || (!a.anim.is_empty() && a.t >= combat.player.length(&a.anim)) {
            a.play("StandIdle", "");
        }
    }
}
