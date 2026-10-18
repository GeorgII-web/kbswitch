---
name: step-2-trigger
description: Step 2 of kbswitch — the double Shift state machine in `src/trigger.rs` field by field (tap_max_ms, double_ms, down_at, interrupted, last_tap_up) and method by method, how `src/capture.rs` feeds it (which vks are Shift, Ctrl/Alt/Win as hotkeys, other keys cancel), the Pause (Break) key trigger and the Shift+Pause selection trigger, the handover through `src/hook.rs` to the main thread, every test and what it proves. Use when changing double Shift, Pause or Shift+Pause detection or timing, debugging false or missed triggers, or adding another trigger.
---

# Step 2: trigger

Decides that the user asked for a conversion: a double tap on Shift (`src/trigger.rs`, fed by `Capture::key` in
`src/capture.rs`), Pause (Break) alone, or Shift+Pause (both `Capture::key` alone). The keyboard hook then posts the request to the main thread
(`src/hook.rs`), which runs step 3 ([step-3-convert](../step-3-convert/SKILL.md)). Step 1 records the phrase
([step-1-capture](../step-1-capture/SKILL.md)); the timings come from `settings.json`
([tray-autostart](../../artifact/tray-autostart/SKILL.md)).

## Definitions

- **Tap**: Shift down, then Shift up, with no other key-down in between, held at most `tap_max_ms`. Left and right
  Shift are the same key; a tap may start on one and end on the other.
- **Double tap**: two taps where the second **down** comes at most `double_ms` after the first **up**. It fires on
  the second **up**, so the user's Shift is already released when step 3 injects input (a held Shift would turn the
  injected Ctrl+C into Ctrl+Shift+C and capitalise the retyped keys).
- Times are `KBDLLHOOKSTRUCT.time`, u32 ms since boot, wrapping after 49.7 days; every difference uses
  `wrapping_sub`, so a wrap between two taps is harmless (`timestamps_wrap` test).

## `DoubleShift` (`src/trigger.rs`)

| Field | Meaning |
|---|---|
| `tap_max_ms: u32` | longest press that is a tap; default `TAP_MAX_MS` = 250 |
| `double_ms: u32` | longest gap between first up and second down; default `DOUBLE_MS` = 300 |
| `down_at: Option<u32>` | time of the Shift down being held, `None` when Shift is up |
| `interrupted: bool` | another key went down during this press, so its up is no tap |
| `last_tap_up: Option<u32>` | time the first tap ended; `Some` means "waiting for the second tap" |

`new()` is `const`, because `hook::CAPTURE` is a `static Mutex<Capture>` built at compile time; both defaults are
replaced by `set_timing(tap_max_ms, double_ms)` on every settings load and reload (`App::apply` →
`Capture::set_timing`). `settings.rs` clamps `tap_max_ms` to 50–1000 and `double_ms` to 100–1000 before they get
here, so a typo in the file can't make the trigger impossible or fire on every Shift.

`shift_down(t)`:

1. `down_at.is_some()` → return. Windows auto-repeats a held Shift as more downs (`auto_repeat_counts_once`), and
   the other Shift pressed while one is held must not restart the press.
2. If `last_tap_up` is older than `double_ms` → drop it. The first tap expired; this down starts a new first tap
   instead of completing a stale one (`slow_second_tap_does_not_fire`).
3. `down_at = Some(t)`, `interrupted = false`: a fresh press.

`shift_up(t) -> bool` (true = fire):

1. No `down_at` → false. An up without a down: the press started before the hook was installed, or belonged to a
   hotkey the capture already cleared.
2. `interrupted`, or held longer than `tap_max_ms` → `last_tap_up = None`, false. A long hold or Shift+letter is no
   tap, and it also cancels a first tap waiting for its second (`long_hold_is_not_a_tap`,
   `shift_with_letter_is_not_a_tap`).
3. `last_tap_up.take()` was `Some` → true: this up completed the second tap. `take` resets the machine, so a third
   tap starts over and three taps fire once (`three_taps_fire_once`).
4. Otherwise `last_tap_up = Some(t)`, false: first tap done, waiting.

`other()`: `interrupted = true`, `last_tap_up = None`. Called for any other key-down and any mouse button: the
current press is no longer a tap and a waiting first tap is dropped (`key_between_taps_cancels`).

The gap is measured in `shift_down` (second down vs first up), the hold in `shift_up`; both compare `> limit`, so a
gap or hold exactly at the limit still counts.

## How `Capture::key` feeds it (`src/capture.rs`)

Shift is `vk` 0x10 (generic `VK_SHIFT`, never sent by the low-level hook but accepted), 0xA0 (`VK_LSHIFT`) or 0xA1
(`VK_RSHIFT`); `is_shift` matches all three. For a Shift event, in this order:

1. `e.down` → `trigger.shift_down(e.time)`.
2. `e.ctrl_alt || e.win` (read by the hook with `GetAsyncKeyState` before the event) → `trigger.other()`:
   Ctrl+Shift and Alt+Shift are the system layout hotkeys, Win+Shift+S is the snipping tool; neither is a tap, and
   the layout change they cause must not be followed by a conversion (`shift_with_ctrl_or_alt_held_is_a_hotkey_not_a_tap`,
   `shift_with_win_held_is_a_hotkey_not_a_tap`). A plain double Shift right after still works, because `other()`
   only clears what was pending.
