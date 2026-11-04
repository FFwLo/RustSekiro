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
//! Places, sizes and text heights are the menu movie's, menu/02_200_mainmenu.gfx (menu_layout.rs:
//! the tabs, the Equipment sections and slots, the list rows, category icons and scroll bar, the
//! details parchment, the Options rows, the status plates, the help line and key guide).
//!
//! gap: the movie's animations are not played; the right-hand arrows, the skill art's box, the
//! status plates' contents and the Equipped Items title bar are still by eye. The lists show what
//! the game state has (no pickups yet); attribute levels, Unseen Aid and skill experience are not
//! modelled.

use std::collections::HashMap;

use bevy::prelude::*;

pub(crate) use crate::hud::{any_icon, sprite};
use crate::hud::atlas_art;
use crate::hud_layout::{StageRect, StageText};
use crate::menu::Screen;
use crate::menu_layout::{within, MenuLayout};

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
    /// The first list row shown (moves only when the cursor would leave the window).
    top: usize,
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

/// The Equipment slots in cursor order: tools 0-2, the art, quick items 0-4, ninjutsu.
fn slots(config: &crate::config::GameConfig) -> Vec<Slot> {
    let mut v = Vec::new();
    for i in 0..3 {
        v.push(config.player.prosthetics.get(i).map_or(Slot::Empty, |id| Slot::Weapon(*id)));
    }
    v.push(Slot::Weapon(config.player.combat_art));
    // Quick items: the five slots (config player.quick_items; items.rs) with their counts.
    for k in 0..crate::items::SLOTS {
        let id = crate::items::in_slot(config, k);
        v.push(if id > 0 { Slot::Goods(id, crate::items::count(config, id)) } else { Slot::Empty });
    }
    v.push(if config.player.ninjutsu > 0 { Slot::Goods(config.player.ninjutsu, 0) } else { Slot::Empty });
    v
}

