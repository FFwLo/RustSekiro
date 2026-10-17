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
/// warning BulletBehavior (judge 980-983) reaches Wolf (enemy_bullet.rs fly).
#[derive(Resource, Default)]
pub struct Perilous {
    pub timer: f32,
    pub kind: i64,
}

impl Perilous {
    /// A warning of `kind`; `text`: show the HUD 危 (off when its effect 600035 plays instead,
    /// enemy_bullet.rs fly).
    pub fn raise(&mut self, kind: i64, text: bool) {
        self.timer = if text { PERILOUS_SHOW } else { 0.0 };
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
/// The lit lotus (MENU_Revive_GaugeMax) over node i while it is charged.
#[derive(Component)]
struct ResLit(u32);
/// The row the resurrection nodes sit in; one node per config player.resurrections.
#[derive(Component)]
struct ResRow;
/// 0 the Spirit Emblem count, 1 the quick item's.
#[derive(Component)]
struct ItemText(u8);
/// The quick item's icon (the item in hand, items.rs).
#[derive(Component)]
struct QuickIcon;
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
            .add_systems(Update, (sync_res_nodes, update_player_hud, update_enemy_hud, update_debug, update_log, debug_only, update_perilous, update_stealth, update_deathblow_mark, update_lock_mark, update_item_icons, update_quick_icon, update_wolf_build_up, hud_shots, update_wire_mark, update_boss_hud, flash_counters));
        if let Some(f) = std::env::var("SHINOBI_HUD_POSTURE").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.insert_resource(HoldPosture(f)).add_systems(Update, hold_posture);
        }
    }
}

/// `SHINOBI_HUD_SHOTS=<dir>[,<every s>[,<count>]]`: saves the game's own frame (HUD included)
/// every few seconds (default 2 s, 8 shots), then quits: a check of HUD art without a screen grab.
/// It also locks on to the nearest enemy, so the lock mark shows (not with `SHINOBI_CAM_PITCH`,
/// whose free camera angle the lock camera would override).
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
    if let (Some(mut lock), Ok(w), false) = (lock, wolf.single(), std::env::var("SHINOBI_CAM_PITCH").is_ok()) {
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
/// The vitality fill: the game's muted orange-red (the user's captures: about 180, 70, 48), on
/// the over-head bars as it is; on the textured MENU_HP_bar as the tint.
const VITALITY: Color = Color::srgb_u8(180, 70, 48);
const VITALITY_TINT: Color = Color::srgb(0.7, 0.27, 0.18);
/// The trail's light, desaturated red (the all-bosses clip, Gyoubu's bar: about 165, 130, 120).
const TRAIL: Color = Color::srgb_u8(165, 130, 120);

/// A framed bar: the fill (and an optional damage trail behind it) inside a thin gold frame.
fn framed(parent: &mut ChildSpawnerCommands, width: Val, height: Val, center: bool, fill: impl Bundle, trail: Option<impl Bundle>) {
    parent
        .spawn((
            Node {
                width,
                height,
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
    // Wolf, bottom left (clip r_000230, 960 x 540: x 5.7-15.2 %, y 90.7-92.2 %): resurrection
    // nodes over the vitality bar (SB_FE MENU_HP_base_2 frame, MENU_HP_bar fill tinted red).
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(5.7),
            bottom: Val::Percent(7.8),
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
            c.spawn((Node { flex_direction: FlexDirection::Row, ..default() }, ResRow));
            sprite_bar(c, &assets, Val::Vw(9.5), Val::Vh(1.25), "hud/MENU_HP_base_2.png", "hud/MENU_HP_bar.png", false, VITALITY_TINT, HpFill, Some(HpTrail));
        });
    // Counters, placed by the HUD movie (menu/01_000_fe.gfx main timeline, 1920 x 1080 stage):
    // PlayerCount at (1849.9, 994.6) is the Spirit Emblems (Timeline_154 "Soul": zanki_ari_symbol =
    // MENU_ItemIcon_00001 at 0.126, 32 px, from x 1750, y 984, the count after it); Item_Right at
    // (1838, 985), scale 0.42 (a 42 px icon), is the quick item: the Healing Gourd (EquipParamGoods
    // 3000, iconId 151) with its stock. Each is the goods' own menu icon (cut by sekiro-extract hud)
    // with the count after it and no label; the icon stays hidden until it is extracted.
    let counter = |c: &mut Commands, right: f32, top: Option<f32>, bottom: Option<f32>, which: u8, icon: i64, size: f32| {
        c.spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Percent(right),
            top: top.map_or(Val::Auto, Val::Percent),
            bottom: bottom.map_or(Val::Auto, Val::Percent),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|r| {
            let file = format!("hud/MENU_ItemIcon_{icon:05}.png");
            let have = crate::paths::extracted().join(&file).exists();
            r.spawn((
                Node { width: Val::Vw(size), height: Val::Vw(size), ..default() },
                if have { ImageNode::new(assets.load(file)) } else { ImageNode::default() },
                if have { Visibility::Inherited } else { Visibility::Hidden },
            ));
            r.spawn((
                Text::new(""),
                TextFont { font: serif.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(22.0), ..default() },
                TextColor(Color::srgba(0.92, 0.88, 0.8, 1.0)),
                TextShadow { offset: Vec2::new(1.5, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 1.0) },
                ItemText(which),
            ));
        });
    };
    let _ = counter;
    let art = |file: &str| -> (ImageNode, Visibility) {
        let f = format!("hud/{file}.png");
        if crate::paths::extracted().join(&f).exists() { (ImageNode::new(assets.load(f)), Visibility::Inherited) } else { (ImageNode::default(), Visibility::Hidden) }
    };
    let number = |s: &str, px: f32| {
        (
            Text::new(s),
            TextFont { font: serif.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(px), ..default() },
            TextColor(Color::srgba(0.92, 0.88, 0.8, 1.0)),
            TextShadow { offset: Vec2::new(1.5, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 1.0) },
        )
    };
    // The skill-point gauge, top right: the movie's SkillPoint_282 at (1484, 88.4): MENU_SkillPoint
    // (120 x 80 at 0.588 = 70 x 47 px) at (-56.75, -21.95) carrying the point count (SkillPiontLv_290),
    // then the experience bar from x 1482 to 1747 (MENU_HP_bar_mark caps, 0.75 tall): the points
    // to spend and the experience towards the next (src/experience.rs). gap: the bar's own art.
    commands
        .spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(74.3), top: Val::Percent(6.2), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() }, Visibility::Hidden, Flash(1)))
        .with_children(|r| {
            let (img, vis) = art("MENU_SkillPoint");
            r.spawn((Node { width: Val::Vw(3.67), height: Val::Vw(2.45), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, img, vis))
                .with_children(|i| {
                    i.spawn((number("0", 22.0), PointsText));
                });
            r.spawn((Node { width: Val::Vw(13.75), height: Val::Vh(0.9), margin: UiRect::left(Val::Vw(-0.7)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85))))
                .with_children(|b| {
                    b.spawn((Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.36, 0.68, 0.74, 1.0)), ExpFill));
                });
        });
    // What the last kills gave, on the bar's right end and under the purse's total (the user's
    // captures) while they show (experience.rs
    // Earned). gap: the game's own placement and font are not read from the movie.
    for (kind, right, top) in [(1u8, 12.0, 7.5), (0u8, 6.6, 70.0)] {
        commands
            .spawn((Node { position_type: PositionType::Absolute, right: Val::Percent(right), top: Val::Percent(top), ..default() }, Visibility::Hidden, Flash(kind)))
            .with_children(|c| {
                c.spawn((number("", 17.0), GainText(kind)));
            });
    }
    // Sen, right middle: the movie's Money_164 at (1702, 748.5): MENU_Money (92 x 94 at 0.355 = 33 px)
    // at (-61.8, -18.65), then a dark backing with the total right-aligned (TotalMoney_txt_166).
    // gap: Sv1 has no Sen, the total stays 0.
    commands
        .spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(85.4), top: Val::Percent(67.6), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() }, Visibility::Hidden, Flash(0)))
        .with_children(|r| {
            let (img, vis) = art("MENU_Money");
            r.spawn((Node { width: Val::Vw(1.72), height: Val::Vw(1.76), ..default() }, img, vis));
            r.spawn((
                Node { width: Val::Vw(7.3), height: Val::Vh(2.1), margin: UiRect::left(Val::Vw(-0.5)), justify_content: JustifyContent::FlexEnd, align_items: AlignItems::Center, padding: UiRect::right(Val::Px(8.0)), border_radius: BorderRadius::all(Val::Px(4.0)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
            ))
            .with_children(|b| {
                b.spawn((number("0", 20.0), SenText));
            });
        });
    // Wolf's posture, bottom centre (clip: x 36.5-63.5 %, y 86.7 %, 1.3 % thick): fills from the
    // middle outward, shown while it has damage.
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, bottom: Val::Percent(12.6), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
            Visibility::Hidden,
            PostureRoot(Side::Player),
        ))
        .with_children(|c| sprite_bar(c, &assets, Val::Vw(27.0), Val::Vh(1.3), "hud/MENU_Taikan_base.png", "hud/MENU_Taikan_bar.png", true, Color::WHITE, PostureFill(Side::Player), None::<()>));
    spawn_boss_hud(&mut commands, &assets, serif.clone());
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

