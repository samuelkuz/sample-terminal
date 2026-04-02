mod color;
mod damage;
mod ops;
mod parser;
mod types;

use std::collections::VecDeque;

use self::damage::DamageTracker;
#[cfg(test)]
pub(crate) use self::types::DEFAULT_FG;
use self::types::{CellAttributes, ParserState, ScreenBuffer, TerminalCell, TerminalModes};
use crate::renderer::{ActiveScreen, CursorState, RenderSnapshot};

const SCROLLBACK_CAPACITY: usize = 2_000;

#[derive(Debug)]
pub struct TerminalBuffer {
    // The root module owns the persistent terminal state; parser/color/ops modules
    // provide the behavior that mutates it.
    primary: ScreenBuffer,
    alternate: ScreenBuffer,
    active_screen: ActiveScreen,
    scrollback: VecDeque<Vec<TerminalCell>>,
    scrollback_capacity: usize,
    viewport_offset: usize,
    parser: ParserState,
    utf8_buffer: Vec<u8>,
    current_attr: CellAttributes,
    modes: TerminalModes,
    damage: DamageTracker,
}

impl TerminalBuffer {
    pub fn new(cols: u16, rows: u16) -> Self {
        let primary = ScreenBuffer::new(cols, rows);
        let alternate = ScreenBuffer::new(cols, rows);
        let mut damage = DamageTracker::default();
        damage.mark_full_rebuild();
        damage.mark_all_rows(rows);
        Self {
            primary,
            alternate,
            active_screen: ActiveScreen::Primary,
            scrollback: VecDeque::new(),
            scrollback_capacity: SCROLLBACK_CAPACITY,
            viewport_offset: 0,
            parser: ParserState::Ground,
            utf8_buffer: Vec::new(),
            current_attr: CellAttributes::default(),
            modes: TerminalModes::default(),
            damage,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        if self.primary.cols == cols.max(1) && self.primary.rows == rows.max(1) {
            return;
        }
        self.primary.resize(cols, rows);
        self.alternate.resize(cols, rows);
        self.resize_scrollback_rows(cols);
        self.viewport_offset = self.viewport_offset.min(self.scrollback.len());
        self.damage.mark_full_rebuild();
        self.damage.mark_all_rows(rows.max(1));
        self.damage.mark_cursor_dirty();
    }

    pub fn cols(&self) -> u16 {
        self.screen().cols
    }

    pub fn rows(&self) -> u16 {
        self.screen().rows
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push_byte(byte);
        }
    }

    pub fn set_viewport_offset(&mut self, offset: usize) {
        let next = offset.min(self.scrollback.len());
        if self.viewport_offset != next {
            self.viewport_offset = next;
            self.damage.mark_full_rebuild();
            self.damage.mark_cursor_dirty();
        }
    }

    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn modes(&self) -> TerminalModes {
        self.modes
    }

    pub fn render_snapshot(&mut self, blink_visible: bool) -> RenderSnapshot {
        let cols = self.screen().cols;
        let rows = self.screen().rows;
        let mut snapshot = RenderSnapshot::new(cols, rows);
        snapshot.set_active_screen(self.active_screen);
        snapshot.damage = self.damage.take();

        let visible_rows = self.visible_rows();
        for (row_index, row) in visible_rows.into_iter().enumerate() {
            let row_index = row_index as u16;
            for (col_index, cell) in row.into_iter().enumerate() {
                snapshot.set_cell(row_index, col_index as u16, cell.into());
            }
        }

        let cursor = self.screen();
        let cursor_allowed =
            self.active_screen == ActiveScreen::Alternate || self.viewport_offset == 0;
        snapshot.set_cursor(Some(CursorState {
            row: cursor.cursor_row,
            col: cursor.cursor_col.min(cols - 1),
            visible: self.modes.cursor_visible && blink_visible && cursor_allowed,
        }));

        snapshot
    }

    fn screen(&self) -> &ScreenBuffer {
        match self.active_screen {
            ActiveScreen::Primary => &self.primary,
            ActiveScreen::Alternate => &self.alternate,
        }
    }

