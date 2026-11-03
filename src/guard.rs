//! Guarding as the exe resolves it: the guard row, block versus deflect by repel values, and the
//! posture and HP a guard lets through (FUN_140b6c880 and the cuts it calls). Ported after
//! sekiro-rs crates/sim/src/combat (hit.rs, posture.rs, hp.rs; MIT / Apache-2.0), every number
//! read from the params (atlas combat.block-vs-deflect-repel, combat.posture-attribute-rates,
//! combat.wolf-guard-posture-cut, combat.enemy-guard-block-reduction).

use serde_json::Value;

/// SpEffect stateInfo of the block family (Wolf's deflect window 105010, the Flowing Water
/// block cut 150420): counts on a block.
pub const STATE_INFO_GUARD: i64 = 158;
/// SpEffect stateInfo of the deflect family (105020-105023, 150421): counts on a deflect.
pub const STATE_INFO_JUST_GUARD: i64 = 204;

/// The guard action's row: the ChrActionFlag 3 (ShieldBlock) event's ArgB judge -> BehaviorParam
/// (NPC: 200000000 + NpcParam behaviorVariationId x 1000 + judge) or BehaviorParam_PC
/// (100000000 + the weapon's behaviorVariationId x 1000 + judge) -> AtkParam. Wolf's sword:
/// judge 90 -> 105000090 -> 5000090 "sword_guard"; the soldier's 900 -> 10100900; the Loaded
/// Umbrella's 100 -> 107600100 -> 7600100.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuardRow {
    /// An NPC's repel defence (guardBreakRate: 20 on the soldier's, the General's, the Owl's).
    pub break_rate: f32,
    /// Wolf's repel defence in percent of the weapon's guardBaseRepel (guardBreakCorrection 100).
    pub break_correction: f32,
    /// guardAttribute: the disableGuard / disableJustGuard column an attack is read by (0 the
    /// sword, 1 the Loaded Umbrella and the shield servant's 11804900).
    pub attribute: i64,
    /// guardStaminaCutRate [%]: scales the posture cut.
    pub stamina_cut_rate: f32,
    /// guardRate [%]: scales the physical HP cut.
    pub guard_rate: f32,
    /// The BehaviorParam row's stamina, added to the guarded posture.
    pub behavior_stamina: f32,
}

impl GuardRow {
    /// The row of guard judge `judge`. A PC variation without its own row takes its base
    /// variation's (variation / 100 x 100): the Loaded Umbrella LV3 7602 / 7603 define only their
    /// own few rows (107602297, 107603292-293), everything else is 7600's.
    pub fn of(pc: bool, variation: i64, judge: i64) -> Option<Self> {
        let (table, base, atk_table) = if pc { ("BehaviorParam_PC", 100_000_000, "AtkParam_Pc") } else { ("BehaviorParam", 200_000_000, "AtkParam_Npc") };
        let vars = if pc { vec![variation, variation / 100 * 100] } else { vec![variation] };
        let b = vars.into_iter().map(|v| crate::items::param(table, base + v * 1000 + judge)).find(|r| !r.is_null())?;
        if b["refType"].as_i64() != Some(0) {
            return None;
        }
        let a = crate::items::param(atk_table, b["refId"].as_i64()?);
        if a.is_null() {
            return None;
        }
        let f = |r: &Value, k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
        Some(Self {
            break_rate: f(a, "guardBreakRate"),
            break_correction: f(a, "guardBreakCorrection"),
            attribute: a["guardAttribute"].as_i64().unwrap_or(0),
            stamina_cut_rate: f(a, "guardStaminaCutRate"),
            guard_rate: f(a, "guardRate"),
            behavior_stamina: f(b, "stamina"),
        })
    }
}

/// How a contact landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guarded {
    Hit,
    Block,
    Deflect,
}

/// The defender's repel defence (FUN_140b6c880 with 842fb0 / bfc440): its base raised to the
/// highest defFlickPower among its active SpEffects (105010, the enemies' 200220: 40).
/// `base`: [`player_repel`] or [`npc_repel`].
pub fn repel_defence(base: i32, sp_effects: &[i64]) -> i32 {
    sp_effects.iter().map(|&id| crate::items::param("SpEffectParam", id)["defFlickPower"].as_i64().unwrap_or(0) as i32).fold(base, i32::max)
}

/// Product of guardDefFlickPowerRate over the guard and deflect families (bfccc0).
fn flick_power_rate(sp_effects: &[i64]) -> f32 {
    sp_effects
        .iter()
        .map(|&id| crate::items::param("SpEffectParam", id))
        .filter(|s| matches!(s["stateInfo"].as_i64(), Some(STATE_INFO_GUARD | STATE_INFO_JUST_GUARD)))
        .map(|s| s["guardDefFlickPowerRate"].as_f64().unwrap_or(1.0) as f32)
        .product()
}

