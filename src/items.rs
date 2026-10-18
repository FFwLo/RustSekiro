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
//!   whose hit and landing sound (AtkParam HitAiSoundId 3500) the enemies hear.
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

/// The use state of a goodsUseAnim: the behaviour's Item* states, whose clips run in goodsUseAnim
/// order (40-44 the sugars -> a000_250800, 250900, 251000, 251100, 251200). Matched by what the
/// items are: the Ceremonial Tanto (80) trades HP for Spirit Emblems, katashiro ("paper dolls"):
/// ItemPaperDollExchangeWhite (a000_250600, the stab, its castSfxId / fireSfxId windows);
/// ItemSuicide (a000_250400, the collapse) is Bite Down's (82) feigned death (the user,
/// 2026-10-18: "Wolf plays death animation when using tanto").
/// gap: 83 (Nightjar Monocular: the telescope's Start / Loop / End) and 84 (Snap Seed) have none.
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
        80 => "ItemPaperDollExchangeWhite",
        82 => "ItemSuicide",
        85 => "ItemRedoBell",
        86 => "ItemStoneStatue",
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
    lock: Option<Res<crate::camera::LockOn>>,
    mut log: Option<ResMut<crate::hud::CombatLog>>,
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
            _ => {}
        }
    }
}

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Used>()
            .init_resource::<Picked>()
            .init_resource::<DialogUp>()
            .add_systems(Startup, spawn_pickups)
            .add_systems(Update, (apply_used, collect))
            .add_systems(PreUpdate, close_dialog.before(crate::menu::MenuInput));
    }
}

// ---------------------------------------------------------------- pickups

/// An item lying in the world: what and how many, where (feet height), and whether it is gone.
#[derive(Clone, Debug)]
pub(crate) struct Pickup {
    pub(crate) goods: i64,
    pub(crate) count: u32,
    pub(crate) pos: Vec3,
    pub(crate) taken: bool,
}

/// The map's pickups (the parkour course's, `parkour_pickups`).
pub(crate) static PICKUPS: std::sync::RwLock<Vec<Pickup>> = std::sync::RwLock::new(Vec::new());

/// How near (m, level) and how far up or down Wolf's feet must be to take one.
const PICK_REACH: f32 = 1.4;
const PICK_REACH_Y: f32 = 0.8;

/// Items about the parkour course: beside its idols and along the way.
pub(crate) fn parkour_pickups() -> Vec<Pickup> {
    let at = |goods: i64, count: u32, x: f32, y: f32, z: f32| Pickup { goods, count, pos: Vec3::new(x, y, z), taken: false };
    vec![
        // Ceramic Shard and a Pellet near the start, Scrap Iron by Platform Top, Ako's Sugar by
        // Cliff Top, Black Gunpowder and a Gokan's Sugar by Shaft Floor.
        at(3500, 3, -1.0, 0.0, 4.0),
        at(3020, 1, 2.5, 0.0, -1.5),
        at(6000, 3, 10.0, 3.0, -11.8),
        at(3400, 1, -11.6, 4.5, -21.4),
        at(6100, 2, 14.2, 0.0, -21.8),
        at(3430, 1, 16.6, 0.0, -21.8),
        // The Mibu Breathing Technique beside the pool (diving, swim.rs).
        at(crate::swim::MIBU_BREATHING, 1, -18.5, 0.0, 4.0),
        // Gachiin's Sugar at the outpost's gate, two Ako's Sugars on top of its watchtower.
        at(3440, 1, -3.0, 0.0, 10.0),
        at(3400, 2, -21.0, 8.0, 41.0),
    ]
}

/// The untaken pickup in reach of `feet`.
pub(crate) fn pickup_near(feet: Vec3) -> Option<usize> {
    let list = PICKUPS.read().ok()?;
    list.iter().position(|p| !p.taken && (p.pos - feet).with_y(0.0).length() < PICK_REACH && (p.pos.y - feet.y).abs() < PICK_REACH_Y)
}

/// Takes the pickup in reach (player.rs, on E): gone from the world, queued for the bag.
pub(crate) fn take_near(feet: Vec3, picked: &mut Picked) -> bool {
    let Some(i) = pickup_near(feet) else { return false };
    let Ok(mut list) = PICKUPS.write() else { return false };
    list[i].taken = true;
    picked.0.push((i, list[i].goods, list[i].count));
    true
}

