//! Status build-up: burn (the Flame Vent) and poison (the Sabimaru) on enemies, and the enemies'
//! poison / burn / terror / Aging on Wolf (Statuses::hit_wolf). A hit's
//! SpEffects (AtkParam / Bullet spEffectId0-4) carry the build-up (Paramdex SDT SpEffect meta):
//! poizonAttackPower with stateInfo 2 (poison), registBlood with stateInfo 6 (burn, Sekiro's
//! "blood" slot). When a gauge reaches the NpcParam resist_poison / resist_blood the effect takes
//! hold: it hurts every motionInterval s (changeHpRate % of max HP + changeHpPoint) for its
//! effectEndurance, then its replaceSpEffectId follows (Sabimaru 9004 -> 9045 poison damage).
//!
//! The exe keeps this in SprjChrResistModule (vtable 0x142a8d178): per type (0 poison, 1
//! disease, 2 blood / burn, 3 curse, 4 freeze) the resist left (+0x10) and its max (+0x24). A hit
//! (FUN_140a00f60) takes the build-up off unless an active SpEffect has disablePoison /
//! disableBlood (FUN_140bff7a0 -> FUN_140c05f20: SpEffect +0x16b bits 2 / 4); below 1 the effect
//! fires, the type's "activation" bit is set for the next frame (+0x4c -> +0x50, read by HKS
//! env(3041) GetStatusActivation) and the resist refills to max (FUN_140bd9d30). Every frame
//! (FUN_140bda460) it recovers TentativePlayerParam ResistRecoverPoint_*_Enemy (_Player for
//! Wolf; 5 a second for poison and burn) times dt, whole points only, the fraction carried
//! (+0x38). Here the gauge counts the build-up up instead: gauge = max - left.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet};
use crate::hud::CombatLog;
use crate::data::{Combat, SpEffect};

/// SpEffect stateInfos of the build-ups (SP_EFFECT_TYPE: 2 poison, 6 burn).
const STATE_INFO_POISON: i64 = 2;
const STATE_INFO_BURN: i64 = 6;
/// Terror (SP_EFFECT_TYPE 5, registIllness): the exe's resist type 1 "disease".
const STATE_INFO_TERROR: i64 = 5;
/// Aging (Paramdex: stateInfo 116 "Enfeeble", registCurse "only works with stateInfo 116"): the
/// exe's resist type 3 "curse". Its one build-up is 9600 "Aging_resistant scraping" (registCurse
/// 40), the Fountainhead strong noble c1300's breath Bullet 13000200; full, it applies its
/// cycleOccurrenceSpEffectId 9620 "Aging_invoking" (30 s, maxHpRate 0.125, behaviorRefId 591
/// SP_EF_REF_AGING: c0000's aged form). gap: the fill -> cycleOccurrence step is inferred from
/// the rows (the exe's use of cycleOccurrence on a fill is not traced).
const STATE_INFO_CURSE: i64 = 116;
/// Shock (Paramdex: stateInfo 260 with registFreeze, the exe's resist type 4 "freeze"): the
/// Lightning Reversal's 9400 (AtkParam_Pc 5000281 / 5030212, effectEndurance 0) fires its
/// cycleOccurrence 9420 (registFreeze 200 vs NpcParam resist_freeze, 100 on Genichiro / the Way
/// of Tomoe) as its one cycle; full, 9420's changeHpRate 67 % + changeHpPoint 320, scaled by the
/// defender's worn freezeDamageRate % (the Way of Tomoe's 9461: 25 -> ~a fifth of her 2556),
/// and its changeStaminaPoint -180 / changeStaminaRate -5 % as posture damage; then its replace
/// 9445 (1.5 s) and cycle 9450 (behaviorRefId 593). The enemy lightning's 9410-9415 -> 9430-9435
/// are the same chain on Wolf (player.rs lightning_shock applies them at once: his resist 200 =
/// their registFreeze 200). Before, 9400 reached `start` as a 0 s row and did nothing: the
/// reversal was a 0-damage hit (boss bug hunt 2026-10-14). gap: whether the posture fields scale
/// with freezeDamageRate too is not traced (taken as HP only, the field's name).
const STATE_INFO_FREEZE: i64 = 260;
/// Wolf's resist max, every type (exe PlayerIns vtable +0x1d0 FUN_140a289a0 -> FUN_140847a00 ->
/// FUN_1408457f0: CalcCorrectGraph 110-114 of the sum of his nine stats, flat 200 from stat 1
/// on, plus 120-124 of one stat, 0 everywhere). gap: the protectors' resist* (0 on all but two
/// rows) and SpEffect resist rates / points (FUN_140bfdbc0 / FUN_140bfdb60) are not added.
pub const WOLF_RESIST: f32 = 200.0;

/// How the hit met the defender: the guard keeps EquipParamWeapon *GuardResist % of the build-up
/// off, a deflect *JustGuardResist % (Wolf's sword rows 5000-7700: burn 33 / 50, poison 0).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Guard {
    None,
    Block,
    Deflect,
}

/// HKS behaviorRefIds of the afflictions (c0000_define / c9997: SP_EF_REF_POISON 500,
/// SP_EFFECT_REF_BURNING 1000130).
pub const REF_POISON: i64 = 500;
pub const REF_BURNING: i64 = 1000130;
/// SP_EF_REF_ENABLE_INSANE: the terror rows 9204-9223 (all instant, stateInfo 5).
pub const REF_ENABLE_INSANE: i64 = 590;
/// SP_EF_REF_AGING (c0000_define.lua:745): 9620 "Aging_invoking".
#[allow(dead_code)]
pub const REF_AGING: i64 = 591;
/// SP_EF_REF_FIRE_ADD_DAMAGE_BURN; Actor.death_ref also takes it for a killing fire hit
/// (env 285 DAMAGE_ELEMENT_FIRE, combat.rs).
pub const REF_FIRE_ADD_DAMAGE_BURN: i64 = 514;

/// One applied SpEffect: seconds left and to the next damage tick, and its effects on the body
/// (SpEffectVfxParam midstSfxId, stopped when it ends).
#[derive(Clone, Debug)]
pub struct Affliction {
    pub id: i64,
    pub left: f32,
    pub next_tick: f32,
    shown: bool,
    fx: Vec<Entity>,
}

#[cfg(test)]
impl Affliction {
    pub(crate) fn fx_mut(&mut self) -> &mut Vec<Entity> {
        &mut self.fx
    }
}

