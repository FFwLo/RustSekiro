//! Sculptor's Idols (#128): near one, the game's guide shows with its key, "Commune with
//! Sculptor's Idol" (イベントテキスト 10010182) at an idol not yet lit and "Rest" (15000120)
//! at a lit one (`Duel::idol_lit`, #187); the action button (E, or Y on a pad) kneels Wolf there (`Duel::idol_input`,
//! which also makes it the checkpoint), and once he prays the idol menu opens: the idol's name
//! (BonfireWarpParam `msgId` in NTC_メニューテキスト), the game's rows with their help lines,
//! Rest and Travel (`super::idol_travel`, #148) working and the rest greyed, and the key guide.
//! Esc (or B) stands him up. While the menu is up the mouse is free, as in the in-game menu
//! (#182): the pointer over a row moves the cursor there and a click takes it, as OK does.
//!
//! The idols are the play world's objects whose entity is a BonfireWarpParam `bonfireEntityId`
//! (o005000, o005500, o115040), facing their front (the object's -Z). Layout, sizes and colours
//! from FwLo's RustSekiro `src/idol.rs`, `src/action_guide.rs` and `src/game_menu.rs`
//! (Fireguy32's idols, set from captures of the game), in percent of the screen and pixels of
//! its 1920 x 1080 stage.
//!
//! Each idol's warp point, where a Travel to it and a respawn after death stand Wolf, is the
//! MSB player part (c0000) whose entity is the bonfire entity minus [`WARP_POINT`]: every one of
//! the 58 BonfireWarpParam rows has one in its own map, 1 to 7 m in front of the idol, with the
//! facing Wolf takes there (docs/FIDELITY.md).
//! gap: the menu's stats plate, Purchase Spirit Emblems, skills and forms (#73, #96).
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button, observe};
use bevy::window::{CursorGrabMode, CursorMoved, CursorOptions};
use sekiro_sim::arena::yaw_toward;
use sekiro_sim::deathblow::Placement;
use sekiro_sim::duel::idol::{IdolPhase, IdolSpot};
use sekiro_sim::kit::Kit;

use super::Sim;
use super::idol_travel::{self, Atlas};
use crate::game_ui::{GameFonts, MenuArt, abs, stage};

/// How far in front of the idol Wolf kneels, and how much further out a respawn stands him when
/// the map has no warp point for it (m).
const KNEEL: f32 = 0.9;
const RESPAWN: f32 = 0.4;
/// An idol's warp point is the player part of entity `bonfireEntityId - WARP_POINT` (idol
/// 1101950, Dilapidated Temple, warps to c0000 part 1100970).
pub(super) const WARP_POINT: i32 = 980;
/// イベントテキスト: "Rest" (the guide and the first row), "Travel", and the menu's rows in the
/// game's order (each one's help line is its id + 100000) with whether they work here.
const REST: i32 = 15000120;
/// イベントテキスト: the guide at an idol not yet lit, "Commune with Sculptor's Idol".
const COMMUNE: i32 = 10010182;
pub(super) const TRAVEL: i32 = 15000150;
const ROWS: [(i32, bool); 10] = [
    (REST, true),
    (15000140, false), // Purchase Spirit Emblems
    (TRAVEL, true),
    (15000300, false), // Acquire Skills
    (15000170, false), // Enhance Physical Attributes
    (15000600, false), // Enhance Attack Power
    (15000250, false), // Dragonrot Restoration
    (15000902, false), // Change Form
    (15000900, false), // Reflection of Strength
    (15000901, false), // Gauntlet of Strength
];
pub(super) const HELP: i32 = 100000;
/// NTC_キーガイド: "OK", "Close".
pub(super) const KEY_OK: i32 = 100000;
const KEY_CLOSE: i32 = 110001;
pub(super) const TEXT: Color = Color::srgb(0.88, 0.85, 0.78);
pub(super) const DIM: Color = Color::srgba(0.75, 0.72, 0.66, 0.85);
pub(super) const GREYED: Color = Color::srgba(0.7, 0.68, 0.64, 0.4);
pub(super) const CURSOR: Color = Color::srgba(0.8, 0.55, 0.3, 0.95);

