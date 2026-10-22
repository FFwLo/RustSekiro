//! Enemy drops (the user, 2026-10-18: "fill in the gaps"): a killed enemy's NpcParam lots, each
//! rolled once when it dies.
//! - itemLotId_1..6 (ItemLotParam): goods as pickups at the body (an Ashina soldier's 15 % Ceramic
//!   Shard); Wolf's itemDropRate (the Mibu Possession Balloon 3621: 0.5) weights up the entries
//!   with enableLuck.
//! - resourceItemLotParamId (ResourceItemLotParam, category 1): Spirit Emblems as a pickup (a
//!   soldier's 50 % one, the General's 15); resourceItemDropRate (the Mibu Balloon of Spirit
//!   3631: 0.5) weights up the entries with spLotPointRate.
//! - resourceItemLotParamId2 (category 5): Resurrective Power, taken at once while a node is
//!   used; `RES_POWER_NODE` of it lights one again. The Mibu Balloon of Soul (3611, stateInfo 345,
//!   "acquire more Resurrective Power") weights its entries up as its siblings do (x1.5).
//! - ninsatsuResourceItemLotId_1 / _2: drawn once for each deathblow whose Ninsatsu hit lands
//!   (Enemy::ninsatsu_landed), dead or not. _2 is 10005800 (10 Resurrective Power) on 277 rows,
//!   190 of them the marked enemies; _1 (10000800) holds nothing.
//! gap: the exe's lot draw, its luck formula, the Balloon of Soul's rate, a node's cost in
//! Resurrective Power and the moment the deathblow lots are drawn are not traced (each lot one
//! draw, base x (1 + rate), 100 a node, at the Ninsatsu hit); the drops' fall from the body.

use std::collections::HashSet;

use bevy::prelude::*;

/// Resurrective Power that lights one used resurrection node.
pub(crate) const RES_POWER_NODE: u32 = 100;
/// ResourceItemLotParam resourceItemCategory: Spirit Emblems, Resurrective Power.
const RESOURCE_EMBLEMS: i64 = 1;
const RESOURCE_POWER: i64 = 5;
/// ItemLotParam lotItemCategory of a goods.
const LOT_GOODS: i64 = 0x4000_0000;
/// The Mibu Balloon of Soul's stateInfo and the rate taken for it.
const SOUL_BALLOON_STATE: i64 = 345;
const SOUL_RATE: f32 = 0.5;

/// The index `r` (0..1) falls on in `weights`; None when they are all 0.
pub(crate) fn pick(weights: &[f32], r: f32) -> Option<usize> {
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut x = r.clamp(0.0, 0.999_999) * total;
    for (i, w) in weights.iter().enumerate() {
        if x < *w {
            return Some(i);
        }
        x -= w;
    }
    weights.iter().rposition(|w| *w > 0.0)
}

/// An ItemLotParam row's draw for `r`: (goods, count), None for its empty entries. `drop_rate`
/// (itemDropRate) weights up the goods entries with enableLuck.
pub(crate) fn roll_item_lot(id: i64, drop_rate: f32, r: f32) -> Option<(i64, u32)> {
    let row = crate::items::param("ItemLotParam", id);
    if row.is_null() {
        return None;
    }
    let field = |k: &str, i: usize| row[format!("{k}0{i}").as_str()].as_i64().unwrap_or(0);
    let goods = |i: usize| field("lotItemCategory", i) == LOT_GOODS && field("lotItemId", i) > 0;
    let w: Vec<f32> = (1..=8)
        .map(|i| {
            let base = field("lotItemBasePoint", i).max(0) as f32;
            if goods(i) && field("enableLuck", i) == 1 { base * (1.0 + drop_rate) } else { base }
        })
        .collect();
    let i = pick(&w, r)? + 1;
    goods(i).then(|| (field("lotItemId", i), field("lotItemNum", i).max(1) as u32))
}

