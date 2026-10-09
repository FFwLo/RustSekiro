//! Render interpolation. The simulation moves actors in 60 Hz fixed steps (Sekiro's rate), but
//! the screen can refresh faster: drawn as-is, an actor holds still for some frames and jumps on
//! others while the camera moves every frame, which smears motion across the view (most visible
//! running sideways). Each frame draws the actor between its last two simulated poses by the
//! fixed clock's overstep; the next fixed step starts from the exact simulated pose again.
//! Game window only: tests read the simulated poses directly.

use bevy::prelude::*;

use crate::actor::Actor;

/// Where between the last two simulated steps this frame is drawn (0 = previous, 1 = latest).
/// Animation poses use it too (anim.rs). Absent (tests) = the latest step.
#[derive(Resource, Clone, Copy)]
pub struct DrawAlpha(pub f32);

/// Last two simulated poses and the pose drawn this frame.
#[derive(Component, Default)]
struct SimPose {
    prev: Option<(Vec3, Quat)>,
    cur: Option<(Vec3, Quat)>,
    drawn: Option<(Vec3, Quat)>,
}

pub struct InterpPlugin;

impl Plugin for InterpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedFirst, restore_sim_pose)
            .add_systems(FixedLast, store_sim_pose)
            .add_systems(RunFixedMainLoop, draw_between.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop));
    }
}

/// Undo the drawn pose before simulating. A pose changed outside the simulation since it was
/// drawn (photo mode, resets) is kept as a teleport.
fn restore_sim_pose(mut commands: Commands, mut q: Query<(Entity, &mut Transform, Option<&mut SimPose>), With<Actor>>) {
    for (e, mut tf, pose) in &mut q {
        let Some(mut pose) = pose else {
            commands.entity(e).insert(SimPose::default());
            continue;
        };
        let now = (tf.translation, tf.rotation);
        if let (Some(cur), Some(drawn)) = (pose.cur, pose.drawn) {
            if now == drawn {
                tf.translation = cur.0;
                tf.rotation = cur.1;
            }
        }
        pose.prev = Some((tf.translation, tf.rotation));
        pose.drawn = None;
    }
}

fn store_sim_pose(mut q: Query<(&Transform, &mut SimPose), With<Actor>>) {
    for (tf, mut pose) in &mut q {
        pose.cur = Some((tf.translation, tf.rotation));
        if pose.prev.is_none() {
            pose.prev = pose.cur;
        }
    }
}

fn draw_between(mut commands: Commands, time: Res<Time<Fixed>>, mut q: Query<(&mut Transform, &mut SimPose), With<Actor>>) {
    let alpha = time.overstep_fraction();
    commands.insert_resource(DrawAlpha(alpha));
    for (mut tf, mut pose) in &mut q {
        let (Some(prev), Some(cur)) = (pose.prev, pose.cur) else { continue };
        // Only when the simulated pose is still what the last step left (nothing moved it since).
        if (tf.translation, tf.rotation) != cur && pose.drawn.map_or(true, |d| (tf.translation, tf.rotation) != d) {
            continue;
        }
        tf.translation = prev.0.lerp(cur.0, alpha);
        tf.rotation = prev.1.slerp(cur.1, alpha);
        pose.drawn = Some((tf.translation, tf.rotation));
    }
}
