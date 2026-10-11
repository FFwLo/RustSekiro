//! Loads the tunable gap values from `config.toml` and reloads them on F5.
//! Values that Sekiro's data provides are not here; see data.rs.

use bevy::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;

/// All tuning values. The layout mirrors the sections in `config.toml`.
#[derive(Resource, Deserialize, Clone, Debug)]
pub struct GameConfig {
    pub camera: CameraConfig,
    pub movement: MovementConfig,
    #[serde(default)]
    pub player: PlayerConfig,
    pub input: InputConfig,
    pub posture: PostureConfig,
    pub combat: CombatConfig,
    pub enemy: EnemyConfig,
    #[serde(default)]
    pub world: WorldConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub video: VideoConfig,
}

/// The Options screen's sound sliders (menu.rs), 0-10 like the game's. `volume` is ours (the
/// game has none): the master category at volume / 10 on top of the game's own 0.7 (fmod.rs
/// MASTER_VOLUME). `music` / `se` / `voice` are the game's option bytes 4 / 5 / 6 (Bgm / Se /
/// Voice), each its FMOD categories at value / 10 (fmod.rs set_mix).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct AudioConfig {
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_volume")]
    pub music: f32,
    #[serde(default = "default_volume")]
    pub se: f32,
    #[serde(default = "default_volume")]
    pub voice: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig { volume: default_volume(), music: default_volume(), se: default_volume(), voice: default_volume() }
    }
}

/// The game's slider default (GameDataMan options bytes 4-6 = 10, FUN_1407bc470).
fn default_volume() -> f32 {
    10.0
}

/// The window (menu.rs apply_video): borderless fullscreen or a window, vsync on / off.
#[derive(Deserialize, Clone, Debug)]
pub struct VideoConfig {
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "default_true")]
    pub vsync: bool,
    /// Window size in pixels (menu.rs apply_video); None = the default 1024x560.
    #[serde(default)]
    pub resolution: Option<[u32; 2]>,
    /// Sun shadow map size in pixels (map.rs, the frame rate's main knob); None = Bevy's 2048.
    /// `SHINOBI_MAP_SHADOW_RES` overrides it for one run.
    #[serde(default)]
    pub shadow_res: Option<usize>,
}

impl Default for VideoConfig {
    fn default() -> Self {
        VideoConfig { fullscreen: false, vsync: true, resolution: None, shadow_res: None }
    }
}

