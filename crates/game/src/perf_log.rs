//! Opt-in (`PLAY_LOG_PERF=1`), buffered per-render-frame measurements in `cache/perf/<ms>.csv`
//! (override the directory with `PLAY_PERF_DIR`):
//! wall time, fixed steps, drawn and simulated clip times and positions, hit stops, hits and
//! sounds, plus where a slow frame went: `x_main_ms` (the main schedule, `First` to `Last`),
//! `x_fixed_ms` (the fixed steps), `x_clip_loads`/`x_clip_ms` (clips read from disk on the frame
//! they were first shown), `x_render_max_ms` and `x_render_sets` (the slowest render-world frame
//! since the last row and its time per render set: start to PrepareAssets, to Specialize, to
//! Queue (this one includes waiting for the swapchain image, about 13 ms at 60 Hz with vsync), to
//! PhaseSort, to Prepare, to Render, to Cleanup, to the end) and the pipeline cache's ready and
//! waiting counts. `tools/perf/summary.py` summarises the files.
use bevy::prelude::*;
use std::time::Instant;

/// Per timed name: runs, total milliseconds and the longest run.
pub type SystemSamples =
    std::collections::BTreeMap<std::borrow::Cow<'static, str>, (usize, f64, f64)>;
static SYSTEM_SAMPLES: std::sync::Mutex<SystemSamples> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());
// Timed scopes completed between CSV rows. Render and main schedules overlap, so these
// describe the wall interval, rather than attributing asynchronous render work to a sim tick.
static FRAME_SAMPLES: std::sync::Mutex<SystemSamples> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub struct SystemTimer(Option<(&'static str, Instant)>);

pub(crate) fn args() -> &'static [String] {
    static ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    ARGS.get_or_init(|| std::env::args().collect())
}

pub(crate) fn profile_systems() -> bool {
    sekiro_sim::perf::enabled()
}

impl SystemTimer {
    pub fn start(name: &'static str) -> Self {
        Self(profile_systems().then(|| (name, Instant::now())))
    }
}

impl Drop for SystemTimer {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0 {
            record(name, start.elapsed().as_secs_f64() * 1000.);
        }
    }
}

/// Add one timed run of `name` to the shared store (system timers and, with the
/// `profile-spans` feature, tracing spans).
pub fn record(name: impl Into<std::borrow::Cow<'static, str>>, ms: f64) {
    fn accumulate(samples: &mut SystemSamples, name: std::borrow::Cow<'static, str>, ms: f64) {
        let sample = samples.entry(name).or_default();
        sample.0 += 1;
        sample.1 += ms;
        sample.2 = sample.2.max(ms);
    }
    let name = name.into();
    static PER_FRAME: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *PER_FRAME.get_or_init(enabled)
        && let Ok(mut samples) = FRAME_SAMPLES.lock()
    {
        accumulate(&mut samples, name.clone(), ms);
    }
    if let Ok(mut samples) = SYSTEM_SAMPLES.lock() {
        accumulate(&mut samples, name, ms);
    }
}

/// Remove and return the stored samples whose names do or do not (`matching`) start with
/// `prefix`.
pub fn take_samples(prefix: &str, matching: bool) -> SystemSamples {
    let Ok(mut samples) = SYSTEM_SAMPLES.lock() else {
        return SystemSamples::new();
    };
    samples
        .extract_if(.., |name, _| name.starts_with(prefix) == matching)
        .collect()
}

/// Log and clear the system timers. Span samples (named `span ...`, with the `profile-spans`
/// feature) stay for `span_profile`'s report.
pub fn report_systems() {
    for (name, (count, sum, max)) in take_samples("span ", false) {
        info!(
            "profile system {name}: mean {:.3} ms, max {max:.3} ms",
            sum / count as f64
        );
    }
}

/// Milliseconds since `start`.
fn since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Render-world timings shared with the main world's log: the render schedule's CPU time per
/// render frame, and the pipeline cache's counts (a pipeline that finishes compiling shows as a
/// step in `pipelines_ok`).
#[derive(Default)]
pub struct RenderProbeData {
    start: Option<Instant>,
    /// Stamps at the start of each probed render set, in order.
    stamps: Vec<Instant>,
    /// Per probed set, its time in the slowest render frame since the last read.
    pub slowest_sets: Vec<f64>,
    /// Longest and total render schedule time since the main log last read it.
    pub max_ms: f64,
    pub sum_ms: f64,
    pub frames: usize,
    pub pipelines_ok: usize,
    pub pipelines_waiting: usize,
    /// Queued draw batches in the main opaque, alpha-mask and transparent passes.
    pub draw_batches: usize,
}

