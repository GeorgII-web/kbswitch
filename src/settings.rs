//! User settings in `settings.json`: next to the exe when that file exists (portable), else
//! `%APPDATA%\kbswitch\settings.json`, created with defaults on first start. Missing keys take defaults, unknown keys
//! are ignored; a broken file is reported and never overwritten by a load.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::trigger::{DOUBLE_MS, TAP_MAX_MS};

pub const FILE: &str = "settings.json";
pub const RETYPE_GAP_MS: u32 = 20;

/// Hotkey sent when a window ignores the layout switch request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hotkey {
    #[default]
    AltShift,
    CtrlShift,
    WinSpace,
}

/// What the Caps Lock key does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsLock {
    #[default]
    Normal,
    /// Does nothing.
    Off,
    /// Switches the layout; Shift+Caps Lock is a normal Caps Lock.
    SwitchLayout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Double Shift converts; off leaves only the LED.
    pub enabled: bool,
    /// Scroll Lock LED shows the layout, on for any layout but EN.
    pub led: bool,
    /// Longest Shift press that counts as a tap, ms (50–1000).
    pub tap_max_ms: u32,
    /// Longest gap between the two taps, ms (100–1000).
    pub double_ms: u32,
    pub switch_hotkey: Hotkey,
    /// Typed phrase: select it and paste the converted text. Off (default): Backspace and retype each key, which
    /// most apps show faster than a growing selection; the clipboard stays untouched.
    pub select_and_paste: bool,
    pub caps_lock: CapsLock,
    /// Blink the Scroll Lock LED when a word typed is no word in its layout but one in the other.
    pub detect_wrong_layout: bool,
    /// Pause between the keys of a retype, ms (0–100): Notepad 11 drops keys sent faster than about 15 ms apart.
    pub retype_gap_ms: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            led: true,
            tap_max_ms: TAP_MAX_MS,
            double_ms: DOUBLE_MS,
            switch_hotkey: Hotkey::AltShift,
            select_and_paste: false,
            caps_lock: CapsLock::Normal,
            detect_wrong_layout: true,
            retype_gap_ms: RETYPE_GAP_MS,
        }
    }
}

impl Settings {
    /// Accepts a UTF-8 BOM (older Notepad); clamps the timings to their ranges.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut settings: Self = serde_json::from_str(text).map_err(|e| format!("{FILE}: {e}"))?;
        settings.tap_max_ms = settings.tap_max_ms.clamp(50, 1000);
        settings.double_ms = settings.double_ms.clamp(100, 1000);
        settings.retype_gap_ms = settings.retype_gap_ms.min(100);
        Ok(settings)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("plain fields always serialize");
        json.push('\n');
        json
    }
}

/// `settings.json` next to the exe when it exists, else in `%APPDATA%\kbswitch`.
pub fn path() -> PathBuf {
    let exe = std::env::current_exe().ok();
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    resolve(exe.as_deref().and_then(Path::parent), appdata.as_deref())
}

fn resolve(exe_dir: Option<&Path>, appdata: Option<&Path>) -> PathBuf {
    let portable = exe_dir.map(|dir| dir.join(FILE));
    match (portable, appdata) {
        (Some(portable), _) if portable.exists() => portable,
        (_, Some(appdata)) => appdata.join("kbswitch").join(FILE),
        (Some(portable), None) => portable,
        (None, None) => PathBuf::from(FILE),
    }
}

pub fn load(path: &Path) -> Result<Settings, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{FILE}: {e}"))?;
    Settings::parse(&text)
}

/// Writes through a temp file, so a crash never leaves half a file.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, settings.to_json())?;
    fs::rename(&tmp, path)
}

/// Settings at start: the file's, or defaults written to a new file. The error is shown to the user.
pub fn load_or_create(path: &Path) -> (Settings, Option<String>) {
    if !path.exists() {
        let defaults = Settings::default();
        let error = save(path, &defaults)
            .err()
            .map(|e| format!("{FILE} not created: {e}"));
        return (defaults, error);
    }
    match load(path) {
        Ok(settings) => (settings, None),
        Err(e) => (Settings::default(), Some(e)),
    }
}

