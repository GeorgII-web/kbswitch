//! Low-level keyboard and mouse hooks on their own thread, so a running conversion never blocks them. The hooks
//! only read the Windows state into a `KeyEvent`; `capture` decides.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, mpsc};
use std::thread;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT,
    MSG, PostThreadMessageW, SetWindowsHookExW, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
};

use crate::capture::{Capture, Effect, KeyEvent};
use crate::input::KBSWITCH_MARKER;
use crate::layout;

/// Posted to the main thread when double Shift fires; `wparam` = `SELECTION_ONLY` for Shift+Pause.
pub const WM_KBSWITCH_CONVERT: u32 = WM_APP + 1;
pub const SELECTION_ONLY: usize = 1;
/// Posted to the main thread when Caps Lock switches the layout.
pub const WM_KBSWITCH_SWITCH: u32 = WM_APP + 2;
/// Posted to the main thread when a Space ended a word to check.
pub const WM_KBSWITCH_CHECK: u32 = WM_APP + 3;

pub static ENABLED: AtomicBool = AtomicBool::new(true);
pub static CONVERTING: AtomicBool = AtomicBool::new(false);
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
static CAPTURE: Mutex<Capture> = Mutex::new(Capture::new());

pub fn capture() -> MutexGuard<'static, Capture> {
    CAPTURE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Installs the hooks on a new thread and returns once they are in place.
pub fn start(main_thread: u32) -> Result<(), String> {
    MAIN_THREAD.store(main_thread, Ordering::Relaxed);
    let (report, installed) = mpsc::channel();
    thread::spawn(move || {
        let result = install();
        let ok = result.is_ok();
        let _ = report.send(result);
        // Low-level hooks are called through this thread's message loop.
        let mut msg = MSG::default();
        // SAFETY: standard message loop on this thread.
        while ok && unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            // SAFETY: dispatching the message just received.
            unsafe { DispatchMessageW(&msg) };
        }
    });
    installed.recv().map_err(|e| e.to_string())?
}

fn install() -> Result<(), String> {
    // SAFETY: the callbacks live as long as the process; Windows removes the hooks when this thread ends.
    unsafe {
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0)
            .and_then(|_| SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0))
            .map(|_| ())
            .map_err(|e| format!("Installing the keyboard and mouse hooks failed: {e}"))
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if kb.dwExtraInfo != KBSWITCH_MARKER {
            let msg = wparam.0 as u32;
            let event = read_event(kb, msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN);
            let effect = capture().key(&event);
            match effect {
                Effect::Pass => {}
                Effect::Convert => request_convert(0),
                Effect::ConvertSelection => {
                    request_convert(SELECTION_ONLY);
                    return LRESULT(1);
                }
                Effect::Check => post(WM_KBSWITCH_CHECK),
                // a non-zero result drops the key: no app sees it and Caps Lock doesn't toggle
                Effect::Swallow => return LRESULT(1),
                Effect::SwitchLayout => {
                    post(WM_KBSWITCH_SWITCH);
                    return LRESULT(1);
                }
            }
        }
    }
    // SAFETY: passing the unchanged arguments on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    const CLICKS: [u32; 4] = [
        WM_LBUTTONDOWN,
        WM_RBUTTONDOWN,
        WM_MBUTTONDOWN,
        WM_XBUTTONDOWN,
    ];
    if code == HC_ACTION as i32 && CLICKS.contains(&(wparam.0 as u32)) {
        capture().click();
    }
    // SAFETY: passing the unchanged arguments on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn held(vk: VIRTUAL_KEY) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetAsyncKeyState(i32::from(vk.0)) < 0 }
}

fn read_event(kb: &KBDLLHOOKSTRUCT, down: bool) -> KeyEvent {
    // SAFETY: plain Win32 query.
    let window = unsafe { GetForegroundWindow() };
    KeyEvent {
        vk: kb.vkCode as u16,
        scan: kb.scanCode as u16,
        down,
        time: kb.time,
        shift: held(VK_SHIFT),
        ctrl_alt: held(VK_CONTROL) || held(VK_MENU),
        win: held(VK_LWIN) || held(VK_RWIN),
        window: window.0 as usize,
        lang: layout::lang_of(window),
    }
}

fn request_convert(what: usize) {
    if ENABLED.load(Ordering::Relaxed) && !CONVERTING.load(Ordering::Relaxed) {
        post_with(WM_KBSWITCH_CONVERT, what);
    }
}

fn post(msg: u32) {
    post_with(msg, 0);
}

fn post_with(msg: u32, wparam: usize) {
    // SAFETY: posting a message with plain values.
    let _ = unsafe {
        PostThreadMessageW(
            MAIN_THREAD.load(Ordering::Relaxed),
            msg,
            WPARAM(wparam),
            LPARAM(0),
        )
    };
}
