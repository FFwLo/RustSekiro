//! Opt-in system profiler: build with `--features profile-spans` and every Bevy system and
//! schedule run is timed through its tracing span, in perf_log's shared timing store. Every five seconds the slowest spans are
//! logged as `span profile` lines (mean milliseconds per rendered frame), main and render world
//! together. Uses only crates the app already depends on, so it builds offline.
use bevy::log::BoxedLayer;
use bevy::log::tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};
use bevy::prelude::*;
use std::time::Instant;
use tracing::span::{Attributes, Id};
use tracing::{Subscriber, field::Field, field::Visit};

struct Label {
    name: String,
    entered: Option<Instant>,
}

struct NameVisitor(Option<String>);

impl Visit for NameVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "name" {
            self.0 = Some(value.to_owned());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

struct SpanTimes;

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SpanTimes {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = NameVisitor(None);
        attrs.record(&mut visitor);
        let base = attrs.metadata().name();
        let name = match visitor.0 {
            Some(field) => format!("span {base} {field}"),
            None => format!("span {base}"),
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Label {
                name,
                entered: None,
            });
        }
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(label) = span.extensions_mut().get_mut::<Label>()
        {
            label.entered = Some(Instant::now());
        }
    }
    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        let Some(label) = extensions.get_mut::<Label>() else {
            return;
        };
        let Some(start) = label.entered.take() else {
            return;
        };
        crate::perf_log::record(label.name.clone(), start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// `LogPlugin::custom_layer` hook.
pub fn layer(_: &mut App) -> Option<BoxedLayer> {
    Some(Box::new(SpanTimes))
}

/// Logs the slowest spans every five seconds, as mean ms per main-world frame.
pub fn report(time: Res<Time<Real>>, mut frames: Local<u64>, mut last: Local<f32>) {
    *frames += 1;
    let now = time.elapsed_secs();
    if now - *last < 5.0 {
        return;
    }
    *last = now;
    let mut rows: Vec<_> = crate::perf_log::take_samples("span ", true)
        .into_iter()
        .collect();
    rows.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
    let n = (*frames).max(1) as f64;
    for (name, (count, ms, _)) in rows.iter().take(40) {
        info!(
            "span profile: {:8.3} ms/frame {:6.1} runs/frame {name}",
            ms / n,
            *count as f64 / n
        );
    }
    // Single runs long enough to stall a frame.
    rows.sort_by(|a, b| b.1.2.total_cmp(&a.1.2));
    for (name, (_, _, max)) in rows.iter().take(25).filter(|r| r.1.2 >= 50.0) {
        info!("span stall: {max:9.1} ms longest run {name}");
    }
    *frames = 0;
}
