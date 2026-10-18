//! The Sculptor's Workshop (the user's captures): Fit New Prosthetic Tool (Create Arm Tools: a
//! tool is made from its key material, goods 97X0 -> EquipParamWeapon 7X000, e.g. the Shuriken
//! Wheel 9700 -> Loaded Shuriken 70000) and Upgrade Prosthetic Tool (the Prosthetic Tool
//! Upgrades tree: SkillParam rows 700+, laid out by dispRow / dispCol and joined by parent1-3,
//! each giving its acquireWeaponId, the upgraded tool, e.g. 701 -> 70100 Spinning Shuriken).
//! An upgrade costs its virtual weapon's baseChangePrice in Sen and its EquipMtrlSetParam
//! materials (materialCate 4: goods). Owned tools and upgrades are config player.owned_tools /
//! player.upgrades; the Equipment's tool slots offer the owned tools and the upgrades' tools.
//!
//! Opened by talking to the Sculptor (sculptor.rs) on the Construct.
//! gap: an upgrade with two parents asks for either (as the skill tree).

use std::sync::OnceLock;

use bevy::prelude::*;

use crate::game_menu::{abs, any_icon, fill_row, image, sprite, text, ItemTexts, Tables, DIM, ORANGE, TEXT};
use crate::items::param;

/// The ten base tools (EquipParamWeapon), in the Create Arm Tools order of the game's list.
pub(crate) const TOOLS: [i64; 10] = [70000, 71000, 72000, 73000, 74000, 75000, 76000, 77000, 78000, 79000];

/// The key material a base tool is made from (97X0 for 7X000).
pub(crate) fn material_of(tool: i64) -> i64 {
    9700 + (tool - 70000) / 1000 * 10
}

/// One upgrade of the tree.
#[derive(Clone, Debug)]
pub(crate) struct Upgrade {
    pub(crate) id: i64,
    pub(crate) row: u32,
    pub(crate) col: u32,
    pub(crate) parents: Vec<i64>,
    /// The upgraded tool (EquipParamWeapon): its name, text and icon.
    pub(crate) weapon: i64,
    pub(crate) sen: u32,
    /// (goods, count).
    pub(crate) materials: Vec<(i64, u32)>,
}

impl Upgrade {
    /// The base tool it upgrades.
    pub(crate) fn tool(&self) -> i64 {
        self.weapon / 1000 * 1000
    }
}

/// The upgrade tree from the params, once.
pub(crate) fn upgrades() -> &'static [Upgrade] {
    static TREE: OnceLock<Vec<Upgrade>> = OnceLock::new();
    TREE.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/SkillParam.json");
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut out: Vec<Upgrade> = v["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                let id = r["id"].as_i64()?;
                let weapon = r["acquireWeaponId"].as_i64().filter(|w| *w > 0)?;
                if id < 700 || r["menuPageNo"].as_i64() != Some(0) {
                    return None;
                }
                let vw = r["virtualWeaponId"].as_i64().unwrap_or(-1);
                let w = param("EquipParamWeapon", vw);
                let set = param("EquipMtrlSetParam", w["materialSetId"].as_i64().unwrap_or(vw));
                let materials = (1..=5)
                    .filter(|k| set[format!("materialCate{k:02}").as_str()].as_i64() == Some(4))
                    .filter_map(|k| Some((set[format!("materialId{k:02}").as_str()].as_i64().filter(|g| *g > 0)?, set[format!("itemNum{k:02}").as_str()].as_u64()? as u32)))
                    .collect();
                Some(Upgrade {
                    id,
                    row: r["dispRow"].as_u64().unwrap_or(0) as u32,
                    col: r["dispCol"].as_u64().unwrap_or(0) as u32,
                    parents: (1..=3).filter_map(|k| r[format!("parent{k}").as_str()].as_i64()).filter(|p| *p > 0).collect(),
                    weapon,
                    sen: w["baseChangePrice"].as_u64().unwrap_or(0) as u32,
                    materials,
                })
            })
            .collect();
        out.sort_by_key(|u| (u.col, u.row, u.id));
        out
    })
}