3. `!e.down && trigger.shift_up(e.time)` → `Effect::Convert`, else `Effect::Pass`. Shift itself always passes
   through to the app.

For every other key-down, `trigger.other()` runs first, before the phrase or Caps Lock logic. That is what makes
Shift+letter a capital and not a tap (`typing_capitals_does_not_fire`), and a key between two taps a cancel
(`key_between_shift_taps_cancels`); Num Lock and Scroll Lock are the exception, `capture` passes them before
`other()` because the LED toggles kbswitch injects echo back as unmarked key-downs
(`scroll_lock_between_shift_taps_does_not_cancel`). Mouse buttons call `Capture::click` → `trigger.other()`
(`click_resets_the_phrase_and_a_pending_tap`). A swallowed Caps Lock (`caps_lock` mode `off` or `switch_layout`)
also calls `other()` (`swallowed_caps_lock_cancels_a_pending_shift_tap`), although it changes no text.

The Caps Lock branch runs before the Shift branch in `key`, and a window change (`e.window != self.window`) resets
the phrase but not the trigger: double Shift in a window you just switched to still converts the selection there.

## Pause and Shift+Pause (`src/capture.rs`)

On a key-down with `e.vk == VK_PAUSE` (0x13, the Pause/Break key) and neither `e.ctrl_alt` nor `e.win`:
`e.shift` → `Effect::ConvertSelection`, else `Effect::ConvertPause`.

- Pause alone is a one-key double Shift (added 2026-10-18 on request, like Punto Switcher's Break): the hook
  swallows it and posts `request_convert(0)`, so step 3 runs `convert::run` (typed phrase, else the selection,
  else only the layout switch). No modifier is held, so no wait for a release is needed; the Pause key sends its
  make and break together, so its up arrives before step 3 injects anything
  (`pause_alone_converts_the_phrase_and_is_swallowed`).
- Shift+Pause is for apps that use double Shift themselves (IDEs: "search everywhere") and converts only the
  selection.

Decisions for both:

- Checked after `trigger.other()`, so a pending Shift tap is cancelled and the Shift of the chord is interrupted and its up is no tap; after the chord a
  fresh double Shift works (`shift_pause_converts_the_selection_and_is_swallowed`).
- Before the phrase logic, so the Pause never reaches `Buffer::key_down`: Pause alone needs the phrase intact for
  the retype; Shift+Pause has step 3 forget it instead (`run_selection` → `capture().forget()`), since the text on
  screen changes.
- Swallowed by the hook (`LRESULT(1)`) in both cases: Pause does nothing useful in editors, and a passed-through Pause would be
  recorded as a Reset key. Its key-up passes (`Effect::Pass`, the up branch has no Pause check).
- Fires on the key-down, unlike double Shift, because there is no second tap to wait for; for Shift+Pause the
  user's Shift is therefore usually still held, and step 3 waits up to 1 s for its release before injecting input.
- With Ctrl, Alt or Win held (Ctrl+Shift+Pause, Win+Pause opens System settings; `VK_CANCEL` arrives as 0x03 for
  Ctrl+Pause anyway) the key passes through and resets the phrase like any Reset-class key
  (`pause_with_ctrl_alt_or_win_is_no_trigger`). Apps that need a bare Pause (debuggers, games) no longer get it
  while kbswitch runs; tray "Enabled" off does not change that, only the post is gated.
- Not configurable: Pause is the one key Windows and common editors leave unused; a setting would need a key parser.

## Handover (`src/hook.rs`, `src/main.rs`, `src/app.rs`)

```text
keyboard_proc ─ Effect::Convert ──────────► request_convert(0)
              ─ Effect::ConvertPause ─────► request_convert(0); return LRESULT(1)
              ─ Effect::ConvertSelection ─► request_convert(SELECTION_ONLY); return LRESULT(1)
request_convert(what): ENABLED && !CONVERTING → post_with(WM_KBSWITCH_CONVERT, what)
post_with: PostThreadMessageW(MAIN_THREAD, msg, WPARAM(what), LPARAM(0))
main loop: WM_KBSWITCH_CONVERT with msg.hwnd invalid → app.convert(wParam == SELECTION_ONLY)
App::convert(selection_only): convert::run_selection(&settings) | convert::run(&settings)
```

- `ENABLED` (tray "Enabled", `settings.enabled`) and `CONVERTING` (set by step 3 while it runs) gate only the
  post; the state machine runs regardless, so turning Enabled on needs no re-tap and a tap during a conversion is
  simply dropped.
- `WM_KBSWITCH_CONVERT` = `WM_APP + 1`; `SELECTION_ONLY` = 1 in `wParam`. `msg.hwnd.is_invalid()` tells a thread
  message from a window message with the same number.
- Never convert inside the hook: the hook's own injected Ctrl+C would wait for the hook that is blocked in the
  conversion (deadlock), and Windows drops a hook slower than about 1 s. The post returns at once.
- Thread messages are lost while a modal loop runs on the main thread (the tray menu is open): a double Shift then
  does nothing, by design.

## Tests

All cases: [test-cases](../../quality-gate/test-cases/SKILL.md), sections trigger.rs and capture.rs. The ones a
timing change breaks first: `two_fast_taps_fire`, `slow_second_tap_does_not_fire`, `long_hold_is_not_a_tap`,
`timing_is_configurable`, `timestamps_wrap`. The hook and the thread message have no tests: `hook.rs` is
`#[cfg(windows)]` and only reads state and posts.
