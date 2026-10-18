---
name: test-cases
description: Every test case of kbswitch in one place — the 76 Linux unit tests and the Windows-only ones per module with what each proves, the 38 end-to-end checks of `scripts/e2e-notepad.ps1` per target (Edit, RichEdit, Windows 11 Notepad) with steps and expected text, the manual checklist, and the known gaps. Use when adding a test, looking for a case that covers a bug, deciding what a change must not break, or reporting what was verified.
---

# Test cases

The three layers and every case in them. Code: `#[cfg(test)]` modules in `src/*.rs` (run with `cargo test`,
Windows-only ones with the test exe from `scripts/cargo-gnu.sh test --no-run`), `scripts/e2e-notepad.ps1` (how to
run it and its mechanics: [windows-build](../windows-build/SKILL.md)), and the manual checklist below. A new rule in
the code gets a unit test; a new user-visible behaviour gets an e2e check or a manual row; both go into this file.

## Unit: buffer.rs (the phrase)

| Test | Proves |
|---|---|
| `appends_letters_digits_punctuation_space` | `Class::Text` keys (0x20, 0-9, A-Z, OEM punctuation, 0xE2) are appended with vk, scan, shift |
| `backspace_pops_and_stops_at_empty` | Backspace pops the last key; on an empty phrase nothing happens |
| `modifiers_and_locks_keep_buffer` | Shift, Ctrl, Alt, Win, Num Lock, Scroll Lock (`Neutral`) change nothing |
| `navigation_enter_and_caps_lock_reset` | Enter, Tab, Esc, Left, Home, Delete, F1, numpad 0, Caps Lock clear the phrase |
| `chord_with_modifier_resets` | a text key with Ctrl, Alt or Win held clears the phrase |
| `full_buffer_starts_over` | key 513 (`MAX_KEYS` 512) starts a new phrase of 1 |
| `last_word_is_the_one_a_space_just_ended` | the keys after the previous Space; empty when the last key is no Space or after two Spaces |
| `layout_change_starts_a_new_phrase` | a key typed in another layout clears and sets `lang` |
| `converted_phrase_goes_on_in_the_new_layout` | `set_lang` after a conversion keeps the keys, later keys in that layout append |

## Unit: capture.rs (hook decisions)

Helpers: `down`/`up`/`tap` build `KeyEvent`s from the `EN_WINDOW` context, `scan = vk + 0x100`, `typed()` lists vks.

| Test | Proves |
|---|---|
| `records_keys_with_scan_code_and_layout` | `phrase()` returns keys with scan codes and the shift flag, and the layout |
| `double_shift_fires_and_keeps_the_phrase` | left then right Shift fires `Convert`; the phrase `[A]` survives |
| `shift_with_ctrl_or_alt_held_is_a_hotkey_not_a_tap` | Alt+Shift taps never fire; a plain double Shift right after does |
| `shift_with_win_held_is_a_hotkey_not_a_tap` | a Win+Shift second tap does not fire |
| `typing_capitals_does_not_fire` | Shift held around a letter: the up passes, letters recorded as shifted |
| `key_between_shift_taps_cancels` | a letter between the taps cancels |
| `scroll_lock_between_shift_taps_does_not_cancel` | 0x91 and 0x90 between the taps (LED echoes) pass; the double Shift still fires |
| `shift_pause_converts_the_selection_and_is_swallowed` | `ConvertSelection` on the Pause down, `Pass` on its up and the Shift up, that Shift is no tap, phrase kept |
| `pause_alone_or_with_ctrl_is_no_trigger` | Pause and Ctrl+Shift+Pause pass and reset the phrase |
| `window_change_starts_a_new_phrase` | a key in another foreground window starts over |
| `click_resets_the_phrase_and_a_pending_tap` | a mouse button clears the phrase and cancels a tap |
| `alt_shift_layout_switch_keeps_the_phrase_until_typing_goes_on` | the hotkey keeps the phrase; the next key in the new layout starts over |
| `win_space_switches_the_layout_without_a_reset` | Win+Space is not fed to the buffer |
| `other_chords_reset` | Ctrl/Alt and Win chords with a text key clear |
| `caps_lock_normal_passes_and_resets_the_phrase` | `normal`: Caps Lock passes, phrase cleared |
| `caps_lock_off_is_swallowed_and_keeps_the_phrase` | `off`: swallowed with and without Shift, phrase kept |
| `caps_lock_switch_layout_switches_once_per_press_and_keeps_the_phrase` | `SwitchLayout` on the first down, repeats and the up swallowed |
| `shift_caps_lock_stays_a_caps_lock_in_switch_layout` | Shift+Caps Lock passes; its release follows the press decision |
| `swallowed_caps_lock_cancels_a_pending_shift_tap` | a swallowed Caps Lock is a cancel |
| `mode_change_while_held_keeps_the_press_decision` | the mode set during a press applies from the next press |
| `space_after_a_word_asks_for_a_check` | `Check` after two keys and Space in RU; `take_word` gives keys and RU once; the phrase keeps the Space |
| `no_check_when_off_short_or_a_second_space` | `Pass` with detection off, after one key, after a second Space, for Win+Space |
| `forget_drops_the_phrase` | `forget()` empties the phrase |
| `converted_phrase_goes_on_in_the_new_layout` | `converted(lang)` keeps the keys in the new layout |

