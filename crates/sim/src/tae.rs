//! TAE (TimeAct) events at run time: what the playing animations say right now.
//!
//! Each animation has a timeline of events. The ones the simulation uses:
//!
//! - `ChrActionFlag` (type 0): cancel windows and state flags, by flag type. The flag types that
//!   gate actions are listed in [`cancel`].
//! - `AddSpEffect` (types 66, 67, 401): applies a SpEffect while the event is active. The effect's
//!   `SpEffectParam.behaviorRefId` is what the scripts query with `env(3036, ref)`.
//! - `SetTurnSpeed` (type 224): turn speed in degrees per second while active.
//! - `ChrPhysicsVelocityChange` (type 920): sets the body velocity from
//!   `ChrPhysicsVelocityChangeParam` when the event starts (jumps).
//! - `AttackBehavior` (1), `BulletBehavior` (2), `PCBehavior` (307) and `IFrames` (954): kept for
//!   combat.
//!
//! Event names and parameter layouts come from DSAnimStudio's Sekiro template
//! (`cache/refs/TAE.Template.SDT.xml`); action-flag meanings are DS3-era community labels checked
//! against which animations carry them, not against the exe.


/// ChrActionFlag types that open action windows, by the action they allow.
pub mod cancel {
}

/// A velocity change for jumps (`ChrPhysicsVelocityChangeParam`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityChange {
    pub fall_type: u8,
    pub horizontal_scale: f32,
    pub vertical_scale: f32,
    pub horizontal: f32,
    pub vertical: f32,
    pub horizontal_angle: f32,
}
