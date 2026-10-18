//! The tray icon picture: the layout's two-letter code (EN, RU, DE …) on a square in the layout's color, white on
//! a filled square when enabled, colored letters in a frame when disabled; white on amber while `settings.json`
//! has an error. Drawn pixel by pixel from a 5×7 font, no font or image crate.

use crate::lang;

/// 32 px: Windows scales it to the tray size; 16 px left no room for two readable letters.
pub const SIZE: u32 = 32;
const FRAME: u32 = 3;
/// Each font pixel is a 2×2 block: two glyphs of 10×14 and a 2 px gap, centered.
const SCALE: u32 = 2;
const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;
const GAP: u32 = 1;
const TEXT_W: u32 = (GLYPH_W * 2 + GAP) * SCALE;
const TEXT_H: u32 = GLYPH_H * SCALE;
const LEFT: u32 = (SIZE - TEXT_W) / 2;
const TOP: u32 = (SIZE - TEXT_H) / 2;

pub const EN_BLUE: [u8; 4] = [0x25, 0x63, 0xEB, 0xFF];
pub const RU_RED: [u8; 4] = [0xDC, 0x26, 0x26, 0xFF];
pub const OTHER_GREY: [u8; 4] = [0x9C, 0xA3, 0xAF, 0xFF];
pub const ERROR_AMBER: [u8; 4] = [0xF5, 0x9E, 0x0B, 0xFF];
const WHITE: [u8; 4] = [0xFF; 4];
const CLEAR: [u8; 4] = [0; 4];

/// What the icon shows; the tray redraws only when it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub color: [u8; 4],
    pub filled: bool,
    /// Two ASCII capitals; `??` for a layout without a known code.
    pub code: [u8; 2],
}

impl Look {
    pub fn new(language: u16, enabled: bool, error: bool) -> Self {
        let code = code(language);
        if error {
            return Self {
                color: ERROR_AMBER,
                filled: true,
                code,
            };
        }
        let color = match language {
            lang::EN => EN_BLUE,
            lang::RU => RU_RED,
            _ => OTHER_GREY,
        };
        Self {
            color,
            filled: enabled,
            code,
        }
    }

    /// RGBA pixels, row by row: the square, then the letters over it.
    pub fn rgba(self) -> Vec<u8> {
        let letters = if self.filled { WHITE } else { self.color };
        (0..SIZE * SIZE)
            .flat_map(|i| {
                let (x, y) = (i % SIZE, i / SIZE);
                let frame = x < FRAME || y < FRAME || x >= SIZE - FRAME || y >= SIZE - FRAME;
                if self.text_pixel(x, y) {
                    letters
                } else if self.filled || frame {
                    self.color
                } else {
                    CLEAR
                }
            })
            .collect()
    }

    fn text_pixel(self, x: u32, y: u32) -> bool {
        if x < LEFT || y < TOP || x >= LEFT + TEXT_W || y >= TOP + TEXT_H {
            return false;
        }
        let (col, row) = ((x - LEFT) / SCALE, (y - TOP) / SCALE);
        let (index, col) = if col < GLYPH_W {
            (0, col)
        } else if col >= GLYPH_W + GAP {
            (1, col - GLYPH_W - GAP)
        } else {
            return false; // the gap
        };
        glyph(self.code[index])[row as usize] >> (GLYPH_W - 1 - col) & 1 == 1
    }
}

/// ISO 639-1 code of the layout's primary language (low byte of the LANGID), capitals; `??` when unknown.
pub fn code(language: u16) -> [u8; 2] {
    match language & 0xFF {
        0x09 => *b"EN",
        0x19 => *b"RU",
        0x07 => *b"DE",
        0x0C => *b"FR",
        0x0A => *b"ES",
        0x10 => *b"IT",
        0x13 => *b"NL",
        0x15 => *b"PL",
        0x16 => *b"PT",
        0x1F => *b"TR",
        0x22 => *b"UK",
        0x05 => *b"CS",
        0x0B => *b"FI",
        0x1D => *b"SV",
        0x06 => *b"DA",
        0x14 => *b"NO",
        0x0E => *b"HU",
        0x08 => *b"EL",
        0x0D => *b"HE",
        0x23 => *b"BE",
        0x3F => *b"KK",
        _ => *b"??",
    }
}

/// 5×7 capitals A–Z and `?`, one byte per row, the high bit of the low 5 the left column.
fn glyph(c: u8) -> [u8; 7] {
    match c {
        b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        b'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        b'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        b'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        b'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        b'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        b'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        b'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        _ => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04], // ?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, RU};

    fn pixel(look: Look, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        look.rgba()[i..i + 4].try_into().unwrap()
    }

    /// The icon as text rows: `#` letters, `.` square, space clear.
    fn ascii(look: Look) -> Vec<String> {
        let letters = if look.filled { WHITE } else { look.color };
        (0..SIZE)
            .map(|y| {
                (0..SIZE)
                    .map(|x| match pixel(look, x, y) {
                        p if p == letters => '#',
                        p if p == CLEAR => ' ',
                        _ => '.',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn color_and_code_follow_the_layout() {
        let en = Look::new(EN, true, false);
        assert_eq!((en.color, en.code), (EN_BLUE, *b"EN"));
        let ru = Look::new(RU, true, false);
        assert_eq!((ru.color, ru.code), (RU_RED, *b"RU"));
        let de = Look::new(0x0407, true, false);
        assert_eq!((de.color, de.code), (OTHER_GREY, *b"DE"));
        let swiss_german = Look::new(0x0807, true, false);
        assert_eq!(swiss_german.code, *b"DE");
        let none = Look::new(0, true, false); // not read yet
        assert_eq!((none.color, none.code), (OTHER_GREY, *b"??"));
    }

    #[test]
    fn enabled_is_a_filled_square_with_white_letters() {
        let look = Look::new(RU, true, false);
        let rows = ascii(look);
        assert_eq!(rows.len() as u32, SIZE);
        assert!(rows.iter().all(|r| !r.contains(' ')));
        assert_eq!(rows[0], ".".repeat(SIZE as usize));
        // the R's top bar and the U's left stem, scaled 2×, at the computed origin
        let top = TOP as usize;
        let left = LEFT as usize;
        assert_eq!(&rows[top][left..left + 10], "########..");
        assert_eq!(&rows[top + 1][left..left + 10], "########..");
        assert_eq!(&rows[top][left + 12..left + 22], "##......##");
    }

    #[test]
    fn disabled_is_a_frame_with_colored_letters() {
        let look = Look::new(EN, false, false);
        let rows = ascii(look);
        for (x, y) in [(0, 0), (2, 8), (8, 29), (31, 31)] {
            assert_eq!(pixel(look, x, y), EN_BLUE, "({x}, {y})");
        }
        assert_eq!(pixel(look, 4, 4), CLEAR);
        assert!(rows[TOP as usize].contains("##########")); // the E's top bar, blue
        assert_eq!(pixel(look, LEFT, TOP), EN_BLUE);
    }

    #[test]
    fn settings_error_is_amber_whatever_the_layout() {
        for (language, enabled) in [(EN, true), (RU, false)] {
            let look = Look::new(language, enabled, true);
            assert_eq!((look.color, look.filled), (ERROR_AMBER, true));
            assert_eq!(look.code, code(language));
        }
    }

    #[test]
    fn every_glyph_fits_five_columns() {
        for c in b'A'..=b'Z' {
            assert!(glyph(c).iter().all(|row| *row < 0x20), "{}", c as char);
        }
        assert_eq!(glyph(b'?')[0], 0x0E);
    }
}
