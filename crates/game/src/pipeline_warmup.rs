//! Specialize resident map layouts during loading, including layouts only used by other LODs.
use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use std::time::{Duration, Instant};

use bevy::material::{
    MaterialProperties,
    key::{ErasedMaterialPipelineKey, ErasedMeshPipelineKey},
    specialize::SpecializedMeshPipelineError,
};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    LightKeyCache, MeshPipelineKey, PreparedMaterial, RenderMaterialInstances, RenderMeshInstances,
    ViewKeyCache, ViewKeyPrepassCache,
};
use bevy::prelude::*;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{CachedPipelineState, CachedRenderPipelineId, PipelineCache};
use bevy::render::{Render, RenderApp, RenderSystems};

#[path = "warm_entities.rs"]
mod entities;

#[derive(Resource, Clone, Default)]
pub(crate) struct PipelineWarmup {
    loaded: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    /// How far through, from 0 to 1, as the bits of an `f32`.
    progress: Arc<AtomicU32>,
}

impl PipelineWarmup {
    pub(crate) fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// How far through the warmup, from 0 to 1, for the loading bar: a fifth for queueing the
    /// layout variants, a fifth for the resident entities and the rest for the compiles.
    pub(crate) fn progress(&self) -> f32 {
        if self.ready() {
            1.0
        } else {
            f32::from_bits(self.progress.load(Ordering::Acquire))
        }
    }
}

pub(crate) fn install<M: Material>(app: &mut App, args: &[String]) {
    // Fixed captures wait for each pose's exact pipelines themselves, including
    // diagnostic passes that never occur in traversal. Avoid compiling unused variants.
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--sweep" | "--render-gate" | "--reference-gate"))
    {
        return;
    }
    let warmup = PipelineWarmup::default();
    app.insert_resource(warmup.clone()).add_systems(
        First,
        |pending: Res<crate::map::Pending>,
         batches: Res<crate::map::StaticBatches>,
         warmup: Res<PipelineWarmup>| {
            warmup
                .loaded
                .store(pending.loaded() && batches.ready(), Ordering::Release);
        },
    );
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        entities::install::<M>(render);
        render.insert_resource(warmup).add_systems(
            Render,
            warm_pipelines::<M>
                .after(RenderSystems::Specialize)
                .before(RenderSystems::PrepareViews),
        );
    }
}

struct Work {
    key: ErasedMaterialPipelineKey,
    layout: MeshVertexBufferLayoutRef,
    properties: Arc<MaterialProperties>,
    prepass: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Color,
    Prepass,
    Shadow,
}

#[derive(Default)]
struct WarmCache {
    started: Option<Instant>,
    seen: HashSet<(bool, ErasedMaterialPipelineKey, MeshVertexBufferLayoutRef)>,
    work: VecDeque<Work>,
    /// Variants found when the warmup started.
    variants: usize,
    queued: usize,
    /// Queued compiles still compiling, so the bar counts only the warmup's own.
    pending: Vec<CachedRenderPipelineId>,
    entities_started: Option<Instant>,
    gpu_timed_out: bool,
    entities_ready: bool,
}

impl WarmCache {
    fn push(&mut self, work: Work) {
        if self
            .seen
            .insert((work.prepass, work.key.clone(), work.layout.clone()))
        {
            self.work.push_back(work);
        }
    }
}

// Hundreds of asynchronous GPU compiles take much longer than 300 fast loading frames.
// Entity specialization has its own limit so compilation does not consume its budget.
const GPU_COMPILE_LIMIT: Duration = Duration::from_secs(90);
const ENTITY_WARMUP_LIMIT: Duration = Duration::from_secs(30);

