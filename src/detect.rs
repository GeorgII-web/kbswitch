//! Wrong-layout detection: a word that is no word in the layout it was typed in, but one in the other layout.

use crate::buffer::Key;
use crate::{charmap, lang};

/// Shorter words are not checked.
pub const MIN_KEYS: usize = 2;

/// Spell checking per layout language; `None` when that language can't be checked.
pub trait Lexicon {
    fn is_word(&self, language: u16, text: &str) -> Option<bool>;
}

/// True when `keys`, typed in `typed_in`, spell no word there but one in the other layout of the pair.
pub fn wrong_layout(lexicon: &impl Lexicon, keys: &[Key], typed_in: u16) -> bool {
    if keys.len() < MIN_KEYS || (typed_in != lang::EN && typed_in != lang::RU) {
        return false;
    }
    let other = lang::other(typed_in);
    lexicon.is_word(typed_in, &charmap::typed_text(keys, typed_in)) == Some(false)
        && lexicon.is_word(other, &charmap::typed_text(keys, other)) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, RU};

    /// Knows the listed words; `None` for a language without words.
    struct Words(&'static [(u16, &'static str)]);

    impl Lexicon for Words {
        fn is_word(&self, language: u16, text: &str) -> Option<bool> {
            let known = self.0.iter().any(|&(l, _)| l == language);
            known.then(|| self.0.contains(&(language, text)))
        }
    }

    const WORDS: Words = Words(&[
        (EN, "hello"),
        (EN, "is"),
        (RU, "привет"),
        (RU, "Привет,"),
        (RU, "я"),
    ]);

    /// Keys for US-layout text: letters (capital = Shift), `,` and `?`.
    fn keys(us: &str) -> Vec<Key> {
        us.chars()
            .map(|c| match c {
                ',' => Key {
                    vk: 0xBC,
                    scan: 0,
                    shift: false,
                },
                '?' => Key {
                    vk: 0xBF,
                    scan: 0,
                    shift: true,
                },
                _ => Key {
                    vk: u16::from(c.to_ascii_uppercase() as u8),
                    scan: 0,
                    shift: c.is_ascii_uppercase(),
                },
            })
            .collect()
    }

    #[test]
    fn russian_typed_in_en_is_wrong() {
        assert!(wrong_layout(&WORDS, &keys("ghbdtn"), EN));
        assert!(wrong_layout(&WORDS, &keys("Ghbdtn?"), EN));
    }

    #[test]
    fn english_typed_in_ru_is_wrong() {
        assert!(wrong_layout(&WORDS, &keys("hello"), RU)); // руддщ
        assert!(wrong_layout(&WORDS, &keys("is"), RU)); // шы
    }

    #[test]
    fn a_word_of_the_typed_layout_is_right() {
        assert!(!wrong_layout(&WORDS, &keys("hello"), EN));
    }

    #[test]
    fn no_word_in_either_layout_is_left_alone() {
        assert!(!wrong_layout(&WORDS, &keys("qwzx"), EN));
    }

    #[test]
    fn too_short_or_another_layout_is_not_checked() {
        assert!(!wrong_layout(&WORDS, &keys("z"), EN)); // я
        assert!(!wrong_layout(&WORDS, &keys("ghbdtn"), 0x0407)); // German
    }

    #[test]
    fn a_language_without_spell_checker_never_flags() {
        let english_only = Words(&[(EN, "hello")]);
        assert!(!wrong_layout(&english_only, &keys("ghbdtn"), EN));
        assert!(!wrong_layout(&english_only, &keys("hello"), RU));
    }
}