/// A resurrection node as the FE movie draws it (menu/01_000_fe.gfx Resurrection_Icon_244, on the
/// 1920 x 1080 stage: ResurrectionList Icon_0..2 at x 129.8 + 46.8 i, y 938.3): MENU_Revive_Window_2
/// at half size (51 x 53 px) with MENU_Revive_GaugeMax over it while the node is charged. gap: the
/// recharge needle and fill (MENU_Revive_Needle / _Gauge) and the Temp (charm) node art.
fn res_node_bundle(i: u32, assets: Option<&AssetServer>) -> impl Bundle {
    let img = |f: &str| assets.map_or_else(ImageNode::default, |a| ImageNode::new(a.load(f.to_string())));
    (
        Node { width: Val::Vw(2.66), height: Val::Vw(2.76), margin: UiRect::left(Val::Vw(if i > 0 { -0.22 } else { 0.0 })), ..default() },
        img("hud/MENU_Revive_Window_2.png"),
        ResNode(i),
        children![(
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            img("hud/MENU_Revive_GaugeMax.png"),
            ResLit(i),
        )],
    )
}

/// Node i of `max` with `left` resurrections: None = past the max (hidden), Some(lit).
fn res_node_look(i: u32, max: u32, left: u32) -> Option<bool> {
    (i < max).then_some(i < left)
}

/// Keeps as many resurrection nodes as config player.resurrections (F5 reload can raise it).
fn sync_res_nodes(mut commands: Commands, config: Res<crate::config::GameConfig>, assets: Option<Res<AssetServer>>, row: Query<Entity, With<ResRow>>, nodes: Query<&ResNode>) {
    let Some(row) = row.iter().next() else { return };
    let have = nodes.iter().count() as u32;
    for i in have..config.player.resurrections {
        commands.entity(row).with_child(res_node_bundle(i, assets.as_deref()));
    }
}

/// The Vitality bar's width (vw) at HP_BAR_BASE maximum Vitality (level 1).
const HP_BAR_VW: f32 = 9.5;
const HP_BAR_BASE: f32 = 320.0;

/// MenuParam SpMinimumDispTime (0.5, extracted/json/params/MenuParam.json; row offset 0xb4, read
/// at exe 140e3.c:7651): the least time the posture gauge stays up once its damage is gone.
const SP_MIN_DISP_TIME: f32 = 0.5;

/// The posture gauge's state (exe FUN_140a02a10 -> chr+0x1a40): 0 normal, 1 about to break (at
/// most DAT_143b06724 = 30 % of the posture left, i.e. the bar 70 % full), 2 breakable (full). 3
/// (full while ChrActionFlag 0x160 holds, the break) is the chr's break reaction, told apart by
/// the caller (update_boss_hud: Enemy::is_broken).
fn posture_state(f: f32) -> usize {
    if f >= 1.0 { 2 } else if f >= 0.7 { 1 } else { 0 }
}

/// The fill per state, from the HUD movie (menu/01_000_fe.gfx, Bar clip labels State_0..3: colour
/// transforms on MENU_Taikan_bar, the same for Wolf and enemies) applied to the bar texture's
/// brightest texel (142, 123, 60): normal x1; State_1 x(0.801, 0.898, 0) + (40, 6, 0); State_2
/// x(0.898, 0.801, 0) + (40, 8, 0); State_3 x(1, 0.699, 0) + (40, 10, 0). gap: the texture's
/// gradient, the frame art (MENU_Taikan_ST1-3) and the Damage glow / flash layers.
const POSTURE_FILL: [[u8; 3]; 4] = [[142, 123, 60], [154, 116, 0], [168, 107, 0], [182, 96, 0]];

/// The tint that turns MENU_Taikan_bar (brightest texel 142, 123, 60) into the state's fill.
fn posture_tint(f: f32) -> Color {
    let [r, g, b] = POSTURE_FILL[posture_state(f)];
    Color::srgb(r as f32 / 142.0, g as f32 / 123.0, b as f32 / 60.0)
}

