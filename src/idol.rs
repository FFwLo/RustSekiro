//! Sculptor's Idols (2026-10-17, the user: "go ahead with an idol", with captures of the game's
//! idol menu, Purchase and Travel screens).
//!
//! - The idols stand on the parkour map (`parkour_idols`): a stand-in figure with a warm light
//!   (gap: the game's idol model is not extracted).
//! - Near one, the "Rest" guide (イベントテキスト 15000120, E); E kneels Wolf before it with the
//!   game's own clips (HKS StatueAttach*: Start a000_710310, Loop 710320, End 710330, Rest 710340,
//!   Warp 710360). Kneeling rests him (full Vitality, the Healing Gourd and resurrection back) and
//!   makes the idol the checkpoint; then the idol menu opens (`Screen::Idol`).
//! - The menu (the capture: the place's name, the game's rows, the stats plate): Rest (the rest
//!   clip, then the menu again), Purchase Spirit Emblems (50 Sen each, up to the carry limit),
//!   Travel (the warp clip, then up at the chosen idol); the rows not built yet are greyed. Esc
//!   stands Wolf up (StatueAttachEnd).
//!
//! The seat is Wolf's (`Player::seat`): player.rs drives his clips from it, this menu reads and
//! commands it. The idol list is the map's global, as the terrain is (map.rs TERRAIN).
//! gap: dying does not yet send Wolf back to the checkpoint idol.

use std::sync::RwLock;

use bevy::prelude::*;

use crate::game_menu::{abs, any_icon, fill_row, image, key_guide, list_screen, sprite, text, title_bar, Detail, ItemTexts, Row, RowButton, Tables, DIM, TEXT};
use crate::menu::Screen;

/// One idol: its name, where it stands (its base, on the floor), the way it faces (Wolf kneels
/// IDOL_SPOT in front of it, facing it).
#[derive(Clone, Default)]
pub struct Idol {
    pub name: String,
    pub pos: Vec3,
    pub out: Vec3,
    /// Under sekiro-rs's map: the idol's entity (BonfireWarpParam bonfireEntityId), the Travel
    /// pages it is listed on, and whether it stands in another map (then `pos` / `out` are
    /// unknown and Travel goes there by a new run, sr_map.rs `travel`).
    pub entity: i64,
    pub pages: Vec<usize>,
    pub away: bool,
}

/// One BonfireWarpParam row: the idol's entity, its Travel page (menuPageNo) and name
/// (NTC_メニューテキスト msgId), in the param's order.
pub struct WarpRow {
    pub entity: i64,
    pub page: usize,
    pub name: String,
}

fn menu_texts() -> &'static std::collections::HashMap<u32, String> {
    static T: std::sync::OnceLock<std::collections::HashMap<u32, String>> = std::sync::OnceLock::new();
    T.get_or_init(|| std::fs::read(crate::paths::extracted().join("msg/engus/menu.msgbnd.d/NTC_メニューテキスト.fmg")).ok().map(|d| crate::action_guide::read_fmg(&d)).unwrap_or_default())
}

/// The game's idol list (BonfireWarpParam), named.
pub fn warp_rows() -> &'static [WarpRow] {
    static ROWS: std::sync::OnceLock<Vec<WarpRow>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/BonfireWarpParam.json");
        let json: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut rows: Vec<&serde_json::Value> = json["rows"].as_array().into_iter().flatten().collect();
        rows.sort_by_key(|r| r["id"].as_i64().unwrap_or(0));
        rows.iter()
            .filter_map(|r| {
                let entity = r["bonfireEntityId"].as_i64().filter(|e| *e > 0)?;
                let msg = r["msgId"].as_i64().unwrap_or(0) as u32;
                let name = menu_texts().get(&msg).cloned().unwrap_or_else(|| format!("Idol {entity}"));
                Some(WarpRow { entity, page: r["menuPageNo"].as_i64().unwrap_or(0).max(0) as usize, name })
            })
            .collect()
    })
}

/// A Travel page's title: NTC_メニューテキスト 211100 + page ("Important Places", then
/// `<?placeName@n?>`, the 地名 text n: "Ashina Outskirts", ...).
fn page_name(page: usize) -> String {
    static PLACES: std::sync::OnceLock<std::collections::HashMap<u32, String>> = std::sync::OnceLock::new();
    let t = menu_texts().get(&(211_100 + page as u32)).cloned().unwrap_or_default();
    match t.strip_prefix("<?placeName@").and_then(|r| r.strip_suffix("?>")).and_then(|n| n.parse::<u32>().ok()) {
        Some(n) => PLACES
            .get_or_init(|| std::fs::read(crate::paths::extracted().join("msg/engus/item.msgbnd.d/地名.fmg")).ok().map(|d| crate::action_guide::read_fmg(&d)).unwrap_or_default())
            .get(&n)
            .cloned()
            .unwrap_or(t),
        None => t,
    }
}

/// Under sekiro-rs's map: every idol of the game, one per entity, those of this play world at
/// their place, the rest away (sr_map.rs).
fn sr_idols() -> Vec<Idol> {
    let mut out: Vec<Idol> = Vec::new();
    for r in warp_rows() {
        if let Some(i) = out.iter_mut().find(|i| i.entity == r.entity) {
            i.pages.push(r.page);
            continue;
        }
        let spot = crate::sr_map::idol_spot(r.entity);
        let (pos, out_dir) = spot.unwrap_or((Vec3::ZERO, Vec3::Z));
        out.push(Idol { name: r.name.clone(), pos, out: out_dir, entity: r.entity, pages: vec![r.page], away: spot.is_none() });
    }
    out
}

/// Whether Travel can go to idol `i`: here, or on a map a new run can start on.
fn reachable(i: &Idol) -> bool {
    !i.away || crate::sr_map::arena_on(&crate::sr_map::map_of(i.entity)).is_some()
}

/// The idol a run starts at (`SHINOBI_IDOL`, idol travel), as an index of [`IDOLS`].
pub fn start_index() -> Option<usize> {
    let e = crate::sr_map::idol_start()?;
    IDOLS.read().ok()?.iter().position(|i| i.entity == e && !i.away)
}

/// How far in front of the idol Wolf kneels (m).
const IDOL_SPOT: f32 = 0.9;
/// How near the spot the "Rest" guide shows (m across, m up / down).
const IDOL_REACH: f32 = 1.3;
const IDOL_REACH_Y: f32 = 0.6;
/// How far into the warp clip (StatueAttachWarp, 6.3 s) Wolf arrives at the other idol (s).
const WARP_CUT: f32 = 2.0;
/// A Spirit Emblem's price (the game's Sculptor's Idol shop: 50 Sen).
const EMBLEM_PRICE: u32 = 50;

impl Idol {
    pub fn spot(&self) -> Vec3 {
        self.pos + self.out * IDOL_SPOT
    }

    /// Wolf's yaw kneeling: facing the idol.
    pub fn wolf_yaw(&self) -> f32 {
        let d = -self.out;
        f32::atan2(-d.x, -d.z)
    }
}

/// The map's idols.
pub static IDOLS: RwLock<Vec<Idol>> = RwLock::new(Vec::new());

