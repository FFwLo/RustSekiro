//! HUD: Sekiro-style posture bars (fill from the centre outward), HP bars,
//! a combat log, and a debug panel showing the live TAE state so timing can
//! be checked frame by frame against the real game.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::actor::{Actor, Side, data_for};
use crate::data::Combat;
use crate::player::{
    FLAG_ACCEPT_ATTACK, FLAG_ACCEPT_GUARD, FLAG_ACCEPT_JUMP, FLAG_ACCEPT_MOVE, FLAG_ACCEPT_STEP, REF_GUARD_COMBO,
    REF_JUST_GUARD,
};

#[derive(Resource, Default)]
pub struct CombatLog {
    pub lines: Vec<(String, Color, f32)>,
}

impl CombatLog {
    pub fn push(&mut self, text: impl Into<String>, color: Color) {
        self.lines.push((text.into(), color, 0.0));
        if self.lines.len() > 6 {
            self.lines.remove(0);
        }
    }
}

/// Perilous-attack warning (the red 危 kanji), raised by the enemy when its TAE
/// fires the warning BulletBehavior (judge 980 sweep, 982 thrust, 983 grab).
#[derive(Resource, Default)]
pub struct Perilous {
    pub timer: f32,
    pub kind: i64,
}

impl Perilous {
    pub fn raise(&mut self, kind: i64) {
        self.timer = PERILOUS_SHOW;
        self.kind = kind;
    }
}

const PERILOUS_SHOW: f32 = 0.8;

#[derive(Component)]
struct PerilousText;

/// Wolf's vitality fill, its damage trail, posture bar, resurrection nodes, item counts; the
/// enemy's over-head bars.
#[derive(Component)]
struct HpFill;
#[derive(Component)]
struct HpTrail;
#[derive(Component)]
struct PostureRoot(Side);
#[derive(Component)]
struct PostureFill(Side);
#[derive(Component)]
struct ResNode(u32);
/// The row the resurrection nodes sit in; one node per config player.resurrections.
#[derive(Component)]
struct ResRow;
#[derive(Component)]
struct ItemText;
/// One enemy's bars over its head (one per enemy, spawned when it appears).
#[derive(Component)]
struct EnemyRoot(Entity);
/// Bottom-right icon slots: the equipped prosthetic (`true`) and combat art (`false`), the game's
/// menu icons (EquipParamWeapon iconId -> SB_Icon* atlas MENU_ItemIcon_<iconId>, cut to
/// extracted/hud by sekiro-extract export_hud). Hidden while the icon is not extracted.
#[derive(Component)]
struct ItemIcon(bool);
#[derive(Component)]
struct EnemyHpFill(Entity);
#[derive(Component)]
struct EnemyPostureFill(Entity);

#[derive(Component)]
struct DebugText;

#[derive(Component)]
struct LogText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatLog>()
            .init_resource::<Perilous>()
            .add_systems(Startup, (spawn_hud, spawn_perilous, spawn_deathblow_mark, spawn_lock_mark, spawn_item_icons, spawn_wire_mark))
            .add_systems(Update, (sync_res_nodes, update_player_hud, update_enemy_hud, update_debug, update_log, debug_only, update_perilous, update_stealth, update_deathblow_mark, update_lock_mark, update_item_icons, update_wolf_build_up, hud_shots, update_wire_mark));
    }
}

/// `SHINOBI_HUD_SHOTS=<dir>[,<every s>[,<count>]]`: saves the game's own frame (HUD included)
/// every few seconds (default 2 s, 8 shots), then quits: a check of HUD art without a screen grab.
/// It also locks on to the nearest enemy, so the lock mark shows.
#[allow(clippy::too_many_arguments)]
fn hud_shots(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut taken: Local<u32>,
    mut exit: MessageWriter<AppExit>,
    lock: Option<ResMut<crate::camera::LockOn>>,
    wolf: Query<&GlobalTransform, With<crate::player::Player>>,
    enemies: Query<(Entity, &GlobalTransform), With<crate::enemy::Enemy>>,
) {
    let Ok(v) = std::env::var("SHINOBI_HUD_SHOTS") else { return };
    if let (Some(mut lock), Ok(w)) = (lock, wolf.single()) {
        if lock.target.is_none() {
            lock.target = enemies.iter().min_by(|a, b| a.1.translation().distance(w.translation()).total_cmp(&b.1.translation().distance(w.translation()))).map(|(e, _)| e);
        }
    }
    let mut it = v.split(',');
    let dir = it.next().unwrap_or(".").to_string();
    let every = it.next().and_then(|s| s.parse::<f32>().ok()).unwrap_or(2.0);
    let count = it.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(8);
    if *taken >= count {
        if time.elapsed_secs() > every * (count as f32 + 1.5) {
            exit.write(AppExit::Success);
        }
        return;
    }
    if time.elapsed_secs() >= every * (*taken as f32 + 1.0) {
        *taken += 1;
        let _ = std::fs::create_dir_all(&dir);
        let path = format!("{dir}/hud_{:02}.png", *taken);
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    }
}

/// Frame colours: dark lacquer with a thin gold edge, like the game's bars.
const FRAME: Color = Color::srgba(0.05, 0.04, 0.03, 0.75);
const EDGE: Color = Color::srgba(0.78, 0.66, 0.42, 0.55);
const VITALITY: Color = Color::srgb(0.66, 0.08, 0.06);
const TRAIL: Color = Color::srgba(0.95, 0.75, 0.65, 0.8);

/// A framed bar: the fill (and an optional damage trail behind it) inside a thin gold frame.
fn framed(parent: &mut ChildSpawnerCommands, width: f32, height: f32, center: bool, fill: impl Bundle, trail: Option<impl Bundle>) {
    parent
        .spawn((
            Node {
                width: Val::Px(width),
                height: Val::Px(height),
                border: UiRect::all(Val::Px(1.0)),
                justify_content: if center { JustifyContent::Center } else { JustifyContent::FlexStart },
                ..default()
            },
            BackgroundColor(FRAME),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            if let Some(t) = trail {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(TRAIL), t));
            }
            b.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, fill));
        });
}

