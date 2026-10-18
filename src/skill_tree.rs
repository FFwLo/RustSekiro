//! The Sculptor's Idol's Acquire Skills: the five skill trees (SkillParam menuPageNo 0-4: Shinobi,
//! Prosthetic, Ashina, Temple and Mushin Arts) laid out as the game's (the user's captures:
//! dispCol left to right, dispRow top to bottom), joined by lines from parent1-3, with the costs
//! (EquipMtrlSetParam of the skill's virtual weapon: itemNum01 of goods 1200, the skill point),
//! the learned seal (MENU_BanSkill) and the skills not yet revealed (an empty octagon until one
//! of the skill's unlovkViewSkill0-7 is learned; a skill with none is always shown).
//!
//! Mushin Arts' bottom row (400-403) repeats the other trees' last skills (the same virtual
//! weapon): those nodes mirror the real ones and are not acquired there.
//!
//! Each page requires its Esoteric Text (EquipParamGoods 2920-2924, in the same order).
//! gap: a skill with two parents asks for one of them (a Mushin skill for all): the game's
//! parent rule is not read from its code.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;

use crate::game_menu::{abs, any_icon, fill_row, image, sprite, text, ItemTexts, Tables, DIM, ORANGE, TEXT};

/// The tree pages: (name, the tab icon in SB_InGame).
pub(crate) const PAGES: [(&str, &str); 5] = [
    ("Shinobi Arts", "MENU_TB_Sinobi"),
    ("Prosthetic Arts", "MENU_TB_ArmSinobi"),
    ("Ashina Arts", "MENU_TB_Ashina"),
    ("Temple Arts", "MENU_TB_Temlpe"),
    ("Mushin Arts", "MENU_TB_Coposite"),
];

/// EquipParamGoods: Shinobi, Prosthetic, Ashina, Senpou and Mushin Esoteric Texts. The item
/// descriptions name the school each text opens; these are permanent key items, not consumed.
pub(crate) const TEXT_GOODS: [i64; 5] = [2920, 2921, 2922, 2923, 2924];

pub(crate) fn has_text(page: usize, items: &[(i64, u32)]) -> bool {
    TEXT_GOODS.get(page).is_some_and(|id| items.iter().any(|(goods, count)| goods == id && *count > 0))
}

/// Actual page ids, kept in school order even when only some books have been found.
pub(crate) fn available_pages(items: &[(i64, u32)]) -> Vec<usize> {
    (0..PAGES.len()).filter(|&page| has_text(page, items)).collect()
}

/// The skill point (EquipParamGoods 1200), the material every skill costs.
const SKILL_POINT_GOODS: i64 = 1200;

/// One node of a tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeSkill {
    pub(crate) id: i64,
    pub(crate) page: usize,
    pub(crate) row: u32,
    pub(crate) col: u32,
    pub(crate) parents: Vec<i64>,
    /// unlovkViewSkill0-7: learning any of these shows this skill.
    pub(crate) revealed_by: Vec<i64>,
    /// The virtual weapon (EquipParamWeapon): the name, text, icon and cost.
    pub(crate) weapon: i64,
    pub(crate) cost: u32,
    /// A Mushin Arts copy of another tree's skill: that skill's id.
    pub(crate) mirror: Option<i64>,
    /// Drawn in the ornate frame (MENU_Skill_Ougi): the Mushin skills and the trees' last skills.
    pub(crate) secret: bool,
}

/// Why a skill cannot be acquired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Block {
    /// This school's Esoteric Text has not been found.
    Text,
    Learned,
    /// A Mushin copy: acquired on its own tree.
    Mirror,
    /// Not revealed, or no parent (Mushin: not every parent) learned yet.
    Locked,
    /// Short of skill points.
    Points,
}

/// The trees from the params (extracted/json/params), once.
pub(crate) fn tree() -> &'static [TreeSkill] {
    static TREE: OnceLock<Vec<TreeSkill>> = OnceLock::new();
    TREE.get_or_init(|| {
        let rows = |name: &str| -> Vec<serde_json::Value> {
            let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
            let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            v["rows"].as_array().cloned().unwrap_or_default()
        };
        let weapons: HashMap<i64, serde_json::Value> = rows("EquipParamWeapon").into_iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect();
        let mtrl: HashMap<i64, serde_json::Value> = rows("EquipMtrlSetParam").into_iter().filter_map(|r| Some((r["id"].as_i64()?, r))).collect();
        build_tree(&rows("SkillParam"), &weapons, &mtrl)
    })
}