pub fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory per test; tests run in parallel.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kbswitch-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_round_trip_through_json() {
        let json = Settings::default().to_json();
        assert!(json.contains(r#""switch_hotkey": "alt_shift""#), "{json}");
        assert!(json.contains(r#""select_and_paste": false"#), "{json}");
        assert!(json.contains(r#""caps_lock": "normal""#), "{json}");
        assert!(json.contains(r#""detect_wrong_layout": true"#), "{json}");
        assert!(json.contains(r#""retype_gap_ms": 20"#), "{json}");
        assert_eq!(Settings::parse(&json), Ok(Settings::default()));
    }

    #[test]
    fn missing_keys_take_defaults_and_unknown_keys_are_ignored() {
        let settings = Settings::parse(r#"{ "led": false, "from_a_newer_version": 1 }"#);
        let expected = Settings {
            led: false,
            ..Settings::default()
        };
        assert_eq!(settings, Ok(expected));
    }

    #[test]
    fn hotkey_names() {
        let settings = Settings::parse(r#"{ "switch_hotkey": "win_space" }"#).unwrap();
        assert_eq!(settings.switch_hotkey, Hotkey::WinSpace);
        let error = Settings::parse(r#"{ "switch_hotkey": "caps" }"#).unwrap_err();
        assert!(error.contains("unknown variant `caps`"), "{error}");
    }

    #[test]
    fn caps_lock_modes() {
        for (name, mode) in [
            ("normal", CapsLock::Normal),
            ("off", CapsLock::Off),
            ("switch_layout", CapsLock::SwitchLayout),
        ] {
            let settings = Settings::parse(&format!(r#"{{ "caps_lock": "{name}" }}"#)).unwrap();
            assert_eq!(settings.caps_lock, mode);
        }
        assert!(Settings::parse(r#"{ "caps_lock": "on" }"#).is_err());
    }

    #[test]
    fn broken_json_names_the_file_and_line() {
        let error = Settings::parse("{\n  \"enabled\": true\n  \"led\": false\n}").unwrap_err();
        assert!(error.starts_with("settings.json: "), "{error}");
        assert!(error.contains("line 3"), "{error}");
    }

    #[test]
    fn wrong_type_is_an_error() {
        assert!(Settings::parse(r#"{ "enabled": "yes" }"#).is_err());
    }

    #[test]
    fn utf8_bom_is_accepted() {
        assert_eq!(Settings::parse("\u{feff}{}"), Ok(Settings::default()));
    }

    #[test]
    fn timings_are_clamped() {
        let settings = Settings::parse(r#"{ "tap_max_ms": 5, "double_ms": 99999 }"#).unwrap();
        assert_eq!((settings.tap_max_ms, settings.double_ms), (50, 1000));
    }

    #[test]
    fn portable_file_next_to_the_exe_wins() {
        let exe_dir = temp_dir("portable");
        let appdata = Path::new("/appdata");
        let in_appdata = appdata.join("kbswitch").join(FILE);
        assert_eq!(resolve(Some(&exe_dir), Some(appdata)), in_appdata);
        assert_eq!(resolve(Some(&exe_dir), None), exe_dir.join(FILE));
        fs::write(exe_dir.join(FILE), "{}").unwrap();
        assert_eq!(resolve(Some(&exe_dir), Some(appdata)), exe_dir.join(FILE));
        fs::remove_dir_all(exe_dir).unwrap();
    }

    #[test]
    fn first_start_creates_the_file_with_defaults() {
        let dir = temp_dir("create");
        let path = dir.join("kbswitch").join(FILE);
        assert_eq!(load_or_create(&path), (Settings::default(), None));
        assert_eq!(load(&path), Ok(Settings::default()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_then_load_leaves_no_temp_file() {
        let dir = temp_dir("save");
        let path = dir.join(FILE);
        let settings = Settings {
            enabled: false,
            double_ms: 400,
            switch_hotkey: Hotkey::CtrlShift,
            select_and_paste: true,
            ..Settings::default()
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), Ok(settings));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broken_file_gives_defaults_and_stays_untouched() {
        let dir = temp_dir("broken");
        let path = dir.join(FILE);
        fs::write(&path, "{ oops").unwrap();
        let (settings, error) = load_or_create(&path);
        assert_eq!(settings, Settings::default());
        assert!(error.is_some_and(|e| e.starts_with("settings.json: ")));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ oops");
        fs::remove_dir_all(dir).unwrap();
    }
}
