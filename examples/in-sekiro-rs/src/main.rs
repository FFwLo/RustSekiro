//! RustSekiro's combat core in another Bevy game: sekiro-rs's `--map` mode draws the map and its
//! lighting; the core (built without the `host` feature) adds Wolf and the config's enemy, who
//! stand and move on sekiro-rs's play-world collision. A follow camera stands in for the host's.
//! Needs sekiro-rs next to this repo with `map::APP_HOOKS` (its PR 185) and its cache.
//!
//! Build and run from the sekiro-rs folder (its cache is found from there):
//!   cargo build --release --manifest-path examples/in-sekiro-rs/Cargo.toml
//!   in-sekiro-rs --map m11_01_00_00 --camera 199.0,-22.9,220.6,-23.55,-12 [--screenshot out.png --at 25]

use std::sync::Arc;

use bevy::prelude::*;

/// His play world's collision behind our core's interface.
struct HisWorld(sekiro_sim::collision::CollisionWorld);

impl rustsekiro::WorldCollision for HisWorld {
    fn floor_hit(&self, x: f32, z: f32, top: f32) -> Option<(f32, u32)> {
        self.0.floor_hit(x, z, top)
    }
    fn floor_material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        self.0.floor_material(x, z, top)
    }
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.0.raycast(origin, dir, max)
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.0.slide(feet, delta)
    }
    fn triangle_count(&self) -> usize {
        self.0.triangle_count()
    }
}

/// The map the fight is on and its arena's place in it (the m11_01 arena RustSekiro starts at:
/// its log's "fight space").
const MAP: &str = "m11_01_00_00";
const YAW: f32 = -0.410_990_66;
const OFFSET: Vec3 = Vec3::new(202.601_93, -25.926, 212.379_9);

fn before(app: &mut App) {
    rustsekiro::register_assets(app);
}

fn after(app: &mut App) {
    let cache = sekiro_sim::cache::root();
    let t0 = std::time::Instant::now();
    let world = (|| -> anyhow::Result<_> {
        let layouts = sekiro_rs::map::world::play_blocks(&cache, MAP)?;
        let stories = sekiro_rs::map::story::load_blocks(&cache, &layouts)?;
        sekiro_rs::map::world::collision_world(&cache, &layouts, &stories)
    })();
    match world {
        Ok(w) => {
            info!("trial: his play-world collision, {} triangles, {:.1} s", w.triangle_count(), t0.elapsed().as_secs_f32());
            rustsekiro::Terrain::from_world(Arc::new(HisWorld(w))).install();
        }
        Err(e) => warn!("trial: no collision ({e}); the core's flat floor"),
    }
    rustsekiro::set_frame(rustsekiro::Frame { rot: Quat::from_rotation_y(YAW), yaw: YAW, offset: OFFSET });
    app.add_plugins(rustsekiro::CombatCorePlugin);
    app.insert_resource(Orbit { yaw: YAW, pitch: -0.3 });
    app.add_systems(PostUpdate, follow_wolf.before(bevy::transform::TransformSystems::Propagate));
}

/// A third-person camera behind our Wolf (in place of his fly camera, which it overrides): the
/// mouse turns it, as in RustSekiro.
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
}

fn follow_wolf(
    motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut orbit: ResMut<Orbit>,
    wolf: Query<&Transform, (With<rustsekiro::Player>, Without<Camera3d>)>,
    mut cams: Query<&mut Transform, With<Camera3d>>,
    mut view: ResMut<rustsekiro::View>,
) {
    orbit.yaw -= motion.delta.x * 0.004;
    orbit.pitch = (orbit.pitch - motion.delta.y * 0.004).clamp(-1.2, 0.6);
    let (Ok(w), Ok(mut cam)) = (wolf.single(), cams.single_mut()) else { return };
    let focus = w.translation + Vec3::Y * 0.6;
    let arm = Quat::from_euler(EulerRot::YXZ, orbit.yaw, orbit.pitch, 0.0) * Vec3::Z * 4.5;
    *cam = Transform::from_translation(focus + arm).looking_at(focus, Vec3::Y);
    // The core's view: the stick turns by the yaw; the grapple aims from the eye.
    *view = rustsekiro::View { orbit: Some((orbit.yaw, orbit.pitch)), eye: Some((cam.translation, cam.rotation)) };
}

fn main() -> anyhow::Result<()> {
    let _ = sekiro_rs::map::APP_HOOKS.set((before, after));
    sekiro_rs::run()
}
