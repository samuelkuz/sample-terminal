const MAX_LINES: usize = 2_000;

#[derive(Debug)]
pub struct TerminalBuffer {
    lines: Vec<Vec<char>>,
    cursor_row: usize,
    cursor_col: usize,
    escape: EscapeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    None,
    Escape,
    Csi,
    Osc,
    OscEsc,
}

impl TerminalBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![Vec::new()],
            cursor_row: 0,
            cursor_col: 0,
            escape: EscapeState::None,
        }
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);

        for ch in text.chars() {
            match self.escape {
                EscapeState::None => self.push_char(ch),
                EscapeState::Escape => self.consume_escape(ch),
                EscapeState::Csi => self.consume_csi(ch),
                EscapeState::Osc => self.consume_osc(ch),
                EscapeState::OscEsc => self.consume_osc_esc(ch),
            }
        }
    }

    pub fn visible_lines(&self, max_lines: usize) -> Vec<String> {
        let start = self.lines.len().saturating_sub(max_lines);
        self.lines[start..]
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect()
    }

    fn push_char(&mut self, ch: char) {
        match ch {
            '\u{1b}' => self.escape = EscapeState::Escape,
            '\n' => self.new_line(),
            '\r' => self.cursor_col = 0,
            '\u{8}' | '\u{7f}' => self.backspace(),
            c if c.is_control() => {}
            c => self.write_char(c),
        }
    }

    fn consume_escape(&mut self, ch: char) {
        self.escape = match ch {
            '[' => EscapeState::Csi,
            ']' => EscapeState::Osc,
            _ => EscapeState::None,
        };
    }

    fn consume_csi(&mut self, ch: char) {
        if ('@'..='~').contains(&ch) {
            self.escape = EscapeState::None;
        }
    }

    fn consume_osc(&mut self, ch: char) {
        self.escape = match ch {
            '\u{7}' => EscapeState::None,
            '\u{1b}' => EscapeState::OscEsc,
            _ => EscapeState::Osc,
        };
    }

    fn consume_osc_esc(&mut self, ch: char) {
        self.escape = if ch == '\\' {
            EscapeState::None
        } else {
            EscapeState::Osc
        };
    }

    fn ensure_current_line(&mut self) {
        while self.lines.len() <= self.cursor_row {
            self.lines.push(Vec::new());
        }
    }

    fn write_char(&mut self, ch: char) {
        self.ensure_current_line();
        let line = &mut self.lines[self.cursor_row];

        if line.len() < self.cursor_col {
            line.resize(self.cursor_col, ' ');
        }

        if self.cursor_col == line.len() {
            line.push(ch);
        } else {
            line[self.cursor_col] = ch;
        }

        self.cursor_col += 1;
    }

    fn new_line(&mut self) {
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.push(Vec::new());

        if self.lines.len() > MAX_LINES {
            let overflow = self.lines.len() - MAX_LINES;
            self.lines.drain(0..overflow);
            self.cursor_row = self.cursor_row.saturating_sub(overflow);
        }
    }

    fn backspace(&mut self) {
        if self.cursor_col == 0 {
            return;
        }

        self.cursor_col -= 1;
        self.ensure_current_line();
        let line = &mut self.lines[self.cursor_row];

        if self.cursor_col < line.len() {
            line.remove(self.cursor_col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalBuffer;

    #[test]
    fn appends_plain_text() {
        let mut buffer = TerminalBuffer::new();
        buffer.push_bytes(b"hello");

        assert_eq!(buffer.visible_lines(10), vec!["hello"]);
    }

    #[test]
    fn handles_newline_and_carriage_return() {
        let mut buffer = TerminalBuffer::new();
        buffer.push_bytes(b"hello\rj\nworld");

        assert_eq!(buffer.visible_lines(10), vec!["jello", "world"]);
    }

    #[test]
    fn handles_backspace() {
        let mut buffer = TerminalBuffer::new();
        buffer.push_bytes(b"abc\x08d");

        assert_eq!(buffer.visible_lines(10), vec!["abd"]);
    }

    #[test]
    fn skips_basic_ansi_sequences() {
        let mut buffer = TerminalBuffer::new();
        buffer.push_bytes(b"\x1b[31mred\x1b[0m");

        assert_eq!(buffer.visible_lines(10), vec!["red"]);
    }
}