/// Why an upgrade cannot be bought.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Block {
    Acquired,
    /// Its tool is not owned, or no parent acquired yet.
    Locked,
    /// Short of Sen or materials.
    Cost,
}

pub(crate) fn owned(config: &crate::config::GameConfig, u: &Upgrade) -> bool {
    config.player.upgrades.contains(&u.id)
}

pub(crate) fn check(config: &crate::config::GameConfig, u: &Upgrade) -> Result<(), Block> {
    if owned(config, u) {
        return Err(Block::Acquired);
    }
    let parent_ok = u.parents.is_empty() || u.parents.iter().any(|p| config.player.upgrades.contains(p));
    if !config.player.owned_tools.contains(&u.tool()) || !parent_ok {
        return Err(Block::Locked);
    }
    if config.player.sen < u.sen || u.materials.iter().any(|(g, n)| crate::items::count(config, *g) < *n) {
        return Err(Block::Cost);
    }
    Ok(())
}

/// Buys the upgrade: Sen and materials spent, the upgrade owned.
pub(crate) fn buy(config: &mut crate::config::GameConfig, u: &Upgrade) -> bool {
    if check(config, u).is_err() {
        return false;
    }
    config.player.sen -= u.sen;
    for (g, n) in &u.materials {
        if let Some(e) = config.player.items.iter_mut().find(|(id, _)| id == g) {
            e.1 = e.1.saturating_sub(*n);
        }
    }
    config.player.upgrades.push(u.id);
    true
}

/// Makes a base tool from its material (spent), if held.
pub(crate) fn craft(config: &mut crate::config::GameConfig, tool: i64) -> bool {
    let m = material_of(tool);
    if config.player.owned_tools.contains(&tool) || crate::items::count(config, m) == 0 {
        return false;
    }
    if let Some(e) = config.player.items.iter_mut().find(|(id, _)| *id == m) {
        e.1 -= 1;
    }
    config.player.owned_tools.push(tool);
    true
}

/// The tools the Equipment's slots may hold: owned base tools and the owned upgrades' tools.
pub(crate) fn equippable(config: &crate::config::GameConfig) -> Vec<i64> {
    let mut v: Vec<i64> = config.player.owned_tools.clone();
    v.extend(upgrades().iter().filter(|u| owned(config, u)).map(|u| u.weapon));
    v.sort();
    v.dedup();
    v
}

/// The cursor step on the tree (nearest node ahead, the cross axis counting extra).
pub(crate) fn step(from: usize, dx: i32, dy: i32) -> Option<usize> {
    let all = upgrades();
    let a = all.get(from)?;
    all.iter()
        .enumerate()
        .filter(|(i, _)| *i != from)
        .filter_map(|(i, n)| {
            let (x, y) = (n.col as i32 - a.col as i32, n.row as i32 - a.row as i32);
            let (along, across) = if dx != 0 { (x * dx, y) } else { (y * dy, x) };
            (along > 0).then_some((along * 2 + across.abs() * 3, i))
        })
        .min()
        .map(|(_, i)| i)
}

/// A tree node's button (its index in `upgrades`).
#[derive(Component)]
pub(crate) struct UpgradeButton(pub(crate) usize);

// The tree's layout, in % of the window (the user's 1280 x 720 capture).
const COL_X: f32 = 16.4;
const COL_STEP: f32 = 8.0;
const ROW_Y: f32 = 19.9;
const ROW_STEP: f32 = 8.83;
const NODE_W: f32 = 4.7;
const NODE_H: f32 = 8.3;
const LINE: Color = Color::srgba(0.05, 0.05, 0.05, 0.9);

fn center(u: &Upgrade) -> (f32, f32) {
    (COL_X + u.col as f32 * COL_STEP, ROW_Y + u.row as f32 * ROW_STEP)
}

