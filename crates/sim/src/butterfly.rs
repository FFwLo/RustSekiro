//! Butterfly's second placed character uses the a244 ThrowParam family.
//! Row identities are from the installed params, not an authored combat timeline.

use crate::deathblow::{Placement, ThrowTable};

/// The event may clear the second entity's entrance effect while the first throw still holds
/// the primary slot. Carry that authored clear to the newly loaded combatant.
pub fn entrance_effect_cleared(scripts: &crate::events::EventScripts) -> bool {
    scripts
        .cast_scene
        .character_effects
        .get(&(1_000_810, 3_509_210))
        == Some(&false)
}

/// The first bar includes the later entity's NpcParam marks, without changing either vitality.
pub fn additional_marks(cache: &std::path::Path, spec: &crate::enemy::EnemySpec) -> i32 {
    if spec.chr != "c5090" || spec.npc_param != 50_900_000 {
        return 0;
    }
    crate::registry::Registry::load(cache)
        .ok()
        .and_then(|registry| {
            registry.npc_row(50_900_001)?["ninsatuNum"]
                .as_i64()
                .map(|marks| marks as i32)
        })
        .unwrap_or(0)
}

/// Both placed characters have one mark, but only 1000810 owns the finishing execution.
/// The family choice is inferred from the authored a244 pair and second entity's ending event.
pub fn second_phase_deathblow(
    table: &ThrowTable,
    attacker: &Placement,
    defender: &Placement,
) -> Option<(i32, i32)> {
    let start = table.rows.iter().find(|row| row.id == 15_090_680)?;
    let body = table.rows.iter().find(|row| row.id == 15_090_681)?;
    (start.atk_chr == 0
        && body.atk_chr == 0
        && start.def_chr == 5090
        && body.def_chr == 5090
        && start.in_range(attacker, defender))
    .then_some((start.id, body.id))
}
