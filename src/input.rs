use objc2_app_kit::{
    NSDeleteFunctionKey, NSDownArrowFunctionKey, NSEndFunctionKey, NSF10FunctionKey,
    NSF11FunctionKey, NSF12FunctionKey, NSF1FunctionKey, NSF2FunctionKey, NSF3FunctionKey,
    NSF4FunctionKey, NSF5FunctionKey, NSF6FunctionKey, NSF7FunctionKey, NSF8FunctionKey,
    NSF9FunctionKey, NSHomeFunctionKey, NSLeftArrowFunctionKey, NSPageDownFunctionKey,
    NSPageUpFunctionKey, NSRightArrowFunctionKey, NSUpArrowFunctionKey,
};

pub enum SelectionPhase {
    Start,
    Update,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputModifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

pub fn encode_paste(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    if text.is_empty() {
        return None;
    }

    let mut bytes = Vec::with_capacity(text.len() + 16);
    if bracketed {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend_from_slice(text.as_bytes());
    if bracketed {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    Some(bytes)
}

pub fn translate_terminal_input(
    text: &str,
    modifiers: InputModifiers,
    application_cursor: bool,
) -> Option<Vec<u8>> {
    if text.is_empty() {
        return None;
    }

    if text.chars().count() == 1 {
        let ch = text.chars().next().unwrap_or_default() as u32;
        let sequence = match ch {
            value if value == NSUpArrowFunctionKey => Some(if application_cursor {
                b"\x1bOA".to_vec()
            } else {
                b"\x1b[A".to_vec()
            }),
            value if value == NSDownArrowFunctionKey => Some(if application_cursor {
                b"\x1bOB".to_vec()
            } else {
                b"\x1b[B".to_vec()
            }),
            value if value == NSRightArrowFunctionKey => Some(if application_cursor {
                b"\x1bOC".to_vec()
            } else {
                b"\x1b[C".to_vec()
            }),
            value if value == NSLeftArrowFunctionKey => Some(if application_cursor {
                b"\x1bOD".to_vec()
            } else {
                b"\x1b[D".to_vec()
            }),
            value if value == NSHomeFunctionKey => Some(if application_cursor {
                b"\x1bOH".to_vec()
            } else {
                b"\x1b[H".to_vec()
            }),
            value if value == NSEndFunctionKey => Some(if application_cursor {
                b"\x1bOF".to_vec()
            } else {
                b"\x1b[F".to_vec()
            }),
            value if value == NSPageUpFunctionKey => Some(b"\x1b[5~".to_vec()),
            value if value == NSPageDownFunctionKey => Some(b"\x1b[6~".to_vec()),
            value if value == NSDeleteFunctionKey => Some(b"\x1b[3~".to_vec()),
            value if value == NSF1FunctionKey => Some(b"\x1bOP".to_vec()),
            value if value == NSF2FunctionKey => Some(b"\x1bOQ".to_vec()),
            value if value == NSF3FunctionKey => Some(b"\x1bOR".to_vec()),
            value if value == NSF4FunctionKey => Some(b"\x1bOS".to_vec()),
            value if value == NSF5FunctionKey => Some(b"\x1b[15~".to_vec()),
            value if value == NSF6FunctionKey => Some(b"\x1b[17~".to_vec()),
            value if value == NSF7FunctionKey => Some(b"\x1b[18~".to_vec()),
            value if value == NSF8FunctionKey => Some(b"\x1b[19~".to_vec()),
            value if value == NSF9FunctionKey => Some(b"\x1b[20~".to_vec()),
            value if value == NSF10FunctionKey => Some(b"\x1b[21~".to_vec()),
            value if value == NSF11FunctionKey => Some(b"\x1b[23~".to_vec()),
            value if value == NSF12FunctionKey => Some(b"\x1b[24~".to_vec()),
            _ => None,
        };
        if sequence.is_some() {
            return sequence;
        }
    }

    let _ = modifiers;
    Some(text.as_bytes().to_vec())
}

pub fn reduce_selection_phase(
    anchor: Option<(u16, u16)>,
    focus: Option<(u16, u16)>,
    dragging: bool,
    phase: SelectionPhase,
    cell: Option<(u16, u16)>,
) -> (Option<(u16, u16)>, Option<(u16, u16)>, bool) {
    match phase {
        SelectionPhase::Start => (cell, cell, cell.is_some()),
        SelectionPhase::Update => {
            if dragging {
                (anchor, cell.or(focus), dragging)
            } else {
                (anchor, focus, dragging)
            }
        }
        SelectionPhase::End => {
            if dragging {
                (anchor, cell.or(focus), false)
            } else {
                (anchor, focus, false)
            }
        }
    }
}

pub fn normalize_scroll_lines(raw_delta: f64, precise: bool) -> i32 {
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
    use super::{InputModifiers, encode_paste, translate_terminal_input};

    #[test]
    fn translates_arrow_keys_to_terminal_sequences() {
        assert_eq!(
            translate_terminal_input("\u{f700}", InputModifiers::default(), false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f701}", InputModifiers::default(), false),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f702}", InputModifiers::default(), false),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f703}", InputModifiers::default(), false),
            Some(b"\x1b[C".to_vec())
        );
    }

    #[test]
    fn preserves_plain_text_input() {
        assert_eq!(
            translate_terminal_input("abc", InputModifiers::default(), false),
            Some(b"abc".to_vec())
        );
        assert_eq!(translate_terminal_input("", InputModifiers::default(), false), None);
    }

    #[test]
    fn wraps_paste_only_when_bracketed_mode_is_enabled() {
        assert_eq!(encode_paste("hello", false), Some(b"hello".to_vec()));
        assert_eq!(
            encode_paste("hello", true),
            Some(b"\x1b[200~hello\x1b[201~".to_vec())
        );
        assert_eq!(encode_paste("", true), None);
    }

    #[test]
    fn translates_navigation_and_function_keys() {
        assert_eq!(
            translate_terminal_input("\u{f729}", InputModifiers::default(), false),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f72b}", InputModifiers::default(), false),
            Some(b"\x1b[F".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f72c}", InputModifiers::default(), false),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f72d}", InputModifiers::default(), false),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f728}", InputModifiers::default(), false),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f704}", InputModifiers::default(), false),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f70f}", InputModifiers::default(), false),
            Some(b"\x1b[24~".to_vec())
        );
    }

    #[test]
    fn application_cursor_mode_changes_arrow_and_home_end_sequences() {
        assert_eq!(
            translate_terminal_input("\u{f700}", InputModifiers::default(), true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f729}", InputModifiers::default(), true),
            Some(b"\x1bOH".to_vec())
        );
        assert_eq!(
            translate_terminal_input("\u{f72b}", InputModifiers::default(), true),
            Some(b"\x1bOF".to_vec())
        );
    }
}
