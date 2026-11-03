//! Fountainhead Monk's authored get-up stat changes and active-group finishing throw.

use std::path::Path;

use crate::{enemy::EnemySpec, fight::Fighter, tae::TaeFrame};

pub fn owns(spec: &EnemySpec) -> bool {
    spec.chr == "c5000" && spec.npc_param == 50_000_000
}

/// The finishing defender clip owns message 40, which enables the centipede prop.
/// Preserve that clip before accepting the map's queued 20200 continuation.
/// Request arbitration is inferred from the two installed timelines, not a fixed delay.
pub fn finishing_message_pending(
    spec: &EnemySpec,
    character: &crate::character::Character,
) -> bool {
    if !owns(spec) {
        return false;
    }
    let Some(clip) = character.behavior.runtime.main_clip() else {
        return false;
    };
    clip.animation == "a100_013700"
        && character.tae.anim(&clip.animation).is_some_and(|anim| {
            anim.events.iter().any(|event| {
                event.kind == crate::tae::TaeKind::EzStateRequest(40) && clip.time < event.end
            })
        })
}

pub struct PhaseStats {
    rates: [(i32, f32, f32); 2],
    applied: Option<i32>,
}

impl PhaseStats {
    pub fn load(cache: &Path) -> Option<Self> {
        let table = crate::hits::load_table(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
            "SpEffectParam",
            "SpEffect",
        )?;
        let rate = |id| {
            let row = table.find(id)?;
            Some((
                id,
                row.f32("maxHpRate").ok()?,
                row.f32("maxStaminaRate").ok()?,
            ))
        };
        Some(Self {
            rates: [rate(275000)?, rate(275001)?],
            applied: None,
        })
    }

    /// Category 100 replaces the previous phase modifier; rates use the original NPC row.
    /// Application before the simultaneous HP-reset event is inferred from the TAE.
    pub fn apply(&mut self, fighter: &mut Fighter, frame: &TaeFrame) {
        let crate::fight::FighterKind::Npc(npc) = &fighter.kind else {
            return;
        };
        for &(id, hp, posture) in &self.rates {
            if frame.sp_effects.contains(&id) && self.applied != Some(id) {
                fighter.vitality.max_hp = (npc.hp as f32 * hp) as i32;
                fighter.posture.max = (npc.max_posture as f32 * posture) as i32;
                fighter.vitality.hp = fighter.vitality.hp.min(fighter.vitality.max_hp);
                fighter.posture.remaining = fighter.posture.remaining.min(fighter.posture.max);
                self.applied = Some(id);
            }
        }
    }
}