#[derive(Default, Debug)]
pub struct Status {
    pub poison: f32,
    pub burn: f32,
    /// Terror (round 36: c1500's dumplings built none). A full gauge applies the row once: its
    /// changeHpRate 100 % of max HP (effectEndurance 0) - the terror death.
    pub terror: f32,
    /// Aging (the exe's "curse"): 9600's registCurse against the resist; full -> 9620.
    pub curse: f32,
    /// Shock (the exe's "freeze"): 9420's registFreeze against resist_freeze; full -> its payload.
    pub freeze: f32,
    pub effects: Vec<Affliction>,
    /// The fractions of a recovered point carried to the next frame (+0x38): poison, burn, terror,
    /// curse, freeze.
    carry: [f32; 5],
    /// changeHpRate % of max HP of instant rows (effectEndurance 0) taken this frame by `tick`,
    /// with the last such row's behaviorRefId (its death anim) and name.
    instant_hp_rate: f32,
    /// Flat HP (changeHpPoint) and posture (-changeStaminaPoint - changeStaminaRate % of max) of
    /// instant rows taken this frame (the shock 9420 / 9430-9435).
    instant_hp_point: f32,
    instant_posture_point: f32,
    instant_posture_rate: f32,
    instant_ref: i64,
    instant_name: &'static str,
    /// Instant rows filled this frame whose effects `show` plays once (round 57: terror's
    /// 48020 init 5013, the kanji on dummy 280, never showed; `start` keeps timed rows only).
    pub(crate) flash: Vec<i64>,
}

impl Status {
    /// Whether an applied effect has HKS behaviorRefId `behavior_ref` (REF_POISON / REF_BURNING).
    pub fn afflicted(&self, combat: &Combat, behavior_ref: i64) -> bool {
        self.effects.iter().any(|a| sp(combat, a.id).is_some_and(|s| s.behavior_ref_id == behavior_ref))
    }
}

/// TentativePlayerParam row 0 ResistRecoverPoint_Poision / _Blood / _Desease / _Curse (points a
/// second) for enemies and for Wolf, the SpEffectParam rows' disablePoison / disableBlood /
/// disableCurse flags, and the curse build-ups (registCurse * registCurseChangeRate,
/// cycleOccurrenceSpEffectId), which the exported rows don't carry.
#[derive(Default)]
pub struct ResistRules {
    pub recover_enemy: [f32; 5],
    pub recover_player: [f32; 5],
    disable: HashMap<i64, [bool; 5]>,
    curse: HashMap<i64, (f32, i64)>,
    /// The shock build-ups: a row -> (its payload row, registFreeze * registFreezeChangeRate).
    /// The payload row itself (stateInfo 260 with registFreeze: 9420 / 9430-9435) or an instant
    /// row (effectEndurance 0) whose cycleOccurrence is one (9400 -> 9420, 9410-9415 -> 9430-9435).
    freeze: HashMap<i64, (i64, f32)>,
    /// A payload row's changeHpRate, changeHpPoint, changeStaminaPoint, changeStaminaRate,
    /// behaviorRefId, cycleOccurrenceSpEffectId.
    freeze_payload: HashMap<i64, [f32; 6]>,
    /// freezeDamageRate / 100 of the rows that have one other than 100 (9460: 0, 9461: 0.25...).
    freeze_rate: HashMap<i64, f32>,
}

impl ResistRules {
    /// Read once from extracted/json/params.
    pub fn get() -> &'static Self {
        static RULES: std::sync::OnceLock<ResistRules> = std::sync::OnceLock::new();
        RULES.get_or_init(Self::load)
    }

    fn load() -> Self {
        let read = |name: &str| -> Vec<serde_json::Value> {
            let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
            let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Vec::new() };
            match &v["rows"] {
                serde_json::Value::Array(a) => a.clone(),
                serde_json::Value::Object(o) => o.values().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let tent = read("TentativePlayerParam");
        let row = tent.iter().find(|r| r["id"].as_i64() == Some(0));
        let f = |k: &str| row.and_then(|r| r[k].as_f64()).unwrap_or(0.0) as f32;
        let sps = read("SpEffectParam");
        let disable = sps
            .iter()
            .filter_map(|r| {
                let on = |k: &str| r[k].as_i64() == Some(1);
                let d = [on("disablePoison"), on("disableBlood"), on("disableDisease"), on("disableCurse"), on("disableFreeze")];
                d.contains(&true).then(|| (r["id"].as_i64().unwrap_or(-1), d))
            })
            .collect();
        let curse = sps
            .iter()
            .filter(|r| r["stateInfo"].as_i64() == Some(STATE_INFO_CURSE) && r["registCurse"].as_f64().is_some_and(|v| v > 0.0))
            .map(|r| {
                let power = r["registCurse"].as_f64().unwrap_or(0.0) * r["registCurseChangeRate"].as_f64().unwrap_or(1.0);
                (r["id"].as_i64().unwrap_or(-1), (power as f32, r["cycleOccurrenceSpEffectId"].as_i64().unwrap_or(-1)))
            })
            .collect();
        let freeze_payload: HashMap<i64, [f32; 6]> = sps
            .iter()
            .filter(|r| r["stateInfo"].as_i64() == Some(STATE_INFO_FREEZE) && r["registFreeze"].as_f64().is_some_and(|v| v > 0.0))
            .filter_map(|r| {
                let f = |k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
                Some((r["id"].as_i64()?, [f("changeHpRate"), f("changeHpPoint"), f("changeStaminaPoint"), f("changeStaminaRate"), f("behaviorRefId"), f("cycleOccurrenceSpEffectId")]))
            })
            .collect();
        let power = |r: &serde_json::Value| (r["registFreeze"].as_f64().unwrap_or(0.0) * r["registFreezeChangeRate"].as_f64().unwrap_or(1.0)) as f32;
        let by_id: HashMap<i64, &serde_json::Value> = sps.iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect();
        let freeze = sps
            .iter()
            .filter_map(|r| {
                let id = r["id"].as_i64()?;
                if freeze_payload.contains_key(&id) {
                    return Some((id, (id, power(r))));
                }
                let cycle = r["cycleOccurrenceSpEffectId"].as_i64().filter(|c| freeze_payload.contains_key(c))?;
                (r["effectEndurance"].as_f64() == Some(0.0)).then(|| (id, (cycle, power(by_id[&cycle]))))
            })
            .collect();
        let freeze_rate = sps
            .iter()
            .filter_map(|r| {
                let rate = r["freezeDamageRate"].as_f64().filter(|v| *v != 100.0)?;
                Some((r["id"].as_i64()?, rate as f32 / 100.0))
            })
            .collect();
        Self {
            freeze,
            freeze_payload,
            freeze_rate,
            recover_enemy: [f("ResistRecoverPoint_Poision_Enemy"), f("ResistRecoverPoint_Blood_Enemy"), f("ResistRecoverPoint_Desease_Enemy"), f("ResistRecoverPoint_Curse_Enemy"), f("ResistRecoverPoint_Freeze_Enemy")],
            recover_player: [f("ResistRecoverPoint_Poision_Player"), f("ResistRecoverPoint_Blood_Player"), f("ResistRecoverPoint_Desease_Player"), f("ResistRecoverPoint_Curse_Player"), f("ResistRecoverPoint_Freeze_Player")],
            disable,
            curse,
        }
    }

    /// An active SpEffect stops this type's build-up (0 poison disablePoison, 1 burn
    /// disableBlood, 2 terror disableDisease, 3 curse disableCurse): an affliction of `status`
    /// or one the character wears (`worn`: NpcParam residents such as 90111 "Poison invalid",
    /// 90011 poison and flame, 91011 fire; round 57: before, only the afflictions counted and a
    /// zombie's 999 poison gauge filled up).
    fn disabled(&self, status: &Status, worn: &[i64], ty: usize) -> bool {
        status.effects.iter().map(|a| a.id).chain(worn.iter().copied()).any(|id| self.disable.get(&id).is_some_and(|d| d[ty]))
    }
}

