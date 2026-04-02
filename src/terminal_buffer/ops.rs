use crate::renderer::ActiveScreen;

use super::TerminalBuffer;
use super::types::{CellAttributes, TerminalCell};

impl TerminalBuffer {
    pub(crate) fn reset(&mut self) {
        self.primary.clear();
        self.alternate.clear();
        self.active_screen = ActiveScreen::Primary;
        self.scrollback.clear();
        self.viewport_offset = 0;
        self.utf8_buffer.clear();
        self.parser = super::types::ParserState::Ground;
        self.current_attr = CellAttributes::default();
        self.damage.mark_full_rebuild();
        self.damage.mark_all_rows(self.rows());
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn write_char(&mut self, ch: char) {
        if self.screen().wrap_pending {
            self.wrap_to_next_line();
        }

        let row = self.screen().cursor_row;
        let col = self.screen().cursor_col;
        let mut cell = TerminalCell::default();
        cell.ch = ch;
        cell.fg = self.current_attr.fg;
        cell.bg = self.current_attr.bg;
        cell.flags = self.current_attr.flags;

        let screen = self.screen_mut();
        screen.set_cell(row, col, cell);
        if screen.cursor_col + 1 >= screen.cols {
            screen.wrap_pending = true;
        } else {
            screen.cursor_col += 1;
        }
        self.viewport_offset = 0;
        self.damage.mark_row(row);
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn carriage_return(&mut self) {
        self.screen_mut().cursor_col = 0;
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn line_feed(&mut self) {
        self.screen_mut().wrap_pending = false;
        self.screen_mut().cursor_col = 0;
        self.index_down();
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn next_line(&mut self) {
        self.line_feed();
        self.screen_mut().cursor_col = 0;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn index_down(&mut self) {
        let (cursor_row, scroll_bottom, rows) = {
            let screen = self.screen();
            (screen.cursor_row, screen.scroll_bottom, screen.rows)
        };
        if cursor_row == scroll_bottom {
            self.scroll_up(1);
        } else {
            self.screen_mut().cursor_row = cursor_row.saturating_add(1).min(rows - 1);
        }
    }

    pub(crate) fn reverse_index(&mut self) {
        self.screen_mut().wrap_pending = false;
        let (cursor_row, scroll_top) = {
            let screen = self.screen();
            (screen.cursor_row, screen.scroll_top)
        };
        if cursor_row == scroll_top {
            self.scroll_down(1);
        } else {
            self.screen_mut().cursor_row = cursor_row.saturating_sub(1);
        }
        self.damage.mark_cursor_dirty();
    }

    fn wrap_to_next_line(&mut self) {
        self.screen_mut().wrap_pending = false;
        self.screen_mut().cursor_col = 0;
        self.index_down();
    }

    pub(crate) fn backspace(&mut self) {
        self.screen_mut().wrap_pending = false;
        self.screen_mut().cursor_col = self.screen().cursor_col.saturating_sub(1);
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn tab(&mut self) {
        let next_stop = ((self.screen().cursor_col as usize / 8) + 1) * 8;
        self.screen_mut().cursor_col = (next_stop.min(self.cols() as usize - 1)) as u16;
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn move_cursor_relative(&mut self, row_delta: i32, col_delta: i32) {
        let row = (self.screen().cursor_row as i32 + row_delta).clamp(0, self.rows() as i32 - 1);
        let col = (self.screen().cursor_col as i32 + col_delta).clamp(0, self.cols() as i32 - 1);
        self.screen_mut().cursor_row = row as u16;
        self.screen_mut().cursor_col = col as u16;
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn set_cursor_position(&mut self, row: u16, col: u16) {
        self.screen_mut().cursor_row = row.min(self.rows() - 1);
        self.screen_mut().cursor_col = col.min(self.cols() - 1);
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn set_cursor_row(&mut self, row: u16) {
        self.screen_mut().cursor_row = row.min(self.rows() - 1);
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn set_cursor_col(&mut self, col: u16) {
        self.screen_mut().cursor_col = col.min(self.cols() - 1);
        self.screen_mut().wrap_pending = false;
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn erase_in_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in self.screen().cursor_row + 1..self.rows() {
                    self.clear_row(row);
                }
            }
            1 => {
                for row in 0..self.screen().cursor_row {
                    self.clear_row(row);
                }
                self.erase_in_line(1);
            }
            2 => {
                for row in 0..self.rows() {
                    self.clear_row(row);
                }
            }
            _ => {}
        }
        self.screen_mut().wrap_pending = false;
    }

    pub(crate) fn erase_in_line(&mut self, mode: usize) {
        let row = self.screen().cursor_row;
        match mode {
            0 => {
                for col in self.screen().cursor_col..self.cols() {
                    self.set_blank(row, col);
                }
            }
            1 => {
                for col in 0..=self.screen().cursor_col {
                    self.set_blank(row, col);
                }
            }
            2 => self.clear_row(row),
            _ => {}
        }
        self.screen_mut().wrap_pending = false;
    }

    pub(crate) fn insert_chars(&mut self, count: u16) {
        let row = self.screen().cursor_row;
        let count = count.max(1).min(self.cols() - self.screen().cursor_col);
        let start = self.screen().cursor_col as usize;
        let width = self.cols() as usize;
        let row_start = row as usize * width;
        let shift = count as usize;

        let screen = self.screen_mut();
        for col in (start..width - shift).rev() {
            screen.cells[row_start + col + shift] = screen.cells[row_start + col];
        }
        for col in start..(start + shift).min(width) {
            screen.cells[row_start + col] = TerminalCell::default();
        }
        screen.wrap_pending = false;
        self.damage.mark_row(row);
    }

    pub(crate) fn delete_chars(&mut self, count: u16) {
        let row = self.screen().cursor_row;
        let count = count.max(1).min(self.cols() - self.screen().cursor_col);
        let start = self.screen().cursor_col as usize;
        let width = self.cols() as usize;
        let row_start = row as usize * width;
        let shift = count as usize;

        let screen = self.screen_mut();
        for col in start..width - shift {
            screen.cells[row_start + col] = screen.cells[row_start + col + shift];
        }
        for col in width.saturating_sub(shift)..width {
            screen.cells[row_start + col] = TerminalCell::default();
        }
        screen.wrap_pending = false;
        self.damage.mark_row(row);
    }

    pub(crate) fn insert_lines(&mut self, count: u16) {
        if self.screen().cursor_row < self.screen().scroll_top
            || self.screen().cursor_row > self.screen().scroll_bottom
        {
            return;
        }
        let cursor_row = self.screen().cursor_row;
        let scroll_bottom = self.screen().scroll_bottom;
        self.scroll_down_in_region(cursor_row, scroll_bottom, count.max(1));
        self.screen_mut().wrap_pending = false;
    }

    pub(crate) fn delete_lines(&mut self, count: u16) {
        if self.screen().cursor_row < self.screen().scroll_top
            || self.screen().cursor_row > self.screen().scroll_bottom
        {
            return;
        }
        let cursor_row = self.screen().cursor_row;
        let scroll_bottom = self.screen().scroll_bottom;
        self.scroll_up_in_region(cursor_row, scroll_bottom, count.max(1));
        self.screen_mut().wrap_pending = false;
    }

    pub(crate) fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        if top >= bottom || bottom >= self.rows() {
            self.screen_mut().scroll_top = 0;
            self.screen_mut().scroll_bottom = self.rows() - 1;
        } else {
            self.screen_mut().scroll_top = top;
            self.screen_mut().scroll_bottom = bottom;
        }
        self.set_cursor_position(0, 0);
    }

    pub(crate) fn save_cursor(&mut self) {
        let row = self.screen().cursor_row;
        let col = self.screen().cursor_col;
        self.screen_mut().saved_cursor = Some((row, col));
    }

    pub(crate) fn restore_cursor(&mut self) {
        if let Some((row, col)) = self.screen().saved_cursor {
            self.set_cursor_position(row, col);
        }
    }

    fn scroll_up(&mut self, count: u16) {
        let top = self.screen().scroll_top;
        let bottom = self.screen().scroll_bottom;
        self.scroll_up_in_region(top, bottom, count.max(1));
    }

    fn scroll_down(&mut self, count: u16) {
        let top = self.screen().scroll_top;
        let bottom = self.screen().scroll_bottom;
        self.scroll_down_in_region(top, bottom, count.max(1));
    }

    fn scroll_up_in_region(&mut self, top: u16, bottom: u16, count: u16) {
        let rows = self.rows();
        let cols = self.cols();
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        if count == 0 {
            return;
        }

        if self.active_screen == ActiveScreen::Primary && top == 0 && bottom == rows - 1 {
            for _ in 0..count {
                let row = (0..cols)
                    .map(|col| self.primary.cell(0, col))
                    .collect::<Vec<_>>();
                self.scrollback.push_back(row);
                while self.scrollback.len() > self.scrollback_capacity {
                    self.scrollback.pop_front();
                }
                self.viewport_offset = 0;
                let primary = &mut self.primary;
                for row in 0..rows - 1 {
                    for col in 0..cols {
                        let from = primary.index(row + 1, col);
                        let to = primary.index(row, col);
                        primary.cells[to] = primary.cells[from];
                    }
                }
                primary.clear_row(rows - 1);
            }
            self.damage.mark_all_rows(rows);
            return;
        }

        let screen = self.screen_mut();
        for row in top..=bottom - count {
            for col in 0..cols {
                let from = screen.index(row + count, col);
                let to = screen.index(row, col);
                screen.cells[to] = screen.cells[from];
            }
        }
        for row in bottom - count + 1..=bottom {
            screen.clear_row(row);
        }
        self.damage.mark_rows(top..=bottom);
    }

    fn scroll_down_in_region(&mut self, top: u16, bottom: u16, count: u16) {
        let cols = self.cols();
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        if count == 0 {
            return;
        }

        let screen = self.screen_mut();
        for row in (top + count..=bottom).rev() {
            for col in 0..cols {
                let from = screen.index(row - count, col);
                let to = screen.index(row, col);
                screen.cells[to] = screen.cells[from];
            }
        }
        for row in top..top + count {
            screen.clear_row(row);
        }
        self.damage.mark_rows(top..=bottom);
    }

    fn clear_row(&mut self, row: u16) {
        self.screen_mut().clear_row(row);
        self.damage.mark_row(row);
    }

    fn set_blank(&mut self, row: u16, col: u16) {
        self.screen_mut().set_cell(row, col, TerminalCell::default());
        self.damage.mark_row(row);
    }

    pub(crate) fn set_private_mode(&mut self, params: &[Option<usize>], enabled: bool) {
        for param in params.iter().copied().flatten() {
            match param {
                47 | 1047 | 1049 if enabled => self.enter_alternate_screen(true),
                47 | 1047 | 1049 if !enabled => self.exit_alternate_screen(),
                1048 if enabled => self.save_cursor(),
                1048 if !enabled => self.restore_cursor(),
                _ => {}
            }
        }
    }

    fn enter_alternate_screen(&mut self, clear: bool) {
        if self.active_screen == ActiveScreen::Alternate {
            return;
        }
        if clear {
            self.alternate.clear();
        }
        self.active_screen = ActiveScreen::Alternate;
        self.damage.mark_full_rebuild();
        self.damage.mark_all_rows(self.rows());
        self.damage.mark_cursor_dirty();
    }

    fn exit_alternate_screen(&mut self) {
        if self.active_screen == ActiveScreen::Primary {
            return;
        }
        self.active_screen = ActiveScreen::Primary;
        self.viewport_offset = 0;
        self.damage.mark_full_rebuild();
        self.damage.mark_all_rows(self.rows());
        self.damage.mark_cursor_dirty();
    }

    pub(crate) fn visible_rows(&self) -> Vec<Vec<TerminalCell>> {
        let rows = self.rows() as usize;
        let cols = self.cols() as usize;
        let screen = self.screen();
        if self.active_screen == ActiveScreen::Alternate {
            return (0..rows)
                .map(|row| {
                    (0..cols)
                        .map(|col| screen.cell(row as u16, col as u16))
                        .collect::<Vec<_>>()
                })
                .collect();
        }

        let history_len = self.scrollback.len();
        let total_rows = history_len + rows;
        let window_end = total_rows.saturating_sub(self.viewport_offset);
        let window_start = window_end.saturating_sub(rows);
        let mut visible_rows = Vec::with_capacity(rows);

        for absolute_row in window_start..window_end {
            if absolute_row < history_len {
                let mut row = self.scrollback[absolute_row].clone();
                row.resize(cols, TerminalCell::default());
                visible_rows.push(row);
            } else {
                let live_row = absolute_row - history_len;
                visible_rows.push(
                    (0..cols)
                        .map(|col| screen.cell(live_row as u16, col as u16))
                        .collect::<Vec<_>>(),
                );
            }
        }

        while visible_rows.len() < rows {
            visible_rows.insert(0, vec![TerminalCell::default(); cols]);
        }

        visible_rows
    }

    pub(crate) fn resize_scrollback_rows(&mut self, cols: u16) {
        let cols = cols as usize;
        for row in &mut self.scrollback {
            row.resize(cols, TerminalCell::default());
        }
    }
}
