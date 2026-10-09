//! Sv1: a Sekiro combat prototype driven by data read from the
//! player's own Sekiro install (see tools/extract.ps1 and data.rs).
//!
//! Each `mod` line pulls in one file from `src/`. Each file provides a
//! "plugin", a bundle of systems (functions that run every frame).

use bevy::prelude::*;
use bevy::window::WindowResolution;

mod actor;
mod ai;
mod anim;
mod camera;
mod cloth;
mod combat;
mod config;
mod data;
mod debug_menu;
mod enemy;
mod fmod;
mod hud;
mod interp;
mod model;
mod paths;
mod photo;
mod player;
mod prosthetic;
#[cfg(test)]
mod sim_tests;
mod sound;
mod trace;
mod world;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin { file_path: paths::extracted().to_string_lossy().into_owned(), ..default() }).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Sv1".into(),
                resolution: WindowResolution::new(1024, 560),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(cloth::ClothPlugin)
        .add_plugins(interp::InterpPlugin)
        .add_plugins(debug_menu::DebugMenuPlugin)
        .add_plugins(trace::TracePlugin)
        .add_plugins((
            config::ConfigPlugin,
            data::DataPlugin,
            actor::ActorPlugin,
            combat::CombatPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            camera::CameraPlugin,
            hud::HudPlugin,
            anim::AnimPlugin,
            model::ModelPlugin,
            sound::SoundPlugin,
            fmod::FmodPlugin,
            prosthetic::ProstheticPlugin,
            photo::PhotoPlugin,
        ))
        .run();
}
