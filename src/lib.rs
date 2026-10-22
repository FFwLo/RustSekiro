//! RustSekiro (Sv1): a Sekiro recreation driven by data read from the player's own Sekiro
//! install (see tools/extract.ps1 and data.rs), built as a library; the `sv1` program is
//! `rustsekiro::run()`.
//!
//! Without the default `host` feature the library is the combat core alone (Wolf, enemies,
//! combat, the HKS scripts, animation, effects): another Bevy game adds [`CombatCorePlugin`] and
//! plugs its map, camera and sound in through the hooks re-exported below. With `embed` it can
//! also take RustSekiro's camera (`CameraPlugin`, on the game's camera marked `HostCamera`) and
//! debug menu (sekiro-rs's `--rustsekiro` mode).
//!
//! Each `mod` line pulls in one file from `src/`. Each file provides a
//! "plugin", a bundle of systems (functions that run every frame).
//!
//! Optimised builds open no console window: the log goes to `sv1.log` next to `config.toml`
//! (and to the terminal the game was started from, if any), a panic shows a "Sv1 stopped"
//! dialog (`startup`).

use bevy::prelude::*;

#[cfg(feature = "embed")]
mod action_guide;
mod actor;
mod ai;
mod anim;
#[cfg(feature = "embed")]
mod area_music;
#[cfg(feature = "embed")]
mod camera;
mod cloth;
#[cfg(feature = "host")]
mod crash;
mod combat;
mod config;
mod data;
#[cfg(feature = "embed")]
mod debug_menu;
#[cfg(feature = "host")]
mod drops;
mod docking;
#[cfg(feature = "host")]
mod duel;
mod emevd;
mod encounter;
mod enemy;
mod enemy_bullet;
#[cfg(feature = "embed")]
mod experience;
mod ffx;
mod fxr;
#[cfg(feature = "host")]
mod fog_wall;
#[cfg(feature = "embed")]
mod fonts;
mod form;
#[cfg(feature = "host")]
mod height_fog;
#[cfg(feature = "host")]
mod sky;
#[cfg(feature = "embed")]
mod fmod;
#[cfg(feature = "embed")]
mod game_menu;
#[cfg(feature = "embed")]
mod idol;
#[cfg(feature = "embed")]
mod idol_found;
mod items;
mod gore;
mod guard;
#[cfg(feature = "host")]
mod grading;
mod hks;
mod hks_enemy;
mod hks_wolf;
#[cfg(all(test, feature = "host"))]
mod hks_lua;
mod hkb_runtime;
#[cfg(feature = "embed")]
mod hud;
#[cfg(feature = "host")]
mod icon;
mod interp;
#[cfg(feature = "host")]
mod map;
#[cfg(feature = "host")]
mod map_fx;
#[cfg(feature = "host")]
mod map_grass;
#[cfg(feature = "host")]
mod map_lights;
#[cfg(feature = "embed")]
mod menu;
#[cfg(feature = "host")]
mod sandbox;
#[cfg(feature = "host")]
mod sculptor;
mod seat;
#[cfg(feature = "host")]
mod showcase;
#[cfg(feature = "host")]
mod shot;
#[cfg(feature = "embed")]
mod skill_tree;
mod model;
pub mod navmesh;
mod paths;
mod perf;
#[cfg(feature = "embed")]
mod pickups;
mod place;
#[cfg(feature = "host")]
mod photo;
mod ninjutsu;
mod player;
mod prosthetic;
#[cfg(all(test, feature = "host"))]
mod shader_tests;
#[cfg(all(test, feature = "host"))]
mod sim_tests;
#[cfg(feature = "embed")]
mod sound;
mod sound_rules;
#[cfg(feature = "host")]
mod sr_map;
mod status;
mod stealth;
mod swim;
mod terrain;
mod traversal;
#[cfg(feature = "host")]
mod throw_trace;
#[cfg(feature = "host")]
mod trace;
#[cfg(feature = "embed")]
mod ui_stage;
#[cfg(feature = "host")]
mod ufo;
#[cfg(feature = "host")]
mod autoboss;
mod vfx;
mod view;
mod wire;
#[cfg(feature = "embed")]
mod workshop;
#[cfg(feature = "host")]
mod world;

/// The combat core for another Bevy game: Wolf, the enemies, combat, the game's HKS scripts and
/// behaviour graphs, animation, models, effects, prosthetics and status effects. The host adds its
/// map (`set_map_hooks`, `set_world_hooks`, a `Terrain` with its `WorldCollision`), the camera
/// (`View`) and plays the `SoundQueue` / `TrackedSounds`.
pub struct CombatCorePlugin;

impl Plugin for CombatCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            config::ConfigPlugin,
            data::DataPlugin,
            actor::ActorPlugin,
            combat::CombatPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            anim::AnimPlugin,
            model::ModelPlugin,
            prosthetic::ProstheticPlugin,
            status::StatusPlugin,
            enemy_bullet::EnemyBulletPlugin,
            hks_wolf::HksWolfPlugin,
            hks_enemy::HksEnemyPlugin,
            cloth::ClothPlugin,
            items::ItemsPlugin,
        ))
        .add_plugins((interp::InterpPlugin, vfx::VfxPlugin, wire::WirePlugin, gore::GorePlugin, ffx::FfxPlugin, fxr::FxrPlugin, perf::PerfPlugin));
    }
}

pub use config::GameConfig;
pub use paths::{extracted, register_assets, set_root};
pub use traversal::{GrapplePoint, GrapplePoints, GrappleTarget, LedgeEdge, LedgeEdges};
pub use wire::WireAim;
#[cfg(feature = "embed")]
pub use camera::CameraPlugin;
#[cfg(all(feature = "embed", not(feature = "host")))]
pub use camera::HostCamera;
#[cfg(feature = "embed")]
pub use debug_menu::DebugMenuPlugin;

