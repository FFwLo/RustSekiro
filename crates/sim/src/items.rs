//! Items lying on the map (#136): each MSB Treasure event puts an ItemLotParam row at a part,
//! picked up with the action button inside its ActionButtonParam reach
//! (`Duel::action_target`). A lot's `getItemFlagId` marks it taken in Wolf's kit, so a taken
//! item stays gone through Rest and death.
//!
//! The lot draw ([`pick`], [`ItemLot::roll`]) is ported from FwLo's RustSekiro `src/drops.rs`
//! (Fireguy32's drops, MIT): one draw over the eight entries weighted by `lotItemBasePoint`,
//! goods only, `enableLuck` entries weighted up by the item drop rate.
//! gap: event-controlled treasures (their part has an entity id the map's event script switches
//! on, such as the dialogue rewards and the paired Gourd Seed / Divine Grass), chests
//! (`InChest`), start-disabled treasures, the first-time item dialog and a pickup animation.
//! Goods over the carry limit go to the kit's Storage (`Kit::add`), as each goods' EquipParamGoods
//! row allows ([`GoodsRule`]), and Rest restocks them from it (`Kit::restock`).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

/// ItemLotParam `lotItemCategory` of a goods.
pub const LOT_GOODS: i32 = 0x4000_0000;
/// The goods id of a Spirit Emblem (kept as a count in the kit, not in its goods).
pub const SPIRIT_EMBLEM: i32 = 1000;
/// The goods id of the Healing Gourd (kept as charges in the kit, not in its goods).
pub const HEALING_GOURD: i32 = 3000;
/// ActionButtonParam "Pick up an item", for treasures whose ActionButtonID is -1.
pub const PICK_UP_BUTTON: i32 = 4000;

/// The index `r` (0..1) falls on in `weights`; None when they are all 0.
/// From FwLo's RustSekiro `src/drops.rs` `pick`.
pub fn pick(weights: &[f32], r: f32) -> Option<usize> {
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

/// One of an ItemLotParam row's eight entries.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LotEntry {
    pub category: i32,
    pub id: i32,
    pub count: u32,
    pub points: u32,
    pub luck: bool,
}

/// An ItemLotParam row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemLot {
    pub id: i32,
    /// `getItemFlagId`: set once the lot is taken; 0 or -1 for none.
    pub flag: i32,
    pub entries: [LotEntry; 8],
}

impl ItemLot {
    fn from_row(id: i32, row: &Value) -> Self {
        let field = |k: &str, i: usize| row[format!("{k}0{i}").as_str()].as_i64().unwrap_or(0);
        let mut entries = [LotEntry::default(); 8];
        for (i, e) in entries.iter_mut().enumerate() {
            let k = i + 1;
            *e = LotEntry {
                category: field("lotItemCategory", k) as i32,
                id: field("lotItemId", k) as i32,
                count: field("lotItemNum", k).max(0) as u32,
                points: field("lotItemBasePoint", k).max(0) as u32,
                luck: field("enableLuck", k) == 1,
            };
        }
        Self {
            id,
            flag: row["getItemFlagId"].as_i64().unwrap_or(0) as i32,
            entries,
        }
    }

    /// The draw for `r` (0..1): (goods, count), None for an empty or non-goods entry.
    /// `drop_rate` (itemDropRate) weights up the goods entries with enableLuck.
    /// From FwLo's RustSekiro `src/drops.rs` `roll_item_lot`.
    pub fn roll(&self, drop_rate: f32, r: f32) -> Option<(i32, u32)> {
        let goods = |e: &LotEntry| e.category == LOT_GOODS && e.id > 0;
        let weights: Vec<f32> = self
            .entries
            .iter()
            .map(|e| {
                let base = e.points as f32;
                if goods(e) && e.luck {
                    base * (1.0 + drop_rate)
                } else {
                    base
                }
            })
            .collect();
        let e = &self.entries[pick(&weights, r)?];
        goods(e).then(|| (e.id, e.count.max(1)))
    }
}

