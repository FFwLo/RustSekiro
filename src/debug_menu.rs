//! Debug menu (F1): practice and test controls in one organised panel.
//!   Up / Down select, Left / Right change a value, Enter runs an action, F1 closes (the numpad
//!   8 / 2 / 4 / 6 work as arrows too).
//! Sections: Enemy (AI mode, drill attack and interval, type and outfit - those two restart the
//! game with the choice written to config.toml), Deathblow, Skills (combat art and the three
//! prosthetic slots, live; names from the game's own text), Player, Map (load any exported map,
//! time of day, a day-night cycle, the map's light sets and draw-param looks, the sky dome),
//! Simulation.

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet};
use crate::config::GameConfig;
use crate::data::Combat;
use crate::enemy::{AiMode, Enemy, EnemyDebug, attack_list};
use crate::player::{Action, PadInput, Player};

/// Enemy characters and their NpcParam outfit rows when there is no roster (the two
/// combat_data.json carries).
const FALLBACK_ENEMIES: [(&str, &str, &[(i64, &str)]); 2] = [
    (
        "c1020",
        "Samurai General",
        &[(10203010, "no haori (as fought live)"), (10200010, "castle, with haori"), (10202000, "Honjo, with haori"), (10202070, "Honjo, no haori")],
    ),
    ("c1010", "Ochimusha", &[(10100000, "one-handed sword")]),
];

/// One pickable enemy: chr id, name, and its NpcParam rows (row, label).
struct EnemyChoice {
    chr: String,
    name: String,
    outfits: Vec<(i64, String)>,
}

/// Every enemy and boss `sekiro-extract npcs` exported (extracted/enemies/roster.json: the chr
/// models the maps place, with each placed NpcParam row), else the two built in.
fn enemy_choices() -> &'static [EnemyChoice] {
    static LIST: std::sync::OnceLock<Vec<EnemyChoice>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let dir = crate::paths::root().join("extracted/enemies");
        let roster: Vec<serde_json::Value> = std::fs::read_to_string(dir.join("roster.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut list: Vec<EnemyChoice> = roster
            .iter()
            .filter_map(|e| {
                let chr = e["chr"].as_str()?.to_string();
                if !dir.join(format!("{chr}.json")).exists() {
                    return None;
                }
                let blows = e["deathblows"].as_i64().unwrap_or(0);
                let boss = if blows > 1 { format!(" [boss, {blows} deathblows]") } else { String::new() };
                let outfits = e["placements"]
                    .as_array()?
                    .iter()
                    .filter_map(|p| {
                        let label = format!("{} (HP {}, posture {}, x{} in {})", p["name"].as_str().unwrap_or(""), p["hp"], p["posture"], p["count"], p["maps"].as_array().map(|m| m.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" ")).unwrap_or_default());
                        Some((p["npc"].as_i64()?, label))
                    })
                    .collect::<Vec<_>>();
                (!outfits.is_empty()).then(|| EnemyChoice { chr, name: format!("{}{boss}", e["name"].as_str().unwrap_or("")), outfits })
            })
            .collect();
        if list.is_empty() {
            list = FALLBACK_ENEMIES
                .iter()
                .map(|(c, n, o)| EnemyChoice { chr: c.to_string(), name: n.to_string(), outfits: o.iter().map(|(r, l)| (*r, l.to_string())).collect() })
                .collect();
        }
        list
    })
}

const SPEEDS: [f32; 4] = [1.0, 0.5, 0.25, 0.1];
const AI_MODES: [(AiMode, &str); 6] = [
    (AiMode::Real, "real AI"),
    (AiMode::Passive, "passive (walks up, never attacks)"),
    (AiMode::Perilous, "perilous attacks only"),
    (AiMode::Thrust, "perilous thrusts only (Mikiri: step toward it)"),
    (AiMode::Repeat, "repeat one attack"),
    (AiMode::Idle, "stand still"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    AiMode,
    Attack,
    Interval,
    EnemyType,
    Outfit,
    EnemyHeal,
    BreakPosture,
    DeathblowNow,
    DeathblowBehind,
    PlungeNow,
    KickDownNow,
    VaultNow,
    StealthSetup,
    StealthFar,
    Art,
    Tool1,
    Tool2,
    Tool3,
    God,
    NoPosture,
    InfiniteEmblems,
    Refill,
    FlowingWater,
    Sheathe,
    DropHeight,
    DropNow,
    HookHeight,
    HookNow,
    HookClear,
    Speed,
    Hitboxes,
    Reset,
    Map,
    Hour,
    DayCycle,
    Weather,
    Sky,
}

const SECTIONS: [(&str, &[Item]); 7] = [
    ("ENEMY", &[Item::AiMode, Item::Attack, Item::Interval, Item::EnemyType, Item::Outfit, Item::EnemyHeal]),
    ("DEATHBLOW", &[Item::BreakPosture, Item::DeathblowNow, Item::DeathblowBehind, Item::PlungeNow, Item::KickDownNow, Item::VaultNow, Item::StealthSetup, Item::StealthFar]),
    ("SKILLS", &[Item::Art, Item::Tool1, Item::Tool2, Item::Tool3]),
    ("PLAYER", &[Item::God, Item::NoPosture, Item::InfiniteEmblems, Item::Refill, Item::FlowingWater, Item::Sheathe]),
    ("MOVEMENT TESTS", &[Item::DropHeight, Item::DropNow, Item::HookHeight, Item::HookNow, Item::HookClear]),
    ("MAP", &[Item::Map, Item::Hour, Item::DayCycle, Item::Weather, Item::Sky]),
    ("SIMULATION", &[Item::Speed, Item::Hitboxes, Item::Reset]),
];

/// Day-night cycle speeds: game hours per real second (map::DayCycle).
const DAY_SPEEDS: [(f32, &str); 4] = [(0.0, "off"), (1.0 / 60.0, "1 game hour a minute"), (0.1, "1 game hour in 10 s"), (0.5, "1 game hour in 2 s")];

/// The castle top's story looks (map.rs FIGHT_LIGHTING, m11_01_00_00.emevd): the Owl's sub 50
/// and Emma / Isshin Ashina's sub 40.
fn look_name(v: &str) -> String {
    match v {
        "base" => "map's own".into(),
        "0750" => "the Owl's golden haze (0750)".into(),
        "0740" => "Isshin's fiery sunset (0740)".into(),
        v => format!("draw params {v}"),
    }
}

fn maps() -> &'static [(String, String)] {
    static LIST: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    LIST.get_or_init(crate::map::exported_maps)
}