fn posture_color(f: f32) -> Color {
    let [r, g, b] = POSTURE_FILL[posture_state(f)];
    Color::srgb_u8(r, g, b)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_player_hud(
    time: Res<Time>,
    mut trail: Local<(f32, f32)>,
    mut posture_hold: Local<f32>,
    player: Query<(&Actor, &crate::player::Player)>,
    mut hp: Query<&mut Node, (With<HpFill>, Without<HpTrail>, Without<PostureFill>)>,
    mut tr: Query<&mut Node, (With<HpTrail>, Without<HpFill>, Without<PostureFill>)>,
    mut posture: Query<(&PostureFill, &mut Node, &mut ImageNode), (Without<HpFill>, Without<HpTrail>, Without<ResNode>)>,
    mut root: Query<(&PostureRoot, &mut Visibility), Without<ResLit>>,
    mut nodes: Query<(&ResNode, &mut Node), (Without<PostureFill>, Without<HpFill>, Without<HpTrail>)>,
    mut lit: Query<(&ResLit, &mut Visibility), Without<PostureRoot>>,
    mut items: Query<(&ItemText, &mut Text)>,
    config: Res<crate::config::GameConfig>,
    hp_frame: Query<&ChildOf, With<HpFill>>,
    mut frames: Query<&mut Node, (Without<HpFill>, Without<HpTrail>, Without<PostureFill>, Without<ResNode>)>,
) {
    let Some((a, p)) = player.iter().next() else { return };
    let f = (a.hp / a.hp_max).clamp(0.0, 1.0);
    step_trail(&mut trail, f, time.delta_secs());
    let shown = &trail.0;
    for mut n in &mut hp {
        n.width = Val::Percent(f * 100.0);
    }
    for mut n in &mut tr {
        n.width = Val::Percent(*shown * 100.0);
    }
    // The bar's length follows the maximum Vitality, as the game's grows with each Vitality level
    // (9.5 % of the width at level 1's 320; the user's capture at Vitality 20, 1120: 32.7 %).
    for c in &hp_frame {
        if let Ok(mut n) = frames.get_mut(c.parent()) {
            let w = Val::Vw(HP_BAR_VW * a.hp_max / HP_BAR_BASE);
            if n.width != w {
                n.width = w;
            }
        }
    }
    let pf = (a.posture / a.posture_max).clamp(0.0, 1.0);
    for (pfl, mut n, mut bg) in &mut posture {
        if pfl.0 == Side::Player {
            n.width = Val::Percent(pf * 100.0);
            bg.color = posture_tint(pf);
        }
    }
    // The gauge stays SP_MIN_DISP_TIME after the posture is back to 0 (exe 140e3.c:7637-7661).
    *posture_hold = if pf > 0.005 { SP_MIN_DISP_TIME } else { (*posture_hold - time.delta_secs()).max(0.0) };
    for (r, mut v) in &mut root {
        if r.0 == Side::Player {
            *v = if *posture_hold > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    for (rn, mut n) in &mut nodes {
        let display = if res_node_look(rn.0, config.player.resurrections, p.resurrections).is_some() { Display::Flex } else { Display::None };
        if n.display != display {
            n.display = display;
        }
    }
    for (rl, mut v) in &mut lit {
        let want = if res_node_look(rl.0, config.player.resurrections, p.resurrections) == Some(true) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    let quick = crate::items::in_slot(&config, p.quick_slot);
    for (t, mut text) in &mut items {
        let want = match (t.0, quick) {
            (0, _) => p.emblems.to_string(),
            (_, 0) => String::new(),
            (_, crate::items::GOURD) => p.gourd.to_string(),
            (_, id) => crate::items::count(&config, id).to_string(),
        };
        if text.0 != want {
            text.0 = want;
        }
    }
}

/// The trail's hold from the first hit (later hits do not restart it) and its collapse time,
/// measured at 60 fps on the all-bosses clip (Gyoubu 11:59-12:03, four trails: 0.48-0.50 s, then
/// gone in 1-3 frames).
const TRAIL_HOLD: f32 = 0.5;
const TRAIL_COLLAPSE: f32 = 0.05;

/// Steps a damage trail `(shown, held)` toward the bar's fill `f`.
fn step_trail(trail: &mut (f32, f32), f: f32, dt: f32) {
    let (shown, hold) = trail;
    if f < *shown {
        *hold += dt;
        if *hold > TRAIL_HOLD {
            // Linear to the fill over the collapse time left.
            let left = (TRAIL_HOLD + TRAIL_COLLAPSE - *hold + dt).max(dt);
            *shown = (*shown - (*shown - f) * (dt / left).min(1.0)).max(f);
        }
    } else {
        *shown = f;
        *hold = 0.0;
    }
}

/// A bar from the game's sprites: the `base` frame with the `bar` fill (tinted `tint`) and an
/// optional damage trail; `center` fills from the middle outward (the posture bars).
#[allow(clippy::too_many_arguments)]
fn sprite_bar(parent: &mut ChildSpawnerCommands, assets: &AssetServer, width: Val, height: Val, base: &'static str, bar: &'static str, center: bool, tint: Color, fill: impl Bundle, trail: Option<impl Bundle>) {
    parent
        .spawn((
            Node { width, height, justify_content: if center { JustifyContent::Center } else { JustifyContent::FlexStart }, ..default() },
            ImageNode::new(assets.load(base)).with_mode(bevy::ui::widget::NodeImageMode::Stretch),
        ))
        .with_children(|b| {
            if let Some(t) = trail {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, ImageNode::new(assets.load("hud/MENU_HP_bar.png")).with_color(TRAIL).with_mode(bevy::ui::widget::NodeImageMode::Stretch), t));
            }
            b.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, ImageNode::new(assets.load(bar)).with_color(tint).with_mode(bevy::ui::widget::NodeImageMode::Stretch), fill));
            // The posture bars' "<" ">" at their outer ends (MENU_Taikan_Arrow, 78 x 36 against
            // the 1274 x 34 base: 6.1 % of the bar's width; clip r_000130 x 31.5 % / 68 %).
            if center {
                for right in [false, true] {
                    // The other side Auto: UiRect's default is 0 on every side, which put the
                    // right arrow on the left one.
                    let side = if right { UiRect { left: Val::Auto, right: Val::Percent(-6.1), ..default() } } else { UiRect { left: Val::Percent(-6.1), right: Val::Auto, ..default() } };
                    b.spawn((
                        Node { position_type: PositionType::Absolute, left: side.left, right: side.right, top: Val::Percent(-3.0), width: Val::Percent(6.1), height: Val::Percent(106.0), ..default() },
                        ImageNode { flip_x: !right, image_mode: bevy::ui::widget::NodeImageMode::Stretch, ..ImageNode::new(assets.load("hud/MENU_Taikan_Arrow.png")) },
                        PostureArrow,
                    ));
                }
            }
        });
}

/// A posture bar's end arrow; shown while the posture is above 0 (the all-bosses clip, Lady
/// Butterfly 15:52). Wolf's go with his bar's root.
#[derive(Component)]
struct PostureArrow;