fn warm_pipelines<M: Material>(world: &mut World, mut state: Local<WarmCache>) {
    let warmup = world.resource::<PipelineWarmup>().clone();
    if warmup.ready() || (state.started.is_none() && !warmup.loaded.load(Ordering::Acquire)) {
        return;
    }
    let _timer = crate::perf_log::SystemTimer::start("pipeline warmup");
    if state.started.is_none() {
        state.started = Some(Instant::now());
        let layouts: HashSet<_> = world
            .resource::<RenderAssets<RenderMesh>>()
            .iter()
            .filter(|(_, mesh)| {
                mesh.layout.0.contains(Mesh::ATTRIBUTE_NORMAL)
                    && mesh.layout.0.contains(Mesh::ATTRIBUTE_UV_0)
            })
            .filter(|(_, mesh)| !mesh.layout.0.contains(Mesh::ATTRIBUTE_JOINT_INDEX))
            .map(|(_, mesh)| (mesh.layout.clone(), mesh.key_bits.bits()))
            .collect();
        let mut by_type: HashMap<TypeId, HashSet<_>> =
            HashMap::from([(TypeId::of::<M>(), layouts)]);
        for (entity, material) in &world.resource::<RenderMaterialInstances>().instances {
            let Some(mesh) = world
                .resource::<RenderMeshInstances>()
                .mesh_asset_id(*entity)
                .and_then(|id| world.resource::<RenderAssets<RenderMesh>>().get(id))
            else {
                continue;
            };
            by_type
                .entry(material.asset_id.type_id())
                .or_default()
                .insert((mesh.layout.clone(), mesh.key_bits.bits()));
        }
        let mut keys = HashSet::new();
        let materials: Vec<_> = world
            .resource::<ErasedRenderAssets<PreparedMaterial>>()
            .iter()
            .filter_map(|(id, material)| {
                let p = &material.properties;
                keys.insert((
                    id.type_id(),
                    p.material_key.clone(),
                    p.mesh_pipeline_key_bits,
                    alpha_key(p.alpha_mode),
                ))
                .then(|| (id.type_id(), p.clone()))
            })
            .collect();
        // Cameras can start without EnvironmentMapLight until their first probe is ready.
        // Probe selection then retains the component at zero intensity between volumes.
        // Warm both initial camera and resident-probe view keys.
        let color: Vec<_> = world
            .resource::<ViewKeyCache>()
            .values()
            .copied()
            .flat_map(|key| {
                [
                    key | MeshPipelineKey::ENVIRONMENT_MAP,
                    key & !MeshPipelineKey::ENVIRONMENT_MAP,
                ]
            })
            .collect();
        let prepass: Vec<_> = world
            .resource::<ViewKeyPrepassCache>()
            .values()
            .copied()
            .collect();
        let shadow: Vec<_> = world
            .resource::<LightKeyCache>()
            .values()
            .copied()
            .collect();
        for (type_id, properties) in materials {
            let Some(layouts) = by_type.get(&type_id) else {
                continue;
            };
            let opaque = matches!(
                properties.render_phase_type,
                bevy::material::RenderPhaseType::Opaque
                    | bevy::material::RenderPhaseType::AlphaMask
            );
            for (layout, bits) in layouts {
                let mesh = MeshPipelineKey::from_bits_retain(*bits);
                for (pass, views) in [
                    (Pass::Color, &color),
                    (Pass::Prepass, &prepass),
                    (Pass::Shadow, &shadow),
                ] {
                    if pass == Pass::Prepass && !opaque {
                        continue;
                    }
                    if pass == Pass::Prepass && !properties.prepass_enabled
                        || pass == Pass::Shadow && !properties.shadows_enabled
                    {
                        continue;
                    }
                    for &view in views {
                        let key = pass_key(pass, view, mesh, layout, &properties);
                        state.push(Work {
                            key: ErasedMaterialPipelineKey {
                                type_id,
                                mesh_key: ErasedMeshPipelineKey::new(key),
                                material_key: properties.material_key.clone(),
                            },
                            layout: layout.clone(),
                            properties: properties.clone(),
                            prepass: pass != Pass::Color,
                        });
                    }
                }
            }
        }
        state.variants = state.work.len();
    }
    // Keep compilation requests bounded even on loading frames.
    for _ in 0..16 {
        let Some(work) = state.work.pop_front() else {
            break;
        };
        let specialized = specialize(
            world,
            work.prepass,
            work.key,
            &work.layout,
            &work.properties,
        );
        // Incompatible layout/material combinations are not renderable and need no pipeline.
        if let Some(Ok(id)) = specialized {
            state.queued += 1;
            state.pending.push(id);
        }
    }
    let cache = world.resource::<PipelineCache>();
    let compiling = cache.waiting_pipelines().next().is_some();
    state.pending.retain(|&id| {
        matches!(
            cache.get_render_pipeline_state(id),
            CachedPipelineState::Queued | CachedPipelineState::Creating(_)
        )
    });
    if !state.gpu_timed_out
        && (compiling || !state.work.is_empty())
        && state.started.unwrap().elapsed() >= GPU_COMPILE_LIMIT
    {
        warn!("map GPU pipeline warmup deadline reached after {GPU_COMPILE_LIMIT:?}");
        state.gpu_timed_out = true;
        state.work.clear();
    }
    // Leave CPU time for asynchronous shader compilation, then use the full loading budget.
    if state.work.is_empty() && !state.entities_ready {
        let started = *state.entities_started.get_or_insert_with(Instant::now);
        if started.elapsed() >= ENTITY_WARMUP_LIMIT {
            warn!("map entity warmup deadline reached after {ENTITY_WARMUP_LIMIT:?}");
            entities::cancel(world);
            state.entities_ready = true;
        } else {
            let budget = Duration::from_millis(if compiling && !state.gpu_timed_out {
                12
            } else {
                50
            });
            state.entities_ready = entities::warm::<M>(world, budget);
        }
    }
    let progress = stage_progress(
        state.variants,
        state.work.len(),
        if state.entities_ready {
            1.0
        } else {
            entities::progress(world)
        },
        state.queued,
        state.pending.len(),
    );
    warmup.progress.store(progress.to_bits(), Ordering::Release);
    if state.work.is_empty() && state.entities_ready && (!compiling || state.gpu_timed_out) {
        info!(
            "map pipeline warmup complete: {} compatible layout/pass variants",
            state.queued
        );
        warmup.ready.store(true, Ordering::Release);
        state.seen.clear();
    }
}