/// The resist recovery of one frame (FUN_140bda460): whole points of rate * dt + the carried
/// fraction come off the gauge (not below 0), the rest is carried.
pub fn recover(gauge: &mut f32, carry: &mut f32, rate: f32, dt: f32) {
    let v = rate * dt + *carry;
    let whole = v.trunc();
    *carry = v - whole;
    *gauge = (*gauge - whole).max(0.0);
}

/// Every character's build-up gauges and afflictions; `stop`: effects of ended afflictions.
#[derive(Resource, Default)]
pub struct Statuses(pub HashMap<Entity, Status>, Vec<Entity>);

impl Statuses {
    /// The body effects waiting to be stopped.
    #[cfg(test)]
    pub(crate) fn stopping(&self) -> &[Entity] {
        &self.1
    }

    /// Test hook: applies SpEffect `id` to `e` as a full build-up would.
    #[cfg(test)]
    pub(crate) fn start_for(&mut self, combat: &Combat, e: Entity, id: i64) {
        let Statuses(all, stop) = self;
        start(combat, all.entry(e).or_default(), id, stop);
    }

    /// A new fight: every gauge and affliction goes, their effects on the body stop (Wolf keeps
    /// his entity across resets, so his entry would carry the poison into the next fight).
    pub fn clear_all(&mut self) {
        let Statuses(all, stop) = self;
        stop.extend(all.drain().flat_map(|(_, s)| s.effects.into_iter().flat_map(|a| a.fx)));
    }

    /// A cure (items.rs: Antidote Powder, Dousing Powder, Pacifying Agent, Divine Grass): the
    /// gauges of `types` (0 poison, 1 burn, 2 terror) empty and their afflictions end.
    pub fn cure(&mut self, e: Entity, types: [bool; 3]) {
        let Statuses(all, stop) = self;
        let Some(status) = all.get_mut(&e) else { return };
        for (ty, gauge) in [&mut status.poison, &mut status.burn, &mut status.terror].into_iter().enumerate() {
            if types[ty] {
                *gauge = 0.0;
            }
        }
        let info = [STATE_INFO_POISON, STATE_INFO_BURN, STATE_INFO_TERROR];
        let mut kept = Vec::new();
        for a in std::mem::take(&mut status.effects) {
            let s = crate::items::param("SpEffectParam", a.id)["stateInfo"].as_i64().unwrap_or(0);
            if (0..3).any(|t| types[t] && s == info[t]) {
                stop.extend(a.fx);
            } else {
                kept.push(a);
            }
        }
        status.effects = kept;
    }

    /// A character's new life (an enemy's in-place respawn): its gauges and afflictions go,
    /// their effects on the body stop.
    pub fn clear(&mut self, e: Entity) {
        let Statuses(all, stop) = self;
        stop.extend(all.remove(&e).into_iter().flat_map(|s| s.effects.into_iter().flat_map(|a| a.fx)));
    }

    /// An affliction with this behaviorRefId is on the character (HKS env(3036, ref)).
    #[allow(dead_code)]
    pub fn has_ref(&self, combat: &Combat, e: Entity, behavior_ref: i64) -> bool {
        self.0.get(&e).is_some_and(|s| s.effects.iter().any(|a| sp(combat, a.id).is_some_and(|x| x.behavior_ref_id == behavior_ref)))
    }

    /// Wolf's hit on an enemy: each of its SpEffects adds its build-up; a full gauge applies the
    /// effect. `guarded`: the NpcParam *GuardResist % of it is kept off. Returns the log lines.
    pub fn hit(&mut self, combat: &Combat, target: Entity, def: &crate::actor::Actor, sps: &[i64], guarded: bool) -> Vec<String> {
        let npc = combat.npc(def);
        let num = |k: &str| npc[k].as_f64().unwrap_or(0.0) as f32;
        let guard = if guarded { [num("poisonGuardResist"), num("bloodGuardResist"), num("diseaseGuardResist"), num("curseGuardResist"), num("freezeGuardResist")] } else { [0.0; 5] };
        self.build_up(combat, target, &crate::combat::worn_sp(def, &[]), sps, [num("resist_poison"), num("resist_blood"), num("resist_desease"), num("resist_curse"), num("resist_freeze")], guard)
    }

