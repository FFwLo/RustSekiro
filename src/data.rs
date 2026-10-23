//! Loads `extracted/combat_data.json` (built by tools/sekiro-extract from the
//! local Sekiro install) and answers timeline questions about it:
//! which TAE events are active at time t, which SpEffect behaviour refs are
//! up, where the root motion moves the character, and so on.
//!
//! All TAE times are seconds; Sekiro authors them on a 30 fps grid.

use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub const TAE_FPS: f32 = 30.0;

#[derive(Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: i32,
    pub start: f32,
    pub end: f32,
    #[serde(default)]
    pub args: Value,
}

impl Event {
    pub fn active(&self, t: f32) -> bool {
        self.in_time(t) && self.ungated()
    }
    pub fn in_time(&self, t: f32) -> bool {
        t >= self.start && t < self.end
    }
    /// Events with a StateInfo apply only while the character has an SpEffect of that
    /// state (exe FUN_140bfebf0 in the TAE handlers): weapon enchantments (152/153/357/358/
    /// 940 add elemental hits to every slash), shuriken/coin upgrades (907, 996-999), the
    /// aimed-deflect SFX (902). None of them is modelled, so gated events never fire.
    pub fn ungated(&self) -> bool {
        self.arg_i64("StateInfo").unwrap_or(0) == 0
    }
    pub fn arg_i64(&self, name: &str) -> Option<i64> {
        self.args.get(name)?.as_i64()
    }
    /// ChrActionFlag FlagType is "87: Name" when the template knows it, else a bare number.
    pub fn flag_type(&self) -> Option<i64> {
        match self.args.get("FlagType")? {
            Value::String(s) => s.split(':').next()?.trim().parse().ok(),
            v => v.as_i64(),
        }
    }
}

#[derive(Deserialize, Default)]
pub struct Anim {
    pub duration: Option<f32>,
    #[serde(rename = "rootRate")]
    pub root_rate: Option<f32>,
    /// [x, y, z, yaw] per sample, game space (forward = -Z), cumulative from t = 0.
    #[serde(default)]
    pub root: Vec<[f32; 4]>,
    #[serde(default)]
    pub events: Vec<Event>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpEffect {
    /// HP damage taken while worn: <attr>DamageCutRate / def<Attr>DmgRate by atkAttribute (the
    /// physical part, damage_cut_field names) and the element rates; 1 when the export lacks them.
    #[serde(flatten, default)]
    pub damage_cut: DamageCut,
    /// def<Attr>StaminaDmgRate: posture damage taken while worn, by the attack's
    /// staminaPhysicsAttribute (combat::def_stamina_rate).
    #[serde(default = "one", rename = "defSlashStaminaDmgRate")]
    pub def_slash_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defLightHitStaminaDmgRate")]
    pub def_light_hit_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defThrustStaminaDmgRate")]
    pub def_thrust_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defNeutralStaminaDmgRate")]
    pub def_neutral_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defHeavyHitStaminaDmgRate")]
    pub def_heavy_hit_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defNinsatuStaminaDmgRate")]
    pub def_ninsatu_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defAntiGroundStaminaDmgRate")]
    pub def_anti_ground_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defAntiAirStaminaDmgRate")]
    pub def_anti_air_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defLightShootStaminaDmgRate")]
    pub def_light_shoot_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defAttriAStaminaDmgRate")]
    pub def_attri_a_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defAttriBStaminaDmgRate")]
    pub def_attri_b_stamina_dmg_rate: f32,
    #[serde(default = "one", rename = "defAttriCStaminaDmgRate")]
    pub def_attri_c_stamina_dmg_rate: f32,
    /// Grapple target while worn (kb/grapple.md section 4): the wire variation (-1 none; the
    /// player's arc row ChrPhysicsHomingParam 3000<variation>), the dummy poly the hook
    /// catches, the one Wolf lands at (260: the model root).
    #[serde(default = "minus_one")]
    pub chr_wire_variation_no: i64,
    #[serde(default = "minus_one")]
    pub chr_wire_target_dmypoly_id: i64,
    #[serde(default = "minus_one")]
    pub chr_wire_landing_point_dmypoly_id: i64,
    /// Healing (negative = heal) as % of max HP, e.g. the gourd's SpEffect 3000 (-40).
    #[serde(default)]
    pub change_hp_estus_flask_rate: f32,
    /// HP change as % of max HP (negative = heal), e.g. resurrection SpEffect 110015 (-50).
    #[serde(default)]
    pub change_hp_rate: f32,
    /// Area scaling multipliers ("Growth Doping" SpEffects).
    #[serde(default = "one")]
    pub max_hp_rate: f32,
    #[serde(default = "one")]
    pub max_stamina_rate: f32,
    #[serde(default = "one")]
    pub physics_attack_power_rate: f32,
    #[serde(default = "one")]
    pub stamina_attack_rate: f32,
    #[serde(default)]
    #[allow(dead_code)] // kept for debugging / future use
    pub name: String,
    #[serde(default)]
    pub behavior_ref_id: i64,
    #[serde(default)]
    pub state_info: i64,
    /// Duration in seconds (effectEndurance; 0 = while applied).
    #[serde(default)]
    pub effect_endurance: f32,
    /// spCategory / categoryPriority: within one category the lower priority wins (Paramdex
    /// SpEffectParam meta "lower one has priority").
    #[serde(default)]
    pub sp_category: i64,
    #[serde(default)]
    pub category_priority: i64,
    /// spCategory 1001 damage-motion replacement enums (dmgLv_None .. dmgLv_Breath, by HKS damage
    /// level 0-11; 0 = keep): `damage_motion_level`.
    #[serde(default, rename = "dmgLv_None")]
    pub dmg_lv_0: i64,
    #[serde(default, rename = "dmgLv_S")]
    pub dmg_lv_1: i64,
    #[serde(default, rename = "dmgLv_M")]
    pub dmg_lv_2: i64,
    #[serde(default, rename = "dmgLv_L")]
    pub dmg_lv_3: i64,
    #[serde(default, rename = "dmgLv_BlowM")]
    pub dmg_lv_4: i64,
    #[serde(default, rename = "dmgLv_Push")]
    pub dmg_lv_5: i64,
    #[serde(default, rename = "dmgLv_Strike")]
    pub dmg_lv_6: i64,
    #[serde(default, rename = "dmgLv_BlowS")]
    pub dmg_lv_7: i64,
    #[serde(default, rename = "dmgLv_Min")]
    pub dmg_lv_8: i64,
    #[serde(default, rename = "dmgLv_Uppercut")]
    pub dmg_lv_9: i64,
    #[serde(default, rename = "dmgLv_BlowLL")]
    pub dmg_lv_10: i64,
    #[serde(default, rename = "dmgLv_Breath")]
    pub dmg_lv_11: i64,
    #[serde(default)]
    pub def_stamina_attack_rate: f32,
    #[serde(default = "one")]
    pub stamina_recover_speed_rate: f32,
    /// Added to the posture regen base while worn (pts/s): kb/enemy.md FUN_140a04850,
    /// SpEffectRecoverRate * (base + SpEffectChangeSpeed). 300240 +40 on c1010 / c1190 battle idles,
    /// 300260 +60 on c1220's guard-break clips.
    #[serde(default)]
    pub stamina_recover_change_speed: f32,
    /// Active only while HP% <= this (resident HP-conditional effects); -1 or 0 = always.
    #[serde(default)]
    pub condition_hp: f32,
    /// Stealth (NPC sight on the wearer): % cut of the sight distances (crouch 109200: 20).
    #[serde(default)]
    pub sight_search_enemy_cut: f32,
    /// Factor on the "around" sight meter fill (wall hug 109210 / hanging 109220: 0).
    #[serde(default = "one")]
    pub around_sight_point_add_rate: f32,
    /// Factor on the AI sounds the wearer makes (Covert B 150010: 0.5).
    #[serde(default = "one")]
    pub hearing_search_enemy_rate: f32,
    /// On a listener: % cut of its hearing (FUN_140bfe590; the smoke / dazzle blinds 200130,
    /// 220011-220016, "hearing 0" 200140: 100).
    #[serde(default)]
    pub hearing_search_enemy_cut: f32,
    /// 0 = always, 1 / 2 = only when the observer's geometry bit 0 / 1 holds (hanging, wall hug).
    #[serde(default)]
    pub sight_cut_limit_type: i64,
    /// % cuts of the sight cone's angles.
    #[serde(default)]
    pub sight_search_left_angle_cut: f32,
    #[serde(default)]
    pub sight_search_right_angle_cut: f32,
    #[serde(default)]
    pub sight_search_upper_angle_cut: f32,
    #[serde(default)]
    pub sight_search_bottom_angle_cut: f32,
    /// Flat HP change per motionInterval (positive = damage; burn 9105: 4).
    #[serde(default)]
    pub change_hp_point: f32,
    /// Seconds between the HP changes of a damage-over-time effect.
    #[serde(default = "one")]
    pub motion_interval: f32,
    /// Build-ups (Paramdex SDT meta): poison with stateInfo 2, burn (Sekiro's "blood") with 6.
    #[serde(default)]
    pub poizon_attack_power: f32,
    #[serde(default)]
    pub regist_blood: f32,
    #[serde(default = "one")]
    pub regist_poizon_change_rate: f32,
    #[serde(default = "one")]
    pub regist_blood_change_rate: f32,
    /// Terror (stateInfo 5, the exe's resist type 1 "disease"): the zombies' dumplings 9222 / 9223.
    #[serde(default)]
    pub regist_illness: f32,
    #[serde(default = "one")]
    pub regist_illness_change_rate: f32,
    /// Applied when this one's effectEndurance runs out (Sabimaru 9004 -> 9045).
    #[serde(default)]
    pub replace_sp_effect_id: i64,
    /// SpEffectVfxParam rows of its look on the wearer (burn 9105: 40010 flames, 48010 kanji).
    #[serde(default = "minus_one")]
    pub vfx_id: i64,
    #[serde(default = "minus_one")]
    pub vfx_id1: i64,
    /// Posture change as % of max posture, positive = recovery as the rows' names read (忍殺時体幹回復
    /// 105050 / 150321 / 150331: 34 at a deathblow).
    #[serde(default)]
    pub change_stamina_rate: f32,
    /// The effect works only while one of these stateInfos is on the character (0 = no condition):
    /// the deathblow recoveries need their skill's permit (150301 HP (A): 986).
    #[serde(default)]
    pub invocation_conditions_state_change1: i64,
    #[serde(default)]
    pub invocation_conditions_state_change2: i64,
    #[serde(default)]
    pub invocation_conditions_state_change3: i64,
    /// A BehaviorParam row run every motionInterval at `dmypoly_id` while worn (stateInfo 275:
    /// the Demon of Hatred's foot checks 3702080 / 3702081 -> 1802 -> Bullet 70200950).
    #[serde(default)]
    pub behavior_id: i64,
    #[serde(default = "minus_one")]
    pub dmypoly_id: i64,
}

/// SpEffectParam def<Attr>StaminaDmgRate for an AtkParam staminaPhysicsAttribute (Paramdex SDT
/// enum ATKPARAM_STAMINA_ATKATTR_TYPE: 1 Slash, 2 Light Hit, 3 Thrust, 4 Neutral, 5 Ninsatsu,
/// 6 Heavy Hit, 7 Anti Ground, 8 Anti Air, 9 Light Shoot (the shuriken), 10-12 Attribute A-C;
/// 0 None has none).
pub fn stamina_rate_field(attribute: i64) -> Option<&'static str> {
    Some(match attribute {
        1 => "defSlashStaminaDmgRate",
        2 => "defLightHitStaminaDmgRate",
        3 => "defThrustStaminaDmgRate",
        4 => "defNeutralStaminaDmgRate",
        5 => "defNinsatuStaminaDmgRate",
        6 => "defHeavyHitStaminaDmgRate",
        7 => "defAntiGroundStaminaDmgRate",
        8 => "defAntiAirStaminaDmgRate",
        9 => "defLightShootStaminaDmgRate",
        10 => "defAttriAStaminaDmgRate",
        11 => "defAttriBStaminaDmgRate",
        12 => "defAttriCStaminaDmgRate",
        _ => return None,
    })
}

/// SpEffectParam HP damage rates while the effect is worn (round 25: 109400 x1.5 while Wolf is
/// posture-broken). gap: the exe site applying them is not traced (see damage_cut_field).
#[derive(Deserialize, Clone)]
pub struct DamageCut {
    #[serde(default = "one", rename = "slashDamageCutRate")]
    pub slash: f32,
    #[serde(default = "one", rename = "lightHitDamageCutRate")]
    pub light_hit: f32,
    #[serde(default = "one", rename = "thrustDamageCutRate")]
    pub thrust: f32,
    #[serde(default = "one", rename = "neutralDamageCutRate")]
    pub neutral: f32,
    #[serde(default = "one", rename = "defNinsatsuDmgRate")]
    pub ninsatsu: f32,
    #[serde(default = "one", rename = "defHeavyHitDmgRate")]
    pub heavy_hit: f32,
    #[serde(default = "one", rename = "defAntiGroundDmgRate")]
    pub anti_ground: f32,
    #[serde(default = "one", rename = "defAntiAirDmgRate")]
    pub anti_air: f32,
    #[serde(default = "one", rename = "defLightShootDmgRate")]
    pub light_shoot: f32,
    #[serde(default = "one", rename = "attriADamageCutRate")]
    pub attri_a: f32,
    #[serde(default = "one", rename = "attriBDamageCutRate")]
    pub attri_b: f32,
    #[serde(default = "one", rename = "attriCDamageCutRate")]
    pub attri_c: f32,
    #[serde(default = "one", rename = "fireDamageCutRate")]
    pub fire: f32,
    #[serde(default = "one", rename = "magicDamageCutRate")]
    pub magic: f32,
    #[serde(default = "one", rename = "thunderDamageCutRate")]
    pub thunder: f32,
    #[serde(default = "one", rename = "darkDamageCutRate")]
    pub dark: f32,
}

impl Default for DamageCut {
    fn default() -> Self {
        DamageCut { slash: 1.0, light_hit: 1.0, thrust: 1.0, neutral: 1.0, ninsatsu: 1.0, heavy_hit: 1.0, anti_ground: 1.0, anti_air: 1.0, light_shoot: 1.0, attri_a: 1.0, attri_b: 1.0, attri_c: 1.0, fire: 1.0, magic: 1.0, thunder: 1.0, dark: 1.0 }
    }
}

impl DamageCut {
    /// The rate for the physical part of a hit of this atkAttribute (damage_cut_field order).
    pub fn physical(&self, attribute: i64) -> f32 {
        match attribute {
            1 => self.slash,
            2 => self.light_hit,
            3 => self.thrust,
            4 => self.neutral,
            5 => self.ninsatsu,
            6 => self.heavy_hit,
            7 => self.anti_ground,
            8 => self.anti_air,
            9 => self.light_shoot,
            10 => self.attri_a,
            11 => self.attri_b,
            12 => self.attri_c,
            _ => 1.0,
        }
    }
}

/// SpEffectParam HP cut rate for an AtkParam atkAttribute (same enum values): <attr>DamageCutRate
/// for Slash, Light Hit, Thrust, Neutral and Attribute A-C, def<Attr>DmgRate for Ninsatsu, Heavy
/// Hit, Anti Ground, Anti Air and Light Shoot (Paramdex SDT SpEffect.xml:2598-2635, the same
/// description as slashDamageCutRate). gap: the exe site applying them is not traced.
pub fn damage_cut_field(attribute: i64) -> Option<&'static str> {
    Some(match attribute {
        1 => "slashDamageCutRate",
        2 => "lightHitDamageCutRate",
        3 => "thrustDamageCutRate",
        4 => "neutralDamageCutRate",
        5 => "defNinsatsuDmgRate",
        6 => "defHeavyHitDmgRate",
        7 => "defAntiGroundDmgRate",
        8 => "defAntiAirDmgRate",
        9 => "defLightShootDmgRate",
        10 => "attriADamageCutRate",
        11 => "attriBDamageCutRate",
        12 => "attriCDamageCutRate",
        _ => return None,
    })
}

/// NpcParam's own per-attribute rates (Paramdex SDT NpcParam.xml: "the damage result times this
/// is the final damage" / "multiplied into the stamina damage when the stamina physical attribute
/// is ..."): the HP one for an AtkParam atkAttribute, the posture one for its
/// staminaPhysicsAttribute (the same enum as `stamina_rate_field`).
pub fn npc_damage_rate_field(attribute: i64) -> Option<&'static str> {
    Some(match attribute {
        1 => "slashDamageCutRate",
        2 => "lightHitDamageCutRate",
        3 => "thrustDamageCutRate",
        4 => "neutralDamageCutRate",
        5 => "ninsatsuDamageRate",
        6 => "heavyHitDamageRate",
        7 => "antiGroundDamageRate",
        8 => "antiAirDamageRate",
        9 => "lightShootDamageRate",
        10 => "attriADamageCutRate",
        11 => "attriBDamageCutRate",
        12 => "attriCDamageCutRate",
        _ => return None,
    })
}