/// The nodes of the SkillParam rows: pages 0-4 and ids below 700 (700+ are the prosthetic tools'
/// upgrades, the Sculptor's other tree).
pub(crate) fn build_tree(skills: &[serde_json::Value], weapons: &HashMap<i64, serde_json::Value>, mtrl: &HashMap<i64, serde_json::Value>) -> Vec<TreeSkill> {
    let ids = |r: &serde_json::Value, keys: &[String]| -> Vec<i64> { keys.iter().filter_map(|k| r[k.as_str()].as_i64()).filter(|&i| i > 0).collect() };
    let parent_keys: Vec<String> = (1..=3).map(|i| format!("parent{i}")).collect();
    let view_keys: Vec<String> = (0..8).map(|i| format!("unlovkViewSkill{i}")).collect();
    let cost = |weapon: i64| -> u32 {
        let set = weapons.get(&weapon).and_then(|w| w["materialSetId"].as_i64()).unwrap_or(weapon);
        let m = mtrl.get(&set);
        (1..=6)
            .filter_map(|k| m.filter(|m| m[format!("materialId{k:02}").as_str()].as_i64() == Some(SKILL_POINT_GOODS)).and_then(|m| m[format!("itemNum{k:02}").as_str()].as_u64()))
            .sum::<u64>() as u32
    };
    let mut out: Vec<TreeSkill> = skills
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_i64()?;
            let page = r["menuPageNo"].as_i64()?;
            if !(0..5).contains(&page) || id >= 700 {
                return None;
            }
            let weapon = r["virtualWeaponId"].as_i64()?;
            Some(TreeSkill {
                id,
                page: page as usize,
                row: r["dispRow"].as_u64().unwrap_or(0) as u32,
                col: r["dispCol"].as_u64().unwrap_or(0) as u32,
                parents: ids(r, &parent_keys),
                revealed_by: ids(r, &view_keys),
                weapon,
                cost: cost(weapon),
                mirror: None,
                secret: false,
            })
        })
        .collect();
    // Mushin's copies of the other trees' skills, and the ornate frame for both.
    let by_weapon: HashMap<i64, i64> = out.iter().filter(|s| s.page < 4).map(|s| (s.weapon, s.id)).collect();
    for s in out.iter_mut().filter(|s| s.page == 4) {
        s.mirror = by_weapon.get(&s.weapon).copied();
        s.secret = true;
    }
    let mirrored: Vec<i64> = out.iter().filter_map(|s| s.mirror).collect();
    for s in out.iter_mut().filter(|s| mirrored.contains(&s.id)) {
        s.secret = true;
    }
    out.sort_by_key(|s| (s.page, s.col, s.row, s.id));
    out
}

fn find(tree: &[TreeSkill], id: i64) -> Option<&TreeSkill> {
    tree.iter().find(|s| s.id == id)
}

/// Learned: in `learned` (config player.skills), or a Mushin copy of a learned skill.
pub(crate) fn is_learned(tree: &[TreeSkill], id: i64, learned: &[i64]) -> bool {
    let id = find(tree, id).and_then(|s| s.mirror).unwrap_or(id);
    learned.contains(&id)
}

/// Shown in full (not the empty octagon): learned, a skill with no revealers, or one a learned
/// skill reveals. A Mushin copy shows as its skill does.
pub(crate) fn is_revealed(tree: &[TreeSkill], s: &TreeSkill, learned: &[i64]) -> bool {
    if let Some(m) = s.mirror.and_then(|m| find(tree, m)) {
        return is_revealed(tree, m, learned);
    }
    is_learned(tree, s.id, learned) || s.revealed_by.is_empty() || s.revealed_by.iter().any(|&r| is_learned(tree, r, learned))
}

/// Whether `s` can be acquired now with `points` and the school's text in the bag.
pub(crate) fn check(tree: &[TreeSkill], s: &TreeSkill, learned: &[i64], points: u32, items: &[(i64, u32)]) -> Result<(), Block> {
    if !has_text(s.page, items) {
        return Err(Block::Text);
    }
    if s.mirror.is_some() {
        return Err(if is_learned(tree, s.id, learned) { Block::Learned } else { Block::Mirror });
    }
    if is_learned(tree, s.id, learned) {
        return Err(Block::Learned);
    }
    let parents_ok = if s.parents.is_empty() {
        true
    } else if s.page == 4 {
        s.parents.iter().all(|&p| is_learned(tree, p, learned))
    } else {
        s.parents.iter().any(|&p| is_learned(tree, p, learned))
    };
    if !parents_ok || !is_revealed(tree, s, learned) {
        return Err(Block::Locked);
    }
    if points < s.cost {
        return Err(Block::Points);
    }
    Ok(())
}

