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
//! idol.
//! The rest (the user, 2026-10-18: "Items"), at the consume event (`apply_used`):
//! - the instant rows of the chain: Divine Grass's full HP and posture (3040) and its cure of
//!   everything (stateInfo 13); the cures (Antidote Powder 3201 poison, Dousing Powder 3211 burn,
//!   Pacifying Agent 3221 terror: build-up -99999 with the type's stateInfo; spCategory 194 the
//!   poison's slip damage) clear the gauge and the affliction (status.rs `cure`);
//! - the worn rows ride on Actor::item_sp, where the combat code reads them as Wolf's own:
//!   their resist rates on the build-up (def<Type>ResistDamageRate: Antidote / Dousing / Pacifying
//!   30 s; Red Lump x2 burn), the element cuts (Dousing fire x0.6, Eel Liver lightning x0.5 and
//!   its shock freezeDamageRate 50, Red Lump fire x2), Red Lump's spCategory 1001 damage levels
//!   (a hit seldom flinches him), Divine Confetti's magicAttackPower 20 on his sword (`magic_bonus`)
//!   and the Mibu Balloon of Wealth's soulRate x1.5 Sen (experience.rs); their maxHpRate on
//!   hp_max (`max_hp_rate`, player.rs: Yashariku's Sugar halves it for its 30 s; its
//!   maxStaminaRate 0.5 is not applied);
//! - the coin purses (goodsUseAnim 60) give their sellValue in Sen (100 / 500 / 1000), the
//!   Ceremonial Tanto (80) takes consumeHpRate 50 % of max HP for useGetItemNum 5 Spirit Emblems;
//! - the throwables (71: Fistful of Ash, Oil) fire their refId_default Bullet chain from the hand
//!   (prosthetic.rs's bullets: the ash cloud 3512's 220018 blinds - the firecracker's flinch stands
//!   in, 30 s cool time 3510; Oil's 3520 makes the target burn twice as easily and take x1.25
//!   fire), the Ceramic Shard (70) its clip's BulletBehavior 910 (Bullet 3490 -> 3491 -> 3492),
//!   whose hit and landing sound (AtkParam HitAiSoundId 3500) the enemies hear;
//! - the Snap Seed (84) its Bullet 3530 around Wolf, which undoes Lady Butterfly's phantoms.
//! gap: the Balloons of Soul, Possession and Spirit (Resurrective Power, item drops: not in the
//! game here), Five-color Rice's marker, Divine Confetti's apparition hits, the regen rates' time
//! base.

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

/// The use state of a goodsUseAnim, as the game's player script picks it (c0000_transition
/// BEH_A_ITEM_USE, itemAnimeType = env 231): the sugars 40-44 -> a000_250800 .. 251200; the
/// Ceremonial Tanto (80, and the red doll 81) the stab ItemPaperDollExchangeWhite (a000_250600);
/// Bite Down (82) the feigned death ItemSuicide (a000_250400); the poison powder 34 shares the
/// cure's ItemPowderMedicineRecoverPoison; the Snap Seed (84, ITEM_ILLUSION_BREAK)
/// ItemAntiHallucinogen. gap: 83 (Nightjar Monocular: the telescope's Start / Loop / End).
pub(crate) fn use_state(goods_use_anim: i64) -> Option<&'static str> {
    Some(match goods_use_anim {
        4 => "ItemColoredRice",
        14 => "ItemColoredRiceEmpty",
        20 | 22 => "ItemPillTonic",
        30 | 34 => "ItemPowderMedicineRecoverPoison",
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
        80 | 81 => "ItemPaperDollExchangeWhite",
        82 => "ItemSuicide",
        84 => "ItemAntiHallucinogen",
        85 => "ItemRedoBell",
        86 => "ItemStoneStatue",
        87 => "ItemReturnStatue",
        88 => "ItemOhagi",
        89 => "ItemKaki",
        _ => return None,
    })
}