/// A ResourceItemLotParam row's draw for `r`: (category, count), None for its empty entries.
/// `rate` weights up its boosted entries (spLotPointRate set, or Resurrective Power's).
pub(crate) fn roll_resource_lot(id: i64, rate: f32, r: f32) -> Option<(i64, u32)> {
    let row = crate::items::param("ResourceItemLotParam", id);
    if row.is_null() {
        return None;
    }
    let field = |k: &str, i: usize| row[format!("{k}0{i}").as_str()].as_i64().unwrap_or(0);
    let w: Vec<f32> = (1..=8)
        .map(|i| {
            let base = field("lotBasePoint", i).max(0) as f32;
            let boosted = field("itemNum", i) > 0 && (field("spLotPointRate", i) == 1 || field("resourceItemCategory", i) == RESOURCE_POWER);
            if boosted { base * (1.0 + rate) } else { base }
        })
        .collect();
    let i = pick(&w, r)? + 1;
    let n = field("itemNum", i);
    (n > 0).then(|| (field("resourceItemCategory", i), n as u32))
}

/// The sum of `key` over Wolf's worn rows (itemDropRate, resourceItemDropRate).
fn worn_rate(worn: &[i64], key: &str) -> f32 {
    worn.iter().map(|id| crate::items::param("SpEffectParam", *id)[key].as_f64().unwrap_or(0.0) as f32).sum()
}

