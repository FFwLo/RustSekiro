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
        Screen::Playing | Screen::Picker | Screen::Options => vec![],
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
    Back,
}

const OPTS: [Opt; 10] = [Opt::CameraSpeed, Opt::InvertY, Opt::Gamepad, Opt::Volume, Opt::Music, Opt::Se, Opt::Voice, Opt::Fullscreen, Opt::Vsync, Opt::Back];

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
            let bosses: Vec<_> = all.iter().filter(|e| e.bar).cloned().collect();
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

/// Seconds every enemy has to stay down before 忍殺 (a boss between phases is down for a moment).
/// gap: hand-set, the game's own timing is not traced.
const WON_HOLD: f32 = 4.0;

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
struct Cursor {
    index: usize,
    built: Option<Screen>,
    /// The screen Options goes back to (Title or Paused).
    options_from: Option<Screen>,
    /// Options rebuilds its overlay after a change.
    dirty: bool,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let automated = std::env::vars().any(|(k, _)| k.starts_with("SHINOBI_") && !matches!(k.as_str(), "SHINOBI_MAP" | "SHINOBI_HOUR" | "SHINOBI_ROOT" | "SHINOBI_TITLE") && !k.starts_with("SHINOBI_MAP_"));
        let start = match std::env::var("SHINOBI_TITLE").ok().as_deref() {
            Some("paused") => Screen::Paused,
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
            .add_systems(Startup, load_fonts)
            .add_systems(PreUpdate, (pad_to_menu_keys, run_menu).chain().after(UiSystems::Focus).after(InputSystems))
            .add_systems(Update, (fight_from_env, apply_video, watch_fight.run_if(in_state(Screen::Playing))))
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
    let list = choices(s, picker);
    let (mark, sub, mark_color, shade) = match s {
        Screen::Title => ("Sv1", "Sv1 · a Sekiro combat recreation", GOLD, 0.55),
        Screen::Paused => ("Paused", "", GOLD, 0.45),
        Screen::Dead => ("死", "DEATH", RED, 0.35),
        Screen::Won => ("忍殺", "SHINOBI EXECUTION", RED, 0.25),
        Screen::Playing | Screen::Picker | Screen::Options => return,
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
    (items, tabs): (Query<(&MenuItem, &Interaction, &Children)>, Query<(&PickerTab, &Interaction)>),
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
        if s == Screen::Playing {
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
    if s != Screen::Playing && !time.is_paused() && real.elapsed_secs() >= WARMUP {
        time.pause();
    }
    if s == Screen::Playing {
        if keys.just_pressed(KeyCode::Escape) {
            next.set(Screen::Paused);
            keys.reset_all();
        }
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
    for (item, interaction, _) in &items {
        match interaction {
            Interaction::Hovered => cursor.index = item.0,
            Interaction::Pressed => {
                cursor.index = item.0;
                pick = list.get(item.0).copied();
            }
            Interaction::None => {}
        }
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
    tabs: &Query<(&PickerTab, &Interaction)>,
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
/// game's own choice runs: attack revives, waiting starts over); -> Won WON_HOLD after a boss's
/// defeat (BossScript run.defeated, the map event's Handle Boss Defeat: right across phases and
/// phantoms), or, in a fight without a boss script, once every enemy has been down for WON_HOLD.
fn watch_fight(
    time: Res<Time<Real>>,
    mut next: ResMut<NextState<Screen>>,
    wolf: Query<(&crate::actor::Actor, &crate::player::Player)>,
    enemies: Query<&crate::enemy::Enemy>,
    boss: Res<crate::enemy::BossScript>,
    mut down_for: Local<f32>,
) {
    if let Ok((a, p)) = wolf.single() {
        if a.state == "GroundDeathLoop_F" && p.resurrections == 0 {
            next.set(Screen::Dead);
            return;
        }
    }
    let all_down = match &boss.run {
        Some(run) => run.defeated.is_some(),
        None => !enemies.is_empty() && enemies.iter().all(|e| e.is_dead()),
    };
    *down_for = if all_down { *down_for + time.delta_secs() } else { 0.0 };
    if *down_for >= WON_HOLD {
        *down_for = 0.0;
        next.set(Screen::Won);
    }
}

/// The title (and the picker) shows only itself: every other UI layer (the HUD, its marks, the debug text) is
/// hidden while it is up and gets back the visibility its own system set when it leaves.
fn hide_hud_on_title(
    mut commands: Commands,
    state: Res<State<Screen>>,
    mut layers: Query<(Entity, &mut Visibility, Option<&mut TitleHidden>), (With<Node>, Without<ChildOf>, Without<MenuRoot>)>,
) {
    let title = matches!(*state.get(), Screen::Title | Screen::Picker | Screen::Options);
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
