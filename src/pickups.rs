//! Items lying in the world and the pick-up dialog (split from items.rs, whose item rules are
//! the combat core's): the map's pickups, taking one near Wolf's feet, enemy drops landing as
//! pickups, the "got" line and the key-item dialog.

use bevy::prelude::*;

use crate::items::*;

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Picked>()
            .init_resource::<DialogUp>()
            .add_systems(Startup, spawn_pickups)
            .add_systems(Update, collect)
            .add_systems(PreUpdate, close_dialog.before(crate::menu::MenuInput));
    }
}

/// An item lying in the world: what and how many, where (feet height), and whether it is gone.
#[derive(Clone, Debug)]
pub(crate) struct Pickup {
    pub(crate) goods: i64,
    pub(crate) count: u32,
    pub(crate) pos: Vec3,
    pub(crate) taken: bool,
}

/// The map's pickups (the parkour course's, `parkour_pickups`).
pub(crate) static PICKUPS: std::sync::RwLock<Vec<Pickup>> = std::sync::RwLock::new(Vec::new());

/// How near (m, level) and how far up or down Wolf's feet must be to take one.
const PICK_REACH: f32 = 1.4;
const PICK_REACH_Y: f32 = 0.8;

/// Items about the parkour course: beside its idols and along the way.
pub(crate) fn parkour_pickups() -> Vec<Pickup> {
    let at = |goods: i64, count: u32, x: f32, y: f32, z: f32| Pickup { goods, count, pos: Vec3::new(x, y, z), taken: false };
    vec![
        // Ceramic Shard and a Pellet near the start, Scrap Iron by Platform Top, Ako's Sugar by
        // Cliff Top, Black Gunpowder and a Gokan's Sugar by Shaft Floor.
        at(3500, 3, -1.0, 0.0, 4.0),
        at(3020, 1, 2.5, 0.0, -1.5),
        at(6000, 3, 10.0, 3.0, -11.8),
        at(3400, 1, -11.6, 4.5, -21.4),
        at(6100, 2, 14.2, 0.0, -21.8),
        at(3430, 1, 16.6, 0.0, -21.8),
        // The Mibu Breathing Technique beside the pool (diving, swim.rs).
        at(crate::swim::MIBU_BREATHING, 1, -18.5, 0.0, 4.0),
        // Gachiin's Sugar at the outpost's gate, two Ako's Sugars on top of its watchtower.
        at(3440, 1, -3.0, 0.0, 10.0),
        at(3400, 2, -21.0, 8.0, 41.0),
        // Esoteric Texts: the start, Platform Top, Cliff Top, Shaft Floor and the outpost's
        // watchtower. These test-map placements replace the story's NPC gifts and temple loot.
        at(crate::skill_tree::TEXT_GOODS[0], 1, -3.0, 0.0, -1.5),
        at(crate::skill_tree::TEXT_GOODS[1], 1, 10.4, 3.0, -13.4),
        at(crate::skill_tree::TEXT_GOODS[2], 1, -13.1, 4.5, -22.5),
        at(crate::skill_tree::TEXT_GOODS[3], 1, 16.0, 0.0, -22.7),
        at(crate::skill_tree::TEXT_GOODS[4], 1, -23.0, 8.0, 41.0),
    ]
}

/// An enemy's drop (drops.rs): a pickup at `pos` (feet height) with its glint.
pub(crate) fn drop_pickup(commands: &mut Commands, goods: i64, count: u32, pos: Vec3) {
    let Ok(mut list) = PICKUPS.write() else { return };
    let i = list.len();
    list.push(Pickup { goods, count, pos, taken: false });
    let mut fx = crate::fxr::FxEffect::new(GLINT_SFX, None, false, Transform::from_translation(pos + Vec3::Y * 0.1));
    fx.keep = true;
    commands.spawn((fx, PickupGlint(i)));
}

/// The untaken pickup in reach of `feet`.
pub(crate) fn pickup_near(feet: Vec3) -> Option<usize> {
    let list = PICKUPS.read().ok()?;
    list.iter().position(|p| !p.taken && (p.pos - feet).with_y(0.0).length() < PICK_REACH && (p.pos.y - feet.y).abs() < PICK_REACH_Y)
}

/// Takes the pickup in reach (player.rs, on E): gone from the world, queued for the bag.
pub(crate) fn take_near(feet: Vec3, picked: &mut Picked) -> bool {
    let Some(i) = pickup_near(feet) else { return false };
    let Ok(mut list) = PICKUPS.write() else { return false };
    list[i].taken = true;
    picked.0.push((i, list[i].goods, list[i].count));
    true
}

