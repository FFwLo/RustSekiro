//! Where the game's files are: the folder holding `extracted/` (the data generated from the
//! player's copy of Sekiro) and `config.toml`. A development build uses the project folder; a
//! release build, run from its own folder, uses the folder the exe sits in. `SHINOBI_ROOT`
//! overrides both; a game that takes the core in sets it with [`set_root`].

use std::path::PathBuf;
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The RustSekiro folder (its `extracted/` and `config.toml`), before anything reads a file;
/// false when the root was already taken.
pub fn set_root(dir: PathBuf) -> bool {
    ROOT.set(dir).is_ok()
}

pub fn root() -> PathBuf {
    ROOT.get_or_init(|| {
        if let Some(r) = std::env::var_os("SHINOBI_ROOT") {
            return PathBuf::from(r);
        }
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from));
        match exe_dir {
            Some(d) if d.join("config.toml").exists() => d,
            _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        }
    })
    .clone()
}

/// `extracted/` under the root.
pub fn extracted() -> PathBuf {
    root().join("extracted")
}

/// The Sekiro install (for its FMOD runtime): `SEKIRO_DIR`, else the folder tools/extract.ps1
/// recorded in extracted/sekiro_dir.txt, else the default Steam location.
pub fn sekiro_dir() -> String {
    std::env::var("SEKIRO_DIR")
        .ok()
        .or_else(|| std::fs::read_to_string(extracted().join("sekiro_dir.txt")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro".to_string())
}

/// The asset source the core loads its textures from (`rustsekiro://tex/...`): extracted/, so they load whatever the host game's own asset folder is.
pub const SOURCE: &str = "rustsekiro";

/// An `extracted/` file as an asset path in [`SOURCE`] (the host game's own asset folder is elsewhere).
pub fn asset(rel: impl AsRef<str>) -> String {
    format!("{SOURCE}://{}", rel.as_ref())
}

/// Registers [`SOURCE`]; before `DefaultPlugins` (the `AssetPlugin`).
pub fn register_assets(app: &mut bevy::prelude::App) {
    use bevy::asset::AssetApp;
    app.register_asset_source(SOURCE, bevy::asset::io::AssetSourceBuilder::platform_default(&extracted().to_string_lossy(), None));
}