/// The boss's bars (an enemy with deathblow dots, NpcParam ninsatuNum > 0), placed as in clip
/// r_000230 (960 x 540): vitality top left (x 5.7-27.3 %, y 8-9.8 %; SB_Boss MENU_HP_base_Boss /
/// MENU_HP_bar_Boss), the deathblow orbs over its left end (MENU_HP_Boss lit, MENU_HP_Boss_base
/// spent; y 6.3 %, 2 % of the width apart), the name under it (y 12.4 %; npc_names.json, the
/// game's NPC名), and its posture top centre (x 30.6-69 %, y 6.3 %; MENU_Taikan_base / _bar,
/// MENU_Sp_Center at the middle). gap: the movie's fade in / out and the HP trail timing.
#[derive(Component)]
struct BossRoot;
#[derive(Component)]
struct BossHpFill;
#[derive(Component)]
struct BossHpTrail;
#[derive(Component)]
struct BossPostureFill;
#[derive(Component)]
struct BossName;
#[derive(Component)]
struct BossOrb(u32);
/// The posture glow behind the bar (MENU_Taikan_ST1-3, 700 x 78 against the 1274 x 34 base),
/// by posture_state 1-3; hidden at 0.
#[derive(Component)]
struct BossGlow;
/// The boss's posture bar and its centre mark: shown while the posture has damage (and
/// SP_MIN_DISP_TIME after), as Wolf's (the user's capture: no empty track at the top).
#[derive(Component)]
struct BossPostureRow;

fn spawn_boss_hud(commands: &mut Commands, assets: &AssetServer, serif: Option<Handle<Font>>) {
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, Visibility::Hidden, BossRoot))
        .with_children(|c| {
            c.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(5.5), top: Val::Percent(4.6), flex_direction: FlexDirection::Row, column_gap: Val::Vw(0.05), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                for i in 0..4 {
                    r.spawn((Node { width: Val::Vw(1.6), height: Val::Vw(1.6), display: Display::None, ..default() }, ImageNode::new(assets.load("hud/MENU_HP_Boss.png")), BossOrb(i)));
                }
            });
            c.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(5.7), top: Val::Percent(8.3), ..default() })
                // As thin as Wolf's and the same orange-red (the user's capture of Genichiro's bar;
                // MENU_HP_bar_Boss drew it thick and dark).
                .with_children(|r| sprite_bar(r, assets, Val::Vw(21.6), Val::Vh(1.25), "hud/MENU_HP_base_2.png", "hud/MENU_HP_bar.png", false, VITALITY_TINT, BossHpFill, Some(BossHpTrail)));
            c.spawn((
                Text::new(""),
                TextFont { font: serif.map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(15.0), ..default() },
                TextColor(Color::srgba(0.92, 0.88, 0.8, 1.0)),
                TextShadow { offset: Vec2::new(1.5, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 1.0) },
                Node { position_type: PositionType::Absolute, left: Val::Percent(6.3), top: Val::Percent(11.0), ..default() },
                BossName,
            ));
            c.spawn(Node { position_type: PositionType::Absolute, top: Val::Percent(4.2), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }).with_children(|r| {
                r.spawn((Node { width: Val::Vw(21.1), height: Val::Vh(4.1), ..default() }, ImageNode::new(assets.load("hud/MENU_Taikan_ST1.png")).with_mode(bevy::ui::widget::NodeImageMode::Stretch), Visibility::Hidden, BossGlow));
            });
            c.spawn((Node { position_type: PositionType::Absolute, top: Val::Percent(5.4), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }, Visibility::Hidden, BossPostureRow)).with_children(|r| {
                sprite_bar(r, assets, Val::Vw(38.4), Val::Vh(1.8), "hud/MENU_Taikan_base.png", "hud/MENU_Taikan_bar.png", true, Color::WHITE, BossPostureFill, None::<()>);
            });
            c.spawn((Node { position_type: PositionType::Absolute, top: Val::Percent(4.6), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }, Visibility::Hidden, BossPostureRow)).with_children(|r| {
                r.spawn((Node { width: Val::Vh(2.6), height: Val::Vh(3.1), ..default() }, ImageNode::new(assets.load("hud/MENU_Sp_Center.png"))));
            });
        });
}

/// The bar's name for an NpcParam row (extracted/enemies/npc_names.json rows).
fn boss_name(row: i64) -> String {
    static NAMES: std::sync::OnceLock<std::collections::HashMap<i64, String>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            let t = std::fs::read_to_string(crate::paths::root().join("extracted/enemies/npc_names.json")).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            v["rows"].as_object().into_iter().flatten().filter_map(|(k, n)| Some((k.parse().ok()?, n.as_str()?.to_string()))).collect()
        })
        .get(&row)
        .cloned()
        .unwrap_or_default()
}

/// The character the running map script put the boss bar on (2003[11], script.rs Run.bar).
fn scripted_bar(script: Option<&crate::enemy::BossScript>) -> Option<Entity> {
    let run = script?.run.as_ref()?;
    run.ents.get(&run.bar?).copied()
}