/// Wolf's base repel defence (a28da0 / 84ce20): guardBreakCorrection % of the weapon's
/// guardBaseRepel (Kusabimaru 20) times the guard rate, truncated (the stat bonus is 0: Sekiro
/// weapons have overStrength 99).
pub fn player_repel(row: &GuardRow, weapon: i64, sp_effects: &[i64]) -> i32 {
    let base = crate::items::param("EquipParamWeapon", weapon)["guardBaseRepel"].as_f64().unwrap_or(0.0) as f32;
    ((row.break_correction / 100.0 * base * flick_power_rate(sp_effects)) as i32).max(0)
}

/// An NPC's base repel defence (a137a0 / 84cd80): the guard row's guardBreakRate times the
/// guard rate, truncated.
pub fn npc_repel(row: &GuardRow, sp_effects: &[i64]) -> i32 {
    (row.break_rate * flick_power_rate(sp_effects)) as i32
}

/// An attack's repel power (record +0x40, 84c7b0): a Wolf weapon attack's guardAtkRateCorrection
/// % of the weapon's attackBaseRepel (Combo1 300 % x 10 = 30), else the row's guardAtkRate (30
/// on 2154 NPC rows, 100 on 213, 0 on 28).
pub fn attack_repel(pc: bool, atk: &crate::data::Attack, weapon: i64) -> i32 {
    if pc && atk.guard_atk_rate_correction > 0.0 {
        (atk.guard_atk_rate_correction / 100.0 * crate::items::param("EquipParamWeapon", weapon)["attackBaseRepel"].as_f64().unwrap_or(0.0) as f32) as i32
    } else {
        atk.guard_atk_rate as i32
    }
}

/// Whether the attack's disableGuard / disableJustGuard columns for guard type `attribute` are
/// set: (no block, no deflect). Type 0 (the sword) is in the export; type 1 (the umbrella, the
/// shield) is read from the full AtkParam row. A sweep is (1, 1) for both; most grabs are
/// (1, 1) for the sword and (0, 0) for the umbrella.
pub fn disabled_for(pc: bool, atk: &crate::data::Attack, attribute: i64) -> (bool, bool) {
    let row = crate::data::atk_row(pc, atk.atk_param_id);
    if attribute != 1 || row.is_null() {
        return (atk.disable_guard == 1, atk.disable_just_guard == 1);
    }
    (row["disableGuard_vsGuardAttribute1"].as_i64() == Some(1), row["disableJustGuard_vsGuardAttribute1"].as_i64() == Some(1))
}

/// Block versus deflect (FUN_140b6c880): a guarding defender whose arc covers the hit blocks
/// when its repel defence is below the attack's repel power and deflects at or above it, unless
/// the attack's flags for its guard type make that a direct hit. So the deflect window works by
/// raising the repel defence (105010: 40 >= 30); an attack of power 100 is never deflected, one
/// of 20 or less is deflected by a plain guard (20).
pub fn outcome(covers: bool, repel: i32, power: i32, no_block: bool, no_deflect: bool) -> Guarded {
    if !covers {
        Guarded::Hit
    } else if repel < power {
        if no_block { Guarded::Hit } else { Guarded::Block }
    } else if no_deflect {
        Guarded::Hit
    } else {
        Guarded::Deflect
    }
}

/// The guard family's SpEffect rows among `sp_effects` (bfafa0 / bfc4a0): stateInfo 158 on a
/// block, 204 on a deflect. Ids are taken once each.
fn family(sp_effects: &[i64], deflect: bool) -> Vec<&'static Value> {
    let want = if deflect { STATE_INFO_JUST_GUARD } else { STATE_INFO_GUARD };
    let mut ids = sp_effects.to_vec();
    ids.sort();
    ids.dedup();
    ids.into_iter().map(|id| crate::items::param("SpEffectParam", id)).filter(|s| s["stateInfo"].as_i64() == Some(want)).collect()
}

/// The defender's side of a guarded posture hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuardDef {
    /// Guard posture cut [%]: NpcParam staminaGuardDef (the soldiers' 50); for Wolf the weapon's
    /// staminaGuardDef (block) or staminaJustGuardDef (deflect) x ReinforceParamWeapon
    /// staminaGuardDefRate (Kusabimaru 0, the umbrellas 25).
    pub def: f32,
    /// Added to `def` first: 1 for Wolf (840870), 0 for an NPC (840550).
    pub bias: f32,
    /// The defender's per-attribute posture rate: NpcParam <attr>StaminaDmgRate; 1 for Wolf.
    pub attribute_rate: f32,
}