/// The tool or upgrade's name and text (an upgrade shares its base's when it has none).
fn weapon_text(texts: &ItemTexts, id: i64) -> (String, String) {
    let name = [id, id / 1000 * 1000].iter().find_map(|i| texts.weapon_name.get(&(*i as u32))).cloned().unwrap_or_default();
    let info = [id, id / 1000 * 1000].iter().find_map(|i| texts.weapon_info.get(&(*i as u32))).cloned().unwrap_or_default();
    (name, info)
}

fn weapon_icon(tables: &Tables, id: i64) -> Option<String> {
    tables.weapon_icon.get(&id).or_else(|| tables.weapon_icon.get(&(id / 1000 * 1000))).and_then(|i| any_icon(*i))
}

/// The Sen held, top right (the coin and the count).
pub(crate) fn sen_plate(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, sen: u32, left: f32, top: f32) {
    root.spawn((abs(left, top, 11.5, 3.6), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.3, 0.3, 0.3)))).with_children(|b| {
        b.spawn(Node { justify_content: JustifyContent::SpaceBetween, padding: UiRect::axes(Val::Percent(5.0), Val::Px(0.0)), ..fill_row() }).with_children(|c| {
            c.spawn((Node { width: Val::Px(20.0), height: Val::Px(20.0), ..default() }, image(assets, sprite("sb_fe/MENU_Money"))));
            c.spawn(text(&sen.to_string(), 17.0, TEXT, font));
        });
    });
}