    fn screen_mut(&mut self) -> &mut ScreenBuffer {
        match self.active_screen {
            ActiveScreen::Primary => &mut self.primary,
            ActiveScreen::Alternate => &mut self.alternate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalBuffer;
    use crate::renderer::{ActiveScreen, CursorState};

    fn lines(buffer: &mut TerminalBuffer) -> Vec<String> {
        let state = buffer.render_snapshot(true);
        (0..buffer.rows())
            .map(|row| {
                (0..buffer.cols())
                    .map(|col| state.char_at(row, col))
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn appends_plain_text() {
        let mut buffer = TerminalBuffer::new(5, 2);
        buffer.push_bytes(b"hello");

        assert_eq!(
            lines(&mut buffer),
            vec!["hello".to_string(), "     ".to_string()]
        );
    }

    #[test]
    fn handles_newline_and_carriage_return_on_grid() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"ab\r12\nxy");

        assert_eq!(
            lines(&mut buffer),
            vec!["12  ".to_string(), "xy  ".to_string()]
        );
    }

    #[test]
    fn wraps_and_scrolls_visible_screen() {
        let mut buffer = TerminalBuffer::new(3, 2);
        buffer.push_bytes(b"abcdefg");

        assert_eq!(
            lines(&mut buffer),
            vec!["def".to_string(), "g  ".to_string()]
        );
        assert_eq!(buffer.scrollback.len(), 1);
        assert_eq!(
            buffer
                .scrollback
                .front()
                .unwrap()
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>(),
            "abc"
        );
    }

    #[test]
    fn cursor_movement_sequences_work() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"ab\x1b[2;4H!\x1b[1;1H#");

        assert_eq!(
            lines(&mut buffer),
            vec!["#b  ".to_string(), "   !".to_string()]
        );
    }

    #[test]
    fn erase_in_line_and_display_clear_expected_ranges() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"abcdwxyz\x1b[1;3H\x1b[K\x1b[2;2H\x1b[1J");

