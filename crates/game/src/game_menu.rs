//! The in-game menu (#127): Esc pauses the world under the game's wooden board with the tabs
//! Equipment, Inventory, Skills and Options, drawn with its menu art, item icons and texts
//! (item.msgbnd) and showing Wolf's kit (`sekiro_sim::kit`). X / Z switch tabs, V / C the
//! category, the arrows move the cursor; Esc closes it. A click on a tab, category or arrow
//! switches it, a click on a row moves the cursor there and a second click on it confirms it, as
//! OK (E) does: Options rows act on the first click, and a Quick Item is used.
//! After FwLo's RustSekiro `src/game_menu.rs` (the layout set from captures of the game).
//! gap: no Equipped Items picker, and a used item only loses one from its count (no effect,
//! no use animation, no Use / Discard submenu); the tab and section
//! labels are English strings, not yet the game's FMG ids; attribute levels, Unseen Aid and
//! skill experience are not modelled; Options holds this build's Resume / Back to start menu /
//! Quit, not the game's settings; Ninjutsu lists nothing (Wolf starts with none, and the game
//! keeps their texts in the goods banks).
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button, observe};
use sekiro_sim::items::{HEALING_GOURD, ItemData};
use sekiro_sim::kit::Kit;

use super::Sim;
use crate::combat_hud::SKILL_BLUE;
use crate::game_ui::{self, GameFonts, MenuArt, abs};
use crate::menu::PAUSE_ROWS;

const TABS: [&str; 4] = ["Equipment", "Inventory", "Skills", "Options"];
const TEXT: Color = Color::srgb(0.88, 0.85, 0.78);
const DIM: Color = Color::srgba(0.75, 0.72, 0.66, 0.85);
const ORANGE: Color = Color::srgb(0.93, 0.6, 0.25);
/// Rows shown at once in a list screen.
const ROWS: usize = 6;
/// gap: Prayer Necklace attribute levels are not modelled; FwLo's starting level.
const PLAYER_LEVEL: u32 = 1;
/// The atlases holding item icons: the icon sheets, the memories' 750-756 in SB_Boss, and a
/// few strays elsewhere (Divine Grass's 15 in SB_TextEffect_02).
const ICONS: [&str; 6] = [
    "SB_Icon",
    "SB_Icon_02",
    "SB_Icon_03",
    "SB_Icon_04",
    "SB_Boss",
    "SB_TextEffect_02",
];
const STORIES: [&str; 2] = ["SB_Icon_Skill", "SB_Icon_Skill_02"];
const INV_CATS: [(&str, &str); 4] = [
    ("Quick Items", "MENU_TB_Item"),
    ("Key Items", "MENU_TB_valuable"),
    ("Upgrade Materials", "MENU_TB_Material"),
    ("Prosthetic Arm Tools", "MENU_TB_Arm"),
];
/// gap: MENU_TB_Attack stands in for the game's plain Combat Arts blade, as in FwLo's menu.
const SKILL_CATS: [(&str, &str); 4] = [
    ("Combat Arts", "MENU_TB_Attack"),
    ("Shinobi Martial Arts", "MENU_TB_Action"),
    ("Latent Skills", "MENU_TB_Sinobi"),
    ("Ninjutsu Techniques", "MENU_TB_Ninjyutu"),
];

/// The last tab and categories persist across pauses; the cursor starts at the first row.
#[derive(Resource, Default)]
struct GameMenu {
    tab: usize,
    index: usize,
    cat: [usize; 2],
    top: usize,
    /// The row of this screen the last click went to: a click on it again confirms it. Only
    /// clicks set it, so the cursor's row (moved by the keys, or the first row on opening) is
    /// never confirmed by a single click.
    armed: Option<usize>,
}

/// One menu input, from a key or a click: both change the menu the same way. As a component
/// it marks the clickable menu element that sends it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Cmd {
    /// Open this tab.
    Tab(usize),
    /// Open the next tab (true) or the previous one, wrapping.
    TabStep(bool),
    /// Show this category of the open list tab.
    Cat(usize),
    /// Show the next category (true) or the previous one of the open list tab, wrapping.
    CatStep(bool),
    /// Put the cursor on this row; when the last click was on it too, confirm it.
    Row(usize),
    /// Confirm the row under the cursor (OK).
    Ok,
    /// Close the menu (Back).
    Back,
    /// Move the cursor one row forward (true) or back, wrapping.
    Step(bool),
}

/// This frame's clicks on menu elements, applied with the keys in `menu_keys`; dropped while
/// the menu is not open, so none is applied late.
#[derive(Resource, Default)]
struct Clicks(Vec<Cmd>);

#[derive(Component)]
struct GameMenuRoot;

/// The screen, cursor and window shape (width over height, in thousandths) whose drawing is
/// currently built.
type Place = (usize, usize, usize, [usize; 2], u32);

/// The game's item names and descriptions, shared by slots, lists and skill categories.
#[derive(Default)]
struct ItemTexts {
    weapon_name: BTreeMap<i32, String>,
    weapon_info: BTreeMap<i32, String>,
    goods_name: BTreeMap<i32, String>,
    goods_info: BTreeMap<i32, String>,
}

impl ItemTexts {
    fn load(cache: &Path) -> Self {
        let read = |bank| game_ui::texts(cache, "item", bank);
        Self {
            weapon_name: read("武器名"),
            weapon_info: read("武器説明"),
            goods_name: read("アイテム名"),
            goods_info: read("アイテム説明"),
        }
    }

    /// The lowest id with this English name, as FwLo's key-item rows use.
    fn goods_id(&self, name: &str) -> Option<i32> {
        self.goods_name
            .iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(id, _)| *id)
    }
}

/// One skill and the virtual weapon that supplies its text and art.
struct Skill {
    id: i32,
    weapon: i32,
}

/// The goods fields needed to order held goods (their carry limit is the sim's, `Book::items`).
struct Goods {
    kind: i64,
    sort: i64,
    /// `isConsume`: using it takes one (not the Homeward Idol).
    consume: bool,
}

#[derive(Default)]
struct Tables {
    skills: [Vec<Skill>; 4],
    weapon_icon: BTreeMap<i32, i64>,
    goods_icon: BTreeMap<i32, i64>,
    goods: BTreeMap<i32, Goods>,
}

impl Tables {
    /// FwLo's categories: skills in id order, deduplicated by name and classified by description.
    fn load(cache: &Path, texts: &ItemTexts) -> Self {
        let rows = |name| {
            sekiro_sim::registry::rows(cache, name)
                .inspect_err(|e| warn!("game menu: {e}"))
                .unwrap_or_default()
        };
        let mut tables = Self::default();
        let mut seen = HashSet::new();
        for (id, row) in rows("SkillParam") {
            if id >= 700 {
                continue;
            }
            let Some(weapon) = row["virtualWeaponId"].as_i64().map(|n| n as i32) else {
                continue;
            };
            let name = texts.weapon_name.get(&weapon).cloned().unwrap_or_default();
            let info = texts
                .weapon_info
                .get(&weapon)
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
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
                tables.skills[cat].push(Skill { id, weapon });
            }
        }
        for (id, row) in rows("EquipParamWeapon") {
            if let Some(icon) = row["iconId"].as_i64() {
                tables.weapon_icon.insert(id, icon);
            }
        }
        for (id, row) in rows("EquipParamGoods") {
            if let Some(icon) = row["iconId"].as_i64() {
                tables.goods_icon.insert(id, icon);
            }
            tables.goods.insert(
                id,
                Goods {
                    kind: row["goodsType"].as_i64().unwrap_or(-1),
                    sort: row["sortId"].as_i64().unwrap_or(0),
                    consume: row["isConsume"].as_i64() == Some(1),
                },
            );
        }
        tables
    }
}

