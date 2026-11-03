//! Posture ("stamina") storage, damage and break.
//!
//! The engine keeps posture in the character data module (`SprjChrDataModule`) as signed 32-bit
//! integers: remaining at +0x148, maximum at +0x14C and a trailing "history" value at +0x258
//! (read from code, `bd6710`, `bd6ae0`, debug display `bd7630`).

use super::effects::{
    ActiveEffect, defender_stamina_rate, deflect_stamina_attack_rate, guard_stamina_cut_rate,
};
use super::hit::HitOutcome;
use super::params::{AttackProfile, StaminaControl, WeaponGuard};
use super::trunc;

/// Limits imposed by the active StaminaControlParam recovery type, in posture points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlRange {
    pub low: i32,
    pub high: i32,
}

impl ControlRange {
    /// The range for recovery type `kind` of `control` at maximum posture `max`.
    ///
    /// Read from code (`bd4e20`): each limit is `ratio * max / 100` in integer arithmetic, and the
    /// value is clamped between the two whichever is larger.
    pub fn new(control: &StaminaControl, kind: usize, max: i32) -> Option<Self> {
        let hi = *control.max_ratio.get(kind)? as i32 * max / 100;
        let lo = *control.min_ratio.get(kind)? as i32 * max / 100;
        Some(Self {
            low: lo.min(hi),
            high: lo.max(hi),
        })
    }
}

/// One character's posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostureMeter {
    /// Remaining posture; full is `max`, broken is below 1.
    pub remaining: i32,
    pub max: i32,
    /// Lowest allowed remaining value: NpcParam `maxDebtStamina` for NPCs,
    /// TentativePlayerParam `DebtSp` (0) for the player (read from code, `bd4e20`).
    pub floor: i32,
    /// Trailing value the engine keeps beside the meter (+0x258); see [`PostureMeter::set`].
    pub history: i32,
    /// The character ignores posture loss (`+0x228` bit 4, or the debug "no stamina consume").
    pub no_consume: bool,
}

/// Fraction of a recoverable loss that the history value keeps (global at 0x143b0a068 in the
/// analysed code, 0.8). Read from code (`bd6710`).
const HISTORY_KEEP: f32 = 0.8;

impl PostureMeter {
    /// A full meter.
    pub fn full(max: i32, floor: i32) -> Self {
        Self {
            remaining: max,
            max,
            floor,
            history: max,
            no_consume: false,
        }
    }

    /// The remaining value after adding `delta`, clamped to `[floor, max]` and then to the
    /// control range when a recovery type is active. Read from code (`bd4e20`).
    pub fn project(&self, delta: i32, control: Option<ControlRange>) -> i32 {
        let mut v = (self.remaining + delta).max(self.floor).min(self.max);
        if let Some(c) = control {
            v = v.clamp(c.low, c.high);
        }
        v
    }

    /// Sets remaining posture to `target`, the engine's single posture setter (`bd6710`).
    ///
    /// Read from code:
    /// - A decrease is ignored entirely while `no_consume` is set.
    /// - The stored value is [`PostureMeter::project`] of `target - remaining`.
    /// - The history value follows the non-negative part of the change: on a loss it drops by the
    ///   full loss, or with `keep_history` (used by combat hits) only by `ceil(0.2 * loss)`; on a
    ///   gain it rises to the new value. It never exceeds `max`.
    pub fn set(&mut self, target: i32, keep_history: bool, control: Option<ControlRange>) {
        if target < self.remaining && self.no_consume {
            return;
        }
        let old = self.remaining;
        let new = self.project(target - old, control);
        self.remaining = new;
        let change = new.max(0) - old.max(0);
        if change < 0 {
            if keep_history {
                let part = ((1.0 - HISTORY_KEEP) * change as f32).ceil();
                self.history = trunc(part + self.history as f32);
            } else {
                self.history += change;
            }
        } else {
            self.history = self.history.max(new.max(0));
        }
        self.history = self.history.min(self.max);
    }

