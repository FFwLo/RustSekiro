//! The game's screens (plan C1): Title, Playing, Paused, Dead and Won as a Bevy `States`.
//!
//! Outside `Playing` the game clock (`Time<Virtual>`) stands still, so the fight, animations
//! and effects freeze behind the menu, and the keyboard / mouse presses are taken by the menu
//! (cleared after it reads them, before any game system runs), so a menu click never swings
//! the sword. Esc pauses; Wolf's death with no resurrection left shows 死 (Retry / Title);
//! every enemy down shows 忍殺 (Retry / Title).
//!
//! Automated runs (any `SHINOBI_*` check variable other than the map / hour / root choices)
//! start straight in `Playing`; `SHINOBI_TITLE=1` forces the title (`=paused` / `dead` / `won`
//! / `picker` open that screen, for checks).
//! The marks are the game's own brush art (死, 忍殺, the 隻狼 title; extracted/hud), plain text
//! when they are not extracted. gap: the game's menu movies (layout, fades, the 死 ink bleed) are
//! not used. The 死 / 忍殺 timings are not traced (shown at once / after
//! WON_HOLD).

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    #[default]
    Title,
    Playing,
    Paused,
    Dead,
    Won,
    /// The encounter picker (plan C2): every boss and enemy of the roster.
    Picker,
    /// Options (plan C3): camera, pad, sound, window; saved to config.toml on leaving.
    Options,
    /// Kneeling at a Sculptor's Idol: its menu (idol.rs).
    Idol,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Choice {
    Start,
    Fight,
    Options,
    Next,
    Resume,
    Retry,
    Title,
    Quit,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Choice::Start => "Start",
            Choice::Fight => "Fight",
            Choice::Options => "Options",
            Choice::Next => "Next",
            Choice::Resume => "Resume",
            Choice::Retry => "Retry",
            Choice::Title => "Title",
            Choice::Quit => "Quit",
        }
    }
}

fn choices(s: Screen, picker: &Picker) -> Vec<Choice> {
    match s {
        Screen::Title => vec![Choice::Start, Choice::Fight, Choice::Options, Choice::Quit],
        Screen::Paused => vec![Choice::Resume, Choice::Retry, Choice::Options, Choice::Title, Choice::Quit],
        Screen::Dead => vec![Choice::Retry, Choice::Title],
        // Next: the encounter after the one fought, in its picker tab.
        Screen::Won if picker.current.is_some() => vec![Choice::Next, Choice::Retry, Choice::Title],
        Screen::Won => vec![Choice::Retry, Choice::Title],
        Screen::Playing | Screen::Picker | Screen::Options | Screen::Idol => vec![],
    }
}

/// The Options rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opt {
    CameraSpeed,
    InvertY,
    Gamepad,
    Volume,
    Music,
    Se,
    Voice,
    Fullscreen,
    Vsync,
    Cloth,
    Back,
}

const OPTS: [Opt; 11] = [Opt::CameraSpeed, Opt::InvertY, Opt::Gamepad, Opt::Volume, Opt::Music, Opt::Se, Opt::Voice, Opt::Fullscreen, Opt::Vsync, Opt::Cloth, Opt::Back];

impl Opt {
    fn label(self) -> &'static str {
        match self {
            Opt::CameraSpeed => "Camera speed",
            Opt::InvertY => "Invert camera up / down",
            Opt::Gamepad => "Gamepad",
            Opt::Volume => "Volume",
            Opt::Music => "Music volume",
            Opt::Se => "Sound effects volume",
            Opt::Voice => "Voice volume",
            Opt::Fullscreen => "Fullscreen",
            Opt::Vsync => "Vertical sync",
            Opt::Cloth => "Cloth physics",
            Opt::Back => "Back",
        }
    }

    fn value(self, c: &crate::config::GameConfig) -> String {
        let on = |b: bool| if b { "On" } else { "Off" }.to_string();
        match self {
            Opt::CameraSpeed => format!("{}", c.input.camera_speed.round()),
            Opt::InvertY => on(c.input.invert_y),
            Opt::Gamepad => on(c.input.gamepad),
            Opt::Volume => format!("{}", c.audio.volume.round()),
            Opt::Music => format!("{}", c.audio.music.round()),
            Opt::Se => format!("{}", c.audio.se.round()),
            Opt::Voice => format!("{}", c.audio.voice.round()),
            Opt::Fullscreen => on(c.video.fullscreen),
            Opt::Vsync => on(c.video.vsync),
            Opt::Cloth => match c.video.cloth {
                crate::config::ClothSetting::On => "On",
                crate::config::ClothSetting::Near => "Near only",
                crate::config::ClothSetting::Off => "Off",
            }
            .to_string(),
            Opt::Back => String::new(),
        }
    }

    /// Left (-1) / right (+1); a switch flips either way.
    fn change(self, c: &mut crate::config::GameConfig, dir: f32) {
        match self {
            Opt::CameraSpeed => c.input.camera_speed = (c.input.camera_speed.round() + dir).clamp(1.0, 10.0),
            Opt::Volume => c.audio.volume = (c.audio.volume.round() + dir).clamp(0.0, 10.0),
            Opt::Music => c.audio.music = (c.audio.music.round() + dir).clamp(0.0, 10.0),
            Opt::Se => c.audio.se = (c.audio.se.round() + dir).clamp(0.0, 10.0),
            Opt::Voice => c.audio.voice = (c.audio.voice.round() + dir).clamp(0.0, 10.0),
            Opt::InvertY => c.input.invert_y = !c.input.invert_y,
            Opt::Gamepad => c.input.gamepad = !c.input.gamepad,
            Opt::Fullscreen => c.video.fullscreen = !c.video.fullscreen,
            Opt::Vsync => c.video.vsync = !c.video.vsync,
            Opt::Cloth => {
                use crate::config::ClothSetting::*;
                const ORDER: [crate::config::ClothSetting; 3] = [Off, Near, On];
                let i = ORDER.iter().position(|s| *s == c.video.cloth).unwrap_or(2) as i32;
                c.video.cloth = ORDER[(i + dir.signum() as i32).rem_euclid(3) as usize];
            }
            Opt::Back => {}
        }
    }
}

/// Saves the Options values to config.toml (kept across runs; F5 reload reads them too).
fn save_options(c: &crate::config::GameConfig) {
    let keys = [
        ("input", "camera_speed", format!("{:.1}", c.input.camera_speed)),
        ("input", "invert_y", c.input.invert_y.to_string()),
        ("input", "gamepad", c.input.gamepad.to_string()),
        ("audio", "volume", format!("{:.1}", c.audio.volume)),
        ("audio", "music", format!("{:.1}", c.audio.music)),
        ("audio", "se", format!("{:.1}", c.audio.se)),
        ("audio", "voice", format!("{:.1}", c.audio.voice)),
        ("video", "fullscreen", c.video.fullscreen.to_string()),
        ("video", "vsync", c.video.vsync.to_string()),
        ("video", "cloth", format!("\"{}\"", c.video.cloth.name())),
    ];
    if let Err(e) = crate::config::save_keys(&keys) {
        warn!("options not saved: {e}");
    }
}