/// Texts and params read once, plus this run's attack power and translated key guides.
#[derive(Resource)]
struct Book {
    texts: ItemTexts,
    tables: Tables,
    /// The sim's item params (the same `Arc` its pickups read): the goods' carry limits.
    items: Arc<ItemData>,
    attack: u8,
    guides: BTreeMap<i32, String>,
    /// Key Items always held: Kusabimaru and the Shinobi Prosthetic (the game keeps
    /// Kusabimaru's name with the goods, not the weapons).
    key_items: Vec<i32>,
    /// The Spirit Emblem's icon, for the status bar.
    emblem: Option<String>,
}

impl Book {
    fn load(cache: &Path, attack: u8, items: Arc<ItemData>) -> Self {
        let texts = ItemTexts::load(cache);
        let tables = Tables::load(cache, &texts);
        let key_items = ["Kusabimaru", "Shinobi Prosthetic"]
            .into_iter()
            .filter_map(|name| texts.goods_id(name))
            .collect();
        let emblem = texts
            .goods_id("Spirit Emblem")
            .and_then(|id| slot_icon(&tables, &Slot::Goods(id, 0)));
        Self {
            texts,
            tables,
            items,
            attack,
            guides: game_ui::menu_texts(cache, "NTC_キーガイド"),
            key_items,
            emblem,
        }
    }
}

/// One Equipment slot, in cursor order: three tools, the art, five quick items, ninjutsu.
enum Slot {
    Weapon(i32),
    Goods(i32, u32),
    Empty,
}

fn slots(kit: &Kit) -> Vec<Slot> {
    let mut slots = Vec::new();
    for i in 0..3 {
        slots.push(
            kit.prosthetics
                .get(i)
                .map_or(Slot::Empty, |id| Slot::Weapon(*id)),
        );
    }
    slots.push(Slot::Weapon(kit.combat_art));
    for id in kit.quick_items {
        slots.push(if id > 0 {
            Slot::Goods(id, kit.count(id))
        } else {
            Slot::Empty
        });
    }
    slots.push(Slot::Empty);
    slots
}

/// Held goods of one goodsType, sorted by the game's sortId then id.
fn held_goods(kit: &Kit, tables: &Tables, kind: i64) -> Vec<(i32, u32)> {
    let mut goods: Vec<_> = kit
        .goods
        .iter()
        .copied()
        .filter(|(id, n)| *n > 0 && tables.goods.get(id).is_some_and(|g| g.kind == kind))
        .collect();
    goods.sort_by_key(|(id, _)| (tables.goods[id].sort, *id));
    goods
}

/// The item icon's sprite name: names, not Bevy assets, so row data can be checked alone.
/// Tool upgrades without their own icon show their base's; the gourd's goods row has no icon,
/// the game draws 151.
fn slot_icon(tables: &Tables, slot: &Slot) -> Option<String> {
    let id = match slot {
        Slot::Weapon(id) => tables
            .weapon_icon
            .get(id)
            .or_else(|| tables.weapon_icon.get(&(id / 100 * 100)))
            .copied(),
        Slot::Goods(HEALING_GOURD, _) => Some(151),
        Slot::Goods(id, _) => tables.goods_icon.get(id).copied(),
        Slot::Empty => None,
    }?;
    Some(format!("item/MENU_ItemIcon_{id:05}"))
}

/// Tool upgrade levels share their base's name and description, independently falling back.
fn slot_text(texts: &ItemTexts, slot: &Slot) -> (String, String) {
    let pick = |names: &BTreeMap<i32, String>, infos: &BTreeMap<i32, String>, id: i32| {
        let base = [id, id / 100 * 100, id / 1000 * 1000];
        (
            base.iter()
                .find_map(|id| names.get(id))
                .cloned()
                .unwrap_or_default(),
            base.iter()
                .find_map(|id| infos.get(id))
                .cloned()
                .unwrap_or_default(),
        )
    };
    match slot {
        Slot::Weapon(id) => pick(&texts.weapon_name, &texts.weapon_info, *id),
        Slot::Goods(id, _) => pick(&texts.goods_name, &texts.goods_info, *id),
        Slot::Empty => (String::new(), String::new()),
    }
}

/// The look shared by each drawing function, with no assets loaded while a screen is built.
struct Ui<'a> {
    art: &'a MenuArt,
    fonts: &'a GameFonts,
    aspect: f32,
}

impl Ui<'_> {
    fn text(&self, text: &str, px: f32, color: Color) -> (Text, TextFont, TextColor, TextShadow) {
        self.fonts.shadowed(text, px, color)
    }

    fn find(&self, file: &str) -> Option<(ImageNode, f32)> {
        let (atlas, name) = file.split_once('/')?;
        match atlas {
            "item" => self.art.find(&ICONS, name),
            "story" => self.art.find(&STORIES, name),
            "sb_in_game" => self.art.sprite("SB_In_Game", name),
            "sb_keytop" => self.art.sprite("SB_KeyTop", name),
            "sb_fe" => self.art.sprite("SB_FE", name),
            _ => None,
        }
    }

    /// Plates and icons stretch into FwLo's boxes; a missing sprite draws nothing.
    fn image(&self, file: &str) -> ImageNode {
        let mut image = self
            .find(file)
            .map(|(i, _)| i)
            .unwrap_or_else(|| ImageNode {
                color: Color::NONE,
                ..default()
            });
        image.image_mode = bevy::ui::widget::NodeImageMode::Stretch;
        image
    }

    /// Story art fits its capture's box without distorting the sprite's aspect.
    fn story(&self, root: &mut ChildSpawnerCommands, file: &str) {
        if let Some((image, aspect)) = self.find(file) {
            let w = (16.5 * aspect / self.aspect).min(27.0);
            let h = w * self.aspect / aspect;
            root.spawn((
                abs(61.0 + (27.0 - w) / 2.0, 20.6 + (16.5 - h) / 2.0, w, h),
                image,
            ));
        }
    }
}

/// What makes a menu element clickable: a press queues `cmd` for `menu_keys`.
fn click(cmd: Cmd) -> impl Bundle {
    (
        Button,
        ActivateOnPress,
        cmd,
        observe(move |_: On<Activate>, mut clicks: ResMut<Clicks>| clicks.0.push(cmd)),
    )
}

