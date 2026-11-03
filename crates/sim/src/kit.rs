//! Wolf's start kit from FwLo's RustSekiro `config.toml`, with Sen and skill points at their
//! new-game zero (CharaInitParam `soul` and `skillLv` are 0 on every default row).
//! Kills pay their NpcParam `getSoul` Sen and `skillPoint` experience ([`Kit::earn`]); a full
//! experience gauge is a skill point, the gauge per point from [`xp_to_next`].
//! gap: only [`Kit::use_goods`] spends (its effect unmodelled; no shops or skills), and death
//! does not take its half of the Sen and experience (TentativePlayerParam `MoneyLostRate` /
//! `SkillPointLostRate` 50).
//! Map items (`crate::items`) go into the kit with [`Kit::add`], and their taken flags too;
//! what is over the carry limit goes to the Storage (`storage`), and Rest at an idol or a
//! respawn after death refills the gourd and tops the carried goods back up from it
//! ([`Kit::restock`]), each as its EquipParamGoods row says ([`GoodsRule`]). The gourd's
//! charges, the Spirit Emblems and the other goods are counted, added and used through one
//! place ([`Kit::held`]).
//! gap: the gourd's maximum stays at the start kit's 3 (no Gourd Seeds).

use std::collections::{BTreeMap, BTreeSet};

use crate::items::GoodsRule;

/// Experience from skill point n to n + 1 (n = points earned so far, spent or not); past the
/// table the last step repeats. The exe computes it, not the params: the table is the one the
/// Fextralife wiki measured (Skill Points and Experience), as FwLo's RustSekiro
/// `src/experience.rs` (MIT) ships it.
const XP_TO_NEXT: [u32; 128] = [
    500, 514, 528, 542, 557, 572, 587, 602, 618, 634, 650, 666, 682, 698, 715, 732, 749, 766, 784,
    802, 820, 838, 856, 874, 893, 1093, 1300, 1515, 1738, 1970, 2210, 2458, 2715, 2980, 3254, 3538,
    3830, 4131, 4442, 4762, 5092, 5431, 5780, 6139, 6507, 6886, 7276, 7675, 8085, 8506, 8938, 9380,
    9833, 10297, 10773, 11260, 11758, 12268, 12789, 13322, 13868, 14425, 14994, 15576, 16170,
    16777, 17396, 18028, 18673, 19331, 20002, 20686, 21383, 22094, 22819, 23558, 24310, 25076,
    25856, 26651, 27460, 28283, 29121, 29973, 30840, 31723, 32620, 33532, 34460, 35403, 36362,
    37336, 38326, 39332, 40354, 41392, 42446, 43516, 44603, 45707, 46828, 47965, 49119, 50290,
    51479, 52685, 53908, 55149, 56407, 57683, 58977, 60290, 61620, 62969, 64336, 65722, 67126,
    68549, 69991, 71452, 72932, 74431, 75949, 77487, 79045, 80622, 82220, 83837,
];

/// Experience needed from `earned` skill points to the next.
fn xp_to_next(earned: u32) -> u32 {
    XP_TO_NEXT[(earned as usize).min(XP_TO_NEXT.len() - 1)]
}

/// One defeat's gain, as [`Kit::earn`] recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Reward {
    /// Experience gained.
    pub xp: u32,
    /// Sen gained.
    pub sen: u32,
    /// Skill points gained.
    pub points: u32,
}

