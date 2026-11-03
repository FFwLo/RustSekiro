//! Travel between Sculptor's Idols (#148): the idol menu's Travel row opens the game's list of
//! the idols Wolf has found (`Kit::found_idols`), and picking one plays the warp
//! (`Duel::travel_to_idol`) under a fade to black.
//!
//! The list is BonfireWarpParam's: one page per `menuPageNo` (0 "Important Places", then the
//! areas, titled NTC_メニューテキスト 211100 + page, whose `<?placeName@n?>` is 地名 n), each
//! listing its rows' idols in row id order by name (`msgId`). Only pages with a found idol
//! show, and only found idols; an idol on two pages (the Dilapidated Temple on Important
//! Places and Ashina Outskirts) is listed on both. After FwLo's RustSekiro `src/idol.rs`
//! (`warp_rows`, `page_name`, `regions` and its Travel screen, laid out from captures of the
//! game): the regions in a column on the left, the idols of the picked one on a parchment on
//! the right.
//!
//! The screen opens on the idol Wolf prays at, and takes the keys, the pad and the mouse (the
//! pointer over a region or idol moves the cursor there, a click is OK), as the idol menu does.
//!
//! An idol of this play world is a teleport in the sim. One on another map ends this run and
//! starts the game again on that map (`crate::menu::relaunch_with`, the path the start menu's
//! map changes take), at the idol (`--at-idol`) with the kit carried over (`--kit`) and the
//! session flags a return to the start menu keeps (`crate::menu::kept_args`). That run is
//! checked before this one ends, as the start menu checks its runs: an idol whose run would
//! not start is greyed and refused, and the reason logged.
//! gap: the game streams the other map in the same run; the grey-out flags
//! (`grayoutEventflagId`), the page's area art (`menuImageId`) and Wolf's marker by his area
//! are not modelled.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::CursorMoved;
use sekiro_sim::duel::Duel;
use sekiro_sim::duel::idol::{Destination, IdolPhase, WARP_CUT};

use super::Sim;
use super::idol::{
    CURSOR, DIM, GREYED, HELP, Idols, KEY_OK, Pointed, TEXT, TRAVEL, WARP_POINT, int, pointable,
};
use crate::game_ui::{GameFonts, MenuArt, abs};

/// NTC_メニューテキスト: the screen's title ("Travel") and page 0's ("Important Places").
const TITLE: i32 = 211000;
const PAGES: i32 = 211100;
/// NTC_キーガイド: "Back".
const KEY_BACK: i32 = 110000;
/// Seconds the black takes to lift after an arrival (the death screen's lift).
const LIFT: f32 = 0.6;

/// One idol of the game's Travel list.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Dest {
    pub entity: i32,
    pub page: usize,
    pub name: String,
    /// Its map (`m11_00_00_00`), from the entity's area and block digits.
    pub map: String,
}

/// The game's Travel list: the page titles and every BonfireWarpParam row's idol in row order,
/// with the maps that are extracted.
#[derive(Debug, Default)]
pub(super) struct Atlas {
    pages: BTreeMap<usize, String>,
    dests: Vec<Dest>,
    maps: BTreeSet<String>,
}