#[derive(Resource, Clone, Default)]
pub struct RenderProbe(pub std::sync::Arc<std::sync::Mutex<RenderProbeData>>);

impl RenderProbe {
    /// (max ms, total ms, frames, pipelines ready, pipelines waiting, per-set ms of the slowest
    /// frame joined by `/`) since the last call.
    pub fn take(&self) -> (f64, f64, usize, usize, usize, String) {
        let Ok(mut d) = self.0.lock() else {
            return (0.0, 0.0, 0, 0, 0, String::new());
        };
        let sets = d
            .slowest_sets
            .iter()
            .map(|ms| format!("{ms:.1}"))
            .collect::<Vec<_>>()
            .join("/");
        let out = (
            d.max_ms,
            d.sum_ms,
            d.frames,
            d.pipelines_ok,
            d.pipelines_waiting,
            sets,
        );
        d.slowest_sets.clear();
        d.max_ms = 0.0;
        d.sum_ms = 0.0;
        d.frames = 0;
        out
    }
}

/// Adds the render-world half of [`RenderProbe`].
pub fn install_render_probe(app: &mut App) -> RenderProbe {
    if let Some(probe) = app.world().get_resource::<RenderProbe>() {
        return probe.clone();
    }
    let probe = RenderProbe::default();
    app.insert_resource(probe.clone());
    use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
    use bevy::render::{Render, RenderApp, RenderSystems};
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return probe;
    };
    render.insert_resource(probe.clone());
    if std::env::var_os("PF_RESPAWN_PROBE").is_some() || profile_systems() {
        render.add_systems(
            Render,
            count_draw_batches
                .after(RenderSystems::Prepare)
                .before(RenderSystems::Render),
        );
    }
    render.add_systems(
        Render,
        (|p: Res<RenderProbe>| {
            if let Ok(mut d) = p.0.lock() {
                d.start = Some(Instant::now());
                d.stamps.clear();
            }
        })
        .before(RenderSystems::ExtractCommands),
    );
    let stamp = |p: Res<RenderProbe>| {
        if let Ok(mut d) = p.0.lock() {
            d.stamps.push(Instant::now());
        }
    };
    let mut previous = RenderSystems::ExtractCommands;
    for set in [
        RenderSystems::PrepareAssets,
        RenderSystems::Specialize,
        RenderSystems::Queue,
        RenderSystems::PhaseSort,
        RenderSystems::Prepare,
        RenderSystems::Render,
        RenderSystems::Cleanup,
    ] {
        render.add_systems(Render, stamp.after(previous).before(set.clone()));
        previous = set;
    }
    render.add_systems(
        Render,
        (|p: Res<RenderProbe>, cache: Res<PipelineCache>| {
            let ok = cache
                .pipelines()
                .filter(|c| matches!(c.state, CachedPipelineState::Ok(_)))
                .count();
            let waiting = cache.waiting_pipelines().count();
            if let Ok(mut d) = p.0.lock() {
                if let Some(start) = d.start.take() {
                    let ms = since(start);
                    if ms > d.max_ms {
                        let mut prev = start;
                        let mut sets = Vec::new();
                        for &t in d.stamps.iter().chain(std::iter::once(&Instant::now())) {
                            sets.push(t.duration_since(prev).as_secs_f64() * 1000.0);
                            prev = t;
                        }
                        d.slowest_sets = sets;
                    }
                    d.max_ms = d.max_ms.max(ms);
                    d.sum_ms += ms;
                    d.frames += 1;
                }
                d.pipelines_ok = ok;
                d.pipelines_waiting = waiting;
            }
        })
        .after(RenderSystems::PostCleanup),
    );
    if profile_systems() {
        install_detailed_probe(render);
    }
    probe
}

type ProbeClock = std::sync::Arc<std::sync::Mutex<Instant>>;

fn reset_probe(clock: ProbeClock) -> impl FnMut() {
    move || *clock.lock().unwrap() = Instant::now()
}

