//! `--arena`: Wolf and an Ashina soldier facing each other on a flat floor, animated with the
//! game's own clips. The soldier attacks on a timer; hold G to guard.
//! This is a presentation harness: combat rules come from `sekiro-sim` once it lands.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bevy::app::AppExit;
use bevy::asset::AssetPlugin;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::character::{AnimLibrary, Animator, Rig, animate, bind_rigs};
use crate::deathblow_demo::{self, DeathblowDemo};
use crate::draw_mask::{DrawMask, MaskSource, apply_draw_masks};

pub struct Options {
    pub cache: PathBuf,
    pub screenshot: Option<PathBuf>,
    /// Seconds after both rigs bind at which to take the screenshot.
    pub at: f32,
    /// Demo: the soldier's posture breaks and Wolf performs the front deathblow.
    pub deathblow: bool,
}

#[derive(Component)]
pub(crate) struct Wolf;

#[derive(Component)]
pub(crate) struct Soldier {
    next_attack: f32,
}

#[derive(Component)]
struct Hud;

/// Clip libraries, kept outside the ECS lookup path for simple synchronous loading.
#[derive(Resource)]
pub(crate) struct Libraries {
    pub(crate) wolf: AnimLibrary,
    pub(crate) soldier: AnimLibrary,
}

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    at: f32,
    since_bound: Option<f32>,
    requested: bool,
}

const ATTACK_INTERVAL: f32 = 4.0;
const SOLDIER_DISTANCE: f32 = 2.4;
/// Starting distance for the deathblow demo: inside the far deathblow's grab range.
const DEATHBLOW_DISTANCE: f32 = 2.0;

#[derive(Resource, Clone, Copy)]
struct SoldierDistance(f32);

