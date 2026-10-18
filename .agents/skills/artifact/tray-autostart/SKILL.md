---
name: tray-autostart
description: The app shell of kbswitch, function by function — `src/main.rs` (single-instance mutex, message loop, `fail`), `src/app.rs` (`App` fields, `tick`, `apply`, `save`, reload, menu actions), `src/settings.rs` (every key, default, clamp, path rule, parse, save), `src/tray.rs` and `src/icon.rs` (menu items, `Action`, `Look`, the two-letter icon font, tooltip), `src/autostart.rs` (HKCU Run value), plus every test. Use when changing the tray menu or icon, a setting or the settings file, autostart, startup, shutdown or the main loop.
---

# Tray, settings and autostart

Everything the user sees besides the conversion itself. Entry `src/main.rs`; state `src/app.rs`; file
`src/settings.rs`; shell `src/tray.rs` + `src/icon.rs`; registry `src/autostart.rs`.

## main.rs

- `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`: release has no console window.
- `debug_log!`: `eprintln!` only when `cfg!(debug_assertions)`; never log typed text, only counts.
- Modules: pure ones (`buffer`, `capture`, `charmap`, `detect`, `icon`, `lang`, `settings`, `trigger`) compile on
  Linux with `allow(dead_code)`; Win32 ones are `#[cfg(windows)]`. Non-Windows `main` prints "runs on Windows only".
- `TICK_MS = 150`: the main-thread timer (`SetTimer(None, 0, TICK_MS, None)`), one `WM_TIMER` per tick.
- Order in `main`: (1) `CreateMutexW(None, true, "Local\kbswitch-single-instance")`; `ERROR_ALREADY_EXISTS` → return
  silently (second instance). The handle is kept for the process lifetime on purpose. (2) `SetConsoleCtrlHandler
  (on_console_exit)`: Ctrl+C or closing a debug console turns Scroll Lock off and exits. (3) `App::new()`, error →
  `fail`. (4) `hook::start(GetCurrentThreadId())`, error → `fail`. (5) timer, then the loop.
- Loop (`GetMessageW`): thread messages (`msg.hwnd.is_invalid()`) are dispatched by hand: `WM_KBSWITCH_CONVERT` →
  `app.convert(wParam == hook::SELECTION_ONLY)`, `WM_KBSWITCH_SWITCH` → `switch_layout`, `WM_KBSWITCH_CHECK` →
  `check_word`, `WM_TIMER` → `tick`; everything else `TranslateMessage` + `DispatchMessageW` (the tray's hidden
  window). After each message every pending `MenuEvent` is drained; `app.menu` false → `PostQuitMessage(0)`.
  After the loop `app.exit()`.
- `fail(text)`: logs, then `MessageBoxW(None, text, "kbswitch", MB_ICONERROR)`; the caller returns, so the exe exits.
  Settings errors never reach `fail`: they show as an amber icon.

## App (app.rs)

| Field | Meaning |
|---|---|
| `settings` | the `Settings` in use |
| `path` | `settings::path()` resolved once at start |
| `modified` | mtime of the file when last loaded or saved; the reload compares against it |
| `polled` | `Instant` of the last mtime check; `SETTINGS_POLL = 1 s` |
| `led_ru` | `Option<bool>`: what kbswitch last set Scroll Lock to; `None` = not set by us. Set only on a layout change, so the LED never fights the user or a stale read |
| `caps_lock` | the `CapsLock` mode last applied; `caps_lock_off()` runs once per mode change |
| `spelling` | `Option<Spelling>` (step 4); created once detection is on, `None` while off or the checkers are missing |
| `blink` | Scroll Lock toggles left of a warning blink; `BLINK_TOGGLES = 6` = three blinks, ends as it was |
| `tray` | `tray::Tray` |

- `new()`: `settings::load_or_create(path)` → `Tray::new(&settings)` (failure → `Err("Creating the tray icon failed:
  …")`) → `apply(error)` → `tray.set_language(layout::foreground_lang())`.
- `convert(selection_only)`: `convert::run_selection` (Shift+Pause) or `convert::run` (double Shift), see
  [step-3-convert](../../domain/step-3-convert/SKILL.md).
- `check_word()`: `capture().take_word()`; with a `spelling` and `detect::wrong_layout` true → `blink =
  BLINK_TOGGLES + blink % 2` (restarted mid-blink with the LED inverted: one toggle more, so it still ends as it was).
- `switch_layout()`: Caps Lock in `switch_layout` mode → `layout::toggle(switch_hotkey)`, logs when not confirmed.
- `tick()` every 150 ms, in this order: (1) if `polled.elapsed() >= SETTINGS_POLL` → `reload_if_changed`; (2) `tray.
  set_language(layout::foreground_lang())`; (3) `blink > 0` → `led::toggle_scroll_lock()`, `blink -= 1` (blinking
  wins over the layout LED; an even count ends as it was, so the tracking stays right); else if `settings.led` →
  `ru = current != lang::EN && current != 0` (on for any layout but EN; 0 = not read yet stays off), and only when `led_ru != Some(ru)` → `led::scroll_lock(ru)`, remember it.
