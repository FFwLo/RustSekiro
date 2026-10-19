//! HUD aspect (Options "HUD aspect", config `video.hud_aspect`): on a window wider than 16:9 the
//! HUD and menus are laid out on a centred 16:9 stage, the game's own 1920 x 1080 movie stage,
//! so their Vw / Percent sizes and places are not stretched. A UI camera with that viewport is
//! the default UI camera; "Stretch" (or a window 16:9 or narrower) gives it the whole window.
//! - `WorldUi` roots, placed over the world with `world_to_viewport` (the over-head bars, lock-on,
//!   deathblow, stealth and grapple marks), stay on the 3D camera's whole window, under the HUD.
//! - `ScreenUi` roots (the fall cut, the death screen) cover the whole window over everything, on
//!   a full-window overlay camera.
//! - Off Playing (title, menus, idol), the bars beside the stage are black.
//!
//! The UI cameras draw into their own (non-HDR) textures cleared to transparent, composited over
//! the scene with premultiplied alpha. Each costs about a millisecond a frame, so each runs only
//! when needed: the stage camera while there is a stage (else the 3D camera is the default UI
//! camera, as before), the overlay camera while a `ScreenUi` root or a bar shows.

use bevy::camera::{CameraOutputMode, Viewport};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::window::PrimaryWindow;

use crate::config::{GameConfig, HudAspect};
use crate::menu::Screen;

/// The stage's width over its height.
const STAGE_ASPECT: f32 = 16.0 / 9.0;

/// A UI root placed over the world: drawn on the 3D camera's whole window.
#[derive(Component)]
pub struct WorldUi;

/// A UI root covering the whole window over everything (fades).
#[derive(Component)]
pub struct ScreenUi;

/// The HUD and menus' camera (the default UI camera).
#[derive(Component)]
struct StageCamera;

/// The full-window camera over it for `ScreenUi`.
#[derive(Component)]
struct OverlayCamera;

/// A black bar beside the stage.
#[derive(Component)]
struct Pillar;

pub struct UiStagePlugin;

impl Plugin for UiStagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(PostUpdate, (target_roots, fit_stage).before(bevy::ui::UiSystems::Prepare));
    }
}

/// The UI cameras' own render layer: no mesh or sprite is on it, so the per-view visibility pass
/// skips the map's thousands of pieces for them (UI ignores render layers).
const UI_LAYER: usize = 31;

fn ui_camera(order: isize) -> (Camera2d, Camera, Msaa, bevy::camera::visibility::RenderLayers, bevy::core_pipeline::tonemapping::Tonemapping) {
    let camera = Camera {
        order,
        clear_color: ClearColorConfig::Custom(Color::NONE),
        output_mode: CameraOutputMode::Write { blend_state: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING), clear_color: ClearColorConfig::None },
        ..default()
    };
    (Camera2d, camera, Msaa::Off, bevy::camera::visibility::RenderLayers::layer(UI_LAYER), bevy::core_pipeline::tonemapping::Tonemapping::None)
}

fn spawn(mut commands: Commands) {
    let mut stage = ui_camera(10);
    stage.1.is_active = false;
    commands.spawn((stage, StageCamera));
    let mut over = ui_camera(20);
    over.1.is_active = false;
    let overlay = commands.spawn((over, OverlayCamera)).id();
    for right in [false, true] {
        let mut node = Node { position_type: PositionType::Absolute, top: Val::Px(0.0), height: Val::Percent(100.0), width: Val::Px(0.0), ..default() };
        if right {
            node.right = Val::Px(0.0);
        } else {
            node.left = Val::Px(0.0);
        }
        commands.spawn((node, BackgroundColor(Color::BLACK), Visibility::Hidden, Pillar, UiTargetCamera(overlay)));
    }
}

/// Points the `WorldUi` roots at the 3D camera and the `ScreenUi` roots at the overlay camera.
fn target_roots(
    mut commands: Commands,
    world: Query<Entity, (With<WorldUi>, Without<UiTargetCamera>)>,
    screen: Query<Entity, (With<ScreenUi>, Without<UiTargetCamera>)>,
    scene: Query<Entity, With<crate::camera::OrbitCamera>>,
    overlay: Query<Entity, With<OverlayCamera>>,
) {
    if let Ok(cam) = scene.single() {
        for e in &world {
            commands.entity(e).insert(UiTargetCamera(cam));
        }
    }
    if let Ok(cam) = overlay.single() {
        for e in &screen {
            commands.entity(e).insert(UiTargetCamera(cam));
        }
    }
}

