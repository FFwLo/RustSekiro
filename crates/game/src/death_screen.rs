//! The death screen (#123): while Wolf lies dead, the game's 死 brush mark (`MENU_Death`, menu
//! atlas SB_TextEffect) fades in white with the game's DEATH caption (イベントテキスト 10010902)
//! under it and slowly turns red while the screen darkens. When the simulation's restart is
//! coming (`Duel::respawn_in`), everything fades to black over its last `FINAL_FADE` seconds,
//! and after the restart the black lifts off the new scene. Without the menu art the caption
//! shows alone.
//!
//! Timings from FwLo's RustSekiro `src/menu.rs` `death_overlay` (set by eye there from the
//! game's screen). gap: the game's own movie for it (01_210_dead.gfx: the ink bleed, its
//! timings) is not decoded, and there is no resurrection yet (#124): every death restarts.
use bevy::prelude::*;

use crate::combat_hud::Gauges;
use crate::game_ui::{GameFonts, MenuArt};

/// Seconds: the mark's fade in, its turn from white to red, the darkening behind it (to
/// `SHADE`), the fade to black before the restart, and the black lifting after it.
const MARK_IN: f32 = 0.8;
const MARK_RED: f32 = 4.0;
const DARKEN: f32 = 4.0;
const SHADE: f32 = 0.45;
const FINAL_FADE: f32 = 2.5;
const LIFT: f32 = 0.6;
const RED: Color = Color::srgb(0.75, 0.06, 0.05);
const CAPTION: Color = Color::srgb(0.85, 0.82, 0.76);
/// イベントテキスト.fmg: "DEATH".
const CAPTION_TEXT: i32 = 10010902;

#[derive(Component)]
struct DeathScreen;
#[derive(Component)]
struct Mark;
#[derive(Component)]
struct Caption;

pub fn install(app: &mut App) {
    GameFonts::insert(app);
    MenuArt::insert(app);
    app.add_systems(Startup, spawn).add_systems(Update, update);
}

fn spawn(mut commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>) {
    let cache = sekiro_sim::cache::root();
    let mark = art.sprite("SB_TextEffect", "MENU_Death");
    let caption = crate::game_ui::menu_texts(&cache, "イベントテキスト")
        .remove(&CAPTION_TEXT)
        .unwrap_or_else(|| "DEATH".into());
    commands
        .spawn((
            DeathScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(150),
            Visibility::Hidden,
        ))
        .with_children(|c| {
            if let Some((image, aspect)) = mark {
                c.spawn((
                    Mark,
                    image.with_color(Color::NONE),
                    Node {
                        height: vh(30),
                        aspect_ratio: Some(aspect),
                        ..default()
                    },
                ));
            }
            c.spawn((
                Caption,
                Text::new(caption),
                fonts.ui(22.0),
                TextColor(Color::NONE),
                Node {
                    margin: UiRect::top(vh(1)),
                    ..default()
                },
            ));
        });
}

/// How the screen looks: the mark's colour, the caption's colour and the black behind them.
#[derive(Debug)]
struct Look {
    mark: Color,
    caption: Color,
    shade: f32,
}

/// `t` seconds after the death, with the restart `respawn_in` seconds away (`None`: none is
/// coming, so the screen holds at `SHADE`).
fn look(t: f32, respawn_in: Option<f32>) -> Look {
    let alpha = (t / MARK_IN).min(1.0);
    let red = (t / MARK_RED).clamp(0.0, 1.0);
    let shade = SHADE * (t / DARKEN).min(1.0);
    let fin = respawn_in.map_or(0.0, |r| (1.0 - r / FINAL_FADE).clamp(0.0, 1.0));
    Look {
        mark: Color::WHITE.mix(&RED, red).with_alpha(alpha),
        caption: Color::WHITE.mix(&CAPTION, red).with_alpha(alpha * 0.75),
        shade: shade + (1.0 - shade) * fin,
    }
}