/// The Construct's idols: by the start, on the L platform, on the cliff, in the shaft, at the
/// stealth courtyard's gate.
pub fn parkour_idols() -> Vec<Idol> {
    let idol = |name: &str, pos: Vec3, out: Vec3| Idol { name: name.to_string(), pos, out, ..default() };
    vec![
        idol("Course Start", Vec3::new(-3.5, 0.0, 1.5), Vec3::X),
        idol("Platform Top", Vec3::new(11.2, 3.0, -13.3), Vec3::NEG_X),
        idol("Cliff Top", Vec3::new(-13.0, 4.5, -23.0), Vec3::X),
        idol("Shaft Floor", Vec3::new(15.5, 0.0, -23.6), Vec3::Z),
        idol("Outpost Gate", Vec3::new(4.5, 0.0, 10.0), Vec3::NEG_Z),
        idol("Waterside", Vec3::new(-17.5, 0.0, -5.5), Vec3::X),
        idol("Second Section", Vec3::new(5.0, 0.0, -48.0), Vec3::NEG_Z),
    ]
}

/// Where Wolf comes back after a death / retry with idol `i` as the checkpoint: standing at its
/// spot, facing away from it.
pub fn respawn_at(i: usize) -> Option<(Vec3, f32)> {
    let idol = IDOLS.read().ok()?.get(i)?.clone();
    let out = idol.out;
    Some((idol.spot() + out * 0.4, f32::atan2(-out.x, -out.z)))
}

/// The idol whose kneeling spot Wolf's feet are at, if any.
pub fn near(feet: Vec3, _yaw: f32) -> Option<usize> {
    let idols = IDOLS.read().ok()?;
    idols.iter().position(|i| {
        if i.away {
            return false;
        }
        let s = i.spot();
        (s - feet).with_y(0.0).length() < IDOL_REACH && (s.y - feet.y).abs() < IDOL_REACH_Y
    })
}

/// Where Wolf is in the idol's clips.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Phase {
    #[default]
    Kneeling,
    /// The loop, with the menu open.
    Menu,
    Resting,
    Warping(usize),
    Leaving,
}

/// What the menu asks of the seat.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Rest,
    Warp(usize),
    Leave,
}

/// The idol Wolf kneels at, the phase, the menu's command, the checkpoint.
#[derive(Default, Clone)]
pub struct Seat {
    pub idol: Option<usize>,
    /// Talking to the Sculptor uses the Workshop, without kneeling, healing or changing idols.
    pub sculptor: bool,
    pub phase: Phase,
    pub cmd: Option<Cmd>,
    /// The menu should open (the kneel or the rest clip is done).
    pub open: bool,
    /// The last idol prayed at.
    pub checkpoint: Option<usize>,
    /// The enemies should come back (a rest; player.rs sets FightReset). Kneeling alone leaves
    /// them (the user: they came back as soon as Wolf sat).
    pub respawn: bool,
}


/// Resting: full Vitality, the Healing Gourd and the resurrection back.
fn rest(p: &mut crate::player::Player, a: &mut crate::actor::Actor, config: &crate::config::GameConfig) {
    a.hp = a.hp_max;
    p.gourd = config.player.gourd_charges;
    p.resurrections = config.player.resurrections;
    p.revive_locked = false;
    p.refill_items = true;
}

/// Wolf at an idol, from player.rs (before the docking): E at a spot kneels him; while kneeling
/// the idol's clips run and nothing else does. True when it took the tick.
#[allow(clippy::too_many_arguments)]
pub fn tick(p: &mut crate::player::Player, a: &mut crate::actor::Actor, d: &crate::data::CharData, tf: &mut Transform, interact: bool, ended: bool, config: &crate::config::GameConfig) -> bool {
    let mut seat = std::mem::take(&mut p.seat);
    let took = seat_tick(&mut seat, p, a, d, tf, interact, ended, config);
    p.seat = seat;
    took
}

#[allow(clippy::too_many_arguments)]
fn seat_tick(seat: &mut Seat, p: &mut crate::player::Player, a: &mut crate::actor::Actor, d: &crate::data::CharData, tf: &mut Transform, interact: bool, ended: bool, config: &crate::config::GameConfig) -> bool {
    let feet = tf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    let Some(at) = seat.idol else {
        if interact && !a.airborne && a.hp > 0.0 && crate::player::is_free(&a.state) {
            if let Some(i) = near(feet, a.yaw) {
                let idol = IDOLS.read().ok().and_then(|v| v.get(i).cloned());
                if let Some(idol) = idol {
                    if a.play_state(d, "StatueAttachStart") {
                        tf.translation = idol.spot() + Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                        a.yaw = idol.wolf_yaw();
                        a.move_vel = Vec3::ZERO;
                        rest(p, a, config);
                        *seat = Seat { idol: Some(i), phase: Phase::Kneeling, cmd: None, open: false, checkpoint: Some(i), respawn: false, sculptor: false };
                        return true;
                    }
                }
            }
        }
        return false;
    };
    // Something else took Wolf (a hit, a grab, death: the world runs under the menu): off the idol.
    if !a.state.starts_with("StatueAttach") {
        *seat = Seat { checkpoint: seat.checkpoint, ..default() };
        return false;
    }
    a.move_vel = Vec3::ZERO;
    if let Some(cmd) = seat.cmd.take() {
        match cmd {
            Cmd::Rest => {
                a.play_state(d, "StatueAttachRest");
                rest(p, a, config);
                seat.respawn = true;
                seat.phase = Phase::Resting;
            }
            Cmd::Warp(to) => {
                a.play_state(d, "StatueAttachWarp");
                seat.phase = Phase::Warping(to);
            }
            Cmd::Leave => {
                a.play_state(d, "StatueAttachEnd");
                seat.phase = Phase::Leaving;
            }
        }
        return true;
    }
    match seat.phase {
        Phase::Kneeling | Phase::Resting if ended => {
            a.play_state(d, "StatueAttachLoop");
            seat.phase = Phase::Menu;
            seat.open = true;
        }
        Phase::Menu if ended => {
            a.t %= d.length(&a.anim).max(1e-3);
        }
        // The warp hands over WARP_CUT in (the game cuts to its loading screen there; the user:
        // the whole 6.3 s clip made travel "a bit too long").
        Phase::Warping(to) if ended || a.t >= WARP_CUT => {
            // Up at the chosen idol: its spot, then stand.
            let idol = IDOLS.read().ok().and_then(|v| v.get(to).cloned());
            // In another map: a new run there (sr_map.rs), which ends this one.
            if let Some(idol) = idol.as_ref().filter(|i| i.away) {
                crate::sr_map::travel(idol.entity);
            }
            if let Some(idol) = idol {
                tf.translation = idol.spot() + Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                a.yaw = idol.wolf_yaw();
                rest(p, a, config);
                seat.idol = Some(to);
                seat.checkpoint = Some(to);
            }
            a.play_state(d, "StatueAttachEnd");
            seat.phase = Phase::Leaving;
        }
        Phase::Leaving if ended => {
            seat.idol = None;
            a.play("StandIdle", "");
            return false;
        }
        _ => {}
    }
    let _ = at;
    true
}

/// The idol menu's rows (the game's, in its order) and whether this recreation has them yet.
const ROWS: [(&str, bool); 10] = [
    ("Rest", true),
    ("Purchase Spirit Emblems", true),
    ("Travel", true),
    ("Acquire Skills", true),
    ("Enhance Physical Attributes", false),
    ("Enhance Attack Power", false),
    ("Dragonrot Restoration", false),
    ("Change Form", true),
    ("Reflection of Strength", false),
    ("Gauntlet of Strength", false),
];

