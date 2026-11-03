//! What a host game's maps add to the core's fight, as hooks the host sets once (this game's map.rs
//! sets sandbox.rs's for the Construct test map): extra characters to load, patrolling guards, the
//! duel enemy's spot, an NPC beside the fight, hiding places, a passive enemy holding its spot.
//! Unset, none of them.

use bevy::prelude::*;
use std::sync::OnceLock;

use crate::config::GameConfig;

/// A guard walking its rounds (spawned from `MapHooks::patrols`): always the real AI.
#[derive(Component)]
pub struct PatrolGuard;

pub struct MapHooks {
    /// Characters the map loads besides the config's (chr, NpcParam row).
    pub extra_chrs: fn(&GameConfig) -> Vec<(String, Option<i64>)>,
    /// Guards walking their rounds: their chr, and each one's start, facing and round.
    pub patrols: fn(&GameConfig) -> Option<(&'static str, Vec<(Vec3, f32, crate::ai::Patrol)>)>,
    /// Where the config's enemy stands and faces (else ahead of Wolf, facing him).
    pub duel_spot: fn(&GameConfig) -> Option<(Vec3, f32)>,
    /// An NPC loaded beside the fight (chr, NpcParam row): Combat::sculptor.
    pub npc: fn(&GameConfig) -> Option<(&'static str, i64)>,
    /// A crouching Wolf's feet here are hidden (bushes).
    pub hidden: fn(Vec3) -> bool,
    /// A passive enemy holds its spot instead of walking into range.
    pub hold: fn() -> bool,
    /// Wolf's start (his centre) in the fight's space, and in arena-local space (the enemy's spot
    /// at (0, y, -4)).
    pub player_start: fn() -> Vec3,
    pub player_start_local: fn() -> Vec3,
    /// Wolf starts at this idol of an open world (no duel line-up).
    pub idol_start: fn() -> Option<i64>,
    /// The map is a whole open world (MSB points at their absolute place).
    pub open_world: fn() -> bool,
    /// The open world's loaded map blocks (its enemies spawn per block).
    pub blocks: fn() -> Vec<String>,
}

/// Wolf's interaction with something of the host's (an idol, an NPC): run each fixed step with
/// whether interact was pressed and the clip ended; true holds Wolf (nothing else of his runs).
pub type Interaction = fn(
    &mut crate::player::Player,
    &mut crate::actor::Actor,
    &crate::data::CharData,
    &mut Transform,
    bool,
    bool,
    &GameConfig,
) -> bool;

/// What Wolf does with the host's world: items lying about, idols and NPCs, the checkpoint.
pub struct WorldHooks {
    /// Takes an item lying at these feet (interact pressed): true when one was taken.
    pub take_item: fn(Vec3) -> bool,
    /// Tried in order, after the item; the first that holds Wolf ends his step.
    pub interactions: &'static [Interaction],
    /// Where Wolf comes back at checkpoint `i` (feet, facing).
    pub respawn_at: fn(usize) -> Option<(Vec3, f32)>,
}

static HOOKS: OnceLock<MapHooks> = OnceLock::new();
static WORLD: OnceLock<WorldHooks> = OnceLock::new();

/// Sets the world hooks (the first call wins).
pub fn set_world(hooks: WorldHooks) {
    let _ = WORLD.set(hooks);
}

pub fn world() -> &'static WorldHooks {
    WORLD.get().unwrap_or(&NO_WORLD)
}

fn no_item(_: Vec3) -> bool {
    false
}
fn no_checkpoint(_: usize) -> Option<(Vec3, f32)> {
    None
}

static NO_WORLD: WorldHooks = WorldHooks { take_item: no_item, interactions: &[], respawn_at: no_checkpoint };

/// An NPC (the Sculptor): not hit, not an enemy.
#[derive(Component)]
pub struct Npc;

/// Sets the hooks (the first call wins).
pub fn set(hooks: MapHooks) {
    let _ = HOOKS.set(hooks);
}

pub fn hooks() -> &'static MapHooks {
    HOOKS.get().unwrap_or(&NONE)
}

fn no_chrs(_: &GameConfig) -> Vec<(String, Option<i64>)> {
    Vec::new()
}
fn no_patrols(_: &GameConfig) -> Option<(&'static str, Vec<(Vec3, f32, crate::ai::Patrol)>)> {
    None
}
fn no_spot(_: &GameConfig) -> Option<(Vec3, f32)> {
    None
}
fn no_npc(_: &GameConfig) -> Option<(&'static str, i64)> {
    None
}
fn never_hidden(_: Vec3) -> bool {
    false
}
fn no_hold() -> bool {
    false
}
fn start_local() -> Vec3 {
    Vec3::new(0.0, crate::player::CAPSULE_HALF_HEIGHT, 4.0)
}
fn start() -> Vec3 {
    crate::place::frame().point(start_local())
}
fn no_idol() -> Option<i64> {
    None
}
fn no_blocks() -> Vec<String> {
    Vec::new()
}

/// No map: the flat arena, Wolf 4 m in front of the enemy.
pub const NONE: MapHooks = MapHooks {
    extra_chrs: no_chrs,
    patrols: no_patrols,
    duel_spot: no_spot,
    npc: no_npc,
    hidden: never_hidden,
    hold: no_hold,
    player_start: start,
    player_start_local: start_local,
    idol_start: no_idol,
    open_world: no_hold,
    blocks: no_blocks,
};
