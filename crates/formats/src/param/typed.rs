//! Typed views of the param tables combat needs, for fast field access in the simulation.
//!
//! Each struct is built once from generic [`Row`]s by Paramdex field name, so a renamed or
//! missing field fails loudly at load time instead of reading the wrong bytes. Field names
//! are the Paramdex internal names in snake case. Sekiro data calls posture "stamina";
//! these structs keep the data's word. Meanings are summarised in `docs/PARAMS.md`.

use std::collections::HashMap;

use serde::Serialize;

use super::{ParamSet, Row, Table};
use crate::reader::{Error, Result};

/// A field type that can be read from a row by name.
pub trait ReadField: Sized {
    type Key: Copy;
    fn read(row: &Row<'_>, key: Self::Key) -> Result<Self>;
}

macro_rules! read_int {
    ($($ty:ty),*) => {$(
        impl ReadField for $ty {
            type Key = &'static str;
            fn read(row: &Row<'_>, key: &'static str) -> Result<Self> {
                let v = row.int(key)?;
                <$ty>::try_from(v).map_err(|_| {
                    Error::Format(format!("{row:?}.{key} = {v} does not fit {}", stringify!($ty)))
                })
            }
        }
    )*};
}
read_int!(i8, u8, i16, u16, i32, u32);

impl ReadField for f32 {
    type Key = &'static str;
    fn read(row: &Row<'_>, key: &'static str) -> Result<Self> {
        row.f32(key)
    }
}

impl ReadField for bool {
    type Key = &'static str;
    fn read(row: &Row<'_>, key: &'static str) -> Result<Self> {
        match row.int(key)? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(Error::Format(format!("{row:?}.{key} = {v} is not a flag"))),
        }
    }
}

impl<T: ReadField<Key = &'static str>, const N: usize> ReadField for [T; N] {
    type Key = [&'static str; N];
    fn read(row: &Row<'_>, keys: Self::Key) -> Result<Self> {
        let items = keys
            .iter()
            .map(|k| T::read(row, k))
            .collect::<Result<Vec<T>>>()?;
        Ok(items
            .try_into()
            .unwrap_or_else(|_| unreachable!("N keys give N items")))
    }
}

