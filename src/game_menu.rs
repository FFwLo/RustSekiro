//! The in-game menu (Esc, `Screen::Paused`): Sekiro's own layout from the user's captures of the
//! real game's menu (Equipment, Inventory's four categories, Skills' four, Options) and its own
//! art (01_common sb_in_game, cut by `sekiro-extract hud-all` to extracted/menu_sprites):
//! - the wooden board MENU_Base_00 over the whole window;
//! - the tabs Equipment / Inventory / Skills / Options on brush strokes, MENU_Caption_01 dark,
//!   MENU_Cursor_01 orange for the open one, switched with Z / X (their key-tops and the brush
//!   arrows MENU_Arrow either side) or a click;
//! - Equipment: the section bars (MENU_Caption_02) Prosthetic Tools (3 slots), Combat Arts,
//!   Quick Items (5), Ninjutsu, on the square frames MENU_IconBase_02 with the items' menu icons;
//! - Inventory and Skills: the list screen (`list_screen`): the category's name over its icons
//!   (MENU_TB_*, C / V), rows of an icon box and an ink plate (MENU_ItemBase_02; the cursor's the
//!   orange stroke MENU_Cursor_03), a scroll bar, and the parchment (MENU_WP_Base) with the name
//!   on a black bar, the big icon or the skill's story art, stat lines and the text;
//! - the status bar (resurrection nodes and Vitality, Unseen Aid, Vitality / Attack Power, the
//!   skill level, Spirit Emblems and Sen) and the key guide along the bottom;
//! - Options: the game's column of brush buttons.
//!
//! gap: the menu movie's (02_000_ingametop.gfx and kin) exact layout and animation are not
//! decoded: places and sizes are set from the captures. The lists show what the game state has
//! (no pickups yet); attribute levels, Unseen Aid and skill experience are not modelled.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::menu::Screen;

const TABS: [&str; 4] = ["Equipment", "Inventory", "Skills", "Options"];
/// The Options column: the game's rows this recreation has (the first four open the Options
/// screen), then Retry / Return to Title, which the game keeps elsewhere.
const OPTION_ITEMS: [&str; 7] = ["Game Options", "Camera Options", "Sound and Display", "Graphics Options", "Retry", "Return to Title", "Quit Game"];
pub(crate) const TEXT: Color = Color::srgb(0.88, 0.85, 0.78);
pub(crate) const DIM: Color = Color::srgba(0.75, 0.72, 0.66, 0.85);
pub(crate) const ORANGE: Color = Color::srgb(0.93, 0.6, 0.25);
/// Rows shown at once in a list screen.
const ROWS: usize = 6;

/// Inventory's categories: name, icon (SB_InGame MENU_TB_*).
const INV_CATS: [(&str, &str); 4] = [("Quick Items", "MENU_TB_Item"), ("Key Items", "MENU_TB_valuable"), ("Upgrade Materials", "MENU_TB_Material"), ("Prosthetic Arm Tools", "MENU_TB_Arm")];
/// Skills' categories. gap: the game's Combat Arts icon (a plain blade) is not among the cut
/// sprites; MENU_TB_Attack stands in.
const SKILL_CATS: [(&str, &str); 4] = [("Combat Arts", "MENU_TB_Attack"), ("Shinobi Martial Arts", "MENU_TB_Action"), ("Latent Skills", "MENU_TB_Sinobi"), ("Ninjutsu Techniques", "MENU_TB_Ninjyutu")];

/// Where the menu stands.
#[derive(Resource, Default)]
pub struct GameMenu {
    tab: usize,
    /// The cursor in the open tab (Equipment: slot 0-9; Inventory / Skills: row; Options: row).
    index: usize,
    /// The open category of Inventory and of Skills.
    cat: [usize; 2],
    /// The Equipment slot whose Equipped Items picker is open.
    pick: Option<usize>,
    built: Option<(usize, usize, [usize; 2], Option<usize>, u64)>,
}

/// The game's item texts (item.msgbnd), loaded once.
#[derive(Resource, Default)]
pub(crate) struct ItemTexts {
    pub(crate) weapon_name: HashMap<u32, String>,
    pub(crate) weapon_info: HashMap<u32, String>,
    pub(crate) goods_name: HashMap<u32, String>,
    pub(crate) goods_info: HashMap<u32, String>,
}

impl ItemTexts {
    fn load() -> Self {
        let dir = crate::paths::extracted().join("msg/engus/item.msgbnd.d");
        let read = |f: &str| std::fs::read(dir.join(f)).ok().map(|d| crate::action_guide::read_fmg(&d)).unwrap_or_default();
        ItemTexts { weapon_name: read("武器名.fmg"), weapon_info: read("武器説明.fmg"), goods_name: read("アイテム名.fmg"), goods_info: read("アイテム説明.fmg") }
    }

    /// The lowest goods / weapon id with this English name.
    pub(crate) fn goods_id(&self, name: &str) -> Option<i64> {
        self.goods_name.iter().filter(|(_, v)| v.as_str() == name).map(|(k, _)| *k as i64).min()
    }

    fn weapon_id(&self, name: &str) -> Option<i64> {
        self.weapon_name.iter().filter(|(_, v)| v.as_str() == name).map(|(k, _)| *k as i64).min()
    }
}

#[derive(Component)]
struct GameMenuRoot;

#[derive(Component)]
struct TabButton(usize);

#[derive(Component)]
pub(crate) struct RowButton(pub(crate) usize);

/// A list screen's category icon (Inventory / Skills; a click opens it).
#[derive(Component)]
pub(crate) struct CatButton(pub(crate) usize);

/// One Equipment slot: what it holds.
#[derive(Clone)]
enum Slot {
    Weapon(i64),
    Goods(i64, u32),
    Empty,
}

