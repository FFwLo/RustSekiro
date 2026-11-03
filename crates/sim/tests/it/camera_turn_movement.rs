//! Free-camera movement: the stick is camera-relative, so whatever the camera and Wolf's facing,
//! Wolf must run where the stick points, turn toward a new stick direction smoothly, and stop
//! when it is let go. Reproduces "movement inverts / feels like tank controls after spinning
//! the camera". Skips when `cache/` has not been extracted. `--nocapture` prints a per-step log.

use super::movement_helpers::{
    DT, OBSERVE_TICKS, RUN_TICKS, Trace, directional_clip, load, reset, settle, stick, tick_speed,
};
use sekiro_sim::input::angle_diff;
use sekiro_sim::tae::cancel;
use sekiro_sim::{Character, InputFrame};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
const CAMERA_PREP_TICKS: usize = 20;
const RESTART_AT: usize = CAMERA_PREP_TICKS + RUN_TICKS;
// Preserve the reported stick components exactly for the reproduction.
#[allow(clippy::approx_constant)]
const DIAGONAL: f32 = 0.7071;

/// One step's movement.
struct Step {
    /// Angle from the stick direction to the direction Wolf travelled, degrees (NaN when the
    /// stick is released or Wolf stands still).
    err: f32,
    speed: f32,
}

/// Runs `steps` with the input from `input(i)`, logging each step.
fn run(wolf: &mut Character, steps: usize, input: impl Fn(usize) -> InputFrame) -> Vec<Step> {
    let mut out = Vec::new();
    for i in 0..steps {
        let frame = input(i);
        let before = wolf.body.position;
        let (r, speed) = tick_speed(wolf, &frame);
        let stick = wolf.input.stick_world_yaw();
        let (dx, dz) = (r.position[0] - before[0], r.position[2] - before[2]);
        let travel = (-dx).atan2(-dz);
        let err = match stick {
            Some(s) if speed > 0.05 => angle_diff(travel, s).to_degrees(),
            _ => f32::NAN,
        };
        let v = |n: &str| wolf.behavior.runtime.variable(n).unwrap_or(f32::NAN);
        eprintln!(
            "{i:3} cam {:7.1} stick {:7.1} face {:7.1} twist {:6.1} TurnAngle {:7.1} v {:.2} \
             err {:7.1} {} {:?} t {:.2}",
            frame.camera_yaw.to_degrees(),
            stick.map_or(f32::NAN, f32::to_degrees),
            r.yaw.to_degrees(),
            wolf.twist,
            v("TurnAngle"),
            speed,
            err,
            r.behavior.state_path.last().cloned().unwrap_or_default(),
            r.animation,
            r.anim_time,
        );
        out.push(Step { err, speed });
    }
    out
}

/// Every moving step in `steps` travels within `limit` degrees of the stick.
fn assert_follows(steps: &[Step], limit: f32, what: &str) {
    let moving: Vec<f32> = steps
        .iter()
        .map(|s| s.err)
        .filter(|e| !e.is_nan())
        .collect();
    assert!(moving.len() * 2 > steps.len(), "{what}: Wolf barely moved");
    let worst = moving.iter().fold(0f32, |m, e| m.max(e.abs()));
    assert!(worst < limit, "{what}: travel {worst:.0} deg off the stick");
}

#[test]
fn camera_sweep_with_stick_forward() {
    let Some(mut wolf) = load() else { return };
    // Run forward 1 s, then sweep the camera a full turn over 3 s, then hold 1 s.
    let steps = run(&mut wolf, 300, |i| {
        let t = (i as f32 - 60.0).clamp(0.0, 180.0) / 180.0;
        stick(0.0, 1.0, t * TAU)
    });
    assert_follows(&steps[30..], 15.0, "sweep");
}

#[test]
fn camera_flips_instantly_with_stick_held() {
    let Some(mut wolf) = load() else { return };
    let steps = run(&mut wolf, 180, |i| {
        stick(0.0, 1.0, if i < 60 { 0.0 } else { PI })
    });
    assert_follows(&steps[30..60], 15.0, "before the flip");
    assert_follows(&steps[120..], 15.0, "after the flip");
}

