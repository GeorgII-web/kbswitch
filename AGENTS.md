# kbswitch

Windows tray app in Rust that fixes text typed in the wrong keyboard layout (EN ↔ RU), like Caramba Switcher or
Punto Switcher. A double Shift press converts the phrase typed since the last reset or, when nothing was typed, the
selected text, and switches the layout of the foreground window; Shift+Pause converts only the selected text, for
apps that use double Shift themselves. The tray icon (the layout code EN or RU as letters, blue or red) and
the Scroll Lock LED (on = not EN) show the layout.
A word typed in the wrong layout blinks the Scroll Lock LED. Caps Lock can be disabled from the tray ("Disable Caps Lock") or made to switch the layout.
Settings live in `settings.json` (`%APPDATA%\kbswitch`, or next to the exe) and apply on save.

## Scope

- Windows only (Win32 API via the `windows` crate), x86_64, single `.exe`, no installer, no console window.
- Layouts: EN (US) and RU only. The pair is a constant, not a setting.
- Conversion runs only on double Shift or Shift+Pause; wrong-layout detection only warns (Scroll Lock blink), it never converts.

## Pipeline

| Step | What | Skill |
|---|---|---|
| 1 | Low-level keyboard and mouse hooks record the typed phrase and its layout; reset rules | [capture](.agents/skills/domain/step-1-capture/SKILL.md) |
| 2 | Detect double Shift or Shift+Pause and hand over to conversion | [trigger](.agents/skills/domain/step-2-trigger/SKILL.md) |
| 3 | Convert the typed phrase (erase and retype, or select and paste) or the selection (clipboard, restored after); switch layout | [convert](.agents/skills/domain/step-3-convert/SKILL.md) |
| 4 | While typing: a word in the wrong layout blinks the Scroll Lock LED | [detect](.agents/skills/domain/step-4-detect/SKILL.md) |

## Skills

| Skill | Read when |
|---|---|
| [architecture](.agents/skills/general/architecture/SKILL.md) | any code change: module map, threads, invariants, code rules |
| [step-1-capture](.agents/skills/domain/step-1-capture/SKILL.md) | keyboard/mouse hook, the phrase and its layout, what resets it, Backspace, the Caps Lock modes, modifiers |
| [step-2-trigger](.agents/skills/domain/step-2-trigger/SKILL.md) | double Shift detection, timing, Shift+Pause, false or missed triggers |
| [step-3-convert](.agents/skills/domain/step-3-convert/SKILL.md) | typed vs selection conversion, `SendInput` replay, clipboard, layout switching |
| [step-4-detect](.agents/skills/domain/step-4-detect/SKILL.md) | wrong-layout detection, the spell checker, the warning blink |
| [clipboard-input](.agents/skills/artifact/clipboard-input/SKILL.md) | clipboard snapshot and restore, Win+V exclusion flags, every injected key sequence (`SendInput` builders, marker, scan-code replay) |
| [tray-autostart](.agents/skills/artifact/tray-autostart/SKILL.md) | tray icon (letters, colors, font) and menu, `settings.json` and its keys, "Start with Windows", single instance, startup errors |
| [test-cases](.agents/skills/quality-gate/test-cases/SKILL.md) | every unit, end-to-end and manual test case with steps and expected result, known gaps; adding a test, what a change must not break, what was verified |
| [windows-build](.agents/skills/quality-gate/windows-build/SKILL.md) | building, `fmt`/`clippy`/tests, the Windows end-to-end script, installing without the security prompt, the manual test checklist, the GitHub release workflow, before reporting a change done |

## Working rules

- The repo lives in WSL. A change is done only when the WSL gates in
  [windows-build](.agents/skills/quality-gate/windows-build/SKILL.md) pass: Linux tests, `fmt`, `clippy` for both
  targets and a release exe through `scripts/cargo-gnu.sh`. Once the Windows toolchain works,
  `scripts/cargo-win.sh` also runs the tests natively (never plain `cargo.exe`). A change to the hooks, capture,
  conversion or clipboard also passes `scripts/e2e-notepad.ps1` on Windows.
- Logic goes into the modules that build on Linux (`capture`, `buffer`, `trigger`, `charmap`, `lang`, `icon`,
  `detect`, `settings`) with unit tests; Win32 modules only read state and call the API.
- Work and temp files go to the git-ignored `var/`, never the repo root.
- Every new or changed test (unit, e2e check, manual row) is listed in
  [test-cases](.agents/skills/quality-gate/test-cases/SKILL.md).
- The skills are the full documentation of the code: every function, constant, state field, message and timing of a
  module is described in the skill that owns it, with the reason behind each decision. **Every code change updates
  the skill that covers the changed code in the same commit**, before the change is reported done; a new module
  gets a section in the right skill or a new skill.
- After adding, renaming or removing a skill, update the tables above and run the validator:
  `SKILLS_ROOT=$PWD bash ~/wawibox/.agents/skills/coding/project-skills-validation/validate.sh`.
- Commit every finished change right away: one commit per logical change, short imperative subject, staged files
  only. Remote `git@github.com:GeorgII-web/kbswitch.git` (SSH, `~/.ssh/id_ed25519`).
- `README.md` is the user-facing page (download, keys, settings, build); update it when a key, setting or the
  tray menu changes.
