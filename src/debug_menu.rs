//! Debug menu (F1): practice and test controls in one organised panel.
//!   Up / Down select, Left / Right change a value, Enter runs an action, F1 closes (the numpad
//!   8 / 2 / 4 / 6 work as arrows too).
//! Sections: Enemy (AI mode, drill attack and interval, type and outfit - those two restart the
//! game with the choice written to config.toml), Deathblow, Player, Simulation.

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet};
use crate::config::GameConfig;
use crate::data::Combat;
use crate::enemy::{AiMode, Enemy, EnemyDebug, attack_list};
use crate::player::{Action, PadInput, Player};

/// Enemy characters and their NpcParam outfit rows (all exported by sekiro-extract).
const ENEMIES: [(&str, &str, &[(i64, &str)]); 2] = [
    (
        "c1020",
        "Samurai General",
        &[(10203010, "no haori (as fought live)"), (10200010, "castle, with haori"), (10202000, "Honjo, with haori"), (10202070, "Honjo, no haori")],
    ),
    ("c1010", "Ochimusha", &[(10100000, "one-handed sword")]),
];
const SPEEDS: [f32; 4] = [1.0, 0.5, 0.25, 0.1];
const AI_MODES: [(AiMode, &str); 5] = [
    (AiMode::Real, "real AI"),
    (AiMode::Passive, "passive (walks up, never attacks)"),
    (AiMode::Perilous, "perilous attacks only"),
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
    God,
    NoPosture,
    Refill,
    FlowingWater,
    Sheathe,
    Speed,
    Hitboxes,
    Reset,
}

const SECTIONS: [(&str, &[Item]); 4] = [
    ("ENEMY", &[Item::AiMode, Item::Attack, Item::Interval, Item::EnemyType, Item::Outfit, Item::EnemyHeal]),
    ("DEATHBLOW", &[Item::BreakPosture, Item::DeathblowNow, Item::DeathblowBehind]),
    ("PLAYER", &[Item::God, Item::NoPosture, Item::Refill, Item::FlowingWater, Item::Sheathe]),
    ("SIMULATION", &[Item::Speed, Item::Hitboxes, Item::Reset]),
];

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
    speed: usize,
    /// Pending type / outfit choice (applied by restarting).
    enemy_type: usize,
    outfit: usize,
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
            .add_systems(FixedUpdate, player_cheats.after(ActorSet::Resolve));
    }
}

fn spawn_menu(mut commands: Commands, config: Res<GameConfig>, mut menu: ResMut<DebugMenu>) {
    menu.enemy_type = ENEMIES.iter().position(|e| e.0 == config.enemy.chr).unwrap_or(0);
    menu.outfit = config.enemy.npc_row.and_then(|r| ENEMIES[menu.enemy_type].2.iter().position(|o| o.0 == r)).unwrap_or(0);
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
    mut player: Query<(&mut Player, &mut Actor, &Transform), Without<Enemy>>,
    mut enemies: Query<(&mut Enemy, &mut Actor, &mut Transform), Without<Player>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::F1) {
        menu.open = !menu.open;
    }
    // Real AI vs passive is the enemy's own switch (T): show what it is.
    if matches!(dbg.mode, AiMode::Real | AiMode::Passive) {
        if let Some((e, _, _)) = enemies.iter().next() {
            let want = if e.aggressive { AiMode::Real } else { AiMode::Passive };
            if dbg.mode != want {
                dbg.mode = want;
            }
        }
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
    let cycle = |i: usize, n: usize| ((i as i32 + step).rem_euclid(n as i32)) as usize;
    let attacks = attack_list(&combat.enemy);
    match list[menu.cursor] {
        Item::AiMode => {
            let i = AI_MODES.iter().position(|m| m.0 == dbg.mode).unwrap_or(0);
            dbg.mode = AI_MODES[cycle(i, AI_MODES.len())].0;
            // The real AI and passive mode are the enemy's own aggressive switch (T).
            for (mut e, _, _) in &mut enemies {
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
                menu.enemy_type = cycle(menu.enemy_type, ENEMIES.len());
                menu.outfit = 0;
            }
            if enter {
                restart_with(&mut menu, &mut config, &mut exit);
            }
        }
        Item::Outfit => {
            if step != 0 {
                menu.outfit = cycle(menu.outfit, ENEMIES[menu.enemy_type].2.len());
            }
            if enter {
                restart_with(&mut menu, &mut config, &mut exit);
            }
        }
        Item::EnemyHeal if enter => {
            for (_, mut a, _) in &mut enemies {
                a.hp = a.hp_max;
                a.posture = 0.0;
            }
            menu.message = "enemy healed".into();
        }
        Item::BreakPosture if enter => {
            for (mut e, mut a, _) in &mut enemies {
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
                for (mut e, mut a, mut tf) in &mut enemies {
                    tf.translation = ptf.translation + fwd * if behind { 1.2 } else { 1.6 };
                    a.yaw = if behind { pa.yaw } else { pa.yaw + std::f32::consts::PI };
                    a.posture = a.posture_max;
                    e.on_posture_break(&mut a, &combat, false, 4.0);
                }
                pad.press(Action::Attack);
                menu.message = if behind { "deathblow from behind" } else { "deathblow" }.into();
            }
        }
        Item::God => menu.god = !menu.god,
        Item::NoPosture => menu.no_posture = !menu.no_posture,
        Item::Refill if enter => {
            for (mut p, _, _) in &mut player {
                p.gourd = config.player.gourd_charges;
                p.emblems = config.player.spirit_emblems;
                p.resurrections = config.player.resurrections;
            }
            menu.message = "gourds, emblems and resurrections refilled".into();
        }
        Item::Sheathe if enter => {
            pad.sheathe = true;
            menu.message = "sheathe / draw (X)".into();
        }
        Item::FlowingWater => {
            if config.player.skills.contains(&FLOWING_WATER) {
                config.player.skills.retain(|s| *s != FLOWING_WATER);
            } else {
                config.player.skills.push(FLOWING_WATER);
            }
        }
        Item::Speed => {
            menu.speed = cycle(menu.speed, SPEEDS.len());
            time.set_relative_speed(SPEEDS[menu.speed]);
        }
        Item::Hitboxes => hurtboxes.0 = !hurtboxes.0,
        Item::Reset if enter => {
            pad.reset = true;
            for (_, mut a, mut tf) in &mut enemies {
                a.hp = a.hp_max;
                a.posture = 0.0;
                tf.translation = Vec3::new(0.0, tf.translation.y, -6.0);
            }
            menu.message = "reset".into();
        }
        _ => {}
    }
}