/// The play world's idols: where Wolf kneels at each (for the sim) and its name, and the
/// game's Travel list.
#[derive(Resource, Default)]
pub(super) struct Idols {
    pub spots: Vec<IdolSpot>,
    /// Each idol object's place and facing, where its effects play (dummy 100 is the model's
    /// origin).
    pub origins: Vec<Transform>,
    names: Vec<String>,
    pub atlas: Atlas,
}

impl Idols {
    /// The idols in the play blocks' `layouts`, standing on `floor(x, z, near_y)`.
    pub fn load(
        cache: &Path,
        layouts: &BTreeMap<String, serde_json::Value>,
        floor: impl Fn(f32, f32, f32) -> Option<f32>,
    ) -> Self {
        let names = crate::game_ui::menu_texts(cache, "NTC_メニューテキスト");
        let rows = sekiro_sim::registry::rows(cache, "BonfireWarpParam")
            .inspect_err(|e| warn!("idols: {e}"))
            .unwrap_or_default();
        let mut idols = Self::place(&warps(rows.values()), &names, layouts, floor);
        idols.atlas = Atlas::load(cache, rows.values(), &names);
        idols
    }

    /// The objects of `layouts` that are idols of `warps`, named from `names`, each with its
    /// warp point from the layouts' player parts.
    fn place(
        warps: &HashMap<i32, i32>,
        names: &BTreeMap<i32, String>,
        layouts: &BTreeMap<String, serde_json::Value>,
        floor: impl Fn(f32, f32, f32) -> Option<f32>,
    ) -> Self {
        let players: HashMap<i32, &serde_json::Value> = layouts
            .values()
            .flat_map(|l| l["players"].as_array().into_iter().flatten())
            .filter_map(|p| Some((int(&p["entity_id"])?, p)))
            .collect();
        let mut idols = Self::default();
        for layout in layouts.values() {
            for o in layout["objects"].as_array().into_iter().flatten() {
                let Some((entity, msg)) =
                    int(&o["entity_id"]).and_then(|e| Some((e, warps.get(&e)?)))
                else {
                    continue;
                };
                let Some(pose) = crate::map::pose(o) else {
                    continue;
                };
                let base = pose.translation;
                let out = (pose.rotation.normalize() * Vec3::NEG_Z)
                    .with_y(0.0)
                    .normalize_or(Vec3::Z);
                let on_floor = |p: Vec3| p.with_y(floor(p.x, p.z, p.y).unwrap_or(p.y));
                let kneel = on_floor(base + out * KNEEL);
                let yaw = yaw_toward(kneel, base);
                let checkpoint = match players.get(&(entity - WARP_POINT)) {
                    Some(p) => Placement {
                        position: on_floor(crate::map::pose(p).map_or(base, |t| t.translation))
                            .to_array(),
                        yaw: (p["yaw_degrees"].as_f64().unwrap_or(0.0) as f32).to_radians(),
                    },
                    None => {
                        warn!("idol {entity}: no warp point {}", entity - WARP_POINT);
                        Placement {
                            position: on_floor(base + out * (KNEEL + RESPAWN)).to_array(),
                            yaw: yaw + std::f32::consts::PI,
                        }
                    }
                };
                idols
                    .origins
                    .push(Transform::from_translation(base).with_rotation(pose.rotation));
                idols.spots.push(IdolSpot {
                    kneel: Placement {
                        position: kneel.to_array(),
                        yaw,
                    },
                    checkpoint,
                    entity,
                });
                idols
                    .names
                    .push(names.get(msg).cloned().unwrap_or_default());
            }
        }
        idols
    }

    /// The play world's idol of bonfire `entity`, as an index of `spots`.
    pub fn index(&self, entity: i32) -> Option<usize> {
        self.spots.iter().position(|s| s.entity == entity)
    }
}

/// BonfireWarpParam `rows` by their bonfire entity: the idol's name (`msgId`).
fn warps<'a>(rows: impl IntoIterator<Item = &'a serde_json::Value>) -> HashMap<i32, i32> {
    rows.into_iter()
        .filter_map(|r| Some((int(&r["bonfireEntityId"])?, int(&r["msgId"])?)))
        .collect()
}

/// A JSON integer that fits an `i32`.
pub(super) fn int(v: &serde_json::Value) -> Option<i32> {
    i32::try_from(v.as_i64()?).ok()
}