        assert_eq!(
            lines(&mut buffer),
            vec!["    ".to_string(), "  yz".to_string()]
        );
    }

    #[test]
    fn insert_and_delete_chars_shift_cells() {
        let mut buffer = TerminalBuffer::new(5, 1);
        buffer.push_bytes(b"abcde\x1b[1;3H\x1b[@\x1b[1;3HZ\x1b[1;2H\x1b[P");

        assert_eq!(lines(&mut buffer), vec!["aZcd ".to_string()]);
    }

    #[test]
    fn insert_and_delete_lines_shift_rows_inside_scroll_region() {
        let mut buffer = TerminalBuffer::new(3, 4);
        buffer.push_bytes(b"aaa\nbbb\nccc\nddd");
        buffer.push_bytes(b"\x1b[2;4r\x1b[2;1H\x1b[L");

        assert_eq!(
            lines(&mut buffer),
            vec![
                "aaa".to_string(),
                "   ".to_string(),
                "bbb".to_string(),
                "ccc".to_string()
            ]
        );

        buffer.push_bytes(b"\x1b[2;1H\x1b[M");
        assert_eq!(
            lines(&mut buffer),
            vec![
                "aaa".to_string(),
                "bbb".to_string(),
                "ccc".to_string(),
                "   ".to_string()
            ]
        );
    }

    #[test]
    fn grow_resize_preserves_top_left_cells() {
        let mut buffer = TerminalBuffer::new(2, 2);
        buffer.push_bytes(b"abcd");
        buffer.resize(4, 3);

        assert_eq!(
            lines(&mut buffer),
            vec!["ab  ".to_string(), "cd  ".to_string(), "    ".to_string()]
        );
    }

    #[test]
    fn shrink_resize_truncates_without_reflow() {
        let mut buffer = TerminalBuffer::new(4, 3);
        buffer.push_bytes(b"abcdefgh");
        buffer.resize(2, 2);

        assert_eq!(lines(&mut buffer), vec!["ab".to_string(), "ef".to_string()]);
    }

    #[test]
    fn render_snapshot_reports_cursor_position() {
        let mut buffer = TerminalBuffer::new(4, 2);
        buffer.push_bytes(b"ab");
        let state = buffer.render_snapshot(true);

        assert_eq!(state.char_at(0, 0), 'a');
        assert_eq!(state.char_at(0, 1), 'b');
        assert_eq!(
            state.cursor,
            Some(CursorState {
                row: 0,
                col: 2,
                visible: true,
            })
        );
    }

    #[test]
    fn unsupported_sequences_recover_parser_state() {
        let mut buffer = TerminalBuffer::new(4, 1);
        buffer.push_bytes(b"\x1b[?9999hhi");

        assert_eq!(lines(&mut buffer), vec!["hi  ".to_string()]);
    }

    #[test]
    fn sgr_colors_update_cells() {
        let mut buffer = TerminalBuffer::new(3, 1);
        buffer.push_bytes(b"\x1b[31mA\x1b[38;5;46mB\x1b[38;2;1;2;3mC");
        let state = buffer.render_snapshot(true);

        assert_ne!(state.cell(0, 0).unwrap().fg, super::DEFAULT_FG);
        assert_ne!(state.cell(0, 1).unwrap().fg, super::DEFAULT_FG);
        assert_eq!(
            state.cell(0, 2).unwrap().fg,
            [1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn alternate_screen_is_isolated_from_primary() {
        let mut buffer = TerminalBuffer::new(3, 2);
        buffer.push_bytes(b"abc");
        buffer.push_bytes(b"\x1b[?1049hXY");
        let alt = buffer.render_snapshot(true);
        assert_eq!(alt.active_screen, ActiveScreen::Alternate);
        assert_eq!(alt.char_at(0, 0), 'X');
        assert_eq!(alt.char_at(0, 1), 'Y');

        buffer.push_bytes(b"\x1b[?1049l");
        let primary = buffer.render_snapshot(true);
        assert_eq!(primary.active_screen, ActiveScreen::Primary);
        assert_eq!(primary.char_at(0, 0), 'a');
        assert_eq!(primary.char_at(0, 1), 'b');
        assert_eq!(primary.char_at(0, 2), 'c');
    }

    #[test]
    fn damage_marks_row_changes_and_resize_full_rebuild() {
        let mut buffer = TerminalBuffer::new(3, 2);
        let initial = buffer.render_snapshot(true);
        assert!(initial.damage.full_rebuild);

        buffer.push_bytes(b"a");
        let changed = buffer.render_snapshot(true);
        assert!(!changed.damage.full_rebuild);
        assert_eq!(changed.damage.dirty_rows.len(), 1);
        assert!(changed.damage.dirty_rows.contains(&0));

        buffer.resize(4, 3);
        let resized = buffer.render_snapshot(true);
        assert!(resized.damage.full_rebuild);
    }

    #[test]
    fn viewport_can_show_scrollback_and_hides_cursor() {
        let mut buffer = TerminalBuffer::new(3, 2);
        buffer.push_bytes(b"abcdefg");

        assert_eq!(
            lines(&mut buffer),
            vec!["def".to_string(), "g  ".to_string()]
        );

        buffer.set_viewport_offset(1);
        let state = buffer.render_snapshot(true);

        assert_eq!(state.char_at(0, 0), 'a');
        assert_eq!(state.char_at(0, 1), 'b');
        assert_eq!(state.char_at(0, 2), 'c');
        assert_eq!(state.char_at(1, 0), 'd');
        assert_eq!(state.char_at(1, 1), 'e');
        assert_eq!(state.char_at(1, 2), 'f');
        assert_eq!(
            state.cursor,
            Some(CursorState {
                row: 1,
                col: 1,
                visible: false,
            })
        );
    }

    #[test]
    fn same_size_resize_does_not_force_damage() {
        let mut buffer = TerminalBuffer::new(3, 2);
        let _ = buffer.render_snapshot(true);

        buffer.resize(3, 2);
        let snapshot = buffer.render_snapshot(true);
        assert!(!snapshot.damage.full_rebuild);
        assert!(snapshot.damage.dirty_rows.is_empty());
    }

    #[test]
    fn terminal_modes_start_with_expected_defaults() {
        let buffer = TerminalBuffer::new(3, 2);
        let modes = buffer.modes();

        assert!(modes.cursor_visible);
        assert!(!modes.bracketed_paste);
        assert!(!modes.application_cursor);
        assert!(!modes.origin_mode);
    }

    #[test]
    fn dec_cursor_visibility_controls_rendered_cursor() {
        let mut buffer = TerminalBuffer::new(3, 2);
        buffer.push_bytes(b"a");
        assert_eq!(
            buffer.render_snapshot(true).cursor,
            Some(CursorState {
                row: 0,
                col: 1,
                visible: true,
            })
        );

        buffer.push_bytes(b"\x1b[?25l");
        assert_eq!(
            buffer.render_snapshot(true).cursor,
            Some(CursorState {
                row: 0,
                col: 1,
                visible: false,
            })
        );

        buffer.push_bytes(b"\x1b[?25h");
        assert_eq!(
            buffer.render_snapshot(true).cursor,
            Some(CursorState {
                row: 0,
                col: 1,
                visible: true,
            })
        );
    }

    #[test]
    fn dec_bracketed_paste_mode_updates_terminal_modes() {
        let mut buffer = TerminalBuffer::new(3, 2);
        assert!(!buffer.modes().bracketed_paste);

        buffer.push_bytes(b"\x1b[?2004h");
        assert!(buffer.modes().bracketed_paste);

        buffer.push_bytes(b"\x1b[?2004l");
        assert!(!buffer.modes().bracketed_paste);
    }

    #[test]
    fn dec_input_modes_update_terminal_modes() {
        let mut buffer = TerminalBuffer::new(3, 2);
        assert!(!buffer.modes().application_cursor);
        assert!(!buffer.modes().origin_mode);

        buffer.push_bytes(b"\x1b[?1h\x1b[?6h");
        assert!(buffer.modes().application_cursor);
        assert!(buffer.modes().origin_mode);

        buffer.push_bytes(b"\x1b[?1l\x1b[?6l");
        assert!(!buffer.modes().application_cursor);
        assert!(!buffer.modes().origin_mode);
    }
}
