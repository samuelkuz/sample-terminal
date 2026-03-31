const OUTER_PADDING_X: f64 = 26.0;
const OUTER_PADDING_Y: f64 = 24.0;
const HEADER_HEIGHT: f64 = 36.0;
const GRID_PADDING_X: f64 = 18.0;
const GRID_PADDING_Y: f64 = 18.0;
const CELL_WIDTH: f64 = 16.0;
const CELL_HEIGHT: f64 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Color(pub(crate) [f32; 4]);

impl Color {
    pub(crate) const fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self([red, green, blue, alpha])
    }
}

const CHROME_BACKGROUND: Color = Color::rgba(0.08, 0.09, 0.12, 1.0);
const CHROME_BORDER: Color = Color::rgba(0.16, 0.18, 0.23, 1.0);
const CHROME_HEADER: Color = Color::rgba(0.12, 0.14, 0.19, 1.0);
const GRID_BACKGROUND: Color = Color::rgba(0.05, 0.07, 0.10, 1.0);
const EMPTY_CELL_FILL: Color = Color::rgba(0.13, 0.16, 0.20, 1.0);
const CURSOR_FILL: Color = Color::rgba(0.95, 0.96, 0.98, 0.18);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
    pub flags: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: [0.92, 0.93, 0.95, 1.0],
            bg: EMPTY_CELL_FILL.0,
            flags: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorState {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderState {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    cursor: Option<CursorState>,
}

impl RenderState {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let len = cols as usize * rows as usize;
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); len],
            cursor: None,
        }
    }

    pub fn set_char(&mut self, row: u16, col: u16, ch: char) {
        if let Some(cell) = self.cell_mut(row, col) {
            cell.ch = ch;
        }
    }

    pub fn set_cell(&mut self, row: u16, col: u16, cell: Cell) {
        if let Some(slot) = self.cell_mut(row, col) {
            *slot = cell;
        }
    }

    pub fn set_cursor(&mut self, cursor: Option<CursorState>) {
        self.cursor = cursor;
    }

    pub fn cursor(&self) -> Option<CursorState> {
        self.cursor
    }

    pub fn char_at(&self, row: u16, col: u16) -> char {
        self.cell(row, col).map(|cell| cell.ch).unwrap_or(' ')
    }

    fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.index(row, col).and_then(|index| self.cells.get(index))
    }

    fn cell_mut(&mut self, row: u16, col: u16) -> Option<&mut Cell> {
        self.index(row, col)
            .and_then(move |index| self.cells.get_mut(index))
    }

    fn index(&self, row: u16, col: u16) -> Option<usize> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        Some(row as usize * self.cols as usize + col as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutMetrics {
    pub(crate) view_width: f32,
    pub(crate) view_height: f32,
    pub(crate) terminal_x: f32,
    pub(crate) terminal_y: f32,
    pub(crate) terminal_width: f32,
    pub(crate) terminal_height: f32,
    pub(crate) header_height: f32,
    pub(crate) content_x: f32,
    pub(crate) content_y: f32,
    pub(crate) content_width: f32,
    pub(crate) content_height: f32,
    pub(crate) cell_width: f32,
    pub(crate) cell_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) color: Color,
}

pub fn terminal_grid_size(view_width: f64, view_height: f64) -> (u16, u16) {
    let inner_width =
        (view_width - (OUTER_PADDING_X * 2.0) - (GRID_PADDING_X * 2.0)).max(CELL_WIDTH);
    let inner_height =
        (view_height - (OUTER_PADDING_Y * 2.0) - HEADER_HEIGHT - (GRID_PADDING_Y * 2.0))
            .max(CELL_HEIGHT);

    let cols = (inner_width / CELL_WIDTH).floor().max(8.0) as u16;
    let rows = (inner_height / CELL_HEIGHT).floor().max(6.0) as u16;
    (cols, rows)
}

pub(crate) fn layout_metrics(view_width: f64, view_height: f64, state: &RenderState) -> LayoutMetrics {
    let view_width = view_width.max(1.0) as f32;
    let view_height = view_height.max(1.0) as f32;
    let padding_x = OUTER_PADDING_X as f32;
    let padding_y = OUTER_PADDING_Y as f32;
    let header_height = HEADER_HEIGHT as f32;
    let grid_padding_x = GRID_PADDING_X as f32;
    let grid_padding_y = GRID_PADDING_Y as f32;
    let cell_width = CELL_WIDTH as f32;
    let cell_height = CELL_HEIGHT as f32;
    let terminal_x = padding_x;
    let terminal_y = padding_y;
    let terminal_width = (view_width - (padding_x * 2.0)).max(180.0);
    let terminal_height = (view_height - (padding_y * 2.0)).max(160.0);
    let content_x = terminal_x + grid_padding_x;
    let content_y = terminal_y + header_height + grid_padding_y;
    let content_width = state.cols as f32 * cell_width;
    let content_height = state.rows as f32 * cell_height;

    LayoutMetrics {
        view_width,
        view_height,
        terminal_x,
        terminal_y,
        terminal_width,
        terminal_height,
        header_height,
        content_x,
        content_y,
        content_width,
        content_height,
        cell_width,
        cell_height,
    }
}

