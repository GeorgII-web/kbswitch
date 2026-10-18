---
name: step-3-convert
description: Step 3 of kbswitch — every function, constant and decision of the conversion in `src/convert.rs` (`run` on double Shift or Pause, `run_selection` on Shift+Pause, `paste_over`, `retype`, `selection`, wait constants), the layout switch in `src/layout.rs`, the EN↔RU tables in `src/charmap.rs`, limits, tests. Use when changing conversion, the Unicode retype and its pacing, the char map, layout switching or timings.
---

# Step 3: convert

Runs on the main thread after `WM_KBSWITCH_CONVERT` (`App::convert(selection_only)` → `convert::run` on double
Shift or Pause, `convert::run_selection` on Shift+Pause). Code: `src/convert.rs`, `src/layout.rs`, `src/charmap.rs`,
`src/lang.rs`. The clipboard and `SendInput` helpers it calls are in
[clipboard-input](../../artifact/clipboard-input/SKILL.md).

## Constants (`convert.rs`)

| Constant | Value | Why |
|---|---|---|
| `PROBE_WAIT_MS` | 150 | after Ctrl+C with nothing selected the clipboard never changes; a real copy lands well inside 150 ms, so this is the "is anything selected" probe |
| `COPY_WAIT_MS` | 500 | the copy of the selected phrase; longer than the probe because a long selection in a slow app (browser field) takes more time |
| `PASTE_WAIT_MS` | 300 | the app must read our text from the clipboard after Ctrl+V before `restore` replaces it; 300 ms is enough for Electron apps |
| `SHIFT_RELEASE_WAIT_MS` | 1000 | Shift+Pause fires on the Pause key-down, so the physical Shift is usually still down; Ctrl+C sent then is Ctrl+Shift+C (DevTools in browsers). Wait up to 1 s for the release, then give up |

`wait(done, ms)`: polls `done()` every 10 ms for `ms / 10` rounds, then once more; returns the last result. Used
for the clipboard sequence number and the Shift release.

## `run(settings)`: double Shift

1. `CONVERTING = true` (the hook posts no second request while it runs; the trigger state machine keeps running).
2. `hotkey = settings.switch_hotkey`, `hwnd = GetForegroundWindow()`, `(keys, typed_in) = capture().phrase()`:
   the recorded keys and the layout they are in on screen (see [step-1-capture](../step-1-capture/SKILL.md)).
   The guard is released at once; never hold it across `SendInput`.
3. **Phrase not empty** → `target = lang::other(typed_in)`.
   - `settings.select_and_paste && !layout::is_console(hwnd)` → `paste_over`, else `Pasted::Untouched`.
   - `Pasted::Done` → nothing more. `Untouched` → `retype`. `Mismatch` → `capture().forget()` (the phrase doesn't
     match the screen) and only `layout::switch(hwnd, target, hotkey)`.
   - Why the phrase layout, not the window's: the user may switch by hand (Alt+Shift, Win+Space) after noticing the
     wrong text and before the double Shift; the phrase is still in the layout it was typed in.
4. **Phrase empty** → `layout::is_console(hwnd) || !selection(hwnd, hotkey)` → only
   `layout::switch(hwnd, lang::other(layout::lang_of(hwnd)), hotkey)`. Consoles never get Ctrl+C (it interrupts
   the running command), so there only the switch happens.
5. `CONVERTING = false`. The phrase is kept (`converted` only changes its layout), so the next double Shift
   converts back.

`enum Pasted { Done, Untouched, Mismatch }`: `Untouched` = nothing changed and the caret is where it was, so a
retype is safe; `Mismatch` = the selection was not the phrase, the text is left alone.

## `run_selection(settings)`: Shift+Pause

Same frame (`CONVERTING`, `hwnd`, `hotkey`), then:

1. `capture().forget()`: the text on screen is about to change, a later double Shift must not retype a stale
   phrase.
2. `wait(|| GetAsyncKeyState(VK_SHIFT) >= 0, SHIFT_RELEASE_WAIT_MS)`: the high bit set = still held. Not released
   in time → `debug_log!` and nothing happens.
3. Else like step 4 of `run`: a console or nothing selected → only the layout switch.

Why forget before converting: the Shift+Home that selected the text is a `Reset`-class key in the buffer anyway;
forgetting is for a mouse-free Shift+arrow selection whose arrows were already a reset, and for safety.

