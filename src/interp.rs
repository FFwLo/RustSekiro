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

/// How far from the actor's previous simulated pose to its latest a frame is drawn. An actor on
/// an update level (Actor::lod: the open world's far enemies step once every 5 or 30 ticks with
/// that many ticks' time) is drawn across its whole window, so it moves every frame instead of
/// holding still and jumping (they twitched); every tick's actors take the fixed clock's overstep.
pub(crate) fn draw_fraction(alpha: f32, a: &Actor) -> f32 {
    let lod = a.lod.max(1);
    ((a.lod_phase.min(lod - 1) as f32 + alpha) / lod as f32).clamp(0.0, 1.0)
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
        match (pose.cur, pose.drawn) {
            (Some(cur), Some(drawn)) if now == drawn => {
                tf.translation = cur.0;
                tf.rotation = cur.1;
            }
            (Some(cur), _) if now == cur => {}
            _ => {
                pose.prev = Some(now);
                pose.cur = Some(now);
            }
        }
        pose.drawn = None;
    }
}

/// The step's pose: on an actor's due tick the window moves on (the latest pose becomes the one
/// it is drawn from); between them nothing it does changes.
fn store_sim_pose(mut q: Query<(&Transform, &Actor, &mut SimPose)>) {
    for (tf, a, mut pose) in &mut q {
        let now = (tf.translation, tf.rotation);
        if a.lod_due || pose.cur.is_none() {
            pose.prev = pose.cur.or(Some(now));
            pose.cur = Some(now);
        } else if pose.cur != Some(now) {
            // Moved off its due tick (a script warp): drawn there at once.
            pose.prev = Some(now);
            pose.cur = Some(now);
        }
        // A teleport inside the step (Wolf's reset, an event or script warp) is drawn as one,
        // not as a slide across the map (map::TELEPORT: longer than any one-tick move).
        if pose.prev.is_none_or(|p| p.0.distance(tf.translation) > crate::terrain::TELEPORT) {
            pose.prev = pose.cur;
        }
    }
}

fn draw_between(mut commands: Commands, time: Res<Time<Fixed>>, mut q: Query<(&mut Transform, &Actor, &mut SimPose)>) {
    let alpha = time.overstep_fraction();
    commands.insert_resource(DrawAlpha(alpha));
    for (mut tf, a, mut pose) in &mut q {
        let (Some(prev), Some(cur)) = (pose.prev, pose.cur) else { continue };
        // Only when the simulated pose is still what the last step left (nothing moved it since).
        if (tf.translation, tf.rotation) != cur && pose.drawn.map_or(true, |d| (tf.translation, tf.rotation) != d) {
            continue;
        }
        let k = draw_fraction(alpha, a);
        tf.translation = prev.0.lerp(cur.0, k);
        tf.rotation = prev.1.slerp(cur.1, k);
        pose.drawn = Some((tf.translation, tf.rotation));
    }
}