/// The Workshop's rows (the user's capture).
const WORKSHOP_ROWS: [&str; 2] = ["Fit New Prosthetic Tool", "Upgrade Prosthetic Tool"];

/// The idol menu's pages.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Page {
    #[default]
    Main,
    Purchase,
    Travel,
    /// Acquire Skills (src/skill_tree.rs).
    Skills,
    /// Change Form (src/form.rs).
    Form,
    /// The Sculptor's Workshop (src/workshop.rs): its two rows, Create Arm Tools, the upgrade tree.
    Workshop,
    Fit,
    Upgrade,
}

#[derive(Resource, Default)]
struct IdolMenu {
    page: Page,
    /// The Workshop was opened by talking to the Sculptor, rather than a screenshot harness.
    sculptor: bool,
    index: usize,
    /// Travel: the region column (false) or its idols (true).
    in_list: bool,
    region: usize,
    /// Acquire Skills: the tree page, its cursor and the confirm dialog.
    tree: crate::skill_tree::View,
    /// The first row shown of a long list (Create Arm Tools; game_menu::keep_visible).
    top: usize,
    /// What the screen was last built from (rebuilt when it changes).
    built: Option<String>,
}

#[derive(Component)]
pub(crate) struct IdolMenuRoot;

/// Travel's regions: (name, idols). Under sekiro-rs's map the game's pages (BonfireWarpParam
/// menuPageNo, those with idols); else this map's idols ("Important Places" lists none).
fn regions() -> Vec<(String, Vec<usize>)> {
    let idols = IDOLS.read().map(|v| v.clone()).unwrap_or_default();
    if idols.iter().any(|i| i.entity != 0) {
        let last = idols.iter().flat_map(|i| i.pages.iter().copied()).max().unwrap_or(0);
        return (0..=last)
            .filter_map(|page| {
                let list: Vec<usize> = warp_rows().iter().filter(|r| r.page == page).filter_map(|r| idols.iter().position(|i| i.entity == r.entity)).collect();
                (!list.is_empty()).then(|| (page_name(page), list))
            })
            .collect();
    }
    vec![("Important Places".to_string(), Vec::new()), ("Construct".to_string(), (0..idols.len()).collect())]
}