/// A clickable switch at one end of the tabs or categories, at `rect`: the brush arrow on the
/// outer side and the key cap on the inner side. `size` gives, in percent of `rect`, the arrow's
/// width and the cap's width, top and height.
fn arrow_key(
    root: &mut ChildSpawnerCommands,
    ui: &Ui,
    rect: [f32; 4],
    cmd: Cmd,
    key: &str,
    arrow_left: bool,
    size: (f32, [f32; 3]),
) {
    let (arrow_w, [cap_w, cap_t, cap_h]) = size;
    let [l, t, w, h] = rect;
    let mut arrow = ui.image("sb_in_game/MENU_Arrow");
    arrow.flip_x = !arrow_left;
    let cap = ui.image(&format!("sb_keytop/{key}"));
    root.spawn((abs(l, t, w, h), click(cmd)))
        .with_children(|b| {
            if arrow_left {
                b.spawn((abs(0.0, 0.0, arrow_w, 100.0), arrow));
                b.spawn((abs(100.0 - cap_w, cap_t, cap_w, cap_h), cap));
            } else {
                b.spawn((abs(0.0, cap_t, cap_w, cap_h), cap));
                b.spawn((abs(100.0 - arrow_w, 0.0, arrow_w, 100.0), arrow));
            }
        });
}

/// A full-size child laid out as a row, vertically centred.
fn fill_row() -> Node {
    Node {
        width: percent(100),
        height: percent(100),
        align_items: AlignItems::Center,
        ..default()
    }
}

/// A list row and its parchment: icon, name and count, then story art, stats and description.
#[derive(Default)]
struct Entry {
    icon: Option<String>,
    name: String,
    count: Option<String>,
    art: Option<String>,
    stats: Vec<(String, String)>,
    info: String,
    /// The consumable goods this row shows: OK uses one.
    usable: Option<i32>,
}

/// Installs the display over the existing pause state, with the kit and its item params
/// (`items`) owned by the sim.
pub(super) fn install(app: &mut App, attack: u8, items: Arc<ItemData>) {
    GameFonts::insert(app);
    MenuArt::insert_with(app, &[&ICONS[..], &STORIES[..], &["SB_FE"]].concat());
    game_ui::install_cover(app);
    app.insert_resource(Book::load(&sekiro_sim::cache::root(), attack, items))
        .init_resource::<GameMenu>()
        .init_resource::<Clicks>()
        .init_resource::<crate::menu::PauseView>()
        .add_systems(
            PreUpdate,
            (
                menu_keys.run_if(crate::menu::paused),
                drop_clicks.run_if(not(crate::menu::paused)),
            )
                .after(bevy::input::InputSystems)
                .after(crate::menu::PauseSystems),
        )
        .add_systems(Update, show);
}

/// Clicks queued as the menu closed (on the frame of the closing Esc) are never applied, not on
/// the next open either.
fn drop_clicks(mut clicks: ResMut<Clicks>) {
    clicks.0.clear();
}

/// The keys and this frame's clicks: tab and category switches, wrapped cursor movement, the
/// Options actions and using a Quick Item.
fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut clicks: ResMut<Clicks>,
    mut menu: ResMut<GameMenu>,
    mut view: ResMut<crate::menu::PauseView>,
    book: Res<Book>,
    mut sim: NonSendMut<Sim>,
) {
    let key = |k: &[KeyCode]| keys.any_just_pressed(k.iter().copied());
    let pad = |b| pads.iter().any(|p| p.just_pressed(b));
    let mut cmds = std::mem::take(&mut clicks.0);
    if key(&[crate::menu::MENU_NEXT_TAB_KEY]) || pad(GamepadButton::RightTrigger) {
        cmds.push(Cmd::TabStep(true));
    }
    if key(&[crate::menu::MENU_PREVIOUS_TAB_KEY]) || pad(GamepadButton::LeftTrigger) {
        cmds.push(Cmd::TabStep(false));
    }
    if key(&[KeyCode::KeyV]) || pad(GamepadButton::RightTrigger2) {
        cmds.push(Cmd::CatStep(true));
    }
    if key(&[KeyCode::KeyC]) || pad(GamepadButton::LeftTrigger2) {
        cmds.push(Cmd::CatStep(false));
    }
    if crate::menu::ok_pressed(&keys, &pads) {
        cmds.push(Cmd::Ok);
    }
    // Back (Q, B) closes the menu, as Resume and Esc do: it has no nested level.
    if crate::menu::back_pressed(&keys, &pads) {
        cmds.push(Cmd::Back);
    }
    // The pause's own arrows, D-pad and stick, plus WASD.
    let step = view.movement.x + view.movement.y;
    if step > 0 || key(&[KeyCode::KeyD, KeyCode::KeyS]) {
        cmds.push(Cmd::Step(true));
    } else if step < 0 || key(&[KeyCode::KeyA, KeyCode::KeyW]) {
        cmds.push(Cmd::Step(false));
    }
    for cmd in cmds {
        if let Some(row) = apply(&mut menu, cmd, &mut sim.duel.kit, &book) {
            view.request = Some(row);
        }
    }
}

/// The rows of the open screen, at least one: the lengths of the lists `build` draws, counted
/// without building them.
fn row_count(menu: &GameMenu, kit: &Kit, book: &Book) -> usize {
    let tables = &book.tables;
    match menu.tab {
        0 => slots(kit).len(),
        1 => match menu.cat[0] {
            0 => 1 + held_goods(kit, tables, 0).len(),
            1 => book.key_items.len() + held_goods(kit, tables, 1).len(),
            2 => held_goods(kit, tables, 2).len(),
            _ => kit.prosthetics.len(),
        },
        2 => learned(menu.cat[1], kit, book).count(),
        _ => PAUSE_ROWS.len(),
    }
    .max(1)
}

/// The place after `i` (true) or before it, of `n` places, wrapping.
fn wrap(i: usize, n: usize, next: bool) -> usize {
    (i + if next { 1 } else { n - 1 }) % n
}

/// Applies one command; returns the Options row (an index into `PAUSE_ROWS`) it picks.
fn apply(menu: &mut GameMenu, cmd: Cmd, kit: &mut Kit, book: &Book) -> Option<usize> {
    let list = menu.tab == 1 || menu.tab == 2;
    match cmd {
        Cmd::Tab(tab) if tab != menu.tab => {
            menu.tab = tab;
            menu.index = 0;
            menu.top = 0;
            menu.armed = None;
        }
        Cmd::TabStep(next) => {
            return apply(menu, Cmd::Tab(wrap(menu.tab, TABS.len(), next)), kit, book);
        }
        Cmd::Cat(cat) if list && cat != menu.cat[menu.tab - 1] => {
            menu.cat[menu.tab - 1] = cat;
            menu.index = 0;
            menu.top = 0;
            menu.armed = None;
        }
        Cmd::CatStep(next) if list => {
            let cat = wrap(menu.cat[menu.tab - 1], INV_CATS.len(), next);
            return apply(menu, Cmd::Cat(cat), kit, book);
        }
        Cmd::Step(next) => {
            let n = row_count(menu, kit, book);
            menu.index = wrap(menu.index, n, next);
            menu.top = keep_visible(menu.top, menu.index, n);
            menu.armed = None;
        }
        Cmd::Row(row) => {
            let n = row_count(menu, kit, book);
            if row >= n {
                return None;
            }
            let again = menu.armed == Some(row);
            menu.index = row;
            menu.top = keep_visible(menu.top, row, n);
            menu.armed = Some(row);
            // Options rows are buttons: one click acts.
            if again || menu.tab == 3 {
                return apply(menu, Cmd::Ok, kit, book);
            }
        }
        Cmd::Ok if menu.tab == 3 => return Some(menu.index),
        Cmd::Ok if menu.tab == 1 && menu.cat[0] == 0 => {
            let usable = inventory(0, kit, book)
                .get(menu.index)
                .and_then(|e| e.usable);
            let before = row_count(menu, kit, book);
            if let Some(id) = usable
                && kit.use_goods(id)
            {
                info!("game menu: used goods {id}, {} left", kit.count(id));
                let n = row_count(menu, kit, book);
                if n != before {
                    // The last one used: its row is gone, and a click must not use the row
                    // that took its place.
                    menu.armed = None;
                }
                menu.index = menu.index.min(n - 1);
                menu.top = keep_visible(menu.top, menu.index, n);
            }
        }
        Cmd::Back => return Some(0),
        _ => {}
    }
    None
}

