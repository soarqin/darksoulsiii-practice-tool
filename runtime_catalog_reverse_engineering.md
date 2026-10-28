# Runtime item catalog and font fallback

The item spawner reads the game's loaded PARAMs and current-language FMGs.
The game's own travel and attunement menus keep their existing behavior.

## PARAMs

The reader follows the same DS3 resource chain as the existing parameter editor:

```text
[BaseAddresses.param] + 0x10 -> resource pointer array start / end
resource + 0x68 -> header + 0x68 -> PARAM
```

PARAM row count is at `+0x0A`; the index starts at `+0x40`, with `0x18` bytes per
entry. Resource names use the string at `+0x10`; its capacity determines whether
the UTF-16 buffer is inline. Reads use `ReadProcessMemory`, checked offsets and
bounded lengths, so unavailable game data returns a waiting state.

Only the required packed row prefixes are read. Offsets in
[`layout.rs`](lib/libds3/src/catalog/layout.rs) were checked against DS3 Paramdex
version 201 and the installed game's regulation data. The catalog uses the four
equipment tables, `ReinforceParamWeapon` and `Magic`.

Infusions come from weapon rows that actually exist. Reinforcement limits come
from each weapon's contiguous reinforcement path. DS3 ammunition uses weapon
categories `13` and `14`, with separate item IDs for each arrow or bolt type;
bow ammunition-slot flags cannot identify ammunition. Spell categories come
from `Magic.ezStateBehaviorType`. Inventory category bits are added only after
looking up the corresponding PARAM row and name.

## Current-language messages

DS3's existing address table has no MsgRepository entry. A unique signature in
the executable's `.text` section resolves its RIP-relative global pointer once;
ambiguous matches or out-of-image targets are rejected. The signature comes
from the DS3 TGA cheat table and was checked in both installed 1.15.2 executables.
Other game versions have not been checked.

On 1.15.2 the MsgRepository global RVA is `0x47972A0`. The text accessor at
`0x142354090` reads `[repository + 8] -> language[0] -> FMG[bundle]`. Language and
bundle counts are at `+0x10` and `+0x14`. The FMG lookup at `0x142354980` reads
groups at `+0x28` and the relocated absolute offset-table pointer at `+0x18`;
individual UTF-16 string offsets remain relative to the FMG.

The name readers follow the game's patch, base, DLC1 and DLC2 lookup order:

| Names | Bundle IDs |
| --- | --- |
| Goods | `111`, `10`, `210`, `250` |
| Weapons | `115`, `11`, `211`, `251` |
| Armor | `117`, `12`, `212`, `252` |
| Rings | `113`, `13`, `213`, `253` |

A present empty string overrides later bundles, matching the game. Empty names
are omitted from the catalog. Refresh preserves an explicit selection by item
ID and infusion value; inserting infusion rows cannot silently change the
selected infusion. Removing the selected infusion resets infusion and upgrade
options. Clearing or filtering out the item leaves no selection and disables spawning.

Missing base message bundles or unreadable PARAM rows keep the catalog in its
waiting state. Invalid overlapping FMG groups are rejected before parsing their
strings, so corrupt group metadata cannot trigger repeated unbounded work.

## Fonts

The font implementation follows the ER branch's on-demand fallback. ComicMono
retains priority and its original 11, 18 and 24 pixel rendering. Missing visible
characters request fonts in Windows `Segoe UI` FontLink order. Font files are
mapped only when needed, and ImGui rasterizes just the requested glyphs.

Pending names and search input display a loading placeholder until their glyphs
have been uploaded. DX11 texture replacement requires unchanged dimensions, so
atlas growth doubles dimensions up to a 4096 pixel side limit, bounding the
total retained texture size. Temporary upload failures retry without permanently
discarding characters. Font
coverage depends on installed linked fonts and currently covers BMP characters.

## Verification

Workspace tests cover guarded PARAM and FMG reads, message precedence, ammunition
and reinforcement paths, localized filtering, empty selections, and the DS3
spawn callback's arguments. Font tests compare glyph pixels and metrics at all
three sizes, exercise atlas growth and upload failures, and check input/paste
and cancellation behavior. Regression tests also cover repeated failed uploads,
partial game-data loading, invalid FMG groups and infusion changes on refresh.
The manual checks are listed in [TESTS.md](TESTS.md).

These checks do not launch the game, spawn actual items or verify GPU rendering
inside the game.

An additional offline check loaded the installed game's six PARAM tables and
actual English, Simplified Chinese and Traditional Chinese FMGs into a simulated
resource layout, then called the production readers. It produced 1,280 English
items and 1,199 items for each Chinese locale, including 129 DLC items in each
case. The difference is 81 legacy armor entries that retain English names but
have no Chinese names. These counts describe this particular data set.

The same check verified 16 dagger infusion choices, ordinary `+10` and special
`+5` reinforcement limits, bows remaining weapons, independent ammunition with
99-arrow and 50-greatarrow limits, all three spell categories, and rejection of
reinforcement for non-weapons. Extracted game data and the temporary validation
harness remain under the ignored `target/catalog-validation` directory.
