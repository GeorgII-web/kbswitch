//! Runs on the main thread after `WM_KBSWITCH_CONVERT`: double Shift (`run`) or Shift+Pause (`run_selection`).

use std::sync::atomic::Ordering;
use std::{thread, time::Duration};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_BACK, VK_C, VK_CONTROL, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::buffer::Key;
use crate::hook::{CONVERTING, capture};
use crate::settings::{Hotkey, Settings};
use crate::{charmap, clipboard, input, lang, layout};

/// Is anything selected: Ctrl+C changes the clipboard within this time or not at all.
const PROBE_WAIT_MS: u64 = 150;
/// The selected phrase reaches the clipboard.
const COPY_WAIT_MS: u64 = 500;
/// The app reads the pasted text before the clipboard is restored.
const PASTE_WAIT_MS: u64 = 300;
/// Shift+Pause fires on the Pause key-down: the Shift still held would turn our Ctrl+C into Ctrl+Shift+C.
const SHIFT_RELEASE_WAIT_MS: u64 = 1000;

/// How replacing the typed phrase by select and paste ended.
enum Pasted {
    Done,
    /// Nothing changed and the caret is back where it was: retype instead.
    Untouched,
    /// The selection doesn't match the phrase: leave the text alone.
    Mismatch,
}

/// Typed phrase first; none → the selection; neither → only switch the layout.
pub fn run(settings: &Settings) {
    CONVERTING.store(true, Ordering::Relaxed);
    let hotkey = settings.switch_hotkey;
    // SAFETY: plain Win32 query.
    let hwnd = unsafe { GetForegroundWindow() };
    let (keys, typed_in) = capture().phrase();
    debug_log!(
        "convert: {} keys typed in {typed_in:#06x}, window {:#x} in {:#06x}",
        keys.len(),
        hwnd.0 as usize,
        layout::lang_of(hwnd)
    );
    if !keys.is_empty() {
        let target = lang::other(typed_in);
        let pasted = if settings.select_and_paste && !layout::is_console(hwnd) {
            paste_over(hwnd, keys.len(), target, hotkey)
        } else {
            Pasted::Untouched
        };
        match pasted {
            Pasted::Done => {}
            Pasted::Untouched => retype(hwnd, &keys, typed_in, target, settings),
            Pasted::Mismatch => {
                capture().forget();
                layout::switch(hwnd, target, hotkey);
            }
        }
    } else if layout::is_console(hwnd) || !selection(hwnd, hotkey) {
        layout::switch(hwnd, lang::other(layout::lang_of(hwnd)), hotkey);
    }
    CONVERTING.store(false, Ordering::Relaxed);
}

/// Shift+Pause: the selected text only, for apps that use double Shift themselves; the typed phrase is left alone
/// and forgotten, since the text on screen changes. Nothing selected → only the layout switch.
pub fn run_selection(settings: &Settings) {
    CONVERTING.store(true, Ordering::Relaxed);
    let hotkey = settings.switch_hotkey;
    // SAFETY: plain Win32 query.
    let hwnd = unsafe { GetForegroundWindow() };
    capture().forget();
    // SAFETY: plain Win32 query.
    if !wait(
        || unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) } >= 0,
        SHIFT_RELEASE_WAIT_MS,
    ) {
        debug_log!("Shift still held, no selection conversion");
    } else if layout::is_console(hwnd) || !selection(hwnd, hotkey) {
        layout::switch(hwnd, lang::other(layout::lang_of(hwnd)), hotkey);
    }
    CONVERTING.store(false, Ordering::Relaxed);
}

/// Selects the phrase (Shift+Left × keys), copies it to check it and pastes it converted: one edit instead of a
/// Backspace and a retyped key per character.
fn paste_over(hwnd: HWND, count: usize, target: u16, hotkey: Hotkey) -> Pasted {
    let saved = clipboard::snapshot();
    let before = clipboard::sequence();
    input::send(&input::select_left(count));
    input::send(&input::chord(VK_CONTROL, VK_C));
    if !wait(|| clipboard::sequence() != before, COPY_WAIT_MS) {
        debug_log!("phrase not copied, retyping");
        input::send(&input::select_right(count));
        return Pasted::Untouched;
    }
    let text = clipboard::text().unwrap_or_default();
    if text.chars().count() != count {
        debug_log!("selected {} chars for {count} keys", text.chars().count());
        input::send(&input::right());
        clipboard::restore(saved);
        return Pasted::Mismatch;
    }
    if !clipboard::set_text(&charmap::convert_to(&text, target)) {
        input::send(&input::right());
        clipboard::restore(saved);
        return Pasted::Untouched;
    }
    debug_log!("typed: {count} keys pasted");
    input::send(&input::chord(VK_CONTROL, VK_V));
    layout::switch(hwnd, target, hotkey);
    thread::sleep(Duration::from_millis(PASTE_WAIT_MS));
    clipboard::restore(saved);
    let now = layout::lang_of(hwnd);
    capture().converted(now);
    Pasted::Done
}

/// Erases the phrase and types its text converted to `target` as Unicode characters, key by key (`send_paced`),
/// then switches the layout. The text is right whether or not the app has applied the switch yet: scan-code replay
/// depended on that and left words unchanged or half converted in apps that switch asynchronously (Notepad 11).
fn retype(hwnd: HWND, keys: &[Key], typed_in: u16, target: u16, settings: &Settings) {
    let text = charmap::convert_to(&charmap::typed_text(keys, typed_in), target);
    debug_log!(
        "typed: {} keys retyped as {} chars",
        keys.len(),
        text.chars().count()
    );
    let mut inputs = input::taps(VK_BACK, keys.len());
    inputs.extend(input::unicode(&text));
    if !input::send_paced(&inputs, settings.retype_gap_ms) {
        debug_log!("retype blocked (elevated window?)");
    }
    if !layout::switch(hwnd, target, settings.switch_hotkey) {
        debug_log!("layout switch not confirmed");
    }
    // the text on screen is in `target` now, whatever the window's layout says
    capture().converted(target);
}

/// Copies the selection, converts it and pastes it back; false when nothing was selected.
fn selection(hwnd: HWND, hotkey: Hotkey) -> bool {
    let saved = clipboard::snapshot();
    let before = clipboard::sequence();
    input::send(&input::chord(VK_CONTROL, VK_C));
    if !wait(|| clipboard::sequence() != before, PROBE_WAIT_MS) {
        return false;
    }
    // whitespace only: an app that copies the empty line when nothing is selected
    let Some(text) = clipboard::text().filter(|t| !t.trim().is_empty()) else {
        clipboard::restore(saved);
        return false;
    };
    let (converted, target) = charmap::convert(&text);
    debug_log!("selection: {} chars", text.chars().count());
    if clipboard::set_text(&converted) {
        input::send(&input::chord(VK_CONTROL, VK_V));
        layout::switch(hwnd, target, hotkey);
        thread::sleep(Duration::from_millis(PASTE_WAIT_MS));
    }
    clipboard::restore(saved);
    true
}

fn wait(done: impl Fn() -> bool, ms: u64) -> bool {
    for _ in 0..ms / 10 {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    done()
}
