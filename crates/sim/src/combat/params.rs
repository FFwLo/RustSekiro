//! Combat inputs built from param rows.
//!
//! These read generic [`Row`]s by Paramdex field name so the combat code does not depend on which
//! fields the typed tables happen to expose.

use sekiro_formats::Result;
use sekiro_formats::param::Row;

use super::effects::{ActiveEffect, StaminaAttribute, StaminaRates};

/// The parts of an AtkParam row (NPC or PC) that hit resolution and damage read.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackProfile {
    pub id: i32,
    /// `guardAtkRate`: repel attack power of an NPC attack (and of non-weapon player attacks).
    /// A guarding defender whose repel defence is at least this deflects instead of blocking.
    /// Read from code: the hit-record builder `10b6880` stores `84c650` in record +0x40, which for
    /// an NPC attacker returns this field (`a13630`, `84c760`); for a player weapon attack see
    /// [`super::hit::player_attack_repel`].
    pub guard_atk_rate: u16,
    /// `guardAtkRateCorrection`: PC only, percent applied to the weapon's `attackBaseRepel`.
    pub guard_atk_rate_correction: u16,
    /// `atkStamCorrection`: PC only, percent applied to the weapon's `attackBaseStamina` on a
    /// guarded contact.
    pub atk_stam_correction: u16,
    /// `directAtkStamCorrection`: PC only, the same for a direct hit.
    pub direct_atk_stam_correction: u16,
    /// `atkAttribute`: physical attribute (1 slash, 2 blow, 3 thrust, 4 neutral, ...), which picks
    /// the defender's per-attribute guard cut correction.
    pub atk_attribute: u8,
    /// Physical, magic, fire, lightning and dark attack power (`atkPhys` .. `atkDark`).
    pub attack_power: [f32; 5],
    /// `directAtkStamDamage`: posture damage to the defender on a direct hit (record +0x3C, read
    /// from code: `10b6880` stores virtual 0x1F0, which reads this field in `847d50`/`847e10`).
    pub direct_stam: i16,
    /// `repelLostStamDamage`: posture damage to the defender when it blocks (record +0x38, virtual
    /// 0x1E8 with the deflect flag clear).
    pub blocked_stam: i16,
    /// `atkStam`: posture damage to the defender when it deflects (record +0x34, virtual 0x1E8
    /// with the flag set by the thunk at `847860`).
    pub deflected_stam: i16,
    /// `directAtkStamDamage_Attacker`.
    pub attacker_direct_stam: i16,
    /// `repelVictoryStamDamage_Attacker`: posture the attacker takes when blocked.
    pub attacker_blocked_stam: i16,
    /// `repelLostStamDamage_Attacker`: posture the attacker takes when deflected.
    pub attacker_deflected_stam: i16,
    /// `staminaPhysicsAttribute`.
    pub stamina_attribute: StaminaAttribute,
    pub disable_stamina_attack: bool,
    pub does_break_repel_stam_damage: bool,
    pub is_all_dir_guard: bool,
    /// `disableGuard_vsGuardAttribute0/1`: cannot be blocked by guard type 0 (katana) or 1.
    pub disable_guard: [bool; 2],
    /// `disableJustGuard_vsGuardAttribute0/1`: cannot be deflected by guard type 0 or 1.
    pub disable_just_guard: [bool; 2],
    pub excess_dmg_keep_hp: u16,
    /// `guardBreakRate`: repel defence of this row when it is a guard action's attack row.
    pub guard_break_rate: u16,
    /// `guardBreakCorrection`: PC guard rows, percent applied to the weapon's base repel.
    pub guard_break_correction: u16,
    /// `guardStaminaCutRate`: correction to the guard posture cut when this is a guard row.
    pub guard_stamina_cut_rate: i16,
    /// `guardRate`: correction to the HP guard cut when this is a guard row.
    pub guard_rate: i16,
    /// `guardAttribute`: guard type of this row when it is a guard action (0 katana, 1 umbrella).
    pub guard_attribute: u8,
    /// `knockbackDist_DirectHit`, `knockbackDist_Guard`, `knockbackDist_JustGuard`: metres the
    /// defender is pushed on a hit, a block and a deflect (def).
    pub knockback_dist: [f32; 3],
    /// `hitStopTime`: the attacker's hit stop when this attack connects, seconds, before the
    /// defender's NpcParam `hitStopType` scale (see [`crate::combat::knockback::HitStop`]).
    pub hit_stop_time: f32,
    /// `hitStopTime_Defencer`: the defender's hit stop, seconds, before its NpcParam
    /// `hitStopType_Defencer` gate.
    pub hit_stop_time_defender: f32,
    /// `isArrowAtk`: a bow or bullet attack; TAE action flag 41 on the defender does not force a
    /// hit stop for it (code `b60ec0`, `b60f10`).
    pub is_arrow: bool,
    /// `spAttribute`: the hit's special attribute, which the player script reads as env 285
    /// (`DAMAGE_ELEMENT_*` in `c0000_define.hks`: 1 none, 2 fire, 6 lightning, 10 blue
    /// lightning; the Owl's firecracker is 2).
    pub sp_attribute: u8,
    /// `atkBehaviorId`: "behaviour identification value" the player script tests with env 334
    /// (`BEHAVIOR_IDENTIFICATION_VALUE_*`: 17 blinding, 18 blast, 19 storm back jump, 20 to 23
    /// special guard reactions). The Owl's smoke screen dazzle is 17.
    pub behavior_id: u8,
}

