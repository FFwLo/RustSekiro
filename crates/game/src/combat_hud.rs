//! HP and posture bars laid out like Sekiro's HUD. Wolf's vitality sits at the bottom left, its
//! length following his maximum vitality, with a light damage trail behind the fill; his posture
//! bar sits at the bottom centre and shows only while it has damage. A plain enemy's vitality
//! and posture float over its Head bone, with its deathblow dots over the bar's left end; a boss
//! bar (one the map events put up) has its vitality at the top left with the deathblow orbs over
//! it and the name under it, and its posture at the top centre. Posture bars grow from the centre
//! outward in the game's state colours, with an arrow at each end while they have damage. The
//! item cluster (quick item and prosthetic tool) sits at the bottom right. Above Wolf's
//! vitality, a purple bar shows poison build-up and a line names the status effects on him
//! (poisoned, recovery sealed, ...) with the seconds left. A red 危 marks a perilous attack's
//! warning on Wolf (see [`update_perilous`]). Over the item cluster, Wolf's skill points with
//! the experience gauge and his Sen show for a few seconds after a kill's reward, as the game
//! shows them there (see [`update_reward`]).
//!
//! Layout, colours and timings adapted from Fireguy32's RustSekiro fork (MIT), commit d11ef65,
//! which measured them from captures of the game's HUD. The game's own HUD sprites (SB_FE,
//! SB_Boss) are not extracted, so plain framed nodes stand in for them.

use bevy::prelude::*;
use sekiro_sim::deathblow::MAX_BAR_MARKS;

use crate::brain_overlay::{PANEL_INSET, PANEL_WIDTH};
use crate::change::set;
use crate::game_ui::{GameFonts, MenuArt};

/// Logical HUD viewport, shared by projected HUD elements when the camera is unavailable.
pub(crate) fn viewport_size(camera: Option<&Camera>) -> Vec2 {
    camera
        .and_then(Camera::logical_viewport_size)
        .unwrap_or(Vec2::new(1920.0, 1080.0))
}

/// What the bars show, filled by the simulation's owner every frame.
#[derive(Resource, Default, Clone)]
pub struct Gauges {
    /// HP left as a fraction of max (`Fighter::hp_fraction`), Wolf then the enemy.
    pub hp: [f32; 2],
    /// Wolf's maximum HP, which sets his vitality bar's length.
    pub wolf_max_hp: i32,
    /// Posture damage as a fraction of max (`Fighter::posture_fraction`).
    pub posture: [f32; 2],
    /// Whether each posture counts as broken (`Posture::broken`).
    pub broken: [bool; 2],
    /// Where the enemy's head is (its skeleton's Head bone), for its floating bars and the
    /// deathblow mark; only filled while one of them shows.
    pub enemy_head: Option<Vec3>,
    /// The enemy's bars are shown: it is alive and, in a boss fight run by the map events,
    /// the events show its health bar (the Owl's is hidden during his heat-up).
    pub enemy_alive: bool,
    /// A map-script boss bar stays on screen rather than following the character's head.
    pub boss_bar: bool,
    pub deathblow: bool,
    /// The enemy's name, under the boss bar.
    pub enemy_name: &'static str,
    /// Deathblow marks drawn over the enemy's vitality (`Duel::enemy_bar_marks`): none on an
    /// enemy taking a single deathblow.
    pub enemy_marks: i32,
    /// Wolf's poison build-up as a fraction of his resistance.
    pub poison: f32,
    /// Wolf's status effects, one line (empty for none).
    pub status: String,
    /// Seconds until the simulation restarts the scene after Wolf's death (`Duel::respawn_in`),
    /// `None` when no restart is coming.
    pub respawn_in: Option<f32>,
    /// Where the perilous-attack kanji is drawn: Wolf's dummy poly 220 (`Spine2`, where
    /// SpEffectVfxParam 48100 plays the game's kanji effect), lifted to above his head.
    pub perilous_at: Option<Vec3>,
    /// Seconds since a perilous warning last reached Wolf; `None` before the first.
    pub perilous_age: Option<f32>,
    /// The item cluster's buttons held this frame: quick item, then prosthetic tool.
    pub held: [bool; 2],
    /// Wolf's Sen (`Kit::sen`).
    pub sen: u32,
    /// Wolf's unspent skill points (`Kit::skill_points`).
    pub skill_points: u32,
    /// How full the experience gauge to the next skill point is (`Kit::xp_fill`).
    pub xp_fill: f32,
    /// Rewards earned so far (`Kit::rewards`): a change pops the reward panel.
    pub rewards: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Hp(usize),
    /// The light damage trail behind a vitality fill.
    Trail(usize),
    Posture(usize),
    /// Wolf's poison build-up.
    Poison,
    /// The experience gauge toward the next skill point.
    Xp,
}

