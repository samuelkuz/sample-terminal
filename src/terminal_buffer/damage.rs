use std::collections::BTreeSet;

use crate::renderer::RenderDamage;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DamageTracker {
    full_rebuild: bool,
    dirty_rows: BTreeSet<u16>,
    selection_dirty: bool,
    cursor_dirty: bool,
    global_dirty: bool,
}

impl DamageTracker {
    pub(crate) fn mark_full_rebuild(&mut self) {
        self.full_rebuild = true;
        self.global_dirty = true;
    }

    pub(crate) fn mark_row(&mut self, row: u16) {
        self.dirty_rows.insert(row);
    }

    pub(crate) fn mark_rows(&mut self, rows: impl IntoIterator<Item = u16>) {
        for row in rows {
            self.mark_row(row);
        }
    }

    pub(crate) fn mark_all_rows(&mut self, rows: u16) {
        self.mark_rows(0..rows);
    }

    pub(crate) fn mark_cursor_dirty(&mut self) {
        self.cursor_dirty = true;
    }

    pub(crate) fn take(&mut self) -> RenderDamage {
        let damage = RenderDamage {
            full_rebuild: self.full_rebuild,
            dirty_rows: std::mem::take(&mut self.dirty_rows),
            selection_dirty: self.selection_dirty,
            cursor_dirty: self.cursor_dirty,
            global_dirty: self.global_dirty,
        };
        *self = Self::default();
        damage
    }
}