    /// An enemy's hit (or bullet) on Wolf: as `hit`, against WOLF_RESIST, the guard's share kept
    /// off by his sword `weapon` (EquipParamWeapon, the equipped combat art's row).
    /// `worn`: his quick items' rows (Actor::item_sp): their resist rates and freezeDamageRate.
    pub fn hit_wolf(&mut self, combat: &Combat, wolf: Entity, weapon: i64, worn: &[i64], sps: &[i64], guard: Guard) -> Vec<String> {
        let w = combat.param("EquipParamWeapon", weapon);
        let num = |k: &str| w[k].as_f64().unwrap_or(0.0) as f32;
        let kept = match guard {
            Guard::None => [0.0; 5],
            Guard::Block => [num("poisonGuardResist"), num("bloodGuardResist"), num("diseaseGuardResist"), num("curseGuardResist"), num("freezeGuardResist")],
            Guard::Deflect => [num("poisonJustGuardResist"), num("bloodJustGuardResist"), num("diseaseJustGuardResist"), num("curseJustGuardResist"), num("freezeJustGuardResist")],
        };
        // gap: Wolf's other worn rows (110010 Pretending Dead disables all five) are not passed.
        self.build_up(combat, wolf, worn, sps, [WOLF_RESIST; 5], kept)
    }

    /// Whether SpEffect `id` is a build-up (poison / burn) rather than an effect of its own.
    pub fn is_build_up(combat: &Combat, id: i64) -> bool {
        sp(combat, id).is_some_and(|s| {
            (s.state_info == STATE_INFO_POISON && s.poizon_attack_power > 0.0)
                || (s.state_info == STATE_INFO_BURN && s.regist_blood > 0.0)
                || (s.state_info == STATE_INFO_TERROR && s.regist_illness > 0.0)
        }) || ResistRules::get().curse.contains_key(&id)
            || freeze_row(id).is_some()
    }

    /// The build-up of `sps` onto `target`'s gauges; `resist` and the guard's kept-off % per type
    /// (0 poison, 1 burn, 2 terror, 3 curse, 4 freeze / shock).
    fn build_up(&mut self, combat: &Combat, target: Entity, worn: &[i64], sps: &[i64], resist: [f32; 5], guard: [f32; 5]) -> Vec<String> {
        let rules = ResistRules::get();
        let Statuses(all, stop) = self;
        let status = all.entry(target).or_default();
        let mut out = Vec::new();
        for &id in sps.iter().filter(|id| **id > 0) {
            let freeze = freeze_row(id);
            // The shock chain's rows are the player's (9400 / 9420) whatever the target.
            let Some(s) = sp(combat, id).or_else(|| freeze.and_then(|(row, _)| sp(combat, row))) else { continue };
            let curse = rules.curse.get(&id).copied();
            let ty = match s.state_info {
                _ if freeze.is_some() => 4,
                STATE_INFO_POISON if s.poizon_attack_power > 0.0 => 0,
                STATE_INFO_BURN if s.regist_blood > 0.0 => 1,
                STATE_INFO_TERROR if s.regist_illness > 0.0 => 2,
                _ if curse.is_some() => 3,
                _ => continue,
            };
            // Poison's own 9045 (disablePoison) keeps more poison off; the burns (9100-9125)
            // disable nothing, so a burning enemy keeps building up (9122 disableBlood aside).
            if rules.disabled(status, worn, ty) {
                continue;
            }
            let (gauge, power, resist, guard, name) = match ty {
                0 => (&mut status.poison, s.poizon_attack_power * s.regist_poizon_change_rate, resist[0], guard[0], "poison"),
                1 => (&mut status.burn, s.regist_blood * s.regist_blood_change_rate, resist[1], guard[1], "burn"),
                2 => (&mut status.terror, s.regist_illness * s.regist_illness_change_rate, resist[2], guard[2], "terror"),
                4 => (&mut status.freeze, freeze.map_or(0.0, |f| f.1), resist[4], guard[4], "shock"),
                _ => (&mut status.curse, curse.map_or(0.0, |c| c.0), resist[3], guard[3], "aging"),
            };
            // Whole points, as the exe's int build-up: (int)(power * rate), then
            // (int)(x * (1 - guard resist %)) when guarded.
            let mut amount = power.trunc();
            // The worn rows' def<Type>ResistDamageRate (the cures' 30 s resist: Antidote 0.55,
            // Dousing 0.65, Pacifying 0.5; Red Lump and Oil x2 burn).
            let resist_rate = resist_damage_rate(worn, ty);
            if resist_rate != 1.0 {
                amount = (amount * resist_rate).trunc();
            }
            if guard > 0.0 && amount > 0.0 {
                amount = (amount * (1.0 - guard / 100.0)).trunc().max(0.0);
            }
            *gauge += amount;
            if *gauge < resist.max(1.0) {
                out.push(format!("{name} {:.0}/{resist:.0}", *gauge));
                continue;
            }
            *gauge = 0.0;
            // An instant row (effectEndurance 0: terror 9222 / 9223) takes its changeHpRate once
            // (`tick`). gap: the terror death anim and HUD gauge are not exported / done.
            // Its death and log are the row's own (round 57: before, every instant row was
            // terror's 590 insane death; poison 4001 / 4003 are instant too).
            if ty == 4 {
                // The shock's payload row (9420): HP scaled by the worn freezeDamageRate %, the
                // posture fields as they are, then its replace (9445, 1.5 s) and cycle (9450).
                let (row, _) = freeze.unwrap();
                let [hp_rate, hp_point, stamina_point, stamina_rate, behavior_ref, cycle] = rules.freeze_payload.get(&row).copied().unwrap_or_default();
                let scale = freeze_damage_rate(status, worn);
                status.instant_hp_rate += hp_rate * scale;
                status.instant_hp_point += hp_point * scale;
                status.instant_posture_point += -stamina_point;
                status.instant_posture_rate += -stamina_rate;
                status.instant_ref = behavior_ref as i64;
                status.instant_name = name;
                status.flash.push(row);
                start(combat, status, row, stop);
                if cycle > 0.0 {
                    start(combat, status, cycle as i64, stop);
                }
                out.push("SHOCK".to_string());
                continue;
            }
            if s.effect_endurance <= 0.0 && s.change_hp_rate > 0.0 {
                status.instant_hp_rate += s.change_hp_rate;
                status.instant_ref = s.behavior_ref_id;
                status.instant_name = name;
                status.flash.push(id);
            }
            // Aging's build-up row has no time of its own: the fill applies its cycleOccurrence
            // row (9600 -> 9620).
            let applied = match curse {
                Some((_, next)) if next > 0 => next,
                _ => id,
            };
            start(combat, status, applied, stop);
            out.push(match name {
                "burn" => "BURNING".to_string(),
                "terror" => "TERROR".to_string(),
                "aging" => "AGING".to_string(),
                _ => "POISONED".to_string(),
            });
        }
        out
    }
}

