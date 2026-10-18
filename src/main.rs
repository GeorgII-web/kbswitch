#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Debug builds print to the console; release builds have none and never log typed text.
#[cfg_attr(not(windows), allow(unused_macros))]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod autostart;
#[cfg_attr(not(windows), allow(dead_code))]
mod buffer;
#[cfg_attr(not(windows), allow(dead_code))]
mod capture;
#[cfg_attr(not(windows), allow(dead_code))]
mod charmap;
#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod convert;
#[cfg_attr(not(windows), allow(dead_code))]
mod detect;
#[cfg(windows)]
mod hook;
#[cfg_attr(not(windows), allow(dead_code))]
mod icon;
#[cfg(windows)]
mod input;
#[cfg_attr(not(windows), allow(dead_code))]
mod lang;
#[cfg(windows)]
mod layout;
#[cfg(windows)]
mod led;
#[cfg_attr(not(windows), allow(dead_code))]
mod settings;
#[cfg(windows)]
mod spell;
#[cfg(windows)]
mod tray;
#[cfg_attr(not(windows), allow(dead_code))]
mod trigger;

/// Main-thread timer: the LED follows the layout, `settings.json` is polled for changes.
#[cfg(windows)]
const TICK_MS: u32 = 150;

#[cfg(windows)]
fn main() {
    use tray_icon::menu::MenuEvent;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Console::SetConsoleCtrlHandler;
    use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PostQuitMessage, SetTimer, TranslateMessage, WM_TIMER,
    };
    use windows::core::w;

    // SAFETY: the mutex handle is kept until the process exits on purpose.
    let first = unsafe {
        CreateMutexW(None, true, w!("Local\\kbswitch-single-instance")).is_ok()
            && GetLastError() != ERROR_ALREADY_EXISTS
    };
    if !first {
        return;
    }

    // SAFETY: the handler is a valid extern "system" fn for the whole process lifetime.
    let _ = unsafe { SetConsoleCtrlHandler(Some(on_console_exit), true) };

    let mut app = match app::App::new() {
        Ok(app) => app,
        Err(e) => {
            fail(&e);
            return;
        }
    };
    // SAFETY: plain Win32 query.
    if let Err(e) = hook::start(unsafe { GetCurrentThreadId() }) {
        fail(&e);
        return;
    }
    // SAFETY: the timer posts WM_TIMER to this thread's queue.
    unsafe { SetTimer(None, 0, TICK_MS, None) };
    debug_log!("kbswitch running");

    let mut msg = MSG::default();
    // SAFETY: standard message loop on this thread.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        match msg.message {
            hook::WM_KBSWITCH_CONVERT if msg.hwnd.is_invalid() => {
                app.convert(msg.wParam.0 == hook::SELECTION_ONLY);
            }
            hook::WM_KBSWITCH_SWITCH if msg.hwnd.is_invalid() => app.switch_layout(),
            hook::WM_KBSWITCH_CHECK if msg.hwnd.is_invalid() => app.check_word(),
            WM_TIMER if msg.hwnd.is_invalid() => app.tick(),
            // SAFETY: dispatching the message just received.
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if !app.menu(&event) {
                // SAFETY: ends the loop above.
                unsafe { PostQuitMessage(0) };
            }
        }
    }
    app.exit();
}

/// Ctrl+C or closing the console (debug builds): leave the LED off.
#[cfg(windows)]
unsafe extern "system" fn on_console_exit(_: u32) -> windows::core::BOOL {
    led::scroll_lock(false);
    std::process::exit(0);
}

/// Startup errors: release builds have no console, so show them.
#[cfg(windows)]
fn fail(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};
    use windows::core::{HSTRING, w};

    debug_log!("{text}");
    // SAFETY: both strings outlive the call.
    unsafe { MessageBoxW(None, &HSTRING::from(text), w!("kbswitch"), MB_ICONERROR) };
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kbswitch runs on Windows only");
}