/// The row picked in the idol menu.
#[derive(Resource, Default)]
pub(super) struct Row(usize);

/// This frame's mouse on an idol screen's parts `P`: the parts the pointer came over and the
/// parts clicked, taken by the screen's key system.
#[derive(Resource)]
pub(super) struct Pointed<P> {
    hovered: Vec<P>,
    clicked: Vec<P>,
}

impl<P> Default for Pointed<P> {
    fn default() -> Self {
        Self {
            hovered: Vec::new(),
            clicked: Vec::new(),
        }
    }
}

impl<P> Pointed<P> {
    /// Takes this frame's hovered and clicked parts. A hover counts only when the pointer
    /// moved (`moved`): a screen built again under a still pointer, or a menu opening under
    /// it, must not pull the cursor away from the keys' row.
    pub fn take(&mut self, moved: bool) -> (Vec<P>, Vec<P>) {
        let hovered = std::mem::take(&mut self.hovered);
        let clicked = std::mem::take(&mut self.clicked);
        (if moved { hovered } else { Vec::new() }, clicked)
    }
}

/// What makes a part `part` of an idol screen take the mouse: the pointer coming over it and a
/// press on it (a `Button` that activates on press, as the in-game menu's) go to [`Pointed`].
pub(super) fn pointable<P: Copy + Send + Sync + 'static>(part: P) -> impl Bundle {
    (
        Button,
        ActivateOnPress,
        observe(move |_: On<Activate>, mut p: ResMut<Pointed<P>>| p.clicked.push(part)),
        observe(
            move |_: On<bevy::picking::events::PointerOver>, mut p: ResMut<Pointed<P>>| {
                p.hovered.push(part)
            },
        ),
    )
}

#[derive(Component)]
struct Guide;
/// The guide's label, with its texts at an unlit idol and a lit one.
#[derive(Component)]
struct GuideLabel {
    commune: String,
    rest: String,
}
#[derive(Component)]
struct Menu;
#[derive(Component)]
struct Place;
#[derive(Component)]
struct Cursor(usize);
#[derive(Component)]
struct Label(usize);
/// The help line, with each row's.
#[derive(Component)]
struct HelpLine(Vec<String>);

/// The idol guide and menu; nothing when the play world has no idols.
pub(super) fn install(app: &mut App, idols: Idols) {
    if idols.spots.is_empty() {
        return;
    }
    GameFonts::insert(app);
    MenuArt::insert(app);
    crate::game_ui::install_cover(app);
    let events = crate::game_ui::menu_texts(&sekiro_sim::cache::root(), "イベントテキスト");
    super::idol_found::install(app, &events);
    super::idol_rest::install(app);
    idol_travel::install(app, &events);
    app.insert_resource(idols)
        .init_resource::<Row>()
        .init_resource::<Pointed<usize>>()
        .init_resource::<crate::menu::Modal>()
        .init_resource::<crate::map_fx::FoundIdols>()
        .add_systems(Update, found_idols)
        .add_systems(
            Startup,
            move |commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>| {
                spawn(commands, fonts, art, &events)
            },
        )
        .add_systems(
            PreUpdate,
            menu_keys
                .after(bevy::input::InputSystems)
                .after(bevy::picking::PickingSystems::Last)
                .run_if(crate::menu::active),
        )
        .add_systems(
            Update,
            (show, guide_label, hold_pause, free_cursor).run_if(crate::menu::active),
        );
}

/// Hands the map's effects the idols Wolf has lit, so each one's idle effect follows its
/// state: the unlit embers, then the flame above its hands (#154). Logged when one is lit.
fn found_idols(sim: NonSend<Sim>, mut found: ResMut<crate::map_fx::FoundIdols>) {
    let lit = (0..sim.duel.idol_spots().len()).filter(|&i| sim.duel.idol_lit(i));
    for i in lit {
        let entity = sim.duel.idol_spots()[i].entity;
        if found.0.insert(entity) {
            info!("idol {entity}: lit, its idle effect is the lit one now");
        }
    }
}

