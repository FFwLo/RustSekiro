//! Full-screen placeholder for map-event cinematics, above world and HUD rendering.
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Fade {
    pub alpha: f32,
    /// Require black to reach presentation before loading a replacement boss synchronously.
    pub black_frames: u8,
}

#[derive(Component)]
struct Overlay;

pub struct CinematicFadePlugin;

impl Plugin for CinematicFadePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fade>()
            .add_systems(Startup, spawn)
            .add_systems(PreUpdate, block_input.after(bevy::input::InputSystems))
            .add_systems(Update, draw);
    }
}

fn block_input(
    fade: Res<Fade>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pads: Query<&mut Gamepad>,
) {
    if fade.alpha > 0.0 {
        keys.reset_all();
        mouse.reset_all();
        for mut pad in &mut pads {
            pad.digital_mut().reset_all();
        }
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Overlay,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(i32::MAX),
        BackgroundColor(Color::NONE),
        Visibility::Hidden,
    ));
}

fn draw(
    mut fade: ResMut<Fade>,
    mut overlay: Query<(&mut BackgroundColor, &mut Visibility), With<Overlay>>,
) {
    if let Ok((mut color, mut visibility)) = overlay.single_mut() {
        color.0 = Color::srgba(0.0, 0.0, 0.0, fade.alpha);
        visibility.set_if_neq(if fade.alpha > 0.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    fade.black_frames = if fade.alpha >= 1.0 {
        fade.black_frames.saturating_add(1)
    } else {
        0
    };
}