/// SkillParam 280 Flowing Water (guard posture damage rates, combat.rs guard_skill_rate).
const FLOWING_WATER: i64 = 280;

/// Writes enemy.chr / enemy.npc_row to config.toml and starts a fresh game process: the enemy's
/// data, model and AI are loaded at startup.
fn restart_with(menu: &mut DebugMenu, config: &mut GameConfig, exit: &mut MessageWriter<AppExit>) {
    let (chr, _, outfits) = ENEMIES[menu.enemy_type];
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

fn player_cheats(menu: Res<DebugMenu>, mut player: Query<&mut Actor, With<Player>>) {
    for mut a in &mut player {
        if menu.god {
            a.hp = a.hp_max;
        }
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
) {
    let Ok((mut t, mut node)) = text.single_mut() else { return };
    let want = if menu.open { Display::Flex } else { Display::None };
    if node.display != want {
        node.display = want;
    }
    if !menu.open {
        return;
    }
    let attacks = attack_list(&combat.enemy);
    let on = |b: bool| if b { "ON" } else { "off" };
    let kind = |p: Option<i64>| match p {
        Some(980) => " [perilous sweep]",
        Some(982) => " [perilous thrust]",
        Some(983) => " [perilous grab]",
        Some(_) => " [perilous]",
        None => "",
    };
    let (chr, name, outfits) = ENEMIES[menu.enemy_type];
    let pending = chr != config.enemy.chr || Some(outfits[menu.outfit].0) != config.enemy.npc_row.or(Some(combat.foe.npc_row));
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
            Item::God => format!("god mode (no HP loss): {}", on(menu.god)),
            Item::NoPosture => format!("no posture damage: {}", on(menu.no_posture)),
            Item::Refill => "refill gourds / emblems / resurrections".into(),
            Item::Sheathe => "sheathe / draw the sword (X)".into(),
            Item::FlowingWater => format!("skill Flowing Water (SkillParam 280): {}", on(config.player.skills.contains(&FLOWING_WATER))),
            Item::Speed => format!("game speed: x{}", time.relative_speed()),
            Item::Hitboxes => format!("hitboxes / hurtboxes (H): {}", on(hurtboxes.0)),
            Item::Reset => "reset fight".into(),
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
    fn enemy_keys_are_set_in_the_enemy_table() {
        let text = "[camera]\nx = 1\n\n[enemy]\nchr = \"c1020\"\nattack_range = 3.0\n\n[input]\nbuffer = 0.0\n";
        let out = super::set_enemy_keys(text, "c1010", 10100000).unwrap();
        assert!(out.contains("[enemy]\nchr = \"c1010\"\nattack_range = 3.0\n\nnpc_row = 10100000\n[input]"), "{out}");
        let again = super::set_enemy_keys(&out, "c1020", 10203010).unwrap();
        assert_eq!(again.matches("npc_row").count(), 1);
        assert!(again.contains("npc_row = 10203010") && again.contains("chr = \"c1020\""));
    }
}
