use std::collections::{BTreeMap, BTreeSet};

use super::layout::{accessory, goods, magic, protector, weapon};
use super::memory::{at, ParamTables};
use super::messages::{Messages, NameKind};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ItemCategory {
    Weapons,
    Ammunition,
    Head,
    Chest,
    Arms,
    Legs,
    Rings,
    Sorceries,
    Pyromancies,
    Miracles,
    Consumables,
    Materials,
    Miscellaneous,
}

impl ItemCategory {
    pub const ALL: [Self; 13] = [
        Self::Weapons,
        Self::Ammunition,
        Self::Head,
        Self::Chest,
        Self::Arms,
        Self::Legs,
        Self::Rings,
        Self::Sorceries,
        Self::Pyromancies,
        Self::Miracles,
        Self::Consumables,
        Self::Materials,
        Self::Miscellaneous,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Weapons => "武器",
            Self::Ammunition => "箭矢/弩箭",
            Self::Head => "防具: 头部",
            Self::Chest => "防具: 身体",
            Self::Arms => "防具: 手部",
            Self::Legs => "防具: 腿部",
            Self::Rings => "戒指/誓约",
            Self::Sorceries => "魔法",
            Self::Pyromancies => "咒术",
            Self::Miracles => "奇迹",
            Self::Consumables => "消耗品",
            Self::Materials => "材料",
            Self::Miscellaneous => "杂项/关键物品",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemVariant {
    pub affinity: u32,
    pub max_upgrade: u32,
}

#[derive(Debug)]
pub struct Item {
    /// Includes the game's inventory category bits.
    pub id: u32,
    pub name: String,
    pub category: ItemCategory,
    pub dlc: bool,
    pub max_quantity: u32,
    pub variants: Vec<ItemVariant>,
}

impl Item {
    pub fn spawn_id(&self, variant: usize, upgrade: u32) -> Option<u32> {
        let variant = self.variants.get(variant)?;
        if upgrade > variant.max_upgrade {
            return None;
        }
        self.id.checked_add(variant.affinity)?.checked_add(upgrade)
    }
}

fn is_ammunition(row: &[u8]) -> Option<bool> {
    Some(matches!(at::<u8>(row, weapon::TYPE)?, 13 | 14))
}

fn weapon_variants(
    weapons: &BTreeMap<u32, Vec<u8>>,
    reinforcements: &BTreeSet<u32>,
) -> Option<BTreeMap<u32, Vec<ItemVariant>>> {
    let mut variants = BTreeMap::<u32, Vec<ItemVariant>>::new();
    for (&id, row) in weapons {
        if !id.is_multiple_of(100) {
            continue;
        }
        // Arrow and bolt IDs are separate items, not infusions of the first
        // arrow.
        let ammunition = is_ammunition(row)?;
        let base = if ammunition { id } else { id - id % 10000 };
        let path: i16 = at(row, weapon::REINFORCE)?;
        let max_upgrade = if ammunition || path < 0 {
            0
        } else {
            // DS3 stores +0 weapon rows; upgrade limits come from their
            // contiguous ReinforceParamWeapon path, including special +5 paths.
            (0..100)
                .take_while(|level| reinforcements.contains(&(path as u32 + level)))
                .last()
                .unwrap_or(0)
        };
        variants.entry(base).or_default().push(ItemVariant { affinity: id - base, max_upgrade });
    }
    Some(variants)
}

fn goods_category(kind: u8, flags: u8, magic_type: Option<u8>) -> ItemCategory {
    match magic_type {
        Some(0) => ItemCategory::Sorceries,
        Some(1) => ItemCategory::Miracles,
        Some(2) => ItemCategory::Pyromancies,
        _ => match kind {
            2 => ItemCategory::Materials,
            0 if flags & goods::CONSUME_MASK != 0 && flags & goods::DISABLE_OFFLINE_MASK == 0 => {
                ItemCategory::Consumables
            },
            _ => ItemCategory::Miscellaneous,
        },
    }
}

pub(super) fn read(params: &ParamTables, messages: &Messages) -> Option<Vec<Item>> {
    let weapons: BTreeMap<_, _> =
        params.get("EquipParamWeapon")?.rows(weapon::SIZE).collect::<Option<_>>()?;
    let reinforcements = params.get("ReinforceParamWeapon")?.ids().collect();
    let mut variants = weapon_variants(&weapons, &reinforcements)?;
    let mut items = Vec::new();
    for (&id, row) in &weapons {
        let ammunition = is_ammunition(row)?;
        if !(if ammunition { id.is_multiple_of(100) } else { id.is_multiple_of(10000) }) {
            continue;
        }
        let Some((name, dlc)) = messages.name(NameKind::Weapon, id).filter(|(n, _)| !n.is_empty())
        else {
            continue;
        };
        let Some(variants) = variants.remove(&id) else {
            continue;
        };
        let category = if ammunition { ItemCategory::Ammunition } else { ItemCategory::Weapons };
        items.push((at::<i32>(row, weapon::SORT)?, Item {
            id,
            name,
            category,
            dlc,
            max_quantity: if ammunition {
                u32::from(at::<u8>(row, weapon::MAX_QUANTITY)?).max(1)
            } else {
                99
            },
            variants,
        }));
    }

    for row in params.get("EquipParamProtector")?.rows(protector::SIZE) {
        let (id, row) = row?;
        let category = match at::<u8>(&row, protector::TYPE)? {
            0 => ItemCategory::Head,
            1 => ItemCategory::Chest,
            2 => ItemCategory::Arms,
            3 => ItemCategory::Legs,
            _ => continue,
        };
        let Some((name, dlc)) =
            messages.name(NameKind::Protector, id).filter(|(n, _)| !n.is_empty())
        else {
            continue;
        };
        items.push((at::<i32>(&row, protector::SORT)?, Item {
            id: id | 0x10000000,
            category,
            name,
            dlc,
            max_quantity: 99,
            variants: vec![ItemVariant { affinity: 0, max_upgrade: 0 }],
        }));
    }

    for row in params.get("EquipParamAccessory")?.rows(accessory::SIZE) {
        let (id, row) = row?;
        let Some((name, dlc)) =
            messages.name(NameKind::Accessory, id).filter(|(n, _)| !n.is_empty())
        else {
            continue;
        };
        items.push((at::<i32>(&row, accessory::SORT)?, Item {
            id: id | 0x20000000,
            category: ItemCategory::Rings,
            name,
            dlc,
            max_quantity: 99,
            variants: vec![ItemVariant { affinity: 0, max_upgrade: 0 }],
        }));
    }

    let magic_types: BTreeMap<_, _> = params
        .get("Magic")?
        .rows(magic::SIZE)
        .map(|row| {
            let (id, row) = row?;
            Some((id, at::<u8>(&row, magic::TYPE)?))
        })
        .collect::<Option<_>>()?;
    for row in params.get("EquipParamGoods")?.rows(goods::SIZE) {
        let (id, row) = row?;
        let Some((name, dlc)) = messages.name(NameKind::Goods, id).filter(|(n, _)| !n.is_empty())
        else {
            continue;
        };
        let category = goods_category(
            at(&row, goods::TYPE)?,
            at(&row, goods::FLAGS)?,
            magic_types.get(&id).copied(),
        );
        items.push((at::<i32>(&row, goods::SORT)?, Item {
            id: id | 0x40000000,
            category,
            name,
            dlc,
            max_quantity: i32::from(at::<i16>(&row, goods::MAX_QUANTITY)?).clamp(1, 999) as u32,
            variants: vec![ItemVariant { affinity: 0, max_upgrade: 0 }],
        }));
    }
    items.sort_by_key(|(sort, item)| (item.category, *sort, item.id));
    (!items.is_empty()).then(|| items.into_iter().map(|(_, item)| item).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon_row(reinforcement: i16, ammunition: bool) -> Vec<u8> {
        let mut row = vec![0; 0x260];
        row[0xd6..0xd8].copy_from_slice(&reinforcement.to_le_bytes());
        row[0xe2] = if ammunition { 13 } else { 0 };
        row
    }

    #[test]
    fn arrow_rows_are_independent_and_upgrades_follow_their_reinforcement_path() {
        let rows = BTreeMap::from([
            (400000, weapon_row(0, true)),
            (400100, weapon_row(0, true)),
            (1000000, weapon_row(0, false)),
            (1000100, weapon_row(100, false)),
            (2000000, weapon_row(2000, false)),
        ]);
        let levels = (0..=10).chain(100..=110).chain(2000..=2005).collect();
        let variants = weapon_variants(&rows, &levels).unwrap();
        assert_eq!(variants[&400000], [ItemVariant { affinity: 0, max_upgrade: 0 }]);
        assert_eq!(variants[&400100], [ItemVariant { affinity: 0, max_upgrade: 0 }]);
        assert_eq!(variants[&1000000], [
            ItemVariant { affinity: 0, max_upgrade: 10 },
            ItemVariant { affinity: 100, max_upgrade: 10 },
        ]);
        assert_eq!(variants[&2000000], [ItemVariant { affinity: 0, max_upgrade: 5 }]);
    }

    #[test]
    fn bow_ammunition_slot_flags_do_not_make_the_bow_an_arrow() {
        let mut bow = weapon_row(0, false);
        bow[0xe2] = 10;
        bow[0x101] = 0x0f;
        assert_eq!(is_ammunition(&bow), Some(false));
        let mut bolt = weapon_row(3000, true);
        bolt[0xe2] = 14;
        assert_eq!(is_ammunition(&bolt), Some(true));
    }

    #[test]
    fn spells_use_magic_rows_and_consumables_use_packed_flags() {
        assert_eq!(goods_category(5, 0, Some(0)), ItemCategory::Sorceries);
        assert_eq!(goods_category(5, 0, Some(1)), ItemCategory::Miracles);
        assert_eq!(goods_category(5, 0, Some(2)), ItemCategory::Pyromancies);
        assert_eq!(goods_category(0, 0x80, None), ItemCategory::Consumables);
        assert_eq!(goods_category(0, 0xa0, None), ItemCategory::Miscellaneous);
        assert_eq!(goods_category(1, 0x80, None), ItemCategory::Miscellaneous);
        assert_eq!(goods_category(2, 0, None), ItemCategory::Materials);
    }

    #[test]
    fn spawn_ids_reject_unsupported_infusions_upgrades_and_integer_overflow() {
        let mut item = Item {
            id: 1000000,
            name: "长剑".into(),
            category: ItemCategory::Weapons,
            dlc: false,
            max_quantity: 99,
            variants: vec![ItemVariant { affinity: 0, max_upgrade: 5 }],
        };
        assert_eq!(item.spawn_id(0, 5), Some(1000005));
        assert_eq!(item.spawn_id(0, 6), None);
        assert_eq!(item.spawn_id(1, 0), None);
        item.id = u32::MAX;
        assert_eq!(item.spawn_id(0, 1), None);
        item.id = 0x40000100;
        item.variants[0].max_upgrade = 0;
        assert_eq!(item.spawn_id(0, 0), Some(0x40000100));
        assert_eq!(item.spawn_id(0, 1), None);
    }

    #[test]
    fn packed_goods_offsets_match_ds3_paramdex() {
        let mut row = vec![0; 0x80];
        row[0x1c..0x20].copy_from_slice(&123i32.to_le_bytes());
        row[0x36..0x38].copy_from_slice(&99i16.to_le_bytes());
        row[0x3a] = 2;
        row[0x44] = 0x80;
        assert_eq!(at::<i32>(&row, goods::SORT), Some(123));
        assert_eq!(at::<i16>(&row, goods::MAX_QUANTITY), Some(99));
        assert_eq!(at::<u8>(&row, goods::TYPE), Some(2));
        assert_eq!(at::<u8>(&row, goods::FLAGS), Some(0x80));
    }
}