/// The enemy that gets the boss bars: alive, in the fight, with deathblow dots.
fn is_boss_bar(e: &crate::enemy::Enemy, ph: Option<&crate::enemy::Phantom>) -> bool {
    e.ninsatsu.1 > 0 && !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked())
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_boss_hud(
    combat: Res<Combat>,
    enemies: Query<(&Actor, &crate::enemy::Enemy, Option<&crate::enemy::Phantom>)>,
    mut root: Query<&mut Visibility, (With<BossRoot>, Without<BossGlow>)>,
    mut hp: Query<&mut Node, (With<BossHpFill>, Without<BossHpTrail>, Without<BossPostureFill>, Without<BossOrb>)>,
    mut hp_trail: Query<&mut Node, (With<BossHpTrail>, Without<BossHpFill>, Without<BossPostureFill>, Without<BossOrb>)>,
    time: Res<Time>,
    mut trail: Local<(f32, f32)>,
    mut posture: Query<(&mut Node, &mut ImageNode), (With<BossPostureFill>, Without<BossHpFill>, Without<BossOrb>)>,
    mut orbs: Query<(&BossOrb, &mut Node, &mut ImageNode), (Without<BossHpFill>, Without<BossPostureFill>)>,
    mut name: Query<&mut Text, With<BossName>>,
    mut glow: Query<(&mut Visibility, &mut ImageNode), (With<BossGlow>, Without<BossRoot>, Without<BossOrb>, Without<BossPostureFill>)>,
    assets: Res<AssetServer>,
    mut arrows: Query<(&ChildOf, &mut Visibility), (With<PostureArrow>, Without<BossRoot>, Without<BossGlow>)>,
    boss_frame: Query<&ChildOf, With<BossPostureFill>>,
    script: Option<Res<crate::enemy::BossScript>>,
    (mut posture_rows, mut boss_posture_hold): (Query<&mut Visibility, (With<BossPostureRow>, Without<BossRoot>, Without<BossGlow>, Without<PostureArrow>)>, Local<f32>),
) {
    // The map script's 2003[11] bar body first (the Old Dragons' 2500810 has no deathblow dots
    // and would never pass is_boss_bar), else the ninsatsu rule.
    let scripted = scripted_bar(script.as_deref()).and_then(|ent| enemies.get(ent).ok()).filter(|(_, e, ph)| !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()));
    // Only bosses and mini-bosses: named (the bar's NPC名) or with two deathblows or more. A
    // plain enemy with one dot keeps only its over-head bars (the user's captures).
    let boss = scripted.or_else(|| enemies.iter().find(|(a, e, ph)| is_boss_bar(e, *ph) && (e.ninsatsu.1 >= 2 || !boss_name(combat.foe_of(a).npc_row).is_empty())));
    for mut v in &mut root {
        *v = if boss.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    let Some((a, e, _)) = boss else {
        *trail = (1.0, 0.0);
        return;
    };
    let f = (a.hp / a.hp_max).clamp(0.0, 1.0);
    *boss_posture_hold = if a.posture > 0.005 * a.posture_max { SP_MIN_DISP_TIME } else { (*boss_posture_hold - time.delta_secs()).max(0.0) };
    for mut v in &mut posture_rows {
        let want = if *boss_posture_hold > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    step_trail(&mut trail, f, time.delta_secs());
    for mut n in &mut hp {
        n.width = Val::Percent(f * 100.0);
    }
    for mut n in &mut hp_trail {
        n.width = Val::Percent(trail.0 * 100.0);
    }
    let pf = (a.posture / a.posture_max).clamp(0.0, 1.0);
    for (mut n, mut img) in &mut posture {
        n.width = Val::Percent(pf * 100.0);
        img.color = posture_tint(pf);
    }
    for (parent, mut v) in &mut arrows {
        if boss_frame.iter().any(|f| f.parent() == parent.parent()) {
            *v = if pf > 0.005 { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    // State 3 (MENU_Taikan_ST3) is the break itself: the HUD movie's State_3 holds while the
    // chr's ChrActionFlag 0x160 (the posture-break reaction) is up, i.e. Enemy::is_broken.
    let st = if e.is_broken() { 3 } else { posture_state(pf) };
    for (mut v, mut img) in &mut glow {
        *v = if st == 0 { Visibility::Hidden } else { Visibility::Inherited };
        let file = ["", "hud/MENU_Taikan_ST1.png", "hud/MENU_Taikan_ST2.png", "hud/MENU_Taikan_ST3.png"][st];
        if st > 0 && img.image.path().is_none_or(|p| p.path().to_str() != Some(file)) {
            img.image = assets.load(file);
        }
    }
    // As the game counts them (GetNinsatsuNum): the dot goes dark at Wolf's Ninsatsu hit, not at
    // the throw's start (the finishers clip: ~1-2 s after the lunge).
    let (left, all) = (e.ninsatsu_left(), e.ninsatsu.1);
    for (o, mut n, mut img) in &mut orbs {
        let display = if o.0 < all { Display::Flex } else { Display::None };
        let lit = o.0 < left;
        // The spent orb is the smaller grey one (MENU_HP_Boss_base 80 px vs MENU_HP_Boss 114).
        // The user's capture of Genichiro's: lit about 1.9 % of the width with its glow.
        let size = Val::Vw(if lit { 1.9 } else { 1.0 });
        if n.display != display || n.width != size {
            n.display = display;
            n.width = size;
            n.height = size;
            img.image = assets.load(if lit { "hud/MENU_HP_Boss.png" } else { "hud/MENU_HP_Boss_base.png" });
        }
    }
    let want = boss_name(combat.foe_of(a).npc_row);
    for mut t in &mut name {
        if t.0 != want {
            t.0 = want.clone();
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

/// Each enemy's bars over its head: vitality above posture, the same width (6.8 % of the screen),
/// 5 and 4 px tall at 1080p with a 2 px gap (the all-bosses clip, Folding Screen Monkeys 30:59);
/// an enemy with deathblows (NpcParam ninsatuNum) has its red dots just over the bar's left end
/// (the user's capture: one dot, MENU_HP_Boss lit / MENU_HP_Boss_base spent). The posture bar
/// shows only while the posture has damage.
fn spawn_enemy_bars(commands: &mut Commands, assets: &AssetServer, enemy: Entity) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Vw(OVERHEAD_W), margin: UiRect::left(Val::Vw(-OVERHEAD_W / 2.0)), flex_direction: FlexDirection::Column, row_gap: Val::Vh(0.3), ..default() },
            Visibility::Hidden,
            EnemyRoot(enemy),
        ))
        .with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Vw(0.15), margin: UiRect { left: Val::Vw(-0.2), bottom: Val::Vh(0.1), ..default() }, ..default() }).with_children(|r| {
                for i in 0..3 {
                    r.spawn((Node { width: Val::Vw(1.15), height: Val::Vw(1.15), display: Display::None, ..default() }, ImageNode::new(assets.load("hud/MENU_HP_Boss.png")), EnemyDot(enemy, i)));
                }
            });
            framed(c, Val::Percent(100.0), Val::Vh(0.75), false, (BackgroundColor(VITALITY), EnemyHpFill(enemy)), None::<()>);
            c.spawn((Node { width: Val::Percent(100.0), ..default() }, Visibility::Hidden, EnemyPostureRow(enemy))).with_children(|r| {
                framed(r, Val::Percent(100.0), Val::Vh(0.6), true, (BackgroundColor(Color::srgb(1.0, 0.7, 0.15)), EnemyPostureFill(enemy)), None::<()>);
            });
        });
}

/// The over-head bars' width (vw): bigger than the clip's 6.8 (the user: "too small").
const OVERHEAD_W: f32 = 8.5;
/// How far over the head bone the bars stand (m).
const OVERHEAD_ABOVE_HEAD: f32 = 0.35;

/// An over-head deathblow dot (enemy, index).
#[derive(Component)]
struct EnemyDot(Entity, u32);
/// The over-head posture bar's row (shown while it has damage).
#[derive(Component)]
struct EnemyPostureRow(Entity);

