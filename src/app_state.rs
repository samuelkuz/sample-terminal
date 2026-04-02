use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::input::{SelectionPhase, reduce_selection_phase};
use crate::layout::terminal_grid_size;
use crate::renderer::SelectionRange;
use crate::session::TerminalSession;
use crate::terminal_buffer::TerminalBuffer;

pub struct AppState {
    session: TerminalSession,
    buffer: Mutex<TerminalBuffer>,
    activity_counter: Mutex<u64>,
    last_winsize: Mutex<Option<(u16, u16, u16, u16)>>,
    blink: Mutex<BlinkState>,
    selection: Mutex<SelectionState>,
}

impl AppState {
    pub fn new_for_window(view_width: f64, view_height: f64) -> Result<Self, String> {
        let (cols, rows) = terminal_grid_size(view_width, view_height);
        Self::new(cols, rows)
    }

    pub fn new(cols: u16, rows: u16) -> Result<Self, String> {
        Ok(Self {
            session: TerminalSession::spawn()?,
            buffer: Mutex::new(TerminalBuffer::new(cols, rows)),
            activity_counter: Mutex::new(0),
            last_winsize: Mutex::new(None),
            blink: Mutex::new(BlinkState::new()),
            selection: Mutex::new(SelectionState::default()),
        })
    }

    pub fn poll_session(&self) -> bool {
        let chunks = self.session.try_read();
        if chunks.is_empty() {
            return false;
        }

        let total_bytes = chunks.iter().map(|chunk| chunk.len() as u64).sum::<u64>();
        if let Ok(mut counter) = self.activity_counter.lock() {
            *counter = counter.saturating_add(total_bytes.max(1));
        }

        let Ok(mut buffer) = self.buffer.lock() else {
            return false;
        };

        for chunk in chunks {
            buffer.push_bytes(&chunk);
        }

        true
    }

    pub fn poll_session_and_should_render(&self) -> bool {
        let did_receive_output = self.poll_session();
        let activity = self
            .activity_counter
            .lock()
            .map(|counter| *counter)
            .unwrap_or(0);

        self.blink
            .lock()
            .map(|mut blink| {
                let activity_changed = blink.sync_activity(activity);
                let blink_changed = blink.tick();
                did_receive_output || activity_changed || blink_changed
            })
            .unwrap_or(did_receive_output)
    }

    pub fn send_input(&self, bytes: &[u8]) {
        if let Ok(mut counter) = self.activity_counter.lock() {
            *counter = counter.saturating_add(bytes.len() as u64 + 1);
        }
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.set_viewport_offset(0);
        }
        if let Ok(mut selection) = self.selection.lock() {
            *selection = SelectionState::default();
        }
        self.session.write_input(bytes);
    }

    pub fn sync_window_size(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        let Ok(mut winsize) = self.last_winsize.lock() else {
            return;
        };

        let next = (cols, rows, pixel_width, pixel_height);
        if winsize.as_ref() == Some(&next) {
            return;
        }

        self.session.resize(rows, cols, pixel_width, pixel_height);
        *winsize = Some(next);
    }

    pub fn cursor_visible(&self) -> bool {
        let activity = self
            .activity_counter
            .lock()
            .map(|counter| *counter)
            .unwrap_or(0);
        self.blink
            .lock()
            .map(|mut blink| {
                let _ = blink.sync_activity(activity);
                blink.visible()
            })
            .unwrap_or(true)
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.buffer
            .lock()
            .map(|buffer| buffer.modes().bracketed_paste)
            .unwrap_or(false)
    }

    pub fn application_cursor_enabled(&self) -> bool {
        self.buffer
            .lock()
            .map(|buffer| buffer.modes().application_cursor)
            .unwrap_or(false)
    }

    pub fn selection_range(&self) -> Option<SelectionRange> {
        self.selection
            .lock()
            .ok()
            .and_then(|selection| selection.range())
    }

    pub fn update_selection(&self, phase: SelectionPhase, cell: Option<(u16, u16)>) {
        if let Ok(mut selection) = self.selection.lock() {
            (selection.anchor, selection.focus, selection.dragging) = reduce_selection_phase(
                selection.anchor,
                selection.focus,
                selection.dragging,
                phase,
                cell,
            );
        }
    }

    pub fn scroll_viewport(&self, lines: i32) {
        if lines == 0 {
            return;
        }

        let Ok(mut buffer) = self.buffer.lock() else {
            return;
        };
        let current = buffer.viewport_offset() as i32;
        let max_offset = buffer.scrollback_len() as i32;
        let next = (current + lines).clamp(0, max_offset) as usize;
        buffer.set_viewport_offset(next);
    }

    pub fn stop_selection_drag(&self) {
        if let Ok(mut selection) = self.selection.lock() {
            selection.dragging = false;
        }
    }

    pub fn render_snapshot(
        &self,
        terminal_cols: u16,
        terminal_rows: u16,
        cursor_visible: bool,
    ) -> crate::renderer::RenderSnapshot {
        self.buffer
            .lock()
            .map(|mut buffer| {
                buffer.resize(terminal_cols, terminal_rows);
                buffer.render_snapshot(cursor_visible)
            })
            .unwrap_or_else(|_| crate::renderer::RenderSnapshot::new(terminal_cols, terminal_rows))
    }
}

struct BlinkState {
    visible: bool,
    last_toggle: Instant,
    last_activity: u64,
}

impl BlinkState {
    fn new() -> Self {
        Self {
            visible: true,
            last_toggle: Instant::now(),
            last_activity: 0,
        }
    }

    fn sync_activity(&mut self, activity: u64) -> bool {
        if activity != self.last_activity {
            self.last_activity = activity;
            let changed = !self.visible;
            self.visible = true;
            self.last_toggle = Instant::now();
            return changed;
        }
        false
    }

    fn tick(&mut self) -> bool {
        if self.last_toggle.elapsed() >= Duration::from_millis(600) {
            self.visible = !self.visible;
            self.last_toggle = Instant::now();
            return true;
        }
        false
    }

    fn visible(&self) -> bool {
        self.visible
    }
}

#[derive(Default)]
pub struct SelectionState {
    pub(crate) anchor: Option<(u16, u16)>,
    pub(crate) focus: Option<(u16, u16)>,
    pub(crate) dragging: bool,
}

impl SelectionState {
    fn range(&self) -> Option<SelectionRange> {
        let (start_row, start_col) = self.anchor?;
        let (end_row, end_col) = self.focus?;
        Some(SelectionRange {
            start_row,
            start_col,
            end_row,
            end_col,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::BlinkState;

    #[test]
    fn blink_resets_to_visible_on_activity() {
        let mut blink = BlinkState::new();
        blink.visible = false;

        assert!(blink.sync_activity(1));
        assert!(blink.visible());
        assert!(!blink.sync_activity(1));
    }

    #[test]
    fn blink_tick_toggles_after_interval() {
        let mut blink = BlinkState::new();
        blink.last_toggle = Instant::now() - Duration::from_millis(601);

        assert!(blink.tick());
        assert!(!blink.visible());
        assert!(!blink.tick());
    }
}