pub(crate) fn build_scene_quads(metrics: LayoutMetrics, state: &RenderState) -> Vec<Quad> {
    let mut quads = Vec::new();
    let tile_width = (metrics.cell_width - 4.0).max(6.0);
    let tile_height = (metrics.cell_height - 4.0).max(8.0);

    quads.push(Quad {
        x: metrics.terminal_x,
        y: metrics.terminal_y,
        width: metrics.terminal_width,
        height: metrics.terminal_height,
        color: CHROME_BACKGROUND,
    });
    quads.push(Quad {
        x: metrics.terminal_x - 1.0,
        y: metrics.terminal_y - 1.0,
        width: metrics.terminal_width + 2.0,
        height: metrics.terminal_height + 2.0,
        color: CHROME_BORDER,
    });
    quads.push(Quad {
        x: metrics.terminal_x,
        y: metrics.terminal_y,
        width: metrics.terminal_width,
        height: metrics.header_height,
        color: CHROME_HEADER,
    });
    quads.push(Quad {
        x: metrics.content_x,
        y: metrics.content_y,
        width: metrics.content_width,
        height: metrics.content_height,
        color: GRID_BACKGROUND,
    });

    for row in 0..state.rows {
        for col in 0..state.cols {
            let cell = state.cells[state.index(row, col).expect("in-bounds cell index")];
            let x = metrics.content_x + (col as f32 * metrics.cell_width) + 2.0;
            let y = metrics.content_y + (row as f32 * metrics.cell_height) + 2.0;

            quads.push(Quad {
                x,
                y,
                width: tile_width,
                height: tile_height,
                color: Color(cell.bg),
            });

            if cell.ch != ' ' {
                quads.push(Quad {
                    x: x + 1.0,
                    y: y + 1.0,
                    width: tile_width - 2.0,
                    height: tile_height - 2.0,
                    color: demo_color_for_char(cell.ch),
                });
            }
        }
    }

    if let Some(cursor) = state.cursor.filter(|cursor| cursor.visible) {
        let cursor_col = cursor.col.min(state.cols.saturating_sub(1));
        let cursor_row = cursor.row.min(state.rows.saturating_sub(1));
        quads.push(Quad {
            x: metrics.content_x + (cursor_col as f32 * metrics.cell_width) + 1.0,
            y: metrics.content_y + (cursor_row as f32 * metrics.cell_height) + 1.0,
            width: metrics.cell_width - 2.0,
            height: metrics.cell_height - 2.0,
            color: CURSOR_FILL,
        });
    }

    quads
}

fn demo_color_for_char(ch: char) -> Color {
    match ch as usize % 6 {
        0 => Color::rgba(0.29, 0.58, 0.78, 1.0),
        1 => Color::rgba(0.34, 0.74, 0.50, 1.0),
        2 => Color::rgba(0.92, 0.68, 0.26, 1.0),
        3 => Color::rgba(0.56, 0.66, 0.82, 1.0),
        4 => Color::rgba(0.72, 0.56, 0.82, 1.0),
        _ => Color::rgba(0.88, 0.44, 0.37, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::{CursorState, RenderState, build_scene_quads, layout_metrics, terminal_grid_size};

    #[test]
    fn grid_size_has_minimums() {
        assert_eq!(terminal_grid_size(120.0, 80.0), (8, 6));
    }

    #[test]
    fn grid_size_scales_with_viewport() {
        assert_eq!(terminal_grid_size(900.0, 640.0), (50, 21));
    }

    #[test]
    fn empty_scene_contains_only_frame_and_cell_backgrounds() {
        let state = RenderState::new(2, 2);
        let metrics = layout_metrics(300.0, 240.0, &state);
        let quads = build_scene_quads(metrics, &state);

        assert_eq!(quads.len(), 8);
    }

    #[test]
    fn non_empty_cells_and_cursor_add_visible_quads() {
        let mut state = RenderState::new(2, 2);
        state.set_char(0, 0, 'A');
        state.set_char(1, 1, 'B');
        state.set_cursor(Some(CursorState {
            row: 1,
            col: 0,
            visible: true,
        }));

        let metrics = layout_metrics(300.0, 240.0, &state);
        let quads = build_scene_quads(metrics, &state);

        assert_eq!(quads.len(), 11);
    }
}