pub(crate) fn sprite(file: &str) -> Option<String> {
    let f = format!("menu_sprites/{file}.png");
    crate::paths::extracted().join(&f).exists().then_some(f)
}

/// A menu icon file for an iconId: the HUD's copy, else the menu atlases' (sb_icon*; the
/// memories' 750-756 sit in sb_boss).
pub(crate) fn any_icon(icon: i64) -> Option<String> {
    let hud = format!("hud/MENU_ItemIcon_{icon:05}.png");
    if crate::paths::extracted().join(&hud).exists() {
        return Some(hud);
    }
    ["sb_icon", "sb_icon_02", "sb_icon_03", "sb_icon_04", "sb_boss"].iter().find_map(|a| sprite(&format!("{a}/MENU_ItemIcon_{icon:05}")))
}

/// The Equipment slots in cursor order: tools 0-2, the art, quick items 0-4, ninjutsu.
fn slots(config: &crate::config::GameConfig) -> Vec<Slot> {
    let mut v = Vec::new();
    for i in 0..3 {
        v.push(config.player.prosthetics.get(i).map_or(Slot::Empty, |id| Slot::Weapon(*id)));
    }
    v.push(Slot::Weapon(config.player.combat_art));
    // Quick items: the Healing Gourd (goods 3000) with its charges, the rest empty.
    v.push(Slot::Goods(3000, config.player.gourd_charges));
    for _ in 0..4 {
        v.push(Slot::Empty);
    }
    v.push(Slot::Empty);
    v
}

fn slot_icon(tables: &Tables, s: &Slot) -> Option<String> {
    let id = match s {
        Slot::Weapon(id) => tables.weapon_icon.get(id).copied(),
        Slot::Goods(3000, _) => Some(151),
        Slot::Goods(id, _) => tables.goods_icon.get(id).copied(),
        Slot::Empty => None,
    }?;
    any_icon(id)
}

/// Name and description of a slot's item (a tool's upgrade levels share the base's text).
fn slot_text(texts: &ItemTexts, s: &Slot) -> (String, String) {
    let pick = |names: &HashMap<u32, String>, infos: &HashMap<u32, String>, id: i64| {
        let base = [id, id / 100 * 100, id / 1000 * 1000];
        let name = base.iter().find_map(|i| names.get(&(*i as u32))).cloned().unwrap_or_default();
        let info = base.iter().find_map(|i| infos.get(&(*i as u32))).cloned().unwrap_or_default();
        (name, info)
    };
    match s {
        Slot::Weapon(id) => pick(&texts.weapon_name, &texts.weapon_info, *id),
        Slot::Goods(id, _) => pick(&texts.goods_name, &texts.goods_info, *id),
        Slot::Empty => (String::new(), String::new()),
    }
}

/// One skill: SkillParam id and its virtual weapon (name, description, icon).
#[derive(Clone)]
struct Skill {
    id: i64,
    weapon: i64,
}

/// The params the menus read (extracted/json/params): the skills by Skills category, and the
/// weapon / goods iconIds. Loaded once.
#[derive(Resource, Default)]
pub(crate) struct Tables {
    skills: [Vec<Skill>; 4],
    pub(crate) weapon_icon: HashMap<i64, i64>,
    pub(crate) goods_icon: HashMap<i64, i64>,
}

impl Tables {
    fn load(texts: &ItemTexts) -> Self {
        let rows = |name: &str| -> Vec<serde_json::Value> {
            let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
            let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            v["rows"].as_array().cloned().unwrap_or_default()
        };
        let mut skills: [Vec<Skill>; 4] = Default::default();
        let mut seen = std::collections::HashSet::new();
        let mut all: Vec<Skill> = rows("SkillParam").iter().filter_map(|r| Some(Skill { id: r["id"].as_i64()?, weapon: r["virtualWeaponId"].as_i64()? })).collect();
        all.sort_by_key(|s| s.id);
        for s in all {
            // Ids 700+ are the prosthetic tools' upgrades (the Sculptor's tree, not a skill).
            if s.id >= 700 {
                continue;
            }
            let name = texts.weapon_name.get(&(s.weapon as u32)).cloned().unwrap_or_default();
            let info = texts.weapon_info.get(&(s.weapon as u32)).map(|i| i.to_lowercase()).unwrap_or_default();
            // The category from the text's first sentence ("A Combat Art that ...", "A Shinobi
            // Martial Art ...", "A Latent Skill ...", "Ninjutsu technique ..."; Mid-air Combat
            // Arts is "A Shinobi Martial Art that ... Combat Arts"). A skill on two tree pages
            // (Shadowrush, Ashina Cross ...) is listed once.
            let first = info.split('.').next().unwrap_or("");
            let cat = if first.contains("latent skill") {
                2
            } else if first.contains("ninjutsu") {
                3
            } else if first.contains("martial art") {
                1
            } else if first.contains("combat art") || info.contains("combat art") {
                0
            } else {
                continue;
            };
            if !name.is_empty() && seen.insert(name) {
                skills[cat].push(s);
            }
        }
        let icons = |name: &str| rows(name).iter().filter_map(|r| Some((r["id"].as_i64()?, r["iconId"].as_i64()?))).collect();
        Tables { skills, weapon_icon: icons("EquipParamWeapon"), goods_icon: icons("EquipParamGoods") }
    }
}

pub(crate) fn text(t: &str, px: f32, color: Color, font: &Option<Handle<Font>>) -> (Text, TextFont, TextColor, TextShadow) {
    (
        Text::new(t),
        TextFont { font: font.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(px), ..default() },
        TextColor(color),
        TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) },
    )
}

/// A sprite stretched over its box (the plates and brushes are laid out to the captures' boxes).
pub(crate) fn image(assets: &AssetServer, file: Option<String>) -> ImageNode {
    let mut i = file.map_or_else(ImageNode::default, |f| ImageNode::new(assets.load(f)));
    i.image_mode = bevy::ui::widget::NodeImageMode::Stretch;
    i
}

