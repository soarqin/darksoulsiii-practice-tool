//! Item catalog from the loaded PARAMs and current-language game messages.

mod discovery;
mod items;
mod layout;
mod memory;
mod messages;

pub use items::{Item, ItemCategory, ItemVariant};
use memory::ParamTables;
use messages::{Messages, NameKind};

/// Reads the loaded PARAMs and localized text; returns `None` while
/// unavailable.
pub fn items(param_address: usize) -> Option<Vec<Item>> {
    items::read(
        &ParamTables::read(param_address, &[
            "EquipParamWeapon",
            "EquipParamProtector",
            "EquipParamAccessory",
            "EquipParamGoods",
            "ReinforceParamWeapon",
            "Magic",
        ])?,
        &Messages::read(&[
            NameKind::Goods,
            NameKind::Weapon,
            NameKind::Protector,
            NameKind::Accessory,
        ])?,
    )
}
