# kbswitch

A small Windows tray app that fixes text typed in the wrong keyboard layout (English ↔ Russian), like Punto
Switcher or Caramba Switcher. One `.exe`, no installer, no console window.

## Download

Get `kbswitch.exe` from the [latest release](https://github.com/GeorgII-web/kbswitch/releases/latest), put it in
any folder (for example `C:\Program Files\kbswitch`) and run it. The exe is not signed, so Windows SmartScreen may
ask once: "More info" → "Run anyway".

Requirements: Windows 10/11 x64 with the English (US) and Russian layouts installed.

## Use

| Keys | What happens |
|---|---|
| Shift, Shift (double tap) | converts the phrase typed since the last reset, or the selected text when nothing was typed, and switches the layout |
| Pause (Break) | the same as double Shift |
| Shift+Pause | converts only the selected text (for apps that use double Shift themselves) |

- The tray icon shows the current layout: `EN` (blue) or `RU` (red).
- The Scroll Lock LED is on for any layout but EN.
- A word typed in the wrong layout blinks the Scroll Lock LED; kbswitch never converts on its own.

Tray menu: Enabled, Scroll Lock LED shows not EN, Blink LED on wrong layout, Disable Caps Lock, Start with Windows,
Settings…, Exit.

## Settings

`settings.json` in `%APPDATA%\kbswitch` (or next to the exe, when a file is there). It is created with defaults on
first start and applied on save, no restart needed.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | double Shift, Pause and Shift+Pause convert |
| `led` | `true` | Scroll Lock LED shows the layout |
| `tap_max_ms` | `250` | longest Shift press that counts as a tap (50–1000) |
| `double_ms` | `300` | longest gap between the two taps (100–1000) |
| `switch_hotkey` | `alt_shift` | `alt_shift`, `ctrl_shift` or `win_space`: the system layout hotkey, for windows that ignore the switch request |
| `select_and_paste` | `false` | `false`: erase and retype; `true`: select and paste through the clipboard |
| `caps_lock` | `normal` | `normal`, `off` or `switch_layout` (Shift+Caps Lock stays Caps Lock) |
| `detect_wrong_layout` | `true` | blink on a wrong-layout word; needs the en-US and ru-RU spell checkers |
| `retype_gap_ms` | `20` | pause between retyped keys (0–100); some apps drop faster input |

## Build

From Linux or WSL, a static exe through MinGW (downloaded to `~/.local/opt/mingw-w64` on first use, no sudo):

```bash
rustup target add x86_64-pc-windows-gnu
scripts/cargo-gnu.sh build --release   # target/x86_64-pc-windows-gnu/release/kbswitch.exe
```

On Windows: `cargo build --release` with the MSVC toolchain.

## Release

Pushing a tag `v*` runs `.github/workflows/release.yml`: tests, the static exe, and a GitHub release with
`kbswitch.exe` attached.

```bash
git tag v0.1.0 && git push origin v0.1.0
```