pub fn npc_stamina_rate_field(attribute: i64) -> Option<&'static str> {
    Some(match attribute {
        1 => "slashStaminaDmgRate",
        2 => "lightHitStaminaDmgRate",
        3 => "thrustStaminaDmgRate",
        4 => "neutralStaminaDmgRate",
        5 => "ninsatuStaminaDmgRate",
        6 => "heavyHitStaminaDmgRate",
        7 => "antiGroundStaminaDmgRate",
        8 => "antiAirStaminaDmgRate",
        9 => "lightShootStaminaDmgRate",
        10 => "attriAStaminaDmgRate",
        11 => "attriBStaminaDmgRate",
        12 => "attriCStaminaDmgRate",
        _ => return None,
    })
}

impl SpEffect {
    /// Its dmgLv_* replacement enum for HKS damage level `level` (0-11).
    pub fn dmg_lv(&self, level: i64) -> i64 {
        [
            self.dmg_lv_0, self.dmg_lv_1, self.dmg_lv_2, self.dmg_lv_3, self.dmg_lv_4, self.dmg_lv_5,
            self.dmg_lv_6, self.dmg_lv_7, self.dmg_lv_8, self.dmg_lv_9, self.dmg_lv_10, self.dmg_lv_11,
        ]
        .get(level as usize)
        .copied()
        .unwrap_or(0)
    }

    /// Its def<Attr>StaminaDmgRate for a staminaPhysicsAttribute (`stamina_rate_field`).
    pub fn def_stamina_rate(&self, attribute: i64) -> f32 {
        match attribute {
            1 => self.def_slash_stamina_dmg_rate,
            2 => self.def_light_hit_stamina_dmg_rate,
            3 => self.def_thrust_stamina_dmg_rate,
            4 => self.def_neutral_stamina_dmg_rate,
            5 => self.def_ninsatu_stamina_dmg_rate,
            6 => self.def_heavy_hit_stamina_dmg_rate,
            7 => self.def_anti_ground_stamina_dmg_rate,
            8 => self.def_anti_air_stamina_dmg_rate,
            9 => self.def_light_shoot_stamina_dmg_rate,
            10 => self.def_attri_a_stamina_dmg_rate,
            11 => self.def_attri_b_stamina_dmg_rate,
            12 => self.def_attri_c_stamina_dmg_rate,
            _ => 1.0,
        }
    }
}

fn one() -> f32 {
    1.0
}

/// SpEffect stateInfo 158: "just guard" (deflect) for both player (105010) and NPCs (200220).
pub const STATE_INFO_JUST_GUARD: i64 = 158;

fn minus_one() -> i64 {
    -1
}

fn one_i64() -> i64 {
    1
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Attack {
    #[serde(default)]
    #[allow(dead_code)] // kept for debugging / future use
    pub behavior_name: String,
    /// An enemy bullet's hit (Bullet sfxId_Hit) and repel (sfxId_Flick) effects with where it met
    /// the defender and its flight: combat.rs plays one by the outcome (enemy_bullet.rs).
    #[serde(skip)]
    pub bullet_fx: Option<(i64, i64, bevy::math::Vec3, bevy::math::Vec3)>,
    /// spEffectId0-4: SpEffects the hit puts on the target (Sabimaru 7500100 -> 9004 poison build-up).
    #[serde(default = "minus_one", rename = "spEffectId0")]
    pub sp_effect_id0: i64,
    /// Its AtkParam row (AtkParam_Pc for Wolf's, _Npc for an NPC's): atk_ai_sounds.
    #[serde(default = "minus_one", rename = "atkParamId")]
    pub atk_param_id: i64,
    #[serde(default = "minus_one", rename = "spEffectId1")]
    pub sp_effect_id1: i64,
    #[serde(default = "minus_one", rename = "spEffectId2")]
    pub sp_effect_id2: i64,
    #[serde(default = "minus_one", rename = "spEffectId3")]
    pub sp_effect_id3: i64,
    #[serde(default = "minus_one", rename = "spEffectId4")]
    pub sp_effect_id4: i64,
    /// opposeTarget: 1 = hits the other side. 0 rows are for the attacker's own side and objects
    /// only (friendlyTarget 1: the Blazing Bull's 13700800 "for destroying objects").
    #[serde(default = "one_i64")]
    pub oppose_target: i64,
    /// Grabs (throwFlag 1): which of the attacker's ThrowParam grab rows (Combat::enemy_grab).
    #[serde(default)]
    pub throw_type_id: i64,
    #[serde(default)]
    pub atk_phys: f32,
    /// NPC elemental damage, added to atkPhys (Shichimen's spirit balls 10800500: atkMag 16).
    #[serde(default)]
    pub atk_mag: f32,
    #[serde(default)]
    pub atk_fire: f32,
    #[serde(default)]
    pub atk_thun: f32,
    #[serde(default)]
    pub atk_dark: f32,
    #[serde(default)]
    pub atk_stam: f32,
    #[serde(rename = "hit0_Radius", default)]
    pub hit0_radius: f32,
    #[serde(rename = "hit0_DmyPoly1", default)]
    pub hit0_dmy1: i64,
    #[serde(rename = "hit0_DmyPoly2", default)]
    pub hit0_dmy2: i64,
    /// Every hit shape (AtkParam hit0..hit15): (DmyPoly1, DmyPoly2 or -1 for a sphere, radius,
    /// hitType, priority). Empty in older exports: `shapes` falls back to hit0.
    #[serde(default)]
    pub hits: Vec<(i64, i64, f32, i64, i64)>,
    #[serde(default)]
    pub direct_atk_stam_damage: f32,
    #[serde(default)]
    /// AtkParam staminaPhysicsAttribute (1 slash, 2 strike / lightHit, 3 thrust, 4 neutral, 6 heavyHit):
    /// picks the defender's def<Attr>StaminaDmgRate (latent skill Flowing Water: slash 0.8, others 0.75).
    #[serde(rename = "staminaPhysicsAttribute")]
    pub stamina_physics_attribute: i64,
    /// AtkParam atkAttribute (same enum): picks the defender's <attr>DamageCutRate (`damage_cut_field`).
    #[serde(rename = "atkAttribute", default)]
    pub atk_attribute: i64,
    pub repel_lost_stam_damage: f32,
    #[serde(rename = "repelLostStamDamage_Attacker", default)]
    pub repel_lost_stam_damage_attacker: f32,
    #[serde(rename = "repelVictoryStamDamage_Attacker", default)]
    pub repel_victory_stam_damage_attacker: f32,
    #[serde(default)]
    #[allow(dead_code)] // kept for debugging / future use
    pub is_disable_parry: i64,
    #[serde(default)]
    pub atk_phys_correction: f32,
    #[serde(default)]
    pub hit_stop_time: f32,
    #[serde(rename = "hitStopTime_Defencer", default)]
    pub hit_stop_time_defencer: f32,
    #[serde(default)]
    pub dmg_level: i64,
    /// AtkParam atkBehaviorId, the HKS env(334) "Behavior identification value": 17 BLINDING
    /// (the Rappa's powder 13600170, the Taro troop's mud 11804500, the Yasha apes' sand
    /// 12500100 ...), 18 SXL_BLAST, 19 STORM_BACK_JUMP (c5200's).
    #[serde(default)]
    pub atk_behavior_id: i64,
    #[serde(rename = "dmgLevel_vsPlayer", default)]
    pub dmg_level_vs_player: i64,
    /// Defender's guard reaction direction: 1 = L, 2 = R (DEFLECT_DIR_*).
    #[serde(default)]
    pub deflect_action: i64,
    #[serde(default)]
    pub just_deflect_action: i64,
    /// Attacker's reaction when blocked / deflected: 1 = bound R, 2 = bound L, 11-14 = additive (BOUND_*).
    #[serde(default)]
    pub deflected_action: i64,
    #[serde(default)]
    pub just_deflected_action: i64,
    /// AtkParam guardAtkRate: an NPC attack's repel power against a guard (guard.rs).
    #[serde(rename = "guardAtkRate", default)]
    pub guard_atk_rate: i64,
    /// AtkParam guardAtkRateCorrection [%]: a Wolf weapon attack's repel power, on the weapon's
    /// attackBaseRepel (guard.rs).
    #[serde(rename = "guardAtkRateCorrection", default)]
    pub guard_atk_rate_correction: f32,
    /// Perilous attacks: cannot be blocked (1) / cannot be deflected either (1).
    #[serde(rename = "disableGuard_vsGuardAttribute0", default)]
    pub disable_guard: i64,
    #[serde(rename = "disableJustGuard_vsGuardAttribute0", default)]
    pub disable_just_guard: i64,
    /// 0 slash, 1 strike, 2 thrust.
    #[serde(default)]
    pub atk_type: i64,
    /// spAttribute, the hit's element (HKS env 285 DAMAGE_ELEMENT_*): 6 lightning (the Divine
    /// Dragon's, Wolf's reversal bolt Bullet 500184), 10 "blue" lightning (Genichiro 71100361,
    /// Isshin 54000750).
    #[serde(default)]
    pub sp_attribute: i64,
    /// decalId1 / 2: the DecalParam stain the hit leaves (-1 / 0 none; the oil pot's 50010 /
    /// 50011). gore.rs PlaceStain.
    #[serde(default = "minus_one", rename = "decalId1")]
    pub decal_id1: i64,
    #[serde(default = "minus_one", rename = "decalId2")]
    pub decal_id2: i64,
    /// decalBaseId1 / 2: the hit-blood stain bases (0 / 100 on most rows; gore::hit_stain_id).
    #[serde(default = "minus_one", rename = "decalBaseId1")]
    pub decal_base_id1: i64,
    #[serde(default = "minus_one", rename = "decalBaseId2")]
    pub decal_base_id2: i64,
    /// Hit sound: HitEffectSeParam column group (0 Iron, 3 Body, ...) and power (0 S, 1 L, 2 LL).
    #[serde(rename = "atkMaterial_forSe", default)]
    pub atk_material_se: i64,
    #[serde(rename = "atkPow_forSe", default)]
    pub atk_pow_se: i64,
    /// Hit effects (vfx::hit_sfx): HitEffectSfxConcept(JustGuard)Param column group (0 Iron, 3 Body,
    /// ...; -1 = not exported), size (0 S .. 4 LLL) and the rows for its guard / deflect effects
    /// (the General's sword: Iron, 2 = L, 100 / 139).
    #[serde(rename = "atkMaterial_forSfx", default = "minus_one")]
    pub atk_material_sfx: i64,
    #[serde(rename = "atkPow_forSfx", default)]
    pub atk_pow_sfx: i64,
    /// The hit effect's turn row (HitEffectSfxAngleParam): 1 Upper, 2 Bottom, 3 Left, 4 Right.
    #[serde(rename = "atkDir_forSfx", default)]
    pub atk_dir_sfx: i64,
    #[serde(rename = "defSfxMaterial1", default = "minus_one")]
    pub def_sfx_material1: i64,
    #[serde(rename = "defSfxMaterial2", default = "minus_one")]
    pub def_sfx_material2: i64,
    /// Rows for this attack's guard / deflect sounds (HitEffectSe(JustGuard)Param), e.g. 100 / 139.
    #[serde(rename = "defSeMaterial1", default = "minus_one")]
    pub def_se_material1: i64,
    #[serde(rename = "defSeMaterial2", default = "minus_one")]
    pub def_se_material2: i64,
    /// Posture damage the attacker takes when the attack is mikiri-countered.
    #[serde(default)]
    pub stamina_damage_attack_hit_parry: f32,
    /// 1 = grab attempt (contact starts the throw), 2 = the throw's damage.
    #[serde(default)]
    pub throw_flag: i64,
    /// AtkParam trackType (+0x193, "attack trajectory"): the exe's i-frame test FUN_140b68a50
    /// (140b6.c:4234-4241) skips the hit when the defender's TAE 954 IFrameType bit of this value
    /// is set: 1 sweeps, 2 general, 6 thrusts, 7 grabs (round 31).
    #[serde(default)]
    pub track_type: i64,
    /// Defender knockback [m] when hit / guarded / deflected.
    #[serde(rename = "knockbackDist_DirectHit", default)]
    pub knockback_hit: f32,
    #[serde(rename = "knockbackDist_Guard", default)]
    pub knockback_guard: f32,
    #[serde(rename = "knockbackDist_JustGuard", default)]
    pub knockback_just_guard: f32,
    /// Element corrections [%] applied to the weapon's attackBase* (combat arts use dark).
    #[serde(default)]
    pub atk_mag_correction: f32,
    #[serde(default)]
    pub atk_fire_correction: f32,
    #[serde(default)]
    pub atk_thun_correction: f32,
    #[serde(default)]
    pub atk_dark_correction: f32,
}

impl Attack {
    /// The row's hit shapes (DmyPoly1, DmyPoly2 or -1 = a sphere, radius): all of hit0..hit15
    /// it uses, else hit0 (exports before the shapes were carried).
    pub fn shapes(&self) -> Vec<(i64, i64, f32)> {
        if self.hits.is_empty() {
            return vec![(self.hit0_dmy1, self.hit0_dmy2, self.hit0_radius)];
        }
        self.hits.iter().map(|h| (h.0, h.1, h.2)).collect()
    }

    /// An NPC hit's damage before its NpcParam atk rate: atkPhys + atkMag + atkFire + atkThun +
    /// atkDark. gap: Wolf's per-element cut rates are not applied (all 1.0 assumed).
    pub fn npc_damage(&self) -> f32 {
        self.atk_phys + self.atk_mag + self.atk_fire + self.atk_thun + self.atk_dark
    }
    /// spEffectId0-4 (the hit's SpEffects on the target: build-ups, reactions).
    pub fn sp_effects(&self) -> [i64; 5] {
        [self.sp_effect_id0, self.sp_effect_id1, self.sp_effect_id2, self.sp_effect_id3, self.sp_effect_id4]
    }
    /// Damage level as the HKS reads it (dmgLevel_vsPlayer overrides against the player).
    pub fn level(&self, vs_player: bool) -> i64 {
        if vs_player && self.dmg_level_vs_player > 0 { self.dmg_level_vs_player } else { self.dmg_level }
    }
}

/// A BehaviorParam_PC row of a prosthetic: wepCost 1 (u8 at +0x0A, Paramdex) = firing it costs the
/// tool level's Spirit Emblems (EquipParamWeapon resourceItemA). Throws, the Firecracker's parent
/// bullet, the Axe's swing and the "form consumption dummy" judges 999 (Flame Vent hold, Mist
/// Raven, Umbrella, Finger Whistle) carry it; follow-ups (the Axe's second hit, the Firecracker's
/// beast twin 110) don't. refType 2 = refId is an SpEffect it puts on Wolf (TAE 940).
#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PcBehavior {
    #[serde(default)]
    pub wep_cost: i64,
    #[serde(default)]
    pub ref_type: i64,
    #[serde(default)]
    pub ref_id: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CharData {
    #[serde(default)]
    pub states: HashMap<String, String>,
    #[serde(default)]
    pub anims: HashMap<String, Anim>,
    #[serde(default)]
    pub sp_effects: HashMap<String, SpEffect>,
    #[serde(default)]
    pub attacks: HashMap<String, Attack>,
    /// Prosthetic bullets by TAE BulletBehavior judge (Bullet param + its AtkParam): "v<variation>:
    /// <judge>" per tool level (BehaviorParam_PC 100000000 + variation * 1000 + judge), the bare
    /// judge for the Shuriken LV1.
    #[serde(default)]
    pub bullets: HashMap<String, BulletSpec>,
    /// Every bullet by Bullet id, with the child bullets they spawn (HitBulletID /
    /// intervalCreateBulletId).
    #[serde(default, rename = "bulletRows")]
    pub bullet_rows: HashMap<String, BulletSpec>,
    /// The prosthetic tools, every level (EquipParamWeapon 70000-79200).
    #[serde(default)]
    pub prosthetics: Vec<Prosthetic>,
    /// Ragdoll body capsules (chrbnd physics HKX), Havok model space, bind pose.
    #[serde(default)]
    pub hurtboxes: Vec<Hurtbox>,
    /// Wolf's prosthetic behaviours by BehaviorParam_PC row (107000000-107999999).
    #[serde(default)]
    pub behaviors: HashMap<String, PcBehavior>,
    /// NPC TAE 700 twist modifiers (shared graph c9997 bound to this character's properties).
    #[serde(default)]
    pub twists: HashMap<String, Twist>,
    /// Anims with TAE data but no clip in the archives -> the sibling whose clip,
    /// duration and root motion they borrow (see `alias_missing_clips`).
    #[serde(skip)]
    pub clip_alias: HashMap<String, String>,
    /// behaviorVariationId of the equipped combat art (config player.combat_art): art anims
    /// resolve their judges as "v<variation>:<judge>" first (upgraded arts, e.g. 7200 -> 5021).
    #[serde(skip)]
    pub art_variation: Option<i64>,
    /// behaviorVariationId of the equipped prosthetic tool: tool anims (a070..a079) resolve their
    /// melee judges as "v<variation>:<judge>" (the Axe, Sabimaru, Spear).
    #[serde(skip)]
    pub tool_variation: Option<i64>,
    /// StateInfos of the equipment's resident SpEffects (EquipParamWeapon residentSpEffectId*: the
    /// tool level, e.g. Flame Vent LV4 127230 -> 916, plus "no enchantment" 914): StateInfo-gated
    /// TAE events with one of these fire (sound::active_state_infos adds the TAE SpEffects).
    /// For the enemy: its NpcParam resident SpEffects' StateInfos (the Blazing Bull's 3137040
    /// "for burning attack judgment" -> 950 turns on its AttackBehaviors gated 950, not the 951 set).
    #[serde(skip)]
    pub resident_gates: Vec<i64>,
}

impl CharData {
    /// Some anims ship only as TAE entries: left/right mirrors and chain steps whose
    /// clips the behavior graph reuses (e.g. a000_003101 deflect -> 3100 guard,
    /// GuardBreakLeft 8551 -> 8550, StandToDeflectGuard4 203007 -> 203006). Borrow the
    /// nearest sibling's clip (id +-1..3, then the same reaction 100 ids lower).
    fn alias_missing_clips(&mut self) {
        let has_clip = |a: &Anim| a.duration.is_some_and(|d| d > 0.0);
        let mut aliases = Vec::new();
        for (key, anim) in &self.anims {
            if has_clip(anim) {
                continue;
            }
            let Some((group, id)) = key.split_once('_').and_then(|(g, i)| Some((g.to_string(), i.parse::<i64>().ok()?))) else {
                continue;
            };
            // Prosthetic tools: c0000_a07x.anibnd ships Wolf's shared moves (the SubWeaponExpand
            // unfolds 412xxx, ...) only as a070 clips; the other tools' a071..a079 TAEs of the same
            // id play that body motion with their own events.
            let tool_base = group.get(1..).and_then(|g| g.parse::<i64>().ok()).filter(|g| (71..=79).contains(g)).map(|_| format!("a070_{id:06}"));
            if let Some(src) = tool_base.filter(|k| self.anims.get(k).is_some_and(has_clip)) {
                aliases.push((key.clone(), src));
                continue;
            }
            // Explicit: the neutral step (no stick) is Sekiro's backstep, not the forward one.
            let preferred: &[i64] = if key == "a000_213300" { &[2] } else { &[] };
            let offsets = [-1i64, 1, -2, 2, -3, 3, -100, -101, -99];
            let found = preferred
                .iter()
                .chain(offsets.iter())
                .map(|o| format!("{group}_{:06}", id + o))
                .find(|k| self.anims.get(k).is_some_and(has_clip));
            if let Some(src) = found {
                aliases.push((key.clone(), src));
            }
        }
        for (key, src) in aliases {
            let (duration, root_rate, root) = {
                let s = &self.anims[&src];
                (s.duration, s.root_rate, s.root.clone())
            };
            let a = self.anims.get_mut(&key).unwrap();
            a.duration = duration;
            a.root_rate = root_rate;
            a.root = root;
            self.clip_alias.insert(key, src);
        }
    }

    /// The clip to show for an anim key (itself, or its alias).
    pub fn clip_key<'a>(&'a self, key: &'a str) -> &'a str {
        self.clip_alias.get(key).map_or(key, |s| s.as_str())
    }
}

/// A prosthetic tool level: EquipParamWeapon row, its anim group a0<group> (wepmotionCategory),
/// behaviorVariationId (judges "v<variation>:<judge>"), Spirit Emblem cost (resourceItemA),
/// resident SpEffect and menu icon.
#[derive(Deserialize, Clone, Debug)]
pub struct Prosthetic {
    pub id: i64,
    pub group: i64,
    pub variation: i64,
    #[serde(default)]
    pub emblems: u32,
    #[serde(default)]
    pub resident: i64,
    /// EquipParamWeapon iconId: the HUD's MENU_ItemIcon_<icon> (hud.rs).
    #[serde(default)]
    pub icon: i64,
}

/// A Bullet param row (projectile) and its AtkParam.

impl Attack {
    /// This attack from a bullet `seconds` past its range: hp damage, stamina damage and
    /// knockback each lose their damp percent per second, linearly, to none. Ported from
    /// AKJama/sekiro-rs bullets.rs `decayed`. gap: the exe's curve is not traced (linear assumed).
    pub fn decayed(&self, damp: [f32; 3], seconds: f32) -> Attack {
        let mut a = self.clone();
        if seconds <= 0.0 {
            return a;
        }
        let scale = |pct: f32| (1.0 - pct / 100.0 * seconds).max(0.0);
        let (hp, stam, kb) = (scale(damp[0]), scale(damp[1]), scale(damp[2]));
        for v in [&mut a.atk_phys, &mut a.atk_mag, &mut a.atk_fire, &mut a.atk_thun, &mut a.atk_dark] {
            *v *= hp;
        }
        for v in [&mut a.atk_stam, &mut a.direct_atk_stam_damage, &mut a.repel_lost_stam_damage, &mut a.stamina_damage_attack_hit_parry] {
            *v *= stam;
        }
        for v in [&mut a.knockback_hit, &mut a.knockback_guard, &mut a.knockback_just_guard] {
            *v *= kb;
        }
        a
    }
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BulletSpec {
    #[serde(default)]
    pub bullet_id: i64,
    #[serde(default)]
    pub init_vellocity: f32,
    /// numShoot bullets in a fan: shootAngle (deg from the facing) + i * shootAngleInterval;
    /// shootAngleXZ tilts them (-90 = straight down).
    #[serde(default)]
    pub num_shoot: i32,
    #[serde(default)]
    pub shoot_angle: f32,
    #[serde(default)]
    pub shoot_angle_interval: f32,
    #[serde(default, rename = "shootAngleXZ")]
    pub shoot_angle_xz: f32,
    /// Child bullets: HitBulletID when it ends (hits or runs out), intervalCreateBulletId every
    /// intervalCreateTimeMin..Max s after intervalCreateWaitTime while it flies.
    #[serde(default = "minus_one", rename = "HitBulletID")]
    pub hit_bullet_id: i64,
    #[serde(default = "minus_one")]
    pub interval_create_bullet_id: i64,
    #[serde(default)]
    pub interval_create_time_min: f32,
    #[serde(default)]
    pub interval_create_wait_time: f32,
    /// Hits once per character for the whole chain (isUseSharedHitList); passes through
    /// characters (isPenetrate).
    #[serde(default)]
    pub is_use_shared_hit_list: i32,
    #[serde(default)]
    pub is_penetrate: i32,
    /// SpEffects the bullet puts on what it hits (e.g. the firecracker burst 710003: 230110 "vs.
    /// non-special character", 107100 its 30 s cool time).
    #[serde(default = "minus_one")]
    pub sp_effect_id0: i64,
    #[serde(default = "minus_one")]
    pub sp_effect_id1: i64,
    #[serde(default = "minus_one")]
    pub sp_effect_id2: i64,
    #[serde(default = "minus_one")]
    pub sp_effect_id3: i64,
    #[serde(default = "minus_one")]
    pub sp_effect_id4: i64,
    /// The game's effects (FXR): on the bullet while it flies, where it hits, where it is
    /// deflected (the Shuriken 700000: 300084 / 300085 / 300086; the firecracker burst 710003:
    /// 300071); isInheritSfxToChild: its children keep the flying one.
    #[serde(default = "minus_one", rename = "sfxId_Bullet")]
    pub sfx_id_bullet: i64,
    #[serde(default = "minus_one", rename = "sfxId_Hit")]
    pub sfx_id_hit: i64,
    #[serde(default = "minus_one", rename = "sfxId_Flick")]
    pub sfx_id_flick: i64,
    #[serde(default)]
    pub is_inherit_sfx_to_child: i32,
    /// The speed stays within minVellocity..maxVellocity (enemy_bullet::flight).
    #[serde(default)]
    pub max_vellocity: f32,
    #[serde(default)]
    pub min_vellocity: f32,
    /// Acceleration (along the flight) / gravity within `dist` metres and beyond.
    #[serde(default)]
    pub accel_in_range: f32,
    #[serde(default)]
    pub gravity_in_range: f32,
    #[serde(default)]
    pub accel_out_range: f32,
    #[serde(default)]
    pub gravity_out_range: f32,
    /// Homing (enemy_bullet::steer): homingAngle deg/s (< 0 none) once past homingBeginDist m,
    /// while the target is hormingStopRange m away or more; its point is raised
    /// targetYOffsetRange m.
    #[serde(default)]
    pub homing_angle: f32,
    #[serde(default)]
    pub homing_begin_dist: f32,
    #[serde(default)]
    pub horming_stop_range: f32,
    #[serde(default)]
    pub target_y_offset_range: f32,
    /// Flies through the map (else the collision stops it).
    #[serde(default)]
    pub is_penetrate_map: i32,
    #[serde(default)]
    pub dist: f32,
    /// damageDamp / staminaDamp / knockbackDamp: percent lost per second past `dist`
    /// (Paramdex: "decreases in 1 second after the attenuation distance"; 100 on the arrows).
    #[serde(default)]
    pub damage_damp: f32,
    #[serde(default)]
    pub stamina_damp: f32,
    #[serde(default)]
    pub knockback_damp: f32,
    #[serde(default)]
    pub life: f32,
    #[serde(default)]
    pub hit_radius: f32,
    /// hitRadiusMax / spreadTime: the radius grows from hitRadius to this over spreadTime s.
    #[serde(default)]
    pub hit_radius_max: f32,
    /// spEffectIDForShooter: put on the one who fires it (the Flame Vent follow-up 720040 ->
    /// 106020 "Flame enchantment", 7.5 s, stateInfo 62; prosthetic.rs).
    #[serde(default, rename = "spEffectIDForShooter")]
    pub sp_effect_id_for_shooter: i64,
    #[serde(default)]
    pub spread_time: f32,
    /// dmgHitRecordLifeTime: how long a penetrating bullet's hit on a character is remembered
    /// (it may hit again after it).
    #[serde(default)]
    pub dmg_hit_record_life_time: f32,
    #[serde(default)]
    pub lock_shoot_limit_ang: f32,
    /// When HitBulletID comes (BULLET_LAUNCH_CONDITION_TYPE, enemy_bullet.rs launches): 0
    /// always, 1 / 2 water, 3 missed, 4 hit an enemy, 254 life ran out (the Divine Dragon's
    /// updraft 52000661 renewing itself), 255 hit the ground or an enemy.
    #[serde(default)]
    pub launch_condition_type: i64,
    /// Paramdex BULLET_FOLLOW_TYPE: 3 "follow target" (the Divine Dragon's warning 52000980:
    /// AtkParam 8 "[System] dummy bullet", SpEffect 211000 "dangerous enemy attack VFX"), 1 / 2
    /// the shooter / its feet (no enemy row).
    #[serde(default, rename = "FollowType")]
    pub follow_type: i64,
    #[serde(default)]
    pub attack: Option<Attack>,
}

/// One hurtbox capsule attached to an animation bone.
#[derive(Deserialize, Clone)]
pub struct Hurtbox {
    pub bone: String,
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub r: f32,
    /// Its NPC part group (chrbnd hkxpwv; HKS env(1120) GetPartGroup), 0 = none.
    #[serde(default)]
    pub part: u8,
}

#[derive(Deserialize)]
struct File {
    player: CharData,
    enemy: CharData,
    #[serde(default)]
    enemies: HashMap<String, CharData>,
    params: Value,
    #[serde(default)]
    rumble: HashMap<String, Rumble>,
    #[serde(default)]
    twists: HashMap<String, Twist>,
    #[serde(default)]
    names: Names,
}

/// Official English names (msg/engus item 武器名): weapon id -> name.
#[derive(Deserialize, Default, Clone)]
pub struct Names {
    #[serde(default)]
    pub weapon: HashMap<String, String>,
}

/// CustomLookAtTwistModifier from c0000.hkx (TAE 700): limits in degrees and the bone chains.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Twist {
    pub up: f32,
    pub down: f32,
    pub right: f32,
    pub left: f32,
    pub chains: Vec<TwistChain>,
}

/// CustomLookAtTwistModifier::TwistParam: the bones from `start` (exclusive) down to `end` share
/// `rate` of the twist; the gains are per-frame (1/30 s) blend factors.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct TwistChain {
    pub start: i16,
    pub end: i16,
    pub rate: f32,
    /// Not applied yet (gain when the target changes).
    #[serde(rename = "newTargetGain")]
    #[allow(dead_code)]
    pub new_target_gain: f32,
    #[serde(rename = "onGain")]
    pub on_gain: f32,
    #[serde(rename = "offGain")]
    pub off_gain: f32,
}

