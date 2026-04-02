use std::collections::BTreeSet;

use crate::layout::LayoutMetrics;
use crate::renderer::atlas::GlyphAtlas;

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
const SELECTION_FILL: Color = Color::rgba(0.46, 0.64, 0.92, 0.30);

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveScreen {
    Primary,
    Alternate,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RenderDamage {
    pub full_rebuild: bool,
    pub dirty_rows: BTreeSet<u16>,
    pub selection_dirty: bool,
    pub cursor_dirty: bool,
    pub global_dirty: bool,
}

impl RenderDamage {}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    pub cursor: Option<CursorState>,
    pub active_screen: ActiveScreen,
    pub damage: RenderDamage,
}

impl RenderSnapshot {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols as usize * rows as usize],
            cursor: None,
            active_screen: ActiveScreen::Primary,
            damage: RenderDamage {
                full_rebuild: true,
                ..RenderDamage::default()
            },
        }
    }

    pub fn set_cell(&mut self, row: u16, col: u16, cell: Cell) {
        if let Some(index) = self.index(row, col) {
            self.cells[index] = cell;
        }
    }

    pub fn set_char(&mut self, row: u16, col: u16, ch: char) {
        if let Some(index) = self.index(row, col) {
            self.cells[index].ch = ch;
        }
    }

    pub fn set_cursor(&mut self, cursor: Option<CursorState>) {
        self.cursor = cursor;
    }

    pub fn set_active_screen(&mut self, active_screen: ActiveScreen) {
        self.active_screen = active_screen;
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.index(row, col).and_then(|index| self.cells.get(index))
    }

    pub fn char_at(&self, row: u16, col: u16) -> char {
        self.cell(row, col).map(|cell| cell.ch).unwrap_or(' ')
    }

    fn index(&self, row: u16, col: u16) -> Option<usize> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        Some(row as usize * self.cols as usize + col as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
}

impl SelectionRange {
    pub fn normalized(self) -> Self {
        if (self.start_row, self.start_col) <= (self.end_row, self.end_col) {
            self
        } else {
            Self {
                start_row: self.end_row,
                start_col: self.end_col,
                end_row: self.start_row,
                end_col: self.start_col,
            }
        }
    }