fn spawn_hud(mut commands: Commands, mut fonts: ResMut<Assets<Font>>, assets: Res<AssetServer>) {
    let serif = std::fs::read("C:/Windows/Fonts/georgia.ttf").ok().map(|b| fonts.add(Font::from_bytes(b)));
    // Wolf, bottom left: resurrection nodes over the vitality bar, item counts under it.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(48.0),
            bottom: Val::Px(44.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(8.0), ..default() }).with_children(|r| {
                for (ty, file) in [(0, "hud/MENU_Resist_Poison.png"), (1, "hud/MENU_Resist_Fire.png")] {
                    spawn_build_up(r, &assets, ty, file);
                }
            });
            c.spawn((Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() }, ResRow));
            framed(c, 320.0, 10.0, false, (BackgroundColor(VITALITY), HpFill), Some(HpTrail));
            c.spawn((
                Text::new(""),
                TextFont { font: serif.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(15.0), ..default() },
                TextColor(Color::srgba(0.92, 0.88, 0.8, 0.9)),
                TextShadow { offset: Vec2::new(1.0, 1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.85) },
                ItemText,
            ));
        });
    // Wolf's posture, bottom centre: fills from the middle outward, shown while it has damage.
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, bottom: Val::Px(92.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
            Visibility::Hidden,
            PostureRoot(Side::Player),
        ))
        .with_children(|c| framed(c, 380.0, 9.0, true, (BackgroundColor(Color::srgb(1.0, 0.7, 0.15)), PostureFill(Side::Player)), None::<()>));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() },
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(10.0), ..default() },
        Visibility::Hidden,
        DebugText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(16.0), ..default() },
        Node { position_type: PositionType::Absolute, bottom: Val::Px(140.0), left: Val::Px(10.0), ..default() },
        Visibility::Hidden,
        LogText,
    ));
}

/// Posture bar colour by fill: exe FUN_140a02a10 shows "danger" when <= 30 % of posture is left
/// (DAT_143b06724 = 30), i.e. the bar 70 % full: red; else yellow -> orange.
fn res_node_bundle(i: u32) -> impl Bundle {
    (
        Node { width: Val::Px(13.0), height: Val::Px(13.0), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(VITALITY),
        BorderColor::all(EDGE),
        ResNode(i),
    )
}

/// Node i of `max` with `left` resurrections: None = past the max (hidden), Some(lit).
fn res_node_look(i: u32, max: u32, left: u32) -> Option<bool> {
    (i < max).then_some(i < left)
}

/// Keeps as many resurrection nodes as config player.resurrections (F5 reload can raise it).
fn sync_res_nodes(mut commands: Commands, config: Res<crate::config::GameConfig>, row: Query<Entity, With<ResRow>>, nodes: Query<&ResNode>) {
    let Some(row) = row.iter().next() else { return };
    let have = nodes.iter().count() as u32;
    for i in have..config.player.resurrections {
        commands.entity(row).with_child(res_node_bundle(i));
    }
}

fn posture_color(f: f32) -> Color {
    if f >= 0.7 { Color::srgb(0.95, 0.12, 0.05) } else { Color::srgb(1.0, 0.85 - 0.5 * f, 0.15) }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_player_hud(
    time: Res<Time>,
    mut trail: Local<(f32, f32)>,
    player: Query<(&Actor, &crate::player::Player)>,
    mut hp: Query<&mut Node, (With<HpFill>, Without<HpTrail>, Without<PostureFill>)>,
    mut tr: Query<&mut Node, (With<HpTrail>, Without<HpFill>, Without<PostureFill>)>,
    mut posture: Query<(&PostureFill, &mut Node, &mut BackgroundColor), (Without<HpFill>, Without<HpTrail>, Without<ResNode>)>,
    mut root: Query<(&PostureRoot, &mut Visibility)>,
    mut nodes: Query<(&ResNode, &mut BackgroundColor, &mut Node), (Without<PostureFill>, Without<HpFill>, Without<HpTrail>)>,
    mut items: Query<&mut Text, With<ItemText>>,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
) {
    let Some((a, p)) = player.iter().next() else { return };
    let f = (a.hp / a.hp_max).clamp(0.0, 1.0);
    // The damage trail holds for 0.5 s, then drains to the bar.
    let (shown, hold) = &mut *trail;
    if f < *shown {
        *hold += time.delta_secs();
        if *hold > 0.5 {
            *shown = (*shown - 0.6 * time.delta_secs()).max(f);
        }
    } else {
        *shown = f;
        *hold = 0.0;
    }
    for mut n in &mut hp {
        n.width = Val::Percent(f * 100.0);
    }
    for mut n in &mut tr {
        n.width = Val::Percent(*shown * 100.0);
    }
    let pf = (a.posture / a.posture_max).clamp(0.0, 1.0);
    for (pfl, mut n, mut bg) in &mut posture {
        if pfl.0 == Side::Player {
            n.width = Val::Percent(pf * 100.0);
            bg.0 = posture_color(pf);
        }
    }
    for (r, mut v) in &mut root {
        if r.0 == Side::Player {
            *v = if pf > 0.005 { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    for (rn, mut bg, mut n) in &mut nodes {
        let look = res_node_look(rn.0, config.player.resurrections, p.resurrections);
        let display = if look.is_some() { Display::Flex } else { Display::None };
        if n.display != display {
            n.display = display;
        }
        bg.0 = if look == Some(true) { VITALITY } else { Color::srgba(0.1, 0.08, 0.07, 0.7) };
    }
    for mut t in &mut items {
        // The equipped prosthetic (Z switches) and combat art, by their in-game names.
        let tool = crate::player::equipped_tool(&combat, &config, p.tool_slot).map_or_else(|| "-".to_string(), |t| combat.weapon_name(t.id));
        let want = format!("{tool}   ·   {}
Healing Gourd  {}      Spirit Emblems  {}", combat.weapon_name(config.player.combat_art), p.gourd, p.emblems);
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// Wolf's poison / burn build-up over his vitality bar, with the game's sprites (SB_FE
/// MENU_Resist_Poison 102 x 134, MENU_Resist_Fire 106 x 132, extracted/hud): a dim icon with the
/// full-colour one revealed from the bottom up as the gauge fills toward status::WOLF_RESIST,
/// whole and bright while he is poisoned / burning; hidden at 0. gap: the HUD movie
/// (menu/01_000_fe.gfx) builds this from ActionScript, not decoded: the place, size (a third of
/// the sprite here) and the fill style are by eye.
#[derive(Component)]
struct WolfBuildUp(usize);
#[derive(Component)]
struct WolfBuildUpFill(usize);
#[derive(Component)]
struct WolfBuildUpBright(usize);

/// The icon's size [px] (a third of the 102 x 134 sprite).
const BUILD_UP_W: f32 = 34.0;
const BUILD_UP_H: f32 = 44.0;

fn spawn_build_up(parent: &mut ChildSpawnerCommands, assets: &AssetServer, ty: usize, file: &'static str) {
    parent
        .spawn((
            Node { width: Val::Px(BUILD_UP_W), height: Val::Px(BUILD_UP_H), display: Display::None, ..default() },
            ImageNode::new(assets.load(file)).with_color(Color::srgba(0.45, 0.45, 0.45, 0.6)),
            WolfBuildUp(ty),
        ))
        .with_children(|c| {
            c.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(0.0), overflow: Overflow::clip(), ..default() },
                WolfBuildUpFill(ty),
            ))
            .with_children(|c| {
                c.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Px(0.0), width: Val::Px(BUILD_UP_W), height: Val::Px(BUILD_UP_H), ..default() },
                    ImageNode::new(assets.load(file)),
                    WolfBuildUpBright(ty),
                ));
            });
        });
}

#[allow(clippy::type_complexity)]
fn update_wolf_build_up(
    time: Res<Time>,
    combat: Res<Combat>,
    statuses: Option<Res<crate::status::Statuses>>,
    player: Query<Entity, With<crate::player::Player>>,
    mut icons: Query<(&WolfBuildUp, &mut Node), Without<WolfBuildUpFill>>,
    mut fills: Query<(&WolfBuildUpFill, &mut Node), Without<WolfBuildUp>>,
    mut bright: Query<(&WolfBuildUpBright, &mut ImageNode)>,
) {
    let status = player.iter().next().zip(statuses.as_deref()).and_then(|(e, s)| s.0.get(&e));
    let state = |ty: usize| -> (f32, bool) {
        let Some(s) = status else { return (0.0, false) };
        let refs = [crate::status::REF_POISON, crate::status::REF_BURNING];
        let gauge = if ty == 0 { s.poison } else { s.burn };
        ((gauge / crate::status::WOLF_RESIST).clamp(0.0, 1.0), s.afflicted(&combat, refs[ty]))
    };
    for (i, mut n) in &mut icons {
        let (f, on) = state(i.0);
        n.display = if f > 0.0 || on { Display::Flex } else { Display::None };
    }
    for (i, mut n) in &mut fills {
        let (f, on) = state(i.0);
        n.height = Val::Percent(if on { 100.0 } else { f * 100.0 });
    }
    // While afflicted the icon pulses (gap: the game's own animation is in the FE movie).
    let pulse = 0.8 + 0.2 * (time.elapsed_secs() * 4.0).sin();
    for (i, mut img) in &mut bright {
        let (_, on) = state(i.0);
        img.color = if on { Color::srgb(pulse, pulse, pulse) } else { Color::WHITE };
    }
}

/// Each enemy's bars over its head: vitality (short, left) above posture (centred).
fn spawn_enemy_bars(commands: &mut Commands, enemy: Entity) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(170.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() },
            Visibility::Hidden,
            EnemyRoot(enemy),
        ))
        .with_children(|c| {
            framed(c, 96.0, 5.0, false, (BackgroundColor(VITALITY), EnemyHpFill(enemy)), None::<()>);
            framed(c, 170.0, 7.0, true, (BackgroundColor(Color::srgb(1.0, 0.7, 0.15)), EnemyPostureFill(enemy)), None::<()>);
        });
}