/// Pickups taken this frame: (index, goods, count).
#[derive(Resource, Default)]
pub(crate) struct Picked(pub(crate) Vec<(usize, i64, u32)>);

/// A pickup's glint (FFX 61: the item glow common_func 20006030 puts on an object's item).
#[derive(Component)]
struct PickupGlint(usize);

/// The glint of an untaken pickup.
const GLINT_SFX: i64 = 61;

fn spawn_pickups(mut commands: Commands, config: Res<crate::config::GameConfig>, mut picked: ResMut<Picked>) {
    // SHINOBI_ITEM_DIALOG=<goods id>: as if that goods were just picked up (SHINOBI_SHOT captures).
    if let Some(id) = std::env::var("SHINOBI_ITEM_DIALOG").ok().and_then(|v| v.parse::<i64>().ok()) {
        picked.0.push((usize::MAX, id, 1));
    }
    let list = if crate::map::map_id(&config).as_deref() == Some(crate::sandbox::CONSTRUCT_ID) { parkour_pickups() } else { Vec::new() };
    for (i, p) in list.iter().enumerate() {
        let mut fx = crate::fxr::FxEffect::new(GLINT_SFX, None, false, Transform::from_translation(p.pos + Vec3::Y * 0.1));
        fx.keep = true;
        commands.spawn((fx, PickupGlint(i)));
    }
    if let Ok(mut v) = PICKUPS.write() {
        *v = list;
    }
}

/// The item dialog (the user's captures): a goods got for the first time shows its name, icon
/// and text on the parchment (key items between the purple bands) until OK; the game waits.
/// A goods already held shows the short line at the bottom instead.
#[derive(Component)]
struct ItemDialog;

/// The short "got" line and when it went up.
#[derive(Component)]
struct GotLine(f32);

/// How long the short line stays (s).
const LINE_SECS: f32 = 2.5;

/// The goods shown in the dialog now, if any.
#[derive(Resource, Default)]
pub(crate) struct DialogUp(pub(crate) Option<i64>);

/// Puts the taken items in the bag, puts out their glint and shows what was got.
#[allow(clippy::too_many_arguments)]
fn collect(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut virt: ResMut<Time<Virtual>>,
    mut picked: ResMut<Picked>,
    mut up: ResMut<DialogUp>,
    mut config: ResMut<crate::config::GameConfig>,
    glints: Query<(Entity, &PickupGlint)>,
    lines: Query<(Entity, &GotLine)>,
    assets: Res<AssetServer>,
    font: Option<Res<crate::game_menu::MenuFont>>,
    texts: Option<Res<crate::game_menu::ItemTexts>>,
) {
    let now = time.elapsed_secs();
    for (e, l) in &lines {
        if now - l.0 > LINE_SECS {
            commands.entity(e).despawn();
        }
    }
    let font = font.as_ref().and_then(|f| f.0.clone());
    for (i, goods, count) in std::mem::take(&mut picked.0) {
        let first = count_held(&config, goods) == 0;
        let got = add(&mut config, goods, count);
        info!("picked up goods {goods} x{count} ({got} kept)");
        for (e, g) in &glints {
            if g.0 == i {
                commands.entity(e).despawn();
            }
        }
        let name = texts.as_ref().and_then(|t| t.goods_name.get(&(goods as u32)).cloned()).unwrap_or_else(|| goods.to_string());
        let info = texts.as_ref().and_then(|t| t.goods_info.get(&(goods as u32)).cloned()).unwrap_or_default();
        let icon = crate::game_menu::any_icon(param("EquipParamGoods", goods)["iconId"].as_i64().unwrap_or(-1));
        if first && up.0.is_none() {
            let key_item = param("EquipParamGoods", goods)["goodsType"].as_i64() == Some(1);
            spawn_dialog(&mut commands, &assets, &font, &name, &info, icon, key_item);
            up.0 = Some(goods);
            virt.pause();
        } else {
            for (e, _) in &lines {
                commands.entity(e).despawn();
            }
            spawn_line(&mut commands, &assets, &font, &name, icon, count, now);
        }
    }
}

fn count_held(config: &crate::config::GameConfig, id: i64) -> u32 {
    count(config, id)
}

