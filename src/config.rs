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
    /// Learned latent skills (SkillParam ids), e.g. 280 Flowing Water (less posture damage when guarding).
    /// Empty = level-1 Wolf.
    #[serde(default)]
    pub skills: Vec<i64>,
}

fn default_combat_art() -> i64 {
    5100
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
        Self { gourd_charges: 3, spirit_emblems: default_emblems(), resurrections: default_resurrections(), combat_art: default_combat_art(), skills: Vec::new() }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct InputConfig {
    pub buffer: f32,
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
}

fn default_doping() -> i64 {
    7010
}