#[allow(clippy::type_complexity)]
fn update_enemy_hud(
    mut commands: Commands,
    enemies: Query<(Entity, &Actor, &crate::enemy::Enemy, &GlobalTransform, Option<&crate::enemy::Phantom>)>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    mut roots: Query<(Entity, &EnemyRoot, &mut Node, &mut Visibility), (Without<EnemyHpFill>, Without<EnemyPostureFill>)>,
    mut hp: Query<(&EnemyHpFill, &mut Node), (Without<EnemyRoot>, Without<EnemyPostureFill>)>,
    mut posture: Query<(&EnemyPostureFill, &mut Node, &mut BackgroundColor), (Without<EnemyRoot>, Without<EnemyHpFill>)>,
) {
    // A set of bars per enemy: new enemies get theirs, removed ones lose them.
    let have: HashSet<Entity> = roots.iter().map(|(_, r, ..)| r.0).collect();
    for (e, ..) in &enemies {
        if !have.contains(&e) {
            spawn_enemy_bars(&mut commands, e);
        }
    }
    let cam = camera.single().ok();
    for (root_e, root, mut node, mut vis) in &mut roots {
        let Ok((_, a, e, g, ph)) = enemies.get(root.0) else {
            commands.entity(root_e).despawn();
            continue;
        };
        // Shown once it is fighting or hurt, over its head; gone when it dies (and while a
        // boss's phantom is out of the fight).
        let show = !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()) && (e.targeting.state >= crate::stealth::FIND || a.posture > 0.0 || a.hp < a.hp_max);
        let at = cam.and_then(|(cam, ct)| cam.world_to_viewport(ct, g.translation() + Vec3::Y * 0.85).ok());
        let Some(p) = at.filter(|_| show) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        node.left = Val::Px(p.x - 85.0);
        node.top = Val::Px(p.y - 20.0);
    }
    for (f, mut n) in &mut hp {
        if let Ok((_, a, ..)) = enemies.get(f.0) {
            n.width = Val::Percent((a.hp / a.hp_max).clamp(0.0, 1.0) * 100.0);
        }
    }
    for (f, mut n, mut bg) in &mut posture {
        if let Ok((_, a, ..)) = enemies.get(f.0) {
            let pf = (a.posture / a.posture_max).clamp(0.0, 1.0);
            n.width = Val::Percent(pf * 100.0);
            bg.0 = posture_color(pf);
        }
    }
}

