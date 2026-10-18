---
name: step-1-capture
description: Step 1 of kbswitch — the low-level hooks in `src/hook.rs` (hook thread, `KeyEvent`, marker, `Effect` handling, thread messages) and their pure rules in `src/capture.rs` (`Capture`, the Caps Lock press decision, Shift+Pause, the word check) and `src/buffer.rs` (`Buffer`, `classify`, resets, `last_word`), every field, method, constant and test. Use when changing the hooks, the phrase, the reset rules, Backspace, Caps Lock or modifier handling, or debugging a wrong or missing phrase.
---

# Step 1: capture

Records the phrase typed since the last reset, as physical keys plus the layout it was typed in, so step 3 can
replay it in the other layout. Entrypoint `hook::start` (`src/hook.rs`) reads the Windows state into a `KeyEvent`;
`src/capture.rs` and `src/buffer.rs` hold the rules, pure and unit-tested on Linux.

## hook.rs

| Item | What |
|---|---|
| `WM_KBSWITCH_CONVERT` = `WM_APP + 1` | to the main thread; `wParam` `0` double Shift, `SELECTION_ONLY` (`1`) Shift+Pause |
| `WM_KBSWITCH_SWITCH` = `WM_APP + 2` | Caps Lock in `switch_layout` mode → `App::switch_layout` |
| `WM_KBSWITCH_CHECK` = `WM_APP + 3` | a Space ended a word → `App::check_word` |
| `ENABLED: AtomicBool` | the tray/`enabled` setting; only gates `request_convert`, the state machine keeps running |
| `CONVERTING: AtomicBool` | set by step 3 while it injects input; a second trigger meanwhile is dropped |
| `MAIN_THREAD: AtomicU32` | thread id for `PostThreadMessageW`, stored by `start` |
| `CAPTURE: Mutex<Capture>` | the one shared state; `capture()` locks it, a poisoned lock is reused (`PoisonError::into_inner`) |

