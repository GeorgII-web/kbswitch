//! Double Shift detection. Times are key event timestamps in ms (wrapping, as in `KBDLLHOOKSTRUCT.time`).

/// Defaults; `settings.json` can change them.
pub const TAP_MAX_MS: u32 = 250;
pub const DOUBLE_MS: u32 = 300;

pub struct DoubleShift {
    tap_max_ms: u32,
    double_ms: u32,
    down_at: Option<u32>,
    interrupted: bool,
    last_tap_up: Option<u32>,
}

impl DoubleShift {
    pub const fn new() -> Self {
        Self {
            tap_max_ms: TAP_MAX_MS,
            double_ms: DOUBLE_MS,
            down_at: None,
            interrupted: false,
            last_tap_up: None,
        }
    }

    /// Longest Shift press that counts as a tap, and longest gap between the two taps.
    pub fn set_timing(&mut self, tap_max_ms: u32, double_ms: u32) {
        self.tap_max_ms = tap_max_ms;
        self.double_ms = double_ms;
    }

    pub fn shift_down(&mut self, t: u32) {
        if self.down_at.is_some() {
            return; // auto-repeat or the other Shift
        }
        if self
            .last_tap_up
            .is_some_and(|up| t.wrapping_sub(up) > self.double_ms)
        {
            self.last_tap_up = None;
        }
        self.down_at = Some(t);
        self.interrupted = false;
    }

    /// Returns true on the release that completes a double tap.
    pub fn shift_up(&mut self, t: u32) -> bool {
        let Some(down) = self.down_at.take() else {
            return false;
        };
        if self.interrupted || t.wrapping_sub(down) > self.tap_max_ms {
            self.last_tap_up = None;
            return false;
        }
        if self.last_tap_up.take().is_some() {
            return true;
        }
        self.last_tap_up = Some(t);
        false
    }

    /// Any other key-down or a mouse button.
    pub fn other(&mut self) {
        self.interrupted = true;
        self.last_tap_up = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(d: &mut DoubleShift, down: u32, up: u32) -> bool {
        d.shift_down(down);
        d.shift_up(up)
    }

    #[test]
    fn two_fast_taps_fire() {
        let mut d = DoubleShift::new();
        assert!(!tap(&mut d, 0, 80));
        assert!(tap(&mut d, 200, 280));
    }

    #[test]
    fn slow_second_tap_does_not_fire() {
        let mut d = DoubleShift::new();
        tap(&mut d, 0, 80);
        assert!(!tap(&mut d, 500, 580));
    }

    #[test]
    fn long_hold_is_not_a_tap() {
        let mut d = DoubleShift::new();
        tap(&mut d, 0, 80);
        assert!(!tap(&mut d, 200, 600));
    }

    #[test]
    fn shift_with_letter_is_not_a_tap() {
        let mut d = DoubleShift::new();
        tap(&mut d, 0, 80);
        d.shift_down(200);
        d.other();
        assert!(!d.shift_up(260));
    }

    #[test]
    fn key_between_taps_cancels() {
        let mut d = DoubleShift::new();
        tap(&mut d, 0, 80);
        d.other();
        assert!(!tap(&mut d, 200, 280));
    }

    #[test]
    fn auto_repeat_counts_once() {
        let mut d = DoubleShift::new();
        tap(&mut d, 0, 80);
        d.shift_down(150);
        d.shift_down(180);
        assert!(d.shift_up(220));
    }

    #[test]
    fn three_taps_fire_once() {
        let mut d = DoubleShift::new();
        let fired = [(0, 80), (200, 280), (400, 480)]
            .iter()
            .filter(|&&(down, up)| tap(&mut d, down, up))
            .count();
        assert_eq!(fired, 1);
    }

    #[test]
    fn timing_is_configurable() {
        let mut d = DoubleShift::new();
        d.set_timing(100, 150);
        tap(&mut d, 0, 80);
        assert!(!tap(&mut d, 300, 380)); // gap 220 > 150
        assert!(!tap(&mut d, 400, 520)); // press 120 > 100
        tap(&mut d, 600, 650);
        assert!(tap(&mut d, 750, 800));
    }

    #[test]
    fn timestamps_wrap() {
        let mut d = DoubleShift::new();
        tap(&mut d, u32::MAX - 100, u32::MAX - 20);
        assert!(tap(&mut d, 100, 180));
    }
}