/// The stats plate of the idol menu (the capture: resurrection nodes over the Vitality bar,
/// Unseen Aid, Prayer Beads and Memories, Vitality / Attack Power, the skill level, Spirit
/// Emblems and Sen). gap: as the pause menu's status bar, the unmodelled values are Wolf's start.
#[allow(clippy::too_many_arguments)]
fn stats_plate(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts, p: &crate::player::Player, a: &crate::actor::Actor) {
    root.spawn((BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.9)), BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.9)), Node { border: UiRect::all(Val::Px(3.0)), ..abs(46.2, 13.2, 47.5, 51.0) })).with_children(|b| {
        for k in 0..config.player.resurrections.max(1) {
            let lit = k < p.resurrections;
            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(5.5 + k as f32 * 4.6), top: Val::Percent(5.5), width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, image(assets, sprite("sb_fe/MENU_Revive_Gauge")).with_color(if lit { Color::WHITE } else { Color::srgba(0.4, 0.4, 0.4, 0.6) })));
        }
        b.spawn((abs(5.5, 16.0, 49.0, 2.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7))));
        b.spawn((abs(5.5, 16.0, 49.0 * (a.hp / a.hp_max.max(1.0)).clamp(0.0, 1.0), 2.0), BackgroundColor(Color::srgb(0.62, 0.17, 0.12))));
        b.spawn(abs(66.0, 6.0, 30.0, 14.0)).with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                t.spawn(text("Unseen Aid", 17.0, TEXT, font));
                t.spawn(text("30%", 17.0, TEXT, font));
            });
        });
        b.spawn((abs(5.0, 30.0, 90.0, 0.3), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12))));
        let goods_icon = |name: &str| texts.goods_id(name).and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
        for (k, (label, icon)) in [("Prayer Bead", goods_icon("Prayer Bead")), ("Memory", None)].into_iter().enumerate() {
            let t = 36.0 + k as f32 * 18.0;
            b.spawn((abs(5.5, t, 8.0, 15.0), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|i| {
                i.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(10.0), top: Val::Percent(10.0), width: Val::Percent(80.0), height: Val::Percent(80.0), ..default() }, image(assets, icon)));
            });
            b.spawn(abs(16.0, t, 30.0, 16.0)).with_children(|c| {
                c.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|r| {
                    r.spawn(text(label, 17.0, TEXT, font));
                    r.spawn(text("   -/   -", 17.0, TEXT, font));
                });
            });
        }
        for (k, (icon, label, value)) in [("MENU_ST_Vitality", "Vitality", format!("{:.0}", config.posture.player_level)), ("MENU_ST_Offensive", "Attack Power", config.player.attack_power.to_string())].into_iter().enumerate() {
            let t = 39.0 + k as f32 * 13.0;
            b.spawn((abs(64.0, t, 5.5, 11.0), image(assets, sprite(&format!("sb_in_game/{icon}")))));
            b.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..abs(70.5, t, 25.0, 11.0) }).with_children(|c| {
                c.spawn((text(label, 17.0, TEXT, font), TextLayout::no_wrap()));
                c.spawn(text(&value, 17.0, TEXT, font));
            });
        }
        b.spawn((abs(68.0, 66.0, 27.0, 3.0), image(assets, sprite("sb_in_game/MENU_GaugeFrame"))));
        let emblem = goods_icon("Spirit Emblem");
        b.spawn((abs(5.5, 80.0, 8.0, 15.0), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|i| {
            i.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(10.0), top: Val::Percent(10.0), width: Val::Percent(80.0), height: Val::Percent(80.0), ..default() }, image(assets, emblem)));
        });
        b.spawn(abs(18.0, 84.0, 30.0, 10.0)).with_children(|c| {
            c.spawn(text(&format!("{} / {}", p.emblems, config.player.spirit_emblems), 17.0, TEXT, font));
        });
        b.spawn((abs(72.0, 83.0, 4.0, 8.0), image(assets, sprite("sb_fe/MENU_Money"))));
        b.spawn(Node { justify_content: JustifyContent::FlexEnd, ..abs(78.0, 83.0, 17.0, 8.0) }).with_children(|c| {
            c.spawn(text(&config.player.sen.to_string(), 17.0, TEXT, font));
        });
    });
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn build(commands: &mut Commands, assets: &AssetServer, menu: &IdolMenu, font: &Option<Handle<Font>>, config: &crate::config::GameConfig, tables: &Tables, texts: &ItemTexts, forms: &crate::form::FormTexts, p: &crate::player::Player, a: &crate::actor::Actor) {
    let seat_idol = p.seat.idol;
    let place = seat_idol.and_then(|i| IDOLS.read().ok().and_then(|v| v.get(i).map(|i| i.name.clone()))).unwrap_or_default();
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    match menu.page {
        Page::Main => {
            commands.spawn((full, GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                // The dark fade on the left over the game.
                root.spawn((abs(0.0, 0.0, 33.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.93))));
                for k in 0..10 {
                    root.spawn((abs(33.0 + k as f32, 0.0, 1.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.93 * (1.0 - (k as f32 + 1.0) / 11.0)))));
                }
                root.spawn(abs(5.9, 7.6, 30.0, 5.0)).with_children(|c| {
                    c.spawn(text(&place, 21.0, Color::WHITE, font));
                });
                root.spawn((abs(0.0, 12.8, 32.0, 0.2), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25))));
                for (i, (label, live)) in ROWS.iter().enumerate() {
                    let t = 14.6 + i as f32 * 5.6;
                    let sel = i == menu.index;
                    if sel {
                        root.spawn((abs(4.0, t, 30.0, 4.8), image(assets, sprite("sb_in_game/MENU_Cursor_03")).with_color(Color::srgba(0.8, 0.55, 0.3, 0.95))));
                    }
                    root.spawn((crate::menu::ui_button(), abs(4.0, t, 30.0, 4.8), RowButton(i))).with_children(|b| {
                        b.spawn(Node { padding: UiRect::left(Val::Percent(6.5)), ..fill_row() }).with_children(|c| {
                            let color = if !live { Color::srgba(0.7, 0.68, 0.64, 0.4) } else if sel { Color::WHITE } else { TEXT };
                            c.spawn(text(label, 17.0, color, font));
                        });
                    });
                    root.spawn((abs(5.9, t + 4.9, 25.5, 0.15), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.18))));
                }
                let help = match ROWS[menu.index].0 {
                    "Rest" => "Rest at the Sculptor's Idol",
                    "Purchase Spirit Emblems" => "Purchase Spirit Emblems with Sen",
                    "Travel" => "Travel to another Sculptor's Idol",
                    "Acquire Skills" => "Acquire new skills with skill points",
                    "Change Form" => "Change Wolf's appearance",
                    "Sculptor's Workshop" => "Fit and upgrade Prosthetic Tools",
                    _ => "Not in this recreation yet",
                };
                root.spawn(abs(5.9, 89.6, 30.0, 4.0)).with_children(|c| {
                    c.spawn(text(help, 16.5, TEXT, font));
                });
                root.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(5.3), top: Val::Percent(93.6), align_items: AlignItems::Center, column_gap: Val::Px(4.0), ..default() },)).with_children(|r| {
                    for (key, label) in [("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Close")] {
                        r.spawn((Node { height: Val::Px(20.0), width: Val::Px(30.0), ..default() }, image(assets, sprite(key))));
                        r.spawn(text(label, 16.0, DIM, font));
                    }
                });
                stats_plate(root, assets, font, config, tables, texts, p, a);
            });
        }
        Page::Purchase => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Purchase");
                let cats = [("All Items", "MENU_TB_Alll"), ("Quick Items", "MENU_TB_Item"), ("Key Items", "MENU_TB_valuable"), ("Upgrade Materials", "MENU_TB_Material")];
                let id = texts.goods_id("Spirit Emblem");
                let (name, info) = id.map(|id| (texts.goods_name.get(&(id as u32)).cloned().unwrap_or_default(), texts.goods_info.get(&(id as u32)).cloned().unwrap_or_default())).unwrap_or_default();
                let icon = id.and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
                let row = Row { icon: icon.clone(), name: name.clone(), value: Some(EMBLEM_PRICE.to_string()), ..default() };
                let detail = Detail { name, icon, art: None, stats: vec![("Number Held".into(), format!("{} / {}", p.emblems, config.player.spirit_emblems)), ("In Storage".into(), "0".into())], info };
                list_screen(root, assets, font, &cats, 0, &[row], 0, 0, Some(&detail));
                // The purse beside the categories.
                root.spawn((abs(38.0, 18.6, 13.0, 4.8), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.35, 0.35, 0.35)))).with_children(|b| {
                    b.spawn(Node { justify_content: JustifyContent::SpaceBetween, padding: UiRect::axes(Val::Percent(4.0), Val::Px(0.0)), ..fill_row() }).with_children(|c| {
                        c.spawn((Node { width: Val::Px(22.0), height: Val::Px(22.0), ..default() }, image(assets, sprite("sb_fe/MENU_Money"))));
                        c.spawn(text(&config.player.sen.to_string(), 17.0, TEXT, font));
                    });
                });
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    let full_carry = p.emblems >= config.player.spirit_emblems;
                    let help = if full_carry { "You cannot carry any more." } else if config.player.sen < EMBLEM_PRICE { "Not enough Sen." } else { "Select an item to purchase." };
                    c.spawn(text(help, 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
        Page::Skills => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Acquire Skills");
                crate::skill_tree::build(root, assets, font, menu.tree, config, texts, tables);
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text("Review and Acquire Skills.", 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":Acquire  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
        Page::Workshop => {
            commands.spawn((full, GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 33.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.93))));
                for k in 0..10 {
                    root.spawn((abs(33.0 + k as f32, 0.0, 1.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.93 * (1.0 - (k as f32 + 1.0) / 11.0)))));
                }
                root.spawn(abs(5.5, 8.6, 30.0, 5.0)).with_children(|c| {
                    c.spawn(text("Sculptor", 21.0, Color::WHITE, font));
                });
                root.spawn((abs(0.0, 13.3, 30.0, 0.2), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25))));
                for (i, label) in WORKSHOP_ROWS.iter().enumerate() {
                    let t = 14.8 + i as f32 * 5.6;
                    let sel = i == menu.index;
                    if sel {
                        root.spawn((abs(4.0, t, 29.0, 4.8), image(assets, sprite("sb_in_game/MENU_Cursor_03")).with_color(Color::srgba(0.8, 0.55, 0.3, 0.95))));
                    }
                    root.spawn((crate::menu::ui_button(), abs(4.0, t, 29.0, 4.8), RowButton(i))).with_children(|b| {
                        b.spawn(Node { padding: UiRect::left(Val::Percent(6.5)), ..fill_row() }).with_children(|c| {
                            c.spawn(text(label, 17.0, if sel { Color::WHITE } else { TEXT }, font));
                        });
                    });
                    root.spawn((abs(5.5, t + 4.9, 25.0, 0.15), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.18))));
                }
                root.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(5.3), top: Val::Percent(92.6), align_items: AlignItems::Center, column_gap: Val::Px(4.0), ..default() },)).with_children(|r| {
                    for (key, label) in [("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Close")] {
                        r.spawn((Node { height: Val::Px(20.0), width: Val::Px(30.0), ..default() }, image(assets, sprite(key))));
                        r.spawn(text(label, 16.0, DIM, font));
                    }
                });
                crate::workshop::sen_plate(root, assets, font, config.player.sen, 81.6, 91.4);
            });
        }
        Page::Fit => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Create Arm Tools");
                let list = crate::workshop::fit_rows(config, texts, tables);
                let rows: Vec<Row> = list.iter().map(|(r, _)| r.clone()).collect();
                list_screen(root, assets, font, &[], 0, &rows, menu.index, menu.top, list.get(menu.index).map(|(_, d)| d));
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text("Select a Prosthetic Tool to Craft.", 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
        Page::Upgrade => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Prosthetic Tool Upgrades");
                crate::workshop::build_tree(root, assets, font, menu.index, menu.tree.asking, config, texts, tables);
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text("Select a Prosthetic Tool to Upgrade.", 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":Upgrade  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
        Page::Form => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Change Form");
                let worn = crate::form::current(config);
                let name = |f: &crate::form::Form| forms.name.get(&(f.protector as u32)).cloned().unwrap_or_default();
                // The memories: the icon box (the worn one marked) and the name on its brush.
                for (i, f) in crate::form::FORMS.iter().enumerate() {
                    let t = 18.0 + i as f32 * 10.7;
                    let sel = i == menu.index;
                    let icon = any_icon(f.icon);
                    root.spawn((crate::menu::ui_button(), abs(7.0, t, 5.1, 9.0), image(assets, sprite("sb_in_game/MENU_IconBase_02")), RowButton(i))).with_children(|b| {
                        b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(5.0), top: Val::Percent(5.0), width: Val::Percent(90.0), height: Val::Percent(90.0), ..default() }, image(assets, icon)));
                        if i == worn {
                            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(-6.0), top: Val::Percent(-6.0), width: Val::Percent(30.0), height: Val::Percent(30.0), ..default() }, image(assets, sprite("sb_in_game/MENU_Equipped"))));
                        }
                    });
                    let plate = if sel { "sb_in_game/MENU_Cursor_03" } else { "sb_in_game/MENU_ItemBase_02" };
                    root.spawn((crate::menu::ui_button(), abs(13.4, t + 1.6, 34.4, 6.4), image(assets, sprite(plate)), RowButton(i))).with_children(|b| {
                        b.spawn(Node { padding: UiRect::left(Val::Percent(4.5)), ..fill_row() }).with_children(|c| {
                            c.spawn(text(&name(f), 17.0, if sel { Color::WHITE } else { TEXT }, font));
                        });
                    });
                }
                // The memory under the cursor: its name, picture and text.
                if let Some(f) = crate::form::FORMS.get(menu.index) {
                    root.spawn((abs(55.4, 13.6, 37.6, 74.4), image(assets, sprite("sb_in_game/MENU_WP_Base"))));
                    root.spawn((abs(56.4, 14.6, 35.6, 4.6), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.25, 0.25, 0.25)))).with_children(|b| {
                        b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                            c.spawn(text(&name(f), 17.0, Color::WHITE, font));
                        });
                    });
                    if let Some(pic) = crate::form::picture(f) {
                        root.spawn((abs(57.7, 19.7, 33.0, 33.8), ImageNode::new(assets.load(pic))));
                    }
                    let mut r = 57.2;
                    while r < 85.5 {
                        root.spawn((abs(57.6, r, 33.2, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18))));
                        r += 3.15;
                    }
                    root.spawn(abs(57.9, 54.2, 32.6, 33.0)).with_children(|c| {
                        c.spawn(text(forms.info.get(&(f.protector as u32)).map_or("", |s| s.as_str()), 15.5, TEXT, font));
                    });
                }
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text("Select Wolf's appearance", 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
        Page::Travel => {
            commands.spawn((full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(110), IdolMenuRoot)).with_children(|root| {
                root.spawn((abs(0.0, 0.0, 100.0, 100.0), image(assets, sprite("sb_in_game/MENU_Base_00"))));
                title_bar(root, assets, font, "Travel");
                let regions = regions();
                let here_region = regions.iter().position(|(_, v)| seat_idol.is_some_and(|i| v.contains(&i)));
                for (i, (name, _)) in regions.iter().enumerate() {
                    let t = 16.0 + i as f32 * 8.2;
                    let sel = i == menu.region;
                    let plate = if sel { "sb_in_game/MENU_Cursor_03" } else { "sb_in_game/MENU_ItemBase_02" };
                    let tint = if sel && menu.in_list { Color::srgba(1.0, 1.0, 1.0, 0.6) } else { Color::WHITE };
                    root.spawn((crate::menu::ui_button(), abs(6.8, t, 34.5, 6.4), image(assets, sprite(plate)).with_color(tint), RowButton(i))).with_children(|b| {
                        b.spawn(Node { padding: UiRect::left(Val::Percent(5.0)), ..fill_row() }).with_children(|c| {
                            c.spawn(text(name, 17.0, if sel { Color::WHITE } else { TEXT }, font));
                        });
                    });
                    // Wolf's figure by the region he is in.
                    if here_region == Some(i) {
                        let idol_icon = texts.goods_id("Homeward Idol").and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
                        root.spawn((abs(4.6, t + 1.2, 1.9, 4.0), image(assets, idol_icon)));
                    }
                }
                // The idols of the region, on a parchment, and the picture panel under it.
                root.spawn((abs(43.2, 14.6, 50.0, 42.6), image(assets, sprite("sb_in_game/MENU_WP_Base"))));
                root.spawn((abs(43.2, 58.4, 50.0, 30.6), image(assets, sprite("sb_in_game/MENU_WP_Base")).with_color(Color::srgb(0.6, 0.6, 0.6))));
                let idols = IDOLS.read().map(|v| v.clone()).unwrap_or_default();
                if let Some((_, list)) = regions.get(menu.region) {
                    // Rows closer together when a region has many (the parchment is 40 high).
                    let step = (40.0 / list.len().max(1) as f32).min(5.6);
                    for (k, &i) in list.iter().enumerate() {
                        let sel = menu.in_list && k == menu.index;
                        let plate = if sel { "sb_in_game/MENU_Cursor_03" } else { "sb_in_game/MENU_ItemBase_07" };
                        root.spawn((abs(45.8, 16.2 + k as f32 * step, 42.6, step.min(4.6)), image(assets, sprite(plate)).with_color(if sel { Color::WHITE } else { Color::srgb(0.5, 0.5, 0.5) }))).with_children(|b| {
                            b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                                let here = if seat_idol == Some(i) { "  (here)" } else { "" };
                                let can = idols.get(i).is_some_and(reachable);
                                let colour = if !can { DIM } else if sel { Color::WHITE } else { TEXT };
                                c.spawn(text(&format!("{}{here}", idols.get(i).map(|x| x.name.as_str()).unwrap_or("")), 16.5, colour, font));
                            });
                        });
                    }
                }
                root.spawn(abs(5.3, 91.6, 50.0, 4.0)).with_children(|c| {
                    c.spawn(text("Select Sculptor's Idol destination.", 16.0, DIM, font));
                });
                key_guide(root, assets, font, &[("sb_keytop/KG_Key_Enter", ":OK  "), ("sb_keytop/KG_Key_Esc", ":Back")]);
            });
        }
    }
}

