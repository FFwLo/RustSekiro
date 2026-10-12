//! Status build-up: burn (the Flame Vent) and poison (the Sabimaru) on enemies, and the enemies'
//! poison / burn on Wolf (Statuses::hit_wolf). A hit's
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
    pub effects: Vec<Affliction>,
    /// The fractions of a recovered point carried to the next frame (+0x38), poison and burn.
    carry: [f32; 2],
}

impl Status {
    /// Whether an applied effect has HKS behaviorRefId `behavior_ref` (REF_POISON / REF_BURNING).
    pub fn afflicted(&self, combat: &Combat, behavior_ref: i64) -> bool {
        self.effects.iter().any(|a| sp(combat, a.id).is_some_and(|s| s.behavior_ref_id == behavior_ref))
    }
}

/// TentativePlayerParam row 0 ResistRecoverPoint_Poision / _Blood (points a second) for enemies
/// and for Wolf, and the SpEffectParam rows' disablePoison / disableBlood flags.
#[derive(Default)]
pub struct ResistRules {
    pub recover_enemy: [f32; 2],
    pub recover_player: [f32; 2],
    disable: HashMap<i64, [bool; 2]>,
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
        let disable = read("SpEffectParam")
            .iter()
            .filter_map(|r| {
                let on = |k: &str| r[k].as_i64() == Some(1);
                let d = [on("disablePoison"), on("disableBlood")];
                (d[0] || d[1]).then(|| (r["id"].as_i64().unwrap_or(-1), d))
            })
            .collect();
        Self {
            recover_enemy: [f("ResistRecoverPoint_Poision_Enemy"), f("ResistRecoverPoint_Blood_Enemy")],
            recover_player: [f("ResistRecoverPoint_Poision_Player"), f("ResistRecoverPoint_Blood_Player")],
            disable,
        }
    }

    /// An active SpEffect of `status` stops this type's build-up (0 poison, 1 burn).
    fn disabled(&self, status: &Status, ty: usize) -> bool {
        status.effects.iter().any(|a| self.disable.get(&a.id).is_some_and(|d| d[ty]))
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

    /// A new fight: every gauge and affliction goes, their effects on the body stop (Wolf keeps
    /// his entity across resets, so his entry would carry the poison into the next fight).
    pub fn clear_all(&mut self) {
        let Statuses(all, stop) = self;
        stop.extend(all.drain().flat_map(|(_, s)| s.effects.into_iter().flat_map(|a| a.fx)));
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
        let guard = if guarded { [num("poisonGuardResist"), num("bloodGuardResist")] } else { [0.0; 2] };
        self.build_up(combat, target, sps, [num("resist_poison"), num("resist_blood")], guard)
    }

    /// An enemy's hit (or bullet) on Wolf: as `hit`, against WOLF_RESIST, the guard's share kept
    /// off by his sword `weapon` (EquipParamWeapon, the equipped combat art's row).
    pub fn hit_wolf(&mut self, combat: &Combat, wolf: Entity, weapon: i64, sps: &[i64], guard: Guard) -> Vec<String> {
        let w = combat.param("EquipParamWeapon", weapon);
        let num = |k: &str| w[k].as_f64().unwrap_or(0.0) as f32;
        let kept = match guard {
            Guard::None => [0.0; 2],
            Guard::Block => [num("poisonGuardResist"), num("bloodGuardResist")],
            Guard::Deflect => [num("poisonJustGuardResist"), num("bloodJustGuardResist")],
        };
        self.build_up(combat, wolf, sps, [WOLF_RESIST; 2], kept)
    }

    /// Whether SpEffect `id` is a build-up (poison / burn) rather than an effect of its own.
    pub fn is_build_up(combat: &Combat, id: i64) -> bool {
        sp(combat, id).is_some_and(|s| (s.state_info == STATE_INFO_POISON && s.poizon_attack_power > 0.0) || (s.state_info == STATE_INFO_BURN && s.regist_blood > 0.0))
    }

    /// The build-up of `sps` onto `target`'s gauges; `resist` and the guard's kept-off % per type
    /// (0 poison, 1 burn).
    fn build_up(&mut self, combat: &Combat, target: Entity, sps: &[i64], resist: [f32; 2], guard: [f32; 2]) -> Vec<String> {
        let rules = ResistRules::get();
        let Statuses(all, stop) = self;
        let status = all.entry(target).or_default();
        let mut out = Vec::new();
        for &id in sps.iter().filter(|id| **id > 0) {
            let Some(s) = sp(combat, id) else { continue };
            let ty = match s.state_info {
                STATE_INFO_POISON if s.poizon_attack_power > 0.0 => 0,
                STATE_INFO_BURN if s.regist_blood > 0.0 => 1,
                _ => continue,
            };
            // Poison's own 9045 (disablePoison) keeps more poison off; the burns (9100-9125)
            // disable nothing, so a burning enemy keeps building up (9122 disableBlood aside).
            if rules.disabled(status, ty) {
                continue;
            }
            let (gauge, power, resist, guard, name) = match ty {
                0 => (&mut status.poison, s.poizon_attack_power * s.regist_poizon_change_rate, resist[0], guard[0], "poison"),
                _ => (&mut status.burn, s.regist_blood * s.regist_blood_change_rate, resist[1], guard[1], "burn"),
            };
            // Whole points, as the exe's int build-up: (int)(power * rate), then
            // (int)(x * (1 - guard resist %)) when guarded.
            let mut amount = power.trunc();
            if guard > 0.0 && amount > 0.0 {
                amount = (amount * (1.0 - guard / 100.0)).trunc().max(0.0);
            }
            *gauge += amount;
            if *gauge < resist.max(1.0) {
                out.push(format!("{name} {:.0}/{resist:.0}", *gauge));
                continue;
            }
            *gauge = 0.0;
            start(combat, status, id, stop);
            out.push(if name == "burn" { "BURNING".to_string() } else { "POISONED".to_string() });
        }
        out
    }
}

