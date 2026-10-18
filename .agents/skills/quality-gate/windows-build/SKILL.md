---
name: windows-build
description: Build and quality gates for kbswitch — the WSL gates (Linux tests, fmt, clippy for both targets, the static GNU release exe via `scripts/cargo-gnu.sh` line by line), the automated Windows end-to-end check `scripts/e2e-notepad.ps1` function by function with its pitfalls, installing the exe locally with `scripts/install-win.sh` and why the security prompt appears, the native MSVC build via `scripts/cargo-win.sh`, the WSL interop fix, exe paths, the Windows-only unit tests, the manual checklist, the GitHub release workflow, debugging. Use when building, running tests or the e2e script, installing the exe, publishing a release, before reporting any change done, or when a build, toolchain or test run fails or hangs.
---

# Windows build

The app runs only on Windows; the repo stays in WSL (`~/projects/kbswitch`). Entry points: `scripts/cargo-gnu.sh`
(exe from WSL), `scripts/e2e-notepad.ps1` (real-input check on Windows), `scripts/install-win.sh` (local copy),
`scripts/cargo-win.sh` (native MSVC build). Code layout and the module split are in
[architecture](../../general/architecture/SKILL.md).

| | WSL, GNU target | Windows, MSVC target |
|---|---|---|
| Script | `scripts/cargo-gnu.sh` | `scripts/cargo-win.sh` |
| Toolchain | Linux Rust + MinGW, unpacked to `~/.local/opt/mingw-w64` on first use (no sudo) | Windows Rust + VS Build Tools |
| Release exe | `target/x86_64-pc-windows-gnu/release/kbswitch.exe` | `C:\dev\kbswitch-target\release\kbswitch.exe` |
| Tests | built, run only on Windows | run |

## Gates (WSL, every change)

```bash
cargo test                                               # pure modules on Linux
cargo fmt --check
cargo clippy --all-targets -- -D warnings
scripts/cargo-gnu.sh clippy --all-targets -- -D warnings # Windows code
scripts/cargo-gnu.sh build --release                     # links the real exe
powershell.exe -ExecutionPolicy Bypass -File "$(wslpath -w scripts/e2e-notepad.ps1)"  # after hook/capture/convert/clipboard/input changes
```

- `cargo test` on Linux compiles only the pure modules: `main.rs` gates every Win32 module with `#[cfg(windows)]`
  and marks the Linux-unused ones `allow(dead_code)`; `debug_log!` is `allow(unused_macros)` there.
- Host `clippy` covers the pure modules; the GNU `clippy` covers the Win32 ones. Both with `--all-targets` so the
  tests are linted too, `-D warnings` so a warning fails.
- The release profile (`Cargo.toml`): `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`; the exe is about
  600 KB. `windows_subsystem = "windows"` applies only without `debug_assertions`, so a debug exe keeps a console.
- The GNU exe is static (`+crt-static`) and needs only system DLLs; check with
  `~/.local/opt/mingw-w64/usr/bin/x86_64-w64-mingw32-objdump -p <exe> | grep "DLL Name"`.
- `scripts/cargo-gnu.sh test --no-run` builds the Windows test exe
  (`target/x86_64-pc-windows-gnu/debug/deps/kbswitch-*.exe`); run it on Windows for the `#[cfg(windows)]` tests:
  `input.rs` (replay by scan code with Shift wrapping, chord order, extended arrows in `select_left`/`right`, the
  marker on every event), `layout.rs` (`console_classes`), `tray.rs` (`tooltip_fits_the_shell_buffer`: 127 UTF-16
  units, no split surrogate).
- `.gitignore`: `/target` and `/var` (work files, logs such as `var/e2e.log`).

## `scripts/cargo-gnu.sh`

- `set -euo pipefail; cd "$(dirname "$0")/.."`: runs from the repo root whatever the caller's directory.
- `MINGW=${MINGW:-$HOME/.local/opt/mingw-w64}`: toolchain location, overridable. When
  `$MINGW/usr/bin/x86_64-w64-mingw32-gcc-win32` is missing: `apt-get download` of six Ubuntu packages
  (`binutils-mingw-w64-x86-64`, `gcc-mingw-w64-x86-64-win32`, its `-runtime`, `gcc-mingw-w64-base`,
  `mingw-w64-x86-64-dev`, `mingw-w64-common`) into a `mktemp -d` and `dpkg -x` of each into `$MINGW`. Download and
  extract need no root, so no sudo prompt; the `-win32` variant (not `-posix`) because the exe needs no pthreads.