/// Opens the menu when the seat asks (the kneel or the rest is done).
fn open_menu(mut next: ResMut<NextState<Screen>>, mut menu: ResMut<IdolMenu>, mut wolf: Query<&mut crate::player::Player>) {
    let Ok(mut p) = wolf.single_mut() else { return };
    if p.seat.open {
        p.seat.open = false;
        *menu = IdolMenu { page: if p.seat.sculptor { Page::Workshop } else { Page::Main }, sculptor: p.seat.sculptor, ..default() };
        next.set(Screen::Idol);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_idol_menu(
    mut commands: Commands,
    state: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut menu: ResMut<IdolMenu>,
    roots: Query<Entity, With<IdolMenuRoot>>,
    rows: Query<(&RowButton, &Interaction), Changed<Interaction>>,
    (nodes, cats, ups): (Query<(&crate::skill_tree::SkillNodeButton, &Interaction), Changed<Interaction>>, Query<(&crate::game_menu::CatButton, &Interaction), Changed<Interaction>>, Query<(&crate::workshop::UpgradeButton, &Interaction), Changed<Interaction>>),
    (assets, font, mut config, texts, tables, forms, pointer): (Res<AssetServer>, Res<crate::fonts::GameFonts>, ResMut<crate::config::GameConfig>, Res<ItemTexts>, Res<Tables>, Res<crate::form::FormTexts>, Res<crate::menu::MenuMouse>),
    mut wolf: Query<(&mut crate::player::Player, &crate::actor::Actor)>,
) {
    if *state.get() != Screen::Idol {
        for r in &roots {
            commands.entity(r).despawn();
        }
        menu.built = None;
        return;
    }
    let Ok((mut p, a)) = wolf.single_mut() else { return };
    // Knocked off the idol (the world runs on): the menu closes. (SHINOBI_IDOL_PAGE captures open
    // it with no seat.)
    if (p.seat.idol.is_none() && !p.seat.sculptor || a.hp <= 0.0) && std::env::var("SHINOBI_IDOL_PAGE").is_err() {
        next.set(Screen::Playing);
        return;
    }
    let any = |k: &[KeyCode]| k.iter().any(|k| keys.just_pressed(*k));
    let (mut down, mut up) = (any(&[KeyCode::ArrowDown, KeyCode::KeyS]), any(&[KeyCode::ArrowUp, KeyCode::KeyW]));
    // The wheel: the lists' cursor moves by its notches and stops at the ends; the trees and
    // Travel take a notch as an arrow press.
    let list_len = match menu.page {
        Page::Main => Some(ROWS.len()),
        Page::Form => Some(crate::form::FORMS.len()),
        Page::Workshop => Some(WORKSHOP_ROWS.len()),
        Page::Fit => Some(crate::workshop::TOOLS.len()),
        _ => None,
    };
    match list_len {
        Some(n) if pointer.wheel != 0 => menu.index = crate::menu::wheel_step(menu.index, pointer.wheel, n),
        None if pointer.wheel != 0 && !menu.tree.asking => {
            up |= pointer.wheel > 0;
            down |= pointer.wheel < 0;
        }
        _ => {}
    }
    let (right, left) = (any(&[KeyCode::ArrowRight, KeyCode::KeyD]), any(&[KeyCode::ArrowLeft, KeyCode::KeyA]));
    let mut confirm = any(&[KeyCode::Enter, KeyCode::Space]);
    let back = keys.just_pressed(KeyCode::Escape);
    // The skill trees' pages: C / V (E and Q are OK and back in every menu, menu.rs).
    let (prev_page, next_page) = (any(&[KeyCode::KeyC]), any(&[KeyCode::KeyV]));
    keys.reset_all();
    mouse.reset_all();
    let mut clicked = None;
    for (r, i) in &rows {
        match i {
            Interaction::Pressed => clicked = Some(r.0),
            Interaction::Hovered if pointer.moved && matches!(menu.page, Page::Main | Page::Form | Page::Workshop | Page::Fit) => menu.index = r.0,
            _ => {}
        }
    }
    let command = |cmd: Cmd, p: &mut crate::player::Player, next: &mut NextState<Screen>| {
        p.seat.cmd = Some(cmd);
        next.set(Screen::Playing);
    };
    match menu.page {
        Page::Main => {
            if let Some(i) = clicked {
                menu.index = i;
                confirm = true;
            }
            if down {
                menu.index = (menu.index + 1) % ROWS.len();
            }
            if up {
                menu.index = (menu.index + ROWS.len() - 1) % ROWS.len();
            }
            if back {
                command(Cmd::Leave, &mut p, &mut next);
                return;
            }
            if confirm {
                match ROWS[menu.index].0 {
                    "Rest" => {
                        command(Cmd::Rest, &mut p, &mut next);
                        return;
                    }
                    "Purchase Spirit Emblems" => {
                        menu.page = Page::Purchase;
                        menu.index = 0;
                    }
                    "Change Form" => {
                        menu.page = Page::Form;
                        menu.index = crate::form::current(&config);
                    }
                    "Acquire Skills" => {
                        menu.page = Page::Skills;
                        menu.tree = crate::skill_tree::View { page: crate::skill_tree::available_pages(&config.player.items).first().copied().unwrap_or(0), ..default() };
                    }
                    "Travel" => {
                        menu.page = Page::Travel;
                        menu.region = regions().iter().position(|(_, v)| !v.is_empty()).unwrap_or(0);
                        menu.in_list = false;
                        menu.index = 0;
                    }
                    _ => {}
                }
            }
        }
        Page::Purchase => {
            if back {
                menu.page = Page::Main;
                menu.index = 1;
            } else if (confirm || clicked.is_some()) && p.emblems < config.player.spirit_emblems && config.player.sen >= EMBLEM_PRICE {
                config.player.sen -= EMBLEM_PRICE;
                p.emblems += 1;
            }
        }
        Page::Workshop => {
            if let Some(i) = clicked {
                menu.index = i;
                confirm = true;
            }
            let n = WORKSHOP_ROWS.len();
            if down {
                menu.index = (menu.index + 1) % n;
            }
            if up {
                menu.index = (menu.index + n - 1) % n;
            }
            if back {
                if menu.sculptor {
                    command(Cmd::Leave, &mut p, &mut next);
                    return;
                }
                menu.page = Page::Main;
                menu.index = 0;
            } else if confirm {
                menu.page = if menu.index == 0 { Page::Fit } else { Page::Upgrade };
                menu.index = 0;
                menu.tree.asking = false;
            }
        }
        Page::Fit => {
            if let Some(i) = clicked {
                menu.index = i;
                confirm = true;
            }
            let n = crate::workshop::TOOLS.len();
            if down {
                menu.index = (menu.index + 1) % n;
            }
            if up {
                menu.index = (menu.index + n - 1) % n;
            }
            if back {
                menu.page = Page::Workshop;
                menu.index = 0;
            } else if confirm && crate::workshop::craft(&mut config, crate::workshop::TOOLS[menu.index]) {
                info!("made tool {}", crate::workshop::TOOLS[menu.index]);
            }
        }
        Page::Upgrade => {
            let all = crate::workshop::upgrades();
            let mut buy = false;
            for (u, i) in &ups {
                match i {
                    Interaction::Pressed if !menu.tree.asking => {
                        menu.index = u.0;
                        buy = true;
                    }
                    Interaction::Hovered if pointer.moved && !menu.tree.asking => menu.index = u.0,
                    _ => {}
                }
            }
            let here = all.get(menu.index).cloned();
            if menu.tree.asking {
                if confirm {
                    if let Some(u) = &here {
                        if crate::workshop::buy(&mut config, u) {
                            info!("upgrade {} -> tool {}", u.id, u.weapon);
                        }
                    }
                    menu.tree.asking = false;
                } else if back {
                    menu.tree.asking = false;
                }
            } else if back {
                menu.page = Page::Workshop;
                menu.index = 1;
            } else {
                for (go, dx, dy) in [(right, 1, 0), (left, -1, 0), (down, 0, 1), (up, 0, -1)] {
                    if go {
                        if let Some(i) = crate::workshop::step(menu.index, dx, dy) {
                            menu.index = i;
                        }
                    }
                }
                if (confirm || buy) && here.as_ref().is_some_and(|u| crate::workshop::check(&config, u).is_ok()) {
                    menu.tree.asking = true;
                }
            }
        }
        Page::Form => {
            let n = crate::form::FORMS.len();
            if let Some(i) = clicked {
                menu.index = i;
                confirm = true;
            }
            if down {
                menu.index = (menu.index + 1) % n;
            }
            if up {
                menu.index = (menu.index + n - 1) % n;
            }
            if back {
                menu.page = Page::Main;
                menu.index = ROWS.iter().position(|r| r.0 == "Change Form").unwrap_or(0);
            } else if confirm && crate::form::current(&config) != menu.index {
                config.player.form = menu.index as u32;
                info!("form: {}", crate::form::FORMS[menu.index].protector);
            }
        }
        Page::Skills => {
            use crate::skill_tree as st;
            let tree = st::tree();
            let pages = st::available_pages(&config.player.items);
            // Config reloads can remove a book while this screen (or its confirmation) is up.
            if !pages.contains(&menu.tree.page) {
                menu.tree = st::View { page: pages.first().copied().unwrap_or(0), ..default() };
            }
            let mut acquire = false;
            for (n, i) in &nodes {
                match i {
                    Interaction::Pressed if !menu.tree.asking => {
                        menu.tree.cursor = n.0;
                        acquire = true;
                    }
                    Interaction::Hovered if pointer.moved && !menu.tree.asking => menu.tree.cursor = n.0,
                    _ => {}
                }
            }
            for (c, i) in &cats {
                if *i == Interaction::Pressed && !menu.tree.asking {
                    if let Some(&page) = pages.get(c.0) {
                        menu.tree = st::View { page, ..default() };
                    }
                }
            }
            let nodes_here = st::page(tree, menu.tree.page);
            let here = nodes_here.get(menu.tree.cursor).map(|s| (*s).clone());
            if menu.tree.asking {
                if confirm {
                    if let Some(s) = here.filter(|s| st::check(tree, s, &config.player.skills, config.player.skill_points, &config.player.items).is_ok()) {
                        config.player.skill_points -= s.cost;
                        config.player.skills.push(s.id);
                        info!("acquired skill {} (SkillParam), {} skill points left", s.id, config.player.skill_points);
                    }
                    menu.tree.asking = false;
                } else if back {
                    menu.tree.asking = false;
                }
            } else if back {
                menu.page = Page::Main;
                menu.index = 3;
            } else if prev_page || next_page {
                if let Some(tab) = pages.iter().position(|&page| page == menu.tree.page) {
                    let n = pages.len();
                    menu.tree = st::View { page: pages[if next_page { (tab + 1) % n } else { (tab + n - 1) % n }], ..default() };
                }
            } else {
                for (go, dx, dy) in [(right, 1, 0), (left, -1, 0), (down, 0, 1), (up, 0, -1)] {
                    if go {
                        if let Some(i) = st::step(&nodes_here, menu.tree.cursor, dx, dy) {
                            menu.tree.cursor = i;
                        }
                    }
                }
                if (confirm || acquire) && here.is_some_and(|s| st::check(tree, &s, &config.player.skills, config.player.skill_points, &config.player.items).is_ok()) {
                    menu.tree.asking = true;
                }
            }
        }
        Page::Travel => {
            let regions = regions();
            if let Some(i) = clicked {
                menu.region = i;
                menu.in_list = false;
            }
            if !menu.in_list {
                if down {
                    menu.region = (menu.region + 1) % regions.len();
                }
                if up {
                    menu.region = (menu.region + regions.len() - 1) % regions.len();
                }
                if (confirm || right) && regions.get(menu.region).is_some_and(|(_, v)| !v.is_empty()) {
                    menu.in_list = true;
                    menu.index = 0;
                } else if back {
                    menu.page = Page::Main;
                    menu.index = 2;
                }
            } else {
                let n = regions.get(menu.region).map_or(1, |(_, v)| v.len().max(1));
                if down {
                    menu.index = (menu.index + 1) % n;
                }
                if up {
                    menu.index = (menu.index + n - 1) % n;
                }
                if back || left {
                    menu.in_list = false;
                } else if confirm {
                    if let Some(&to) = regions.get(menu.region).and_then(|(_, v)| v.get(menu.index)) {
                        let can = IDOLS.read().ok().and_then(|v| v.get(to).map(reachable)).unwrap_or(false);
                        if p.seat.idol != Some(to) && can {
                            command(Cmd::Warp(to), &mut p, &mut next);
                            return;
                        }
                    }
                }
            }
        }
    }
    if menu.page == Page::Fit {
        menu.top = crate::game_menu::keep_visible(menu.top, menu.index, crate::workshop::TOOLS.len());
    } else {
        menu.top = 0;
    }
    let key = format!("{} ", menu.top) + &format!("{:?} {} {} {} {} {} {:?} {:?} {} {} {:?} {:?} {:?}", menu.page, menu.index, menu.in_list, menu.region, p.emblems, config.player.sen, menu.tree, config.player.skills, config.player.skill_points, config.player.form, config.player.upgrades, config.player.owned_tools, config.player.items);
    if menu.built.as_ref() != Some(&key) {
        for r in &roots {
            commands.entity(r).despawn();
        }
        build(&mut commands, &assets, &menu, &font.ui, &config, &tables, &texts, &forms, &p, a);
        menu.built = Some(key);
    }
}

/// The idol's model: the game's Sculptor's Idol (obj o005000, the model of 35 of the 58
/// BonfireWarpParam idols; textures o005500_body_a / _n, shared with the shrine idol o005500),
/// exported by `sekiro-extract objmesh` to extracted/objmesh/o005000.json. X mirrored as every
/// FromSoftware position; the winding stays as stored (the mirror already turns it the way Bevy culls).
fn idol_mesh(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, assets: &AssetServer) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>)> {
    let path = crate::paths::extracted().join("objmesh/o005000.json");
    let Some(json) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Vec::new() };
    let floats = |v: &serde_json::Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect::<Vec<f32>>()).unwrap_or_default();
    let tex = |stem: &str, srgb: bool| -> Option<Handle<Image>> {
        (!stem.is_empty() && crate::paths::extracted().join(format!("tex/{stem}.dds")).is_file()).then(|| {
            assets.load_builder().with_settings(move |s: &mut bevy::image::ImageLoaderSettings| s.is_srgb = srgb).load(format!("tex/{stem}.dds"))
        })
    };
    let mut out = Vec::new();
    for m in json["meshes"].as_array().into_iter().flatten() {
        let (pos, nrm, uv) = (floats(&m["pos"]), floats(&m["normal_vec"]), floats(&m["uv"]));
        let idx: Vec<u32> = m["indices"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|i| i as u32)).collect()).unwrap_or_default();
        let n = pos.len() / 3;
        if n == 0 || idx.len() < 3 {
            continue;
        }
        let positions: Vec<[f32; 3]> = pos.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect();
        let normals: Vec<[f32; 3]> = nrm.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect();
        let uvs: Vec<[f32; 2]> = uv.chunks(2).map(|p| [p[0], p[1]]).collect();
        let tris: Vec<u32> = idx.chunks(3).filter(|t| t.len() == 3).flatten().copied().collect();
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        if normals.len() == n {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
        if uvs.len() == n {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        }
        mesh.insert_indices(bevy::mesh::Indices::U32(tris));
        // No normal map: the game's _n textures pack their channels for its own shaders (the map
        // decodes them in map_material.wgsl); read as a plain tangent-space map they turned the
        // carving dark with only its rims lit.
        let albedo = tex(m["albedo"].as_str().unwrap_or(""), true);
        let mat = StandardMaterial {
            // The flame lights the carving from its own hands: a faint blue glow of the wood's
            // texture, so the idol does not stand as a dark shape against the sun.
            emissive: LinearRgba::from(IDOL_LIGHT) * IDOL_GLOW,
            emissive_texture: albedo.clone(),
            base_color_texture: albedo,
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        };
        out.push((meshes.add(mesh), mats.add(mat)));
    }
    out
}