/// The picker's two lists, from `data::encounter_list` (extracted/enemies/roster.json):
/// "Bosses" every placed row with a boss or mini-boss health bar (Encounter.bar), "Enemies" one
/// row per other chr (its first placement). The names are the bars' own (NPC名 text). Rows named as dummies (the roster's "... dummy character" helpers: HP display,
/// bullet owners, sight checks) are left out: they are not fighters.
const TABS: [&str; 2] = ["Bosses", "Enemies"];

/// The main-boss rows (ours; docs/PLAN.md "Update 2026-10-13" names these 20 as "the 22"). The Bosses tab shows only these
/// for now (the user, 2026-10-16: "in bosses menu keep the bosses that i work on only"); the
/// mini-bosses and generals are the outside contributor's. 52000000 (Divine Dragon) is the Old
/// Dragons' second phase (data::encounter_list).
pub(crate) const MAIN_BOSS_ROWS: [i64; 20] = [
    12600050, 50000000, 50001000, 50600000, 50601010, 50800000, 50900000, 50900001, 51000000, 51000100, 52000000,
    53000000, 54000000, 54300000, 70200000, 71000000, 71001000, 71100000, 71101000, 74000010,
];

/// Rows the picker shows at once.
const PICKER_ROWS: usize = 10;

#[derive(Resource, Default)]
struct Picker {
    lists: Option<[Vec<crate::data::Encounter>; 2]>,
    tab: usize,
    index: [usize; 2],
    /// The encounter being fought (tab, index), for Next.
    current: Option<(usize, usize)>,
    /// The (tab, index) the overlay was built for.
    built: Option<(usize, usize)>,
}

impl Picker {
    fn lists(&mut self) -> &[Vec<crate::data::Encounter>; 2] {
        self.lists.get_or_insert_with(|| {
            let all: Vec<_> = crate::data::encounter_list().into_iter().filter(|e| !e.name.to_lowercase().contains("dummy")).collect();
            let bosses: Vec<_> = all.iter().filter(|e| e.bar && MAIN_BOSS_ROWS.contains(&e.npc_row)).cloned().collect();
            let mut enemies: Vec<crate::data::Encounter> = Vec::new();
            for e in all.into_iter().filter(|e| !e.bar) {
                if !enemies.iter().any(|o| o.chr == e.chr) {
                    enemies.push(e);
                }
            }
            [bosses, enemies]
        })
    }

    /// The tab / index of the encounter config enemy.chr / npc_row names (the fight at start).
    fn find(&mut self, chr: &str, row: Option<i64>) -> Option<(usize, usize)> {
        let lists = self.lists();
        (0..2).find_map(|t| lists[t].iter().position(|e| e.chr == chr && row.is_none_or(|r| r == e.npc_row)).map(|i| (t, i)))
    }
}

/// Seconds every enemy has to stay down before 忍殺 in a fight without a boss script (a boss
/// between phases is down for a moment). gap: hand-set, the game's own timing is not traced.
const WON_HOLD: f32 = 4.0;

/// After the end, Won waits for the finisher to play out (`finisher_done`): the game shows no
/// menu over it. A safety cap for a clip that never ends (a held pose): 30 s.
const FINISHER_CAP: f32 = 30.0;

/// The fight's last moves are over: Wolf is back in a standby state (player::is_free: out of the
/// deathblow / Todome / behind-finisher clip) and every enemy's clip has played to its end (its
/// death or ThrowDef pose). Round 93, the user's bug: Won came WON_HOLD (4 s) after Handle Boss
/// Defeat and cut the long finishers (Isshin's 12900, the Mortal Blade Todomes).
/// Only the characters the ending plays on count: the dead (their death / ThrowDef pose) and
/// one still in a ThrowDef or Todome clip. A cast member alive by design never reaches a clip
/// end (the Divine Dragon's trees re-play their bloom loop, Mode::Held(true) loops, the Old
/// Dragons' bodies keep their AI), so waiting on all of them ran into FINISHER_CAP: the banner
/// 30 s after Handle Boss Defeat on rows 52000000 / 53000000.
/// A boss kneeling for its Todome (its Event20200's 105 window, held at the clip's end; player.rs
/// deathblow_check) is not done: the fight waits for Wolf's Todome.
pub(crate) fn finisher_done<'a>(combat: &crate::data::Combat, wolf_state: &str, enemies: impl IntoIterator<Item = (&'a crate::enemy::Enemy, &'a crate::actor::Actor)>) -> bool {
    crate::player::is_free(wolf_state)
        && enemies.into_iter().filter(|(e, a)| e.is_dead() || a.state.starts_with("ThrowDef") || a.state.starts_with("Event20200")).all(|(_, a)| {
            let d = combat.data_of(a);
            let len = if a.anim.is_empty() { 0.0 } else { d.length(&a.anim) };
            let todome = d.finisher_window(&a.anim, a.t) == Some(105) && combat.throw(combat.foe_of(a).throw_row(181)).is_some();
            !todome && (len <= 0.0 || a.t >= len - 1e-3)
        })
}

/// Real seconds the world runs behind the first title before it freezes: Wolf, the camera and
/// the map settle (frozen at frame 0 the camera sat in the map's rock).
const WARMUP: f32 = 1.0;

const RED: Color = Color::srgb(0.75, 0.06, 0.05);
const GOLD: Color = Color::srgb(0.85, 0.74, 0.52);
const DIM: Color = Color::srgba(0.85, 0.82, 0.76, 0.55);

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct MenuItem(usize);