#[allow(clippy::type_complexity)]
fn update_enemy_hud(
    mut commands: Commands,
    assets: Res<AssetServer>,
    combat: Res<Combat>,
    mut dots: Query<(&EnemyDot, &mut Node, &mut ImageNode), (Without<EnemyRoot>, Without<EnemyHpFill>, Without<EnemyPostureFill>)>,
    mut posture_rows: Query<(&EnemyPostureRow, &mut Visibility), Without<EnemyRoot>>,
    skeletons: Query<&crate::anim::Skeleton>,
    bones: Query<(&Name, &GlobalTransform)>,
    mut heads: Local<std::collections::HashMap<Entity, Entity>>,
    enemies: Query<(Entity, &Actor, &crate::enemy::Enemy, &GlobalTransform, Option<&crate::enemy::Phantom>)>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    mut roots: Query<(Entity, &EnemyRoot, &mut Node, &mut Visibility), (Without<EnemyHpFill>, Without<EnemyPostureFill>)>,
    mut hp: Query<(&EnemyHpFill, &mut Node), (Without<EnemyRoot>, Without<EnemyPostureFill>)>,
    mut posture: Query<(&EnemyPostureFill, &mut Node, &mut BackgroundColor), (Without<EnemyRoot>, Without<EnemyHpFill>)>,
    script: Option<Res<crate::enemy::BossScript>>,
) {
    let scripted = scripted_bar(script.as_deref());
    // A set of bars per enemy: new enemies get theirs, removed ones lose them.
    let have: HashSet<Entity> = roots.iter().map(|(_, r, ..)| r.0).collect();
    for (e, ..) in &enemies {
        if !have.contains(&e) {
            spawn_enemy_bars(&mut commands, &assets, e);
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
        // Not the boss bar's (named, or two deathblows or more: update_boss_hud).
        let boss = e.ninsatsu.1 >= 2 || !boss_name(combat.foe_of(a).npc_row).is_empty();
        let show = !boss && scripted != Some(root.0) && !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()) && (e.targeting.state >= crate::stealth::FIND || a.posture > 0.0 || a.hp < a.hp_max);
        // Over the enemy's own head (its skeleton's Head bone), whatever its height (the user:
        // "the position should be adjusted by height"); without a skeleton, 1.9 m up.
        if !heads.contains_key(&root.0) {
            if let Some(h) = skeletons.get(root.0).ok().and_then(|sk| sk.bones.iter().copied().find(|b| bones.get(*b).is_ok_and(|(n, _)| n.as_str() == "Head"))) {
                heads.insert(root.0, h);
            }
        }
        let top = heads.get(&root.0).and_then(|h| bones.get(*h).ok()).map(|(_, hg)| hg.translation().with_x(g.translation().x).with_z(g.translation().z) + Vec3::Y * OVERHEAD_ABOVE_HEAD).unwrap_or(g.translation() + Vec3::Y * 1.9);
        let at = cam.and_then(|(cam, ct)| cam.world_to_viewport(ct, top).ok());
        let Some(p) = at.filter(|_| show) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        node.left = Val::Px(p.x);
        // The bars' bottom at that point.
        node.top = Val::Px(p.y - 26.0);
    }
    for (f, mut n) in &mut hp {
        if let Ok((_, a, ..)) = enemies.get(f.0) {
            n.width = Val::Percent((a.hp / a.hp_max).clamp(0.0, 1.0) * 100.0);
        }
    }
    for (d, mut n, mut img) in &mut dots {
        if let Ok((_, _, e, ..)) = enemies.get(d.0) {
            let display = if d.1 < e.ninsatsu.1 { Display::Flex } else { Display::None };
            if n.display != display {
                n.display = display;
            }
            let file = if d.1 < e.ninsatsu_left() { "hud/MENU_HP_Boss.png" } else { "hud/MENU_HP_Boss_base.png" };
            if img.image.path().is_none_or(|p| !p.to_string().ends_with(&file[4..])) {
                img.image = assets.load(file);
            }
        }
    }
    for (r, mut v) in &mut posture_rows {
        if let Ok((_, a, ..)) = enemies.get(r.0) {
            let want = if a.posture > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
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

/// The item cluster, bottom right, after the user's captures of the game (1273 x 717): the quick
/// item on its brush box (SB_FE MENU_Item_Left; x 79.1-83.3 %, y 87.2-94.4 %) with its count in
/// the corner, the prosthetic tool on the bigger one (MENU_Item_Top; x 84.2-90.5 %, y 83.1-94.4 %),
/// and a column (x 91.4 %): the combat art's mark (MENU_FE_Skill), the ninjutsu's
/// (MENU_FE_Ninsatul, shown with one equipped), the Spirit Emblems with their count.
fn spawn_item_icons(mut commands: Commands, mut fonts: ResMut<Assets<Font>>, assets: Res<AssetServer>) {
    let serif = std::fs::read("C:/Windows/Fonts/georgia.ttf").ok().map(|b| fonts.add(Font::from_bytes(b)));
    let img = |f: &str| -> (ImageNode, Visibility) {
        let file = ["hud", "menu_sprites/sb_fe"].iter().map(|d| format!("{d}/{f}.png")).find(|p| crate::paths::extracted().join(p).exists());
        match file {
            Some(p) => {
                let mut i = ImageNode::new(assets.load(p));
                i.image_mode = bevy::ui::widget::NodeImageMode::Stretch;
                (i, Visibility::Inherited)
            }
            None => (ImageNode::default(), Visibility::Hidden),
        }
    };
    let at = |l: f32, t: f32, w: f32, h: f32| Node { position_type: PositionType::Absolute, left: Val::Percent(l), top: Val::Percent(t), width: Val::Percent(w), height: Val::Percent(h), ..default() };
    let inner = || Node { position_type: PositionType::Absolute, left: Val::Percent(10.0), top: Val::Percent(10.0), width: Val::Percent(80.0), height: Val::Percent(80.0), ..default() };
    let number = |px: f32, which: u8| {
        (
            Text::new(""),
            TextFont { font: serif.clone().map(bevy::text::FontSource::Handle).unwrap_or_default(), font_size: bevy::text::FontSize::Px(px), ..default() },
            TextColor(Color::srgba(0.92, 0.88, 0.8, 1.0)),
            TextShadow { offset: Vec2::new(1.5, 1.5), color: Color::srgba(0.0, 0.0, 0.0, 1.0) },
            ItemText(which),
        )
    };
    // The quick item in hand (the Healing Gourd, iconId 151, to start; items.rs) and its count.
    commands.spawn((at(79.1, 87.2, 4.2, 7.3), img("MENU_Item_Left").0)).with_children(|b| {
        let (i, v) = img("MENU_ItemIcon_00151");
        b.spawn((inner(), i, v, QuickIcon));
        b.spawn(Node { position_type: PositionType::Absolute, right: Val::Percent(2.0), bottom: Val::Percent(-4.0), ..default() }).with_children(|c| {
            c.spawn(number(17.0, 1));
        });
    });
    // The prosthetic tool.
    commands.spawn((at(84.2, 83.1, 6.3, 11.3), img("MENU_Item_Top").0)).with_children(|b| {
        b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(6.0), top: Val::Percent(6.0), width: Val::Percent(88.0), height: Val::Percent(88.0), ..default() }, ImageNode::default(), Visibility::Hidden, ItemIcon(true)));
    });
    // The column: the art's mark, the ninjutsu's, the Spirit Emblems.
    let (i, v) = img("MENU_FE_Skill");
    commands.spawn((at(91.4, 83.3, 1.9, 3.2), i, v, ArtMark));
    let (i, _) = img("MENU_FE_Ninsatul");
    commands.spawn((at(91.4, 87.3, 1.9, 3.1), i, Visibility::Hidden, NinjutsuMark));
    let (i, v) = img("MENU_ItemIcon_00001");
    commands.spawn((at(91.4, 91.4, 1.9, 3.2), i, v));
    commands.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(93.5), top: Val::Percent(91.0), ..default() }).with_children(|c| {
        c.spawn(number(17.0, 0));
    });
}

