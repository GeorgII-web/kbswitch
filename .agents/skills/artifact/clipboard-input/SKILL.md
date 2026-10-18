---
name: clipboard-input
description: The Win32 helpers of kbswitch — `src/clipboard.rs` (all-formats `Snapshot`, excluded format ranges, `restore` with retries and the never-empty rule, `set_text`, the Win+V exclusion flags) and `src/input.rs` (`KBSWITCH_MARKER`, `chord`, `taps`, extended arrows, `unicode` text events, `send`, `send_paced`). Use when changing what is saved or restored on the clipboard or any injected key sequence.
---

# Clipboard and input helpers

Everything step 3 ([step-3-convert](../../domain/step-3-convert/SKILL.md)) sends or copies goes through these two
modules. Entrypoints: `clipboard::snapshot` / `restore` / `set_text` / `text` / `sequence`, `input::send` with the
builders below. Both are `#[cfg(windows)]`; `input.rs` has tests that run on Windows.

## `clipboard.rs`

- `CF_UNICODETEXT = 13`: the only format we read and write ourselves.
- `sequence()`: `GetClipboardSequenceNumber`; a change means some app wrote the clipboard. Step 3 compares it
  before and after Ctrl+C to know whether anything was selected.
- `with_open(f) -> Option<T>`: `OpenClipboard(None)` up to 10 × 10 ms (another app may hold it for a moment,
  especially right after its own paste), runs `f`, `CloseClipboard`. `None` = never opened; every caller treats
  that as "unknown", not as "empty".
- `text() -> Option<String>`: `GetClipboardData(CF_UNICODETEXT)`, `GlobalLock`, reads `GlobalSize / 2` UTF-16
  units up to the first NUL (the size is a block size, not the string length), `from_utf16_lossy`. `None` when
  there is no text or the clipboard couldn't be opened.

### Snapshot and restore

- `Snapshot(Vec<(u32, Vec<u8>)>)`: every format with its raw bytes, in enumeration order.
- `is_global(format)`: false for the formats whose handle is no `HGLOBAL` or that only the owner can render:
  `CF_BITMAP` (2, an `HBITMAP`), `CF_METAFILEPICT` (3), `CF_PALETTE` (9), `CF_ENHMETAFILE` (14),
  `0x0080..=0x008F` (`CF_OWNERDISPLAY`, `CF_DSPTEXT/BITMAP/METAFILEPICT/ENHMETAFILE`, owner-drawn),
  `0x0200..=0x02FF` (`CF_PRIVATEFIRST..LAST`, meaning known only to the owner) and `0x0300..=0x03FF`
  (`CF_GDIOBJFIRST..LAST`, GDI handles). Everything else, including every registered format (≥ `0xC000`: HTML,
  RTF, `FileNameW`, `Preferred DropEffect`, `Shell IDList Array`) and `CF_DIB`/`CF_DIBV5`/`CF_HDROP`, is global
  memory by contract and is copied.
- `snapshot() -> Option<Snapshot>`: inside `with_open`, `EnumClipboardFormats` from 0 until 0; for each global
  format `GetClipboardData` (this also renders delayed formats), `GlobalLock`, copy `GlobalSize` bytes,
  `GlobalUnlock`. A format whose data can't be fetched or locked is skipped, not fatal. Synthesized formats
  (`CF_TEXT`, `CF_OEMTEXT`, `CF_LOCALE` from `CF_UNICODETEXT`) are enumerated and copied too; putting them back
  explicitly is harmless.
- `put(format, bytes)` (unsafe, clipboard open and emptied): `GlobalAlloc(GMEM_MOVEABLE, len.max(1))` (a zero
  size allocation fails), copy, `SetClipboardData`; on failure the global is freed, on success the clipboard owns
  it. Returns false on any failure.
