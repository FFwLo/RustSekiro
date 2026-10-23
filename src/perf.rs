//! Performance check (user, 2026-10-13: "check game performance and optimize the game").
//!
//! - F3: an overlay with the frame rate, the frame time (average / 99th percentile / worst over
//!   the last 2 s), the main world's time per schedule phase and the entity count. "render wait"
//!   is what is left of the frame after the main world's schedules: Last, and the wait for the
//!   render thread when the frame is GPU / render bound.
//! - `SHINOBI_BENCH=<seconds>` (empty = 20): after 5 s of warm-up (`SHINOBI_BENCH_WARMUP=<s>`), records that many seconds,
//!   writes `perf_report.txt` next to `config.toml` (`paths::root`), logs it and quits. Run it
//!   the same way before and after a change to compare.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bevy::app::{MainScheduleOrder, RunFixedMainLoop};
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// Marker schedules between Bevy's main ones: each only stamps the time.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AfterPreUpdate;
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AfterFixed;
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AfterUpdate;
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AfterPostUpdate;

/// The phases of one frame, in order; the last is the rest of the frame.
const PHASES: [&str; 5] = ["first + pre-update", "fixed update", "update", "post-update", "render wait + last"];
const WINDOW: Duration = Duration::from_secs(2);
const WARM_UP: Duration = Duration::from_secs(5);

/// The warm-up before a bench records: `SHINOBI_BENCH_WARMUP=<seconds>` (a big map's load),
/// else [`WARM_UP`].
fn warm_up() -> Duration {
    std::env::var("SHINOBI_BENCH_WARMUP").ok().and_then(|v| v.trim().parse().ok()).map_or(WARM_UP, Duration::from_secs_f32)
}

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    /// Milliseconds: the whole frame, then each phase.
    frame: f32,
    phases: [f32; 5],
}

struct Bench {
    secs: f32,
    start: Option<Instant>,
    samples: Vec<Sample>,
    entities: Vec<usize>,
}

#[derive(Resource)]
struct Perf {
    launched: Instant,
    first_frame: Option<Instant>,
    frame_start: Option<Instant>,
    marks: [Option<Instant>; 4],
    recent: VecDeque<Sample>,
    bench: Option<Bench>,
    shown: bool,
    refreshed: Option<Instant>,
}

#[derive(Component)]
struct PerfText;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        let bench = std::env::var("SHINOBI_BENCH").ok().map(|v| Bench { secs: v.trim().parse().unwrap_or(20.0), start: None, samples: Vec::new(), entities: Vec::new() });
        engine_spans(app);
        app.insert_resource(Perf { launched: Instant::now(), first_frame: None, frame_start: None, marks: [None; 4], recent: VecDeque::new(), bench, shown: false, refreshed: None });
        for (label, i) in [(AfterPreUpdate.intern(), 0usize), (AfterFixed.intern(), 1), (AfterUpdate.intern(), 2), (AfterPostUpdate.intern(), 3)] {
            app.init_schedule(label);
            app.add_systems(label, move |mut perf: ResMut<Perf>| perf.marks[i] = Some(Instant::now()));
        }
        let mut order = app.world_mut().resource_mut::<MainScheduleOrder>();
        order.insert_after(PreUpdate, AfterPreUpdate);
        order.insert_after(RunFixedMainLoop, AfterFixed);
        order.insert_after(Update, AfterUpdate);
        order.insert_after(PostUpdate, AfterPostUpdate);
        app.add_systems(Startup, spawn_overlay).add_systems(First, frame_begin).add_systems(Update, (toggle, refresh_overlay, run_bench));
    }
}

/// Start of a frame: the previous frame's sample from its marks.
fn frame_begin(mut perf: ResMut<Perf>) {
    let now = Instant::now();
    perf.first_frame.get_or_insert(now);
    if let Some(start) = perf.frame_start {
        let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f32() * 1000.0;
        let mut phases = [0.0; 5];
        let mut prev = start;
        for (i, m) in perf.marks.iter().enumerate() {
            if let Some(m) = *m {
                phases[i] = ms(prev, m);
                prev = m;
            }
        }
        phases[4] = ms(prev, now);
        let s = Sample { at: now, frame: ms(start, now), phases };
        perf.recent.push_back(s);
        while perf.recent.front().is_some_and(|f| now.duration_since(f.at) > WINDOW) {
            perf.recent.pop_front();
        }
        if let Some(b) = perf.bench.as_mut().filter(|b| b.start.is_some()) {
            b.samples.push(s);
        }
    }
    perf.frame_start = Some(now);
    perf.marks = [None; 4];
}

fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        PerfText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() },
        TextColor(Color::srgb(0.9, 1.0, 0.7)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Node { position_type: PositionType::Absolute, right: Val::Px(6.0), top: Val::Px(6.0), padding: UiRect::all(Val::Px(4.0)), ..default() },
        GlobalZIndex(100),
        Visibility::Hidden,
    ));
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, mut perf: ResMut<Perf>, mut text: Query<&mut Visibility, With<PerfText>>) {
    if keys.just_pressed(KeyCode::F3) {
        perf.shown = !perf.shown;
        for mut v in &mut text {
            *v = if perf.shown { Visibility::Visible } else { Visibility::Hidden };
        }
    }
}

/// Frame-time statistics: (average ms, fps, 99th percentile ms, worst ms, 1 % low fps).
fn stats(frames: &[f32]) -> (f32, f32, f32, f32, f32) {
    if frames.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, 0.0);
    }
    let mut sorted = frames.to_vec();
    sorted.sort_by(f32::total_cmp);
    let n = sorted.len();
    let avg = sorted.iter().sum::<f32>() / n as f32;
    let p99 = sorted[((n as f32 * 0.99) as usize).min(n - 1)];
    let worst = sorted[n - 1];
    let tail = &sorted[n - (n / 100).max(1)..];
    let low = 1000.0 / (tail.iter().sum::<f32>() / tail.len() as f32);
    (avg, 1000.0 / avg.max(1e-3), p99, worst, low)
}

fn phase_avgs(samples: &[Sample]) -> [f32; 5] {
    let mut out = [0.0; 5];
    for s in samples {
        for (o, p) in out.iter_mut().zip(s.phases) {
            *o += p;
        }
    }
    out.map(|v| v / samples.len().max(1) as f32)
}

fn refresh_overlay(mut perf: ResMut<Perf>, mut text: Query<&mut Text, With<PerfText>>, all: Query<()>) {
    let now = Instant::now();
    if !perf.shown || perf.refreshed.is_some_and(|r| now.duration_since(r) < Duration::from_millis(250)) {
        return;
    }
    perf.refreshed = Some(now);
    let samples: Vec<Sample> = perf.recent.iter().copied().collect();
    let frames: Vec<f32> = samples.iter().map(|s| s.frame).collect();
    let (avg, fps, p99, worst, low) = stats(&frames);
    let mut s = format!("{fps:.0} fps  (1% low {low:.0})\nframe {avg:.1} ms  p99 {p99:.1}  worst {worst:.1}\n");
    for (name, ms) in PHASES.iter().zip(phase_avgs(&samples)) {
        s += &format!("{name}: {ms:.2} ms\n");
    }
    s += &format!("entities: {}", all.iter().count());
    for mut t in &mut text {
        t.0 = s.clone();
    }
}

fn run_bench(mut perf: ResMut<Perf>, all: Query<()>, mut exit: MessageWriter<AppExit>) {
    let now = Instant::now();
    let (launched, first) = (perf.launched, perf.first_frame);
    let Some(b) = perf.bench.as_mut() else { return };
    let Some(first) = first else { return };
    let start = match b.start {
        Some(s) => s,
        None if now.duration_since(first) >= warm_up() => {
            info!("bench: recording {} s", b.secs);
            b.start = Some(now);
            return;
        }
        None => return,
    };
    if b.samples.len() % 60 == 0 {
        b.entities.push(all.iter().count());
    }
    if now.duration_since(start).as_secs_f32() < b.secs {
        return;
    }
    let frames: Vec<f32> = b.samples.iter().map(|s| s.frame).collect();
    let (avg, fps, p99, worst, low) = stats(&frames);
    let mut r = String::from("Sv1 performance report (src/perf.rs, SHINOBI_BENCH)\n");
    r += &format!("launch to first frame: {:.1} s\n", first.duration_since(launched).as_secs_f32());
    r += &format!("recorded: {} frames over {:.1} s after {} s warm-up\n", frames.len(), b.secs, warm_up().as_secs_f32());
    r += &format!("fps: {fps:.1} average, {low:.1} 1% low\n");
    r += &format!("frame ms: {avg:.2} average, {p99:.2} p99, {worst:.2} worst\n");
    let mut sorted = frames.clone();
    sorted.sort_by(f32::total_cmp);
    if !sorted.is_empty() {
        r += &format!("frame ms p50 {:.2}, p95 {:.2}\n", sorted[sorted.len() / 2], sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)]);
    }
    r += "main world, average ms per frame:\n";
    for (name, ms) in PHASES.iter().zip(phase_avgs(&b.samples)) {
        r += &format!("  {name}: {ms:.2}\n");
    }
    let ents = &b.entities;
    r += &format!("entities: {} average\n", ents.iter().sum::<usize>() / ents.len().max(1));
    let path = crate::paths::root().join("perf_report.txt");
    let _ = std::fs::write(&path, &r);
    info!("{r}wrote {}", path.display());
    exit.write(AppExit::Success);
}

