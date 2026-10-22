//! Where the fight is in the host's world: the arena's frame in the map, the map loaded, whether
//! it is a boss's own arena. The host's map sets them on load (this game's map.rs); the start
//! spots and the open world come through encounter.rs `MapHooks`.

use std::sync::RwLock;

use bevy::prelude::*;

/// The host's map loading (Startup): Wolf and the enemies spawn after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapLoad;

/// Arena-local space (the export's, boss at (0, y, -4)) -> the space the fight runs in.
/// Identity, except where the host's map puts the arena at its place in a whole map (this game:
/// under sekiro-rs's map, the source map's absolute Bevy space, map.rs `Frame::of_arena`). A turn
/// about Y plus a shift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub rot: Quat,
    /// The turn's angle (`rot` = Ry(yaw)): added to an arena-local actor yaw.
    pub yaw: f32,
    pub offset: Vec3,
}

impl Frame {
    pub const IDENTITY: Self = Self { rot: Quat::IDENTITY, yaw: 0.0, offset: Vec3::ZERO };

    pub fn point(&self, p: Vec3) -> Vec3 {
        self.rot * p + self.offset
    }

    pub fn dir(&self, d: Vec3) -> Vec3 {
        self.rot * d
    }
}

static FRAME: RwLock<Frame> = RwLock::new(Frame::IDENTITY);

pub fn set_frame(f: Frame) {
    *FRAME.write().unwrap() = f;
}

/// The loaded map's [`Frame`].
pub fn frame() -> Frame {
    *FRAME.read().unwrap()
}

/// json "origin" / "arena_yaw" of an arena export as the readers turning MSB positions into
/// the fight's space need them: in an open world (sekiro-rs's map) origin 0 and no turn, so MSB points land
/// at their absolute place (X mirrored) like the frame's.
pub fn json_frame(json: &serde_json::Value) -> Option<(Vec3, f32)> {
    let o = json["origin"].as_array().filter(|a| a.len() == 3)?;
    if (crate::encounter::hooks().open_world)() {
        return Some((Vec3::ZERO, 0.0));
    }
    Some((Vec3::from_array(std::array::from_fn(|i| o[i].as_f64().unwrap_or(0.0) as f32)), json["arena_yaw"].as_f64().unwrap_or(0.0) as f32))
}

/// Set when the loaded map is a boss's own arena (the host's map: map.rs boss_arena).
static BOSS_ARENA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Is the fight on the boss's own arena (its MSB surroundings, so script positions hold)?
pub fn on_boss_arena() -> bool {
    BOSS_ARENA.load(std::sync::atomic::Ordering::Relaxed) && crate::terrain::terrain().is_some()
}

pub fn set_boss_arena(on: bool) {
    BOSS_ARENA.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The map id loaded last (the host's map: map.rs `load_map_id`): what the world shows now,
/// after any encounter swap. None before the first load or with the map off.
static LOADED_MAP: RwLock<Option<String>> = RwLock::new(None);

pub fn loaded_map() -> Option<String> {
    LOADED_MAP.read().ok()?.clone()
}

pub fn set_loaded_map(id: Option<String>) {
    if let Ok(mut l) = LOADED_MAP.write() {
        *l = id;
    }
}
