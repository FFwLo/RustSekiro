//! Small recording panel, fed only by the driver's read-only view.
use bevy::prelude::*;
use sekiro_sim::brain::driver::BrainOverlay;

#[derive(Resource)]
pub struct RecordingBrain {
    pub enabled: bool,
    pub mode: String,
    pub player: String,
    pub view: BrainOverlay,
    pub unicode_font: bool,
}

/// The panel's distance from the window's top right corner, and its width (px).
pub(crate) const PANEL_INSET: f32 = 24.0;
pub(crate) const PANEL_WIDTH: f32 = 344.0;

#[derive(Component)]
pub struct Panel;
#[derive(Component)]
pub struct Section(usize);

pub fn spawn(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    mut brain: ResMut<RecordingBrain>,
) {
    let font = std::fs::read("C:/Windows/Fonts/segoeui.ttf")
        .ok()
        .map(|bytes| bevy::text::FontSource::Handle(fonts.add(Font::from_bytes(bytes))));
    brain.unicode_font = font.is_some();
    let font = font.unwrap_or_default();
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                right: px(PANEL_INSET),
                top: px(PANEL_INSET),
                width: px(PANEL_WIDTH),
                padding: UiRect::all(px(16)),
                border: UiRect::left(px(2)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.028, 0.035, 0.88)),
            BorderColor::all(Color::srgba(0.72, 0.59, 0.36, 0.8)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            for i in 0..8 {
                p.spawn((
                    Section(i),
                    Text::new(""),
                    Node {
                        width: percent(100),
                        padding: if i == 3 {
                            UiRect::all(px(9))
                        } else {
                            UiRect::ZERO
                        },
                        ..default()
                    },
                    TextFont {
                        font: font.clone(),
                        font_size: bevy::text::FontSize::Px(if i == 0 {
                            15.0
                        } else if i == 4 || i == 7 {
                            12.0
                        } else {
                            14.0
                        }),
                        ..default()
                    },
                    TextColor(if i == 0 || i == 3 {
                        Color::srgb(0.91, 0.78, 0.54)
                    } else if i == 4 || i == 7 {
                        Color::srgb(0.68, 0.71, 0.75)
                    } else {
                        Color::srgb(0.92, 0.9, 0.85)
                    }),
                    BackgroundColor(if i == 3 {
                        Color::srgba(0.45, 0.33, 0.14, 0.22)
                    } else {
                        Color::NONE
                    }),
                ));
            }
        });
}

pub fn update(
    keys: Res<ButtonInput<KeyCode>>,
    mut brain: ResMut<RecordingBrain>,
    mut panel: Query<(&mut Visibility, &mut Node), With<Panel>>,
    mut sections: Query<(&Section, &mut Text, &mut Node, &mut TextFont), Without<Panel>>,
    windows: Query<&Window>,
) {
    if keys.just_pressed(KeyCode::F8) {
        brain.enabled = !brain.enabled;
    }
    let Ok((mut visibility, mut node)) = panel.single_mut() else {
        return;
    };
    *visibility = if brain.enabled {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !brain.enabled {
        return;
    }
    let height = windows.single().map_or(900.0, Window::height);
    let view = &brain.view;
    let mut content = [
        format!(
            "{} · fight {}\nplayer · {}",
            brain.mode, view.fight, brain.player
        ),
        format!(
            "NOW\n{}\n{}{}\nCountered: {}",
            view.set,
            view.running,
            if view.label.is_empty() {
                String::new()
            } else {
                format!("\n{}", view.label)
            },
            view.countered
        ),
        format!(
            "LAST CHECKPOINT\n{}",
            view.checkpoint
                .as_deref()
                .unwrap_or("Waiting for a checkpoint")
        ),
        match (&view.coach, view.thinking) {
            (Some(card), true) => format!("{card}\ncoach thinking…"),
            (Some(card), false) => card.clone(),
            (None, true) => "coach thinking…".into(),
            _ => String::new(),
        },
        view.principles.join("\n"),
        view.ban.clone().unwrap_or_default(),
        view.scorecard.clone().unwrap_or_default(),
        format!(
            "{}{}",
            view.cost,
            view.budget_notice
                .as_ref()
                .map_or(String::new(), |notice| format!("\n{notice}"))
        ),
    ];
    if !brain.unicode_font {
        for text in &mut content {
            *text = text.replace('·', "|").replace('…', "...");
        }
    }
    // Keep even a dense principles list above Wolf's HUD on a smaller recording window.
    let lines: usize = content
        .iter()
        .map(|s| {
            s.lines()
                .map(|l| l.chars().count().div_ceil(39).max(1))
                .sum::<usize>()
        })
        .sum();
    let scale = ((height - 155.0) / (lines as f32 * 17.0 + 115.0)).clamp(0.55, 1.0);
    node.row_gap = px(10.0 * scale);
    for (section, mut text, mut node, mut font) in &mut sections {
        let value = &content[section.0];
        if text.0 != *value {
            text.0.clone_from(value);
        }
        node.display = if value.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        font.font_size = bevy::text::FontSize::Px(
            scale
                * if section.0 == 0 {
                    15.0
                } else if section.0 == 4 || section.0 == 7 {
                    12.0
                } else {
                    14.0
                },
        );
    }
}
