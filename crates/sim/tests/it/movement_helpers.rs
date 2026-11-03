//! Shared setup and observations for real-data movement regressions.

use sekiro_sim::{Character, InputFrame, StepReport};
use std::fmt::{self, Write};
use std::path::PathBuf;

pub const DT: f32 = 1.0 / 60.0;
pub const SETTLE_TICKS: usize = 400;
pub const RESET_TICKS: usize = 120;
pub const RUN_TICKS: usize = 120;
pub const RUNUP_TICKS: usize = 90;
pub const SPRINT_TICKS: usize = 120;
pub const OBSERVE_TICKS: usize = 90;
pub const RUN_LOOP: &str = "a000_000500";
pub const RUN_FROM_SPRINT_START: &str = "a000_000450";
pub const RUN_FROM_SPRINT_LOOP: &str = "a000_000460";
const DIRECTIONAL_CLIP_BASES: [u32; 5] = [100, 200, 400, 500, 600];

/// The extracted data ([`sekiro_sim::cache::root`]: `SEKIRO_CACHE` when it is set, so a clone
/// can use the main checkout's cache, else the repo's `cache/`). `None` without Wolf's behaviour.
pub fn cache() -> Option<PathBuf> {
    let path = sekiro_sim::cache::root();
    path.join("raw/chr/c0000.behbnd.d/c0000.hkx")
        .is_file()
        .then_some(path)
}

pub fn load() -> Option<Character> {
    let mut wolf = Character::load_from_cache(&cache()?).unwrap();
    settle(&mut wolf);
    Some(wolf)
}

pub fn settle(wolf: &mut Character) {
    assert!(wolf.settle(DT, SETTLE_TICKS) < SETTLE_TICKS);
}

pub fn stick(x: f32, y: f32, camera_yaw: f32) -> InputFrame {
    InputFrame {
        move_stick: [x, y],
        camera_yaw,
        ..Default::default()
    }
}

pub fn movement(x: f32, y: f32) -> InputFrame {
    stick(x, y, 0.0)
}

pub fn advance(wolf: &mut Character, input: &InputFrame, frames: usize) {
    for _ in 0..frames {
        let report = wolf.tick(input, DT);
        assert!(
            report.behavior.script_errors.is_empty(),
            "{:?}",
            report.behavior.script_errors
        );
    }
}

/// Return to idle through HKS, preserving the loaded graph/clip cache between cases.
pub fn reset(wolf: &mut Character) {
    wolf.lock_target = None;
    wolf.release_move_lock();
    advance(wolf, &InputFrame::default(), RESET_TICKS);
    settle(wolf);
    assert!(wolf.body.grounded);
    wolf.body.position = [0.0; 3];
    wolf.snap_yaw(0.0);
}

pub fn tick_speed(wolf: &mut Character, input: &InputFrame) -> (StepReport, f32) {
    let before = wolf.body.position;
    let report = wolf.tick(input, DT);
    let speed = (report.position[0] - before[0]).hypot(report.position[2] - before[2]) / DT;
    (report, speed)
}

pub fn directional_clip(clip: &str) -> bool {
    clip.strip_prefix("a000_000")
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|id| {
            DIRECTIONAL_CLIP_BASES
                .iter()
                .any(|base| (1..=3).contains(&id.saturating_sub(*base)))
        })
}

/// No CSV allocation or formatting unless the caller requests an artifact.
pub struct Trace {
    path: Option<std::ffi::OsString>,
    csv: String,
}

impl Trace {
    pub fn from_env(variable: &str, header: &str) -> Self {
        let path = std::env::var_os(variable);
        let csv = if path.is_some() {
            header.to_owned()
        } else {
            String::new()
        };
        Self { path, csv }
    }

    pub fn record(&mut self, row: fmt::Arguments<'_>) {
        if self.path.is_some() {
            self.csv.write_fmt(row).unwrap();
            self.csv.push('\n');
        }
    }

    pub fn save(&self) {
        if let Some(path) = &self.path {
            std::fs::write(path, &self.csv).unwrap();
        }
    }
}

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.csv)
    }
}