pub fn run(options: Options) -> Result<()> {
    let cache = options.cache.canonicalize()?;
    let mut wolf = AnimLibrary::load(&cache, "c0000")?;
    let mut soldier = AnimLibrary::load(&cache, "c1010")?;
    // Preload the clips this harness uses so missing data fails at startup.
    for c in ["a000_000000", "a050_203000", "a050_002000"] {
        wolf.clip(c)?;
    }
    for c in ["a000_000000", "a000_003000"] {
        soldier.clip(c)?;
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "sekiro-rs".into(),
                    resolution: (1600, 900).into(),
                    present_mode: crate::presentation::requested_mode(),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: cache.join("models").to_string_lossy().into_owned(),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::srgb(0.30, 0.34, 0.40)))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.95, 0.95, 1.0),
        brightness: 450.0,
        ..default()
    })
    .insert_resource(Libraries { wolf, soldier })
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (bind_rigs, (wolf_input, soldier_ai), animate, hud).chain(),
    )
    .add_systems(Update, apply_draw_masks);
    if options.deathblow {
        app.insert_resource(DeathblowDemo::load(&cache)?)
            .insert_resource(SoldierDistance(DEATHBLOW_DISTANCE))
            .add_systems(
                Update,
                (deathblow_demo::drive, deathblow_demo::follow_camera)
                    .chain()
                    .after(animate)
                    .before(hud),
            );
    } else {
        app.insert_resource(SoldierDistance(SOLDIER_DISTANCE));
    }
    if let Some(path) = options.screenshot {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        app.insert_resource(Capture {
            path,
            at: options.at,
            since_bound: None,
            requested: false,
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
    distance: Res<SoldierDistance>,
    assets: Res<AssetServer>,
    mut libs: ResMut<Libraries>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(1.1, 1.9, 3.2)
            .looking_at(Vec3::new(0.0, 1.0, -distance.0 * 0.5), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            color: Color::srgb(1.0, 0.96, 0.90),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-3.0, 5.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 2_500.0,
            color: Color::srgb(0.85, 0.9, 1.0),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 2.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.21, 0.19),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));

    let wolf_idle = libs.wolf.clip("a000_000000").unwrap();
    let mut wolf_anim = Animator::new(wolf_idle, true);
    wolf_anim.apply_root_motion = false;
    commands.spawn((
        Wolf,
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("c0000.glb"))),
        Transform::IDENTITY,
        Rig::new(Arc::new(libs.wolf.skeleton.clone())),
        wolf_anim,
    ));

    let soldier_idle = libs.soldier.clip("a000_000000").unwrap();
    let mut soldier_anim = Animator::new(soldier_idle, true);
    soldier_anim.apply_root_motion = false;
    commands.spawn((
        Soldier { next_attack: 2.0 },
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("c1010.glb"))),
        // In combat the soldier holds his katana: the weapon-draw TAE draw mask.
        DrawMask::new(MaskSource::Combat),
        Transform::from_xyz(0.0, 0.0, -distance.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        Rig::new(Arc::new(libs.soldier.skeleton.clone())),
        soldier_anim,
    ));

    commands.spawn((
        Hud,
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn wolf_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut libs: ResMut<Libraries>,
    mut q: Query<&mut Animator, With<Wolf>>,
) {
    let Ok(mut anim) = q.single_mut() else { return };
    let held = keys.pressed(KeyCode::KeyG);
    let current = anim.clip_name().to_string();
    match (held, current.as_str()) {
        (true, "a000_000000") => anim.play(libs.wolf.clip("a050_203000").unwrap(), false),
        (true, "a050_203000") if anim.finished() => {
            anim.play(libs.wolf.clip("a050_002000").unwrap(), true)
        }
        (false, "a050_203000" | "a050_002000") => {
            anim.play(libs.wolf.clip("a000_000000").unwrap(), true)
        }
        _ => {}
    }
}

fn soldier_ai(
    time: Res<Time>,
    demo: Option<Res<DeathblowDemo>>,
    mut libs: ResMut<Libraries>,
    mut q: Query<(&mut Soldier, &mut Animator, &Rig)>,
) {
    if demo.is_some() {
        return; // the deathblow demo drives the soldier
    }
    for (mut soldier, mut anim, rig) in &mut q {
        if !rig.bound {
            continue;
        }
        soldier.next_attack -= time.delta_secs();
        if anim.clip_name() == "a000_003000" && anim.finished() {
            anim.play(libs.soldier.clip("a000_000000").unwrap(), true);
        }
        if soldier.next_attack <= 0.0 {
            soldier.next_attack = ATTACK_INTERVAL;
            anim.play(libs.soldier.clip("a000_003000").unwrap(), false);
        }
    }
}

fn hud(
    mut text: Query<&mut Text, With<Hud>>,
    q: Query<(&Animator, Has<Wolf>)>,
    demo: Option<Res<DeathblowDemo>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let mut s = String::from("hold G to guard\n");
    for (anim, is_wolf) in &q {
        let who = if is_wolf { "Wolf" } else { "Soldier" };
        s += &format!("{who}: {} {:.2}s\n", anim.clip_name(), anim.time);
    }
    if let Some(d) = demo {
        s += &format!("{}\n", d.status);
    }
    text.0 = s;
}

fn capture(
    mut commands: Commands,
    mut readiness: crate::map::ready::Ready,
    time: Res<Time>,
    mut cap: ResMut<Capture>,
    rigs: Query<&Rig>,
    mut exit: MessageWriter<AppExit>,
) {
    if !rigs.iter().all(|r| r.bound) || rigs.is_empty() {
        return;
    }
    let since = cap.since_bound.get_or_insert(0.0);
    *since += time.delta_secs();
    let since = *since;
    if !cap.requested && since >= cap.at {
        if !readiness.allow() {
            return;
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(cap.path.clone()));
        cap.requested = true;
    } else if cap.requested && cap.path.is_file() {
        std::thread::sleep(Duration::from_millis(200));
        println!("screenshot saved to {}", cap.path.display());
        exit.write(AppExit::Success);
    }
}