/// A box at (left, top) of (width, height), all in % of the window.
pub(crate) fn abs(l: f32, t: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Percent(l), top: Val::Percent(t), width: Val::Percent(w), height: Val::Percent(h), ..default() }
}

/// A full-size child laid out as a row (vertically centred).
pub(crate) fn fill_row() -> Node {
    Node { width: Val::Percent(100.0), height: Val::Percent(100.0), align_items: AlignItems::Center, ..default() }
}

/// One list row: icon, name, the number on the icon box, and the value at the plate's end (a
/// price).
#[derive(Clone, Default)]
pub(crate) struct Row {
    pub(crate) icon: Option<String>,
    pub(crate) name: String,
    pub(crate) count: Option<String>,
    pub(crate) value: Option<String>,
    /// The game's red equipped mark (MENU_Equipped) on the icon box.
    pub(crate) equipped: bool,
}

/// The parchment's content: name, the big icon (items) or the story art (skills), stat lines and
/// the text.
#[derive(Clone, Default)]
pub(crate) struct Detail {
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) art: Option<String>,
    pub(crate) stats: Vec<(String, String)>,
    pub(crate) info: String,
}

/// The list screen of Inventory / Skills (and the Sculptor's shop): the category name and icons
/// with C / V, the rows (a window of ROWS round the cursor), the scroll bar, the parchment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn list_screen(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, cats: &[(&str, &str)], cat: usize, rows: &[Row], index: usize, detail: Option<&Detail>) {
    if let Some((name, _)) = cats.get(cat) {
        root.spawn(abs(9.7, 14.0, 30.0, 4.0)).with_children(|c| {
            c.spawn(text(name, 17.0, TEXT, font));
        });
    }
    if !cats.is_empty() {
        root.spawn((abs(5.4, 19.2, 1.4, 4.4), image(assets, sprite("sb_in_game/MENU_Arrow"))));
        root.spawn((abs(7.4, 19.8, 1.6, 3.2), image(assets, sprite("sb_keytop/KG_Key_C"))));
        for (i, (_, icon)) in cats.iter().enumerate() {
            let l = 10.0 + i as f32 * 4.85;
            if i == cat {
                root.spawn((abs(l - 0.35, 17.7, 3.9, 7.2), image(assets, sprite("sb_in_game/MENU_Cursor_TB")).with_color(ORANGE)));
            }
            root.spawn((Button, abs(l, 18.3, 3.2, 6.0), image(assets, sprite(&format!("sb_in_game/{icon}"))), CatButton(i)));
        }
        let end = 10.0 + cats.len() as f32 * 4.85;
        root.spawn((abs(end, 19.8, 1.6, 3.2), image(assets, sprite("sb_keytop/KG_Key_V"))));
        let mut right = image(assets, sprite("sb_in_game/MENU_Arrow"));
        right.flip_x = true;
        root.spawn((abs(end + 2.1, 19.2, 1.4, 4.4), right));
    }
    let start = index.saturating_sub(ROWS - 1).min(rows.len().saturating_sub(ROWS));
    for (k, (i, row)) in rows.iter().enumerate().skip(start).take(ROWS).enumerate() {
        let sel = i == index;
        let t = 26.6 + k as f32 * 8.3;
        root.spawn((Button, abs(7.4, t, 4.2, 7.4), image(assets, sprite("sb_in_game/MENU_IconBase_02")), RowButton(i))).with_children(|b| {
            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(6.0), top: Val::Percent(6.0), width: Val::Percent(88.0), height: Val::Percent(88.0), ..default() }, image(assets, row.icon.clone())));
            if row.equipped {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(-6.0), top: Val::Percent(-6.0), width: Val::Percent(34.0), height: Val::Percent(34.0), ..default() }, image(assets, sprite("sb_in_game/MENU_Equipped"))));
            }
            if let Some(n) = &row.count {
                b.spawn(Node { position_type: PositionType::Absolute, right: Val::Percent(4.0), bottom: Val::Percent(0.0), ..default() }).with_children(|c| {
                    c.spawn(text(n, 13.0, Color::WHITE, font));
                });
            }
        });
        let plate = if sel { "sb_in_game/MENU_Cursor_03" } else { "sb_in_game/MENU_ItemBase_02" };
        root.spawn((Button, abs(12.3, t + 0.6, 36.6, 6.2), image(assets, sprite(plate)), RowButton(i))).with_children(|b| {
            b.spawn(Node { padding: UiRect::axes(Val::Percent(4.5), Val::Px(0.0)), justify_content: JustifyContent::SpaceBetween, ..fill_row() }).with_children(|c| {
                c.spawn(text(&row.name, 17.0, if sel { Color::WHITE } else { TEXT }, font));
                if let Some(v) = &row.value {
                    c.spawn(Node { column_gap: Val::Px(14.0), align_items: AlignItems::Center, margin: UiRect::right(Val::Percent(4.0)), ..default() }).with_children(|r| {
                        r.spawn((Node { height: Val::Px(18.0), width: Val::Px(18.0), ..default() }, image(assets, sprite("sb_fe/MENU_Money"))));
                        r.spawn(text(v, 16.0, TEXT, font));
                    });
                }
            });
        });
    }
    // The scroll bar (its thumb the shown share of the list).
    if rows.len() > ROWS {
        root.spawn((abs(50.0, 27.5, 0.9, 47.5), image(assets, sprite("sb_in_game/MENU_Scroll_Base"))));
        let share = ROWS as f32 / rows.len() as f32;
        root.spawn((abs(50.17, 27.5 + 47.5 * start as f32 / rows.len() as f32, 0.55, 47.5 * share), image(assets, sprite("sb_in_game/MENU_Scroll_Bar"))));
    }
    if let Some(d) = detail {
        parchment(root, assets, font, d);
    }
}

