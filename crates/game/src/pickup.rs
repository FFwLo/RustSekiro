//! Map items (#136): each item lying on the play blocks glints with the game's common item
//! effect (FFX 61, common_func 20006030) until it is taken; in reach, the "Pick Up Item" guide
//! (ActionButtonParam 4000 textId 10010103 in イベントテキスト) shows with its key, and the
//! action button (E, or Y on a pad) takes it (`Duel::action_target`). A short line then names
//! what was got and how many for 2.5 s, and the item's marker part (o000100) is hidden.
//!
//! After FwLo's RustSekiro `src/items.rs` (`spawn_line`, the glint) and `src/action_guide.rs`
//! (Fireguy32's pickups); the guide is the idol's (`crate::game_ui::spawn_action_guide`).
//! gap: the first-time item dialog and the line's item icon.
use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use sekiro_sim::items::{ItemData, MapPickup};

use super::Sim;
use crate::game_ui::{GameFonts, MenuArt, abs};
use crate::map::visibility::TakenVisibility;

/// The item glint (`sfxbnd_commoneffects` f000000061).
const GLINT: i32 = 61;
/// Glints are held only this near Wolf (m).
const GLINT_RANGE: f32 = 60.0;
/// イベントテキスト: "Pick Up Item".
const PICK_UP: i32 = 10010103;
/// How long the "got" line stays (s).
const LINE_SECS: f32 = 2.5;
const COUNT_TEXT: Color = Color::srgb(0.88, 0.85, 0.78);

/// The item params, read once for the sim's pickups and Storage and the game menu's carry
/// limits; empty (logged here, once) when they are missing.
pub(super) fn item_data(cache: &std::path::Path) -> Arc<ItemData> {
    match ItemData::load(cache) {
        Ok(data) => Arc::new(data),
        Err(e) => {
            warn!(
                "item params: {e}; no map items, and goods are carried up to 99 but never stored or restocked"
            );
            Arc::default()
        }
    }
}

/// The items lying on the play `blocks`; empty when the params or the blocks' MSBs are missing.
pub(super) fn load(
    cache: &std::path::Path,
    blocks: &BTreeMap<String, serde_json::Value>,
    data: &ItemData,
) -> Vec<MapPickup> {
    if data.lots.is_empty() {
        return Vec::new();
    }
    let mut pickups = Vec::new();
    for block in blocks.keys() {
        match sekiro_sim::items::map_pickups(cache, block, data) {
            Ok(found) => pickups.extend(found),
            Err(e) => warn!("map items: {e}"),
        }
    }
    eprintln!("map items: {} on {} blocks", pickups.len(), blocks.len());
    pickups
}

#[derive(Component)]
struct Guide;
#[derive(Component)]
struct Line;
#[derive(Component)]
struct LineName;
#[derive(Component)]
struct LineCount;

/// Item names (アイテム名), the pickups already shown in the line and how long it stays up.
#[derive(Resource, Default)]
struct Book {
    names: BTreeMap<i32, String>,
    shown: u32,
    left: f32,
}

/// The glints, the guide, the line and the taken parts; nothing when the play world has no
/// items.
pub(super) fn install(app: &mut App, any: bool) {
    if !any {
        return;
    }
    GameFonts::insert(app);
    MenuArt::insert(app);
    let cache = sekiro_sim::cache::root();
    app.insert_resource(Book {
        names: crate::game_ui::texts(&cache, "item", "アイテム名"),
        ..default()
    })
    .add_systems(Startup, spawn)
    .add_systems(
        Update,
        (glint, show, hide_taken)
            .after(super::simulate)
            .run_if(crate::menu::active),
    );
}