/// The guide and the menu, hidden, with their texts from `events` (イベントテキスト).
fn spawn(
    mut commands: Commands,
    fonts: Res<GameFonts>,
    art: Res<MenuArt>,
    events: &BTreeMap<i32, String>,
) {
    let cache = sekiro_sim::cache::root();
    let key = |name: &str, height: f32| {
        art.sprite("SB_KeyTop", name).map(|(image, aspect)| {
            (
                image,
                Node {
                    height: stage(height),
                    aspect_ratio: Some(aspect),
                    ..default()
                },
            )
        })
    };
    let guides = crate::game_ui::menu_texts(&cache, "NTC_キーガイド");
    let event = |id: i32| events.get(&id).cloned().unwrap_or_default();
    let text = |t: String, px: f32, color: Color| fonts.shadowed(t, px, color);

    // The guide, low in the middle: "[E] : Rest", or "Commune with Sculptor's Idol" at an idol
    // not yet lit (`guide_label`).
    let label = crate::game_ui::spawn_action_guide(&mut commands, &fonts, &art, "", Guide);
    commands.entity(label).insert(GuideLabel {
        commune: event(COMMUNE),
        rest: event(REST),
    });

    // The menu: a dark fade on the left with the idol's name, the rows, the help line and the
    // key guide.
    let cursor = art
        .sprite("SB_In_Game", "MENU_Cursor_03")
        .map(|(image, _)| ImageNode {
            image_mode: bevy::ui::widget::NodeImageMode::Stretch,
            ..image.with_color(CURSOR)
        })
        .unwrap_or_default();
    commands
        .spawn((
            Menu,
            crate::game_ui::Covers,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            GlobalZIndex(110),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((abs(0.0, 0.0, 33.0, 100.0), BackgroundColor(fade(0.93))));
            root.spawn((
                abs(33.0, 0.0, 10.0, 100.0),
                BackgroundGradient(vec![
                    LinearGradient::to_right(vec![fade(0.93).into(), fade(0.0).into()]).into(),
                ]),
            ));
            root.spawn(abs(5.9, 7.6, 30.0, 5.0))
                .with_child((Place, text(String::new(), 21.0, Color::WHITE)));
            root.spawn((
                abs(0.0, 12.8, 32.0, 0.2),
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)),
            ));
            for (i, (id, _)) in ROWS.iter().enumerate() {
                let t = 14.6 + i as f32 * 5.6;
                let row = abs(4.0, t, 30.0, 4.8);
                root.spawn((Cursor(i), row.clone(), cursor.clone()));
                root.spawn((
                    Node {
                        align_items: AlignItems::Center,
                        // 6.5 % of the row.
                        padding: UiRect::left(percent(6.5 * 0.3)),
                        ..row
                    },
                    pointable(i),
                    children![(Label(i), text(event(*id), 17.0, TEXT))],
                ));
                root.spawn((
                    abs(5.9, t + 4.9, 25.5, 0.15),
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.18)),
                ));
            }
            let lines = ROWS.iter().map(|(id, _)| event(id + HELP)).collect();
            root.spawn(abs(5.9, 89.6, 30.0, 4.0))
                .with_child((HelpLine(lines), text(String::new(), 16.5, TEXT)));
            root.spawn(Node {
                position_type: PositionType::Absolute,
                left: percent(5.3),
                top: percent(93.6),
                align_items: AlignItems::Center,
                column_gap: px(4),
                ..default()
            })
            .with_children(|r| {
                for (name, label) in [("KG_Key_Enter", KEY_OK), ("KG_Key_Esc", KEY_CLOSE)] {
                    if let Some(icon) = key(name, 20.0) {
                        r.spawn(icon);
                    }
                    let label = guides.get(&label).cloned().unwrap_or_default();
                    r.spawn(text(format!(":{label}  "), 16.0, DIM));
                }
            });
        });
}

fn fade(alpha: f32) -> Color {
    Color::BLACK.with_alpha(alpha)
}

