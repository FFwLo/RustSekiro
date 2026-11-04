//! Saving and loading Wolf's progress in RSekiro's own file, `saves/<slot>.json` under the root
//! (`SHINOBI_SAVE=<slot>`, default `slot0`; `SHINOBI_SAVE=0` off). The game saves at an idol
//! rest and on quitting; so does this (`rest` sets [`request`], AppExit writes once more).
//!
//! The fields are the game's own save chunks (the exe's SaveData* classes, NS_SPRJ RTTI, each a
//! write / read pair for save versions 100-102; read on our own screen, the format is ours, JSON):
//! - `game` (SaveDataGameDataMan: a 0x138-byte block FUN_1407b3880 gathers and FUN_1407b3450
//!   applies): the play time (GameDataMan+0x9c) and the cycle (GameDataMan+0x70, NG+).
//! - `player` (SaveDataPlayerGameData -> PlayerGameData vtable 0x1429cc9a0 [1] FUN_1407c3ef0):
//!   the 0x180-byte player info at +0x10 (Vitality, Attack Power +0x48, experience +0x160, Sen,
//!   the skill points and level), a 0xb0 block at +0x198, EquipGameData at +0x278 (the
//!   equipment: combat art, prosthetic slots, quick items, ninjutsu; the inventory), FaceData at
//!   +0x740 (the look: our `form`), two MenuRefSpecialEffect at +0x858 / +0x860, and a u32 list at
//!   +0x8b8 (taken as the learned skills).
//! - `event` (SaveDataEvent -> the EventFlagMan singleton's own write, FUN_1406aa9d0): the event
//!   flags that are on; the idols lit (the game's are flags too; here idol.rs's list).
//! - `start` (SaveDataStartPointInfo: 0x50 bytes, FUN_140838480 from GameMan+0xb20.. +0xc3c,
//!   applied by FUN_140833c90): where Wolf comes back, a position and a direction, the map, the
//!   last idol (GameMan+0xc3c). Here: the map blocks, the position, the yaw and the idol entity.
//!
//! gap: the other chunks (Gaitem, EquipHistory, EquipMagic/ThrowSkill, Gesture, Chr/Obj/Decal
//! init states, RematchEvent, Ghost, Network, Trophy, MenuSystem, Option / PcOption / KeyConfig:
//! options stay in config.toml) are not kept; the 0xb0 block's and the player info's other fields
//! are not mapped; loading into another map than the run's needs a new run (the saved idol, as
//! `SHINOBI_IDOL`): only Wolf's place on the same map is restored.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

/// The file's version (ours; the game's chunks are 100-102).
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SaveFile {
    pub version: u32,
    pub game: GameData,
    pub player: PlayerData,
    pub event: EventData,
    pub start: StartPoint,
}

/// SaveDataGameDataMan.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GameData {
    /// Play time (s).
    pub play_time: f64,
    /// Journeys finished (0 the first, NG+ 1...).
    pub cycle: u32,
}

/// SaveDataPlayerGameData: the player info, the equipment and the inventory.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PlayerData {
    pub hp: f32,
    pub attack_power: u32,
    pub sen: u32,
    pub skill_points: u32,
    pub skill_exp: u32,
    pub skill_level: u32,
    pub gourd_charges: u32,
    pub gourd: u32,
    pub spirit_emblems: u32,
    pub resurrections: u32,
    pub res_power: u32,
    pub hard_mode: bool,
    pub form: u32,
    pub combat_art: i64,
    pub ninjutsu: i64,
    pub prosthetics: Vec<i64>,
    pub quick_items: Vec<i64>,
    pub quick_slot: usize,
    pub items: Vec<(i64, u32)>,
    pub owned_tools: Vec<i64>,
    pub upgrades: Vec<i64>,
    pub skills: Vec<i64>,
}

/// SaveDataEvent.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct EventData {
    pub flags: Vec<i64>,
    pub idols: Vec<i64>,
}

/// SaveDataStartPointInfo.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct StartPoint {
    pub map: Vec<String>,
    pub pos: [f32; 3],
    pub yaw: f32,
    /// The last idol rested at (BonfireWarpParam entity; 0 none).
    pub idol: i64,
}

