use super::types::ParsedCsi;
use super::TerminalBuffer;

impl TerminalBuffer {
    pub(crate) fn push_byte(&mut self, byte: u8) {
        match self.parser {
            super::types::ParserState::Ground => self.handle_ground_byte(byte),
            super::types::ParserState::Escape => self.handle_escape_byte(byte),
            super::types::ParserState::Csi(_) => self.handle_csi_byte(byte),
            super::types::ParserState::Osc => self.handle_osc_byte(byte),
            super::types::ParserState::OscEsc => self.handle_osc_esc_byte(byte),
        }
    }

    fn handle_ground_byte(&mut self, byte: u8) {
        match byte {
            0x1b => {
                self.flush_utf8_lossy();
                self.parser = super::types::ParserState::Escape;
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
            0x07 => self.flush_utf8_lossy(),
            0x00..=0x1f => self.flush_utf8_lossy(),
            0x20..=0x7e => {
                self.flush_utf8_lossy();
                self.write_char(byte as char);
            }
            _ => self.push_utf8_byte(byte),
        }
    }

    fn handle_escape_byte(&mut self, byte: u8) {
        self.parser = match byte {
            b'[' => super::types::ParserState::Csi(Vec::new()),
            b']' => super::types::ParserState::Osc,
            b'D' => {
                self.index_down();
                super::types::ParserState::Ground
            }
            b'M' => {
                self.reverse_index();
                super::types::ParserState::Ground
            }
            b'E' => {
                self.next_line();
                super::types::ParserState::Ground
            }
            b'7' => {
                self.save_cursor();
                super::types::ParserState::Ground
            }
            b'8' => {
                self.restore_cursor();
                super::types::ParserState::Ground
            }
            b'c' => {
                self.reset();
                super::types::ParserState::Ground
            }
            _ => super::types::ParserState::Ground,
        };
    }

    fn handle_csi_byte(&mut self, byte: u8) {
        let super::types::ParserState::Csi(mut bytes) =
            std::mem::replace(&mut self.parser, super::types::ParserState::Ground)
        else {
            return;
        };

        if (0x40..=0x7e).contains(&byte) {
            self.execute_csi(byte, &bytes);
            return;
        }

        if bytes.len() < 64 {
            bytes.push(byte);
        }
        self.parser = super::types::ParserState::Csi(bytes);
    }

    fn handle_osc_byte(&mut self, byte: u8) {
        self.parser = match byte {
            0x07 => super::types::ParserState::Ground,
            0x1b => super::types::ParserState::OscEsc,
            _ => super::types::ParserState::Osc,
        };
    }

    fn handle_osc_esc_byte(&mut self, byte: u8) {
        self.parser = if byte == b'\\' {
            super::types::ParserState::Ground
        } else {
            super::types::ParserState::Osc
        };
    }

    fn execute_csi(&mut self, final_byte: u8, raw: &[u8]) {
        let (private, params) = parse_csi(raw);
        let csi = ParsedCsi {
            private,
            params: &params,
        };
        match (csi.private, final_byte) {
            (false, b'A') => self.move_cursor_relative(-(param_or(csi.params, 0, 1) as i32), 0),
            (false, b'B') => self.move_cursor_relative(param_or(csi.params, 0, 1) as i32, 0),
            (false, b'C') => self.move_cursor_relative(0, param_or(csi.params, 0, 1) as i32),
            (false, b'D') => self.move_cursor_relative(0, -(param_or(csi.params, 0, 1) as i32)),
            (false, b'G') => {
                self.set_cursor_col(param_or(csi.params, 0, 1).saturating_sub(1) as u16)
            }
            (false, b'd') => {
                self.set_cursor_row(param_or(csi.params, 0, 1).saturating_sub(1) as u16)
            }
            (false, b'H' | b'f') => {
                let row = param_or(csi.params, 0, 1).saturating_sub(1) as u16;
                let col = param_or(csi.params, 1, 1).saturating_sub(1) as u16;
                self.set_cursor_position(row, col);
            }
            (false, b'J') => self.erase_in_display(param_or(csi.params, 0, 0)),
            (false, b'K') => self.erase_in_line(param_or(csi.params, 0, 0)),
            (false, b'@') => self.insert_chars(param_or(csi.params, 0, 1) as u16),
            (false, b'P') => self.delete_chars(param_or(csi.params, 0, 1) as u16),
            (false, b'L') => self.insert_lines(param_or(csi.params, 0, 1) as u16),
            (false, b'M') => self.delete_lines(param_or(csi.params, 0, 1) as u16),
            (false, b'm') => self.set_graphics_rendition(csi.params),
            (false, b'r') => self.set_scroll_region(
                param_or(csi.params, 0, 1).saturating_sub(1) as u16,
                param_or(csi.params, 1, self.rows() as usize).saturating_sub(1) as u16,
            ),
            (false, b's') => self.save_cursor(),
            (false, b'u') => self.restore_cursor(),
            (true, b'h') => self.set_private_mode(csi.params, true),
            (true, b'l') => self.set_private_mode(csi.params, false),
            _ => {}
        }
        self.screen_mut().wrap_pending = false;
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
}

fn parse_csi(raw: &[u8]) -> (bool, Vec<Option<usize>>) {
    let private = raw.first() == Some(&b'?');
    let start = if private { 1 } else { 0 };
    let filtered = raw[start..]
        .iter()
        .copied()
        .filter(|byte| byte.is_ascii_digit() || *byte == b';')
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return (private, Vec::new());
    }
    let params = filtered
        .split(|byte| *byte == b';')
        .map(|part| {
            if part.is_empty() {
                None
            } else {
                std::str::from_utf8(part).ok()?.parse::<usize>().ok()
            }
        })
        .collect();
    (private, params)
}

fn param_or(params: &[Option<usize>], index: usize, default: usize) -> usize {
    params
        .get(index)
        .and_then(|value| *value)
        .unwrap_or(default)
}