/// The dialog, laid out after the user's 2000 x 1125 capture (in % of the window).
fn spawn_dialog(commands: &mut Commands, assets: &AssetServer, font: &Option<Handle<Font>>, name: &str, info: &str, icon: Option<String>, key_item: bool) {
    use crate::game_menu::{abs, fill_row, image, sprite, text, TEXT};
    commands
        .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), GlobalZIndex(120), ItemDialog))
        .with_children(|root| {
            if key_item {
                root.spawn((abs(26.5, 19.0, 47.1, 6.2), image(assets, sprite("sb_map/MENU_Dialog_valuable"))));
                root.spawn((abs(26.5, 75.6, 47.1, 6.2), image(assets, sprite("sb_map/MENU_Dialog_valuable"))));
            }
            root.spawn((abs(26.2, 24.0, 47.7, 52.6), image(assets, sprite("sb_dialog/MENU_Dialog_base_02"))));
            root.spawn((abs(27.4, 26.2, 45.2, 4.6), image(assets, sprite("sb_in_game/MENU_ItemBase_08")).with_color(Color::srgb(0.22, 0.22, 0.22)))).with_children(|b| {
                b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                    c.spawn(text(name, 21.0, Color::WHITE, font));
                });
            });
            root.spawn((abs(29.9, 34.0, 11.8, 20.4), image(assets, sprite("sb_in_game/MENU_IconBase_01")))).with_children(|b| {
                b.spawn((Node { position_type: PositionType::Absolute, left: Val::Percent(8.0), top: Val::Percent(8.0), width: Val::Percent(84.0), height: Val::Percent(84.0), ..default() }, image(assets, icon)));
            });
            // The text over the parchment down to the rule; a long one is cut there.
            root.spawn(Node { overflow: Overflow::clip(), ..abs(43.8, 33.2, 28.5, 33.6) }).with_children(|c| {
                c.spawn(text(info, 15.0, TEXT, font));
            });
            root.spawn((abs(28.0, 67.3, 41.0, 0.15), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3))));
            root.spawn((Button, abs(41.7, 68.4, 16.8, 6.7), image(assets, sprite("sb_in_game/MENU_Cursor_03")), DialogOk)).with_children(|b| {
                b.spawn(Node { justify_content: JustifyContent::Center, ..fill_row() }).with_children(|c| {
                    c.spawn(text("OK", 17.0, Color::WHITE, font));
                });
            });
        });
}

/// The dialog's OK.
#[derive(Component)]
struct DialogOk;

/// The short line for a goods already held: icon, name and count at the bottom.
fn spawn_line(commands: &mut Commands, assets: &AssetServer, font: &Option<Handle<Font>>, name: &str, icon: Option<String>, count: u32, now: f32) {
    use crate::game_menu::{abs, fill_row, image, sprite, text, TEXT};
    commands.spawn((abs(36.0, 72.0, 28.0, 7.0), image(assets, sprite("sb_in_game/MENU_ItemBase_07")), GlobalZIndex(90), GotLine(now))).with_children(|b| {
        b.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(14.0), padding: UiRect::left(Val::Percent(7.0)), ..fill_row() }).with_children(|r| {
            r.spawn((Node { width: Val::Px(40.0), height: Val::Px(40.0), ..default() }, image(assets, icon)));
            r.spawn(text(name, 18.0, Color::WHITE, font));
            if count > 1 {
                r.spawn(text(&format!("x {count}"), 17.0, TEXT, font));
            }
        });
    });
}

/// OK (Enter, E, Space, Esc or a click) closes the dialog and the game goes on. Its keys are
/// spent here, before the menus and Wolf read them.
fn close_dialog(
    mut commands: Commands,
    mut up: ResMut<DialogUp>,
    mut virt: ResMut<Time<Virtual>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    ok: Query<&Interaction, (With<DialogOk>, Changed<Interaction>)>,
    dialogs: Query<Entity, With<ItemDialog>>,
) {
    if up.0.is_none() {
        return;
    }
    let pressed = [KeyCode::Enter, KeyCode::KeyE, KeyCode::Space, KeyCode::Escape].iter().any(|k| keys.just_pressed(*k)) || mouse.just_pressed(MouseButton::Left) || ok.iter().any(|i| *i == Interaction::Pressed);
    keys.reset_all();
    mouse.reset_all();
    if pressed {
        for e in &dialogs {
            commands.entity(e).despawn();
        }
        up.0 = None;
        virt.unpause();
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
