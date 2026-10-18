use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_LEFT, VK_LSHIFT, VK_RIGHT,
};

/// Marks our own injected input, so the keyboard hook ignores it.
pub const KBSWITCH_MARKER: usize = 0x4B42_5357;

fn event(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: KBSWITCH_MARKER,
            },
        },
    }
}

pub fn down(vk: VIRTUAL_KEY) -> INPUT {
    event(vk, 0, KEYBD_EVENT_FLAGS(0))
}

pub fn up(vk: VIRTUAL_KEY) -> INPUT {
    event(vk, 0, KEYEVENTF_KEYUP)
}

/// `modifier` held while `key` is tapped, e.g. Ctrl+C.
pub fn chord(modifier: VIRTUAL_KEY, key: VIRTUAL_KEY) -> [INPUT; 4] {
    [down(modifier), down(key), up(key), up(modifier)]
}

pub fn taps(vk: VIRTUAL_KEY, count: usize) -> Vec<INPUT> {
    (0..count).flat_map(|_| [down(vk), up(vk)]).collect()
}

/// Arrows are extended keys: without the flag they count as numpad keys, which Num Lock turns into digits or
/// moves without Shift.
fn arrow(vk: VIRTUAL_KEY) -> [INPUT; 2] {
    [
        event(vk, 0, KEYEVENTF_EXTENDEDKEY),
        event(vk, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
    ]
}

fn shifted_arrows(vk: VIRTUAL_KEY, count: usize) -> Vec<INPUT> {
    let mut inputs = vec![down(VK_LSHIFT)];
    inputs.extend((0..count).flat_map(|_| arrow(vk)));
    inputs.push(up(VK_LSHIFT));
    inputs
}

/// Shift+Left × `count`: selects the `count` characters before the caret.
pub fn select_left(count: usize) -> Vec<INPUT> {
    shifted_arrows(VK_LEFT, count)
}

/// Shift+Right × `count`: undoes `select_left`, also where Shift+Left only moved the caret.
pub fn select_right(count: usize) -> Vec<INPUT> {
    shifted_arrows(VK_RIGHT, count)
}

/// Right: collapses a selection to its end.
pub fn right() -> [INPUT; 2] {
    arrow(VK_RIGHT)
}

/// `text` as Unicode key events (`KEYEVENTF_UNICODE`, one down/up per UTF-16 unit): the characters arrive whatever
/// the layout is, so the result never depends on when the app applies a layout switch.
pub fn unicode(text: &str) -> Vec<INPUT> {
    text.encode_utf16()
        .flat_map(|unit| {
            [
                event(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE),
                event(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        })
        .collect()
}

/// `inputs` one key (down and up) per `SendInput` call, `gap_ms` apart: Notepad 11 drops keys and repeats the
/// last one when a batch or keys 3 ms apart arrive, takes keys 15 ms apart with a rare drop and 20 ms apart in full. `gap_ms` = 0 sends one
/// batch.
pub fn send_paced(inputs: &[INPUT], gap_ms: u32) -> bool {
    if gap_ms == 0 {
        return send(inputs);
    }
    inputs.chunks(2).all(|key| {
        let sent = send(key);
        std::thread::sleep(std::time::Duration::from_millis(u64::from(gap_ms)));
        sent
    })
}

/// Returns false when not all events were sent (UIPI blocks input into elevated windows).
pub fn send(inputs: &[INPUT]) -> bool {
    // SAFETY: the slice and the size describe valid INPUT structs.
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYBDINPUT, VK_BACK, VK_C, VK_CONTROL};

    fn ki(input: &INPUT) -> KEYBDINPUT {
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        // SAFETY: every INPUT built here is a keyboard input.
        unsafe { input.Anonymous.ki }
    }

    #[test]
    fn unicode_sends_each_utf16_unit_down_and_up() {
        let events: Vec<_> = unicode("пр")
            .iter()
            .map(ki)
            .map(|k| (k.wVk, k.wScan, k.dwFlags))
            .collect();
        let none = VIRTUAL_KEY(0);
        assert_eq!(
            events,
            [
                (none, 0x43F, KEYEVENTF_UNICODE),
                (none, 0x43F, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
                (none, 0x440, KEYEVENTF_UNICODE),
                (none, 0x440, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        );
    }

    #[test]
    fn chord_holds_the_modifier_around_the_key() {
        let events: Vec<_> = chord(VK_CONTROL, VK_C)
            .iter()
            .map(ki)
            .map(|k| (k.wVk, k.dwFlags))
            .collect();
        assert_eq!(
            events,
            [
                (VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
                (VK_C, KEYBD_EVENT_FLAGS(0)),
                (VK_C, KEYEVENTF_KEYUP),
                (VK_CONTROL, KEYEVENTF_KEYUP),
            ]
        );
    }

    #[test]
    fn selection_uses_extended_arrows_inside_shift() {
        let events: Vec<_> = select_left(2)
            .iter()
            .map(ki)
            .map(|k| (k.wVk, k.dwFlags))
            .collect();
        let ext = KEYEVENTF_EXTENDEDKEY;
        assert_eq!(
            events,
            [
                (VK_LSHIFT, KEYBD_EVENT_FLAGS(0)),
                (VK_LEFT, ext),
                (VK_LEFT, ext | KEYEVENTF_KEYUP),
                (VK_LEFT, ext),
                (VK_LEFT, ext | KEYEVENTF_KEYUP),
                (VK_LSHIFT, KEYEVENTF_KEYUP),
            ]
        );
        assert_eq!(select_right(2).len(), 6);
        assert!(
            right()
                .iter()
                .map(ki)
                .all(|k| k.wVk == VK_RIGHT && k.dwFlags.contains(ext))
        );
    }

    #[test]
    fn every_event_carries_the_marker() {
        let all = [
            taps(VK_BACK, 3),
            chord(VK_CONTROL, VK_C).to_vec(),
            unicode("ab"),
            select_left(1),
            right().to_vec(),
            unicode("a"),
        ]
        .concat();
        assert_eq!(all.len(), 6 + 4 + 4 + 4 + 2 + 2);
        assert!(all.iter().all(|i| ki(i).dwExtraInfo == KBSWITCH_MARKER));
    }
}