impl Atlas {
    /// From BonfireWarpParam `rows` in id order and the menu texts `names`.
    pub fn load<'a>(
        cache: &Path,
        rows: impl IntoIterator<Item = &'a serde_json::Value>,
        names: &BTreeMap<i32, String>,
    ) -> Self {
        let places = crate::game_ui::texts(cache, "item", "地名");
        let mut atlas = Self::from_rows(rows, names, &places);
        atlas.maps = atlas
            .dests
            .iter()
            .map(|d| d.map.clone())
            .filter(|m| cache.join("maps").join(m).join("layout.json").is_file())
            .collect();
        atlas
    }

    fn from_rows<'a>(
        rows: impl IntoIterator<Item = &'a serde_json::Value>,
        names: &BTreeMap<i32, String>,
        places: &BTreeMap<i32, String>,
    ) -> Self {
        let mut atlas = Self::default();
        for r in rows {
            let (Some(entity), Some(page)) = (
                int(&r["bonfireEntityId"]).filter(|e| *e > 0),
                int(&r["menuPageNo"]).and_then(|p| usize::try_from(p).ok()),
            ) else {
                continue;
            };
            let name = int(&r["msgId"])
                .and_then(|m| names.get(&m).cloned())
                .unwrap_or_else(|| format!("Idol {entity}"));
            atlas.pages.entry(page).or_insert_with(|| {
                let title = names.get(&(PAGES + page as i32)).cloned();
                page_title(title.unwrap_or_default(), places)
            });
            atlas.dests.push(Dest {
                entity,
                page,
                name,
                map: map_of(entity),
            });
        }
        atlas
    }

    /// The pages with a `found` idol, in page order: the title and the indices of its found
    /// idols in `dests`.
    fn regions(&self, found: &BTreeSet<i32>) -> Vec<(String, Vec<usize>)> {
        self.pages
            .iter()
            .filter_map(|(page, title)| {
                let list: Vec<usize> = (self.dests.iter().enumerate())
                    .filter(|(_, d)| d.page == *page && found.contains(&d.entity))
                    .map(|(i, _)| i)
                    .collect();
                (!list.is_empty()).then(|| (title.clone(), list))
            })
            .collect()
    }

    /// Where idol `entity` is in `regions`: the first region listing it and its row there.
    fn find(&self, regions: &[(String, Vec<usize>)], entity: i32) -> Option<(usize, usize)> {
        regions.iter().enumerate().find_map(|(r, (_, list))| {
            let k = list.iter().position(|&d| self.dests[d].entity == entity)?;
            Some((r, k))
        })
    }

    /// Whether a Travel can reach `d`: in this play world, or on a map that is extracted.
    fn reachable(&self, d: &Dest, idols: &Idols) -> bool {
        idols.index(d.entity).is_some() || self.maps.contains(&d.map)
    }
}

/// A page title with its `<?placeName@n?>` tag resolved from `places` (地名).
fn page_title(title: String, places: &BTreeMap<i32, String>) -> String {
    title
        .strip_prefix("<?placeName@")
        .and_then(|r| r.strip_suffix("?>"))
        .and_then(|n| n.parse().ok())
        .and_then(|n| places.get(&n).cloned())
        .unwrap_or(title)
}

/// The map an idol's entity is in: Sekiro's entity ids start with the map's area and block
/// (1111950 is in m11_01_00_00), as FwLo's `sr_map.rs` `map_of` reads them. Every
/// BonfireWarpParam idol and its warp point are in that map's MSB.
fn map_of(entity: i32) -> String {
    format!(
        "m{:02}_{:02}_00_00",
        entity / 100_000,
        (entity / 10_000) % 10
    )
}

/// The Travel screen's state.
#[derive(Resource, Default)]
pub(super) struct TravelMenu {
    pub open: bool,
    /// The picked region, and the picked idol in it while the cursor is in the list.
    region: usize,
    index: usize,
    in_list: bool,
    /// Opened this frame: the OK that opened it is not a pick.
    just_opened: bool,
    /// The found idols' regions as the screen opened.
    regions: Vec<(String, Vec<usize>)>,
    /// Idols on another map whose run would not start (`validate`): greyed, and refused.
    refused: BTreeSet<i32>,
}

impl TravelMenu {
    /// Opens the screen on the idol Wolf prays at: its region, with the cursor on its row in
    /// the list.
    pub fn open_at(&mut self, idols: &Idols, duel: &Duel) {
        let regions = idols.atlas.regions(&duel.kit.found_idols);
        let at = duel
            .idol()
            .and_then(|i| idols.atlas.find(&regions, idols.spots[i].entity));
        *self = Self {
            open: !regions.is_empty(),
            region: at.map_or(0, |(r, _)| r),
            index: at.map_or(0, |(_, k)| k),
            in_list: at.is_some(),
            just_opened: true,
            regions,
            refused: std::mem::take(&mut self.refused),
        };
    }

    /// Whether the screen lets Wolf travel to `d`: not where he is, reachable and not refused.
    fn can_pick(&self, d: &Dest, idols: &Idols, here: Option<i32>) -> bool {
        Some(d.entity) != here
            && idols.atlas.reachable(d, idols)
            && !self.refused.contains(&d.entity)
    }