/// Adds Resurrective Power (`power`) while a node is used (`nodes` lit of `max_nodes`), lighting
/// nodes as it fills; the nodes lit.
pub(crate) fn gain_power(nodes: &mut u32, power: &mut u32, max_nodes: u32, n: u32) -> u32 {
    if *nodes >= max_nodes {
        return 0;
    }
    *power += n;
    let mut lit = 0;
    while *power >= RES_POWER_NODE && *nodes < max_nodes {
        *power -= RES_POWER_NODE;
        *nodes += 1;
        lit += 1;
    }
    if *nodes >= max_nodes {
        *power = 0;
    }
    lit
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn drop_loot(
    mut commands: Commands,
    combat: Res<crate::data::Combat>,
    config: Res<crate::config::GameConfig>,
    enemies: Query<(Entity, &crate::enemy::Enemy, &crate::actor::Actor, &Transform)>,
    mut wolf: Query<&mut crate::player::Player>,
    mut log: Option<ResMut<crate::hud::CombatLog>>,
    mut done: Local<HashSet<Entity>>,
    mut rng: Local<u32>,
    mut landed: Local<std::collections::HashMap<Entity, u32>>,
) {
    done.retain(|e| enemies.get(*e).is_ok_and(|(_, en, ..)| en.is_dead()));
    landed.retain(|e, _| enemies.get(*e).is_ok());
    let Ok(mut p) = wolf.single_mut() else { return };
    let worn: Vec<i64> = p.timed.iter().map(|t| t.0).collect();
    let item_rate = worn_rate(&worn, "itemDropRate");
    let emblem_rate = worn_rate(&worn, "resourceItemDropRate");
    let soul = worn.iter().any(|id| crate::items::param("SpEffectParam", *id)["stateInfo"].as_i64() == Some(SOUL_BALLOON_STATE));
    let mut say = |s: String| {
        if let Some(l) = log.as_mut() {
            l.push(s, Color::srgb(0.9, 0.8, 0.6));
        }
    };
    for (e, en, a, tf) in &enemies {
        let seen = landed.insert(e, en.ninsatsu_landed).unwrap_or(0);
        let deathblows = en.ninsatsu_landed.saturating_sub(seen);
        let died = en.is_dead() && done.insert(e);
        if !died && deathblows == 0 {
            continue;
        }
        let mut r = || {
            if *rng == 0 {
                *rng = 0x9E37_79B9;
            }
            *rng ^= *rng << 13;
            *rng ^= *rng >> 17;
            *rng ^= *rng << 5;
            (*rng % 1_000_000) as f32 / 1_000_000.0
        };
        // The raw row: the exported NpcParam keeps only the combat fields, not the lots.
        let npc = crate::items::param("NpcParam", combat.foe_of(a).npc_row);
        let lot = |k: &str| npc[k].as_i64().filter(|v| *v > 0);
        let feet = tf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
        let side = a.forward().cross(Vec3::Y).normalize_or(Vec3::X);
        let mut spot = 0;
        let mut place = |commands: &mut Commands, goods: i64, n: u32| {
            spot += 1;
            crate::pickups::drop_pickup(commands, goods, n, feet + side * (0.35 * spot as f32 - 0.5));
        };
        // A resource lot's draw: Resurrective Power taken at once; Spirit Emblems returned to drop.
        let mut grant = |p: &mut crate::player::Player, (cat, n): (i64, u32)| {
            if cat == RESOURCE_EMBLEMS {
                return Some(n);
            }
            if cat == RESOURCE_POWER && p.resurrections < config.player.resurrections {
                let lit = gain_power(&mut p.resurrections, &mut p.res_power, config.player.resurrections, n);
                say(format!("Resurrective Power +{n} ({} / {RES_POWER_NODE})", p.res_power));
                if lit > 0 {
                    say("a resurrection node is lit".to_string());
                }
            }
            None
        };
        let mut emblems = 0;
        let power_rate = if soul { SOUL_RATE } else { 0.0 };
        for _ in 0..deathblows {
            for k in ["ninsatsuResourceItemLotId_1", "ninsatsuResourceItemLotId_2"] {
                if let Some(draw) = lot(k).and_then(|id| roll_resource_lot(id, power_rate, r())) {
                    emblems += grant(&mut p, draw).unwrap_or(0);
                }
            }
        }
        if died {
            if let Some(draw) = lot("resourceItemLotParamId").and_then(|id| roll_resource_lot(id, emblem_rate, r())).filter(|d| d.0 == RESOURCE_EMBLEMS) {
                emblems += grant(&mut p, draw).unwrap_or(0);
            }
            if let Some(draw) = lot("resourceItemLotParamId2").and_then(|id| roll_resource_lot(id, power_rate, r())).filter(|d| d.0 == RESOURCE_POWER) {
                grant(&mut p, draw);
            }
        }
        if emblems > 0 {
            place(&mut commands, crate::items::SPIRIT_EMBLEM, emblems);
        }
        if !died {
            continue;
        }
        for k in ["itemLotId_1", "itemLotId_2", "itemLotId_3", "itemLotId_4", "itemLotId_5", "itemLotId_6"] {
            if let Some((goods, n)) = lot(k).and_then(|id| roll_item_lot(id, item_rate, r())) {
                place(&mut commands, goods, n);
            }
        }
    }
}

pub struct DropsPlugin;

impl Plugin for DropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, drop_loot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The soldier's lots (NpcParam 10100000): a 15 % Ceramic Shard (ItemLotParam 10100000, its
    /// luck entry), a 50 % Spirit Emblem (10000000), a 30 % 5 Resurrective Power (10005000); the
    /// Possession Balloon's 0.5 lifts the shard to 150 / 1000 x 1.5 of the weight.
    #[test]
    fn the_soldier_lots_draw_as_their_weights() {
        let n = 10_000;
        let draws = |f: &dyn Fn(f32) -> bool| (0..n).filter(|i| f((*i as f32 + 0.5) / n as f32)).count() as f32 / n as f32;
        let shard = draws(&|r| roll_item_lot(10100000, 0.0, r) == Some((3500, 1)));
        assert!((shard - 0.15).abs() < 0.01, "{shard}");
        let lucky = draws(&|r| roll_item_lot(10100000, 0.5, r) == Some((3500, 1)));
        assert!((lucky - 225.0 / 1075.0).abs() < 0.01, "{lucky}");
        let emblem = draws(&|r| roll_resource_lot(10000000, 0.0, r) == Some((RESOURCE_EMBLEMS, 1)));
        assert!((emblem - 0.5).abs() < 0.01, "{emblem}");
        let spirit = draws(&|r| roll_resource_lot(10000000, 0.5, r) == Some((RESOURCE_EMBLEMS, 1)));
        assert!((spirit - 0.6).abs() < 0.01, "{spirit}");
        let power = draws(&|r| roll_resource_lot(10005000, 0.0, r) == Some((RESOURCE_POWER, 5)));
        assert!((power - 0.3).abs() < 0.01, "{power}");
    }

    #[test]
    fn power_lights_used_nodes_only() {
        let (mut nodes, mut power) = (0, 0);
        assert_eq!(gain_power(&mut nodes, &mut power, 2, 67), 0);
        assert_eq!(gain_power(&mut nodes, &mut power, 2, 67), 1);
        assert_eq!((nodes, power), (1, 34));
        assert_eq!(gain_power(&mut nodes, &mut power, 2, 200), 1);
        assert_eq!((nodes, power), (2, 0));
        assert_eq!(gain_power(&mut nodes, &mut power, 2, 50), 0);
        assert_eq!(power, 0);
    }
}
