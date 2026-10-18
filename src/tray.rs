use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::autostart;
use crate::icon::{self, Look};
use crate::settings::{CapsLock, Settings};

pub struct Tray {
    icon: TrayIcon,
    enabled: CheckMenuItem,
    led: CheckMenuItem,
    detect: CheckMenuItem,
    /// `caps_lock` = `off`; `switch_layout` (file only) shows unticked and is kept until the box is ticked.
    caps_off: CheckMenuItem,
    autostart: CheckMenuItem,
    settings: MenuItem,
    exit: MenuItem,
    /// What the picture is drawn from, and the picture shown now.
    language: u16,
    active: bool,
    error: bool,
    look: Look,
}

/// What a menu click asks the app to do; autostart is handled here, the registry is its store.
pub enum Action {
    None,
    Enabled(bool),
    Led(bool),
    Detect(bool),
    /// The "Disable Caps Lock" box: true → `caps_lock: off`, false → `normal`.
    CapsOff(bool),
    OpenSettings,
    Exit,
}

impl Tray {
    pub fn new(settings: &Settings) -> Result<Self, Box<dyn std::error::Error>> {
        let enabled = CheckMenuItem::new("Enabled", true, settings.enabled, None);
        let led = CheckMenuItem::new("Scroll Lock LED shows not EN", true, settings.led, None);
        let detect = CheckMenuItem::new(
            "Blink LED on wrong layout",
            true,
            settings.detect_wrong_layout,
            None,
        );
        let caps_off = CheckMenuItem::new(
            "Disable Caps Lock",
            true,
            settings.caps_lock == CapsLock::Off,
            None,
        );
        let autostart =
            CheckMenuItem::new("Start with Windows", true, autostart::is_enabled(), None);
        let open = MenuItem::new("Settings…", true, None);
        let exit = MenuItem::new("Exit", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &enabled,
            &led,
            &detect,
            &caps_off,
            &autostart,
            &PredefinedMenuItem::separator(),
            &open,
            &exit,
        ])?;
        let look = Look::new(0, settings.enabled, false);
        let icon = TrayIconBuilder::new()
            .with_tooltip("kbswitch")
            .with_menu(Box::new(menu))
            .with_icon(picture(look))
            .build()?;
        Ok(Self {
            icon,
            enabled,
            led,
            detect,
            caps_off,
            autostart,
            settings: open,
            exit,
            language: 0,
            active: settings.enabled,
            error: false,
            look,
        })
    }

    pub fn handle(&self, event: &MenuEvent) -> Action {
        if event.id == *self.enabled.id() {
            Action::Enabled(self.enabled.is_checked())
        } else if event.id == *self.led.id() {
            Action::Led(self.led.is_checked())
        } else if event.id == *self.detect.id() {
            Action::Detect(self.detect.is_checked())
        } else if event.id == *self.caps_off.id() {
            Action::CapsOff(self.caps_off.is_checked())
        } else if event.id == *self.autostart.id() {
            if let Err(e) = autostart::set(self.autostart.is_checked()) {
                debug_log!("autostart: {e}");
            }
            self.autostart.set_checked(autostart::is_enabled());
            Action::None
        } else if event.id == *self.settings.id() {
            Action::OpenSettings
        } else if event.id == *self.exit.id() {
            Action::Exit
        } else {
            Action::None
        }
    }

    /// Checkboxes, icon and tooltip for `settings`; an `error` turns the icon amber and goes into the tooltip.
    pub fn show(&mut self, settings: &Settings, error: Option<&str>) {
        self.enabled.set_checked(settings.enabled);
        self.led.set_checked(settings.led);
        self.detect.set_checked(settings.detect_wrong_layout);
        self.caps_off
            .set_checked(settings.caps_lock == CapsLock::Off);
        self.active = settings.enabled;
        self.error = error.is_some();
        self.redraw();
        let tip = error.map_or_else(|| "kbswitch".to_owned(), |e| format!("kbswitch: {e}"));
        let _ = self.icon.set_tooltip(Some(tooltip(&tip)));
    }

    /// The foreground window's layout language; the icon color follows it.
    pub fn set_language(&mut self, language: u16) {
        self.language = language;
        self.redraw();
    }

    fn redraw(&mut self) {
        let look = Look::new(self.language, self.active, self.error);
        if look != self.look {
            self.look = look;
            let _ = self.icon.set_icon(Some(picture(look)));
        }
    }
}

fn picture(look: Look) -> Icon {
    Icon::from_rgba(look.rgba(), icon::SIZE, icon::SIZE).expect("valid RGBA icon")
}

/// The shell keeps at most 127 UTF-16 units of a tooltip plus the terminating NUL.
fn tooltip(text: &str) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= 127
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_fits_the_shell_buffer() {
        assert_eq!(tooltip("kbswitch"), "kbswitch");
        assert_eq!(tooltip(&"x".repeat(200)).encode_utf16().count(), 127);
        // a character outside the BMP takes two units and is never split
        assert_eq!(tooltip(&format!("{}😀", "x".repeat(126))), "x".repeat(126));
    }
}