/// The deathblow mark (忍殺, the game's FE sprites MENU_ninsatu_02 - the red glow - with
/// MENU_ninsatu_01 at its core, extracted/hud/) on the enemy's lock-on dummy 220 (the chest) while a
/// deathblow is possible: posture or vitality broken, or Wolf within reach behind an enemy that has
/// not noticed him (player::deathblow_check). gap: the FE movie (menu/01_000_fe.gfx) that sizes and
/// animates it is not decoded; size by camera distance and a slow pulse are made to match clip 1.
#[derive(Component)]
struct DeathblowMark;

fn spawn_item_icons(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(56.0),
            bottom: Val::Px(44.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(10.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn((Node { width: Val::Px(64.0), height: Val::Px(64.0), ..default() }, ImageNode::default(), Visibility::Hidden, ItemIcon(false)));
            c.spawn((Node { width: Val::Px(96.0), height: Val::Px(96.0), ..default() }, ImageNode::default(), Visibility::Hidden, ItemIcon(true)));
        });
}

fn update_item_icons(
    mut shown: Local<[i64; 2]>,
    assets: Res<AssetServer>,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
    player: Query<&crate::player::Player>,
    mut q: Query<(&ItemIcon, &mut ImageNode, &mut Visibility)>,
) {
    let slot = player.iter().next().map_or(0, |p| p.tool_slot);
    for (icon, mut img, mut vis) in &mut q {
        // The tool's iconId rides on its exported Prosthetic row; the art's on its EquipParamWeapon row.
        let id = if icon.0 {
            crate::player::equipped_tool(&combat, &config, slot).map_or(0, |t| t.icon)
        } else {
            combat.param("EquipParamWeapon", config.player.combat_art)["iconId"].as_i64().unwrap_or(0)
        };
        let i = icon.0 as usize;
        if shown[i] == id {
            continue;
        }
        shown[i] = id;
        let file = format!("hud/MENU_ItemIcon_{id:05}.png");
        if id > 0 && crate::paths::extracted().join(&file).exists() {
            img.image = assets.load(file);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// The deathblow mark's side on the 1920 x 1080 stage: the FE movie's MenuVFX instance (class
/// MenuVFX_Top_294) child State_1 = MENU_ninsatu_02 (405 x 405) at scale 0.45, centred on the
/// clip origin, no colour transform. (MENU_ninsatu_01 belongs to the skill-point screen.)
/// Timeline: FadeIn frames 1-12 alpha 0 -> 1, hold to 38; Loop 38-98 alpha 1 -> 0.6016 (68) ->
/// 0.98 (98); FadeOut 99-114 alpha 1 -> 0. gap: the engine's placement (native: the anchor, any
/// scale with distance) is not traced; its State_2 (MENU_ninjyutu, the ninjutsu mark) is not used.
const DEATHBLOW_SIDE: f32 = 405.0 * 0.45;

fn spawn_deathblow_mark(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, ImageNode::new(assets.load("hud/MENU_ninsatu_02.png")), Visibility::Hidden, DeathblowMark));
}

/// The deathblow clip's alpha `t` s after FadeIn, `gone` s into FadeOut.
fn deathblow_clip(t: f32, gone: Option<f32>) -> f32 {
    if let Some(g) = gone {
        return (1.0 - g * 60.0 / 15.0).max(0.0);
    }
    keys(&[(1.0, 0.0), (12.0, 1.0), (38.0, 1.0), (68.0, 0.6016), (98.0, 0.98)], clip_frame(t, 38.0, 98.0))
}

#[allow(clippy::type_complexity)]
fn update_deathblow_mark(
    time: Res<Time>,
    combat: Res<Combat>,
    wolf: Query<(&Actor, &GlobalTransform), With<crate::player::Player>>,
    enemies: MarkEnemies,
    dummies: Query<&GlobalTransform>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    lock: Option<Res<crate::camera::LockOn>>,
    window: Query<&Window>,
    mut mark: Query<(&mut Node, &mut ImageNode, &mut Visibility), With<DeathblowMark>>,
    mut clip: Local<MarkClip>,
) {
    let Ok((mut node, mut img, mut vis)) = mark.single_mut() else { return };
    let at = deathblow_mark_at(&combat, &wolf, &enemies, &dummies, &camera, lock.as_ref().and_then(|l| l.target));
    let Some((t, gone, p)) = clip.step(at, time.delta_secs(), 15.0 / 60.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    let side = DEATHBLOW_SIDE * window.single().map_or(720.0, |w| w.height()) / 1080.0;
    node.width = Val::Px(side);
    node.height = Val::Px(side);
    node.left = Val::Px(p.x - side / 2.0);
    node.top = Val::Px(p.y - side / 2.0);
    img.color = Color::WHITE.with_alpha(deathblow_clip(t, gone));
    *vis = Visibility::Inherited;
}

/// The deathblow mark's enemies query.
pub(crate) type MarkEnemies<'w, 's> = Query<'w, 's, (Entity, &'static Actor, &'static crate::enemy::Enemy, &'static GlobalTransform, Option<&'static crate::model::Dummies>), Without<crate::player::Player>>;

/// Where the deathblow mark goes this frame (a screen point), if an enemy can take one.
fn deathblow_mark_at(
    combat: &Combat,
    wolf: &Query<(&Actor, &GlobalTransform), With<crate::player::Player>>,
    enemies: &MarkEnemies,
    dummies: &Query<&GlobalTransform>,
    camera: &Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    lock: Option<Entity>,
) -> Option<Vec2> {
    let (Ok((wa, wg)), Ok((cam, ct))) = (wolf.single(), camera.single()) else { return None };
    let pick = deathblow_mark_target(combat, wa, wg.translation(), enemies, dummies, lock)?;
    let (_, _, _, eg, dm) = enemies.get(pick).ok()?;
    let at = dm.and_then(|d| d.0.get(&220)).and_then(|d| dummies.get(*d).ok()).map_or(eg.translation() + Vec3::Y * 1.4, |g| g.translation());
    cam.world_to_viewport(ct, at).ok()
}

/// The enemy the deathblow mark sits on: the one a deathblow would take (player.rs
/// deathblow_pick), else a broken one out of reach (locked first, then the nearest).
pub(crate) fn deathblow_mark_target(combat: &Combat, wa: &Actor, wolf: Vec3, enemies: &MarkEnemies, dummies: &Query<&GlobalTransform>, lock: Option<Entity>) -> Option<Entity> {
    // Gone once the deathblow starts (Deathblow, DeathblowStart, PlungeDeathblow).
    if wa.state.contains("Deathblow") {
        return None;
    }
    let cands = enemies.iter().filter_map(|(ent, ea, e, eg, dm)| {
        // The same set the attack picks from (player.rs): a break pose (ThrowDef12100 / 13100)
        // takes a deathblow; the throws themselves end the mark above (Wolf in a Deathblow state).
        if e.is_dead() {
            return None;
        }
        let dmy = |id: i16| dm.and_then(|dm| dm.0.get(&id)).and_then(|m| dummies.get(*m).ok()).map(|g| g.translation());
        let db = crate::player::deathblow_check(combat, wolf, ea, e, eg.translation(), &dmy)?;
        Some((ent, eg.translation(), db.in_reach, e.is_broken()))
    });
    crate::player::deathblow_pick(cands, lock, wolf, true)
}

/// A mark's FE-movie timeline (menu/01_000_fe.gfx, 60 fps; the engine plays "FadeIn" when the
/// mark appears, the clip loops "Loop", and "FadeOut" when it goes): seconds since it appeared,
/// and since it was let go (it fades out where it was last).
#[derive(Default)]
struct MarkClip {
    shown: Option<f32>,
    gone: Option<f32>,
    last: Vec2,
}

impl MarkClip {
    /// Advances by `dt` with the mark wanted at `at` (None = not wanted). Returns the time since
    /// it appeared, the time since it was let go, and where it is; None once it is fully gone.
    fn step(&mut self, at: Option<Vec2>, dt: f32, fade_out: f32) -> Option<(f32, Option<f32>, Vec2)> {
        match at {
            Some(p) => {
                self.last = p;
                self.gone = None;
                self.shown = Some(self.shown.map_or(0.0, |t| t + dt));
            }
            None => {
                let t = self.shown?;
                let g = self.gone.map_or(0.0, |g| g + dt);
                if g >= fade_out {
                    self.shown = None;
                    self.gone = None;
                    return None;
                }
                self.gone = Some(g);
                self.shown = Some(t + dt);
            }
        }
        Some((self.shown?, self.gone, self.last))
    }
}

/// A linear key track (frame, value) at frame `f`.
fn keys(track: &[(f32, f32)], f: f32) -> f32 {
    let i = track.iter().rposition(|k| k.0 <= f).unwrap_or(0);
    match track.get(i + 1) {
        Some(n) => track[i].1 + (n.1 - track[i].1) * ((f - track[i].0) / (n.0 - track[i].0)).clamp(0.0, 1.0),
        None => track[i].1,
    }
}

/// The FE clip's frame `t` seconds after FadeIn (frame 1): straight on up to `loop_at`, then the
/// loop `loop_at..=loop_end` repeating (its last frame's script is gotoAndPlay("Loop")).
fn clip_frame(t: f32, loop_at: f32, loop_end: f32) -> f32 {
    let f = 1.0 + t * 60.0;
    if f < loop_at { f } else { loop_at + (f - loop_at) % (loop_end - loop_at + 1.0) }
}

/// The lock-on mark: the FE movie's LockOn instance (class Lockon_Ani_297, 01_000_fe.gfx). Its
/// shape 1042 is a 32 x 32 px square centred on the clip origin, filled with MENU_Lockon01
/// (menu/hi/01_common.tpf, a standalone 64 x 32 texture) one texel per pixel from texel (0, 0):
/// only the texture's left 32 x 32 half shows, its disc centre (16, 16) on the origin. No colour
/// transform (white). Timeline: FadeIn frames 1-16, scale 3 -> 1 and alpha 0 -> 1; Loop 16-83,
/// alpha 1 -> 0.7383 (frame 50) -> 0.9922 (83); FadeOut 84-89, alpha 1 -> 0 in 0.2 steps. The
/// engine places it (native code, no ActionScript names it); here on the locked target's lock-on
/// dummy 220, sized by window height / 1080. gap: the anchor, and whether the engine scales it
/// with distance.
#[derive(Component)]
struct LockMark;

/// The square's side on the stage, and the texels it shows.
const LOCK_SIDE: f32 = 32.0;

fn spawn_lock_mark(mut commands: Commands, assets: Res<AssetServer>) {
    let mut img = ImageNode::new(assets.load("hud/MENU_Lockon01.png"));
    img.rect = Some(Rect::new(0.0, 0.0, LOCK_SIDE, LOCK_SIDE));
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, img, Visibility::Hidden, LockMark));
}