/// The title screen after the game's (2026-10-17, the user's reference): black, the 隻狼 brush
/// calligraphy (SB_Title MENU_Title_SEKIRO) large and faint behind, SEKIRO in its brush letters
/// (SB_Title_02 MENU_Title_S/E/K/I/R/O), SHADOWS DIE TWICE under it (MENU_Shadows /
/// MENU_DieTwice), the choices in capitals with a brush stroke behind the cursor's (SB_InGame
/// MENU_Cursor_03, tinted), and a footer. Without the extracted sprites: the old serif title.
/// gap: sizes, places and tints by eye from the reference; the title's movie is not decoded.
fn build_title(commands: &mut Commands, assets: &AssetServer, fonts: &MenuFonts, list: &[Choice], index: usize) -> bool {
    let sprite = |f: &str| crate::paths::root().join(format!("extracted/menu_sprites/{f}.png")).exists();
    let letters = ["S", "E", "K", "I", "R", "O"];
    if !letters.iter().all(|l| sprite(&format!("sb_title_02/MENU_Title_{l}"))) || !sprite("sb_in_game/MENU_Cursor_03") {
        return false;
    }
    let full = || Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    commands.spawn((full(), BackgroundColor(Color::BLACK), GlobalZIndex(100), MenuRoot)).with_children(|c| {
        // The calligraphy, faint, behind everything.
        if sprite("sb_title/MENU_Title_SEKIRO") {
            c.spawn(full()).with_children(|k| {
                k.spawn((Node { height: Val::Vh(92.0), ..default() }, ImageNode::new(assets.load("menu_sprites/sb_title/MENU_Title_SEKIRO.png")).with_color(Color::srgb(0.16, 0.16, 0.16))));
            });
        }
        c.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..full() }).with_children(|col| {
            col.spawn(Node { column_gap: Val::Vh(1.8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                for l in letters {
                    r.spawn((Node { height: Val::Vh(11.0), ..default() }, ImageNode::new(assets.load(format!("menu_sprites/sb_title_02/MENU_Title_{l}.png")))));
                }
            });
            if sprite("sb_title/MENU_Shadows") && sprite("sb_title/MENU_DieTwice") {
                col.spawn(Node { column_gap: Val::Vh(1.4), margin: UiRect::top(Val::Vh(2.2)), ..default() }).with_children(|r| {
                    for f in ["MENU_Shadows", "MENU_DieTwice"] {
                        r.spawn((Node { height: Val::Vh(2.6), ..default() }, ImageNode::new(assets.load(format!("menu_sprites/sb_title/{f}.png"))).with_color(Color::srgb(0.78, 0.74, 0.68))));
                    }
                });
            }
            col.spawn(Node { height: Val::Vh(9.0), ..default() });
            for (i, ch) in list.iter().enumerate() {
                col.spawn((Button, Node { width: Val::Px(320.0), height: Val::Px(46.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::NONE), MenuItem(i)))
                    .with_children(|b| {
                        b.spawn((
                            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                            ImageNode::new(assets.load("menu_sprites/sb_in_game/MENU_Cursor_03.png")).with_color(TITLE_BRUSH),
                            if i == index { Visibility::Inherited } else { Visibility::Hidden },
                            TitleBrush(i),
                        ));
                        b.spawn((Text::new(ch.label().to_uppercase()), font(&fonts.serif, 24.0), TextColor(DIM)));
                    });
            }
        });
        c.spawn(Node { position_type: PositionType::Absolute, bottom: Val::Vh(3.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }).with_children(|f| {
            f.spawn((Text::new("Sv1  \u{00b7}  an unofficial Sekiro recreation        Enter  select        Esc  back"), font(&fonts.serif, 13.0), TextColor(Color::srgba(0.85, 0.82, 0.76, 0.35))));
        });
    });
    true
}

/// The title's cursor stroke (MENU_Cursor_03 is orange; this darkens it toward the reference's
/// brown).
const TITLE_BRUSH: Color = Color::srgba(0.5, 0.42, 0.38, 0.9);

/// The brush stroke behind title choice n (shown on the cursor's).
#[derive(Component)]
struct TitleBrush(usize);


/// A picker tab button.
#[derive(Component)]
struct PickerTab(usize);

/// A HUD layer hidden by the title, with the visibility its own system last gave it.
#[derive(Component)]
struct TitleHidden(Visibility);

#[derive(Resource, Default)]
struct MenuFonts {
    serif: Option<Handle<Font>>,
    kanji: Option<Handle<Font>>,
}

#[derive(Resource, Default)]
pub(crate) struct Cursor {
    index: usize,
    built: Option<Screen>,
    /// The screen Options goes back to (Title or Paused).
    pub(crate) options_from: Option<Screen>,
    /// Options rebuilds its overlay after a change.
    dirty: bool,
}

/// The screens' input (pad_to_menu_keys, run_menu): game_menu.rs runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MenuInput;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let automated = std::env::vars().any(|(k, _)| k.starts_with("SHINOBI_") && !matches!(k.as_str(), "SHINOBI_MAP" | "SHINOBI_HOUR" | "SHINOBI_ROOT" | "SHINOBI_TITLE") && !k.starts_with("SHINOBI_MAP_"));
        let start = match std::env::var("SHINOBI_TITLE").ok().as_deref() {
            Some("paused") => Screen::Paused,
            Some("idol") => Screen::Idol,
            Some("dead") => Screen::Dead,
            Some("won") => Screen::Won,
            Some("picker") => Screen::Picker,
            Some("options") => Screen::Options,
            Some(_) => Screen::Title,
            None if automated => Screen::Playing,
            None => Screen::Title,
        };
        // The Won screen stays up until Retry / Title instead of the duel's old restart 5 s after
        // a boss's defeat (kept for the automated runs).
        if start != Screen::Playing || !automated {
            app.insert_resource(crate::enemy::AutoRestart(false));
        }
        app.insert_state(start)
            .init_resource::<Cursor>()
            .init_resource::<Picker>()
            .init_resource::<MenuFonts>()
            .init_resource::<BannerDone>()
            .add_systems(Startup, load_fonts)
            .init_resource::<MenuMouse>()
            .add_systems(PreUpdate, (menu_keys_and_wheel, pad_to_menu_keys, run_menu).chain().in_set(MenuInput).after(UiSystems::Focus).after(InputSystems))
            .add_systems(Update, (fight_from_env, apply_video, watch_fight.run_if(in_state(Screen::Playing)), defeat_banner))
            .add_systems(PostUpdate, hide_hud_on_title.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
    }
}

fn load_fonts(mut fonts: ResMut<Assets<Font>>, mut mf: ResMut<MenuFonts>) {
    let read = |names: &[&str]| names.iter().find_map(|f| std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(f)).ok());
    mf.serif = read(&["georgia.ttf"]).map(|b| fonts.add(Font::from_bytes(b)));
    mf.kanji = read(&["YuGothB.ttc", "msgothic.ttc", "meiryo.ttc"]).map(|b| fonts.add(Font::from_bytes(b)));
}

fn font(h: &Option<Handle<Font>>, px: f32) -> TextFont {
    TextFont { font: h.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(px), ..default() }
}

/// The game's brush mark for a screen (extracted/hud, from menu/hi/01_common SB_TextEffect /
/// SB_Title; sekiro-extract export): file, height in % of the window height, tint.
/// gap: the sizes and places are set by eye; the game's movies that show them are not decoded.
fn marks(s: Screen) -> &'static [(&'static str, f32, Color)] {
    match s {
        Screen::Title => &[("MENU_Title_SEKIRO", 42.0, Color::WHITE)],
        Screen::Dead => &[("MENU_Death", 36.0, RED)],
        Screen::Won => &[("MENU_Ninsatsu", 50.0, RED)],
        _ => &[],
    }
}

fn have(file: &str) -> bool {
    crate::paths::root().join(format!("extracted/hud/{file}.png")).exists()
}

