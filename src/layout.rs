use std::{thread, time::Duration};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardLayoutList, HKL, INPUT, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, PostMessageW,
    WM_INPUTLANGCHANGEREQUEST,
};

use crate::settings::Hotkey;
use crate::{input, lang};

/// Language id of the window's keyboard layout.
pub fn lang_of(hwnd: HWND) -> u16 {
    // SAFETY: plain Win32 queries; a missing window gives thread 0 = this thread's layout.
    let hkl = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(hwnd, None)) };
    lang::from_hkl(hkl.0 as usize)
}

pub fn foreground_lang() -> u16 {
    // SAFETY: plain Win32 query.
    lang_of(unsafe { GetForegroundWindow() })
}

/// Switches the window to `target` and waits until it applies; true when it did. Falls back to `hotkey` when the
/// window ignores the request; does nothing when `target` is not installed.
pub fn switch(hwnd: HWND, target: u16, hotkey: Hotkey) -> bool {
    if lang_of(hwnd) == target {
        return true;
    }
    let Some(hkl) = installed(target) else {
        debug_log!("layout {target:#06x} not installed");
        return false;
    };
    // SAFETY: posting a message with plain values.
    let posted = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_INPUTLANGCHANGEREQUEST,
            WPARAM(0),
            LPARAM(hkl.0 as isize),
        )
    }
    .is_ok();
    if posted && wait_for(hwnd, target) {
        return true;
    }
    debug_log!("layout request ignored, sending {hotkey:?}");
    input::send(&chord(hotkey));
    wait_for(hwnd, target)
}

fn chord(hotkey: Hotkey) -> [INPUT; 4] {
    match hotkey {
        Hotkey::AltShift => input::chord(VK_LMENU, VK_LSHIFT),
        Hotkey::CtrlShift => input::chord(VK_LCONTROL, VK_LSHIFT),
        Hotkey::WinSpace => input::chord(VK_LWIN, VK_SPACE),
    }
}

/// Switches the foreground window to the other layout of the pair.
pub fn toggle(hotkey: Hotkey) -> bool {
    // SAFETY: plain Win32 query.
    let hwnd = unsafe { GetForegroundWindow() };
    switch(hwnd, lang::other(lang_of(hwnd)), hotkey)
}

fn wait_for(hwnd: HWND, target: u16) -> bool {
    for _ in 0..10 {
        if lang_of(hwnd) == target {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

fn installed(target: u16) -> Option<HKL> {
    // SAFETY: the first call returns the count, the second fills a buffer of that size.
    let list = unsafe {
        let mut list = vec![HKL::default(); GetKeyboardLayoutList(None) as usize];
        let filled = GetKeyboardLayoutList(Some(&mut list)) as usize;
        list.truncate(filled);
        list
    };
    list.into_iter()
        .find(|hkl| lang::from_hkl(hkl.0 as usize) == target)
}

/// Console windows: Ctrl+C would interrupt the running command.
pub fn is_console(hwnd: HWND) -> bool {
    let mut buf = [0u16; 64];
    // SAFETY: the buffer outlives the call and its length is passed with it.
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    is_console_class(&String::from_utf16_lossy(&buf[..len]))
}

fn is_console_class(class: &str) -> bool {
    matches!(
        class,
        "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_classes() {
        assert!(is_console_class("ConsoleWindowClass"));
        assert!(is_console_class("CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(!is_console_class("Notepad"));
        assert!(!is_console_class(""));
    }
}
