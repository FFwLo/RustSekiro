//! Throw geometry trace, the game-side twin of tools/rec_throw.py. With
//! SHINOBI_THROW_TRACE=<kind> (front, behind, vault) the game breaks the enemy, puts it in front
//! of Wolf (facing him, or away from him for "behind"), presses the action and prints one line per
//! fixed frame: both anims, the enemy's distance, its position in Wolf's frame (forward / right)
//! and the yaw difference, then exits. SHINOBI_THROW_DIST sets the starting distance (m).

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::Combat;
use crate::enemy::{AiMode, Enemy, EnemyDebug};
use crate::player::{Action, PadInput, Player};

#[derive(Resource)]
struct ThrowTrace {
    kind: String,
    dist: f32,
    frame: u32,
    started: Option<u32>,
    kicked: bool,
}

pub struct ThrowTracePlugin;

impl Plugin for ThrowTracePlugin {
    fn build(&self, app: &mut App) {
        let Ok(kind) = std::env::var("SHINOBI_THROW_TRACE") else { return };
        let dist = std::env::var("SHINOBI_THROW_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(1.0);
        app.insert_resource(ThrowTrace { kind, dist, frame: 0, started: None, kicked: false })
            .add_systems(FixedPostUpdate, step)
            .add_systems(Update, hold_stick.after(crate::player::read_input))
            .add_systems(PostUpdate, render_cam.after(bevy::transform::TransformSystems::Propagate));
    }
}

#[allow(clippy::type_complexity)]
fn step(
    mut tr: ResMut<ThrowTrace>,
    combat: Res<Combat>,
    mut pad: ResMut<PadInput>,
    debug: Option<ResMut<EnemyDebug>>,
    mut wolf: Query<(&mut Actor, &mut Transform), (With<Player>, Without<Enemy>)>,
    mut enemy: Query<(&mut Enemy, &mut Actor, &mut Transform), Without<Player>>,
    mut cam: Query<(&mut crate::camera::OrbitCamera, &Transform), (Without<Player>, Without<Enemy>)>,
    mut lock: ResMut<crate::camera::LockOn>,
    enemy_e: Query<Entity, With<Enemy>>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    tr.frame += 1;
    // SHINOBI_THROW_SHOTS=<dir>: screenshots; SHINOBI_THROW_SHOT_RANGE="from,to,step" (frames after
    // the press, default 30,120,3).
    if let (Some(s), Ok(dir)) = (tr.started, std::env::var("SHINOBI_THROW_SHOTS")) {
        let f = tr.frame - s;
        let r: Vec<u32> = std::env::var("SHINOBI_THROW_SHOT_RANGE").unwrap_or("30,120,3".into()).split(',').filter_map(|v| v.parse().ok()).collect();
        if r.len() == 3 && (r[0]..=r[1]).contains(&f) && (f - r[0]) % r[2] == 0 {
            let path = format!("{dir}/f{f:03}.png");
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    if let Some(mut d) = debug {
        d.mode = AiMode::Idle;
    }
    let (Ok((mut pa, mut ptf)), Ok((mut e, mut ea, mut etf))) = (wolf.single_mut(), enemy.single_mut()) else { return };
    // 3 s for the models to load, then the setup.
    if tr.frame == 180 {
        pa.yaw = std::f32::consts::PI;
        pa.play("StandIdle", "");
        ptf.translation.x = 0.0;
        ptf.translation.z = 0.0;
        let behind = tr.kind == "behind";
        ea.yaw = if behind { std::f32::consts::PI } else { 0.0 };
        etf.translation.x = 0.0;
        etf.translation.z = tr.dist;
        if tr.kind == "idle" || tr.kind == "jump" {
            etf.translation.z = 30.0;
        } else if tr.kind != "kick" && tr.kind != "runkick" {
            ea.posture = ea.posture_max;
            e.on_posture_break(&mut ea, &combat, behind, 8.0);
        }
    }
    if tr.kind == "runkick" {
        // Locked on, running at the enemy (hold_stick), jump, and jump again once over / at it.
        lock.target = enemy_e.single().ok();
        let near = (etf.translation - ptf.translation).with_y(0.0).length() < 1.2;
        if tr.frame == 182 + 20 || (tr.started.is_some() && pa.airborne && near && pa.state.contains("GroundJump") && !tr.kicked) {
            if tr.frame != 182 + 20 {
                tr.kicked = true;
            }
            pad.press(Action::Jump);
        }
        if tr.frame == 182 + 20 {
            tr.started = Some(tr.frame);
        }
    } else if tr.kind == "kick" {
        // Locked on, jump, then jump again on the way down onto the enemy's head.
        lock.target = enemy_e.single().ok();
        if tr.frame == 182 || tr.frame == 182 + 30 {
            pad.press(Action::Jump);
        }
        if tr.frame == 182 {
            tr.started = Some(tr.frame);
        }
    } else if tr.kind == "jump" {
        // Jump in place every second (cloth checks).
        if tr.frame >= 182 && (tr.frame - 182) % 60 == 0 {
            pad.press(Action::Jump);
        }
        if tr.frame == 182 {
            tr.started = Some(tr.frame);
        }
    } else if tr.frame == 182 {
        // "idle": nothing pressed (cloth / pose checks).
        if tr.kind != "idle" {
            pad.press(if tr.kind == "vault" { Action::Jump } else { Action::Attack });
        }
        tr.started = Some(tr.frame);
    }
    // SHINOBI_THROW_CAM_YAW: hold the (unlocked) camera at this angle [deg] around the scene.
    if let (Ok(y), Ok(mut oc)) = (std::env::var("SHINOBI_THROW_CAM_YAW").map(|v| v.parse::<f32>().unwrap_or(90.0)), cam.single_mut()) {
        let oc = &mut oc.0;
        if tr.kind != "kick" && tr.kind != "runkick" {
            oc.yaw = y.to_radians();
            oc.pitch = 10f32.to_radians();
        }
    }
    let Some(s) = tr.started else { return };
    let fwd = pa.forward().with_y(0.0).normalize_or_zero();
    let right = fwd.cross(Vec3::Y);
    let d = (etf.translation - ptf.translation).with_y(0.0);
    let dyaw = ((ea.yaw - pa.yaw).to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
    println!(
        "THROW {:4} wolf {:<12} {:<18} enemy {:<12} {:<18} dist {:5.2} fwd {:+5.2} right {:+5.2} dyaw {:7.1} | W ({:+.2},{:+.2}) {:7.1} E ({:+.2},{:+.2}) {:7.1}",
        tr.frame - s,
        pa.anim,
        pa.state,
        ea.anim,
        ea.state,
        d.length(),
        d.dot(fwd),
        d.dot(right),
        dyaw,
        ptf.translation.x,
        ptf.translation.z,
        pa.yaw.to_degrees(),
        etf.translation.x,
        etf.translation.z,
        ea.yaw.to_degrees()
    );
    if let Ok((oc, ct)) = cam.single() {
        let oc = &*oc;
        let (y, x, _) = ct.rotation.to_euler(EulerRot::YXZ);
        println!(
            "CAM {:4} {:<26} pos ({:+.3},{:+.3},{:+.3}) yaw {:7.2} pitch {:6.2} orbit {:7.2}/{:6.2} dist {:.3} wolf y {:.3}",
            tr.frame - s,
            pa.state,
            ct.translation.x,
            ct.translation.y,
            ct.translation.z,
            y.to_degrees(),
            x.to_degrees(),
            oc.yaw.to_degrees(),
            oc.pitch.to_degrees(),
            oc.cam.dist,
            ptf.translation.y
        );
    }
    if tr.frame - s > 420 {
        exit.write(AppExit::Success);
    }
}

/// runkick: hold the stick forward (toward the enemy) from the setup on.
fn hold_stick(tr: Res<ThrowTrace>, mut pad: ResMut<PadInput>) {
    if tr.kind == "runkick" && tr.frame >= 181 && !tr.kicked {
        pad.stick = Vec2::new(0.0, 1.0);
    }
}

/// One line per rendered frame (not per fixed step): the drawn camera and Wolf, for jitter checks.
fn render_cam(time: Res<Time>, tr: Res<ThrowTrace>, cam: Query<&GlobalTransform, With<crate::camera::OrbitCamera>>, wolf: Query<&GlobalTransform, With<Player>>) {
    let (Some(s), Ok(c), Ok(w)) = (tr.started, cam.single(), wolf.single()) else { return };
    let (y, x, _) = c.rotation().to_euler(EulerRot::YXZ);
    let p = c.translation();
    let wp = w.translation();
    println!(
        "CAMR {:4} t {:.4} cam ({:+.3},{:+.3},{:+.3}) yaw {:8.3} pitch {:7.3} wolf ({:+.3},{:+.3},{:+.3})",
        tr.frame - s,
        time.elapsed_secs(),
        p.x, p.y, p.z, y.to_degrees(), x.to_degrees(), wp.x, wp.y, wp.z
    );
}
