use crate::renderer::{Cell, CursorState, RenderState};

#[derive(Debug)]
pub struct TerminalBuffer {
    cols: u16,
    rows: u16,
    cells: Vec<TerminalCell>,
    cursor_row: u16,
    cursor_col: u16,
    saved_cursor: Option<(u16, u16)>,
    wrap_pending: bool,
    scroll_top: u16,
    scroll_bottom: u16,
    parser: ParserState,
    utf8_buffer: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalCell {
    ch: char,
    fg: [f32; 4],
    bg: [f32; 4],
    flags: u8,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: [0.92, 0.93, 0.95, 1.0],
            bg: [0.13, 0.16, 0.20, 1.0],
            flags: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParserState {
    Ground,
    Escape,
    Csi(Vec<u8>),
    Osc,
    OscEsc,
}

impl TerminalBuffer {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            cells: vec![TerminalCell::default(); cols as usize * rows as usize],
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor: None,
            wrap_pending: false,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            parser: ParserState::Ground,
            utf8_buffer: Vec::new(),
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.cols == cols && self.rows == rows {
            return;
        }

        let old_cols = self.cols;
        let old_rows = self.rows;
        let old_cells = self.cells.clone();

        self.cols = cols;
        self.rows = rows;
        self.cells = vec![TerminalCell::default(); cols as usize * rows as usize];

        let copy_rows = old_rows.min(rows);
        let copy_cols = old_cols.min(cols);
        for row in 0..copy_rows {
            for col in 0..copy_cols {
                let old_index = row as usize * old_cols as usize + col as usize;
                let new_index = row as usize * cols as usize + col as usize;
                self.cells[new_index] = old_cells[old_index];
            }
        }

        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.wrap_pending = false;
        if let Some((row, col)) = self.saved_cursor {
            self.saved_cursor = Some((row.min(rows - 1), col.min(cols - 1)));
        }
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push_byte(byte);
        }
    }

    pub fn render_state(&self) -> RenderState {
        let mut state = RenderState::new(self.cols, self.rows);
        for row in 0..self.rows {
            for col in 0..self.cols {
                let source = self.cells[self.index(row, col)];
                state.set_cell(
                    row,
                    col,
                    Cell {
                        ch: source.ch,
                        fg: source.fg,
                        bg: source.bg,
                        flags: source.flags,
                    },
                );
            }
        }
        state.set_cursor(Some(CursorState {
            row: self.cursor_row,
            col: self.cursor_col.min(self.cols - 1),
            visible: true,
        }));
        state
    }

    fn push_byte(&mut self, byte: u8) {
        match self.parser {
            ParserState::Ground => self.handle_ground_byte(byte),
            ParserState::Escape => self.handle_escape_byte(byte),
            ParserState::Csi(_) => self.handle_csi_byte(byte),
            ParserState::Osc => self.handle_osc_byte(byte),
            ParserState::OscEsc => self.handle_osc_esc_byte(byte),
        }
    }

    fn handle_ground_byte(&mut self, byte: u8) {
        match byte {
            0x1b => {
                self.flush_utf8_lossy();
                self.parser = ParserState::Escape;
            }
            b'\r' => {
                self.flush_utf8_lossy();
                self.carriage_return();
            }
            b'\n' | 0x0b | 0x0c => {
                self.flush_utf8_lossy();
                self.line_feed();
            }
            0x08 | 0x7f => {
                self.flush_utf8_lossy();
                self.backspace();
            }
            b'\t' => {
                self.flush_utf8_lossy();
                self.tab();
            }
            0x07 => {
                self.flush_utf8_lossy();
            }
            0x00..=0x1f => {
                self.flush_utf8_lossy();
            }
            0x20..=0x7e => {
                self.flush_utf8_lossy();
                self.write_char(byte as char);
            }
            _ => self.push_utf8_byte(byte),
        }
    }

