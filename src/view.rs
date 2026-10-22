//! The combat core's side of the camera: what the host game's camera tells it and what it asks of
//! the camera. camera.rs (this game's Sekiro camera) fills `View` after it moves; another game's
//! camera can fill it the same way.

use bevy::prelude::*;

/// The lock-on target (set by the camera's lock-on, read by Wolf, the scripts and the arts).
#[derive(Resource, Default)]
pub struct LockOn {
    pub target: Option<Entity>,
}

/// Set when Wolf is placed (the first frame, a retry): the camera jumps behind him at the
/// starting pitch instead of chasing there from where it was.
#[derive(Resource)]
pub struct CameraSnap(pub bool);

impl Default for CameraSnap {
    fn default() -> Self {
        Self(true)
    }
}

/// The camera as the core reads it, `None` without a camera (the sim tests).
#[derive(Resource, Default, Clone, Copy)]
pub struct View {
    /// The orbit's yaw and pitch (radians): the stick turns by the yaw.
    pub orbit: Option<(f32, f32)>,
    /// The camera's place and rotation (its -Z looks ahead): the grapple aim.
    pub eye: Option<(Vec3, Quat)>,
}

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    // Added by the core (player.rs) and by the camera; the resources are made once.
    fn is_unique(&self) -> bool {
        false
    }

    fn build(&self, app: &mut App) {
        app.init_resource::<LockOn>().init_resource::<CameraSnap>().init_resource::<View>();
    }
}