/// Wolf's own state the save keeps (player.rs Player / Actor).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WolfState {
    pub hp: f32,
    pub gourd: u32,
    pub emblems: u32,
    pub resurrections: u32,
    pub res_power: u32,
    pub quick_slot: usize,
}

impl WolfState {
    pub fn of(p: &crate::player::Player, a: &crate::actor::Actor) -> Self {
        WolfState { hp: a.hp, gourd: p.gourd, emblems: p.emblems, resurrections: p.resurrections, res_power: p.res_power, quick_slot: p.quick_slot }
    }
    pub fn put(&self, p: &mut crate::player::Player, a: &mut crate::actor::Actor) {
        a.hp = self.hp;
        p.gourd = self.gourd;
        p.emblems = self.emblems;
        p.resurrections = self.resurrections;
        p.res_power = self.res_power;
        p.quick_slot = self.quick_slot;
    }
}

impl PlayerData {
    /// From the config's progress and Wolf's own state.
    pub fn capture(c: &crate::config::PlayerConfig, w: &WolfState) -> Self {
        PlayerData {
            hp: w.hp,
            attack_power: c.attack_power,
            sen: c.sen,
            skill_points: c.skill_points,
            skill_exp: c.skill_exp,
            skill_level: c.skill_level,
            gourd_charges: c.gourd_charges,
            gourd: w.gourd,
            spirit_emblems: w.emblems,
            resurrections: w.resurrections,
            res_power: w.res_power,
            hard_mode: c.hard_mode,
            form: c.form,
            combat_art: c.combat_art,
            ninjutsu: c.ninjutsu,
            prosthetics: c.prosthetics.clone(),
            quick_items: c.quick_items.clone(),
            quick_slot: w.quick_slot,
            items: c.items.clone(),
            owned_tools: c.owned_tools.clone(),
            upgrades: c.upgrades.clone(),
            skills: c.skills.clone(),
        }
    }

    /// Back into the config (what the menus and the next run read). The resurrection max stays
    /// the config's (the nodes left are Wolf's, `wolf`).
    pub fn apply_config(&self, c: &mut crate::config::PlayerConfig) {
        c.attack_power = self.attack_power;
        c.sen = self.sen;
        c.skill_points = self.skill_points;
        c.skill_exp = self.skill_exp;
        c.skill_level = self.skill_level;
        c.gourd_charges = self.gourd_charges;
        c.spirit_emblems = self.spirit_emblems;
        c.hard_mode = self.hard_mode;
        c.form = self.form;
        c.combat_art = self.combat_art;
        c.ninjutsu = self.ninjutsu;
        c.prosthetics = self.prosthetics.clone();
        c.quick_items = self.quick_items.clone();
        c.items = self.items.clone();
        c.owned_tools = self.owned_tools.clone();
        c.upgrades = self.upgrades.clone();
        c.skills = self.skills.clone();
    }

    /// Wolf as saved, his Vitality capped at `hp_max` (a dead or empty save: full), the gourd at
    /// its charges, the quick slot on the list.
    pub fn wolf(&self, hp_max: f32) -> WolfState {
        WolfState {
            hp: if self.hp > 0.0 { self.hp.min(hp_max) } else { hp_max },
            gourd: self.gourd.min(self.gourd_charges),
            emblems: self.spirit_emblems,
            resurrections: self.resurrections,
            res_power: self.res_power,
            quick_slot: self.quick_slot.min(self.quick_items.len().saturating_sub(1)),
        }
    }
}

impl SaveFile {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
    /// A file of another version or not ours: None (a new game).
    pub fn from_json(t: &str) -> Option<Self> {
        let s: SaveFile = serde_json::from_str(t).ok()?;
        (s.version == VERSION).then_some(s)
    }
}

/// The slot (`SHINOBI_SAVE`), None when saving is off.
pub fn slot() -> Option<String> {
    match std::env::var("SHINOBI_SAVE") {
        Ok(v) if v == "0" => None,
        Ok(v) if !v.is_empty() => Some(v),
        // The tests' apps never read or write a save unless asked.
        _ if cfg!(test) => None,
        _ => Some("slot0".to_string()),
    }
}

pub fn path(slot: &str) -> PathBuf {
    crate::paths::root().join("saves").join(format!("{slot}.json"))
}