/// The parts of the bars [`update`] changes each frame.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum HudPart {
    Fill(Fill),
    /// Wolf's vitality frame, whose length follows his maximum vitality.
    HpFrame,
    /// A posture bar's row, shown while the posture has damage (and [`SP_MIN_DISP_TIME`] after).
    PostureRow(usize),
    /// A posture bar's end arrow, shown while its posture has damage.
    PostureArrow(usize),
    /// The enemy's floating group.
    EnemyPanel,
    /// The boss bars' group.
    BossPanel,
    /// A deathblow dot (over the floating bars' left end) or orb (over the boss's vitality),
    /// by index.
    DeathblowDot(i32),
    /// The boss's name under its vitality.
    BossName,
    /// The deathblow mark on the enemy.
    DeathblowMark,
    /// An item cluster slot: 0 the quick item, 1 the prosthetic tool.
    ItemSlot(usize),
}

/// Wolf's status line.
#[derive(Component)]
pub struct StatusText;

/// The poison bar's frame, shown only while poison builds up.
#[derive(Component)]
pub struct PoisonRow;

/// Frame colours: dark lacquer with a thin gold edge, like the game's bars.
pub(crate) const FRAME: Color = Color::srgba(0.05, 0.04, 0.03, 0.75);
const EDGE: Color = Color::srgba(0.78, 0.66, 0.42, 0.55);
/// The HUD's cream text colour (labels, the boss name, the Sen count).
const CREAM: Color = Color::srgb(0.92, 0.88, 0.8);
/// The vitality fill: the game's muted orange-red (about 180, 70, 48 in captures).
pub(crate) const VITALITY: Color = Color::srgb(180.0 / 255.0, 70.0 / 255.0, 48.0 / 255.0);
/// The damage trail's light, desaturated red (about 165, 130, 120 in captures).
const TRAIL: Color = Color::srgb(165.0 / 255.0, 130.0 / 255.0, 120.0 / 255.0);
const POISON_WIDTH: f32 = 160.0;

/// Wolf's vitality bar width (percent of the screen width) at [`HP_BAR_BASE`] maximum
/// vitality (level 1); it grows in proportion, as the game's does with each Vitality level.
const HP_BAR_VW: f32 = 9.5;
const HP_BAR_BASE: f32 = 320.0;
/// The widest the vitality bar gets (ours: keeps a modded maximum on screen).
const HP_BAR_MAX_VW: f32 = 60.0;
/// The over-head bars' width (percent of the screen width).
pub(crate) const OVERHEAD_W: f32 = 8.5;
/// How far over the Head bone the floating bars stand (m).
const OVERHEAD_ABOVE_HEAD: f32 = 0.35;
/// The gap the floating bars keep from the recording panel (px, ours).
const OVERLAY_GAP: f32 = 12.0;
/// MenuParam SpMinimumDispTime: the least time a posture gauge stays up once its damage is gone.
const SP_MIN_DISP_TIME: f32 = 0.5;
/// Posture damage below this fraction counts as none: the gauge and its arrows hide.
const POSTURE_SHOWN: f32 = 0.005;
/// The damage trail's hold from the first hit (later hits do not restart it) and its collapse
/// time.
const TRAIL_HOLD: f32 = 0.5;
const TRAIL_COLLAPSE: f32 = 0.05;
/// The posture fill per state (normal, about to break, breakable, broken), from the HUD movie's
/// colour transforms on the posture bar.
const POSTURE_FILL: [[u8; 3]; 4] = [[142, 123, 60], [154, 116, 0], [168, 107, 0], [182, 96, 0]];

/// Wolf's vitality bar width, percent of the screen width, for `max_hp`.
pub fn hp_bar_vw(max_hp: i32) -> f32 {
    (HP_BAR_VW * max_hp.max(1) as f32 / HP_BAR_BASE).min(HP_BAR_MAX_VW)
}

