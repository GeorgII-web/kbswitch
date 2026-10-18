//! The main thread's state and what each message does: settings, LED and tray.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

use tray_icon::menu::MenuEvent;

use crate::settings::{self, CapsLock, Settings};
use crate::spell::Spelling;
use crate::{convert, detect, hook, lang, layout, led, tray};

/// How often a changed `settings.json` is picked up.
const SETTINGS_POLL: Duration = Duration::from_secs(1);
/// Scroll Lock toggles of a wrong-layout warning, one per tick: three blinks, ending as it was.
const BLINK_TOGGLES: u8 = 6;

pub struct App {
    settings: Settings,
    path: PathBuf,
    /// mtime of the settings file when last loaded or saved.
    modified: Option<SystemTime>,
    polled: Instant,
    /// What kbswitch set the LED to; set only when the layout changes, so it never fights a stale read or the user.
    led_ru: Option<bool>,
    /// The `caps_lock` mode last applied.
    caps_lock: Option<CapsLock>,
    /// Created once detection is on; `None` while it is off or the spell checkers are missing.
    spelling: Option<Spelling>,
    /// Scroll Lock toggles left of a running wrong-layout blink.
    blink: u8,
    tray: tray::Tray,
}

impl App {
    pub fn new() -> Result<Self, String> {
        let path = settings::path();
        let (settings, error) = settings::load_or_create(&path);
        let tray = tray::Tray::new(&settings)
            .map_err(|e| format!("Creating the tray icon failed: {e}"))?;
        let mut app = Self {
            modified: settings::modified(&path),
            settings,
            path,
            polled: Instant::now(),
            led_ru: None,
            caps_lock: None,
            spelling: None,
            blink: 0,
            tray,
        };
        app.apply(error.as_deref());
        app.tray.set_language(layout::foreground_lang());
        debug_log!("settings: {}", app.path.display());
        Ok(app)
    }

    /// Double Shift, or Shift+Pause (`selection_only`).
    pub fn convert(&self, selection_only: bool) {
        if selection_only {
            convert::run_selection(&self.settings);
        } else {
            convert::run(&self.settings);
        }
    }

    /// A Space ended a word: blink when it was typed in the wrong layout.
    pub fn check_word(&mut self) {
        let word = hook::capture().take_word();
        let (Some((keys, typed_in)), Some(spelling)) = (word, &self.spelling) else {
            return;
        };
        if detect::wrong_layout(spelling, &keys, typed_in) {
            debug_log!("wrong layout: {} keys", keys.len());
            // restarted mid-blink with the LED inverted: one toggle more, so it still ends as it was
            self.blink = BLINK_TOGGLES + self.blink % 2;
        }
    }

    /// Caps Lock in the `switch_layout` mode.
    pub fn switch_layout(&self) {
        if !layout::toggle(self.settings.switch_hotkey) {
            debug_log!("layout switch not confirmed");
        }
    }

    /// Timer tick: the tray icon and the LED follow the layout; about once a second a changed settings file.
    pub fn tick(&mut self) {
        if self.polled.elapsed() >= SETTINGS_POLL {
            self.polled = Instant::now();
            self.reload_if_changed();
        }
        let current = layout::foreground_lang();
        self.tray.set_language(current);
        if self.blink > 0 {
            // an even number of toggles: the LED ends as it was, its layout tracking stays right
            led::toggle_scroll_lock();
            self.blink -= 1;
        } else if self.settings.led {
            let ru = current != lang::EN && current != 0; // on for any layout but EN; 0 = not read yet
            if self.led_ru != Some(ru) {
                led::scroll_lock(ru);
                self.led_ru = Some(ru);
            }
        }
    }

    /// False when the user chose Exit.
    pub fn menu(&mut self, event: &MenuEvent) -> bool {
        match self.tray.handle(event) {
            tray::Action::Exit => return false,
            tray::Action::Enabled(on) => {
                self.settings.enabled = on;
                self.save();
            }
            tray::Action::Led(on) => {
                self.settings.led = on;
                self.save();
            }
            tray::Action::Detect(on) => {
                self.settings.detect_wrong_layout = on;
                self.save();
            }
            tray::Action::CapsOff(off) => {
                self.settings.caps_lock = if off { CapsLock::Off } else { CapsLock::Normal };
                self.save();
            }
            tray::Action::OpenSettings => self.open_settings(),
            tray::Action::None => {}
        }
        true
    }

    pub fn exit(&mut self) {
        self.led_off();
    }

    /// Turns the LED off if kbswitch turned it on.
    fn led_off(&mut self) {
        if self.led_ru == Some(true) {
            led::scroll_lock(false);
        }
        self.led_ru = None;
    }

    fn apply(&mut self, error: Option<&str>) {
        hook::ENABLED.store(self.settings.enabled, Ordering::Relaxed);
        let mut detect_error = None;
        if self.settings.detect_wrong_layout && self.spelling.is_none() {
            match Spelling::new() {
                Ok(spelling) => self.spelling = Some(spelling),
                Err(e) => detect_error = Some(e),
            }
        }
        let detect = self.settings.detect_wrong_layout && self.spelling.is_some();
        {
            let mut capture = hook::capture();
            capture.set_timing(self.settings.tap_max_ms, self.settings.double_ms);
            capture.set_caps_lock(self.settings.caps_lock);
            capture.set_detect(detect);
        }
        let caps_lock = self.settings.caps_lock;
        if caps_lock != CapsLock::Normal && self.caps_lock != Some(caps_lock) {
            led::caps_lock_off();
        }
        self.caps_lock = Some(caps_lock);
        if !self.settings.led {
            self.led_off();
        }
        self.tray
            .show(&self.settings, error.or(detect_error.as_deref()));
    }

    fn save(&mut self) {
        let error = settings::save(&self.path, &self.settings)
            .err()
            .map(|e| format!("{} not saved: {e}", settings::FILE));
        self.modified = settings::modified(&self.path);
        self.apply(error.as_deref());
    }

    /// A missing file counts as unchanged: it is written again on the next save.
    fn reload_if_changed(&mut self) {
        let modified = settings::modified(&self.path);
        if modified.is_none() || modified == self.modified {
            return;
        }
        self.modified = modified;
        match settings::load(&self.path) {
            Ok(settings) => {
                self.settings = settings;
                self.apply(None);
            }
            // keep the settings in use until the file is fixed
            Err(e) => self.apply(Some(&e)),
        }
    }

    fn open_settings(&mut self) {
        if !self.path.exists() {
            self.save();
        }
        if let Err(e) = std::process::Command::new("notepad.exe")
            .arg(&self.path)
            .spawn()
        {
            debug_log!("notepad: {e}");
        }
    }
}