#[test]
fn stick_repushed_after_camera_turn() {
    let Some(mut wolf) = load() else { return };
    let steps = run(&mut wolf, 220, |i| {
        let y = if (60..100).contains(&i) { 0.0 } else { 1.0 };
        stick(0.0, y, if i < 70 { 0.0 } else { 2.5 })
    });
    assert_follows(&steps[110..], 15.0, "re-pushed");
}

#[test]
fn free_run_restart_turns_instead_of_strafing() {
    let Some(mut wolf) = load() else { return };
    let mut trace = Trace::from_env(
        "SEKIRO_RUN_RESTART_TRACE",
        "case,tick,clip,locked,MoveAngle,TurnAngle,MoveDirection,camera_yaw,facing,state\n",
    );
    let mut failures = Vec::new();
    for (before_yaw, after_yaw) in [(0.0, 0.0), (0.8, 0.8), (-1.2, 1.1)] {
        for gap in [1, 12, 60] {
            for (x, y) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0)] {
                reset(&mut wolf);
                let case = format!("{before_yaw}:{after_yaw}:{gap}:{x}:{y}");
                let mut strafed = false;
                for tick in 0..(RESTART_AT + gap + OBSERVE_TICKS) {
                    let frame = if tick < CAMERA_PREP_TICKS {
                        stick(
                            0.0,
                            0.0,
                            before_yaw * tick as f32 / (CAMERA_PREP_TICKS - 1) as f32,
                        )
                    } else if tick < RESTART_AT {
                        stick(0.0, 1.0, before_yaw)
                    } else if tick < RESTART_AT + gap {
                        stick(0.0, 0.0, after_yaw)
                    } else {
                        stick(x, y, after_yaw)
                    };
                    let r = wolf.tick(&frame, DT);
                    let v = |name| wolf.behavior.runtime.variable(name).unwrap_or(f32::NAN);
                    let clip = r.animation.as_deref().unwrap_or("");
                    trace.record(format_args!(
                        "{case},{tick},{clip},{},{:.4},{:.4},{:.0},{:.4},{:.4},{}",
                        wolf.lock_target.is_some(),
                        v("MoveAngle"),
                        v("TurnAngle"),
                        v("MoveDirection"),
                        frame.camera_yaw.to_degrees(),
                        r.yaw.to_degrees(),
                        wolf.state_name()
                    ));
                    assert!(r.behavior.script_errors.is_empty(), "{case}");
                    // The graph's directional walk/run starts, loops and stops are locked
                    // strafes. The free 180-degree turn-and-go starts are allowed.
                    if tick >= RESTART_AT + gap {
                        strafed |= directional_clip(clip);
                    }
                }
                let expected = after_yaw + (-x).atan2(y);
                let error = angle_diff(wolf.body.yaw, expected).to_degrees().abs();
                if strafed || error > 15.0 {
                    failures.push(format!("{case}: strafe={strafed}, facing error={error:.1}"));
                }
            }
        }
    }
    trace.save();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Free (no lock-on) run, then `action` pressed for a few ticks, then the stick held sideways:
