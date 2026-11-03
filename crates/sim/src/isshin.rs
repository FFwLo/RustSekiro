//! Isshin's finishing throw owns its defeat message, not the following event clip.

use crate::{character::Character, enemy::EnemySpec, tae::TaeKind};

/// Keep the map request until Ashina's message 50 or Sword Saint's message 20 is delivered.
/// This reconstructs the native request hand-off; it does not invent a finishing duration.
pub fn defer_finishing_request(spec: &EnemySpec, character: &Character, command: i32) -> bool {
    if !matches!(spec.chr.as_str(), "c5400" | "c5430") || command != 20200 {
        return false;
    }
    let Some(clip) = character.behavior.runtime.main_clip() else {
        return false;
    };
    let message = if spec.chr == "c5400" { 20 } else { 50 };
    character.tae.anim(&clip.animation).is_some_and(|anim| {
        anim.events
            .iter()
            .any(|event| event.kind == TaeKind::EzStateRequest(message) && event.end > clip.time)
    })
}