impl AttackProfile {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id: row.id(),
            guard_atk_rate: row.u16("guardAtkRate")?,
            guard_atk_rate_correction: row.u16("guardAtkRateCorrection")?,
            atk_stam_correction: row.u16("atkStamCorrection")?,
            direct_atk_stam_correction: row.u16("directAtkStamCorrection")?,
            atk_attribute: row.u8("atkAttribute")?,
            attack_power: [
                row.int("atkPhys")? as f32,
                row.int("atkMag")? as f32,
                row.int("atkFire")? as f32,
                row.int("atkThun")? as f32,
                row.int("atkDark")? as f32,
            ],
            direct_stam: row.i16("directAtkStamDamage")?,
            blocked_stam: row.i16("repelLostStamDamage")?,
            deflected_stam: row.i16("atkStam")?,
            attacker_direct_stam: row.i16("directAtkStamDamage_Attacker")?,
            attacker_blocked_stam: row.i16("repelVictoryStamDamage_Attacker")?,
            attacker_deflected_stam: row.i16("repelLostStamDamage_Attacker")?,
            stamina_attribute: StaminaAttribute(row.u8("staminaPhysicsAttribute")?),
            disable_stamina_attack: row.bool("disableStaminaAttack")?,
            does_break_repel_stam_damage: row.bool("doesBreakRepelStamDamage")?,
            is_all_dir_guard: row.bool("isAllDirGuard")?,
            disable_guard: [
                row.u8("disableGuard_vsGuardAttribute0")? != 0,
                row.u8("disableGuard_vsGuardAttribute1")? != 0,
            ],
            disable_just_guard: [
                row.u8("disableJustGuard_vsGuardAttribute0")? != 0,
                row.u8("disableJustGuard_vsGuardAttribute1")? != 0,
            ],
            excess_dmg_keep_hp: row.u16("excessDmgKeepHp")?,
            guard_break_rate: row.u16("guardBreakRate")?,
            guard_break_correction: row.u16("guardBreakCorrection")?,
            guard_stamina_cut_rate: row.i16("guardStaminaCutRate")?,
            guard_rate: row.i16("guardRate")?,
            guard_attribute: row.u8("guardAttribute")?,
            knockback_dist: [
                row.f32("knockbackDist_DirectHit")?,
                row.f32("knockbackDist_Guard")?,
                row.f32("knockbackDist_JustGuard")?,
            ],
            hit_stop_time: row.f32("hitStopTime")?,
            hit_stop_time_defender: row.f32("hitStopTime_Defencer")?,
            is_arrow: row.bool("isArrowAtk")?,
            sp_attribute: row.u8("spAttribute")?,
            behavior_id: row.u8("atkBehaviorId")?,
        })
    }
}