#[derive(Resource)]
pub struct Combat {
    pub player: CharData,
    /// The Construct's friendly Sculptor, kept outside encounter kinds so boss swaps cannot
    /// change his model or animations. He has no Enemy component or combat target.
    pub sculptor: Option<EnemyKind>,
    /// The enemy kinds in play (a chr with one NpcParam row each): kind 0 is config enemy.chr,
    /// then config enemy.group's and the ones the boss map events bring in. An enemy actor's
    /// `Actor::kind` indexes it.
    pub kinds: Vec<EnemyKind>,
    pub params: Value,
    /// Camera shakes by RumbleCam id (other/default.rumblebnd).
    pub rumble: HashMap<String, Rumble>,
    /// Wolf's TAE 700 twist modifiers by name ("0_TwistUD", "100_Attack", ...).
    pub twists: HashMap<String, Twist>,
    /// Official names (combat arts, prosthetic tools, skills).
    pub names: Names,
}

/// One enemy kind: which chr / rows (`foe`) and its character data (anims, attacks, ...).
pub struct EnemyKind {
    pub foe: Foe,
    pub data: CharData,
}

/// The NPC's kind index (`Combat::sculptor`), outside the replaceable enemy kinds: encounter
/// changes must not turn it into a boss.
pub const NPC_KIND: usize = usize::MAX;

impl Combat {
    /// Enemy kind `i` (kind 0 when out of range).
    pub fn kind(&self, i: usize) -> &EnemyKind {
        if i == NPC_KIND {
            if let Some(sculptor) = &self.sculptor {
                return sculptor;
            }
        }
        self.kinds.get(i).unwrap_or(&self.kinds[0])
    }

    /// An actor's character data: Wolf's, or its enemy kind's.
    pub fn data_of(&self, a: &crate::actor::Actor) -> &CharData {
        match a.side {
            crate::actor::Side::Player => &self.player,
            crate::actor::Side::Enemy => &self.kind(a.kind).data,
        }
    }

    /// An enemy actor's chr and rows (kind 0's for the player).
    pub fn foe_of(&self, a: &crate::actor::Actor) -> &Foe {
        &self.kind(a.kind).foe
    }

    /// An enemy actor's NpcParam row.
    pub fn npc(&self, a: &crate::actor::Actor) -> &Value {
        self.param("NpcParam", self.foe_of(a).npc_row)
    }

    /// The first enemy kind's data and rows (single-enemy tools and tests).
    pub fn enemy0(&self) -> &CharData {
        &self.kinds[0].data
    }

    pub fn foe0(&self) -> &Foe {
        &self.kinds[0].foe
    }

    /// The kind index of (chr, NpcParam row), if loaded.
    pub fn kind_index(&self, chr: &str, npc_row: Option<i64>) -> Option<usize> {
        self.kinds.iter().position(|k| k.foe.is(chr, npc_row))
    }

