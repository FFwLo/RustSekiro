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
    #[serde(default)]
    pub def_stamina_attack_rate: f32,
    #[serde(default = "one")]
    pub stamina_recover_speed_rate: f32,
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
    /// spEffectId0-4: SpEffects the hit puts on the target (Sabimaru 7500100 -> 9004 poison build-up).
    #[serde(default = "minus_one", rename = "spEffectId0")]
    pub sp_effect_id0: i64,
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
    #[serde(default)]
    pub direct_atk_stam_damage: f32,
    #[serde(default)]
    /// AtkParam staminaPhysicsAttribute (1 slash, 2 strike / lightHit, 3 thrust, 4 neutral, 6 heavyHit):
    /// picks the defender's def<Attr>StaminaDmgRate (latent skill Flowing Water: slash 0.8, others 0.75).
    #[serde(rename = "staminaPhysicsAttribute")]
    pub stamina_physics_attribute: i64,
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
    /// Perilous attacks: cannot be blocked (1) / cannot be deflected either (1).
    #[serde(rename = "disableGuard_vsGuardAttribute0", default)]
    pub disable_guard: i64,
    #[serde(rename = "disableJustGuard_vsGuardAttribute0", default)]
    pub disable_just_guard: i64,
    /// 0 slash, 1 strike, 2 thrust.
    #[serde(default)]
    pub atk_type: i64,
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
    #[serde(default, rename = "attackBasePhysics")]
    pub attack_base_physics: f32,
    #[serde(default, rename = "attackBaseFire")]
    pub attack_base_fire: f32,
}

/// A Bullet param row (projectile) and its AtkParam.
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
    #[serde(default)]
    #[allow(dead_code)] // kept for debugging / future use
    pub max_vellocity: f32,
    /// Acceleration / gravity beyond `dist` metres.
    #[serde(default)]
    pub accel_out_range: f32,
    #[serde(default)]
    pub gravity_out_range: f32,
    #[serde(default)]
    pub dist: f32,
    #[serde(default)]
    pub life: f32,
    #[serde(default)]
    pub hit_radius: f32,
    #[serde(default)]
    pub lock_shoot_limit_ang: f32,
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
    pub enemy: CharData,
    /// Which enemy `enemy` is (config enemy.chr) and its param rows.
    pub foe: Foe,
    pub params: Value,
    /// Camera shakes by RumbleCam id (other/default.rumblebnd).
    pub rumble: HashMap<String, Rumble>,
    /// Wolf's TAE 700 twist modifiers by name ("0_TwistUD", "100_Attack", ...).
    pub twists: HashMap<String, Twist>,
    /// Official names (combat arts, prosthetic tools, skills).
    pub names: Names,
}

impl Combat {
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
}