/// Writes the save (a temporary file renamed over it, so a crash mid-write keeps the last one).
pub fn write(slot: &str, s: &SaveFile) -> std::io::Result<()> {
    let p = path(slot);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, s.to_json())?;
    std::fs::rename(tmp, p)
}

pub fn read(slot: &str) -> Option<SaveFile> {
    SaveFile::from_json(&std::fs::read_to_string(path(slot)).ok()?)
}

static REQUEST: AtomicBool = AtomicBool::new(false);
/// The flags of the loaded save, for the map events that start after it (world.rs `load`).
static LOADED_FLAGS: RwLock<Vec<i64>> = RwLock::new(Vec::new());

/// Asks for a save at the end of the frame (an idol rest, idol.rs).
pub fn request() {
    REQUEST.store(true, Ordering::Relaxed);
}

/// The event flags the loaded save had on.
pub fn loaded_flags() -> Vec<i64> {
    LOADED_FLAGS.read().map(|f| f.clone()).unwrap_or_default()
}

/// Play time since the run started plus the loaded save's.
#[derive(Resource, Default)]
pub struct PlayTime {
    pub secs: f64,
    pub cycle: u32,
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        if slot().is_none() {
            return;
        }
        app.init_resource::<PlayTime>().add_systems(OnEnter(crate::menu::Screen::Playing), load_once).add_systems(Last, (tick_play_time, save_when_asked));
    }
}

fn tick_play_time(time: Res<Time<Real>>, mut t: ResMut<PlayTime>) {
    t.secs += time.delta_secs_f64();
}

/// Everything the save keeps, from the world.
#[allow(clippy::too_many_arguments)]
fn capture(config: &crate::config::GameConfig, p: &crate::player::Player, a: &crate::actor::Actor, tf: &Transform, t: &PlayTime, bs: Option<&crate::enemy::BossScript>, ws: Option<&crate::enemy::world::WorldScripts>) -> SaveFile {
    let mut flags: Vec<i64> = bs.and_then(|b| b.run.as_ref()).map(|r| r.flags_on()).unwrap_or_default();
    flags.extend(ws.map(|w| w.flags_on()).unwrap_or_default());
    flags.extend(loaded_flags());
    flags.sort_unstable();
    flags.dedup();
    let idol = p.seat.checkpoint.and_then(|i| crate::idol::IDOLS.read().ok()?.get(i).map(|i| i.entity)).unwrap_or(0);
    SaveFile {
        version: VERSION,
        game: GameData { play_time: t.secs, cycle: t.cycle },
        player: PlayerData::capture(&config.player, &WolfState::of(p, a)),
        event: EventData { flags, idols: crate::idol::found_idols() },
        start: StartPoint { map: (crate::encounter::hooks().blocks)(), pos: tf.translation.to_array(), yaw: a.yaw, idol },
    }
}

#[allow(clippy::type_complexity)]
fn save_when_asked(
    config: Res<crate::config::GameConfig>,
    t: Res<PlayTime>,
    wolf: Query<(&crate::player::Player, &crate::actor::Actor, &Transform)>,
    bs: Option<Res<crate::enemy::BossScript>>,
    ws: Option<Res<crate::enemy::world::WorldScripts>>,
    mut exit: MessageReader<AppExit>,
) {
    let quitting = exit.read().count() > 0;
    if !(REQUEST.swap(false, Ordering::Relaxed) || quitting) {
        return;
    }
    let (Some(slot), Ok((p, a, tf))) = (slot(), wolf.single()) else { return };
    let s = capture(&config, p, a, tf, &t, bs.as_deref(), ws.as_deref());
    match write(&slot, &s) {
        Ok(()) => info!("saved {} ({} flags, {} idols)", path(&slot).display(), s.event.flags.len(), s.event.idols.len()),
        Err(e) => warn!("save {}: {e}", path(&slot).display()),
    }
}