/// The parts of an NpcParam row that combat reads, and what its defeat pays.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcCombat {
    pub id: i32,
    pub hp: i32,
    /// `stamina`: maximum posture.
    pub max_posture: i32,
    pub stamina_recover_base_vel: f32,
    /// `maxDebtStamina`: lowest remaining posture (zero or negative).
    pub max_debt_stamina: i32,
    pub stamina_control_param_id: u32,
    /// `ninsatuNum`: deathblows needed.
    pub deathblows: i32,
    pub guard_angle: i16,
    /// `guardLevel`: which guard reaction takes a blocked hit ("which guard motion receives
    /// the enemy's attack", def). The NPC script reads it as env 258.
    pub guard_level: i32,
    /// `defFlickPower`: repel defence while not guarding.
    pub def_flick_power: u16,
    pub stamina_guard_def: u8,
    pub phys_guard_cut_rate: f32,
    /// Per-attribute guard cut corrections in percent (`slashGuardCutRate` .. `attriCGuardCutRate`,
    /// indexed by `atkAttribute - 1`), read from code `10c60d0`.
    pub guard_cut_attribute_rates: [i16; 12],
    pub stamina_dmg_rates: StaminaRates,
    /// Resident SpEffects `spEffectID0..31` (unused slots are -1).
    pub sp_effect_ids: Vec<i32>,
    /// `knockbackParamId`: the KnockBackParam row that times this NPC's pushes (0 on every row).
    pub knockback_param_id: u8,
    /// `knockbackRate_vsPlayer_DirectHit` / `_Guard` / `_JustGuard`: percent of the push cut when
    /// the player's attack hits, is blocked or is deflected (a cut rate per its description,
    /// def; 0 on the soldier).
    pub knockback_cut_vs_player: [u8; 3],
    /// `knockbackRate_vsEnemy_*`: the same against other characters' attacks.
    pub knockback_cut_vs_enemy: [u8; 3],
    /// `hitStopType`: whether a character that hits this NPC gets its hit stop: 0 none, 1 half
    /// (the engine's weak rate 0.5), 2 full (code `b9cd90`; 0 on the soldier and the Owl).
    pub hit_stop_type: u8,
    /// `hitStopType_Defencer`: 1 when this NPC takes the defender's hit stop of attacks that hit
    /// it, else none (code `b9cfb0`; 0 on every NPC row).
    pub hit_stop_type_defender: u8,
    /// `getSoul`: the Sen its defeat pays.
    pub get_soul: u32,
    /// `skillPoint`: the skill experience its defeat pays.
    pub skill_point: u32,
}