/// The parchment on the right: the name on its black bar, the big icon (items) or the story art
/// (skills), the stat lines on ink strokes, the text over faint rules.
pub(crate) fn parchment(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, d: &Detail) {
    root.spawn((abs(55.5, 14.4, 38.0, 62.3), image(assets, sprite("sb_in_game/MENU_WP_Base"))));
    root.spawn((abs(56.5, 15.6, 36.0, 4.4), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.25, 0.25, 0.25)))).with_children(|b| {
        b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
            c.spawn(text(&d.name, 17.0, Color::WHITE, font));
        });
    });
    let mut y = 22.0;
    if let Some(art) = &d.art {
        root.spawn((abs(61.0, 20.6, 27.0, 16.5), ImageNode::new(assets.load(art.clone()))));
        y = 38.0;
    } else if d.icon.is_some() {
        root.spawn((abs(81.3, 20.8, 9.4, 16.2), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|b| {
            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(8.0), top: Val::Percent(8.0), width: Val::Percent(84.0), height: Val::Percent(84.0), ..default() }, image(assets, d.icon.clone())));
        });
        y = 30.0;
    }
    for (label, value) in &d.stats {
        root.spawn((abs(57.0, y, 22.5, 3.6), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgba(0.5, 0.5, 0.5, 0.9)))).with_children(|b| {
            b.spawn(Node { justify_content: JustifyContent::SpaceBetween, padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..fill_row() }).with_children(|c| {
                c.spawn(text(label, 15.5, TEXT, font));
                c.spawn((Node { margin: UiRect::right(Val::Percent(12.0)), ..default() },)).with_children(|v| {
                    v.spawn(text(value, 15.5, TEXT, font));
                });
            });
        });
        y += 4.4;
    }
    y += 1.0;
    // Faint rules under the text lines (the game's ruled parchment).
    let mut r = y + 3.2;
    while r < 75.5 {
        root.spawn((abs(57.3, r, 34.5, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18))));
        r += 3.2;
    }
    root.spawn(abs(57.6, y, 34.0, 75.0 - y)).with_children(|c| {
        c.spawn(text(&d.info, 15.5, TEXT, font));
    });
}

/// The status bar along the bottom (all but Options): resurrection nodes over the Vitality bar,
/// Unseen Aid, Vitality / Attack Power, the skill level and its gauge, Spirit Emblems and Sen.
/// gap: attribute levels, Unseen Aid and skill experience are not modelled: Wolf's start values.
pub(crate) fn status_bar(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts, wolf: Option<(f32, f32)>) {
    let plate = Color::srgba(0.06, 0.06, 0.06, 0.78);
    let (t, h) = (79.2, 8.8);
    let (hp, hp_max) = wolf.unwrap_or((0.0, 1.0));
    // Resurrection and Vitality.
    root.spawn((abs(7.1, t, 19.1, h), BackgroundColor(plate))).with_children(|b| {
        for k in 0..config.player.resurrections.max(1) {
            let lit = k < config.player.resurrections;
            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(14.0 + k as f32 * 22.0), top: Val::Px(6.0), width: Val::Px(18.0), height: Val::Px(18.0), ..default() }, image(assets, sprite("sb_fe/MENU_Revive_Gauge")).with_color(if lit { Color::WHITE } else { Color::srgba(0.4, 0.4, 0.4, 0.6) })));
        }
        b.spawn((abs(5.0, 50.0, 90.0, 9.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7))));
        b.spawn((abs(5.0, 50.0, 90.0 * (hp / hp_max.max(1.0)).clamp(0.0, 1.0), 9.0), BackgroundColor(Color::srgb(0.62, 0.17, 0.12))));
    });
    // Unseen Aid (gap: no Dragonrot; the game's base 30 %).
    root.spawn((abs(27.1, t, 15.2, h), BackgroundColor(plate))).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, padding: UiRect::left(Val::Percent(30.0)), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }).with_children(|c| {
            c.spawn(text("Unseen Aid", 16.5, TEXT, font));
            c.spawn(text("30%", 16.5, TEXT, font));
        });
    });
    // Vitality / Attack Power.
    root.spawn((abs(43.4, t, 15.2, h), BackgroundColor(plate))).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: Val::Px(2.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..default() }).with_children(|c| {
            for (icon, label, value) in [("MENU_ST_Vitality", "Vitality", format!("{:.0}", config.posture.player_level)), ("MENU_ST_Offensive", "Attack Power", "1".to_string())] {
                c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                    r.spawn((Node { width: Val::Px(20.0), height: Val::Px(20.0), ..default() }, image(assets, sprite(&format!("sb_in_game/{icon}")))));
                    r.spawn((Node { flex_grow: 1.0, ..default() },)).with_children(|n| {
                        n.spawn(text(label, 16.0, TEXT, font));
                    });
                    r.spawn(text(&value, 16.0, TEXT, font));
                });
            }
        });
    });
    // The skill level and the gauge to the next point.
    root.spawn((abs(59.5, t, 20.0, h), BackgroundColor(plate))).with_children(|b| {
        b.spawn((abs(2.0, 12.0, 11.0, 40.0), image(assets, sprite("sb_fe/MENU_Skill_Lv")).with_color(Color::srgb(0.15, 0.15, 0.15)))).with_children(|d| {
            d.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                c.spawn(text(&config.player.skill_points.to_string(), 14.0, Color::srgb(0.55, 0.85, 0.9), font));
            });
        });
        b.spawn((abs(14.0, 25.0, 60.0, 12.0), image(assets, sprite("sb_in_game/MENU_GaugeFrame"))));
        b.spawn((Node { position_type: PositionType::Absolute, right: Val::Percent(5.0), bottom: Val::Percent(14.0), ..default() },)).with_children(|c| {
            c.spawn(text("to next Skill Point  -", 14.0, Color::srgb(0.55, 0.85, 0.9), font));
        });
    });
    // Spirit Emblems and Sen.
    let emblem = texts.goods_id("Spirit Emblem").and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
    root.spawn((abs(80.5, t, 12.8, h), BackgroundColor(plate))).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: Val::Px(2.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..default() }).with_children(|c| {
            let n = config.player.spirit_emblems;
            for (icon, value) in [(emblem, format!("{n}/{n}")), (sprite("sb_fe/MENU_Money"), config.player.sen.to_string())] {
                c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() }).with_children(|r| {
                    r.spawn((Node { width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, image(assets, icon)));
                    r.spawn((Node { flex_grow: 1.0, justify_content: JustifyContent::FlexEnd, ..default() },)).with_children(|n| {
                        n.spawn(text(&value, 16.0, TEXT, font));
                    });
                });
            }
        });
    });
}