/// The Prosthetic Tool Upgrades screen (under the menu backdrop and title).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_tree(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, cursor: usize, asking: bool, config: &crate::config::GameConfig, texts: &ItemTexts, tables: &Tables) {
    let all = upgrades();
    sen_plate(root, assets, font, config.player.sen, 81.6, 8.0);
    // The column bands (the last darker) and the Shinobi Prosthetic at the root.
    let cols = all.iter().map(|u| u.col).max().unwrap_or(0) + 1;
    for c in 0..cols {
        let a = if c + 1 == cols { 0.32 } else { 0.14 };
        root.spawn((abs(COL_X + c as f32 * COL_STEP - 3.6, 13.6, 7.2, 75.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, a))));
    }
    let prosthetic = texts.goods_id("Shinobi Prosthetic").and_then(|id| tables.goods_icon.get(&id)).and_then(|i| any_icon(*i));
    root.spawn((abs(6.3, ROW_Y - 4.8, 5.4, 9.6), image(assets, prosthetic)));
    // The lines: parent's right edge (the root's for the first column) -> half way -> the child.
    for child in all {
        let (cx, cy) = center(child);
        let from: Vec<(f32, f32)> = if child.parents.is_empty() {
            vec![(11.7 - NODE_W / 2.0 + 0.6, ROW_Y)]
        } else {
            child.parents.iter().filter_map(|p| all.iter().find(|u| u.id == *p)).map(center).collect()
        };
        let n = from.len();
        for (k, (px, py)) in from.into_iter().enumerate() {
            let cy = cy + if n > 1 { (k as f32 - (n - 1) as f32 / 2.0) * 1.1 } else { 0.0 };
            let (x0, x1) = (px + NODE_W / 2.0, cx - NODE_W / 2.0);
            let mid = if child.parents.is_empty() { x1 - 1.4 } else { (px + cx) / 2.0 };
            let (t, w) = (0.32, 0.18);
            root.spawn((abs(x0, py - t / 2.0, (mid - x0 + w).max(0.0), t), BackgroundColor(LINE)));
            root.spawn((abs(mid, py.min(cy) - t / 2.0, w, (py - cy).abs() + t), BackgroundColor(LINE)));
            root.spawn((abs(mid, cy - t / 2.0, (x1 - mid).max(0.0), t), BackgroundColor(LINE)));
            root.spawn((Node { border_radius: BorderRadius::MAX, ..abs(x1 - 0.55, cy - 0.5, 0.55, 1.0) }, BackgroundColor(LINE)));
        }
    }
    for (i, u) in all.iter().enumerate() {
        let (x, y) = center(u);
        let has = owned(config, u);
        let tool_owned = config.player.owned_tools.contains(&u.tool());
        if i == cursor {
            let (gw, gh) = (NODE_W * 1.3, NODE_H * 1.3);
            root.spawn((Node { border: UiRect::all(Val::Px(3.0)), border_radius: BorderRadius::all(Val::Percent(18.0)), ..abs(x - gw / 2.0, y - gh / 2.0, gw, gh) }, BorderColor::all(ORANGE), BackgroundColor(ORANGE.with_alpha(0.3))));
        }
        let tint = if has || check(config, u).is_ok() { Color::WHITE } else if tool_owned { Color::srgb(0.6, 0.6, 0.6) } else { Color::srgb(0.3, 0.3, 0.3) };
        root.spawn((Button, abs(x - NODE_W / 2.0, y - NODE_H / 2.0, NODE_W, NODE_H), image(assets, sprite("sb_in_game/MENU_IconBase_02")), UpgradeButton(i))).with_children(|b| {
            b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(6.0), top: Val::Percent(6.0), width: Val::Percent(88.0), height: Val::Percent(88.0), ..default() }, image(assets, weapon_icon(tables, u.weapon)).with_color(tint)));
            if has {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(68.0), top: Val::Percent(-6.0), width: Val::Percent(36.0), height: Val::Percent(36.0), ..default() }, image(assets, sprite("sb_remodeling/MENU_BanArm"))));
            }
        });
    }
    // The upgrade under the cursor.
    let Some(u) = all.get(cursor) else { return };
    let (name, info) = weapon_text(texts, u.weapon);
    root.spawn((abs(60.8, 14.6, 33.0, 74.0), image(assets, sprite("sb_in_game/MENU_WP_Base"))));
    root.spawn((abs(61.6, 15.6, 31.4, 4.4), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.25, 0.25, 0.25)))).with_children(|b| {
        b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
            c.spawn(text(&name, 17.0, Color::WHITE, font));
        });
    });
    let status = match check(config, u) {
        Err(Block::Acquired) => "Upgrade Acquired",
        Err(Block::Locked) if !config.player.owned_tools.contains(&u.tool()) => "Tool not yet made",
        Err(Block::Locked) => "Requires the previous upgrade",
        Err(Block::Cost) => "Not enough Sen or materials",
        Ok(()) => "Upgrade available",
    };
    root.spawn((abs(62.4, 20.6, 19.0, 3.1), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgba(0.35, 0.35, 0.35, 0.9)))).with_children(|b| {
        b.spawn(Node { padding: UiRect::left(Val::Percent(3.5)), ..fill_row() }).with_children(|c| {
            c.spawn(text(status, 14.5, TEXT, font));
        });
    });
    root.spawn((abs(83.2, 20.8, 8.8, 15.4), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|b| {
        b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(8.0), top: Val::Percent(8.0), width: Val::Percent(84.0), height: Val::Percent(84.0), ..default() }, image(assets, weapon_icon(tables, u.weapon))));
    });
    let mut r = 41.8;
    while r < 66.0 {
        root.spawn((abs(62.8, r, 29.0, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18))));
        r += 3.05;
    }
    root.spawn(Node { overflow: Overflow::clip(), ..abs(62.9, 38.6, 29.0, 27.0) }).with_children(|c| {
        c.spawn(text(&info, 15.0, TEXT, font));
    });
    // The cost: each material held / needed, and the Sen.
    if !owned(config, u) {
        let mut y = 67.0;
        let line = |root: &mut ChildSpawnerCommands, y: f32, icon: Option<String>, label: String, have: u32, need: u32| {
            let short = have < need;
            root.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..abs(63.0, y, 29.0, 3.4) }).with_children(|r| {
                r.spawn((Node { width: Val::Px(24.0), height: Val::Px(24.0), ..default() }, image(assets, icon)));
                r.spawn((Node { flex_grow: 1.0, ..default() },)).with_children(|n| {
                    n.spawn(text(&label, 15.0, TEXT, font));
                });
                r.spawn(text(&format!("{have} / {need}"), 15.0, if short { Color::srgb(0.85, 0.35, 0.3) } else { TEXT }, font));
            });
        };
        for (g, n) in &u.materials {
            let gname = texts.goods_name.get(&(*g as u32)).cloned().unwrap_or_else(|| g.to_string());
            let icon = tables.goods_icon.get(g).and_then(|i| any_icon(*i));
            line(root, y, icon, gname, crate::items::count(config, *g), *n);
            y += 3.5;
        }
        line(root, y, sprite("sb_fe/MENU_Money"), "Sen".to_string(), config.player.sen, u.sen);
    }
    if asking {
        root.spawn((abs(0.0, 0.0, 100.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5))));
        root.spawn((abs(30.0, 38.0, 40.0, 22.0), image(assets, sprite("sb_dialog/MENU_Dialog_base")))).with_children(|d| {
            d.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: Val::Px(14.0), ..fill_row() }).with_children(|c| {
                c.spawn(text(&format!("Upgrade to \"{name}\"?"), 18.0, Color::WHITE, font));
                c.spawn(text("Enter: Upgrade      Esc: Cancel", 15.0, DIM, font));
            });
        });
    }
}

