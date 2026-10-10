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
pub(crate) fn apply_knockback(combat: &Combat, da: &mut Actor, from: Vec3, at: Vec3, dist: f32, kind: &str, cut_field: &str) {
    let row_id = match da.side {
        Side::Enemy => combat.param("NpcParam", combat.foe.npc_row)["knockbackParamId"].as_i64().unwrap_or(0),
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
        data_for(combat, da.side)
            .events_at(&da.anim, da.t)
            .find(|e| e.kind == 226)
            .and_then(|e| e.arg_i64("KnockbackPercent"))
            .map_or(1.0, |p| p as f32 / 100.0)
    };
    let cut = if da.side == Side::Enemy {
        combat.param("NpcParam", combat.foe.npc_row)[cut_field].as_f64().unwrap_or(0.0) as f32
    } else {
        0.0
    };
    let d = dist * (1.0 - cut / 100.0) * percent;
    da.knockback(at - from, d, f(&format!("{kind}_ContTime")), f(&format!("{kind}_DecTime")));
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
fn dodged(combat: &Combat, def: &Actor, att: &Actor, atk: &Attack) -> bool {
    if def.anim.is_empty() {
        return false;
    }
    let dd = data_for(combat, def.side);
    let ad = data_for(combat, att.side);
    // Mikiri takes precedence: a perilous thrust into the step's MikiriCounter window.
    if atk.atk_type == 2 && atk.disable_guard == 1 && dd.flag(&def.anim, def.t, FLAG_MIKIRI) {
        return false;
    }
    let is_sweep = || ad.anim(&att.anim).is_some_and(|a| a.events.iter().any(|e| e.kind == 2 && e.arg_i64("BehaviorJudgeID") == Some(980)));
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

/// AtkParam dummy id -> model dummy: 10000+ addresses the right-hand weapon model's
/// own dummy (id - 10000), e.g. Whirlwind Slash 10120 -> 120.
fn dmy_key(id: i64) -> i16 {
    (if id >= 10000 { id % 10000 } else { id }) as i16
}

/// Player damage: the weapon's attackBase* per element (Kusabimaru 5000; a prosthetic's own
/// EquipParamWeapon row for its a070..a079 swings, e.g. the Loaded Axe 73000 attackBasePhysics 45) times the AtkParam element
/// corrections [%], each scaled by the NPC's NpcParam <element>DamageCutRate (c1020: dark
/// 0.6, others 1.0; NpcParam def_* are 0). Combat arts deal "dark" (Whirlwind Slash
/// atkDarkCorrection 125 on attackBaseDark 40).
fn player_attack_damage(combat: &Combat, atk: &Attack, weapon: i64) -> f32 {
    let w = combat.param("EquipParamWeapon", weapon);
    let npc = combat.param("NpcParam", combat.foe.npc_row);
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
/// c0000_transition.lua): a blow variant by damage level, else StandDamageBreak_F/B; a
/// hit landing while he is already broken (StandDamageBreak / StandDeflectBreak) knocks
/// him down (StandDamageBreakDamage, face down).
pub(crate) fn player_break_state(combat: &Combat, level: i64, defender: &Actor, from: Vec3, at: Vec3) -> String {
    let to = (from - at).with_y(0.0).normalize_or_zero();
    let fb = if to.dot(defender.forward()) >= 0.0 { "F" } else { "B" };
    let candidates = if is_player_broken(&defender.state) {
        vec!["StandDamageBreakDamage".to_string()]
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

/// NPC damage reaction (c9997.lua ExecDamage*): SMALL / MIDDLE -> Damage{Small,Middle}_<dir>,
/// LARGE / FLING / UPPER -> DamageLarge_<dir>, PUSH -> DamagePushFront/Back, SMALL_BLOW / BREATH
/// -> DamageBlowFront/Back (ExecDamageBlow), BLOW (4) / EX_BLAST -> DamageLargeBlow; MINIMUM (8)
/// and NONE play no reaction. Direction from where the attacker stands (GetDirOfPlayableDamage).
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
        for (e, atk, _) in data_for(&combat, a.side).attack_windows(&a.anim) {
            if !e.in_time(a.t) || (atk.atk_stam <= 0.0 && atk.atk_phys <= 0.0) {
                continue;
            }
            let get = |id: i64| d.0.get(&dmy_key(id)).and_then(|e| tfs.get(*e).ok()).map(|g| g.translation());
            if let (Some(p1), Some(p2)) = (get(atk.hit0_dmy1), get(atk.hit0_dmy2)) {
                let red = Color::srgb(1.0, 0.15, 0.1);
                gizmos.line(p1, p2, red);
                gizmos.sphere(Isometry3d::from_translation(p1), atk.hit0_radius, red);
                gizmos.sphere(Isometry3d::from_translation(p2), atk.hit0_radius, red);
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
/// ChrActionFlag 73 ("SetBool32_0x7C_10"): NPC damage motion allowed (see the Hit branch).
const FLAG_DAMAGE_MOTION: i64 = 73;
/// ChrActionFlag 125 "MikiriCounter": on in the forward step (GroundStep_F, frames 0-12).
const FLAG_MIKIRI: i64 = 125;
// ThrowParam "見切り PC -> enemy" (mikiri, suffix 0190): player a201_511800, enemy ThrowDef13800.

/// c9997.lua guard_damage_table[damage_level][guard_level]: NONE 0, SMALL 1, LARGE 3, EXLARGE 4,
/// ADD 5. Returns the GuardDamage size, None for NONE / ADD (no state change). c1020 has
/// no GuardDamageExLarge, so EXLARGE plays Large.
fn enemy_guard_reaction(level: i64, guard_level: i64) -> Option<&'static str> {
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
    let row = T.get(level.clamp(0, 11) as usize)?;
    match row[guard_level.clamp(0, 4) as usize] {
        1 => Some("Small"),
        3 | 4 => Some("Large"),
        _ => None,
    }
}

/// Guard arc half-angle [deg]: the active ShieldBlock flag's ArgB (exe FUN_140b6ab40: blocked
/// when facing . attack dir < cos(ArgB + 180 deg); ArgB 0 acts like 90 = the front half).
fn guard_half_angle(d: &crate::data::CharData, a: &Actor) -> f32 {
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
    let field = match attribute {
        1 => "defSlashStaminaDmgRate",
        2 => "defLightHitStaminaDmgRate",
        3 => "defThrustStaminaDmgRate",
        4 => "defNeutralStaminaDmgRate",
        6 => "defHeavyHitStaminaDmgRate",
        _ => return 1.0,
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
    let mut pairs = q.iter_combinations_mut();
    while let Some([mut x, mut y]) = pairs.fetch_next() {
        for flip in [false, true] {
            let (att, def) = if flip { (&mut y, &mut x) } else { (&mut x, &mut y) };
            let (att_e, ref mut aa, atf, ref mut ae, adummies, _) = *att;
            let (def_e, ref mut da, dtf, ref mut de, _, dhurt) = *def;
            let defender_dead = de.as_ref().is_some_and(|e| e.is_dead());
            if aa.hp <= 0.0 || (da.hp <= 0.0 && da.side == Side::Player) || defender_dead || (aa.anim.is_empty() && aa.bullet_hits.is_empty()) {
                continue;
            }
            let ad = data_for(&combat, aa.side);
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
                if !bullet && aa.hits_done.contains(&(judge, start)) {
                    continue;
                }
                let to_def = (dtf.translation - atf.translation).with_y(0.0);
                // Hit capsule between the attack's two dummy polys (AtkParam hit0_DmyPoly1/2, hit0_Radius),
                // swept from where they were on the previous tick so a fast slash cannot pass through a
                // thin limb capsule between ticks.
                let capsule = adummies.and_then(|d| {
                    let (e1, e2) = (*d.0.get(&dmy_key(atk.hit0_dmy1))?, *d.0.get(&dmy_key(atk.hit0_dmy2))?);
                    let p1 = dummy_tf.get(e1).ok()?.translation();
                    let p2 = dummy_tf.get(e2).ok()?.translation();
                    let (q1, q2) = (prev_dmy.get(&e1).copied().unwrap_or(p1), prev_dmy.get(&e2).copied().unwrap_or(p2));
                    Some(std::array::from_fn::<_, SWEEP_STEPS, _>(|i| {
                        let k = i as f32 / (SWEEP_STEPS - 1) as f32;
                        (q1.lerp(p1, k), q2.lerp(p2, k))
                    }))
                });
                let connects = bullet || match capsule {
                    Some(sweep) => sweep.iter().any(|&(p1, p2)| match dhurt {
                        // Ragdoll hurtbox capsules (follow the animation).
                        Some(h) => h.0.iter().any(|&(ma, mb, r)| match (dummy_tf.get(ma), dummy_tf.get(mb)) {
                            (Ok(a), Ok(b)) => segment_distance(p1, p2, a.translation(), b.translation()) <= atk.hit0_radius + r,
                            _ => false,
                        }),
                        None => {
                            let feet = dtf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                            segment_distance(p1, p2, feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5) <= atk.hit0_radius + BODY_RADIUS
                        }
                    }),
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
                if dodged(&combat, da, aa, &atk) {
                    continue;
                }
                aa.hits_done.push((judge, start));
                aa.attack_hit = true;

                let dd = data_for(&combat, da.side);
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
                    let broken_row = aa.posture_broken().then(|| combat.throw(combat.foe.throw_row(120))).flatten().filter(|th| dd.anim(&th.atk_anim).is_some());
                    let (row, state) = match broken_row {
                        Some(th) => (Some(th), "ThrowBreak"),
                        None => (combat.throw(combat.foe.throw_row(190)), "Mikiri"),
                    };
                    if let Some(th) = row {
                        if dd.anim(&th.atk_anim).is_some() {
                            da.play(state, &th.atk_anim);
                            da.move_vel = Vec3::ZERO;
                            da.yaw = yaw_to(dtf.translation, atf.translation);
                            aa.yaw = yaw_to(atf.translation, dtf.translation);
                            if let Some(e) = ae.as_deref_mut() {
                                e.react(aa, &combat, &format!("ThrowDef{}", th.def_anim));
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
                if aa.side == Side::Enemy && atk.throw_flag == 1 && (da.state == "Grabbed" || aa.state.starts_with("ThrowAtk")) {
                    continue;
                }
                if aa.side == Side::Enemy && atk.throw_flag == 1 && !da.airborne {
                    let row = combat.enemy_grab(atk.throw_type_id);
                    if let Some(e) = ae.as_deref_mut() {
                        e.react(aa, &combat, &format!("ThrowAtk{}", row.as_ref().map_or(4100, |t| t.atk_anim_id)));
                    }
                    aa.yaw = yaw_to(atf.translation, dtf.translation);
                    da.yaw = yaw_to(dtf.translation, atf.translation);
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

                let level = atk.level(da.side == Side::Player);
                let mid = (atf.translation + dtf.translation) / 2.0 + Vec3::Y * 0.4;
                match outcome {
                    Outcome::Deflect { window, at, rate } => {
                        apply_knockback(&combat, da, atf.translation, dtf.translation, atk.knockback_just_guard, guard_kb(level), "knockbackRate_vsPlayer_JustGuard");
                        let skill = if da.side == Side::Player { guard_skill_rate(&combat, &config, atk.stamina_physics_attribute, true) } else { 1.0 };
                        let pd = (atk.atk_stam * skill).floor();
                        let full = da.posture + pd >= da.posture_max;
                        // A deflect never breaks the defender (HKS: GUARDATTACKER_STAMZERO -> stagger instead).
                        da.posture = (da.posture + pd).min(da.posture_max - 1.0);
                        da.since_posture_damage = 0.0;
                        // Wolf's deflect: his latent skills' defStaminaAttackRate (270: x1.25).
                        let skill_atk = if da.side == Side::Player { skill_rate(&combat, &config, "defStaminaAttackRate") } else { 1.0 };
                        aa.add_posture(atk.repel_lost_stam_damage_attacker * rate * skill_atk);
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
                                let state = if atk.just_deflect_action == 2 { "JustGuardDanage_LefttoRight" } else { "JustGuardDamage_RighttoLeft" };
                                if let Some(e) = de.as_deref_mut() {
                                    e.react(da, &combat, state);
                                    e.add_clash(if atk.just_deflect_action == 1 { 200211 } else { 200210 });
                                }
                            }
                        }
                        // Attacker reaction.
                        let jd = atk.just_deflected_action;
                        match aa.side {
                            Side::Enemy => {
                                if let Some(e) = ae.as_deref_mut() {
                                    // A deflect that empties the enemy's posture plays the break pose
                                    // pair 弾き0 (ThrowParam 0010: Wolf a20x_510100, enemy ThrowDef12100,
                                    // held on Wolf's dummy atkSorbDmyId); player.rs takes it on to the
                                    // deathblow. Live: the deflect frame goes straight into both. An air
                                    // deflect break keeps AttackBoundEmptyStamina (rec_c1010_b: Wolf
                                    // AirDeflectHardLarge_L, enemy 8650, front deathblow after landing).
                                    let pose = (da.side == Side::Player && !da.airborne)
                                        .then(|| combat.throw(combat.foe.throw_row(10)))
                                        .flatten()
                                        .filter(|th| dd.anim(&th.atk_anim).is_some());
                                    if let (true, Some(th)) = (attacker_broken, pose) {
                                        da.play("ThrowBreak", &th.atk_anim);
                                        da.move_vel = Vec3::ZERO;
                                        da.yaw = yaw_to(dtf.translation, atf.translation);
                                        aa.yaw = yaw_to(atf.translation, dtf.translation);
                                        e.react(aa, &combat, &format!("ThrowDef{}", th.def_anim));
                                        if let Some(t) = throw.as_deref_mut() {
                                            t.0 = Some(crate::player::ThrowHold::new(att_e, &th, "ThrowBreak"));
                                        }
                                    } else if attacker_broken {
                                        let s = if jd == 2 { "AttackBoundEmptyStaminaEnemy_Left" } else { "AttackBoundEmptyStaminaEnemy_Right" };
                                        e.react(aa, &combat, s);
                                    } else if jd == 1 || jd == 2 {
                                        e.react(aa, &combat, if jd == 1 { "AttackBoundEnemy1_Right" } else { "AttackBoundEnemy1_Left" });
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
                        if let Some(q) = sounds.as_mut() {
                            for se in crate::sound::guard_sounds(&combat, &atk, false) {
                                q.0.push((se, mid));
                            }
                        }
                        if aa.side == Side::Player && da.side == Side::Enemy {
                            let lines = statuses.hit(&combat, def_e, &atk.sp_effects(), true);
                            // Catching fire flails him through his guard (gap: as prosthetic.rs fly_bullets).
                            if lines.iter().any(|l| l == "BURNING") && dd.states.contains_key("FireReaction") && !da.posture_broken() {
                                if let Some(e) = de.as_deref_mut() {
                                    e.react(da, &combat, "FireReaction");
                                }
                            }
                            for m in lines {
                                log.push(m, Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        apply_knockback(&combat, da, atf.translation, dtf.translation, atk.knockback_guard, guard_kb(level), "knockbackRate_vsPlayer_Guard");
                        let skill = if da.side == Side::Player { guard_skill_rate(&combat, &config, atk.stamina_physics_attribute, false) } else { 1.0 };
                        da.add_posture((atk.repel_lost_stam_damage * aa.stam_atk_rate * skill).floor());
                        aa.add_posture(atk.repel_victory_stam_damage_attacker);
                        let broken = da.posture_broken();
                        // HKS ExecAttackAddBound: a blocked NPC attack with guard bound type BOUND_ADD01-04
                        // (11-14) flinches additively (AttackGuardBound_Add0N = anim 9700 + N - 1).
                        if aa.side == Side::Enemy && (11..=14).contains(&atk.deflect_action) {
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
                                    let side = if atk.deflect_action == 2 { "LefttoRight" } else { "RighttoLeft" };
                                    if broken {
                                        e.react(da, &combat, if atk.deflect_action == 2 { "GuardBreakLeft" } else { "GuardBreakRight" });
                                    } else {
                                        // guard_damage_table[damage level][NpcParam guardLevel] (c9997.lua).
                                        let glv = combat.param("NpcParam", combat.foe.npc_row)["guardLevel"].as_i64().unwrap_or(4);
                                        match enemy_guard_reaction(level, glv) {
                                            Some(size) => e.react(da, &combat, &format!("GuardDamage{size}_{side}")),
                                            // GUARD_LEVEL_ADD: W_SABlend_Add_{Front,Left,Right,Back} (anim 9500+),
                                            // by the hit's direction relative to the enemy.
                                            None => {
                                                let fwd = da.forward();
                                                let to = (atf.translation - dtf.translation).with_y(0.0).normalize_or_zero();
                                                let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
                                                let dir = if f.abs() >= r.abs() { if f >= 0.0 { "Front" } else { "Back" } } else if r > 0.0 { "Right" } else { "Left" };
                                                start_additive(dd, da, &format!("SABlend_Add_{dir}"));
                                            }
                                        }
                                    }
                                    e.add_clash(if atk.deflect_action == 1 { 200216 } else { 200215 });
                                }
                                log.push(format!("enemy blocked  (+{:.0} posture)", atk.repel_lost_stam_damage), Color::srgb(0.7, 0.7, 0.8));
                            }
                        }
                        let dact = atk.deflected_action;
                        match aa.side {
                            Side::Enemy => {
                                if let Some(e) = ae.as_deref_mut() {
                                    if dact == 1 || dact == 2 {
                                        e.react(aa, &combat, if dact == 1 { "AttackNoBoundEnemy1_Right" } else { "AttackNoBoundEnemy1_Left" });
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
                        apply_knockback(&combat, da, atf.translation, dtf.translation, atk.knockback_hit, damage_kb(level), "knockbackRate_vsPlayer_DirectHit");
                        if aa.side == Side::Player && da.side == Side::Enemy {
                            for m in statuses.hit(&combat, def_e, &atk.sp_effects(), false) {
                                log.push(m, Color::srgb(0.7, 0.55, 0.9));
                            }
                        }
                        // c9997 GetSpDamage: a hit on a burning enemy (STATUS_BURNING) with anim 8900 is
                        // SP_DAMAGE_BURNING -> ExecDebuffReaction -> W_FireReaction (gap: rank 3 not checked).
                        let burning = da.side == Side::Enemy && statuses.has_ref(&combat, def_e, crate::status::REF_BURNING) && dd.states.contains_key("FireReaction");
                        // NpcParam def_* = 0 and *DamageCutRate = 1.0 for c1020: no reduction.
                        let dmg = match aa.side {
                            Side::Player => player_attack_damage(&combat, &atk, if crate::player::is_tool_anim(&aa.anim) { tool_weapon.unwrap_or(5000) } else { 5000 }),
                            Side::Enemy => atk.npc_damage() * aa.atk_rate,
                        };
                        da.hp = (da.hp - dmg).max(0.0);
                        let pd = aa.stam_atk_rate * if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
                        da.add_posture(pd);
                        match da.side {
                            Side::Player => {
                                if da.posture_broken() || is_player_broken(&da.state) {
                                    let state = player_break_state(&combat, level, da, atf.translation, dtf.translation);
                                    if da.posture_broken() {
                                        log.push("POSTURE BROKEN", Color::srgb(1.0, 0.3, 0.2));
                                    }
                                    if !state.is_empty() {
                                        da.play_state(dd, &state);
                                    }
                                    da.posture = 0.0;
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
                                da.move_vel = Vec3::ZERO;
                            }
                            Side::Enemy => {
                                // Super armour: NpcParam toughness / superArmorDurability are 0 for c1020,
                                // so the engine's damage level (HKS env 236) drops to an additive flinch
                                // during swings. Inferred from the TAE: ChrActionFlag 73 marks the frames
                                // where damage motion is allowed (idle, windup start, recovery, reactions)
                                // and is off through every attack's active part.
                                if let Some(e) = de.as_deref_mut() {
                                    let can_flinch = !e.is_attacking() || dd.flag(&da.anim, da.t, FLAG_DAMAGE_MOTION);
                                    let reaction = if burning && !da.posture_broken() {
                                        Some("FireReaction".to_string())
                                    } else {
                                        (can_flinch && !da.posture_broken()).then(|| enemy_damage_state(level, da, atf.translation, dtf.translation)).flatten()
                                    };
                                    match reaction {
                                        Some(state) => e.react(da, &combat, &state),
                                        // c9997 ExecNoSyncAddDamage: a hit with no reaction (rejected by the swing,
                                        // or minimum level) flinches additively - PartBlend_Add by body part when
                                        // the chr has those anims (c1020 / c1010 don't), else SABlend_Add_<dir>.
                                        None if !da.posture_broken() => {
                                            let fwd = da.forward();
                                            let to = (atf.translation - dtf.translation).with_y(0.0).normalize_or_zero();
                                            let (f, r) = (to.dot(fwd), to.dot(fwd.cross(Vec3::Y)));
                                            let dir = if f.abs() >= r.abs() { if f >= 0.0 { "Front" } else { "Back" } } else if r > 0.0 { "Right" } else { "Left" };
                                            start_additive(dd, da, &format!("SABlend_Add_{dir}"));
                                        }
                                        None => {}
                                    }
                                }
                            }
                        }
                        aa.hit_stop = atk.hit_stop_time;
                        da.hit_stop = atk.hit_stop_time_defencer;
                        log.push(
                            format!("{} hit: -{dmg:.0} HP, +{pd:.0} posture", if aa.side == Side::Player { "you" } else { "enemy" }),
                            Color::srgb(1.0, 0.45, 0.4),
                        );
                        let sfx = crate::vfx::hit_sfx(&combat, &atk, crate::vfx::Kind::Hit, crate::vfx::defender_sfx_materials(&combat, da.side));
                        commands.spawn(crate::vfx::Clash::bundle_fx(mid, (atf.translation - dtf.translation).with_y(0.0), crate::vfx::Kind::Hit, sfx));
                        if let Some(q) = sounds.as_mut() {
                            for se in crate::sound::hit_sounds(&combat, &atk, crate::sound::defender_materials(&combat, da.side)) {
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
                        let other = positions[1 - i];
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
            enemy: serde_json::from_value(v["enemy"].clone()).unwrap(),
            foe: crate::data::Foe::for_chr("c1020"),
            params: v["params"].clone(),
            rumble: Default::default(),
            twists: Default::default(),
            names: Default::default(),
        }
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
            assert!(c.enemy.anim_key(s).is_some(), "missing enemy state {s}");
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
fn resolve_throws(combat: Res<Combat>, mut log: Option<ResMut<CombatLog>>, mut q: Query<&mut Actor>, mut was: Local<Option<(String, f32)>>) {
    let mut lands: Option<i64> = None;
    let mut throwing = false;
    let mut now = None;
    for a in &q {
        if a.side != Side::Enemy || !a.state.starts_with("ThrowAtk") {
            continue;
        }
        throwing = true;
        now = Some((a.anim.clone(), a.t));
        if let Some(anim) = combat.enemy.anim(&a.anim) {
            lands = anim
                .events
                .iter()
                .find(|e| e.kind == 304 && e.start > a.prev_t && e.start <= a.t)
                .and_then(|e| e.arg_i64("BehaviorJudgeID"))
                .or(lands);
        }
    }
    // Left the throw anim more than a few frames before its end: knocked out of it.
    let cut = !throwing && was.as_ref().is_some_and(|(anim, t)| combat.enemy.anim(anim).is_some() && *t + 0.1 < combat.enemy.length(anim));
    *was = now;
    for mut a in &mut q {
        if a.side != Side::Player || a.state != "Grabbed" {
            continue;
        }
        if let Some(atk) = lands.and_then(|j| combat.enemy.attacks.get(&j.to_string())) {
            let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
            a.hp = (a.hp - atk.atk_phys).max(0.0);
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
