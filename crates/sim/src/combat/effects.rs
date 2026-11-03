//! The parts of active SpEffects that combat reads, and the engine's ways of combining them.

use super::{STATE_INFO_GUARD, STATE_INFO_JUST_GUARD};

/// Which per-attribute posture rate a hit uses (`AtkParam.staminaPhysicsAttribute`).
///
/// The order is the order the engine switches on, which matches the NpcParam fields from
/// `slashStaminaDmgRate` (0x27c) onward and the SpEffect fields from `defSlashStaminaDmgRate`
/// (0x328) onward (read from code, `844660` and `10ce8d0`). Value 0 and anything above 12 use 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaminaAttribute(pub u8);

/// Twelve per-attribute posture damage multipliers, indexed by [`StaminaAttribute`] minus one:
/// slash, light hit (blow), thrust, neutral, deathblow, heavy hit, anti-ground, anti-air,
/// light shoot, attribute A, B, C.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaminaRates(pub [f32; 12]);

impl StaminaRates {
    pub const ONE: Self = Self([1.0; 12]);

    /// The rate for `attr`, or 1.0 when the attribute has no rate (read from code, `844660`).
    pub fn get(&self, attr: StaminaAttribute) -> f32 {
        match attr.0 {
            1..=12 => self.0[attr.0 as usize - 1],
            _ => 1.0,
        }
    }
}

impl Default for StaminaRates {
    fn default() -> Self {
        Self::ONE
    }
}

/// The combat-relevant fields of one active SpEffect (SpEffectParam row).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveEffect {
    /// `physics/magic/fire/thunder/darkAttackRate`, applied before HP truncation.
    pub hp_attack_rates: [f32; 5],
    pub id: i32,
    pub state_info: u16,
    /// `defFlickPower`: overrides the owner's repel defence when higher (def: "上書き").
    pub def_flick_power: u8,
    pub guard_def_flick_power_rate: f32,
    pub guard_stamina_cut_rate: f32,
    pub def_stamina_attack_rate: f32,
    /// `staminaAttackRate`: the owner's own posture attack multiplier.
    pub stamina_attack_rate: f32,
    pub def_stamina_dmg_rates: StaminaRates,
    pub stamina_recover_speed_rate: f32,
    pub stamina_recover_change_speed: i32,
    /// `conditionHp`: the effect only works at or below this percent of max HP; -1 disables.
    pub condition_hp: f32,
    /// `conditionHpRate`: the effect only works at or above this percent of max HP; -1 disables.
    pub condition_hp_rate: f32,
    pub max_stamina_rate: f32,
    /// `changeHpRate`: percent of max HP taken when the effect starts (negative heals; -100 is
    /// SpEffect 300610 "HP reset").
    pub change_hp_rate: f32,
    /// The defender's HP multipliers from `slash/lightHit/thrust/neutralDamageCutRate` (indexed
    /// by `atkAttribute` 1 to 4) and `magic/fire/thunder/darkDamageCutRate`. 0 makes the owner
    /// immune (the Demon's 300331 "HP damage multiplier 0" during its heat-up roar).
    pub hp_cut_phys: [f32; 4],
    pub hp_cut_elements: [f32; 4],
}

impl Default for ActiveEffect {
    fn default() -> Self {
        Self {
            hp_attack_rates: [1.0; 5],
            id: -1,
            state_info: 0,
            def_flick_power: 0,
            guard_def_flick_power_rate: 1.0,
            guard_stamina_cut_rate: 1.0,
            def_stamina_attack_rate: 1.0,
            stamina_attack_rate: 1.0,
            def_stamina_dmg_rates: StaminaRates::ONE,
            stamina_recover_speed_rate: 1.0,
            stamina_recover_change_speed: 0,
            condition_hp: -1.0,
            condition_hp_rate: -1.0,
            max_stamina_rate: 1.0,
            change_hp_rate: 0.0,
            hp_cut_phys: [1.0; 4],
            hp_cut_elements: [1.0; 4],
        }
    }
}

impl ActiveEffect {
    /// Whether the HP gates let this effect work at `hp` out of `max_hp`.
    ///
    /// Source: def descriptions of `conditionHp` ("activates when remaining HP reaches this % of
    /// max") and `conditionHpRate` ("only while HP is at least this %"). The engine-side check was
    /// not traced; inclusive comparisons are our assumption.
    pub fn hp_gate_open(&self, hp: i32, max_hp: i32) -> bool {
        if max_hp <= 0 {
            return true;
        }
        let percent = hp as f32 * 100.0 / max_hp as f32;
        let below_ok = self.condition_hp < 0.0 || percent <= self.condition_hp;
        let above_ok = self.condition_hp_rate < 0.0 || percent >= self.condition_hp_rate;
        below_ok && above_ok
    }
}