    /// The kind of `chr` with NpcParam `row`, loaded now when it is not yet (the open world's
    /// enemies come and go with Wolf, src/enemy/world.rs); AnimLib::sync_kinds must follow a new
    /// one. None when the chr has no exported data (extracted/enemies/<chr>.json).
    pub fn ensure_kind(&mut self, chr: &str, row: i64) -> Option<usize> {
        if let Some(k) = self.kind_index(chr, Some(row)) {
            return Some(k);
        }
        let kind = self.with_params_file(|f| load_kind(f, chr, Some(row)));
        (kind.foe.chr == chr).then(|| {
            self.kinds.push(kind);
            self.kinds.len() - 1
        })
    }

    /// The official English name of a weapon row (art, tool, skill entry), else its id.
    pub fn weapon_name(&self, id: i64) -> String {
        self.names.weapon.get(&id.to_string()).cloned().unwrap_or_else(|| id.to_string())
    }
}

/// The enemy in play: chr id plus the rows its data lives in.
#[derive(Clone, Debug)]
pub struct Foe {
    pub chr: String,
    /// Whose animations / behaviour the chr uses (NpcParam normalChangeAnimChrId: c1021 the
    /// spear General plays c1020's, c5430 Isshin Ashina c5400's); `chr` itself when unset.
    pub anim_chr: String,
    /// NpcParam row (stats, guard, knockback, cut rates).
    pub npc_row: i64,
    /// NpcThinkParam row: its battleGoalID / logicId pick the Lua battle and logic scripts.
    pub think_id: i64,
    /// The chr number (c1020 -> 1020): ThrowParam DefChrId / AtkChrId.
    pub chr_num: i64,
    /// Passive-mode parry: None = the enemy's own Goal.Parry (102000_battle.lua), Some = the
    /// shared Common_Parry(guardMult, stepProb, stepType, rushAnim) its battle script calls.
    pub common_parry: Option<(i32, i32, i32, String)>,
    /// The NpcParam row the config asked for when it is not one of this chr's (`npc_row` then
    /// holds the main row): the line-up and the dedupe still find this kind by it (2026-10-12
    /// bug hunt: c1110 row 11103000 was dropped).
    pub asked_row: Option<i64>,
}

impl Foe {
    /// Whether this is the kind for config (chr, npc_row): any row of the chr when unset.
    pub fn is(&self, chr: &str, npc_row: Option<i64>) -> bool {
        self.chr == chr && npc_row.is_none_or(|r| self.npc_row == r || self.asked_row == Some(r))
    }

    pub fn new(chr: &str, anim_chr: &str, npc_row: i64, think_id: i64) -> Foe {
        let chr_num = chr.get(1..).and_then(|n| n.parse().ok()).unwrap_or(1020);
        Foe { chr: chr.to_string(), anim_chr: anim_chr.to_string(), npc_row, think_id, chr_num, common_parry: None, asked_row: None }
    }

    /// The two enemies combat_data.json carries (tests and the fallback when extracted/enemies is
    /// missing). NpcParam 10203010: the regular General (behaviorVariationId 10200) as fought live
    /// (rec_20261008_054259); c1010 101000_battle.lua Goal.Interrupt calls
    /// Common_Parry(ai, goal, 50, 25, 0, 3102).
    pub fn for_chr(chr: &str) -> Foe {
        if chr == "c1010" {
            Foe { common_parry: Some((50, 25, 0, "a000_003102".into())), ..Foe::new("c1010", "c1010", 10100000, 10100000) }
        } else {
            Foe::new("c1020", "c1020", 10203010, 10200000)
        }
    }

    /// ThrowParam "PC -> this enemy": 10000000 + chr number * 1000 + suffix (DefChrId = the chr
    /// number; c1020 11020000 = posture-break start, 0001 its body, 0010 deflect break, 0190
    /// mikiri). The enemy's grabs are 20000000 + chr number * 1000.
    pub fn throw_row(&self, suffix: i64) -> i64 {
        10_000_000 + self.chr_num * 1_000 + suffix
    }

    /// Common_Parry(ai, goal, guardMult, stepProb, stepType, rushAnim) in the enemy's battle
    /// script (the decompiled copy in extracted/ai_src), if it calls it.
    fn read_common_parry(battle_goal: i64) -> Option<(i32, i32, i32, String)> {
        let path = crate::paths::root().join(format!("extracted/ai_src/{battle_goal:06}_battle.lua"));
        let text = std::fs::read_to_string(path).ok()?;
        let args = text.split("Common_Parry(").nth(1)?.split(')').next()?;
        let v: Vec<i32> = args.split(',').skip(2).filter_map(|a| a.trim().parse().ok()).collect();
        (v.len() == 4).then(|| (v[0], v[1], v[2], format!("a000_{:06}", v[3])))
    }
}

/// extracted/enemies/<chr>.json (`sekiro-extract npcs`): one enemy's character data, its own
/// param rows and its roster entry (MSB placements: NpcParam / NpcThinkParam rows).
pub struct EnemyFile {
    pub data: CharData,
    pub params: Value,
    pub roster: Value,
}

impl EnemyFile {
    pub fn load(chr: &str) -> Option<EnemyFile> {
        let path = crate::paths::root().join(format!("extracted/enemies/{chr}.json"));
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let params = v.get_mut("params").map(Value::take).unwrap_or_default();
        let roster = v.get_mut("roster").map(Value::take).unwrap_or_default();
        match serde_json::from_value(v) {
            Ok(data) => Some(EnemyFile { data, params, roster }),
            Err(e) => {
                warn!("extracted/enemies/{chr}.json: {e}");
                None
            }
        }
    }

    /// The fight setup: the configured NpcParam row if it is one of this chr's, else the most
    /// placed one; its think row from the placement that uses it.
    pub fn foe(&self, chr: &str, npc_row: Option<i64>) -> Foe {
        let main_npc = self.roster["npc"].as_i64().unwrap_or(0);
        let main_think = self.roster["think"].as_i64().unwrap_or(0);
        let anim_chr = self.roster["animChr"].as_str().unwrap_or(chr).to_string();
        let row = match npc_row {
            Some(r) if self.params["NpcParam"].get(r.to_string()).is_some() => r,
            Some(r) => {
                warn!("enemy.npc_row {r} is not a {chr} row; using {main_npc}");
                main_npc
            }
            None => main_npc,
        };
        let placements = self.roster["placements"].as_array().cloned().unwrap_or_default();
        let think = placements
            .iter()
            .find(|p| p["npc"].as_i64() == Some(row) && p["think"].as_i64().is_some_and(|t| t > 0))
            .and_then(|p| p["think"].as_i64())
            .unwrap_or(main_think);
        let mut foe = Foe::new(chr, &anim_chr, row, think);
        foe.asked_row = npc_row.filter(|&r| r != row);
        let goal = self.params["NpcThinkParam"][think.to_string()]["battleGoalID"].as_i64().unwrap_or(0);
        foe.common_parry = Foe::read_common_parry(goal);
        foe
    }
}

/// BehaviorParam `category` of an NPC hit's behaviour row (200000000 + behaviorVariationId *
/// 1000 + judge), from the full param table (the per-chr export carries no NPC behaviour rows). The exe's damage type for a guarded NPC attack comes from
/// it (FUN_140b698d0, 140b6.c:5487-5534: category 6 -> GUARDED_BOUND_ENEMY1 1008 / NO_BOUND 1019,
/// 7 -> ENEMY2 1009 / 1020, 8 -> ENEMY3 1010 / 1021), which c9997 ExecAttackJustBound /
/// ExecAttackBound turn into the AttackBoundEnemy1-3 / AttackNoBoundEnemy1-3 clips. 0 when
/// the row is missing.
pub fn behavior_category(row: i64) -> i64 {
    crate::items::param("BehaviorParam", row)["category"].as_i64().unwrap_or(0)
}

/// Does the behaviour graph keep the from generator's root motion through the blend into
/// `state`? c0000.hkx's transitions (sekiro-extract `sms`, tallied per target state into
/// extracted/blend_src.json): TaeBlend / _Continue / _Sync (Havok flags without bit 1) blend
/// the from and to world-from-model by the blend weight; TaeBlend_NoSrcMotion* (flags bit 1,
/// FLAG_IGNORE_FROM_WORLD_FROM_MODEL) take the to generator's at once. A state reached both
/// ways takes the majority (Idle: 2 drop / 1 keep); a state the table lacks (CMSG-selected
/// locomotion, enemy labels) keeps it, the common case (934 TaeBlend vs 337 NoSrcMotion).
pub fn blend_keeps_src(state: &str, player: bool) -> bool {
    static KEEP: std::sync::OnceLock<HashMap<String, bool>> = std::sync::OnceLock::new();
    KEEP.get_or_init(|| {
        let path = crate::paths::extracted().join("blend_src.json");
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    })
    .get(state)
    .copied()
    // Sv1 names the directional children of a graph state with a suffix (GroundStep_F,
    // SprintStartFromStep_F); blend_src.json holds the graph state (GroundStep,
    // SprintStartFromStep). Missing that, the step -> sprint start kept the braking step's motion
    // and dropped to 3 m/s; live (move_log 2026-10-15, 213301 -> 001151) it stays at 6.7-8.8.
    .or_else(|| {
        if !player {
            return None;
        }
        let mut s = state;
        while let Some(i) = s.rfind('_') {
            s = &s[..i];
            if let Some(&v) = KEEP.get().and_then(|m| m.get(s)) {
                return Some(v);
            }
        }
        // The run stops carry the direction without an underscore (StandRunStopF/B/L/R, graph
        // state StandRunStop). Missing that, the stop kept the run's motion through its blend
        // and slid 1 m; live (rec_20261016_*: W_StandRunStop events) the speed is down to
        // 0.5 m/s 3 ticks after the stick is released, 0.15-0.3 m in all (clip a000_000600: 0.3 m).
        let base = state.strip_suffix(['F', 'B', 'L', 'R'])?;
        KEEP.get().and_then(|m| m.get(base)).copied()
    })
    .unwrap_or(true)
}

/// Whether Wolf's clip loops in his behaviour graph (c0000.hkx hkbClipGenerator mode 1
/// MODE_LOOPING: 247 of 2566 generators, extracted/loop_clips_c0000.json from `sekiro-extract
/// clips`): a looping from generator keeps running past its end through a blend-in. gap: the
/// enemies' graphs (c9997 CustomManualSelectorGenerator) carry no such flag; theirs wrap by
/// nothing.
pub fn loops_c0000(key: &str) -> bool {
    static LOOPS: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    LOOPS
        .get_or_init(|| {
            let path = crate::paths::extracted().join("loop_clips_c0000.json");
            std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()).unwrap_or_default().into_iter().collect()
        })
        .contains(key)
}

/// The bound / no-bound clip number of an NPC attack guarded by Wolf (see [`behavior_category`]).
pub fn bound_clip_no(category: i64) -> i64 {
    match category {
        7 => 2,
        8 => 3,
        _ => 1,
    }
}

/// Makes the NpcParam row's behavior variation the default: "v<variation>:<judge>" attacks and
/// bullets (BehaviorParam 200000000 + variation * 1000 + judge) replace the bare judge keys.
pub fn select_variation(d: &mut CharData, variation: i64) {
    let prefix = format!("v{variation}:");
    let attacks: Vec<(String, Attack)> = d.attacks.iter().filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v.clone()))).collect();
    d.attacks.extend(attacks);
    let bullets: Vec<(String, BulletSpec)> = d.bullets.iter().filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), v.clone()))).collect();
    d.bullets.extend(bullets);
}

/// One camera shake: duration and per-frame [tx, ty, tz, qx, qy, qz, qw] (Havok space).
#[derive(Deserialize, Clone, Default)]
pub struct Rumble {
    pub duration: f32,
    pub frames: Vec<[f32; 7]>,
}

/// ChrPhysicsVelocityChangeParam row: new velocity = current * scale + change
/// (horizontal change rotated by horizontalVelocityAngle from the facing).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityChange {
    pub h_scale: f32,
    pub v_scale: f32,
    pub h_change: f32,
    pub v_change: f32,
    pub h_angle: f32,
    pub fall_type: u8,
    /// ChrPhysicsHomingParam row the launch homes with (0 none; 300000000 the wire's).
    pub homing_id: i64,
    /// homingTargetType: which target the homing aims at (0 none). gap: the enum's names are not
    /// in Paramdex; 1 (the jump arts' 10010) is taken as the lock-on target (player.rs `LeapHoming`).
    pub homing_target_type: u8,
}

/// Fall control per ChrPhysicsVelocityChangeParam.fallType. Not a param: a static
/// table in the exe (0x143b09ce0, exposed by its physics debug menu as
/// HorizontalAcceleration / verticalAcceleration / StickAcceleration / StickAccelerationMaxVelocity).
#[derive(Clone, Copy, Debug)]
pub struct FallType {
    pub h_accel: f32,
    pub v_accel: f32,
    pub stick_accel: f32,
    pub stick_max_vel: f32,
    pub controllable: bool,
}

/// Havok world gravity. The fall type's verticalAcceleration is extra on top of it: the live
/// game falls at 23.52 m/s^2 (= 9.8 + 13.72; 5 recorded jumps, docs/kb/movement.md).
pub const WORLD_GRAVITY: f32 = 9.8;

impl FallType {
    /// Total vertical acceleration while airborne (negative = down).
    pub fn gravity(&self) -> f32 {
        self.v_accel - WORLD_GRAVITY
    }
}

pub const FALL_TYPES: [FallType; 4] = [
    FallType { h_accel: -0.1, v_accel: -13.72, stick_accel: 10.0, stick_max_vel: 2.0, controllable: true }, // Normal
    FallType { h_accel: -0.1, v_accel: -13.72, stick_accel: 10.0, stick_max_vel: 2.0, controllable: false }, // Normal_Uncontrollable
    FallType { h_accel: -15.0, v_accel: -9.8, stick_accel: 9.0, stick_max_vel: 6.0, controllable: true }, // Wire
    FallType { h_accel: -0.1, v_accel: -13.72, stick_accel: 10.0, stick_max_vel: 2.0, controllable: true }, // Decelerating
];

/// One ThrowParam row (attacker / defender anims and reach).
#[derive(Clone, Debug)]
pub struct Throw {
    /// Attacker anim key, e.g. "a201_510000" (atkAnimOffset group).
    pub atk_anim: String,
    /// Defender behavior anim id (ThrowDef<id>, ThrowDefDeath<id + 1> when it kills).
    pub def_anim: i64,
    pub dist: f32,
    /// Throw absorb: the defender's root goes onto the attacker's dummy poly atkSorbDmyId, or the
    /// attacker onto the defender's defSorbDmyId (0 = the character root). player.rs follow_throw.
    pub atk_dmy: i16,
    pub def_dmy: i16,
    /// The attacker's anim id (atkAnimId: an enemy grab's ThrowAtk<id> state).
    pub atk_anim_id: i64,
    /// The defender's anim group (defAnimOffset: Wolf's a210 for the General, a223 for the zombies).
    pub def_group: i64,
    /// Where the reach is measured on the defender (judgeRangeBasePosDmyId2: the Divine Dragon's
    /// 230 by its head; -1 = its root). The facings: player.rs throw_facing_ok.
    pub judge_dmy: i16,
    /// adsrobModelPosInterpolationTime (s): after the absorb snaps the capsule onto the dummy,
    /// the model slides to the anim's position over this time (0 = no slide). anim.rs ModelLag.
    pub model_interp: f32,
}

impl Combat {
    pub fn throw(&self, id: i64) -> Option<Throw> {
        let r = self.params.get("ThrowParam")?.get(id.to_string())?;
        let i = |k: &str| r.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        Some(Throw {
            atk_anim: format!("a{:03}_{:06}", i("atkAnimOffset"), i("atkAnimId")),
            def_anim: i("defAnimId"),
            dist: r.get("Dist").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32,
            atk_dmy: i("atkSorbDmyId") as i16,
            def_dmy: i("defSorbDmyId") as i16,
            atk_anim_id: i("atkAnimId"),
            def_group: i("defAnimOffset"),
            judge_dmy: r.get("judgeRangeBasePosDmyId2").and_then(|v| v.as_i64()).unwrap_or(-1) as i16,
            model_interp: r.get("adsrobModelPosInterpolationTime").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        })
    }

    /// Wolf's finisher on a kneeling boss: its ThrowParam row with throwKind 250000 ("敵特殊状態A",
    /// DefChrId = the chr number; the lowest id when there are several), and that id.
    pub fn finisher(&self, foe: &Foe) -> Option<(i64, Throw)> {
        let rows = self.params.get("ThrowParam")?.as_object()?;
        let id = rows
            .iter()
            .filter(|(_, r)| r["DefChrId"].as_i64() == Some(foe.chr_num) && r["throwKind"].as_i64() == Some(250_000))
            .filter_map(|(k, _)| k.parse::<i64>().ok())
            .min()?;
        Some((id, self.throw(id)?))
    }

