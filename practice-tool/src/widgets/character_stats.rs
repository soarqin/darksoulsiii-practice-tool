use libds3::prelude::*;
use practice_tool_core::key::Key;
use practice_tool_core::widgets::stats_editor::{Datum, Stats, StatsEditor};
use practice_tool_core::widgets::Widget;

#[derive(Debug)]
struct CharacterStatsEdit {
    ptr: PointerChain<CharacterStats>,
    stats: Option<CharacterStats>,
}

impl Stats for CharacterStatsEdit {
    fn data(&mut self) -> Option<impl Iterator<Item = Datum>> {
        self.stats.as_mut().map(|s| {
            [
                Datum::int("等级", &mut s.level, 1, i32::MAX),
                Datum::int("生命力", &mut s.vigor, 1, 99),
                Datum::int("集中力", &mut s.attunement, 1, 99),
                Datum::int("持久力", &mut s.endurance, 1, 99),
                Datum::int("体力", &mut s.vitality, 1, 99),
                Datum::int("力气", &mut s.strength, 1, 99),
                Datum::int("敏捷", &mut s.dexterity, 1, 99),
                Datum::int("智力", &mut s.intelligence, 1, 99),
                Datum::int("信仰", &mut s.faith, 1, 99),
                Datum::int("运气", &mut s.luck, 1, 99),
                Datum::int("灵魂", &mut s.souls, 0, i32::MAX),
            ]
            .into_iter()
        })
    }

    fn read(&mut self) {
        self.stats = self.ptr.read();
    }

    fn write(&mut self) {
        if let Some(stats) = self.stats.clone() {
            self.ptr.write(stats);
        }
    }

    fn clear(&mut self) {
        self.stats = None;
    }
}

pub(crate) fn character_stats_edit(
    character_stats: PointerChain<CharacterStats>,
    key_open: Option<Key>,
    key_close: Key,
) -> Box<dyn Widget> {
    Box::new(StatsEditor::new(
        CharacterStatsEdit { ptr: character_stats, stats: None },
        key_open,
        Some(key_close),
    ))
}
