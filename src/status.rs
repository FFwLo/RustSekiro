//! Status build-up on the enemy: burn (the Flame Vent) and poison (the Sabimaru). A hit's
//! SpEffects (AtkParam / Bullet spEffectId0-4) carry the build-up (Paramdex SDT SpEffect meta):
//! poizonAttackPower with stateInfo 2 (poison), registBlood with stateInfo 6 (burn, Sekiro's
//! "blood" slot). When a gauge reaches the NpcParam resist_poison / resist_blood the effect takes
//! hold: it hurts every motionInterval s (changeHpRate % of max HP + changeHpPoint) for its
//! effectEndurance, then its replaceSpEffectId follows (Sabimaru 9004 -> 9045 poison damage).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet};
use crate::hud::CombatLog;
use crate::data::{Combat, SpEffect};

/// SpEffect stateInfos of the build-ups (SP_EFFECT_TYPE: 2 poison, 6 burn).
const STATE_INFO_POISON: i64 = 2;
const STATE_INFO_BURN: i64 = 6;
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

#[derive(Default, Debug)]
pub struct Status {
    pub poison: f32,
    pub burn: f32,
    pub effects: Vec<Affliction>,
}

/// Every character's build-up gauges and afflictions; `stop`: effects of ended afflictions.
#[derive(Resource, Default)]
pub struct Statuses(pub HashMap<Entity, Status>, Vec<Entity>);

impl Statuses {
    /// An affliction with this behaviorRefId is on the character (HKS env(3036, ref) / env(3041)).
    pub fn has_ref(&self, combat: &Combat, e: Entity, behavior_ref: i64) -> bool {
        self.0.get(&e).is_some_and(|s| s.effects.iter().any(|a| sp(combat, a.id).is_some_and(|x| x.behavior_ref_id == behavior_ref)))
    }

    /// Wolf's hit on an enemy: each of its SpEffects adds its build-up; a full gauge applies the
    /// effect. `guarded`: the NpcParam *GuardResist % of it is kept off. Returns the log lines.
    pub fn hit(&mut self, combat: &Combat, target: Entity, def: &crate::actor::Actor, sps: &[i64], guarded: bool) -> Vec<String> {
        let npc = combat.npc(def);
        let num = |k: &str| npc[k].as_f64().unwrap_or(0.0) as f32;
        let status = self.0.entry(target).or_default();
        let mut out = Vec::new();
        for &id in sps.iter().filter(|id| **id > 0) {
            let Some(s) = sp(combat, id) else { continue };
            let (gauge, power, resist, guard, name) = match s.state_info {
                STATE_INFO_POISON if s.poizon_attack_power > 0.0 => (&mut status.poison, s.poizon_attack_power * s.regist_poizon_change_rate, num("resist_poison"), num("poisonGuardResist"), "poison"),
                STATE_INFO_BURN if s.regist_blood > 0.0 => (&mut status.burn, s.regist_blood * s.regist_blood_change_rate, num("resist_blood"), num("bloodGuardResist"), "burn"),
                _ => continue,
            };
            // gap: an affliction already running takes no more build-up, and the gauges never
            // drain (the exe's decay rate is not traced).
            let running = status.effects.iter().any(|a| sp(combat, a.id).is_some_and(|x| x.state_info == s.state_info || x.behavior_ref_id == s.behavior_ref_id));
            if running {
                continue;
            }
            *gauge += power * if guarded { 1.0 - guard / 100.0 } else { 1.0 };
            if *gauge < resist.max(1.0) {
                out.push(format!("{name} {:.0}/{resist:.0}", *gauge));
                continue;
            }
            *gauge = 0.0;
            start(combat, status, id);
            out.push(if name == "burn" { "BURNING".to_string() } else { "POISONED".to_string() });
        }
        out
    }
}

fn sp(combat: &Combat, id: i64) -> Option<&SpEffect> {
    combat.player.sp_effects.get(&id.to_string())
}

/// Applies an SpEffect; one with no duration hands straight on to its replaceSpEffectId.
fn start(combat: &Combat, status: &mut Status, id: i64) {
    let mut id = id;
    for _ in 0..8 {
        let Some(s) = sp(combat, id) else { return };
        if s.effect_endurance > 0.0 {
            status.effects.retain(|a| a.id != id);
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
    statuses.0.retain(|e, _| actors.contains(*e));
    let Statuses(all, stop) = &mut *statuses;
    for (e, mut a) in &mut actors {
        let Some(status) = all.get_mut(&e) else { continue };
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
                start(&combat, status, next);
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