impl GuardDef {
    /// Wolf guarding with `weapon`.
    pub fn player(weapon: i64, deflect: bool) -> Self {
        let w = crate::items::param("EquipParamWeapon", weapon);
        let def = w[if deflect { "staminaJustGuardDef" } else { "staminaGuardDef" }].as_f64().unwrap_or(0.0) as f32;
        let reinforce = crate::items::param("ReinforceParamWeapon", w["reinforceTypeId"].as_i64().unwrap_or(0))["staminaGuardDefRate"].as_f64().unwrap_or(1.0) as f32;
        Self { def: def * reinforce, bias: 1.0, attribute_rate: 1.0 }
    }

    /// An NPC guarding with NpcParam row `npc`.
    pub fn npc(npc: &Value, attribute_rate: f32) -> Self {
        Self { def: npc["staminaGuardDef"].as_f64().unwrap_or(0.0) as f32, bias: 0.0, attribute_rate }
    }
}

/// Posture a guard lets through (8439a0 with 840550 / 840870): `base` (repelLostStamDamage on a
/// block, atkStam on a deflect, times the attacker's rate) less the guard cut
/// clamp((def + bias) x (1 + guard row guardStaminaCutRate / 100) x the family's
/// guardStaminaCutRate, 0, 100) %, plus the guard row's BehaviorParam stamina, times the
/// defender's attribute rate and the family's def<attr>StaminaDmgRate, truncated. Wolf's sword
/// blocking a soldier's slash (18): (1 - 1 %) x 18 = 17.82 -> 17.
pub fn guarded_posture(base: f32, def: &GuardDef, row: Option<&GuardRow>, sp_effects: &[i64], deflect: bool, attribute: i64) -> f32 {
    let fam = family(sp_effects, deflect);
    let rate = |k: &str| fam.iter().map(|s| s[k].as_f64().unwrap_or(1.0) as f32).product::<f32>();
    let cut = ((def.def + def.bias) * (1.0 + row.map_or(0.0, |r| r.stamina_cut_rate) / 100.0) * rate("guardStaminaCutRate")).clamp(0.0, 100.0);
    let guarded = (1.0 - cut / 100.0) * base + row.map_or(0.0, |r| r.behavior_stamina);
    let attr = crate::data::stamina_rate_field(attribute).map_or(1.0, rate);
    (guarded * def.attribute_rate * attr).trunc()
}

/// A blocking NPC's HP multipliers per element (845ed0): physical, magic, fire, lightning, dark.
/// Each is (100 - cut x (1 + guard row guardRate / 100)) / 100 with NpcParam physGuardCutRate /
/// magGuardCutRate / fireGuardCutRate / thunGuardCutRate / darkGuardCutRate; the physical cut is
/// first scaled by 1 + the NpcParam <attr>GuardCutRate for the hit's atkAttribute / 100 (10c60d0;
/// the soldier's slashGuardCutRate 100 doubles it). Negative values mean none (840eb0 clamps).
pub fn npc_guard_cut(npc: &Value, row: Option<&GuardRow>, atk_attribute: i64) -> [f32; 5] {
    let f = |k: &str| npc[k].as_f64().unwrap_or(0.0) as f32;
    let rate = 1.0 + row.map_or(0.0, |r| r.guard_rate) / 100.0;
    let attr = crate::data::npc_damage_rate_field(atk_attribute).map_or(0.0, |k| f(&k.replace("DamageCutRate", "GuardCutRate").replace("DamageRate", "GuardCutRate")));
    let cut = |c: f32| (100.0 - c * rate) / 100.0;
    [cut((1.0 + attr / 100.0) * f("physGuardCutRate")), cut(f("magGuardCutRate")), cut(f("fireGuardCutRate")), cut(f("thunGuardCutRate")), cut(f("darkGuardCutRate"))]
}

/// The defender's SpEffects that count on a direct hit's posture (bfafa0): all but the guard
/// families (158, 204), 110 (a second rate for boosted attackers) and 300 (throws).
pub fn counts_on_a_hit(state_info: i64) -> bool {
    !matches!(state_info, STATE_INFO_GUARD | STATE_INFO_JUST_GUARD | 110 | 300)
}

/// A guard arc's half-angle in degrees: NpcParam guardAngle for an NPC, EquipParamWeapon
/// guardAngle for Wolf (the umbrellas 180); 0 means 90 (b6c880).
pub fn half_angle(angle: f32) -> f32 {
    if angle > 0.0 { angle } else { 90.0 }
}