/// The map's weather choices: each draw-param look with each of its light sets.
fn weathers(looks: &crate::map::MapLooks) -> Vec<(usize, i32)> {
    let sets = if looks.light_sets.is_empty() { vec![0] } else { looks.light_sets.clone() };
    (0..looks.looks.len().max(1)).flat_map(|l| sets.iter().map(move |&id| (l, id))).collect()
}

/// The map-section resources, one parameter (a system takes 16 at most).
type MapParams<'w> = (
    Option<ResMut<'w, crate::map::MapLighting>>,
    Option<Res<'w, crate::map::MapLooks>>,
    Option<ResMut<'w, crate::map::DayCycle>>,
    Option<ResMut<'w, crate::sky::SkyChoice>>,
    Option<ResMut<'w, Messages<crate::map::GoToMap>>>,
);

/// Fall tests: the heights Wolf is lifted to (m). 20 m is where a fall can kill at all
/// (exe FUN_14084e5d0, HitMtrlParam fallDeathDist 40 on a normal floor); 45 m is over it.
const DROP_HEIGHTS: [f32; 9] = [5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 45.0, 60.0];
/// Grapple tests: how far above the floor the test hook hangs (m); it is placed 10 m ahead.
const HOOK_HEIGHTS: [f32; 4] = [4.0, 8.0, 12.0, 16.0];
const HOOK_AHEAD: f32 = 10.0;

/// Every combat art (EquipParamWeapon 5100-7700 with an spAtkcategory), in id order.
fn art_ids(combat: &Combat) -> Vec<i64> {
    let mut ids: Vec<i64> = combat.params["EquipParamWeapon"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, r)| Some((k.parse::<i64>().ok()?, r["spAtkcategory"].as_i64()?)))
        .filter(|&(id, cat)| (5100..8000).contains(&id) && cat > 0)
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

/// The three prosthetic slots (0 = empty). The game keeps config player.prosthetics packed
/// (player::equipped_tool, the Z key), so the menu keeps its own slot layout and only uses it
/// while it still packs to the config's list; otherwise (F5, start-up) it is rebuilt packed.
fn slots(layout: [i64; 3], packed: &[i64]) -> [i64; 3] {
    if pack(layout) == packed {
        return layout;
    }
    let mut out = [0; 3];
    for (i, id) in packed.iter().take(3).enumerate() {
        out[i] = *id;
    }
    out
}

fn pack(slots: [i64; 3]) -> Vec<i64> {
    slots.iter().copied().filter(|id| *id != 0).collect()
}

fn items() -> Vec<Item> {
    SECTIONS.iter().flat_map(|(_, it)| it.iter().copied()).collect()
}

#[derive(Resource, Default)]
pub struct DebugMenu {
    pub open: bool,
    cursor: usize,
    /// Player cheats.
    pub god: bool,
    pub no_posture: bool,
    pub infinite_emblems: bool,
    speed: usize,
    /// Movement tests: index into DROP_HEIGHTS / HOOK_HEIGHTS, and how many hooks were added.
    drop_height: usize,
    hook_height: usize,
    hooks_added: usize,
    /// Pending type / outfit choice (applied by restarting).
    enemy_type: usize,
    outfit: usize,
    /// The slot layout shown for prosthetic slots 1-3 (see slots()).
    tool_slots: [i64; 3],
    /// The config enemy (chr, npc_row) the pending choice was last synced to.
    seen_enemy: Option<(String, Option<i64>)>,
    /// Map section: the picked map (index into maps()), the day-cycle speed, the weather (index
    /// into weathers()) and the map it was set on.
    map: usize,
    day_speed: usize,
    weather: usize,
    weather_map: Option<String>,
    message: String,
}

#[derive(Component)]
struct MenuText;

pub struct DebugMenuPlugin;

impl Plugin for DebugMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugMenu>()
            .add_systems(Startup, spawn_menu)
            .add_systems(Update, (menu_input, draw_menu).chain().run_if(resource_exists::<ButtonInput<KeyCode>>))
            .add_systems(FixedUpdate, player_cheats.after(ActorSet::Resolve).after(crate::status::tick));
    }
}