- `menu(event)`: `tray.handle` → `Exit` returns false; `Enabled(on)` / `Led(on)` / `Detect(on)` set the field and
  `save()`; `OpenSettings` → `open_settings`; `None` nothing. Autostart never reaches `App`.
- `exit()` = `led_off()`: Scroll Lock off only if `led_ru == Some(true)`, then `led_ru = None`.
- `apply(error)`: (1) `hook::ENABLED = settings.enabled`; (2) `detect_wrong_layout` and no `spelling` yet →
  `Spelling::new()`, an `Err` becomes `detect_error`; `detect = detect_wrong_layout && spelling.is_some()`; (3) one
  short `hook::capture()` lock: `set_timing(tap_max_ms, double_ms)`, `set_caps_lock`, `set_detect`; (4) a mode other
  than `Normal` applied for the first time → `led::caps_lock_off()`; remember the mode; (5) `!settings.led` →
  `led_off()`; (6) `tray.show(&settings, error.or(detect_error))`. The settings error wins over the spell-checker one.
- `save()`: `settings::save`, an `Err` becomes "settings.json not saved: …"; `modified = settings::modified(path)`
  so the own write isn't reloaded; `apply(error)`.
- `reload_if_changed()`: `modified(path)` `None` (file missing) or equal → return (a missing file counts as unchanged
  and is written again on the next save). Else remember it; `load` Ok → replace settings, `apply(None)`; Err →
  keep the settings in use, `apply(Some(e))` (amber icon until the file is fixed).
- `open_settings()`: write the file first when missing (`save`), then `notepad.exe <path>` spawned, error logged.

## Settings (settings.rs)

`FILE = "settings.json"`. `Settings` derives `Serialize, Deserialize` with `#[serde(default)]`: missing keys take the
default, unknown keys are ignored and dropped on the next save. Enums use `rename_all = "snake_case"`.

| Key | Type / values | Default | Meaning |
|---|---|---|---|
| `enabled` | bool | `true` | double Shift and Shift+Pause convert; `hook::ENABLED`. Off: step 2 keeps tracking, LED keeps working |
| `led` | bool | `true` | Scroll Lock shows the layout, on for any layout but EN (0 = not read yet counts as EN) |
| `tap_max_ms` | u32, clamped 50–1000 | `TAP_MAX_MS` 250 | longest Shift press that is a tap |
| `double_ms` | u32, clamped 100–1000 | `DOUBLE_MS` 300 | longest gap between the taps |
| `switch_hotkey` | `Hotkey`: `alt_shift`, `ctrl_shift`, `win_space` | `alt_shift` | sent when a window ignores `WM_INPUTLANGCHANGEREQUEST` |
| `select_and_paste` | bool | `false` | `false`: Backspace and retype, clipboard untouched; `true`: select and paste, one edit but a slower growing selection |
| `caps_lock` | `CapsLock`: `normal`, `off`, `switch_layout` | `normal` | `off`: key does nothing (the tray box "Disable Caps Lock"); `switch_layout`: switches EN/RU, Shift+Caps Lock stays Caps Lock (file only) |
| `detect_wrong_layout` | bool | `true` | step 4 blink; needs the en-US and ru-RU spell checkers |
| `retype_gap_ms` | u32 | `20` (`RETYPE_GAP_MS`) | pause between the keys of a retype, clamped to 0–100 (`min(100)`); Notepad 11 drops keys faster than ~15 ms, `0` = one batch |

- `parse(text)`: strips a UTF-8 BOM (older Notepad writes one), `serde_json::from_str`, error → `"settings.json:
  <serde error with line>"`, then clamps both timings. `to_json()`: pretty JSON plus a trailing newline.
- Path rule, `resolve(exe_dir, appdata)`:

| exe dir has `settings.json` | `%APPDATA%` set | Result |
|---|---|---|
| yes | any | next to the exe (portable) |
| no | yes | `%APPDATA%\kbswitch\settings.json` |
| no | no | next to the exe |
| no exe dir | no | `settings.json` in the cwd |

- `load(path)`: read + parse, an io error also prefixed with the file name. `save(path, s)`: `create_dir_all`, write
  `settings.json.tmp`, `rename` over the file (a crash never leaves half a file). `load_or_create`: missing file →
  defaults written, a failed write becomes "settings.json not created: …"; broken file → defaults in memory, the error
  reported, the file untouched. `modified(path)`: the mtime, `None` when unreadable.
- New setting: field with default here, a parse test, applied in `App::apply`, row above, checklist row in
  [windows-build](../../quality-gate/windows-build/SKILL.md).