/// Aging on / off the body (round 44, bug fixes): its row's maxHpRate scales hp_max (HP kept
/// under it; back up on the end), and the row is worn in `resident`, so combat.rs's
/// npc_hit_damage takes its <attr>DamageCutRate 99 (x99). Its changeStaminaRate -100 applies
/// every frame (round 46): a timed row with motionInterval 0 re-fires each tick (exe
/// FUN_140c06b70 / FUN_140c06c60, summed by FUN_140bfe2c0 and added to the remaining posture at
/// 140a0.c:5119), so the aged Wolf's posture stays drained to the floor (our full bar).
/// gap: the player's floor in the clamp FUN_140bd4e20 (taken as empty, without a break
/// reaction); defNinsatsuDmgRate (99, as the cut rates) is not applied; the exe's order of
/// maxHpRate is not traced.
pub(crate) fn set_aging(a: &mut Actor, combat: &Combat, aging: Option<i64>) {
    if let Some(rate) = aging.and_then(|id| sp(combat, id)).map(|s| s.change_stamina_rate).filter(|r| *r < 0.0) {
        a.posture = (a.posture - a.posture_max * rate / 100.0).min(a.posture_max);
    }
    if a.aged == aging {
        return;
    }
    let rate = |id: i64| sp(combat, id).map(|s| s.max_hp_rate).filter(|r| *r > 0.0).unwrap_or(1.0);
    if let Some(old) = a.aged.take() {
        a.hp_max /= rate(old);
        a.resident.retain(|r| *r != old);
    }
    if let Some(id) = aging {
        a.hp_max *= rate(id);
        a.hp = a.hp.min(a.hp_max);
        a.resident.push(id);
        a.aged = Some(id);
    }
}

/// SpEffect `id` from Wolf's exported rows, else an enemy kind's (their poison / burn build-ups:
/// 9015 Rappa blowgun, 9115 bandit torches ...).
/// The shock build-up `id` carries (ResistRules::freeze): (its payload row, its power).
/// The product of the worn rows' def<Type>ResistDamageRate for build-up type `ty` (0 poison,
/// 1 burn, 2 terror, 3 curse; the shock has freezeDamageRate instead).
fn resist_damage_rate(worn: &[i64], ty: usize) -> f32 {
    let key = match ty {
        0 => "defPoisonResistDamageRate",
        1 => "defBloodResistDamageRate",
        2 => "defDiseaseResistDamageRate",
        3 => "defCurseResistDamageRate",
        _ => return 1.0,
    };
    worn.iter().map(|id| crate::items::param("SpEffectParam", *id)[key].as_f64().unwrap_or(1.0) as f32).product()
}

fn freeze_row(id: i64) -> Option<(i64, f32)> {
    ResistRules::get().freeze.get(&id).copied()
}

/// The product of the worn rows' freezeDamageRate % (the Way of Tomoe's resident 9461: 25; the
/// shock's own rows 100).
fn freeze_damage_rate(status: &Status, worn: &[i64]) -> f32 {
    let rules = ResistRules::get();
    status.effects.iter().map(|a| a.id).chain(worn.iter().copied()).filter_map(|id| rules.freeze_rate.get(&id)).product()
}

fn sp(combat: &Combat, id: i64) -> Option<&SpEffect> {
    let key = id.to_string();
    combat.player.sp_effects.get(&key).or_else(|| combat.kinds.iter().find_map(|k| k.data.sp_effects.get(&key)))
}

/// Applies an SpEffect; one with no duration hands straight on to its replaceSpEffectId. An
/// active one of the same nonzero spCategory (else the same id) gives way to it: the burns
/// 9100-9125 share category 10004, so a second burn row replaces the first instead of stacking
/// (double ticks, two flames). Its body effects stop (`stop`) and the new row's start.
fn start(combat: &Combat, status: &mut Status, id: i64, stop: &mut Vec<Entity>) {
    let mut id = id;
    for _ in 0..8 {
        let Some(s) = sp(combat, id) else { return };
        if s.effect_endurance > 0.0 {
            // Already on (a burning target re-ignites: burns have disableBlood 0): its time starts
            // over and it keeps its body effects, so no second flame layer is left running.
            // The interval timer starts at 0 on every set (FUN_140c06860 140c0.c:5011-5014,
            // FUN_140c03af0 :2506-2510, all stateInfos but 50/52/67/68), and FUN_140c06b70 ticks
            // when it is <= 0: the first tick lands on the frame it takes hold (round 66: it came
            // one motionInterval late, a 9.9 s burn ticking 9 times instead of 10).
            let same = |a: &Affliction| a.id == id || (s.sp_category != 0 && sp(combat, a.id).is_some_and(|o| o.sp_category == s.sp_category));
            if let Some(a) = status.effects.iter_mut().find(|a| same(a)) {
                a.left = s.effect_endurance;
                a.next_tick = 0.0;
                if a.id != id {
                    stop.append(&mut a.fx);
                    a.id = id;
                    a.shown = false;
                }
                return;
            }
            status.effects.push(Affliction { id, left: s.effect_endurance, next_tick: 0.0, shown: false, fx: Vec::new() });
            return;
        }
        if s.replace_sp_effect_id <= 0 {
            return;
        }
        id = s.replace_sp_effect_id;
    }
}

