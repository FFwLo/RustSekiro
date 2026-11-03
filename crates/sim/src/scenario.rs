//! Scripted input sequences for the player, frame by frame.

use crate::player::{PlayerBehavior, TickReport, arm};

pub const FRAME: f32 = 1.0 / 30.0;

/// Behaviour reference id of the guard-combo window (repeated deflect presses).
pub const REF_GUARD_COMBO: i32 = 212;

/// Guard input for one frame: requested (pressed this frame) and/or held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    Released,
    Pressed,
    Held,
}

fn apply_guard(player: &mut PlayerBehavior, g: Guard) {
    let env = &mut player.env;
    env.requested.remove(&arm::GUARD);
    match g {
        Guard::Released => {
            env.held_ms.remove(&arm::GUARD);
        }
        Guard::Pressed => {
            env.requested.insert(arm::GUARD);
            env.held_ms.insert(arm::GUARD, FRAME * 1000.0);
        }
        Guard::Held => {
            *env.held_ms.entry(arm::GUARD).or_insert(0.0) += FRAME * 1000.0;
        }
    }
}

/// Runs `inputs`, one per frame. With `combo_window`, behaviour reference 212 is reported active
/// whenever the main state is one of the stand-to-guard states; this stands in for the TAE event
/// that opens the window until TAE and SpEffects drive `env(3036)`.
pub fn run(player: &mut PlayerBehavior, inputs: &[Guard], combo_window: bool) -> Vec<TickReport> {
    let mut out = Vec::with_capacity(inputs.len());
    for &g in inputs {
        apply_guard(player, g);
        if combo_window {
            let in_guard_start = player
                .runtime
                .main_state_path()
                .last()
                .is_some_and(|s| s.starts_with("StandToDeflectGuard"));
            if in_guard_start {
                player.env.sp_effect_refs.insert(REF_GUARD_COMBO);
            } else {
                player.env.sp_effect_refs.remove(&REF_GUARD_COMBO);
            }
        }
        out.push(player.tick(FRAME));
    }
    out
}

/// Ticks until the character stands idle (the load starts in a map-enter animation), up to
/// `max_frames`. Returns the number of frames used.
pub fn settle_to_idle(player: &mut PlayerBehavior, max_frames: usize) -> usize {
    for i in 0..max_frames {
        if player.runtime.main_state_path().last().map(String::as_str) == Some("StandIdle") {
            return i;
        }
        player.tick(FRAME);
    }
    max_frames
}

/// Idle 30 frames, press and hold guard for 20 frames, release, then 40 frames to settle.
pub fn guard_hold() -> Vec<Guard> {
    let mut v = vec![Guard::Released; 30];
    v.push(Guard::Pressed);
    v.extend(std::iter::repeat_n(Guard::Held, 19));
    v.extend(std::iter::repeat_n(Guard::Released, 40));
    v
}

/// Idle 10 frames, then four quick guard presses (press, hold 2, release 3).
pub fn guard_repeat() -> Vec<Guard> {
    let mut v = vec![Guard::Released; 10];
    for _ in 0..4 {
        v.push(Guard::Pressed);
        v.extend([Guard::Held, Guard::Held]);
        v.extend([Guard::Released; 3]);
    }
    v.extend(std::iter::repeat_n(Guard::Released, 40));
    v
}