/// The goods Wolf holds of an EquipParamGoods goodsType (0 consumables, 1 key items, 2
/// materials), by sortId, with their counts.
fn held_goods(config: &crate::config::GameConfig, goods_type: i64) -> Vec<(i64, u32)> {
    let mut v: Vec<(i64, u32)> = config.player.items.iter().copied().filter(|(id, n)| *n > 0 && crate::items::param("EquipParamGoods", *id)["goodsType"].as_i64() == Some(goods_type)).collect();
    v.sort_by_key(|(id, _)| (crate::items::param("EquipParamGoods", *id)["sortId"].as_i64().unwrap_or(0), *id));
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

/// A text at the height of a menu-movie field (a share of the stage's height, as the HUD's).
pub(crate) fn stage_text(t: &str, field: &StageText, color: Color, font: &Option<Handle<Font>>) -> (Text, TextFont, TextColor, TextShadow) {
    let mut x = text(t, 0.0, color, font);
    x.1.font_size = bevy::text::FontSize::Vh(field.vh());
    x
}

/// A node at stage rectangle `inner`, as a child of a node at `outer`.
pub(crate) fn inside(outer: &StageRect, inner: &StageRect) -> Node {
    let (l, t, w, h) = within(outer, inner);
    abs(l, t, w, h)
}

/// A text box over field `f` (a child of `outer`'s node, or of the root with `outer` the stage),
/// its text aligned as `justify` (the movie's field alignment).
pub(crate) fn field_box(outer: &StageRect, f: &StageRect, justify: JustifyContent) -> Node {
    Node { justify_content: justify, align_items: AlignItems::Center, ..inside(outer, f) }
}

/// The movie's stage (the root's box).
const STAGE: StageRect = StageRect::new(0.0, 0.0, crate::hud_layout::STAGE_W, crate::hud_layout::STAGE_H);

/// A sprite stretched over its box (the plates and brushes are laid out to the captures' boxes).
pub(crate) fn image(assets: &AssetServer, file: Option<String>) -> ImageNode {
    let mut i = file.map_or_else(ImageNode::default, |f| ImageNode::new(assets.load(crate::paths::asset(f))));
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
/// The first row a list of `n` shows with the cursor at `index`, from the last one: kept while
/// the cursor stays inside the window of ROWS, moved just enough when it leaves.
pub(crate) fn keep_visible(top: usize, index: usize, n: usize) -> usize {
    let top = if index < top { index } else if index >= top + ROWS { index + 1 - ROWS } else { top };
    top.min(n.saturating_sub(ROWS))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn list_screen(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, cats: &[(&str, &str)], cat: usize, rows: &[Row], index: usize, top: usize, detail: Option<&Detail>) {
    let lay = MenuLayout::get();
    if let Some((name, _)) = cats.get(cat) {
        root.spawn((lay.cat_bar.node(), image(assets, sprite("sb_in_game/MENU_Caption_02"))));
        root.spawn(field_box(&STAGE, &lay.cat_name.rect, JustifyContent::FlexStart)).with_children(|c| {
            c.spawn(stage_text(name, &lay.cat_name, TEXT, font));
        });
    }
    if !cats.is_empty() {
        root.spawn((lay.cat_arrow.node(), image(assets, sprite("sb_in_game/MENU_Arrow"))));
        root.spawn((key_top(&lay.cat_key, &lay.cat_arrow, false), image(assets, sprite("sb_keytop/KG_Key_C"))));
        for (i, (_, icon)) in cats.iter().enumerate().take(4) {
            if i == cat {
                root.spawn((lay.cat_cursors[i].node(), image(assets, sprite("sb_in_game/MENU_Cursor_TB")).with_color(ORANGE)));
            }
            root.spawn((crate::menu::ui_button(), lay.cat_icons[i].node(), image(assets, sprite(&format!("sb_in_game/{icon}"))), CatButton(i)));
        }
        let (arrow, key) = lay.cat_arrow_right(cats.len());
        root.spawn((key_top(&key, &arrow, true), image(assets, sprite("sb_keytop/KG_Key_V"))));
        let mut right = image(assets, sprite("sb_in_game/MENU_Arrow"));
        right.flip_x = true;
        root.spawn((arrow.node(), right));
        root.spawn((lay.cat_rule.node(), image(assets, sprite("sb_in_game/MENU_ME_Line"))));
    }
    let start = keep_visible(top, index, rows.len());
    let r0 = lay.row;
    for (k, (i, row)) in rows.iter().enumerate().skip(start).take(ROWS).enumerate() {
        let sel = i == index;
        let at = |part: StageRect| lay.in_row(part, k);
        let plate = at(r0.plate);
        root.spawn((crate::menu::ui_button(), plate.node(), image(assets, atlas_art("MENU_ItemBase_09")), RowButton(i))).with_children(|b| {
            if sel {
                b.spawn((inside(&plate, &at(r0.cursor)), image(assets, sprite("sb_in_game/MENU_Cursor_03"))));
            }
            b.spawn(field_box(&plate, &at(r0.name.rect), JustifyContent::FlexStart)).with_children(|c| {
                c.spawn(stage_text(&row.name, &r0.name, if sel { Color::WHITE } else { TEXT }, font));
            });
            if let Some(v) = &row.value {
                // A price has the coin; a word ("Owned") stands alone, dimmed.
                let price = v.parse::<u32>().is_ok();
                let end = StageRect::new(at(r0.name.rect).x1, plate.y0, plate.x1 - plate.w() * 0.04, plate.y1);
                b.spawn(Node { column_gap: Val::Percent(1.5), ..field_box(&plate, &end, JustifyContent::FlexEnd) }).with_children(|r| {
                    if price {
                        r.spawn((Node { height: Val::Percent(50.0), aspect_ratio: Some(1.0), ..default() }, image(assets, sprite("sb_fe/MENU_Money"))));
                    }
                    r.spawn(stage_text(v, &r0.name, if price { TEXT } else { DIM }, font));
                });
            }
        });
        let bx = at(r0.icon_box);
        root.spawn((crate::menu::ui_button(), bx.node(), image(assets, atlas_art("MENU_Item_Top")), RowButton(i))).with_children(|b| {
            b.spawn((inside(&bx, &at(r0.icon)), image(assets, row.icon.clone())));
            if row.equipped {
                b.spawn((inside(&bx, &at(r0.equipped)), image(assets, sprite("sb_in_game/MENU_Equipped"))));
            }
            if let Some(n) = &row.count {
                b.spawn(field_box(&bx, &at(r0.stock.rect), JustifyContent::FlexEnd)).with_children(|c| {
                    c.spawn(stage_text(n, &r0.stock, Color::WHITE, font));
                });
            }
        });
    }
    // The scroll bar (its thumb the shown share of the list).
    if rows.len() > ROWS {
        let tr = lay.scroll_track;
        root.spawn((tr.node(), image(assets, sprite("sb_in_game/MENU_Scroll_Base"))));
        let share = ROWS as f32 / rows.len() as f32;
        let y0 = tr.y0 + tr.h() * start as f32 / rows.len() as f32;
        root.spawn((StageRect::new(lay.scroll_bar.x0, y0, lay.scroll_bar.x1, y0 + tr.h() * share).node(), image(assets, sprite("sb_in_game/MENU_Scroll_Bar"))));
    }
    if let Some(d) = detail {
        parchment(root, assets, font, d);
    }
}

/// A key-top in an arrow's key field: as tall as the arrow, at the field's end by the arrow.
fn key_top(field: &StageRect, arrow: &StageRect, right: bool) -> Node {
    let h = arrow.h() * 0.75;
    let w = h * 1.5;
    let y0 = arrow.cy() - h / 2.0;
    let x0 = if right { field.x1 - (arrow.w() + w) - 4.0 } else { arrow.x1 + 4.0 };
    let x0 = if right { x0.max(field.x0) } else { x0 };
    StageRect::new(x0, y0, x0 + w, y0 + h).node()
}

/// The parchment on the right (the movie's ItemText_0): MENU_ItemDetails, the name on its brush
/// bar, the icon in its box (items) or the story art (skills), the stat lines, the text over the
/// ruled lines.
pub(crate) fn parchment(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, d: &Detail) {
    let lay = &MenuLayout::get().details;
    root.spawn((lay.base.node(), image(assets, atlas_art("MENU_ItemDetails"))));
    root.spawn((lay.name_bar.node(), image(assets, sprite("sb_in_game/MENU_Caption_02"))));
    root.spawn(field_box(&STAGE, &lay.name.rect, JustifyContent::Center)).with_children(|c| {
        c.spawn(stage_text(&d.name, &lay.name, Color::WHITE, font));
    });
    let mut y = lay.stat.y0;
    if let Some(art) = &d.art {
        // gap: the skill art's place is not in ItemText_0: under the name bar, as captured.
        let a = StageRect::new(1171.0, lay.name_bar.y1 + 3.0, 1690.0, lay.name_bar.y1 + 181.0);
        root.spawn((a.node(), ImageNode::new(assets.load(crate::paths::asset(art.clone())))));
        y = a.y1 + 8.0;
    } else if d.icon.is_some() {
        root.spawn((lay.icon_box.node(), image(assets, atlas_art("MENU_Item_Top")))).with_children(|b| {
            b.spawn((inside(&lay.icon_box, &lay.icon), image(assets, d.icon.clone())));
        });
    }
    let st = lay.stat_text;
    for (label, value) in &d.stats {
        let line = StageRect::new(lay.stat.x0, y, lay.stat.x1, y + lay.stat.h());
        root.spawn((line.node(), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgba(0.5, 0.5, 0.5, 0.9)))).with_children(|b| {
            let pad = StageRect::new(st.rect.x0, line.y0, line.x1 - (st.rect.x0 - line.x0) * 3.0, line.y1);
            b.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..field_box(&line, &pad, JustifyContent::SpaceBetween) }).with_children(|c| {
                c.spawn(stage_text(label, &st, TEXT, font));
                c.spawn(stage_text(value, &st, TEXT, font));
            });
        });
        y += lay.stat_pitch;
    }
    // The ruled lines under the description, then the text over them.
    let mut r = lay.line.y0;
    while r <= lay.line_last + 0.5 {
        if r > y {
            root.spawn((StageRect::new(lay.line.x0, r, lay.line.x1, r + lay.line.h()).node(), image(assets, sprite("sb_in_game/MENU_ME_Line"))));
        }
        r += lay.line_pitch;
    }
    let top = lay.desc.rect.y0.max(y);
    root.spawn(StageRect::new(lay.desc.rect.x0, top, lay.desc.rect.x1, lay.desc.rect.y1.max(top + 1.0)).node()).with_children(|c| {
        c.spawn(stage_text(&d.info, &lay.desc, TEXT, font));
    });
}

