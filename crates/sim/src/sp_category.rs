//! SpEffect categories: `SpEffectParam.spCategory` with `categoryPriority`. An effect applied later
//! replaces the active effects of its category that have the same priority. This is inferred from
//! data, not traced in the executable: the Demon of Hatred's NpcParam residents share category 100
//! with the priorities 3 (200030), 1 (200004), 20 (221002) and 4 (3702001), so they coexist, which
//! reads as one effect per category and priority. Its heat-up clip adds 3702002 (category 100,
//! priority 4), after which the Lua's `HasSpecialEffectId(3702001)` is false and the second
//! phase's act table takes over; the anim-set effects 200030-34 (category 100, priority 3) replace
//! one another the same way (FwLo's notes record the anim-set overwrite).
//! The duel applies it to the Demon only until other bosses are re-verified with it.

use crate::tae::TaeDb;

/// The effects among `active` that applying `new` replaces.
pub fn replaced(db: &TaeDb, new: i32, active: impl IntoIterator<Item = i32>) -> Vec<i32> {
    let Some(info) = db.sp_effects.get(&new) else {
        return Vec::new();
    };
    if info.category <= 0 {
        return Vec::new();
    }
    active
        .into_iter()
        .filter(|&other| {
            other != new
                && db
                    .sp_effects
                    .get(&other)
                    .is_some_and(|o| o.category == info.category && o.priority == info.priority)
        })
        .collect()
}