fn spawn_menu(mut commands: Commands, config: Res<GameConfig>, combat: Option<Res<Combat>>, mut menu: ResMut<DebugMenu>) {
    menu.infinite_emblems = config.player.infinite_emblems;
    sync_enemy_choice(&mut menu, &config, combat.as_deref());
    commands.spawn((
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(0.92, 0.92, 0.88)),
        BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(90.0),
            right: Val::Px(14.0),
            padding: UiRect::all(Val::Px(12.0)),
            display: Display::None,
            ..default()
        },
        MenuText,
    ));
}

/// Points the pending type / outfit at the enemy in the config: at start-up and whenever the
/// Fight picker (enemy::spawn_encounter) swaps config.enemy in memory.
/// No npc_row in the config = the kind's default row (the one being fought), as draw_menu's
/// `pending` reads it; before, outfit 0, so the menu showed a restart into another row.
fn sync_enemy_choice(menu: &mut DebugMenu, config: &GameConfig, combat: Option<&Combat>) {
    let row = config.enemy.npc_row.or_else(|| combat.and_then(|c| c.kinds.first()).map(|k| k.foe.npc_row));
    let seen = (config.enemy.chr.clone(), row);
    if menu.seen_enemy.as_ref() == Some(&seen) {
        return;
    }
    menu.enemy_type = enemy_choices().iter().position(|e| e.chr == config.enemy.chr).unwrap_or(0);
    menu.outfit = row.and_then(|r| enemy_choices()[menu.enemy_type].outfits.iter().position(|o| o.0 == r)).unwrap_or(0);
    menu.seen_enemy = Some(seen);
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<DebugMenu>,
    mut dbg: ResMut<EnemyDebug>,
    mut config: ResMut<GameConfig>,
    combat: Res<Combat>,
    mut time: ResMut<Time<Virtual>>,
    mut pad: ResMut<PadInput>,
    mut hurtboxes: ResMut<crate::combat::ShowHurtboxes>,
    mut player: Query<(&mut Player, &mut Actor, &mut Transform), Without<Enemy>>,
    mut enemies: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Option<&crate::enemy::Phantom>), Without<Player>>,
    mut statuses: Option<ResMut<crate::status::Statuses>>,
    mut hooks: Option<ResMut<crate::map::GrapplePoints>>,
    mut exit: MessageWriter<AppExit>,
    (mut lighting, looks, mut day, mut sky, mut go): MapParams,
) {
    if keys.just_pressed(KeyCode::F1) {
        menu.open = !menu.open;
    }
    sync_enemy_choice(&mut menu, &config, Some(&combat));
    // The drill's attack belongs to the kind it was picked on: after the Fight picker swaps the
    // kinds it names no anim, and drill fell back to random attacks while the menu still named it.
    // Any kind's anim counts (SHINOBI_AI_ATTACK=3042 picks a later cast member's, c7110). The env
    // attack is kept: on the first frames the kinds are still the start kind's, before the
    // SHINOBI_FIGHT encounter loads, and the reset stuck it on a000_003000 (VFX, c7110 3043).
    let env = crate::enemy::env_attack().is_some_and(|a| a == dbg.attack);
    if !env && !dbg.attack.is_empty() && !combat.kinds.is_empty() && combat.kinds.iter().all(|k| k.data.anim(&dbg.attack).is_none()) {
        dbg.attack = attack_list(combat.enemy0()).first().map(|a| a.0.clone()).unwrap_or_default();
    }
    // Real AI vs passive is the enemy's own switch (T): show what it is.
    if matches!(dbg.mode, AiMode::Real | AiMode::Passive) {
        if let Some((_, e, _, _, _)) = enemies.iter().next() {
            let want = if e.aggressive { AiMode::Real } else { AiMode::Passive };
            if dbg.mode != want {
                dbg.mode = want;
            }
        }
    }
    // A freshly loaded map starts on its own look and light set.
    let loaded = crate::map::loaded_map();
    if menu.weather_map != loaded {
        if let (Some(l), Some(lk)) = (lighting.as_deref(), looks.as_deref()) {
            menu.weather = weathers(lk).iter().position(|w| *w == (0, l.light_set)).unwrap_or(0);
        }
        if let Some(i) = loaded.as_ref().and_then(|m| maps().iter().position(|(id, _)| id == m)) {
            menu.map = i;
        }
        menu.weather_map = loaded;
    }
    if !menu.open {
        return;
    }
    let list = items();
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::Numpad2]) {
        menu.cursor = (menu.cursor + 1) % list.len();
    }
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::Numpad8]) {
        menu.cursor = (menu.cursor + list.len() - 1) % list.len();
    }
    let step: i32 = if keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::Numpad6]) {
        1
    } else if keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::Numpad4]) {
        -1
    } else {
        0
    };
    let enter = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    if step == 0 && !enter {
        return;
    }
    // The enemy actions below take one enemy: the nearest one alive and in play (not dead, not
    // disabled by a map script, not a parked phantom). Before, each moved / broke / revived every
    // enemy entity at once (a group stacked on one spot, dead members came back).
    let wolf_at = player.iter().next().map_or(Vec3::ZERO, |(_, _, t)| t.translation);
    let target = enemies
        .iter()
        .filter(|(_, e, _, _, ph)| !e.is_dead() && !e.is_disabled() && !ph.is_some_and(|p| p.parked()))
        .min_by(|a, b| a.3.translation.distance_squared(wolf_at).total_cmp(&b.3.translation.distance_squared(wolf_at)))
        .map(|(id, ..)| id);
    let cycle = |i: usize, n: usize| ((i as i32 + step).rem_euclid(n as i32)) as usize;
    let attacks = attack_list(combat.enemy0());
    match list[menu.cursor] {
        Item::AiMode => {
            let i = AI_MODES.iter().position(|m| m.0 == dbg.mode).unwrap_or(0);
            dbg.mode = AI_MODES[cycle(i, AI_MODES.len())].0;
            // The real AI and passive mode are the enemy's own aggressive switch (T).
            for (_, mut e, _, _, _) in &mut enemies {
                e.aggressive = dbg.mode != AiMode::Passive;
            }
            if dbg.mode == AiMode::Repeat && dbg.attack.is_empty() {
                dbg.attack = attacks.first().map(|a| a.0.clone()).unwrap_or_default();
            }
        }
        Item::Attack if !attacks.is_empty() => {
            let i = attacks.iter().position(|a| a.0 == dbg.attack).unwrap_or(0);
            dbg.attack = attacks[cycle(i, attacks.len())].0.clone();
            dbg.mode = AiMode::Repeat;
        }
        Item::Interval => dbg.interval = (dbg.interval + 0.25 * step as f32).clamp(0.25, 6.0),
        Item::EnemyType => {
            if step != 0 {
                menu.enemy_type = cycle(menu.enemy_type, enemy_choices().len());
                menu.outfit = 0;
            }
            if enter {
                restart_with(&mut menu, &mut config, &mut exit);
            }
        }
        Item::Outfit => {
            if step != 0 {
                menu.outfit = cycle(menu.outfit, enemy_choices()[menu.enemy_type].outfits.len());
            }
            if enter {
                restart_with(&mut menu, &mut config, &mut exit);
            }
        }
        Item::EnemyHeal if enter => {
            for (_, mut e, mut a, _, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                a.hp = a.hp_max;
                a.posture = 0.0;
                e.end_break_now();
            }
            // Its afflictions go too (round 57: a healed burning enemy kept burning and draining).
            if let (Some(t), Some(s)) = (target, statuses.as_mut()) {
                s.clear(t);
            }
            menu.message = "enemy healed".into();
        }
        Item::BreakPosture if enter => {
            for (_, mut e, mut a, _, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                a.posture = a.posture_max;
                e.on_posture_break(&mut a, &combat, false, 4.0);
            }
            menu.message = "enemy posture broken - attack to deathblow".into();
        }
        Item::DeathblowNow | Item::DeathblowBehind if enter => {
            // Break the posture, put the enemy in front of Wolf (facing him, or his back turned for
            // the behind deathblow), and press attack: the real deathblow flow (start throw,
            // absorb, ThrowDef -> ThrowDefDeath) runs.
            let behind = list[menu.cursor] == Item::DeathblowBehind;
            if let Ok((_, pa, ptf)) = player.single() {
                let fwd = pa.forward();
                for (_, mut e, mut a, mut tf, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                    tf.translation = ptf.translation + fwd * if behind { 1.2 } else { 1.6 };
                    a.yaw = if behind { pa.yaw } else { pa.yaw + std::f32::consts::PI };
                    a.posture = a.posture_max;
                    e.on_posture_break(&mut a, &combat, false, 4.0);
                }
                pad.press(Action::Attack);
                menu.message = if behind { "deathblow from behind" } else { "deathblow" }.into();
            }
        }
        Item::PlungeNow | Item::KickDownNow | Item::VaultNow if enter => {
            // Break the posture with the enemy 1.2 m in front of Wolf, facing him. Plunge: Wolf
            // falls from 3 m above, just short of him, attacking (崩し落下). Kick-down: Wolf in the
            // head-kick jump above him, attacking (蹴り崩し). Vault: jump (崩し蹴りジャンプ).
            let it = list[menu.cursor];
            if let Ok((_, mut pa, mut ptf)) = player.single_mut() {
                let fwd = pa.forward();
                for (_, mut e, mut a, mut tf, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                    tf.translation = ptf.translation + fwd * 1.2;
                    a.yaw = pa.yaw + std::f32::consts::PI;
                    a.posture = a.posture_max;
                    e.on_posture_break(&mut a, &combat, false, 6.0);
                }
                match it {
                    Item::VaultNow => {
                        pad.press(Action::Jump);
                        menu.message = "vault over (jump at a broken enemy)".into();
                    }
                    _ => {
                        let kick = it == Item::KickDownNow;
                        pa.play_state(&combat.player, if kick { "AirKickEnemyJumpStart_N" } else { "VerticalGroundJumpStart" });
                        if !kick {
                            // Past the jump's attack accept flag 115 (frame 3); the fall loop has none.
                            pa.t = 0.5;
                            pa.prev_t = 0.5;
                        }
                        pa.airborne = true;
                        pa.vel_y = if kick { 3.0 } else { -1.0 };
                        pa.air_base = Vec3::ZERO;
                        ptf.translation.y += if kick { 1.5 } else { 3.0 };
                        ptf.translation += fwd * 0.6;
                        pad.press(Action::Attack);
                        menu.message = if kick { "kick-down deathblow" } else { "plunge deathblow" }.into();
                    }
                }
            }
        }
        Item::StealthSetup if enter => {
            // The enemy 5 m in front of Wolf with its back turned, not having noticed him: sneak
            // up and attack from behind (stealth deathblow), or jump and plunge onto it.
            if let Ok((_, pa, ptf)) = player.single() {
                let fwd = pa.forward();
                for (_, mut e, mut a, mut tf, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                    tf.translation = ptf.translation + fwd * 5.0;
                    a.yaw = pa.yaw;
                    a.hp = a.hp_max;
                    a.posture = 0.0;
                    e.make_unaware(&mut a);
                }
                if let (Some(t), Some(s)) = (target, statuses.as_mut()) {
                    s.clear(t);
                }
                menu.message = "enemy unaware, back turned - sneak up and attack".into();
            }
        }
        Item::StealthFar if enter => {
            // The enemy 22 m ahead, facing Wolf, not having noticed him: inside its wide "around"
            // sight (the meter fills, then it comes to look) but outside its near sight.
            if let Ok((_, pa, ptf)) = player.single() {
                let fwd = pa.forward();
                for (_, mut e, mut a, mut tf, _) in target.and_then(|t| enemies.get_mut(t).ok()) {
                    tf.translation = ptf.translation + fwd * 22.0;
                    a.yaw = pa.yaw + std::f32::consts::PI;
                    a.hp = a.hp_max;
                    a.posture = 0.0;
                    e.make_unaware(&mut a);
                    e.aggressive = true;
                }
                if let (Some(t), Some(s)) = (target, statuses.as_mut()) {
                    s.clear(t);
                }
                menu.message = "enemy 22 m away, unaware, watching - stay out of its sight".into();
            }
        }
        Item::God => menu.god = !menu.god,
        Item::NoPosture => menu.no_posture = !menu.no_posture,
        Item::InfiniteEmblems => menu.infinite_emblems = !menu.infinite_emblems,
        Item::Refill if enter => {
            for (mut p, _, _) in &mut player {
                p.gourd = config.player.gourd_charges;
                p.emblems = config.player.spirit_emblems;
                p.resurrections = config.player.resurrections;
                p.revive_locked = false;
                p.refill_items = true;
            }
            menu.message = "gourds, emblems and resurrections refilled".into();
        }
        Item::Sheathe if enter => {
            pad.sheathe = true;
            menu.message = "sheathe / draw (X)".into();
        }
        Item::Art if step != 0 => {
            let arts = art_ids(&combat);
            if !arts.is_empty() {
                let i = arts.iter().position(|a| *a == config.player.combat_art).unwrap_or(0);
                config.player.combat_art = arts[cycle(i, arts.len())];
                menu.message = format!("combat art: {}", combat.weapon_name(config.player.combat_art));
            }
        }
        Item::Tool1 | Item::Tool2 | Item::Tool3 if step != 0 => {
            let n = match list[menu.cursor] {
                Item::Tool1 => 0,
                Item::Tool2 => 1,
                _ => 2,
            };
            // Empty, then every tool level (EquipParamWeapon 70000-79200).
            let choices: Vec<i64> = std::iter::once(0).chain(combat.player.prosthetics.iter().map(|t| t.id)).collect();
            let mut sl = slots(menu.tool_slots, &config.player.prosthetics);
            let i = choices.iter().position(|c| *c == sl[n]).unwrap_or(0);
            sl[n] = choices[cycle(i, choices.len())];
            menu.tool_slots = sl;
            config.player.prosthetics = pack(sl);
            if let Ok((mut p, _, _)) = player.single_mut() {
                p.tool_slot = 0;
            }
            menu.message = if sl[n] == 0 { format!("slot {}: empty", n + 1) } else { format!("slot {}: {}", n + 1, combat.weapon_name(sl[n])) };
        }
        Item::FlowingWater => {
            if config.player.skills.contains(&FLOWING_WATER) {
                config.player.skills.retain(|s| *s != FLOWING_WATER);
            } else {
                config.player.skills.push(FLOWING_WATER);
            }
        }
        Item::DropHeight if step != 0 => {
            menu.drop_height = cycle(menu.drop_height, DROP_HEIGHTS.len());
            menu.message = format!("drop height {} m", DROP_HEIGHTS[menu.drop_height]);
        }
        Item::DropNow if enter => {
            // Lifts Wolf straight up and lets go: the fall, the landing and (from 20 m) the
            // death / safe return are the normal game code (player.rs).
            let h = DROP_HEIGHTS[menu.drop_height];
            if let Ok((mut p, mut a, mut t)) = player.single_mut() {
                t.translation.y += h;
                a.airborne = true;
                a.vel_y = 0.0;
                a.fall_type = 0;
                a.air_base = Vec3::ZERO;
                p.air_top = t.translation.y;
                a.play_state(&combat.player, "FreeFall");
                menu.message = format!("lifted {h} m and let go");
            }
        }
        Item::HookHeight if step != 0 => {
            menu.hook_height = cycle(menu.hook_height, HOOK_HEIGHTS.len());
            menu.message = format!("test hook height {} m", HOOK_HEIGHTS[menu.hook_height]);
        }
        Item::HookNow if enter => {
            // A rideable wire point HOOK_AHEAD in front of Wolf, up in the air: aim the camera
            // at it and press the grapple button. Landing is on the point, so he falls from there.
            if let (Some(hooks), Ok((_, a, t))) = (hooks.as_mut(), player.single()) {
                let fwd = Vec3::new(-a.yaw.sin(), 0.0, -a.yaw.cos());
                let at = t.translation + fwd * HOOK_AHEAD + Vec3::Y * HOOK_HEIGHTS[menu.hook_height];
                hooks.0.push(crate::map::GrapplePoint { hook: at, land: at - Vec3::Y * 1.0, fwd: -fwd, variation: 0 });
                menu.hooks_added += 1;
                menu.message = format!("test hook {} m ahead, {} m up", HOOK_AHEAD, HOOK_HEIGHTS[menu.hook_height]);
            }
        }
        Item::HookClear if enter => {
            if let Some(hooks) = hooks.as_mut() {
                let n = hooks.0.len().saturating_sub(menu.hooks_added);
                hooks.0.truncate(n);
                menu.hooks_added = 0;
                menu.message = "test hooks removed".into();
            }
        }
        Item::Speed => {
            menu.speed = cycle(menu.speed, SPEEDS.len());
            time.set_relative_speed(SPEEDS[menu.speed]);
        }
        Item::Hitboxes => hurtboxes.0 = !hurtboxes.0,
        Item::Reset if enter => {
            // Wolf's reset also brings back the starting enemies (enemy::FightReset).
            pad.reset = true;
            menu.message = "reset".into();
        }
        Item::Map if !maps().is_empty() => {
            menu.map = cycle(menu.map, maps().len());
            if enter {
                let id = maps()[menu.map].0.clone();
                match go.as_mut() {
                    Some(m) => {
                        m.write(crate::map::GoToMap(id.clone()));
                        menu.message = format!("loading {id}");
                    }
                    None => menu.message = "no map system in this run".into(),
                }
            }
        }
        Item::Hour => {
            if let Some(l) = lighting.as_mut() {
                l.hour = ((l.hour + step as f32).round()).rem_euclid(24.0);
                l.applied = false;
            }
        }
        Item::DayCycle => {
            menu.day_speed = cycle(menu.day_speed, DAY_SPEEDS.len());
            if let Some(d) = day.as_mut() {
                d.hours_per_sec = DAY_SPEEDS[menu.day_speed].0;
            }
        }
        Item::Weather => {
            if let (Some(l), Some(lk)) = (lighting.as_mut(), looks.as_deref()) {
                let ws = weathers(lk);
                menu.weather = cycle(menu.weather.min(ws.len() - 1), ws.len());
                let (look, set) = ws[menu.weather];
                if let Some((_, dp)) = lk.looks.get(look) {
                    l.drawparam = dp.clone();
                }
                l.light_set = set;
                l.applied = false;
            }
        }
        Item::Sky => {
            if let Some(c) = sky.as_mut() {
                // None (the map's and hour's own), then each dome.
                let i = c.0.and_then(|d| crate::sky::DOMES.iter().position(|x| x.0 == d)).map_or(0, |i| i + 1);
                let n = cycle(i, crate::sky::DOMES.len() + 1);
                c.0 = (n > 0).then(|| crate::sky::DOMES[n - 1].0);
            }
        }
        _ => {}
    }
}

