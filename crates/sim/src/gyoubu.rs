//! Gyoubu's mounted encounter uses the a225 finishing family in installed ThrowParam.
use crate::deathblow::{Placement, ThrowTable};

/// The 170/171 execution is distinct from the reusable 0/1 mounted posture deathblow.
/// Selecting this family on the last mark is inferred from its paired animation data.
pub fn finishing_rows(
    table: &ThrowTable,
    attacker: &Placement,
    defender: &Placement,
) -> Option<(i32, i32)> {
    let start = table.rows.iter().find(|r| r.id == 15_080_170)?;
    let body = table.rows.iter().find(|r| r.id == 15_080_171)?;
    (start.atk_chr == 0
        && body.atk_chr == 0
        && start.def_chr == 5080
        && body.def_chr == 5080
        && start.in_range(attacker, defender))
    .then_some((start.id, body.id))
}