impl NpcCombat {
    pub fn from_row(row: &Row) -> Result<Self> {
        const RATES: [&str; 12] = [
            "slashStaminaDmgRate",
            "lightHitStaminaDmgRate",
            "thrustStaminaDmgRate",
            "neutralStaminaDmgRate",
            "ninsatuStaminaDmgRate",
            "heavyHitStaminaDmgRate",
            "antiGroundStaminaDmgRate",
            "antiAirStaminaDmgRate",
            "lightShootStaminaDmgRate",
            "attriAStaminaDmgRate",
            "attriBStaminaDmgRate",
            "attriCStaminaDmgRate",
        ];
        let mut rates = [1.0; 12];
        for (r, name) in rates.iter_mut().zip(RATES) {
            *r = row.f32(name)?;
        }
        const GUARD_CUTS: [&str; 12] = [
            "slashGuardCutRate",
            "lightHitGuardCutRate",
            "thrustGuardCutRate",
            "neutralGuardCutRate",
            "ninsatsuGuardCutRate",
            "heavyHitGuardCutRate",
            "antiGroundGuardCutRate",
            "antiAirGuardCutRate",
            "lightShootGuardCutRate",
            "attriAGuardCutRate",
            "attriBGuardCutRate",
            "attriCGuardCutRate",
        ];
        let mut guard_cut_attribute_rates = [0; 12];
        for (r, name) in guard_cut_attribute_rates.iter_mut().zip(GUARD_CUTS) {
            *r = row.i16(name)?;
        }
        let mut sp_effect_ids = Vec::with_capacity(32);
        for i in 0..32 {
            sp_effect_ids.push(row.i32(&format!("spEffectID{i}"))?);
        }
        Ok(Self {
            id: row.id(),
            hp: row.int("hp")? as i32,
            max_posture: row.int("stamina")? as i32,
            stamina_recover_base_vel: row.int("staminaRecoverBaseVel")? as f32,
            max_debt_stamina: row.int("maxDebtStamina")? as i32,
            stamina_control_param_id: row.u32("staminaControlParamId")?,
            deathblows: row.int("ninsatuNum")? as i32,
            guard_angle: row.i16("guardAngle")?,
            guard_level: row.int("guardLevel")? as i32,
            def_flick_power: row.u16("defFlickPower")?,
            stamina_guard_def: row.u8("staminaGuardDef")?,
            phys_guard_cut_rate: row.f32("physGuardCutRate")?,
            guard_cut_attribute_rates,
            stamina_dmg_rates: StaminaRates(rates),
            sp_effect_ids,
            knockback_param_id: row.u8("knockbackParamId")?,
            knockback_cut_vs_player: [
                row.u8("knockbackRate_vsPlayer_DirectHit")?,
                row.u8("knockbackRate_vsPlayer_Guard")?,
                row.u8("knockbackRate_vsPlayer_JustGuard")?,
            ],
            knockback_cut_vs_enemy: [
                row.u8("knockbackRate_vsEnemy_DirectHit")?,
                row.u8("knockbackRate_vsEnemy_Guard")?,
                row.u8("knockbackRate_vsEnemy_JustGuard")?,
            ],
            hit_stop_type: row.u8("hitStopType")?,
            hit_stop_type_defender: row.u8("hitStopType_Defencer")?,
            get_soul: row.int("getSoul")?.max(0) as u32,
            skill_point: row.int("skillPoint")?.max(0) as u32,
        })
    }
}

impl ActiveEffect {
    pub fn from_row(row: &Row) -> Result<Self> {
        const RATES: [&str; 12] = [
            "defSlashStaminaDmgRate",
            "defLightHitStaminaDmgRate",
            "defThrustStaminaDmgRate",
            "defNeutralStaminaDmgRate",
            "defNinsatuStaminaDmgRate",
            "defHeavyHitStaminaDmgRate",
            "defAntiGroundStaminaDmgRate",
            "defAntiAirStaminaDmgRate",
            "defLightShootStaminaDmgRate",
            "defAttriAStaminaDmgRate",
            "defAttriBStaminaDmgRate",
            "defAttriCStaminaDmgRate",
        ];
        let mut rates = [1.0; 12];
        for (r, name) in rates.iter_mut().zip(RATES) {
            *r = row.f32(name)?;
        }
        Ok(Self {
            id: row.id(),
            state_info: row.u16("stateInfo")?,
            hp_attack_rates: [
                row.f32("physicsAttackRate")?,
                row.f32("magicAttackRate")?,
                row.f32("fireAttackRate")?,
                row.f32("thunderAttackRate")?,
                row.f32("darkAttackRate")?,
            ],
            def_flick_power: row.u8("defFlickPower")?,
            guard_def_flick_power_rate: row.f32("guardDefFlickPowerRate")?,
            guard_stamina_cut_rate: row.f32("guardStaminaCutRate")?,
            def_stamina_attack_rate: row.f32("defStaminaAttackRate")?,
            stamina_attack_rate: row.f32("staminaAttackRate")?,
            def_stamina_dmg_rates: StaminaRates(rates),
            stamina_recover_speed_rate: row.f32("staminaRecoverSpeedRate")?,
            stamina_recover_change_speed: row.i32("staminaRecoverChangeSpeed")?,
            condition_hp: row.f32("conditionHp")?,
            condition_hp_rate: row.f32("conditionHpRate")?,
            max_stamina_rate: row.f32("maxStaminaRate")?,
            change_hp_rate: row.f32("changeHpRate")?,
            hp_cut_phys: [
                row.f32("slashDamageCutRate").unwrap_or(1.0),
                row.f32("lightHitDamageCutRate").unwrap_or(1.0),
                row.f32("thrustDamageCutRate").unwrap_or(1.0),
                row.f32("neutralDamageCutRate").unwrap_or(1.0),
            ],
            hp_cut_elements: [
                row.f32("magicDamageCutRate").unwrap_or(1.0),
                row.f32("fireDamageCutRate").unwrap_or(1.0),
                row.f32("thunderDamageCutRate").unwrap_or(1.0),
                row.f32("darkDamageCutRate").unwrap_or(1.0),
            ],
        })
    }
}