/// A typed row built from a generic row.
pub trait FromRow: Sized {
    /// Paramdex param type this struct reads, e.g. `ATK_PARAM_ST`.
    const PARAM_TYPE: &'static str;
    fn from_row(row: &Row<'_>) -> Result<Self>;
}

/// All rows of a table converted to `T`, with id lookup.
#[derive(Debug, Clone)]
pub struct TypedTable<T> {
    rows: Vec<T>,
    ids: Vec<i32>,
    index: HashMap<i32, usize>,
}

impl<T: FromRow> TypedTable<T> {
    pub fn from_table(table: &Table) -> Result<Self> {
        if table.def().param_type != T::PARAM_TYPE {
            return Err(Error::Format(format!(
                "{} is {}, not {}",
                table.name(),
                table.def().param_type,
                T::PARAM_TYPE
            )));
        }
        let mut rows = Vec::with_capacity(table.len());
        let mut ids = Vec::with_capacity(table.len());
        let mut index = HashMap::with_capacity(table.len());
        for (i, row) in table.rows().enumerate() {
            rows.push(T::from_row(&row)?);
            ids.push(row.id());
            index.entry(row.id()).or_insert(i);
        }
        Ok(Self { rows, ids, index })
    }
}

impl<T> TypedTable<T> {
    pub fn get(&self, id: i32) -> Option<&T> {
        self.index.get(&id).map(|&i| &self.rows[i])
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// `(id, row)` pairs in file order.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &T)> {
        self.ids.iter().copied().zip(self.rows.iter())
    }
}

macro_rules! typed_param {
    (
        $(#[$meta:meta])*
        $name:ident: $param_type:literal {
            $( $(#[$fmeta:meta])* $field:ident : $ty:ty = $key:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Serialize)]
        pub struct $name {
            pub id: i32,
            $( $(#[$fmeta])* pub $field: $ty, )*
        }

        impl FromRow for $name {
            const PARAM_TYPE: &'static str = $param_type;
            fn from_row(row: &Row<'_>) -> Result<Self> {
                Ok(Self {
                    id: row.id(),
                    $( $field: <$ty as ReadField>::read(row, $key)?, )*
                })
            }
        }
    };
}

typed_param! {
    /// An attack hit (`AtkParam_Npc`, `AtkParam_Pc`). "Stam" fields are posture.
    AtkParam: "ATK_PARAM_ST" {
        hit_radius: [f32; 16] = [
            "hit0_Radius", "hit1_Radius", "hit2_Radius", "hit3_Radius", "hit4_Radius",
            "hit5_Radius", "hit6_Radius", "hit7_Radius", "hit8_Radius", "hit9_Radius",
            "hit10_Radius", "hit11_Radius", "hit12_Radius", "hit13_Radius", "hit14_Radius",
            "hit15_Radius",
        ],
        hit_dmy_poly1: [i16; 16] = [
            "hit0_DmyPoly1", "hit1_DmyPoly1", "hit2_DmyPoly1", "hit3_DmyPoly1", "hit4_DmyPoly1",
            "hit5_DmyPoly1", "hit6_DmyPoly1", "hit7_DmyPoly1", "hit8_DmyPoly1", "hit9_DmyPoly1",
            "hit10_DmyPoly1", "hit11_DmyPoly1", "hit12_DmyPoly1", "hit13_DmyPoly1",
            "hit14_DmyPoly1", "hit15_DmyPoly1",
        ],
        hit_dmy_poly2: [i16; 16] = [
            "hit0_DmyPoly2", "hit1_DmyPoly2", "hit2_DmyPoly2", "hit3_DmyPoly2", "hit4_DmyPoly2",
            "hit5_DmyPoly2", "hit6_DmyPoly2", "hit7_DmyPoly2", "hit8_DmyPoly2", "hit9_DmyPoly2",
            "hit10_DmyPoly2", "hit11_DmyPoly2", "hit12_DmyPoly2", "hit13_DmyPoly2",
            "hit14_DmyPoly2", "hit15_DmyPoly2",
        ],
        hit_type: [u8; 16] = [
            "hit0_hitType", "hit1_hitType", "hit2_hitType", "hit3_hitType", "hit4_hitType",
            "hit5_hitType", "hit6_hitType", "hit7_hitType", "hit8_hitType", "hit9_hitType",
            "hit10_hitType", "hit11_hitType", "hit12_hitType", "hit13_hitType", "hit14_hitType",
            "hit15_hitType",
        ],
        knockback_dist_direct_hit: f32 = "knockbackDist_DirectHit",
        knockback_dist_guard: f32 = "knockbackDist_Guard",
        knockback_dist_just_guard: f32 = "knockbackDist_JustGuard",
        hit_stop_time: f32 = "hitStopTime",
        hit_stop_time_defencer: f32 = "hitStopTime_Defencer",
        sp_effect_ids: [i32; 5] = [
            "spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4",
        ],
        /// PC only: multiplier on the weapon's physical attack.
        atk_phys_correction: u16 = "atkPhysCorrection",
        /// PC only: multiplier on guarded posture attack.
        atk_stam_correction: u16 = "atkStamCorrection",
        /// PC only: multiplier on direct-hit posture attack.
        direct_atk_stam_correction: u16 = "directAtkStamCorrection",
        guard_atk_rate_correction: u16 = "guardAtkRateCorrection",
        guard_break_correction: u16 = "guardBreakCorrection",
        atk_super_armor_correction: u16 = "atkSuperArmorCorrection",
        /// NPC only: base physical damage.
        atk_phys: u16 = "atkPhys",
        atk_mag: u16 = "atkMag",
        atk_fire: u16 = "atkFire",
        atk_thun: u16 = "atkThun",
        atk_dark: u16 = "atkDark",
        /// Posture attack when the attacker loses the deflect contest (def description).
        atk_stam: i16 = "atkStam",
        /// Posture attack on direct hit.
        direct_atk_stam_damage: i16 = "directAtkStamDamage",
        /// Posture attack when the attacker wins the deflect contest (despite the name).
        repel_lost_stam_damage: i16 = "repelLostStamDamage",
        /// Posture the attacker itself takes on a direct hit.
        direct_atk_stam_damage_attacker: i16 = "directAtkStamDamage_Attacker",
        /// Posture the attacker itself takes when it wins the deflect contest.
        repel_victory_stam_damage_attacker: i16 = "repelVictoryStamDamage_Attacker",
        /// Posture the attacker itself takes when it loses the deflect contest (deflected).
        repel_lost_stam_damage_attacker: i16 = "repelLostStamDamage_Attacker",
        /// Posture the attacker takes when its attack is contact-parried (Mikiri counter).
        stamina_damage_attack_hit_parry: i16 = "staminaDamageAttackHitParry",
        /// NPC only: repel ("flick") attack power.
        guard_atk_rate: u16 = "guardAtkRate",
        /// NPC only: repel defence, used to decide whether the attack is repelled.
        guard_break_rate: u16 = "guardBreakRate",
        atk_super_armor: u16 = "atkSuperArmor",
        atk_throw_escape: u16 = "atkThrowEscape",
        guard_stamina_cut_rate: i16 = "guardStaminaCutRate",
        guard_rate: i16 = "guardRate",
        throw_type_id: u16 = "throwTypeId",
        dmg_level: u8 = "dmgLevel",
        dmg_level_vs_player: i8 = "dmgLevel_vsPlayer",
        guard_cut_cancel_rate: i8 = "guardCutCancelRate",
        atk_attribute: u8 = "atkAttribute",
        sp_attribute: u8 = "spAttribute",
        atk_type: u8 = "atkType",
        stamina_physics_attribute: u8 = "staminaPhysicsAttribute",
        hit_source_type: u8 = "hitSourceType",
        throw_flag: u8 = "throwFlag",
        is_all_dir_guard: bool = "isAllDirGuard",
        disable_stamina_attack: bool = "disableStaminaAttack",
        disable_hit_sp_effect: bool = "disableHitSpEffect",
        is_disable_no_damage: bool = "isDisableNoDamage",
        oppose_target: bool = "opposeTarget",
        friendly_target: bool = "friendlyTarget",
        self_target: bool = "selfTarget",
        /// Whether posture hitting 0 from a deflect sends both sides into break reactions.
        does_break_repel_stam_damage: bool = "doesBreakRepelStamDamage",
        is_disable_parry: bool = "isDisableParry",
        atk_behavior_id: u8 = "atkBehaviorId",
        atk_behavior_id_2: u8 = "atkBehaviorId_2",
        excess_dmg_keep_hp: u16 = "excessDmgKeepHp",
        deflect_action: u8 = "deflectAction",
        deflected_action: u8 = "deflectedAction",
        just_deflect_action: u8 = "justDeflectAction",
        just_deflected_action: u8 = "justDeflectedAction",
        counter_sp_effect_condition: u8 = "counterSpEffectCondition",
        attack_direction_point: u8 = "attackDirectionPoint",
        guard_attribute: u8 = "guardAttribute",
        disable_guard_vs_guard_attribute0: u8 = "disableGuard_vsGuardAttribute0",
        disable_just_guard_vs_guard_attribute0: u8 = "disableJustGuard_vsGuardAttribute0",
        disable_guard_vs_guard_attribute1: u8 = "disableGuard_vsGuardAttribute1",
        disable_just_guard_vs_guard_attribute1: u8 = "disableJustGuard_vsGuardAttribute1",
        overwrite_attack_element_correct_id: i32 = "overwriteAttackElementCorrectId",
    }
}

typed_param! {
    /// Links an animation event (variation + judge id) to an attack, bullet or effect
    /// (`BehaviorParam`, `BehaviorParam_PC`).
    BehaviorParam: "BEHAVIOR_PARAM_ST" {
        variation_id: i32 = "variationId",
        behavior_judge_id: i32 = "behaviorJudgeId",
        ez_state_behavior_type_old: u8 = "ezStateBehaviorType_old",
        /// 0 attack, 1 bullet, 2 special effect (community docs).
        ref_type: u8 = "refType",
        ref_id: i32 = "refId",
        sfx_variation_id: i32 = "sfxVariationId",
        stamina: i32 = "stamina",
        mp: i32 = "mp",
        category: u8 = "category",
        hero_point: u8 = "heroPoint",
    }
}

typed_param! {
    /// A special effect (`SpEffectParam`): buffs, states, and per-frame combat modifiers.
    SpEffectParam: "SP_EFFECT_PARAM_ST" {
        effect_endurance: f32 = "effectEndurance",
        motion_interval: f32 = "motionInterval",
        condition_hp: f32 = "conditionHp",
        condition_hp_rate: f32 = "conditionHpRate",
        state_info: u16 = "stateInfo",
        sp_category: u16 = "spCategory",
        category_priority: u8 = "categoryPriority",
        save_category: i8 = "saveCategory",
        invocation_conditions_state_change: [u16; 3] = [
            "invocationConditionsStateChange1",
            "invocationConditionsStateChange2",
            "invocationConditionsStateChange3",
        ],
        max_hp_rate: f32 = "maxHpRate",
        max_stamina_rate: f32 = "maxStaminaRate",
        max_hp_increase: i16 = "maxHpIncrease",
        max_sp_increase: i16 = "maxSpIncrease",
        change_hp_rate: f32 = "changeHpRate",
        change_hp_point: i32 = "changeHpPoint",
        hp_recover_rate: f32 = "hpRecoverRate",
        change_stamina_rate: f32 = "changeStaminaRate",
        change_stamina_point: i32 = "changeStaminaPoint",
        stamina_recover_change_speed: i32 = "staminaRecoverChangeSpeed",
        /// Multiplies posture recovery speed.
        stamina_recover_speed_rate: f32 = "staminaRecoverSpeedRate",
        consume_stamina_rate: f32 = "consumeStaminaRate",
        /// Attacker side: multiplier on posture attack power.
        stamina_attack_rate: f32 = "staminaAttackRate",
        /// Defender side: on a successful deflect, multiplies the attack's
        /// `repelLostStamDamage_Attacker` (posture sent back to the attacker).
        def_stamina_attack_rate: f32 = "defStaminaAttackRate",
        /// On a successful Mikiri counter, multiplies `staminaDamageAttackHitParry`.
        attack_hit_parry_stamina_attack_rate: f32 = "attackHitParryStaminaAttackRate",
        guard_stamina_cut_rate: f32 = "guardStaminaCutRate",
        guard_def_flick_power_rate: f32 = "guardDefFlickPowerRate",
        def_flick_power: u8 = "defFlickPower",
        flick_damage_cut_rate: u8 = "flickDamageCutRate",
        stamina_physics_attribute: u8 = "staminaPhysicsAttribute",
        def_slash_stamina_dmg_rate: f32 = "defSlashStaminaDmgRate",
        def_light_hit_stamina_dmg_rate: f32 = "defLightHitStaminaDmgRate",
        def_thrust_stamina_dmg_rate: f32 = "defThrustStaminaDmgRate",
        def_neutral_stamina_dmg_rate: f32 = "defNeutralStaminaDmgRate",
        def_ninsatu_stamina_dmg_rate: f32 = "defNinsatuStaminaDmgRate",
        def_heavy_hit_stamina_dmg_rate: f32 = "defHeavyHitStaminaDmgRate",
        def_anti_ground_stamina_dmg_rate: f32 = "defAntiGroundStaminaDmgRate",
        def_anti_air_stamina_dmg_rate: f32 = "defAntiAirStaminaDmgRate",
        def_light_shoot_stamina_dmg_rate: f32 = "defLightShootStaminaDmgRate",
        atk_player_dmg_correct_rate_stamina: f32 = "atkPlayerDmgCorrectRate_Stamina",
        atk_enemy_dmg_correct_rate_stamina: f32 = "atkEnemyDmgCorrectRate_Stamina",
        def_player_dmg_correct_rate_stamina: f32 = "defPlayerDmgCorrectRate_Stamina",
        def_enemy_dmg_correct_rate_stamina: f32 = "defEnemyDmgCorrectRate_Stamina",
        physics_attack_rate: f32 = "physicsAttackRate",
        physics_attack_power_rate: f32 = "physicsAttackPowerRate",
        physics_diffence_rate: f32 = "physicsDiffenceRate",
        slash_damage_cut_rate: f32 = "slashDamageCutRate",
        light_hit_damage_cut_rate: f32 = "lightHitDamageCutRate",
        thrust_damage_cut_rate: f32 = "thrustDamageCutRate",
        neutral_damage_cut_rate: f32 = "neutralDamageCutRate",
        no_guard_damage_rate: f32 = "NoGuardDamageRate",
        def_player_dmg_correct_rate_physics: f32 = "defPlayerDmgCorrectRate_Physics",
        def_enemy_dmg_correct_rate_physics: f32 = "defEnemyDmgCorrectRate_Physics",
        atk_player_dmg_correct_rate_physics: f32 = "atkPlayerDmgCorrectRate_Physics",
        atk_enemy_dmg_correct_rate_physics: f32 = "atkEnemyDmgCorrectRate_Physics",
        toughness_damage_cut_rate: f32 = "toughnessDamageCutRate",
        /// Restores remaining deathblows needed.
        recove_rremain_ninsatsu_num: u8 = "recoveRremainNinsatsuNum",
        ninsatsu_attack_power: i32 = "ninsatsuAttackPower",
        ninsatsu_attack_power_rate: f32 = "ninsatsuAttackPowerRate",
        atk_ninsatsu_dmg_rate: f32 = "atkNinsatsuDmgRate",
        def_ninsatsu_dmg_rate: f32 = "defNinsatsuDmgRate",
    }
}

typed_param! {
    /// An enemy or NPC character (`NpcParam`). `stamina` is maximum posture.
    NpcParam: "NPC_PARAM_ST" {
        behavior_variation_id: i32 = "behaviorVariationId",
        name_id: i32 = "nameId",
        hp: u32 = "hp",
        /// Maximum posture.
        stamina: u16 = "stamina",
        /// Base posture recovery in points per second.
        stamina_recover_base_vel: u16 = "staminaRecoverBaseVel",
        /// Row in `StaminaControlParam` scaling recovery per animation state.
        stamina_control_param_id: u32 = "staminaControlParamId",
        max_debt_stamina: i16 = "maxDebtStamina",
        /// Resident special effects 0..31.
        sp_effect_ids: [i32; 32] = [
            "spEffectID0", "spEffectID1", "spEffectID2", "spEffectID3", "spEffectID4",
            "spEffectID5", "spEffectID6", "spEffectID7", "spEffectID8", "spEffectID9",
            "spEffectID10", "spEffectID11", "spEffectID12", "spEffectID13", "spEffectID14",
            "spEffectID15", "spEffectID16", "spEffectID17", "spEffectID18", "spEffectID19",
            "spEffectID20", "spEffectID21", "spEffectID22", "spEffectID23", "spEffectID24",
            "spEffectID25", "spEffectID26", "spEffectID27", "spEffectID28", "spEffectID29",
            "spEffectID30", "spEffectID31",
        ],
        game_clear_sp_effect_id: i32 = "GameClearSpEffectID",
        hard_mode_sp_effect_id: i32 = "HardModeSpEffectID",
        growth_doping_sp_effect_id: i32 = "growthDopingSpEffectID",
        /// Deathblows needed to kill (boss "health bars").
        ninsatu_num: i8 = "ninsatuNum",
        def_flick_power: u16 = "defFlickPower",
        flick_damage_cut_rate: u8 = "flickDamageCutRate",
        guard_angle: i16 = "guardAngle",
        guard_level: i8 = "guardLevel",
        phys_guard_cut_rate: f32 = "physGuardCutRate",
        slash_guard_cut_rate: i16 = "slashGuardCutRate",
        light_hit_guard_cut_rate: i16 = "lightHitGuardCutRate",
        thrust_guard_cut_rate: i16 = "thrustGuardCutRate",
        stamina_guard_def: u8 = "staminaGuardDef",
        def_phys: u16 = "def_phys",
        def_slash: i16 = "def_slash",
        def_light_hit: i16 = "def_lightHit",
        def_thrust: i16 = "def_thrust",
        neutral_damage_cut_rate: f32 = "neutralDamageCutRate",
        slash_damage_cut_rate: f32 = "slashDamageCutRate",
        light_hit_damage_cut_rate: f32 = "lightHitDamageCutRate",
        thrust_damage_cut_rate: f32 = "thrustDamageCutRate",
        ninsatsu_damage_rate: f32 = "ninsatsuDamageRate",
        slash_stamina_dmg_rate: f32 = "slashStaminaDmgRate",
        light_hit_stamina_dmg_rate: f32 = "lightHitStaminaDmgRate",
        thrust_stamina_dmg_rate: f32 = "thrustStaminaDmgRate",
        neutral_stamina_dmg_rate: f32 = "neutralStaminaDmgRate",
        ninsatu_stamina_dmg_rate: f32 = "ninsatuStaminaDmgRate",
        heavy_hit_stamina_dmg_rate: f32 = "heavyHitStaminaDmgRate",
        anti_ground_stamina_dmg_rate: f32 = "antiGroundStaminaDmgRate",
        anti_air_stamina_dmg_rate: f32 = "antiAirStaminaDmgRate",
        light_shoot_stamina_dmg_rate: f32 = "lightShootStaminaDmgRate",
        super_armor_durability: i16 = "superArmorDurability",
        super_armor_recover_correction: f32 = "superArmorRecoverCorrection",
        toughness: u32 = "toughness",
        toughness_recover_correction: f32 = "toughnessRecoverCorrection",
        parry_attack: u8 = "parryAttack",
        parry_defence: u8 = "parryDefence",
        knockback_rate_vs_player: [u8; 3] = [
            "knockbackRate_vsPlayer_DirectHit",
            "knockbackRate_vsPlayer_Guard",
            "knockbackRate_vsPlayer_JustGuard",
        ],
        knockback_rate_vs_enemy: [u8; 3] = [
            "knockbackRate_vsEnemy_DirectHit",
            "knockbackRate_vsEnemy_Guard",
            "knockbackRate_vsEnemy_JustGuard",
        ],
        hit_stop_type: u8 = "hitStopType",
        hit_stop_type_defencer: u8 = "hitStopType_Defencer",
        is_no_damage_motion: bool = "isNoDamageMotion",
        weight: u32 = "weight",
        turn_vellocity: f32 = "turnVellocity",
        chr_hit_height: f32 = "chrHitHeight",
        chr_hit_radius: f32 = "chrHitRadius",
        hit_height: f32 = "hitHeight",
        hit_radius: f32 = "hitRadius",
        npc_type: u8 = "npcType",
        team_type: u8 = "teamType",
        lock_dist: u8 = "lockDist",
    }
}

typed_param! {
    /// A grab or deathblow pairing (`ThrowParam`).
    ThrowParam: "THROW_INFO_BANK" {
        atk_chr_id: i32 = "AtkChrId",
        def_chr_id: i32 = "DefChrId",
        dist: f32 = "Dist",
        diff_ang_min: f32 = "DiffAngMin",
        diff_ang_max: f32 = "DiffAngMax",
        upper_y_range: f32 = "upperYRange",
        lower_y_range: f32 = "lowerYRange",
        diff_ang_my_to_def: f32 = "diffAngMyToDef",
        throw_type_id: i32 = "throwTypeId",
        atk_anim_id: i32 = "atkAnimId",
        def_anim_id: i32 = "defAnimId",
        esc_hp: u16 = "escHp",
        self_esc_cycle_time: u16 = "selfEscCycleTime",
        pad_type: u8 = "PadType",
        atk_enable_state: u8 = "AtkEnableState",
        self_esc_cycle_cnt: u8 = "selfEscCycleCnt",
        dmy_has_chr_dir_type: u8 = "dmyHasChrDirType",
        is_turn_atker: bool = "isTurnAtker",
        is_skip_wep_cate: bool = "isSkipWepCate",
        is_skip_sphere_cast: bool = "isSkipSphereCast",
        atk_sorb_dmy_id: i16 = "atkSorbDmyId",
        def_sorb_dmy_id: i16 = "defSorbDmyId",
        dist_start: f32 = "Dist_start",
        diff_ang_min_start: f32 = "DiffAngMin_start",
        diff_ang_max_start: f32 = "DiffAngMax_start",
        upper_y_range_start: f32 = "upperYRange_start",
        lower_y_range_start: f32 = "lowerYRange_start",
        diff_ang_my_to_def_start: f32 = "diffAngMyToDef_start",
        throw_kind: u32 = "throwKind",
        additional_condition_sp_effect_for_atk: i32 = "additionalConditionSpecialEffect_forAtk",
        additional_condition_sp_effect_for_def: i32 = "additionalConditionSpecialEffect_forDef",
        atk_anim_offset: u16 = "atkAnimOffset",
        def_anim_offset: u16 = "defAnimOffset",
    }
}

typed_param! {
    /// A weapon (`EquipParamWeapon`); the Kusabimaru and prosthetic tools.
    EquipParamWeapon: "EQUIP_PARAM_WEAPON_ST" {
        behavior_variation_id: i32 = "behaviorVariationId",
        weight: f32 = "weight",
        sp_effect_behavior_ids: [i32; 3] = [
            "spEffectBehaviorId0", "spEffectBehaviorId1", "spEffectBehaviorId2",
        ],
        resident_sp_effect_ids: [i32; 3] = [
            "residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2",
        ],
        attack_base_physics: u16 = "attackBasePhysics",
        attack_base_stamina: u16 = "attackBaseStamina",
        attack_base_parry: u8 = "attackBaseParry",
        defense_base_parry: u8 = "defenseBaseParry",
        guard_base_repel: u8 = "guardBaseRepel",
        attack_base_repel: u8 = "attackBaseRepel",
        parry_damage_life: i16 = "parryDamageLife",
        guard_angle: i16 = "guardAngle",
        guard_level: i8 = "guardLevel",
        guard_cut_cancel_rate: i8 = "guardCutCancelRate",
        phys_guard_cut_rate: f32 = "physGuardCutRate",
        phys_just_guard_cut_rate: f32 = "physJustGuardCutRate",
        slash_guard_cut_rate: i8 = "slashGuardCutRate",
        light_hit_guard_cut_rate: i8 = "lightHitGuardCutRate",
        thrust_guard_cut_rate: i8 = "thrustGuardCutRate",
        stamina_guard_def: i16 = "staminaGuardDef",
        stamina_just_guard_def: i16 = "staminaJustGuardDef",
        stamina_attack_power_rate: f32 = "staminaAttackPowerRate",
        passive_stamina_atk_rate: f32 = "passiveStaminaAtkRate",
        stamina_consumption_rate: f32 = "staminaConsumptionRate",
        knock_back_cut_rate_vs_player_guard: u8 = "knockBackCutRate_vsPlayer_Guard",
        knock_back_cut_rate_vs_player_just_guard: u8 = "knockBackCutRate_vsPlayer_JustGuard",
        knock_back_cut_rate_vs_enemy_guard: u8 = "knockBackCutRate_vsEnemy_Guard",
        knock_back_cut_rate_vs_enemy_just_guard: u8 = "knockBackCutRate_vsEnemy_JustGuard",
        wepmotion_category: u8 = "wepmotionCategory",
        guardmotion_category: u8 = "guardmotionCategory",
        sp_atkcategory: u16 = "spAtkcategory",
        enable_guard: bool = "enableGuard",
        enable_parry: bool = "enableParry",
        attack_element_correct_id: i32 = "attackElementCorrectId",
        toughness_correct_rate: f32 = "toughnessCorrectRate",
    }
}

typed_param! {
    /// Posture recovery scaling by animation-selected type 0..15 (`StaminaControlParam`).
    StaminaControlParam: "STAMINA_CONTROL_PARAM_ST" {
        /// Recovery rate in percent per type.
        recover_ratio: [u32; 16] = [
            "staminaRecoverRatio_forType000", "staminaRecoverRatio_forType001",
            "staminaRecoverRatio_forType002", "staminaRecoverRatio_forType003",
            "staminaRecoverRatio_forType004", "staminaRecoverRatio_forType005",
            "staminaRecoverRatio_forType006", "staminaRecoverRatio_forType007",
            "staminaRecoverRatio_forType008", "staminaRecoverRatio_forType009",
            "staminaRecoverRatio_forType010", "staminaRecoverRatio_forType011",
            "staminaRecoverRatio_forType012", "staminaRecoverRatio_forType013",
            "staminaRecoverRatio_forType014", "staminaRecoverRatio_forType015",
        ],
        /// Upper posture limit in percent per type.
        max_ratio: [u16; 16] = [
            "staminaMaxRatio_forType000", "staminaMaxRatio_forType001",
            "staminaMaxRatio_forType002", "staminaMaxRatio_forType003",
            "staminaMaxRatio_forType004", "staminaMaxRatio_forType005",
            "staminaMaxRatio_forType006", "staminaMaxRatio_forType007",
            "staminaMaxRatio_forType008", "staminaMaxRatio_forType009",
            "staminaMaxRatio_forType010", "staminaMaxRatio_forType011",
            "staminaMaxRatio_forType012", "staminaMaxRatio_forType013",
            "staminaMaxRatio_forType014", "staminaMaxRatio_forType015",
        ],
        /// Lower posture limit in percent per type.
        min_ratio: [u16; 16] = [
            "staminaMinRatio_forType000", "staminaMinRatio_forType001",
            "staminaMinRatio_forType002", "staminaMinRatio_forType003",
            "staminaMinRatio_forType004", "staminaMinRatio_forType005",
            "staminaMinRatio_forType006", "staminaMinRatio_forType007",
            "staminaMinRatio_forType008", "staminaMinRatio_forType009",
            "staminaMinRatio_forType010", "staminaMinRatio_forType011",
            "staminaMinRatio_forType012", "staminaMinRatio_forType013",
            "staminaMinRatio_forType014", "staminaMinRatio_forType015",
        ],
    }
}

/// The typed tables the combat simulation reads, loaded together.
#[derive(Debug, Clone)]
pub struct CombatParams {
    pub atk_npc: TypedTable<AtkParam>,
    pub atk_pc: TypedTable<AtkParam>,
    pub behavior_npc: TypedTable<BehaviorParam>,
    pub behavior_pc: TypedTable<BehaviorParam>,
    pub sp_effect: TypedTable<SpEffectParam>,
    pub npc: TypedTable<NpcParam>,
    pub throw: TypedTable<ThrowParam>,
    pub weapon: TypedTable<EquipParamWeapon>,
    pub stamina_control: TypedTable<StaminaControlParam>,
}

impl CombatParams {
    pub fn from_set(set: &ParamSet) -> Result<Self> {
        fn load<T: FromRow>(set: &ParamSet, name: &str) -> Result<TypedTable<T>> {
            TypedTable::from_table(set.table(name)?)
        }
        Ok(Self {
            atk_npc: load(set, "AtkParam_Npc")?,
            atk_pc: load(set, "AtkParam_Pc")?,
            behavior_npc: load(set, "BehaviorParam")?,
            behavior_pc: load(set, "BehaviorParam_PC")?,
            sp_effect: load(set, "SpEffectParam")?,
            npc: load(set, "NpcParam")?,
            throw: load(set, "ThrowParam")?,
            weapon: load(set, "EquipParamWeapon")?,
            stamina_control: load(set, "StaminaControlParam")?,
        })
    }
}