/// Builds the overlay of a screen: its big mark / title and the choices.
fn build(commands: &mut Commands, assets: &AssetServer, s: Screen, fonts: &MenuFonts, picker: &mut Picker, config: &crate::config::GameConfig, index: usize) {
    if s == Screen::Picker {
        build_picker(commands, fonts, picker);
        return;
    }
    if s == Screen::Options {
        build_options(commands, fonts, config, index);
        return;
    }
    // Paused is the in-game menu (game_menu.rs: Sekiro's Equipment / Inventory / Skills / Options).
    if s == Screen::Paused {
        return;
    }
    let list = choices(s, picker);
    if s == Screen::Title && build_title(commands, assets, fonts, &list, index) {
        return;
    }
    let (mark, sub, mark_color, shade) = match s {
        Screen::Title => ("Sv1", "Sv1 · a Sekiro combat recreation", GOLD, 0.55),
        Screen::Paused => ("Paused", "", GOLD, 0.45),
        Screen::Dead => ("死", "DEATH", RED, 0.35),
        Screen::Won => ("忍殺", "SHINOBI EXECUTION", RED, 0.25),
        Screen::Playing | Screen::Picker | Screen::Options | Screen::Idol => return,
    };
    let art = marks(s);
    let art_ok = !art.is_empty() && art.iter().all(|(f, ..)| have(f));
    let kanji = matches!(s, Screen::Dead | Screen::Won);
    let has_kanji_font = fonts.kanji.is_some();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, shade)),
            GlobalZIndex(100),
            MenuRoot,
        ))
        .with_children(|c| {
            if art_ok {
                for (file, h, tint) in art {
                    c.spawn((Node { height: Val::Vh(*h), ..default() }, ImageNode::new(assets.load(format!("hud/{file}.png"))).with_color(*tint)));
                }
                // The title's SHADOWS DIE TWICE line (SB_Title MENU_Shadows / MENU_DieTwice).
                if s == Screen::Title && have("MENU_Shadows") && have("MENU_DieTwice") {
                    c.spawn(Node { column_gap: Val::Vh(1.6), ..default() }).with_children(|r| {
                        for f in ["MENU_Shadows", "MENU_DieTwice"] {
                            r.spawn((Node { height: Val::Vh(4.0), ..default() }, ImageNode::new(assets.load(format!("hud/{f}.png")))));
                        }
                    });
                }
            } else {
                let (text, f) = if kanji && has_kanji_font { (mark, font(&fonts.kanji, 150.0)) } else { (if kanji { sub } else { mark }, font(&fonts.serif, if s == Screen::Title { 72.0 } else { 56.0 })) };
                c.spawn((Text::new(text), f, TextColor(mark_color)));
            }
            // Under the brush 死 / 忍殺 the game shows no caption; the text stands in without them.
            if !sub.is_empty() && !(art_ok && kanji) && (!kanji || has_kanji_font) {
                c.spawn((Text::new(sub), font(&fonts.serif, 22.0), TextColor(DIM)));
            }
            c.spawn(Node { height: Val::Px(30.0), ..default() });
            for (i, ch) in list.iter().enumerate() {
                c.spawn((Button, Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(4.0)), ..default() }, BackgroundColor(Color::NONE), MenuItem(i))).with_children(|b| {
                    b.spawn((Text::new(ch.label()), font(&fonts.serif, 30.0), TextColor(DIM)));
                });
            }
        });
}

/// The picker overlay: the tabs, a window of PICKER_ROWS rows around the cursor (gold), each with
/// its deathblow count as red dots, and the keys.
fn build_picker(commands: &mut Commands, fonts: &MenuFonts, picker: &mut Picker) {
    let tab = picker.tab;
    let index = picker.index[tab];
    let list = picker.lists()[tab].clone();
    let start = index.saturating_sub(PICKER_ROWS / 2).min(list.len().saturating_sub(PICKER_ROWS));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            GlobalZIndex(100),
            MenuRoot,
        ))
        .with_children(|c| {
            c.spawn(Node { column_gap: Val::Px(40.0), margin: UiRect::bottom(Val::Px(14.0)), ..default() }).with_children(|r| {
                for (t, name) in TABS.iter().enumerate() {
                    r.spawn((Button, Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(2.0)), border: UiRect::bottom(Val::Px(2.0)), ..default() }, BorderColor::all(if t == tab { GOLD } else { Color::NONE }), PickerTab(t)))
                        .with_children(|b| {
                            b.spawn((Text::new(*name), font(&fonts.serif, 26.0), TextColor(if t == tab { GOLD } else { DIM })));
                        });
                }
            });
            if list.is_empty() {
                c.spawn((Text::new("No encounters extracted (tools/extract.ps1 writes extracted/enemies)."), font(&fonts.serif, 22.0), TextColor(DIM)));
            }
            for (i, e) in list.iter().enumerate().skip(start).take(PICKER_ROWS) {
                c.spawn((Button, Node { width: Val::Px(640.0), justify_content: JustifyContent::SpaceBetween, padding: UiRect::axes(Val::Px(12.0), Val::Px(2.0)), ..default() }, BackgroundColor(if i == index { Color::srgba(0.85, 0.74, 0.52, 0.08) } else { Color::NONE }), MenuItem(i)))
                    .with_children(|b| {
                        // One line each: a long placement name is cut.
                        let name = if e.name.chars().count() > 52 { format!("{}…", e.name.chars().take(51).collect::<String>()) } else { e.name.clone() };
                        b.spawn((Text::new(name), TextLayout::no_wrap(), font(&fonts.serif, 18.0), TextColor(if i == index { GOLD } else { DIM })));
                        b.spawn((Text::new("\u{25cf}".repeat(e.deathblows.clamp(0, 9) as usize)), font(&fonts.kanji, 13.0), TextColor(RED)));
                    });
            }
            c.spawn((
                Text::new(format!("{} / {}    Up / Down choose    Left / Right tab    Enter fight    Esc back", if list.is_empty() { 0 } else { index + 1 }, list.len())),
                font(&fonts.serif, 14.0),
                TextColor(DIM),
                Node { margin: UiRect::top(Val::Px(12.0)), ..default() },
            ));
        });
    picker.built = Some((tab, index));
}

