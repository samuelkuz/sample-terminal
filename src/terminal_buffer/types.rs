use crate::renderer::Cell;

pub(crate) const DEFAULT_FG: [f32; 4] = [0.92, 0.93, 0.95, 1.0];
pub(crate) const DEFAULT_BG: [f32; 4] = [0.13, 0.16, 0.20, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalCell {
    pub(crate) ch: char,
    pub(crate) fg: [f32; 4],
    pub(crate) bg: [f32; 4],
    pub(crate) flags: u8,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            flags: 0,
        }
    }
}

impl From<TerminalCell> for Cell {
    fn from(value: TerminalCell) -> Self {
        Self {
            ch: value.ch,
            fg: value.fg,
            bg: value.bg,
            flags: value.flags,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellAttributes {
    pub(crate) fg: [f32; 4],
    pub(crate) bg: [f32; 4],
    pub(crate) flags: u8,
}

impl Default for CellAttributes {
    fn default() -> Self {
        Self {
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            flags: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParserState {
    Ground,
    Escape,
    Csi(Vec<u8>),
    Osc,
    OscEsc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalModes {
    pub(crate) cursor_visible: bool,
    pub(crate) bracketed_paste: bool,
    pub(crate) application_cursor: bool,
    pub(crate) origin_mode: bool,
}

impl Default for TerminalModes {
    fn default() -> Self {
        Self {
            cursor_visible: true,
            bracketed_paste: false,
            application_cursor: false,
            origin_mode: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ScreenBuffer {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) cells: Vec<TerminalCell>,
    pub(crate) cursor_row: u16,
    pub(crate) cursor_col: u16,
    pub(crate) saved_cursor: Option<(u16, u16)>,
    pub(crate) wrap_pending: bool,
    pub(crate) scroll_top: u16,
    pub(crate) scroll_bottom: u16,
}

impl ScreenBuffer {
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
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
        }
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
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

    pub(crate) fn clear(&mut self) {
        self.cells.fill(TerminalCell::default());
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.saved_cursor = None;
        self.wrap_pending = false;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
    }

    pub(crate) fn index(&self, row: u16, col: u16) -> usize {
        row as usize * self.cols as usize + col as usize
    }

    pub(crate) fn cell(&self, row: u16, col: u16) -> TerminalCell {
        self.cells[self.index(row, col)]
    }

    pub(crate) fn set_cell(&mut self, row: u16, col: u16, cell: TerminalCell) {
        let index = self.index(row, col);
        self.cells[index] = cell;
    }

    pub(crate) fn clear_row(&mut self, row: u16) {
        for col in 0..self.cols {
            self.set_cell(row, col, TerminalCell::default());
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedCsi<'a> {
    pub(crate) private: bool,
    pub(crate) params: &'a [Option<usize>],
}