    /// Applies `damage` posture points the way a combat hit does (`bd4de0` with the history flag).
    pub fn take(&mut self, damage: i32, control: Option<ControlRange>) {
        self.set(self.remaining - damage, true, control);
    }

    /// Whether this meter counts as broken: remaining below 1 (read from code, `b6e6a0`).
    pub fn broken(&self) -> bool {
        self.remaining < 1
    }

    /// Maximum posture from the base value and SpEffects: `trunc(flat + rate * base)`, and the
    /// remaining value is clamped to `[-100, max]` afterward. Read from code (`bd60f0`).
    /// `rate` is the product of positive `maxStaminaRate` values and `flat` the sum of the flat
    /// increases (field roles inferred from `bfd500`, `bfd4c0`). HP plays no part.
    pub fn recompute_max(&mut self, base: i32, rate: f32, flat: i32) {
        self.max = trunc(flat as f32 + rate * base as f32);
        self.remaining = self.remaining.max(-100).min(self.max);
    }
}

/// What the defender's side contributes to its own posture damage.
#[derive(Debug, Clone, PartialEq)]
pub struct DefenderPosture {
    /// Per-attribute posture rate of the defender: NpcParam `<attr>StaminaDmgRate` for NPCs;
    /// 1.0 for the player (inferred: the player's rate comes from armour, which Sekiro lacks).
    pub attribute_rate: f32,
    /// Guard posture cut in percent: NpcParam `staminaGuardDef` for NPCs; for the player the
    /// weapon's `staminaGuardDef` when blocking and `staminaJustGuardDef` when deflecting, times
    /// ReinforceParamWeapon `staminaGuardDefRate`, plus the stat bonus from
    /// `staminaGuardDef_MaxCorrect` (0 on the Kusabimaru).
    pub guard_def: f32,
    /// Added to `guard_def` before the multipliers: 1.0 for the player, 0 for NPCs (read from
    /// code: `840870` adds a constant 1.0, `840550` does not).
    pub cut_bias: f32,
    /// `guardStaminaCutRate` of the guard action's AtkParam row.
    pub guard_row_cut_rate: i16,
    /// `stamina` of the guard action's BehaviorParam row, added after the cut (both NPCs and the
    /// player, read from code).
    pub guard_behavior_stamina: i32,
}

impl Default for DefenderPosture {
    fn default() -> Self {
        Self {
            attribute_rate: 1.0,
            guard_def: 0.0,
            cut_bias: 0.0,
            guard_row_cut_rate: 0,
            guard_behavior_stamina: 0,
        }
    }
}

impl DefenderPosture {
    /// The player guarding with `weapon` (block or deflect), with reinforcement rate
    /// `staminaGuardDefRate` and no stat bonus.
    pub fn player(weapon: &WeaponGuard, deflect: bool, reinforce_guard_def_rate: f32) -> Self {
        let def = if deflect {
            weapon.stamina_just_guard_def
        } else {
            weapon.stamina_guard_def
        };
        Self {
            attribute_rate: 1.0,
            guard_def: def as f32 * reinforce_guard_def_rate,
            cut_bias: 1.0,
            guard_row_cut_rate: 0,
            guard_behavior_stamina: 0,
        }
    }
}