/// A refilled goods run out (isFullSuppleItem: Five-color Rice 3580): the use state of its empty
/// twin, the next row with isSuppleItem (3581, goodsUseAnim 14 -> ItemColoredRiceEmpty).
pub(crate) fn empty_state(goods: i64) -> Option<&'static str> {
    if param("EquipParamGoods", goods)["isFullSuppleItem"].as_i64() != Some(1) {
        return None;
    }
    let twin = param("EquipParamGoods", goods + 1);
    (twin["isSuppleItem"].as_i64() == Some(1)).then(|| use_state(twin["goodsUseAnim"].as_i64().unwrap_or(255))).flatten()
}

/// Resting at an idol (idol.rs `rest`, the debug Refill) tops up the goods the game refills there
/// (isFullSuppleItem, autoReplenishType 2: Five-color Rice to its maxNum 5).
pub(crate) fn refill_rested(mut config: ResMut<crate::config::GameConfig>, mut wolf: Query<&mut crate::player::Player>) {
    for mut p in &mut wolf {
        if !std::mem::take(&mut p.refill_items) {
            continue;
        }
        for (g, n) in config.player.items.iter_mut() {
            let row = param("EquipParamGoods", *g);
            if row["isFullSuppleItem"].as_i64() == Some(1) {
                *n = row["maxNum"].as_u64().unwrap_or(*n as u64) as u32;
            }
        }
    }
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
/// chain's) spEffectIDForShooter (refCategory 1); with refCategory 2 refId_default is the
/// SpEffect itself (the gourd's heal 3000, the coin purses' 3700, the tanto's 3800: their looks).
pub(crate) fn effects_of(goods: i64) -> Vec<i64> {
    let g = param("EquipParamGoods", goods);
    match g["refCategory"].as_i64() {
        Some(1) => {}
        Some(2) => return g["refId_default"].as_i64().filter(|s| *s > 0 && !param("SpEffectParam", *s).is_null()).into_iter().collect(),
        _ => return Vec::new(),
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

/// Puts the effect on Wolf for its effectEndurance (a fresh use restarts it). `extend`: the
/// learned skills' extendLifeRate (`extend_rate`), for rows with isExtendSpEffectLife.
pub(crate) fn wear(timed: &mut Vec<(i64, f32)>, sp: i64, extend: f32) {
    let row = param("SpEffectParam", sp);
    let mut endurance = row["effectEndurance"].as_f64().unwrap_or(0.0) as f32;
    if endurance <= 0.0 {
        return;
    }
    if row["isExtendSpEffectLife"].as_i64() == Some(1) {
        endurance *= extend;
    }
    timed.retain(|(id, _)| *id != sp);
    timed.push((sp, endurance));
}

/// The learned latent skills' extendLifeRate product (SkillParam 370 -> 150600: 1.5), which
/// lengthens the rows flagged isExtendSpEffectLife (the sugars and spiritfalls 3351-3441: 30 s
/// -> 45 s).
pub(crate) fn extend_rate(skill_sps: impl Iterator<Item = i64>) -> f32 {
    skill_sps.filter_map(|id| param("SpEffectParam", id)["extendLifeRate"].as_f64()).filter(|r| *r > 0.0).product::<f64>() as f32
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

/// Items used this frame (player.rs pushes them at the consume event; `apply_used` takes them
/// off the bag and applies what they do beyond the worn effects).
#[derive(Resource, Default)]
pub(crate) struct Used(pub(crate) Vec<i64>);

/// An SpEffectParam row as the combat code reads Wolf's own (data::SpEffect), for the item rows
/// his exported data does not carry.
pub(crate) fn sp_row(id: i64) -> Option<&'static crate::data::SpEffect> {
    static ROWS: OnceLock<std::sync::Mutex<HashMap<i64, Option<&'static crate::data::SpEffect>>>> = OnceLock::new();
    let mut rows = ROWS.get_or_init(Default::default).lock().ok()?;
    *rows.entry(id).or_insert_with(|| {
        let row = param("SpEffectParam", id);
        (!row.is_null()).then(|| serde_json::from_value(row.clone()).ok()).flatten().map(|s| &*Box::leak(Box::new(s)))
    })
}

/// A goods' Bullet row (with its AtkParam_Pc) as prosthetic.rs flies them: the throwables'
/// chains (Fistful of Ash 3510 -> 3511 -> 3512, Oil 3520 -> 3521), which the export does not
/// carry (it walks the behaviour judges only).
pub(crate) fn bullet_row(id: i64) -> Option<&'static crate::data::BulletSpec> {
    static ROWS: OnceLock<std::sync::Mutex<HashMap<i64, Option<&'static crate::data::BulletSpec>>>> = OnceLock::new();
    let mut rows = ROWS.get_or_init(Default::default).lock().ok()?;
    *rows.entry(id).or_insert_with(|| {
        let row = param("Bullet", id);
        if row.is_null() {
            return None;
        }
        let mut v = row.clone();
        v["bulletId"] = serde_json::json!(id);
        if let Some(aid) = row["atkId_Bullet"].as_i64().filter(|a| *a > 0) {
            let atk = param("AtkParam_Pc", aid);
            if !atk.is_null() {
                let mut a = atk.clone();
                a["atkParamId"] = serde_json::json!(aid);
                v["attack"] = a;
            }
        }
        serde_json::from_value(v).ok().map(|s| &*Box::leak(Box::new(s)))
    })
}

/// The worn rows' maxHpRate product (Yashariku's Sugar 3411: 0.5); player.rs scales hp_max by it.
pub(crate) fn max_stamina_rate(worn: &[i64]) -> f32 {
    worn.iter().map(|id| param("SpEffectParam", *id)["maxStaminaRate"].as_f64().filter(|r| *r > 0.0).unwrap_or(1.0) as f32).product()
}

/// The worn rows' maxHpRate product (see `max_stamina_rate` for the posture's).
pub(crate) fn max_hp_rate(worn: &[i64]) -> f32 {
    worn.iter().map(|id| param("SpEffectParam", *id)["maxHpRate"].as_f64().filter(|r| *r > 0.0).unwrap_or(1.0) as f32).product()
}

/// Divine Confetti's spirit: the worn rows' magicAttackPower (20), added to the sword's
/// attackBaseMagic (wepParamChange 1: the weapon's) and scaled as its physical part (the sword's
/// atkMagCorrection is 0). gap: the exe's spirit damage and the apparitions it opens.
pub(crate) fn magic_bonus(worn: &[i64]) -> f32 {
    worn.iter().map(|id| param("SpEffectParam", *id)["magicAttackPower"].as_f64().unwrap_or(0.0) as f32).sum()
}

/// The worn rows' soulRate (the Mibu Balloon of Wealth's 1.5): the Sen a kill gives.
pub(crate) fn sen_rate(worn: &[i64]) -> f32 {
    worn.iter().map(|id| param("SpEffectParam", *id)["soulRate"].as_f64().unwrap_or(1.0) as f32).product()
}

/// What an instant row (effectEndurance 0) of an item's chain does: HP and posture back (% of
/// the maximum) and the status types it cures (0 poison, 1 burn, 2 terror).
pub(crate) fn instant(sp: i64) -> (f32, f32, [bool; 3]) {
    let s = param("SpEffectParam", sp);
    if s.is_null() || s["effectEndurance"].as_f64().unwrap_or(0.0) != 0.0 {
        return (0.0, 0.0, [false; 3]);
    }
    let f = |k: &str| s[k].as_f64().unwrap_or(0.0) as f32;
    let info = s["stateInfo"].as_i64().unwrap_or(0);
    // stateInfo 13: Divine Grass's "abnormal condition recovery".
    let all = info == 13;
    let cures = [
        all || (info == 2 && f("poizonAttackPower") < 0.0) || s["spCategory"].as_i64() == Some(194),
        all || (info == 6 && f("registBlood") < 0.0),
        all || (info == 5 && f("registIllness") < 0.0),
    ];
    ((-f("changeHpEstusFlaskRate")).max(0.0), f("changeStaminaRate").max(0.0), cures)
}

/// Each item used: off the bag, then what it does beyond its worn rows (module doc).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn apply_used(
    mut commands: Commands,
    mut used: ResMut<Used>,
    mut config: ResMut<crate::config::GameConfig>,
    mut statuses: Option<ResMut<crate::status::Statuses>>,
    mut wolf: Query<(Entity, &mut crate::player::Player, &mut crate::actor::Actor, &Transform)>,
    enemies: Query<(&crate::actor::Actor, &Transform, &crate::enemy::Enemy), Without<crate::player::Player>>,
    lock: Option<Res<crate::view::LockOn>>,
    mut log: Option<ResMut<crate::combat::CombatLog>>,
    mut bursts: Option<ResMut<crate::status::SpBursts>>,
    mut chain: Local<u32>,
) {
    for id in std::mem::take(&mut used.0) {
        spend(&mut config, id);
        let Ok((we, mut p, mut a, tf)) = wolf.single_mut() else { continue };
        let g = param("EquipParamGoods", id);
        let mut say = |s: String| {
            if let Some(l) = log.as_mut() {
                l.push(s, Color::srgb(0.5, 1.0, 0.6));
            }
        };
        for sp in effects_of(id) {
            // An instant row's look (its SpEffectVfx init effects) once; lasting ones show while
            // worn (status.rs `lasting_glow`).
            if param("SpEffectParam", sp)["effectEndurance"].as_f64().unwrap_or(0.0) <= 0.0 {
                if let Some(b) = bursts.as_mut() {
                    b.0.push((we, sp));
                }
            }
            let (hp, posture, cures) = instant(sp);
            if hp > 0.0 && a.hp > 0.0 {
                a.hp = (a.hp + a.hp_max * hp / 100.0).min(a.hp_max);
            }
            if posture > 0.0 {
                a.posture = (a.posture - a.posture_max * posture / 100.0).max(0.0);
            }
            if cures.iter().any(|c| *c) {
                if let Some(s) = statuses.as_mut() {
                    s.cure(we, cures);
                }
                say(format!("cured{}{}{}", if cures[0] { " poison" } else { "" }, if cures[1] { " burn" } else { "" }, if cures[2] { " terror" } else { "" }));
            }
        }
        match g["goodsUseAnim"].as_i64().unwrap_or(255) {
            // The coin purses: their sellValue in Sen.
            60 => {
                let sen = g["sellValue"].as_u64().unwrap_or(0) as u32;
                config.player.sen = config.player.sen.saturating_add(sen);
                say(format!("+{sen} Sen"));
            }
            // The Ceremonial Tanto: half the max HP (never the last point) for Spirit Emblems.
            80 => {
                let cost = a.hp_max * g["consumeHpRate"].as_f64().unwrap_or(0.0) as f32;
                a.hp = (a.hp - cost).max(1.0);
                let n = g["useGetItemNum"].as_u64().unwrap_or(0) as u32;
                p.emblems = (p.emblems + n).min(config.player.spirit_emblems.max(p.emblems));
                say(format!("-{cost:.0} HP, Spirit Emblems {}", p.emblems));
            }
            // Thrown from the hand at the lock-on target (else the nearest enemy within 15 deg
            // of the facing, 30 m: LockCamParam's bullet capture), else ahead.
            71 => {
                let Some(spec) = g["refId_default"].as_i64().and_then(bullet_row) else { continue };
                let from = tf.translation + a.forward() * 0.4 + Vec3::Y * 0.5;
                let alive = |ea: &crate::actor::Actor, e: &crate::enemy::Enemy| ea.hp > 0.0 && !e.is_dead();
                let locked = lock.as_ref().and_then(|l| l.target).and_then(|t| enemies.get(t).ok()).filter(|(ea, _, e)| alive(ea, e)).map(|(_, t, _)| t.translation);
                let captured = || {
                    enemies
                        .iter()
                        .filter(|(ea, t, e)| {
                            let v = t.translation - tf.translation;
                            alive(ea, e) && v.length() <= 30.0 && v.with_y(0.0).normalize_or_zero().angle_between(a.forward()).to_degrees() <= 15.0
                        })
                        .map(|(_, t, _)| t.translation)
                        .min_by(|x, y| x.distance(tf.translation).total_cmp(&y.distance(tf.translation)))
                };
                let dir = locked.or_else(captured).map_or(a.forward(), |t| (t + Vec3::Y * 0.3 - from).normalize_or(a.forward()));
                *chain += 1;
                // Base -1: the AtkParam's own flat damage (Oil's 1), no weapon row.
                crate::prosthetic::spawn_row(&mut commands, spec, from, dir, 0x4000_0000 + *chain, -1, -1);
            }
            // The Snap Seed (ITEM_ILLUSION_BREAK): its Bullet 3530 around Wolf, a sphere of 2 m
            // spreading to 16 m in 0.5 s (life 1 s), whose AtkParam_Pc 3530 undoes the illusions
            // (prosthetic.rs hit_enemy).
            84 => {
                let Some(spec) = g["refId_default"].as_i64().and_then(bullet_row) else { continue };
                *chain += 1;
                crate::prosthetic::spawn_row(&mut commands, spec, tf.translation + Vec3::Y, a.forward(), 0x4000_0000 + *chain, -1, -1);
            }
            // Five-color Rice: its Bullet 3580 let fall from the hand (0.1 m/s, gravity 10, its
            // rice 310162); where it lands its sfxId_Hit 310163 marks the ground (prosthetic.rs).
            4 => {
                let Some(spec) = g["refId_default"].as_i64().and_then(bullet_row) else { continue };
                let from = tf.translation + a.forward() * 0.4 + Vec3::Y * 0.3;
                *chain += 1;
                crate::prosthetic::spawn_row(&mut commands, spec, from, a.forward(), 0x4000_0000 + *chain, -1, -1);
            }
            _ => {}
        }
    }
}

/// Spirit Emblems (goods 1000) go to Wolf's emblems, not the bag.
pub(crate) const SPIRIT_EMBLEM: i64 = 1000;

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Used>().add_systems(Update, (apply_used, refill_rested));
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
        wear(&mut timed, 3401, 1.0);
        assert_eq!(timed, vec![(3401, 30.0)]);
        // The buff-duration latent skill (SkillParam 370 -> 150600 extendLifeRate 1.5): 45 s.
        let ext = extend_rate([150600].into_iter());
        assert!((ext - 1.5).abs() < 1e-6, "{ext}");
        let mut long = Vec::new();
        wear(&mut long, 3401, ext);
        assert_eq!(long, vec![(3401, 45.0)]);
        let r = wolf_rates(&timed);
        assert!((r[0] - 1.125).abs() < 1e-4 && (r[1] - 1.25).abs() < 1e-4 && r[2] == 1.0, "{r:?}");
        // Ungo's Sugar cuts the damage taken, Gokan's the posture damage.
        let mut t2 = Vec::new();
        for g in [3420, 3430] {
            for sp in effects_of(g) {
                wear(&mut t2, sp, 1.0);
            }
        }
        let r = wolf_rates(&t2);
        assert!((r[2] - 0.67).abs() < 1e-3 && (r[3] - 0.33).abs() < 1e-3, "{r:?}");
    }

    /// The rows the module doc names: the cures' instants, the throwables' chains from the
    /// params, Divine Confetti's and the Balloon of Wealth's rates.
    #[test]
    fn the_items_read_their_rows() {
        // Divine Grass: full HP and posture, every cure.
        let grass: Vec<_> = effects_of(3040).into_iter().map(instant).collect();
        assert!(grass.iter().any(|(hp, posture, _)| *hp >= 100.0 && *posture >= 100.0), "{grass:?}");
        assert!(grass.iter().any(|(.., c)| *c == [true; 3]));
        // Antidote / Dousing / Pacifying: their own type only.
        for (goods, ty) in [(3200, 0), (3210, 1), (3220, 2)] {
            let cures: Vec<[bool; 3]> = effects_of(goods).into_iter().map(|s| instant(s).2).filter(|c| c.iter().any(|x| *x)).collect();
            assert!(!cures.is_empty() && cures.iter().all(|c| c[ty]), "{goods}: {cures:?}");
        }
        // Fistful of Ash: the launch -> the throw -> the cloud with its blinding row.
        let ash = bullet_row(3510).expect("3510");
        assert_eq!(ash.hit_bullet_id, 3511);
        let cloud = bullet_row(3512).expect("3512");
        assert!(cloud.sp_effect_id0 == 220018 && cloud.attack.is_some());
        assert_eq!(sp_row(220018).map(|s| s.behavior_ref_id), Some(1000058));
        // Oil flies at 40 m/s and carries 3520.
        let oil = bullet_row(3521).expect("3521");
        assert!(oil.init_vellocity == 40.0 && oil.sp_effect_id0 == 3520);
        // Red Lump's damage levels and fire weakness.
        let lump = sp_row(3051).expect("3051");
        assert!(lump.sp_category == 1001 && lump.damage_cut.fire == 2.0);
        assert_eq!(magic_bonus(&[3451]), 20.0);
        assert_eq!(sen_rate(&[3601]), 1.5);
    }

    /// Every usable item of the starter bag shows its looks: the use clip's SpawnFFX_GoodsAndMagic
    /// (TAE 114) effects (ffx.rs: castSfxId / fireSfxId / effectSfxId) and its SpEffects'
    /// SpEffectVfx effects are in the extracted FXRs (tools/fxr_extract.py).
    #[test]
    fn every_bag_item_has_its_effects() {
        let root = crate::paths::extracted();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("combat_data.json")).unwrap()).unwrap();
        let (states, anims) = (&v["player"]["states"], &v["player"]["anims"]);
        let vfx: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("json/params/SpEffectVfxParam.json")).unwrap()).unwrap();
        let vfx_row = |id: i64| match &vfx["rows"] {
            serde_json::Value::Array(a) => a.iter().find(|r| r["id"].as_i64() == Some(id)).cloned(),
            o => o.get(id.to_string()).cloned(),
        };
        let have = |id: i64| root.join(format!("fxr/{id}.json")).exists();
        let mut missing = Vec::new();
        for (goods, _) in crate::config::GameConfig::load().unwrap().player.items {
            let g = param("EquipParamGoods", goods);
            let Some(state) = use_state(g["goodsUseAnim"].as_i64().unwrap_or(255)) else { continue };
            let clip = states[state].as_str().unwrap();
            for e in anims[clip]["events"].as_array().unwrap().iter().filter(|e| e["type"] == 114) {
                let col: usize = e["args"]["SFXIndexID"].as_str().unwrap().split(':').next().unwrap().parse().unwrap();
                let id = g[["castSfxId", "fireSfxId", "effectSfxId"][col]].as_i64().unwrap_or(-1);
                if id > 0 && !have(id) {
                    missing.push(format!("{goods} {clip} sfx {id}"));
                }
            }
            for sp in effects_of(goods) {
                let s = param("SpEffectParam", sp);
                for k in ["vfxId", "vfxId1"] {
                    let Some(r) = s[k].as_i64().filter(|v| *v > 0).and_then(vfx_row) else { continue };
                    for f in ["initSfxId", "midstSfxId"] {
                        let id = r[f].as_i64().unwrap_or(-1);
                        if id > 0 && !have(id) {
                            missing.push(format!("{goods} sp {sp} {f} {id}"));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{missing:#?}");
    }

    #[test]
    fn every_quick_item_state_is_exported() {
        let path = crate::paths::extracted().join("combat_data.json");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let states = &v["player"]["states"];
        for anim in [4, 20, 30, 34, 40, 41, 42, 43, 44, 45, 50, 60, 70, 71, 80, 82, 85, 86, 87, 88, 89] {
            let s = use_state(anim).unwrap();
            assert!(states[s].is_string(), "{s} (goodsUseAnim {anim}) has no clip");
        }
    }
}