/// The posture gauge's state for damage fraction `f`: 0 normal, 1 about to break (at most 30 %
/// of the posture left), 2 breakable (full), 3 broken.
fn posture_state(f: f32, broken: bool) -> usize {
    if broken {
        3
    } else if f >= 1.0 {
        2
    } else if f >= 0.7 {
        1
    } else {
        0
    }
}

pub(crate) fn posture_color(f: f32, broken: bool) -> Color {
    let [r, g, b] = POSTURE_FILL[posture_state(f, broken)];
    Color::srgb_u8(r, g, b)
}

/// Steps a damage trail `(shown, held)` toward the bar's fill `f`: it holds [`TRAIL_HOLD`]
/// from the first hit, then collapses onto the fill over [`TRAIL_COLLAPSE`].
fn step_trail(trail: &mut (f32, f32), f: f32, dt: f32) {
    let (shown, hold) = trail;
    if f < *shown {
        *hold += dt;
        if *hold > TRAIL_HOLD {
            let left = (TRAIL_HOLD + TRAIL_COLLAPSE - *hold + dt).max(dt);
            *shown = (*shown - (*shown - f) * (dt / left).min(1.0)).max(f);
        }
    } else {
        *shown = f;
        *hold = 0.0;
    }
}

/// Steps a display hold: `duration` while `on` (a posture gauge's damage, a new reward), then
/// counting down. Returns whether it still shows.
pub(crate) fn step_hold(hold: &mut f32, on: bool, duration: f32, dt: f32) -> bool {
    *hold = if on { duration } else { (*hold - dt).max(0.0) };
    *hold > 0.0
}

/// A framed bar of `width` x `height` showing `fill` inside a thin gold frame. A vitality fill
/// gets its damage trail behind it; a posture fill grows from the middle out and gets an arrow
/// at each end. Returns the frame.
pub(crate) fn bar<'a>(
    parent: &'a mut ChildSpawnerCommands,
    fill: Fill,
    (width, height): (Val, Val),
) -> EntityCommands<'a> {
    let posture = match fill {
        Fill::Posture(i) => Some(i),
        _ => None,
    };
    let mut frame = parent.spawn((
        Node {
            width,
            height,
            border: UiRect::all(px(1)),
            justify_content: if posture.is_some() {
                JustifyContent::Center
            } else {
                JustifyContent::FlexStart
            },
            ..default()
        },
        BackgroundColor(FRAME),
        BorderColor::all(EDGE),
    ));
    frame.with_children(|b| {
        if let Fill::Hp(i) = fill {
            b.spawn((
                HudPart::Fill(Fill::Trail(i)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(TRAIL),
            ));
        }
        b.spawn((
            HudPart::Fill(fill),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(match fill {
                Fill::Hp(_) => VITALITY,
                Fill::Trail(_) => TRAIL,
                Fill::Posture(_) => posture_color(0.0, false),
                Fill::Poison => Color::NONE,
                Fill::Xp => SKILL_BLUE,
            }),
        ));
        // The game's "<" ">" arrows at the posture bar's outer ends; a gold diamond stands in
        // for the arrow sprite (MENU_Taikan_Arrow, not extracted).
        if let Some(i) = posture {
            for right in [false, true] {
                b.spawn((
                    HudPart::PostureArrow(i),
                    Node {
                        position_type: PositionType::Absolute,
                        left: if right { Val::Auto } else { percent(-2.5) },
                        right: if right { percent(-2.5) } else { Val::Auto },
                        top: percent(-10),
                        height: percent(120),
                        aspect_ratio: Some(1.0),
                        ..default()
                    },
                    UiTransform::from_rotation(Rot2::degrees(45.0)),
                    BackgroundColor(Color::srgb(0.85, 0.72, 0.45)),
                    Visibility::Hidden,
                ));
            }
        }
    });
    frame
}

/// Character `i`'s posture bar of `size`, centred in a full-width row laid out by `node` and
/// shown while the posture has damage. Returns the row.
fn posture_row<'a>(
    parent: &'a mut ChildSpawnerCommands,
    i: usize,
    node: Node,
    size: (Val, Val),
) -> EntityCommands<'a> {
    let mut row = parent.spawn((
        HudPart::PostureRow(i),
        Node {
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..node
        },
        Visibility::Hidden,
    ));
    row.with_children(|r| {
        bar(r, Fill::Posture(i), size);
    });
    row
}