/// The lock-on clip's scale and alpha `t` s after FadeIn, `gone` s into FadeOut.
fn lock_clip(t: f32, gone: Option<f32>) -> (f32, f32) {
    if let Some(g) = gone {
        return (1.0, (1.0 - (g * 60.0).floor() * 0.2).max(0.0));
    }
    let f = clip_frame(t, 16.0, 83.0);
    let scale = keys(&[(1.0, 3.0), (16.0, 1.0)], f);
    let alpha = keys(&[(1.0, 0.0), (16.0, 1.0), (50.0, 0.7383), (83.0, 0.9922)], f);
    (scale, alpha)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_lock_mark(
    time: Res<Time>,
    lock: Option<Res<crate::camera::LockOn>>,
    enemies: Query<(&GlobalTransform, Option<&crate::model::Dummies>), Without<crate::player::Player>>,
    dummies: Query<&GlobalTransform>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    window: Query<&Window>,
    mut mark: Query<(&mut Node, &mut ImageNode, &mut Visibility), With<LockMark>>,
    mut clip: Local<MarkClip>,
    mut last_target: Local<Option<Entity>>,
) {
    let Ok((mut node, mut img, mut vis)) = mark.single_mut() else { return };
    let target = lock.and_then(|l| l.target);
    // A new target plays FadeIn again.
    if target.is_some() && target != *last_target {
        *clip = MarkClip::default();
    }
    *last_target = target;
    let at = target.and_then(|t| enemies.get(t).ok()).and_then(|(eg, dm)| {
        let (cam, ct) = camera.single().ok()?;
        let at = dm.and_then(|d| d.0.get(&220)).and_then(|d| dummies.get(*d).ok()).map_or(eg.translation() + Vec3::Y * 1.4, |g| g.translation());
        cam.world_to_viewport(ct, at).ok()
    });
    let Some((t, gone, p)) = clip.step(at, time.delta_secs(), 6.0 / 60.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    let (scale, alpha) = lock_clip(t, gone);
    let side = LOCK_SIDE * scale * window.single().map_or(720.0, |w| w.height()) / 1080.0;
    node.width = Val::Px(side);
    node.height = Val::Px(side);
    node.left = Val::Px(p.x - side / 2.0);
    node.top = Val::Px(p.y - side / 2.0);
    img.color = Color::WHITE.with_alpha(alpha);
    *vis = Visibility::Inherited;
}

/// The grapple point mark (SB_FE): MENU_Wire_Marker on the point the hook would fire at when it is
/// in the action range, MENU_Wire_NG (greyed) when it is only found (WirePointSearchParam 0: act
/// to 15 m, find to 25 m; wire::WireAim). Hidden during a wire move. gap: the FE movie that sizes
/// and animates it is not decoded; drawn at its atlas size (66 / 156 px on the 1920 x 1080 stage)
/// scaled to the window.
#[derive(Component)]
struct WireMark;

fn spawn_wire_mark(mut commands: Commands) {
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, ImageNode::default(), Visibility::Hidden, WireMark));
}