/// The light at an idol: the game's cold blue-white idol glow (the user: "the light coming from
/// the idol should be blue"), under the idol's own effect's light.
const IDOL_LIGHT: Color = Color::srgb(0.45, 0.62, 1.0);

/// The emissive strength of the idol's own glow (by eye against the parkour map's daylight).
const IDOL_GLOW: f32 = 0.15;

/// The lit idol's flame: o005000's TAE plays FFX 1011 on dummy 100 (the model's origin) in slot 1
/// in its lit animations 10, 11, 13 and 100 (1010 in the unlit 1, 1000 the lighting flash).
const IDOL_SFX: i64 = 1011;
/// The heights (m) the flame's copies play at over dummy 100.
const IDOL_SFX_LIFTS: [f32; 2] = [0.0, 0.15];

/// The id the flame without its mist plays under (not a game id).
const IDOL_SFX_NO_MIST: i64 = 9_001_011;

/// FFX 1011 without its ground mist: node 0/0/1/0 (big soft 21051 puffs from a ring at the
/// idol's feet, 12 s each) drew a blue fog round the base that the game's idols do not show
/// (the user: "there was no smoke at the idol"). The flame, sparks and light are kept.
fn flame_without_mist(lib: &mut crate::fxr::FxrLib) -> i64 {
    let Some(v) = lib.get(IDOL_SFX) else { return IDOL_SFX };
    let mut v = (*v).clone();
    let Some(nodes) = v.pointer_mut("/root/nodes/0/nodes/0/nodes/1/nodes").and_then(|n| n.as_array_mut()) else { return IDOL_SFX };
    let mist = nodes.first().and_then(|n| n.pointer("/configs/0/nodeTransform/rotation/0")).and_then(|r| r.as_f64()) == Some(-90.0);
    if !mist {
        warn!("idol flame {IDOL_SFX}: the ground mist node is not where expected; played whole");
        return IDOL_SFX;
    }
    nodes.remove(0);
    lib.put(IDOL_SFX_NO_MIST, v);
    IDOL_SFX_NO_MIST
}