/// An ActionButtonParam cylinder (regionType 0) about the thing it acts on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    pub radius: f32,
    /// Its height, from `base` above the thing's origin (negative: below it).
    pub height: f32,
    pub base: f32,
}

impl Reach {
    /// The cylinder of an ActionButtonParam row (`radius`, `height`, `baseHeightOffset`).
    pub fn from_button(row: &Value) -> Option<Self> {
        let f = |k: &str| row[k].as_f64().map(|v| v as f32);
        Some(Self {
            radius: f("radius")?,
            height: f("height")?,
            base: f("baseHeightOffset")?,
        })
    }

    /// The horizontal distance from `at` to the feet when a body `tall` metres high from them
    /// overlaps the cylinder (0 for the feet alone).
    pub fn distance(&self, at: [f32; 3], feet: [f32; 3], tall: f32) -> Option<f32> {
        let d = (feet[0] - at[0]).hypot(feet[2] - at[2]);
        let bottom = at[1] + self.base;
        let top = bottom + self.height;
        (d <= self.radius && feet[1] <= top && feet[1] + tall >= bottom).then_some(d)
    }
}

/// What an EquipParamGoods row says about carrying a goods and keeping it in the Storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoodsRule {
    /// `maxNum`: how many Wolf carries.
    pub carry: u32,
    /// `maxRepositoryNum` when `isDeposit` is 1, else 0: how many the Storage keeps (the
    /// Healing Gourds, the Homeward Idol and most key items have none, so their extra is lost).
    pub store: u32,
    /// `autoReplenishType` set: Rest and a respawn top the carried count up from the Storage
    /// ([`crate::kit::Kit::restock`]). The field is 0 or 2; 2 is on the consumables, materials
    /// and the Spirit Emblem, on 44 key items (the Memories 5200-5322, the Sakes 9100-9105, the
    /// carp feeds 9180-9182 and the Treasure Carp Scales 10000) and on 21 isDeposit-0 rows such
    /// as the Healing Gourd 3000. Only isDeposit-1 rows ever reach the Storage, so those 21
    /// never restock from it.
    pub restock: bool,
    /// `isFullSuppleItem` 1: Rest and a respawn refill the carried count to `carry` from
    /// nothing, no Storage involved ([`crate::kit::Kit::restock`], #208). The six such rows are
    /// the "[Replenishment]" goods, all isDeposit 0: the Healing Gourd 3000 (whose charges
    /// follow `gourd_max` instead), the prevention goods 3300, 3310 and 3320, Five-color Rice
    /// 3580 and goods 3800. Each has an empty twin at the next id (isSuppleItem 1).
    pub refill: bool,
}

impl GoodsRule {
    /// The rule of a goods with no EquipParamGoods row (or no params loaded): carried up to 99
    /// as before, but neither stored nor restocked, so nothing appears from nowhere.
    pub const UNKNOWN: Self = Self {
        carry: 99,
        store: 0,
        restock: false,
        refill: false,
    };

    /// A stored, restocking consumable with this carry limit: a 9999 Storage (the
    /// `maxRepositoryNum` of most consumables), topped up from it on Rest.
    pub const fn consumable(carry: u32) -> Self {
        Self {
            carry,
            store: 9999,
            restock: true,
            refill: false,
        }
    }

    fn from_row(row: &Value) -> Self {
        let int = |k: &str| row[k].as_i64().unwrap_or(0);
        Self {
            carry: row["maxNum"].as_u64().unwrap_or(99) as u32,
            store: if int("isDeposit") == 1 {
                int("maxRepositoryNum").max(0) as u32
            } else {
                0
            },
            restock: int("autoReplenishType") != 0,
            refill: int("isFullSuppleItem") == 1,
        }
    }
}

