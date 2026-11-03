//! `sekiro-rs`'s streaming map, lighting, sky and fog, kept as a library for RSekiro
//! (`src/sr_map.rs`); the rest of the sekiro-rs app was removed (crates/README.md).

mod change;
mod fxr;
mod fxr_tree;
pub mod lighting;
mod lighting_selection;
pub mod map;
mod map_fog_surface;
mod map_fx;
mod map_grass;
mod material_residency;
mod mesh_residency;
mod perf_log;
mod pipeline_warmup;
mod split_sun_cascades;
mod texture_residency;
mod vram;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// When this process started, for load timings measured from launch.
fn process_start() -> std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

/// Seconds since this process started.
fn since_start() -> f32 {
    process_start().elapsed().as_secs_f32()
}