/// The afflictions' damage ticks and ends.
pub(crate) fn tick(time: Res<Time>, combat: Res<Combat>, mut statuses: ResMut<Statuses>, mut actors: Query<(Entity, &mut Actor)>, mut log: Option<ResMut<CombatLog>>) {
    let real_dt = time.delta_secs();
    let rules = ResistRules::get();
    let Statuses(all, stop) = &mut *statuses;
    // A despawned character (a phase swap, Genichiro -> Tomoe) takes its body effects along: they
    // are kept (keep = true) and would loop on at its last spot. Before, only dropped (round 51).
    all.retain(|e, s| {
        let alive = actors.contains(*e);
        if !alive {
            stop.extend(s.effects.iter_mut().flat_map(|a| a.fx.drain(..)));
        }
        alive
    });
    for (e, mut a) in &mut actors {
        // Status recovery and the effects' timers run on the character's clock (hit stop holds
        // them: FUN_140bda460 reads the per-character FD4Time, round 64).
        let dt = a.clock(real_dt);
        let aging = all.get(&e).and_then(|s| s.effects.iter().find(|f| sp(&combat, f.id).is_some_and(|s| s.behavior_ref_id == REF_AGING)).map(|f| f.id));
        set_aging(&mut a, &combat, aging);
        let Some(status) = all.get_mut(&e) else { continue };
        let rate = if a.side == crate::actor::Side::Player { rules.recover_player } else { rules.recover_enemy };
        recover(&mut status.poison, &mut status.carry[0], rate[0], dt);
        recover(&mut status.burn, &mut status.carry[1], rate[1], dt);
        recover(&mut status.terror, &mut status.carry[2], rate[2], dt);
        recover(&mut status.curse, &mut status.carry[3], rate[3], dt);
        recover(&mut status.freeze, &mut status.carry[4], rate[4], dt);
        let posture = std::mem::take(&mut status.instant_posture_point) + a.posture_max * std::mem::take(&mut status.instant_posture_rate) / 100.0;
        if posture != 0.0 && a.hp > 0.0 {
            a.add_posture(posture);
        }
        let instant = std::mem::take(&mut status.instant_hp_rate);
        let point = std::mem::take(&mut status.instant_hp_point);
        if (instant > 0.0 || point > 0.0) && a.hp > 0.0 {
            let dmg = a.hp_max * instant / 100.0 + point;
            a.hp = (a.hp - dmg).max(0.0);
            if a.hp <= 0.0 {
                a.death_ref = status.instant_ref;
            }
            if let Some(log) = log.as_mut() {
                log.push(format!("{}: -{dmg:.0} HP", status.instant_name), Color::srgb(0.7, 0.55, 0.9));
            }
        }
        let mut ended = Vec::new();
        for f in status.effects.iter_mut() {
            let Some(s) = sp(&combat, f.id) else { continue };
            f.next_tick -= dt;
            while f.next_tick <= 0.0 {
                f.next_tick += s.motion_interval.max(0.1);
                // changeHpRate is % of max HP, changeHpPoint flat (positive = damage).
                let dmg = a.hp_max * s.change_hp_rate / 100.0 + s.change_hp_point;
                if dmg > 0.0 && a.hp > 0.0 {
                    a.hp = (a.hp - dmg).max(0.0);
                    if a.hp <= 0.0 {
                        a.death_ref = s.behavior_ref_id;
                    }
                    if let Some(log) = log.as_mut() {
                        let what = match s.behavior_ref_id {
                            REF_BURNING => "burn",
                            REF_POISON => "poison",
                            _ => "status",
                        };
                        log.push(format!("{what}: -{dmg:.0} HP"), Color::srgb(0.7, 0.55, 0.9));
                    }
                }
            }
            f.left -= dt;
            if f.left <= 0.0 {
                ended.push((f.id, s.replace_sp_effect_id));
            }
        }
        stop.extend(status.effects.iter().filter(|f| f.left <= 0.0).flat_map(|f| f.fx.iter().copied()));
        status.effects.retain(|f| f.left > 0.0);
        for (_, next) in ended {
            if next > 0 {
                start(&combat, status, next, stop);
            }
        }
    }
}

/// SpEffectVfxParam rows: (initSfxId, initDmyId, midstSfxId, midstDmyId).
#[derive(Resource, Default)]
pub(crate) struct VfxRows(pub(crate) HashMap<i64, [i64; 4]>);

impl VfxRows {
    fn load() -> Self {
        let path = crate::paths::extracted().join("json/params/SpEffectVfxParam.json");
        let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Self::default() };
        let rows: Vec<&serde_json::Value> = match &v["rows"] {
            serde_json::Value::Array(a) => a.iter().collect(),
            serde_json::Value::Object(o) => o.values().collect(),
            _ => Vec::new(),
        };
        let n = |r: &serde_json::Value, k: &str| r[k].as_i64().unwrap_or(-1);
        Self(rows.iter().filter_map(|r| Some((r["id"].as_i64()?, [n(r, "initSfxId"), n(r, "initDmyId"), n(r, "midstSfxId"), n(r, "midstDmyId")]))).collect())
    }
}

/// The afflictions' effects on the body (SpEffect vfxId / vfxId1 -> SpEffectVfxParam): the flames
/// (midst 3012 on dummy 850) and the "炎上" kanji (init 3013 on 280) of a burn, poison's 4011 /
/// 4013. The midst effect rides on the dummy until the affliction ends.
fn show(
    mut commands: Commands,
    combat: Res<Combat>,
    rows: Res<VfxRows>,
    mut statuses: ResMut<Statuses>,
    dummies: Query<&crate::model::Dummies>,
    tfs: Query<&GlobalTransform>,
    mut fx: Query<&mut crate::fxr::FxEffect>,
) {
    let Statuses(all, stop) = &mut *statuses;
    for e in stop.drain(..) {
        if let Ok(mut f) = fx.get_mut(e) {
            f.stop = true;
        }
    }
    for (&owner, status) in all.iter_mut() {
        for a in status.effects.iter_mut().filter(|a| !a.shown) {
            a.shown = true;
            a.fx.extend(spawn_vfx(&mut commands, &combat, &rows, a.id, owner, &dummies, &tfs));
        }
        // An instant row has no time to hold a midst effect: any is stopped next frame.
        for id in std::mem::take(&mut status.flash) {
            stop.extend(spawn_vfx(&mut commands, &combat, &rows, id, owner, &dummies, &tfs));
        }
    }
}

/// Plays SpEffect `id`'s effects (vfxId / vfxId1 -> SpEffectVfxParam) on `owner`: the init one
/// once, the midst one held (returned, for the caller to stop when the SpEffect ends).
fn spawn_vfx(commands: &mut Commands, combat: &Combat, rows: &VfxRows, id: i64, owner: Entity, dummies: &Query<&crate::model::Dummies>, tfs: &Query<&GlobalTransform>) -> Vec<Entity> {
    let mut held = Vec::new();
    let Some(vfxs) = sp(combat, id).map(|s| [s.vfx_id, s.vfx_id1]).or_else(|| param_vfx(id)) else { return held };
    for vfx in vfxs {
        let Some(&[init, init_dmy, midst, midst_dmy]) = rows.0.get(&vfx) else { continue };
        for (id, dmy, keep) in [(init, init_dmy, false), (midst, midst_dmy, true)] {
            if id <= 0 {
                continue;
            }
            let anchor = dummies.get(owner).ok().and_then(|d| d.0.get(&(dmy as i16)).copied()).unwrap_or(owner);
            let at = tfs.get(anchor).map(|g| g.compute_transform()).unwrap_or_default();
            let mut fx = crate::fxr::FxEffect::new(id, Some(crate::fxr::Anchor::Entity(anchor)), true, at);
            // The midst effect loops for the whole affliction (poison 4011 / burn 3012 on
            // dummy 850); tick / clear stop it, so fxr's 20 s age cap must not (round 22).
            fx.keep = keep;
            let spawned = commands.spawn(fx).id();
            if keep {
                held.push(spawned);
            }
        }
    }
    held
}