/// Inventory's rows and details for a category: Quick Items (the Healing Gourd, the Homeward
/// Idol), Key Items (Kusabimaru, the Shinobi Prosthetic), Upgrade Materials (none yet), Prosthetic
/// Arm Tools (those fitted). gap: no pickups, so only what Wolf starts with.
fn inventory(cat: usize, config: &crate::config::GameConfig, texts: &ItemTexts, tables: &Tables) -> Vec<(Row, Detail)> {
    let goods = |id: i64, count: Option<u32>, stats: Vec<(String, String)>| {
        let (name, info) = slot_text(texts, &Slot::Goods(id, 0));
        let icon = if id == 3000 { any_icon(151) } else { tables.goods_icon.get(&id).and_then(|i| any_icon(*i)) };
        (Row { icon: icon.clone(), name: name.clone(), count: count.map(|n| n.to_string()), ..default() }, Detail { name, icon, art: None, stats, info })
    };
    let weapon = |id: i64, stats: Vec<(String, String)>| {
        let (name, info) = slot_text(texts, &Slot::Weapon(id));
        let icon = tables.weapon_icon.get(&id).or_else(|| tables.weapon_icon.get(&(id / 100 * 100))).and_then(|i| any_icon(*i));
        (Row { icon: icon.clone(), name: name.clone(), ..default() }, Detail { name, icon, art: None, stats, info })
    };
    let held = |n: u32, of: u32| ("Number Held".to_string(), format!("{n}/ {of}"));
    let storage = || ("In Storage".to_string(), "-".to_string());
    match cat {
        0 => {
            let g = config.player.gourd_charges;
            let mut v = vec![goods(3000, Some(g), vec![("Uses".to_string(), format!("{g}/{g}")), storage()])];
            if let Some(id) = texts.goods_id("Homeward Idol") {
                v.push(goods(id, None, vec![]));
            }
            v
        }
        1 => {
            let mut v = Vec::new();
            if let Some(id) = texts.weapon_id("Kusabimaru") {
                v.push(weapon(id, vec![held(1, 1), storage()]));
            }
            if let Some(id) = texts.goods_id("Shinobi Prosthetic") {
                v.push(goods(id, None, vec![held(1, 1), storage()]));
            }
            v
        }
        2 => Vec::new(),
        _ => config.player.prosthetics.iter().map(|id| weapon(*id, vec![])).collect(),
    }
}

/// Skills' rows and details for a category: the unlocked skills (config player.skills; with
/// unlock_all_moves, as the moves are, all of them), each with its story art (sb_icon_skill
/// MENU_SkillStory_<iconId>).
fn skills(cat: usize, config: &crate::config::GameConfig, texts: &ItemTexts, tables: &Tables) -> Vec<(Row, Detail)> {
    tables.skills[cat]
        .iter()
        .filter(|s| config.player.unlock_all_moves || config.player.skills.contains(&s.id))
        .map(|s| {
            let name = texts.weapon_name.get(&(s.weapon as u32)).cloned().unwrap_or_default();
            let info = texts.weapon_info.get(&(s.weapon as u32)).cloned().unwrap_or_default();
            let icon_id = tables.weapon_icon.get(&s.weapon).copied();
            let icon = icon_id.and_then(any_icon);
            let art = icon_id.and_then(|i| sprite(&format!("sb_icon_skill/MENU_SkillStory_{i:05}")).or_else(|| sprite(&format!("sb_icon_skill_02/MENU_SkillStory_{i:05}"))));
            (Row { icon: icon.clone(), name: name.clone(), ..default() }, Detail { name, icon, art, stats: vec![], info })
        })
        .collect()
}

/// The Equipped Items picker (Enter on an Equipment slot): what can go in the slot. Tools: the
/// prosthetic tools the combat data has (combat player.prosthetics, upgrades included); the art:
/// the combat arts (EquipParamWeapon 5100-7700 with a name); quick items: the Healing Gourd and
/// the Homeward Idol; Ninjutsu: none yet.
fn equip_choices(slot: usize, combat: &crate::data::Combat, texts: &ItemTexts) -> Vec<Slot> {
    match slot {
        0..=2 => {
            let mut ids: Vec<i64> = combat.player.prosthetics.iter().map(|t| t.id).collect();
            ids.sort();
            ids.dedup();
            ids.into_iter().filter(|id| texts.weapon_name.contains_key(&(*id as u32))).map(Slot::Weapon).collect()
        }
        3 => (5100..=7700).step_by(100).filter(|id| texts.weapon_name.contains_key(&(*id as u32))).map(Slot::Weapon).collect(),
        4..=8 => {
            let mut v = vec![Slot::Goods(3000, 0)];
            if let Some(id) = texts.goods_id("Homeward Idol") {
                v.push(Slot::Goods(id, 0));
            }
            v
        }
        _ => Vec::new(),
    }
}

