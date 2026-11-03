//! Picking up map items (#136): the action button inside an item's ActionButtonParam reach takes
//! its lot into Wolf's kit and sets the lot's taken flag there, so the item stays gone through
//! Rest and death. The press goes to the nearer of an item and an idol's kneeling spot.
//! Each pickup keeps its own taken state too, seeded from the kit's flags, so a lot with no
//! flag goes away once taken (for this load).
//! After FwLo's RustSekiro `src/items.rs` (Fireguy32's pickups): goods 1000 is a Spirit Emblem,
//! and there is no pickup animation (60070 has no clip).
//! gap: the lot is drawn with a fixed sequence, not the exe's random numbers (every m11_00
//! treasure lot is one 1000-point entry, so it does not matter there), and ActionButtonParam's
//! angle and dummy polygon (4201, a corpse's purse) are not checked.

use std::sync::Arc;

use super::Duel;
use crate::items::{GoodsRule, ItemData, MapPickup};

/// The play world's map items, by a stable index (later drops append).
#[derive(Debug, Default)]
pub(super) struct Pickups {
    spots: Vec<MapPickup>,
    /// Per pickup: taken already.
    taken: Vec<bool>,
    /// The goods params: what a pickup carries and stores, and what Rest restocks.
    data: Arc<ItemData>,
    /// Pickups taken so far, so a screen can tell a new one; it also seeds the draw.
    count: u32,
    /// The goods (EquipParamGoods id, count) the latest pickup gave.
    last: Option<(i32, u32)>,
}

/// What the action button would act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTarget {
    Idol(usize),
    Pickup(usize),
}

impl Duel {
    /// The play world's items; those whose lot flag the kit holds start taken.
    pub fn set_pickups(&mut self, spots: Vec<MapPickup>, data: Arc<ItemData>) {
        self.pickups.taken = spots
            .iter()
            .map(|p| p.lot.flag > 0 && self.kit.taken_flags.contains(&p.lot.flag))
            .collect();
        self.pickups.spots = spots;
        self.pickups.data = data;
    }

    /// The carry and Storage rule of the goods `id`, from the params [`Duel::set_pickups`] gave
    /// ([`GoodsRule::UNKNOWN`] without them).
    pub fn goods_rule(&self, id: i32) -> GoodsRule {
        self.pickups.data.rule(id)
    }

    /// Rest at an idol or a respawn after death: the gourd refilled and the carried goods
    /// topped up from the Storage ([`crate::kit::Kit::restock`], #186).
    pub(super) fn restock_kit(&mut self) {
        let data = &self.pickups.data;
        self.kit.restock(|id| data.rule(id));
    }

    pub fn pickups(&self) -> &[MapPickup] {
        &self.pickups.spots
    }

    /// Whether the item is still on the map.
    pub fn pickup_lies(&self, i: usize) -> bool {
        self.pickups.taken.get(i) == Some(&false)
    }

    /// The number of pickups taken, and the goods the latest one gave.
    pub fn last_pickup(&self) -> (u32, Option<(i32, u32)>) {
        (self.pickups.count, self.pickups.last)
    }

    /// The nearest item lying with Wolf's feet in its reach, and how far it is.
    fn pickup_near(&self) -> Option<(usize, f32)> {
        let feet = self.player.body.position;
        self.pickups
            .spots
            .iter()
            .enumerate()
            .filter(|(i, _)| self.pickup_lies(*i))
            .filter_map(|(i, p)| Some((i, p.reach.distance(p.position, feet, 0.0)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// What the action button acts on now: the nearer of an idol in reach and an item.
    pub fn action_target(&self) -> Option<ActionTarget> {
        if self.idol().is_some() || !self.can_act() {
            return None;
        }
        match (self.idol_near(), self.pickup_near()) {
            (Some((_, di)), Some((p, dp))) if dp < di => Some(ActionTarget::Pickup(p)),
            (Some((i, _)), _) => Some(ActionTarget::Idol(i)),
            (None, Some((p, _))) => Some(ActionTarget::Pickup(p)),
            (None, None) => None,
        }
    }

    /// The item the action button would pick up (the "Pick Up Item" guide).
    pub fn pickup_in_reach(&self) -> Option<usize> {
        match self.action_target()? {
            ActionTarget::Pickup(p) => Some(p),
            ActionTarget::Idol(_) => None,
        }
    }

    /// Takes the item's lot into the kit and marks it taken.
    pub(super) fn pick_up(&mut self, i: usize) {
        let Some(spot) = self.pickups.spots.get(i) else {
            return;
        };
        let lot = &spot.lot;
        // A golden-ratio sequence in 0..1 stands in for the exe's random draw.
        let r = ((self.pickups.count + 1) as f32 * 0.618_034).fract();
        let got = lot.roll(0.0, r);
        if let Some((id, n)) = got {
            // The line names the lot's count even when some is over the carry limit.
            self.kit.add(id, n, self.goods_rule(id));
        }
        if lot.flag > 0 {
            self.kit.taken_flags.insert(lot.flag);
        }
        eprintln!("picked up {} (ItemLotParam {}): {got:?}", spot.name, lot.id);
        self.pickups.taken[i] = true;
        self.pickups.count += 1;
        self.pickups.last = got;
    }
}