/// What Wolf carries, has learned and has found, independent of the screen that shows it.
/// It is plain data (serde), so Travel to an idol on another map hands it to the next run; a
/// field missing from the JSON (a partial `--kit`, or a kit from before a field was added)
/// keeps its start-kit value.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Kit {
    /// Healing Gourd uses left.
    pub gourd_charges: u32,
    /// Healing Gourd uses when full, what Rest and a respawn refill it to (#184; Gourd Seeds
    /// raise it in the game, not modelled).
    pub gourd_max: u32,
    /// Spirit Emblems carried (the menu's starting capacity is 15).
    pub spirit_emblems: u32,
    /// Resurrection nodes available.
    pub resurrections: u32,
    /// Equipped combat art, an EquipParamWeapon id.
    pub combat_art: i32,
    /// Fitted tools, up to three EquipParamWeapon ids.
    pub prosthetics: Vec<i32>,
    /// Quick-item EquipParamGoods ids, with 0 for an empty slot.
    pub quick_items: [i32; 5],
    /// Goods held besides the gourd and emblems: EquipParamGoods id and count.
    pub goods: Vec<(i32, u32)>,
    /// Goods sent to the Storage for being over the carry limit, Spirit Emblems (goods 1000)
    /// included: EquipParamGoods id and count.
    pub storage: BTreeMap<i32, u32>,
    /// Sen carried.
    pub sen: u32,
    /// Skill points available.
    pub skill_points: u32,
    /// Skill points earned in all, spent or not: it picks the gauge size ([`xp_to_next`]).
    /// It equals the points earned, so it differs from `skill_points` once skills are bought.
    pub skill_level: u32,
    /// Experience toward the next skill point.
    pub skill_xp: u32,
    /// Whether every skill counts as learned (FwLo's `unlock_all_moves`).
    pub all_skills: bool,
    /// Learned SkillParam ids when `all_skills` is false.
    pub skills: Vec<i32>,
    /// Earns that gained anything, so a screen can tell a new reward from later spending.
    pub rewards: u32,
    /// The gain of the latest of those earns.
    pub last_reward: Reward,
    /// ItemLotParam `getItemFlagId`s of the map items taken: they stay gone.
    pub taken_flags: BTreeSet<i32>,
    /// The Sculptor's Idols found (lit by the first kneel at each, #152), by BonfireWarpParam
    /// `bonfireEntityId`: Travel lists these (#148).
    pub found_idols: BTreeSet<i32>,
}

impl Default for Kit {
    fn default() -> Self {
        Self {
            gourd_charges: 3,
            gourd_max: 3,
            spirit_emblems: 15,
            resurrections: 2,
            combat_art: 5100,
            prosthetics: vec![70000],
            quick_items: [3000, 3400, 3020, 3070, 3980],
            goods: vec![
                (3020, 3),
                (3040, 1),
                (3050, 3),
                (3060, 1),
                (3070, 3),
                (3080, 3),
                (3200, 3),
                (3210, 3),
                (3220, 3),
                (3230, 3),
                (3400, 3),
                (3410, 3),
                (3420, 3),
                (3430, 3),
                (3440, 3),
                (3450, 5),
                (3500, 10),
                (3510, 5),
                (3520, 5),
                (3580, 5),
                (3600, 3),
                (3610, 3),
                (3620, 3),
                (3630, 3),
                (3700, 3),
                (3704, 2),
                (3708, 1),
                (3800, 1),
                (3980, 1),
                (6000, 20),
                (6010, 5),
                (6100, 10),
                (6200, 5),
                (6300, 10),
                (6400, 3),
                (9720, 1),
                (9750, 1),
            ],
            storage: BTreeMap::new(),
            sen: 0,
            skill_points: 0,
            skill_level: 0,
            skill_xp: 0,
            all_skills: true,
            skills: vec![280],
            rewards: 0,
            last_reward: Reward::default(),
            taken_flags: BTreeSet::new(),
            found_idols: BTreeSet::new(),
        }
    }
}

impl Kit {
    /// The number of this goods id held: the gourd's charges, the Spirit Emblems or a goods
    /// entry (zero when absent).
    pub fn count(&self, id: i32) -> u32 {
        self.held(id).unwrap_or(0)
    }

    /// The number of this goods id in the Storage (zero when absent).
    pub fn stored(&self, id: i32) -> u32 {
        self.storage.get(&id).copied().unwrap_or(0)
    }

    /// Takes `n` of the goods `id` up to its carry limit, the rest to the Storage up to what the
    /// Storage keeps of it (beyond that it is lost). The gourd (goods 3000) fills its charges
    /// and Spirit Emblems (goods 1000) their count, as [`Kit::held`] keeps them.
    pub fn add(&mut self, id: i32, n: u32, rule: GoodsRule) {
        let over = n - self.carry(id, n, rule.carry);
        if over > 0 && rule.store > 0 {
            let stored = self.storage.entry(id).or_default();
            *stored = stored.saturating_add(over).min(rule.store);
        }
    }

