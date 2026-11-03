//! `--viewer`: shows one exported GLB in bind pose with an orbit camera.
//! With `--screenshot`, saves a frame once the model is on screen and exits.

use std::path::PathBuf;
use std::time::{Duration, Instant};

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::app::AppExit;
use bevy::asset::AssetPlugin;
use bevy::gltf::GltfMaterialName;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::world_serialization::WorldInstanceReady;

use crate::draw_mask::{self, DrawMask, MaskSource};

pub struct Options {
    pub glb: PathBuf,
    pub screenshot: Option<PathBuf>,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub distance: f32,
    pub height: f32,
    /// Hide primitives whose material matches any of these (see [`matches`]).
    pub hide: Vec<String>,
    /// When non-empty, show only primitives whose material matches one of these.
    pub only: Vec<String>,
    /// Show enemies with weapons drawn (`combatDrawMask`) instead of their default look.
    pub combat: bool,
}

#[derive(Resource)]
struct Model(String, MaskSource);

#[derive(Resource)]
struct Filter {
    hide: Vec<String>,
    only: Vec<String>,
}

#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
}

#[derive(Resource, Default)]
struct Loaded {
    ready: bool,
    ready_at: Option<Instant>,
    frames_since: u32,
}

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    requested_at: Option<u32>,
    started: Instant,
}

/// Frames and time to wait after the scene spawns, so pipelines compile and textures upload
/// (Bevy skips drawing a mesh until its pipeline is ready).
const SETTLE_FRAMES: u32 = 90;
const SETTLE_TIME: Duration = Duration::from_secs(3);

pub fn run(options: Options) -> Result<()> {
    let glb = options
        .glb
        .canonicalize()
        .with_context(|| format!("{} not found", options.glb.display()))?;
    let dir = glb.parent().context("model has no parent folder")?;
    let file = glb
        .file_name()
        .and_then(|f| f.to_str())
        .context("model file name is not UTF-8")?
        .to_string();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("sekiro-rs viewer - {file}"),
                    resolution: (1280, 960).into(),
                    present_mode: crate::presentation::requested_mode(),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: dir.to_string_lossy().into_owned(),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::srgb(0.30, 0.34, 0.40)))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.95, 0.95, 1.0),
        brightness: 450.0,
        ..default()
    })
    .insert_resource(Model(
        file,
        if options.combat {
            MaskSource::Combat
        } else {
            MaskSource::Default
        },
    ))
    .insert_resource(Filter {
        hide: options.hide,
        only: options.only,
    })
    .insert_resource(Orbit {
        yaw: options.yaw_degrees.to_radians(),
        pitch: options.pitch_degrees.to_radians(),
        distance: options.distance,
        target: Vec3::new(0.0, options.height, 0.0),
    })
    .init_resource::<Loaded>()
    .add_systems(Startup, setup)
    .add_systems(Update, (orbit_input, apply_orbit).chain())
    .add_systems(Update, (draw_mask::apply_draw_masks, apply_filter).chain());
    if let Some(path) = options.screenshot {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        app.insert_resource(Capture {
            path,
            requested_at: None,
            started: Instant::now(),
        })
        .add_systems(Last, capture);
    }
    crate::map::ready::install(&mut app);
    app.add_plugins(crate::presentation::PresentationPlugin);
    app.run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    model: Res<Model>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((Camera3d::default(), Transform::default()));
    // Key light from the front-left and above, with shadows.
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            color: Color::srgb(1.0, 0.96, 0.90),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-2.0, 4.0, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Soft rim/fill from behind-right.
    commands.spawn((
        DirectionalLight {
            illuminance: 2_500.0,
            color: Color::srgb(0.85, 0.9, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 2.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.21, 0.19),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    commands
        .spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(model.0.clone()))),
            DrawMask::new(model.1),
        ))
        .observe(|_: On<WorldInstanceReady>, mut loaded: ResMut<Loaded>| {
            info!("model spawned");
            loaded.ready = true;
            loaded.ready_at = Some(Instant::now());
        });
}

fn orbit_input(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut orbit: ResMut<Orbit>,
) {
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.006;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.006).clamp(-1.4, 1.4);
    }
    if buttons.pressed(MouseButton::Right) {
        orbit.target.y += motion.delta.y * 0.004;
    }
    if scroll.delta.y != 0.0 {
        orbit.distance = (orbit.distance * (1.0 - scroll.delta.y * 0.1)).clamp(0.3, 50.0);
    }
}

fn apply_orbit(orbit: Res<Orbit>, mut camera: Query<&mut Transform, With<Camera3d>>) {
    let dir = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    );
    for mut t in &mut camera {
        *t = Transform::from_translation(orbit.target + dir * orbit.distance)
            .looking_at(orbit.target, Vec3::Y);
    }
}

fn capture(
    mut commands: Commands,
    mut readiness: crate::map::ready::Ready,
    mut loaded: ResMut<Loaded>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if capture.started.elapsed() > Duration::from_secs(120) {
        error!("timed out waiting for the model");
        exit.write(AppExit::error());
        return;
    }
    if !loaded.ready {
        return;
    }
    loaded.frames_since += 1;
    match capture.requested_at {
        None if loaded.frames_since >= SETTLE_FRAMES
            && loaded.ready_at.is_some_and(|t| t.elapsed() >= SETTLE_TIME) =>
        {
            if !readiness.allow() {
                return;
            }
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(capture.path.clone()));
            capture.requested_at = Some(loaded.frames_since);
        }
        Some(at) if loaded.frames_since > at + 5 && capture.path.is_file() => {
            println!("screenshot saved to {}", capture.path.display());
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

/// Material names exported by `sekiro-extract models` start with `m<flver mesh index>`.
/// A token like `m68` matches that index exactly; any other token matches a substring.
fn matches(material: &str, token: &str) -> bool {
    let first = material.split(' ').next().unwrap_or_default();
    let is_index =
        token.len() > 1 && token.starts_with('m') && token[1..].chars().all(|c| c.is_ascii_digit());
    if is_index {
        first == token
    } else {
        material.contains(token)
    }
}

fn apply_filter(filter: Res<Filter>, mut meshes: Query<(&GltfMaterialName, &mut Visibility)>) {
    if filter.hide.is_empty() && filter.only.is_empty() {
        return;
    }
    for (name, mut visibility) in &mut meshes {
        let hidden = filter.hide.iter().any(|t| matches(name, t))
            || (!filter.only.is_empty() && !filter.only.iter().any(|t| matches(name, t)));
        if hidden {
            *visibility = Visibility::Hidden;
        }
    }
}