/// The enemy's bars, for the floating and the boss panel: its vitality in a box laid out by
/// `hp`, then its posture row laid out by `posture`. Returns the posture row.
fn enemy_bars<'a>(
    panel: &'a mut ChildSpawnerCommands,
    (hp, hp_size): (Node, (Val, Val)),
    (posture, posture_size): (Node, (Val, Val)),
) -> EntityCommands<'a> {
    panel.spawn(hp).with_children(|r| {
        bar(r, Fill::Hp(1), hp_size);
    });
    posture_row(panel, 1, posture, posture_size)
}

/// A row of [`MAX_BAR_MARKS`] round deathblow dots `size` across, `gap` apart, in a row laid
/// out by `node`.
fn dots(parent: &mut ChildSpawnerCommands, node: Node, size: Val, gap: Val) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: gap,
            ..node
        })
        .with_children(|r| {
            for i in 0..MAX_BAR_MARKS {
                r.spawn((
                    HudPart::DeathblowDot(i),
                    Node {
                        width: size,
                        height: size,
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::MAX,
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.06, 0.05)),
                    BorderColor::all(Color::srgb(0.25, 0.02, 0.0)),
                ));
            }
        });
}

fn text(s: &str, size: f32) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font_size: bevy::text::FontSize::Px(size),
            ..default()
        },
        TextColor(CREAM),
        TextShadow {
            offset: Vec2::new(1.5, 1.5),
            color: Color::BLACK,
        },
    )
}

/// Spawns the bars (call once at startup).
pub fn spawn(commands: &mut Commands) {
    // Wolf: vitality bottom left, status and poison above it.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: percent(5.7),
            bottom: percent(7.8),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                StatusText,
                Text::new(""),
                TextFont {
                    font_size: bevy::text::FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.55, 0.95)),
            ));
            p.spawn((PoisonRow, Node::default(), Visibility::Hidden))
                .with_children(|r| {
                    bar(r, Fill::Poison, (px(POISON_WIDTH), px(6)));
                });
            bar(p, Fill::Hp(0), (Val::Vw(HP_BAR_VW), Val::Vh(1.25))).insert(HudPart::HpFrame);
        });
    // Wolf: posture bottom centre.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: percent(12.6),
            ..default()
        })
        .with_children(|p| {
            posture_row(p, 0, Node::default(), (Val::Vw(27.0), Val::Vh(1.3)));
        });
    // A plain enemy: deathblow dots, vitality over posture, above its head.
    commands
        .spawn((
            HudPart::EnemyPanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Vw(OVERHEAD_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Vh(0.3),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|p| {
            dots(
                p,
                Node {
                    margin: UiRect {
                        left: Val::Vw(-0.2),
                        bottom: Val::Vh(0.1),
                        ..default()
                    },
                    ..default()
                },
                Val::Vw(1.15),
                Val::Vw(0.15),
            );
            enemy_bars(
                p,
                (Node::default(), (percent(100), Val::Vh(0.75))),
                (Node::default(), (percent(100), Val::Vh(0.6))),
            );
        });
    // A boss: vitality top left with the deathblow orbs over it and the name under it, posture
    // top centre with its centre mark.
    commands
        .spawn((
            HudPart::BossPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|c| {
            dots(
                c,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(5.5),
                    top: percent(4.6),
                    align_items: AlignItems::Center,
                    ..default()
                },
                Val::Vw(1.4),
                Val::Vw(0.3),
            );
            enemy_bars(
                c,
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(5.7),
                        top: percent(8.3),
                        ..default()
                    },
                    (Val::Vw(21.6), Val::Vh(1.25)),
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        top: percent(5.4),
                        ..default()
                    },
                    (Val::Vw(38.4), Val::Vh(1.8)),
                ),
            )
            .with_children(|r| {
                // The centre mark (MENU_Sp_Center, not extracted: a gold tick), 5 % of the
                // screen down, over the bar.
                r.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(50),
                        top: Val::Vh(-0.4),
                        margin: UiRect::left(Val::Vh(-0.25)),
                        width: Val::Vh(0.5),
                        height: Val::Vh(2.6),
                        ..default()
                    },
                    BackgroundColor(EDGE),
                ));
            });
            c.spawn((
                HudPart::BossName,
                text("", 15.0),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(6.3),
                    top: percent(11.0),
                    ..default()
                },
            ));
        });
    // The deathblow mark on the enemy: a red diamond with a dark rim (the game's MENU_ninsatu
    // sprite is not extracted).
    commands.spawn((HudPart::DeathblowMark, deathblow_mark()));
    // The item cluster, bottom right: the quick item's box, then the prosthetic tool's bigger
    // one (percent of the screen). No items are equipped in our game yet, so each box shows only
    // its button and lights its edge while it is held.
    for (slot, ((left, top, w, h), key)) in [(79.1, 87.2, 4.2, 7.3), (84.2, 83.1, 6.3, 11.3)]
        .into_iter()
        .zip(crate::play::ITEM_KEY_LABELS)
        .enumerate()
    {
        commands
            .spawn((
                HudPart::ItemSlot(slot),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(left),
                    top: percent(top),
                    width: percent(w),
                    height: percent(h),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::FlexEnd,
                    padding: UiRect::all(px(4)),
                    ..default()
                },
                BackgroundColor(FRAME),
                BorderColor::all(EDGE),
            ))
            .with_children(|b| {
                b.spawn(text(key, 13.0));
            });
    }
}

