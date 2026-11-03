//! Opt-in CPU scopes for the live frame profiler; absent from ordinary simulation work.
use std::sync::OnceLock;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::args().any(|a| a == "--perf" || a == "--profile-systems"))
}
