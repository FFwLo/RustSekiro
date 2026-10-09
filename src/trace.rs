//! Motion trace for finding animation pops. With SHINOBI_TRACE=<file.csv> the game drives Wolf
//! through a fixed input script (idle, walk, run, stops, locked-on strafes, attacks, guard walk and
//! release), writes one line per rendered frame (state, anim, clock, position and a few bones in
//! Wolf's own frame, after transform propagation = what is drawn) and exits.
//! tools/trace_pops.py lists the frames where a bone jumps far more than around it.

use bevy::prelude::*;
use std::io::Write;

use crate::actor::Actor;
use crate::camera::LockOn;
use crate::enemy::{AiMode, Enemy, EnemyDebug};
use crate::player::{Action, PadInput, Player};

/// Bones logged (Wolf's frame): body, hands, feet.
const BONES: [&str; 7] = ["Pelvis", "Spine2", "Head", "R_Hand", "L_Hand", "R_Foot", "L_Foot"];

/// (start [s], label, stick x/y, walk, guard, lock-on, press at start; None + label "sheathe"
/// presses X).
type Step = (f32, &'static str, Vec2, bool, bool, bool, Option<Action>);
const SCRIPT: &[Step] = &[
    (0.0, "warmup", Vec2::ZERO, false, false, false, None),
    (4.0, "idle", Vec2::ZERO, false, false, false, None),
    (7.0, "walk_f", Vec2::new(0.0, 1.0), true, false, false, None),
    (10.0, "walk_stop", Vec2::ZERO, false, false, false, None),
    (11.5, "run_f", Vec2::new(0.0, 1.0), false, false, false, None),
    (14.5, "run_stop", Vec2::ZERO, false, false, false, None),
    (16.0, "lock_idle", Vec2::ZERO, false, false, true, None),
    (17.0, "lock_run_l", Vec2::new(-1.0, 0.0), false, false, true, None),
    (20.0, "lock_run_r", Vec2::new(1.0, 0.0), false, false, true, None),
    (23.0, "lock_walk_l", Vec2::new(-1.0, 0.0), true, false, true, None),
    (26.0, "lock_walk_b", Vec2::new(0.0, -1.0), true, false, true, None),
    (29.0, "lock_walk_fl", Vec2::new(-0.7071, 0.7071), true, false, true, None),
    (32.0, "lock_walk_br", Vec2::new(0.7071, -0.7071), true, false, true, None),
    (35.0, "lock_run_fr", Vec2::new(0.7071, 0.7071), false, false, true, None),
    (38.0, "lock_run_bl", Vec2::new(-0.7071, -0.7071), false, false, true, None),
    (41.0, "lock_stop", Vec2::ZERO, false, false, true, None),
    (42.5, "attack", Vec2::ZERO, false, false, true, Some(Action::Attack)),
    (45.0, "guard", Vec2::ZERO, false, true, true, Some(Action::Guard)),
    (46.5, "guard_walk_fl", Vec2::new(-0.7071, 0.7071), false, true, true, None),
    (49.5, "guard_release_moving", Vec2::new(-1.0, 0.0), false, false, true, None),
    (52.5, "end_idle", Vec2::ZERO, false, false, false, None),
    (54.5, "sheathe", Vec2::ZERO, false, false, false, None),
    (57.5, "sheathed_walk", Vec2::new(0.0, 1.0), true, false, false, None),
    (60.5, "sheathed_run", Vec2::new(0.0, 1.0), false, false, false, None),
    (63.5, "sheathed_stop", Vec2::ZERO, false, false, false, None),
    (65.0, "draw", Vec2::ZERO, false, false, false, Some(Action::Attack)),
    (67.5, "done", Vec2::ZERO, false, false, false, None),
];

#[derive(Resource)]
struct Trace {
    out: std::fs::File,
    clock: f32,
    step: usize,
}

pub struct TracePlugin;

impl Plugin for TracePlugin {
    fn build(&self, app: &mut App) {
        let Ok(path) = std::env::var("SHINOBI_TRACE") else { return };
        let mut out = std::fs::File::create(&path).expect("trace file");
        let _ = writeln!(out, "clock,dt,step,state,anim,t,shown,blend,x,z,yaw,{}", BONES.iter().map(|b| format!("{b}_x,{b}_y,{b}_z")).collect::<Vec<_>>().join(","));
        app.insert_resource(Trace { out, clock: 0.0, step: 0 })
            .add_systems(Update, drive.after(crate::player::read_input))
            .add_systems(PostUpdate, log.after(TransformSystems::Propagate));
    }
}

fn drive(
    time: Res<Time>,
    mut trace: ResMut<Trace>,
    mut pad: ResMut<PadInput>,
    mut lock: ResMut<LockOn>,
    enemy: Query<Entity, With<Enemy>>,
    debug: Option<ResMut<EnemyDebug>>,
    mut exit: MessageWriter<AppExit>,
) {
    trace.clock += time.delta_secs();
    if let Some(mut d) = debug {
        d.mode = AiMode::Idle;
    }
    let next = SCRIPT.iter().rposition(|s| s.0 <= trace.clock).unwrap_or(0);
    let (_, label, stick, walk, guard, locked, press) = SCRIPT[next];
    if label == "done" {
        exit.write(AppExit::Success);
        return;
    }
    if next != trace.step {
        trace.step = next;
        if let Some(a) = press {
            pad.press(a);
        }
        if label == "sheathe" {
            pad.sheathe = true;
        }
    }
    pad.stick = stick;
    pad.walk = walk;
    pad.guard_held = guard;
    lock.target = if locked { enemy.single().ok() } else { None };
}

fn log(
    time: Res<Time>,
    lib: Res<crate::anim::AnimLib>,
    mut trace: ResMut<Trace>,
    wolf: Query<(&Actor, &GlobalTransform, Option<&crate::anim::Skeleton>), With<Player>>,
    bones: Query<&GlobalTransform>,
) {
    let Ok((a, root, skel)) = wolf.single() else { return };
    let (shown, blend) = skel.map_or((String::new(), 1.0), |s| (s.shown.clone(), s.blend));
    let inv = root.affine().inverse();
    let mut cols = Vec::new();
    for b in BONES {
        // Wolf's own skeleton entity for the bone.
        let p = lib
            .player
            .bones
            .iter()
            .position(|d| d.name == b)
            .zip(skel)
            .and_then(|(i, s)| bones.get(s.bones[i]).ok())
            .map(|g| inv.transform_point3(g.translation()))
            .unwrap_or(Vec3::NAN);
        cols.push(format!("{:.4},{:.4},{:.4}", p.x, p.y, p.z));
    }
    let t = root.translation();
    let (clock, step) = (trace.clock, SCRIPT[trace.step].1);
    let _ = writeln!(
        trace.out,
        "{clock:.4},{:.4},{step},{},{},{:.4},{shown},{blend:.3},{:.4},{:.4},{:.4},{}",
        time.delta_secs(),
        a.state,
        a.anim,
        a.t,
        t.x,
        t.z,
        a.yaw,
        cols.join(",")
    );
}