/// The idols of the map: installed, and drawn with the game's model (a stand-in figure if the
/// model is not exported), with the idol's blue light.
fn spawn_idols(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, assets: Res<AssetServer>, config: Res<crate::config::GameConfig>, fxr: Option<ResMut<crate::fxr::FxrLib>>) {
    // Under sekiro-rs's map the game's idols: its map draws them (and their light) itself.
    if crate::sr_map::world() {
        let idols = sr_idols();
        info!("idols: {} of the game's, {} in this play world", idols.len(), idols.iter().filter(|i| !i.away).count());
        if let Ok(mut v) = IDOLS.write() {
            *v = idols;
        }
        return;
    }
    let idols = if crate::map::map_id(&config).as_deref() == Some(crate::sandbox::CONSTRUCT_ID) { parkour_idols() } else { Vec::new() };
    let model = if idols.is_empty() { Vec::new() } else { idol_mesh(&mut meshes, &mut mats, &assets) };
    let flame = fxr.map_or(IDOL_SFX, |mut lib| flame_without_mist(&mut lib));
    let wood = mats.add(StandardMaterial { base_color: Color::srgb(0.32, 0.2, 0.11), perceptual_roughness: 0.85, ..default() });
    for idol in &idols {
        let rot = Quat::from_rotation_y(f32::atan2(idol.out.x, idol.out.z) + IDOL_FACING);
        let at = |off: Vec3| Transform::from_translation(idol.pos + rot * off).with_rotation(rot);
        if model.is_empty() {
            commands.spawn((Mesh3d(meshes.add(Capsule3d::new(0.2, 0.3))), MeshMaterial3d(wood.clone()), at(Vec3::new(0.0, 0.45, 0.0))));
        } else {
            for (mesh, mat) in &model {
                commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), at(Vec3::ZERO)));
            }
        }
        // A blue pool round it (the user: "a good amount of blue light around it"): at the flame's
        // height a little in front, wide enough to tint the ground round the idol in daylight.
        commands.spawn((PointLight { color: IDOL_LIGHT, intensity: 60_000.0, range: 7.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(idol.pos + idol.out * 0.6 + Vec3::Y * 1.2)));
        // Two copies, the second a little higher: one alone burns thinner and lower than the
        // game's plume (the user's capture); the effects play at scale 1 (fxr.rs `place`).
        for lift in IDOL_SFX_LIFTS {
            let mut fx = crate::fxr::FxEffect::new(flame, None, false, at(Vec3::Y * lift));
            fx.keep = true;
            commands.spawn(fx);
        }
    }
    if let Ok(mut v) = IDOLS.write() {
        *v = idols;
    }
}

