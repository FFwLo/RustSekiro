//! `--script` rows aim Wolf in world space (#215): a row's yaw column turns the camera it is
//! played from, which starts at the spawn yaw, so a forward stick runs (and sprint-jumps) along
//! world yaw `spawn + column`, a back stick the opposite way, whatever the column. The rows
//! here go through the same conversion as `--play` (`InputFrame::from_camera`). Skips when
//! `cache/` has not been extracted.

use super::movement_helpers::{DT, load};
use glam::Vec2;
use sekiro_sim::input::{InputFrame, angle_diff, parse_action_script};

/// The play camera's yaw at the start: the `--spawn` yaw (m11_00's start 0 faces -158).
const CAMERA: f32 = -158.0;

/// Each row and the world yaw, in degrees, Wolf must travel along over its last 40 steps
/// (`None`: a stop between moves, not measured).
const ROWS: [(&str, Option<f32>); 9] = [
    ("90 0 1 - 0", Some(CAMERA)),
    ("40 0 0 - 0", None),
    ("90 0 1 - 90", Some(CAMERA + 90.0)),
    ("40 0 0 - 90", None),
    ("90 0 1 - -135", Some(CAMERA - 135.0)),
    ("40 0 0 - 0", None),
    ("90 0 -1 - 0", Some(CAMERA + 180.0)),
    ("40 0 0 - 0", None),
    // A sprint into a jump, as the #176 fence check wanted.
    (
        "60 0 1 dodge 45\n3 0 1 dodge,jump 45\n50 0 1 dodge 45",
        Some(CAMERA + 45.0),
    ),
];

#[test]
fn script_rows_move_wolf_along_the_spawn_yaw_plus_their_yaw_column() {
    let Some(mut wolf) = load() else {
        return;
    };
    wolf.snap_yaw(CAMERA.to_radians());
    let mut wrong = Vec::new();
    for (row, want) in ROWS {
        let frames = parse_action_script(row).unwrap();
        let mut trail = Vec::new();
        for frame in &frames {
            let played: InputFrame = frame.from_camera(CAMERA.to_radians(), 0.25);
            let report = wolf.tick(&played, DT);
            assert!(
                report.behavior.script_errors.is_empty(),
                "{:?}",
                report.behavior.script_errors
            );
            trail.push(Vec2::new(report.position[0], report.position[2]));
        }
        let Some(want) = want else {
            continue;
        };
        let d = trail[trail.len() - 1] - trail[trail.len() - 41];
        let travel = (-d.x).atan2(-d.y);
        let err = angle_diff(travel, want.to_radians()).to_degrees();
        eprintln!(
            "{row:?}: moved {:.2} m along {:.1}, want {want:.1} (error {err:.1})",
            d.length(),
            travel.to_degrees()
        );
        if d.length() < 2.0 || err.abs() > 10.0 {
            wrong.push((row, d.length(), travel.to_degrees(), want));
        }
    }
    assert!(wrong.is_empty(), "rows off their aim: {wrong:?}");
}
