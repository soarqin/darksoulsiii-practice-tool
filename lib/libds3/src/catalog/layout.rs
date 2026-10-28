// Packed DS3 PARAM offsets, checked against Paramdex's version 201 definitions.
// Read only the prefix needed here; editable repr(C) structs are not the wire
// layout.

pub(super) mod weapon {
    pub const SIZE: usize = 0x232;
    pub const SORT: usize = 0x04;
    pub const REINFORCE: usize = 0xd6;
    pub const TYPE: usize = 0xe2;
    pub const MAX_QUANTITY: usize = 0x231;
}

pub(super) mod protector {
    pub const SIZE: usize = 0xd3;
    pub const SORT: usize = 0x00;
    pub const TYPE: usize = 0xd2;
}

pub(super) mod accessory {
    pub const SIZE: usize = 0x1c;
    pub const SORT: usize = 0x18;
}

pub(super) mod goods {
    pub const SIZE: usize = 0x45;
    pub const SORT: usize = 0x1c;
    pub const MAX_QUANTITY: usize = 0x36;
    pub const TYPE: usize = 0x3a;
    pub const FLAGS: usize = 0x44;
    pub const CONSUME_MASK: u8 = 0x80;
    pub const DISABLE_OFFLINE_MASK: u8 = 0x20;
}

pub(super) mod magic {
    pub const SIZE: usize = 0x23;
    pub const TYPE: usize = 0x22;
}