- `PATH="$MINGW/usr/bin:$HOME/.cargo/bin:$PATH"`: the MinGW binutils (`ar`, `dlltool`) before anything else.
- `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc-win32`: the linker for that target only; the
  host build keeps its own.
- `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C target-feature=+crt-static"`: static CRT, so the exe does
  not need `libgcc_s` or `libwinpthread` DLLs next to it.
- `exec cargo "$1" --target x86_64-pc-windows-gnu "${@:2}"`: the first argument is the cargo subcommand, the target
  is inserted after it, the rest passed through (`build --release`, `clippy --all-targets -- -D warnings`,
  `test --no-run`).

## End-to-end check on Windows (from WSL)

`scripts/e2e-notepad.ps1` runs the real exe with real input against three targets: a WinForms `TextBox` (the
Win32 Edit control), a `RichTextBox` (RichEdit) and the real Windows 11 Notepad: 38 checks, about 2 min; hands off the
keyboard and mouse while it runs. Output is `PASS`/`FAIL` lines, exit 1 on any failure; the same lines plus the
app's own `debug_log!` output (when `-Exe` is a debug build) go to `var/e2e.log`. Run after every change to
`capture`, `convert`, `clipboard`, `input` or `hook`; the manual checklist covers the rest.

```bash
powershell.exe -ExecutionPolicy Bypass -File "$(wslpath -w scripts/e2e-notepad.ps1)"                      # release exe, all targets
powershell.exe -ExecutionPolicy Bypass -File "$(wslpath -w scripts/e2e-notepad.ps1)" -Only Notepad \
  -Exe "$(wslpath -w target/x86_64-pc-windows-gnu/debug/kbswitch.exe)"                                  # with the app log
```

- `param`: `$Exe` (default the GNU release exe), `$LogFile` (default `var/e2e.log`, written line by line with
  `Add-Content`, because output through the WSL pipe is lost when a run is killed), `$Only` = `All` |
  `Controls` | `Notepad`.
- `Native` (C# via `Add-Type`): `KEYBDINPUT` and `MOUSEINPUT` sequential; `INPUT` explicit with `Size = 40`, `ki` and `mi` both at offset 8
  (the union is 8-aligned after the 4-byte `type`; the real union is the 32-byte `MOUSEINPUT`, so the struct must
  be 40 bytes or `SendInput` rejects every call with no error). `Key(vk, scan, up)`: type 1 (keyboard); flags =
  `KEYEVENTF_SCANCODE` (8) when a scan code is given, `KEYEVENTF_KEYUP` (2) on release, `KEYEVENTF_EXTENDEDKEY`
  (1) for Home (0x24) and Left (0x25) so Num Lock can't turn them into numpad keys (End 0x23 is not extended: a
  Ctrl+Shift+End selection did not work, hence the programmatic `ClearAll`). `dwExtraInfo` stays 0: the input is
  unmarked, so the hook treats it as the user's typing. `Mouse(flags)`: one unmarked mouse event (2 = left down,
  4 = left up) at the current cursor; `SetCursorPos` moves it. `SetForegroundWindow`/`GetForegroundWindow` for
  focus.
- `Wait(ms)`: a loop of `Application.DoEvents()` + 10 ms sleeps, so the WinForms window keeps processing messages
  while the script waits. Never replace its inner `Start-Sleep` with `Wait` (an earlier edit did: infinite
  recursion, the script hung with no output).
- `Tap(vk, scan, holdMs)`: down, 30 ms, up. `Chord(mod, vk)`: modifier down, 30 ms, tap, modifier up, 100 ms.
- `$scans`: US scan codes of `g h b d t n v c` and Space (0x22 0x23 0x30 0x20 0x14 0x31 0x2F 0x2E 0x39);
  `TypeKeys` taps them with `vk = 0` and 60 ms gaps, then 300 ms, so the active layout decides the characters,
  as for a real user. `Other(text)`: the three test words in the other layout (`ghbdtn vbh ntcn` ↔
  `привет мир тест`), decided by whether `text` has Cyrillic.
- `DoubleShift`: Left Shift tap, 100 ms gap, tap, 1.5 s for the conversion; within `tap_max_ms` 250 and
  `double_ms` 300 (defaults, [trigger](../../domain/step-2-trigger/SKILL.md)).
- `ShiftPause`: Left Shift down, 50 ms, Pause (0x13) tap, 50 ms, Shift up, 1.5 s: the conversion waits for that
  Shift release ([convert](../../domain/step-3-convert/SKILL.md)).
