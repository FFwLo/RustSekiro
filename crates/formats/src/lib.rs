//! Readers for the data formats shipped with Sekiro: Shadows Die Twice (PC, v1.6.0.0).
//!
//! Everything here reads; nothing writes back to the game. Format knowledge comes from public
//! community documentation (SoulsFormats, Paramdex, DSAnimStudio, HavokLib), reimplemented.

pub mod anim;
pub mod bhd;
pub mod bnd4;
pub mod btl;
pub mod bxf4;
pub mod cache;
pub mod dcx;
pub mod dvd;
pub mod edge;
pub mod emedf;
pub mod emevd;
pub mod fev;
pub mod flver;
pub mod fmg;
pub mod fsb;
pub mod fxr;
pub mod fxr_json;
pub mod fxr_schema;
pub mod gfx_font;
pub mod gparam;
pub mod hkb;
pub mod hknp;
pub mod hks;
pub mod hkx;
pub mod map_layers;
pub mod menu_layout;
pub mod msb;
pub mod mtd;
pub mod param;
pub mod paramdef;
pub mod reader;
pub mod tae;
pub mod tpf;

pub use reader::{Error, Result};