    /// Moves the cursor onto region `region`, out of the list; a new region starts its list
    /// at its first idol.
    fn cursor_to_region(&mut self, region: usize) {
        if region >= self.regions.len() {
            return;
        }
        if region != self.region {
            self.region = region;
            self.index = 0;
        }
        self.in_list = false;
    }

    /// Moves the cursor onto row `index` of the region's list.
    fn cursor_to_idol(&mut self, index: usize) {
        if index < self.regions.get(self.region).map_or(0, |(_, l)| l.len()) {
            self.index = index;
            self.in_list = true;
        }
    }
}

/// A clickable part of the Travel screen: a region's plate or a row of its list.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Part {
    Region(usize),
    Idol(usize),
}

/// What the screen shows: its region, idol, column, refusals and the idol Wolf prays at.
type Shown = Option<(usize, usize, bool, usize, Option<usize>)>;

#[derive(Component)]
struct TravelRoot;
#[derive(Component)]
struct Fade;

/// The screen's texts, resolved once: its title, the Travel row's help line, and the key
/// guide's "OK" and "Back".
#[derive(Resource, Default)]
struct Texts {
    title: String,
    help: String,
    ok: String,
    back: String,
}

impl Texts {
    fn load(events: &BTreeMap<i32, String>) -> Self {
        let cache = sekiro_sim::cache::root();
        let menu = crate::game_ui::menu_texts(&cache, "NTC_メニューテキスト");
        let guides = crate::game_ui::menu_texts(&cache, "NTC_キーガイド");
        let text = |bank: &BTreeMap<i32, String>, id: i32| bank.get(&id).cloned();
        Self {
            title: text(&menu, TITLE).unwrap_or_else(|| "Travel".into()),
            help: text(events, TRAVEL + HELP).unwrap_or_default(),
            ok: text(&guides, KEY_OK).unwrap_or_default(),
            back: text(&guides, KEY_BACK).unwrap_or_default(),
        }
    }
}

pub(super) fn install(app: &mut App, events: &BTreeMap<i32, String>) {
    app.init_resource::<TravelMenu>()
        .init_resource::<Pointed<Part>>()
        .insert_resource(Texts::load(events))
        .add_systems(Startup, spawn_fade)
        .add_systems(
            PreUpdate,
            travel_keys
                .after(super::idol::menu_keys)
                .run_if(crate::menu::active),
        )
        .add_systems(Update, (show, fade, depart).run_if(crate::menu::active));
}

