//! Opt-in CPU scopes for the live frame profiler; absent from ordinary simulation work.
use std::{cell::RefCell, collections::BTreeMap, sync::OnceLock, time::Instant};

pub type Samples = BTreeMap<&'static str, (usize, f64, f64)>;
thread_local! { static SAMPLES: RefCell<Samples> = RefCell::default(); }

pub struct Scope(Option<(&'static str, Instant)>);

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::args().any(|a| a == "--perf" || a == "--profile-systems"))
}

impl Scope {
    pub fn start(name: &'static str) -> Self {
        Self(enabled().then(|| (name, Instant::now())))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0 {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            SAMPLES.with_borrow_mut(|samples| {
                let row = samples.entry(name).or_default();
                row.0 += 1;
                row.1 += ms;
                row.2 = row.2.max(ms);
            });
        }
    }
}

pub fn take() -> Samples {
    SAMPLES.with_borrow_mut(std::mem::take)
}