fn record_probe(clock: ProbeClock, name: &'static str, reset: bool) -> impl FnMut() {
    move || {
        let mut last = clock.lock().unwrap();
        record(name, last.elapsed().as_secs_f64() * 1000.0);
        if reset {
            *last = Instant::now();
        }
    }
}

/// Ordered boundaries, including submission (not covered by GPU pass diagnostics).
fn install_detailed_probe(render: &mut SubApp) {
    use bevy::render::renderer::{RenderGraph, RenderGraphSystems};
    use bevy::render::{Render, RenderSystems};
    render.add_systems(Render, pipeline_misses.after(RenderSystems::Specialize));
    for (previous, set, next, name) in [
        (
            RenderSystems::ExtractCommands,
            RenderSystems::PrepareAssets,
            RenderSystems::PrepareMeshes,
            "render assets",
        ),
        (
            RenderSystems::CreateViews,
            RenderSystems::Specialize,
            RenderSystems::PrepareViews,
            "render specialize",
        ),
        (
            RenderSystems::Specialize,
            RenderSystems::PrepareViews,
            RenderSystems::Queue,
            "render views/acquire",
        ),
        (
            RenderSystems::PhaseSort,
            RenderSystems::PrepareResources,
            RenderSystems::PrepareResourcesBatchPhases,
            "render resources",
        ),
        (
            RenderSystems::PrepareResources,
            RenderSystems::PrepareResourcesBatchPhases,
            RenderSystems::PrepareResourcesWritePhaseBuffers,
            "render batch phases",
        ),
        (
            RenderSystems::PrepareResourcesFlush,
            RenderSystems::PrepareBindGroups,
            RenderSystems::Render,
            "render bind groups",
        ),
    ] {
        let clock = std::sync::Arc::new(std::sync::Mutex::new(Instant::now()));
        render.add_systems(
            Render,
            reset_probe(clock.clone())
                .after(previous)
                .before(set.clone()),
        );
        render.add_systems(
            Render,
            record_probe(clock, name, false).after(set).before(next),
        );
    }
    let clock = std::sync::Arc::new(std::sync::Mutex::new(Instant::now()));
    render.add_systems(
        RenderGraph,
        reset_probe(clock.clone()).before(RenderGraphSystems::Begin),
    );
    for (previous, next, name) in [
        (
            RenderGraphSystems::Render,
            RenderGraphSystems::Submit,
            "render graph encode",
        ),
        (
            RenderGraphSystems::Submit,
            RenderGraphSystems::Finish,
            "render graph submit",
        ),
    ] {
        render.add_systems(
            RenderGraph,
            record_probe(clock.clone(), name, true)
                .after(previous)
                .before(next),
        );
    }
    render.add_systems(
        Render,
        record_probe(clock, "render finish/present", false)
            .after(bevy::render::renderer::render_system)
            .before(RenderSystems::Cleanup),
    );
    render.add_systems(
        Render,
        render_counts
            .after(RenderSystems::Prepare)
            .before(RenderSystems::Render),
    );
    use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
    let clock = std::sync::Arc::new(std::sync::Mutex::new(Instant::now()));
    render.add_systems(
        Core3d,
        reset_probe(clock.clone())
            .before(Core3dSystems::Prepass)
            .before(bevy::pbr::per_view_shadow_pass::<false>),
    );
    render.add_systems(
        Core3d,
        record_probe(clock, "render camera passes", false).after(Core3dSystems::PostProcess),
    );
}

fn pipeline_misses(
    cache: Res<bevy::render::render_resource::PipelineCache>,
    warmup: Option<Res<crate::pipeline_warmup::PipelineWarmup>>,
    mut seen: Local<usize>,
) {
    let count = cache.pipelines().count();
    if warmup.is_some_and(|w| w.ready()) {
        for (index, pipeline) in cache.pipelines().enumerate().skip(*seen) {
            if let bevy::render::render_resource::PipelineDescriptor::RenderPipelineDescriptor(
                descriptor,
            ) = &pipeline.descriptor
            {
                info!(
                    "traversal pipeline {index}: {:?}, vertex {:?}, fragment {:?}",
                    descriptor.label,
                    descriptor.vertex.shader_defs,
                    descriptor.fragment.as_ref().map(|f| &f.shader_defs)
                );
            }
        }
    }
    *seen = count;
}

