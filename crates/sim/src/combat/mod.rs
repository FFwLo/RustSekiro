//! Engine-side combat rules: hit resolution, HP and posture damage, posture recovery and the
//! damage codes the player script reads.
//!
//! Everything here is original Rust written from our reading of `sekiro.exe` 1.6.0.0 in Ghidra
//! (see `docs/COMBAT-RULES.md` for the rule list, the source RVAs and the open questions).
//! Every formula carries a source tag:
//!
//! - **read from code**: the arithmetic, order and rounding were read from the named function.
//! - **inferred**: the shape was read from code but the meaning of an input (which param field
//!   fills a record slot, for example) was inferred from matching offsets or values.
//! - **def** or **community**: taken from the Paramdex description or a community source.
//!
//! Sekiro's data calls posture "stamina". The engine stores the *remaining* posture: it starts at
//! the maximum, damage lowers it, and the bar on screen is `max - remaining`. A posture break is
//! the remaining value dropping below 1.
//!
//! All arithmetic that the engine does in single precision is done in `f32` here, and every
//! float-to-integer conversion truncates toward zero like the engine's `cvttss2si`, unless a
//! function says otherwise.

pub mod damage_type;
pub mod effects;
pub mod hit;
pub mod hp;
pub mod knockback;
pub mod params;
pub mod posture;
pub mod recovery;

#[cfg(test)]
mod tests;

pub use damage_type::{AttackerReaction, DefenderReaction, attacker_reaction, defender_reaction};
pub use effects::{ActiveEffect, StaminaAttribute};
pub use hit::{
    GuardInput, HitOutcome, HitResolution, player_attack_repel, resolve_hit,
    resolve_hit_with_repel, resolve_part_hit,
};
pub use hp::Vitality;
pub use knockback::{
    HIT_STOP_HOLD, HitStop, Knockback, KnockbackClass, KnockbackTable, KnockbackTiming,
    attacker_hit_stop_scale, defender_hit_stop_scale,
};
pub use posture::{ControlRange, PostureMeter};
pub use recovery::{RecoveryAccumulator, RecoveryInputs, posture_recovery_per_second};

/// SpEffect `stateInfo` of the guard (block) state effects, such as the just-guard window 105010.
/// The engine reads guard multipliers from these while a hit is blocked (read from code,
/// `bfafa0`, `bfcd40`, `bfccc0`).
pub const STATE_INFO_GUARD: u16 = 158;

/// SpEffect `stateInfo` of the deflect (just guard) effects, such as 105020..105023.
/// The engine reads deflect multipliers from these while a hit is deflected (read from code,
/// `bfafa0`, `bfcd40`, `bfc490`).
pub const STATE_INFO_JUST_GUARD: u16 = 204;

/// Truncates toward zero, like the engine's float-to-int conversion.
pub(crate) fn trunc(v: f32) -> i32 {
    v as i32
}