/// The Options overlay: a row per option with its value ("<  5  >" on the cursor's row).
fn build_options(commands: &mut Commands, fonts: &MenuFonts, config: &crate::config::GameConfig, index: usize) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            GlobalZIndex(100),
            MenuRoot,
        ))
        .with_children(|c| {
            c.spawn((Text::new("Options"), font(&fonts.serif, 34.0), TextColor(GOLD), Node { margin: UiRect::bottom(Val::Px(16.0)), ..default() }));
            for (i, o) in OPTS.iter().enumerate() {
                let sel = i == index;
                let color = if sel { GOLD } else { DIM };
                c.spawn((Button, Node { width: Val::Px(520.0), justify_content: JustifyContent::SpaceBetween, padding: UiRect::axes(Val::Px(12.0), Val::Px(3.0)), ..default() }, BackgroundColor(if sel { Color::srgba(0.85, 0.74, 0.52, 0.08) } else { Color::NONE }), MenuItem(i)))
                    .with_children(|b| {
                        b.spawn((Text::new(o.label()), font(&fonts.serif, 20.0), TextColor(color)));
                        let v = o.value(config);
                        if !v.is_empty() {
                            b.spawn((Text::new(if sel { format!("<  {v}  >") } else { v }), font(&fonts.serif, 20.0), TextColor(color)));
                        }
                    });
            }
            c.spawn((Text::new("Up / Down choose    Left / Right change    Esc back (saved)"), font(&fonts.serif, 14.0), TextColor(DIM), Node { margin: UiRect::top(Val::Px(14.0)), ..default() }));
        });
}

/// Starts encounter (tab, index) of the picker: the enemy swap (enemy.rs SpawnEncounter: kinds,
/// clips, line-up) and Wolf back at the start.
fn fight(picker: &mut Picker, at: (usize, usize), pad: &mut Option<ResMut<crate::player::PadInput>>, spawn: &mut MessageWriter<crate::enemy::SpawnEncounter>) -> bool {
    let Some(e) = picker.lists()[at.0].get(at.1).cloned() else { return false };
    spawn.write(crate::enemy::SpawnEncounter { chr: e.chr, npc_row: Some(e.npc_row), group: vec![] });
    if let Some(p) = pad.as_mut() {
        p.reset = true;
    }
    picker.current = Some(at);
    true
}

#[allow(clippy::too_many_arguments)]
/// The mouse on a menu's rows: a click picks its row; a hover moves the cursor only when it
/// begins (the Interaction changed), so a mouse resting on a row neither undoes Up / Down nor
/// makes Enter / Cross pick the row under it (the pause menu's Quit). Items: (row,
/// interaction, changed this frame). Returns the clicked row.
fn mouse_pick(cursor: &mut usize, items: impl Iterator<Item = (usize, Interaction, bool)>) -> Option<usize> {
    let mut pick = None;
    for (row, interaction, changed) in items {
        match interaction {
            Interaction::Hovered if changed => *cursor = row,
            Interaction::Pressed => {
                *cursor = row;
                pick = Some(row);
            }
            _ => {}
        }
    }
    pick
}