/// The params the pickups read, once.
#[derive(Debug, Clone, Default)]
pub struct ItemData {
    pub lots: BTreeMap<i32, ItemLot>,
    /// Each EquipParamGoods row's carry and Storage rule.
    pub goods: BTreeMap<i32, GoodsRule>,
    pub buttons: BTreeMap<i32, Reach>,
}

impl ItemData {
    pub fn load(cache: &Path) -> Result<Self, String> {
        let rows = |t| crate::registry::rows(cache, t);
        let lots = rows("ItemLotParam")?
            .iter()
            .map(|(id, r)| (*id, ItemLot::from_row(*id, r)))
            .collect();
        let goods = rows("EquipParamGoods")?
            .iter()
            .map(|(id, r)| (*id, GoodsRule::from_row(r)))
            .collect();
        let buttons = rows("ActionButtonParam")?
            .iter()
            .filter_map(|(id, r)| Some((*id, Reach::from_button(r)?)))
            .collect();
        Ok(Self {
            lots,
            goods,
            buttons,
        })
    }

    /// The carry and Storage rule of `goods` ([`GoodsRule::UNKNOWN`] for a missing row, or when
    /// the params failed to load and this is the empty default).
    pub fn rule(&self, goods: i32) -> GoodsRule {
        self.goods
            .get(&goods)
            .copied()
            .unwrap_or(GoodsRule::UNKNOWN)
    }
}

/// One item lying on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPickup {
    /// The Treasure event's name (`Item_宝_02`).
    pub name: String,
    /// The block and the name of the part it lies at (`o000100_0002`, the item's marker).
    pub block: String,
    pub part: String,
    /// Where it lies, in play space (the layout's: FromSoftware x mirrored).
    pub position: [f32; 3],
    pub lot: ItemLot,
    pub reach: Reach,
}

/// The items a block's MSB (`cache/raw/map/mapstudio/<block>.msb`) puts on the map: its
/// Treasure events with a lot and a part, less the start-disabled, chest and
/// event-controlled ones.
pub fn map_pickups(cache: &Path, block: &str, data: &ItemData) -> Result<Vec<MapPickup>, String> {
    let path = cache.join(format!("raw/map/mapstudio/{block}.msb"));
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let msb = sekiro_formats::msb::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(pickups_of(&msb, block, data))
}

fn pickups_of(msb: &sekiro_formats::msb::Msb, block: &str, data: &ItemData) -> Vec<MapPickup> {
    let fallback = Reach {
        radius: 1.5,
        height: 2.5,
        base: -1.0,
    };
    msb.treasures
        .iter()
        .filter(|t| !t.start_disabled && !t.in_chest)
        .filter_map(|t| {
            let part = msb.part_by_index(t.part_index)?;
            let lot = data.lots.get(&t.item_lot)?;
            if part.entity_id > 0 {
                return None;
            }
            let button = if t.action_button < 0 {
                PICK_UP_BUTTON
            } else {
                t.action_button
            };
            let [x, y, z] = part.position;
            Some(MapPickup {
                name: t.name.clone(),
                block: block.to_string(),
                part: part.name.clone(),
                position: [-x, y, z],
                lot: lot.clone(),
                reach: data.buttons.get(&button).copied().unwrap_or(fallback),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_follows_the_weights_and_skips_empty_entries() {
        let mut lot = ItemLot::default();
        lot.entries[0] = LotEntry {
            category: LOT_GOODS,
            id: 3500,
            count: 1,
            points: 150,
            luck: true,
        };
        lot.entries[1].points = 850;
        assert_eq!(lot.roll(0.0, 0.1), Some((3500, 1)));
        assert_eq!(lot.roll(0.0, 0.2), None);
        // The luck rate doubles the shard's weight: 300 of 1150.
        assert_eq!(lot.roll(1.0, 0.25), Some((3500, 1)));
        assert_eq!(pick(&[0.0, 0.0], 0.5), None);
    }
}