    /// The Grappling Hook Attack on this enemy (kb/grapple.md section 4: "崩しワイヤー", breaking
    /// wire): throwKind 180000 "[PC] Breaking wire 0" (Wolf a<offset>_511600 / its 13600), 180100
    /// "... 1" (511610 / 13610) and 180900 (Gyoubu's Todome 15080179, 511690 / 13690): the Ogre
    /// 15020170 / 71, Gyoubu 15080170 / 71 / 79. (throwTypeId 3023 / 3123 alone also takes the
    /// falls and kick-downs.)
    pub fn wire_throws(&self, foe: &Foe) -> Vec<(i64, Throw)> {
        self.throws_of_kinds(foe, &[180_000, 180_100, 180_900])
    }

    /// This enemy's ThrowParam rows (DefChrId = the chr number) of the given ThrowKindParam
    /// kinds, in id order. Wolf's deathblows by kind ("[PC] ..."): 90000 / 90010 wall-hug left
    /// (start / body: Wolf 500600 / 510600, its 12600), 100000 / 100010 right (500700 / 510700,
    /// 12700), 110000 / 110010 hanging (500800 / 510800, 12800; and 501000 / 511000, 13000),
    /// 70000 / 70100 / 70900 the plunge, 180000-180900 the wire (wire_throws). The common
    /// enemies have the hug / hang rows (48 kinds, c1010 11010060-131), the bosses none.
    pub fn throws_of_kinds(&self, foe: &Foe, kinds: &[i64]) -> Vec<(i64, Throw)> {
        let Some(rows) = self.params.get("ThrowParam").and_then(|t| t.as_object()) else { return Vec::new() };
        let mut ids: Vec<i64> = rows
            .iter()
            .filter(|(_, r)| r["DefChrId"].as_i64() == Some(foe.chr_num) && r["throwKind"].as_i64().is_some_and(|k| kinds.contains(&k)))
            .filter_map(|(k, _)| k.parse().ok())
            .collect();
        ids.sort();
        ids.into_iter().filter_map(|id| Some((id, self.throw(id)?))).collect()
    }

    /// The enemy's grab for a throwFlag-1 hit: its ThrowParam row (AtkChrId = the chr number)
    /// whose throwKind is a ThrowKindParam row checked on a grab hit (throwCheckSituation 4,
    /// only the 10 "[Enemy] Throw n" rows 1000000-1000140) with the hit's AtkParam throwTypeId
    /// (Paramdex SDT ThrowKindParam throwTypeId: "on a throw-transition damage hit, this kind
    /// is checked only when it matches the attack param's throw type id"). c1500 zombies: knife
    /// stab 0 -> 21500000 ThrowAtk4100, eye gouge 1 -> 21500100 4110, restraint 2 -> 21500200
    /// 4120; the General's 0 -> 21020000. The underwater kinds (throwKindCategory 101, only
    /// c1340's 21340000) are taken only when the chr has no other. gap: no water, so the
    /// underwater check itself is not modelled.
    pub fn enemy_grab(&self, foe: &Foe, throw_type: i64) -> Option<Throw> {
        let kinds = self.params.get("ThrowKindParam")?.as_object()?;
        let kind_of = |k: i64| {
            let r = kinds.get(&k.to_string())?;
            (r["throwCheckSituation"].as_i64() == Some(4) && r["throwTypeId"].as_i64() == Some(throw_type))
                .then(|| r["throwKindCategory"].as_i64() == Some(101))
        };
        let rows = self.params.get("ThrowParam")?.as_object()?;
        let (_, id) = rows
            .iter()
            .filter(|(_, r)| r["AtkChrId"].as_i64() == Some(foe.chr_num))
            .filter_map(|(k, r)| Some((kind_of(r["throwKind"].as_i64()?)?, k.parse::<i64>().ok()?)))
            .min()?;
        self.throw(id)
    }

    pub fn velocity_change_row(&self, id: i64) -> Option<VelocityChange> {
        let r = self.params.get("ChrPhysicsVelocityChangeParam")?.get(id.to_string())?;
        let f = |k: &str| r.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        Some(VelocityChange {
            h_scale: f("horizontalVelocityScale"),
            v_scale: f("verticalVelocityScale"),
            h_change: f("horizontalVelocityChange"),
            v_change: f("verticalVelocityChange"),
            h_angle: f("horizontalVelocityAngle"),
            fall_type: r.get("fallType").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            homing_id: r.get("homingId").and_then(|v| v.as_i64()).unwrap_or(0),
            homing_target_type: r.get("homingTargetType").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        })
    }
}

/// CalcCorrectGraph evaluation, matching the exe (FUN_140850b10): find the segment
/// containing x, t = (x - x0) / (x1 - x0), adj = adjPt_maxGrowVal of the segment:
///   adj >= 0: y = y0 + t^adj * (y1 - y0)
///   adj <  0: y = y0 + (1 - (1 - t)^(-adj)) * (y1 - y0)
/// clamped to the segment's y range; x <= 0 gives y0 of the first point.
pub fn calc_correct(graph: &Value, x: f32) -> f32 {
    let get = |k: &str, i: usize| graph[format!("{k}{i}")].as_f64().unwrap_or(0.0) as f32;
    let xs: Vec<f32> = (0..5).map(|i| get("stageMaxVal", i)).collect();
    let ys: Vec<f32> = (0..5).map(|i| get("stageMaxGrowVal", i)).collect();
    let adj: Vec<f32> = (0..5).map(|i| get("adjPt_maxGrowVal", i)).collect();
    let x = x.min(xs[4]);
    if x <= 0.0 {
        return ys[0];
    }
    let mut i = 0;
    while i < 3 && x > xs[i + 1] {
        i += 1;
    }
    let dx = xs[i + 1] - xs[i];
    let t = if dx > 0.0 { ((x - xs[i]) / dx).clamp(0.0, 1.0) } else { 1.0 };
    let f = if adj[i] >= 0.0 { t.powf(adj[i]) } else { 1.0 - (1.0 - t).powf(-adj[i]) };
    let y = ys[i] + f * (ys[i + 1] - ys[i]);
    y.clamp(ys[i].min(ys[i + 1]), ys[i].max(ys[i + 1]))
}

impl Combat {
    pub fn param(&self, table: &str, row: i64) -> &Value {
        &self.params[table][row.to_string()]
    }
}

/// AtkParam AppearAiSoundId (+0x98) / HitAiSoundId (+0x9c) of a row of AtkParam_Pc (`pc`) or
/// _Npc, from extracted/json/params (the combat export leaves them out); (0, 0) for none.
pub fn atk_ai_sounds(pc: bool, id: i64) -> (i64, i64) {
    static T: std::sync::OnceLock<[HashMap<i64, (i64, i64)>; 2]> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        ["AtkParam_Pc", "AtkParam_Npc"].map(|name| {
            param_table(name).iter().map(|(id, r)| (*id, (r["AppearAiSoundId"].as_i64().unwrap_or(0), r["HitAiSoundId"].as_i64().unwrap_or(0)))).collect()
        })
    });
    t[if pc { 0 } else { 1 }].get(&id).copied().unwrap_or((0, 0))
}

/// The cycle / team fields of a SpEffectParam row, from extracted/json/params (combat_data holds
/// only some rows): (stateInfo, effectEndurance, cycleOccurrenceSpEffectId,
/// invocationConditionsStateChange1-3, changeTeamType, addBehaviorJudgeId_condition / _add).
#[derive(Clone, Copy, Debug)]
pub struct SpCycle {
    pub state_info: i64,
    pub endurance: f32,
    pub cycle: i64,
    pub gate: [i64; 3],
    pub team: i64,
    pub judge_add: (i64, i64),
    /// saReceiveDamageRate: scales a hit's super-armour drain (FUN_140bfdd20).
    pub sa_rate: f32,
    /// changeSuperArmorPoint: a stateInfo-155 row's bonus to the super-armour pool's max.
    pub sa_point: f32,
    /// isWaitModeDelete (+0x275 bit 5): deleted the moment its gates fail instead of waiting
    /// (FUN_140c04090, 140c0.c:2641).
    pub wait_delete: bool,
    /// changeHpRate (% of max HP) and changeHpPoint: damage (negative = heal) as it lands.
    pub hp_rate: f32,
    pub hp_point: f32,
    /// spCategory and replaceSpEffectId (the refresh chain 8002 -> 8007, player.rs).
    pub category: i64,
    pub replace: i64,
}


/// EquipParamGoods useLimitSpEffectType0-4 of a goods row: the stateInfos that refuse its use
/// (goods 3000, the Healing Gourd: 341 aging, 910 the Owl's recovery seal).
pub fn goods_use_limits(goods: i64) -> Vec<i64> {
    let t = param_table("EquipParamGoods");
    t.get(&goods).map_or(Vec::new(), |r| (0..5).filter_map(|i| r[format!("useLimitSpEffectType{i}")].as_i64()).filter(|v| *v > 0).collect())
}

pub fn sp_cycle(id: i64) -> Option<SpCycle> {
    static T: std::sync::OnceLock<HashMap<i64, SpCycle>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let rows: Vec<&Value> = param_table("SpEffectParam").values().collect();
        let n = |r: &Value, k: &str| r[k].as_i64().unwrap_or(0);
        rows.iter()
            .filter_map(|r| {
                Some((
                    r["id"].as_i64()?,
                    SpCycle {
                        state_info: n(r, "stateInfo"),
                        endurance: r["effectEndurance"].as_f64().unwrap_or(0.0) as f32,
                        cycle: n(r, "cycleOccurrenceSpEffectId"),
                        gate: [n(r, "invocationConditionsStateChange1"), n(r, "invocationConditionsStateChange2"), n(r, "invocationConditionsStateChange3")],
                        team: r["changeTeamType"].as_i64().unwrap_or(-1),
                        judge_add: (r["addBehaviorJudgeId_condition"].as_i64().unwrap_or(-1), n(r, "addBehaviorJudgeId_add")),
                        sa_rate: r["saReceiveDamageRate"].as_f64().unwrap_or(1.0) as f32,
                        sa_point: r["changeSuperArmorPoint"].as_f64().unwrap_or(0.0) as f32,
                        wait_delete: r["isWaitModeDelete"].as_i64().unwrap_or(0) != 0,
                        hp_rate: r["changeHpRate"].as_f64().unwrap_or(0.0) as f32,
                        hp_point: n(r, "changeHpPoint") as f32,
                        category: n(r, "spCategory"),
                        replace: n(r, "replaceSpEffectId"),
                    },
                ))
            })
            .collect()
    });
    t.get(&id).copied()
}

/// A full param table from extracted/json/params (rows by id), for fields the export leaves out.
/// SpEffectParam behaviorRefId of any row (the HKS env 3036 ref), for effects outside Wolf's
/// exported set (map area effects such as 108900 or the storm-jump areas); 0 for none.
pub fn sp_effect_behavior_ref(id: i64) -> i64 {
    static REFS: std::sync::OnceLock<HashMap<i64, i64>> = std::sync::OnceLock::new();
    let refs = REFS.get_or_init(|| param_table("SpEffectParam").iter().filter_map(|(id, r)| Some((*id, r["behaviorRefId"].as_i64().filter(|&v| v > 0)?))).collect());
    refs.get(&id).copied().unwrap_or(0)
}

/// An SpEffect's behaviorRefId while it lasts past its TAE window (behaviorRefFlag_checkAliveFlagForBehavior
/// 0: the exe's env 3036, FUN_140b32410 case 0xbdc, counts the row for its whole effectEndurance;
/// 1 only while the TAE window is open). The sprint's 100304 ends into 100303, ref 1 for 0.2 s.
pub fn sp_effect_lingering_ref(id: i64) -> i64 {
    static REFS: std::sync::OnceLock<HashMap<i64, i64>> = std::sync::OnceLock::new();
    let refs = REFS.get_or_init(|| {
        param_table("SpEffectParam")
            .iter()
            .filter(|(_, r)| r["behaviorRefFlag_checkAliveFlagForBehavior"].as_i64() == Some(0))
            .filter_map(|(id, r)| Some((*id, r["behaviorRefId"].as_i64().filter(|&v| v > 0)?)))
            .collect()
    });
    refs.get(&id).copied().unwrap_or(0)
}

/// How an event state ends in a chr's HKS, at the anim's end (`env(339, 1) == TRUE`):
/// `Fire("W_Event<n>")` goes on to that event, `Fire("W_Idle")` back to the AI's idle,
/// `ExecTransToDeathIdle` (its own 339 check) into the death pose (c1020's sting death 20005).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EndTo {
    Event(i64),
    Idle,
    DeathIdle,
}

/// An end in a chr's own HKS: Event<from>_onUpdate's end under the enclosing
/// `if env(3036, ref) == TRUE` checks (`(ref, true)`; an `else` gives `(ref, false)`). The zombie
/// c1500's held pose 20010 -> 21015, c1020 20002 -> 21001 / 20004 -> 21007, the Divine Dragon's
/// trees c5310 20000 -> 21000. `at_end`: under the clip's end (env 339); else it fires at any
/// frame, `grounded` (env 201) when it asks (c1100 21005 -> 20005 once on the ground). No talk ESD
/// runs here: `env(3054) ~= ref` holds and `env(3054) == ref` never (c1020 21007 -> 21005), and
/// ExecTalkTransitionIndividual's NOT_TALKING moves are taken (c1070 21000 -> 20001).
#[derive(Clone, Debug, PartialEq)]
pub struct EndChain {
    pub from: i64,
    pub to: EndTo,
    pub refs: Vec<(i64, bool)>,
    pub at_end: bool,
    pub grounded: Option<bool>,
}

/// A chr's event states as its HKS runs them (extracted/hks_src/<chr>.lua, DSLuaDecompiler on
/// the behbnd's c<chr>.hks, over c9997.lua's for the states the chr leaves out).
#[derive(Clone, Debug, Default)]
pub struct HksEvents {
    pub ends: Vec<EndChain>,
    /// Event<n>_onUpdate exists; true when it calls EventCommonFunction, whose
    /// ExecActiveTransition(TRUE, ...) takes the AI's cancel-slot requests (the clip's TAE
    /// windows), ExecPassiveTransition the hits and ExecTalkTransition the talk. A state whose
    /// update fires no end for the refs worn holds: its loop clip plays on (c1020 21000, the
    /// Divine Dragon's trees c5310 21000-21002).
    pub states: HashMap<i64, bool>,
}

impl HksEvents {
    /// Where event state `from` goes now, for the behaviour refs worn, on the ground or not, at
    /// its clip's end or before; None: it stays.
    pub fn next(&self, from: i64, refs: &[i64], grounded: bool, at_end: bool) -> Option<EndTo> {
        self.ends
            .iter()
            .find(|c| c.from == from && (at_end || !c.at_end) && c.grounded.is_none_or(|g| g == grounded) && c.refs.iter().all(|(r, w)| refs.contains(r) == *w))
            .map(|c| c.to)
    }
}

/// `hks_events` for a chr; empty without its decompiled HKS (then enemy.rs keeps its own rule).
pub fn hks_events(chr: &str) -> std::sync::Arc<HksEvents> {
    static T: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<HksEvents>>>> = std::sync::OnceLock::new();
    let mut t = T.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    t.entry(chr.to_string())
        .or_insert_with(|| {
            let read = |c: &str| std::fs::read_to_string(crate::paths::extracted().join(format!("hks_src/{c}.lua"))).ok();
            let Some(own) = read(chr) else { return Default::default() };
            let (mut ev, talk) = parse_hks_events(&own);
            if let Some(common) = read("c9997") {
                let (base, _) = parse_hks_events(&common);
                for (n, c) in base.states {
                    if let std::collections::hash_map::Entry::Vacant(v) = ev.states.entry(n) {
                        v.insert(c);
                        ev.ends.extend(base.ends.iter().filter(|e| e.from == n).cloned());
                    }
                }
            }
            // c9997 EventCommonFunction runs ExecTalkTransition(anim_id) first.
            for (from, to) in talk {
                if ev.states.get(&from) == Some(&true) {
                    ev.ends.insert(0, EndChain { from, to: EndTo::Event(to), refs: Vec::new(), at_end: false, grounded: None });
                }
            }
            std::sync::Arc::new(ev)
        })
        .clone()
}

/// The end chains of a chr's own HKS (`hks_events`).
#[cfg(test)]
pub fn hks_end_chains(chr: &str) -> Vec<EndChain> {
    hks_events(chr).ends.clone()
}

