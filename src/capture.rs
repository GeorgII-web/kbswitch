//! The hooks' decisions: the typed phrase and the double Shift trigger. The hook reads the Windows state into a
//! `KeyEvent`, so these rules are unit-tested without Windows.

use crate::buffer::{Buffer, Key};
use crate::detect;
use crate::settings::CapsLock;
use crate::trigger::DoubleShift;

const VK_PAUSE: u16 = 0x13;
/// Num Lock and Scroll Lock: every LED change kbswitch injects comes back as an unmarked key-down (the keyboard
/// driver echoes toggle keys), so they must not cancel a pending Shift tap.
const VK_NUMLOCK: u16 = 0x90;
const VK_SCROLL: u16 = 0x91;
const VK_CAPITAL: u16 = 0x14;
const VK_SPACE: u16 = 0x20;

fn is_shift(vk: u16) -> bool {
    matches!(vk, 0x10 | 0xA0 | 0xA1)
}

/// A keyboard event plus the state the hook read when it happened.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyEvent {
    pub vk: u16,
    pub scan: u16,
    pub down: bool,
    /// Timestamp in ms.
    pub time: u32,
    /// Shift held before this event.
    pub shift: bool,
    /// Ctrl or Alt held.
    pub ctrl_alt: bool,
    /// Win held.
    pub win: bool,
    /// Foreground window and the language of its keyboard layout.
    pub window: usize,
    pub lang: u16,
}

/// What the hook does with a key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Pass the key on.
    Pass,
    /// Pass it on; it completed a double Shift.
    Convert,
    /// Swallow it; Shift+Pause asks to convert the selected text only.
    ConvertSelection,
    /// Pass it on; a Space ended a word to check for the wrong layout (`Capture::take_word`).
    Check,
    /// Swallow it: no app sees it, Caps Lock doesn't toggle.
    Swallow,
    /// Swallow it and switch the layout.
    SwitchLayout,
}

pub struct Capture {
    buffer: Buffer,
    trigger: DoubleShift,
    window: usize,
    caps_lock: CapsLock,
    /// The decision for the Caps Lock press being held; its repeats and release follow it.
    caps_press: Option<Effect>,
    detect: bool,
    /// The last word a Space ended, with its layout, until the main thread takes it.
    word: Option<(Vec<Key>, u16)>,
}