/// SkillParam 280 Flowing Water (guard posture damage rates, combat.rs guard_skill_rate).
const FLOWING_WATER: i64 = 280;

/// Writes enemy.chr / enemy.npc_row to config.toml and starts a fresh game process: the enemy's
/// data, model and AI are loaded at startup.
fn restart_with(menu: &mut DebugMenu, config: &mut GameConfig, exit: &mut MessageWriter<AppExit>) {
    let e = &enemy_choices()[menu.enemy_type];
    let (chr, outfits) = (e.chr.as_str(), &e.outfits);
    let row = outfits[menu.outfit.min(outfits.len() - 1)].0;
    let path = crate::paths::root().join("config.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        menu.message = "config.toml not readable".into();
        return;
    };
    match set_enemy_keys(&text, chr, row) {
        Some(new) if std::fs::write(&path, &new).is_ok() => {}
        _ => {
            menu.message = "could not write config.toml".into();
            return;
        }
    }
    config.enemy.chr = chr.to_string();
    config.enemy.npc_row = Some(row);
    if let Ok(exe) = std::env::current_exe() {
        if std::process::Command::new(exe).current_dir(crate::paths::root()).spawn().is_ok() {
            exit.write(AppExit::Success);
            return;
        }
    }
    menu.message = "restart failed - start the game again".into();
}