/// The nodes of a page, in cursor order (column, then row).
pub(crate) fn page(tree: &[TreeSkill], page: usize) -> Vec<&TreeSkill> {
    tree.iter().filter(|s| s.page == page).collect()
}

/// The node the cursor moves to from `from` in direction (dx, dy) (grid steps): the nearest one
/// ahead, the cross-axis distance counting double. None at the tree's edge.
pub(crate) fn step(nodes: &[&TreeSkill], from: usize, dx: i32, dy: i32) -> Option<usize> {
    let a = nodes.get(from)?;
    nodes
        .iter()
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

/// A tree node's button (its index on the page).
#[derive(Component)]
pub(crate) struct SkillNodeButton(pub(crate) usize);

// Layout, in % of the window (from the user's 1280 x 720 captures).
const COL_X: f32 = 12.1;
const COL_STEP: f32 = 14.5;
const ROW_Y: f32 = 28.75;
const ROW_STEP: f32 = 9.2;
const NODE_W: f32 = 4.85;
const NODE_H: f32 = 8.6;
const LINE: Color = Color::srgba(0.05, 0.05, 0.05, 0.9);
/// The skill-point teal (the counter and the gauge).
const TEAL: Color = Color::srgb(0.45, 0.8, 0.85);

fn center(s: &TreeSkill) -> (f32, f32) {
    (COL_X + s.col as f32 * COL_STEP, ROW_Y + s.row as f32 * ROW_STEP)
}

/// The kind of skill by its text's first sentence, for the art's frame: Combat Art (red), Martial
/// Art (blue), Latent Skill and the rest (gold), as on the nodes' own frames.
fn frame_color(info: &str) -> Color {
    let first = info.split('.').next().unwrap_or("").to_lowercase();
    if first.contains("martial art") || first.contains("ninjutsu") {
        Color::srgb(0.2, 0.3, 0.48)
    } else if first.contains("combat art") {
        Color::srgb(0.5, 0.2, 0.13)
    } else {
        Color::srgb(0.58, 0.4, 0.13)
    }
}

/// The screen's state the idol menu keeps.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct View {
    pub(crate) page: usize,
    pub(crate) cursor: usize,
    /// The "Acquire?" dialog is up.
    pub(crate) asking: bool,
}