- `restore(saved: Option<Snapshot>)`: `None` → `debug_log!` and return: the clipboard couldn't be read, so our
  converted text stays rather than the clipboard being emptied (the earlier text-only version emptied it here,
  which was the "clipboard content disappears" bug). Else up to 10 × 100 ms: `with_open` → `EmptyClipboard` and
  `put` every format; all ok → done. The 1 s is longer than any read because the pasting app may still hold the
  clipboard right after Ctrl+V. Failure is logged with the format count.

### Writing text

- `set_text(text) -> bool`: UTF-16 with a NUL, bytes little-endian, `with_open` → `EmptyClipboard`,
  `put(CF_UNICODETEXT)`, `keep_out_of_history()`.
- `keep_out_of_history()` (unsafe, clipboard open): registers and sets three formats, each a 4-byte zero DWORD:
  `ExcludeClipboardContentFromMonitorProcessing` (clipboard managers skip it),
  `CanIncludeInClipboardHistory` = 0 (not in Win+V), `CanUploadToCloudClipboard` = 0 (not synced). A failed
  `SetClipboardData` frees that one global and goes on. So our converted text never appears in history; the
  user's original is restored with its own flags.

## `input.rs`

- `KBSWITCH_MARKER = 0x4B42_5357` ("KBSW"): `dwExtraInfo` of every event we inject; the keyboard hook skips such
  events, so the replay, Ctrl+C/V, LED toggles and layout hotkeys are never recorded and never trigger.
- `event(vk, scan, flags)`: one `INPUT_KEYBOARD` with the marker, `time = 0`. All builders go through it; an
  `INPUT` built elsewhere would lack the marker (test `every_event_carries_the_marker`).
- `down(vk)` / `up(vk)`: plain virtual-key press and release.
- `chord(modifier, key)`: `[down(mod), down(key), up(key), up(mod)]`, e.g. Ctrl+C, Alt+Shift.
- `taps(vk, count)`: `count` × down/up, used for the Backspaces.
- `arrow(vk)`: down/up with `KEYEVENTF_EXTENDEDKEY`. Without the flag an arrow is the numpad key: with Num Lock on,
  Shift+Left becomes a `4`, and the selection types digits.
- `shifted_arrows(vk, count)`: `down(VK_LSHIFT)`, `count` × `arrow(vk)`, `up(VK_LSHIFT)`: one `SendInput`, so no
  real key can slip into the middle.
- `select_left(count)`: Shift+Left × count, selects the characters before the caret. `select_right(count)`: the
  reverse, undoes a failed selection, also where Shift+Left only moved the caret (fields without selection).
- `right()`: one extended Right, collapses a selection to its end.
- `unicode(text)`: per UTF-16 unit of `text`: `wVk = 0`, `wScan = unit`, `KEYEVENTF_UNICODE` down and up. The app
  receives `VK_PACKET` and the exact character, in any layout; replaced the scan-code `replay` (its characters
  depended on when the app applied the layout switch, see [convert](../../domain/step-3-convert/SKILL.md)).
- `send(inputs) -> bool`: one `SendInput` for the whole slice; false when fewer events were sent than given,
  which is what UIPI does for an elevated foreground window (nothing arrives, nothing is lost either).
- `send_paced(inputs, gap_ms) -> bool`: `gap_ms == 0` → `send`; else `chunks(2)` (one key's down and up) per
  `SendInput` with `thread::sleep(gap_ms)` after each, `all` sent. Notepad 11 drops keys of a batch and of keys
  3 ms apart and auto-repeats the last one when its key-up got lost; 20 ms apart it takes everything. Only the
  retype uses it; select, copy and paste chords stay single calls.

Rules: build every `INPUT` here; send each phase (select, copy, paste) in one call and the retype through
`send_paced`; no sleep inside a builder.

## Tests

All cases: [test-cases](../../quality-gate/test-cases/SKILL.md), section input.rs (Windows only). Every new builder
must be added to `every_event_carries_the_marker`. `clipboard.rs` has no unit tests; the e2e check 5 (`KEEP ME`
survives a selection conversion) and the manual image row cover it.