impl Foe {
    pub fn new(chr: &str, anim_chr: &str, npc_row: i64, think_id: i64) -> Foe {
        let chr_num = chr.get(1..).and_then(|n| n.parse().ok()).unwrap_or(1020);
        Foe { chr: chr.to_string(), anim_chr: anim_chr.to_string(), npc_row, think_id, chr_num, common_parry: None }
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
        let goal = self.params["NpcThinkParam"][think.to_string()]["battleGoalID"].as_i64().unwrap_or(0);
        foe.common_parry = Foe::read_common_parry(goal);
        foe
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
        })
    }

    /// The enemy's grab for a throwFlag-1 hit: its ThrowParam row (AtkChrId = the chr number)
    /// whose throwKind is 1000000 + AtkParam throwTypeId * 10 (c1500 zombies: knife stab 0 ->
    /// 21500000 ThrowAtk4100, eye gouge 1 -> 21500100 4110, restraint 2 -> 21500200 4120; the
    /// General's 0 -> 21020000). gap: the exe's row match is not traced; this pattern holds for
    /// every enemy grab row.
    pub fn enemy_grab(&self, throw_type: i64) -> Option<Throw> {
        let rows = self.params.get("ThrowParam")?.as_object()?;
        let kind = 1_000_000 + throw_type * 10;
        let id = rows
            .iter()
            .filter(|(_, r)| r["AtkChrId"].as_i64() == Some(self.foe.chr_num) && r["throwKind"].as_i64() == Some(kind))
            .filter_map(|(k, _)| k.parse::<i64>().ok())
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

    /// Is a ChrActionFlag (TAE type 0) of this FlagType active?
    pub fn flag(&self, key: &str, t: f32, flag_type: i64) -> bool {
        self.events_at(key, t).any(|e| e.kind == 0 && e.flag_type() == Some(flag_type))
    }

    /// SpEffects applied by the anim's TAE (type 67, and 401) at time t.
    pub fn sp_effects_at(&self, key: &str, t: f32) -> Vec<(i64, &SpEffect)> {
        self.events_at(key, t)
            .filter(|e| e.kind == 67 || e.kind == 401)
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

    /// TAE 922 ChrPhysicsVelosityScale starting in (t0, t1] (its row: arg ChrPhysicsVelocityParam ID).
    pub fn velocity_scale(&self, key: &str, t0: f32, t1: f32) -> Option<&Event> {
        self.anim(key)?.events.iter().find(|e| e.kind == 922 && e.start > t0 && e.start <= t1)
    }

    /// Perilous-attack warning (the red kanji): a TAE BulletBehavior (type 2) with
    /// judge 980-989 starting in (t0, t1]. 980 sweep, 982 thrust, 983 grab.
    pub fn perilous_warning(&self, key: &str, t0: f32, t1: f32) -> Option<i64> {
        self.anim(key)?
            .events
            .iter()
            .filter(|e| e.kind == 2 && ((e.start > t0 && e.start <= t1) || (t0 == 0.0 && e.start == 0.0)))
            .filter_map(|e| e.arg_i64("BehaviorJudgeID"))
            .find(|j| (980..990).contains(j))
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
        let anim_key = key;
        self.anim(key)
            .into_iter()
            .flat_map(|a| a.events.iter())
            .filter(|e| (e.kind == 1 || e.kind == 307) && self.fires(e))
            .filter_map(|e| {
                let j = e.arg_i64("BehaviorJudgeID")?;
                // PCBehavior (307, e.g. the air kick) resolves straight to BehaviorParam_PC "pc<judge>".
                let key = if e.kind == 307 { format!("pc{j}") } else { j.to_string() };
                // Combat-art anims (a100..a110) resolve through their art's variation first.
                let art = anim_key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| (100..=110).contains(g));
                let var = self.art_variation.and_then(|v| self.attacks.get(&format!("v{v}:{j}")));
                let tool = anim_key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| (70..=79).contains(g));
                let tool_atk = tool.and(self.tool_variation).and_then(|v| self.attacks.get(&format!("v{v}:{j}")));
                let atk = art.and_then(|g| var.or_else(|| self.attacks.get(&format!("a{g}:{j}")))).or(tool_atk).or_else(|| self.attacks.get(&key));
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
        // enemies combat_data.json carries.
        let (chr, npc_row) = app
            .world()
            .get_resource::<crate::config::GameConfig>()
            .map_or(("c1020".to_string(), None), |c| (c.enemy.chr.clone(), c.enemy.npc_row));
        let (mut enemy, foe) = if let Some(file) = EnemyFile::load(&chr) {
            let foe = file.foe(&chr, npc_row);
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
            let mut foe = Foe::for_chr(&chr);
            // config enemy.npc_row: another NpcParam row of the same character (outfit variant).
            if let Some(row) = npc_row {
                let family = foe.chr.get(1..4).and_then(|s| s.parse::<i64>().ok());
                match f.params.get("NpcParam").and_then(|t| t.get(row.to_string())) {
                    Some(r) if r["behaviorVariationId"].as_i64().map(|v| v / 100) == family => foe.npc_row = row,
                    Some(_) => warn!("enemy.npc_row {row} is not a {} row; using {}", foe.chr, foe.npc_row),
                    None => warn!("enemy.npc_row {row} not exported; using {}", foe.npc_row),
                }
            }
            match f.enemies.remove(&foe.chr) {
                Some(e) if foe.chr == chr => (e, foe),
                _ => {
                    // Not exported (rerun the extractor): fall back to the Samurai General.
                    if chr != "c1020" {
                        warn!("enemy {chr} not exported (sekiro-extract npcs), using c1020");
                    }
                    (f.enemy, Foe::for_chr("c1020"))
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
        f.player.alias_missing_clips();
        enemy.alias_missing_clips();
        app.insert_resource(Combat { player: f.player, enemy, foe, params: f.params, rumble: f.rumble, twists: f.twists, names: f.names });
    }
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
}
