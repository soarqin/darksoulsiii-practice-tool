use std::ffi::c_void;
use std::time::{Duration, Instant};

use imgui::sys::{igGetCursorPosX, igGetCursorPosY, igGetWindowPos, igSetNextWindowPos, ImVec2};
use imgui::{Condition, TreeNodeFlags, WindowFlags};
use libds3::prelude::*;
use practice_tool_core::crossbeam_channel::Sender;
use practice_tool_core::key::Key;
use practice_tool_core::widgets::{scaling_factor, Widget, BUTTON_HEIGHT, BUTTON_WIDTH};

use super::font_text::{display_text, filter_input};
use crate::fonts::request_text;

static AFFINITIES: [(u32, &str); 16] = [
    (0, "Normal"),
    (100, "Heavy"),
    (200, "Sharp"),
    (300, "Refined"),
    (400, "Simple"),
    (500, "Crystal"),
    (600, "Fire"),
    (700, "Chaos"),
    (800, "Lightning"),
    (900, "Deep"),
    (1000, "Dark"),
    (1100, "Poison"),
    (1200, "Blood"),
    (1300, "Raw"),
    (1400, "Blessed"),
    (1500, "Hollow"),
];

fn string_match(needle: &str, haystack: &str) -> bool {
    let needle = needle.chars().flat_map(char::to_lowercase);
    let mut haystack = haystack.chars().flat_map(char::to_lowercase);

    'o: for c in needle {
        for d in &mut haystack {
            if c == d {
                continue 'o;
            }
        }
        return false;
    }
    true
}

const ISP_TAG: &str = "##item-spawn";

#[derive(Debug)]
pub(crate) struct ItemSpawner {
    func_ptr: usize,
    map_item_man: usize,
    param: usize,
    hotkey_load: Option<Key>,
    hotkey_close: Key,
    sentinel: &'static Bitflag<u8>,
    label_load: String,
    label_close: String,
    qty: u32,
    durability: u32,
    current: Option<usize>,
    upgrade: u32,
    variant: usize,
    filter_string: String,
    source_filter: usize,
    logs: Vec<String>,
    items: Vec<Item>,
    filtered: Vec<usize>,
    last_read: Option<Instant>,
}

impl ItemSpawner {
    pub(crate) fn new(
        func_ptr: usize,
        map_item_man: usize,
        param: usize,
        sentinel: &'static Bitflag<u8>,
        hotkey_load: Option<Key>,
        hotkey_close: Key,
    ) -> Self {
        Self {
            func_ptr,
            map_item_man,
            param,
            hotkey_load,
            hotkey_close,
            sentinel,
            label_load: hotkey_load
                .map(|k| format!("Spawn item ({k})"))
                .unwrap_or_else(|| "Spawn item".into()),
            label_close: format!("Close ({hotkey_close})"),
            qty: 1,
            durability: 100,
            current: None,
            upgrade: 0,
            variant: 0,
            filter_string: String::new(),
            source_filter: 0,
            logs: Vec::new(),
            items: Vec::new(),
            filtered: Vec::new(),
            last_read: None,
        }
    }

    fn refresh(&mut self) {
        self.set_items(libds3::catalog::items(self.param).unwrap_or_default());
        self.last_read = Some(Instant::now());
    }

    fn set_items(&mut self, items: Vec<Item>) {
        let previous = self
            .current
            .and_then(|i| self.items.get(i))
            .map(|item| (item.id, item.variants.get(self.variant).map(|variant| variant.affinity)));
        self.items = items;
        self.current =
            previous.and_then(|(id, _)| self.items.iter().position(|item| item.id == id));
        let affinity = previous.and_then(|(_, affinity)| affinity);
        let variant = self.current.and_then(|i| {
            self.items[i].variants.iter().position(|variant| Some(variant.affinity) == affinity)
        });
        self.variant = variant.unwrap_or(0);
        if variant.is_none() {
            self.upgrade = 0;
        }
        self.filter();
        self.clamp_options();
    }