- `SelectLine`: Shift held around Home: selects back to the line start, a selection the phrase logic does not
  record (Home resets the phrase). `SelectLeft(n)`: Shift held around n Left taps. `DoubleClickWord(box)`: two
  clicks at (12, 14) inside the control, selects the word. `DragSelect(box)`: left down at x = 2, cursor moved in
  two steps to x = 90, left up: a drag selection. Both go through kbswitch's mouse hook like the user's mouse.
- `ReadAll`: for a control `DoEvents` then `$box.Text.Trim()`; for Notepad `Set-Clipboard 'E2E-EMPTY'`, Ctrl+A,
  Ctrl+C, End, `Get-Clipboard` (the sentinel means an empty document, which copies nothing). The Notepad read
  resets kbswitch's phrase (Ctrl+A is a modifier chord), so the Notepad cases never read between typing and the
  double Shift; they retype the words for every conversion. `ClearAll`: `$box.Clear()` for a control, Ctrl+A and
  Delete for Notepad, then an Escape tap in both so kbswitch resets its phrase like after a real key (Ctrl+A
  does not select all in a multiline `TextBox`, and a `$box.Clear()` alone leaves the phrase: the next retype
  then typed the old words, exactly the "strange text in an empty Notepad" bug). `ReleaseModifiers`: key-ups for
  both Shifts, Ctrls and Alts (0xA0–0xA5). `Out(line)`: prints and appends to `$LogFile`. `Expect(name, got,
  ok[])`: `PASS`/`FAIL` through `Out`, sets `$script:failed`.
- Start: `ReleaseModifiers` (a killed run may have left a Shift down: earlier runs then typed capitals and
  switched layouts mid-phrase); stop any running kbswitch (single instance mutex); copy the exe to
  `%LOCALAPPDATA%\kbswitch\e2e\kbswitch.exe` and `Unblock-File` it (a copy from `\\wsl.localhost` carries a zone
  mark and `Start-Process` blocks on its security dialog); `Start-Process` it hidden with stderr redirected to
  `kbswitch.log` next to it (appended to `$LogFile` at the end; a release exe writes nothing), 1 s.
- Window: an Alt tap (0xA4) *before* the form is shown, while the console is in front, is the standard trick that
  lets this process take the foreground; sent to the shown form it starts the system-menu mode whose modal loop
  never returns to `DoEvents` (the script hung after "activated"). Then a topmost WinForms `Form` with a
  multiline `TextBox` docked top and a `RichTextBox` filling the rest, `Show`, `SetForegroundWindow`, `Activate`,
  500 ms, and a `WARN` line when the foreground window is still another one.
- Cases: every check with steps and expected text is listed in [test-cases](../test-cases/SKILL.md) (per
  control 1–7, Notepad N1–N6). Layout-agnostic because the test can't know the layout at start. Notepad: the
  running Notepad with a window (started when none; Windows 11 Notepad is one process with tabs), `TopMost` off on
  the form, Alt tap, `SetForegroundWindow`, Ctrl+N for a fresh tab, `ClearAll` and Ctrl+W at the end closes the
  empty tab without a prompt; `SKIP` when Notepad is not available.
- `finally`: modifiers released, the form closed, kbswitch stopped, the app log appended. `exit 1` when anything
  failed.
- Pitfalls: the file carries a UTF-8 BOM, or PowerShell 5.1 reads the Cyrillic literals as ANSI and the parse
  fails; a hang with kbswitch running means a dialog or a loop before the first log line, not the app;
  `Start-Sleep -Milliseconds 3` on Windows really sleeps about 15 ms (timer resolution), so a PowerShell probe
  that "works at 3 ms" says nothing about a Rust 3 ms sleep; Notepad 11 drops keys of a `SendInput` batch and
  auto-repeats the last one (`ghbdриве ттттттттт`), the reason for `retype_gap_ms`. A disposable probe that
  injects into Notepad without kbswitch (a copy of the `Native` class, `TypeKeys`, then the variants to compare)
  tells app behaviour from kbswitch behaviour in a minute; keep such probes in `var/`.

## Installing and the security prompt