/// The stage's place in the window (physical px): `None` = the whole window.
pub(crate) fn stage_rect(window: UVec2, aspect: HudAspect) -> Option<(UVec2, UVec2)> {
    if aspect == HudAspect::Stretch || window.x == 0 || window.y == 0 {
        return None;
    }
    let w = (window.y as f32 * STAGE_ASPECT).round() as u32;
    // A window at most a pixel or two wider than 16:9 keeps the whole width.
    (w + 2 < window.x).then(|| (UVec2::new((window.x - w) / 2, 0), UVec2::new(w, window.y)))
}

#[allow(clippy::type_complexity)]
fn fit_stage(
    mut commands: Commands,
    config: Res<GameConfig>,
    state: Option<Res<State<Screen>>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut stage: Query<(Entity, &mut Camera, Has<IsDefaultUiCamera>), (With<StageCamera>, Without<OverlayCamera>)>,
    mut overlay: Query<&mut Camera, With<OverlayCamera>>,
    scene: Query<(Entity, Has<IsDefaultUiCamera>), With<crate::camera::OrbitCamera>>,
    mut pillars: Query<(&mut Node, &mut Visibility), With<Pillar>>,
    screen: Query<&Visibility, (With<ScreenUi>, Without<Pillar>)>,
) {
    let (Ok(window), Ok((stage_e, mut camera, stage_default)), Ok(mut over)) = (window.single(), stage.single_mut(), overlay.single_mut()) else { return };
    let rect = stage_rect(window.physical_size(), config.video.hud_aspect);
    // The default UI camera: the stage camera while there is a stage, else the 3D camera.
    if camera.is_active != rect.is_some() {
        camera.is_active = rect.is_some();
    }
    if let Ok((scene_e, scene_default)) = scene.single() {
        if rect.is_some() && !stage_default {
            commands.entity(scene_e).remove::<IsDefaultUiCamera>();
            commands.entity(stage_e).insert(IsDefaultUiCamera);
        } else if rect.is_none() && !scene_default {
            commands.entity(stage_e).remove::<IsDefaultUiCamera>();
            commands.entity(scene_e).insert(IsDefaultUiCamera);
        }
    }
    let viewport = rect.map(|(at, size)| Viewport { physical_position: at, physical_size: size, ..default() });
    if camera.viewport.as_ref().map(|v| (v.physical_position, v.physical_size)) != viewport.as_ref().map(|v| (v.physical_position, v.physical_size)) {
        camera.viewport = viewport;
    }
    let playing = state.is_none_or(|s| *s.get() == Screen::Playing);
    let bar = rect.map_or(0.0, |(at, _)| at.x as f32 / window.scale_factor());
    for (mut node, mut vis) in &mut pillars {
        let want = if bar > 0.0 && !playing { Visibility::Inherited } else { Visibility::Hidden };
        vis.set_if_neq(want);
        if node.width != Val::Px(bar + 1.0) {
            node.width = Val::Px(bar + 1.0);
        }
    }
    let bars = bar > 0.0 && !playing;
    let shown = bars || screen.iter().any(|v| *v != Visibility::Hidden);
    if over.is_active != shown {
        over.is_active = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultrawide_gets_a_centred_16_9_stage() {
        // The 3456 x 1416 ultrawide: a 2517 px stage, 469 px either side.
        assert_eq!(stage_rect(UVec2::new(3456, 1416), HudAspect::Stage), Some((UVec2::new(469, 0), UVec2::new(2517, 1416))));
        assert_eq!(stage_rect(UVec2::new(3456, 1416), HudAspect::Stretch), None);
        // 16:9 and narrower windows keep the whole window.
        assert_eq!(stage_rect(UVec2::new(1920, 1080), HudAspect::Stage), None);
        assert_eq!(stage_rect(UVec2::new(1920, 1200), HudAspect::Stage), None);
        assert_eq!(stage_rect(UVec2::new(0, 0), HudAspect::Stage), None);
    }
}
