use hudhook::tracing::debug;
use imgui::*;
use libds3::prelude::*;
use sys::{igGetCursorPosX, igGetCursorPosY, igGetWindowPos, igSetNextWindowPos, ImVec2};

use crate::util::KeyState;
use crate::widgets::{scaling_factor, Widget};

#[derive(Debug)]
pub(crate) struct CharacterStatsEdit {
    hotkey_open: KeyState,
    hotkey_close: KeyState,
    label_open: String,
    label_close: String,
    ptr: PointerChain<CharacterStats>,
    stats: Option<CharacterStats>,
}

impl CharacterStatsEdit {
    pub(crate) fn new(
        hotkey_open: KeyState,
        hotkey_close: KeyState,
        ptr: PointerChain<CharacterStats>,
    ) -> Self {
        let label_open = format!("修改属性 ({hotkey_open})");
        let label_close = format!("关闭 ({hotkey_close})");
        CharacterStatsEdit { hotkey_open, hotkey_close, label_open, label_close, ptr, stats: None }
    }
}

impl Widget for CharacterStatsEdit {
    fn render(&mut self, ui: &imgui::Ui) {
        let scale = scaling_factor(ui);
        let button_width = super::BUTTON_WIDTH * super::scaling_factor(ui);

        let (x, y) = unsafe {
            let mut wnd_pos = ImVec2::default();
            igGetWindowPos(&mut wnd_pos);
            (igGetCursorPosX() + wnd_pos.x, igGetCursorPosY() + wnd_pos.y)
        };

        if ui.button_with_size(&self.label_open, [button_width, super::BUTTON_HEIGHT]) {
            self.stats = self.ptr.read();
            debug!("{:?}", self.stats);
        }

        if self.stats.is_some() {
            ui.open_popup("##character_stats_edit");
        }

        unsafe {
            igSetNextWindowPos(
                ImVec2::new(x + 200. * scale, y),
                Condition::Always as i8 as _,
                ImVec2::new(0., 0.),
            )
        };

        if let Some(_token) = ui
            .modal_popup_config("##character_stats_edit")
            .resizable(false)
            .movable(false)
            .title_bar(false)
            .scroll_bar(false)
            .begin_popup()
        {
            let _tok = ui.push_item_width(150.);
            if let Some(stats) = self.stats.as_mut() {
                if ui.input_int("等级", &mut stats.level).build() {
                    stats.level = stats.level.clamp(1, i32::MAX);
                }
                if ui.input_int("生命力", &mut stats.vigor).build() {
                    stats.vigor = stats.vigor.clamp(1, 99);
                }
                if ui.input_int("集中力", &mut stats.attunement).build() {
                    stats.attunement = stats.attunement.clamp(1, 99);
                }
                if ui.input_int("持久力", &mut stats.endurance).build() {
                    stats.endurance = stats.endurance.clamp(1, 99);
                }
                if ui.input_int("体力", &mut stats.vitality).build() {
                    stats.vitality = stats.vitality.clamp(1, 99);
                }
                if ui.input_int("力气", &mut stats.strength).build() {
                    stats.strength = stats.strength.clamp(1, 99);
                }
                if ui.input_int("敏捷", &mut stats.dexterity).build() {
                    stats.dexterity = stats.dexterity.clamp(1, 99);
                }
                if ui.input_int("智力", &mut stats.intelligence).build() {
                    stats.intelligence = stats.intelligence.clamp(1, 99);
                }
                if ui.input_int("信仰", &mut stats.faith).build() {
                    stats.faith = stats.faith.clamp(1, 99);
                }
                if ui.input_int("运气", &mut stats.luck).build() {
                    stats.luck = stats.luck.clamp(1, 99);
                }
                if ui.input_int("灵魂", &mut stats.souls).build() {
                    stats.souls = stats.souls.clamp(0, i32::MAX);
                }

                if ui.button_with_size("应用", [button_width, super::BUTTON_HEIGHT]) {
                    self.ptr.write(stats.clone());
                }
            }

            if ui.button_with_size(&self.label_close, [button_width, super::BUTTON_HEIGHT])
                || (self.hotkey_close.keyup(ui) && !ui.is_any_item_active())
            {
                ui.close_current_popup();
                self.stats.take();
            }
        }
    }

    fn interact(&mut self, ui: &imgui::Ui) {
        if self.hotkey_open.keyup(ui) {
            self.stats = self.ptr.read();
        }
    }
}