/// The Travel screen's keys and mouse: up and down move the cursor, OK (or right) goes into the
/// region's list and in the list travels, Esc or back (or left in the list) goes back a level.
/// The mouse over a region or idol moves the cursor there, and a click on it is OK.
#[allow(clippy::too_many_arguments)]
fn travel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut moved: MessageReader<CursorMoved>,
    mut pointed: ResMut<Pointed<Part>>,
    mut menu: ResMut<TravelMenu>,
    mut row: ResMut<super::idol::Row>,
    idols: Res<Idols>,
    mut sim: NonSendMut<Sim>,
) {
    let (hovered, clicked) = pointed.take(moved.read().count() > 0);
    if !menu.open {
        return;
    }
    if std::mem::take(&mut menu.just_opened) {
        return;
    }
    if sim.duel.idol_phase() != Some(IdolPhase::Praying) {
        menu.open = false;
        return;
    }
    let pad = |b| pads.iter().any(|p| p.just_pressed(b));
    let key = |k: &[KeyCode]| keys.any_just_pressed(k.iter().copied());
    for part in hovered {
        match part {
            Part::Region(r) => menu.cursor_to_region(r),
            Part::Idol(k) => menu.cursor_to_idol(k),
        }
    }
    let len = if menu.in_list {
        menu.regions[menu.region].1.len()
    } else {
        menu.regions.len()
    };
    let at = if menu.in_list {
        menu.index
    } else {
        menu.region
    };
    let mut to = at;
    if key(&[KeyCode::ArrowDown, KeyCode::KeyS]) || pad(GamepadButton::DPadDown) {
        to = (at + 1) % len;
    }
    if key(&[KeyCode::ArrowUp, KeyCode::KeyW]) || pad(GamepadButton::DPadUp) {
        to = (at + len - 1) % len;
    }
    if menu.in_list {
        menu.index = to;
    } else {
        menu.cursor_to_region(to);
    }
    let mut ok = crate::menu::ok_pressed(&keys, &pads);
    // A click on a region opens its list, on an idol travels there, as OK does.
    if let Some(&part) = clicked.last() {
        match part {
            Part::Region(r) => {
                menu.cursor_to_region(r);
                menu.in_list = true;
                return;
            }
            Part::Idol(k) => {
                menu.cursor_to_idol(k);
                ok = true;
            }
        }
    }
    let back =
        key(&[KeyCode::Escape, KeyCode::Backspace]) || crate::menu::back_pressed(&keys, &pads);
    if !menu.in_list {
        if ok || key(&[KeyCode::ArrowRight, KeyCode::KeyD]) || pad(GamepadButton::DPadRight) {
            menu.in_list = true;
        } else if back {
            menu.open = false;
        }
        return;
    }
    if back || key(&[KeyCode::ArrowLeft, KeyCode::KeyA]) || pad(GamepadButton::DPadLeft) {
        menu.in_list = false;
        return;
    }
    if !ok {
        return;
    }
    let d = &idols.atlas.dests[menu.regions[menu.region].1[menu.index]];
    let here = sim.duel.idol().map(|i| idols.spots[i].entity);
    if !menu.can_pick(d, &idols, here) {
        return;
    }
    let to = match idols.index(d.entity) {
        Some(i) => Destination::Here(i),
        None => {
            // The run on the other map must start before this one ends.
            if let Err(e) = validate(d, &sim.duel.kit) {
                error!("idol travel: {} ({}) cannot start: {e:#}", d.name, d.entity);
                menu.refused.insert(d.entity);
                return;
            }
            Destination::Away(d.entity)
        }
    };
    if sim.duel.travel_to_idol(to) {
        info!("idol travel: to {} ({}, {to:?})", d.name, d.entity);
        menu.open = false;
        // The idol menu opens on Rest again at the next idol.
        *row = default();
    }
}

/// Whether the run a Travel to `d` on another map starts, with `kit`, would start: its options
/// parse, its map is extracted and the idol and its warp point are in its layout, as the start
/// menu checks its run before it leaves.
fn validate(d: &Dest, kit: &sekiro_sim::kit::Kit) -> anyhow::Result<()> {
    let kit = serde_json::to_string(kit)?;
    let mut cli = vec!["sekiro-rs".to_string()];
    cli.extend(travel_args(
        std::env::args().skip(1),
        &d.map,
        d.entity,
        &kit,
    ));
    let options = crate::play_options(&cli)?;
    let dir = crate::map::map_dir(&options.cache, &d.map)?;
    let layout: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
    warp_point(&layout, d.entity)
}

/// Whether `layout` holds idol `entity` and its warp point (the player part `entity -
/// WARP_POINT`), where a run started with `--at-idol` stands Wolf.
fn warp_point(layout: &serde_json::Value, entity: i32) -> anyhow::Result<()> {
    let has = |list: &str, e: i32| {
        layout[list]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| int(&p["entity_id"]) == Some(e))
    };
    anyhow::ensure!(has("objects", entity), "no idol {entity} in the layout");
    anyhow::ensure!(
        has("players", entity - WARP_POINT),
        "no warp point {} for idol {entity} in the layout",
        entity - WARP_POINT
    );
    Ok(())
}