    fn handle_escape_byte(&mut self, byte: u8) {
        self.parser = match byte {
            b'[' => ParserState::Csi(Vec::new()),
            b']' => ParserState::Osc,
            b'D' => {
                self.index_down();
                ParserState::Ground
            }
            b'M' => {
                self.reverse_index();
                ParserState::Ground
            }
            b'E' => {
                self.next_line();
                ParserState::Ground
            }
            b'7' => {
                self.save_cursor();
                ParserState::Ground
            }
            b'8' => {
                self.restore_cursor();
                ParserState::Ground
            }
            b'c' => {
                self.reset();
                ParserState::Ground
            }
            _ => ParserState::Ground,
        };
    }

    fn handle_csi_byte(&mut self, byte: u8) {
        let ParserState::Csi(mut bytes) = std::mem::replace(&mut self.parser, ParserState::Ground) else {
            return;
        };

        if (0x40..=0x7e).contains(&byte) {
            self.execute_csi(byte, &bytes);
            return;
        }

        if bytes.len() < 64 {
            bytes.push(byte);
        }
        self.parser = ParserState::Csi(bytes);
    }

    fn handle_osc_byte(&mut self, byte: u8) {
        self.parser = match byte {
            0x07 => ParserState::Ground,
            0x1b => ParserState::OscEsc,
            _ => ParserState::Osc,
        };
    }

    fn handle_osc_esc_byte(&mut self, byte: u8) {
        self.parser = if byte == b'\\' {
            ParserState::Ground
        } else {
            ParserState::Osc
        };
    }

    fn execute_csi(&mut self, final_byte: u8, raw: &[u8]) {
        let params = parse_csi_params(raw);
        match final_byte {
            b'A' => self.move_cursor_relative(-(param_or(&params, 0, 1) as i32), 0),
            b'B' => self.move_cursor_relative(param_or(&params, 0, 1) as i32, 0),
            b'C' => self.move_cursor_relative(0, param_or(&params, 0, 1) as i32),
            b'D' => self.move_cursor_relative(0, -(param_or(&params, 0, 1) as i32)),
            b'G' => self.set_cursor_col(param_or(&params, 0, 1).saturating_sub(1) as u16),
            b'd' => self.set_cursor_row(param_or(&params, 0, 1).saturating_sub(1) as u16),
            b'H' | b'f' => {
                let row = param_or(&params, 0, 1).saturating_sub(1) as u16;
                let col = param_or(&params, 1, 1).saturating_sub(1) as u16;
                self.set_cursor_position(row, col);
            }
            b'J' => self.erase_in_display(param_or(&params, 0, 0)),
            b'K' => self.erase_in_line(param_or(&params, 0, 0)),
            b'@' => self.insert_chars(param_or(&params, 0, 1) as u16),
            b'P' => self.delete_chars(param_or(&params, 0, 1) as u16),
            b'L' => self.insert_lines(param_or(&params, 0, 1) as u16),
            b'M' => self.delete_lines(param_or(&params, 0, 1) as u16),
            b'r' => self.set_scroll_region(
                param_or(&params, 0, 1).saturating_sub(1) as u16,
                param_or(&params, 1, self.rows as usize).saturating_sub(1) as u16,
            ),
            b's' => self.save_cursor(),
            b'u' => self.restore_cursor(),
            _ => {}
        }
        self.wrap_pending = false;
    }

    fn push_utf8_byte(&mut self, byte: u8) {
        self.utf8_buffer.push(byte);
        match std::str::from_utf8(&self.utf8_buffer) {
            Ok(text) => {
                let decoded = text.chars().collect::<Vec<_>>();
                self.utf8_buffer.clear();
                for ch in decoded {
                    self.write_char(ch);
                }
            }
            Err(error) if error.error_len().is_none() => {}
            Err(_) => {
                self.write_char('\u{fffd}');
                self.utf8_buffer.clear();
            }
        }
    }

