use std::collections::{BTreeMap, BTreeSet};

use super::discovery::MSG_REPOSITORY;
use super::memory::{at, bytes, value};

#[derive(Clone, Copy)]
pub(super) enum NameKind {
    Goods,
    Weapon,
    Protector,
    Accessory,
}

impl NameKind {
    fn bundles(self) -> &'static [u32; 4] {
        // Match DS3's accessors: patch, base game, Ashes of Ariandel, Ringed
        // City.
        match self {
            Self::Goods => &[111, 10, 210, 250],
            Self::Weapon => &[115, 11, 211, 251],
            Self::Protector => &[117, 12, 212, 252],
            Self::Accessory => &[113, 13, 213, 253],
        }
    }
}

pub(super) struct Messages(BTreeMap<u32, BTreeMap<u32, String>>);

impl Messages {
    pub(super) fn read(kinds: &[NameKind]) -> Option<Self> {
        Self::read_from((*MSG_REPOSITORY)?, kinds)
    }

    pub(super) fn read_from(repository_address: usize, kinds: &[NameKind]) -> Option<Self> {
        let repository: usize = value(repository_address)?;
        let header = bytes(repository, 0x18)?;
        let languages: usize = at(&header, 8)?;
        let language_count: u32 = at(&header, 0x10)?;
        let bundle_count: u32 = at(&header, 0x14)?;
        if language_count == 0 || language_count > 64 || bundle_count == 0 || bundle_count > 4096 {
            return None;
        }
        // DS3's item name accessors explicitly use language slot 0.
        let bundles: usize = value(languages)?;
        let pointers = bytes(bundles, bundle_count as usize * 8)?;
        let wanted: BTreeSet<_> =
            kinds.iter().flat_map(|kind| kind.bundles().iter().copied()).collect();
        let mut tables = BTreeMap::new();
        for id in wanted {
            let Some(address) = at::<usize>(&pointers, id as usize * 8).filter(|p| *p != 0) else {
                continue;
            };
            tables.insert(id, read_fmg(address)?);
        }
        // Patch and DLC bundles are optional, but every requested base bundle
        // must be loaded before caching the catalog as ready.
        (!tables.is_empty() && kinds.iter().all(|kind| tables.contains_key(&kind.bundles()[1])))
            .then_some(Self(tables))
    }

    pub(super) fn name(&self, kind: NameKind, id: u32) -> Option<(String, bool)> {
        kind.bundles()
            .iter()
            .find_map(|bundle| Some((self.0.get(bundle)?.get(&id)?.clone(), *bundle >= 210)))
    }
}

fn read_fmg(address: usize) -> Option<BTreeMap<u32, String>> {
    let header = bytes(address, 0x28)?;
    let size: u32 = at(&header, 4)?;
    if !(0x28..=16 * 1024 * 1024).contains(&size) || header[0] != 0 || header[2] != 2 {
        return None;
    }
    // DS3 relocates the offset-table address; individual UTF-16 offsets stay
    // relative.
    let offsets: usize = at(&header, 0x18)?;
    let offsets = offsets.checked_sub(address)?;
    parse_fmg(&bytes(address, size as usize)?, offsets)
}