/// The warmup's progress from 0 to 1: a fifth for queueing the `variants` layout variants
/// (`left` still to queue), a fifth for the resident `entities` fraction and three fifths for
/// the `queued` compiles, of which `waiting` are still compiling. Compiles count only once every
/// variant is queued: before that, the few queued so far finishing would read as done, and the
/// bar never goes back to correct it.
fn stage_progress(
    variants: usize,
    left: usize,
    entities: f32,
    queued: usize,
    waiting: usize,
) -> f32 {
    use crate::play::fraction;
    let queueing = fraction(variants.saturating_sub(left), variants);
    let compiled = if left == 0 {
        fraction(queued.saturating_sub(waiting), queued)
    } else {
        0.0
    };
    0.2 * queueing + 0.2 * entities + 0.6 * compiled
}

/// Both resident warmup paths select the same Bevy material specializer.
fn specialize(
    world: &mut World,
    prepass: bool,
    key: ErasedMaterialPipelineKey,
    layout: &MeshVertexBufferLayoutRef,
    properties: &Arc<MaterialProperties>,
) -> Option<Result<CachedRenderPipelineId, SpecializedMeshPipelineError>> {
    if prepass {
        properties
            .prepass_specialize
            .map(|specialize| specialize(world, &key, layout, properties))
    } else {
        properties
            .base_specialize
            .map(|specialize| specialize(world, key, layout, properties))
    }
}

/// Preserve mask thresholds without allocating formatted material keys.
fn alpha_key(alpha: AlphaMode) -> (std::mem::Discriminant<AlphaMode>, u32) {
    (
        std::mem::discriminant(&alpha),
        match alpha {
            AlphaMode::Mask(value) => value.to_bits(),
            _ => 0,
        },
    )
}

/// Match Bevy's color, prepass and shadow key rules for both warmup paths.
fn pass_key(
    pass: Pass,
    view: MeshPipelineKey,
    mesh: MeshPipelineKey,
    layout: &MeshVertexBufferLayoutRef,
    properties: &MaterialProperties,
) -> MeshPipelineKey {
    let mut key = view | mesh;
    if pass == Pass::Color {
        key |= properties
            .mesh_pipeline_key_bits
            .downcast::<MeshPipelineKey>();
    }
    if pass != Pass::Shadow {
        key |= bevy::pbr::alpha_mode_pipeline_key(
            properties.alpha_mode,
            &Msaa::from_samples(view.msaa_samples()),
        );
    } else if !matches!(
        properties.alpha_mode,
        AlphaMode::Opaque | AlphaMode::Multiply
    ) {
        key |= MeshPipelineKey::MAY_DISCARD;
    }
    if pass != Pass::Color
        && (properties
            .get_shader(bevy::pbr::PrepassVertexShader)
            .is_some()
            || properties
                .get_shader(bevy::pbr::PrepassFragmentShader)
                .is_some())
    {
        key |= MeshPipelineKey::PREPASS_READS_MATERIAL;
    }
    if pass != Pass::Shadow
        && view.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS)
        && layout.0.contains(Mesh::ATTRIBUTE_JOINT_INDEX)
    {
        key |= MeshPipelineKey::HAS_PREVIOUS_SKIN;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::stage_progress;

    #[test]
    fn warmup_progress_rises_through_a_load() {
        // 40 variants queued 16 a frame, compiles finishing as they go, then the entities.
        // (left to queue, entities, queued, still compiling)
        let frames = [
            (40, 0.0, 0, 0),
            (24, 0.0, 16, 0),
            (8, 0.0, 30, 12),
            (0, 0.0, 38, 30),
            (0, 0.3, 38, 12),
            (0, 0.8, 38, 0),
            (0, 1.0, 38, 0),
        ];
        let bars: Vec<f32> = frames
            .iter()
            .map(|&(left, entities, queued, waiting)| {
                stage_progress(40, left, entities, queued, waiting)
            })
            .collect();
        assert_eq!(bars[0], 0.0, "{bars:?}");
        assert!(bars.windows(2).all(|w| w[1] >= w[0]), "{bars:?}");
        assert!((bars[bars.len() - 1] - 1.0).abs() < 1e-6, "{bars:?}");
        // Every compile queued so far has finished, but variants are still queueing.
        assert!((bars[1] - 0.2 * 16.0 / 40.0).abs() < 1e-6, "{bars:?}");
    }
}