/// config.toml text with `chr` and `npc_row` set in the [enemy] table (added when missing).
fn set_enemy_keys(text: &str, chr: &str, row: i64) -> Option<String> {
    let mut out = Vec::new();
    let (mut in_enemy, mut seen_enemy, mut chr_done, mut row_done) = (false, false, false, false);
    let flush = |out: &mut Vec<String>, chr_done: &mut bool, row_done: &mut bool| {
        if !*chr_done {
            out.push(format!("chr = \"{chr}\""));
            *chr_done = true;
        }
        if !*row_done {
            out.push(format!("npc_row = {row}"));
            *row_done = true;
        }
    };
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            if in_enemy {
                flush(&mut out, &mut chr_done, &mut row_done);
            }
            in_enemy = t.starts_with("[enemy]");
            seen_enemy |= in_enemy;
        }
        if in_enemy && t.starts_with("chr") && t[3..].trim_start().starts_with('=') {
            out.push(format!("chr = \"{chr}\""));
            chr_done = true;
            continue;
        }
        if in_enemy && t.starts_with("npc_row") {
            out.push(format!("npc_row = {row}"));
            row_done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_enemy {
        flush(&mut out, &mut chr_done, &mut row_done);
    }
    if !seen_enemy {
        return None;
    }
    Some(out.join("\n") + "\n")
}