fn run_menu(
    mut commands: Commands,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut time: ResMut<Time<Virtual>>,
    real: Res<Time<Real>>,
    mut cursor: ResMut<Cursor>,
    fonts: Res<MenuFonts>,
    assets: Res<AssetServer>,
    roots: Query<Entity, With<MenuRoot>>,
    (items, tabs, hovers, mut brushes): (Query<(&MenuItem, &Interaction, &Children)>, Query<(&PickerTab, &Interaction), Changed<Interaction>>, Query<&MenuItem, Changed<Interaction>>, Query<(&TitleBrush, &mut Visibility)>),
    mut texts: Query<&mut TextColor>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    (mut pad, mut exit, mut spawn, mut picker, mut config): (
        Option<ResMut<crate::player::PadInput>>,
        MessageWriter<AppExit>,
        MessageWriter<crate::enemy::SpawnEncounter>,
        ResMut<Picker>,
        ResMut<crate::config::GameConfig>,
    ),
) {
    let s = *state.get();
    // The fight at start (config enemy.*), for Won's Next.
    if picker.current.is_none() && picker.lists.is_none() {
        picker.current = picker.find(&config.enemy.chr, config.enemy.npc_row);
    }
    if s == Screen::Picker && cursor.built == Some(s) {
        if picker_input(&keys, &items, &tabs, &mut picker, &mut next) {
            let at = (picker.tab, picker.index[picker.tab]);
            if fight(&mut picker, at, &mut pad, &mut spawn) {
                next.set(Screen::Playing);
            }
        }
        keys.reset_all();
        mouse.reset_all();
        if picker.built != Some((picker.tab, picker.index[picker.tab])) {
            for r in &roots {
                commands.entity(r).despawn();
            }
            build_picker(&mut commands, &fonts, &mut picker);
        }
        return;
    }
    if s == Screen::Options && cursor.built == Some(s) {
        let n = OPTS.len();
        let any = |k: &[KeyCode]| k.iter().any(|k| keys.just_pressed(*k));
        let before = cursor.index;
        if any(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
            cursor.index = (cursor.index + 1) % n;
        }
        if any(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
            cursor.index = (cursor.index + n - 1) % n;
        }
        let mut clicked = None;
        for (item, interaction, _) in &items {
            if *interaction == Interaction::Pressed {
                clicked = Some(item.0);
            }
        }
        if let Some(i) = clicked {
            cursor.index = i;
        }
        let o = OPTS[cursor.index];
        let dir = if any(&[KeyCode::ArrowLeft, KeyCode::KeyA]) { -1.0 } else if any(&[KeyCode::ArrowRight, KeyCode::KeyD]) || any(&[KeyCode::Enter, KeyCode::Space]) || clicked.is_some() { 1.0 } else { 0.0 };
        let back = keys.just_pressed(KeyCode::Escape) || (o == Opt::Back && dir != 0.0);
        if dir != 0.0 && o != Opt::Back {
            o.change(&mut config, dir);
            cursor.dirty = true;
        }
        keys.reset_all();
        mouse.reset_all();
        if back {
            save_options(&config);
            next.set(cursor.options_from.unwrap_or(Screen::Title));
            return;
        }
        if cursor.dirty || cursor.index != before {
            cursor.dirty = false;
            for r in &roots {
                commands.entity(r).despawn();
            }
            build_options(&mut commands, &fonts, &config, cursor.index);
        }
        return;
    }
    // (Re)build the overlay when the screen changed.
    if cursor.built != Some(s) {
        for r in &roots {
            commands.entity(r).despawn();
        }
        build(&mut commands, &assets, s, &fonts, &mut picker, &config, 0);
        cursor.built = Some(s);
        cursor.index = 0;
        // The world runs on under the idol's menu, as in the game (the user: "the game pauses when
        // I meditate at a Sculptor's Idol").
        if s == Screen::Idol {
            time.unpause();
            window.grab_mode = CursorGrabMode::None;
            window.visible = true;
        } else if s == Screen::Playing {
            time.unpause();
            window.grab_mode = CursorGrabMode::Confined;
            window.visible = false;
        } else {
            if real.elapsed_secs() >= WARMUP {
                time.pause();
            }
            window.grab_mode = CursorGrabMode::None;
            window.visible = true;
        }
        // The press that changed the screen is spent.
        keys.reset_all();
        mouse.reset_all();
        return;
    }
    if s != Screen::Playing && s != Screen::Idol && !time.is_paused() && real.elapsed_secs() >= WARMUP {
        time.pause();
    }
    if s == Screen::Playing {
        if keys.just_pressed(KeyCode::Escape) {
            next.set(Screen::Paused);
            keys.reset_all();
        }
        return;
    }
    // game_menu.rs runs the in-game menu's input, idol.rs the idol's.
    if s == Screen::Paused || s == Screen::Idol {
        return;
    }
    let list = choices(s, &picker);
    let mut pick: Option<Choice> = None;
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) {
        cursor.index = (cursor.index + 1) % list.len();
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) {
        cursor.index = (cursor.index + list.len() - 1) % list.len();
    }
    let moved: Vec<usize> = hovers.iter().map(|m| m.0).collect();
    if let Some(i) = mouse_pick(&mut cursor.index, items.iter().map(|(item, i, _)| (item.0, *i, moved.contains(&item.0)))) {
        pick = list.get(i).copied();
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        pick = list.get(cursor.index).copied();
    }
    if s == Screen::Paused && keys.just_pressed(KeyCode::Escape) {
        pick = Some(Choice::Resume);
    }
    for (item, _, children) in &items {
        for c in children.iter() {
            if let Ok(mut tc) = texts.get_mut(c) {
                tc.0 = if item.0 == cursor.index { GOLD } else { DIM };
            }
        }
    }
    for (b, mut vis) in &mut brushes {
        *vis = if b.0 == cursor.index { Visibility::Inherited } else { Visibility::Hidden };
    }
    // Nothing reaches the game while a menu is up.
    keys.reset_all();
    mouse.reset_all();
    let Some(pick) = pick else { return };
    match pick {
        Choice::Start | Choice::Resume => next.set(Screen::Playing),
        Choice::Fight => next.set(Screen::Picker),
        Choice::Options => {
            cursor.options_from = Some(s);
            next.set(Screen::Options);
        }
        Choice::Next => {
            let (t, i) = picker.current.unwrap_or((0, 0));
            let len = picker.lists()[t].len().max(1);
            fight(&mut picker, (t, (i + 1) % len), &mut pad, &mut spawn);
            next.set(Screen::Playing);
        }
        Choice::Retry => {
            // The whole fight over (player.rs: Wolf and the enemies' line-up).
            if let Some(p) = pad.as_mut() {
                p.reset = true;
            }
            next.set(Screen::Playing);
        }
        Choice::Title => {
            if let Some(p) = pad.as_mut() {
                p.reset = true;
            }
            next.set(Screen::Title);
        }
        Choice::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

/// The Options window settings (config video): borderless fullscreen on the current monitor or
/// the window; vsync on (AutoVsync) or off (AutoNoVsync). Applied when they change (not at
/// startup unless config.toml asks for fullscreen / vsync off).
fn apply_video(config: Res<crate::config::GameConfig>, mut window: Single<&mut Window, With<PrimaryWindow>>, mut last: Local<Option<(bool, bool, Option<[u32; 2]>)>>) {
    use bevy::window::{MonitorSelection, PresentMode, WindowMode};
    let want = (config.video.fullscreen, config.video.vsync, config.video.resolution);
    let first = last.is_none();
    let before = *last;
    if before == Some(want) {
        return;
    }
    *last = Some(want);
    if !first || want.0 {
        window.mode = if want.0 { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    }
    if !first || !want.1 {
        window.present_mode = if want.1 { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    }
    // The window size (config video.resolution, Map's C3 field); None = main.rs's 1024 x 560.
    if before.map(|b| b.2) != Some(want.2) && (!first || want.2.is_some()) {
        let [w, h] = want.2.unwrap_or([1024, 560]);
        window.resolution.set(w as f32, h as f32);
    }
}

/// Whether the pointer moved this frame: a hover moves a menu's cursor only then, so a list
/// rebuilt under a still pointer (the wheel, the arrows) does not snap back to the row under it.
#[derive(Resource, Default)]
pub(crate) struct MenuMouse {
    pub(crate) moved: bool,
}

/// Menu conveniences (the user): in every menu E is Enter (OK) and Q is Esc (back), and the
/// mouse wheel moves the cursor (a notch = an arrow press). The keys it pressed are let go the
/// next frame, so a menu that does not clear its keys still sees each notch.
fn menu_keys_and_wheel(
    state: Res<State<Screen>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    scroll: Option<Res<bevy::input::mouse::AccumulatedMouseScroll>>,
    motion: Option<Res<bevy::input::mouse::AccumulatedMouseMotion>>,
    mut mouse: ResMut<MenuMouse>,
    mut notches: Local<f32>,
    mut held: Local<Vec<KeyCode>>,
) {
    mouse.moved = motion.is_some_and(|m| m.delta != Vec2::ZERO);
    for k in held.drain(..) {
        keys.release(k);
    }
    if *state.get() == Screen::Playing {
        *notches = 0.0;
        return;
    }
    let mut press = |keys: &mut ButtonInput<KeyCode>, k: KeyCode| {
        if !keys.pressed(k) {
            keys.press(k);
            held.push(k);
        }
    };
    if keys.just_pressed(KeyCode::KeyE) {
        press(&mut keys, KeyCode::Enter);
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        press(&mut keys, KeyCode::Escape);
    }
    *notches += scroll.map_or(0.0, |s| match s.unit {
        bevy::input::mouse::MouseScrollUnit::Line => s.delta.y,
        bevy::input::mouse::MouseScrollUnit::Pixel => s.delta.y / 40.0,
    });
    if *notches >= 1.0 {
        *notches -= 1.0;
        press(&mut keys, KeyCode::ArrowUp);
    } else if *notches <= -1.0 {
        *notches += 1.0;
        press(&mut keys, KeyCode::ArrowDown);
    }
    *notches = notches.clamp(-3.0, 3.0);
}

/// A gamepad in the menus (plan C4) presses the menu's keys: d-pad / left stick (a tilt past
/// 0.5, once per tilt) the arrows, cross Enter, circle Esc (back / resume), Start Esc (pause /
/// resume, also while playing). The menu clears every key after reading it, and the pad's
/// buttons here.
fn pad_to_menu_keys(state: Res<State<Screen>>, config: Res<crate::config::GameConfig>, mut pads: Query<&mut Gamepad>, mut keys: ResMut<ButtonInput<KeyCode>>, mut tilted: Local<bool>) {
    if !config.input.gamepad {
        return;
    }
    let playing = *state.get() == Screen::Playing;
    for mut g in &mut pads {
        if g.just_pressed(GamepadButton::Start) {
            keys.press(KeyCode::Escape);
        }
        if playing {
            continue;
        }
        for (b, k) in [
            (GamepadButton::DPadUp, KeyCode::ArrowUp),
            (GamepadButton::DPadDown, KeyCode::ArrowDown),
            (GamepadButton::DPadLeft, KeyCode::ArrowLeft),
            (GamepadButton::DPadRight, KeyCode::ArrowRight),
            (GamepadButton::South, KeyCode::Enter),
            (GamepadButton::East, KeyCode::Escape),
        ] {
            if g.just_pressed(b) {
                keys.press(k);
            }
        }
        let st = g.left_stick();
        if st.length() < 0.5 {
            *tilted = false;
        } else if !*tilted {
            *tilted = true;
            keys.press(if st.x.abs() > st.y.abs() {
                if st.x > 0.0 { KeyCode::ArrowRight } else { KeyCode::ArrowLeft }
            } else if st.y > 0.0 {
                KeyCode::ArrowUp
            } else {
                KeyCode::ArrowDown
            });
        }
        // Nothing reaches the game: the cross that picks Start does not also jump.
        g.digital_mut().reset_all();
    }
}

/// `SHINOBI_FIGHT=<chr>[:<npc row>]`: starts that encounter through the picker's swap once the
/// game runs (a check of SpawnEncounter without the menu; e.g. c7100:71001000 Genichiro).
fn fight_from_env(mut done: Local<bool>, mut picker: ResMut<Picker>, mut pad: Option<ResMut<crate::player::PadInput>>, mut spawn: MessageWriter<crate::enemy::SpawnEncounter>) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    let Ok(v) = std::env::var("SHINOBI_FIGHT") else { return };
    let mut it = v.split(':');
    let chr = it.next().unwrap_or_default().to_string();
    let row = it.next().and_then(|r| r.parse::<i64>().ok());
    match picker.find(&chr, row) {
        Some(at) => {
            fight(&mut picker, at, &mut pad, &mut spawn);
        }
        None => {
            spawn.write(crate::enemy::SpawnEncounter { chr, npc_row: row, group: vec![] });
        }
    }
}

/// The picker's keys and clicks: up / down (W / S) one row, page up / down a page, left / right
/// (A / D, Tab) or a click the tab, Esc back to the title. True when the cursor's row is picked
/// (Enter / Space, or a click on it). Hovering does not move the cursor (the rows scroll under
/// a still mouse).
fn picker_input(
    keys: &ButtonInput<KeyCode>,
    items: &Query<(&MenuItem, &Interaction, &Children)>,
    tabs: &Query<(&PickerTab, &Interaction), Changed<Interaction>>,
    picker: &mut Picker,
    next: &mut NextState<Screen>,
) -> bool {
    let tab = picker.tab;
    let len = picker.lists()[tab].len();
    let any = |k: &[KeyCode]| k.iter().any(|k| keys.just_pressed(*k));
    let i = &mut picker.index[tab];
    if len > 0 {
        if any(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
            *i = (*i + 1) % len;
        }
        if any(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
            *i = (*i + len - 1) % len;
        }
        if any(&[KeyCode::PageDown]) {
            *i = (*i + PICKER_ROWS).min(len - 1);
        }
        if any(&[KeyCode::PageUp]) {
            *i = i.saturating_sub(PICKER_ROWS);
        }
    }
    if any(&[KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::KeyA, KeyCode::KeyD, KeyCode::Tab]) {
        picker.tab = 1 - picker.tab;
    }
    // Only a new press: the menu clears the mouse (reset_all), so the button's release is never
    // seen and a clicked tab stays Pressed.
    for (t, interaction) in tabs {
        if *interaction == Interaction::Pressed {
            picker.tab = t.0;
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(Screen::Title);
        return false;
    }
    for (item, interaction, _) in items {
        if *interaction == Interaction::Pressed && picker.tab == tab {
            picker.index[tab] = item.0;
            return true;
        }
    }
    len > 0 && picker.tab == tab && any(&[KeyCode::Enter, KeyCode::Space])
}

/// Playing -> Dead when Wolf lies in his death loop with no resurrection left (with one left the
/// game's own choice runs: attack revives, waiting starts over); -> Won once the finisher has played
/// out (`finisher_done`) after a boss's
/// defeat (BossScript run.defeated, the map event's Handle Boss Defeat: right across phases and
/// phantoms), or, in a fight without a boss script, once every enemy has been down for WON_HOLD.
fn watch_fight(
    time: Res<Time<Real>>,
    mut next: ResMut<NextState<Screen>>,
    wolf: Query<(&crate::actor::Actor, &crate::player::Player)>,
    enemies: Query<(&crate::enemy::Enemy, &crate::actor::Actor)>,
    boss: Res<crate::enemy::BossScript>,
    combat: Res<crate::data::Combat>,
    pad: Option<Res<crate::player::PadInput>>,
    banner_done: Res<BannerDone>,
    mut down_for: Local<f32>,
) {
    // Retry / Title -> Start asked for the fight over: the reset runs in the next fixed step
    // (player.rs), so until then Wolf still lies dead - not a new death.
    if pad.is_some_and(|p| p.reset) {
        *down_for = 0.0;
        return;
    }
    if let Ok((a, p)) = wolf.single() {
        if (a.state.starts_with("GroundDeathLoop") || a.state.starts_with("LandDeathLoop") || a.state == "ThrowDefDeathLoop") && p.resurrections == 0 || a.state == "AgingGroundDeathLoop" {
            next.set(Screen::Dead);
            return;
        }
    }
    let (all_down, hold) = match &boss.run {
        // Handle Boss Defeat is final: no hold for a phase change. With a banner, Won waits for
        // the banner to play out (`defeat_banner`, after the finisher).
        Some(run) if run.banner > 0 && run.defeated.is_some() && !banner_done.0 => return,
        Some(run) => (run.defeated.is_some(), 0.0),
        None => (!enemies.is_empty() && enemies.iter().all(|(e, _)| e.is_dead()), WON_HOLD),
    };
    *down_for = if all_down { *down_for + time.delta_secs() } else { 0.0 };
    let done = wolf.single().is_ok_and(|(a, _)| finisher_done(&combat, &a.state, enemies.iter()));
    if *down_for >= hold && all_down && (done || *down_for >= FINISHER_CAP) {
        *down_for = 0.0;
        next.set(Screen::Won);
    }
}

/// The defeat banner's fade in, full hold and fade out [s], measured on the all-bosses clip
/// (Gyoubu's 忍殺 at 12:16.8-12:20.1, 10 fps): 0.4 + 2.5 + 0.4.
const BANNER_IN: f32 = 0.4;
const BANNER_HOLD: f32 = 2.5;
const BANNER_SECS: f32 = BANNER_IN + BANNER_HOLD + 0.4;

/// The banner's alpha `t` s after it shows.
fn banner_alpha(t: f32) -> f32 {
    (t / BANNER_IN).min((BANNER_SECS - t) / 0.4).clamp(0.0, 1.0)
}

#[derive(Component)]
struct DefeatBanner;
#[derive(Component)]
struct DefeatBannerPart;

/// The defeat banner has played out (Won waits for it); cleared with the defeat.
#[derive(Resource, Default)]
pub(crate) struct BannerDone(pub bool);

/// The defeat's text banner over the play, with no pause (round 93): EMEVD 2003[12] / [74]
/// "Handle Boss Defeat and Display Banner" shows it at the defeat, while Wolf's finisher and the
/// boss's death play on; Won comes when they are done (`watch_fight`). Banner 1 = the brush
/// 忍殺 (MENU_Ninsatsu); 5 Immortality Severed and 3 Gracious Gift of Tears have no exported
/// brush art (gap): their names in the serif font.
fn defeat_banner(
    mut commands: Commands,
    time: Res<Time<Real>>,
    state: Res<State<Screen>>,
    boss: Res<crate::enemy::BossScript>,
    assets: Res<AssetServer>,
    fonts: Option<Res<MenuFonts>>,
    mut shown: Local<Option<(Entity, f32)>>,
    mut done: Local<bool>,
    mut banner_done: ResMut<BannerDone>,
    combat: Res<crate::data::Combat>,
    wolf: Query<&crate::actor::Actor, With<crate::player::Player>>,
    enemies: Query<(&crate::enemy::Enemy, &crate::actor::Actor)>,
    mut since_defeat: Local<f32>,
    mut images: Query<&mut ImageNode, With<DefeatBannerPart>>,
    mut texts: Query<&mut TextColor, With<DefeatBannerPart>>,
) {
    let banner = boss.run.as_ref().filter(|r| r.defeated.is_some()).map(|r| r.banner);
    if banner.is_none() {
        *done = false;
        banner_done.0 = false;
        *since_defeat = 0.0;
    }
    if let Some((e, t)) = shown.as_mut() {
        *t += time.delta_secs();
        let a = banner_alpha(*t);
        for mut i in &mut images {
            i.color.set_alpha(a);
        }
        for mut c in &mut texts {
            c.0.set_alpha(a);
        }
        if *t >= BANNER_SECS || *state.get() != Screen::Playing || banner.is_none() {
            banner_done.0 = banner.is_some();
            commands.entity(*e).try_despawn();
            *shown = None;
        }
        return;
    }
    let (Some(kind), false, Screen::Playing) = (banner, *done, state.get()) else { return };
    // The banner comes once the boss's death (and Wolf's finisher) has played out: the
    // all-bosses clip, Gyoubu: 9.5 s after the fatal deathblow, after his death animation.
    *since_defeat += time.delta_secs();
    let played_out = wolf.single().is_ok_and(|w| finisher_done(&combat, &w.state, enemies.iter()));
    if !played_out && *since_defeat < FINISHER_CAP {
        return;
    }
    *done = true;
    let (text, art) = match kind {
        5 => ("IMMORTALITY SEVERED", have("MENU_Fusigiri").then_some("MENU_Fusigiri")),
        3 => ("GRACIOUS GIFT OF TEARS", None),
        _ => ("SHINOBI EXECUTION", have("MENU_Ninsatsu").then_some("MENU_Ninsatsu")),
    };
    let serif = fonts.as_ref().and_then(|f| f.serif.clone());
    let e = commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, flex_direction: FlexDirection::Column, row_gap: Val::Vh(4.0), ..default() },
            GlobalZIndex(90),
            DefeatBanner,
        ))
        .with_children(|c| match art {
            // The clip: the brush kanji over y 24-52 %, the name in small spaced caps at y 59 %.
            Some(f) => {
                c.spawn((Node { height: Val::Vh(30.0), ..default() }, ImageNode::new(assets.load(format!("hud/{f}.png"))).with_color(Color::WHITE.with_alpha(0.0)), DefeatBannerPart));
                c.spawn((Text::new(text), font(&serif, 26.0), TextColor(Color::srgba(0.92, 0.88, 0.8, 0.0)), DefeatBannerPart));
            }
            None => {
                c.spawn((Text::new(text), font(&serif, 56.0), TextColor(RED.with_alpha(0.0)), DefeatBannerPart));
            }
        })
        .id();
    *shown = Some((e, 0.0));
}