/// The menu's keys and mouse while Wolf prays (not while a first light's banner plays): up and
/// down pick a row, OK (Enter, Space, E, A) takes it, Esc or back (Q, B) stands him up. The
/// pointer over a row picks it and a click takes it.
#[allow(clippy::too_many_arguments)]
pub(super) fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut moved: MessageReader<CursorMoved>,
    mut pointed: ResMut<Pointed<usize>>,
    mut row: ResMut<Row>,
    mut travel: ResMut<idol_travel::TravelMenu>,
    mut rested: ResMut<super::idol_rest::KitBefore>,
    idols: Res<Idols>,
    mut sim: NonSendMut<Sim>,
) {
    let (hovered, clicked) = pointed.take(moved.read().count() > 0);
    if sim.duel.idol_phase() != Some(IdolPhase::Praying) || travel.open {
        return;
    }
    let pad = |b| pads.iter().any(|p| p.just_pressed(b));
    let key = |k: &[KeyCode]| keys.any_just_pressed(k.iter().copied());
    if let Some(&i) = hovered.last() {
        row.0 = i;
    }
    if key(&[KeyCode::ArrowDown, KeyCode::KeyS]) || pad(GamepadButton::DPadDown) {
        row.0 = (row.0 + 1) % ROWS.len();
    }
    if key(&[KeyCode::ArrowUp, KeyCode::KeyW]) || pad(GamepadButton::DPadUp) {
        row.0 = (row.0 + ROWS.len() - 1) % ROWS.len();
    }
    let clicked = clicked.last().copied();
    if let Some(i) = clicked {
        row.0 = i;
    }
    if crate::menu::ok_pressed(&keys, &pads) || clicked.is_some() {
        match ROWS[row.0].0 {
            REST => {
                let before = sim.duel.kit.clone();
                sim.duel.rest_at_idol();
                if sim.duel.idol_rest().is_some() {
                    info!("idol: rest taken");
                    rested.0 = Some(before);
                }
            }
            TRAVEL => travel.open_at(&idols, &sim.duel),
            _ => {}
        }
        return;
    }
    if key(&[KeyCode::Escape, KeyCode::Backspace]) || crate::menu::back_pressed(&keys, &pads) {
        sim.duel.leave_idol();
    }
}

/// What a rest changed in the kit, for the log (#202): the gourd's uses, the Pellets (goods
/// 3020) held and stored, and every other goods whose held or stored count moved.
pub(super) fn kit_change(before: &Kit, after: &Kit) -> String {
    const PELLET: i32 = 3020;
    let ids: BTreeSet<i32> = [before, after]
        .iter()
        .flat_map(|k| {
            k.goods
                .iter()
                .map(|(id, _)| *id)
                .chain(k.storage.keys().copied())
        })
        .chain([sekiro_sim::items::SPIRIT_EMBLEM, PELLET])
        .collect();
    let counts = |k: &Kit, id| (k.count(id), k.stored(id));
    let mut out = format!("gourd {}->{}", before.gourd_charges, after.gourd_charges);
    for id in ids {
        let (b, a) = (counts(before, id), counts(after, id));
        if b != a || id == PELLET {
            out += &format!(", goods {id} held {}->{} stored {}->{}", b.0, a.0, b.1, a.1);
        }
    }
    out
}

/// The idol menu is up while Wolf prays and through a rest.
fn menu_open(sim: &Sim) -> bool {
    matches!(
        sim.duel.idol_phase(),
        Some(IdolPhase::Praying | IdolPhase::Resting)
    )
}

