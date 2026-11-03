//! The game's own enemy AI: its compiled Lua 5.0 scripts run in [`lua50`], with the engine
//! side (goal objects, the AI object's methods and the leaf goals) re-implemented in
//! [`runtime`]. [`AiBrain`] turns the active goals into an [`crate::character::NpcControl`].
//!
//! Plugging in: each step, fill an [`AiWorld`] from the NPC and its target and call
//! [`AiBrain::think`]; put the result in `Character::npc` as the stand-in brain does. See
//! `docs/AI.md`.

pub mod brain;
pub mod lua50;
pub mod patrol;
pub mod regions;
pub mod runtime;
pub mod world;

#[cfg(test)]
mod tests;

pub use brain::{AiBrain, AiLoadError, REF_AI_BATTLE, read_luainfo};
pub use runtime::{ActChoice, ActGoal, ActHook, ActSlot, AiEvent, NumberOnActivate};
pub use world::{AiActor, AiWorld, ThinkParams, anim_id_from_clip};
