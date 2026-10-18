//! EN (US) ↔ RU (ЙЦУКЕН) character map for selected text: the same physical key at the same position in both
//! strings.

use crate::buffer::Key;
use crate::lang;

const EN_KEYS: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?@#$^&|";
const RU_KEYS: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,\"№;:?/";

/// Converts text typed in the wrong layout; returns it with the language it now belongs to.
/// More Cyrillic than Latin letters → RU to EN, else EN to RU.
pub fn convert(text: &str) -> (String, u16) {
    let cyrillic = text.chars().filter(|&c| is_cyrillic(c)).count();
    let latin = text.chars().filter(char::is_ascii_alphabetic).count();
    let target = if cyrillic > latin { lang::EN } else { lang::RU };
    (convert_to(text, target), target)
}

/// Converts text typed in the other layout into `target`: EN keys to RU for RU, else RU keys to EN. Characters
/// of the target layout stay as they are.
pub fn convert_to(text: &str, target: u16) -> String {
    let (from, to) = if target == lang::RU {
        (EN_KEYS, RU_KEYS)
    } else {
        (RU_KEYS, EN_KEYS)
    };
    text.chars()
        .map(|c| {
            from.chars()
                .position(|f| f == c)
                .and_then(|i| to.chars().nth(i))
                .unwrap_or(c)
        })
        .collect()
}

/// What `keys` type in layout `lang`: ЙЦУКЕН for RU, the US layout for any other.
pub fn typed_text(keys: &[Key], lang: u16) -> String {
    let us: String = keys.iter().filter_map(|k| us_char(k.vk, k.shift)).collect();
    if lang == lang::RU {
        convert_to(&us, lang::RU)
    } else {
        us
    }
}

/// The character a key types in the US layout.
fn us_char(vk: u16, shift: bool) -> Option<char> {
    const DIGITS: &[u8; 10] = b"0123456789";
    const SHIFTED_DIGITS: &[u8; 10] = b")!@#$%^&*(";
    let (plain, shifted) = match vk {
        0x20 => (' ', ' '),
        0x30..=0x39 => {
            let i = usize::from(vk - 0x30);
            (char::from(DIGITS[i]), char::from(SHIFTED_DIGITS[i]))
        }
        0x41..=0x5A => {
            let c = char::from(u8::try_from(vk).ok()?);
            (c.to_ascii_lowercase(), c)
        }
        0xBA => (';', ':'),
        0xBB => ('=', '+'),
        0xBC => (',', '<'),
        0xBD => ('-', '_'),
        0xBE => ('.', '>'),
        0xBF => ('/', '?'),
        0xC0 => ('`', '~'),
        0xDB => ('[', '{'),
        0xDC | 0xE2 => ('\\', '|'),
        0xDD => (']', '}'),
        0xDE => ('\'', '"'),
        _ => return None,
    };
    Some(if shift { shifted } else { plain })
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, 'а'..='я' | 'А'..='Я' | 'ё' | 'Ё')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, RU};

    #[test]
    fn tables_are_one_to_one() {
        for table in [EN_KEYS, RU_KEYS] {
            let mut chars: Vec<char> = table.chars().collect();
            let len = chars.len();
            chars.sort_unstable();
            chars.dedup();
            assert_eq!(chars.len(), len, "duplicate in {table}");
        }
        assert_eq!(EN_KEYS.chars().count(), RU_KEYS.chars().count());
    }

    #[test]
    fn en_to_ru() {
        assert_eq!(convert("Ghbdtn vbh!"), ("Привет мир!".into(), RU));
        assert_eq!(convert("[jhjij? 'nj ;tkt"), ("хорошо, это желе".into(), RU));
    }

    #[test]
    fn ru_to_en() {
        assert_eq!(convert("руддщ цщкдв"), ("hello world".into(), EN));
        assert_eq!(convert("ЫЕЩЗ?"), ("STOP&".into(), EN));
    }

    #[test]
    fn every_key_round_trips() {
        let (ru, _) = convert(EN_KEYS);
        assert_eq!(ru, RU_KEYS);
        assert_eq!(convert(&ru).0, EN_KEYS);
    }

    #[test]
    fn mixed_text_follows_the_majority_and_keeps_the_rest() {
        assert_eq!(convert("Ghbdtn мир").0, "Привет мир");
        assert_eq!(convert("руддщ Bob").0, "hello Bob");
    }

    #[test]
    fn convert_to_takes_the_direction_it_is_given() {
        assert_eq!(convert_to("Ghbdtn vbh", RU), "Привет мир");
        assert_eq!(convert_to("Руддщ цщкдв", EN), "Hello world");
        // punctuation alone has no script to guess from
        assert_eq!(convert_to("?", RU), ",");
        assert_eq!(convert_to(",", EN), "?");
        // characters already in the target layout stay
        assert_eq!(convert_to("ghbdtn мир", RU), "привет мир");
    }

    fn key(vk: u16, shift: bool) -> Key {
        Key { vk, scan: 0, shift }
    }

    #[test]
    fn typed_text_follows_the_layout() {
        // "Ghbdtn," with Shift on G, the comma being Shift+/ on RU
        let keys = [
            key(0x47, true),
            key(0x48, false),
            key(0x42, false),
            key(0x44, false),
            key(0x54, false),
            key(0x4E, false),
            key(0xBF, true),
        ];
        assert_eq!(typed_text(&keys, EN), "Ghbdtn?");
        assert_eq!(typed_text(&keys, RU), "Привет,");
    }

    #[test]
    fn typed_text_of_digits_punctuation_and_other_keys() {
        let keys = [
            key(0x32, true),
            key(0x33, true),
            key(0x31, false),
            key(0x20, false),
            key(0x0D, false),
        ];
        assert_eq!(typed_text(&keys, EN), "@#1 ");
        assert_eq!(typed_text(&keys, RU), "\"№1 ");
        assert_eq!(typed_text(&[key(0xBC, false), key(0xDE, true)], RU), "бЭ");
    }

    #[test]
    fn digits_whitespace_and_unknown_stay() {
        assert_eq!(convert("abc 123\n\t€").0, "фис 123\n\t€");
        assert_eq!(convert(""), (String::new(), RU));
    }
}