/// The skill points, experience gauge and Sen over the item cluster.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum RewardPart {
    Panel,
    SkillPoints,
    Sen,
}

/// The skill-point blue of the experience gauge and its count (the menu's status bar too).
pub(crate) const SKILL_BLUE: Color = Color::srgb(0.55, 0.85, 0.9);
/// Seconds the reward panel stays after the last reward.
const REWARD_SHOWN: f32 = 4.0;

/// Spawns the reward panel, hidden (call once at startup, after the game fonts and menu art).
pub fn spawn_reward(mut commands: Commands, fonts: Res<GameFonts>, art: Res<MenuArt>) {
    commands
        .spawn((
            RewardPart::Panel,
            Node {
                position_type: PositionType::Absolute,
                right: percent(9.5),
                top: percent(75.6),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Vh(0.5),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Vh(0.8),
                ..default()
            })
            .with_children(|r| {
                r.spawn((
                    RewardPart::SkillPoints,
                    fonts.shadowed("0", 16.0, SKILL_BLUE),
                ));
                bar(r, Fill::Xp, (Val::Vw(8.0), Val::Vh(0.7)));
            });
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Vh(0.6),
                ..default()
            })
            .with_children(|r| {
                if let Some((mut coin, _)) = art.sprite("SB_FE", "MENU_Money") {
                    coin.image_mode = bevy::ui::widget::NodeImageMode::Stretch;
                    r.spawn((
                        Node {
                            width: Val::Vh(2.3),
                            height: Val::Vh(2.3),
                            ..default()
                        },
                        coin,
                    ));
                }
                r.spawn((RewardPart::Sen, fonts.shadowed("0", 19.0, CREAM)));
            });
        });
}

/// The reward count and the skill points and Sen last written, the seconds the panel has left,
/// and whether it is hidden.
#[derive(Default)]
pub struct RewardShown {
    rewards: Option<u32>,
    values: Option<(u32, u32)>,
    left: f32,
    hidden: bool,
}

/// Fills the reward panel and shows it for [`REWARD_SHOWN`] seconds after a new reward
/// (`Kit::rewards` changes, so spending or the death penalty does not pop it). The gauge's
/// fill is set by [`update`] with the other bars.
pub fn update_reward(
    gauges: Res<Gauges>,
    time: Res<Time>,
    mut shown: Local<RewardShown>,
    mut parts: Query<(&RewardPart, Option<&mut Text>, Option<&mut Visibility>)>,
) {
    let new = shown.rewards.is_some_and(|r| r != gauges.rewards);
    shown.rewards = Some(gauges.rewards);
    let values = (gauges.skill_points, gauges.sen);
    let changed = shown.values != Some(values);
    if !new && !changed && shown.hidden {
        return;
    }
    shown.values = Some(values);
    let on = step_hold(&mut shown.left, new, REWARD_SHOWN, time.delta_secs());
    shown.hidden = !on;
    let texts = changed.then(|| [values.0.to_string(), values.1.to_string()]);
    for (part, text, visibility) in &mut parts {
        let value = match part {
            RewardPart::Panel => {
                show(visibility, on);
                continue;
            }
            RewardPart::SkillPoints => texts.as_ref().map(|t| &t[0]),
            RewardPart::Sen => texts.as_ref().map(|t| &t[1]),
        };
        if let (Some(value), Some(mut t)) = (value, text) {
            set(&mut t, |t| &mut t.0, value.clone());
        }
    }
}