/// The status bar along the bottom (all but Options), on the movie's five black plates
/// (PlayerStatus): resurrection nodes over the Vitality bar, Unseen Aid, Vitality / Attack Power,
/// the skill level and its gauge, Spirit Emblems and Sen.
/// gap: attribute levels, Unseen Aid and skill experience are not modelled: Wolf's start values;
/// the plates' contents are placed inside them by eye.
pub(crate) fn status_bar(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts, wolf: Option<(f32, f32)>) {
    let lay = MenuLayout::get();
    let st = lay.status_text;
    let small = StageText { px: st.px * 0.85, ..st };
    let (hp, hp_max) = wolf.unwrap_or((0.0, 1.0));
    let plate = |i: usize| (lay.status[i].node(), image(assets, sprite("sb_in_game/MENU_BlackPlate")));
    root.spawn((lay.status_rule.node(), image(assets, sprite("sb_in_game/MENU_Base_00_FUTA"))));
    // Resurrection and Vitality.
    root.spawn(plate(0)).with_children(|b| {
        for k in 0..config.player.resurrections.max(1) {
            let lit = k < config.player.resurrections;
            b.spawn((abs(4.0 + k as f32 * 6.5, 12.0, 5.0, 16.5), image(assets, sprite("sb_fe/MENU_Revive_Gauge")).with_color(if lit { Color::WHITE } else { Color::srgba(0.4, 0.4, 0.4, 0.6) })));
        }
        b.spawn((abs(5.0, 50.0, 90.0, 9.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7))));
        b.spawn((abs(5.0, 50.0, 90.0 * (hp / hp_max.max(1.0)).clamp(0.0, 1.0), 9.0), BackgroundColor(Color::srgb(0.62, 0.17, 0.12))));
    });
    // Unseen Aid (gap: no Dragonrot; the game's base 30 %).
    root.spawn(plate(1)).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, padding: UiRect::left(Val::Percent(30.0)), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }).with_children(|c| {
            c.spawn(stage_text("Unseen Aid", &st, TEXT, font));
            c.spawn(stage_text("30%", &st, TEXT, font));
        });
    });
    // Vitality / Attack Power.
    root.spawn(plate(2)).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: Val::Percent(2.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..default() }).with_children(|c| {
            for (icon, label, value) in [("MENU_ST_Vitality", "Vitality", format!("{:.0}", config.posture.player_level)), ("MENU_ST_Offensive", "Attack Power", config.player.attack_power.to_string())] {
                c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Percent(2.0), ..default() }).with_children(|r| {
                    r.spawn((Node { width: Val::Vh(2.6), height: Val::Vh(2.6), ..default() }, image(assets, sprite(&format!("sb_in_game/{icon}")))));
                    r.spawn((Node { flex_grow: 1.0, ..default() },)).with_children(|n| {
                        n.spawn(stage_text(label, &st, TEXT, font));
                    });
                    r.spawn(stage_text(&value, &st, TEXT, font));
                });
            }
        });
    });
    // The skill level and the gauge to the next point.
    root.spawn(plate(3)).with_children(|b| {
        b.spawn((abs(2.0, 12.0, 11.0, 40.0), image(assets, sprite("sb_fe/MENU_Skill_Lv")).with_color(Color::srgb(0.15, 0.15, 0.15)))).with_children(|d| {
            d.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                c.spawn(stage_text(&config.player.skill_points.to_string(), &small, Color::srgb(0.55, 0.85, 0.9), font));
            });
        });
        b.spawn((abs(14.0, 25.0, 60.0, 12.0), image(assets, sprite("sb_in_game/MENU_GaugeFrame"))));
        b.spawn((Node { position_type: PositionType::Absolute, right: Val::Percent(5.0), bottom: Val::Percent(14.0), ..default() },)).with_children(|c| {
            c.spawn(stage_text("to next Skill Point  -", &small, Color::srgb(0.55, 0.85, 0.9), font));
        });
    });
    // Spirit Emblems and Sen.
    let emblem = texts.goods_id("Spirit Emblem").and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
    root.spawn(plate(4)).with_children(|b| {
        b.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: Val::Percent(2.0), width: Val::Percent(100.0), height: Val::Percent(100.0), padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..default() }).with_children(|c| {
            let n = config.player.spirit_emblems;
            for (icon, value) in [(emblem, format!("{n}/{n}")), (sprite("sb_fe/MENU_Money"), config.player.sen.to_string())] {
                c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Percent(3.0), ..default() }).with_children(|r| {
                    r.spawn((Node { width: Val::Vh(3.0), height: Val::Vh(3.0), ..default() }, image(assets, icon)));
                    r.spawn((Node { flex_grow: 1.0, justify_content: JustifyContent::FlexEnd, ..default() },)).with_children(|n| {
                        n.spawn(stage_text(&value, &st, TEXT, font));
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
            for (id, n) in held_goods(config, 0) {
                let max = crate::items::param("EquipParamGoods", id)["maxNum"].as_u64().unwrap_or(99) as u32;
                v.push(goods(id, Some(n), vec![held(n, max), storage()]));
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
            for (id, n) in held_goods(config, 1) {
                v.push(goods(id, (n > 1).then_some(n), vec![]));
            }
            v
        }
        2 => held_goods(config, 2)
            .into_iter()
            .map(|(id, n)| {
                let max = crate::items::param("EquipParamGoods", id)["maxNum"].as_u64().unwrap_or(99) as u32;
                goods(id, Some(n), vec![held(n, max), storage()])
            })
            .collect(),
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
/// the Homeward Idol; Ninjutsu: the three held (ninjutsu::NINJUTSU, all with
/// `unlock_all_moves`).
fn equip_choices(slot: usize, config: &crate::config::GameConfig, combat: &crate::data::Combat, texts: &ItemTexts) -> Vec<Slot> {
    match slot {
        0..=2 => {
            // The tools made and the upgrades bought (workshop.rs).
            let ok = crate::workshop::equippable(config);
            let mut ids: Vec<i64> = combat.player.prosthetics.iter().map(|t| t.id).filter(|id| ok.contains(id)).collect();
            ids.sort();
            ids.dedup();
            ids.into_iter().filter(|id| texts.weapon_name.contains_key(&(*id as u32))).map(Slot::Weapon).collect()
        }
        3 => (5100..=7700).step_by(100).filter(|id| texts.weapon_name.contains_key(&(*id as u32))).map(Slot::Weapon).collect(),
        4..=8 => {
            // The gourd and every consumable held that has a use (items.rs use_state).
            let mut v = vec![Slot::Goods(3000, 0)];
            for (id, n) in held_goods(config, 0) {
                if crate::items::use_state(crate::items::param("EquipParamGoods", id)["goodsUseAnim"].as_i64().unwrap_or(255)).is_some() && texts.goods_name.contains_key(&(id as u32)) {
                    v.push(Slot::Goods(id, n));
                }
            }
            v
        }
        9 => crate::ninjutsu::NINJUTSU.iter().filter(|&&id| config.player.unlock_all_moves || crate::items::count(config, id) > 0).map(|&id| Slot::Goods(id, 0)).collect(),
        _ => Vec::new(),
    }
}

/// Is this choice equipped (in any slot of its kind)?
fn is_equipped(slot: usize, choice: &Slot, config: &crate::config::GameConfig) -> bool {
    match (slot, choice) {
        (0..=2, Slot::Weapon(id)) => config.player.prosthetics.contains(id),
        (3, Slot::Weapon(id)) => config.player.combat_art == *id,
        (4..=8, Slot::Goods(id, _)) => config.player.quick_items.contains(id),
        (9, Slot::Goods(id, _)) => config.player.ninjutsu == *id,
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
        // The ninjutsu slot: on, or off when it is the one worn.
        (9, Slot::Goods(id, _)) => config.player.ninjutsu = if config.player.ninjutsu == *id { 0 } else { *id },
        // A quick item goes in this slot (out of any other); the slot's own comes off.
        (4..=8, Slot::Goods(id, _)) => {
            let q = &mut config.player.quick_items;
            q.resize(crate::items::SLOTS, 0);
            let k = slot - 4;
            if q[k] == *id {
                q[k] = 0;
            } else {
                for s in q.iter_mut().filter(|s| **s == *id) {
                    *s = 0;
                }
                q[k] = *id;
            }
        }
        _ => {}
    }
}

/// The picker's rows and details.
fn equip_rows(slot: usize, config: &crate::config::GameConfig, combat: &crate::data::Combat, texts: &ItemTexts, tables: &Tables) -> Vec<(Row, Detail)> {
    equip_choices(slot, config, combat, texts)
        .iter()
        .map(|c| {
            let (name, info) = slot_text(texts, c);
            let icon = slot_icon(tables, c);
            let art = match c {
                Slot::Weapon(id) if slot == 3 => tables.weapon_icon.get(id).and_then(|i| sprite(&format!("sb_icon_skill/MENU_SkillStory_{i:05}")).or_else(|| sprite(&format!("sb_icon_skill_02/MENU_SkillStory_{i:05}")))),
                _ => None,
            };
            let count = match c {
                Slot::Goods(id, _) => Some(crate::items::count(config, *id).to_string()),
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
    let lay = MenuLayout::get();
    if let Some(slot) = menu.pick {
        commands
            .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), GameMenuRoot))
            .with_children(|root| {
                root.spawn((lay.base.node(), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Equipped Items");
                let list = equip_rows(slot, config, combat, texts, tables);
                let rows: Vec<Row> = list.iter().map(|(r, _)| r.clone()).collect();
                list_screen(root, assets, font, &[], 0, &rows, menu.index, menu.top, list.get(menu.index).map(|(_, d)| d));
                help_line(root, font, if rows.is_empty() { "Nothing to equip here yet." } else { "Choose item to equip." });
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
            root.spawn((lay.base.node(), image(assets, sprite("sb_in_game/MENU_Base_00"))));
            // Tabs (the movie's TabList), with the switch keys and the brush arrows either end.
            root.spawn((lay.tab_arrow.node(), image(assets, sprite("sb_in_game/MENU_Arrow"))));
            root.spawn((key_top(&lay.tab_key, &lay.tab_arrow, false), image(assets, sprite("sb_keytop/KG_Key_Z"))));
            let (arrow, key) = lay.tab_arrow_right();
            root.spawn((key_top(&key, &arrow, true), image(assets, sprite("sb_keytop/KG_Key_X"))));
            let mut right = image(assets, sprite("sb_in_game/MENU_Arrow"));
            right.flip_x = true;
            root.spawn((arrow.node(), right));
            for (i, name) in TABS.iter().enumerate() {
                let open = i == menu.tab;
                let (brush, at) = if open { ("sb_in_game/MENU_Cursor_01", lay.tab_cursors[i]) } else { ("sb_in_game/MENU_Caption_01", lay.tabs[i]) };
                // The label sits where the first tab's is, moved with its tab.
                let dx = lay.tabs[i].x0 - lay.tabs[0].x0;
                let label = StageRect::new(lay.tab_text.rect.x0 + dx, lay.tab_text.rect.y0, lay.tab_text.rect.x1 + dx, lay.tab_text.rect.y1);
                root.spawn((crate::menu::ui_button(), at.node(), image(assets, sprite(brush)), TabButton(i))).with_children(|t| {
                    t.spawn(field_box(&at, &label, JustifyContent::Center)).with_children(|c| {
                        c.spawn(stage_text(name, &lay.tab_text, if open { Color::WHITE } else { TEXT }, font));
                    });
                });
            }
            // The rule under the tabs.
            root.spawn((lay.tab_rule.node(), image(assets, sprite("sb_in_game/MENU_Base_00_FUTA"))));
            match menu.tab {
                0 => equipment(root, assets, menu, font, config, tables, texts),
                1 | 2 => {
                    let k = menu.tab - 1;
                    let (cats, list) = if menu.tab == 1 { (&INV_CATS, inventory(menu.cat[k], config, texts, tables)) } else { (&SKILL_CATS, skills(menu.cat[k], config, texts, tables)) };
                    let rows: Vec<Row> = list.iter().map(|(r, _)| r.clone()).collect();
                    let detail = list.get(menu.index).map(|(_, d)| d);
                    list_screen(root, assets, font, cats, menu.cat[k], &rows, menu.index, menu.top, detail);
                }
                _ => {
                    // The movie's Options column (State_2): plates MENU_ItemBase_01, the cursor
                    // MENU_Cursor_03 over the chosen one.
                    let ot = lay.option_text;
                    for (i, label) in OPTION_ITEMS.iter().enumerate() {
                        let sel = i == menu.index;
                        let at = lay.options[i.min(lay.options.len() - 1)];
                        let text_at = StageRect::new(ot.rect.x0, at.y0 + (ot.rect.y0 - lay.options[0].y0), ot.rect.x1, at.y0 + (ot.rect.y1 - lay.options[0].y0));
                        root.spawn((crate::menu::ui_button(), at.node(), image(assets, sprite("sb_in_game/MENU_ItemBase_01")), RowButton(i))).with_children(|b| {
                            if sel {
                                b.spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, image(assets, sprite("sb_in_game/MENU_Cursor_03"))));
                            }
                            b.spawn(field_box(&at, &text_at, JustifyContent::Center)).with_children(|c| {
                                c.spawn(stage_text(label, &ot, if sel { Color::WHITE } else { TEXT }, font));
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
            help_line(root, font, help);
            key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Close")]);
        });
}

/// The help line at the bottom left (the movie's StatusBar/LineHelp).
pub(crate) fn help_line(root: &mut ChildSpawnerCommands, font: &Option<Handle<Font>>, help: &str) {
    let f = MenuLayout::get().help;
    root.spawn(field_box(&STAGE, &f.rect, JustifyContent::FlexStart)).with_children(|c| {
        c.spawn(stage_text(help, &f, DIM, font));
    });
}

/// The keys at the bottom right (StatusBar/KeyGuide, right-aligned): (key-top sprite, label).
pub(crate) fn key_guide(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, keys: &[(&str, &str)]) {
    let f = MenuLayout::get().keys;
    root.spawn(Node { column_gap: Val::Vh(0.4), ..field_box(&STAGE, &f.rect, JustifyContent::FlexEnd) }).with_children(|r| {
        for (key, label) in keys {
            r.spawn((Node { height: Val::Vh(f.vh() * 0.9), aspect_ratio: Some(1.5), ..default() }, image(assets, sprite(key))));
            r.spawn(stage_text(label, &f, DIM, font));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn equipment(root: &mut ChildSpawnerCommands, assets: &AssetServer, menu: &GameMenu, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts) {
    let lay = MenuLayout::get();
    let all = slots(config);
    // Section bars (MENU_Caption_02) with their labels.
    for (i, label) in ["Prosthetic Tools", "Combat Arts", "Quick Items", "Ninjutsu"].iter().enumerate() {
        let bar = lay.sections[i];
        root.spawn((bar.node(), image(assets, sprite("sb_in_game/MENU_Caption_02")))).with_children(|b| {
            b.spawn(field_box(&bar, &lay.section_text[i].rect, JustifyContent::FlexStart)).with_children(|c| {
                c.spawn(stage_text(label, &lay.section_text[i], TEXT, font));
            });
        });
    }
    // The slots (the movie's grid_0 items): frames MENU_IconBase_02, the art's and the
    // ninjutsu's MENU_ItemBase_Skill.
    for (i, s) in lay.slots.iter().enumerate().take(all.len()) {
        let slot = &all[i];
        let frame = if i == 3 || i == 9 { atlas_art("MENU_ItemBase_Skill") } else { sprite("sb_in_game/MENU_IconBase_02") };
        let b0 = s.base;
        root.spawn((crate::menu::ui_button(), b0.node(), image(assets, frame), RowButton(i))).with_children(|b| {
            if i == menu.index {
                b.spawn((inside(&b0, &s.cursor), image(assets, sprite("sb_in_game/MENU_Cursor_02"))));
            }
            if let Some(icon) = slot_icon(tables, slot) {
                b.spawn((inside(&b0, &s.icon), ImageNode::new(assets.load(crate::paths::asset(icon)))));
            }
            if let Slot::Goods(_, n) = slot {
                b.spawn(field_box(&b0, &s.stock.rect, JustifyContent::FlexEnd)).with_children(|c| {
                    c.spawn(stage_text(&n.to_string(), &s.stock, Color::WHITE, font));
                });
            }
        });
    }
    // The item under the cursor on the parchment.
    let slot = &all[menu.index.min(all.len() - 1)];
    let (name, info) = slot_text(texts, slot);
    parchment(root, assets, font, &Detail { name, icon: slot_icon(tables, slot), art: None, stats: vec![], info });
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
    (assets, font, mut config, texts, tables, combat, pointer): (Res<AssetServer>, Res<crate::fonts::GameFonts>, ResMut<crate::config::GameConfig>, Res<ItemTexts>, Res<Tables>, Res<crate::data::Combat>, Res<crate::menu::MenuMouse>),
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
        let choices = equip_choices(slot, &config, &combat, &texts);
        let n = choices.len().max(1);
        if any(&[KeyCode::ArrowDown, KeyCode::KeyS, KeyCode::ArrowRight, KeyCode::KeyD]) {
            menu.index = (menu.index + 1) % n;
        }
        if any(&[KeyCode::ArrowUp, KeyCode::KeyW, KeyCode::ArrowLeft, KeyCode::KeyA]) {
            menu.index = (menu.index + n - 1) % n;
        }
        menu.index = crate::menu::wheel_step(menu.index, pointer.wheel, n);
        for (r, i) in &rows {
            match i {
                Interaction::Pressed => {
                    menu.index = r.0;
                    confirm = true;
                }
                Interaction::Hovered if pointer.moved => menu.index = r.0,
                _ => {}
            }
        }
        let back = keys.just_pressed(KeyCode::Escape);
        keys.reset_all();
        mouse.reset_all();
        menu.top = keep_visible(menu.top, menu.index, n);
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
            Interaction::Hovered if pointer.moved => menu.index = r.0,
            _ => {}
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
    menu.index = crate::menu::wheel_step(menu.index, pointer.wheel, n);
    menu.top = keep_visible(menu.top, menu.index, n);
    let close = keys.just_pressed(KeyCode::Escape);
    keys.reset_all();
    mouse.reset_all();
    if close {
        next.set(Screen::Playing);
        return;
    }
    if confirm && menu.tab == 0 {
        let slot = menu.index;
        menu.index = equip_choices(slot, &config, &combat, &texts).iter().position(|c| match c { Slot::Goods(id, _) => slot >= 4 && crate::items::in_slot(&config, slot - 4) == *id, _ => is_equipped(slot, c, &config) }).unwrap_or(0);
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
    let key = (menu.tab, menu.index + menu.top * 1000, menu.cat, menu.pick, gear);
    if menu.built != Some(key) {
        for r in &roots {
            commands.entity(r).despawn();
        }
        let w = wolf.single().ok().map(|a| (a.hp, a.hp_max));
        build(&mut commands, &assets, &menu, &font.ui, &config, &combat, &texts, &tables, w);
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
            .insert_resource(texts)
            .insert_resource(tables)
            .add_systems(PreUpdate, run_game_menu.after(crate::menu::MenuInput));
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    /// The menu's places from the menu movie fit game_menu's own order: the Equipment slots run
    /// three tools in a row, then the art right of them, five quick items in a row under the tools
    /// (smaller frames), the ninjutsu under the art, each slot's icon inside its frame; the list
    /// shows ROWS rows of the movie, the Options column holds every option row, evenly pitched;
    /// the status plates run left to right; the right-hand tab arrow ends inside the stage.
    #[test]
    fn the_in_game_menu_takes_its_places_from_the_menu_movie() {
        let l = MenuLayout::get();
        let s = &l.slots;
        let n = slots(&crate::config::GameConfig::load().expect("config.toml")).len();
        assert_eq!(n, s.len(), "one movie slot per menu slot");
        assert!(s[0].base.x1 <= s[1].base.x0 && s[1].base.x1 <= s[2].base.x0 && s[2].base.x1 <= s[3].base.x0, "tools left to right, then the art");
        assert!((s[0].base.y0 - s[2].base.y0).abs() < 1.0, "the tools in one row");
        for k in 4..8 {
            assert!(s[k].base.x1 <= s[k + 1].base.x0 + 1.0 && (s[k].base.y0 - s[4].base.y0).abs() < 1.0, "quick items in one row");
        }
        assert!(s[4].base.y0 > s[0].base.y1 && s[4].base.w() < s[0].base.w(), "quick items under the tools, smaller");
        assert!(s[9].base.y0 > s[3].base.y1 && (s[9].base.x0 - s[3].base.x0).abs() < 5.0, "the ninjutsu under the art");
        for x in s {
            let (a, b, w, h) = within(&x.base, &x.icon);
            assert!(a >= 0.0 && b >= 0.0 && a + w <= 100.5 && b + h <= 100.5, "icon inside its frame");
        }
        assert_eq!(l.rows.len(), ROWS, "the list's window is the movie's six rows");
        assert!(OPTION_ITEMS.len() <= l.options.len(), "every option has a row");
        let pitch = l.options[1].y0 - l.options[0].y0;
        assert!(l.options.windows(2).all(|w| ((w[1].y0 - w[0].y0) - pitch).abs() < 1.0), "options evenly pitched");
        assert!(l.status.windows(2).all(|w| w[0].x0 < w[1].x0), "status plates left to right");
        let (arrow, _) = l.tab_arrow_right();
        assert!(arrow.x0 > l.tabs[3].x1 && arrow.x1 < crate::hud_layout::STAGE_W, "the right arrow past the last tab, on the stage");
        assert!(l.details.base.x0 > l.row.plate.x1, "the parchment right of the list");
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;

    /// The list keeps its window while the cursor stays in it (scrolling up no longer drags the
    /// whole list with the cursor pinned to the bottom row), and the wheel stops at the ends.
    #[test]
    fn the_list_window_holds_until_the_cursor_leaves_it() {
        let n = 12;
        let mut top = 0;
        for i in 0..=7 {
            top = keep_visible(top, i, n);
        }
        assert_eq!(top, 2, "the cursor on row 7 shows rows 2-7");
        top = keep_visible(top, 5, n);
        assert_eq!(top, 2, "back up to row 5: the window stays");
        top = keep_visible(top, 1, n);
        assert_eq!(top, 1);
        assert_eq!(keep_visible(9, 11, n), 6, "never past the end");
        assert_eq!(crate::menu::wheel_step(0, 3, n), 0);
        assert_eq!(crate::menu::wheel_step(10, -5, n), 11);
        assert_eq!(crate::menu::wheel_step(4, -2, n), 6);
    }
}
