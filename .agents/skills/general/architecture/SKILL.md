---
name: architecture
description: Architecture of kbswitch — every module in `src/` with its public items and callers, the two threads and the four thread messages (`WM_KBSWITCH_CONVERT` with `SELECTION_ONLY`, `_SWITCH`, `_CHECK`, `WM_TIMER`), the statics in `src/hook.rs`, the startup and shutdown sequence in `src/main.rs`, the invariants and their reasons, code rules, dependencies and crate features, the `scripts/` folder. Use when making any code change, adding a module, a message, a dependency or a script, or deciding where new code belongs.
---

# Architecture

Map of the whole app and the rules every change keeps. Entrypoint `src/main.rs`; the per-step and per-subsystem
skills linked from `AGENTS.md` hold the details of each module, this skill holds how they fit together.

## Modules, public items, callers

Pure modules (no Win32) build and test on Linux (`cargo test`, 74 tests); the others are `#[cfg(windows)]`. The
`cfg_attr(not(windows), allow(dead_code))` on the pure modules exists because on Linux nothing calls them.

| File | Public items | Called from |
|---|---|---|
| `main.rs` | `debug_log!` macro, `TICK_MS` 150, `main`, `on_console_exit`, `fail` | process start |
| `app.rs` | `App::new`, `convert(selection_only)`, `check_word`, `switch_layout`, `tick`, `menu -> bool`, `exit` | `main` message loop |
| `hook.rs` | `WM_KBSWITCH_CONVERT/_SWITCH/_CHECK`, `SELECTION_ONLY`, statics `ENABLED`, `CONVERTING`, `capture()`, `start(main_thread)` | `main` (start), `app` (ENABLED, capture), `convert` (CONVERTING, capture) |
| `capture.rs` (pure) | `KeyEvent`, `Effect` {Pass, Convert, ConvertSelection, Check, Swallow, SwitchLayout}, `Capture` {`new`, `key`, `set_detect`, `take_word`, `set_caps_lock`, `set_timing`, `click`, `phrase`, `converted`, `forget`} | `hook` callbacks, `app::apply`, `app::check_word`, `convert` |
| `buffer.rs` (pure) | `Key {vk, scan, shift}`, `Buffer` {`new`, `key_down`, `reset`, `keys`, `last_word`, `lang`, `set_lang`} | `capture` only |
| `trigger.rs` (pure) | `TAP_MAX_MS` 250, `DOUBLE_MS` 300, `DoubleShift` {`new`, `set_timing`, `shift_down`, `shift_up -> bool`, `other`} | `capture` only; the constants also `settings` defaults |
| `charmap.rs` (pure) | `convert(text) -> (String, lang)`, `convert_to(text, target)`, `typed_text(keys, lang)` | `convert` (selection, paste), `detect` |
| `lang.rs` (pure) | `EN` 0x0409, `RU` 0x0419, `from_hkl`, `other` | `layout`, `convert`, `icon`, `detect`, `spell`, `app` |
| `icon.rs` (pure) | `SIZE` 16, the four colors, `Look {color, filled}` {`new(language, enabled, error)`, `rgba`} | `tray` |
| `detect.rs` (pure) | `MIN_KEYS` 2, trait `Lexicon::is_word(lang, text) -> Option<bool>`, `wrong_layout` | `capture` (MIN_KEYS), `app::check_word`, `spell` (impl) |
| `settings.rs` (std only) | `FILE`, `Hotkey`, `CapsLock`, `Settings` {`parse`, `to_json`}, `path`, `load`, `save`, `load_or_create`, `modified` | `app`, `convert` (Hotkey), `capture` (CapsLock), `tray` |
| `convert.rs` | `run(&Settings)`, `run_selection(&Settings)` | `app::convert` |
| `input.rs` | `KBSWITCH_MARKER`, `down`, `up`, `chord`, `taps`, `select_left`, `select_right`, `right`, `unicode`, `send -> bool`, `send_paced` | `convert`, `layout` (hotkey chord), `led` (taps), `hook` (marker) |
| `clipboard.rs` | `Snapshot`, `sequence`, `text`, `snapshot`, `set_text`, `restore` | `convert` only |
| `layout.rs` | `lang_of(hwnd)`, `foreground_lang`, `switch(hwnd, target, hotkey) -> bool`, `toggle(hotkey)`, `is_console(hwnd)` | `hook` (lang_of per key), `convert`, `app` (tick, switch_layout) |
| `led.rs` | `scroll_lock(on)`, `toggle_scroll_lock`, `caps_lock_off` | `app`, `main::on_console_exit` |
| `spell.rs` | `Spelling::new -> Result`, `impl Lexicon` | `app::apply` (created once) |
| `tray.rs` | `Tray` {`new`, `handle -> Action`, `show`, `set_language`}, `Action` {None, Enabled, Led, Detect, OpenSettings, Exit} | `app` |
| `autostart.rs` | `is_enabled`, `set(on)` | `tray` only; the registry is the store, not `settings.json` |