/// The guard-related parts of an EquipParamWeapon row.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponGuard {
    pub id: i32,
    /// `guardAngle`: half-angle of the guard arc in degrees; 0 means the default 90.
    pub guard_angle: i16,
    pub guard_base_repel: u8,
    pub stamina_guard_def: i16,
    pub stamina_just_guard_def: i16,
    pub phys_guard_cut_rate: f32,
    pub phys_just_guard_cut_rate: f32,
    /// `passiveStaminaAtkRate`: multiplies the posture sent back to an attacker on a deflect.
    pub passive_stamina_atk_rate: f32,
    /// `attackBaseStamina`: weapon posture attack, scaled by the attack row's stamina correction.
    pub attack_base_stamina: u16,
    /// `attackBaseRepel`: weapon repel attack, scaled by `guardAtkRateCorrection`.
    pub attack_base_repel: u8,
    /// `staminaAttackPowerRate`: multiplies the attack row's fixed posture value.
    pub stamina_attack_power_rate: f32,
    /// `staminaGuardDef_MaxCorrect`: stat-driven guard posture bonus (0 on the Kusabimaru).
    pub stamina_guard_def_max_correct: f32,
    /// Per-attribute guard cut corrections in percent (`slashGuardCutRate` ..
    /// `attriCAttackCutRate`, indexed by `atkAttribute - 1`), read from code `10d1b40`.
    pub guard_cut_attribute_rates: [i8; 12],
}

impl WeaponGuard {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id: row.id(),
            guard_angle: row.i16("guardAngle")?,
            guard_base_repel: row.u8("guardBaseRepel")?,
            stamina_guard_def: row.i16("staminaGuardDef")?,
            stamina_just_guard_def: row.i16("staminaJustGuardDef")?,
            phys_guard_cut_rate: row.f32("physGuardCutRate")?,
            phys_just_guard_cut_rate: row.f32("physJustGuardCutRate")?,
            passive_stamina_atk_rate: row.f32("passiveStaminaAtkRate")?,
            attack_base_stamina: row.u16("attackBaseStamina")?,
            attack_base_repel: row.u8("attackBaseRepel")?,
            stamina_attack_power_rate: row.f32("staminaAttackPowerRate")?,
            stamina_guard_def_max_correct: row.f32("staminaGuardDef_MaxCorrect")?,
            guard_cut_attribute_rates: {
                const CUTS: [&str; 12] = [
                    "slashGuardCutRate",
                    "lightHitGuardCutRate",
                    "thrustGuardCutRate",
                    "neutralAttackCutRate",
                    "ninsatsuAttackCutRate",
                    "heavyHitAttackCutRate",
                    "antiGroundAttackCutRate",
                    "antiAirAttackCutRate",
                    "lightShootAttackCutRate",
                    "attriAAttackCutRate",
                    "attriBAttackCutRate",
                    "attriCAttackCutRate",
                ];
                let mut out = [0; 12];
                for (r, name) in out.iter_mut().zip(CUTS) {
                    *r = row.i8(name)?;
                }
                out
            },
        })
    }
}