    fn filter(&mut self) {
        let filter = self.filter_string.trim();
        let decimal = filter.parse::<u32>().ok();
        let hexadecimal = filter
            .strip_prefix("0x")
            .or_else(|| filter.strip_prefix("0X"))
            .or_else(|| (filter.len() == 8).then_some(filter))
            .and_then(|id| u32::from_str_radix(id, 16).ok());
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                (self.source_filter == 0 || item.dlc == (self.source_filter == 2))
                    && (string_match(filter, &item.name)
                        || [decimal, hexadecimal]
                            .into_iter()
                            .flatten()
                            .any(|id| id == item.id || id == item.id & 0x0fff_ffff))
            })
            .map(|(index, _)| index)
            .collect();
        if self.current.is_some_and(|index| !self.filtered.contains(&index)) {
            self.select(None);
        }
    }

    fn select(&mut self, index: Option<usize>) {
        if self.current != index {
            self.current = index;
            self.variant = 0;
            self.upgrade = 0;
            self.clamp_options();
        }
    }

    fn clear(&mut self) {
        self.filter_string.clear();
        self.source_filter = 0;
        self.qty = 1;
        self.durability = 100;
        self.current = None;
        self.variant = 0;
        self.upgrade = 0;
        self.filter();
    }

    fn clamp_options(&mut self) {
        if let Some(item) = self.current.and_then(|index| self.items.get(index)) {
            self.variant = self.variant.min(item.variants.len().saturating_sub(1));
            self.upgrade = self
                .upgrade
                .min(item.variants.get(self.variant).map(|v| v.max_upgrade).unwrap_or(0));
            self.qty = self.qty.clamp(1, item.max_quantity);
        } else {
            self.variant = 0;
            self.upgrade = 0;
        }
    }

    fn render_item(&self, ui: &imgui::Ui, index: usize, selection: &mut Option<usize>) {
        let item = &self.items[index];
        let _id = ui.push_id_int(item.id as i32);
        let label = format!("{}###row", display_text(ui, &item.name));
        if ui.selectable_config(&label).selected(self.current == Some(index)).build() {
            *selection = Some(index);
        }
        if ui.is_item_visible() {
            request_text(ui, &item.name);
        }
    }

    fn spawn(&mut self) {
        if self.sentinel.get().is_none() {
            self.logs.push("Not spawning item when not in game".into());
            return;
        }
        if self.items.is_empty() {
            self.refresh();
        }
        self.clamp_options();
        let Some(item) = self.current.and_then(|index| self.items.get(index)) else {
            self.logs.push("No item selected".into());
            return;
        };
        let Some(item_id) = item.spawn_id(self.variant, self.upgrade) else {
            self.logs.push("Invalid upgrade or affinity".into());
            return;
        };
        let Some(manager) =
            PointerChain::<usize>::new(&[self.map_item_man]).read().filter(|p| *p != 0)
        else {
            self.logs.push("Item manager is not loaded".into());
            return;
        };
        self.logs.push(format!(
            "Spawning {} x {} +{} (#{:08X})",
            self.qty, item.name, self.upgrade, item_id
        ));
        unsafe {
            spawn_item(self.func_ptr, manager, item_id, self.qty, self.durability);
        }
    }
}