/// The SpEffectParam fields the body looks read, for any row (combat data holds only the rows the
/// characters list; the Finger Whistle's madness 230515 / 230516 comes from a SpEffect cycle):
/// vfxId / vfxId1, cycleOccurrenceSpEffectId, motionInterval, effectEndurance. Parsed once.
struct ParamRow {
    vfx: [i64; 2],
    cycle: i64,
    interval: f32,
    endurance: f32,
}

fn param_row(id: i64) -> Option<&'static ParamRow> {
    static T: std::sync::OnceLock<HashMap<i64, ParamRow>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/SpEffectParam.json");
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let rows: Vec<&serde_json::Value> = match &v["rows"] {
            serde_json::Value::Array(a) => a.iter().collect(),
            serde_json::Value::Object(o) => o.values().collect(),
            _ => Vec::new(),
        };
        let n = |r: &serde_json::Value, k: &str| r[k].as_i64().unwrap_or(-1);
        let f = |r: &serde_json::Value, k: &str| r[k].as_f64().unwrap_or(0.0) as f32;
        rows.iter()
            .filter_map(|r| {
                let row = ParamRow { vfx: [n(r, "vfxId"), n(r, "vfxId1")], cycle: n(r, "cycleOccurrenceSpEffectId"), interval: f(r, "motionInterval"), endurance: f(r, "effectEndurance") };
                (row.vfx.iter().any(|x| *x > 0) || row.cycle > 0).then_some((r["id"].as_i64()?, row))
            })
            .collect()
    });
    t.get(&id)
}

fn param_vfx(id: i64) -> Option<[i64; 2]> {
    param_row(id).map(|r| r.vfx).filter(|v| v.iter().any(|x| *x > 0))
}

/// (cycleOccurrenceSpEffectId, motionInterval s): the row that fires every interval while this one
/// is worn (the resurrection's heartbeat parent 110091 -> 110092 every 2.5 s).
fn param_cycle(id: i64) -> Option<(i64, f32)> {
    param_row(id).filter(|r| r.cycle > 0 && r.interval > 0.0).map(|r| (r.cycle, r.interval))
}

/// effectEndurance (s) of a row with a look or a cycle.
fn param_endurance(id: i64) -> Option<f32> {
    param_row(id).map(|r| r.endurance)
}

/// A lasting SpEffect's look on the body: while a row is on `Actor::sp_chained` (a SpEffect cycle,
/// or the row a TAE SpEffect hands on to), its SpEffectVfx plays as an affliction's does and the
/// midst effect stops when the row goes. The Finger Whistle's madness 230515 -> SpEffectVfx 7900
/// (midst 300205 on dummy 7), the minor one 230516 -> 7901 (304205). Rows the current anim still
/// carries are left to ffx.rs (`tae_sp_vfx`), which shows them while the TAE does. Wolf's timed
/// rows (`Player::timed`, TAE 940's refType-2 SpEffect for its effectEndurance) too: Divine
/// Abduction's "wind collecting" 107700 / 107702 -> SpEffectVfx 7700 (midst 300190), seen in the
/// game during a077_400000 (rec_20261015_203558_skills_hooks, 460-493 ms).
fn lasting_glow(
    mut commands: Commands,
    combat: Res<Combat>,
    rows: Res<VfxRows>,
    actors: Query<(Entity, &Actor, Option<&crate::player::Player>)>,
    dummies: Query<&crate::model::Dummies>,
    tfs: Query<&GlobalTransform>,
    mut fx: Query<&mut crate::fxr::FxEffect>,
    mut live: Local<HashMap<(Entity, i64), Vec<Entity>>>,
) {
    let now: Vec<(Entity, i64)> = actors
        .iter()
        .flat_map(|(e, a, p)| {
            let chained = a.sp_chained.iter().filter(|(id, _)| !a.tae_prev.contains(id)).map(|(id, _)| *id);
            chained.chain(p.into_iter().flat_map(|p| p.timed.iter().map(|(id, _)| *id))).map(move |id| (e, id)).collect::<Vec<_>>()
        })
        .collect();
    live.retain(|k, held| {
        let on = now.contains(k);
        if !on {
            for f in held.iter() {
                if let Ok(mut f) = fx.get_mut(*f) {
                    f.stop = true;
                }
            }
        }
        on
    });
    for k in now {
        if !live.contains_key(&k) {
            let held = spawn_vfx(&mut commands, &combat, &rows, k.1, k.0, &dummies, &tfs);
            live.insert(k, held);
        }
    }
}

/// Instant SpEffects (effectEndurance 0) put on a character this frame, shown once: their
/// SpEffectVfx init effects (the Pellet's 3020 -> 310021 / 310022 sparkle), and a midst one, if
/// any (the Red Lump's 3050 -> 310461), for `BURST_MIDST` s. items.rs `apply_used` fills it.
#[derive(Resource, Default)]
pub(crate) struct SpBursts(pub(crate) Vec<(Entity, i64)>);
const BURST_MIDST: f32 = 1.5;

#[allow(clippy::too_many_arguments)]
fn sp_bursts(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    rows: Res<VfxRows>,
    mut bursts: ResMut<SpBursts>,
    mut players: Query<(Entity, &mut crate::player::Player)>,
    dummies: Query<&crate::model::Dummies>,
    tfs: Query<&GlobalTransform>,
    mut fx: Query<&mut crate::fxr::FxEffect>,
    mut held: Local<Vec<(Entity, f32)>>,
) {
    let now = time.elapsed_secs();
    let mut queued = std::mem::take(&mut bursts.0);
    for (e, mut p) in &mut players {
        queued.extend(p.sp_bursts.drain(..).map(|id| (e, id)));
    }
    for (owner, id) in queued {
        held.extend(spawn_vfx(&mut commands, &combat, &rows, id, owner, &dummies, &tfs).into_iter().map(|e| (e, now + BURST_MIDST)));
    }
    held.retain(|(e, until)| {
        if now < *until {
            return true;
        }
        if let Ok(mut f) = fx.get_mut(*e) {
            f.stop = true;
        }
        false
    });
}

