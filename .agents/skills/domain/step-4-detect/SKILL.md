---
name: step-4-detect
description: Step 4 of kbswitch — wrong-layout detection while typing, every piece — the word a Space ends (`src/capture.rs`, `src/buffer.rs`), the `Lexicon` and `wrong_layout` rule (`src/detect.rs`), the COM spell checker (`src/spell.rs`), the Scroll Lock blink (`src/app.rs`, `src/led.rs`), the setting, tray checkbox and amber error. Use when changing detection, its rule, dictionary, timing or LED, or debugging a false, missed or stuck warning.
---

# Step 4: detect

Warns while typing: a word that is no word in the layout it was typed in, but one in the other, blinks the Scroll
Lock LED three times. It never converts; double Shift does (step 3). Entrypoint: `Effect::Check` from
`Capture::key` → `WM_KBSWITCH_CHECK` → `App::check_word`.

| File | Piece |
|---|---|
| `src/buffer.rs` | `Buffer::last_word` |
| `src/capture.rs` | `detect` flag, `word` slot, `Effect::Check`, `set_detect`, `take_word` |
| `src/detect.rs` | pure: `MIN_KEYS`, `Lexicon`, `wrong_layout` |
| `src/charmap.rs` | `typed_text(keys, lang)`: the text the keys produce per layout |
| `src/spell.rs` | `Spelling`: the Windows spell checker as `Lexicon` |
| `src/app.rs` | `spelling`, `blink`, `check_word`, the blink in `tick`, the setup in `apply` |
| `src/led.rs` | `scroll_lock`, `toggle_scroll_lock`, `caps_lock_off` |
| `src/tray.rs` | "Blink LED on wrong layout" checkbox, amber icon and tooltip on a missing checker |

## Flow

1. **Hook thread, `Capture::key`** on a key-down that is not Caps Lock, Shift, or Win+Space: the key goes into
   the buffer, then `if self.detect && e.vk == VK_SPACE` (`0x20`): `word = buffer.last_word()`; when
   `word.len() >= detect::MIN_KEYS` the word is kept as `self.word = Some((word.to_vec(), buffer.lang()))` and the
   effect is `Effect::Check`. The Space is passed on to the app like any key; the phrase goes on (step 1).
2. **`hook::keyboard_proc`**: `Effect::Check` → `post(WM_KBSWITCH_CHECK)` (`WM_APP + 3`, `wParam` 0) with
   `PostThreadMessageW` to the main thread. Nothing else runs in the hook: the spell check is COM and may take
   milliseconds, which would get the hook dropped (`LowLevelHooksTimeout`).
3. **Main thread, `App::check_word`**: `hook::capture().take_word()` (`Option::take`, so a word is checked once)
   and `self.spelling` must both be `Some`, else return. `detect::wrong_layout(spelling, &keys, typed_in)` true →
   `debug_log!("wrong layout: {} keys")` and `self.blink = BLINK_TOGGLES + self.blink % 2`.
4. **`App::tick`** (the 150 ms `WM_TIMER`, `TICK_MS` in `main.rs`): `if self.blink > 0 { led::toggle_scroll_lock();
   self.blink -= 1 }`, else the normal layout LED logic. So one toggle per tick: 6 toggles = 3 blinks in 900 ms.

A word ended while a previous one is still in the slot (two fast Spaces with a 2-key word between, or the main
thread busy in a conversion) overwrites the slot: only the latest word is checked. A thread message is lost while a
modal loop runs (tray menu open), then that word is never checked.

## The word

- `Buffer::last_word`: the last key must be a Space (`split_last`, `last.vk == SPACE`), else `&[]`; the word is the
  keys after the previous Space (`rposition`) or from the start. Two Spaces in a row → empty word → no check.
- Only keys the buffer keeps (step 1 `classify`: letters, digits, OEM punctuation, Space). Enter, Tab, arrows
  reset the buffer, so a word followed by Enter is never checked: the Space is the only trigger.
- `MIN_KEYS = 2`: one-letter words are in both dictionaries (`я`, `a`, `i` = `ш`), the warning would be noise.
- `Capture::set_detect(on)` sets the flag and clears the slot; `App::apply` turns it on only when the setting is on
  **and** the spell checkers exist, so with a missing checker the hook never collects words.

## The rule (`detect::wrong_layout`)

```
keys.len() < MIN_KEYS || typed_in not in {EN, RU}  → false
other = lang::other(typed_in)
is_word(typed_in, typed_text(keys, typed_in)) == Some(false)
  && is_word(other, typed_text(keys, other)) == Some(true)
```

- `Lexicon::is_word(language, text) -> Option<bool>`: `None` = that language can't be checked; `None` on either
  side never flags. Only EN and RU: the phrase layout comes from the window (`KeyEvent::lang`), a German layout
  returns false before any lookup.
- Both conditions are needed: "no word here" alone would flag every typo and every name; "a word there" alone
  would flag text that is a word in both layouts.
- `charmap::typed_text(keys, lang)`: each key to its US character by `us_char(vk, shift)` (letters lower/upper
  by `shift`, digits and their Shift symbols, the OEM keys `;:` `=+` `,<` `-_` `.>` `/?` `` `~ `` `[{` `\|` `]}`
  `'"`; other vks are skipped), then for RU `convert_to(us, RU)` through the EN↔RU table. So `Ghbdtn?` (Shift on
  G and `/`) is `Привет,` in RU. Punctuation stays in the text: the spell checker ignores it, and keeping the
  capital matters for names.