fn update_wire_mark(
    assets: Res<AssetServer>,
    aim: Option<Res<crate::wire::WireAim>>,
    wolf: Query<&crate::player::Player>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    window: Query<&Window>,
    mut shown: Local<Option<bool>>,
    mut mark: Query<(&mut Node, &mut ImageNode, &mut Visibility), With<WireMark>>,
) {
    let Ok((mut node, mut img, mut vis)) = mark.single_mut() else { return };
    *vis = Visibility::Hidden;
    let busy = wolf.iter().any(|p| p.wire.is_some());
    let (Some(t), Ok((cam, ct))) = (aim.and_then(|a| a.target), camera.single()) else { return };
    if busy {
        return;
    }
    let Ok(p) = cam.world_to_viewport(ct, t.point.hook) else { return };
    if *shown != Some(t.in_action_range) {
        *shown = Some(t.in_action_range);
        img.image = assets.load(if t.in_action_range { "hud/MENU_Wire_Marker.png" } else { "hud/MENU_Wire_NG.png" });
    }
    let scale = window.single().map_or(720.0, |w| w.height()) / 1080.0;
    let size = if t.in_action_range { 66.0 } else { 156.0 } * scale;
    node.width = Val::Px(size);
    node.height = Val::Px(size);
    node.left = Val::Px(p.x - size / 2.0);
    node.top = Val::Px(p.y - size / 2.0);
    *vis = Visibility::Inherited;
}