/// The guard-state effect family a contact uses: block effects (158) or deflect effects (204).
pub(crate) fn guard_state_info(deflect: bool) -> u16 {
    if deflect {
        STATE_INFO_JUST_GUARD
    } else {
        STATE_INFO_GUARD
    }
}

/// Highest `defFlickPower` among active effects, folded into a base repel value.
///
/// Read from code (`bfc440`): start from the base and take the maximum with every active effect's
/// override, so an effect can only raise repel defence.
pub fn repel_with_overrides(base: i32, effects: &[ActiveEffect]) -> i32 {
    effects
        .iter()
        .map(|e| e.def_flick_power as i32)
        .fold(base, i32::max)
}

/// Product of `guardDefFlickPowerRate` over active block and deflect effects (158 and 204).
/// Read from code (`bfccc0`).
pub fn guard_flick_power_rate(effects: &[ActiveEffect]) -> f32 {
    effects
        .iter()
        .filter(|e| e.state_info == STATE_INFO_GUARD || e.state_info == STATE_INFO_JUST_GUARD)
        .fold(1.0, |acc, e| acc * e.guard_def_flick_power_rate)
}

/// Product of `guardStaminaCutRate` over active effects of the given guard family.
/// Read from code (`bfcd40` for NPCs, `bfcde0` for the player).
pub fn guard_stamina_cut_rate(effects: &[ActiveEffect], deflect: bool) -> f32 {
    let want = guard_state_info(deflect);
    effects
        .iter()
        .filter(|e| e.state_info == want)
        .fold(1.0, |acc, e| acc * e.guard_stamina_cut_rate)
}

/// Product of `defStaminaAttackRate` over active deflect effects (204).
/// Read from code (`bfc490`); this is the 1 / 0.5 / 0.25 / 0.125 chain of 105020..105023.
pub fn deflect_stamina_attack_rate(effects: &[ActiveEffect]) -> f32 {
    effects
        .iter()
        .filter(|e| e.state_info == STATE_INFO_JUST_GUARD)
        .fold(1.0, |acc, e| acc * e.def_stamina_attack_rate)
}

/// Product of the defender's per-attribute posture damage rates from its effects.
///
/// Read from code (`bfafa0`): on a direct hit every effect counts except the guard families
/// (158, 204) and the special families 110 and 300; on a block only 158 effects count, on a
/// deflect only 204 effects count. Family 110 feeds a second multiplier that only matters for
/// attackers with a boost above 1 and family 300 only for throws; both are left out here.
pub fn defender_stamina_rate(
    effects: &[ActiveEffect],
    attr: StaminaAttribute,
    guarded: bool,
    deflect: bool,
) -> f32 {
    let want = guard_state_info(deflect);
    effects
        .iter()
        .filter(|e| {
            if guarded {
                e.state_info == want
            } else {
                !matches!(
                    e.state_info,
                    STATE_INFO_GUARD | STATE_INFO_JUST_GUARD | 110 | 300
                )
            }
        })
        .fold(1.0, |acc, e| acc * e.def_stamina_dmg_rates.get(attr))
}

/// Product of `staminaRecoverSpeedRate` over active effects. Read from code (`bfe360`, `c05740`).
pub fn recover_speed_rate(effects: &[ActiveEffect]) -> f32 {
    effects
        .iter()
        .fold(1.0, |acc, e| acc * e.stamina_recover_speed_rate)
}

/// Sum of `staminaRecoverChangeSpeed` over active effects.
///
/// Inferred: the engine adds the result of `bfbe40` to the base recovery speed, and the def says
/// the field "adds to the base recovery speed"; Ghidra lost the loop body, so the sum is assumed.
pub fn recover_change_speed(effects: &[ActiveEffect]) -> i32 {
    effects.iter().map(|e| e.stamina_recover_change_speed).sum()
}

/// Product of `staminaAttackRate` over the attacker's active effects.
///
/// Read from code (`bfe3b0`, `c057d0`): the hit-record builder folds this into the three posture
/// bases it stores (record +0x34, +0x38, +0x3C). Effects flagged `bGameClearBonus` are further
/// scaled by a NG+ table, which is left out.
pub fn attacker_stamina_attack_rate(effects: &[ActiveEffect]) -> f32 {
    effects
        .iter()
        .filter(|e| e.stamina_attack_rate != 1.0)
        .fold(1.0, |acc, e| acc * e.stamina_attack_rate)
}