/// The Create Arm Tools rows: (row, detail) for each base tool, "Owned" or its material.
pub(crate) fn fit_rows(config: &crate::config::GameConfig, texts: &ItemTexts, tables: &Tables) -> Vec<(crate::game_menu::Row, crate::game_menu::Detail)> {
    TOOLS
        .iter()
        .map(|&t| {
            let (name, info) = weapon_text(texts, t);
            let icon = weapon_icon(tables, t);
            let m = material_of(t);
            let mname = texts.goods_name.get(&(m as u32)).cloned().unwrap_or_default();
            let value = if config.player.owned_tools.contains(&t) {
                "Owned".to_string()
            } else if crate::items::count(config, m) > 0 {
                "Craft".to_string()
            } else {
                "-".to_string()
            };
            let stats = if config.player.owned_tools.contains(&t) { vec![] } else { vec![("Required".to_string(), format!("{mname} {}/1", crate::items::count(config, m).min(1)))] };
            (crate::game_menu::Row { icon: icon.clone(), name: name.clone(), value: Some(value), ..default() }, crate::game_menu::Detail { name, icon, art: None, stats, info })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_upgrade_tree_matches_the_capture() {
        let all = upgrades();
        // 701 Spinning Shuriken at the top left (Scrap Iron x3, 200 Sen); six columns.
        let u701 = all.iter().find(|u| u.id == 701).unwrap();
        assert_eq!((u701.row, u701.col, u701.weapon, u701.tool(), u701.sen), (0, 0, 70100, 70000, 200));
        assert_eq!(u701.materials, vec![(6000, 3)]);
        assert!(all.len() >= 30, "{}", all.len());
        assert_eq!(all.iter().map(|u| u.col).max(), Some(5));
        assert_eq!(material_of(78000), 9780);
    }

    #[test]
    fn an_upgrade_costs_its_sen_and_materials_after_its_parent() {
        let mut config = crate::config::GameConfig::load().expect("config.toml");
        config.player.owned_tools = vec![70000];
        config.player.sen = 1000;
        config.player.items = vec![(6000, 5), (6010, 5)];
        let u = |id: i64| upgrades().iter().find(|u| u.id == id).unwrap().clone();
        assert_eq!(check(&config, &u(711)), Err(Block::Locked), "its tool is not made");
        assert_eq!(check(&config, &u(731)), Err(Block::Locked), "its parent 701 is not owned");
        assert!(buy(&mut config, &u(701)));
        assert_eq!((config.player.sen, crate::items::count(&config, 6000)), (800, 2));
        assert_eq!(check(&config, &u(701)), Err(Block::Acquired));
        assert!(equippable(&config).contains(&70100));
    }
}