/// SpEffectVfxParam midstSeId of a vfx row (5010, the resurrection's heart sound: 200001).
fn vfx_se(vfx: i64) -> Option<i64> {
    static T: std::sync::OnceLock<HashMap<i64, i64>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/SpEffectVfxParam.json");
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let rows: Vec<&serde_json::Value> = match &v["rows"] {
            serde_json::Value::Array(a) => a.iter().collect(),
            serde_json::Value::Object(o) => o.values().collect(),
            _ => Vec::new(),
        };
        rows.iter().filter_map(|r| Some((r["id"].as_i64()?, r["midstSeId"].as_i64().unwrap_or(0)))).filter(|(_, se)| *se > 0).collect()
    });
    t.get(&vfx).copied()
}

/// The resurrection's heartbeat (Bosees, HANDOFF 4.42): Wolf's resurrection anims a000_110020 / 21
/// put on 110091 (kept in `Player::held`, -1 endurance) until 110093 cancels it. 110091 fires
/// 110092 every motionInterval (2.5 s); 110092's SpEffectVfx 5010 has no effect, only its midst
/// sound 200001 (event c000200001, the main bank's only key with that number). A midst sound
/// lasts while its SpEffect does: each beat is stopped after 110092's effectEndurance (2.4 s; the
/// event runs ~3.4 s), or when 110091 goes. Without FMOD (tests, the PCM fallback) the beat is
/// queued whole. Reads the state only.
fn heartbeat(
    time: Res<Time>,
    players: Query<(&crate::player::Player, &GlobalTransform)>,
    mut sounds: Option<ResMut<crate::sound::SoundQueue>>,
    mut fmod: Option<NonSendMut<crate::fmod::Fmod>>,
    mut next: Local<HashMap<i64, f32>>,
    mut beats: Local<Vec<(i64, f32, crate::fmod::EventHandle)>>,
) {
    let now = time.elapsed_secs();
    let mut worn = Vec::new();
    for (p, g) in &players {
        for &id in &p.held {
            let Some((cycle, every)) = param_cycle(id) else { continue };
            worn.push(id);
            let at = next.entry(id).or_insert(now);
            if now < *at {
                continue;
            }
            *at = now + every;
            let Some(se) = sp_vfx_ids(cycle).into_iter().find_map(vfx_se) else { continue };
            let key = format!("c{se:09}");
            if let Some(fm) = fmod.as_mut() {
                let lasts = param_endurance(cycle).filter(|e| *e > 0.0).unwrap_or(every);
                if let Some(ev) = fm.play_tracked(&key, g.translation()) {
                    beats.push((id, now + lasts, ev));
                }
            } else if let Some(q) = sounds.as_mut() {
                q.0.push((key, g.translation()));
            }
        }
    }
    next.retain(|id, _| worn.contains(id));
    if let Some(fm) = fmod.as_mut() {
        beats.retain(|(id, until, ev)| {
            let on = now < *until && worn.contains(id);
            if !on {
                fm.stop(*ev);
            }
            on
        });
    }
}


/// vfxId / vfxId1 of a SpEffectParam row (any row, `param_vfx`), the unset ones dropped.
fn sp_vfx_ids(id: i64) -> Vec<i64> {
    param_vfx(id).map(|v| v.into_iter().filter(|x| *x > 0).collect()).unwrap_or_default()
}

/// The lightning charge on the body (manual test #5): while `Actor::electro` holds a SpEffect
/// (combat.rs: 9490 / 9495, or the attack's charge 9492 / 9493), its effects play as an
/// affliction's do (9492 / 9493 -> SpEffectVfx 6000, midst 404041; 9495 -> 652030), and stop
/// when the charge is spent, discharged or replaced. Reads the state only.
fn electro_glow(
    mut commands: Commands,
    combat: Res<Combat>,
    rows: Res<VfxRows>,
    actors: Query<(Entity, &crate::actor::Actor)>,
    dummies: Query<&crate::model::Dummies>,
    tfs: Query<&GlobalTransform>,
    mut fx: Query<&mut crate::fxr::FxEffect>,
    mut live: Local<HashMap<Entity, (i64, Vec<Entity>)>>,
) {
    let now: HashMap<Entity, i64> = actors.iter().filter_map(|(e, a)| a.electro.map(|(id, _)| (e, id))).collect();
    live.retain(|e, (id, held)| {
        let on = now.get(e) == Some(id);
        if !on {
            for f in held.iter() {
                if let Ok(mut f) = fx.get_mut(*f) {
                    f.stop = true;
                }
            }
        }
        on
    });
    for (&e, &id) in &now {
        if !live.contains_key(&e) {
            let mut held = spawn_vfx(&mut commands, &combat, &rows, id, e, &dummies, &tfs);
            // The enemies' charges 9491-9494 have no row on Wolf; they show his plain charge 9490.
            if held.is_empty() && (9491..=9494).contains(&id) {
                held = spawn_vfx(&mut commands, &combat, &rows, 9490, e, &dummies, &tfs);
            }
            live.insert(e, (id, held));
        }
    }
}

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Statuses>().init_resource::<SpBursts>().insert_resource(VfxRows::load()).add_systems(FixedUpdate, tick.after(ActorSet::Resolve)).add_systems(Update, (show, electro_glow, lasting_glow, heartbeat, sp_bursts));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gauges_drain_in_whole_points() {
        // 5 a second at 60 fps: a point about every 12 frames (float sums), the fraction carried.
        let (mut g, mut c) = (100.0f32, 0.0f32);
        for _ in 0..15 {
            super::recover(&mut g, &mut c, 5.0, 1.0 / 60.0);
        }
        assert_eq!(g, 99.0);
        assert!(c > 0.1 && c < 0.3, "carry {c}");
        for _ in 0..60 * 30 {
            super::recover(&mut g, &mut c, 5.0, 1.0 / 60.0);
        }
        assert_eq!(g, 0.0);
    }

    #[test]
    fn resist_rules_from_the_params() {
        let r = super::ResistRules::get();
        if r.disable.is_empty() {
            return;
        }
        assert_eq!(r.recover_enemy, [5.0, 5.0, 2.5, 20.0, 5.0]);
        assert_eq!(r.recover_player[2], 2.5, "terror: ResistRecoverPoint_Desease_Player");
        // Poison 9045 stops more poison; the Flame Vent burn 9100 stops nothing.
        assert_eq!(r.disable.get(&9045), Some(&[true, false, false, false, false]));
        assert!(!r.disable.contains_key(&9100));
    }
}