/// SpEffect `id` from Wolf's exported rows, else an enemy kind's (their poison / burn build-ups:
/// 9015 Rappa blowgun, 9115 bandit torches ...).
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
            // gap: whether the exe also restarts the damage tick cadence is not traced.
            let same = |a: &Affliction| a.id == id || (s.sp_category != 0 && sp(combat, a.id).is_some_and(|o| o.sp_category == s.sp_category));
            if let Some(a) = status.effects.iter_mut().find(|a| same(a)) {
                a.left = s.effect_endurance;
                if a.id != id {
                    stop.append(&mut a.fx);
                    a.id = id;
                    a.next_tick = s.motion_interval.max(0.1);
                    a.shown = false;
                }
                return;
            }
            status.effects.push(Affliction { id, left: s.effect_endurance, next_tick: s.motion_interval.max(0.1), shown: false, fx: Vec::new() });
            return;
        }
        if s.replace_sp_effect_id <= 0 {
            return;
        }
        id = s.replace_sp_effect_id;
    }
}

/// The afflictions' damage ticks and ends.
fn tick(time: Res<Time>, combat: Res<Combat>, mut statuses: ResMut<Statuses>, mut actors: Query<(Entity, &mut Actor)>, mut log: Option<ResMut<CombatLog>>) {
    let dt = time.delta_secs();
    let rules = ResistRules::get();
    statuses.0.retain(|e, _| actors.contains(*e));
    let Statuses(all, stop) = &mut *statuses;
    for (e, mut a) in &mut actors {
        let Some(status) = all.get_mut(&e) else { continue };
        let rate = if a.side == crate::actor::Side::Player { rules.recover_player } else { rules.recover_enemy };
        recover(&mut status.poison, &mut status.carry[0], rate[0], dt);
        recover(&mut status.burn, &mut status.carry[1], rate[1], dt);
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
struct VfxRows(HashMap<i64, [i64; 4]>);

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
            let Some(s) = sp(&combat, a.id) else { continue };
            for vfx in [s.vfx_id, s.vfx_id1] {
                let Some(&[init, init_dmy, midst, midst_dmy]) = rows.0.get(&vfx) else { continue };
                for (id, dmy, keep) in [(init, init_dmy, false), (midst, midst_dmy, true)] {
                    if id <= 0 {
                        continue;
                    }
                    let anchor = dummies.get(owner).ok().and_then(|d| d.0.get(&(dmy as i16)).copied()).unwrap_or(owner);
                    let at = tfs.get(anchor).map(|g| g.compute_transform()).unwrap_or_default();
                    let spawned = commands.spawn(crate::fxr::FxEffect::new(id, Some(crate::fxr::Anchor::Entity(anchor)), true, at)).id();
                    if keep {
                        a.fx.push(spawned);
                    }
                }
            }
        }
    }
}

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Statuses>().insert_resource(VfxRows::load()).add_systems(FixedUpdate, tick.after(ActorSet::Resolve)).add_systems(Update, show);
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
        assert_eq!(r.recover_enemy, [5.0, 5.0]);
        // Poison 9045 stops more poison; the Flame Vent burn 9100 stops nothing.
        assert_eq!(r.disable.get(&9045), Some(&[true, false]));
        assert!(!r.disable.contains_key(&9100));
    }
}
