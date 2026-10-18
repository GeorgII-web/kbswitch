//! The phrase typed since the last reset, stored as physical keys so it can be replayed in the other layout.

const MAX_KEYS: usize = 512;
const SPACE: u16 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub vk: u16,
    pub scan: u16,
    pub shift: bool,
}

enum Class {
    Text,
    Back,
    Neutral,
    Reset,
}

fn classify(vk: u16) -> Class {
    match vk {
        0x08 => Class::Back,
        // Space, 0-9, A-Z, OEM punctuation (;=,-./` [\]' and the 102nd key)
        0x20 | 0x30..=0x39 | 0x41..=0x5A | 0xBA..=0xC0 | 0xDB..=0xDE | 0xE2 => Class::Text,
        // Shift, Ctrl, Alt (generic and left/right), Win, Num Lock, Scroll Lock
        0x10..=0x12 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5 => Class::Neutral,
        // everything else, Caps Lock too: the replay must run with the Caps Lock state of the typing
        _ => Class::Reset,
    }
}

pub struct Buffer {
    keys: Vec<Key>,
    lang: u16,
}

impl Buffer {
    pub const fn new() -> Self {
        Self {
            keys: Vec::new(),
            lang: 0,
        }
    }

    /// A key-down typed in layout `lang`; `modifier` = Ctrl, Alt or Win is held.
    pub fn key_down(&mut self, key: Key, lang: u16, modifier: bool) {
        match classify(key.vk) {
            Class::Neutral => {}
            Class::Text if !modifier => {
                // a new phrase starts after a layout change and at the size cap
                if lang != self.lang || self.keys.len() >= MAX_KEYS {
                    self.keys.clear();
                }
                self.lang = lang;
                self.keys.push(key);
            }
            Class::Back if !modifier => {
                self.keys.pop();
            }
            _ => self.keys.clear(),
        }
    }

    pub fn reset(&mut self) {
        self.keys.clear();
    }

    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// The word the last key, a Space, just ended: the keys back to the Space before it.
    pub fn last_word(&self) -> &[Key] {
        match self.keys.split_last() {
            Some((last, before)) if last.vk == SPACE => {
                let start = before
                    .iter()
                    .rposition(|k| k.vk == SPACE)
                    .map_or(0, |i| i + 1);
                &before[start..]
            }
            _ => &[],
        }
    }

    /// Layout the phrase on screen is in.
    pub fn lang(&self) -> u16 {
        self.lang
    }

    /// After a conversion the phrase on screen is in `lang`.
    pub fn set_lang(&mut self, lang: u16) {
        self.lang = lang;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, RU};

    fn key(vk: u16) -> Key {
        Key {
            vk,
            scan: 0,
            shift: false,
        }
    }

    fn typed(vks: &[u16]) -> Buffer {
        let mut b = Buffer::new();
        for &vk in vks {
            b.key_down(key(vk), EN, false);
        }
        b
    }

    #[test]
    fn appends_letters_digits_punctuation_space() {
        assert_eq!(typed(&[0x41, 0x31, 0xBC, 0x20, 0xDB, 0xE2]).keys().len(), 6);
    }

    #[test]
    fn backspace_pops_and_stops_at_empty() {
        assert_eq!(typed(&[0x41, 0x42, 0x08]).keys(), &[key(0x41)]);
        assert!(typed(&[0x08, 0x08]).keys().is_empty());
    }

    #[test]
    fn modifiers_and_locks_keep_buffer() {
        assert_eq!(typed(&[0x41, 0xA0, 0x11, 0x5B, 0x90, 0x91]).keys().len(), 1);
    }

    #[test]
    fn navigation_enter_and_caps_lock_reset() {
        for vk in [0x0D, 0x09, 0x1B, 0x25, 0x24, 0x2E, 0x70, 0x60, 0x14] {
            assert!(typed(&[0x41, vk]).keys().is_empty(), "vk {vk:#x}");
        }
    }

    #[test]
    fn chord_with_modifier_resets() {
        let mut b = typed(&[0x41]);
        b.key_down(key(0x43), EN, true);
        assert!(b.keys().is_empty());
    }

    #[test]
    fn full_buffer_starts_over() {
        let b = typed(&[0x41; MAX_KEYS + 1]);
        assert_eq!(b.keys().len(), 1);
    }

    #[test]
    fn last_word_is_the_one_a_space_just_ended() {
        let word = |vks: &[u16]| {
            typed(vks)
                .last_word()
                .iter()
                .map(|k| k.vk)
                .collect::<Vec<_>>()
        };
        assert_eq!(word(&[0x41, 0x42, 0x20, 0x43, 0x44, 0x20]), [0x43, 0x44]);
        assert_eq!(word(&[0x41, 0x42, 0x20]), [0x41, 0x42]);
        assert!(word(&[0x41, 0x42]).is_empty()); // not ended yet
        assert!(word(&[0x41, 0x20, 0x20]).is_empty()); // two spaces
    }

    #[test]
    fn layout_change_starts_a_new_phrase() {
        let mut b = typed(&[0x41, 0x42]);
        b.key_down(key(0x43), RU, false);
        assert_eq!((b.keys(), b.lang()), (&[key(0x43)][..], RU));
    }

    #[test]
    fn converted_phrase_goes_on_in_the_new_layout() {
        let mut b = typed(&[0x41]);
        b.set_lang(RU);
        b.key_down(key(0x42), RU, false);
        assert_eq!((b.keys(), b.lang()), (&[key(0x41), key(0x42)][..], RU));
    }
}