impl Widget for ItemSpawner {
    fn render(&mut self, ui: &imgui::Ui) {
        let scale = scaling_factor(ui);
        let button_height = BUTTON_HEIGHT;
        let (x, y) = unsafe {
            let mut wnd_pos = ImVec2::default();
            igGetWindowPos(&mut wnd_pos);
            (igGetCursorPosX() + wnd_pos.x, igGetCursorPosY() + wnd_pos.y)
        };
        if ui.button_with_size(&self.label_load, [BUTTON_WIDTH * scale, button_height]) {
            self.refresh();
            ui.open_popup(ISP_TAG);
        }
        unsafe {
            igSetNextWindowPos(
                ImVec2::new(x + 200. * scale, y),
                Condition::Always as i8 as _,
                ImVec2::new(0., 0.),
            )
        };
        if let Some(_token) = ui
            .modal_popup_config(ISP_TAG)
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::ALWAYS_AUTO_RESIZE,
            )
            .begin_popup()
        {
            let button_height = button_height * scale;
            let _width = ui.push_item_width(400.);
            if self.items.is_empty()
                && self.last_read.is_none_or(|time| time.elapsed() >= Duration::from_secs(1))
            {
                self.refresh();
            }
            let mut changed = filter_input(
                ui,
                "##item-spawn-filter",
                "Filter name or ID...",
                &mut self.filter_string,
            );
            changed |= ui.combo_simple_string("##item-source", &mut self.source_filter, &[
                "All items",
                "Base game",
                "DLC",
            ]);
            if ui.button("Refresh") {
                self.refresh();
            }
            if changed {
                self.filter();
            }
            let mut selection = self.current;
            ui.child_window("##item-spawn-list").size([400., 220.]).build(|| {
                for category in ItemCategory::ALL {
                    if !self.filtered.iter().any(|&index| self.items[index].category == category) {
                        continue;
                    }
                    let node = ui.tree_node_config(category.label());
                    let node = if self.filter_string.is_empty() {
                        node
                    } else {
                        node.opened(true, Condition::Always)
                    };
                    node.flags(TreeNodeFlags::SPAN_AVAIL_WIDTH).build(|| {
                        let indices = self
                            .filtered
                            .iter()
                            .copied()
                            .filter(|&index| self.items[index].category == category);
                        for index in indices {
                            self.render_item(ui, index, &mut selection);
                        }
                    });
                }
                if self.items.is_empty() {
                    ui.text("Waiting for game data...");
                } else if self.filtered.is_empty() {
                    ui.text("No matching items");
                }
            });
            self.select(selection);
            let item = self.current.and_then(|index| self.items.get(index));
            let labels: Vec<String> = item
                .map(|item| {
                    item.variants
                        .iter()
                        .map(|v| {
                            AFFINITIES
                                .iter()
                                .find(|(offset, _)| *offset == v.affinity)
                                .map(|(_, name)| (*name).to_owned())
                                .unwrap_or_else(|| format!("Affinity {}", v.affinity))
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![AFFINITIES[0].1.into()]);
            {
                let _disabled = ui.begin_disabled(labels.len() <= 1);
                ui.set_next_item_width(195.);
                if ui.combo_simple_string("##item-spawn-affinity", &mut self.variant, &labels) {
                    self.upgrade = self.upgrade.min(
                        item.and_then(|item| item.variants.get(self.variant))
                            .map(|variant| variant.max_upgrade)
                            .unwrap_or(0),
                    );
                }
            }
            ui.same_line();
            let max_upgrade = item
                .and_then(|item| item.variants.get(self.variant))
                .map(|variant| variant.max_upgrade)
                .unwrap_or(0);
            {
                let _disabled = ui.begin_disabled(max_upgrade == 0);
                let upgrades: Vec<_> = (0..=max_upgrade).map(|level| format!("+{level}")).collect();
                let mut upgrade = self.upgrade as usize;
                ui.set_next_item_width(195.);
                if ui.combo_simple_string("##item-spawn-upgrade", &mut upgrade, &upgrades) {
                    self.upgrade = upgrade as u32;
                }
            }
            {
                let _disabled = ui.begin_disabled(item.is_none());
                let _width = ui.push_item_width(0.);
                ui.slider_config("Qty", 1, item.map(|item| item.max_quantity).unwrap_or(99))
                    .build(&mut self.qty);
                ui.slider_config("Dur", 0, 9999).build(&mut self.durability);
            }
            {
                let _disabled =
                    ui.begin_disabled(self.current.is_none() || self.sentinel.get().is_none());
                if ui.button_with_size(&self.label_load, [400., button_height]) {
                    self.spawn();
                }
            }
            if ui.button_with_size("Clear", [400., button_height]) {
                self.clear();
            }
            if ui.button_with_size(&self.label_close, [400., button_height])
                || (self.hotkey_close.is_pressed(ui)
                    && !(ui.io().want_capture_keyboard && ui.is_any_item_active()))
            {
                ui.close_current_popup();
            }
        }
    }

    fn interact(&mut self, ui: &imgui::Ui) {
        if self.hotkey_load.is_some_and(|key| key.is_pressed(ui)) {
            self.spawn();
        }
    }

    fn log(&mut self, tx: &Sender<String>) {
        for log in self.logs.drain(..) {
            tx.send(log).ok();
        }
    }
}

unsafe fn spawn_item(function: usize, manager: usize, item_id: u32, qty: u32, durability: u32) {
    #[repr(C)]
    struct SpawnRequest {
        one: u32,
        item_id: u32,
        qty: u32,
        durability: u32,
    }

    type SpawnItemFn = extern "system" fn(*const c_void, *mut SpawnRequest, *mut [u32; 4]);
    let spawn_fn_ptr = std::mem::transmute::<usize, SpawnItemFn>(function);
    let mut request = SpawnRequest { one: 1, item_id, qty, durability };
    spawn_fn_ptr(manager as *const c_void, &mut request, &mut [0; 4]);
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use once_cell::sync::Lazy;

    use super::*;

    static IN_GAME: u8 = 1;
    static SENTINEL: Lazy<Bitflag<u8>> =
        Lazy::new(|| Bitflag::new(PointerChain::new(&[&IN_GAME as *const u8 as usize]), 1));
    static SPAWNED: Mutex<Option<(usize, [u32; 4])>> = Mutex::new(None);

    extern "system" fn record_spawn(manager: *const c_void, request: *mut u32, _: *mut [u32; 4]) {
        *SPAWNED.lock().unwrap() =
            Some((manager as usize, unsafe { request.cast::<[u32; 4]>().read() }));
    }

    fn spawner(manager: &usize) -> ItemSpawner {
        let mut spawner = ItemSpawner::new(
            record_spawn as *const () as usize,
            manager as *const usize as usize,
            0,
            &SENTINEL,
            None,
            Key::try_from("0").unwrap(),
        );
        spawner.items = vec![
            Item {
                id: 1000000,
                name: "长剑".into(),
                category: ItemCategory::Weapons,
                dlc: false,
                max_quantity: 12,
                variants: vec![ItemVariant { affinity: 0, max_upgrade: 5 }, ItemVariant {
                    affinity: 100,
                    max_upgrade: 10,
                }],
            },
            Item {
                id: 0x40000100,
                name: "火焰壶".into(),
                category: ItemCategory::Consumables,
                dlc: true,
                max_quantity: 20,
                variants: vec![ItemVariant { affinity: 0, max_upgrade: 0 }],
            },
        ];
        spawner.filter();
        spawner
    }

    #[test]
    fn localized_and_id_filters_clear_hidden_selection_without_selecting_results() {
        let manager = 0;
        let mut spawner = spawner(&manager);
        assert_eq!(spawner.current, None);
        spawner.select(Some(0));
        spawner.filter_string = "火壶".into();
        spawner.filter();
        assert_eq!(spawner.filtered, [1]);
        assert_eq!(spawner.current, None);
        for query in ["0x40000100", "40000100", "1073742080", "0x100"] {
            spawner.filter_string = query.into();
            spawner.filter();
            assert_eq!(spawner.filtered, [1]);
            assert_eq!(spawner.current, None);
        }
        spawner.filter_string.clear();
        spawner.source_filter = 1;
        spawner.filter();
        assert_eq!(spawner.filtered, [0]);
        spawner.source_filter = 2;
        spawner.filter();
        assert_eq!(spawner.filtered, [1]);
        spawner.select(Some(1));
        spawner.clear();
        assert_eq!(spawner.current, None);
        assert_eq!(spawner.filtered, [0, 1]);
        spawner.refresh();
        assert!(spawner.items.is_empty());
        assert_eq!(spawner.current, None);
    }

    #[test]
    fn reload_preserves_affinity_by_value_and_resets_a_removed_affinity() {
        let manager = 0;
        let mut widget = spawner(&manager);
        widget.items[0].variants[1].affinity = 300;
        widget.select(Some(0));
        widget.variant = 1;
        widget.upgrade = 8;

        let mut updated = spawner(&manager).items;
        updated[0].variants.push(ItemVariant { affinity: 300, max_upgrade: 10 });
        updated.swap(0, 1);
        widget.set_items(updated);
        assert_eq!((widget.current, widget.variant, widget.upgrade), (Some(1), 2, 8));
        assert_eq!(widget.items[1].spawn_id(widget.variant, widget.upgrade), Some(1000308));

        let mut updated = spawner(&manager).items;
        updated[0].variants.truncate(1);
        widget.set_items(updated);
        assert_eq!((widget.current, widget.variant, widget.upgrade), (Some(0), 0, 0));
        widget.set_items(Vec::new());
        assert_eq!(widget.current, None);
        widget.set_items(spawner(&manager).items);
        assert_eq!(widget.current, None);
    }

    #[test]
    fn spawning_requires_selection_and_manager_and_passes_ds3_request_fields() {
        let mut manager = 0x1234;
        let mut spawner = spawner(&manager);
        *SPAWNED.lock().unwrap() = None;
        spawner.spawn();
        assert!(SPAWNED.lock().unwrap().is_none());
        spawner.select(Some(0));
        spawner.variant = 1;
        spawner.upgrade = 50;
        spawner.qty = 99;
        spawner.spawn();
        assert_eq!(SPAWNED.lock().unwrap().take(), Some((manager, [1, 1000110, 12, 100])));
        manager = 0;
        spawner.spawn();
        assert!(SPAWNED.lock().unwrap().is_none());
        assert_eq!(manager, 0);
        spawner.select(Some(1));
        spawner.upgrade = 10;
        spawner.variant = 10;
        spawner.clamp_options();
        assert_eq!((spawner.variant, spawner.upgrade), (0, 0));
        spawner.clear();
        spawner.spawn();
        assert!(SPAWNED.lock().unwrap().is_none());
    }
}