Where new code goes: a decision → a pure module with a unit test; a Win32 call → the module that owns that API
(`input` for `SendInput`, `clipboard`, `layout`, `led`, `spell`, `autostart`). Win32 modules read state and call
the API, they decide nothing. A new pure module needs the `cfg_attr(not(windows), allow(dead_code))` line in
`main.rs`.

## Threads

- **Hook thread** (`hook::start`): spawned once, installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` (`install`), reports
  the result over an `mpsc` channel so `start` returns only when the hooks exist (an error ends in `fail`), then
  runs its own `GetMessageW` loop: low-level hooks are called on the thread that installed them, through its
  message loop. `keyboard_proc` skips events carrying `KBSWITCH_MARKER`, builds a `KeyEvent` (`read_event`: vk,
  scan, down from `WM_KEYDOWN`/`WM_SYSKEYDOWN`, time, `GetAsyncKeyState` for Shift, Ctrl/Alt, Win, the foreground
  window and `layout::lang_of` it), locks `Capture`, and carries out the `Effect`: `Convert` →
  `request_convert(0)`, `ConvertSelection` → `request_convert(SELECTION_ONLY)` and `LRESULT(1)`, `Check` →
  `post(WM_KBSWITCH_CHECK)`, `Swallow` → `LRESULT(1)`, `SwitchLayout` → post `WM_KBSWITCH_SWITCH` and
  `LRESULT(1)`. `mouse_proc` calls `capture().click()` on any button down. Callbacks never `SendInput`, touch the
  clipboard or sleep.
- **Main thread** (`main` + `App`): tray, timer, conversions, Caps Lock switches, word checks with COM. It may
  sleep and wait inside a conversion because the hooks keep running on their thread. Converting on the hook
  thread would deadlock: our injected Ctrl+C would wait for our own blocked hook.

Messages, all posted with `PostThreadMessageW` to `MAIN_THREAD` and matched only with `msg.hwnd.is_invalid()`:

| Message | Posted by | wParam | Handler |
|---|---|---|---|
| `WM_KBSWITCH_CONVERT` = `WM_APP+1` | `request_convert`, only if `ENABLED && !CONVERTING` | 0 double Shift or Pause, `SELECTION_ONLY` = 1 Shift+Pause | `App::convert(selection_only)` → `convert::run` / `run_selection` |
| `WM_KBSWITCH_SWITCH` = `WM_APP+2` | Caps Lock in `switch_layout` mode | 0 | `App::switch_layout` → `layout::toggle` |
| `WM_KBSWITCH_CHECK` = `WM_APP+3` | a Space ended a word, detection on | 0 | `App::check_word` → `capture().take_word`, `detect::wrong_layout`, start the blink |
| `WM_TIMER` | `SetTimer(None, 0, TICK_MS)` every 150 ms | | `App::tick` |

Thread messages are lost while a modal loop runs (tray menu open): a double Shift then does nothing. Windows
drops a hook whose callback exceeds `LowLevelHooksTimeout` (about 1 s); keep callbacks under 1 ms. After every
message the loop drains `MenuEvent::receiver()`; `App::menu` returning false posts `WM_QUIT`.

## Statics (`src/hook.rs`)

| Static | Type | Meaning |
|---|---|---|
| `ENABLED` | `AtomicBool`, true | the "Enabled" setting; checked only in `request_convert`, so the state machine, Caps Lock and detection keep running while off |
| `CONVERTING` | `AtomicBool` | set by `convert::run`/`run_selection` for their duration; a second trigger meanwhile is dropped |
| `MAIN_THREAD` | `AtomicU32` | thread id for `PostThreadMessageW`, set in `start` |
| `CAPTURE` | `Mutex<Capture>` | the one shared state; `capture()` recovers from poisoning |

All `Relaxed`: each is an independent flag, nothing is published through them.

## Startup and shutdown (`src/main.rs`)

1. `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`: release has no console; debug keeps one
   for `debug_log!` (`eprintln!` under `cfg!(debug_assertions)`, never typed text, only counts).
2. `CreateMutexW("Local\\kbswitch-single-instance")`: `ERROR_ALREADY_EXISTS` → return silently; the handle is kept
   for the process lifetime on purpose.
3. `SetConsoleCtrlHandler(on_console_exit)`: Ctrl+C or closing the debug console turns Scroll Lock off and exits.
4. `App::new`: `settings::path`, `load_or_create` (defaults written on first start; an error is kept for the tray),
   `Tray::new`, `apply` (ENABLED, timings, Caps Lock mode, detection with `Spelling::new`, LED, tray), the tray
   color from `layout::foreground_lang`. An error → `fail` (a `MessageBoxW`, because release has no console).
5. `hook::start(GetCurrentThreadId())`; an error → `fail`.
6. `SetTimer` 150 ms, then the message loop above.
7. After `WM_QUIT`: `App::exit` → `led_off` turns Scroll Lock off only if kbswitch turned it on. The hooks die with
   the process; the mutex is released by the OS.

## Invariants and why

- Every injected `INPUT` carries `KBSWITCH_MARKER` (`0x4B42_5357`) in `dwExtraInfo` and is built in `input.rs`
  only; `keyboard_proc` skips it. Otherwise the retype, Ctrl+C/V, the LED taps and the hotkey chords would be
  recorded as a phrase or fire the trigger again.
- `Capture` is locked only briefly, never across `SendInput`, the clipboard or a sleep: the hook thread locks it
  on every key and would stall, and Windows would drop the hook.
- The hook passes every key on except what `Effect::Swallow`, `SwitchLayout`, `ConvertPause` and
  `ConvertSelection` drop (Caps Lock in its modes, Pause alone and of Shift+Pause). Dropping anything else would eat the user's typing.
- The phrase layout, not the window's, decides the conversion target (`Buffer::lang`): the user may switch by hand
  before the double Shift.
- `CONVERTING` guards reentry; `ENABLED` is read on the hook thread but changed on the main thread.
- Every `unsafe` block has a `// SAFETY:` comment stating why the call is sound.
- Win32 modules take no decisions; pure modules have no `windows` imports (the exception: `input.rs` tests
  run on Windows only and check the built `INPUT`s).