/// One StaminaControlParam row: per recovery type (0..15) a recovery percent and the upper and
/// lower posture limits in percent of max.
#[derive(Debug, Clone, PartialEq)]
pub struct StaminaControl {
    pub id: i32,
    pub recover_ratio: [u32; 16],
    pub max_ratio: [u16; 16],
    pub min_ratio: [u16; 16],
}

impl StaminaControl {
    pub fn from_row(row: &Row) -> Result<Self> {
        let mut out = Self {
            id: row.id(),
            recover_ratio: [100; 16],
            max_ratio: [100; 16],
            min_ratio: [0; 16],
        };
        for i in 0..16 {
            out.recover_ratio[i] = row.u32(&format!("staminaRecoverRatio_forType{i:03}"))?;
            out.max_ratio[i] = row.u16(&format!("staminaMaxRatio_forType{i:03}"))?;
            out.min_ratio[i] = row.u16(&format!("staminaMinRatio_forType{i:03}"))?;
        }
        Ok(out)
    }

    /// Recovery percent for `kind`, or 100 for an unknown type (read from code, `10cf5c0`).
    pub fn recover_ratio(&self, kind: usize) -> u32 {
        self.recover_ratio.get(kind).copied().unwrap_or(100)
    }
}

/// One CalcCorrectGraph row: five breakpoints with output values and curve exponents.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcCorrectGraph {
    pub id: i32,
    pub stage_max_val: [f32; 5],
    pub stage_max_grow_val: [f32; 5],
    pub adj_pt: [f32; 5],
}

impl CalcCorrectGraph {
    pub fn from_row(row: &Row) -> Result<Self> {
        let mut out = Self {
            id: row.id(),
            stage_max_val: [0.0; 5],
            stage_max_grow_val: [0.0; 5],
            adj_pt: [1.0; 5],
        };
        for i in 0..5 {
            out.stage_max_val[i] = row.f32(&format!("stageMaxVal{i}"))?;
            out.stage_max_grow_val[i] = row.f32(&format!("stageMaxGrowVal{i}"))?;
            out.adj_pt[i] = row.f32(&format!("adjPt_maxGrowVal{i}"))?;
        }
        Ok(out)
    }

    /// Evaluates the graph at `x` (read from code, `850b10`).
    ///
    /// `x` is capped at the last breakpoint; `x <= 0` returns the first output. The segment is the
    /// first one whose upper breakpoint is at least `x`. With exponent `e >= 0` the output is
    /// `g0 + ((x - v0) / (v1 - v0))^e * (g1 - g0)`; with `e < 0` it is
    /// `g0 + (1 - ((v1 - x) / (v1 - v0))^(-e)) * (g1 - g0)`. The result is clamped to the
    /// segment's output range.
    pub fn eval(&self, x: f32) -> f32 {
        let v = &self.stage_max_val;
        let g = &self.stage_max_grow_val;
        let x = x.min(v[4]);
        if x <= 0.0 {
            return g[0];
        }
        let i = (0..4).find(|&i| v[i + 1] >= x).unwrap_or(3);
        let dv = v[i + 1] - v[i];
        let dg = g[i + 1] - g[i];
        let e = self.adj_pt[i];
        let y = if e >= 0.0 {
            g[i] + ((x - v[i]) / dv).powf(e) * dg
        } else {
            g[i] + (1.0 - ((dv - (x - v[i])) / dv).powf(-e)) * dg
        };
        y.clamp(g[i].min(g[i + 1]), g[i].max(g[i + 1]))
    }
}