/// The HUD for another game (`embed`): Wolf's and the enemy's bars, posture, the lock and
/// deathblow marks, item icons, Sen and experience, on RustSekiro's 16:9 UI stage.
#[cfg(feature = "embed")]
pub struct HudPlugin;

#[cfg(feature = "embed")]
impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((fonts::FontsPlugin, ui_stage::UiStagePlugin, experience::ExperiencePlugin, hud::HudPlugin));
    }
}
pub use actor::Actor;
#[cfg(feature = "embed")]
pub use idol::{set_idol_hooks, IdolHooks};

/// The menus for another game (`embed`): the pause and death screens, the in-game menu, the
/// Sculptor's Idols (rest, travel, skills, emblems; the host places them with
/// `set_idol_hooks`), items on the ground and the action guide.
#[cfg(feature = "embed")]
pub struct MenuPlugins;

#[cfg(feature = "embed")]
impl Plugin for MenuPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((menu::MenuPlugin, game_menu::GameMenuPlugin, idol::IdolPlugin, idol_found::IdolFoundPlugin, pickups::PickupsPlugin, action_guide::ActionGuidePlugin));
    }
}
pub use player::{Player, CAPSULE_HALF_HEIGHT};
pub use encounter::{hooks as map_hooks, set as set_map_hooks, set_world as set_world_hooks, world as world_hooks, Interaction, MapHooks, WorldHooks, NONE as NO_MAP_HOOKS};
pub use place::{set_boss_arena, set_frame, set_loaded_map, Frame, MapLoad};
pub use sound_rules::{MapSpace, SoundCmd, SoundPlace, SoundQueue, TrackedSound, TrackedSounds};

/// The sound for another game (`embed`): FMOD from the Sekiro install (the cues, footsteps,
/// voices and reverb) and the area and boss music. The host sets the [`SoundPlace`].
#[cfg(feature = "embed")]
pub struct SoundPlugins;

#[cfg(feature = "embed")]
impl Plugin for SoundPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((sound::SoundPlugin, fmod::FmodPlugin, area_music::AreaMusicPlugin));
    }
}
pub use terrain::{Terrain, WorldCollision};
pub use view::{CameraSnap, LockOn, View};

/// The whole game (`sv1`): the window, every plugin and the map.
#[cfg(feature = "host")]
mod host_app {
    use super::*;
    use bevy::window::WindowResolution;

    /// The game: the window, every plugin and the map, as the `sv1` program runs it.
    pub fn run() {
        startup();
        let sr = sr_map::active();
        let mut app = App::new();
        paths::register_assets(&mut app);
        if sr {
            sr_map::register_source(&mut app);
        }
        let defaults = DefaultPlugins.set(AssetPlugin { file_path: paths::extracted().to_string_lossy().into_owned(), ..default() }).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Sv1".into(),
                resolution: WindowResolution::new(1024, 560),
                ..default()
            }),
            ..default()
        }).set(bevy::log::LogPlugin { custom_layer: log_file_layer, ..default() });
        // sekiro-rs's map GLBs carry its layered-material vertex attributes.
        let defaults = if sr { defaults.set(sekiro_rs::map::layered_gltf_plugin()) } else { defaults };
        app.add_plugins(defaults)
            .add_plugins(cloth::ClothPlugin)
            .add_plugins(hks_wolf::HksWolfPlugin)
            .add_plugins(hks_enemy::HksEnemyPlugin)
            .add_plugins(action_guide::ActionGuidePlugin)
            .add_plugins(shot::ShotPlugin)
            .add_plugins(fonts::FontsPlugin)
            .add_plugins(ui_stage::UiStagePlugin)
            .add_plugins(game_menu::GameMenuPlugin)
            .add_plugins(idol::IdolPlugin)
            .add_plugins(idol_found::IdolFoundPlugin)
            .add_plugins(sculptor::SculptorPlugin)
            .add_plugins(items::ItemsPlugin)
            .add_plugins(pickups::PickupsPlugin)
            .add_plugins(experience::ExperiencePlugin)
            .add_plugins(drops::DropsPlugin)
            .add_plugins(interp::InterpPlugin)
            .add_plugins(debug_menu::DebugMenuPlugin)
            .add_plugins(ufo::UfoPlugin)
            .add_plugins(trace::TracePlugin)
            .add_plugins(autoboss::AutoBossPlugin)
            .add_plugins(throw_trace::ThrowTracePlugin)
            .add_plugins(vfx::VfxPlugin)
            .add_plugins(wire::WirePlugin)
            .add_plugins(gore::GorePlugin)
            .add_plugins(area_music::AreaMusicPlugin)
            .add_plugins(ffx::FfxPlugin)
            .add_plugins(fxr::FxrPlugin)
            .add_plugins(fog_wall::FogWallPlugin)
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
            .add_plugins((photo::PhotoPlugin, status::StatusPlugin, duel::DuelPlugin, enemy_bullet::EnemyBulletPlugin, showcase::ShowcasePlugin, perf::PerfPlugin));
        if sr {
            // sekiro-rs draws the world, sky, fog and lights (sr_map.rs).
            sr_map::install(&mut app);
            if !sr_map::has_grass() {
                app.add_plugins(map_grass::MapGrassPlugin);
            }
        } else {
            app.add_plugins((height_fog::HeightFogPlugin, sky::SkyPlugin, map_lights::MapLightsPlugin, map_fx::MapFxPlugin, map_grass::MapGrassPlugin));
        }
        app.run();
    }

    /// The log file next to `config.toml` (`paths::root`), rewritten on every start.
    pub(crate) fn log_path() -> std::path::PathBuf {
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
}

#[cfg(feature = "host")]
pub use host_app::run;