/// The controls line, state readout and combat log show only with the debug menu (F1) open.
fn debug_only(menu: Option<Res<crate::debug_menu::DebugMenu>>, mut q: Query<&mut Visibility, Or<(With<DebugText>, With<LogText>)>>) {
    let open = menu.is_some_and(|m| m.open);
    for mut v in &mut q {
        *v = if open { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn update_debug(
    combat: Res<Combat>,
    actors: Query<(&Actor, Option<&crate::enemy::Enemy>)>,
    mut text: Single<&mut Text, With<DebugText>>,
) {
    let mut s = String::from(
        "LMB/J attack (hold = thrust)  RMB/K deflect  LMB+RMB art  Shift step (hold = sprint)  Space jump  Q lock-on  Alt walk  T enemy AI on/off  E gourd  F shuriken  X sheathe/draw  LMB (dead) resurrect  H hurtboxes  R reset  F5 reload config  F1 debug menu\n\n",
    );
    for (a, enemy) in &actors {
        let d = data_for(&combat, &a);
        let who = if a.side == Side::Player { "YOU  " } else { "ENEMY" };
        s += &format!("{who} {:<28} {}  f{:>5.1}/{:.0}", a.state, if a.anim.is_empty() { "-" } else { &a.anim }, a.frame(), d.length(&a.anim) * crate::data::TAE_FPS);
        s += &format!("  HP {:.0}/{:.0}  posture {:.0}/{:.0}\n", a.hp, a.hp_max, a.posture, a.posture_max);
        if let Some(e) = enemy.filter(|e| e.aggressive && !e.ai_desc.is_empty()) {
            s += &format!("      AI: {}\n", e.ai_desc);
        }
        if a.side == Side::Player && !a.anim.is_empty() {
            let mut tags = Vec::new();
            if d.has_ref(&a.anim, a.t, REF_JUST_GUARD) { tags.push("[DEFLECT WINDOW]"); }
            if d.has_ref(&a.anim, a.t, REF_GUARD_COMBO) { tags.push("[guard chain]"); }
            for (flag, name) in [(FLAG_ACCEPT_ATTACK, "atk"), (FLAG_ACCEPT_GUARD, "grd"), (FLAG_ACCEPT_STEP, "step"), (FLAG_ACCEPT_JUMP, "jump"), (FLAG_ACCEPT_MOVE, "move")] {
                if d.flag(&a.anim, a.t, flag) { tags.push(name); }
            }
            if !d.attack_windows(&a.anim).iter().all(|(e, _, _)| !e.in_time(a.t)) { tags.push("[HITBOX]"); }
            if let Some(t) = d.stamina_ratio_type(&a.anim, a.t) { s += &format!("      posture regen type {t}  "); }
            s += &format!("      accepts: {}\n", tags.join(" "));
        }
    }
    text.0 = s;
}

fn update_log(time: Res<Time>, mut log: ResMut<CombatLog>, mut text: Single<&mut Text, With<LogText>>) {
    let dt = time.delta_secs();
    log.lines.retain_mut(|l| {
        l.2 += dt;
        l.2 < 6.0
    });
    text.0 = log.lines.iter().map(|l| l.0.clone()).collect::<Vec<_>>().join("\n");
}

/// The kanji needs a Japanese font: the first of Windows' Yu Gothic Bold / MS Gothic
/// found; without one the warning falls back to "!!".
fn spawn_perilous(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = ["YuGothB.ttc", "msgothic.ttc", "meiryo.ttc"]
        .iter()
        .find_map(|f| std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(f)).ok())
        .map(|bytes| fonts.add(Font::from_bytes(bytes)));
    let has_cjk = font.is_some();
    commands
        .spawn(Node { position_type: PositionType::Absolute, top: Val::Percent(18.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() })
        .with_children(|c| {
            c.spawn((
                Text::new(""),
                TextFont { font: font.map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(110.0), ..default() },
                TextColor(Color::srgba(0.9, 0.05, 0.05, 0.0)),
                PerilousText,
                Name::new(if has_cjk { "kanji" } else { "ascii" }),
            ));
        });
}

fn update_perilous(time: Res<Time>, mut p: ResMut<Perilous>, mut q: Query<(&mut Text, &mut TextColor, &Name), With<PerilousText>>) {
    p.timer -= time.delta_secs();
    for (mut text, mut color, name) in &mut q {
        if p.timer > 0.0 {
            text.0 = if name.as_str() == "kanji" { "危".into() } else { "!!".into() };
            // Pop in, hold, fade.
            let a = (p.timer / PERILOUS_SHOW * 3.0).min(1.0);
            color.0 = Color::srgba(0.9, 0.05, 0.05, a);
        } else if !text.0.is_empty() {
            text.0.clear();
        }
    }
}

/// The stealth indicator over the enemy's head, fed like the game's (FUN_14061ff40 ->
/// FUN_1408c5fb0: level 0 / 1 / 2 and the meter ratio): an amber bar that fills while it
/// watches Wolf in its wide "around" sight, solid amber while it is alerted (caution), red when
/// it has found him. The art is the game's (SB_FE MENU_Find_02 the thin triangle frame,
/// MENU_Find_01 the brush triangle, extracted/hud): the brush triangle shows from the bottom tip
/// up as the meter fills. Look from the HUD movie (menu/01_000_fe.gfx, decoded 2026-10-12):
/// FindIcon0-7 (class MarkerTop_334, placed by the engine) -> State_0 / 1 / 2, MENU_Find_02 at
/// 28.07 px and MENU_Find_01 at 28.8 px on the 1080 stage, centred 35.5 px above the clip origin;
/// colours are colour transforms (multiply 0 + add = a solid colour, alpha kept): State_0 (normal)
/// frame black at alpha 0.30, brush untinted at 0.70; State_1 (caution) frame (154, 134, 70) at
/// 0.90, brush x (0.719, 0.629, 0.238); State_2 (discovery) frame (163, 64, 57) at 0.32, brush
/// the same red at 0.50. MarkerTop: FadeIn frames 1-16 alpha 0 -> 1, FadeOut 17-38 1 -> 0.
/// gap: the engine's anchor (here the lock-on dummy 220), the gauge clips' mask travel (drawn as
/// the brush shown from the bottom up), Caution's red fill layer and drop shadow, the
/// discovery Highlight (x 1.4) are not drawn.
/// One enemy's stealth mark (`found_t`: seconds it has been fighting, for FOUND_SHOW).
#[derive(Component)]
struct StealthMark {
    enemy: Entity,
    found_t: f32,
    clip: MarkClip,
}
/// The clip box that reveals the brush triangle (height = the meter).
#[derive(Component)]
struct StealthFill(Entity);
#[derive(Component)]
struct StealthBrush(Entity);

/// The mark's side at 1080p [px] (MENU_Find_02's 128 px at 0.2193) and its centre's height
/// over the anchor.
const STEALTH_PX: f32 = 28.07;
const STEALTH_LIFT: f32 = 35.5;

/// Seconds the red "found" mark stays once the fight starts.
const FOUND_SHOW: f32 = 1.5;

fn spawn_stealth(commands: &mut Commands, assets: &AssetServer, enemy: Entity) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(STEALTH_PX), height: Val::Px(STEALTH_PX), ..default() },
            ImageNode::new(assets.load("hud/MENU_Find_02.png")),
            Visibility::Hidden,
            StealthMark { enemy, found_t: 0.0, clip: MarkClip::default() },
        ))
        .with_children(|c| {
            c.spawn((
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(0.0), overflow: Overflow::clip(), ..default() },
                StealthFill(enemy),
            ))
            .with_children(|c| {
                c.spawn((
                    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Px(0.0), width: Val::Px(STEALTH_PX), height: Val::Px(STEALTH_PX), ..default() },
                    ImageNode::new(assets.load("hud/MENU_Find_01.png")),
                    StealthBrush(enemy),
                ));
            });
        });
}

