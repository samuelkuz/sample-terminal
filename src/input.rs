use objc2_app_kit::{
    NSDownArrowFunctionKey, NSLeftArrowFunctionKey, NSRightArrowFunctionKey,
    NSUpArrowFunctionKey,
};

use crate::app_state::SelectionState;

pub enum SelectionPhase {
    Start,
    Update,
    End,
}

pub fn terminal_input_bytes(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() {
        return None;
    }

    if text.chars().count() == 1 {
        let ch = text.chars().next().unwrap_or_default() as u32;
        let sequence = match ch {
            value if value == NSUpArrowFunctionKey => Some(b"\x1b[A".to_vec()),
            value if value == NSDownArrowFunctionKey => Some(b"\x1b[B".to_vec()),
            value if value == NSRightArrowFunctionKey => Some(b"\x1b[C".to_vec()),
            value if value == NSLeftArrowFunctionKey => Some(b"\x1b[D".to_vec()),
            _ => None,
        };
        if sequence.is_some() {
            return sequence;
        }
    }

    Some(text.as_bytes().to_vec())
}

pub fn apply_selection_gesture(
    selection: &mut SelectionState,
    phase: SelectionPhase,
    cell: Option<(u16, u16)>,
) {
    match phase {
        SelectionPhase::Start => {
            selection.anchor = cell;
            selection.focus = cell;
            selection.dragging = cell.is_some();
        }
        SelectionPhase::Update => {
            if selection.dragging {
                selection.focus = cell.or(selection.focus);
            }
        }
        SelectionPhase::End => {
            if selection.dragging {
                selection.focus = cell.or(selection.focus);
            }
            selection.dragging = false;
        }
    }
}

pub fn normalized_scroll_lines(raw_delta: f64, precise: bool) -> i32 {
    if raw_delta == 0.0 {
        return 0;
    }

    if precise {
        let scaled = (raw_delta / 24.0).round() as i32;
        if scaled == 0 {
            raw_delta.signum() as i32
        } else {
            scaled
        }
    } else {
        raw_delta.round() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_input_bytes;

    #[test]
    fn translates_arrow_keys_to_terminal_sequences() {
        assert_eq!(terminal_input_bytes("\u{f700}"), Some(b"\x1b[A".to_vec()));
        assert_eq!(terminal_input_bytes("\u{f701}"), Some(b"\x1b[B".to_vec()));
        assert_eq!(terminal_input_bytes("\u{f702}"), Some(b"\x1b[D".to_vec()));
        assert_eq!(terminal_input_bytes("\u{f703}"), Some(b"\x1b[C".to_vec()));
    }

    #[test]
    fn preserves_plain_text_input() {
        assert_eq!(terminal_input_bytes("abc"), Some(b"abc".to_vec()));
        assert_eq!(terminal_input_bytes(""), None);
    }
}