    pub fn contains(self, row: u16, col: u16) -> bool {
        let normalized = self.normalized();
        if row < normalized.start_row || row > normalized.end_row {
            return false;
        }
        if normalized.start_row == normalized.end_row {
            return row == normalized.start_row
                && col >= normalized.start_col
                && col <= normalized.end_col;
        }
        if row == normalized.start_row {
            return col >= normalized.start_col;
        }
        if row == normalized.end_row {
            return col <= normalized.end_col;
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) color: Color,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextInstance {
    pub(crate) origin: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) uv_origin: [f32; 2],
    pub(crate) uv_size: [f32; 2],
    pub(crate) color: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RowGeometry {
    pub(crate) background_quads: Vec<Quad>,
    pub(crate) text_instances: Vec<TextInstance>,
    pub(crate) overlay_quads: Vec<Quad>,
}

pub(crate) fn build_chrome_quads(metrics: LayoutMetrics) -> Vec<Quad> {
    vec![
        Quad {
            x: metrics.terminal_x,
            y: metrics.terminal_y,
            width: metrics.terminal_width,
            height: metrics.terminal_height,
            color: CHROME_BACKGROUND,
        },
        Quad {
            x: metrics.terminal_x - 1.0,
            y: metrics.terminal_y - 1.0,
            width: metrics.terminal_width + 2.0,
            height: metrics.terminal_height + 2.0,
            color: CHROME_BORDER,
        },
        Quad {
            x: metrics.terminal_x,
            y: metrics.terminal_y,
            width: metrics.terminal_width,
            height: metrics.header_height,
            color: CHROME_HEADER,
        },
        Quad {
            x: metrics.content_x,
            y: metrics.content_y,
            width: metrics.content_width,
            height: metrics.content_height,
            color: GRID_BACKGROUND,
        },
    ]
}

pub(crate) fn build_row_geometry(
    metrics: LayoutMetrics,
    snapshot: &RenderSnapshot,
    atlas: &GlyphAtlas,
    row: u16,
    selection: Option<SelectionRange>,
) -> RowGeometry {
    let mut geometry = RowGeometry::default();
    let tile_width = (metrics.cell_width - 4.0).max(6.0);
    let tile_height = (metrics.cell_height - 4.0).max(8.0);

    for col in 0..snapshot.cols {
        let cell = snapshot
            .cell(row, col)
            .copied()
            .unwrap_or_else(Cell::default);
        let x = metrics.content_x + (col as f32 * metrics.cell_width) + 2.0;
        let y = metrics.content_y + (row as f32 * metrics.cell_height) + 2.0;

        geometry.background_quads.push(Quad {
            x,
            y,
            width: tile_width,
            height: tile_height,
            color: Color(cell.bg),
        });

        if let Some(selection) = selection.filter(|selection| selection.contains(row, col)) {
            let _ = selection;
            geometry.overlay_quads.push(Quad {
                x: metrics.content_x + (col as f32 * metrics.cell_width),
                y: metrics.content_y + (row as f32 * metrics.cell_height),
                width: metrics.cell_width,
                height: metrics.cell_height,
                color: SELECTION_FILL,
            });
        }

        if cell.ch != ' ' {
            let glyph = atlas.glyph_for(cell.ch);
            if glyph.bitmap_size[0] > 0.0 && glyph.bitmap_size[1] > 0.0 {
                geometry.text_instances.push(TextInstance {
                    origin: [
                        metrics.content_x + (col as f32 * metrics.cell_width) + glyph.offset[0],
                        metrics.content_y + (row as f32 * metrics.cell_height) + glyph.offset[1],
                    ],
                    size: glyph.bitmap_size,
                    uv_origin: glyph.uv_origin,
                    uv_size: glyph.uv_size,
                    color: cell.fg,
                });
            }
        }
    }

    geometry
}

pub(crate) fn build_cursor_quad(
    metrics: LayoutMetrics,
    snapshot: &RenderSnapshot,
    cursor_visible: bool,
) -> Option<Quad> {
    let cursor = snapshot
        .cursor
        .filter(|cursor| cursor.visible && cursor_visible)?;
    let cursor_col = cursor.col.min(snapshot.cols.saturating_sub(1));
    let cursor_row = cursor.row.min(snapshot.rows.saturating_sub(1));
    Some(Quad {
        x: metrics.content_x + (cursor_col as f32 * metrics.cell_width) + 1.0,
        y: metrics.content_y + (cursor_row as f32 * metrics.cell_height) + 1.0,
        width: metrics.cell_width - 2.0,
        height: metrics.cell_height - 2.0,
        color: CURSOR_FILL,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::layout::layout_metrics;
    use objc2_metal::MTLCreateSystemDefaultDevice;

    use crate::renderer::atlas::GlyphAtlas;

    use super::{
        CursorState, RenderDamage, RenderSnapshot, SelectionRange, build_chrome_quads,
        build_cursor_quad, build_row_geometry,
    };

    fn atlas() -> GlyphAtlas {
        let device = MTLCreateSystemDefaultDevice().expect("default metal device");
        GlyphAtlas::new(&device).expect("glyph atlas")
    }

    #[test]
    fn chrome_contains_frame_quads() {
        let metrics = layout_metrics(300.0, 240.0, 2, 2);
        assert_eq!(build_chrome_quads(metrics).len(), 4);
    }

    #[test]
    fn row_geometry_emits_text_and_selection() {
        let mut snapshot = RenderSnapshot::new(2, 2);
        snapshot.set_char(0, 0, 'A');
        let metrics = layout_metrics(300.0, 240.0, snapshot.cols, snapshot.rows);
        let geometry = build_row_geometry(
            metrics,
            &snapshot,
            &atlas(),
            0,
            Some(SelectionRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 0,
            }),
        );

        assert_eq!(geometry.background_quads.len(), 2);
        assert_eq!(geometry.text_instances.len(), 1);
        assert_eq!(geometry.overlay_quads.len(), 1);
    }

    #[test]
    fn cursor_quad_respects_visibility() {
        let mut snapshot = RenderSnapshot::new(2, 2);
        snapshot.set_cursor(Some(CursorState {
            row: 1,
            col: 1,
            visible: true,
        }));
        let metrics = layout_metrics(300.0, 240.0, snapshot.cols, snapshot.rows);
        assert!(build_cursor_quad(metrics, &snapshot, true).is_some());
        assert!(build_cursor_quad(metrics, &snapshot, false).is_none());
    }

    #[test]
    fn selection_normalization_contains_rows() {
        let selection = SelectionRange {
            start_row: 2,
            start_col: 4,
            end_row: 1,
            end_col: 1,
        }
        .normalized();
        assert!(selection.contains(1, 1));
        assert!(selection.contains(2, 4));
        assert!(selection.contains(2, 0));
        assert!(!selection.contains(0, 0));
    }

    #[test]
    fn damage_default_is_empty() {
        assert_eq!(
            RenderDamage::default(),
            RenderDamage {
                full_rebuild: false,
                dirty_rows: BTreeSet::new(),
                selection_dirty: false,
                cursor_dirty: false,
                global_dirty: false,
            }
        );
    }
}