fn parse_fmg(data: &[u8], offsets: usize) -> Option<BTreeMap<u32, String>> {
    let group_count: u32 = at(data, 0xc)?;
    let string_count: u32 = at(data, 0x10)?;
    if group_count > string_count || string_count > 1_000_000 {
        return None;
    }
    let groups = data.get(0x28..0x28usize.checked_add(group_count as usize * 0x10)?)?;
    let pointers = data.get(offsets..offsets.checked_add(string_count as usize * 8)?)?;
    let mut result = BTreeMap::new();
    let mut previous_id = None;
    let mut previous_end = 0;
    for group in groups.as_chunks::<0x10>().0 {
        let first_index: u32 = at(group, 0)?;
        let first_id: u32 = at(group, 4)?;
        let last_id: u32 = at(group, 8)?;
        let count = last_id.checked_sub(first_id)?.checked_add(1)?;
        let end = first_index.checked_add(count)?;
        // The game searches sorted, disjoint groups. Overlap can also make a
        // small malformed FMG expand into quadratic parsing work.
        if first_index < previous_end
            || previous_id.is_some_and(|id| first_id <= id)
            || end > string_count
        {
            return None;
        }
        previous_id = Some(last_id);
        previous_end = end;
        for offset in 0..count {
            let string_offset: u64 = at(pointers, (first_index + offset) as usize * 8)?;
            if string_offset == 0 {
                continue;
            }
            let remaining = data.get(usize::try_from(string_offset).ok()?..)?;
            let mut units = Vec::new();
            let mut terminated = false;
            for unit in remaining.as_chunks::<2>().0.iter().take(8192) {
                let unit = u16::from_le_bytes([unit[0], unit[1]]);
                if unit == 0 {
                    terminated = true;
                    break;
                }
                units.push(unit);
            }
            if !terminated {
                return None;
            }
            result.insert(first_id + offset, String::from_utf16_lossy(&units));
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_names_follow_ds3_patch_and_dlc_priority() {
        let messages = Messages(BTreeMap::from([
            (11, BTreeMap::from([(1000000, "Sword".into()), (1001000, "Black Knife".into())])),
            (115, BTreeMap::from([(1000000, "长剑".into()), (1001000, String::new())])),
            (211, BTreeMap::from([(2000000, "夜の剣".into())])),
            (251, BTreeMap::from([(3000000, "Клинок".into())])),
        ]));
        assert_eq!(messages.name(NameKind::Weapon, 1000000), Some(("长剑".into(), false)));
        assert_eq!(messages.name(NameKind::Weapon, 2000000), Some(("夜の剣".into(), true)));
        assert_eq!(messages.name(NameKind::Weapon, 3000000), Some(("Клинок".into(), true)));
        // A present blank string also overrides the base entry in the game.
        assert_eq!(messages.name(NameKind::Weapon, 1001000), Some((String::new(), false)));
        assert_eq!(messages.name(NameKind::Weapon, 0), None);
    }

    #[test]
    fn loaded_fmg_uses_absolute_offset_table_and_relative_utf16_strings() {
        let mut fmg = [0u8; 0x54];
        fmg[2] = 2;
        fmg[4..8].copy_from_slice(&0x54u32.to_le_bytes());
        fmg[0xc..0x10].copy_from_slice(&1u32.to_le_bytes());
        fmg[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
        let offsets = fmg.as_ptr() as usize + 0x38;
        fmg[0x18..0x20].copy_from_slice(&offsets.to_le_bytes());
        fmg[0x2c..0x30].copy_from_slice(&42u32.to_le_bytes());
        fmg[0x30..0x34].copy_from_slice(&42u32.to_le_bytes());
        fmg[0x38..0x40].copy_from_slice(&0x40u64.to_le_bytes());
        let text: Vec<_> = "篝火 🔥".encode_utf16().flat_map(u16::to_le_bytes).collect();
        fmg[0x40..0x40 + text.len()].copy_from_slice(&text);
        assert_eq!(read_fmg(fmg.as_ptr() as usize).unwrap().get(&42).unwrap(), "篝火 🔥");
        fmg[0x30..0x34].copy_from_slice(&100u32.to_le_bytes());
        assert!(read_fmg(fmg.as_ptr() as usize).is_none());
    }

    #[test]
    fn fmg_groups_reject_overlapping_ids_and_reused_string_indices() {
        let mut fmg = [0u8; 0x60];
        fmg[0xc..0x10].copy_from_slice(&2u32.to_le_bytes());
        fmg[0x10..0x14].copy_from_slice(&3u32.to_le_bytes());
        fmg[0x2c..0x30].copy_from_slice(&10u32.to_le_bytes());
        fmg[0x30..0x34].copy_from_slice(&11u32.to_le_bytes());
        fmg[0x38..0x3c].copy_from_slice(&2u32.to_le_bytes());
        fmg[0x3c..0x40].copy_from_slice(&20u32.to_le_bytes());
        fmg[0x40..0x44].copy_from_slice(&20u32.to_le_bytes());
        assert!(parse_fmg(&fmg, 0x48).is_some());
        fmg[0x38..0x3c].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_fmg(&fmg, 0x48).is_none());
        fmg[0x38..0x3c].copy_from_slice(&2u32.to_le_bytes());
        fmg[0x3c..0x40].copy_from_slice(&11u32.to_le_bytes());
        fmg[0x40..0x44].copy_from_slice(&11u32.to_le_bytes());
        assert!(parse_fmg(&fmg, 0x48).is_none());
    }

    #[test]
    fn message_repository_waits_for_every_requested_base_bundle() {
        let mut fmg = [0u8; 0x28];
        fmg[2] = 2;
        fmg[4..8].copy_from_slice(&0x28u32.to_le_bytes());
        let offsets = fmg.as_ptr() as usize + fmg.len();
        fmg[0x18..0x20].copy_from_slice(&offsets.to_le_bytes());
        let mut bundles = [0usize; 116];
        bundles[11] = fmg.as_ptr() as usize;
        let languages = [bundles.as_ptr() as usize];
        let mut repository = [0u8; 0x18];
        repository[8..16].copy_from_slice(&(languages.as_ptr() as usize).to_le_bytes());
        repository[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
        repository[0x14..0x18].copy_from_slice(&(bundles.len() as u32).to_le_bytes());
        let pointer = repository.as_ptr() as usize;
        let address = &pointer as *const _ as usize;
        let kinds = [NameKind::Weapon, NameKind::Goods];
        assert!(Messages::read_from(address, &kinds).is_none());
        bundles[10] = fmg.as_ptr() as usize;
        assert!(Messages::read_from(address, &kinds).is_some());
        assert_ne!(bundles[10], 0);
        bundles[115] = 1;
        assert!(Messages::read_from(address, &kinds).is_none());
        bundles[115] = fmg.as_ptr() as usize;
        assert!(Messages::read_from(address, &kinds).is_some());
        assert_ne!(bundles[115], 0);
    }
}
