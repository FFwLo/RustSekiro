//! The "SCULPTOR'S IDOL FOUND" banner: the first kneel at an idol lights it (idol.rs `light`,
//! `Seat::lighting`), and the game's text effect plays once over the screen while Wolf kneels
//! on: the screen darkens, the brush mark 鬼仏見出 (SB_TextEffect `MENU_Onihotoke`) and the
//! caption (イベントテキスト 10010901) fade in gold, a soft glow (`MENU_Onihotoke_02`) swells
//! over the mark, and all of it fades out. The idol menu opens after it.
//!
//! The keyframes are the game's own movie: the `BonfireLit` clip of the text effects in
//! `menu/01_000_FE.gfx` (Scaleform, 60 frames a second, played from its `Normal` label), its
//! layers' alphas and scales from its place-object tags, positions in pixels of the 1920 x 1080
//! stage (the values as read by sekiro-rs `idol_found.rs`).
//! gap: the movie's blend modes are not read (all layers blend normally here).
use bevy::curve::UnevenSampleAutoCurve;
use bevy::prelude::*;
use bevy::text::LetterSpacing;
use bevy::window::PrimaryWindow;

/// The movie's frame rate, and the frame its `Normal` label (where playback starts) is on.
const FPS: f32 = 60.0;
const START: f32 = 1.0;
/// (movie frame, value) keyframes, straight lines between them.
type Keys = &'static [(f32, f32)];
/// The black over the screen, the mark, the caption and the glow's alpha.
const SHADE: Keys = &[(69.0, 0.0), (101.0, 0.5), (174.0, 0.5), (196.0, 0.0)];
const MARK: Keys = &[(81.0, 0.0), (85.0, 0.36), (90.0, 0.7), (95.0, 0.91), (100.0, 1.0), (174.0, 1.0), (175.0, 0.91), (180.0, 0.53), (185.0, 0.25), (190.0, 0.074), (195.0, 0.0)];
const CAPTION: Keys = &[(81.0, 0.0), (101.0, 1.0), (174.0, 1.0), (196.0, 0.0)];
const GLOW: Keys = &[(85.0, 0.0), (86.0, 0.04), (90.0, 0.18), (95.0, 0.3), (100.0, 0.375), (127.0, 0.45), (174.0, 0.3), (185.0, 0.227), (193.0, 0.078), (197.0, 0.0)];
/// The glow's width and height over its first size.
const GLOW_WIDTH: Keys = &[(85.0, 1.0), (90.0, 1.136), (95.0, 1.218), (100.0, 1.292), (105.0, 1.312)];
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
const CAPTION_TEXT: u32 = 10010901;

#[derive(Component)]
struct Banner;
#[derive(Component)]
struct Mark;
#[derive(Component)]
struct Glow;
#[derive(Component)]
struct Caption;

pub(crate) struct IdolFoundPlugin;

impl Plugin for IdolFoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(Update, update);
    }
}

/// Stage pixels as a share of the window's height (the stage is scaled to fit it).
fn stage(px: f32) -> Val {
    Val::Vh(px / 10.8)
}

fn spawn(mut commands: Commands, assets: Res<AssetServer>, fonts: Res<crate::fonts::GameFonts>, texts: Res<crate::action_guide::GuideTexts>) {
    let caption = texts.text(CAPTION_TEXT).unwrap_or("SCULPTOR'S IDOL FOUND").to_string();
    let image = |name: &str| ImageNode::new(assets.load(crate::paths::asset(format!("hud/{name}.png")))).with_color(Color::NONE);
    // The mark, or the glow's first size, centred on the middle of the mark.
    let centred = |size: Vec2| Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        margin: UiRect::left(stage(-size.x / 2.0)),
        top: stage(MARK_TOP + (MARK_SIZE.y - size.y) / 2.0),
        width: stage(size.x),
        height: stage(size.y),
        ..default()
    };
    commands
        .spawn((Banner, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::NONE), GlobalZIndex(120), Visibility::Hidden))
        .with_children(|c| {
            c.spawn((Mark, image("MENU_Onihotoke"), centred(MARK_SIZE)));
            // Swells about its centre (`UiTransform` scales about the node's middle).
            c.spawn((Glow, image("MENU_Onihotoke_02"), centred(GLOW_SIZE), UiTransform::IDENTITY));
            c.spawn(Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: stage(CAPTION_TOP), justify_content: JustifyContent::Center, ..default() })
                .with_child((Caption, Text::new(caption), crate::fonts::text_font(&fonts.ui, CAPTION_PX), TextColor(Color::NONE), TextLayout::justify(Justify::Center), LetterSpacing::default()));
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

/// The banner `t` seconds after the idol was lit, `None` once the clip is over.
fn look(t: f32) -> Option<Look> {
    let frame = START + t * FPS;
    let at = |keys: Keys| UnevenSampleAutoCurve::new(keys.iter().copied()).map_or(0.0, |c| c.sample_clamped(frame));
    (frame <= END).then(|| Look {
        shade: at(SHADE),
        mark: at(MARK),
        caption: at(CAPTION),
        glow: at(GLOW),
        glow_scale: Vec2::new(at(GLOW_WIDTH), at(GLOW_HEIGHT)),
    })
}

/// Draws the banner at the clock of Wolf's seat while it lights its idol (idol.rs ends the
/// lighting with the clip and the menu then opens); hidden otherwise.
#[allow(clippy::type_complexity)]
fn update(
    wolf: Query<&crate::player::Player>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<(&mut Visibility, &mut BackgroundColor), With<Banner>>,
    mut mark: Query<&mut ImageNode, (With<Mark>, Without<Glow>)>,
    mut glow: Query<(&mut ImageNode, &mut UiTransform), With<Glow>>,
    mut caption: Query<(&mut TextColor, &mut TextFont, &mut LetterSpacing), With<Caption>>,
) {
    let look = wolf.single().ok().and_then(|p| p.seat.lighting).and_then(look);
    let Ok((mut visibility, mut background)) = root.single_mut() else { return };
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
    // The movie's sizes are stage pixels; text is in logical pixels of the window.
    let scale = window.single().map_or(1.0, |w| w.height() / 1080.0);
    for (mut color, mut font, mut spacing) in &mut caption {
        color.set_if_neq(TextColor(CAPTION_COLOR.with_alpha(look.caption)));
        font.font_size = bevy::text::FontSize::Px(CAPTION_PX * scale);
        spacing.set_if_neq(LetterSpacing::Px(CAPTION_SPACING * scale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_idol_found_banner_plays_the_clip_once_and_ends() {
        // The seat holds Wolf for exactly the clip.
        assert_eq!((END - START) / FPS, crate::idol::LIGHTING_SECS);
        let first = look(0.0).unwrap();
        assert_eq!((first.shade, first.mark, first.caption, first.glow), (0.0, 0.0, 0.0, 0.0));
        let held = look((140.0 - START) / FPS).unwrap();
        assert_eq!((held.shade, held.mark, held.caption), (0.5, 1.0, 1.0));
        assert!(held.glow > 0.3 && held.glow_scale.x > 1.3, "{held:?}");
        let last = look((END - START) / FPS).unwrap();
        assert_eq!((last.shade, last.mark, last.caption, last.glow), (0.0, 0.0, 0.0, 0.0));
        assert_eq!(look((END - START) / FPS + 0.1), None);
    }
}