/// The title (and the picker) shows only itself: every other UI layer (the HUD, its marks, the debug text) is
/// hidden while it is up and gets back the visibility its own system set when it leaves.
fn hide_hud_on_title(
    mut commands: Commands,
    state: Res<State<Screen>>,
    mut layers: Query<(Entity, &mut Visibility, Option<&mut TitleHidden>), (With<Node>, Without<ChildOf>, Without<MenuRoot>, Without<crate::idol::IdolMenuRoot>)>,
) {
    // The idol's menu shows over the world without the HUD too (the game's capture).
    let title = matches!(*state.get(), Screen::Title | Screen::Picker | Screen::Options | Screen::Idol);
    for (e, mut v, hidden) in &mut layers {
        match (title, hidden) {
            (true, Some(mut h)) => {
                if *v != Visibility::Hidden {
                    h.0 = *v;
                }
                *v = Visibility::Hidden;
            }
            (true, None) => {
                commands.entity(e).insert(TitleHidden(*v));
                *v = Visibility::Hidden;
            }
            (false, Some(h)) => {
                *v = h.0;
                commands.entity(e).remove::<TitleHidden>();
            }
            (false, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    /// The banner fades in over 0.4 s, holds 2.5 s and fades out over 0.4 s (the all-bosses clip).
    #[test]
    fn the_defeat_banner_fades_as_the_clip_does() {
        assert_eq!(super::banner_alpha(0.0), 0.0);
        assert!((super::banner_alpha(0.2) - 0.5).abs() < 1e-4);
        assert_eq!(super::banner_alpha(1.5), 1.0);
        assert_eq!(super::banner_alpha(2.9), 1.0);
        assert!((super::banner_alpha(3.1) - 0.5).abs() < 1e-4);
        assert!((super::BANNER_SECS - 3.3).abs() < 1e-4);
    }
    use super::*;

    /// A resting mouse does not undo the keys: the hovered row takes the cursor only when the
    /// hover begins; a click picks (round 17).
    #[test]
    fn a_resting_mouse_does_not_move_the_menu_cursor() {
        let mut cursor = 0;
        assert_eq!(mouse_pick(&mut cursor, [(0, Interaction::None, false), (2, Interaction::Hovered, true)].into_iter()), None);
        assert_eq!(cursor, 2, "a new hover moves the cursor");
        cursor = 1; // Up / Down
        assert_eq!(mouse_pick(&mut cursor, [(2, Interaction::Hovered, false)].into_iter()), None);
        assert_eq!(cursor, 1, "the resting mouse undid the key");
        assert_eq!(mouse_pick(&mut cursor, [(2, Interaction::Pressed, true)].into_iter()), Some(2));
        assert_eq!(cursor, 2);
    }
}