- `start(main_thread)`: spawns the hook thread, which runs `install` and reports the `Result` over an `mpsc`
  channel, then runs its own `GetMessageW`/`DispatchMessageW` loop (low-level hooks are delivered through the
  installing thread's loop). `start` returns only after the report, so `main` can `fail` on a hook error.
- `install`: `SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_proc)` then `WH_MOUSE_LL, mouse_proc`, module handle
  from `GetModuleHandleW(None)`; one error string for both.
- `keyboard_proc`: only `HC_ACTION`; the `KBDLLHOOKSTRUCT` with `dwExtraInfo == KBSWITCH_MARKER` is our own
  injection (step 3 replay, Ctrl+C/V, LED toggles) and is never recorded. Otherwise `read_event` → `capture().key`
  → the `Effect`; every path ends in `CallNextHookEx` except the swallows, which `return LRESULT(1)` so no app
  sees the key and Caps Lock doesn't toggle.
- `read_event(kb, down)`: `down` = `WM_KEYDOWN | WM_SYSKEYDOWN` (`WM_SYSKEYDOWN` is any key while Alt is held);
  `vk`/`scan`/`time` from the struct; `shift`, `ctrl_alt` (`VK_CONTROL || VK_MENU`), `win` (`VK_LWIN || VK_RWIN`)
  from `GetAsyncKeyState < 0` (held now, generic codes cover both sides); `window` = `GetForegroundWindow`,
  `lang` = `layout::lang_of(window)`. All read inside the callback, which must stay under 1 ms.
- `mouse_proc`: `WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN` → `capture().click()`; moves
  and wheel are ignored (the caret doesn't move).
- `request_convert(what)`: posts `WM_KBSWITCH_CONVERT` with `wParam = what` only when `ENABLED && !CONVERTING`.
  `post(msg)` = `post_with(msg, 0)`; `post_with` wraps `PostThreadMessageW`, result ignored (a lost message is a
  missed conversion, never a crash).

Effects carried out by the hook: `Pass` → next hook; `Convert` → `request_convert(0)`; `ConvertSelection` →
`request_convert(SELECTION_ONLY)` and `LRESULT(1)`; `Check` → `post(WM_KBSWITCH_CHECK)`; `Swallow` → `LRESULT(1)`;
`SwitchLayout` → `post(WM_KBSWITCH_SWITCH)` and `LRESULT(1)`.

## KeyEvent and Effect (capture.rs)

`KeyEvent { vk, scan, down, time, shift, ctrl_alt, win, window: usize, lang }`, `Copy + Default`; `time` is the
hook's wrapping ms timestamp, `shift` means Shift was held before this event (so for a Shift key-down it is the
other Shift). `Effect`: `Pass`, `Convert` (passed on, completed a double Shift), `ConvertSelection` (swallowed,
Shift+Pause), `Check` (passed on, a Space ended a word; the word waits in `Capture::take_word`), `Swallow`,
`SwitchLayout` (swallowed and the layout switches). Constants: `VK_PAUSE` `0x13`, `VK_CAPITAL` `0x14`, `VK_SPACE`
`0x20`; `is_shift` = `0x10 | 0xA0 | 0xA1`.

## Capture

Fields: `buffer: Buffer`, `trigger: DoubleShift` (`../step-2-trigger/SKILL.md`), `window` (last key-down's
foreground window), `caps_lock: CapsLock` (setting), `caps_press: Option<Effect>` (decision of the Caps Lock press
being held), `detect: bool`, `word: Option<(Vec<Key>, u16)>`. `new` is `const` so the static `Mutex` needs no lazy
init.

`key(e)` in this order, each step returning early:

1. `vk == VK_CAPITAL` → `caps_lock_key(down, shift)`; a non-`Pass` result returns at once (nothing else sees a
   swallowed Caps Lock), `Pass` continues to the general rules, where Caps Lock resets the phrase.
2. Key-down in another foreground window than the last → `window` updated, `buffer.reset()`. Only on key-down:
   a key-up after Alt+Tab must not clear the phrase typed before it.
3. Shift (`is_shift`): down → `trigger.shift_down(time)`; with `ctrl_alt || win` held → `trigger.other()`
   (Ctrl+Shift, Alt+Shift, Win+Shift are hotkeys, not taps); up → `trigger.shift_up(time)`, fired → `Convert`.
   Shift never reaches the buffer (it is `Neutral` anyway).
4. Num Lock (`0x90`) and Scroll Lock (`0x91`), down or up: `Pass` before anything else. Every LED change kbswitch
   injects (layout LED, warning blink) comes back through the hook as an *unmarked* key-down (the keyboard driver
   echoes toggle keys), and before this rule each one called `trigger.other()` and cancelled a pending Shift tap:
   a double Shift during the blink or right after a layout change did nothing.
5. Any other key-down: `trigger.other()` first (a key between or inside the taps cancels). Then, before the
   buffer, `vk == VK_PAUSE && shift && !ctrl_alt && !win` → `ConvertSelection`: the Pause must not reset the
   phrase (step 3 forgets it itself) and must not reach the app. Then the Win+Space exception: `vk == VK_SPACE &&
   win && !ctrl_alt` is the system layout hotkey and is not fed to the buffer (a Space with Win held would
   otherwise reset as a modifier chord), so a phrase survives a layout switch through it. Every other key-down
   goes to `buffer.key_down(Key { vk, scan, shift }, lang, ctrl_alt || win)`. With `detect` on and `vk ==
   VK_SPACE`, `buffer.last_word()` of at least `detect::MIN_KEYS` keys is stored in `word` with `buffer.lang()` and
   `Check` is returned; the Space itself was already appended, so the phrase goes on.
5. Key-ups and everything else → `Pass`.

`caps_lock_key(down, shift)`: `decided` from the setting (`Normal` → `Pass`, `Off` → `Swallow`, `SwitchLayout`
→ `Pass` when `shift` else `SwitchLayout`). `follow(press)` maps a press decision to what its repeats and release
get: `Pass` → `Pass`, anything else → `Swallow`. Key-up → `follow(caps_press.take().unwrap_or(decided))`; a
key-down while `caps_press` is set is auto-repeat → `follow(press)`; the first key-down stores `decided` and, when
not `Pass`, calls `trigger.other()` (cancels a pending tap; the phrase stays, no text changed). Hence one switch
per press however long it is held, the release always matches its press even when Shift or the setting changed
meanwhile, and Shift+Caps Lock stays a real Caps Lock in `switch_layout`.

Other methods: `set_detect(on)` sets the flag and drops a pending `word`; `take_word()` hands the word to
`App::check_word` once; `set_caps_lock(mode)`; `set_timing(tap_max_ms, double_ms)` → `trigger.set_timing`;
`click()` resets the buffer and calls `trigger.other()` (the caret may have moved, a click between taps cancels);
`phrase()` clones `(keys, lang)` so the lock is released before step 3 injects; `converted(lang)` →
`buffer.set_lang` (the text on screen is now in `lang`, the next double Shift converts back); `forget()` resets the
buffer (the phrase no longer matches the screen). `App::apply` calls `set_timing`, `set_caps_lock` and
`set_detect(detect_wrong_layout && spell checkers present)` under one lock on every settings load.

## Buffer

`Key { vk, scan, shift }` (`Copy, PartialEq`): keys, not characters, because the same key gives `q` in EN and `й`
in RU and the replay must press the same physical key by scan code. Caps Lock is not stored: a real Caps Lock
resets, so the replay runs with the Caps Lock state of the typing. `MAX_KEYS` 512, `SPACE` `0x20`.

`classify(vk)`:

| Class | vk | Why |
|---|---|---|
| `Back` | `0x08` | pops |
| `Text` | `0x20` Space, `0x30..=0x39` digits, `0x41..=0x5A` letters, `0xBA..=0xC0` (`VK_OEM_1 ; VK_OEM_PLUS = VK_OEM_COMMA , VK_OEM_MINUS - VK_OEM_PERIOD . VK_OEM_2 / VK_OEM_3 \``), `0xDB..=0xDE` (`VK_OEM_4 [ VK_OEM_5 \ VK_OEM_6 ] VK_OEM_7 '`), `0xE2` (`VK_OEM_102`, the 102nd key) | every key with a character in both layouts |
| `Neutral` | `0x10..=0x12` Shift/Ctrl/Alt generic, `0x5B`/`0x5C` Win, `0x90` Num Lock, `0x91` Scroll Lock, `0xA0..=0xA5` left/right Shift/Ctrl/Alt | change no text; Scroll Lock because the LED toggles are injected and the user may press it too |
| `Reset` | everything else: Enter, Tab, Esc, arrows, Home/End/PgUp/PgDn, Insert/Delete, F-keys, numpad, Caps Lock, Pause … | the caret moved or the text changed in a way the phrase can't follow |

`key_down(key, lang, modifier)`: `Neutral` → nothing. `Text` without modifier → when `lang != self.lang` or the
buffer holds `MAX_KEYS`, clear first (a key in another layout starts a new phrase; the cap bounds memory and
replay time), then `self.lang = lang` and push. `Back` without modifier → pop (empty stays empty; Backspace
before the phrase doesn't matter). Anything else, including `Text` or `Back` with Ctrl/Alt/Win held (Ctrl+A,
Ctrl+Backspace, Alt+letter) → clear. `reset()`, `keys()`, `lang()`, `set_lang(lang)` are plain accessors.
`last_word()`: only when the last key is a Space, the keys after the previous Space (or from the start); a second
Space in a row gives an empty slice, so `Capture` asks for no check.

## Tests

All cases with what each proves: [test-cases](../../quality-gate/test-cases/SKILL.md), sections buffer.rs and
capture.rs. The ones a change here breaks first: `navigation_enter_and_caps_lock_reset` and
`chord_with_modifier_resets` (what resets the phrase), `scroll_lock_between_shift_taps_does_not_cancel` (LED
echoes must not cancel a tap), `shift_pause_converts_the_selection_and_is_swallowed`,
`window_change_starts_a_new_phrase`, `click_resets_the_phrase_and_a_pending_tap`.

## Rules

- A new rule goes into `capture.rs` or `buffer.rs` with a test; `hook.rs` only reads state and carries out the
  `Effect`, never calls `SendInput`, the clipboard or `sleep` (`../../general/architecture/SKILL.md`).
- Input injected by other apps without our marker counts as typing.
- Every key goes on with `CallNextHookEx` except the three swallows above.