## Unit: trigger.rs (double Shift)

Fake timestamps through `tap(d, down, up)`.

| Test | Proves |
|---|---|
| `two_fast_taps_fire` | taps at 0–80 and 200–280 fire on the second up |
| `slow_second_tap_does_not_fire` | gap 420 > 300: the stale first tap is dropped in `shift_down` |
| `long_hold_is_not_a_tap` | hold 400 > 250 on the second tap: no fire, pending tap cleared |
| `shift_with_letter_is_not_a_tap` | `other()` during the press sets `interrupted`, the up returns false |
| `key_between_taps_cancels` | `other()` between taps clears `last_tap_up` |
| `auto_repeat_counts_once` | repeated downs keep the first `down_at`; the up still fires |
| `three_taps_fire_once` | the fire resets the machine; the third tap is a new first tap |
| `timing_is_configurable` | `set_timing(100, 150)`: gap 220 and hold 120 fail, 100/50 pass |
| `timestamps_wrap` | first tap near `u32::MAX`, second after the wrap, fires |

## Unit: charmap.rs, lang.rs, layout.rs (conversion tables)

| Test | Proves |
|---|---|
| `charmap::tables_are_one_to_one` | no duplicate in either table, equal lengths |
| `en_to_ru`, `ru_to_en` | `convert` picks the direction and maps letters, punctuation and case |
| `every_key_round_trips` | `EN_KEYS` ↔ `RU_KEYS` both ways |
| `mixed_text_follows_the_majority_and_keeps_the_rest` | majority rule, target-layout characters untouched |
| `convert_to_takes_the_direction_it_is_given` | explicit direction, pure punctuation, already converted text |
| `digits_whitespace_and_unknown_stay` | non-table characters pass through, empty text → RU |
| `typed_text_follows_the_layout` | `Ghbdtn,` keys → `Ghbdtn?` in EN, `Привет,` in RU (what retype types) |
| `typed_text_of_digits_punctuation_and_other_keys` | Shift+digits, Space kept, Enter skipped; `,` `'` OEM keys in RU |
| `lang::language_is_the_low_word_of_the_hkl` | HKL decoding |
| `lang::other_toggles_the_pair` | EN ↔ RU; a foreign layout maps to RU |
| `layout::console_classes` (Windows) | the two console classes match, `Notepad` and `""` don't |

## Unit: detect.rs (wrong layout)

Fake `Lexicon` `Words` with a word list per language, `None` for a language without one; `keys(us)` builds `Key`s
from US text (capital = Shift, `,` = 0xBC, `?` = Shift+0xBF).

| Test | Proves |
|---|---|
| `russian_typed_in_en_is_wrong` | `ghbdtn` and `Ghbdtn?` typed in EN flag (shift and `?`→`,` mapping) |
| `english_typed_in_ru_is_wrong` | `hello` (`руддщ`) and the 2-key `is` (`шы`) typed in RU flag |
| `a_word_of_the_typed_layout_is_right` | a word of the typed layout never flags |
| `no_word_in_either_layout_is_left_alone` | `qwzx` flags nowhere: both conditions required |
| `too_short_or_another_layout_is_not_checked` | `z` is under `MIN_KEYS`; a German layout returns false |
| `a_language_without_spell_checker_never_flags` | `None` from the lexicon on either side blocks the warning |

## Unit: settings.rs, icon.rs, tray.rs (shell)

