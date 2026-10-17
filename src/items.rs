//! Quick items: Wolf's goods (config player.items: EquipParamGoods id and count) and the five
//! quick-item slots (config player.quick_items), used with R and cycled with C (the pad's d-pad
//! down). An item plays its use state (EquipParamGoods goodsUseAnim -> the behaviour's Item*
//! state, `use_state`), is spent at the clip's ConsumeCurrentGoods (TAE 65, as the gourd) and
//! takes effect: refId_default names a Bullet fired at Wolf himself, whose HitBulletID carries
//! the effect as spEffectIDForShooter (Ako's Sugar 3400 -> 3401: attack x1.125, posture damage
//! x1.25, 30 s). The effect rides on Player::timed with its effectEndurance.
//!
//! Effects modelled (`wolf_rates`, `tick_regen`): the sugars' attack / posture-damage rates
//! (Ako's, Yashariku's), damage cut (Ungo's), posture damage taken (Gokan's); HP regeneration
//! (changeHpEstusFlaskRate: Pellet, Sweet Rice Ball) and posture recovery (changeStaminaRate:
//! Persimmon), both read as % of the maximum per second; the Homeward Idol's warp to the last
//! idol. gap: the rest (stealth, spirit damage, cures, the throwables' bullets, coin purses) play
//! their animation and are spent with no effect yet; the regen rates' time base is not traced.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;

/// A param table's rows by id, read once from extracted/json/params (the combat data carries
/// only the tables the fights use).
pub(crate) fn param(table: &'static str, id: i64) -> &'static serde_json::Value {
    static TABLES: OnceLock<std::sync::Mutex<HashMap<&'static str, &'static HashMap<i64, serde_json::Value>>>> = OnceLock::new();
    static NULL: serde_json::Value = serde_json::Value::Null;
    let mut tables = TABLES.get_or_init(Default::default).lock().unwrap();
    let rows = *tables.entry(table).or_insert_with(|| {
        let path = crate::paths::extracted().join(format!("json/params/{table}.json"));
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let rows: HashMap<i64, serde_json::Value> = v["rows"].as_array().into_iter().flatten().filter_map(|r| Some((r["id"].as_i64()?, r.clone()))).collect();
        Box::leak(Box::new(rows))
    });
    rows.get(&id).unwrap_or(&NULL)
}

/// The Healing Gourd (its own refill and drinks in player.rs).
pub(crate) const GOURD: i64 = 3000;
/// The Homeward Idol (not consumed: isConsume 0).
pub(crate) const HOMEWARD_IDOL: i64 = 3980;
/// Quick-item slots.
pub(crate) const SLOTS: usize = 5;

/// The use state of a goodsUseAnim: the behaviour's Item* states, whose clips run in goodsUseAnim
/// order (40-44 the sugars -> a000_250800, 250900, 251000, 251100, 251200).
pub(crate) fn use_state(goods_use_anim: i64) -> Option<&'static str> {
    Some(match goods_use_anim {
        4 => "ItemColoredRice",
        20 | 22 => "ItemPillTonic",
        30 => "ItemPowderMedicineRecoverPoison",
        34 => "ItemAntiHallucinogen",
        40 => "ItemCandyAttackBuff",
        41 => "ItemCandyAttackBuffSacrifice",
        42 => "ItemCandyC",
        43 => "ItemCandyBodyCoreBuff",
        44 => "ItemCandyE",
        45 => "ItemAntiGhostBuff",
        50 => "ItemBalloonMoneyDropBuff",
        60 => "ItemCoinBag",
        70 => "ItemPottery",
        71 => "ItemStone",
        80 => "ItemSuicide",
        87 => "ItemReturnStatue",
        88 => "ItemOhagi",
        89 => "ItemKaki",
        _ => return None,
    })
}

/// How many of `id` Wolf holds.
pub(crate) fn count(config: &crate::config::GameConfig, id: i64) -> u32 {
    if id == GOURD {
        return config.player.gourd_charges;
    }
    config.player.items.iter().find(|(g, _)| *g == id).map_or(0, |(_, n)| *n)
}

/// Adds `n` of `id` (up to the goods' maxNum); returns how many were taken.
pub(crate) fn add(config: &mut crate::config::GameConfig, id: i64, n: u32) -> u32 {
    let max = param("EquipParamGoods", id)["maxNum"].as_u64().unwrap_or(99) as u32;
    let items = &mut config.player.items;
    let i = items.iter().position(|(g, _)| *g == id).unwrap_or_else(|| {
        items.push((id, 0));
        items.len() - 1
    });
    let took = n.min(max.saturating_sub(items[i].1));
    items[i].1 += took;
    took
}

/// Spends one of `id`, if held and the goods is consumed on use.
pub(crate) fn spend(config: &mut crate::config::GameConfig, id: i64) {
    if param("EquipParamGoods", id)["isConsume"].as_i64() == Some(0) {
        return;
    }
    if let Some(e) = config.player.items.iter_mut().find(|(g, _)| *g == id) {
        e.1 = e.1.saturating_sub(1);
    }
}

/// The quick item in slot `slot` (0 = empty).
pub(crate) fn in_slot(config: &crate::config::GameConfig, slot: usize) -> i64 {
    config.player.quick_items.get(slot).copied().unwrap_or(0)
}