/// The model's front against `Idol::out` (rad): the carved face looks along its own -Z.
const IDOL_FACING: f32 = std::f32::consts::PI;

pub struct IdolPlugin;

impl Plugin for IdolPlugin {
    fn build(&self, app: &mut App) {
        // SHINOBI_IDOL_PAGE=<0-6>: open on Main / Purchase / Travel / Acquire Skills / Change Form / Create Arm Tools / Tool Upgrades (SHINOBI_SHOT captures with
        // SHINOBI_TITLE=idol).
        let page = match std::env::var("SHINOBI_IDOL_PAGE").ok().as_deref() {
            Some("1") => Page::Purchase,
            Some("2") => Page::Travel,
            Some("3") => Page::Skills,
            Some("4") => Page::Form,
            Some("5") => Page::Fit,
            Some("6") => Page::Upgrade,
            _ => Page::Main,
        };
        // SHINOBI_TREE_PAGE=<0-4> / SHINOBI_TREE_CURSOR=<n>: the Acquire Skills tree and node shown.
        let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let tree = crate::skill_tree::View { page: num("SHINOBI_TREE_PAGE"), cursor: num("SHINOBI_TREE_CURSOR"), asking: false };
        app.insert_resource(crate::form::FormTexts::load());
        app.insert_resource(IdolMenu { page, region: if page == Page::Travel { 1 } else { 0 }, tree, ..default() })
            .add_systems(Startup, spawn_idols)
            .add_systems(Update, open_menu)
            .add_systems(PreUpdate, run_idol_menu.after(crate::menu::MenuInput));
    }
}
