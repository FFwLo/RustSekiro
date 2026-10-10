//! HUD: Sekiro-style posture bars (fill from the centre outward), HP bars,
//! a combat log, and a debug panel showing the live TAE state so timing can
//! be checked frame by frame against the real game.

use bevy::prelude::*;

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
#[derive(Component)]
struct ItemText;
#[derive(Component)]
struct EnemyRoot;
#[derive(Component)]
struct EnemyHpFill;

#[derive(Component)]
struct DebugText;

#[derive(Component)]
struct LogText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatLog>()
            .init_resource::<Perilous>()
            .add_systems(Startup, (spawn_hud, spawn_perilous, spawn_stealth, spawn_deathblow_mark))
            .add_systems(Update, (update_player_hud, update_enemy_hud, update_debug, update_log, debug_only, update_perilous, update_stealth, update_deathblow_mark));
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

fn spawn_hud(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
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
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                for i in 0..2 {
                    r.spawn((
                        Node { width: Val::Px(13.0), height: Val::Px(13.0), border: UiRect::all(Val::Px(1.5)), border_radius: BorderRadius::MAX, ..default() },
                        BackgroundColor(VITALITY),
                        BorderColor::all(EDGE),
                        ResNode(i),
                    ));
                }
            });
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
    // The enemy's bars over its head: vitality (short, left) above posture (centred).
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(170.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() },
            Visibility::Hidden,
            EnemyRoot,
        ))
        .with_children(|c| {
            framed(c, 96.0, 5.0, false, (BackgroundColor(VITALITY), EnemyHpFill), None::<()>);
            framed(c, 170.0, 7.0, true, (BackgroundColor(Color::srgb(1.0, 0.7, 0.15)), PostureFill(Side::Enemy)), None::<()>);
        });
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
    mut nodes: Query<(&ResNode, &mut BackgroundColor), Without<PostureFill>>,
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
    for (rn, mut bg) in &mut nodes {
        bg.0 = if rn.0 < p.resurrections { VITALITY } else { Color::srgba(0.1, 0.08, 0.07, 0.7) };
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

#[allow(clippy::type_complexity)]
fn update_enemy_hud(
    enemies: Query<(&Actor, &crate::enemy::Enemy, &GlobalTransform)>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    mut root: Query<(&mut Node, &mut Visibility), (With<EnemyRoot>, Without<EnemyHpFill>, Without<PostureFill>)>,
    mut hp: Query<&mut Node, (With<EnemyHpFill>, Without<EnemyRoot>, Without<PostureFill>)>,
    mut posture: Query<(&PostureFill, &mut Node, &mut BackgroundColor), (Without<EnemyRoot>, Without<EnemyHpFill>)>,
) {
    let Ok((mut node, mut vis)) = root.single_mut() else { return };
    let Some((a, e, g)) = enemies.iter().find(|(_, e, _)| !e.is_dead()) else {
        *vis = Visibility::Hidden;
        return;
    };
    // Shown once it is fighting or hurt, over its head.
    let show = e.targeting.state >= crate::stealth::FIND || a.posture > 0.0 || a.hp < a.hp_max;
    let at = camera.single().ok().and_then(|(cam, ct)| cam.world_to_viewport(ct, g.translation() + Vec3::Y * 0.85).ok());
    let Some(p) = at.filter(|_| show) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    node.left = Val::Px(p.x - 85.0);
    node.top = Val::Px(p.y - 20.0);
    for mut n in &mut hp {
        n.width = Val::Percent((a.hp / a.hp_max).clamp(0.0, 1.0) * 100.0);
    }
    let pf = (a.posture / a.posture_max).clamp(0.0, 1.0);
    for (pfl, mut n, mut bg) in &mut posture {
        if pfl.0 == Side::Enemy {
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

fn spawn_deathblow_mark(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(100.0), height: Val::Px(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            ImageNode::new(assets.load("hud/MENU_ninsatu_02.png")),
            Visibility::Hidden,
            DeathblowMark,
        ))
        .with_children(|c| {
            c.spawn((Node { width: Val::Percent(32.0), height: Val::Percent(32.0), ..default() }, ImageNode::new(assets.load("hud/MENU_ninsatu_01.png"))));
        });
}

#[allow(clippy::type_complexity)]
fn update_deathblow_mark(
    time: Res<Time>,
    combat: Res<Combat>,
    wolf: Query<(&Actor, &GlobalTransform), With<crate::player::Player>>,
    enemies: Query<(&Actor, &crate::enemy::Enemy, &GlobalTransform, Option<&crate::model::Dummies>), Without<crate::player::Player>>,
    dummies: Query<&GlobalTransform>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    window: Query<&Window>,
    mut mark: Query<(&mut Node, &mut Visibility), With<DeathblowMark>>,
) {
    let Ok((mut node, mut vis)) = mark.single_mut() else { return };
    *vis = Visibility::Hidden;
    let (Ok((wa, wg)), Ok((cam, ct))) = (wolf.single(), camera.single()) else { return };
    // Gone once the deathblow starts (Deathblow, DeathblowStart, PlungeDeathblow).
    if wa.state.contains("Deathblow") {
        return;
    }
    let h = window.single().map_or(720.0, |w| w.height());
    for (ea, e, eg, dm) in &enemies {
        // Not while it is being thrown (any ThrowDef anim: deathblows, plunges, kick-downs).
        if e.is_dead() || ea.state.starts_with("ThrowDef") {
            continue;
        }
        let Some(db) = crate::player::deathblow_check(&combat, wg.translation(), ea, e, eg.translation()) else { continue };
        if !e.is_broken() && !db.in_reach {
            continue;
        }
        let at = dm.and_then(|d| d.0.get(&220)).and_then(|d| dummies.get(*d).ok()).map_or(eg.translation() + Vec3::Y * 1.4, |g| g.translation());
        let Ok(p) = cam.world_to_viewport(ct, at) else { continue };
        let dist = ct.translation().distance(at).max(0.5);
        let pulse = 1.0 + 0.06 * (time.elapsed_secs() * std::f32::consts::TAU * 1.2).sin();
        let size = h * 0.24 * (4.5 / dist).clamp(0.45, 1.3) * pulse;
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        node.left = Val::Px(p.x - size / 2.0);
        node.top = Val::Px(p.y - size / 2.0);
        *vis = Visibility::Inherited;
        return;
    }
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
        let d = data_for(&combat, a.side);
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
            if !d.attack_windows(&a.anim).iter().all(|(e, _, _)| !e.active(a.t)) { tags.push("[HITBOX]"); }
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
/// it has found him. gap: the real indicator art (menu textures) is not used.
#[derive(Component)]
struct StealthMark;
#[derive(Component)]
struct StealthFill;

/// Seconds the red "found" mark stays once the fight starts.
const FOUND_SHOW: f32 = 1.5;

fn spawn_stealth(mut commands: Commands) {
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: Val::Px(54.0), height: Val::Px(7.0), border: UiRect::all(Val::Px(1.0)), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            Visibility::Hidden,
            StealthMark,
        ))
        .with_children(|c| {
            c.spawn((Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::NONE), StealthFill));
        });
}

fn update_stealth(
    time: Res<Time>,
    mut found_t: Local<f32>,
    enemies: Query<(&crate::enemy::Enemy, &GlobalTransform)>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::OrbitCamera>>,
    mut mark: Single<(&mut Node, &mut Visibility), (With<StealthMark>, Without<StealthFill>)>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<StealthFill>>,
) {
    let Some((e, etf)) = enemies.iter().find(|(e, _)| !e.is_dead()) else {
        *mark.1 = Visibility::Hidden;
        return;
    };
    let (level, ratio) = e.targeting.hud();
    if e.targeting.state == crate::stealth::BATTLE {
        *found_t += time.delta_secs();
    } else {
        *found_t = 0.0;
    }
    let show = (level > 0 || ratio > 0.0) && *found_t < FOUND_SHOW;
    let pos = camera.single().ok().and_then(|(cam, ctf)| cam.world_to_viewport(ctf, etf.translation() + Vec3::Y * 1.0).ok());
    let Some(p) = pos.filter(|_| show) else {
        *mark.1 = Visibility::Hidden;
        return;
    };
    *mark.1 = Visibility::Inherited;
    mark.0.left = Val::Px(p.x - 27.0);
    mark.0.top = Val::Px(p.y - 4.0);
    let (w, color) = match level {
        2 => (1.0, Color::srgb(0.9, 0.12, 0.08)),
        1 => (1.0, Color::srgb(1.0, 0.72, 0.12)),
        _ => (ratio, Color::srgb(1.0, 0.82, 0.3)),
    };
    fill.0.width = Val::Percent(w * 100.0);
    fill.1.0 = color;
}
