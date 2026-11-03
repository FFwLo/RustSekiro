//! The "SCULPTOR'S IDOL FOUND" banner (#152): the first kneel at an idol lights it, and while
//! Wolf is `IdolPhase::Lighting` (`Duel::idol_lighting`, the sim's clock) the game's text effect
//! plays once over the screen: the screen darkens, the brush mark 鬼仏見出 (`MENU_Onihotoke`,
//! menu atlas SB_TextEffect) and the caption (イベントテキスト 10010901) fade in gold, a soft
//! glow (`MENU_Onihotoke_02`) swells over the mark, and all of it fades out. The idol menu
//! opens once the phase moves on to the prayer.
//!
//! Everything here is the game's own movie: the `BonfireLit` clip of the text effects in
//! `menu/01_000_FE.gfx` (a Scaleform movie at 60 frames a second, played from its `Normal`
//! label). Its keyframes below are that clip's frame numbers and the alphas and scales of its
//! layers, read from its place-object tags (ours, from the uncompressed movie); its positions
//! are pixels of the 1920 x 1080 stage, with the hi-res atlas art drawn at half size.
//! gap: the movie's blend modes are not read (all layers blend normally here), and the
//! caption's font is the menu font, not the movie's MenuFont_03.
use std::collections::BTreeMap;
use std::sync::LazyLock;

use bevy::curve::UnevenSampleAutoCurve;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::window::PrimaryWindow;

use super::Sim;
use crate::game_ui::{Covers, GameFonts, MenuArt, stage};

/// The movie's frame rate, and the frame its `Normal` label (where playback starts) is on.
const FPS: f32 = 60.0;
const START: f32 = 1.0;
/// (movie frame, value) keyframes, straight lines between them.
type Keys = &'static [(f32, f32)];
/// The black over the screen, the mark, the caption and the glow's alpha.
const SHADE: Keys = &[(69.0, 0.0), (101.0, 0.5), (174.0, 0.5), (196.0, 0.0)];
const MARK: Keys = &[
    (81.0, 0.0),
    (85.0, 0.36),
    (90.0, 0.7),
    (95.0, 0.91),
    (100.0, 1.0),
    (174.0, 1.0),
    (175.0, 0.91),
    (180.0, 0.53),
    (185.0, 0.25),
    (190.0, 0.074),
    (195.0, 0.0),
];
const CAPTION: Keys = &[(81.0, 0.0), (101.0, 1.0), (174.0, 1.0), (196.0, 0.0)];
const GLOW: Keys = &[
    (85.0, 0.0),
    (86.0, 0.04),
    (90.0, 0.18),
    (95.0, 0.3),
    (100.0, 0.375),
    (127.0, 0.45),
    (174.0, 0.3),
    (185.0, 0.227),
    (193.0, 0.078),
    (197.0, 0.0),
];
/// The glow's width and height over its first size.
const GLOW_WIDTH: Keys = &[
    (85.0, 1.0),
    (90.0, 1.136),
    (95.0, 1.218),
    (100.0, 1.292),
    (105.0, 1.312),
];
const GLOW_HEIGHT: Keys = &[(85.0, 1.0), (105.0, 1.075), (174.0, 1.047), (194.0, 1.068)];
/// The clip's last frame.
const END: f32 = 197.0;
/// Stage pixels: the mark's size and top, the glow's first size (centred on the mark), and the
/// caption's top, size and letter spacing.
const MARK_SIZE: Vec2 = Vec2::new(117.0, 538.0);
const MARK_TOP: f32 = 147.0;
const GLOW_SIZE: Vec2 = Vec2::new(130.0, 552.0);
const CAPTION_TOP: f32 = 746.0;
const CAPTION_PX: f32 = 33.0;
const CAPTION_SPACING: f32 = 14.0;
/// The mark's and the glow's tint, and the caption's colour.
const GOLD: Color = Color::srgb(220.0 / 255.0, 188.0 / 255.0, 131.0 / 255.0);
const CAPTION_COLOR: Color = Color::srgb(226.0 / 255.0, 201.0 / 255.0, 154.0 / 255.0);
/// イベントテキスト: "SCULPTOR'S IDOL FOUND".
const CAPTION_TEXT: i32 = 10010901;

#[derive(Component)]
struct Banner;
#[derive(Component)]
struct Mark;
#[derive(Component)]
struct Glow;
#[derive(Component)]
struct Caption;

/// The banner, with its caption from `events` (イベントテキスト).
pub(super) fn install(app: &mut App, events: &BTreeMap<i32, String>) {
    let caption = events
        .get(&CAPTION_TEXT)
        .cloned()
        .unwrap_or_else(|| "SCULPTOR'S IDOL FOUND".into());
    app.add_systems(
        Startup,
        move |commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>| {
            spawn(commands, fonts, art, &caption)
        },
    )
    // One system rather than run conditions on the phase: `Sim` is main-thread data, and a run
    // condition reading it is evaluated off the main thread (bevy aborts the app).
    .add_systems(Update, update.run_if(crate::menu::active));
}

