//! A HavokScript (HKS) virtual machine that runs the game's own compiled character scripts.
//!
//! Sekiro drives player and enemy behaviour from compiled HKS (a Lua 5.1 derivative) in
//! `action/script/*.hks`. The engine calls script functions every frame (`Update`, and
//! `<State>_onUpdate` / `_onActivate` / `_onDeactivate` for active behaviour states), and the
//! scripts call back through `env`, `act` and `hkb*` functions, which a simulation provides by
//! implementing [`Host`].
//!
//! The VM is original code. Format and opcode knowledge come from public community work; see
//! `docs/HKS-INTERFACE.md`.

pub mod host;
pub mod inventory;
pub mod table;
pub mod value;
pub mod vm;

pub use host::{CommandId, Host, LoggingHost};
pub use sekiro_formats::hks as bytecode;
pub use value::Value;
pub use vm::{Vm, VmError};

/// The player script set. Each main chunk only defines globals (and re-installs the same `_G`
/// fallback), so the load order does not matter; `Initialize` must run after all four.
pub const PLAYER_SCRIPTS: &[&str] = &[
    "c0000.hks",
    "c0000_define.hks",
    "c0000_transition.hks",
    "c0000_cmsg.hks",
];