#[allow(clippy::type_complexity)]
fn update_stealth(
    mut commands: Commands,
    assets: Res<AssetServer>,
    window: Query<&Window>,
    time: Res<Time>,
    enemies: Query<(Entity, &crate::enemy::Enemy, &GlobalTransform, Option<&crate::enemy::Phantom>, Option<&crate::model::Dummies>)>,
    dummies: Query<&GlobalTransform>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    mut marks: Query<(Entity, &mut StealthMark, &mut Node, &mut ImageNode, &mut Visibility), (Without<StealthFill>, Without<StealthBrush>)>,
    mut fills: Query<(&StealthFill, &mut Node), (Without<StealthMark>, Without<StealthBrush>)>,
    mut brushes: Query<(&StealthBrush, &mut Node, &mut ImageNode), (Without<StealthMark>, Without<StealthFill>)>,
) {
    let have: HashSet<Entity> = marks.iter().map(|(_, m, ..)| m.enemy).collect();
    let k = window.single().map_or(720.0, |w| w.height()) / 1080.0;
    for (e, ..) in &enemies {
        if !have.contains(&e) {
            spawn_stealth(&mut commands, &assets, e);
        }
    }
    let side = STEALTH_PX * k;
    let cam = camera.single().ok();
    for (me, mut m, mut node, mut frame, mut vis) in &mut marks {
        let Ok((_, e, etf, ph, dm)) = enemies.get(m.enemy) else {
            commands.entity(me).despawn();
            continue;
        };
        let (level, ratio) = e.targeting.hud();
        if e.targeting.state == crate::stealth::BATTLE {
            m.found_t += time.delta_secs();
        } else {
            m.found_t = 0.0;
        }
        let show = !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()) && (level > 0 || ratio > 0.0) && m.found_t < FOUND_SHOW;
        // Anchored on the lock-on dummy 220 (root + 1.4 m without it), the mark's centre
        // STEALTH_LIFT px above it. gap: the game's anchor for this mark is not traced.
        let head = dm.and_then(|d| d.0.get(&220)).and_then(|d| dummies.get(*d).ok()).map_or(etf.translation() + Vec3::Y * 1.4, |g| g.translation());
        let pos = cam.and_then(|(cam, ctf)| cam.world_to_viewport(ctf, head).ok()).filter(|_| show);
        let Some((t, gone, p)) = m.clip.step(pos, time.delta_secs(), 22.0 / 60.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let fade = match gone {
            Some(g) => (1.0 - g * 60.0 / 22.0).max(0.0),
            None => (t * 60.0 / 15.0).min(1.0),
        };
        *vis = Visibility::Inherited;
        node.width = Val::Px(side);
        node.height = Val::Px(side);
        node.left = Val::Px(p.x - side / 2.0);
        node.top = Val::Px(p.y - STEALTH_LIFT * k - side / 2.0);
        let rgb = |r: u8, g: u8, b: u8| Color::srgb_u8(r, g, b);
        let (w, frame_color, color) = match level {
            2 => (1.0, rgb(163, 64, 57).with_alpha(0.32), rgb(163, 64, 57).with_alpha(0.5)),
            1 => (1.0, rgb(154, 134, 70).with_alpha(0.9), Color::srgb(0.719, 0.629, 0.238)),
            _ => (ratio, Color::BLACK.with_alpha(0.3), Color::WHITE.with_alpha(0.7)),
        };
        frame.color = frame_color.with_alpha(frame_color.alpha() * fade);
        let color = color.with_alpha(color.alpha() * fade);
        for (f, mut fnode) in &mut fills {
            if f.0 == m.enemy {
                fnode.height = Val::Percent(w.clamp(0.0, 1.0) * 100.0);
            }
        }
        for (b, mut bnode, mut img) in &mut brushes {
            if b.0 == m.enemy {
                bnode.width = Val::Px(side);
                bnode.height = Val::Px(side);
                img.color = color;
            }
        }
    }
}

#[cfg(test)]
mod mark_tests {
    /// The FE clips: the lock mark shrinks in from 3x while fading in over 15 frames, then pulses
    /// 1 -> 0.74 -> 0.99 every 68 frames, and fades out in 5; the deathblow mark fades in over 11
    /// frames, holds, then pulses down to 0.60.
    #[test]
    fn marks_follow_the_fe_movie() {
        let (s, a) = super::lock_clip(0.0, None);
        assert!((s - 3.0).abs() < 1e-4 && a.abs() < 1e-4);
        let (s, a) = super::lock_clip(15.0 / 60.0, None);
        assert!((s - 1.0).abs() < 1e-4 && (a - 1.0).abs() < 1e-4);
        let (_, a) = super::lock_clip(49.0 / 60.0, None);
        assert!((a - 0.7383).abs() < 1e-3, "{a}");
        let (_, a) = super::lock_clip((49.0 + 68.0) / 60.0, None);
        assert!((a - 0.7383).abs() < 1e-3, "{a}");
        assert!((super::lock_clip(1.0, Some(2.0 / 60.0)).1 - 0.6).abs() < 1e-5);
        assert!(super::deathblow_clip(0.0, None).abs() < 1e-4);
        assert!((super::deathblow_clip(30.0 / 60.0, None) - 1.0).abs() < 1e-4);
        assert!((super::deathblow_clip(67.0 / 60.0, None) - 0.6016).abs() < 1e-3);
    }
}

#[cfg(test)]
mod res_node_tests {
    use bevy::prelude::*;

    /// One resurrection node per configured resurrection (default 1, not a fixed 2); a raised
    /// max adds nodes, nodes past a lowered max hide, and only the spent ones dim (round 19).
    #[test]
    fn the_resurrection_nodes_follow_the_configured_max() {
        let mut app = App::new();
        let mut config: crate::config::GameConfig = toml::from_str(&std::fs::read_to_string(crate::paths::root().join("config.toml")).unwrap()).unwrap();
        config.player = crate::config::PlayerConfig::default();
        app.insert_resource(config).add_systems(Update, super::sync_res_nodes);
        app.world_mut().spawn((Node::default(), super::ResRow));
        app.update();
        let count = |app: &mut App| app.world_mut().query::<&super::ResNode>().iter(app.world()).count();
        assert_eq!(count(&mut app), crate::config::PlayerConfig::default().resurrections as usize);
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.resurrections = 3;
        app.update();
        app.update();
        assert_eq!(count(&mut app), 3);
        assert_eq!(super::res_node_look(0, 1, 1), Some(true));
        assert_eq!(super::res_node_look(1, 1, 1), None);
        assert_eq!(super::res_node_look(1, 3, 1), Some(false));
    }
}
