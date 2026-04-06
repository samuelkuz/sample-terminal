const OUTER_PADDING_X: f64 = 26.0;
const OUTER_PADDING_Y: f64 = 24.0;
const HEADER_HEIGHT: f64 = 36.0;
const GRID_PADDING_X: f64 = 18.0;
const GRID_PADDING_Y: f64 = 18.0;
pub(crate) const CELL_WIDTH: f64 = 12.0;
pub(crate) const CELL_HEIGHT: f64 = 20.0;

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

pub(crate) fn layout_metrics(
    view_width: f64,
    view_height: f64,
    cols: u16,
    rows: u16,
) -> LayoutMetrics {
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
    let content_width = cols as f32 * cell_width;
    let content_height = rows as f32 * cell_height;

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

pub fn point_to_cell(
    view_width: f64,
    view_height: f64,
    cols: u16,
    rows: u16,
    point_x: f64,
    point_y: f64,
) -> Option<(u16, u16)> {
    let metrics = layout_metrics(view_width, view_height, cols, rows);
    if point_x < metrics.content_x as f64
        || point_y < metrics.content_y as f64
        || point_x >= (metrics.content_x + metrics.content_width) as f64
        || point_y >= (metrics.content_y + metrics.content_height) as f64
    {
        return None;
    }

    let col = ((point_x as f32 - metrics.content_x) / metrics.cell_width).floor() as u16;
    let row = ((point_y as f32 - metrics.content_y) / metrics.cell_height).floor() as u16;
    if row < rows && col < cols {
        Some((row, col))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{layout_metrics, point_to_cell, terminal_grid_size};

    #[test]
    fn grid_size_has_minimums() {
        assert_eq!(terminal_grid_size(120.0, 80.0), (8, 6));
    }

    #[test]
    fn grid_size_scales_with_viewport() {
        assert_eq!(terminal_grid_size(900.0, 640.0), (67, 26));
    }

    #[test]
    fn point_mapping_uses_content_grid() {
        let (cols, rows) = (10, 5);
        let metrics = layout_metrics(500.0, 300.0, cols, rows);
        let point = point_to_cell(
            500.0,
            300.0,
            cols,
            rows,
            metrics.content_x as f64 + 1.0,
            metrics.content_y as f64 + 1.0,
        );
        assert_eq!(point, Some((0, 0)));
    }
}
