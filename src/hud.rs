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

#[derive(Component)]
pub struct Spark {
    age: f32,
}

impl Spark {
    pub fn bundle(pos: Vec3, color: Color) -> impl Bundle {
        (Spark { age: 0.0 }, SparkColor(color), Transform::from_translation(pos).with_scale(Vec3::splat(0.1)))
    }
}

#[derive(Component)]
struct SparkColor(Color);

#[derive(Component)]
struct Bar {
    side: Side,
    posture: bool,
}

#[derive(Component)]
struct DebugText;

#[derive(Component)]
struct LogText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatLog>()
            .init_resource::<Perilous>()
            .add_systems(Startup, (spawn_hud, spawn_perilous))
            .add_systems(Update, (update_bars, update_debug, update_log, animate_sparks, update_perilous));
    }
}

fn bar(parent: &mut ChildSpawnerCommands, side: Side, posture: bool, width: f32, color: Color) {
    parent
        .spawn((
            Node { width: Val::Px(width), height: Val::Px(if posture { 10.0 } else { 6.0 }), margin: UiRect::all(Val::Px(3.0)), justify_content: if posture { JustifyContent::Center } else { JustifyContent::FlexStart }, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|b| {
            b.spawn((Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(color), Bar { side, posture }));
        });
}

fn spawn_hud(mut commands: Commands, combat: Res<crate::data::Combat>) {
    let name = match combat.foe.chr.as_str() {
        "c1010" => "Ochimusha",
        _ => "Ashina Samurai General",
    };
    // Enemy bars, top centre.
    commands
        .spawn(Node { position_type: PositionType::Absolute, top: Val::Px(14.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() })
        .with_children(|c| {
            c.spawn((Text::new(name), TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() }));
            bar(c, Side::Enemy, false, 260.0, Color::srgb(0.75, 0.15, 0.15));
            bar(c, Side::Enemy, true, 360.0, Color::srgb(1.0, 0.6, 0.1));
        });
    // Player bars, bottom centre.
    commands
        .spawn(Node { position_type: PositionType::Absolute, bottom: Val::Px(18.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() })
        .with_children(|c| {
            bar(c, Side::Player, true, 360.0, Color::srgb(1.0, 0.6, 0.1));
            bar(c, Side::Player, false, 260.0, Color::srgb(0.75, 0.15, 0.15));
        });
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() },
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(10.0), ..default() },
        DebugText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(16.0), ..default() },
        Node { position_type: PositionType::Absolute, bottom: Val::Px(70.0), left: Val::Px(10.0), ..default() },
        LogText,
    ));
}

fn update_bars(actors: Query<&Actor>, mut bars: Query<(&Bar, &mut Node, &mut BackgroundColor)>) {
    for (bar, mut node, mut bg) in &mut bars {
        let Some(a) = actors.iter().find(|a| a.side == bar.side) else { continue };
        let f = if bar.posture { (a.posture / a.posture_max).min(1.0) } else { a.hp / a.hp_max };
        node.width = Val::Percent((f * 100.0).clamp(0.0, 100.0));
        if bar.posture {
            // Exe FUN_140a02a10 posture display state: "danger" when <= 30 % of posture is
            // left (DAT_143b06724 = 30), i.e. the bar 70 % full: red; else yellow -> orange.
            bg.0 = if f >= 0.7 { Color::srgb(0.95, 0.1, 0.05) } else { Color::srgb(1.0, 0.85 - 0.5 * f, 0.15) };
        }
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

fn animate_sparks(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut Spark, &mut Transform, &SparkColor, Option<&Mesh3d>)>,
) {
    for (e, mut s, mut tf, c, mesh) in &mut q {
        if mesh.is_none() {
            commands.entity(e).insert((
                Mesh3d(meshes.add(Sphere::new(1.0))),
                MeshMaterial3d(materials.add(StandardMaterial { base_color: c.0, emissive: c.0.to_linear() * 8.0, unlit: true, ..default() })),
            ));
        }
        s.age += time.delta_secs();
        tf.scale = Vec3::splat(0.1 + s.age * 2.5);
        if s.age > 0.15 {
            commands.entity(e).despawn();
        }
    }
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