fn spawn(mut commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>, caption: &str) {
    let image = |name: &str| {
        art.sprite("SB_TextEffect", name)
            .map(|(image, _)| image.with_color(Color::NONE))
    };
    // The mark, or the glow's first size, centred on the middle of the mark.
    let centred = |size: Vec2| Node {
        position_type: PositionType::Absolute,
        left: percent(50),
        margin: UiRect::left(stage(-size.x / 2.0)),
        top: stage(MARK_TOP + (MARK_SIZE.y - size.y) / 2.0),
        width: stage(size.x),
        height: stage(size.y),
        ..default()
    };
    // A covering screen of its own, so it hides the HUD as the idol menu does.
    commands
        .spawn((
            Banner,
            Covers,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(120),
            Visibility::Hidden,
        ))
        .with_children(|c| {
            if let Some(image) = image("MENU_Onihotoke") {
                c.spawn((Mark, image, centred(MARK_SIZE)));
            }
            if let Some(image) = image("MENU_Onihotoke_02") {
                // Swells about its centre (`UiTransform` scales about the node's middle).
                c.spawn((Glow, image, centred(GLOW_SIZE), UiTransform::IDENTITY));
            }
            c.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: stage(CAPTION_TOP),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child((
                Caption,
                Text::new(caption),
                fonts.stage(CAPTION_PX),
                TextColor(Color::NONE),
                TextLayout::justify(Justify::Center),
            ));
        });
}

/// How the banner looks: its layers' alphas and the glow's scale.
#[derive(Debug, PartialEq)]
struct Look {
    shade: f32,
    mark: f32,
    caption: f32,
    glow: f32,
    glow_scale: Vec2,
}

/// The clip's keyframe tracks as curves over its frames, held before the first key and after
/// the last.
struct Tracks {
    shade: UnevenSampleAutoCurve<f32>,
    mark: UnevenSampleAutoCurve<f32>,
    caption: UnevenSampleAutoCurve<f32>,
    glow: UnevenSampleAutoCurve<f32>,
    glow_width: UnevenSampleAutoCurve<f32>,
    glow_height: UnevenSampleAutoCurve<f32>,
}

static TRACKS: LazyLock<Tracks> = LazyLock::new(|| {
    let curve = |keys: Keys| UnevenSampleAutoCurve::new(keys.iter().copied()).unwrap();
    Tracks {
        shade: curve(SHADE),
        mark: curve(MARK),
        caption: curve(CAPTION),
        glow: curve(GLOW),
        glow_width: curve(GLOW_WIDTH),
        glow_height: curve(GLOW_HEIGHT),
    }
});

/// The banner `t` seconds after the idol was lit, `None` once the clip is over.
fn look(t: f32) -> Option<Look> {
    let frame = START + t * FPS;
    let k = &*TRACKS;
    (frame <= END).then(|| Look {
        shade: k.shade.sample_clamped(frame),
        mark: k.mark.sample_clamped(frame),
        caption: k.caption.sample_clamped(frame),
        glow: k.glow.sample_clamped(frame),
        glow_scale: Vec2::new(
            k.glow_width.sample_clamped(frame),
            k.glow_height.sample_clamped(frame),
        ),
    })
}

/// The banner at the sim's lighting clock while Wolf lights an idol (`IdolPhase::Lighting`),
/// taken down once when the phase ends; nothing else in between.
#[allow(clippy::type_complexity)]
fn update(
    sim: NonSend<Sim>,
    mut shown: Local<bool>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<(&mut Visibility, &mut BackgroundColor), With<Banner>>,
    mut mark: Query<&mut ImageNode, (With<Mark>, Without<Glow>)>,
    mut glow: Query<(&mut ImageNode, &mut UiTransform), With<Glow>>,
    mut caption: Query<(&mut TextColor, &mut LetterSpacing), With<Caption>>,
) {
    let look = sim.duel.idol_lighting().and_then(look);
    if look.is_none() && !*shown {
        return;
    }
    let Ok((mut visibility, mut background)) = root.single_mut() else {
        return;
    };
    *shown = look.is_some();
    let Some(look) = look else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    background.set_if_neq(BackgroundColor(Color::BLACK.with_alpha(look.shade)));
    for mut image in &mut mark {
        image.color = GOLD.with_alpha(look.mark);
    }
    for (mut image, mut transform) in &mut glow {
        image.color = GOLD.with_alpha(look.glow);
        transform.scale = look.glow_scale;
    }
    // The movie's letter spacing is stage pixels; ours is logical pixels of the window.
    let scale = window.single().map_or(1.0, |w| w.height() / 1080.0);
    for (mut color, mut spacing) in &mut caption {
        color.set_if_neq(TextColor(CAPTION_COLOR.with_alpha(look.caption)));
        spacing.set_if_neq(LetterSpacing::Px(CAPTION_SPACING * scale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_plays_the_clip_once_and_ends() {
        // The sim holds Wolf for exactly the clip.
        assert_eq!((END - START) / FPS, sekiro_sim::duel::idol::LIGHTING_SECS);
        // Nothing shows before the screen darkens, the mark is full at its hold, and the clip
        // ends at its last frame.
        let first = look(0.0).unwrap();
        assert_eq!(
            (first.shade, first.mark, first.caption, first.glow),
            (0.0, 0.0, 0.0, 0.0)
        );
        let held = look((140.0 - START) / FPS).unwrap();
        assert_eq!((held.shade, held.mark, held.caption), (0.5, 1.0, 1.0));
        assert!(held.glow > 0.3 && held.glow_scale.x > 1.3, "{held:?}");
        let last = look((END - START) / FPS).unwrap();
        assert_eq!(
            (last.shade, last.mark, last.caption, last.glow),
            (0.0, 0.0, 0.0, 0.0)
        );
        assert_eq!(look((END - START) / FPS + 0.1), None);
    }
}