    /// Rest at an idol, or a respawn after death: the Healing Gourd is refilled to
    /// `gourd_max`, and every goods in the Storage whose rule restocks is carried again up to
    /// its carry limit, the rest left stored. EquipParamGoods `autoReplenishType` marks it
    /// ([`GoodsRule::restock`]); the exe's use of it is not read, so this takes from the Storage
    /// only what tops the carried count up, as the game behaves.
    /// The full-supply goods held (EquipParamGoods `isFullSuppleItem` 1, [`GoodsRule::refill`],
    /// such as Five-color Rice) are set to their carry limit from nothing, without the Storage,
    /// as FwLo's RustSekiro `refill_rested` does (#208). The gourd's row is one too, but its
    /// charges are kept apart and follow `gourd_max`.
    pub fn restock(&mut self, rule: impl Fn(i32) -> GoodsRule) {
        self.gourd_charges = self.gourd_charges.max(self.gourd_max);
        for (id, n) in &mut self.goods {
            let r = rule(*id);
            if r.refill {
                *n = r.carry;
            }
        }
        for (id, n) in std::mem::take(&mut self.storage) {
            let r = rule(id);
            let taken = if r.restock {
                self.carry(id, n, r.carry)
            } else {
                0
            };
            if n > taken {
                self.storage.insert(id, n - taken);
            }
        }
    }

    /// Carries up to `n` more of `id` without passing `limit`; returns how many it took.
    fn carry(&mut self, id: i32, n: u32, limit: u32) -> u32 {
        let taken = n.min(limit.saturating_sub(self.count(id)));
        if taken > 0 {
            match self.held_mut(id) {
                Some(held) => *held += taken,
                None => self.goods.push((id, taken)),
            }
        }
        taken
    }

    /// A defeated enemy's reward: its experience fills the gauge, each full gauge a skill point
    /// with the rest carried over. Returns the skill points gained.
    /// Records the gain in `rewards` and `last_reward`.
    pub fn earn(&mut self, xp: u32, sen: u32) -> u32 {
        let before = self.skill_points;
        self.sen = self.sen.saturating_add(sen);
        self.skill_xp = self.skill_xp.saturating_add(xp);
        while self.skill_xp >= self.xp_needed() {
            self.skill_xp -= self.xp_needed();
            self.skill_level += 1;
            self.skill_points += 1;
        }
        let points = self.skill_points - before;
        if xp > 0 || sen > 0 {
            self.rewards += 1;
            self.last_reward = Reward { xp, sen, points };
        }
        points
    }

    /// The experience gauge's size at the current skill level.
    fn xp_needed(&self) -> u32 {
        xp_to_next(self.skill_level)
    }

    /// Experience still needed for the next skill point.
    pub fn xp_left(&self) -> u32 {
        self.xp_needed().saturating_sub(self.skill_xp)
    }

    /// How full the experience gauge is, 0 to 1.
    pub fn xp_fill(&self) -> f32 {
        (self.skill_xp as f32 / self.xp_needed() as f32).clamp(0.0, 1.0)
    }

    /// Uses one of the goods `id` (the gourd: one charge); false when none is left.
    /// gap: only the count drops; the item's effect (its SpEffect and use animation) is not
    /// modelled yet.
    pub fn use_goods(&mut self, id: i32) -> bool {
        match self.held_mut(id) {
            Some(held) if *held > 0 => {
                *held -= 1;
                true
            }
            _ => false,
        }
    }

    /// The count of `id` where it is kept: the gourd's charges, the Spirit Emblems or its
    /// goods entry (None when absent). The one place those three are told apart, with
    /// [`Kit::held_mut`].
    fn held(&self, id: i32) -> Option<u32> {
        match id {
            crate::items::HEALING_GOURD => Some(self.gourd_charges),
            crate::items::SPIRIT_EMBLEM => Some(self.spirit_emblems),
            _ => self.goods.iter().find(|(g, _)| *g == id).map(|(_, n)| *n),
        }
    }