/// On the first Playing: the save, if there is one, over the config's new-game state.
fn load_once(mut done: Local<bool>, mut config: ResMut<crate::config::GameConfig>, mut t: ResMut<PlayTime>, mut wolf: Query<(&mut crate::player::Player, &mut crate::actor::Actor, &mut Transform)>) {
    if *done {
        return;
    }
    *done = true;
    let Some(s) = slot().and_then(|s| read(&s)) else { return };
    s.player.apply_config(&mut config.player);
    t.secs = s.game.play_time;
    t.cycle = s.game.cycle;
    if let Ok(mut f) = LOADED_FLAGS.write() {
        *f = s.event.flags.clone();
    }
    crate::idol::restore_found(&s.event.idols);
    if let Ok((mut p, mut a, mut tf)) = wolf.single_mut() {
        let hp_max = a.hp_max;
        s.player.wolf(hp_max).put(&mut p, &mut a);
        // Only on the map the save was made on (another needs a new run there).
        if s.start.map == (crate::encounter::hooks().blocks)() && s.start.pos.iter().all(|v| v.is_finite()) {
            tf.translation = Vec3::from_array(s.start.pos);
            a.yaw = s.start.yaw;
        }
    }
    info!("loaded {} (play time {:.0} s)", slot().map(|s| path(&s).display().to_string()).unwrap_or_default(), s.game.play_time);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveFile {
        SaveFile {
            version: VERSION,
            game: GameData { play_time: 3723.5, cycle: 1 },
            player: PlayerData {
                hp: 640.0,
                attack_power: 4,
                sen: 1234,
                skill_points: 2,
                skill_exp: 77,
                skill_level: 5,
                gourd_charges: 5,
                gourd: 3,
                spirit_emblems: 12,
                resurrections: 2,
                res_power: 40,
                hard_mode: false,
                form: 1,
                combat_art: 5300,
                ninjutsu: 2100,
                prosthetics: vec![70000, 71000, 72000],
                quick_items: vec![3000, 3100, 0, 0, 0],
                quick_slot: 1,
                items: vec![(3100, 4), (9000, 1)],
                owned_tools: vec![70000, 71000, 72000],
                upgrades: vec![700],
                skills: vec![280, 300],
            },
            event: EventData { flags: vec![11100000, 6000], idols: vec![1101950] },
            start: StartPoint { map: vec!["m11_00_00_00".into()], pos: [1.0, 2.0, 3.0], yaw: 0.5, idol: 1101950 },
        }
    }

    /// A save keeps the game's chunks (game data, player game data, event flags, start point)
    /// through the file: written and read back the same, a file of another version or not
    /// ours is a new game.
    #[test]
    fn the_save_keeps_the_games_chunks_through_its_file() {
        let s = sample();
        assert_eq!(SaveFile::from_json(&s.to_json()), Some(s.clone()));
        let mut old = s.clone();
        old.version = VERSION + 1;
        assert_eq!(SaveFile::from_json(&old.to_json()), None);
        assert_eq!(SaveFile::from_json("{not json"), None);
        // On disk under the root's saves/, through a temporary file.
        let slot = format!("test_{}", std::process::id());
        write(&slot, &s).unwrap();
        assert_eq!(read(&slot), Some(s));
        let _ = std::fs::remove_file(path(&slot));
    }

    /// Loading puts the saved progress over the config's new-game state and back into Wolf
    /// (Vitality capped at his max, the gourd at its charges, the quick slot on the list).
    #[test]
    fn loading_a_save_restores_wolfs_progress() {
        let s = sample();
        let mut c = crate::config::PlayerConfig::default();
        s.player.apply_config(&mut c);
        assert_eq!((c.sen, c.skill_points, c.attack_power, c.combat_art, c.ninjutsu), (1234, 2, 4, 5300, 2100));
        assert_eq!(c.prosthetics, vec![70000, 71000, 72000]);
        assert_eq!(c.items, vec![(3100, 4), (9000, 1)]);
        assert_eq!(c.skills, vec![280, 300]);
        let mut d = s.player.clone();
        d.gourd = 9;
        d.quick_slot = 7;
        let w = d.wolf(500.0);
        assert_eq!(w.hp, 500.0, "capped at the max");
        assert_eq!((w.gourd, w.emblems, w.resurrections, w.res_power, w.quick_slot), (5, 12, 2, 40, 4));
        d.hp = 0.0;
        assert_eq!(d.wolf(500.0).hp, 500.0, "a dead save comes back whole");
        // And captured back the same.
        let back = PlayerData::capture(&c, &w);
        assert_eq!((back.sen, back.skills.clone(), back.gourd, back.quick_slot, back.hp), (1234, vec![280, 300], 5, 4, 500.0));
    }
}