## Tray (tray.rs, icon.rs)

Menu, top to bottom (`tray-icon` crate, `CheckMenuItem` / `MenuItem`, a separator before the last two):

| Item | Field | `Action` | Does |
|---|---|---|---|
| Enabled | `enabled` | `Enabled(checked)` | `enabled` setting |
| Scroll Lock LED shows not EN | `led` | `Led(checked)` | `led` setting |
| Blink LED on wrong layout | `detect` | `Detect(checked)` | `detect_wrong_layout` setting |
| Disable Caps Lock | `caps_off` | `CapsOff(checked)` | `caps_lock` = `off` when ticked, `normal` when unticked (`App::menu`); ticked only while the setting is `off`, so `switch_layout` from the file shows unticked and stays until the box is ticked, which turns it into `off` |
| Start with Windows | `autostart` | `None` | handled inside `handle`: `autostart::set(checked)`, error logged, then the box is re-read from `is_enabled()` so it shows the registry truth |
| Settings… | `settings` | `OpenSettings` | Notepad on the file |
| Exit | `exit` | `Exit` | ends the loop |

- `Tray::new(settings)`: checkboxes from the settings (autostart from the registry), tooltip "kbswitch", icon
  `Look::new(0, enabled, false)` (grey until the first `set_language`). Fields `language`, `active`, `error` are
  what the picture is drawn from; `look` is the picture shown now.
- `show(settings, error)`: checkboxes, `active`, `error = error.is_some()`, `redraw`, tooltip "kbswitch" or
  "kbswitch: <error>" cut by `tooltip()` to 127 UTF-16 units (the shell buffer is 128 with the NUL); a char never
  split. `set_language(lang)` every tick → `redraw`. `redraw`: new `Look`; `set_icon` only when it differs.
- `Look { color, filled, code }` (`icon.rs`): `error` → `ERROR_AMBER` filled whatever the layout; else color by
  language `lang::EN` → `EN_BLUE` (#2563EB), `lang::RU` → `RU_RED` (#DC2626), anything else (German, 0 = not read
  yet) → `OTHER_GREY` (#9CA3AF); `filled = enabled`. `code` = `icon::code(language)`: the ISO 639-1 code of the
  primary language (`language & 0xFF`, so Swiss German is `DE` too) from a 21-entry table (`EN`, `RU`, `DE`, `FR`,
  `ES`, `IT`, `NL`, `PL`, `PT`, `TR`, `UK`, `CS`, `FI`, `SV`, `DA`, `NO`, `HU`, `EL`, `HE`, `BE`, `KK`), else `??`.
- `rgba()`: `SIZE = 32` (16 px left no room for two readable letters; Windows scales it to the tray), row by row:
  square first (filled, or a `FRAME = 3` px frame around `CLEAR` when disabled), then the two letters over it,
  `WHITE` on a filled square, the layout color inside the frame. Letters come from `glyph(c)`, a 5×7 font for A–Z
  and `?` (one byte per row, bit 4 = left column, `every_glyph_fits_five_columns` checks `< 0x20`), scaled
  `SCALE = 2` to 10×14 each with a `GAP = 1` font column (2 px) between, centered by `LEFT`/`TOP`
  (`text_pixel(x, y)` maps a pixel back to glyph, row and column). No font or image crate. `picture()` builds the
  `Icon` from it.

## Autostart (autostart.rs)

- `RUN_KEY = HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, `VALUE = "kbswitch"`, data = `command()` = the
  quoted full path of `current_exe()`. No admin rights (HKCU).
- `registered()`: `RegGetValueW` twice (size, then data, `RRF_RT_REG_SZ`), cut at the first NUL. `is_enabled()`:
  true only when the value equals this exe's command: a value for another copy (old build path, the e2e copy)
  shows unchecked, and checking it rewrites the path. `set(on)`: `RegOpenKeyExW(KEY_SET_VALUE)`, then
  `RegSetValueExW(REG_SZ, UTF-16 + NUL)` or `RegDeleteValueW`, key closed either way.
- The registry is the truth: nothing about autostart is in `settings.json`. Run from a local copy
  (`scripts/install-win.sh`), not `\\wsl.localhost`, see [windows-build](../../quality-gate/windows-build/SKILL.md).

## Shutdown

Exit ends the loop → `App::exit` turns the LED off if kbswitch turned it on → the tray icon drops with `App`;
Windows removes the hooks with the process. Ctrl+C in a debug console does the same through `on_console_exit`.
Not handled: the LED on Windows logoff.

## Tests

All cases: [test-cases](../../quality-gate/test-cases/SKILL.md), section settings/icon/tray. A new setting must
keep `defaults_round_trip_through_json` and `missing_keys_take_defaults_and_unknown_keys_are_ignored` green and
add its own parse test; `app.rs`, `autostart.rs` and `main.rs` have no unit tests: the manual checklist covers them.