impl Capture {
    pub const fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            trigger: DoubleShift::new(),
            window: 0,
            caps_lock: CapsLock::Normal,
            caps_press: None,
            detect: false,
            word: None,
        }
    }

    pub fn key(&mut self, e: &KeyEvent) -> Effect {
        if e.vk == VK_CAPITAL {
            let effect = self.caps_lock_key(e.down, e.shift);
            if effect != Effect::Pass {
                return effect;
            }
        }
        if e.down && e.window != self.window {
            self.window = e.window;
            self.buffer.reset();
        }
        if is_shift(e.vk) {
            if e.down {
                self.trigger.shift_down(e.time);
            }
            if e.ctrl_alt || e.win {
                self.trigger.other(); // Ctrl+Shift, Alt+Shift: a hotkey, not a tap
            }
            let fired = !e.down && self.trigger.shift_up(e.time);
            return if fired { Effect::Convert } else { Effect::Pass };
        }
        if e.vk == VK_NUMLOCK || e.vk == VK_SCROLL {
            return Effect::Pass;
        }
        if e.down {
            self.trigger.other();
            if e.vk == VK_PAUSE && e.shift && !e.ctrl_alt && !e.win {
                return Effect::ConvertSelection;
            }
            let layout_hotkey = e.vk == VK_SPACE && e.win && !e.ctrl_alt; // Win+Space
            if !layout_hotkey {
                let key = Key {
                    vk: e.vk,
                    scan: e.scan,
                    shift: e.shift,
                };
                self.buffer.key_down(key, e.lang, e.ctrl_alt || e.win);
                if self.detect && e.vk == VK_SPACE {
                    let word = self.buffer.last_word();
                    if word.len() >= detect::MIN_KEYS {
                        self.word = Some((word.to_vec(), self.buffer.lang()));
                        return Effect::Check;
                    }
                }
            }
        }
        Effect::Pass
    }

    pub fn set_detect(&mut self, on: bool) {
        self.detect = on;
        self.word = None;
    }

    /// The word to check, with the layout it was typed in.
    pub fn take_word(&mut self) -> Option<(Vec<Key>, u16)> {
        self.word.take()
    }

    /// Caps Lock per the `caps_lock` setting, decided on the first key-down of a press: only that one switches.
    fn caps_lock_key(&mut self, down: bool, shift: bool) -> Effect {
        let decided = match self.caps_lock {
            CapsLock::Normal => Effect::Pass,
            CapsLock::Off => Effect::Swallow,
            CapsLock::SwitchLayout if shift => Effect::Pass, // Shift+Caps Lock stays a real Caps Lock
            CapsLock::SwitchLayout => Effect::SwitchLayout,
        };
        let follow = |press: Effect| match press {
            Effect::Pass => Effect::Pass,
            _ => Effect::Swallow,
        };
        if !down {
            return follow(self.caps_press.take().unwrap_or(decided));
        }
        if let Some(press) = self.caps_press {
            return follow(press); // auto-repeat
        }
        self.caps_press = Some(decided);
        if decided != Effect::Pass {
            self.trigger.other(); // cancels a pending Shift tap; no text changed, so the phrase stays
        }
        decided
    }

    pub fn set_caps_lock(&mut self, mode: CapsLock) {
        self.caps_lock = mode;
    }

    pub fn set_timing(&mut self, tap_max_ms: u32, double_ms: u32) {
        self.trigger.set_timing(tap_max_ms, double_ms);
    }

    /// A mouse button: the caret may have moved.
    pub fn click(&mut self) {
        self.buffer.reset();
        self.trigger.other();
    }

    /// The typed phrase and the layout it is in on screen.
    pub fn phrase(&self) -> (Vec<Key>, u16) {
        (self.buffer.keys().to_vec(), self.buffer.lang())
    }

    /// After a conversion the phrase on screen is in `lang`.
    pub fn converted(&mut self, lang: u16) {
        self.buffer.set_lang(lang);
    }

    /// The phrase no longer matches the screen.
    pub fn forget(&mut self) {
        self.buffer.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, RU};

    const A: u16 = 0x41;
    const B: u16 = 0x42;
    const ALT: u16 = 0xA4;
    const CAPS: u16 = 0x14;
    const PAUSE: u16 = 0x13;
    const LSHIFT: u16 = 0xA0;
    const RSHIFT: u16 = 0xA1;

    /// Typing in window 1, EN layout, no modifiers held.
    const EN_WINDOW: KeyEvent = KeyEvent {
        vk: 0,
        scan: 0,
        down: false,
        time: 0,
        shift: false,
        ctrl_alt: false,
        win: false,
        window: 1,
        lang: EN,
    };

    fn down(vk: u16, time: u32, ctx: KeyEvent) -> KeyEvent {
        KeyEvent {
            vk,
            scan: vk + 0x100,
            down: true,
            time,
            ..ctx
        }
    }

    fn up(vk: u16, time: u32, ctx: KeyEvent) -> KeyEvent {
        KeyEvent {
            down: false,
            ..down(vk, time, ctx)
        }
    }

    /// Down at `time`, up 50 ms later; true when the release fired.
    fn tap(c: &mut Capture, vk: u16, time: u32, ctx: KeyEvent) -> bool {
        c.key(&down(vk, time, ctx));
        c.key(&up(vk, time + 50, ctx)) == Effect::Convert
    }

    fn typed(c: &Capture) -> Vec<u16> {
        c.phrase().0.iter().map(|k| k.vk).collect()
    }

    #[test]
    fn records_keys_with_scan_code_and_layout() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        let shifted = KeyEvent {
            shift: true,
            ..EN_WINDOW
        };
        tap(&mut c, B, 100, shifted);
        let (keys, lang) = c.phrase();
        assert_eq!(lang, EN);
        assert_eq!(
            keys,
            [
                Key {
                    vk: A,
                    scan: A + 0x100,
                    shift: false
                },
                Key {
                    vk: B,
                    scan: B + 0x100,
                    shift: true
                }
            ]
        );
    }

    #[test]
    fn double_shift_fires_and_keeps_the_phrase() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        assert!(!tap(&mut c, LSHIFT, 1000, EN_WINDOW));
        assert!(tap(&mut c, RSHIFT, 1200, EN_WINDOW));
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn shift_with_ctrl_or_alt_held_is_a_hotkey_not_a_tap() {
        let mut c = Capture::new();
        let alt = KeyEvent {
            ctrl_alt: true,
            ..EN_WINDOW
        };
        assert!(!tap(&mut c, LSHIFT, 0, alt));
        assert!(!tap(&mut c, LSHIFT, 200, alt));
        // a plain double Shift right after still works
        assert!(!tap(&mut c, LSHIFT, 400, EN_WINDOW));
        assert!(tap(&mut c, LSHIFT, 600, EN_WINDOW));
    }

    #[test]
    fn shift_with_win_held_is_a_hotkey_not_a_tap() {
        let mut c = Capture::new();
        let win = KeyEvent {
            win: true,
            ..EN_WINDOW
        };
        assert!(!tap(&mut c, LSHIFT, 0, EN_WINDOW));
        assert!(!tap(&mut c, LSHIFT, 200, win));
    }

    #[test]
    fn typing_capitals_does_not_fire() {
        let mut c = Capture::new();
        let shifted = KeyEvent {
            shift: true,
            ..EN_WINDOW
        };
        for (t, vk) in [(0, 0x47), (150, 0x48)] {
            c.key(&down(LSHIFT, t, EN_WINDOW));
            tap(&mut c, vk, t + 20, shifted);
            assert_eq!(c.key(&up(LSHIFT, t + 100, shifted)), Effect::Pass);
        }
        assert_eq!(typed(&c), [0x47, 0x48]);
    }

    #[test]
    fn shift_pause_converts_the_selection_and_is_swallowed() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        c.key(&down(LSHIFT, 100, EN_WINDOW));
        let shifted = KeyEvent {
            shift: true,
            ..EN_WINDOW
        };
        assert_eq!(c.key(&down(PAUSE, 120, shifted)), Effect::ConvertSelection);
        assert_eq!(c.key(&up(PAUSE, 160, shifted)), Effect::Pass);
        // the Shift of the chord is no tap
        assert_eq!(c.key(&up(LSHIFT, 200, EN_WINDOW)), Effect::Pass);
        assert!(!tap(&mut c, LSHIFT, 300, EN_WINDOW));
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn pause_alone_or_with_ctrl_is_no_trigger() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        assert_eq!(c.key(&down(PAUSE, 100, EN_WINDOW)), Effect::Pass);
        let ctrl_shift = KeyEvent {
            shift: true,
            ctrl_alt: true,
            ..EN_WINDOW
        };
        assert_eq!(c.key(&down(PAUSE, 200, ctrl_shift)), Effect::Pass);
        assert!(typed(&c).is_empty()); // Pause resets the phrase like any other key
    }

    #[test]
    fn scroll_lock_between_shift_taps_does_not_cancel() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        assert!(!tap(&mut c, LSHIFT, 100, EN_WINDOW));
        assert_eq!(c.key(&down(0x91, 150, EN_WINDOW)), Effect::Pass); // the LED following the layout
        assert_eq!(c.key(&down(0x90, 160, EN_WINDOW)), Effect::Pass);
        assert!(tap(&mut c, LSHIFT, 200, EN_WINDOW));
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn key_between_shift_taps_cancels() {
        let mut c = Capture::new();
        tap(&mut c, LSHIFT, 0, EN_WINDOW);
        tap(&mut c, A, 100, EN_WINDOW);
        assert!(!tap(&mut c, LSHIFT, 200, EN_WINDOW));
    }

    #[test]
    fn window_change_starts_a_new_phrase() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        let other_window = KeyEvent {
            window: 2,
            ..EN_WINDOW
        };
        tap(&mut c, B, 100, other_window);
        assert_eq!(typed(&c), [B]);
    }

    #[test]
    fn click_resets_the_phrase_and_a_pending_tap() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        tap(&mut c, LSHIFT, 100, EN_WINDOW);
        c.click();
        assert!(typed(&c).is_empty());
        assert!(!tap(&mut c, LSHIFT, 250, EN_WINDOW));
    }

    #[test]
    fn alt_shift_layout_switch_keeps_the_phrase_until_typing_goes_on() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        let alt = KeyEvent {
            ctrl_alt: true,
            ..EN_WINDOW
        };
        c.key(&down(ALT, 100, EN_WINDOW));
        tap(&mut c, LSHIFT, 120, alt);
        c.key(&up(ALT, 200, EN_WINDOW));
        assert_eq!((typed(&c), c.phrase().1), (vec![A], EN));

        let ru = KeyEvent {
            lang: RU,
            ..EN_WINDOW
        };
        tap(&mut c, B, 300, ru);
        assert_eq!((typed(&c), c.phrase().1), (vec![B], RU));
    }

    #[test]
    fn win_space_switches_the_layout_without_a_reset() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        let win = KeyEvent {
            win: true,
            ..EN_WINDOW
        };
        tap(&mut c, VK_SPACE, 100, win);
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn other_chords_reset() {
        for ctx in [
            KeyEvent {
                ctrl_alt: true,
                ..EN_WINDOW
            },
            KeyEvent {
                win: true,
                ..EN_WINDOW
            },
        ] {
            let mut c = Capture::new();
            tap(&mut c, A, 0, EN_WINDOW);
            tap(&mut c, B, 100, ctx);
            assert!(typed(&c).is_empty());
        }
    }

    #[test]
    fn caps_lock_normal_passes_and_resets_the_phrase() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        assert_eq!(c.key(&down(CAPS, 100, EN_WINDOW)), Effect::Pass);
        assert_eq!(c.key(&up(CAPS, 150, EN_WINDOW)), Effect::Pass);
        assert!(typed(&c).is_empty());
    }

    #[test]
    fn caps_lock_off_is_swallowed_and_keeps_the_phrase() {
        let mut c = Capture::new();
        c.set_caps_lock(CapsLock::Off);
        tap(&mut c, A, 0, EN_WINDOW);
        let shifted = KeyEvent {
            shift: true,
            ..EN_WINDOW
        };
        for (t, ctx) in [(100, EN_WINDOW), (200, shifted)] {
            assert_eq!(c.key(&down(CAPS, t, ctx)), Effect::Swallow);
            assert_eq!(c.key(&up(CAPS, t + 50, ctx)), Effect::Swallow);
        }
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn caps_lock_switch_layout_switches_once_per_press_and_keeps_the_phrase() {
        let mut c = Capture::new();
        c.set_caps_lock(CapsLock::SwitchLayout);
        tap(&mut c, A, 0, EN_WINDOW);
        assert_eq!(c.key(&down(CAPS, 100, EN_WINDOW)), Effect::SwitchLayout);
        assert_eq!(c.key(&down(CAPS, 600, EN_WINDOW)), Effect::Swallow); // auto-repeat
        assert_eq!(c.key(&up(CAPS, 650, EN_WINDOW)), Effect::Swallow);
        assert_eq!(c.key(&down(CAPS, 800, EN_WINDOW)), Effect::SwitchLayout);
        assert_eq!(typed(&c), [A]);
    }

    #[test]
    fn shift_caps_lock_stays_a_caps_lock_in_switch_layout() {
        let mut c = Capture::new();
        c.set_caps_lock(CapsLock::SwitchLayout);
        let shifted = KeyEvent {
            shift: true,
            ..EN_WINDOW
        };
        assert_eq!(c.key(&down(CAPS, 0, shifted)), Effect::Pass);
        // Shift released before Caps Lock: the release follows the press
        assert_eq!(c.key(&up(CAPS, 50, EN_WINDOW)), Effect::Pass);
    }

    #[test]
    fn swallowed_caps_lock_cancels_a_pending_shift_tap() {
        let mut c = Capture::new();
        c.set_caps_lock(CapsLock::SwitchLayout);
        tap(&mut c, LSHIFT, 0, EN_WINDOW);
        c.key(&down(CAPS, 100, EN_WINDOW));
        c.key(&up(CAPS, 120, EN_WINDOW));
        assert!(!tap(&mut c, LSHIFT, 200, EN_WINDOW));
    }

    #[test]
    fn mode_change_while_held_keeps_the_press_decision() {
        let mut c = Capture::new();
        c.set_caps_lock(CapsLock::Off);
        assert_eq!(c.key(&down(CAPS, 0, EN_WINDOW)), Effect::Swallow);
        c.set_caps_lock(CapsLock::Normal);
        assert_eq!(c.key(&up(CAPS, 50, EN_WINDOW)), Effect::Swallow);
        assert_eq!(c.key(&down(CAPS, 100, EN_WINDOW)), Effect::Pass);
    }

    #[test]
    fn space_after_a_word_asks_for_a_check() {
        let mut c = Capture::new();
        c.set_detect(true);
        let ru = KeyEvent {
            lang: RU,
            ..EN_WINDOW
        };
        tap(&mut c, A, 0, ru);
        tap(&mut c, B, 100, ru);
        assert_eq!(c.key(&down(VK_SPACE, 200, ru)), Effect::Check);
        let (keys, lang) = c.take_word().unwrap();
        assert_eq!(
            (keys.iter().map(|k| k.vk).collect::<Vec<_>>(), lang),
            (vec![A, B], RU)
        );
        assert!(c.take_word().is_none());
        assert_eq!(typed(&c), [A, B, VK_SPACE]); // the phrase goes on
    }

    #[test]
    fn no_check_when_off_short_or_a_second_space() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        tap(&mut c, B, 100, EN_WINDOW);
        assert_eq!(c.key(&down(VK_SPACE, 200, EN_WINDOW)), Effect::Pass); // off

        c.set_detect(true);
        tap(&mut c, A, 300, EN_WINDOW);
        assert_eq!(c.key(&down(VK_SPACE, 400, EN_WINDOW)), Effect::Pass); // one key
        assert_eq!(c.key(&down(VK_SPACE, 500, EN_WINDOW)), Effect::Pass); // no word
        let win = KeyEvent {
            win: true,
            ..EN_WINDOW
        };
        tap(&mut c, A, 600, EN_WINDOW);
        tap(&mut c, B, 700, EN_WINDOW);
        assert_eq!(c.key(&down(VK_SPACE, 800, win)), Effect::Pass); // Win+Space switches the layout
    }

    #[test]
    fn forget_drops_the_phrase() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        c.forget();
        assert!(typed(&c).is_empty());
    }

    #[test]
    fn converted_phrase_goes_on_in_the_new_layout() {
        let mut c = Capture::new();
        tap(&mut c, A, 0, EN_WINDOW);
        c.converted(RU);
        let ru = KeyEvent {
            lang: RU,
            ..EN_WINDOW
        };
        tap(&mut c, B, 100, ru);
        assert_eq!((typed(&c), c.phrase().1), (vec![A, B], RU));
    }
}