/// The parse behind `hks_events`, with ExecTalkTransitionIndividual's not-talking moves (from,
/// to). An end under any other condition (a random `fate`, an `elseif` after a live branch,
/// another env) is left out, and so is one to a state other than W_Event<n> / W_Idle (c1190's
/// W_GuardIdle).
pub(crate) fn parse_hks_events(text: &str) -> (HksEvents, Vec<(i64, i64)>) {
    #[derive(Clone, Copy)]
    enum C {
        End,
        Ref(i64, bool),
        Grounded(bool),
        Always,
        Never,
        Other,
    }
    let cond = |c: &str| {
        match c {
            "env(201) == TRUE" => return C::Grounded(true),
            "env(201) == FALSE" => return C::Grounded(false),
            _ if c.starts_with("env(3054) == ") => return C::Never,
            _ if c.starts_with("env(3054) ~= ") => return C::Always,
            _ => {}
        }
        let (ref_, want) = match c.strip_prefix("env(3036, ") {
            Some(r) => match (r.strip_suffix(") == TRUE"), r.strip_suffix(") == FALSE")) {
                (Some(r), _) => (r, true),
                (_, Some(r)) => (r, false),
                _ => return C::Other,
            },
            None => return if c == "env(339, 1) == TRUE" { C::End } else { C::Other },
        };
        // c9997 SP_EFFECT_REF_WEAPON_0-4 = 1000020-1000024 (the anim-set SpEffects 200030-4).
        let id = ref_.parse().ok().or_else(|| ref_.strip_prefix("SP_EFFECT_REF_WEAPON_")?.parse::<i64>().ok().map(|n| 1_000_020 + n));
        id.map_or(C::Other, |id| C::Ref(id, want))
    };
    // `a and b`: one stack entry per part (c1500 21008's `env(3036, 1000021) == TRUE and
    // env(3054) ~= ...`); a part that never holds kills it, one that always does drops out.
    let conds = |c: &str| {
        let parts: Vec<C> = if c.contains(" or ") { vec![C::Other] } else { c.split(" and ").map(cond).collect() };
        if parts.iter().any(|p| matches!(p, C::Never)) {
            return vec![C::Never];
        }
        let parts: Vec<C> = parts.into_iter().filter(|p| !matches!(p, C::Always)).collect();
        if parts.is_empty() { vec![C::Always] } else { parts }
    };
    let mut out = HksEvents::default();
    let mut talk = Vec::new();
    let mut from: Option<i64> = None;
    let (mut in_talk, mut talk_from, mut talk_anim) = (false, None::<i64>, None::<i64>);
    let mut stack: Vec<(usize, C)> = Vec::new();
    let refs_of = |stack: &[(usize, C)]| stack.iter().filter_map(|(_, c)| if let C::Ref(r, w) = c { Some((*r, *w)) } else { None }).collect::<Vec<_>>();
    let grounded_of = |stack: &[(usize, C)]| stack.iter().rev().find_map(|(_, c)| if let C::Grounded(g) = c { Some(*g) } else { None });
    // 81 of the decompiled files start with a BOM, which hid their first function.
    for line in text.trim_start_matches('\u{feff}').lines() {
        let s = line.trim();
        if line.starts_with("function") {
            from = line.strip_prefix("function Event").and_then(|r| r.strip_suffix("_onUpdate()")).and_then(|n| n.parse().ok());
            if let Some(f) = from {
                out.states.insert(f, false);
            }
            (in_talk, talk_from, talk_anim) = (line.starts_with("function ExecTalkTransitionIndividual("), None, None);
            stack.clear();
            continue;
        }
        // `anim_id == <n> and (... talk_ref_Id == TALK_BEH_IDENTIFIER__NOT_TALKING)` then
        // Fire("W_Event<m>"), or the talk check nested under `if anim_id == <n> then` (c1110).
        if in_talk {
            if s.starts_with("if ") || s.starts_with("elseif ") {
                if let Some(r) = s.split("anim_id == ").nth(1) {
                    talk_anim = r.split(|ch: char| !ch.is_ascii_digit()).next().and_then(|n| n.parse().ok());
                }
                talk_from = s.contains("talk_ref_Id == TALK_BEH_IDENTIFIER__NOT_TALKING").then_some(talk_anim).flatten();
            } else if let (Some(n), Some(m)) = (talk_from, s.strip_prefix("Fire(\"W_Event").and_then(|r| r.strip_suffix("\")")).and_then(|m| m.parse().ok())) {
                talk.push((n, m));
                talk_from = None;
            }
            continue;
        }
        let Some(f) = from else { continue };
        let ind = line.len() - line.trim_start().len();
        let dead = stack.iter().any(|(_, c)| matches!(c, C::Other | C::Never));
        if s.contains("EventCommonFunction(") {
            out.states.insert(f, true);
        }
        if s.contains("ExecTransToDeathIdle(") && !dead {
            out.ends.push(EndChain { from: f, to: EndTo::DeathIdle, refs: refs_of(&stack), at_end: true, grounded: grounded_of(&stack) });
        }
        if s == "end" || s == "else" || s.starts_with("elseif ") {
            let n = stack.iter().rev().take_while(|(i, _)| *i == ind).count();
            if n > 0 {
                // An `a and b` group's else is an `or`: left out.
                let c = if n == 1 { stack.pop().unwrap().1 } else { stack.truncate(stack.len() - n); C::Other };
                let next = match (s, c) {
                    ("end", _) => vec![],
                    ("else", C::Ref(r, w)) => vec![C::Ref(r, !w)],
                    ("else", C::Grounded(g)) => vec![C::Grounded(!g)],
                    ("else", C::Never) => vec![C::Always],
                    // After a branch that never runs the elseif is a plain if; after one that
                    // always runs nothing else does.
                    (_, C::Never) => s.strip_prefix("elseif ").and_then(|c| c.strip_suffix(" then")).map_or(vec![C::Other], conds),
                    (_, C::Always) => vec![C::Never],
                    _ => vec![C::Other],
                };
                stack.extend(next.into_iter().map(|c| (ind, c)));
            }
        } else if let Some(c) = s.strip_prefix("if ").and_then(|c| c.strip_suffix(" then")) {
            stack.extend(conds(c).into_iter().map(|c| (ind, c)));
        } else if let Some(name) = s.strip_prefix("Fire(\"W_").and_then(|r| r.strip_suffix("\")")) {
            let to = match name.strip_prefix("Event").and_then(|n| n.parse().ok()) {
                Some(n) => EndTo::Event(n),
                None if name == "Idle" => EndTo::Idle,
                None => continue,
            };
            if !dead {
                let at_end = stack.iter().any(|(_, c)| matches!(c, C::End));
                out.ends.push(EndChain { from: f, to, refs: refs_of(&stack), at_end, grounded: grounded_of(&stack) });
            }
        }
    }
    (out, talk)
}

/// Parsed once and shared by every reader: SpEffectParam.json is 40 MB, about 0.3 s a parse, and
/// each lookup built from it used to parse it again on its first use.
pub fn param_table(name: &str) -> &'static HashMap<i64, Value> {
    type Slot = std::sync::OnceLock<HashMap<i64, Value>>;
    static TABLES: std::sync::OnceLock<std::sync::Mutex<HashMap<String, &'static Slot>>> = std::sync::OnceLock::new();
    // The lock covers only the slot lookup, so a table being parsed by warm_params does not hold
    // up a reader of another table.
    let slot: &'static Slot = TABLES.get_or_init(Default::default).lock().unwrap().entry(name.to_string()).or_insert_with(|| Box::leak(Box::default()));
    slot.get_or_init(|| {
        let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
        let mut v: Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let rows: Vec<Value> = match v["rows"].take() {
            Value::Array(a) => a,
            Value::Object(o) => o.into_iter().map(|(_, r)| r).collect(),
            _ => Vec::new(),
        };
        rows.into_iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect()
    })
}

/// Builds the big param tables and the lookups made from them on a worker thread at start.
/// Built on first use instead, the first hit on Wolf parsed them on the main thread: a 0.3 s
/// stall in combat::resolve.
#[cfg(not(test))]
fn warm_params() {
    std::thread::spawn(|| {
        for t in ["SpEffectParam", "AtkParam_Pc", "AtkParam_Npc", "Bullet", "EquipParamGoods"] {
            param_table(t);
        }
        sp_cycle(0);
        sp_effect_behavior_ref(0);
        sp_effect_lingering_ref(0);
        atk_occurrence(&[0], HIT_CAT_SWORD, false, false);
        prediction_observe_time(0);
        atk_ai_sounds(true, 0);
        crate::status::warm();
    });
}

/// A hit's super-armour damage before the defender's rates (FUN_140bdb470): an NPC attack's
/// AtkParam_Npc atkSuperArmor (+0x66: 0 on most, 999 / 45 on a few); Wolf's the weapon's
/// EquipParamWeapon saWeaponDamage (+0xCE) x AtkParam_Pc atkSuperArmorCorrection (+0x56) / 100.
/// The weapon is the hit's equip-slot weapon (FUN_14084fc50 hit+0x12c, 14084.c:8469; read in
/// FUN_140848440, 14084.c:4836): a katana 5000-7700 or a tool 7xxxx, all 40, never a combat
/// art's 2xxxxx row (0). So 5000 stands for them all. ReinforceParamWeapon saWeaponAtkRate
/// (+0x14) multiplies in, and it is 1 on every row. gap: the 110000 bare-hands fallback (hit
/// flags & 0x60) and FUN_140841bd0's rate are not traced.
/// The attack-occurrence SpEffect a hit adds (FUN_140bfc170, 140bf.c:8239, applied by the damage
/// apply FUN_140b6e6a0 at 140b6.c:9012-9029): the first worn row of the attacker whose stateInfo
/// is 62 / 64 / 151-153 / 205 / 355-358 gives its atkOccurrenceSpEffectId. Wolf's enchantments:
/// Flame Vent 106020 -> 106021 (endurance 0 -> its replace, burn 9100), Spirit 106030 -> 104016,
/// poison-blood 106050 -> 9002, white-blood 106060 -> 106065 (HP +10 %, on the attacker); the
/// Nightjars' 3145000 -> 9125. Returns (id, the ids that land: it and an endurance-0 row's
/// replaceSpEffectId, on the attacker or not, changeHpRate).
/// A row counts only for the hit's behaviour category (FUN_140c06240, 140c0.c:5288; the hit's
/// +0xda = BehaviorParam category, 14084.c:8460): wepParamChange 0 passes every category, 1
/// (right hand) the sword and combat arts (`HIT_CAT_SWORD`), 2 the prosthetic tools
/// (`HIT_CAT_TOOL`), 4 a combat art's bullets (9). throwAttackParamChange asks for a throw hit
/// (AtkParam throwFlag 2) and chargeAttackParamChange for a charge attack (isChargeAtk). Every
/// enchantment of Wolf's is wepParamChange 1, so his tools' bullets carry none.
/// gap: the categories 3 / 4 / 10-12 branches (no row uses them) and the entry skip mask
/// (+0x1c & 0x800c0003) are not modelled.
/// BehaviorParam(_PC) category of a hit (FUN_140c06240): 1 the sword and combat arts, 2 the
/// prosthetic tools.
pub const HIT_CAT_SWORD: i64 = 1;
pub const HIT_CAT_TOOL: i64 = 2;

pub fn atk_occurrence(worn: &[i64], cat: i64, throw_hit: bool, charge_hit: bool) -> Option<(i64, Vec<i64>, bool, f32)> {
    #[derive(Clone)]
    struct Row {
        state_info: i64,
        occurrence: i64,
        endurance: f32,
        replace: i64,
        on_attacker: bool,
        hp_rate: f32,
        wep: i64,
        throw_only: bool,
        charge_only: bool,
    }
    static ROWS: std::sync::OnceLock<HashMap<i64, Row>> = std::sync::OnceLock::new();
    let rows = ROWS.get_or_init(|| {
        let i = |r: &Value, k: &str| r[k].as_i64().unwrap_or(-1);
        param_table("SpEffectParam")
            .iter()
            .map(|(id, r)| {
                (*id, Row {
                    state_info: i(r, "stateInfo"),
                    occurrence: i(r, "atkOccurrenceSpEffectId"),
                    endurance: r["effectEndurance"].as_f64().unwrap_or(0.0) as f32,
                    replace: i(r, "replaceSpEffectId"),
                    on_attacker: i(r, "effectTargetAttacker") == 1 && i(r, "effectTargetEnemy") != 1,
                    hp_rate: r["changeHpRate"].as_f64().unwrap_or(0.0) as f32,
                    wep: i(r, "wepParamChange").max(0),
                    throw_only: i(r, "throwAttackParamChange") == 1,
                    charge_only: i(r, "chargeAttackParamChange") == 1,
                })
            })
            .filter(|(_, r)| r.occurrence > 0 || r.replace > 0 || r.on_attacker)
            .collect()
    });
    let passes = |r: &Row| {
        let wep = match cat {
            HIT_CAT_SWORD => matches!(r.wep, 0 | 1),
            HIT_CAT_TOOL => matches!(r.wep, 0 | 2),
            9 => matches!(r.wep, 0 | 4),
            _ => r.wep == 0,
        };
        wep && (!r.throw_only || throw_hit) && (!r.charge_only || charge_hit)
    };
    let id = worn.iter().filter_map(|w| rows.get(w)).find(|r| r.occurrence > 0 && matches!(r.state_info, 62 | 64 | 151..=153 | 205 | 355..=358) && passes(r))?.occurrence;
    let row = rows.get(&id).cloned();
    let mut lands = vec![id];
    if let Some(r) = row.as_ref().filter(|r| r.endurance == 0.0 && r.replace > 0) {
        lands.push(r.replace);
    }
    Some((id, lands, row.as_ref().is_some_and(|r| r.on_attacker), row.map_or(0.0, |r| r.hp_rate)))
}

/// BulletParam predictionShootObserveTime (+0xB8, 0 = off): the window (s) over which a
/// predictive shot averages its target's velocity (FUN_140b0acf0, 140b0.c:9107). The export
/// leaves it out, so it is read from the full table.
pub fn prediction_observe_time(bullet_id: i64) -> f32 {
    static ROWS: std::sync::OnceLock<HashMap<i64, f32>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| param_table("Bullet").iter().map(|(id, r)| (*id, r["predictionShootObserveTime"].as_f64().unwrap_or(0.0) as f32)).filter(|(_, t)| *t > 0.0).collect())
        .get(&bullet_id)
        .copied()
        .unwrap_or(0.0)
}

pub fn super_armor_damage(pc: bool, atk_param_id: i64) -> f32 {
    let f = |r: &Value, k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
    if pc {
        f(crate::items::param("EquipParamWeapon", 5000), "saWeaponDamage") * f(atk_row(true, atk_param_id), "atkSuperArmorCorrection") / 100.0
    } else {
        f(atk_row(false, atk_param_id), "atkSuperArmor")
    }
}

/// A whole AtkParam row (AtkParam_Pc for Wolf's attacks, else AtkParam_Npc) for the fields the
/// combat export does not carry (isDisableNoDamage, guardAtkRate, guardBreakRate ...). Null when
/// the row is missing.
pub fn atk_row(pc: bool, atk_param_id: i64) -> &'static Value {
    crate::items::param(if pc { "AtkParam_Pc" } else { "AtkParam_Npc" }, atk_param_id)
}

/// An NPC's behaviour judge as its worn SpEffects shift it (exe FUN_140a12b80, 140a1.c:2191-2236,
/// building BehaviorParam 200000000 + variation * 1000 + judge): every worn row whose
/// addBehaviorJudgeId_condition is judge / 100 adds its addBehaviorJudgeId_add. A row counts
/// while its invocationConditionsStateChange gate holds (a worn SpEffect of that stateInfo): the
/// Shura samurai's a000_003014 adds both 3107010 (poison, +5, gate 950) and 3107011 (flame, +7,
/// gate 951); its resident 3107040 (950) or 3107041 (951) picks one, so 100 -> 105 or 107 for
/// 15 s. The Nightjars wear 3145040 (700 -> 701) for good. There is no fallback on a missing row
/// (the shifted attack does nothing, 14084.c:8433). gap: the exe's "add 0 -> id 5" branch
/// (only the test row 0 has condition 0) is not modelled.
pub fn shift_judge(judge: i64, worn: &[i64]) -> i64 {
    let mut ids: Vec<i64> = worn.to_vec();
    ids.sort();
    ids.dedup();
    let rows: Vec<SpCycle> = ids.iter().filter_map(|id| sp_cycle(*id)).collect();
    let states: Vec<i64> = rows.iter().map(|c| c.state_info).filter(|s| *s > 0).collect();
    judge
        + rows
            .iter()
            .filter(|c| c.judge_add.0 >= 0 && c.judge_add.0 == judge / 100 && c.judge_add.1 != 0)
            .filter(|c| c.gate.iter().filter(|g| **g > 0).all(|g| states.contains(g)))
            .map(|c| c.judge_add.1)
            .sum::<i64>()
}