/// Sets `key = value` in `[table]` of config.toml's text, keeping every other line and comment
/// (adds the key under the table, or the table at the end, when missing).
pub fn set_key(text: &str, table: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let (mut inside, mut seen, mut done) = (false, false, false);
    let header = format!("[{table}]");
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            if inside && !done {
                // Before the next table (and the blank / comment lines that lead into it).
                let at = out.iter().rposition(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')).map_or(out.len(), |i| i + 1);
                out.insert(at, format!("{key} = {value}"));
                done = true;
            }
            inside = t.starts_with(&header);
            seen |= inside;
        }
        if inside && !done && t.starts_with(key) && t[key.len()..].trim_start().starts_with('=') {
            // The trailing comment with the spaces before it.
            let comment = t.find('#').map(|i| t[t[..i].trim_end().len()..].to_string()).unwrap_or_default();
            out.push(format!("{key} = {value}{comment}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !done {
        if !seen {
            out.push(String::new());
            out.push(header);
        }
        out.push(format!("{key} = {value}"));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// Writes these `(table, key, value)` into config.toml (set_key each).
pub fn save_keys(keys: &[(&str, &str, String)]) -> Result<(), String> {
    let path = GameConfig::path();
    let mut text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    for (table, key, value) in keys {
        text = set_key(&text, table, key, value);
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The arena: the game's own map (`sekiro-extract map` output) or the flat test floor.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct WorldConfig {
    /// Map id of `extracted/map_<id>.bin` (e.g. "m11_01_00_00"); unset = flat arena.
    #[serde(default)]
    pub map: Option<String>,
    /// Time of day for the draw params (hours). gap: the game's clock for the area.
    #[serde(default = "default_hour")]
    pub hour: f32,
}

fn default_hour() -> f32 {
    18.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct CameraConfig {
    pub mouse_sensitivity: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MovementConfig {
    pub turn_speed: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PlayerConfig {
    /// Healing Gourd charges (goods 3000 maxNum 10; one more per Gourd Seed in the game).
    pub gourd_charges: u32,
    /// Spirit Emblems carried (prosthetic ammunition).
    #[serde(default = "default_emblems")]
    pub spirit_emblems: u32,
    /// Resurrection nodes (the game starts with 1).
    #[serde(default = "default_resurrections")]
    pub resurrections: u32,
    /// Equipped combat art = its virtual weapon (EquipParamWeapon): 5100 Whirlwind Slash,
    /// 5300 Ichimonji, 5200 / 5400-5900 the others (anim group a<spAtkcategory>).
    #[serde(default = "default_combat_art")]
    pub combat_art: i64,
    /// Prosthetic tool slots (EquipParamWeapon 70000-79200, up to 3 like the game; Z switches):
    /// 70000 Loaded Shuriken, 71000 Shinobi Firecracker, 72000 Flame Vent, 73000 Loaded Axe,
    /// 74000 Mist Raven, 75000 Sabimaru, 76000 Loaded Umbrella, 77000 Divine Abduction,
    /// 78000 Loaded Spear, 79000 Finger Whistle (+100 per upgrade level).
    #[serde(default = "default_prosthetics")]
    pub prosthetics: Vec<i64>,
    /// Learned latent skills (SkillParam ids), e.g. 280 Flowing Water (less posture damage when guarding).
    /// Empty = level-1 Wolf.
    #[serde(default)]
    pub skills: Vec<i64>,
    /// Every move the game locks behind a skill (`combat::action_unlocked`: mid-air deflect / tools
    /// / arts, the sprint slide, the Prosthetic Arts follow-ups) is open, as for a Wolf with the
    /// whole tree. false = only those the `skills` list unlocks.
    #[serde(default = "default_true")]
    pub unlock_all_moves: bool,
}

fn default_true() -> bool {
    true
}

fn default_combat_art() -> i64 {
    5100
}

fn default_prosthetics() -> Vec<i64> {
    vec![70000]
}

fn default_enemy_chr() -> String {
    "c1020".to_string()
}

fn default_resurrections() -> u32 {
    1
}

fn default_emblems() -> u32 {
    15
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self { gourd_charges: 3, spirit_emblems: default_emblems(), resurrections: default_resurrections(), combat_art: default_combat_art(), prosthetics: default_prosthetics(), skills: Vec::new(), unlock_all_moves: true }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct InputConfig {
    pub buffer: f32,
    /// A gamepad drives Wolf, the camera and the menus (plan C4).
    #[serde(default = "default_true")]
    pub gamepad: bool,
    /// The game's camera speed option, 1-10: the right stick turns at CameraParam
    /// rotSpeed_Min + (Max - Min) * (speed / 10)^2 (camera.rs stick_rate).
    #[serde(default = "default_camera_speed")]
    pub camera_speed: f32,
    /// Right stick / mouse up looks down.
    #[serde(default)]
    pub invert_y: bool,
}

/// Byte 0 of the game's option defaults is 5 (FUN_1407bc470, beside the volume bytes 4-6 = 10);
/// it is the camera speed: FUN_140734790 hands byte 0 / 10 to the camera (+0x270), which scales
/// the CameraParam stick speeds by it (FUN_14073c260). gap: the menu's slider range (1-10 here).
fn default_camera_speed() -> f32 {
    5.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct PostureConfig {
    pub default_ratio_type: i64,
    pub regen_delay: f32,
    pub hp_scales_regen: bool,
    pub player_level: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CombatConfig {
    pub player_reach: f32,
    pub enemy_reach: f32,
    pub attack_arc: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct EnemyConfig {
    /// Which enemy to fight: "c1020" Samurai General, "c1010" Ochimusha.
    #[serde(default = "default_enemy_chr")]
    pub chr: String,
    pub attack_range: f32,
    pub cooldown_min: f32,
    pub cooldown_max: f32,
    pub broken_time: f32,
    #[serde(default = "default_doping")]
    pub area_doping: i64,
    /// NpcParam row (outfit variant / stats), e.g. c1020 10203010 no haori, 10200010 with haori.
    /// Unset: the default row of `chr`.
    #[serde(default)]
    pub npc_row: Option<i64>,
    /// More enemies fought at the same time, e.g.
    /// `group = [{ chr = "c1010", at = [3.0, -6.0] }, { chr = "c1020", npc_row = 10200010 }]`.
    /// Empty: only `chr`.
    #[serde(default)]
    pub group: Vec<GroupEnemy>,
}

/// One more enemy (config enemy.group): its chr, NpcParam row (unset: its default) and where it
/// stands (x, z; unset: in a row beside the first).
#[derive(Deserialize, Clone, Debug)]
pub struct GroupEnemy {
    pub chr: String,
    #[serde(default)]
    pub npc_row: Option<i64>,
    #[serde(default)]
    pub at: Option<[f32; 2]>,
}

impl GameConfig {
    /// `config.toml` sits next to `Cargo.toml`, wherever the game is started from.
    fn path() -> PathBuf {
        crate::paths::root().join("config.toml")
    }

    fn load() -> Result<Self, String> {
        let path = Self::path();
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("config.toml has a mistake: {e}"))
    }
}

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        let config = GameConfig::load().unwrap_or_else(|e| panic!("{e}"));
        app.insert_resource(config)
            .add_systems(Update, reload_on_f5.run_if(resource_exists::<ButtonInput<KeyCode>>));
    }
}

fn reload_on_f5(keys: Res<ButtonInput<KeyCode>>, mut config: ResMut<GameConfig>) {
    if !keys.just_pressed(KeyCode::F5) {
        return;
    }
    match GameConfig::load() {
        Ok(new_config) => {
            *config = new_config;
            info!("config.toml reloaded");
        }
        // A typo in the file keeps the old values instead of crashing the game.
        Err(e) => error!("{e} (keeping the previous values)"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn config_toml_parses() {
        super::GameConfig::load().expect("config.toml must parse");
    }

    /// The Options writer keeps the comments, replaces in place and adds what is missing.
    #[test]
    fn set_key_keeps_the_file() {
        let text = "# top\n[input]\n# the buffer\nbuffer = 0.0  # seconds\n\n[posture]\nx = 1\n";
        let t = super::set_key(text, "input", "buffer", "0.5");
        assert!(t.contains("buffer = 0.5  # seconds") && t.contains("# the buffer") && t.contains("# top"), "{t}");
        let t = super::set_key(&t, "input", "invert_y", "true");
        assert!(t.find("invert_y = true").unwrap() < t.find("[posture]").unwrap(), "{t}");
        let t = super::set_key(&t, "video", "vsync", "false");
        assert!(t.ends_with("[video]\nvsync = false\n"), "{t}");
        let c: toml::Value = toml::from_str(&t).unwrap();
        assert_eq!(c["input"]["buffer"].as_float(), Some(0.5));
    }
}

fn default_doping() -> i64 {
    7010
}