/// Wolf must turn and run that way, never play a locked directional strafe (#144).
fn check_free_sideways_after(action: fn(&mut InputFrame), what: &str) {
    let Some(mut wolf) = load() else { return };
    let mut failures = Vec::new();
    // Stick held through the action (forward) or released; sideways right after it ends or later.
    for hold in [true, false] {
        // Left and right mirror each other, so each wait takes one side.
        for (action_ticks, x) in [(60, -1.0), (120, 1.0)] {
            reset(&mut wolf);
            let case = format!("{what} hold={hold} wait={action_ticks} x={x}");
            let mut strafed = Vec::new();
            for tick in 0..(RUN_TICKS + action_ticks + OBSERVE_TICKS) {
                let frame = if tick < RUN_TICKS {
                    stick(0.0, 1.0, 0.0)
                } else if tick < RUN_TICKS + action_ticks {
                    let mut f = stick(0.0, if hold { 1.0 } else { 0.0 }, 0.0);
                    if tick < RUN_TICKS + 4 {
                        action(&mut f);
                    }
                    f
                } else {
                    stick(x, 0.0, 0.0)
                };
                let r = wolf.tick(&frame, DT);
                assert!(r.behavior.script_errors.is_empty(), "{case}");
                let clip = r.animation.as_deref().unwrap_or("");
                if tick >= RUN_TICKS + action_ticks && directional_clip(clip) {
                    let v = |name| wolf.behavior.runtime.variable(name).unwrap_or(f32::NAN);
                    strafed.push(format!(
                        "{tick}:{clip}:{}:MoveDirection={}:MoveAngle={:.0}",
                        wolf.state_name(),
                        v("MoveDirection"),
                        v("MoveAngle")
                    ));
                }
            }
            let expected = (-x).atan2(0.0);
            let error = angle_diff(wolf.body.yaw, expected).to_degrees().abs();
            if !strafed.is_empty() || error > 15.0 {
                strafed.truncate(4);
                failures.push(format!(
                    "{case}: strafe {strafed:?}, facing error={error:.1}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn free_swing_then_sideways_turns_instead_of_strafing() {
    check_free_sideways_after(|f| f.buttons.attack = true, "swing");
}

#[test]
fn free_jump_landing_then_sideways_turns_instead_of_strafing() {
    check_free_sideways_after(|f| f.buttons.jump = true, "jump");
}

#[test]
fn moves_off_in_every_direction_from_every_facing() {
    let Some(mut wolf) = load() else { return };
    for facing in 0..4 {
        for dir in 0..8 {
            settle(&mut wolf);
            wolf.body.yaw = facing as f32 * FRAC_PI_2;
            let a = dir as f32 * TAU / 8.0;
            let (x, y) = (a.sin(), a.cos());
            let steps = run(&mut wolf, 60, |_| stick(x, y, 0.3));
            assert_follows(&steps[10..], 15.0, &format!("facing {facing} stick {dir}"));
            run(&mut wolf, 90, |_| stick(0.0, 0.0, 0.3));
        }
    }
}

#[test]
fn quick_reverse_and_stop() {
    let Some(mut wolf) = load() else { return };
    // Run forward, pull the stick back (W then S), then let go.
    let steps = run(&mut wolf, 260, |i| match i {
        0..90 => stick(0.0, 1.0, 0.0),
        90..200 => stick(0.0, -1.0, 0.0),
        _ => stick(0.0, 0.0, 0.0),
    });
    assert_follows(&steps[150..200], 15.0, "reversed");
    let last = &steps[250..];
    assert!(
        last.iter().all(|s| s.speed < 0.3),
        "still moving after letting go"
    );
}

#[test]
fn sprint_quick_turn_ends_facing_the_stick() {
    let Some(mut wolf) = load() else { return };
    // Sprint forward (dodge held), then pull the stick back: the script's sprint quick turn
    // turns Wolf by its root motion and he sprints the other way.
    let mut steps = Vec::new();
    let mut windup_ticks = 0;
    for i in 0..240 {
        let before_yaw = wolf.body.yaw;
        let mut f = stick(0.0, if i < 100 { 1.0 } else { -1.0 }, 0.0);
        f.buttons.dodge = i > 5;
        steps.extend(run(&mut wolf, 1, |_| f));
        if wolf.state_name() == "SprintQuickTurnReady" {
            windup_ticks += 1;
            assert_eq!(wolf.tae_runtime.frame.turn_speed_for(false), Some(0.0));
            assert!(
                angle_diff(wolf.body.yaw, before_yaw).abs() < 1e-6,
                "quick-turn wind-up steered before its authored turn"
            );
        }
    }
    assert!(windup_ticks > 0, "never exercised the zero-rate wind-up");
    assert_follows(&steps[60..100], 15.0, "sprinting");
    assert_follows(&steps[180..], 15.0, "after the sprint quick turn");
}

#[test]
fn running_never_stalls_between_clips() {
    let Some(mut wolf) = load() else { return };
    // The run start hands over to the run loop at its end; the body keeps moving through it.
    let steps = run(&mut wolf, 300, |_| stick(0.0, 1.0, 0.0));
    let slowest = steps[20..].iter().map(|s| s.speed).fold(f32::MAX, f32::min);
    assert!(
        slowest > 3.0,
        "Wolf stalled at {slowest:.2} m/s while running"
    );
}

#[test]
fn locked_on_strafes_follow_the_stick() {
    let Some(mut wolf) = load() else { return };
    let target = [0.0, 0.0, -6.0];
    // Locked on, the camera looks from behind Wolf at the target; strafe right, then left,
    // then back off and close in.
    let keys = [(1.0, 0.0), (-1.0, 0.0), (0.0, -1.0), (0.0, 1.0)];
    for (k, (x, y)) in keys.into_iter().enumerate() {
        let mut steps = Vec::new();
        for _ in 0..90 {
            wolf.lock_target = Some(target);
            let cam = wolf.yaw_to(target);
            steps.extend(run(&mut wolf, 1, |_| stick(x, y, cam)));
        }
        assert_follows(&steps[30..], 20.0, &format!("locked strafe {k}"));
    }
}

struct LockedStep {
    movement: Step,
    state: String,
    clip: String,
    facing: f32,
    target_err: f32,
    move_direction: f32,
    move_angle: f32,
    /// The clip's TAE disabled turning on this step ("Disable Turning", flag 7).
    turn_disabled: bool,
}

/// Records the real player's root motion without resetting lock-on between ticks.
fn run_locked(
    wolf: &mut Character,
    frames: usize,
    input: impl Fn(usize, &Character) -> InputFrame,
) -> Vec<LockedStep> {
    let mut steps = Vec::new();
    for i in 0..frames {
        let frame = input(i, wolf);
        let before = wolf.body.position;
        let report = wolf.tick(&frame, DT);
        let dx = report.position[0] - before[0];
        let dz = report.position[2] - before[2];
        let speed = dx.hypot(dz) / DT;
        let err = wolf.input.stick_world_yaw().map_or(f32::NAN, |yaw| {
            if speed > 0.05 {
                angle_diff((-dx).atan2(-dz), yaw).to_degrees()
            } else {
                f32::NAN
            }
        });
        steps.push(LockedStep {
            movement: Step { err, speed },
            state: report.behavior.state_path.join("/"),
            clip: report.animation.unwrap_or_default(),
            facing: report.yaw.to_degrees(),
            target_err: wolf.lock_target.map_or(f32::NAN, |target| {
                angle_diff(report.yaw, wolf.yaw_to(target)).to_degrees()
            }),
            move_direction: wolf
                .behavior
                .runtime
                .variable("MoveDirection")
                .unwrap_or(f32::NAN),
            move_angle: wolf
                .behavior
                .runtime
                .variable("MoveAngle")
                .unwrap_or(f32::NAN),
            turn_disabled: wolf.tae_frame().flag(cancel::DISABLE_TURN),
        });
    }
    steps
}

/// A compact table plus the worst moving frame makes a failed direction easy to diagnose.
fn locked_table(steps: &[LockedStep]) -> String {
    use std::fmt::Write;
    let worst = steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.movement.err.is_finite())
        .max_by(|(_, a), (_, b)| a.movement.err.abs().total_cmp(&b.movement.err.abs()))
        .map(|(i, _)| i);
    let mut table =
        String::from("time state clip MoveDirection MoveAngle facing stick_err speed target_err\n");
    for (i, s) in steps.iter().enumerate() {
        if i % 10 == 0 || i + 1 == steps.len() || Some(i) == worst {
            writeln!(
                table,
                "{:.2} {} {} {:.1} {:.1} {:.1} {:.1} {:.2} {:.1}",
                i as f32 * DT,
                s.state,
                s.clip,
                s.move_direction,
                s.move_angle,
                s.facing,
                s.movement.err,
                s.movement.speed,
                s.target_err,
            )
            .unwrap();
        }
    }
    table
}

fn locked_wolf(side_target: bool) -> Option<Character> {
    let mut wolf = load()?;
    let p = wolf.body.position;
    wolf.body.yaw = 0.0;
    wolf.lock_target = Some(if side_target {
        [p[0] + 4.0, p[1], p[2]]
    } else {
        [p[0], p[1], p[2] - 4.0]
    });
    // Let the player script enter locked idle before pressing the stick and dodge.
    run_locked(&mut wolf, 30, |_, _| InputFrame::default());
    Some(wolf)
}

fn check_locked_step(x: f32, y: f32) {
    let Some(mut wolf) = locked_wolf(false) else {
        return;
    };
    let steps = run_locked(&mut wolf, 60, |i, _| {
        let mut frame = stick(x, y, 0.0);
        frame.buttons.dodge = i < 2;
        frame
    });
    let stepping: Vec<_> = steps
        .iter()
        .filter(|s| s.state.contains("Step") && !s.state.contains("Sprint"))
        .collect();
    let distance: f32 = stepping.iter().map(|s| s.movement.speed * DT).sum();
    // The step TAE (a000_2133xx) disables turning for its first 0.4 s and after 0.6 s; in
    // between, the engine turns a locked-on Wolf back to the target. With the target 4 m away,
    // a 3.5 m side step has carried him 30-40 degrees around it by then, so the travel bends by
    // that much in the turn window (and the rest of the step follows the new facing). The
    // direction of the dodge itself is its first, turn-disabled part, which must follow the
    // stick; then Wolf must face the target again.
    let worst = stepping
        .iter()
        .take_while(|s| s.turn_disabled)
        .filter(|s| s.movement.err.is_finite())
        .map(|s| s.movement.err.abs())
        .fold(0.0_f32, f32::max);
    let refaced = stepping.last().is_some_and(|s| s.target_err.abs() <= 10.0);
    assert!(
        !stepping.is_empty() && distance >= 1.5 && worst <= 30.0 && refaced,
        "locked step: distance {distance:.2} m, worst travel angle {worst:.1} deg, \
         refaced the target {refaced}\n{}",
        locked_table(&steps)
    );
}

fn check_locked_sprint(x: f32, y: f32) {
    // Forward has an open lane; the enemy remains locked on four metres to the side.
    let Some(mut wolf) = locked_wolf(y > 0.0) else {
        return;
    };
    let target = wolf.lock_target;
    let steps = run_locked(&mut wolf, 150, |_, _| {
        let mut frame = stick(x, y, 0.0);
        frame.buttons.dodge = true;
        frame
    });
    let window = &steps[48..];
    let average = window.iter().map(|s| s.movement.speed).sum::<f32>() / window.len() as f32;
    let worst = window
        .iter()
        .filter(|s| s.movement.speed > 3.0)
        .map(|s| s.movement.err.abs())
        .fold(0.0_f32, f32::max);
    assert!(
        average > 5.0 && worst <= 30.0 && wolf.lock_target == target,
        "locked sprint: average {average:.2} m/s, worst travel angle {worst:.1} deg, lock {:?}\n{}",
        wolf.lock_target,
        locked_table(&steps)
    );
}

#[test]
fn locked_step_goes_where_the_stick_points() {
    use rayon::prelude::*;
    // Left, right, back and forward; each case builds its own Wolf.
    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
        .par_iter()
        .for_each(|&(x, y)| check_locked_step(x, y));
}

/// A diagonal stick picks the nearest of the four steps and tilts it onto the stick
/// (`_set4DirStepTilt`: `act(3025, angle - quadrant centre)`, positive to the right).
#[test]
fn locked_step_tilts_onto_a_diagonal_stick() {
    let d = std::f32::consts::FRAC_1_SQRT_2;
    for (x, y) in [(d, d), (-d, d), (d, -d), (-d, -d), (0.9, 0.44)] {
        check_locked_step(x, y);
    }
}

#[test]
fn locked_sprint_goes_where_the_stick_points() {
    use rayon::prelude::*;
    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
        .par_iter()
        .for_each(|&(x, y)| check_locked_sprint(x, y));
}

#[test]
fn locked_sprint_release_returns_to_locked_movement() {
    let Some(mut wolf) = locked_wolf(false) else {
        return;
    };
    let target = wolf.lock_target;
    let mut steps = run_locked(&mut wolf, 120, |_, _| {
        let mut frame = stick(-1.0, 0.0, 0.0);
        frame.buttons.dodge = true;
        frame
    });
    steps.extend(run_locked(&mut wolf, 90, |_, wolf| {
        stick(-1.0, 0.0, wolf.yaw_to(target.unwrap()))
    }));
    // The final tenth of a second must be sustained target-facing left strafing.
    let returned = steps[204..].iter().all(|s| {
        s.target_err.abs() <= 30.0
            && s.movement.speed > 0.3
            && s.movement.err.abs() <= 30.0
            && !s.state.contains("Sprint")
    });
    assert!(
        returned && wolf.lock_target == target,
        "sprint release did not return to target-facing left strafe within 1.5 s\n{}",
        locked_table(&steps)
    );
}

/// Sustained locked movement must twist the legs onto the camera-relative diagonal.
fn check_locked_diagonal(move_stick: [f32; 2]) {
    let Some(mut wolf) = locked_wolf(false) else {
        eprintln!("SKIPPED locked diagonal: extracted player cache is missing");
        return;
    };
    let p = wolf.body.position;
    // Far enough that the forward runs do not reach it: holding a fixed angle off the target
    // bearing is an inward spiral that closes at about 3.9 m/s on a run, and this test has no
    // enemy body to stop Wolf. At 6 m he reached the target after 1.5 s and the facing swung
    // round as he ran through it.
    let target = [p[0], p[1], p[2] - 20.0];
    wolf.lock_target = Some(target);
    run_locked(&mut wolf, 30, |_, _| InputFrame::default());
    let steps = run_locked(&mut wolf, 120, |_, wolf| {
        stick(move_stick[0], move_stick[1], wolf.yaw_to(target))
    });
    let distance = steps.iter().map(|s| s.movement.speed * DT).sum::<f32>();
    let window = &steps[30..];
    let moving = window.iter().filter(|s| s.movement.speed > 0.5).count();
    let worst_travel = window
        .iter()
        .filter(|s| s.movement.speed > 0.5)
        .map(|s| s.movement.err.abs())
        .fold(0.0_f32, f32::max);
    let worst_facing = window
        .iter()
        .map(|s| s.target_err.abs())
        .fold(0.0_f32, f32::max);
    let follows = window.iter().all(|s| {
        s.movement.speed <= 0.5 || (s.movement.err.is_finite() && s.movement.err.abs() <= 20.0)
    });
    let faces = window
        .iter()
        .all(|s| s.target_err.is_finite() && s.target_err.abs() <= 30.0);
    // Every diagonal is 45 degrees off the clip it plays (forward or back), so the legs end up
    // turned by 45 degrees one way or the other.
    let twist = wolf.walk_twist;
    let legs_twisted = (twist.abs() - 45.0).abs() <= 5.0;
    let summary = format!(
        "locked diagonal {move_stick:?}: distance {distance:.2} m, moving {moving}/90, \
         worst travel {worst_travel:.1} deg, worst facing {worst_facing:.1} deg, \
         walk twist {twist:.1} deg"
    );
    eprintln!("{summary}\n{}", locked_table(&steps));
    assert!(
        follows && faces && legs_twisted && distance >= 1.0,
        "{summary}\n{}",
        locked_table(&steps)
    );
}

#[test]
fn locked_diagonals_twist_the_legs_onto_the_stick() {
    use rayon::prelude::*;
    // Runs and walks (a 0.4 stick) in all four diagonals, plus the keyboard's W+A: play.rs
    // gather_input subtracts opposing WASD keys, then normalizes lengths > 1, so with no gamepad
    // override W+A is a unit diagonal, not [-1, 1].
    let mut cases = Vec::new();
    for scale in [1.0, 0.4] {
        for (x, y) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            cases.push([x * DIAGONAL * scale, y * DIAGONAL * scale]);
        }
    }
    let (w, a, s, d) = (true, true, false, false);
    let mut keyboard = [(d as i32 - a as i32) as f32, (w as i32 - s as i32) as f32];
    let length = keyboard[0].hypot(keyboard[1]);
    if length > 1.0 {
        keyboard[0] /= length;
        keyboard[1] /= length;
    }
    cases.push(keyboard);
    cases
        .par_iter()
        .for_each(|&stick| check_locked_diagonal(stick));
}
