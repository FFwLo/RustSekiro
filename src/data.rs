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
        t >= self.start && t < self.end && self.ungated()
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
    #[serde(default)]
    pub def_stamina_attack_rate: f32,
    #[serde(default = "one")]
    pub stamina_recover_speed_rate: f32,
    /// Active only while HP% <= this (resident HP-conditional effects); -1 or 0 = always.
    #[serde(default)]
    pub condition_hp: f32,
}

fn one() -> f32 {
    1.0
}

/// SpEffect stateInfo 158: "just guard" (deflect) for both player (105010) and NPCs (200220).
pub const STATE_INFO_JUST_GUARD: i64 = 158;

fn minus_one() -> i64 {
    -1
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Attack {
    #[serde(default)]
    #[allow(dead_code)] // kept for debugging / future use
    pub behavior_name: String,
    #[serde(default)]
    pub atk_phys: f32,
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
    /// Damage level as the HKS reads it (dmgLevel_vsPlayer overrides against the player).
    pub fn level(&self, vs_player: bool) -> i64 {
        if vs_player && self.dmg_level_vs_player > 0 { self.dmg_level_vs_player } else { self.dmg_level }
    }
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
    /// Prosthetic bullets by TAE BulletBehavior judge (Bullet param + its AtkParam).
    #[serde(default)]
    pub bullets: HashMap<String, BulletSpec>,
    /// Ragdoll body capsules (chrbnd physics HKX), Havok model space, bind pose.
    #[serde(default)]
    pub hurtboxes: Vec<Hurtbox>,
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

/// A Bullet param row (projectile) and its AtkParam.
#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BulletSpec {
    #[serde(default)]
    pub init_vellocity: f32,
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
}

/// The enemy in play: chr id plus the rows its data lives in.
#[derive(Clone, Debug)]
pub struct Foe {
    pub chr: String,
    /// NpcParam row (stats, guard, knockback, cut rates).
    pub npc_row: i64,
    /// NpcThinkParam row and its battleGoalID (the Lua battle script).
    pub think_id: i64,
    pub battle_goal: i64,
    /// ThrowParam rows "PC -> this enemy" are 11000000 + n * 1000 + suffix (0001 posture
    /// break, 0010 deflect break, 0190 mikiri); the enemy grab is 21000000 + n * 1000.
    pub throw_n: i64,
    /// Passive-mode parry: None = the General's own Goal.Parry (102000_battle.lua), Some = the
    /// shared Common_Parry(guardMult, stepProb, stepType, rushAnim) its script calls.
    pub common_parry: Option<(i32, i32, i32, &'static str)>,
}

impl Foe {
    /// Known enemies (NpcParam / NpcThinkParam rows read from the params; see PROGRESS.md).
    pub fn for_chr(chr: &str) -> Foe {
        let (npc_row, think_id, battle_goal, throw_n, common_parry) = match chr {
            // 101000_battle.lua Goal.Interrupt: Common_Parry(ai, goal, 50, 25, 0, 3102).
            "c1010" => (10100000, 10100000, 101000, 10, Some((50, 25, 0, "a000_003102"))),
            // NpcParam 10203010: the regular General (behaviorVariationId 10200, the variant whose
            // attacks the export uses) as fought live (rec_20261008_054259). 10219000 was an
            // Ashina Shitenno sample row (variation 10201, another outfit).
            _ => (10203010, 10200000, 102000, 20, None),
        };
        Foe { chr: if chr == "c1010" { chr.to_string() } else { "c1020".to_string() }, npc_row, think_id, battle_goal, throw_n, common_parry }
    }
    pub fn throw_row(&self, suffix: i64) -> i64 {
        11_000_000 + self.throw_n * 1_000 + suffix
    }
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
        })
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
        self.anim(key).into_iter().flat_map(|a| a.events.iter()).filter(move |e| e.active(t))
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
            .filter(|e| (e.kind == 1 || e.kind == 307) && e.ungated())
            .filter_map(|e| {
                let j = e.arg_i64("BehaviorJudgeID")?;
                // PCBehavior (307, e.g. the air kick) resolves straight to BehaviorParam_PC "pc<judge>".
                let key = if e.kind == 307 { format!("pc{j}") } else { j.to_string() };
                // Combat-art anims (a100..a110) resolve through their art's variation first.
                let art = anim_key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| (100..=110).contains(g));
                let var = self.art_variation.and_then(|v| self.attacks.get(&format!("v{v}:{j}")));
                let atk = art.and_then(|g| var.or_else(|| self.attacks.get(&format!("a{g}:{j}")))).or_else(|| self.attacks.get(&key));
                atk.map(|a| (e, a, if e.kind == 307 { 1_000_000 + j } else { j }))
            })
            .collect()
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
        // config enemy.chr picks the enemy (c1020 Samurai General by default).
        let chr = app.world().get_resource::<crate::config::GameConfig>().map_or("c1020".to_string(), |c| c.enemy.chr.clone());
        let mut foe = Foe::for_chr(&chr);
        // config enemy.npc_row: another NpcParam row of the same character (outfit variant).
        if let Some(row) = app.world().get_resource::<crate::config::GameConfig>().and_then(|c| c.enemy.npc_row) {
            // Only a row of this character (behaviorVariationId 102xx for c1020, 101xx for c1010).
            let family = foe.chr.get(1..4).and_then(|s| s.parse::<i64>().ok());
            match f.params.get("NpcParam").and_then(|t| t.get(row.to_string())) {
                Some(r) if r["behaviorVariationId"].as_i64().map(|v| v / 100) == family => foe.npc_row = row,
                Some(_) => warn!("enemy.npc_row {row} is not a {} row; using {}", foe.chr, foe.npc_row),
                None => warn!("enemy.npc_row {row} not exported; using {}", foe.npc_row),
            }
        }
        let mut enemy = match f.enemies.remove(&foe.chr) {
            Some(e) => e,
            None => {
                // Not exported (rerun the extractor): fall back to the Samurai General.
                if foe.chr != "c1020" {
                    warn!("enemy {} not in combat_data.json, using c1020", foe.chr);
                }
                foe = Foe::for_chr("c1020");
                f.enemy
            }
        };
        f.player.alias_missing_clips();
        enemy.alias_missing_clips();
        app.insert_resource(Combat { player: f.player, enemy, foe, params: f.params, rumble: f.rumble, twists: f.twists });
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