/// Rebuilds only for a changed cursor, screen or kit; closing keeps the tab and categories.
fn show(
    mut commands: Commands,
    state: Res<crate::menu::PanelState>,
    mut menu: ResMut<GameMenu>,
    (fonts, art, book): (Res<GameFonts>, Res<MenuArt>, Res<Book>),
    sim: NonSend<Sim>,
    (roots, windows): (Query<Entity, With<GameMenuRoot>>, Query<&Window>),
    mut built: Local<Option<(Place, Kit)>>,
) {
    if !state.paused {
        if built.take().is_some() {
            for root in &roots {
                commands.entity(root).despawn();
            }
            menu.index = 0;
            menu.top = 0;
            menu.armed = None;
        }
        return;
    }
    let aspect = windows
        .iter()
        .next()
        .map_or(16.0 / 9.0, |w| w.width() / w.height().max(1.0));
    // A resize rebuilds too, for the story art's shape.
    let now = (
        menu.tab,
        menu.index,
        menu.top,
        menu.cat,
        (aspect * 1000.0) as u32,
    );
    // A changed kit (an item used) rebuilds under an unchanged cursor.
    let kit = &sim.duel.kit;
    if built
        .as_ref()
        .is_some_and(|(place, drawn)| *place == now && drawn == kit)
    {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    let hp = &sim.duel.rules.fighters[0].vitality;
    build(
        &mut commands,
        &Ui {
            art: &art,
            fonts: &fonts,
            aspect,
        },
        &menu,
        kit,
        &book,
        (hp.hp as f32, hp.max_hp as f32),
    );
    *built = Some((now, kit.clone()));
}

/// The key guide at the bottom right, using the game's OK and Close labels.
fn key_guide(root: &mut ChildSpawnerCommands, ui: &Ui, guides: &BTreeMap<i32, String>) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        right: percent(5.3),
        top: percent(91.4),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(4),
        ..default()
    })
    .with_children(|r| {
        for (name, id, suffix) in [("KG_Key_Enter", 100000, "  "), ("KG_Key_Esc", 110001, "")] {
            r.spawn((
                Node {
                    height: px(20),
                    width: px(30),
                    ..default()
                },
                ui.image(&format!("sb_keytop/{name}")),
            ));
            let label = guides.get(&id).map(String::as_str).unwrap_or("");
            r.spawn(ui.text(&format!(":{label}{suffix}"), 16.0, DIM));
        }
    });
}

/// The first row shown is kept while the cursor stays in its six-row window.
fn keep_visible(top: usize, index: usize, n: usize) -> usize {
    let top = if index < top {
        index
    } else if index >= top + ROWS {
        index + 1 - ROWS
    } else {
        top
    };
    top.min(n.saturating_sub(ROWS))
}