/// Builds the screen again when it opens, closes or its cursor moves.
#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    menu: Res<TravelMenu>,
    idols: Res<Idols>,
    sim: NonSend<Sim>,
    fonts: Res<GameFonts>,
    art: Res<MenuArt>,
    texts: Res<Texts>,
    roots: Query<Entity, With<TravelRoot>>,
    mut shown: Local<Shown>,
) {
    let now = menu.open.then_some((
        menu.region,
        menu.index,
        menu.in_list,
        menu.refused.len(),
        sim.duel.idol(),
    ));
    if *shown == now {
        return;
    }
    *shown = now;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !menu.open {
        return;
    }
    let here = sim.duel.idol().map(|i| idols.spots[i].entity);
    let sprite = |name: &str| {
        art.sprite("SB_In_Game", name)
            .map(|(image, _)| ImageNode {
                image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                ..image
            })
            .unwrap_or_default()
    };
    let text = |t: &str, px: f32, color: Color| fonts.shadowed(t, px, color);
    let row = Node {
        width: percent(100),
        height: percent(100),
        align_items: AlignItems::Center,
        ..default()
    };
    commands
        .spawn((
            TravelRoot,
            crate::game_ui::Covers,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(111),
        ))
        .with_children(|root| {
            root.spawn((abs(0.0, 0.0, 100.0, 100.0), sprite("MENU_Base_00")));
            root.spawn(abs(5.9, 7.6, 30.0, 5.0))
                .with_child(text(&texts.title, 21.0, Color::WHITE));
            root.spawn((
                abs(0.0, 12.8, 32.0, 0.2),
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)),
            ));
            for (i, (name, _)) in menu.regions.iter().enumerate() {
                let t = 16.0 + i as f32 * 8.2;
                let sel = i == menu.region;
                let plate = if sel {
                    "MENU_Cursor_03"
                } else {
                    "MENU_ItemBase_02"
                };
                let tint = match (sel, menu.in_list) {
                    (true, true) => CURSOR.with_alpha(0.55),
                    (true, false) => CURSOR,
                    _ => Color::WHITE,
                };
                root.spawn((
                    abs(6.8, t, 34.5, 6.4),
                    sprite(plate).with_color(tint),
                    pointable(Part::Region(i)),
                ))
                .with_child((
                    Node {
                        padding: UiRect::left(percent(5.0)),
                        ..row.clone()
                    },
                    children![text(name, 17.0, if sel { Color::WHITE } else { TEXT })],
                ));
            }
            // The region's idols on a parchment, and the picture panel under it.
            root.spawn((abs(43.2, 14.6, 50.0, 42.6), sprite("MENU_WP_Base")));
            root.spawn((
                abs(43.2, 58.4, 50.0, 30.6),
                sprite("MENU_WP_Base").with_color(Color::srgb(0.6, 0.6, 0.6)),
            ));
            if let Some((_, list)) = menu.regions.get(menu.region) {
                // Rows closer together when a region has many (the parchment is 40 high).
                let step = (40.0 / list.len().max(1) as f32).min(5.6);
                for (k, &d) in list.iter().enumerate() {
                    let d = &idols.atlas.dests[d];
                    let sel = menu.in_list && k == menu.index;
                    let plate = if sel {
                        "MENU_Cursor_03"
                    } else {
                        "MENU_ItemBase_07"
                    };
                    let colour = match (menu.can_pick(d, &idols, here), sel) {
                        (false, _) => GREYED,
                        (true, true) => Color::WHITE,
                        (true, false) => TEXT,
                    };
                    root.spawn((
                        abs(45.8, 16.2 + k as f32 * step, 42.6, step.min(4.6)),
                        sprite(plate).with_color(if sel {
                            CURSOR
                        } else {
                            Color::srgb(0.5, 0.5, 0.5)
                        }),
                        pointable(Part::Idol(k)),
                    ))
                    .with_child((
                        Node {
                            justify_content: JustifyContent::Center,
                            ..row.clone()
                        },
                        children![text(&d.name, 16.5, colour)],
                    ));
                }
            }
            root.spawn(abs(5.9, 89.6, 30.0, 4.0))
                .with_child(text(&texts.help, 16.5, TEXT));
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: percent(5.3),
                top: percent(93.6),
                align_items: AlignItems::Center,
                column_gap: px(4),
                ..default()
            })
            .with_children(|r| {
                for (name, label) in [("KG_Key_Enter", &texts.ok), ("KG_Key_Esc", &texts.back)] {
                    if let Some((image, aspect)) = art.sprite("SB_KeyTop", name) {
                        r.spawn((
                            image,
                            Node {
                                height: crate::game_ui::stage(20.0),
                                aspect_ratio: Some(aspect),
                                ..default()
                            },
                        ));
                    }
                    r.spawn(text(&format!(":{label}  "), 16.0, DIM));
                }
            });
        });
}