/// Is this choice equipped (in any slot of its kind)?
fn is_equipped(slot: usize, choice: &Slot, config: &crate::config::GameConfig) -> bool {
    match (slot, choice) {
        (0..=2, Slot::Weapon(id)) => config.player.prosthetics.contains(id),
        (3, Slot::Weapon(id)) => config.player.combat_art == *id,
        (4..=8, Slot::Goods(3000, _)) => true,
        _ => false,
    }
}

/// Enter on a choice: equip it in the slot, or take it off when it is the slot's own. Tools keep
/// up to three, one of a kind; the art slot always holds one.
fn equip(slot: usize, choice: &Slot, config: &mut crate::config::GameConfig) {
    match (slot, choice) {
        (0..=2, Slot::Weapon(id)) => {
            let tools = &mut config.player.prosthetics;
            if tools.get(slot) == Some(id) {
                tools.remove(slot);
            } else {
                tools.retain(|t| t != id);
                if slot < tools.len() {
                    tools[slot] = *id;
                } else {
                    tools.push(*id);
                }
                tools.truncate(3);
            }
        }
        (3, Slot::Weapon(id)) => config.player.combat_art = *id,
        _ => {}
    }
}

/// The picker's rows and details.
fn equip_rows(slot: usize, config: &crate::config::GameConfig, combat: &crate::data::Combat, texts: &ItemTexts, tables: &Tables) -> Vec<(Row, Detail)> {
    equip_choices(slot, combat, texts)
        .iter()
        .map(|c| {
            let (name, info) = slot_text(texts, c);
            let icon = slot_icon(tables, c);
            let art = match c {
                Slot::Weapon(id) if slot == 3 => tables.weapon_icon.get(id).and_then(|i| sprite(&format!("sb_icon_skill/MENU_SkillStory_{i:05}")).or_else(|| sprite(&format!("sb_icon_skill_02/MENU_SkillStory_{i:05}")))),
                _ => None,
            };
            let count = match c {
                Slot::Goods(3000, _) => Some(config.player.gourd_charges.to_string()),
                _ => None,
            };
            (Row { icon: icon.clone(), name: name.clone(), count, value: None, equipped: is_equipped(slot, c, config) }, Detail { name, icon, art, stats: vec![], info })
        })
        .collect()
}

/// A sub-screen's title on its black brush (SB_InGame MENU_ItemBase_07): "Equipped Items",
/// "Purchase", "Travel" ...
pub(crate) fn title_bar(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, title: &str) {
    root.spawn((abs(5.6, 5.6, 55.0, 8.4), image(assets, sprite("sb_in_game/MENU_ItemBase_07")))).with_children(|b| {
        b.spawn(Node { padding: UiRect::left(Val::Percent(3.5)), ..fill_row() }).with_children(|c| {
            c.spawn(text(title, 24.0, Color::WHITE, font));
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn build(commands: &mut Commands, assets: &AssetServer, menu: &GameMenu, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, combat: &crate::data::Combat, texts: &ItemTexts, tables: &Tables, wolf: Option<(f32, f32)>) {
    if let Some(slot) = menu.pick {
        commands
            .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), GameMenuRoot))
            .with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Equipped Items");
                let list = equip_rows(slot, config, combat, texts, tables);
                let rows: Vec<Row> = list.iter().map(|(r, _)| r.clone()).collect();
                list_screen(root, assets, font, &[], 0, &rows, menu.index, list.get(menu.index).map(|(_, d)| d));
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text(if rows.is_empty() { "Nothing to equip here yet." } else { "Choose item to equip." }, 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":Equip/Remove  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        return;
    }
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(110),
            GameMenuRoot,
        ))
        .with_children(|root| {
            root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
            // Tabs, with the switch keys and the brush arrows at either end.
            root.spawn((abs(5.3, 6.0, 1.3, 4.6), image(assets, sprite("sb_in_game/MENU_Arrow"))));
            root.spawn((abs(7.3, 6.4, 1.8, 3.4), image(assets, sprite("sb_keytop/KG_Key_Z"))));
            root.spawn((abs(90.0, 6.4, 1.8, 3.4), image(assets, sprite("sb_keytop/KG_Key_X"))));
            let mut right = image(assets, sprite("sb_in_game/MENU_Arrow"));
            right.flip_x = true;
            root.spawn((abs(92.4, 6.0, 1.3, 4.6), right));
            for (i, name) in TABS.iter().enumerate() {
                let open = i == menu.tab;
                let brush = if open { "sb_in_game/MENU_Cursor_01" } else { "sb_in_game/MENU_Caption_01" };
                root.spawn((Button, abs(11.5 + i as f32 * 19.0, 4.2, 18.0, 9.0), image(assets, sprite(brush)), TabButton(i))).with_children(|t| {
                    t.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                        c.spawn(text(name, 18.0, if open { Color::WHITE } else { TEXT }, font));
                    });
                });
            }
            // The rule under the tabs.
            root.spawn((abs(7.0, 13.6, 86.0, 0.25), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))));
            match menu.tab {
                0 => equipment(root, assets, menu, font, config, tables, texts),
                1 | 2 => {
                    let k = menu.tab - 1;
                    let (cats, list) = if menu.tab == 1 { (&INV_CATS, inventory(menu.cat[k], config, texts, tables)) } else { (&SKILL_CATS, skills(menu.cat[k], config, texts, tables)) };
                    let rows: Vec<Row> = list.iter().map(|(r, _)| r.clone()).collect();
                    let detail = list.get(menu.index).map(|(_, d)| d);
                    list_screen(root, assets, font, cats, menu.cat[k], &rows, menu.index, detail);
                }
                _ => {
                    for (i, label) in OPTION_ITEMS.iter().enumerate() {
                        let sel = i == menu.index;
                        let plate = if sel { "sb_in_game/MENU_Cursor_01" } else { "sb_in_game/MENU_Caption_01" };
                        root.spawn((Button, abs(6.8, 18.0 + i as f32 * 7.55, 22.2, 6.4), image(assets, sprite(plate)), RowButton(i))).with_children(|b| {
                            b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                                c.spawn(text(label, 17.0, if sel { Color::WHITE } else { TEXT }, font));
                            });
                        });
                    }
                }
            }
            if menu.tab != 3 {
                status_bar(root, assets, font, config, tables, texts, wolf);
            }
            // The key guide: help on the left, the keys on the right.
            let help = match menu.tab {
                0 => "Replace and confirm equipped items.",
                1 => "Check and use items in your possession.",
                2 => "View unlocked Skills",
                _ => "Check and make changes to various settings.",
            };
            root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                c.spawn(text(help, 16.0, DIM, font));
            });
            key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Close")]);
        });
}