/// Taken by Wolf's interact (the core's `WorldHooks::take_item`), shown by `collect`.
static TAKEN: std::sync::Mutex<Picked> = std::sync::Mutex::new(Picked(Vec::new()));

/// The core's item hook: takes the item at these feet into `TAKEN`.
pub(crate) fn take_item(feet: Vec3) -> bool {
    TAKEN.lock().is_ok_and(|mut t| take_near(feet, &mut t))
}

/// Pickups taken this frame: (index, goods, count).
#[derive(Resource, Default)]
pub(crate) struct Picked(pub(crate) Vec<(usize, i64, u32)>);

/// A pickup's glint (FFX 61: the item glow common_func 20006030 puts on an object's item).
#[derive(Component)]
struct PickupGlint(usize);

/// The glint of an untaken pickup.
const GLINT_SFX: i64 = 61;

fn spawn_pickups(mut commands: Commands, config: Res<crate::config::GameConfig>, mut picked: ResMut<Picked>) {
    // SHINOBI_ITEM_DIALOG=<goods id>: as if that goods were just picked up (SHINOBI_SHOT captures).
    if let Some(id) = std::env::var("SHINOBI_ITEM_DIALOG").ok().and_then(|v| v.parse::<i64>().ok()) {
        picked.0.push((usize::MAX, id, 1));
    }
    #[cfg(feature = "host")]
    let list = if crate::map::map_id(&config).as_deref() == Some(crate::sandbox::CONSTRUCT_ID) { parkour_pickups() } else { Vec::new() };
    #[cfg(not(feature = "host"))]
    let list: Vec<Pickup> = { let _ = &config; Vec::new() };
    for (i, p) in list.iter().enumerate() {
        let mut fx = crate::fxr::FxEffect::new(GLINT_SFX, None, false, Transform::from_translation(p.pos + Vec3::Y * 0.1));
        fx.keep = true;
        commands.spawn((fx, PickupGlint(i)));
    }
    if let Ok(mut v) = PICKUPS.write() {
        *v = list;
    }
}

/// The item dialog (the user's captures): a goods got for the first time shows its name, icon
/// and text on the parchment (key items between the purple bands) until OK; the game waits.
/// A goods already held shows the short line at the bottom instead.
#[derive(Component)]
struct ItemDialog;

/// The short "got" line and when it went up.
#[derive(Component)]
struct GotLine(f32);

/// How long the short line stays (s).
const LINE_SECS: f32 = 2.5;

/// The goods shown in the dialog now, if any.
#[derive(Resource, Default)]
pub(crate) struct DialogUp(pub(crate) Option<i64>);

/// Puts the taken items in the bag, puts out their glint and shows what was got.
#[allow(clippy::too_many_arguments)]
fn collect(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut virt: ResMut<Time<Virtual>>,
    mut picked: ResMut<Picked>,
    mut up: ResMut<DialogUp>,
    mut config: ResMut<crate::config::GameConfig>,
    glints: Query<(Entity, &PickupGlint)>,
    lines: Query<(Entity, &GotLine)>,
    assets: Res<AssetServer>,
    font: Option<Res<crate::fonts::GameFonts>>,
    texts: Option<Res<crate::game_menu::ItemTexts>>,
    mut wolf: Query<&mut crate::player::Player>,
) {
    let now = time.elapsed_secs();
    for (e, l) in &lines {
        if now - l.0 > LINE_SECS {
            commands.entity(e).despawn();
        }
    }
    let font = font.as_ref().and_then(|f| f.ui.clone());
    if let Ok(mut t) = TAKEN.lock() {
        picked.0.append(&mut t.0);
    }
    for (i, goods, count) in std::mem::take(&mut picked.0) {
        // Spirit Emblems: onto Wolf's count (up to the carried maximum), the short line only.
        let emblem = goods == SPIRIT_EMBLEM;
        let first = !emblem && count_held(&config, goods) == 0;
        let got = if emblem {
            let max = config.player.spirit_emblems;
            wolf.iter_mut().next().map_or(0, |mut p| {
                let before = p.emblems;
                p.emblems = (p.emblems + count).min(max.max(p.emblems));
                p.emblems - before
            })
        } else {
            add(&mut config, goods, count)
        };
        info!("picked up goods {goods} x{count} ({got} kept)");
        for (e, g) in &glints {
            if g.0 == i {
                commands.entity(e).despawn();
            }
        }
        let name = texts.as_ref().and_then(|t| t.goods_name.get(&(goods as u32)).cloned()).unwrap_or_else(|| goods.to_string());
        let info = texts.as_ref().and_then(|t| t.goods_info.get(&(goods as u32)).cloned()).unwrap_or_default();
        let icon = crate::hud::any_icon(param("EquipParamGoods", goods)["iconId"].as_i64().unwrap_or(-1));
        if first && up.0.is_none() {
            let key_item = param("EquipParamGoods", goods)["goodsType"].as_i64() == Some(1);
            spawn_dialog(&mut commands, &assets, &font, &name, &info, icon, key_item);
            up.0 = Some(goods);
            virt.pause();
        } else {
            for (e, _) in &lines {
                commands.entity(e).despawn();
            }
            spawn_line(&mut commands, &assets, &font, &name, icon, count, now);
        }
    }
}