/// Shows (`Inherited`) or hides a part with a visibility.
fn show(visibility: Option<Mut<Visibility>>, on: bool) {
    if let Some(mut v) = visibility {
        v.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Lays a node out, or takes it (and everything under it) out of the layout.
fn display(node: &mut Mut<Node>, on: bool) {
    set(
        node,
        |n| &mut n.display,
        if on { Display::Flex } else { Display::None },
    );
}

pub(crate) fn deathblow_mark() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            width: px(16),
            height: px(16),
            border: UiRect::all(px(2)),
            ..default()
        },
        UiTransform::from_rotation(Rot2::degrees(45.0)),
        BackgroundColor(Color::srgb(0.85, 0.05, 0.05)),
        BorderColor::all(Color::srgb(0.15, 0.0, 0.0)),
        Visibility::Hidden,
    )
}
/// Updates widths, colours, the floating panel and the boss bars.
#[allow(clippy::type_complexity)]
pub fn update(
    gauges: Res<Gauges>,
    time: Res<Time>,
    (mut trails, mut holds): (Local<[(f32, f32); 2]>, Local<[f32; 2]>),
    (recording, overlay_size): (
        Option<Res<crate::brain_overlay::RecordingBrain>>,
        Query<&ComputedNode, With<crate::brain_overlay::Panel>>,
    ),
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut parts: Query<(
        &HudPart,
        &mut Node,
        Option<&mut BackgroundColor>,
        Option<&mut Visibility>,
        Option<&mut BorderColor>,
        Option<&mut Text>,
    )>,
) {
    let dt = time.delta_secs();
    for (trail, &f) in trails.iter_mut().zip(&gauges.hp) {
        step_trail(trail, f, dt);
    }
    let damaged = gauges.posture.map(|f| f > POSTURE_SHOWN);
    let shown = [0, 1].map(|i| step_hold(&mut holds[i], damaged[i], SP_MIN_DISP_TIME, dt));
    let posture_colors = [0, 1].map(|i| posture_color(gauges.posture[i], gauges.broken[i]));
    let wolf_width = Val::Vw(hp_bar_vw(gauges.wolf_max_hp));
    let boss = gauges.boss_bar && gauges.enemy_alive;
    let camera = camera.single().ok();
    let project = |at: Vec3| camera.and_then(|(cam, tf)| cam.world_to_viewport(tf, at).ok());
    let head = gauges.enemy_head.filter(|_| gauges.enemy_alive);
    let mark = head.filter(|_| gauges.deathblow).and_then(project);
    // The floating panel's top-left corner, with the bars' bottom at the point over the head.
    let floating = head
        .filter(|_| !gauges.boss_bar)
        .and_then(|h| project(h + Vec3::Y * OVERHEAD_ABOVE_HEAD))
        .map(|p| {
            let viewport = viewport_size(camera.map(|(cam, _)| cam));
            let width = viewport.x * OVERHEAD_W / 100.0;
            let top = p.y - 0.035 * viewport.y;
            let mut left = p.x - width * 0.5;
            if recording.as_ref().is_some_and(|r| r.enabled)
                && let Ok(size) = overlay_size.single()
            {
                let overlay_left = viewport.x - PANEL_INSET - PANEL_WIDTH;
                let overlay_bottom = PANEL_INSET + size.size().y * size.inverse_scale_factor;
                if left + width > overlay_left - OVERLAY_GAP
                    && p.y > PANEL_INSET
                    && top < overlay_bottom
                {
                    left = overlay_left - width - OVERLAY_GAP;
                }
            }
            Vec2::new(left, top)
        });
    for (part, mut node, color, visibility, border, text) in &mut parts {
        // Population bars use actor slots above the two duel HUD slots.
        if matches!(*part, HudPart::Fill(Fill::Hp(i) | Fill::Trail(i) | Fill::Posture(i)) | HudPart::PostureArrow(i) if i >= 2)
        {
            continue;
        }
        match *part {
            HudPart::Fill(fill) => {
                let (fraction, c) = match fill {
                    Fill::Hp(i) => (gauges.hp[i], VITALITY),
                    Fill::Trail(i) => (trails[i].0, TRAIL),
                    Fill::Posture(i) => (gauges.posture[i], posture_colors[i]),
                    Fill::Poison => (gauges.poison, Color::srgb(0.55, 0.2, 0.75)),
                    Fill::Xp => (gauges.xp_fill, SKILL_BLUE),
                };
                set(&mut node, |n| &mut n.width, percent(fraction * 100.0));
                if let Some(mut color) = color {
                    color.set_if_neq(BackgroundColor(c));
                }
            }
            HudPart::HpFrame => set(&mut node, |n| &mut n.width, wolf_width),
            HudPart::PostureRow(i) => show(visibility, shown[i] && (i == 0 || gauges.enemy_alive)),
            HudPart::PostureArrow(i) => show(visibility, damaged[i]),
            HudPart::EnemyPanel => {
                display(&mut node, floating.is_some());
                if let Some(p) = floating {
                    set(&mut node, |n| &mut n.left, px(p.x));
                    set(&mut node, |n| &mut n.top, px(p.y));
                }
            }
            HudPart::BossPanel => display(&mut node, boss),
            HudPart::DeathblowDot(i) => display(&mut node, i < gauges.enemy_marks),
            HudPart::BossName => {
                if let Some(mut t) = text
                    && t.0 != gauges.enemy_name
                {
                    t.0 = gauges.enemy_name.to_string();
                }
            }
            HudPart::DeathblowMark => {
                show(visibility, mark.is_some());
                if let Some(p) = mark {
                    set(&mut node, |n| &mut n.left, px(p.x - 8.0));
                    set(&mut node, |n| &mut n.top, px(p.y - 8.0));
                }
            }
            HudPart::ItemSlot(slot) => {
                if let Some(mut border) = border {
                    border.set_if_neq(BorderColor::all(if gauges.held[slot] {
                        Color::srgb(1.0, 0.9, 0.6)
                    } else {
                        EDGE
                    }));
                }
            }
        }
    }
}

