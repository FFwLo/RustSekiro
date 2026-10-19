//! The game's own fonts, loaded once for every screen.
//!
//! Sekiro embeds them in font/<lang>_<use>/font.gfx; `tools/gfx_font.py` converts those to
//! extracted/font/*.ttf:
//! - `ui`: FOT-Matisse ProN DB (engus_std), menus, HUD and prompts;
//! - `map`: FOT-TsukuOldMin Pro R (engus_map), the Antique Map;
//! - `effect`: FOT-Greco Std M (engus_texteffect), big text effects;
//! - `kanji`: the Japanese Matisse (jpnjp_std, 7197 glyphs), for 危, 死 and other marks.
//!
//! Without extracted fonts each falls back to a Windows font (Georgia / Yu Gothic) and then to
//! Bevy's default.

use bevy::prelude::*;

#[derive(Resource, Default, Clone)]
pub(crate) struct GameFonts {
    pub(crate) ui: Option<Handle<Font>>,
    pub(crate) map: Option<Handle<Font>>,
    pub(crate) effect: Option<Handle<Font>>,
    pub(crate) kanji: Option<Handle<Font>>,
}

pub(crate) struct FontsPlugin;

impl Plugin for FontsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameFonts>().add_systems(PreStartup, load);
    }
}

fn load(mut fonts: ResMut<Assets<Font>>, mut gf: ResMut<GameFonts>) {
    let dir = crate::paths::extracted().join("font");
    let mut first = |files: &[std::path::PathBuf]| files.iter().find_map(|p| std::fs::read(p).ok()).map(|b| fonts.add(Font::from_bytes(b)));
    let win = |f: &str| std::path::Path::new("C:/Windows/Fonts").join(f);
    gf.ui = first(&[dir.join("engus_std.ttf"), win("georgia.ttf")]);
    gf.map = first(&[dir.join("engus_map.ttf"), win("georgia.ttf")]);
    gf.effect = first(&[dir.join("engus_texteffect.ttf"), win("georgia.ttf")]);
    gf.kanji = first(&[dir.join("jpnjp_std.ttf"), win("YuGothB.ttc"), win("msgothic.ttc"), win("meiryo.ttc")]);
}

/// A `TextFont` of `px` pixels in `font` (Bevy's default when the font is missing).
pub(crate) fn text_font(font: &Option<Handle<Font>>, px: f32) -> TextFont {
    TextFont { font: font.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(px), ..default() }
}