fn count_held(config: &crate::config::GameConfig, id: i64) -> u32 {
    count(config, id)
}

/// The dialog, laid out after the user's 2000 x 1125 capture (in % of the window).
fn spawn_dialog(commands: &mut Commands, assets: &AssetServer, font: &Option<Handle<Font>>, name: &str, info: &str, icon: Option<String>, key_item: bool) {
    use crate::game_menu::{abs, fill_row, image, sprite, text, TEXT};
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(120), ItemDialog))
        .with_children(|root| {
            if key_item {
                root.spawn((abs(26.5, 19.0, 47.1, 6.2), image(assets, sprite("sb_map/MENU_Dialog_valuable"))));
                root.spawn((abs(26.5, 75.6, 47.1, 6.2), image(assets, sprite("sb_map/MENU_Dialog_valuable"))));
            }
            root.spawn((abs(26.2, 24.0, 47.7, 52.6), image(assets, sprite("sb_dialog/MENU_Dialog_base_02"))));
            root.spawn((abs(27.4, 26.2, 45.2, 4.6), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.22, 0.22, 0.22)))).with_children(|b| {
                b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                    c.spawn(text(name, 21.0, Color::WHITE, font));
                });
            });
            root.spawn((abs(29.9, 34.0, 11.8, 20.4), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|b| {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(8.0), top: Val::Percent(8.0), width: Val::Percent(84.0), height: Val::Percent(84.0), ..default() }, image(assets, icon)));
            });
            // The text over the parchment down to the rule; a long one is cut there.
            root.spawn(Node { overflow: Overflow::clip(), ..abs(43.8, 33.2, 28.5, 33.6) }).with_children(|c| {
                c.spawn(text(info, 15.0, TEXT, font));
            });
            root.spawn((abs(28.0, 67.3, 41.0, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3))));
            root.spawn((crate::menu::ui_button(), abs(41.7, 68.4, 16.8, 6.7), image(assets, sprite("sb_in_game/MENU_Cursor_03")), DialogOk)).with_children(|b| {
                b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                    c.spawn(text("OK", 17.0, Color::WHITE, font));
                });
            });
        });
}

/// The dialog's OK.
#[derive(Component)]
struct DialogOk;

/// The short line for a goods already held: icon, name and count at the bottom.
fn spawn_line(commands: &mut Commands, assets: &AssetServer, font: &Option<Handle<Font>>, name: &str, icon: Option<String>, count: u32, now: f32) {
    use crate::game_menu::{abs, fill_row, image, sprite, text, TEXT};
    commands.spawn((abs(36.0, 72.0, 28.0, 7.0), image(assets, sprite("sb_in_game/MENU_ItemBase_07")), GlobalZIndex(90), GotLine(now))).with_children(|b| {
        b.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(14.0), padding: UiRect::left(Val::Percent(7.0)), ..fill_row() }).with_children(|r| {
            r.spawn((Node { width: Val::Px(40.0), height: Val::Px(40.0), ..default() }, image(assets, icon)));
            r.spawn(text(name, 18.0, Color::WHITE, font));
            if count > 1 {
                r.spawn(text(&format!("x {count}"), 17.0, TEXT, font));
            }
        });
    });
}

/// OK (Enter, E, Space, Esc or a click) closes the dialog and the game goes on. Its keys are
/// spent here, before the menus and Wolf read them.
fn close_dialog(
    mut commands: Commands,
    mut up: ResMut<DialogUp>,
    mut virt: ResMut<Time<Virtual>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    ok: Query<&Interaction, (With<DialogOk>, Changed<Interaction>)>,
    dialogs: Query<Entity, With<ItemDialog>>,
) {
    if up.0.is_none() {
        return;
    }
    let pressed = [KeyCode::Enter, KeyCode::KeyE, KeyCode::Space, KeyCode::Escape].iter().any(|k| keys.just_pressed(*k)) || mouse.just_pressed(MouseButton::Left) || ok.iter().any(|i| *i == Interaction::Pressed);
    keys.reset_all();
    mouse.reset_all();
    if pressed {
        for e in &dialogs {
            commands.entity(e).despawn();
        }
        up.0 = None;
        virt.unpause();
    }
}