- Caps Lock is not in `Key`, so a word typed with Caps Lock on is checked in lower case; `Shift` is per key as
  recorded. Capitals via Caps Lock in RU would give lower-case RU text, still a valid check.

## The dictionary (`spell::Spelling`)

- `Spelling { en: ISpellChecker, ru: ISpellChecker }`, created by `Spelling::new()` on the main thread and used only
  there (apartment-threaded COM objects).
- `new`: `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` (result ignored: a second init only counts up, the main
  thread is already STA for the tray), `CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)` →
  `ISpellCheckerFactory`, error `"no spell checker: {e}"`. Then `checker(&factory, "en-US")` and `"ru-RU"`.
- `checker`: `factory.IsSupported(tag)` must be `Ok(true)`, then `CreateSpellChecker(tag)`; either failing gives
  `"detection needs the {tag} spell checker (language settings, Basic typing)"`. Windows installs a language's
  spell checker with its "Basic typing" feature; `en-US` is usually there, `ru-RU` often not.
- `is_word`: picks the checker by `lang::EN` / `lang::RU`, else `None`. `checker.Check(HSTRING(text))` returns an
  `IEnumSpellingError`; `errors.Next(&mut first)` returns `S_OK` when an error came back and `S_FALSE` when none:
  `found.is_ok().then_some(found != S_OK)` = `Some(true)` for a word, `Some(false)` for a misspelling, `None` when
  `Check` or `Next` failed. Only the first error is asked for.
- Local Windows API, no word lists in the app, nothing leaves the machine. Logs show key counts, never text.

## Setup and the setting (`App::apply`)

- `detect_wrong_layout` (default `true`) in `settings.json`; tray checkbox "Blink LED on wrong layout"
  (`tray::Action::Detect(bool)` → `settings.detect_wrong_layout = on`, `save`, `apply`).
- `apply`: `if detect_wrong_layout && spelling.is_none()` → `Spelling::new()`, `Ok` stored in `self.spelling`,
  `Err(e)` into `detect_error`. Retried on every apply (settings change, tray toggle), so installing the language
  and toggling the checkbox is enough; never dropped once created, also with the setting off (cheap, and the next
  on needs no COM again).
- `detect = detect_wrong_layout && spelling.is_some()` → `capture.set_detect(detect)` under the `Capture` lock
  together with timing and Caps Lock.
- `tray.show(&settings, error.or(detect_error))`: a settings-file error wins over the detection error. With a
  detection error the icon is `ERROR_AMBER` (`Look::new(_, _, error = true)`, whatever the layout) and the tooltip
  is `"kbswitch: detection needs the ru-RU spell checker (language settings, Basic typing)"` cut to 127 UTF-16
  units (`tray::tooltip`). Unticking the checkbox clears it on the next `apply`.

## The LED (`src/led.rs`)

- `is_on(key)`: `GetKeyState(vk) & 1`, the toggle bit (Scroll Lock, Caps Lock state).
- `set(key, on)`: when the toggle state differs, one `input::taps(key, 1)` (down, up) through `input::send`; it
  carries `KBSWITCH_MARKER`, so the hook ignores it (never recorded, never a reset, never a tap).
- `scroll_lock(on)`: the layout LED, on for any layout but EN (`current != EN && current != 0`), used by `tick` only when the layout changed (`led_ru`).
- `toggle_scroll_lock()`: an unconditional tap, one blink step. No state read: the blink must flip whatever the
  LED shows now.
- `caps_lock_off()`: `set(VK_CAPITAL, false)`, for the `caps_lock` modes of step 1 (not detection).
- Why Scroll Lock: it is the only lock key with an LED that changes nothing while typing; Caps Lock or Num Lock
  toggles would change the characters typed.
- `BLINK_TOGGLES = 6`, an even number: the LED ends as it was, so the layout tracking in `tick` (`led_ru`) stays
  right without a re-sync. A new warning mid-blink: `blink = 6 + blink % 2`; the extra toggle when the count is
  odd (the LED currently inverted) keeps the total even, so it still ends as it was.
- While `blink > 0` the layout LED branch is skipped; with `led` on it catches up after the blink (the `led_ru`
  comparison runs again). With `led` off the blink still runs: the warning does not depend on the layout LED.
- Blink and LED taps are `SendInput`: into an elevated foreground window they are blocked by UIPI like all input
  (the LED state is global, but the injection fails), logged as a short `SendInput` return.

## Limits

- Only on Space, only words of 2+ keys, only EN/RU phrase layouts, only after a key the buffer keeps.
- Caps Lock state unknown (lower case); a word in both dictionaries is never flagged; a misspelling in the right
  layout that happens to be a word in the other is flagged (rare, by design).
- The check runs on the main thread after the Space reached the app: no delay for typing, but a long conversion
  running there delays or drops it.
- Needs both spell checkers; without `ru-RU` nothing is detected and the icon stays amber until the checkbox is off.

## Tests

All cases: [test-cases](../../quality-gate/test-cases/SKILL.md), section detect.rs (the fake `Lexicon` `Words`
and `keys(us)` are described there). The rule's two conditions are pinned by `no_word_in_either_layout_is_left_alone`
and `a_word_of_the_typed_layout_is_right`; `spell.rs`, `led.rs` and `App::check_word` have no tests (COM,
`GetKeyState`, timer): the manual rows `ghbdtn ` blinks three times, `hello ` not, and the checkbox turning it off.