/// Updates Wolf's status line and shows the poison bar while poison builds up.
pub fn update_status(
    gauges: Res<Gauges>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusText>>,
    mut poison_row: Query<&mut Visibility, With<PoisonRow>>,
) {
    if let Ok((mut t, mut color)) = status.single_mut() {
        if t.0 != gauges.status {
            t.0.clone_from(&gauges.status);
        }
        color.set_if_neq(TextColor(
            if gauges.status.contains("Electrified") || gauges.status.contains("Lightning") {
                Color::srgb(0.4, 0.8, 1.0)
            } else {
                Color::WHITE
            },
        ));
    }
    show(poison_row.single_mut().ok(), gauges.poison > 0.0);
}

/// The perilous-attack kanji.
#[derive(Component)]
pub struct PerilousMark;

/// How long the kanji shows, seconds: pops in, holds, fades. The game's effect (FXR 600035) is
/// not decoded; 0.8 s is FwLo's RustSekiro value (`src/hud.rs`), and close to what the game
/// shows.
const PERILOUS_SHOW: f32 = 0.8;

/// Windows fonts that have the kanji, tried in order; without one the mark falls back to "!!".
const KANJI_FONTS: [&str; 3] = ["YuGothB.ttc", "msgothic.ttc", "meiryo.ttc"];