/// A SpEffect's effectEndurance [s] (SpEffectParam: 0 = a moment, -1 = for good), from Wolf's
/// exported SpEffects or the SpEffectParam rows in combat_data.
pub fn sp_effect_endurance(combat: &Combat, id: i64) -> f32 {
    combat
        .player
        .sp_effects
        .get(&id.to_string())
        .map(|s| s.effect_endurance)
        .or_else(|| combat.param("SpEffectParam", id)["effectEndurance"].as_f64().map(|v| v as f32))
        .unwrap_or(0.0)
}

impl CharData {
    pub fn anim_key(&self, state: &str) -> Option<&str> {
        self.states.get(state).map(String::as_str)
    }

    pub fn anim(&self, key: &str) -> Option<&Anim> {
        self.anims.get(key)
    }

    /// Clip length; falls back to the last TAE event for clips without an HKX.
    pub fn length(&self, key: &str) -> f32 {
        let Some(a) = self.anim(key) else { return 0.0 };
        a.duration.unwrap_or_else(|| a.events.iter().map(|e| e.end).fold(0.0, f32::max))
    }

    pub fn events_at<'a>(&'a self, key: &str, t: f32) -> impl Iterator<Item = &'a Event> + 'a {
        self.anim(key).into_iter().flat_map(|a| a.events.iter()).filter(move |e| e.in_time(t) && self.fires(e))
    }

    /// An event fires: ungated, or gated by a StateInfo of the equipment's resident SpEffects.
    pub fn fires(&self, e: &Event) -> bool {
        e.ungated() || self.resident_gates.contains(&e.arg_i64("StateInfo").unwrap_or(0))
    }

    /// `fires` with the character's live gates (`combat::event_gates`) in place of the static
    /// NpcParam ones; None keeps `resident_gates` (Wolf's: player.rs keeps them with his gear).
    pub fn fires_on(&self, e: &Event, gates: Option<&[i64]>) -> bool {
        match gates {
            Some(g) => e.ungated() || g.contains(&e.arg_i64("StateInfo").unwrap_or(0)),
            None => self.fires(e),
        }
    }

    /// `events_at` with live gates (`fires_on`).
    pub fn events_at_on<'a>(&'a self, key: &str, t: f32, gates: Option<&'a [i64]>) -> impl Iterator<Item = &'a Event> + 'a {
        self.anim(key).into_iter().flat_map(|a| a.events.iter()).filter(move |e| e.in_time(t) && self.fires_on(e, gates))
    }

    /// The character's opacity from TAE 193 SetOpacityKeyframe (OpacityAtEventStart -> End over
    /// the event), None before the anim's first keyframe: the Corrupted Monk's phantoms (c5005)
    /// fade in at the start of their a200 attacks and out at the end, and stand at 0 in their
    /// a200 idle, which is how the game keeps them out of sight between calls. The value holds
    /// to the clip's next keyframe; a clip without one takes the group idle's (actor.rs). gap:
    /// the hold after a keyframe ends (a200_003001's last ends at 5.0 s of 6.67) is assumed from
    /// the event's name, not traced in the exe.
    pub fn opacity_key(&self, key: &str, t: f32) -> Option<f32> {
        let a = self.anim(key)?;
        let f = |e: &Event, n: &str| e.args.get(n).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let e = a.events.iter().filter(|e| e.kind == 193 && e.start <= t).max_by(|x, y| x.start.total_cmp(&y.start))?;
        let (from, to) = (f(e, "OpacityAtEventStart"), f(e, "OpacityAtEventEnd"));
        let k = if e.end > e.start { ((t - e.start) / (e.end - e.start)).clamp(0.0, 1.0) } else { 1.0 };
        Some((from + (to - from) * k).clamp(0.0, 1.0))
    }

    /// The finisher window: TAE 932 with unk0 105 (in the major bosses' kneeling death, their
    /// Event20200: Isshin 1-5 s, the Guardian Ape, Genichiro ...) or 104 (an alive collapse: the
    /// Divine Dragon's Event21000 after m25 12505887's 20002, the headless Ape's a100_021000,
    /// c1200's a000_010510), the time Wolf can deal the "enemy special state A" deathblow
    /// (ThrowParam throwKind 250000; Isshin's 15400590 -> ThrowDef12900, whose message 30 at
    /// 11.6 s ends the fight: m11_02 11125860; the dragon's 15200090). Its clip held at the end
    /// counts too. gap: what 932 / 104-105 is in the exe is not traced; the match of such windows
    /// to the characters with a 250000 row is the evidence (c1010 / c1140 have 104 and no row).
    pub fn finisher_open(&self, key: &str, t: f32) -> bool {
        self.finisher_window(key, t).is_some()
    }

    /// Which finisher window is open (finisher_open): 105, the Todome's (every boss with Todome
    /// rows has it on its Event20200, 1-5 s: c5000 c5060 c5090 c5100 c5400 c5430 c7020 c7110), or
    /// 104, the "enemy special state A" one (Event21000 and the like). The recordings
    /// (rec_2026101x_*_boss_*: Butterfly, Genichiro castle, Ape, True Monk, Demon of Hatred, Owl
    /// Father, Isshin, Isshin Ashina) all take the Todome 0180 / 0181 from the boss's 20200, 0.8-1.8 s
    /// into Wolf's Event7102xx; Isshin's 590 comes from his 21000 after it.
    pub fn finisher_window(&self, key: &str, t: f32) -> Option<i64> {
        let a = self.anim(key)?;
        let end = t >= self.length(key);
        a.events.iter().find(|e| e.kind == 932 && matches!(e.arg_i64("unk0"), Some(104 | 105)) && (e.in_time(t) || end)).and_then(|e| e.arg_i64("unk0"))
    }

    /// Is a ChrActionFlag (TAE type 0) of this FlagType active?
    pub fn flag(&self, key: &str, t: f32, flag_type: i64) -> bool {
        self.events_at(key, t).any(|e| e.kind == 0 && e.flag_type() == Some(flag_type))
    }

    /// SpEffects applied by the anim's TAE (type 67 AddSpEffect, 66 AddSpEffect_Multiplayer
    /// (the online-synced one: the Corrupted Monk's 5031 in her 3032 / 3033), and 401) at time t.
    pub fn sp_effects_at(&self, key: &str, t: f32) -> Vec<(i64, &SpEffect)> {
        self.events_at(key, t)
            .filter(|e| e.kind == 67 || e.kind == 66 || e.kind == 401)
            .filter_map(|e| {
                let id = e.arg_i64("SpEffectID")?;
                self.sp_effects.get(&id.to_string()).map(|s| (id, s))
            })
            .collect()
    }

    /// Is any TAE-applied SpEffect with this stateInfo active? (engine-side checks use stateInfo)
    pub fn has_state_info(&self, key: &str, t: f32, state_info: i64) -> bool {
        self.sp_effects_at(key, t).iter().any(|(_, s)| s.state_info == state_info)
    }

    /// HKS `env(3036, ref)`: is a SpEffect with this behaviorRefId active?
    pub fn has_ref(&self, key: &str, t: f32, behavior_ref: i64) -> bool {
        self.sp_effects_at(key, t).iter().any(|(_, s)| s.behavior_ref_id == behavior_ref)
    }

    /// TAE 960: StaminaControlParam ratio type for posture regen, if set at t.
    pub fn stamina_ratio_type(&self, key: &str, t: f32) -> Option<i64> {
        self.events_at(key, t).find(|e| e.kind == 960).and_then(|e| e.arg_i64("StaminaRatioType"))
    }

    /// TAE 920 ChrPhysicsVelocityChange starting in (t0, t1] (or at 0 on the first tick).
    pub fn velocity_change(&self, key: &str, t0: f32, t1: f32) -> Option<i64> {
        self.anim(key)?
            .events
            .iter()
            .find(|e| e.kind == 920 && ((e.start > t0 && e.start <= t1) || (t0 == 0.0 && e.start == 0.0)))
            .and_then(|e| e.arg_i64("ChrPhysicsVelocityParam ID"))
    }

    /// TAE 922 ChrPhysicsVelosityScale starting in (t0, t1], or at 0 on the first tick (its row:
    /// arg ChrPhysicsVelocityParam ID).
    pub fn velocity_scale(&self, key: &str, t0: f32, t1: f32) -> Option<&Event> {
        self.anim(key)?.events.iter().find(|e| e.kind == 922 && ((e.start > t0 && e.start <= t1) || (t0 == 0.0 && e.start == 0.0 && t1 > 0.0)))
    }

    /// Perilous-attack warning (the red kanji): a TAE BulletBehavior (type 2) with
    /// judge 980-989 starting in (t0, t1]. 980 sweep, 982 thrust, 983 grab.
    pub fn perilous_warning(&self, key: &str, t0: f32, t1: f32) -> Option<i64> {
        // By the bullet's SpEffect 211000 (the kanji), not the 980-989 suffix alone: the Guardian
        // Ape's phase-1 grabs a000_003008 / 003016 / 003027 fire Bullet 51000480, which carries
        // 211000 too (boss fidelity 2026-10-14: the trace and the AI's perilous answers missed them).
        let kanji = |j: i64| self.bullets.get(&j.to_string()).is_some_and(|b| [b.sp_effect_id0, b.sp_effect_id1, b.sp_effect_id2, b.sp_effect_id3, b.sp_effect_id4].contains(&211000));
        self.anim(key)?
            .events
            .iter()
            .filter(|e| e.kind == 2 && ((e.start > t0 && e.start <= t1) || (t0 == 0.0 && e.start == 0.0)))
            .filter_map(|e| e.arg_i64("BehaviorJudgeID"))
            .find(|j| (980..990).contains(j) || kanji(*j))
    }

    /// TAE 16 "Blend" at the start of an anim: its crossfade-in time [s].
    pub fn blend_in(&self, key: &str) -> Option<f32> {
        self.anim(key)?.events.iter().find(|e| e.kind == 16 && e.start <= 1e-4).map(|e| e.end - e.start)
    }

    /// TAE 224 SetTurnSpeed (deg/s) if set at t.
    /// TAE 224 SetTurnSpeed [deg/s]. Events with IsLockOnCheck apply only while locked on
    /// (exe FUN_140b574b0 checks chr+0x1070).
    pub fn turn_speed(&self, key: &str, t: f32, locked: bool) -> Option<f32> {
        self.events_at(key, t)
            .filter(|e| locked || !e.args.get("IsLockOnCheck").and_then(|v| v.as_bool()).unwrap_or(false))
            .find(|e| e.kind == 224)
            .and_then(|e| e.args.get("TurnSpeed")?.as_f64())
            .map(|v| v as f32)
    }

    /// AttackBehavior (TAE type 1) events whose judge has AtkParam data.
    pub fn attack_windows<'a>(&'a self, key: &str) -> Vec<(&'a Event, &'a Attack, i64)> {
        self.attack_windows_on(key, None)
    }

    /// `attack_windows` with live gates (`fires_on`).
    pub fn attack_windows_on<'a>(&'a self, key: &str, gates: Option<&[i64]>) -> Vec<(&'a Event, &'a Attack, i64)> {
        let anim_key = key;
        self.anim(key)
            .into_iter()
            .flat_map(|a| a.events.iter())
            .filter(|e| (e.kind == 1 || e.kind == 307) && self.fires_on(e, gates))
            .filter_map(|e| {
                let j = e.arg_i64("BehaviorJudgeID")?;
                // PCBehavior (307, e.g. the air kick) resolves straight to BehaviorParam_PC "pc<judge>".
                let key = if e.kind == 307 { format!("pc{j}") } else { j.to_string() };
                // Combat-art anims (a100..a110) resolve through their art's variation first.
                let art = anim_key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| (100..=110).contains(g));
                let var = self.art_variation.and_then(|v| self.attacks.get(&format!("v{v}:{j}")));
                let tool = anim_key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| (70..=79).contains(g));
                let tool_atk = tool.and(self.tool_variation).and_then(|v| self.attacks.get(&format!("v{v}:{j}")));
                // A PCBehavior judge is BehaviorParam_PC's own row, never the art's / tool's attack of
                // the same number: the air Mortal Draw a106_316200's 16 contact windows (307 judge
                // 210, pc210: no damage) hit as v5007:210 (the Mortal Draw slash, ~200 HP + 38
                // posture each) - round 28.
                let atk = if e.kind == 307 {
                    self.attacks.get(&key)
                } else {
                    art.and_then(|g| var.or_else(|| self.attacks.get(&format!("a{g}:{j}")))).or(tool_atk).or_else(|| self.attacks.get(&key))
                };
                atk.map(|a| (e, a, if e.kind == 307 { 1_000_000 + j } else { j }))
            })
            .collect()
    }

    /// A tool anim judge's behaviour: BehaviorParam_PC 100000000 + variation * 1000 + judge.
    pub fn behavior(&self, variation: i64, judge: i64) -> Option<&PcBehavior> {
        self.behaviors.get(&(100_000_000 + variation * 1000 + judge).to_string())
    }

    /// Root motion position (x, z) and yaw at time t, linearly interpolated.
    pub fn root_at(&self, key: &str, t: f32) -> Option<(Vec2, f32)> {
        let a = self.anim(key)?;
        let rate = a.root_rate?;
        if a.root.is_empty() {
            return None;
        }
        let f = (t.max(0.0) * rate).min((a.root.len() - 1) as f32);
        let i = f.floor() as usize;
        let j = (i + 1).min(a.root.len() - 1);
        let k = f - i as f32;
        let (p, q) = (a.root[i], a.root[j]);
        let lerp = |u: f32, v: f32| u + (v - u) * k;
        Some((Vec2::new(lerp(p[0], q[0]), lerp(p[2], q[2])), lerp(p[3], q[3])))
    }

    /// Root motion height at time t (the clip's vertical root track, e.g. the vault 511900).
    pub fn root_y_at(&self, key: &str, t: f32) -> f32 {
        let Some(a) = self.anim(key) else { return 0.0 };
        let (Some(rate), n) = (a.root_rate, a.root.len()) else { return 0.0 };
        if n == 0 {
            return 0.0;
        }
        let f = (t.max(0.0) * rate).min((n - 1) as f32);
        let i = f.floor() as usize;
        let j = (i + 1).min(n - 1);
        a.root[i][1] + (a.root[j][1] - a.root[i][1]) * (f - i as f32)
    }

    /// Root-motion velocity (m/s, local direction) of a clip at time t: the slope of the sampled
    /// root track there (loops wrap t over the clip length).
    pub fn root_velocity_at(&self, key: &str, t: f32, looping: bool) -> Vec2 {
        let Some(a) = self.anim(key) else { return Vec2::ZERO };
        let (Some(rate), n) = (a.root_rate, a.root.len()) else { return Vec2::ZERO };
        if n < 2 {
            return Vec2::ZERO;
        }
        let len = (n - 1) as f32 / rate;
        let t = if looping { t.rem_euclid(len.max(1e-3)) } else { t.clamp(0.0, len) };
        let i = ((t * rate).floor() as usize).min(n - 2);
        let (p, q) = (a.root[i], a.root[i + 1]);
        Vec2::new(q[0] - p[0], q[2] - p[2]) * rate
    }

    /// Average horizontal speed of a clip's root motion (m/s), local direction.
    pub fn root_velocity(&self, key: &str) -> Vec2 {
        let len = self.length(key);
        match (self.root_at(key, len), len > 0.0) {
            (Some((p, _)), true) => p / len,
            _ => Vec2::ZERO,
        }
    }
}

pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(test))]
        app.add_systems(Startup, warm_params);
        let path = crate::paths::root().join("extracted/combat_data.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "{} not found.\nRun tools/extract.ps1 first: it reads your own Sekiro install and writes this file.",
                path.display()
            )
        });
        let mut f: File = serde_json::from_str(&text).expect("combat_data.json is malformed; rerun tools/extract.ps1");
        // config enemy.chr picks the enemy (c1020 Samurai General by default): its own file from
        // `sekiro-extract npcs` (extracted/enemies/<chr>.json) when there is one, else the two
        // enemies combat_data.json carries. config enemy.group adds more kinds.
        let config = app.world().get_resource::<crate::config::GameConfig>();
        let (chr, npc_row) = config.map_or(("c1020".to_string(), None), |c| (c.enemy.chr.clone(), c.enemy.npc_row));
        let mut wanted = vec![(chr, npc_row)];
        for g in config.map(|c| c.enemy.group.clone()).unwrap_or_default() {
            wanted.push((g.chr, g.npc_row));
        }
        // The map's own characters (encounter.rs hooks: the Construct's courtyard guards, the
        // sandbox map's line-up).
        wanted.extend(config.map(crate::encounter::hooks().extra_chrs).unwrap_or_default());
        let mut kinds: Vec<EnemyKind> = Vec::new();
        for (chr, npc_row) in wanted {
            if kinds.iter().any(|k| k.foe.is(&chr, npc_row)) {
                continue;
            }
            let kind = load_kind(&mut f, &chr, npc_row);
            // Boss map events bring in other kinds (Genichiro -> Tomoe c7110).
            kinds.push(kind);
        }
        f.player.alias_missing_clips();
        let sculptor = config.and_then(crate::encounter::hooks().npc).and_then(|(chr, row)| {
            let kind = load_kind(&mut f, chr, Some(row));
            (kind.foe.chr == chr).then_some(kind)
        });
        let mut combat = Combat { player: f.player, sculptor, kinds, params: f.params, rumble: f.rumble, twists: f.twists, names: f.names };
        combat.add_event_kinds();
        app.insert_resource(combat);
    }
}

/// Loads one enemy kind: extracted/enemies/<chr>.json (its rows merged into `f.params`), else
/// one of the two enemies combat_data.json carries (else the Samurai General).
impl Combat {
    /// The kinds the first kind's boss map events enable (the Corrupted Monk's clones are her
    /// own rows; Genichiro's Way of Tomoe is c7110).
    fn add_event_kinds(&mut self) {
        for (chr, row) in crate::enemy::event_kinds(self, self.kinds[0].foe.npc_row) {
            if self.kind_index(&chr, Some(row)).is_none() {
                let kind = self.with_params_file(|f| load_kind(f, &chr, Some(row)));
                if kind.foe.chr == chr {
                    self.kinds.push(kind);
                }
            }
        }
    }

    /// Runs `op` on a File holding this Combat's params (load_kind merges an enemy's rows in).
    fn with_params_file<T>(&mut self, op: impl FnOnce(&mut File) -> T) -> T {
        let mut f = File { player: CharData::default(), enemy: CharData::default(), enemies: HashMap::new(), params: std::mem::take(&mut self.params), rumble: HashMap::new(), twists: HashMap::new(), names: Names::default() };
        let out = op(&mut f);
        self.params = f.params;
        out
    }

    /// Replaces the enemy kinds at runtime (the encounter picker, enemy.rs SpawnEncounter): the
    /// main kind, the group's and the kinds its boss events bring in, as at startup. Kind
    /// indices change, so every enemy must be respawned (SpawnEncounter does it through
    /// FightReset) and AnimLib::sync_kinds must follow.
    pub fn load_encounter(&mut self, chr: &str, npc_row: Option<i64>, group: &[crate::config::GroupEnemy]) {
        let wanted: Vec<(String, Option<i64>)> = std::iter::once((chr.to_string(), npc_row)).chain(group.iter().map(|g| (g.chr.clone(), g.npc_row))).collect();
        let mut kinds: Vec<EnemyKind> = Vec::new();
        for (c, r) in wanted {
            if kinds.iter().any(|k| k.foe.is(&c, r)) {
                continue;
            }
            let kind = self.with_params_file(|f| load_kind(f, &c, r));
            // A missing / unreadable enemies/<chr>.json comes back as an empty "c1020" here (the
            // startup fallback needs combat_data's General, which this File lacks): skip it, and
            // keep the current fight when the main enemy fails (round 24).
            if kind.foe.chr != c {
                warn!("encounter: no usable data for {c}; skipped");
                if kinds.is_empty() {
                    return;
                }
                continue;
            }
            kinds.push(kind);
        }
        self.kinds = kinds;
        self.add_event_kinds();
    }
}

/// One fight the encounter picker can start: an exported enemy with one of its placed NpcParam
/// rows (extracted/enemies/roster.json, `sekiro-extract npcs`). `boss`: the row has a boss
/// script (boss_scripts.json: its arena, events and phases); `deathblows`: the red dots.
#[derive(Clone, Debug)]
pub struct Encounter {
    pub chr: String,
    pub npc_row: i64,
    /// The chr's name and, after " - ", the placement's (when they differ).
    pub name: String,
    pub deathblows: i64,
    pub boss: bool,
    /// The game gives it a health bar (a boss or a mini-boss: its name is the bar's).
    pub bar: bool,
    /// The maps the row is placed in (m11_00_00_00 ...).
    pub maps: Vec<String>,
}

/// Every encounter, in roster order (chr id), one per placed NpcParam row. A boss or mini-boss
/// row is named by its health bar (extracted/enemies/npc_names.json, tools/npc_names.py: the
/// game's English NPC名 text); rows sharing a name get their place's (地名, in brackets), and
/// if that is shared too, a number (in row order: the Guardian Ape's headless row is "2").
pub fn encounter_list() -> Vec<Encounter> {
    let dir = crate::paths::root().join("extracted/enemies");
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let official = read("npc_names.json").unwrap_or_default();
    let bosses = read("boss_scripts.json").and_then(|v| v.as_object().map(|o| o.keys().cloned().collect::<std::collections::HashSet<_>>())).unwrap_or_default();
    let mut out = Vec::new();
    for e in read("roster.json").and_then(|v| v.as_array().cloned()).unwrap_or_default() {
        let Some(chr) = e["chr"].as_str() else { continue };
        if !dir.join(format!("{chr}.json")).exists() {
            continue;
        }
        let chr_name = e["name"].as_str().unwrap_or(chr);
        for p in e["placements"].as_array().into_iter().flatten() {
            let Some(row) = p["npc"].as_i64() else { continue };
            // (A row placed in two map setups is listed twice in the roster.)
            if out.iter().any(|o: &Encounter| o.chr == chr && o.npc_row == row) {
                continue;
            }
            // One fight, two phases (the user, 2026-10-15): the Old Dragons of the Tree (53000000)
            // hand over to the Divine Dragon on their fall (m25 12505885 -> 9318, enemy/script.rs),
            // so the Divine Dragon's row is not a fight of its own and the Old Dragons' is named
            // for both.
            if row == 52000000 {
                continue;
            }
            let pname = p["name"].as_str().unwrap_or("");
            let name = match official["rows"][row.to_string()].as_str() {
                Some(n) if row == 53000000 => format!("{n} / Divine Dragon"),
                Some(n) => n.to_string(),
                None if pname.is_empty() || pname == chr_name => chr_name.to_string(),
                None => format!("{chr_name} - {pname}"),
            };
            out.push(Encounter {
                chr: chr.to_string(),
                npc_row: row,
                name,
                deathblows: p["deathblows"].as_i64().unwrap_or(0),
                boss: bosses.contains(&row.to_string()),
                bar: official["rows"].get(row.to_string()).is_some(),
                maps: p["maps"].as_array().map(|m| m.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default(),
            });
        }
    }
    let count = |out: &[Encounter]| {
        let mut n: HashMap<String, usize> = HashMap::new();
        for e in out {
            *n.entry(e.name.clone()).or_default() += 1;
        }
        n
    };
    let shared = count(&out);
    for e in &mut out {
        if shared[&e.name] > 1 {
            if let Some(place) = e.maps.first().and_then(|m| official["places"][m].as_str()) {
                e.name = format!("{} ({place})", e.name);
            }
        }
    }
    let shared = count(&out);
    let mut seen: HashMap<String, usize> = HashMap::new();
    for e in &mut out {
        if shared[&e.name] > 1 {
            let k = seen.entry(e.name.clone()).or_default();
            *k += 1;
            if *k > 1 {
                e.name = format!("{} {k}", e.name);
            }
        }
    }
    out
}

fn load_kind(f: &mut File, chr: &str, npc_row: Option<i64>) -> EnemyKind {
        let (mut enemy, foe) = if let Some(file) = EnemyFile::load(chr) {
            let foe = file.foe(chr, npc_row);
            if let (Some(dst), Some(src)) = (f.params.as_object_mut(), file.params.as_object()) {
                for (table, rows) in src {
                    let t = dst.entry(table.clone()).or_insert_with(|| Value::Object(Default::default()));
                    if let (Some(t), Some(rows)) = (t.as_object_mut(), rows.as_object()) {
                        t.extend(rows.iter().map(|(k, v)| (k.clone(), v.clone())));
                    }
                }
            }
            (file.data, foe)
        } else {
            let mut foe = Foe::for_chr(chr);
            // config enemy.npc_row: another NpcParam row of the same character (outfit variant).
            if let Some(row) = npc_row {
                let family = foe.chr.get(1..4).and_then(|s| s.parse::<i64>().ok());
                match f.params.get("NpcParam").and_then(|t| t.get(row.to_string())) {
                    Some(r) if r["behaviorVariationId"].as_i64().map(|v| v / 100) == family => foe.npc_row = row,
                    Some(_) => warn!("enemy.npc_row {row} is not a {} row; using {}", foe.chr, foe.npc_row),
                    None => warn!("enemy.npc_row {row} not exported; using {}", foe.npc_row),
                }
            }
            // (Taken, not copied: only the first kind can use these. Every placed enemy has its
            // own extracted/enemies file, so this is the no-extract fallback only.)
            match f.enemies.remove(&foe.chr) {
                Some(e) if foe.chr == chr => (e, foe),
                _ => {
                    // Not exported (rerun the extractor): fall back to the Samurai General.
                    if chr != "c1020" {
                        warn!("enemy {chr} not exported (sekiro-extract npcs), using c1020");
                    }
                    (std::mem::take(&mut f.enemy), Foe::for_chr("c1020"))
                }
            }
        };
        let variation = f.params["NpcParam"][foe.npc_row.to_string()]["behaviorVariationId"].as_i64().unwrap_or(0);
        select_variation(&mut enemy, variation);
        let npc = &f.params["NpcParam"][foe.npc_row.to_string()];
        enemy.resident_gates = (0..32)
            .filter_map(|i| npc[format!("spEffectID{i}")].as_i64().filter(|v| *v > 0))
            .filter_map(|id| enemy.sp_effects.get(&id.to_string()).map(|s| s.state_info))
            .filter(|si| *si != 0)
            .collect();
        enemy.alias_missing_clips();
        EnemyKind { foe, data: enemy }
}

#[cfg(test)]
mod tests {
    //! Checks the Sekiro rules the game relies on, straight from combat_data.json.
    //! Needs tools/extract.ps1 to have been run.
    use super::*;

    fn load() -> File {
        let path = crate::paths::root().join("extracted/combat_data.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("run tools/extract.ps1 first")).unwrap()
    }

    /// Frames (30 fps) during which the deflect window (behaviour ref 203) is up.
    fn deflect_frames(d: &CharData, state: &str) -> Vec<u32> {
        let key = d.anim_key(state).unwrap();
        (0..60).filter(|f| d.has_ref(key, (*f as f32 + 0.5) / TAE_FPS, 203)).collect()
    }

    #[test]
    fn deflect_window_shrinks_with_spam() {
        let f = load();
        let d = &f.player;
        assert_eq!(deflect_frames(d, "StandToDeflectGuard").len(), 6);
        assert_eq!(deflect_frames(d, "StandToDeflectGuard2").len(), 3);
        assert_eq!(deflect_frames(d, "StandToDeflectGuard3").len(), 2);
        assert_eq!(deflect_frames(d, "StandToDeflectGuard4").len(), 0);
        assert_eq!(deflect_frames(d, "SprintToDeflectGuard").len(), 12);
    }

    #[test]
    fn guard_chain_window_is_open_for_18_frames() {
        let f = load();
        let d = &f.player;
        let key = d.anim_key("StandToDeflectGuard").unwrap();
        let open: Vec<u32> = (0..40).filter(|fr| d.has_ref(key, (*fr as f32 + 0.5) / TAE_FPS, 212)).collect();
        assert_eq!(open.first(), Some(&0));
        assert_eq!(open.len(), 18);
    }

    #[test]
    fn deflects_and_left_swings_route_to_the_reverse_slash() {
        let f = load();
        let d = &f.player;
        // ref 231 = SP_EF_REF_TAE_TRANSITION_GROUND_ATTACK_COMBO_1_REVERSE
        for s in ["StandDeflectEasySmall_V1_F", "GroundAttackCombo2", "DeflectGuardAttack", "DeflectGuardToStand"] {
            let k = d.anim_key(s).unwrap();
            assert!(d.has_ref(k, 1.5 / TAE_FPS, 231), "{s}");
        }
        assert!(d.anim_key("GroundAttackCombo1Reverse").is_some());
        let r = d.anim_key("GroundAttackCombo1ReverseRelease").unwrap();
        assert!(!d.attack_windows(r).is_empty());
    }

    #[test]
    fn release_attack_and_combo_routing() {
        let f = load();
        let d = &f.player;
        let c1 = d.anim_key("GroundAttackCombo1").unwrap();
        let release: Vec<u32> = (0..40).filter(|fr| d.has_ref(c1, (*fr as f32 + 0.5) / TAE_FPS, 208)).collect();
        assert_eq!((release[0], *release.last().unwrap()), (6, 14));
        let r1 = d.anim_key("GroundAttackCombo1Release").unwrap();
        assert!(d.has_ref(r1, 0.5 / TAE_FPS, 215), "release 1 routes to combo 2");
        let (ev, atk, judge) = d.attack_windows(r1).into_iter().find(|(_, a, _)| a.atk_stam > 0.0).unwrap();
        assert_eq!(judge, 10);
        assert_eq!(((ev.start * TAE_FPS).round(), (ev.end * TAE_FPS).round()), (7.0, 11.0));
        assert_eq!(atk.atk_stam, 15.0);
    }

    #[test]
    fn root_motion_speeds() {
        let f = load();
        let d = &f.player;
        let walk = d.root_velocity("a000_000100").length();
        let run = d.root_velocity("a000_000400").length();
        assert!((walk - 1.62).abs() < 0.02, "walk {walk}");
        assert!((run - 5.29).abs() < 0.02, "run {run}");
        let step = d.anim_key("GroundStep_F").unwrap();
        let (p, _) = d.root_at(step, d.length(step)).unwrap();
        assert!((p.length() - 3.99).abs() < 0.02, "step {}", p.length());
    }

    #[test]
    fn the_chr_scripts_end_chains_parse() {
        if !crate::paths::extracted().join("hks_src/c1500.lua").exists() {
            return;
        }
        // The zombie's held pose 20010 -> 21015; 20004 -> 21008 only with ref 1000021 (a100).
        let z = hks_end_chains("c1500");
        assert!(z.contains(&EndChain { from: 20010, to: EndTo::Event(21015), refs: vec![], at_end: true, grounded: None }));
        assert!(z.contains(&EndChain { from: 20004, to: EndTo::Event(21008), refs: vec![(1_000_021, true)], at_end: true, grounded: None }));
        // c1190 20000 -> 21001 under ref 1000023; its else (W_GuardIdle) is no event chain.
        assert_eq!(hks_end_chains("c1190").iter().filter(|c| c.from == 20000).collect::<Vec<_>>(), vec![&EndChain { from: 20000, to: EndTo::Event(21001), refs: vec![(1_000_023, true)], at_end: true, grounded: None }]);
        // An else of an env check: c1010 20010 -> 21001 without ref 1000024.
        assert!(hks_end_chains("c1010").contains(&EndChain { from: 20010, to: EndTo::Event(21001), refs: vec![(1_000_024, false)], at_end: true, grounded: None }));
    }
}

/// An FMG (Sekiro: version 2, little endian, UTF-16 strings): id -> text.
pub fn read_fmg(d: &[u8]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let i32_ = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let i64_ = |o: usize| d.get(o..o + 8).map(|b| i64::from_le_bytes(b.try_into().unwrap()));
    let (Some(groups), Some(offsets)) = (i32_(0x0C), i64_(0x18)) else { return out };
    for g in 0..groups.max(0) as usize {
        let at = 0x28 + g * 16;
        let (Some(index), Some(first), Some(last)) = (i32_(at), i32_(at + 4), i32_(at + 8)) else { break };
        for k in 0..=(last - first).max(-1) {
            let Some(o) = i64_(offsets as usize + (index + k) as usize * 8) else { break };
            if o <= 0 {
                continue;
            }
            let mut e = o as usize;
            let mut units = Vec::new();
            while let Some(b) = d.get(e..e + 2) {
                let u = u16::from_le_bytes([b[0], b[1]]);
                if u == 0 {
                    break;
                }
                units.push(u);
                e += 2;
            }
            out.insert((first + k) as u32, String::from_utf16_lossy(&units));
        }
    }
    out
}