/// Posture damage the defender takes from one contact, before the attacker/defender
/// player-versus-enemy corrections.
///
/// Read from code:
/// - Direct hit (`844070`): `trunc(directAtkStamDamage * attack_rate * attribute_rate *
///   effect_rate)`.
/// - Block or deflect (`8439a0`, NPC guard cut `840550`, player guard cut `840870`): the base is
///   `repelLostStamDamage` on a block and `atkStam` on a deflect, times `attack_rate`. The guard
///   cut is `clamp((guard_def + cut_bias) * (1 + guard_row_cut_rate / 100) * cut_effects, 0, 100)`
///   percent, and the guarded value is `(1 - cut / 100) * base + guard_behavior_stamina`. That is
///   multiplied by `attribute_rate` and the guard-family effect rate and truncated.
///   The player version also scales the stat bonus by 1.5 and the result by 0.7 (weapon
///   category 12) or 0.9 in an unidentified guard state, and by a percent at PlayerIns +0x21EC
///   when a module flag is set; these are not modelled.
///
/// `attack_rate` is the attacker's posture attack multiplier: the product of its
/// `staminaAttackRate` effects ([`super::effects::attacker_stamina_attack_rate`], folded into the
/// record by the builder, read from code) times record slot +0x188 (writer not found; 1.0
/// assumed). For player weapon attacks use [`player_posture_base`] with
/// [`defender_posture_damage_from_base`] instead.
pub fn defender_posture_damage(
    outcome: HitOutcome,
    attack: &AttackProfile,
    attack_rate: f32,
    defender: &DefenderPosture,
    defender_effects: &[ActiveEffect],
) -> i32 {
    let base = match outcome {
        HitOutcome::Hit => attack.direct_stam,
        HitOutcome::Block => attack.blocked_stam,
        HitOutcome::Deflect => attack.deflected_stam,
    } as f32
        * attack_rate;
    defender_posture_damage_from_base(outcome, base, attack, defender, defender_effects)
}

/// The posture base a player weapon attack stores in the hit record (`847e10`, read from code):
/// `(staminaAttackPowerRate * field + correction / 100 * attackBaseStamina * reinforce) *
/// reinforce * attack_rate`, where `field` is the AtkParam_Pc value for the outcome
/// (`directAtkStamDamage`, `repelLostStamDamage` or `atkStam`), `correction` is
/// `directAtkStamCorrection` for a direct hit and `atkStamCorrection` otherwise, and `reinforce`
/// is ReinforceParamWeapon `staminaAtkRate`. Non-weapon player attacks use `field * attack_rate`.
pub fn player_posture_base(
    outcome: HitOutcome,
    attack: &AttackProfile,
    weapon: &WeaponGuard,
    reinforce_stamina_atk_rate: f32,
    attack_rate: f32,
) -> f32 {
    let (field, correction) = match outcome {
        HitOutcome::Hit => (attack.direct_stam, attack.direct_atk_stam_correction),
        HitOutcome::Block => (attack.blocked_stam, attack.atk_stam_correction),
        HitOutcome::Deflect => (attack.deflected_stam, attack.atk_stam_correction),
    };
    let weapon_part =
        correction as f32 * 0.01 * (weapon.attack_base_stamina as f32 * reinforce_stamina_atk_rate);
    (weapon.stamina_attack_power_rate * field as f32 + weapon_part)
        * reinforce_stamina_atk_rate
        * attack_rate
}

/// [`defender_posture_damage`] from an already computed record base (`844070`, `8439a0`).
pub fn defender_posture_damage_from_base(
    outcome: HitOutcome,
    base: f32,
    attack: &AttackProfile,
    defender: &DefenderPosture,
    defender_effects: &[ActiveEffect],
) -> i32 {
    let attr = attack.stamina_attribute;
    match outcome {
        HitOutcome::Hit => {
            let effect_rate = defender_stamina_rate(defender_effects, attr, false, false);
            trunc(base * defender.attribute_rate * effect_rate)
        }
        HitOutcome::Block | HitOutcome::Deflect => {
            let deflect = outcome == HitOutcome::Deflect;
            let cut = ((defender.guard_def + defender.cut_bias)
                * (defender.guard_row_cut_rate as f32 * 0.01 + 1.0)
                * guard_stamina_cut_rate(defender_effects, deflect))
            .clamp(0.0, 100.0);
            let guarded = (1.0 - cut * 0.01) * base + defender.guard_behavior_stamina as f32;
            let effect_rate = defender_stamina_rate(defender_effects, attr, true, deflect);
            trunc(guarded * defender.attribute_rate * effect_rate)
        }
    }
}

