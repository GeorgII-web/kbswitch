//! Keyboard layout language ids (the low word of an HKL) and the EN ↔ RU pair.

pub const EN: u16 = 0x0409;
pub const RU: u16 = 0x0419;

/// Language id of a keyboard layout handle (HKL).
pub fn from_hkl(hkl: usize) -> u16 {
    (hkl & 0xFFFF) as u16
}

/// The other layout of the pair; any layout but RU converts to RU.
pub fn other(lang: u16) -> u16 {
    if lang == RU { EN } else { RU }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_is_the_low_word_of_the_hkl() {
        assert_eq!(from_hkl(0x0409_0409), EN);
        assert_eq!(from_hkl(0xF0A8_0419), RU); // a layout variant keeps the language in the low word
    }

    #[test]
    fn other_toggles_the_pair() {
        assert_eq!(other(EN), RU);
        assert_eq!(other(RU), EN);
        assert_eq!(other(0x0407), RU); // German
        assert_eq!(other(0), RU); // unknown
    }
}