| Test | Proves |
|---|---|
| `settings::defaults_round_trip_through_json` | snake_case enum names, `select_and_paste: false`, `retype_gap_ms: 20`, parse back equal |
| `missing_keys_take_defaults_and_unknown_keys_are_ignored` | `#[serde(default)]`, unknown keys tolerated |
| `hotkey_names` | `win_space` parses; `caps` errors "unknown variant" |
| `caps_lock_modes` | the three names parse; `on` errors |
| `broken_json_names_the_file_and_line` | error starts with `settings.json: ` and names line 3 |
| `wrong_type_is_an_error` | `"enabled": "yes"` rejected, not coerced |
| `utf8_bom_is_accepted` | BOM + `{}` = defaults |
| `timings_are_clamped` | 5 → 50, 99999 → 1000 |
| `portable_file_next_to_the_exe_wins` | the `resolve` table: appdata first, exe dir without appdata, portable wins once it exists |
| `first_start_creates_the_file_with_defaults` | `load_or_create` writes the file and directory, loads it back |
| `save_then_load_leaves_no_temp_file` | tmp + rename leaves one file; non-default values round-trip |
| `broken_file_gives_defaults_and_stays_untouched` | defaults in memory, error reported, file bytes unchanged |
| `icon::color_and_code_follow_the_layout` | EN blue `EN`, RU red `RU`, German grey `DE`, Swiss German `DE`, 0 grey `??` |
| `enabled_is_a_filled_square_with_white_letters` | no clear pixel, row 0 all color, the R's bar and the U's stems at `LEFT`/`TOP` |
| `disabled_is_a_frame_with_colored_letters` | frame pixels colored, (4, 4) clear, the E's bar in the layout color |
| `settings_error_is_amber_whatever_the_layout` | amber filled for EN enabled and RU disabled, code kept |
| `every_glyph_fits_five_columns` | every A–Z row `< 0x20`; `?` starts with `0x0E` |
| `tray::tooltip_fits_the_shell_buffer` (Windows) | 200 x → 127 units; a non-BMP char that would split is dropped |

## Unit: input.rs (Windows)

| Test | Proves |
|---|---|
| `unicode_sends_each_utf16_unit_down_and_up` | `wVk = 0`, the unit as `wScan`, `KEYEVENTF_UNICODE` on down and up |
| `chord_holds_the_modifier_around_the_key` | order down-mod, down-key, up-key, up-mod |
| `selection_uses_extended_arrows_inside_shift` | Shift around the arrows, `EXTENDEDKEY` on each, `select_right` length, `right()` extended |
| `every_event_carries_the_marker` | all builders set `KBSWITCH_MARKER`, event counts |

No unit tests: `hook.rs`, `convert.rs`, `clipboard.rs`, `layout.rs` (except the classes), `led.rs`, `spell.rs`,
`app.rs`, `autostart.rs`, `main.rs`. They only call the API; the two layers below cover them.

## End to end: `scripts/e2e-notepad.ps1` (38 checks)

Real unmarked input into a running kbswitch. `Other(x)` = the three words in the other layout
(`ghbdtn vbh ntcn` ↔ `привет мир тест`); the layout at start is unknown, so a first result may be either spelling.

Per control, `TextBox` (Edit) and `RichTextBox` (RichEdit), 11 checks each (10 for the TextBox):

| # | Steps | Expected |
|---|---|---|
| 1 | empty document, double Shift | still empty (selection path finds nothing, only the layout switches) |
| 2 | type `ghbdtn vbh ntcn` | one of the two spellings |
| 3 | double Shift, four times | each result is `Other` of the previous |
| 4 | type `ghbdtn`, Backspace, `n`, double Shift | `привет` or `ghbdtn` |
| 5 | clipboard `KEEP ME`, type `ghbdtn`, Shift+Home, Shift+Pause | clipboard still `KEEP ME`; text = first word of `Other(typed)` |
| 6a | type `ghbdtn`, Shift+Home, double Shift | first word of `Other(typed)` |
| 6b | type `ghbdtn`, Shift+Left × 6, double Shift | same |
| 6c | (RichEdit only) type `ghbdtn`, Ctrl+A, double Shift | same |
| 7a | type `ghbdtn`, mouse double-click on it, double Shift | same |
| 7b | type `ghbdtn`, mouse drag over it, double Shift | same |

Windows 11 Notepad, a new tab, 10 checks (reading goes through Ctrl+A, which resets the phrase, so the words are
retyped before every conversion):