/// The keys at the bottom right: (key-top sprite, label).
pub(crate) fn key_guide(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, keys: &[(&str, &str)]) {
    root.spawn((Node { position_type: PositionType::Absolute, right: Val::Percent(5.3), top: Val::Percent(91.4), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(4.0), ..default() },)).with_children(|r| {
        for (key, label) in keys {
            r.spawn((Node { height: Val::Px(20.0), width: Val::Px(30.0), ..default() }, image(assets, sprite(key))));
            r.spawn(text(label, 16.0, DIM, font));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn equipment(root: &mut ChildSpawnerCommands, assets: &AssetServer, menu: &GameMenu, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts) {
    let all = slots(config);
    // Section bars.
    for (label, l, t) in [("Prosthetic Tools", 6.5, 16.5), ("Combat Arts", 38.5, 16.5), ("Quick Items", 6.5, 45.0), ("Ninjutsu", 38.5, 45.0)] {
        root.spawn((abs(l, t, 17.0, 4.5), image(assets, sprite("sb_in_game/MENU_Caption_02")))).with_children(|b| {
            b.spawn(Node { padding: UiRect::left(Val::Percent(6.0)), ..fill_row() }).with_children(|c| {
                c.spawn(text(label, 17.0, TEXT, font));
            });
        });
    }
    // The slots: (index, left %, top %, size % of height).
    let mut places: Vec<(usize, f32, f32, f32)> = Vec::new();
    for i in 0..3 {
        places.push((i, 7.0 + i as f32 * 10.2, 22.5, 15.0));
    }
    places.push((3, 39.5, 22.5, 15.0));
    for i in 0..5 {
        places.push((4 + i, 7.0 + i as f32 * 6.2, 51.0, 10.0));
    }
    places.push((9, 39.5, 51.0, 15.0));
    for (i, l, t, size) in places {
        let w = size * 9.0 / 16.0;
        let slot = &all[i];
        root.spawn((Button, abs(l, t, w, size), image(assets, sprite("sb_in_game/MENU_IconBase_02")), RowButton(i))).with_children(|s| {
            // The cursor glows behind the item, not over it.
            if i == menu.index {
                s.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(-8.0), top: Val::Percent(-8.0), width: Val::Percent(116.0), height: Val::Percent(116.0), ..default() }, image(assets, sprite("sb_in_game/MENU_Cursor_TB")).with_color(ORANGE.with_alpha(0.55))));
            }
            if let Some(icon) = slot_icon(tables, slot) {
                s.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(4.0), top: Val::Percent(4.0), width: Val::Percent(92.0), height: Val::Percent(92.0), ..default() }, ImageNode::new(assets.load(icon))));
            }
            if let Slot::Goods(_, n) = slot {
                s.spawn((Node { position_type: PositionType::Absolute, right: Val::Percent(6.0), bottom: Val::Percent(2.0), ..default() },)).with_children(|c| {
                    c.spawn(text(&n.to_string(), 16.0, Color::WHITE, font));
                });
            }
        });
    }
    // The item under the cursor on the parchment.
    let slot = &all[menu.index.min(all.len() - 1)];
    let (name, info) = slot_text(texts, slot);
    parchment(root, assets, font, &Detail { name, icon: slot_icon(tables, slot), art: None, stats: vec![], info });
}

#[derive(Resource, Default)]
pub(crate) struct MenuFont(pub(crate) Option<Handle<Font>>);

fn load_font(mut fonts: ResMut<Assets<Font>>, mut f: ResMut<MenuFont>) {
    f.0 = std::fs::read("C:/Windows/Fonts/georgia.ttf").ok().map(|b| fonts.add(Font::from_bytes(b)));
}

