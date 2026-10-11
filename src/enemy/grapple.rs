//! Enemies as grapple (wire) targets (kb/grapple.md section 4). An enemy is one while an
//! SpEffect with chrWireVariationNo != -1 is on it: its TAE puts them on with event 66
//! AddSpEffect_Multiplayer in the moves that open it (the Chained Ogre's grabs 3502020, tackles
//! 3502021 and collapse 3502022; Gyoubu's helmet 3508020-22 and drifts 3508060 / 61; the Guardian
//! Ape's sword 3510050), or as a resident one. The hook catches chrWireTargetDmypolyId (650-652 on
//! the body part) and Wolf lands at chrWireLandingPointDmypolyId (260 = the model root; the Ape's
//! 40 on his sword). The player's arc row is ChrPhysicsHomingParam 3000<variation> (300050200:
//! the Ogre grabbing, target offset (0, 1.5, -2)).
//! The map's own points and the search are map.rs; the player's move is player.rs.

use bevy::prelude::*;

use super::Enemy;
use crate::actor::Actor;
use crate::data::Combat;
use crate::model::Dummies;

/// One enemy open to the grapple this frame.
#[derive(Clone, Debug)]
pub struct EnemyGrapple {
    pub entity: Entity,
    /// World position of the dummy the hook catches.
    pub hook: Vec3,
    /// World position Wolf lands at.
    pub land: Vec3,
    /// chrWireVariationNo (e.g. 50200).
    pub variation: i64,
    /// ChrPhysicsHomingParam row of the player's arc: 300000000 + variation.
    pub homing_row: i64,
    /// The SpEffect that opened it.
    pub sp_effect: i64,
}

/// Every enemy open to the grapple, refreshed each frame (after the dummies' transforms).
#[derive(Resource, Default)]
pub struct EnemyGrapples(pub Vec<EnemyGrapple>);

/// The wire SpEffect on an enemy now (TAE first, then its residents): (id, variation, target
/// dummy, landing dummy).
pub fn wire_effect(combat: &Combat, a: &Actor) -> Option<(i64, i64, i64, i64)> {
    let d = combat.data_of(a);
    let wire = |id: i64, s: &crate::data::SpEffect| (s.chr_wire_variation_no >= 0).then_some((id, s.chr_wire_variation_no, s.chr_wire_target_dmypoly_id, s.chr_wire_landing_point_dmypoly_id));
    let from_tae = if a.anim.is_empty() { None } else { d.sp_effects_at(&a.anim, a.t).into_iter().find_map(|(id, s)| wire(id, s)) };
    from_tae.or_else(|| a.resident.iter().find_map(|&id| d.sp_effects.get(&id.to_string()).and_then(|s| wire(id, s))))
}

pub(super) fn update(
    combat: Res<Combat>,
    mut out: ResMut<EnemyGrapples>,
    q: Query<(Entity, &Enemy, &Actor, &GlobalTransform, Option<&Dummies>, Option<&super::script::Phantom>)>,
    markers: Query<&GlobalTransform>,
) {
    out.0.clear();
    for (entity, e, a, gt, dummies, ph) in &q {
        if e.disabled || e.is_dead() || a.hp <= 0.0 || ph.is_some_and(|p| p.parked()) {
            continue;
        }
        let Some((sp_effect, variation, target, landing)) = wire_effect(&combat, a) else { continue };
        let at = |id: i64| dummies.and_then(|dm| dm.0.get(&(id as i16))).and_then(|&m| markers.get(m).ok()).map(|g| g.translation());
        // Without the model's dummies: the hook a body height up, the landing at the root.
        let root = gt.translation();
        let hook = at(target).unwrap_or(root + Vec3::Y * 2.0);
        let land = at(landing).unwrap_or(root);
        out.0.push(EnemyGrapple { entity, hook, land, variation, homing_row: 300_000_000 + variation, sp_effect });
    }
}

/// Wolf's hook stuck in an enemy (his AtkParam 9500000 "when the wire is stabbed" gives it
/// SpEffect 510000, behaviorRefId 1000410 SP_EFFECT_REF_WIRE_ATTACK). Sent by the player's wire
/// move.
#[derive(Message, Clone, Copy, Debug)]
pub struct WireHit(pub Entity);

/// SP_EFFECT_REF_ENABLE_WIRE_DAMAGE0-4 (c9997 HKS): its moves' TAE 66 puts 510010-510014 on in
/// the windows (the Ogre's grabs 510010, tackles 510011; Gyoubu's drifts; the Ape's sword).
const ENABLE_WIRE_DAMAGE: std::ops::RangeInclusive<i64> = 1_000_400..=1_000_404;

/// c9997 HKS GetSpDamage / ExecSpReaction: the wire hit and one of ENABLE_WIRE_DAMAGE0-4 up ->
/// SP_DAMAGE_WIRE, Replanning, W_DamageWire<n> (DamageWire0 / 1: a000_008910 / 008911, the
/// lowest n wins). gap: its DAMAGE_TRANSITION_RANK__4 check (which states refuse it) is not
/// modelled; the THROW branch (5020_WIRE_DAMAGE1 while thrown) neither.
pub(super) fn on_wire_hit(combat: Res<Combat>, mut hits: MessageReader<WireHit>, mut q: Query<(&mut Enemy, &mut Actor)>) {
    for &WireHit(ent) in hits.read() {
        let Ok((mut e, mut a)) = q.get_mut(ent) else { continue };
        if e.is_dead() {
            continue;
        }
        let d = combat.data_of(&a);
        let mut refs: Vec<i64> = if a.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&a.anim, a.t).iter().map(|(_, s)| s.behavior_ref_id).collect() };
        refs.extend(a.resident.iter().filter_map(|id| d.sp_effects.get(&id.to_string()).map(|s| s.behavior_ref_id)));
        let Some(n) = refs.iter().filter(|r| ENABLE_WIRE_DAMAGE.contains(r)).map(|r| r - 1_000_400).min() else { continue };
        if !a.play_state(d, &format!("DamageWire{n}")) {
            continue;
        }
        let len = d.length(&a.anim).max(0.1);
        e.phase_replan = true;
        e.cur_ez = None;
        e.throw_death = None;
        a.move_vel = Vec3::ZERO;
        e.mode = super::Mode::Rising(len);
    }
}