/// The next filled slot after `from` (wrapping; `from` itself when it is the only one).
pub(crate) fn next_slot(config: &crate::config::GameConfig, from: usize) -> usize {
    (1..=SLOTS).map(|k| (from + k) % SLOTS).find(|&s| in_slot(config, s) > 0).unwrap_or(from)
}

/// The SpEffects an item puts on Wolf: its refId_default Bullet's (and that bullet's HitBulletID
/// chain's) spEffectIDForShooter.
pub(crate) fn effects_of(goods: i64) -> Vec<i64> {
    let g = param("EquipParamGoods", goods);
    if g["refCategory"].as_i64() != Some(1) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut bullet = g["refId_default"].as_i64().unwrap_or(-1);
    for _ in 0..4 {
        if bullet <= 0 {
            break;
        }
        let b = param("Bullet", bullet);
        if let Some(sp) = b["spEffectIDForShooter"].as_i64().filter(|&s| s > 0 && !out.contains(&s)) {
            out.push(sp);
        }
        // A hit bullet may name itself (3401 -> 3401): the chain ends there.
        let next = b["HitBulletID"].as_i64().unwrap_or(-1);
        bullet = if next == bullet { -1 } else { next };
    }
    out
}

/// Puts the effect on Wolf for its effectEndurance (a fresh use restarts it).
pub(crate) fn wear(timed: &mut Vec<(i64, f32)>, sp: i64) {
    let endurance = param("SpEffectParam", sp)["effectEndurance"].as_f64().unwrap_or(0.0) as f32;
    if endurance <= 0.0 {
        return;
    }
    timed.retain(|(id, _)| *id != sp);
    timed.push((sp, endurance));
}

/// Wolf's rates from his timed effects: (HP damage dealt, posture damage dealt, HP damage taken,
/// posture damage taken). Slash stands for the physical attributes (the sugars set them all
/// alike).
pub(crate) fn wolf_rates(timed: &[(i64, f32)]) -> [f32; 4] {
    let mut r = [1.0f32; 4];
    for (id, _) in timed {
        let s = param("SpEffectParam", *id);
        let f = |k: &str| s[k].as_f64().unwrap_or(1.0) as f32;
        r[0] *= f("slashAttackRate");
        r[1] *= f("staminaAttackRate");
        r[2] *= f("slashDamageCutRate");
        r[3] *= f("defSlashStaminaDmgRate");
    }
    r
}

/// HP and posture recovery from the timed effects over `dt` s: changeHpEstusFlaskRate (negative
/// heals, % of max HP) and changeStaminaRate (% of max posture), per second.
pub(crate) fn tick_regen(timed: &[(i64, f32)], a: &mut crate::actor::Actor, dt: f32) {
    if a.hp <= 0.0 {
        return;
    }
    for (id, _) in timed {
        let s = param("SpEffectParam", *id);
        let hp = s["changeHpEstusFlaskRate"].as_f64().unwrap_or(0.0) as f32;
        if hp < 0.0 {
            a.hp = (a.hp - a.hp_max * hp / 100.0 * dt).min(a.hp_max);
        }
        let posture = s["changeStaminaRate"].as_f64().unwrap_or(0.0) as f32;
        if posture > 0.0 {
            a.posture = (a.posture - a.posture_max * posture / 100.0 * dt).max(0.0);
        }
    }
}

/// Items used this frame (player.rs pushes them at the consume event; `spend_used` takes them
/// off the bag, which needs the config mutable).
#[derive(Resource, Default)]
pub(crate) struct Used(pub(crate) Vec<i64>);

fn spend_used(mut used: ResMut<Used>, mut config: ResMut<crate::config::GameConfig>) {
    for id in std::mem::take(&mut used.0) {
        spend(&mut config, id);
    }
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Used>().add_systems(Update, spend_used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sugar_wears_its_effect_and_raises_the_attack() {
        // Ako's Sugar: Bullet 3400 (SpEffect 3400, the trigger with its visual, no endurance) ->
        // its hit bullet 3401 (SpEffect 3401, the effect).
        assert_eq!(effects_of(3400), vec![3400, 3401]);
        let mut timed = Vec::new();
        wear(&mut timed, 3401);
        assert_eq!(timed, vec![(3401, 30.0)]);
        let r = wolf_rates(&timed);
        assert!((r[0] - 1.125).abs() < 1e-4 && (r[1] - 1.25).abs() < 1e-4 && r[2] == 1.0, "{r:?}");
        // Ungo's Sugar cuts the damage taken, Gokan's the posture damage.
        let mut t2 = Vec::new();
        for g in [3420, 3430] {
            for sp in effects_of(g) {
                wear(&mut t2, sp);
            }
        }
        let r = wolf_rates(&t2);
        assert!((r[2] - 0.67).abs() < 1e-3 && (r[3] - 0.33).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn every_quick_item_state_is_exported() {
        let path = crate::paths::extracted().join("combat_data.json");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let states = &v["player"]["states"];
        for anim in [4, 20, 30, 34, 40, 41, 42, 43, 44, 45, 50, 60, 70, 71, 80, 87, 88, 89] {
            let s = use_state(anim).unwrap();
            assert!(states[s].is_string(), "{s} (goodsUseAnim {anim}) has no clip");
        }
    }
}