    fn flush_utf8_lossy(&mut self) {
        if !self.utf8_buffer.is_empty() {
            self.write_char('\u{fffd}');
            self.utf8_buffer.clear();
        }
    }

    fn reset(&mut self) {
        self.cells.fill(TerminalCell::default());
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.saved_cursor = None;
        self.wrap_pending = false;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.utf8_buffer.clear();
        self.parser = ParserState::Ground;
    }

    fn write_char(&mut self, ch: char) {
        if self.wrap_pending {
            self.wrap_to_next_line();
        }

        let index = self.index(self.cursor_row, self.cursor_col);
        self.cells[index].ch = ch;

        if self.cursor_col + 1 >= self.cols {
            self.wrap_pending = true;
        } else {
            self.cursor_col += 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
        self.wrap_pending = false;
    }

    fn line_feed(&mut self) {
        self.wrap_pending = false;
        self.index_down();
    }

    fn next_line(&mut self) {
        self.line_feed();
        self.cursor_col = 0;
    }

    fn index_down(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor_row = self.cursor_row.saturating_add(1).min(self.rows - 1);
        }
    }

    fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_top {
            self.scroll_down(1);
        } else {
            self.cursor_row = self.cursor_row.saturating_sub(1);
        }
    }

    fn wrap_to_next_line(&mut self) {
        self.wrap_pending = false;
        self.cursor_col = 0;
        self.index_down();
    }

    fn backspace(&mut self) {
        self.wrap_pending = false;
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    fn tab(&mut self) {
        let next_stop = ((self.cursor_col as usize / 8) + 1) * 8;
        self.cursor_col = (next_stop.min(self.cols as usize - 1)) as u16;
        self.wrap_pending = false;
    }

    fn move_cursor_relative(&mut self, row_delta: i32, col_delta: i32) {
        let row = (self.cursor_row as i32 + row_delta).clamp(0, self.rows as i32 - 1);
        let col = (self.cursor_col as i32 + col_delta).clamp(0, self.cols as i32 - 1);
        self.cursor_row = row as u16;
        self.cursor_col = col as u16;
        self.wrap_pending = false;
    }

    fn set_cursor_position(&mut self, row: u16, col: u16) {
        self.cursor_row = row.min(self.rows - 1);
        self.cursor_col = col.min(self.cols - 1);
        self.wrap_pending = false;
    }

    fn set_cursor_row(&mut self, row: u16) {
        self.cursor_row = row.min(self.rows - 1);
        self.wrap_pending = false;
    }

    fn set_cursor_col(&mut self, col: u16) {
        self.cursor_col = col.min(self.cols - 1);
        self.wrap_pending = false;
    }

    fn erase_in_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in self.cursor_row + 1..self.rows {
                    self.clear_row(row);
                }
            }
            1 => {
                for row in 0..self.cursor_row {
                    self.clear_row(row);
                }
                self.erase_in_line(1);
            }
            2 => {
                for row in 0..self.rows {
                    self.clear_row(row);
                }
            }
            _ => {}
        }
        self.wrap_pending = false;
    }

    fn erase_in_line(&mut self, mode: usize) {
        match mode {
            0 => {
                for col in self.cursor_col..self.cols {
                    self.set_blank(self.cursor_row, col);
                }
            }
            1 => {
                for col in 0..=self.cursor_col {
                    self.set_blank(self.cursor_row, col);
                }
            }
            2 => self.clear_row(self.cursor_row),
            _ => {}
        }
        self.wrap_pending = false;
    }

    fn insert_chars(&mut self, count: u16) {
        let count = count.max(1).min(self.cols - self.cursor_col);
        let row = self.cursor_row;
        let start = self.cursor_col as usize;
        let width = self.cols as usize;
        let row_start = row as usize * width;
        let row_end = row_start + width;
        let shift = count as usize;

        for col in (start..width - shift).rev() {
            self.cells[row_start + col + shift] = self.cells[row_start + col];
        }
        for col in start..(start + shift).min(width) {
            self.cells[row_start + col] = TerminalCell::default();
        }
        let _ = row_end;
        self.wrap_pending = false;
    }

    fn delete_chars(&mut self, count: u16) {
        let count = count.max(1).min(self.cols - self.cursor_col);
        let row = self.cursor_row;
        let start = self.cursor_col as usize;
        let width = self.cols as usize;
        let row_start = row as usize * width;
        let shift = count as usize;

        for col in start..width - shift {
            self.cells[row_start + col] = self.cells[row_start + col + shift];
        }
        for col in width.saturating_sub(shift)..width {
            self.cells[row_start + col] = TerminalCell::default();
        }
        self.wrap_pending = false;
    }

    fn insert_lines(&mut self, count: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        self.scroll_down_in_region(self.cursor_row, self.scroll_bottom, count.max(1));
        self.wrap_pending = false;
    }

    fn delete_lines(&mut self, count: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }
        self.scroll_up_in_region(self.cursor_row, self.scroll_bottom, count.max(1));
        self.wrap_pending = false;
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        if top >= bottom || bottom >= self.rows {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows - 1;
        } else {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }
        self.set_cursor_position(0, 0);
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = Some((self.cursor_row, self.cursor_col));
    }

    fn restore_cursor(&mut self) {
        if let Some((row, col)) = self.saved_cursor {
            self.set_cursor_position(row, col);
        }
    }

    fn scroll_up(&mut self, count: u16) {
        self.scroll_up_in_region(self.scroll_top, self.scroll_bottom, count.max(1));
    }

    fn scroll_down(&mut self, count: u16) {
        self.scroll_down_in_region(self.scroll_top, self.scroll_bottom, count.max(1));
    }

    fn scroll_up_in_region(&mut self, top: u16, bottom: u16, count: u16) {
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        if count == 0 {
            return;
        }

        for row in top..=bottom - count {
            for col in 0..self.cols {
                let from = self.index(row + count, col);
                let to = self.index(row, col);
                self.cells[to] = self.cells[from];
            }
        }
        for row in bottom - count + 1..=bottom {
            self.clear_row(row);
        }
    }

    fn scroll_down_in_region(&mut self, top: u16, bottom: u16, count: u16) {
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        if count == 0 {
            return;
        }

        for row in (top + count..=bottom).rev() {
            for col in 0..self.cols {
                let from = self.index(row - count, col);
                let to = self.index(row, col);
                self.cells[to] = self.cells[from];
            }
        }
        for row in top..top + count {
            self.clear_row(row);
        }
    }

    fn clear_row(&mut self, row: u16) {
        for col in 0..self.cols {
            self.set_blank(row, col);
        }
    }

    fn set_blank(&mut self, row: u16, col: u16) {
        let index = self.index(row, col);
        self.cells[index] = TerminalCell::default();
    }

    fn index(&self, row: u16, col: u16) -> usize {
        row as usize * self.cols as usize + col as usize
    }
}