fn list_screen(
    root: &mut ChildSpawnerCommands,
    ui: &Ui,
    categories: (&[(&str, &str)], usize),
    entries: &[Entry],
    window: (usize, usize),
) {
    let (cats, cat) = categories;
    let (index, top) = window;
    if let Some((name, _)) = cats.get(cat) {
        root.spawn(abs(9.7, 14.0, 30.0, 4.0)).with_children(|c| {
            c.spawn(ui.text(name, 17.0, TEXT));
        });
    }
    if !cats.is_empty() {
        let (back, next) = (Cmd::CatStep(false), Cmd::CatStep(true));
        let rect = [5.4, 19.2, 3.6, 4.4];
        arrow_key(
            root,
            ui,
            rect,
            back,
            "KG_Key_C",
            true,
            (38.9, [44.4, 13.6, 72.7]),
        );
        for (i, (_, icon)) in cats.iter().enumerate() {
            let l = 10.0 + i as f32 * 4.85;
            if i == cat {
                root.spawn((
                    abs(l - 0.35, 17.7, 3.9, 7.2),
                    ui.image("sb_in_game/MENU_Cursor_TB").with_color(ORANGE),
                ));
            }
            root.spawn((
                abs(l, 18.3, 3.2, 6.0),
                ui.image(&format!("sb_in_game/{icon}")),
                click(Cmd::Cat(i)),
            ));
        }
        let end = 10.0 + cats.len() as f32 * 4.85;
        let rect = [end, 19.2, 3.5, 4.4];
        arrow_key(
            root,
            ui,
            rect,
            next,
            "KG_Key_V",
            false,
            (40.0, [45.7, 13.6, 72.7]),
        );
    }
    for (k, (i, row)) in entries.iter().enumerate().skip(top).take(ROWS).enumerate() {
        let sel = i == index;
        let t = 26.6 + k as f32 * 8.3;
        root.spawn((
            abs(7.4, t, 4.2, 7.4),
            ui.image("sb_in_game/MENU_IconBase_02"),
            click(Cmd::Row(i)),
        ))
        .with_children(|b| {
            if let Some(icon) = &row.icon {
                b.spawn((abs(6.0, 6.0, 88.0, 88.0), ui.image(icon)));
            }
            if let Some(n) = &row.count {
                b.spawn(Node {
                    position_type: PositionType::Absolute,
                    right: Val::Percent(4.0),
                    bottom: Val::Percent(0.0),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(ui.text(n, 13.0, Color::WHITE));
                });
            }
        });
        let plate = if sel {
            "sb_in_game/MENU_Cursor_03"
        } else {
            "sb_in_game/MENU_ItemBase_02"
        };
        root.spawn((abs(12.3, t + 0.6, 36.6, 6.2), ui.image(plate)))
            .insert(click(Cmd::Row(i)))
            .with_children(|b| {
                b.spawn(Node {
                    padding: UiRect::axes(Val::Percent(4.5), Val::Px(0.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    ..fill_row()
                })
                .with_children(|c| {
                    c.spawn(ui.text(&row.name, 17.0, if sel { Color::WHITE } else { TEXT }));
                });
            });
    }
    // The scroll bar (its thumb the shown share of the list).
    if entries.len() > ROWS {
        root.spawn((
            abs(50.0, 27.5, 0.9, 47.5),
            ui.image("sb_in_game/MENU_Scroll_Base"),
        ));
        let share = ROWS as f32 / entries.len() as f32;
        root.spawn((
            abs(
                50.17,
                27.5 + 47.5 * top as f32 / entries.len() as f32,
                0.55,
                47.5 * share,
            ),
            ui.image("sb_in_game/MENU_Scroll_Bar"),
        ));
    }
    if let Some(d) = entries.get(index) {
        parchment(root, ui, d);
    }
}

/// The parchment on the right: the name on its black bar, the big icon (items) or the story art
/// (skills), the stat lines on ink strokes, the text over faint rules.
fn parchment(root: &mut ChildSpawnerCommands, ui: &Ui, d: &Entry) {
    root.spawn((
        abs(55.5, 14.4, 38.0, 62.3),
        ui.image("sb_in_game/MENU_WP_Base"),
    ));
    root.spawn((
        abs(56.5, 15.6, 36.0, 4.4),
        ui.image("sb_in_game/MENU_ItemBase_08")
            .with_color(Color::srgb(0.25, 0.25, 0.25)),
    ))
    .with_children(|b| {
        b.spawn(Node {
            justify_content: JustifyContent::Center,
            ..fill_row()
        })
        .with_children(|c| {
            c.spawn(ui.text(&d.name, 17.0, Color::WHITE));
        });
    });
    let mut y = 22.0;
    if let Some(art) = &d.art {
        ui.story(root, art);
        y = 38.0;
    } else if let Some(icon) = &d.icon {
        root.spawn((
            abs(81.3, 20.8, 9.4, 16.2),
            ui.image("sb_in_game/MENU_IconBase_01"),
        ))
        .with_children(|b| {
            b.spawn((abs(8.0, 8.0, 84.0, 84.0), ui.image(icon)));
        });
        y = 30.0;
    }
    for (label, value) in &d.stats {
        root.spawn((
            abs(57.0, y, 22.5, 3.6),
            ui.image("sb_in_game/MENU_ItemBase_08")
                .with_color(Color::srgba(0.5, 0.5, 0.5, 0.9)),
        ))
        .with_children(|b| {
            b.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)),
                ..fill_row()
            })
            .with_children(|c| {
                c.spawn(ui.text(label, 15.5, TEXT));
                c.spawn((Node {
                    margin: UiRect::right(Val::Percent(12.0)),
                    ..default()
                },))
                    .with_children(|v| {
                        v.spawn(ui.text(value, 15.5, TEXT));
                    });
            });
        });
        y += 4.4;
    }
    y += 1.0;
    // The text starts below the big icon even with no stat lines beside it (Equipment).
    if d.art.is_none() && d.icon.is_some() {
        y = y.max(38.0);
    }
    // Faint rules under the text lines (the game's ruled parchment).
    let mut r = y + 3.2;
    while r < 75.5 {
        root.spawn((
            abs(57.3, r, 34.5, 0.15),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18)),
        ));
        r += 3.2;
    }
    root.spawn(abs(57.6, y, 34.0, 75.0 - y)).with_children(|c| {
        c.spawn(ui.text(&d.info, 15.5, TEXT));
    });
}

/// The status bar along the bottom (all but Options): resurrection nodes over the Vitality bar,
/// Unseen Aid, Vitality / Attack Power, the skill level and its gauge, Spirit Emblems and Sen.
/// gap: attribute levels and Unseen Aid are not modelled: Wolf's start values.
fn status_bar(root: &mut ChildSpawnerCommands, ui: &Ui, kit: &Kit, book: &Book, wolf: (f32, f32)) {
    let plate = Color::srgba(0.06, 0.06, 0.06, 0.78);
    let (t, h) = (79.2, 8.8);
    let (hp, hp_max) = wolf;
    // Resurrection and Vitality.
    root.spawn((abs(7.1, t, 19.1, h), BackgroundColor(plate)))
        .with_children(|b| {
            for k in 0..kit.resurrections.max(1) {
                let lit = k < kit.resurrections;
                b.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(14.0 + k as f32 * 22.0),
                        top: Val::Px(6.0),
                        width: Val::Px(18.0),
                        height: Val::Px(18.0),
                        ..default()
                    },
                    ui.image("sb_fe/MENU_Revive_Gauge").with_color(if lit {
                        Color::WHITE
                    } else {
                        Color::srgba(0.4, 0.4, 0.4, 0.6)
                    }),
                ));
            }
            b.spawn((
                abs(5.0, 50.0, 90.0, 9.0),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            ));
            b.spawn((
                abs(
                    5.0,
                    50.0,
                    90.0 * (hp / hp_max.max(1.0)).clamp(0.0, 1.0),
                    9.0,
                ),
                BackgroundColor(Color::srgb(0.62, 0.17, 0.12)),
            ));
        });
    // Unseen Aid (gap: no Dragonrot; the game's base 30 %).
    root.spawn((abs(27.1, t, 15.2, h), BackgroundColor(plate)))
        .with_children(|b| {
            b.spawn(Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::left(Val::Percent(30.0)),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn(ui.text("Unseen Aid", 16.5, TEXT));
                c.spawn(ui.text("30%", 16.5, TEXT));
            });
        });
    // Vitality / Attack Power.
    root.spawn((abs(43.4, t, 15.2, h), BackgroundColor(plate)))
        .with_children(|b| {
            b.spawn(Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(2.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)),
                ..default()
            })
            .with_children(|c| {
                for (icon, label, value) in [
                    ("MENU_ST_Vitality", "Vitality", PLAYER_LEVEL.to_string()),
                    ("MENU_ST_Offensive", "Attack Power", book.attack.to_string()),
                ] {
                    c.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|r| {
                        r.spawn((
                            Node {
                                width: Val::Px(20.0),
                                height: Val::Px(20.0),
                                ..default()
                            },
                            ui.image(&format!("sb_in_game/{icon}")),
                        ));
                        r.spawn((Node {
                            flex_grow: 1.0,
                            ..default()
                        },))
                            .with_children(|n| {
                                n.spawn(ui.text(label, 16.0, TEXT));
                            });
                        r.spawn(ui.text(&value, 16.0, TEXT));
                    });
                }
            });
        });
    // The skill level and the gauge to the next point.
    root.spawn((abs(59.5, t, 20.0, h), BackgroundColor(plate)))
        .with_children(|b| {
            b.spawn((
                abs(2.0, 12.0, 11.0, 40.0),
                ui.image("sb_fe/MENU_Skill_Lv")
                    .with_color(Color::srgb(0.15, 0.15, 0.15)),
            ))
            .with_children(|d| {
                d.spawn(Node {
                    justify_content: JustifyContent::Center,
                    ..fill_row()
                })
                .with_children(|c| {
                    c.spawn(ui.text(&kit.skill_points.to_string(), 14.0, SKILL_BLUE));
                });
            });
            b.spawn((
                abs(14.6, 33.0, 58.8 * kit.xp_fill(), 4.0),
                BackgroundColor(SKILL_BLUE),
            ));
            b.spawn((
                abs(14.0, 25.0, 60.0, 12.0),
                ui.image("sb_in_game/MENU_GaugeFrame"),
            ));
            b.spawn((Node {
                position_type: PositionType::Absolute,
                right: Val::Percent(5.0),
                bottom: Val::Percent(14.0),
                ..default()
            },))
                .with_children(|c| {
                    let left = kit.xp_left();
                    c.spawn(ui.text(&format!("to next Skill Point  {left}"), 14.0, SKILL_BLUE));
                });
        });
    // Spirit Emblems and Sen.
    root.spawn((abs(80.5, t, 12.8, h), BackgroundColor(plate)))
        .with_children(|b| {
            b.spawn(Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(2.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)),
                ..default()
            })
            .with_children(|c| {
                let n = kit.spirit_emblems;
                for (icon, value) in [
                    (book.emblem.as_deref().unwrap_or(""), format!("{n}/15")),
                    ("sb_fe/MENU_Money", kit.sen.to_string()),
                ] {
                    c.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|r| {
                        r.spawn((
                            Node {
                                width: Val::Px(22.0),
                                height: Val::Px(22.0),
                                ..default()
                            },
                            ui.image(icon),
                        ));
                        r.spawn((Node {
                            flex_grow: 1.0,
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        },))
                            .with_children(|n| {
                                n.spawn(ui.text(&value, 16.0, TEXT));
                            });
                    });
                }
            });
        });
}

