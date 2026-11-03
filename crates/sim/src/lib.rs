//! Bevy-free gameplay simulation.
//!
//! [`behavior`] runs a Havok behaviour graph (states, events, clips), [`player`] drives it with
//! the game's own HKS player scripts through the `sekiro-hks` VM, [`tae`] turns the playing
//! clips' TimeAct events into cancel windows and SpEffects, [`input`] and [`body`] translate
//! controls and move the character, and [`character`] puts them together into a playable Wolf.
//! [`scenario`] and [`demo`] hold scripted input for tests and CLIs.

pub mod ai;
pub mod arena;
pub mod arena_b;
pub mod arena_c;
pub mod behavior;
pub mod body;
pub mod brain;
pub mod bullets;
pub use sekiro_formats::cache;
pub mod camera;
pub mod character;
pub mod clips;
pub mod collision;
pub mod combat;
pub mod deathblow;
pub mod demo;
#[path = "bosses/demon.rs"]
pub mod demon;
pub mod docking;
pub mod duel;
pub mod enemy;
pub mod events;
pub mod fight;
pub mod fogwall;
pub mod grab;
pub mod hit_filter;
pub mod hit_fx;
pub mod hits;
pub mod homing;
pub mod input;
pub mod isshin;
pub mod isshin_fire;
pub mod items;
pub mod kit;
pub mod lo_collision;
pub mod lua_ai;
pub mod map_effects;
pub mod motion;
pub mod navmesh;
pub mod npc_fall;
pub mod perf;
pub mod perilous;
pub mod player;
pub mod progression;
pub mod registry;
pub mod scenario;
pub mod shared;
pub mod sound;
pub mod sp_category;
pub mod status;
pub mod story;
pub mod tae;
pub mod wire;
pub mod world_events;

pub use behavior::{AnimOffsets, BehaviorRuntime, ClipDurations, ClipState, HookCall, HookKind};
pub use character::{Character, ControlKind, NpcControl, PlayerCharacter, StepReport};
pub use input::{Buttons, InputFrame};
pub use player::{PlayerBehavior, PlayerEnv, TickReport};
pub mod arenas_a;
pub mod butterfly;
pub mod event_values;
pub mod guardian_ape;
pub mod gyoubu;
pub mod owl_father;
pub mod sword_saint;
pub mod true_monk;