pub(crate) fn player_cheats(menu: Res<DebugMenu>, config: Res<GameConfig>, mut player: Query<(&mut Actor, &mut Player)>) {
    for (mut a, mut p) in &mut player {
        // Topped up every tick to the carried count (at least the default 15).
        if menu.infinite_emblems {
            p.emblems = p.emblems.max(config.player.spirit_emblems.max(15));
        }
        if menu.god {
            a.hp = a.hp_max;
            // The killing hit's death pick (combat.rs) is spent unplayed: drop it, or a later real
            // death would play this one's (a fire kill's GroundDeathStartFire for a plain hit).
            a.death_ref = 0;
            a.death_hit = None;
        }
        // Before Resolve's next hits too (actor.rs add_posture): zeroing here alone came after
        // Resolve, so one blocked hit that filled the bar still broke the guard (round 65).
        a.no_posture = menu.no_posture;
        if menu.no_posture {
            a.posture = 0.0;
        }
    }
}

fn draw_menu(
    menu: Res<DebugMenu>,
    dbg: Res<EnemyDebug>,
    config: Res<GameConfig>,
    combat: Res<Combat>,
    time: Res<Time<Virtual>>,
    hurtboxes: Res<crate::combat::ShowHurtboxes>,
    mut text: Query<(&mut Text, &mut Node), With<MenuText>>,
    (lighting, looks, sky): (Option<Res<crate::map::MapLighting>>, Option<Res<crate::map::MapLooks>>, Option<Res<crate::sky::SkyChoice>>),
) {
    let Ok((mut t, mut node)) = text.single_mut() else { return };
    let want = if menu.open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
    if !menu.open {
        return;
    }
    let attacks = attack_list(combat.enemy0());
    let on = |b: bool| if b { "ON" } else { "off" };
    // The kind by the attack's AtkParam (enemy::perilous_kind), not the warning judge.
    let kind = |p: Option<i64>| match p.map(|_| crate::enemy::perilous_kind(combat.enemy0(), &dbg.attack)) {
        Some("sweep") => " [perilous sweep]",
        Some("thrust") => " [perilous thrust]",
        Some("grab") => " [perilous grab]",
        Some(_) => " [perilous]",
        None => "",
    };
    let e = &enemy_choices()[menu.enemy_type];
    let (chr, name, outfits) = (e.chr.as_str(), e.name.as_str(), &e.outfits);
    let pending = chr != config.enemy.chr || Some(outfits[menu.outfit].0) != config.enemy.npc_row.or(Some(combat.foe0().npc_row));
    let value = |it: Item| -> String {
        match it {
            Item::AiMode => format!("AI: {}", AI_MODES.iter().find(|m| m.0 == dbg.mode).map_or("?", |m| m.1)),
            Item::Attack => {
                let p = attacks.iter().find(|a| a.0 == dbg.attack).and_then(|a| a.1);
                format!("attack: {}{}", if dbg.attack.is_empty() { "-" } else { &dbg.attack }, kind(p))
            }
            Item::Interval => format!("attack interval: {:.2} s", dbg.interval),
            Item::EnemyType => format!("type: {name} ({chr}){}", if pending { "  - Enter: restart" } else { "" }),
            Item::Outfit => format!("outfit: {} {}{}", outfits[menu.outfit].0, outfits[menu.outfit].1, if pending { "  - Enter: restart" } else { "" }),
            Item::EnemyHeal => "heal enemy".into(),
            Item::BreakPosture => "break enemy posture".into(),
            Item::DeathblowNow => "deathblow now (front)".into(),
            Item::DeathblowBehind => "deathblow now (from behind)".into(),
            Item::PlungeNow => "plunge deathblow now (from above)".into(),
            Item::KickDownNow => "kick-down deathblow now (after a head kick)".into(),
            Item::VaultNow => "vault over a broken enemy now (jump)".into(),
            Item::StealthSetup => "stealth: enemy unaware, back turned".into(),
            Item::StealthFar => "stealth: enemy 22 m away, unaware, facing you".into(),
            Item::Art => format!("combat art: {} ({})", combat.weapon_name(config.player.combat_art), config.player.combat_art),
            Item::Tool1 | Item::Tool2 | Item::Tool3 => {
                let n = match it {
                    Item::Tool1 => 0,
                    Item::Tool2 => 1,
                    _ => 2,
                };
                let id = slots(menu.tool_slots, &config.player.prosthetics)[n];
                // Every tool's HKS branch is ported (prosthetic.rs; sim_tests every_tool_*).
                format!("prosthetic slot {}: {}", n + 1, if id == 0 { "empty".to_string() } else { format!("{} ({id})", combat.weapon_name(id)) })
            }
            Item::God => format!("god mode (no HP loss): {}", on(menu.god)),
            Item::NoPosture => format!("no posture damage: {}", on(menu.no_posture)),
            Item::InfiniteEmblems => format!("infinite Spirit Emblems: {}", on(menu.infinite_emblems)),
            Item::Refill => "refill gourds / emblems / resurrections".into(),
            Item::Sheathe => "sheathe / draw the sword (X)".into(),
            Item::FlowingWater => format!("skill Flowing Water (SkillParam 280): {}", on(config.player.skills.contains(&FLOWING_WATER))),
            Item::DropHeight => format!("fall test height: {} m  (20 m = kill limit starts, 45 m kills)", DROP_HEIGHTS[menu.drop_height]),
            Item::DropNow => "lift Wolf to that height and drop".into(),
            Item::HookHeight => format!("test hook height: {} m up, {} m ahead", HOOK_HEIGHTS[menu.hook_height], HOOK_AHEAD),
            Item::HookNow => format!("place a test hook ({} placed)", menu.hooks_added),
            Item::HookClear => "remove the test hooks".into(),
            Item::Speed => format!("game speed: x{}", time.relative_speed()),
            Item::Hitboxes => format!("hitboxes / hurtboxes (H): {}", on(hurtboxes.0)),
            Item::Reset => "reset fight".into(),
            Item::Map => match maps().get(menu.map) {
                Some((id, label)) => format!("map: {label}{}", if crate::map::loaded_map().as_deref() == Some(id.as_str()) { "  (here)" } else { "  - Enter: go" }),
                None => "map: none exported".into(),
            },
            Item::Hour => match lighting.as_deref() {
                Some(l) => {
                    let h = l.hour.rem_euclid(24.0);
                    format!("time of day: {:02}:{:02}", h.floor() as u32, ((h.fract() * 60.0).round() as u32).min(59))
                }
                None => "time of day: no map".into(),
            },
            Item::DayCycle => format!("day-night cycle: {}", DAY_SPEEDS[menu.day_speed].1),
            Item::Weather => match (lighting.as_deref(), looks.as_deref()) {
                (Some(l), Some(lk)) => {
                    let ws = weathers(lk);
                    let (look, set) = ws[menu.weather.min(ws.len() - 1)];
                    // The light set's sun (DiffColor0 weight) at the hour, as a hint of the look.
                    let sun = l.at("LightSet", "DiffColor0", set).and_then(|c| c.get(3).copied()).map_or(String::new(), |w| format!(", sun x{w:.1}"));
                    format!("weather: light set {set}{sun}, {}  ({}/{})", look_name(lk.looks.get(look).map_or("base", |x| x.0.as_str())), menu.weather.min(ws.len() - 1) + 1, ws.len())
                }
                _ => "weather: no map".into(),
            },
            Item::Sky => match sky.as_deref().and_then(|c| c.0) {
                Some(d) => format!("sky: {} ({d})", crate::sky::DOMES.iter().find(|x| x.0 == d).map_or("?", |x| x.1)),
                None => "sky: by map and hour".into(),
            },
        }
    };
    let mut s = String::from("DEBUG MENU  (F1 close, arrows select / change, Enter run)\n");
    let mut i = 0;
    for (title, its) in SECTIONS {
        s += &format!("\n{title}\n");
        for &it in its {
            let cur = if i == menu.cursor { "> " } else { "  " };
            s += &format!("{cur}{}\n", value(it));
            i += 1;
        }
    }
    if !menu.message.is_empty() {
        s += &format!("\n{}", menu.message);
    }
    if t.0 != s {
        t.0 = s;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_tool_set_in_slot_3_stays_in_slot_3() {
        // Default config: one tool in slot 1. Fill slot 3, then empty slot 1.
        let mut config = vec![70000];
        let mut layout = super::slots([0; 3], &config);
        layout[2] = 70100;
        config = super::pack(layout);
        assert_eq!(config, vec![70000, 70100]);
        assert_eq!(super::slots(layout, &config), [70000, 0, 70100]);
        layout[0] = 0;
        config = super::pack(layout);
        assert_eq!(super::slots(layout, &config), [0, 0, 70100]);
        // A list changed elsewhere (F5) is shown packed again.
        assert_eq!(super::slots(layout, &[71000, 72000]), [71000, 72000, 0]);
    }

    #[test]
    fn enemy_keys_are_set_in_the_enemy_table() {
        let text = "[camera]\nx = 1\n\n[enemy]\nchr = \"c1020\"\nattack_range = 3.0\n\n[input]\nbuffer = 0.0\n";
        let out = super::set_enemy_keys(text, "c1010", 10100000).unwrap();
        assert!(out.contains("[enemy]\nchr = \"c1010\"\nattack_range = 3.0\n\nnpc_row = 10100000\n[input]"), "{out}");
        let again = super::set_enemy_keys(&out, "c1020", 10203010).unwrap();
        assert_eq!(again.matches("npc_row").count(), 1);
        assert!(again.contains("npc_row = 10203010") && again.contains("chr = \"c1020\""));
    }
}