/// Inventory's entries for a category: Quick Items (the Healing Gourd, the Homeward Idol), Key
/// Items (Kusabimaru, the Shinobi Prosthetic), Upgrade Materials, Prosthetic Arm Tools (those
/// fitted). Counts and the Storage come from the kit, so picked-up items show here.
fn inventory(cat: usize, kit: &Kit, book: &Book) -> Vec<Entry> {
    let (texts, tables) = (&book.texts, &book.tables);
    let entry = |slot: Slot, count: Option<u32>, stats: Vec<(String, String)>| {
        let (name, info) = slot_text(texts, &slot);
        Entry {
            icon: slot_icon(tables, &slot),
            name,
            count: count.map(|n| n.to_string()),
            stats,
            info,
            usable: match slot {
                Slot::Goods(id, _) if tables.goods.get(&id).is_some_and(|g| g.consume) => Some(id),
                _ => None,
            },
            ..default()
        }
    };
    let stocked =
        |(id, n): (i32, u32)| entry(Slot::Goods(id, n), Some(n), goods_stats(id, n, kit, book));
    match cat {
        0 => std::iter::once((HEALING_GOURD, kit.count(HEALING_GOURD)))
            .chain(held_goods(kit, tables, 0))
            .map(stocked)
            .collect(),
        1 => book
            .key_items
            .iter()
            .map(|id| {
                let held = ("Number Held".to_string(), "1/ 1".to_string());
                entry(Slot::Goods(*id, 1), None, vec![held, in_storage(kit, *id)])
            })
            .chain(
                held_goods(kit, tables, 1)
                    .into_iter()
                    .map(|(id, n)| entry(Slot::Goods(id, n), (n > 1).then_some(n), vec![])),
            )
            .collect(),
        2 => held_goods(kit, tables, 2)
            .into_iter()
            .map(stocked)
            .collect(),
        _ => kit
            .prosthetics
            .iter()
            .map(|id| entry(Slot::Weapon(*id), None, vec![]))
            .collect(),
    }
}

/// A held goods' stat lines on the parchment, in Inventory and Equipment alike: the gourd's
/// uses of its maximum or the number held of the carry limit, then the number in the Storage.
fn goods_stats(id: i32, n: u32, kit: &Kit, book: &Book) -> Vec<(String, String)> {
    let held = if id == HEALING_GOURD {
        ("Uses".to_string(), format!("{n}/{}", kit.gourd_max))
    } else {
        let carry = book.items.rule(id).carry;
        ("Number Held".to_string(), format!("{n}/ {carry}"))
    };
    vec![held, in_storage(kit, id)]
}

/// The "In Storage" stat line: the Storage's count of `id`, "-" for none.
fn in_storage(kit: &Kit, id: i32) -> (String, String) {
    let n = match kit.stored(id) {
        0 => "-".into(),
        n => n.to_string(),
    };
    ("In Storage".to_string(), n)
}

/// Skills' entries for a category: the learned skills (Kit::skills; with all_skills, all of
/// them), each with its story art (sb_icon_skill MENU_SkillStory_<iconId>).
fn skills(cat: usize, kit: &Kit, book: &Book) -> Vec<Entry> {
    let tables = &book.tables;
    learned(cat, kit, book)
        .map(|s| {
            let slot = Slot::Weapon(s.weapon);
            let (name, info) = slot_text(&book.texts, &slot);
            Entry {
                icon: slot_icon(tables, &slot),
                name,
                art: tables
                    .weapon_icon
                    .get(&s.weapon)
                    .map(|i| format!("story/MENU_SkillStory_{i:05}")),
                info,
                ..default()
            }
        })
        .collect()
}

/// The learned skills of a category (`Kit::skills`; with `all_skills`, all of them).
fn learned<'a>(cat: usize, kit: &'a Kit, book: &'a Book) -> impl Iterator<Item = &'a Skill> {
    book.tables.skills[cat]
        .iter()
        .filter(|s| kit.all_skills || kit.skills.contains(&s.id))
}