/// Where the screen is: since when Wolf has been dead, or since when he has been back and
/// the shade the lift starts from.
#[derive(Default)]
enum Phase {
    #[default]
    Alive,
    Dead {
        since: f32,
        shade: f32,
    },
    Back {
        since: f32,
        from: f32,
    },
}

#[allow(clippy::type_complexity)]
fn update(
    time: Res<Time>,
    gauges: Res<Gauges>,
    mut phase: Local<Phase>,
    mut root: Query<(&mut Visibility, &mut BackgroundColor), With<DeathScreen>>,
    mut marks: Query<&mut ImageNode, With<Mark>>,
    mut caption: Query<&mut TextColor, With<Caption>>,
) {
    let Ok((mut visibility, mut background)) = root.single_mut() else {
        return;
    };
    let now = time.elapsed_secs();
    // The bars start empty (max 0) before the first simulation step.
    let dead = gauges.wolf_max_hp > 0 && gauges.hp[0] <= 0.0;
    *phase = match (&*phase, dead) {
        (phase, true) => {
            let since = match phase {
                Phase::Dead { since, .. } => *since,
                _ => now,
            };
            let shade = look(now - since, gauges.respawn_in).shade;
            Phase::Dead { since, shade }
        }
        (Phase::Dead { shade, .. }, false) => Phase::Back {
            since: now,
            from: *shade,
        },
        (Phase::Back { since, from }, false) if now - since < LIFT => Phase::Back {
            since: *since,
            from: *from,
        },
        (_, false) => Phase::Alive,
    };
    let look = match *phase {
        Phase::Alive => {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        }
        Phase::Dead { since, .. } => look(now - since, gauges.respawn_in),
        Phase::Back { since, from } => Look {
            mark: Color::NONE,
            caption: Color::NONE,
            shade: from * (1.0 - (now - since) / LIFT),
        },
    };
    visibility.set_if_neq(Visibility::Inherited);
    background.set_if_neq(BackgroundColor(Color::BLACK.with_alpha(look.shade)));
    for mut image in &mut marks {
        if image.color != look.mark {
            image.color = look.mark;
        }
    }
    for mut text in &mut caption {
        text.set_if_neq(TextColor(look.caption));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_goes_black_only_before_a_restart_and_lifts_after_it() {
        let start = look(0.0, Some(sekiro_sim::duel::RESPAWN_DELAY));
        assert_eq!((start.mark.alpha(), start.shade), (0.0, 0.0));
        // No restart coming (an explore fall): red mark, held at the shade.
        let held = look(10.0, None);
        assert!((held.mark.to_srgba().green - 0.06).abs() < 1e-4, "{held:?}");
        assert_eq!(held.shade, SHADE);
        assert!(look(MARK_RED, Some(FINAL_FADE / 2.0)).shade > SHADE);
        assert_eq!(look(MARK_RED, Some(0.0)).shade, 1.0);

        // Dying shows the screen; coming back lifts the shade it had, then hides it.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Gauges {
                hp: [0.0, 1.0],
                wolf_max_hp: 100,
                respawn_in: Some(0.0),
                ..default()
            })
            .add_systems(Update, update);
        let screen = app
            .world_mut()
            .spawn((
                DeathScreen,
                BackgroundColor(Color::NONE),
                Visibility::Hidden,
            ))
            .id();
        let shade = |app: &App| {
            app.world()
                .get::<BackgroundColor>(screen)
                .unwrap()
                .0
                .alpha()
        };
        app.update();
        app.update();
        assert_eq!(shade(&app), 1.0);
        {
            let mut gauges = app.world_mut().resource_mut::<Gauges>();
            gauges.hp[0] = 1.0;
            gauges.respawn_in = None;
        }
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(screen),
            Some(&Visibility::Inherited)
        );
        assert!(shade(&app) > 0.9);
        // Virtual time steps at most 0.25 s a frame.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f32(0.2),
        ));
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().get::<Visibility>(screen),
            Some(&Visibility::Hidden)
        );
    }
}
