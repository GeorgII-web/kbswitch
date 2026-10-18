//! Lock keys; their LEDs follow the toggle state. Scroll Lock shows the layout, Caps Lock is turned off when its key
//! gets another job.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_SCROLL,
};

use crate::input;

fn is_on(key: VIRTUAL_KEY) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetKeyState(i32::from(key.0)) & 1 == 1 }
}

/// Presses the key once when its toggle state differs; injected, so the hook lets it through.
fn set(key: VIRTUAL_KEY, on: bool) {
    if is_on(key) != on {
        input::send(&input::taps(key, 1));
    }
}

/// The layout LED: on for any layout but EN.
pub fn scroll_lock(on: bool) {
    set(VK_SCROLL, on);
}

/// One blink step of the wrong-layout warning.
pub fn toggle_scroll_lock() {
    input::send(&input::taps(VK_SCROLL, 1));
}

/// With `caps_lock` off or switching the layout, the key itself can't turn Caps Lock off any more.
pub fn caps_lock_off() {
    set(VK_CAPITAL, false);
}
