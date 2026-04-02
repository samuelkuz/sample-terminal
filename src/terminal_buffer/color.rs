use super::types::{DEFAULT_BG, DEFAULT_FG};
use super::TerminalBuffer;

pub(crate) fn ansi_16_color(index: u8, bright: bool) -> [f32; 4] {
    const NORMAL: [[f32; 4]; 8] = [
        [0.08, 0.09, 0.12, 1.0],
        [0.78, 0.29, 0.26, 1.0],
        [0.36, 0.67, 0.33, 1.0],
        [0.76, 0.63, 0.28, 1.0],
        [0.36, 0.50, 0.80, 1.0],
        [0.67, 0.42, 0.79, 1.0],
        [0.28, 0.66, 0.73, 1.0],
        [0.80, 0.82, 0.86, 1.0],
    ];
    const BRIGHT: [[f32; 4]; 8] = [
        [0.38, 0.41, 0.49, 1.0],
        [0.93, 0.41, 0.38, 1.0],
        [0.49, 0.81, 0.45, 1.0],
        [0.95, 0.83, 0.42, 1.0],
        [0.48, 0.63, 0.95, 1.0],
        [0.82, 0.58, 0.92, 1.0],
        [0.47, 0.86, 0.93, 1.0],
        [0.95, 0.96, 0.98, 1.0],
    ];
    if bright {
        BRIGHT[index as usize]
    } else {
        NORMAL[index as usize]
    }
}

pub(crate) fn parse_extended_color(params: &[Option<usize>]) -> Option<([f32; 4], usize)> {
    match params.first().copied().flatten()? {
        5 => {
            let index = params.get(1).and_then(|value| *value)?;
            Some((ansi_256_color(index as u8), 2))
        }
        2 => {
            let red = params.get(1).and_then(|value| *value)? as f32 / 255.0;
            let green = params.get(2).and_then(|value| *value)? as f32 / 255.0;
            let blue = params.get(3).and_then(|value| *value)? as f32 / 255.0;
            Some(([red, green, blue, 1.0], 4))
        }
        _ => None,
    }
}

fn ansi_256_color(index: u8) -> [f32; 4] {
    match index {
        0..=7 => ansi_16_color(index, false),
        8..=15 => ansi_16_color(index - 8, true),
        16..=231 => {
            let cube = index - 16;
            let r = cube / 36;
            let g = (cube % 36) / 6;
            let b = cube % 6;
            let scale = |value: u8| {
                if value == 0 {
                    0.0
                } else {
                    (value as f32 * 40.0 + 55.0) / 255.0
                }
            };
            [scale(r), scale(g), scale(b), 1.0]
        }
        232..=255 => {
            let gray = (8 + (index - 232) as u16 * 10) as f32 / 255.0;
            [gray, gray, gray, 1.0]
        }
    }
}

impl TerminalBuffer {
    pub(crate) fn set_graphics_rendition(&mut self, params: &[Option<usize>]) {
        let params = if params.is_empty() {
            vec![Some(0)]
        } else {
            params.to_vec()
        };
        let mut index = 0usize;
        while index < params.len() {
            let param = params[index].unwrap_or(0);
            match param {
                0 => self.current_attr = Default::default(),
                30..=37 => self.current_attr.fg = ansi_16_color((param - 30) as u8, false),
                40..=47 => self.current_attr.bg = ansi_16_color((param - 40) as u8, false),
                90..=97 => self.current_attr.fg = ansi_16_color((param - 90) as u8, true),
                100..=107 => self.current_attr.bg = ansi_16_color((param - 100) as u8, true),
                39 => self.current_attr.fg = DEFAULT_FG,
                49 => self.current_attr.bg = DEFAULT_BG,
                38 | 48 => {
                    let is_fg = param == 38;
                    if let Some((color, consumed)) = parse_extended_color(&params[index + 1..]) {
                        if is_fg {
                            self.current_attr.fg = color;
                        } else {
                            self.current_attr.bg = color;
                        }
                        index += consumed;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}