fn equipment(root: &mut ChildSpawnerCommands, ui: &Ui, menu: &GameMenu, kit: &Kit, book: &Book) {
    let tables = &book.tables;
    let all = slots(kit);
    // Section bars.
    for (label, l, t) in [
        ("Prosthetic Tools", 6.5, 16.5),
        ("Combat Arts", 38.5, 16.5),
        ("Quick Items", 6.5, 45.0),
        ("Ninjutsu", 38.5, 45.0),
    ] {
        root.spawn((abs(l, t, 17.0, 4.5), ui.image("sb_in_game/MENU_Caption_02")))
            .with_children(|b| {
                b.spawn(Node {
                    padding: UiRect::left(Val::Percent(6.0)),
                    ..fill_row()
                })
                .with_children(|c| {
                    c.spawn(ui.text(label, 17.0, TEXT));
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
        root.spawn((abs(l, t, w, size), ui.image("sb_in_game/MENU_IconBase_02")))
            .insert(click(Cmd::Row(i)))
            .with_children(|s| {
                // The cursor glows behind the item, not over it.
                if i == menu.index {
                    s.spawn((
                        abs(-8.0, -8.0, 116.0, 116.0),
                        ui.image("sb_in_game/MENU_Cursor_TB")
                            .with_color(ORANGE.with_alpha(0.55)),
                    ));
                }
                if let Some(icon) = slot_icon(tables, slot) {
                    s.spawn((abs(4.0, 4.0, 92.0, 92.0), ui.image(&icon)));
                }
                if let Slot::Goods(_, n) = slot {
                    s.spawn((Node {
                        position_type: PositionType::Absolute,
                        right: Val::Percent(6.0),
                        bottom: Val::Percent(2.0),
                        ..default()
                    },))
                        .with_children(|c| {
                            c.spawn(ui.text(&n.to_string(), 16.0, Color::WHITE));
                        });
                }
            });
    }
    // The item under the cursor on the parchment.
    let slot = &all[menu.index.min(all.len() - 1)];
    parchment(root, ui, &slot_entry(slot, kit, book));
}

/// An Equipment slot's parchment: its name, icon and text, and a quick item's number held and
/// in the Storage, as Inventory shows them (#186: a Pellet picked up over the limit shows here).
fn slot_entry(slot: &Slot, kit: &Kit, book: &Book) -> Entry {
    let (name, info) = slot_text(&book.texts, slot);
    Entry {
        name,
        icon: slot_icon(&book.tables, slot),
        info,
        stats: match slot {
            Slot::Goods(id, n) => goods_stats(*id, *n, kit, book),
            _ => Vec::new(),
        },
        ..default()
    }
}

fn build(
    commands: &mut Commands,
    ui: &Ui,
    menu: &GameMenu,
    kit: &Kit,
    book: &Book,
    wolf: (f32, f32),
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(110),
            GameMenuRoot,
            game_ui::Covers,
        ))
        .with_children(|root| {
            root.spawn((
                abs(0.0, 0.0, 100.0, 100.0),
                ui.image("sb_in_game/MENU_Base_00"),
            ));
            // Tabs, with the switch keys and the brush arrows at either end.
            let (back, next) = (Cmd::TabStep(false), Cmd::TabStep(true));
            let rect = [5.3, 6.0, 3.8, 4.6];
            arrow_key(
                root,
                ui,
                rect,
                back,
                "KG_Key_Z",
                true,
                (34.2, [47.4, 8.7, 73.9]),
            );
            let rect = [90.0, 6.0, 3.7, 4.6];
            arrow_key(
                root,
                ui,
                rect,
                next,
                "KG_Key_X",
                false,
                (35.1, [48.6, 8.7, 73.9]),
            );
            for (i, name) in TABS.iter().enumerate() {
                let open = i == menu.tab;
                let brush = if open {
                    "sb_in_game/MENU_Cursor_01"
                } else {
                    "sb_in_game/MENU_Caption_01"
                };
                root.spawn((abs(11.5 + i as f32 * 19.0, 4.2, 18.0, 9.0), ui.image(brush)))
                    .insert(click(Cmd::Tab(i)))
                    .with_children(|t| {
                        t.spawn(Node {
                            justify_content: JustifyContent::Center,
                            ..fill_row()
                        })
                        .with_children(|c| {
                            c.spawn(ui.text(name, 18.0, if open { Color::WHITE } else { TEXT }));
                        });
                    });
            }
            // The rule under the tabs.
            root.spawn((
                abs(7.0, 13.6, 86.0, 0.25),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ));
            match menu.tab {
                0 => equipment(root, ui, menu, kit, book),
                1 | 2 => {
                    let k = menu.tab - 1;
                    let (cats, list) = if menu.tab == 1 {
                        (&INV_CATS, inventory(menu.cat[k], kit, book))
                    } else {
                        (&SKILL_CATS, skills(menu.cat[k], kit, book))
                    };
                    list_screen(root, ui, (cats, menu.cat[k]), &list, (menu.index, menu.top));
                }
                _ => {
                    for (i, label) in PAUSE_ROWS.iter().enumerate() {
                        let sel = i == menu.index;
                        let plate = if sel {
                            "sb_in_game/MENU_Cursor_01"
                        } else {
                            "sb_in_game/MENU_Caption_01"
                        };
                        root.spawn((abs(6.8, 18.0 + i as f32 * 7.55, 22.2, 6.4), ui.image(plate)))
                            .insert(click(Cmd::Row(i)))
                            .with_children(|b| {
                                b.spawn(Node {
                                    justify_content: JustifyContent::Center,
                                    ..fill_row()
                                })
                                .with_children(|c| {
                                    c.spawn(ui.text(
                                        label,
                                        17.0,
                                        if sel { Color::WHITE } else { TEXT },
                                    ));
                                });
                            });
                    }
                }
            }
            if menu.tab != 3 {
                status_bar(root, ui, kit, book, wolf);
            }
            // The key guide: help on the left, the keys on the right.
            let help = match menu.tab {
                0 => "Check equipped items.",
                1 => "Check items in your possession.",
                2 => "View unlocked Skills",
                _ => "Resume, return to the start menu, or quit.",
            };
            root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                c.spawn(ui.text(help, 16.0, DIM));
            });
            key_guide(root, ui, &book.guides);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Option<Book> {
        let cache = sekiro_sim::cache::root();
        if !cache.join("params/SkillParam.json").exists() {
            eprintln!("skipping: cache not extracted");
            return None;
        }
        let items = ItemData::load(&cache).ok()?;
        Some(Book::load(&cache, 1, Arc::new(items)))
    }

    #[test]
    fn the_book_names_items_and_classifies_skills() {
        let Some(book) = book() else {
            return;
        };
        for (slot, name) in [
            (Slot::Weapon(70000), "Loaded Shuriken"),
            (Slot::Weapon(5100), "Whirlwind Slash"),
            (Slot::Goods(3000, 0), "Healing Gourd"),
        ] {
            assert_eq!(slot_text(&book.texts, &slot).0, name);
        }
        // Ninjutsu are goods, not SkillParam rows: that category stays empty.
        for cat in &book.tables.skills[..3] {
            assert!(!cat.is_empty());
        }
        assert!(
            book.tables.skills[0]
                .iter()
                .any(|s| book.texts.weapon_name[&s.weapon] == "Whirlwind Slash")
        );
    }

    #[test]
    fn inventory_shows_the_gourd_and_held_materials() {
        let Some(book) = book() else {
            return;
        };
        let kit = Kit::default();
        let quick = inventory(0, &kit, &book);
        assert_eq!(quick[0].name, "Healing Gourd");
        assert_eq!(quick[0].count.as_deref(), Some("3"));
        assert_eq!(inventory(1, &kit, &book)[0].name, "Kusabimaru");
        let materials = inventory(2, &kit, &book);
        assert!(
            materials
                .iter()
                .any(|r| r.name == "Scrap Iron" && r.count.as_deref() == Some("20"))
        );
    }

    #[test]
    fn picked_up_goods_change_what_the_inventory_lists() {
        let Some(book) = book() else {
            return;
        };
        let mut kit = Kit::default();
        // What pick_up does with the lots of #150: 2 Pellets (3020) with 3 of 3 held, and Scrap
        // Iron (6000) with room to carry.
        for (id, n) in [(3020, 2), (6000, 2)] {
            kit.add(id, n, book.items.rule(id));
        }
        // A gourd use drunk: its line keeps the maximum (#184).
        assert!(kit.use_goods(HEALING_GOURD));
        let row = |cat, name: &str| {
            inventory(cat, &kit, &book)
                .into_iter()
                .find(|r| r.name == name)
                .unwrap_or_else(|| panic!("{name} is not listed"))
        };
        let stored = ("In Storage".to_string(), "2".to_string());
        let pellets = row(0, "Pellet");
        assert_eq!(pellets.count.as_deref(), Some("3"));
        assert_eq!(
            pellets.stats,
            [("Number Held".into(), "3/ 3".into()), stored.clone()]
        );
        let gourd = &row(0, "Healing Gourd").stats[0];
        assert_eq!(gourd, &("Uses".to_string(), "2/3".to_string()));
        assert_eq!(row(2, "Scrap Iron").count.as_deref(), Some("22"));
        // #186: the menu opens on Equipment, where the Pellets are a quick item; their parchment
        // there showed no stat lines, so the Storage's 2 never appeared.
        let slots = slots(&kit);
        let quick = slots
            .iter()
            .find(|s| matches!(s, Slot::Goods(3020, _)))
            .unwrap();
        let equipped = slot_entry(quick, &kit, &book);
        assert_eq!(equipped.name, "Pellet");
        assert!(equipped.stats.contains(&stored), "{:?}", equipped.stats);
    }

    #[test]
    fn equipment_follows_the_ten_slots_of_the_kit() {
        let Some(book) = book() else {
            return;
        };
        let kit = Kit::default();
        let slots = slots(&kit);
        assert_eq!(slot_text(&book.texts, &slots[0]).0, "Loaded Shuriken");
        assert_eq!(slot_text(&book.texts, &slots[3]).0, "Whirlwind Slash");
        for (slot, id) in slots[4..9].iter().zip(kit.quick_items) {
            assert!(matches!(slot, Slot::Goods(i, n) if *i == id && *n == kit.count(id)));
            assert!(!slot_text(&book.texts, slot).0.is_empty());
        }
        assert!(matches!(slots[9], Slot::Empty));
        assert_eq!(
            slot_icon(&book.tables, &slots[4]).as_deref(),
            Some("item/MENU_ItemIcon_00151")
        );
    }

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
    }

    /// Clicks on the drawn menu go through picking to the same commands as the keys: a tab
    /// click opens it, a row click selects, a second click on it uses the Quick Item, and an
    /// Options row acts on one click. The keyboard cursor never arms a row.
    #[test]
    fn clicks_switch_tabs_select_rows_and_use_items() {
        let Some(book) = book() else {
            return;
        };
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::ui_widgets::ButtonPlugin))
            .init_resource::<Clicks>();
        let mut menu = GameMenu::default();
        let mut kit = Kit::default();
        let mut requests = Vec::new();
        let mut click_on = |app: &mut App, menu: &mut GameMenu, kit: &mut Kit, cmd: Cmd| {
            // Redraw the menu as `show` does, then press and release on the element.
            let world = app.world_mut();
            let roots: Vec<_> = world
                .query_filtered::<Entity, With<GameMenuRoot>>()
                .iter(world)
                .collect();
            for root in roots {
                world.entity_mut(root).despawn();
            }
            let (art, fonts) = (MenuArt::default(), GameFonts::default());
            let ui = Ui {
                art: &art,
                fonts: &fonts,
                aspect: 16.0 / 9.0,
            };
            build(&mut world.commands(), &ui, menu, kit, &book, (1.0, 1.0));
            world.flush();
            let entity = world
                .query::<(Entity, &Cmd)>()
                .iter(world)
                .find_map(|(e, c)| (*c == cmd).then_some(e))
                .unwrap_or_else(|| panic!("no element sends {cmd:?}"));
            crate::menu::test_click(world, entity, entity);
            app.update();
            let clicks = std::mem::take(&mut app.world_mut().resource_mut::<Clicks>().0);
            assert_eq!(clicks, [cmd], "one click sends its command once");
            requests.extend(apply(menu, cmd, kit, &book));
        };
        let gourd = HEALING_GOURD;
        click_on(&mut app, &mut menu, &mut kit, Cmd::Tab(1));
        assert_eq!((menu.tab, menu.cat[0], menu.index), (1, 0, 0));
        click_on(&mut app, &mut menu, &mut kit, Cmd::Row(0));
        assert_eq!(
            (menu.index, kit.count(gourd)),
            (0, 3),
            "the first click on the gourd under the opening cursor only selects it"
        );
        click_on(&mut app, &mut menu, &mut kit, Cmd::Row(0));
        assert_eq!(kit.count(gourd), 2, "a second click uses the Healing Gourd");
        click_on(&mut app, &mut menu, &mut kit, Cmd::Row(1));
        assert_eq!(
            (menu.index, kit.count(gourd)),
            (1, 2),
            "another row: selected"
        );
        // The keys move the cursor back to the gourd: one click there only selects.
        apply(&mut menu, Cmd::Step(false), &mut kit, &book);
        assert_eq!(menu.index, 0);
        click_on(&mut app, &mut menu, &mut kit, Cmd::Row(0));
        assert_eq!(
            kit.count(gourd),
            2,
            "a keyboard-selected row is not used by one click"
        );
        click_on(&mut app, &mut menu, &mut kit, Cmd::Tab(3));
        click_on(&mut app, &mut menu, &mut kit, Cmd::Row(1));
        assert_eq!(requests, [1], "Back to start menu acts on one click");
    }

    /// Tab and category steps wrap, and a category step in the same frame as a tab step
    /// applies to the tab it opened (X then V from Equipment).
    #[test]
    fn steps_wrap_and_apply_in_order() {
        let Some(book) = book() else {
            return;
        };
        let (mut menu, mut kit) = (GameMenu::default(), Kit::default());
        for cmd in [Cmd::TabStep(true), Cmd::CatStep(true)] {
            apply(&mut menu, cmd, &mut kit, &book);
        }
        assert_eq!((menu.tab, menu.cat), (1, [1, 0]));
        apply(&mut menu, Cmd::CatStep(false), &mut kit, &book);
        apply(&mut menu, Cmd::CatStep(false), &mut kit, &book);
        assert_eq!(menu.cat, [3, 0], "categories wrap");
        apply(&mut menu, Cmd::TabStep(false), &mut kit, &book);
        apply(&mut menu, Cmd::TabStep(false), &mut kit, &book);
        assert_eq!(menu.tab, 3, "tabs wrap");
        apply(&mut menu, Cmd::CatStep(true), &mut kit, &book);
        assert_eq!(menu.cat, [3, 0], "Options has no categories");
    }

    /// The rows counted for the cursor are the rows drawn, on every screen.
    #[test]
    fn row_count_matches_the_drawn_lists() {
        let Some(book) = book() else {
            return;
        };
        let kit = Kit::default();
        let mut menu = GameMenu::default();
        assert_eq!(row_count(&menu, &kit, &book), slots(&kit).len());
        menu.tab = 3;
        assert_eq!(row_count(&menu, &kit, &book), PAUSE_ROWS.len());
        for cat in 0..4 {
            menu.tab = 1;
            menu.cat = [cat, cat];
            let n = inventory(cat, &kit, &book).len().max(1);
            assert_eq!(row_count(&menu, &kit, &book), n, "inventory category {cat}");
            menu.tab = 2;
            let n = skills(cat, &kit, &book).len().max(1);
            assert_eq!(row_count(&menu, &kit, &book), n, "skill category {cat}");
        }
    }
}
