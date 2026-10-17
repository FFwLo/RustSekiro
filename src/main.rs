//! Sv1: a Sekiro combat prototype driven by data read from the
//! player's own Sekiro install (see tools/extract.ps1 and data.rs).
//!
//! Each `mod` line pulls in one file from `src/`. Each file provides a
//! "plugin", a bundle of systems (functions that run every frame).
//!
//! Optimised builds open no console window: the log goes to `sv1.log` next to `config.toml`
//! (and to the terminal the game was started from, if any), a panic shows a "Sv1 stopped"
//! dialog (`startup`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::prelude::*;
use bevy::window::WindowResolution;

mod action_guide;
mod actor;
mod ai;
mod anim;
mod area_music;
mod camera;
mod cloth;
mod crash;
mod combat;
mod config;
mod data;
mod debug_menu;
mod docking;
mod duel;
mod emevd;
mod enemy;
mod enemy_bullet;
mod ffx;
mod fxr;
mod fog_wall;
mod height_fog;
mod sky;
mod fmod;
mod game_menu;
mod idol;
mod gore;
mod grading;
mod hud;
mod icon;
mod interp;
mod map;
mod menu;
mod sandbox;
mod showcase;
mod shot;
mod skill_tree;
mod model;
mod paths;
mod perf;
mod photo;
mod player;
mod prosthetic;
#[cfg(test)]
mod sim_tests;
mod sound;
mod status;
mod stealth;
mod throw_trace;
mod trace;
mod vfx;
mod wire;
mod world;

fn main() {
    startup();
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin { file_path: paths::extracted().to_string_lossy().into_owned(), ..default() }).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Sv1".into(),
                resolution: WindowResolution::new(1024, 560),
                ..default()
            }),
            ..default()
        }).set(bevy::log::LogPlugin { custom_layer: log_file_layer, ..default() }))
        .add_plugins(cloth::ClothPlugin)
        .add_plugins(action_guide::ActionGuidePlugin)
        .add_plugins(shot::ShotPlugin)
        .add_plugins(game_menu::GameMenuPlugin)
        .add_plugins(idol::IdolPlugin)
        .add_plugins(interp::InterpPlugin)
        .add_plugins(debug_menu::DebugMenuPlugin)
        .add_plugins(trace::TracePlugin)
        .add_plugins(throw_trace::ThrowTracePlugin)
        .add_plugins(vfx::VfxPlugin)
        .add_plugins(wire::WirePlugin)
        .add_plugins(gore::GorePlugin)
        .add_plugins(area_music::AreaMusicPlugin)
        .add_plugins(ffx::FfxPlugin)
        .add_plugins(fxr::FxrPlugin)
        .add_plugins(fog_wall::FogWallPlugin)
        .add_plugins(height_fog::HeightFogPlugin)
        .add_plugins(sky::SkyPlugin)
        .add_plugins(map::MapPlugin)
        .add_plugins(icon::IconPlugin)
        .add_plugins((
            config::ConfigPlugin,
            data::DataPlugin,
            actor::ActorPlugin,
            combat::CombatPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            camera::CameraPlugin,
            menu::MenuPlugin,
            hud::HudPlugin,
            anim::AnimPlugin,
            model::ModelPlugin,
            sound::SoundPlugin,
            fmod::FmodPlugin,
            prosthetic::ProstheticPlugin,
        ))
        // Bevy's plugin tuples hold at most 15.
        .add_plugins((photo::PhotoPlugin, status::StatusPlugin, duel::DuelPlugin, enemy_bullet::EnemyBulletPlugin, showcase::ShowcasePlugin, perf::PerfPlugin))
        .run();
}

/// The log file next to `config.toml` (`paths::root`), rewritten on every start.
fn log_path() -> std::path::PathBuf {
    paths::root().join("sv1.log")
}

/// A second log layer that writes every line (no colours) to `sv1.log`; the terminal keeps
/// Bevy's own. `SHINOBI_NO_LOG_FILE=1` turns it off.
fn log_file_layer(_: &mut App) -> Option<bevy::log::BoxedLayer> {
    use bevy::log::tracing_subscriber::fmt;
    if std::env::var_os("SHINOBI_NO_LOG_FILE").is_some() {
        return None;
    }
    let file = std::fs::File::create(log_path()).ok()?;
    Some(Box::new(fmt::layer().with_ansi(false).with_writer(std::sync::Mutex::new(file))))
}

/// Before the app: without a console of its own (`windows_subsystem = "windows"`) the game
/// attaches to the terminal it was started from, so `sv1.exe` in a shell still prints. The
/// panic hook writes the message to `sv1.log` and, when there is no terminal to read it in,
/// shows it in a dialog with the extract.bat hint (a missing `extracted/` file is the usual
/// cause: config.rs / data.rs `expect`s).
fn startup() {
    #[cfg(windows)]
    let has_console = unsafe { windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS) != 0 } || cfg!(debug_assertions);
    #[cfg(not(windows))]
    let has_console = true;
    crash::install();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let text = format!("{info}");
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(log_path()) {
            use std::io::Write;
            let _ = writeln!(f, "PANIC {text}");
        }
        if !has_console {
            message_box("Sv1 stopped", &format!("{text}\n\nIf a file under extracted/ is missing, run extract.bat first.\nThe full log is in sv1.log."));
        }
    }));
}

#[cfg(windows)]
fn message_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, x) = (wide(title), wide(text));
    unsafe { MessageBoxW(std::ptr::null_mut(), x.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR) };
}

#[cfg(not(windows))]
fn message_box(_title: &str, text: &str) {
    eprintln!("{text}");
}
