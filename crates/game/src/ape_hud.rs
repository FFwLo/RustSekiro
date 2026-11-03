//! The summoned ape has its own small vitality and posture panel.
use super::super::Sim;
use bevy::prelude::*;

#[derive(Component)]
pub(super) struct Panel;
#[derive(Component)]
pub(super) struct Fill(bool);
#[derive(Component)]
pub(super) struct Deathblow;

pub(super) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                width: px(150.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(3.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Deathblow,
                Text::new("●"),
                TextFont {
                    font_size: bevy::text::FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.05, 0.04)),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(-24.0),
                    top: px(-6.0),
                    ..default()
                },
                Visibility::Hidden,
            ));
            for posture in [true, false] {
                p.spawn((
                    Node {
                        width: percent(100.0),
                        height: px(if posture { 5.0 } else { 8.0 }),
                        padding: UiRect::all(px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.75)),
                ))
                .with_children(|b| {
                    b.spawn((
                        Fill(posture),
                        Node {
                            width: percent(100.0),
                            height: percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ));
                });
            }
        });
}

#[allow(clippy::type_complexity)]
pub(super) fn update(
    sim: NonSend<Sim>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut panel: Query<(&mut Node, &mut Visibility), With<Panel>>,
    mut fills: Query<(&Fill, &mut Node, &mut BackgroundColor), Without<Panel>>,
    mut mark: Query<&mut Visibility, (With<Deathblow>, Without<Panel>)>,
) {
    let Ok((mut node, mut visibility)) = panel.single_mut() else {
        return;
    };
    let Some(ape) = sim
        .duel
        .headless_ape
        .as_ref()
        .filter(|a| a.duel.enemy_active && !a.duel.rules.fighters[1].dead())
    else {
        *visibility = Visibility::Hidden;
        return;
    };
    let head = Vec3::from_array(ape.duel.enemy.body.position)
        + Vec3::Y * (ape.duel.fighters[1].height + 0.2);
    let screen = camera
        .single()
        .ok()
        .and_then(|(cam, tf)| cam.world_to_viewport(tf, head).ok());
    let Some(screen) = screen else {
        *visibility = Visibility::Hidden;
        return;
    };
    node.left = px(screen.x - 75.0);
    node.top = px(screen.y - 16.0);
    *visibility = Visibility::Inherited;
    if let Ok(mut mark) = mark.single_mut() {
        *mark = if sim.duel.brown_deathblow_available() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let f = &ape.duel.rules.fighters[1];
    for (fill, mut node, mut color) in &mut fills {
        let fraction = if fill.0 {
            1.0 - f.posture.remaining as f32 / f.posture.max.max(1) as f32
        } else {
            f.vitality.hp as f32 / f.vitality.max_hp.max(1) as f32
        };
        node.width = percent(fraction.clamp(0.0, 1.0) * 100.0);
        color.0 = if fill.0 {
            Color::srgb(0.95, 0.78 - 0.62 * fraction.clamp(0.0, 1.0), 0.18)
        } else {
            Color::srgb(0.72, 0.1, 0.08)
        };
    }
}