fn spawn_fade(mut commands: Commands) {
    commands.spawn((
        Fade,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(140),
        Pickable::IGNORE,
    ));
}

/// Where the loading fade is: clear, darkening with a warp (the alpha it shows), or lifting
/// since a time from the alpha the warp had reached.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Black {
    #[default]
    Clear,
    Warp(f32),
    Lift {
        since: f32,
        from: f32,
    },
}

impl Black {
    /// The next state and its alpha at `now`, from the warp's clock (`travel`) and whether Wolf
    /// has left for another map (`departed`, black until the run ends).
    fn next(self, travel: Option<f32>, departed: bool, now: f32) -> (Self, f32) {
        let next = match (travel, self) {
            (Some(t), _) => Self::Warp((t / WARP_CUT).clamp(0.0, 1.0)),
            (None, _) if departed => Self::Warp(1.0),
            // An arrival lifts the black from full, a warp cut short (a hit, a death) from
            // wherever it had got to, as the death screen's lift starts from its shade.
            (None, Self::Warp(from)) => Self::Lift { since: now, from },
            (None, Self::Lift { since, .. }) if now - since >= LIFT => Self::Clear,
            (None, s) => s,
        };
        let alpha = match next {
            Self::Clear => 0.0,
            Self::Warp(a) => a,
            Self::Lift { since, from } => (from * (1.0 - (now - since) / LIFT)).max(0.0),
        };
        (next, alpha)
    }
}

/// The loading fade: black rises over the warp clip up to the cut, holds while the next map
/// loads, and lifts over [`LIFT`] seconds once Wolf has arrived or the warp was cut short.
fn fade(
    time: Res<Time>,
    sim: NonSend<Sim>,
    mut black: Local<Black>,
    mut fades: Query<&mut BackgroundColor, With<Fade>>,
) {
    let travel = sim.duel.idol_travel();
    let departed = travel.is_none() && sim.duel.idol_phase() == Some(IdolPhase::Traveling);
    let (next, alpha) = black.next(travel, departed, time.elapsed_secs());
    *black = next;
    for mut c in &mut fades {
        c.set_if_neq(BackgroundColor(Color::BLACK.with_alpha(alpha)));
    }
}

/// Travel to another map: this run ends and the game starts again on that map, at the idol,
/// with the kit (restocked as the warp left).
fn depart(mut sim: NonSendMut<Sim>, mut exit: MessageWriter<AppExit>) {
    let Some(entity) = sim.duel.take_departure() else {
        return;
    };
    let kit = match serde_json::to_string(&sim.duel.kit) {
        Ok(kit) => kit,
        Err(e) => {
            error!("idol travel: the kit does not serialize: {e}");
            return;
        }
    };
    let args = travel_args(std::env::args().skip(1), &map_of(entity), entity, &kit);
    info!(
        "idol travel: relaunching on {} at idol {entity} (warp point {})",
        map_of(entity),
        entity - WARP_POINT
    );
    crate::menu::relaunch_with(args);
    exit.write(AppExit::Success);
}

