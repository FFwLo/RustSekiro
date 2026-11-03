//! Player input: logical buttons and the movement stick, translated into what the script reads.
//!
//! The script never sees keys. It asks the engine:
//!
//! - `env(1106, arm)`: is the action requested now. The engine latches a press inside the
//!   animation's accept window and reports it inside its execute window (see [`accept_flags`]).
//!   Starting an action calls `act(9101)`, which clears the buffer.
//! - `env(1108, arm)`: how long the button has been held. The script compares it with 0 and, for
//!   sprinting in swamps, with 200, so milliseconds is our working unit (unverified).
//! - Behaviour variables the engine writes every frame: `MoveSpeedLevel` (0 to 2, stick
//!   magnitude), `TurnAngle` and `MoveAngle` (degrees from the character's facing to the stick
//!   direction, positive to the right), and the per-action copies `JumpAngle`, `AttackAngle`,
//!   `JumpStickLevel`, `AttackStickLevel` and so on.
//!
//! Action-arm ids (`ACTION_ARM_*` in the define script): attack 0, prosthetic 1, guard 2, grapple
//! 3, jump 4, step/dodge 5, item 7, crouch 15, combat art 32. Sekiro's dodge button steps on a
//! press and sprints while held, which the script reads as 1106 and 1108 of arm 5.


/// Action-arm ids, as the script names them.
pub mod arm {
}

/// Signed smallest angle `to - from`, radians in (-pi, pi].
pub fn angle_diff(to: f32, from: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d <= -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}
