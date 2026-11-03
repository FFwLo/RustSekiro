//! Resolve presentation before Bevy configures the surface, and measure ready frames.
use std::collections::HashMap;
use std::time::Instant;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::renderer::{RenderAdapter, RenderInstance};
use bevy::window::{PresentMode, RawHandleWrapper};

pub(crate) fn requested_mode() -> PresentMode {
    mode(
        std::env::args().any(|a| a == "--uncapped"),
        PresentMode::default(),
    )
}

pub(crate) fn mode(uncapped: bool, default_mode: PresentMode) -> PresentMode {
    match crate::arg_value(&std::env::args().collect::<Vec<_>>(), "--present-mode").as_deref() {
        Some("mailbox") => PresentMode::Mailbox,
        Some("immediate") => PresentMode::Immediate,
        Some("fifo") => PresentMode::Fifo,
        _ if uncapped => PresentMode::AutoNoVsync,
        _ => default_mode,
    }
}

pub(crate) struct PresentationPlugin;

#[derive(Resource)]
struct SurfaceInstance(RenderInstance);

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        if std::env::args().any(|a| a == "--uncapped") {
            app.insert_resource(bevy::winit::WinitSettings::continuous());
        }
        app.add_systems(Last, (configure_mode, frame_times));
    }

    fn finish(&self, app: &mut App) {
        let instance = app
            .sub_app(RenderApp)
            .world()
            .resource::<RenderInstance>()
            .clone();
        app.insert_resource(SurfaceInstance(instance));
    }
}

fn resolve(requested: PresentMode, supported: &[wgpu::PresentMode]) -> PresentMode {
    use PresentMode as B;
    use wgpu::PresentMode as W;
    let modes: &[(B, W)] = match requested {
        B::AutoNoVsync => &[
            (B::Immediate, W::Immediate),
            (B::Mailbox, W::Mailbox),
            (B::Fifo, W::Fifo),
        ],
        B::AutoVsync => &[(B::FifoRelaxed, W::FifoRelaxed), (B::Fifo, W::Fifo)],
        B::Mailbox => &[
            (B::Mailbox, W::Mailbox),
            (B::Immediate, W::Immediate),
            (B::Fifo, W::Fifo),
        ],
        B::Immediate => &[(B::Immediate, W::Immediate), (B::Fifo, W::Fifo)],
        B::FifoRelaxed => &[(B::FifoRelaxed, W::FifoRelaxed), (B::Fifo, W::Fifo)],
        B::Fifo => &[(B::Fifo, W::Fifo)],
    };
    modes
        .iter()
        .find(|(_, mode)| supported.contains(mode))
        .expect("surface must support FIFO")
        .0
}

fn configure_mode(
    mut windows: Query<(Entity, &mut Window, &RawHandleWrapper)>,
    instance: Res<SurfaceInstance>,
    adapter: Res<RenderAdapter>,
    mut configured: Local<HashMap<Entity, PresentMode>>,
    _main_thread: NonSendMarker,
) {
    for (entity, mut window, handle) in &mut windows {
        if configured.get(&entity) == Some(&window.present_mode) {
            continue;
        }
        // SAFETY: The handle retains its native window and this system runs on the main thread.
        let surface = instance
            .0
            .create_surface(unsafe { handle.get_handle() })
            .expect("window surface capabilities");
        let caps = surface.get_capabilities(&adapter);
        let requested = window.present_mode;
        let selected = resolve(requested, &caps.present_modes);
        // A supported concrete mode goes to Bevy's create_surfaces unchanged, avoiding
        // an unobservable Auto mode fallback inside the renderer's private surface data.
        window.present_mode = selected;
        info!(
            "presentation: backend {:?}, requested {:?}, configured {:?}, supported {:?}",
            adapter.get_info().backend,
            requested,
            selected,
            caps.present_modes
        );
        if requested == PresentMode::AutoNoVsync && selected == PresentMode::Fifo {
            warn!(
                "uncapped presentation unavailable: the surface supports only synchronized modes"
            );
        }
        configured.insert(entity, selected);
    }
}

fn frame_times(
    pipelines: Option<Res<crate::map::ready::Pipelines>>,
    mut last: Local<Option<Instant>>,
    mut samples: Local<Vec<f64>>,
) {
    let now = Instant::now();
    let previous = last.replace(now);
    let Some(pipelines) = pipelines else {
        return;
    };
    let status = pipelines.status();
    if status.ready == 0 || status.pending > 0 || status.failed > 0 {
        samples.clear();
        return;
    }
    if let Some(previous) = previous {
        samples.push(now.duration_since(previous).as_secs_f64() * 1000.0);
    }
    if samples.len() == 120 {
        samples.sort_by(f64::total_cmp);
        info!(
            "presentation frame times: 120 pipeline-ready frames, min {:.3} ms, median {:.3} ms ({:.1} fps), p95 {:.3} ms",
            samples[0],
            samples[60],
            1000.0 / samples[60],
            samples[113]
        );
        samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncapped_selects_supported_concrete_mode() {
        use wgpu::PresentMode as W;
        for (supported, expected) in [
            (
                vec![W::Fifo, W::Mailbox, W::Immediate],
                PresentMode::Immediate,
            ),
            (vec![W::Fifo, W::Mailbox], PresentMode::Mailbox),
            (vec![W::Fifo], PresentMode::Fifo),
        ] {
            assert_eq!(resolve(PresentMode::AutoNoVsync, &supported), expected);
        }
        assert_eq!(
            resolve(PresentMode::Fifo, &[W::Fifo, W::Immediate]),
            PresentMode::Fifo
        );
    }
}
