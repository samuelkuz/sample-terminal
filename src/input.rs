use objc2_app_kit::{
    NSDeleteFunctionKey, NSDownArrowFunctionKey, NSEndFunctionKey, NSF1FunctionKey,
    NSF2FunctionKey, NSF3FunctionKey, NSF4FunctionKey, NSF5FunctionKey, NSF6FunctionKey,
    NSF7FunctionKey, NSF8FunctionKey, NSF9FunctionKey, NSF10FunctionKey, NSF11FunctionKey,
    NSF12FunctionKey, NSHomeFunctionKey, NSLeftArrowFunctionKey, NSPageDownFunctionKey,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecialKey {
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    F(u8),
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
        let sequence = match special_key(ch) {
            Some(key) => Some(encode_special_key(key, modifiers, application_cursor)),
            _ => None,
        };
        if sequence.is_some() {
            return sequence;
        }
    }

    let _ = modifiers;
    Some(text.as_bytes().to_vec())
}

fn special_key(ch: u32) -> Option<SpecialKey> {
    match ch {
        value if value == NSUpArrowFunctionKey => Some(SpecialKey::ArrowUp),
        value if value == NSDownArrowFunctionKey => Some(SpecialKey::ArrowDown),
        value if value == NSRightArrowFunctionKey => Some(SpecialKey::ArrowRight),
        value if value == NSLeftArrowFunctionKey => Some(SpecialKey::ArrowLeft),
        value if value == NSHomeFunctionKey => Some(SpecialKey::Home),
        value if value == NSEndFunctionKey => Some(SpecialKey::End),
        value if value == NSPageUpFunctionKey => Some(SpecialKey::PageUp),
        value if value == NSPageDownFunctionKey => Some(SpecialKey::PageDown),
        value if value == NSDeleteFunctionKey => Some(SpecialKey::Delete),
        value if value == NSF1FunctionKey => Some(SpecialKey::F(1)),
        value if value == NSF2FunctionKey => Some(SpecialKey::F(2)),
        value if value == NSF3FunctionKey => Some(SpecialKey::F(3)),
        value if value == NSF4FunctionKey => Some(SpecialKey::F(4)),
        value if value == NSF5FunctionKey => Some(SpecialKey::F(5)),
        value if value == NSF6FunctionKey => Some(SpecialKey::F(6)),
        value if value == NSF7FunctionKey => Some(SpecialKey::F(7)),
        value if value == NSF8FunctionKey => Some(SpecialKey::F(8)),
        value if value == NSF9FunctionKey => Some(SpecialKey::F(9)),
        value if value == NSF10FunctionKey => Some(SpecialKey::F(10)),
        value if value == NSF11FunctionKey => Some(SpecialKey::F(11)),
        value if value == NSF12FunctionKey => Some(SpecialKey::F(12)),
        _ => None,
    }
}

fn encode_special_key(
    key: SpecialKey,
    modifiers: InputModifiers,
    application_cursor: bool,
) -> Vec<u8> {
    let modifier = modifier_parameter(modifiers);
    match key {
        SpecialKey::ArrowUp => encode_cursor_key(b'A', modifier, application_cursor),
        SpecialKey::ArrowDown => encode_cursor_key(b'B', modifier, application_cursor),
        SpecialKey::ArrowRight => encode_cursor_key(b'C', modifier, application_cursor),
        SpecialKey::ArrowLeft => encode_cursor_key(b'D', modifier, application_cursor),
        SpecialKey::Home => encode_cursor_key(b'H', modifier, application_cursor),
        SpecialKey::End => encode_cursor_key(b'F', modifier, application_cursor),
        SpecialKey::PageUp => encode_tilde_key(5, modifier),
        SpecialKey::PageDown => encode_tilde_key(6, modifier),
        SpecialKey::Delete => encode_tilde_key(3, modifier),
        SpecialKey::F(1) => encode_function_key_ss3(b'P', modifier),
        SpecialKey::F(2) => encode_function_key_ss3(b'Q', modifier),
        SpecialKey::F(3) => encode_function_key_ss3(b'R', modifier),
        SpecialKey::F(4) => encode_function_key_ss3(b'S', modifier),
        SpecialKey::F(5) => encode_tilde_key(15, modifier),
        SpecialKey::F(6) => encode_tilde_key(17, modifier),
        SpecialKey::F(7) => encode_tilde_key(18, modifier),
        SpecialKey::F(8) => encode_tilde_key(19, modifier),
        SpecialKey::F(9) => encode_tilde_key(20, modifier),
        SpecialKey::F(10) => encode_tilde_key(21, modifier),
        SpecialKey::F(11) => encode_tilde_key(23, modifier),
        SpecialKey::F(12) => encode_tilde_key(24, modifier),
        SpecialKey::F(_) => Vec::new(),
    }
}

fn modifier_parameter(modifiers: InputModifiers) -> Option<u8> {
    let mut value = 1;
    if modifiers.shift {
        value += 1;
    }
    if modifiers.option {
        value += 2;
    }
    if modifiers.control {
        value += 4;
    }

    if value == 1 { None } else { Some(value) }
}

fn encode_cursor_key(final_byte: u8, modifier: Option<u8>, application_cursor: bool) -> Vec<u8> {
    match modifier {
        Some(modifier) => format!("\x1b[1;{modifier}{}", final_byte as char).into_bytes(),
        None if application_cursor => vec![0x1b, b'O', final_byte],
        None => vec![0x1b, b'[', final_byte],
    }
}

fn encode_function_key_ss3(final_byte: u8, modifier: Option<u8>) -> Vec<u8> {
    match modifier {
        Some(modifier) => format!("\x1b[1;{modifier}{}", final_byte as char).into_bytes(),
        None => vec![0x1b, b'O', final_byte],
    }
}

fn encode_tilde_key(code: u8, modifier: Option<u8>) -> Vec<u8> {
    match modifier {
        Some(modifier) => format!("\x1b[{code};{modifier}~").into_bytes(),
        None => format!("\x1b[{code}~").into_bytes(),
    }
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
        assert_eq!(
            translate_terminal_input("", InputModifiers::default(), false),
            None
        );
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

    #[test]
    fn modifiers_expand_special_key_escape_sequences() {
        assert_eq!(
            translate_terminal_input(
                "\u{f700}",
                InputModifiers {
                    shift: true,
                    ..InputModifiers::default()
                },
                false
            ),
            Some(b"\x1b[1;2A".to_vec())
        );
        assert_eq!(
            translate_terminal_input(
                "\u{f729}",
                InputModifiers {
                    control: true,
                    ..InputModifiers::default()
                },
                true
            ),
            Some(b"\x1b[1;5H".to_vec())
        );
        assert_eq!(
            translate_terminal_input(
                "\u{f728}",
                InputModifiers {
                    option: true,
                    ..InputModifiers::default()
                },
                false
            ),
            Some(b"\x1b[3;3~".to_vec())
        );
        assert_eq!(
            translate_terminal_input(
                "\u{f704}",
                InputModifiers {
                    shift: true,
                    control: true,
                    ..InputModifiers::default()
                },
                false
            ),
            Some(b"\x1b[1;6P".to_vec())
        );
        assert_eq!(
            translate_terminal_input(
                "\u{f70f}",
                InputModifiers {
                    shift: true,
                    option: true,
                    control: true,
                    ..InputModifiers::default()
                },
                false
            ),
            Some(b"\x1b[24;8~".to_vec())
        );
    }
}