    /// [`Kit::held`]'s count, to change it.
    fn held_mut(&mut self, id: i32) -> Option<&mut u32> {
        match id {
            crate::items::HEALING_GOURD => Some(&mut self.gourd_charges),
            crate::items::SPIRIT_EMBLEM => Some(&mut self.spirit_emblems),
            _ => self
                .goods
                .iter_mut()
                .find(|(g, _)| *g == id)
                .map(|(_, n)| n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_kit_keeps_the_start_kit_for_what_it_leaves_out() {
        let kit: Kit = serde_json::from_str(r#"{"sen":1}"#).unwrap();
        assert_eq!(
            kit,
            Kit {
                sen: 1,
                ..Kit::default()
            }
        );
        // A whole kit comes back as it went.
        let json = serde_json::to_string(&kit).unwrap();
        assert_eq!(serde_json::from_str::<Kit>(&json).unwrap(), kit);
    }

    #[test]
    fn full_gauges_roll_over_into_skill_points() {
        let mut kit = Kit::default();
        assert_eq!(kit.earn(499, 7), 0);
        assert_eq!(kit.earn(1, 0), 1);
        assert_eq!((kit.skill_points, kit.skill_level, kit.skill_xp), (1, 1, 0));
        // 514 + 528 for the next two, 10 over; the gauge grows with the points earned.
        assert_eq!(kit.earn(514 + 528 + 10, 0), 2);
        assert_eq!((kit.skill_points, kit.skill_xp, kit.sen), (3, 10, 7));
        assert_eq!(
            (kit.rewards, kit.last_reward.xp, kit.last_reward.points),
            (3, 1052, 2)
        );
        assert_eq!(xp_to_next(25), 1093);
        assert_eq!(xp_to_next(1000), 83837);
    }

    #[test]
    fn counts_the_gourd_emblems_goods_and_missing_items() {
        let kit = Kit::default();
        assert_eq!(kit.count(crate::items::HEALING_GOURD), 3);
        assert_eq!(kit.count(crate::items::SPIRIT_EMBLEM), 15);
        assert_eq!(kit.count(3020), 3);
        assert_eq!(kit.count(1), 0);
    }

    #[test]
    fn using_goods_takes_one_until_none_is_left() {
        let mut kit = Kit::default();
        assert!(kit.use_goods(crate::items::HEALING_GOURD));
        assert_eq!(kit.gourd_charges, 2);
        assert!(kit.use_goods(3040));
        assert_eq!(kit.count(3040), 0);
        assert!(!kit.use_goods(3040), "none left");
        assert!(!kit.use_goods(1), "never held");
    }

    #[test]
    fn goods_over_the_carry_limit_go_to_the_storage() {
        let mut kit = Kit::default();
        // Pellets (3020) are at their limit of 3: both go to the Storage.
        kit.add(3020, 2, GoodsRule::consumable(3));
        assert_eq!((kit.count(3020), kit.stored(3020)), (3, 2));
        // Room for one of two Spirit Emblems.
        kit.spirit_emblems = 14;
        kit.add(crate::items::SPIRIT_EMBLEM, 2, GoodsRule::consumable(15));
        assert_eq!((kit.spirit_emblems, kit.stored(1000)), (15, 1));
        // A goods not held is carried, with nothing stored.
        kit.add(7, 2, GoodsRule::consumable(99));
        assert_eq!((kit.count(7), kit.stored(7)), (2, 0));
        // Nothing carried at a limit of 0: no empty goods entry.
        kit.add(8, 1, GoodsRule::consumable(0));
        assert_eq!((kit.count(8), kit.stored(8)), (0, 1));
        assert!(kit.goods.iter().all(|(g, _)| *g != 8));
    }

    #[test]
    fn adding_the_gourd_fills_its_charges_and_loses_the_rest() {
        let mut kit = Kit::default();
        let gourd = crate::items::HEALING_GOURD;
        kit.gourd_charges = 1;
        // Room for two of three charges; goods 3000 has no Storage (isDeposit 0).
        let rule = GoodsRule {
            store: 0,
            ..GoodsRule::consumable(3)
        };
        kit.add(gourd, 3, rule);
        assert_eq!((kit.gourd_charges, kit.stored(gourd)), (3, 0));
        assert!(kit.goods.iter().all(|(g, _)| *g != gourd));
        assert!(kit.storage.is_empty());
    }

    #[test]
    fn rest_refills_the_gourd_and_restocks_goods_from_the_storage_up_to_their_limit() {
        let mut kit = Kit::default();
        let emblem = crate::items::SPIRIT_EMBLEM;
        kit.storage = BTreeMap::from([(3020, 5), (3500, 1), (emblem, 4), (5400, 2)]);
        // Two Pellets used, all 10 Ceramic Shards thrown, 13 of 15 Spirit Emblems spent.
        kit.use_goods(3020);
        kit.use_goods(3020);
        kit.goods.retain(|(g, _)| *g != 3500);
        kit.spirit_emblems = 2;
        // One gourd use drunk: the gourd has no Storage, it refills to its maximum.
        kit.use_goods(crate::items::HEALING_GOURD);
        let rule = |id| match id {
            3020 => GoodsRule::consumable(3),
            // A goods whose row does not restock stays stored.
            5400 => GoodsRule {
                restock: false,
                ..GoodsRule::consumable(1)
            },
            _ => GoodsRule::consumable(if id == emblem { 15 } else { 10 }),
        };
        kit.restock(rule);
        assert_eq!(kit.gourd_charges, kit.gourd_max);
        assert_eq!((kit.count(3020), kit.stored(3020)), (3, 3));
        assert_eq!((kit.count(3500), kit.stored(3500)), (1, 0));
        assert_eq!((kit.spirit_emblems, kit.stored(emblem)), (6, 0));
        assert_eq!((kit.count(5400), kit.stored(5400)), (0, 2));
        assert_eq!(
            kit.storage.keys().copied().collect::<Vec<_>>(),
            [3020, 5400]
        );
    }

    /// Pellets (goods 3020: maxNum 3, isDeposit 1, autoReplenishType 2) and the gourd (3000:
    /// isDeposit 0) as EquipParamGoods has them.
    fn pellet_rules(id: i32) -> GoodsRule {
        match id {
            3020 => GoodsRule::consumable(3),
            crate::items::HEALING_GOURD => GoodsRule {
                store: 0,
                ..GoodsRule::consumable(10)
            },
            _ => GoodsRule::UNKNOWN,
        }
    }

    /// The start kit with two Pellets in the Storage, as after picking up m11_00's pair (#202).
    fn kit_with_stored_pellets() -> Kit {
        let mut kit = Kit::default();
        kit.storage.insert(3020, 2);
        kit
    }

    #[test]
    fn rest_with_every_pellet_carried_leaves_the_storage_alone() {
        let mut kit = kit_with_stored_pellets();
        // The start kit carries the Pellets' limit, so a first Rest has nothing to top up.
        assert_eq!(kit.count(3020), pellet_rules(3020).carry);
        let before = kit.clone();
        kit.restock(pellet_rules);
        assert_eq!(kit, before);
    }

    #[test]
    fn rest_after_one_pellet_moves_exactly_one_from_the_storage() {
        let mut kit = kit_with_stored_pellets();
        assert!(kit.use_goods(3020));
        kit.restock(pellet_rules);
        assert_eq!((kit.count(3020), kit.stored(3020)), (3, 1));
    }

    #[test]
    fn the_gourd_refill_does_not_touch_the_pellets() {
        let mut kit = kit_with_stored_pellets();
        assert!(kit.use_goods(crate::items::HEALING_GOURD));
        assert!(kit.use_goods(crate::items::HEALING_GOURD));
        kit.restock(pellet_rules);
        assert_eq!(kit.gourd_charges, kit.gourd_max);
        assert_eq!((kit.count(3020), kit.stored(3020)), (3, 2));
    }

    #[test]
    fn restocking_twice_moves_nothing_more() {
        // A Rest then a Travel, or a Rest then a death: each restocks.
        let mut kit = kit_with_stored_pellets();
        assert!(kit.use_goods(3020));
        kit.restock(pellet_rules);
        let once = kit.clone();
        kit.restock(pellet_rules);
        kit.restock(pellet_rules);
        assert_eq!(kit, once);
        assert_eq!((kit.count(3020), kit.stored(3020)), (3, 1));
    }

    #[test]
    fn rest_refills_full_supply_goods_to_their_limit_without_the_storage() {
        // Five-color Rice 3580 (maxNum 5) and goods 3800 (maxNum 3): isFullSuppleItem 1,
        // isDeposit 0. The gourd's row is one too (maxNum 10), but its charges follow gourd_max.
        let full = |carry| GoodsRule {
            store: 0,
            refill: true,
            ..GoodsRule::consumable(carry)
        };
        let rule = |id| match id {
            3580 => full(5),
            3800 => full(3),
            crate::items::HEALING_GOURD => full(10),
            _ => pellet_rules(id),
        };
        let mut kit = kit_with_stored_pellets();
        for _ in 0..5 {
            assert!(kit.use_goods(3580));
        }
        assert!(kit.use_goods(crate::items::HEALING_GOURD));
        assert_eq!((kit.count(3580), kit.count(3800)), (0, 1));
        kit.restock(rule);
        assert_eq!((kit.count(3580), kit.count(3800)), (5, 3));
        assert_eq!(kit.gourd_charges, kit.gourd_max);
        // Nothing comes from or goes to the Storage for them, and the Pellets stay as they were.
        assert_eq!(kit.storage, BTreeMap::from([(3020, 2)]));
        assert_eq!(kit.count(3020), 3);
    }
}
