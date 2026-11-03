//! Sv1: the RustSekiro game program. The game itself is the library (src/lib.rs); this only
//! starts it. Optimised builds open no console window (the log goes to `sv1.log`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    rustsekiro::run();
}