/// Shows the red 危 over Wolf when a perilous warning reaches him (`docs/COMBAT-RULES.md`,
/// "Perilous attacks"). A stand-in for the game's effect: its texture lives in the install's
/// effect archive and its FXR is not decoded, so a Japanese system font draws the glyph (FwLo's
/// RustSekiro does the same, `src/hud.rs`). Spawns the mark on its first run.
pub fn update_perilous(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    gauges: Res<Gauges>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut mark: Query<
        (&mut Node, &mut TextColor, &mut TextShadow, &mut Visibility),
        With<PerilousMark>,
    >,
) {
    let Ok((mut node, mut color, mut shadow, mut vis)) = mark.single_mut() else {
        let font = KANJI_FONTS
            .iter()
            .find_map(|f| std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(f)).ok());
        let glyph = if font.is_some() { "危" } else { "!!" };
        let font = font
            .map(|bytes| bevy::text::FontSource::Handle(fonts.add(Font::from_bytes(bytes))))
            .unwrap_or_default();
        commands.spawn((
            PerilousMark,
            Text::new(glyph),
            TextFont {
                font,
                font_size: bevy::text::FontSize::Px(PERILOUS_SIZE),
                ..default()
            },
            TextColor(Color::NONE),
            TextShadow {
                offset: Vec2::splat(2.0),
                color: Color::srgba(0.1, 0.0, 0.0, 0.0),
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
        ));
        return;
    };
    let shown = gauges.perilous_age.filter(|&a| a < PERILOUS_SHOW);
    let screen = shown.zip(gauges.perilous_at).and_then(|(_, at)| {
        let (cam, tf) = camera.single().ok()?;
        cam.world_to_viewport(tf, at).ok()
    });
    match (shown, screen) {
        (Some(age), Some(p)) => {
            // Pops in over the first tenth of a second, holds, fades over the last third.
            let pop = (age / 0.1).min(1.0);
            let fade = ((PERILOUS_SHOW - age) / (PERILOUS_SHOW / 3.0)).min(1.0);
            let a = pop.min(fade);
            color.0 = Color::srgba(0.92, 0.05, 0.04, a);
            shadow.color = Color::srgba(0.1, 0.0, 0.0, 0.8 * a);
            node.left = px(p.x - PERILOUS_SIZE * 0.5);
            node.top = px(p.y - PERILOUS_SIZE * 0.5);
            *vis = Visibility::Inherited;
        }
        _ => *vis = Visibility::Hidden,
    }
}

/// The kanji's size on screen, pixels.
const PERILOUS_SIZE: f32 = 96.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vitality_bar_length_follows_max_hp() {
        // Level 1's 320 is the measured 9.5 % of the screen; Vitality 20's 1120 is 3.5 times it.
        assert!((hp_bar_vw(320) - 9.5).abs() < 1e-4);
        assert!((hp_bar_vw(1120) - 33.25).abs() < 1e-3);
        assert!((hp_bar_vw(640) - 2.0 * hp_bar_vw(320)).abs() < 1e-4);
        // A missing maximum still draws a sliver, and a huge one stays on screen.
        assert!(hp_bar_vw(0) > 0.0);
        assert_eq!(hp_bar_vw(100_000), HP_BAR_MAX_VW);
    }

    #[test]
    fn posture_states_follow_the_hud_movie_thresholds() {
        assert_eq!(posture_state(0.0, false), 0);
        assert_eq!(posture_state(0.69, false), 0);
        assert_eq!(posture_state(0.7, false), 1);
        assert_eq!(posture_state(1.0, false), 2);
        assert_eq!(posture_state(1.0, true), 3);
        assert_eq!(posture_color(0.0, false), Color::srgb_u8(142, 123, 60));
    }

    #[test]
    fn damage_trail_holds_then_collapses_onto_the_fill() {
        let dt = 1.0 / 60.0;
        let mut trail = (1.0, 0.0);
        // Held for the first half second.
        for _ in 0..29 {
            step_trail(&mut trail, 0.6, dt);
        }
        assert_eq!(trail.0, 1.0);
        // On the fill a few frames later.
        for _ in 0..6 {
            step_trail(&mut trail, 0.6, dt);
        }
        assert!((trail.0 - 0.6).abs() < 1e-5);
        // Healing snaps the trail up with the fill and resets the hold.
        step_trail(&mut trail, 0.9, dt);
        assert_eq!(trail, (0.9, 0.0));
    }

    #[test]
    fn posture_gauge_stays_up_for_the_minimum_display_time() {
        let mut hold = 0.0;
        assert!(!step_hold(&mut hold, false, SP_MIN_DISP_TIME, 0.1));
        assert!(step_hold(&mut hold, true, SP_MIN_DISP_TIME, 0.1));
        assert!(step_hold(&mut hold, false, SP_MIN_DISP_TIME, 0.3));
        assert!(!step_hold(&mut hold, false, SP_MIN_DISP_TIME, 0.3));
    }
}