/// This run's `args` for a run on `map` that starts at idol `entity` with `kit`: play, the
/// session flags a relaunch keeps (`crate::menu::kept_args`, as Back to start menu keeps them),
/// then the map, idol and kit. One-shot flags (a script, input, shots, `--exit`) are dropped.
fn travel_args(
    args: impl IntoIterator<Item = String>,
    map: &str,
    entity: i32,
    kit: &str,
) -> Vec<String> {
    let args: Vec<String> = args.into_iter().collect();
    let mut out = crate::menu::kept_args(&args);
    if !out.iter().any(|a| a == "--play") {
        out.insert(0, "--play".into());
    }
    out.extend([
        "--map".into(),
        map.into(),
        "--at-idol".into(),
        entity.to_string(),
        "--kit".into(),
        kit.into(),
    ]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_travel_list_is_the_games_pages_of_found_idols_in_row_order() {
        let cache = sekiro_sim::cache::root();
        let Ok(rows) = sekiro_sim::registry::rows(&cache, "BonfireWarpParam") else {
            eprintln!("skipping: cache not extracted");
            return;
        };
        let names = crate::game_ui::menu_texts(&cache, "NTC_メニューテキスト");
        let atlas = Atlas::load(&cache, rows.values(), &names);
        // The temple (on Important Places and Ashina Outskirts), the Castle Gate and an idol of
        // Ashina Castle, found out of order.
        let found = BTreeSet::from([1111950, 1101951, 1101950]);
        let regions: Vec<_> = (atlas.regions(&found).into_iter())
            .map(|(title, list)| {
                let names: Vec<_> = list
                    .iter()
                    .map(|&d| atlas.dests[d].name.replace('\u{a0}', " "))
                    .collect();
                (title, names)
            })
            .collect();
        assert_eq!(regions.len(), 3, "{regions:?}");
        assert_eq!(regions[0].0, "Important Places");
        assert_eq!(regions[0].1, ["Dilapidated Temple"]);
        assert_eq!(regions[1].0, "Ashina Outskirts");
        // Row 1100 (the temple) before row 1102 (the gate path).
        assert_eq!(regions[1].1[0], "Dilapidated Temple");
        assert_eq!(regions[1].1.len(), 2);
        assert_eq!(regions[2].0, "Ashina Castle");
        assert_eq!(regions[2].1.len(), 1);
        // Every idol's map is its entity's, and extracted ones are reachable from anywhere.
        let castle = atlas.dests.iter().find(|d| d.entity == 1111950).unwrap();
        assert_eq!(castle.map, "m11_01_00_00");
    }

    #[test]
    fn travel_to_another_map_restarts_this_run_there_at_the_idol_with_the_kit() {
        let args = [
            "--play",
            "--map",
            "m11_00_00_00",
            "--spawn",
            "1,2,3,4,0",
            "--perf",
            "--script",
            "fight.txt",
            "--input",
            "replay.jsonl",
            "--shots",
            "60:a.png",
            "--exit",
            "--kit",
            "{}",
            "--window-size",
            "1920x1080",
            "--checkpoint",
            "off",
            "--no-enemy",
            "--map-enemies",
            "--no-connected-blocks",
            "--perf-seconds",
            "30",
            "--present-mode",
            "immediate",
        ]
        .map(String::from);
        assert_eq!(
            travel_args(args, "m20_00_00_00", 2001950, "{\"sen\":1}"),
            [
                "--play",
                "--perf",
                "--window-size",
                "1920x1080",
                "--checkpoint",
                "off",
                "--no-enemy",
                "--map-enemies",
                "--no-connected-blocks",
                "--present-mode",
                "immediate",
                "--map",
                "m20_00_00_00",
                "--at-idol",
                "2001950",
                "--kit",
                "{\"sen\":1}"
            ]
        );
        // A boss arena's run plays the idol's map.
        let arena = ["--arena", "owl", "--cache", "c:/cache"].map(String::from);
        assert_eq!(
            travel_args(arena, "m11_00_00_00", 1101950, "{}")[..3],
            ["--play", "--cache", "c:/cache"]
        );
    }

    #[test]
    fn a_run_at_an_idol_needs_the_idol_and_its_warp_point_in_the_layout() {
        let layout = serde_json::json!({
            "objects": [{"entity_id": 1101950}],
            "players": [{"entity_id": 1100970}],
        });
        assert!(warp_point(&layout, 1101950).is_ok());
        let err = warp_point(&layout, 1101951).unwrap_err().to_string();
        assert!(err.contains("no idol 1101951"), "{err}");
        let no_warp = serde_json::json!({"objects": [{"entity_id": 1101950}]});
        let err = warp_point(&no_warp, 1101950).unwrap_err().to_string();
        assert!(err.contains("no warp point 1100970"), "{err}");
    }

    #[test]
    fn the_travel_target_on_another_map_starts_from_the_cache() {
        let cache = sekiro_sim::cache::root();
        let Ok(rows) = sekiro_sim::registry::rows(&cache, "BonfireWarpParam") else {
            eprintln!("skipping: cache not extracted");
            return;
        };
        let names = crate::game_ui::menu_texts(&cache, "NTC_メニューテキスト");
        let atlas = Atlas::load(&cache, rows.values(), &names);
        let gate = atlas.dests.iter().find(|d| d.entity == 1101951).unwrap();
        if !atlas.maps.contains(&gate.map) {
            eprintln!("skipping: {} not extracted", gate.map);
            return;
        }
        validate(gate, &sekiro_sim::kit::Kit::default()).unwrap();
        // An idol whose map is not extracted does not start.
        let missing = Dest {
            map: "m99_09_00_00".into(),
            ..gate.clone()
        };
        assert!(validate(&missing, &sekiro_sim::kit::Kit::default()).is_err());
    }

    #[test]
    fn the_travel_screen_opens_on_the_idol_wolf_is_at() {
        let mut atlas = Atlas::from_rows(
            &[
                serde_json::json!({"bonfireEntityId": 1101950, "menuPageNo": 0, "msgId": 1}),
                serde_json::json!({"bonfireEntityId": 1101950, "menuPageNo": 1, "msgId": 1}),
                serde_json::json!({"bonfireEntityId": 1101951, "menuPageNo": 1, "msgId": 2}),
                serde_json::json!({"bonfireEntityId": 1101952, "menuPageNo": 1, "msgId": 3}),
            ],
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        atlas.maps.insert("m11_00_00_00".into());
        let found = BTreeSet::from([1101950, 1101951, 1101952]);
        let regions = atlas.regions(&found);
        // Wolf at the third idol of region 1: the screen opens in the list, on its row.
        assert_eq!(atlas.find(&regions, 1101952), Some((1, 2)));
        // The temple is on both pages; the first one it is on wins.
        assert_eq!(atlas.find(&regions, 1101950), Some((0, 0)));
        assert_eq!(atlas.find(&regions, 1111950), None);
        let mut idols = Idols::default();
        idols.atlas = atlas;
        // Mouse moves keep the cursor in range.
        let mut menu = TravelMenu {
            open: true,
            regions,
            ..Default::default()
        };
        menu.cursor_to_idol(5);
        assert!(!menu.in_list);
        menu.cursor_to_region(1);
        menu.cursor_to_idol(2);
        assert_eq!((menu.region, menu.index, menu.in_list), (1, 2, true));
        menu.cursor_to_region(0);
        assert_eq!((menu.region, menu.index, menu.in_list), (0, 0, false));
        // An idol here is greyed, a refused one too.
        let gate = idols.atlas.dests[2].clone();
        assert!(menu.can_pick(&gate, &idols, Some(1101950)));
        assert!(!menu.can_pick(&gate, &idols, Some(gate.entity)));
        menu.refused.insert(gate.entity);
        assert!(!menu.can_pick(&gate, &idols, Some(1101950)));
    }

    #[test]
    fn the_black_lifts_from_where_the_warp_left_it() {
        fn step(black: &mut Black, travel: Option<f32>, departed: bool, now: f32) -> f32 {
            let (next, alpha) = black.next(travel, departed, now);
            *black = next;
            alpha
        }
        let b = &mut Black::Clear;
        assert_eq!(step(b, Some(0.5), false, 0.0), 0.25);
        // A hit cuts the warp short at a quarter black: the lift starts there, not from full.
        assert_eq!(step(b, None, false, 1.0), 0.25);
        assert!((step(b, None, false, 1.0 + LIFT / 2.0) - 0.125).abs() < 1e-6);
        assert_eq!(step(b, None, false, 1.1 + LIFT), 0.0);
        assert_eq!(*b, Black::Clear);
        // An arrival lifts from full black; a departure holds it.
        assert_eq!(step(b, Some(WARP_CUT), false, 2.0), 1.0);
        assert_eq!(step(b, None, false, 2.0), 1.0);
        assert!((step(b, None, false, 2.0 + LIFT / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(step(b, Some(WARP_CUT), false, 3.0), 1.0);
        assert_eq!(step(b, None, true, 9.0), 1.0);
    }
}
