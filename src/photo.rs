//! Photo mode for cheap, repeatable visual checks. With the environment variable
//! SHINOBI_PHOTO=<dir> the game places the enemy in front of Wolf, takes a fixed list of shots
//! (camera around Wolf, then Wolf mid-slash), saves them as <dir>/<name>.png and exits.
//! tools/photo.ps1 runs it and tiles the shots into one small contact sheet.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::actor::Actor;
use crate::camera::{LockOn, OrbitCamera};
use crate::data::{Combat, TAE_FPS};
use crate::enemy::Enemy;
use crate::player::Player;

/// (file name, camera yaw offset from Wolf's facing [deg], camera pitch [deg], Wolf state + TAE frame).
/// The last shot is locked on to the enemy (TAE 700 0_Twist: head and torso turn toward it).
const SHOTS: [(&str, f32, f32, Option<(&str, f32)>); 4] = [
    ("1_back", 25.0, 12.0, None),
    ("2_front", 180.0, 8.0, None),
    ("3_slash", 60.0, 10.0, Some(("GroundAttackCombo1", 9.0))),
    ("4_lock_front", 160.0, 8.0, None),
];
/// Seconds before the first shot (models and textures load) and between shots.
const WARMUP: f32 = 4.0;
const SETTLE: f32 = 0.8;

#[derive(Resource)]
struct Photo {
    /// Shots: built-in SHOTS, or SHINOBI_PHOTO_POSES="State:frame,State:frame" (side view each).
    shots: Vec<(String, f32, f32, Option<(String, f32)>, bool)>,
    dir: String,
    /// Wolf's spawn position: he is put back there every frame (held poses keep no root motion).
    home: Option<Vec3>,
    step: usize,
    timer: f32,
    taken: bool,
}

pub struct PhotoPlugin;

impl Plugin for PhotoPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("SHINOBI_PHOTO") {
            let _ = std::fs::create_dir_all(&dir);
            let shots = match std::env::var("SHINOBI_PHOTO_POSES") {
                Ok(list) => list
                    .split(',')
                    .filter_map(|p| {
                        let (st, fr) = p.trim().split_once(':')?;
                        Some((st.to_string(), fr.parse::<f32>().ok()?))
                    })
                    .enumerate()
                    .map(|(i, (st, fr))| (format!("{:02}_{st}_{fr}", i + 1), 70.0, 8.0, Some((st, fr)), false))
                    .collect(),
                Err(_) => SHOTS
                    .iter()
                    .enumerate()
                    .map(|(i, &(n, y, p, pose))| (n.to_string(), y, p, pose.map(|(st, fr)| (st.to_string(), fr)), i == SHOTS.len() - 1))
                    .collect(),
            };
            app.insert_resource(Photo { shots, dir, home: None, step: 0, timer: -WARMUP, taken: false })
                .add_systems(Update, run_photo);
        }
    }
}

fn run_photo(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    mut photo: ResMut<Photo>,
    mut lock: ResMut<LockOn>,
    mut player: Query<(&mut Actor, &mut Transform), (With<Player>, Without<Enemy>)>,
    mut enemy: Query<(&mut Actor, &mut Transform), (With<Enemy>, Without<Player>)>,
    enemy_e: Query<Entity, With<Enemy>>,
    mut camera: Query<&mut OrbitCamera>,
    mut exit: MessageWriter<AppExit>,
) {
    photo.timer += time.delta_secs();
    let n_shots = photo.shots.len();
    let locked = photo.shots.get(photo.step).is_some_and(|s| s.4);
    lock.target = if locked { enemy_e.single().ok() } else { None };
    let Ok((mut wolf, mut wtf)) = player.single_mut() else { return };
    let home = *photo.home.get_or_insert(wtf.translation);
    wtf.translation = home;
    if photo.step >= n_shots {
        if photo.timer > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (name, yaw_off, pitch, pose, _) = photo.shots[photo.step].clone();
    // The enemy stands 2.6 m ahead and 1.4 m to Wolf's left, facing him, so the front view
    // sees both.
    if let Ok((mut ea, mut etf)) = enemy.single_mut() {
        let fwd = wolf.forward();
        let left = Vec3::Y.cross(fwd);
        let at = wtf.translation + fwd * 2.6 + left * 1.4;
        etf.translation = Vec3::new(at.x, etf.translation.y, at.z);
        let to_wolf = (wtf.translation - at).with_y(0.0);
        ea.yaw = f32::atan2(-to_wolf.x, -to_wolf.z);
        // Held in its battle idle so nothing swings during the shots.
        hold(&mut ea, &combat.enemy, "IdleBattle", 0.0);
    }
    // Wolf holds the shot's pose (re-applied every frame), else his standing idle.
    match &pose {
        Some((state, frame)) => hold(&mut wolf, &combat.player, state, *frame),
        None => hold(&mut wolf, &combat.player, "StandIdle", 0.0),
    }
    if let Ok(mut cam) = camera.single_mut() {
        cam.yaw = wolf.yaw + yaw_off.to_radians();
        cam.pitch = pitch.to_radians();
    }
    if photo.timer >= SETTLE && !photo.taken {
        let path = format!("{}/{name}.png", photo.dir);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        photo.taken = true;
    }
    if photo.timer >= SETTLE + 0.3 {
        photo.step += 1;
        photo.timer = 0.0;
        photo.taken = false;
    }
}

/// Keeps an actor in `state` at TAE `frame` (procedural when the state has no anim).
/// `state` can also be a raw anim key ("a106_316000"), e.g. combat-art anims.
fn hold(a: &mut Actor, data: &crate::data::CharData, state: &str, frame: f32) {
    if a.state != state {
        if data.anim(state).is_some() {
            a.play(state, state);
        } else if !a.play_state(data, state) {
            a.procedural(state);
        }
    }
    a.t = frame / TAE_FPS;
    a.prev_t = a.t;
    a.move_vel = Vec3::ZERO;
    // Weapon style at that frame (TAE 32 SetWeaponStyle up to it; drawn by default), so the sword
    // sits where the pose has it however many sim steps ran while loading.
    let t = a.t;
    a.sheathed = data
        .anim(&a.anim)
        .and_then(|an| an.events.iter().filter(|e| e.kind == 32 && e.start <= t).filter_map(|e| e.args.get("WeaponStyle")?.as_str()).last())
        .is_some_and(|s| s.starts_with('0'));
}