#[allow(clippy::too_many_arguments)] // Independent render-world counters.
fn render_counts(
    probe: Res<RenderProbe>,
    views: Query<(), With<bevy::render::view::ExtractedView>>,
    shadows: Res<bevy::render::render_phase::ViewBinnedRenderPhases<bevy::pbr::Shadow>>,
    meshes: Res<bevy::render::render_asset::RenderAssets<bevy::render::mesh::RenderMesh>>,
    materials: Res<
        bevy::render::erased_render_asset::ErasedRenderAssets<bevy::pbr::PreparedMaterial>,
    >,
    casters: Query<&bevy::render::view::RenderShadowMapVisibleEntities>,
    instances: Res<bevy::pbr::RenderMeshInstances>,
    mut last: Local<Option<Instant>>,
) {
    if last.is_some_and(|t| t.elapsed().as_secs_f32() < 1.0) {
        return;
    }
    *last = Some(Instant::now());
    let batches: Vec<_> = shadows
        .0
        .values()
        .map(|p| {
            p.multidrawable_meshes.len()
                + p.batchable_meshes.len()
                + p.unbatchable_meshes
                    .values()
                    .map(|b| b.entities.len())
                    .sum::<usize>()
        })
        .collect();
    let mut caster_counts: Vec<_> = casters
        .iter()
        .flat_map(|light| {
            light.subviews.iter().map(|(view, (_, visible))| {
                let count: usize = visible
                    .classes
                    .values()
                    .map(|class| {
                        class.entities_cpu_culling.len() + class.entities_gpu_culling.len()
                    })
                    .sum();
                (format!("{view:?}"), count)
            })
        })
        .collect();
    caster_counts.sort_unstable();
    let instances = match &*instances {
        bevy::pbr::RenderMeshInstances::CpuBuilding(instances) => instances.len(),
        bevy::pbr::RenderMeshInstances::GpuBuilding(instances) => instances.len(),
    };
    let source_materials = materials
        .iter()
        .filter(|(id, _)| id.type_id() == std::any::TypeId::of::<StandardMaterial>())
        .count();
    info!(
        "render counts: views {}, shadow views {}, shadow batches {:?}, meshes {}, materials {}, main batches {}",
        views.iter().count(),
        batches.len(),
        batches,
        meshes.iter().count(),
        materials.iter().count(),
        probe.0.lock().map_or(0, |p| p.draw_batches)
    );
    info!("render shadow casters: {caster_counts:?}");
    info!("render instances: {instances}, prepared source materials: {source_materials}");
}

fn count_draw_batches(
    probe: Res<RenderProbe>,
    opaque: Res<
        bevy::render::render_phase::ViewBinnedRenderPhases<bevy::core_pipeline::core_3d::Opaque3d>,
    >,
    masked: Res<
        bevy::render::render_phase::ViewBinnedRenderPhases<
            bevy::core_pipeline::core_3d::AlphaMask3d,
        >,
    >,
    transparent: Res<
        bevy::render::render_phase::ViewSortedRenderPhases<
            bevy::core_pipeline::core_3d::Transparent3d,
        >,
    >,
) {
    fn binned<P: bevy::render::render_phase::BinnedPhaseItem>(
        phases: &bevy::render::render_phase::ViewBinnedRenderPhases<P>,
    ) -> usize {
        phases
            .0
            .values()
            .map(|p| {
                p.multidrawable_meshes.len()
                    + p.batchable_meshes.len()
                    + p.unbatchable_meshes
                        .values()
                        .map(|b| b.entities.len())
                        .sum::<usize>()
                    + p.non_mesh_items
                        .values()
                        .map(|b| b.entities.len())
                        .sum::<usize>()
            })
            .sum()
    }
    if let Ok(mut data) = probe.0.lock() {
        data.draw_batches = binned(&opaque)
            + binned(&masked)
            + transparent
                .0
                .values()
                .map(|p| {
                    p.items
                        .iter()
                        .filter(|(_, i)| !i.batch_range.is_empty())
                        .count()
                })
                .sum::<usize>();
    }
}

pub fn enabled() -> bool {
    args().iter().any(|a| a == "--perf") || std::env::var("PLAY_LOG_PERF").is_ok_and(|v| v == "1")
}