## Code rules

- Dependencies (`Cargo.toml`): `windows` 0.62 with the features `Win32_Foundation`, `_UI_WindowsAndMessaging`,
  `_UI_Input_KeyboardAndMouse`, `_System_Console`, `_System_DataExchange`, `_System_Memory`, `_System_Registry`,
  `_System_Threading`, `_System_LibraryLoader`, `_Security`, `_Globalization`, `_System_Com`; `tray-icon` 0.26;
  both Windows-only. `serde` + `serde_json` on every target for the settings. A new API means its feature named
  here; ask before adding another crate.
- Release profile: `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`. The app never panics on a failed
  conversion: Win32 results are logged with `debug_log!` and skipped; only startup errors go to `fail`.
- Edition 2024. `cargo fmt` and `clippy --all-targets -D warnings` on both targets before any change is reported
  done ([windows-build](../../quality-gate/windows-build/SKILL.md)).
- Work and temp files go to the git-ignored `var/`; build output to `target/`.
- Every code change updates the skill that owns the changed code, in the same commit (`AGENTS.md`).

## Scripts

| Script | Does |
|---|---|
| `scripts/cargo-gnu.sh` | cargo for `x86_64-pc-windows-gnu` in WSL with a MinGW unpacked to `~/.local/opt/mingw-w64`, static CRT; the release exe used everywhere |
| `scripts/cargo-win.sh` | Windows `cargo.exe` (MSVC) on the WSL repo, `CARGO_TARGET_DIR=C:\dev\kbswitch-target` through `WSLENV`; native tests |
| `scripts/e2e-notepad.ps1` | end-to-end check on Windows: starts a local copy of the exe, types into an Edit control, checks double Shift, Pause, Shift+Pause and the clipboard |
| `scripts/install-win.sh` | copies the release exe to `%LOCALAPPDATA%\kbswitch`, unblocks and restarts it (no security prompt from `\\wsl.localhost`) |

Details, gates and the manual checklist: [windows-build](../../quality-gate/windows-build/SKILL.md).