/// The guide near an idol and the menu while Wolf prays.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
fn show(
    sim: NonSend<Sim>,
    row: Res<Row>,
    idols: Res<Idols>,
    travel: Res<idol_travel::TravelMenu>,
    mut shown_for: Local<Option<(usize, usize)>>,
    mut parts: Query<
        (&mut Visibility, Has<Guide>, Option<&Cursor>),
        Or<(With<Guide>, With<Menu>, With<Cursor>)>,
    >,
    mut labels: Query<(&Label, &mut TextColor)>,
    mut texts: Query<(&mut Text, Option<&HelpLine>), Or<(With<Place>, With<HelpLine>)>>,
) {
    let near = sim.duel.idol_in_reach().is_some();
    let open = menu_open(&sim) && !travel.open;
    for (mut v, guide, cursor) in &mut parts {
        let on = match (guide, cursor) {
            (true, _) => near,
            (_, Some(c)) => c.0 == row.0,
            _ => open,
        };
        v.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    // The texts change only with the idol and the row.
    let now = sim.duel.idol().filter(|_| open).map(|i| (i, row.0));
    if now.is_none() || *shown_for == now {
        return;
    }
    *shown_for = now;
    let (idol, row) = now.unwrap();
    for (l, mut color) in &mut labels {
        color.0 = match (ROWS[l.0].1, l.0 == row) {
            (false, _) => GREYED,
            (true, true) => Color::WHITE,
            (true, false) => TEXT,
        };
    }
    for (mut t, help) in &mut texts {
        t.0 = match help {
            Some(lines) => lines.0[row].clone(),
            None => idols.names.get(idol).cloned().unwrap_or_default(),
        };
    }
}

/// The guide's label for the idol in reach: "Commune with Sculptor's Idol" until it is lit,
/// "Rest" after, as the game's kneel prompt. Logged when it changes, for live checks.
fn guide_label(
    sim: NonSend<Sim>,
    mut shown: Local<Option<(usize, bool)>>,
    mut labels: Query<(&GuideLabel, &mut Text)>,
) {
    let Some(i) = sim.duel.idol_in_reach() else {
        return;
    };
    let lit = sim.duel.idol_lit(i);
    if *shown == Some((i, lit)) {
        return;
    }
    *shown = Some((i, lit));
    let entity = sim.duel.idol_spots()[i].entity;
    for (label, mut t) in &mut labels {
        let text = if lit { &label.rest } else { &label.commune };
        t.0 = crate::game_ui::action_guide_label(text);
        info!("idol guide: idol {entity} lit {lit}: {text}");
    }
}

/// Frees the mouse while the idol menu (or its Travel screen) is up, for pointing at its rows,
/// and takes it back for the camera when the menu goes, as the pause menu does. Play does not
/// take it back on a click meanwhile (`gather_input`, while [`crate::menu::Modal`] holds).
fn free_cursor(
    sim: NonSend<Sim>,
    mut was_open: Local<bool>,
    mut cursors: Query<&mut CursorOptions>,
) {
    let open = menu_open(&sim);
    if open == *was_open {
        return;
    }
    *was_open = open;
    for mut c in &mut cursors {
        c.visible = open;
        c.grab_mode = if open {
            CursorGrabMode::None
        } else {
            CursorGrabMode::Locked
        };
    }
}

/// The pause menu stays shut while Wolf is at an idol, since Esc stands him up.
fn hold_pause(sim: NonSend<Sim>, mut modal: ResMut<crate::menu::Modal>) {
    let at_idol = sim.duel.idol_phase().is_some();
    if modal.0 != at_idol {
        modal.0 = at_idol;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_on_a_pointable_part_picks_it_and_hovers_need_a_moved_cursor() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::ui_widgets::ButtonPlugin))
            .init_resource::<Pointed<usize>>();
        let world = app.world_mut();
        let rows: Vec<Entity> = (0..3usize)
            .map(|i| world.spawn(pointable(i)).id())
            .collect();
        world.flush();
        crate::menu::test_click(world, rows[2], rows[2]);
        app.update();
        let mut pointed = app.world_mut().resource_mut::<Pointed<usize>>();
        assert_eq!(pointed.clicked, [2], "one click picks its row once");
        pointed.hovered.push(1);
        assert_eq!(pointed.take(false), (Vec::new(), vec![2]));
        pointed.hovered.push(1);
        assert_eq!(pointed.take(true), (vec![1], Vec::new()));
    }

    #[test]
    fn the_guide_reads_commune_at_an_unlit_idol_and_rest_once_it_is_lit() {
        let cache = sekiro_sim::cache::root();
        if !cache.join("anim/c0000/a000_710310.bin").exists() {
            eprintln!("skipping: cache not extracted");
            return;
        }
        let events = crate::game_ui::menu_texts(&cache, "イベントテキスト");
        assert_eq!(events[&COMMUNE], "Commune with Sculptor's Idol");
        assert_eq!(events[&REST], "Rest");
        let mut duel = sekiro_sim::duel::Duel::load(&cache, 30.0).unwrap();
        duel.enemy_present = false;
        duel.enemy_active = false;
        for _ in 0..60 {
            duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
        }
        let here = Placement {
            position: duel.player.body.position,
            yaw: 0.0,
        };
        let entity = 1101950;
        duel.set_idols(vec![IdolSpot {
            kneel: here,
            checkpoint: here,
            entity,
        }]);
        assert_eq!(duel.idol_in_reach(), Some(0));

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_non_send(Sim::for_test(duel))
            .add_systems(Update, guide_label);
        let label = app
            .world_mut()
            .spawn((
                GuideLabel {
                    commune: events[&COMMUNE].clone(),
                    rest: events[&REST].clone(),
                },
                Text::default(),
            ))
            .id();
        let shown = |app: &App| app.world().get::<Text>(label).unwrap().0.clone();
        app.update();
        assert_eq!(shown(&app), " : Commune with Sculptor's Idol");
        // Once lit (a first kneel, `Duel::idol_lit`), the same idol's guide reads Rest.
        app.world_mut()
            .non_send_mut::<Sim>()
            .duel
            .kit
            .found_idols
            .insert(entity);
        app.update();
        assert_eq!(shown(&app), " : Rest");
    }

    #[test]
    fn the_idols_of_a_block_are_found_named_and_faced() {
        let cache = sekiro_sim::cache::root();
        let Ok(layout) = std::fs::read(cache.join("maps/m11_00_00_00/layout.json")) else {
            eprintln!("skipping: cache not extracted");
            return;
        };
        let layouts = BTreeMap::from([(
            "m11_00_00_00".to_string(),
            serde_json::from_slice(&layout).unwrap(),
        )]);
        let idols = Idols::load(&cache, &layouts, |_, _, y| Some(y));
        assert_eq!(idols.spots.len(), 8, "{:?}", idols.names);
        assert!(idols.names.iter().any(|n| n == "Ashina Castle Gate"));
        assert!(
            idols.names.iter().all(|n| !n.is_empty()),
            "{:?}",
            idols.names
        );
        // Each is its own idol, and Wolf arrives at its warp point: the c0000 player part of
        // entity `idol - 980`, a few metres off the idol.
        let entities: std::collections::BTreeSet<_> =
            idols.spots.iter().map(|s| s.entity).collect();
        assert_eq!(entities.len(), idols.spots.len());
        for s in &idols.spots {
            let off = Vec3::from_array(s.checkpoint.position) - Vec3::from_array(s.kneel.position);
            assert!(off.length() < 8.0, "{s:?}");
        }
        let gate = &idols.spots[idols.index(1101951).expect("idol 1101951")];
        let [x, y, z] = gate.checkpoint.position;
        assert!(
            (x + 161.22).abs() < 0.05 && (y + 28.61).abs() < 0.05 && (z - 23.0).abs() < 0.05,
            "{gate:?}"
        );
        assert!(
            (gate.checkpoint.yaw.to_degrees() - 99.7).abs() < 0.1,
            "{gate:?}"
        );
    }

    #[test]
    fn an_idol_without_a_warp_point_still_stands_with_a_spot_of_its_own() {
        let rows = [
            serde_json::json!({"bonfireEntityId": 1101950, "msgId": 7}),
            serde_json::json!({"bonfireEntityId": 1101951, "msgId": 8}),
        ];
        let part = |entity: i32, x: f32| {
            serde_json::json!({
                "entity_id": entity,
                "translation": [x, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "yaw_degrees": 90.0,
            })
        };
        let layouts = BTreeMap::from([(
            "m00".to_string(),
            serde_json::json!({
                "objects": [part(1101950, 0.0), part(1101951, 10.0)],
                "players": [part(1100970, 3.0)],
            }),
        )]);
        let idols = Idols::place(&warps(&rows), &BTreeMap::new(), &layouts, |_, _, y| Some(y));
        assert_eq!(idols.spots.len(), 2);
        let warped = idols.spots[0].checkpoint;
        assert_eq!(warped.position, [3.0, 0.0, 0.0]);
        assert!((warped.yaw - 90f32.to_radians()).abs() < 1e-6);
        // No part 1100971: he stands out from the second idol, facing away from it.
        let spot = &idols.spots[1];
        let out =
            Vec3::from_array(spot.checkpoint.position) - Vec3::from_array(spot.kneel.position);
        assert!((out.length() - RESPAWN).abs() < 1e-3, "{spot:?}");
    }
}