## `retype(hwnd, keys, typed_in, target, settings)`: the default

1. `text = charmap::convert_to(&charmap::typed_text(keys, typed_in), target)`: the phrase as it is on screen
   (`typed_text`, the US character of each key, mapped to RU when typed in RU), converted to `target`.
2. `inputs = input::taps(VK_BACK, keys.len())` + `input::unicode(&text)`: n Backspaces, then the converted text
   as Unicode key events (`KEYEVENTF_UNICODE`, one down/up per UTF-16 unit). Sent with
   `input::send_paced(&inputs, settings.retype_gap_ms)`: one key per `SendInput`, `retype_gap_ms` (default 20)
   apart; a short send (UIPI, elevated window) is logged.
3. `layout::switch(hwnd, target, settings.switch_hotkey)`; a false result is only logged.
4. `capture().converted(target)`: the text on screen is in `target` now, whatever the window's layout says (a
   failed switch must not make the next double Shift convert the wrong way).

Why Unicode, not scan codes: the earlier replay by scan code (`KEYEVENTF_SCANCODE`) let the active layout decide
the characters, so it depended on the app having applied the switch from step 3 before it processed the keys.
Windows 11 Notepad applies it asynchronously: the phrase came out unchanged or half converted ("ghbdtпривет").
Unicode events arrive as `VK_PACKET` and give the same characters in any layout.

Why paced: Notepad 11 drops keys from one big `SendInput` batch and repeats the last one for as long as it misses
its key-up (`ghbdриве ттттттттт`); 3 ms apart it still drops some, 15 ms apart one in a few runs, 20 ms apart none
in 15 probes and 3 full e2e runs. Classic Edit and RichEdit controls take a batch in full; `retype_gap_ms: 0`
sends one batch for them.

Why default over select and paste: no app-independent way exists to select exactly n characters in one step
(Ctrl+Shift+Left is by word, Shift+Home by line, `EM_SETSEL` only in Edit controls), and a Shift+Left selection
growing by one character is drawn slower than n Backspaces in most apps. Retype never touches the clipboard.

## `paste_over(hwnd, count, target, hotkey)`: `select_and_paste: true`

1. `saved = clipboard::snapshot()`, `before = clipboard::sequence()`.
2. `input::select_left(count)` (Shift+Left × count, extended arrows), then `chord(VK_CONTROL, VK_C)`.
3. `wait(sequence != before, COPY_WAIT_MS)` false → the app didn't copy (no selection support, console-like
   field): `select_right(count)` undoes the selection, also where Shift+Left only moved the caret, return
   `Untouched` → retype. The clipboard was not changed, nothing to restore.
4. `text = clipboard::text()`; `text.chars().count() != count` → the selection is not the phrase (a word wrap,
   auto-completion, a field that ignored some keys): `input::right()` collapses the selection to its end,
   `restore(saved)`, return `Mismatch`.
5. `clipboard::set_text(charmap::convert_to(&text, target))` false → `right()`, `restore`, `Untouched`.
6. `chord(VK_CONTROL, VK_V)`, `layout::switch`, sleep `PASTE_WAIT_MS`, `restore(saved)`,
   `capture().converted(lang_of(hwnd))`, `Done`.

Why copy before pasting: the copied length is the only check that the selection is the phrase. Why one edit: a
paste is one undo step and one redraw, however long the phrase.

## `selection(hwnd, hotkey) -> bool`

1. `saved = snapshot()`, `before = sequence()`, `chord(VK_CONTROL, VK_C)`.
2. `wait(sequence != before, PROBE_WAIT_MS)` false → nothing selected, return false (clipboard untouched).
3. `clipboard::text()` empty or not text → `restore(saved)`, false.
4. `(converted, target) = charmap::convert(&text)`: direction from the text itself.
5. `set_text(&converted)` ok → `chord(VK_CONTROL, VK_V)`, `layout::switch(hwnd, target, hotkey)`, sleep
   `PASTE_WAIT_MS`. Always `restore(saved)`; return true.

After it the pasted text is not selected, so a second double Shift only switches the layout.

## `layout.rs`

