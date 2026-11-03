//! What RSekiro keeps of sekiro-rs's Bevy-free simulation: the collision world ([`collision`],
//! [`body`]'s ground queries, [`hit_filter`], [`lo_collision`]), the navmesh ([`navmesh`]), and
//! the boss arenas, event scripts and story state the streaming map and its lighting read
//! ([`arena`], [`events`], [`story`], [`world_events`], [`map_effects`]). The rest (characters,
//! fights, bosses, brains) was removed (crates/README.md).

pub mod arena;
pub mod arena_b;
pub mod arena_c;
mod arenas_a;
pub mod body;
pub use sekiro_formats::cache;
pub mod collision;
pub mod enemy;
mod event_values;
pub mod events;
pub mod hit_filter;
mod input;
pub mod lo_collision;
pub mod map_effects;
pub mod navmesh;
mod owl_father;
pub mod perf;
mod registry;
pub mod story;
mod sword_saint;
mod tae;
pub mod world_events;