fn spawn(mut commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>) {
    let cache = sekiro_sim::cache::root();
    let events = crate::game_ui::menu_texts(&cache, "イベントテキスト");
    let label = events.get(&PICK_UP).cloned().unwrap_or_default();
    // The guide, low in the middle like the idol's: "[E] : Pick Up Item".
    crate::game_ui::spawn_action_guide(&mut commands, &fonts, &art, &label, Guide);
    // The "got" line above the guide: the item's name and count on the game's item plate.
    let text = |t: String, px: f32, color: Color| fonts.shadowed(t, px, color);
    let plate = art
        .sprite("SB_In_Game", "MENU_ItemBase_07")
        .map(|(image, _)| ImageNode {
            image_mode: bevy::ui::widget::NodeImageMode::Stretch,
            ..image
        });
    let mut line = commands.spawn((
        Line,
        abs(36.0, 72.0, 28.0, 7.0),
        GlobalZIndex(90),
        Visibility::Hidden,
    ));
    match plate {
        Some(plate) => line.insert(plate),
        None => line.insert(BackgroundColor(Color::BLACK.with_alpha(0.6))),
    };
    line.with_children(|b| {
        b.spawn(Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            column_gap: px(14),
            padding: UiRect::left(percent(7.0)),
            ..default()
        })
        .with_children(|r| {
            r.spawn((LineName, text(String::new(), 18.0, Color::WHITE)));
            r.spawn((LineCount, text(String::new(), 17.0, COUNT_TEXT)));
        });
    });
}

/// A held glint on each item still lying within [`GLINT_RANGE`] of Wolf, let go once it is
/// taken or out of range.
fn glint(sim: NonSend<Sim>, mut fx: ResMut<crate::fxr::FxrPlayer>, mut held: Local<Vec<bool>>) {
    let wolf = Vec3::from_array(sim.duel.player.body.position);
    let pickups = sim.duel.pickups();
    held.resize(pickups.len(), false);
    for (i, p) in pickups.iter().enumerate() {
        let at = Vec3::from_array(p.position);
        let on = sim.duel.pickup_lies(i) && at.distance_squared(wolf) < GLINT_RANGE * GLINT_RANGE;
        if on || held[i] {
            let at = Transform::from_translation(at);
            fx.hold(crate::fxr::GLINTS, GLINT, i as i32, at, on);
        }
        held[i] = on;
    }
}

/// The guide in reach of an item, and the line after a pickup.
#[allow(clippy::type_complexity)]
fn show(
    sim: NonSend<Sim>,
    time: Res<Time<Real>>,
    mut book: ResMut<Book>,
    mut parts: Query<(&mut Visibility, Has<Guide>), Or<(With<Guide>, With<Line>)>>,
    mut texts: Query<(&mut Text, Has<LineName>), Or<(With<LineName>, With<LineCount>)>>,
) {
    let (taken, got) = sim.duel.last_pickup();
    let mut new = false;
    if taken != book.shown {
        book.shown = taken;
        if let Some((id, n)) = got {
            let name = book
                .names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| id.to_string());
            for (mut t, is_name) in &mut texts {
                t.0 = if is_name {
                    name.clone()
                } else if n > 1 {
                    format!("x {n}")
                } else {
                    String::new()
                };
            }
            new = true;
        }
    }
    let line_up = crate::combat_hud::step_hold(&mut book.left, new, LINE_SECS, time.delta_secs());
    let near = sim.duel.pickup_in_reach().is_some();
    for (mut v, guide) in &mut parts {
        let on = if guide { near } else { line_up };
        v.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Hides the marker parts of the items taken, whenever one more is taken (and at the start, for
/// those the kit already holds).
fn hide_taken(
    sim: NonSend<Sim>,
    mut synced: Local<Option<u32>>,
    taken: Option<ResMut<TakenVisibility>>,
) {
    let count = sim.duel.last_pickup().0;
    let Some(mut taken) = taken else {
        return;
    };
    if *synced == Some(count) {
        return;
    }
    *synced = Some(count);
    taken.maps.clear();
    for (i, p) in sim.duel.pickups().iter().enumerate() {
        if !sim.duel.pickup_lies(i) {
            let state = taken.maps.entry(p.block.clone()).or_default();
            state.by_name.insert(p.part.clone(), false);
        }
    }
}