- `lang_of(hwnd)`: `GetKeyboardLayout(GetWindowThreadProcessId(hwnd))` → `lang::from_hkl`. A null window gives
  thread 0 = our own thread's layout.
- `foreground_lang()`: `lang_of(GetForegroundWindow())`; the timer uses it for the tray icon and LED.
- `switch(hwnd, target, hotkey) -> bool`:
  1. already `target` → true.
  2. `installed(target)`: `GetKeyboardLayoutList` twice (count, then fill), first HKL whose low word is `target`;
     none → log, false (nothing is sent: a hotkey would switch to some other installed layout).
  3. `PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0, hkl)`, then `wait_for`: polls `lang_of` 10 × 10 ms.
  4. Ignored (consoles, some UWP and Electron apps handle the message themselves or not at all) → log, send
     `chord(hotkey)` and poll again; the result is whatever the window shows then.
- `chord(hotkey)`: `AltShift` → `input::chord(VK_LMENU, VK_LSHIFT)`, `CtrlShift` → `(VK_LCONTROL, VK_LSHIFT)`,
  `WinSpace` → `(VK_LWIN, VK_SPACE)`. Left-hand keys, because the Windows hotkey settings bind the left ones.
- `toggle(hotkey)`: foreground window to `lang::other(lang_of(hwnd))`; Caps Lock in `switch_layout` mode.
- `is_console(hwnd)`: `GetClassNameW` into 64 UTF-16 units → `is_console_class`: `ConsoleWindowClass` (conhost)
  or `CASCADIA_HOSTING_WINDOW_CLASS` (Windows Terminal). Other terminals (ConEmu, mintty) are not detected and
  get Ctrl+C.

## `charmap.rs` and `lang.rs`

- `EN_KEYS` / `RU_KEYS`: same physical key at the same index; lower row, upper row, then the shifted digits that
  differ (`@#$^&|` ↔ `"№;:?/`). Both tables are one-to-one and equal in length (tested).
- `convert_to(text, target)`: `target == RU` maps `EN_KEYS → RU_KEYS`, anything else `RU_KEYS → EN_KEYS`; a
  character not in the source table stays (digits, whitespace, characters already in the target layout, `€`).
- `convert(text) -> (String, lang)`: counts Cyrillic (`а..я`, `А..Я`, `ё`, `Ё`) vs ASCII letters; more Cyrillic →
  to EN, else (ties and pure punctuation included) to RU. Used only for the selection, where no typing layout is
  known.
- `typed_text(keys, lang)`: what the recorded keys show on screen: `us_char(vk, shift)` for Space, digits (with
  the shifted `)!@#$%^&*(` row), letters, the OEM keys `;=,-./`[\]'` and the 102nd key `0xE2`; RU → `convert_to`
  of that. Keys outside the table (Enter) are dropped. Used by step 4 for the spell check.
- `lang.rs`: `EN = 0x0409`, `RU = 0x0419`, `from_hkl` = low word of the HKL (a variant keeps the language there),
  `other`: RU → EN, anything else → RU (a German layout converts "to RU").

## Limits

- Consoles never get Ctrl+C; only the layout switch, and typed phrases are retyped.
- A `CF_BITMAP`-only or private clipboard format is lost after a selection conversion; text, files and most images
  come back (see [clipboard-input](../../artifact/clipboard-input/SKILL.md)).
- The app's own copy of the selection lands in Win+V history when that is on (the app wrote it, not us).
- Editors that copy the whole line on Ctrl+C without a selection (VS Code, JetBrains) get the line converted and
  pasted at the cursor when there is no phrase.
- Elevated windows: `SendInput` is blocked by UIPI unless kbswitch runs as admin; logged as a short return.
- Apps with their own input stack (remote desktops, games) may drop scan-code input; no fix planned.
- A word wrap or autocomplete between typing and the double Shift breaks the `count` check in `paste_over`
  (→ `Mismatch`); retype has no check and deletes `count` characters whatever they are.

## Tests

All cases: [test-cases](../../quality-gate/test-cases/SKILL.md), section charmap/lang/layout for the tables and
the end-to-end section for the conversion itself (`convert.rs` has no unit tests, Win32 only). A change to the
tables breaks `tables_are_one_to_one` and `every_key_round_trips` first; a change to retype shows in the e2e
checks 3, 4 and N3 (Notepad drops keys when the pacing is wrong).