fn parse_csi_params(raw: &[u8]) -> Vec<Option<usize>> {
    let filtered = raw
        .iter()
        .copied()
        .filter(|byte| byte.is_ascii_digit() || *byte == b';')
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        return Vec::new();
    }

    filtered
        .split(|byte| *byte == b';')
        .map(|part| {
            if part.is_empty() {
                None
            } else {
                std::str::from_utf8(part).ok()?.parse::<usize>().ok()
            }
        })
        .collect()
}

fn param_or(params: &[Option<usize>], index: usize, default: usize) -> usize {
    params.get(index).and_then(|value| *value).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::TerminalBuffer;
    use crate::renderer::CursorState;

    fn lines(buffer: &TerminalBuffer) -> Vec<String> {
        let state = buffer.render_state();
        (0..buffer.rows)
            .map(|row| {
                (0..buffer.cols)
                    .map(|col| state.char_at(row, col))
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn appends_plain_text() {
        let mut buffer = TerminalBuffer::new(5, 2);
        buffer.push_bytes(b"hello");

        assert_eq!(lines(&buffer), vec!["hello", "     "]);
    }

    #[test]
    fn handles_newline_and_carriage_return_on_grid() {
        let mut buffer = TerminalBuffer::new(5, 2);
        buffer.push_bytes(b"hello\rj\nxy");

        assert_eq!(lines(&buffer), vec!["jello", " xy  "]);
    }

    #[test]
    fn handles_backspace() {
        let mut buffer = TerminalBuffer::new(4, 1);
        buffer.push_bytes(b"abc\x08d");

        assert_eq!(lines(&buffer), vec!["abd "]);
    }

    #[test]
    fn wraps_and_scrolls_visible_screen() {
        let mut buffer = TerminalBuffer::new(3, 2);
        buffer.push_bytes(b"abcdefg");

        assert_eq!(lines(&buffer), vec!["def", "g  "]);
    }

    #[test]
    fn cursor_movement_sequences_work() {
        let mut buffer = TerminalBuffer::new(4, 3);
        buffer.push_bytes(b"ab\x1b[2;3H!\x1b[1A?");

        assert_eq!(lines(&buffer), vec!["ab ?", "  ! ", "    "]);
    }

    #[test]
    fn erase_in_line_and_display_clear_expected_ranges() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"abcd\x1b[1;3H\x1b[0K");

        assert_eq!(lines(&buffer), vec!["ab  ", "    "]);

        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"abcd\r\nzzzz\x1b[1;3H\x1b[0J");

        assert_eq!(lines(&buffer), vec!["ab  ", "    "]);
    }

    #[test]
    fn insert_and_delete_chars_shift_cells() {
        let mut buffer = TerminalBuffer::new(5, 1);
        buffer.push_bytes(b"abcde\r\x1b[2C\x1b[@Z\x1b[2P");

        assert_eq!(lines(&buffer), vec!["abZ  "]);
    }

    #[test]
    fn insert_and_delete_lines_shift_rows_inside_scroll_region() {
        let mut buffer = TerminalBuffer::new(3, 4);
        buffer.push_bytes(b"111222333444\x1b[2;4r\x1b[2;1H\x1b[L");

        assert_eq!(lines(&buffer), vec!["111", "   ", "222", "333"]);

        buffer.push_bytes(b"\x1b[2;1H\x1b[M");
        assert_eq!(lines(&buffer), vec!["111", "222", "333", "   "]);
    }

    #[test]
    fn unsupported_sequences_recover_parser_state() {
        let mut buffer = TerminalBuffer::new(4, 1);
        buffer.push_bytes(b"\x1b[?25lOK");

        assert_eq!(lines(&buffer), vec!["OK  "]);
    }

    #[test]
    fn grow_resize_preserves_top_left_cells() {
        let mut buffer = TerminalBuffer::new(2, 2);
        buffer.push_bytes(b"ab\r\ncd");
        buffer.resize(4, 3);

        assert_eq!(lines(&buffer), vec!["ab  ", "cd  ", "    "]);
    }

    #[test]
    fn shrink_resize_truncates_without_reflow() {
        let mut buffer = TerminalBuffer::new(4, 3);
        buffer.push_bytes(b"abcd\r\nwxyz");
        buffer.resize(2, 2);

        assert_eq!(lines(&buffer), vec!["ab", "wx"]);
    }

    #[test]
    fn render_state_reports_cursor_position() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"ab");
        let state = buffer.render_state();

        assert_eq!(state.char_at(0, 0), 'a');
        assert_eq!(state.char_at(0, 1), 'b');
        assert_eq!(
            state.cursor(),
            Some(CursorState {
                row: 0,
                col: 2,
                visible: true,
            })
        );
    }
}