/// The combat art's mark (shown with an art equipped) and the ninjutsu's (gap: no ninjutsu yet).
#[derive(Component)]
struct ArtMark;
#[derive(Component)]
struct NinjutsuMark;

/// The Sen purse and the skill gauge, which the game shows only for a while after they change
/// (the user's captures: neither on the plain HUD).
#[derive(Component)]
struct Flash(u8);
#[derive(Component)]
struct SenText;
/// The skill gauge's point count and experience fill.
#[derive(Component)]
struct PointsText;
#[derive(Component)]
struct ExpFill;
/// "+N": the Sen (0) or experience (1) the kills since the purse / gauge came up gave.
#[derive(Component)]
struct GainText(u8);

/// How long the purse / gauge stay up after a change (s). gap: the movie's fade is not read.
const FLASH_SHOW: f32 = 4.0;

#[allow(clippy::too_many_arguments)]
fn flash_counters(
    time: Res<Time>,
    config: Res<crate::config::GameConfig>,
    mut last: Local<Option<(u32, (u32, u32))>>,
    mut left: Local<[f32; 2]>,
    mut q: Query<(&Flash, &mut Visibility)>,
    mut texts: Query<&mut Text, (With<SenText>, Without<PointsText>, Without<GainText>)>,
    mut points: Query<&mut Text, (With<PointsText>, Without<GainText>)>,
    (earned, mut base, mut gains): (Res<crate::experience::Earned>, Local<Option<([u32; 2], [u32; 2])>>, Query<(&mut Text, &GainText)>),
    mut fill: Query<&mut Node, With<ExpFill>>,
    mut art: Query<&mut Visibility, (With<ArtMark>, Without<Flash>)>,
) {
    let sen = config.player.sen;
    let exp = (config.player.skill_exp, config.player.skill_points);
    // A kill's gain shows as "+N"; more kills while the counter is up add to it. While it is
    // hidden the base follows the totals, so it starts over (prev: last frame's totals).
    let now = [earned.sen, earned.exp];
    let (from, prev) = base.get_or_insert((now, now));
    for i in 0..2 {
        if left[i] == 0.0 {
            from[i] = prev[i];
        }
        if now[i] != prev[i] {
            left[i] = FLASH_SHOW;
        }
    }
    *prev = now;
    for (mut t, g) in &mut gains {
        let n = now[g.0 as usize].wrapping_sub(from[g.0 as usize]);
        let s = if n > 0 { format!("+{n}") } else { String::new() };
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Some((s, x)) = *last {
        if s != sen {
            left[0] = FLASH_SHOW;
        }
        if x != exp {
            left[1] = FLASH_SHOW;
        }
    }
    *last = Some((sen, exp));
    for l in left.iter_mut() {
        *l = (*l - time.delta_secs()).max(0.0);
    }
    for (f, mut v) in &mut q {
        // 0: the purse; 1: the skill gauge.
        let want = if left[f.0 as usize] > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    for mut t in &mut texts {
        let s = sen.to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    for mut t in &mut points {
        let s = config.player.skill_points.to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    let w = Val::Percent(100.0 * crate::experience::fill(&config.player).clamp(0.0, 1.0));
    for mut n in &mut fill {
        if n.width != w {
            n.width = w;
        }
    }
    for mut v in &mut art {
        let want = if config.player.combat_art > 0 { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// The quick item's icon follows the item in hand (its EquipParamGoods iconId; none when the
/// slot is empty).
fn update_quick_icon(
    mut shown: Local<Option<i64>>,
    assets: Res<AssetServer>,
    config: Res<crate::config::GameConfig>,
    player: Query<&crate::player::Player>,
    mut q: Query<(&mut ImageNode, &mut Visibility), With<QuickIcon>>,
) {
    let goods = player.iter().next().map_or(0, |p| crate::items::in_slot(&config, p.quick_slot));
    if *shown == Some(goods) {
        return;
    }
    *shown = Some(goods);
    let icon = if goods == crate::items::GOURD { 151 } else { crate::items::param("EquipParamGoods", goods)["iconId"].as_i64().unwrap_or(-1) };
    let file = crate::game_menu::any_icon(icon).filter(|_| goods > 0 && icon >= 0);
    for (mut img, mut vis) in &mut q {
        match &file {
            Some(f) => {
                img.image = assets.load(f.clone());
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
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
        // A kneeling boss is dead here too, but its Todome / finisher window (data.rs
        // finisher_window) takes the deathblow, so it keeps the mark (the user, 2026-10-16: "no
        // red dot on third deathblow").
        if e.is_dead() && !combat.data_of(ea).finisher_open(&ea.anim, ea.t) {
            return None;
        }
        let dmy = |id: i16| dm.and_then(|dm| dm.0.get(&id)).and_then(|m| dummies.get(*m).ok()).map(|g| g.translation());
        let db = crate::player::deathblow_check(combat, wolf, wa.forward(), wa.aged.is_some(), ea, e, eg.translation(), &dmy)?;
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
        "LMB/J attack (hold = thrust)  RMB/K deflect  LMB+RMB art  Shift step (hold = sprint)  Space jump  MMB lock-on  Alt walk  Q crouch  Ctrl prosthetic  Z switch tool  F grapple  R quick item  C next item  E hug/hang  X sheathe/draw  LMB (dead) resurrect  T enemy AI on/off  H hurtboxes  Backspace reset  F5 reload config  F1 debug menu\n\n",
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
/// An enemy off the screen (or behind the camera) gets the same mark beside Wolf instead, turned
/// so its tip points the way to it, as if seen from above: behind the camera it sits below him
/// (the user's captures). gap: the game's ring radius is not read from the movie.
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
const STEALTH_PX: f32 = 28.07 * STEALTH_SCALE;
const STEALTH_LIFT: f32 = 35.5 * STEALTH_SCALE;
/// Larger and more solid than the movie's (the user: "a bit too small and too transparent").
const STEALTH_SCALE: f32 = 1.6;

/// The off-screen mark's distance from Wolf's chest at 1080p [px].
const STEALTH_RING: f32 = 120.0;

/// The off-screen mark's place and turn: from Wolf's screen point `wolf`, the way to the enemy
/// (`to`, world) as seen from above with the camera's forward up the screen; the rotation turns
/// the downward tip towards it (UiTransform turns clockwise). None when it is right above him.
fn ring_mark(wolf: Vec2, to: Vec3, cam_right: Vec3, cam_fwd: Vec3, ring: f32) -> Option<(Vec2, f32)> {
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
    let (to, right, fwd) = (flat(to), flat(cam_right), flat(cam_fwd));
    let v = Vec2::new(to.dot(right), -to.dot(fwd)).normalize_or_zero();
    (v != Vec2::ZERO).then(|| (wolf + v * ring, f32::atan2(-v.x, v.y)))
}

/// Seconds the red "found" mark stays once the fight starts.
const FOUND_SHOW: f32 = 1.5;

fn spawn_stealth(commands: &mut Commands, assets: &AssetServer, enemy: Entity) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(STEALTH_PX), height: Val::Px(STEALTH_PX), ..default() },
            ImageNode::new(assets.load("hud/MENU_Find_02.png")),
            UiTransform::IDENTITY,
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
    wolf: Query<&GlobalTransform, (With<crate::player::Player>, Without<crate::enemy::Enemy>)>,
    mut marks: Query<(Entity, &mut StealthMark, &mut Node, &mut ImageNode, &mut UiTransform, &mut Visibility), (Without<StealthFill>, Without<StealthBrush>)>,
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
    let view = window.single().map_or(Vec2::new(1280.0, 720.0), |w| w.size());
    let chest = wolf.single().ok().map(|g| g.translation() + Vec3::Y * 0.3);
    for (me, mut m, mut node, mut frame, mut turn, mut vis) in &mut marks {
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
        // (Every enemy, the ones with deathblow dots too: spawn_one counts at least 1, so the
        // old "no dots" test hid them all; the game marks the minibosses as well.)
        let show = !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()) && (level > 0 || ratio > 0.0) && m.found_t < FOUND_SHOW;
        // Anchored on the lock-on dummy 220 (root + 1.4 m without it), the mark's centre
        // STEALTH_LIFT px above it. gap: the game's anchor for this mark is not traced.
        let head = dm.and_then(|d| d.0.get(&220)).and_then(|d| dummies.get(*d).ok()).map_or(etf.translation() + Vec3::Y * 1.4, |g| g.translation());
        let on_screen = cam.and_then(|(cam, ctf)| cam.world_to_viewport(ctf, head).ok()).filter(|p| p.x >= 0.0 && p.y >= 0.0 && p.x <= view.x && p.y <= view.y);
        let (pos, angle) = match (on_screen, cam, chest) {
            (Some(p), ..) => (Some(p), 0.0),
            (None, Some((cam, ctf)), Some(chest)) => match cam.world_to_viewport(ctf, chest).ok().and_then(|w| ring_mark(w, head - chest, ctf.right().into(), ctf.forward().into(), STEALTH_RING * k)) {
                // The ring mark sits on its centre (no lift over a head).
                Some((p, a)) => (Some(p + Vec2::Y * STEALTH_LIFT * k), a),
                None => (None, 0.0),
            },
            _ => (None, 0.0),
        };
        let pos = pos.filter(|_| show);
        if pos.is_some() {
            turn.rotation = Rot2::radians(angle);
        }
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
            2 => (1.0, rgb(163, 64, 57).with_alpha(0.85), rgb(200, 60, 50).with_alpha(0.95)),
            1 => (1.0, rgb(154, 134, 70).with_alpha(0.95), Color::srgb(0.719, 0.629, 0.238)),
            _ => (ratio, Color::BLACK.with_alpha(0.65), Color::WHITE.with_alpha(0.95)),
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
mod ring_tests {
    use bevy::prelude::*;

    /// With the camera looking down -Z: an enemy ahead puts the mark above Wolf pointing up, one
    /// behind the camera below him pointing down, one to the right to his right.
    #[test]
    fn the_off_screen_mark_points_the_way_to_the_enemy() {
        let (right, fwd) = (Vec3::X, Vec3::NEG_Z);
        let (p, a) = super::ring_mark(Vec2::ZERO, Vec3::NEG_Z * 5.0, right, fwd, 100.0).unwrap();
        assert!(p.abs_diff_eq(Vec2::new(0.0, -100.0), 1e-3) && (a.abs() - std::f32::consts::PI).abs() < 1e-3, "{p} {a}");
        let (p, a) = super::ring_mark(Vec2::ZERO, Vec3::Z * 5.0, right, fwd, 100.0).unwrap();
        assert!(p.abs_diff_eq(Vec2::new(0.0, 100.0), 1e-3) && a.abs() < 1e-3, "{p} {a}");
        let (p, a) = super::ring_mark(Vec2::ZERO, Vec3::X * 5.0, right, fwd, 100.0).unwrap();
        assert!(p.abs_diff_eq(Vec2::new(100.0, 0.0), 1e-3), "{p}");
        // The tip (down when unturned) turned clockwise by a: down -> right is counter-clockwise.
        let tip = Vec2::new(-a.sin(), a.cos());
        assert!(tip.abs_diff_eq(Vec2::X, 1e-3), "{tip}");
        assert!(super::ring_mark(Vec2::ZERO, Vec3::Y, right, fwd, 100.0).is_none());
    }
}

#[cfg(test)]
mod mark_tests {
    /// The FE clips: the lock mark shrinks in from 3x while fading in over 15 frames, then pulses
    /// 1 -> 0.74 -> 0.99 every 68 frames, and fades out in 5; the deathblow mark fades in over 11
    /// frames, holds, then pulses down to 0.60.
    /// The trail holds 0.5 s from the first hit (a later hit does not restart it), then is gone
    /// within 0.05 s (the all-bosses clip at 60 fps).
    #[test]
    fn the_damage_trail_holds_then_collapses() {
        let dt = 1.0 / 60.0;
        let mut t = (1.0, 0.0);
        super::step_trail(&mut t, 0.9, dt);
        for i in 0..28 {
            // A second hit at 0.3 s.
            super::step_trail(&mut t, if i < 17 { 0.9 } else { 0.8 }, dt);
        }
        assert_eq!(t.0, 1.0, "still held at 0.48 s");
        for _ in 0..4 {
            super::step_trail(&mut t, 0.8, dt);
        }
        assert_eq!(t.0, 0.8, "gone by 0.55 s, though the second hit came at 0.3 s");
    }

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

#[cfg(test)]
mod posture_tests {
    use bevy::prelude::*;

    /// The posture fill switches at 70 % and full (FUN_140a02a10's 30 % left) to the HUD movie's
    /// state colours, from dark gold to orange.
    #[test]
    fn posture_bar_follows_the_game_states() {
        assert_eq!([0.0, 0.69, 0.7, 0.99, 1.0].map(super::posture_state), [0, 0, 1, 1, 2]);
        assert_eq!(super::posture_color(0.5), Color::srgb_u8(142, 123, 60));
        assert_eq!(super::posture_color(1.0), Color::srgb_u8(168, 107, 0));
    }
}

/// `SHINOBI_HUD_POSTURE=<0-1>`: every fighter's posture held at that fill (SHINOBI_SHOT captures of
/// the posture bars).
#[derive(Resource)]
struct HoldPosture(f32);

fn hold_posture(f: Res<HoldPosture>, mut actors: Query<&mut Actor>) {
    for mut a in &mut actors {
        a.posture = a.posture_max * f.0;
    }
}