An exe started from `\\wsl.localhost\…` (double-click in Explorer, a shortcut or a Run value pointing there)
shows "Open File - Security Warning" every time: the path is in the network zone, unsigned, and a new build is a
new file. A local copy does not. `scripts/install-win.sh`, line by line: needs the GNU release exe; target
`INSTALL_DIR` or `C:\Program Files\kbswitch`; kills a running kbswitch; tries the copy + `Unblock-File` in a
normal PowerShell and, when that fails (Program Files is not writable), runs the same command through
`Start-Process powershell -Verb RunAs -Wait` (one UAC prompt); then, unelevated: fails if the exe is missing,
rewrites the `kbswitch` Run value to the new path when one exists ("Start with Windows" follows the move; the app
itself writes `current_exe` there, `autostart::command`), writes the Start Menu shortcut `kbswitch.lnk`
(`WScript.Shell`), starts the exe. `settings.json` stays in `%APPDATA%\kbswitch` (no file next to the exe,
Program Files is read-only for the app). Checked 2026-10-17: a ShellExecute start of the installed copy is silent,
`Get-Item -Stream` shows only `:$DATA`, `NotSigned` is fine in the local zone. The e2e script uses its own copy in
`%LOCALAPPDATA%\kbswitch\e2e`. Alternative for the share itself: add `wsl.localhost` to the Local intranet zone.

## Native MSVC build

Setup, once on Windows:

- Visual Studio Build Tools, workload "Desktop development with C++" (MSVC linker + Windows SDK):
  `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`.
- Rust: `winget install Rustlang.Rustup`, then in a new terminal `rustup default stable-x86_64-pc-windows-msvc`.
- Check from WSL: `cargo.exe --version`. If `cmd.exe /c ver` fails with "Exec format error", WSL lost its interop
  entry (systemd clears it):
  `echo ':WSLInterop:M::MZ::/init:PF' | sudo tee /usr/lib/binfmt.d/WSLInterop.conf && sudo systemctl restart systemd-binfmt`.

`scripts/cargo-win.sh`: `cd` to the repo root; `CARGO_TARGET_DIR='C:\dev\kbswitch-target'` and
`WSLENV="CARGO_TARGET_DIR${WSLENV:+:$WSLENV}"` so the variable crosses into the Windows process (appended to an
existing `WSLENV`); `exec cargo.exe "$@"`. Sources are read from `\\wsl.localhost`, the build output stays on
NTFS, which keeps incremental builds fast. Never run plain `cargo.exe` in the repo: it would write `target\` into
WSL through the slow share. Gates there: `scripts/cargo-win.sh fmt --check`,
`clippy --all-targets -- -D warnings`, `test`.

## GitHub release

`.github/workflows/release.yml` publishes the exe for download at
`https://github.com/GeorgII-web/kbswitch/releases/latest`. Release: bump `version` in `Cargo.toml`, commit, then
`git tag vX.Y.Z && git push origin vX.Y.Z`.

- Trigger: a pushed tag `v*` only; a branch push builds nothing. `permissions: contents: write` lets the job's
  `GITHUB_TOKEN` create the release.
- Runner `ubuntu-24.04`, not Windows: the same `scripts/cargo-gnu.sh` as in WSL, so the released exe is the static
  GNU build that was tested here, not a different MSVC one.
- `rustup toolchain install stable --profile minimal --component clippy,rustfmt --target x86_64-pc-windows-gnu`:
  the GNU target is not preinstalled on the runner.
- `sudo apt-get update`: the runner image ships without package lists, and `cargo-gnu.sh` uses `apt-get download`
  for MinGW (it unpacks to `~/.local/opt/mingw-w64` as in WSL).
- Gates before the build: `cargo test`, `cargo fmt --check`, GNU `clippy --all-targets -- -D warnings`; a failure
  stops the job and no release is created. Host `clippy` is skipped there; it runs locally.
- `softprops/action-gh-release@v2`: creates the release named after the tag, attaches
  `target/x86_64-pc-windows-gnu/release/kbswitch.exe`, `generate_release_notes` lists the commits since the last tag.
- The exe is unsigned: a downloaded copy carries the internet zone mark, so SmartScreen asks once ("More info" →
  "Run anyway"); the README says so.

## Manual check

The checklist with every row and expected result is in [test-cases](../test-cases/SKILL.md); run it with the
release exe in Notepad, a browser text field and Windows Terminal before a change to the LED, the tray, settings
or autostart is reported done (nothing automated covers those).

## Debugging

Debug builds (no `--release`) open a console: `debug_log!` prints decisions and counts there, never typed text. No
reaction at all usually means Windows dropped the hook because a callback took too long: look for work inside it.
Running instances: `tasklist.exe | grep -i kbswitch`; stop with `taskkill.exe /F /IM kbswitch.exe`. A stuck Shift
after an aborted e2e run: run the script again (it releases the modifiers first) or tap both Shifts.
