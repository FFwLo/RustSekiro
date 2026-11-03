//! `SHINOBI_SHOT=<file.png>`: after `SHINOBI_SHOT_FRAMES` frames (default 90) the window saves a
//! screenshot there and the game exits. With `SHINOBI_TITLE=<screen>` (menu.rs) it captures that
//! screen; it never brings the window to the front (tools/screenshot.ps1 does), so it can run
//! while the desktop is in use.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

#[derive(Resource)]
struct ShotPlan {
    path: String,
    frames: u32,
    seen: u32,
    taken: bool,
}

fn shoot(mut commands: Commands, mut plan: ResMut<ShotPlan>, mut exit: MessageWriter<AppExit>) {
    plan.seen += 1;
    if !plan.taken && plan.seen >= plan.frames {
        plan.taken = true;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(plan.path.clone()));
    } else if plan.taken && plan.seen >= plan.frames + 30 {
        exit.write(AppExit::Success);
    }
}

pub struct ShotPlugin;

impl Plugin for ShotPlugin {
    fn build(&self, app: &mut App) {
        let Ok(path) = std::env::var("SHINOBI_SHOT") else { return };
        let frames = std::env::var("SHINOBI_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
        app.insert_resource(ShotPlan { path, frames, seen: 0, taken: false }).add_systems(Update, shoot);
    }
}