/// `SHINOBI_SYS_PROF=1`: the wall time between a stamp before and one after each of Bevy's own
/// PostUpdate passes (transform propagation, UI layout, visibility), as spans; other systems may
/// run in between on other threads.
fn engine_spans(app: &mut App) {
    if std::env::var_os("SHINOBI_SYS_PROF").is_none() {
        return;
    }
    use bevy::camera::visibility::VisibilitySystems;
    use bevy::transform::TransformSystems;
    use bevy::ui::UiSystems;
    #[derive(Resource, Default)]
    struct Marks(std::collections::HashMap<&'static str, Instant>);
    fn begin(name: &'static str) -> impl FnMut(ResMut<Marks>) {
        move |mut m: ResMut<Marks>| {
            m.0.insert(name, Instant::now());
        }
    }
    fn end(name: &'static str) -> impl FnMut(Res<Marks>) {
        move |m: Res<Marks>| {
            if let Some(&t) = m.0.get(name) {
                record(name, t);
            }
        }
    }
    app.init_resource::<Marks>().add_systems(
        PostUpdate,
        (
            begin("propagate").before(TransformSystems::Propagate),
            end("propagate").after(TransformSystems::Propagate),
            begin("ui_layout").before(UiSystems::Layout),
            end("ui_layout").after(UiSystems::Layout),
            begin("visibility").before(VisibilitySystems::CheckVisibility),
            end("visibility").after(VisibilitySystems::CheckVisibility),
        ),
    );
}

/// `SHINOBI_SYS_PROF=1`: the time spent in the systems that open a [`span`] (ms per second,
/// calls per second), logged every 5 s. Nothing when unset.
pub fn span(name: &'static str) -> Option<Span> {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    ON.get_or_init(|| std::env::var_os("SHINOBI_SYS_PROF").is_some()).then(|| Span(name, Instant::now()))
}

pub struct Span(&'static str, Instant);

impl Drop for Span {
    fn drop(&mut self) {
        record(self.0, self.1);
    }
}

/// Adds the time since `start` to `name`'s total, and logs the totals every 5 s.
fn record(name: &'static str, start: Instant) {
    {
        static SPANS: std::sync::Mutex<Option<(Instant, std::collections::HashMap<&'static str, (f64, u32)>)>> = std::sync::Mutex::new(None);
        let Ok(mut g) = SPANS.lock() else { return };
        let (since, m) = g.get_or_insert_with(|| (Instant::now(), Default::default()));
        let e = m.entry(name).or_default();
        e.0 += start.elapsed().as_secs_f64() * 1e3;
        e.1 += 1;
        let secs = since.elapsed().as_secs_f64();
        if secs >= 5.0 {
            let mut v: Vec<_> = m.drain().collect();
            v.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
            let line: Vec<String> = v.iter().map(|(n, (ms, c))| format!("{n} {:.1} ms/s ({:.0}/s)", ms / secs, *c as f64 / secs)).collect();
            info!("sys prof: {}", line.join(", "));
            *since = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stats;

    #[test]
    fn stats_read_the_frame_times() {
        let mut frames = vec![10.0f32; 99];
        frames.push(100.0);
        let (avg, fps, p99, worst, low) = stats(&frames);
        assert!((avg - 10.9).abs() < 1e-3, "{avg}");
        assert!((fps - 1000.0 / 10.9).abs() < 0.01);
        assert_eq!((p99, worst), (100.0, 100.0));
        assert!((low - 10.0).abs() < 1e-3, "1% low = the worst frame: {low}");
    }
}