/// Applies the second stage of the defender's posture damage (`b6c880`): the result of
/// [`defender_posture_damage`] times the player/enemy correction product (`8480a0`, from the
/// `atk*/def*DmgCorrectRate_Stamina` effects, inferred) and the hit part's damage-group rate,
/// truncated again. Read from code for the arithmetic.
pub fn corrected_posture_damage(raw: i32, correction: f32, part_rate: f32) -> i32 {
    trunc(raw as f32 * correction * part_rate)
}

/// Posture damage the attacker takes from its own contact.
///
/// Read from code (`842790`, applied by the attacker's damage module `b698d0`):
/// - not guarded: `directAtkStamDamage_Attacker`;
/// - blocked, or the block broke the defender's guard: `repelVictoryStamDamage_Attacker`;
/// - deflected: `trunc(repelLostStamDamage_Attacker * weapon_rate * deflect_rate)`, where
///   `deflect_rate` is the product of `defStaminaAttackRate` over the defender's active 204
///   effects (105020..105023 give 1, 0.5, 0.25, 0.125) and `weapon_rate` is the defender's weapon
///   `passiveStaminaAtkRate` times its reinforcement rate (1.0 for NPC defenders).
pub fn attacker_posture_damage(
    outcome: HitOutcome,
    attack: &AttackProfile,
    guard_broken: bool,
    weapon_rate: f32,
    defender_effects: &[ActiveEffect],
) -> i32 {
    match outcome {
        HitOutcome::Hit => attack.attacker_direct_stam as i32,
        HitOutcome::Block => attack.attacker_blocked_stam as i32,
        HitOutcome::Deflect if guard_broken => attack.attacker_blocked_stam as i32,
        HitOutcome::Deflect => trunc(
            attack.attacker_deflected_stam as f32
                * weapon_rate
                * deflect_stamina_attack_rate(defender_effects),
        ),
    }
}

/// The posture side of one contact for the defender, as `b6e6a0` applies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostureContact {
    /// Damage actually subtracted (0 when the attack has `disableStaminaAttack`).
    pub applied: i32,
    /// The value the break test saw.
    pub projected: i32,
    /// Posture broke: on a direct hit this is a posture break (damage type 1027); on a block it
    /// is a guard break (1001). A deflect never breaks the defender.
    pub broke: bool,
}

/// Applies posture `damage` to the defender for `outcome` and decides whether it broke.
///
/// Read from code (`b6e6a0`): the break test uses the value projected *before*
/// `disableStaminaAttack` zeroes the damage, so such attacks can still break a meter that is
/// already empty; damage is then applied with the history flag. On a direct hit a break also
/// needs positive damage; on a block an empty projection is a guard break; on a deflect the meter
/// may reach its floor but no break is reported, which is the "deflect never breaks your posture"
/// rule. The HP side of the same test (HP below 2 with deathblows left) is in
/// [`super::hp::Vitality::deathblow_ready`].
pub fn apply_defender_posture(
    meter: &mut PostureMeter,
    outcome: HitOutcome,
    damage: i32,
    attack: &AttackProfile,
    control: Option<ControlRange>,
) -> PostureContact {
    let projected = meter.project(-damage, control);
    let applied = if attack.disable_stamina_attack || meter.no_consume {
        0
    } else {
        damage
    };
    meter.take(applied, control);
    let empty = !meter.no_consume && projected < 1;
    let broke = empty
        && match outcome {
            HitOutcome::Hit => damage > 0,
            HitOutcome::Block => true,
            HitOutcome::Deflect => false,
        };
    PostureContact {
        applied,
        projected,
        broke,
    }
}

/// Whether the attacker's own posture breaks from being deflected or blocked: the attacker's
/// remaining posture minus the damage is below 1 (read from code, `b6e6a0`, flag +0x1CF).
/// With `doesBreakRepelStamDamage` this turns the reports into 1028 and 1033.
pub fn attacker_breaks(attacker: &PostureMeter, damage: i32) -> bool {
    damage > 0 && attacker.remaining - damage < 1
}