/// The Acquire Skills screen under `root` (already holding the menu backdrop and title).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(root: &mut ChildSpawnerCommands, assets: &AssetServer, font: &Option<Handle<Font>>, view: View, config: &crate::config::GameConfig, texts: &ItemTexts, tables: &Tables) {
    let all = tree();
    let (learned, points, items) = (&config.player.skills[..], config.player.skill_points, &config.player.items[..]);
    let pages = available_pages(items);
    if pages.is_empty() {
        root.spawn(abs(9.7, 28.0, 44.0, 16.0)).with_children(|c| {
            c.spawn(text("Obtain an Esoteric Text to unlock its skill tree.", 18.0, TEXT, font));
        });
        return;
    }
    let tab = pages.iter().position(|&page| page == view.page).unwrap_or(0);
    let view = View { page: pages[tab], ..view };
    let tabs: Vec<_> = pages.iter().map(|&page| PAGES[page]).collect();
    // The tree name and the tabs with C / V (the list screens' category row).
    crate::game_menu::list_screen(root, assets, font, &tabs, tab, &[], 0, 0, None);
    // The skill points: the ink drop with the count, and the gauge to the next point.
    root.spawn((abs(40.4, 17.6, 3.4, 4.6), image(assets, sprite("sb_fe/MENU_SkillPoint")).with_color(Color::srgb(0.1, 0.1, 0.1)))).with_children(|b| {
        b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
            c.spawn(text(&points.to_string(), 16.0, TEAL, font));
        });
    });
    root.spawn((abs(43.6, 19.1, 15.0, 1.7), image(assets, sprite("sb_in_game/MENU_GaugeFrame"))));
    root.spawn((abs(6.6, 23.2, 52.4, 0.2), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4))));
    let nodes = page(all, view.page);
    // The lines first, under the nodes: parent's right edge -> half way -> the child's left edge,
    // a dot where each joins the child.
    for child in &nodes {
        let on_page: Vec<&TreeSkill> = child.parents.iter().filter_map(|&p| nodes.iter().find(|n| n.id == p).copied()).collect();
        let n = on_page.len();
        for (k, parent) in on_page.iter().enumerate() {
            let (px, py) = center(parent);
            let (cx, cy) = center(child);
            let cy = cy + if n > 1 { (k as f32 - (n - 1) as f32 / 2.0) * 1.1 } else { 0.0 };
            let (x0, x1) = (px + NODE_W / 2.0, cx - NODE_W / 2.0);
            let mid = (px + cx) / 2.0;
            let (t, w) = (0.32, 0.18);
            root.spawn((abs(x0, py - t / 2.0, mid - x0 + w, t), BackgroundColor(LINE)));
            root.spawn((abs(mid, py.min(cy) - t / 2.0, w, (py - cy).abs() + t), BackgroundColor(LINE)));
            root.spawn((abs(mid, cy - t / 2.0, x1 - mid, t), BackgroundColor(LINE)));
            root.spawn((Node { border_radius: BorderRadius::MAX, ..abs(x1 - 0.55, cy - 0.5, 0.55, 1.0) }, BackgroundColor(LINE)));
        }
    }
    for (i, s) in nodes.iter().enumerate() {
        let (x, y) = center(s);
        let shown = is_revealed(all, s, learned);
        let has = is_learned(all, s.id, learned);
        let open = check(all, s, learned, u32::MAX, items).is_ok();
        let scale = if s.secret { 1.15 } else { 1.0 };
        let (w, h) = (NODE_W * scale, NODE_H * scale);
        if i == view.cursor {
            let (gw, gh) = (w * 1.3, h * 1.3);
            root.spawn((Node { border: UiRect::all(Val::Px(3.0)), border_radius: BorderRadius::MAX, ..abs(x - gw / 2.0, y - gh / 2.0, gw, gh) }, BorderColor::all(ORANGE), BackgroundColor(ORANGE.with_alpha(0.3))));
        }
        let icon = if shown { tables.weapon_icon.get(&s.weapon).and_then(|i| any_icon(*i)) } else { sprite("sb_remodeling/MENU_ItemBase_Skill") };
        // Not yet in reach: the node greyed (the game dims them the same way).
        let tint = if !shown || has || open { Color::WHITE } else { Color::srgb(0.55, 0.55, 0.55) };
        root.spawn((Button, abs(x - w / 2.0, y - h / 2.0, w, h), image(assets, icon).with_color(tint), SkillNodeButton(i))).with_children(|b| {
            if s.secret && shown {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(-9.0), top: Val::Percent(-9.0), width: Val::Percent(118.0), height: Val::Percent(118.0), ..default() }, image(assets, sprite("sb_remodeling/MENU_Skill_Ougi"))));
            }
            if has {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(66.0), top: Val::Percent(-6.0), width: Val::Percent(38.0), height: Val::Percent(38.0), ..default() }, image(assets, sprite("sb_remodeling/MENU_BanSkill"))));
            }
        });
    }
    // The skill under the cursor on the parchment.
    if let Some(s) = nodes.get(view.cursor) {
        let shown = is_revealed(all, s, learned);
        let (name, info) = if shown {
            (texts.weapon_name.get(&(s.weapon as u32)).cloned().unwrap_or_default(), texts.weapon_info.get(&(s.weapon as u32)).cloned().unwrap_or_default())
        } else {
            ("???".to_string(), String::new())
        };
        let status = match check(all, s, learned, points, items) {
            _ if !shown => String::new(),
            Err(Block::Learned) => "Learned".to_string(),
            Err(Block::Mirror) => format!("Acquired on its own tree ({} Skill Points)", find(all, s.mirror.unwrap_or(s.id)).map_or(0, |m| m.cost)),
            _ => format!("Skill Points Required: {}", s.cost),
        };
        root.spawn((abs(60.6, 14.6, 33.4, 70.4), image(assets, sprite("sb_in_game/MENU_WP_Base"))));
        root.spawn((abs(61.4, 15.9, 31.8, 4.4), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.25, 0.25, 0.25)))).with_children(|b| {
            b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                c.spawn(text(&name, 17.0, Color::WHITE, font));
            });
        });
        if !status.is_empty() {
            root.spawn((abs(62.4, 20.9, 30.5, 3.1), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgba(0.35, 0.35, 0.35, 0.9)))).with_children(|b| {
                b.spawn(Node { padding: UiRect::left(Val::Percent(2.5)), ..fill_row() }).with_children(|c| {
                    c.spawn(text(&status, 14.5, TEXT, font));
                });
            });
        }
        let art = tables.weapon_icon.get(&s.weapon).and_then(|i| sprite(&format!("sb_icon_skill/MENU_SkillStory_{i:05}")).or_else(|| sprite(&format!("sb_icon_skill_02/MENU_SkillStory_{i:05}"))));
        if let (true, Some(art)) = (shown, art) {
            root.spawn((abs(62.7, 24.6, 29.2, 18.0), BackgroundColor(frame_color(&info)))).with_children(|f| {
                f.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(1.6), top: Val::Percent(3.0), width: Val::Percent(96.8), height: Val::Percent(94.0), ..default() }, ImageNode::new(assets.load(art))));
            });
        }
        let mut r = 46.8;
        while r < 82.5 {
            root.spawn((abs(63.0, r, 29.0, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.18))));
            r += 3.05;
        }
        root.spawn(abs(63.4, 43.9, 28.4, 40.0)).with_children(|c| {
            c.spawn(text(&info, 15.5, TEXT, font));
        });
        if view.asking {
            root.spawn((abs(0.0, 0.0, 100.0, 100.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5))));
            root.spawn((abs(30.0, 38.0, 40.0, 22.0), image(assets, sprite("sb_dialog/MENU_Dialog_base")))).with_children(|d| {
                d.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: Val::Px(14.0), ..fill_row() }).with_children(|c| {
                    c.spawn(text(&format!("Acquire \"{name}\"?"), 18.0, Color::WHITE, font));
                    c.spawn(text(&format!("Skill Points  {points}  >  {}", points.saturating_sub(s.cost)), 16.0, TEAL, font));
                    c.spawn(text("Enter: Acquire      Esc: Cancel", 15.0, DIM, font));
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_TEXTS: [(i64, u32); 5] = [(2920, 1), (2921, 1), (2922, 1), (2923, 1), (2924, 1)];

    fn node(id: i64) -> &'static TreeSkill {
        tree().iter().find(|s| s.id == id).unwrap_or_else(|| panic!("SkillParam {id} not in the trees"))
    }

    #[test]
    fn the_trees_match_the_captures() {
        let t = tree();
        let sizes: Vec<usize> = (0..5).map(|p| page(t, p).len()).collect();
        // Shinobi 13, Prosthetic 12, Ashina 7, Temple 7, Mushin 7 (the user's captures).
        assert_eq!(sizes, vec![13, 12, 7, 7, 7]);
        // Mushin's bottom row repeats the trees' last skills, all four in the ornate frame.
        let mirrors: Vec<Option<i64>> = [400, 401, 402, 403].iter().map(|&i| node(i).mirror).collect();
        assert_eq!(mirrors, vec![Some(321), Some(20), Some(220), Some(103)]);
        assert!([20, 103, 220, 321, 411, 421, 431].iter().all(|&i| node(i).secret));
        assert!(!node(10).secret);
        // Costs: EquipMtrlSetParam itemNum01 of the skill point.
        assert_eq!((node(2).cost, node(60).cost, node(411).cost, node(421).cost), (1, 2, 6, 9));
    }

    #[test]
    fn a_skill_is_revealed_and_bought_after_its_parent() {
        let t = tree();
        let (root, child) = (node(10), node(2));
        assert!(is_revealed(t, root, &[]));
        assert!(!is_revealed(t, child, &[]), "skill 2 shows only once one of its revealers is learned");
        assert_eq!(check(t, child, &[], 99, &ALL_TEXTS), Err(Block::Locked));
        assert_eq!(check(t, root, &[], 0, &ALL_TEXTS), if root.cost == 0 { Ok(()) } else { Err(Block::Points) });
        assert_eq!(check(t, root, &[], 99, &ALL_TEXTS), Ok(()));
        assert!(is_revealed(t, child, &[10]));
        assert_eq!(check(t, child, &[10], 99, &ALL_TEXTS), Ok(()));
        assert_eq!(check(t, child, &[10, 2], 99, &ALL_TEXTS), Err(Block::Learned));
    }

    #[test]
    fn mushin_asks_for_both_last_skills_and_mirrors_them() {
        let t = tree();
        let shura = node(411);
        assert_eq!(check(t, node(400), &[], 99, &ALL_TEXTS), Err(Block::Mirror));
        assert!(is_learned(t, 400, &[321]), "the copy is learned with its skill");
        assert_eq!(check(t, node(400), &[321], 99, &ALL_TEXTS), Err(Block::Learned));
        assert_eq!(check(t, shura, &[321], 99, &ALL_TEXTS), Err(Block::Locked));
        assert_eq!(check(t, shura, &[321, 20], 99, &ALL_TEXTS), Ok(()));
    }

    #[test]
    fn each_text_opens_only_its_school() {
        assert!(available_pages(&[]).is_empty());
        // Empty stacks and unrelated key items do not unlock anything.
        assert!(available_pages(&[(2920, 0), (2420, 1)]).is_empty());
        for (page, &goods) in TEXT_GOODS.iter().enumerate() {
            assert_eq!(available_pages(&[(goods, 1)]), vec![page]);
        }
        // Tabs keep their school ids, rather than mistaking the second visible tab for page 1.
        assert_eq!(available_pages(&[(2924, 1), (2921, 1)]), vec![1, 4]);
        assert_eq!(available_pages(&ALL_TEXTS), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn even_with_points_and_parents_a_skill_requires_its_text() {
        let t = tree();
        for school in 0..PAGES.len() {
            let s = page(t, school).into_iter().find(|s| s.mirror.is_none()).unwrap();
            let learned: Vec<_> = s.parents.iter().chain(&s.revealed_by).map(|&id| find(t, id).and_then(|s| s.mirror).unwrap_or(id)).collect();
            assert_eq!(check(t, s, &learned, 99, &[]), Err(Block::Text));
            assert_eq!(check(t, s, &learned, 99, &[(TEXT_GOODS[(school + 1) % 5], 1)]), Err(Block::Text));
            assert_eq!(check(t, s, &learned, 99, &[(TEXT_GOODS[school], 1)]), Ok(()));
        }
    }

    #[test]
    fn the_construct_books_are_permanent_key_items_and_do_not_teach_skills() {
        let mut config = crate::config::GameConfig::load().unwrap();
        config.player.items.retain(|(id, _)| !TEXT_GOODS.contains(id));
        let (skills, points) = (config.player.skills.clone(), config.player.skill_points);
        let pickups = crate::items::parkour_pickups();
        let (solids, _) = crate::sandbox::parkour_course();
        for p in pickups.iter().filter(|p| TEXT_GOODS.contains(&p.goods)) {
            let ground = solids.iter().filter(|(lo, hi)| p.pos.x > lo.x && p.pos.x < hi.x && p.pos.z > lo.z && p.pos.z < hi.z)
                .map(|(_, hi)| hi.y).max_by(f32::total_cmp).unwrap_or(0.0);
            assert!((p.pos.y - ground).abs() < 0.01, "text {} at {:?} is not on the course (ground {ground})", p.goods, p.pos);
        }
        for goods in TEXT_GOODS {
            assert_eq!(pickups.iter().filter(|p| p.goods == goods && p.count == 1).count(), 1);
            assert_eq!(crate::items::param("EquipParamGoods", goods)["goodsType"].as_i64(), Some(1));
            assert_eq!(crate::items::add(&mut config, goods, 1), 1);
            assert_eq!(crate::items::add(&mut config, goods, 1), 0, "unique key item");
            crate::items::spend(&mut config, goods);
            assert_eq!(crate::items::count(&config, goods), 1, "reading does not consume the text");
        }
        assert_eq!(available_pages(&config.player.items), vec![0, 1, 2, 3, 4]);
        assert_eq!((config.player.skills, config.player.skill_points), (skills, points));
    }

    #[test]
    fn the_cursor_steps_to_the_nearest_node_ahead() {
        let nodes = page(tree(), 0);
        let at = |id: i64| nodes.iter().position(|s| s.id == id).unwrap();
        // From skill 10 (row 2, column 0) right is the node on its row.
        assert_eq!(step(&nodes, at(10), 1, 0), Some(at(2)));
        assert_eq!(step(&nodes, at(10), 0, 1), Some(at(4)));
        assert_eq!(step(&nodes, at(10), -1, 0), None);
    }
}