| # | Steps | Expected |
|---|---|---|
| N1 | empty document, double Shift | still empty |
| N2 | type `ghbdtn vbh ntcn`, double Shift | one of the two spellings |
| N3 | three times: clear, type the words, double Shift | a spelling, and `Other` of the previous result |
| N4 | type `ghbdtn`, Backspace, `n`, double Shift | `привет` or `ghbdtn` |
| N6a | type `ghbdtn`, Shift+Home, double Shift | `привет` or `ghbdtn` |
| N6b | type `ghbdtn`, Shift+Left × 6, double Shift | `привет` or `ghbdtn` |

Last full runs: 38/38 three times in a row (release exe, 2026-10-17).

## Manual checklist

Release exe, in Notepad, a browser text field and Windows Terminal, unless a row says otherwise.

| Case | Expected |
|---|---|
| type `ghbdtn` in EN, double Shift | erased and retyped as `привет` key by key (about 20 ms per key), layout RU, LED on |
| double Shift again | `ghbdtn`, layout EN, LED off |
| `Ghbdtn vbh` with a capital and a space | `Привет мир` |
| typo, Backspace, retype, double Shift | only the final text converted |
| type `ghbdtn`, switch to RU by Alt+Shift or Win+Space, double Shift | `привет`, layout stays RU |
| copy an image or a file in Explorer, select `руддщ` (not in Terminal), double Shift | `hello`; Ctrl+V afterwards pastes the image or file again |
| type `ghbdtn`, select it with Shift+Home, Shift+Pause | `привет`, layout RU; nothing typed in the app from the Pause key |
| Shift+Pause with nothing selected; Pause alone | only the layout switch; Pause reaches the app |
| typing capitals, slow double Shift, Ctrl+Shift twice | no conversion |
| Caps Lock or a click, then double Shift | only the layout switch |
| Terminal: type `ghbdtn`, double Shift | retyped, never Ctrl+C; a selection there only switches the layout |
| switch the layout with the system hotkey | tray icon white `EN` on blue or `RU` on red (German: `DE` on grey), LED on for RU and DE, off for EN, within 150 ms |
| untick "Enabled" | the icon becomes a frame with the letters in the layout color; double Shift does nothing |
| untick "Scroll Lock LED shows not EN" | LED off and stays off; `"led": false` in `settings.json` |
| tray "Settings…", change `double_ms` to `500`, save | Notepad opens the file; slower double taps work within a second |
| break `settings.json` (delete a comma), save | amber icon, tooltip names the line; the old settings keep working |
| copy `settings.json` next to the exe, restart | that file is used, not `%APPDATA%\kbswitch` |
| `"select_and_paste": true`, type and double Shift | selected and replaced at once, clipboard content unchanged afterwards |
| `"retype_gap_ms": 0` in Notepad 11, type three words, double Shift | keys lost or the last one repeated (why the default is 20) |
| `"caps_lock": "switch_layout"`: Caps Lock, then Shift+Caps Lock | layout switches without caps; caps toggles |
| `"caps_lock": "off"` while Caps Lock is on | caps turns off; the key does nothing after |
| tick "Disable Caps Lock" in the tray while Caps Lock is on; untick | caps turns off and the key is dead, `"caps_lock": "off"` in `settings.json`; after unticking the key toggles caps again, `"normal"` |
| type `ghbdtn`, Caps Lock (`switch_layout`), double Shift | `привет`, layout stays RU |
| type `ghbdtn` and Space in EN; then `hello` and Space | Scroll Lock blinks three times; no blink |
| untick "Blink LED on wrong layout", type `ghbdtn` and Space | no blink |
| "Start with Windows" ticked, sign out and in | kbswitch runs from the installed path (`HKCU\...\Run`) |
| start the exe twice | the second instance exits silently (mutex) |

## Known gaps

- Reported, not reproduced (2026-10-17): "select the typed text, double Shift: the text is deleted and one
  random character inserted". Not seen with Shift+Home, Shift+Left, Ctrl+A, mouse double-click or drag in Edit,
  RichEdit or Notepad. The code path that would do it is a retype while a selection exists (the phrase not reset
  by the selection). Needs the app and the selection method.
- No e2e coverage of browsers, Electron apps, Terminal, elevated windows, Caps Lock modes, the LED, the tray
  menu, settings reload or autostart: manual rows only.
- `clipboard.rs` is covered only by the `KEEP ME` check (text); an image on the clipboard is a manual row.
