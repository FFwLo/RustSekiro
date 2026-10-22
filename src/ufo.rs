//! UFO (debug menu, PLAYER): no-clip flight for looking round a map. Wolf leaves the state
//! machine (player.rs `decide` skips him), gravity and the terrain (map.rs `bodies_on_terrain`
//! slides nothing: his last place is where he flies to). W A S D fly along the camera's view,
//! Space up, Ctrl down, Shift fast. Switched off, he stands on the floor under him.

use std::sync::atomic::Ordering;

use bevy::prelude::*;

use crate::actor::Actor;
use crate::camera::OrbitCamera;
use crate::player::{CAPSULE_HALF_HEIGHT, Player};

/// Flight speeds (m/s): normal and with Shift.
const SPEED: f32 = 12.0;
const FAST: f32 = 45.0;

pub fn on() -> bool {
    crate::player::FLOWN.load(Ordering::Relaxed)
}

pub fn set(on: bool) {
    crate::player::FLOWN.store(on, Ordering::Relaxed);
}

pub struct UfoPlugin;

impl Plugin for UfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, fly.after(crate::terrain::bodies_on_terrain).before(crate::anim::PoseSet));
    }
}

/// Wolf's flight: `at` is where he hangs while UFO is on (None off).
fn fly(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    combat: Res<crate::data::Combat>,
    camera: Query<&Transform, (With<OrbitCamera>, Without<Player>)>,
    mut wolf: Query<(&mut Transform, &mut Actor, Option<&mut crate::terrain::OnTerrain>), With<Player>>,
    mut at: Local<Option<Vec3>>,
) {
    let Ok((mut tf, mut a, terrain)) = wolf.single_mut() else { return };
    if !on() {
        // Landing: onto the floor under him, as from a standstill (no fall from the flight).
        if at.take().is_some() {
            tf.translation.y = crate::terrain::ground_y(tf.translation) + CAPSULE_HALF_HEIGHT;
            (a.airborne, a.vel_y, a.air_base) = (false, 0.0, tf.translation);
            if let Some(mut t) = terrain {
                t.prev = tf.translation;
            }
        }
        return;
    }
    let pos = at.get_or_insert_with(|| {
        a.play_state(&combat.player, "StandIdle");
        tf.translation
    });
    let mut v = Vec3::ZERO;
    if let (Some(keys), Ok(cam)) = (keys, camera.single()) {
        let (fwd, right) = (cam.forward().as_vec3(), cam.right().as_vec3());
        for (key, dir) in [(KeyCode::KeyW, fwd), (KeyCode::KeyS, -fwd), (KeyCode::KeyD, right), (KeyCode::KeyA, -right), (KeyCode::Space, Vec3::Y), (KeyCode::ControlLeft, Vec3::NEG_Y)] {
            if keys.pressed(key) {
                v += dir;
            }
        }
        v = v.normalize_or_zero() * if keys.pressed(KeyCode::ShiftLeft) { FAST } else { SPEED };
    }
    *pos += v * time.delta_secs();
    // Facing the way he flies (yaw as player.rs `yaw_of`).
    if v.xz().length_squared() > 1e-4 {
        a.yaw = f32::atan2(-v.x, -v.z);
    }
    tf.translation = *pos;
    tf.rotation = Quat::from_rotation_y(a.yaw);
    (a.airborne, a.vel_y, a.air_base) = (true, 0.0, *pos);
    if let Some(mut t) = terrain {
        t.prev = *pos;
    }
}