#[allow(clippy::too_many_arguments)]
fn run_game_menu(
    mut commands: Commands,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut menu: ResMut<GameMenu>,
    roots: Query<Entity, With<GameMenuRoot>>,
    (tabs, rows, cats): (Query<(&TabButton, &Interaction), Changed<Interaction>>, Query<(&RowButton, &Interaction), Changed<Interaction>>, Query<(&CatButton, &Interaction), Changed<Interaction>>),
    (assets, font, mut config, texts, tables, combat): (Res<AssetServer>, Res<MenuFont>, ResMut<crate::config::GameConfig>, Res<ItemTexts>, Res<Tables>, Res<crate::data::Combat>),
    wolf: Query<&crate::actor::Actor, With<crate::player::Player>>,
    (mut pad, mut exit, mut cursor): (Option<ResMut<crate::player::PadInput>>, MessageWriter<AppExit>, ResMut<crate::menu::Cursor>),
) {
    if *state.get() != Screen::Paused {
        for r in &roots {
            commands.entity(r).despawn();
        }
        menu.built = None;
        menu.pick = None;
        return;
    }
    let any = |k: &[KeyCode]| k.iter().any(|k| keys.just_pressed(*k));
    let mut confirm = any(&[KeyCode::Enter, KeyCode::Space]);
    // The Equipped Items picker: up / down, Enter equips or removes, Esc back to its slot.
    if let Some(slot) = menu.pick {
        let choices = equip_choices(slot, &combat, &texts);
        let n = choices.len().max(1);
        if any(&[KeyCode::ArrowDown, KeyCode::KeyS, KeyCode::ArrowRight, KeyCode::KeyD]) {
            menu.index = (menu.index + 1) % n;
        }
        if any(&[KeyCode::ArrowUp, KeyCode::KeyW, KeyCode::ArrowLeft, KeyCode::KeyA]) {
            menu.index = (menu.index + n - 1) % n;
        }
        for (r, i) in &rows {
            match i {
                Interaction::Pressed => {
                    menu.index = r.0;
                    confirm = true;
                }
                Interaction::Hovered => menu.index = r.0,
                Interaction::None => {}
            }
        }
        let back = keys.just_pressed(KeyCode::Escape);
        keys.reset_all();
        mouse.reset_all();
        if confirm {
            if let Some(c) = choices.get(menu.index) {
                equip(slot, c, &mut config);
            }
        }
        if back {
            menu.pick = None;
            menu.index = slot;
        }
    } else {
    if any(&[KeyCode::KeyX]) {
        menu.tab = (menu.tab + 1) % TABS.len();
        menu.index = 0;
    }
    if any(&[KeyCode::KeyZ]) {
        menu.tab = (menu.tab + TABS.len() - 1) % TABS.len();
        menu.index = 0;
    }
    for (t, i) in &tabs {
        if *i == Interaction::Pressed {
            menu.tab = t.0;
            menu.index = 0;
        }
    }
    // Inventory / Skills: C / V the category.
    if menu.tab == 1 || menu.tab == 2 {
        let k = menu.tab - 1;
        if any(&[KeyCode::KeyV]) {
            menu.cat[k] = (menu.cat[k] + 1) % 4;
            menu.index = 0;
        }
        if any(&[KeyCode::KeyC]) {
            menu.cat[k] = (menu.cat[k] + 3) % 4;
            menu.index = 0;
        }
        for (c, i) in &cats {
            if *i == Interaction::Pressed {
                menu.cat[k] = c.0;
                menu.index = 0;
            }
        }
    }
    for (r, i) in &rows {
        match i {
            Interaction::Pressed => {
                menu.index = r.0;
                confirm = true;
            }
            Interaction::Hovered => menu.index = r.0,
            Interaction::None => {}
        }
    }
    let n = match menu.tab {
        0 => 10,
        1 => inventory(menu.cat[0], &config, &texts, &tables).len(),
        2 => skills(menu.cat[1], &config, &texts, &tables).len(),
        _ => OPTION_ITEMS.len(),
    }
    .max(1);
    if any(&[KeyCode::ArrowRight, KeyCode::KeyD, KeyCode::ArrowDown, KeyCode::KeyS]) {
        menu.index = (menu.index + 1) % n;
    }
    if any(&[KeyCode::ArrowLeft, KeyCode::KeyA, KeyCode::ArrowUp, KeyCode::KeyW]) {
        menu.index = (menu.index + n - 1) % n;
    }
    let close = keys.just_pressed(KeyCode::Escape);
    keys.reset_all();
    mouse.reset_all();
    if close {
        next.set(Screen::Playing);
        return;
    }
    if confirm && menu.tab == 0 {
        let slot = menu.index;
        menu.index = equip_choices(slot, &combat, &texts).iter().position(|c| is_equipped(slot, c, &config) && !matches!(c, Slot::Goods(..))).unwrap_or(0);
        menu.pick = Some(slot);
    }
    if confirm && menu.tab == 3 {
        match OPTION_ITEMS[menu.index.min(OPTION_ITEMS.len() - 1)] {
            "Retry" | "Return to Title" => {
                if let Some(p) = pad.as_mut() {
                    p.reset = true;
                }
                next.set(if OPTION_ITEMS[menu.index] == "Retry" { Screen::Playing } else { Screen::Title });
            }
            "Quit Game" => {
                exit.write(AppExit::Success);
            }
            _ => {
                cursor.options_from = Some(Screen::Paused);
                next.set(Screen::Options);
            }
        }
        return;
    }
    }
    // Rebuilt when the place or the equipment changes.
    let gear = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&config.player.prosthetics, config.player.combat_art).hash(&mut h);
        h.finish()
    };
    let key = (menu.tab, menu.index, menu.cat, menu.pick, gear);
    if menu.built != Some(key) {
        for r in &roots {
            commands.entity(r).despawn();
        }
        let w = wolf.single().ok().map(|a| (a.hp, a.hp_max));
        build(&mut commands, &assets, &menu, &font.0, &config, &combat, &texts, &tables, w);
        menu.built = Some(key);
    }
}

pub struct GameMenuPlugin;

impl Plugin for GameMenuPlugin {
    fn build(&self, app: &mut App) {
        // SHINOBI_MENU_TAB=<0-3> / SHINOBI_MENU_CAT=<0-3>: open on that tab / category (for
        // SHINOBI_SHOT captures with SHINOBI_TITLE=paused).
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let (tab, cat) = (env("SHINOBI_MENU_TAB") % TABS.len(), env("SHINOBI_MENU_CAT") % 4);
        let texts = ItemTexts::load();
        let tables = Tables::load(&texts);
        // SHINOBI_MENU_PICK=<slot>: open on that Equipment slot's Equipped Items.
        let pick = std::env::var("SHINOBI_MENU_PICK").ok().and_then(|v| v.parse::<usize>().ok());
        app.insert_resource(GameMenu { tab, cat: [cat, cat], pick, ..default() })
            .init_resource::<MenuFont>()
            .insert_resource(texts)
            .insert_resource(tables)
            .add_systems(Startup, load_font)
            .add_systems(PreUpdate, run_game_menu.after(crate::menu::MenuInput));
    }
}
