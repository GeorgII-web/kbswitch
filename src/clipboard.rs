//! The clipboard: Unicode text for the conversion, and a snapshot of every global-memory format (text, HTML,
//! files, DIB images …) that is put back after a conversion. GDI handles (CF_BITMAP, metafiles, palettes) and
//! owner-drawn or private formats can't be copied and are the only ones lost.

use std::{ptr, slice, thread, time::Duration};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::core::w;

const CF_UNICODETEXT: u32 = 13;

/// What was on the clipboard: each global-memory format with its bytes.
pub struct Snapshot(Vec<(u32, Vec<u8>)>);

/// Formats whose handle is not an HGLOBAL, or that only their owner can render.
fn is_global(format: u32) -> bool {
    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;
    !matches!(
        format,
        CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE
        | 0x0080..=0x008F // CF_OWNERDISPLAY, CF_DSP*
        | 0x0200..=0x03FF // CF_PRIVATEFIRST..CF_GDIOBJLAST
    )
}

pub fn sequence() -> u32 {
    // SAFETY: plain Win32 query.
    unsafe { GetClipboardSequenceNumber() }
}

/// Runs `f` with the clipboard open; another app may hold it for a moment, so retry.
fn with_open<T>(f: impl FnOnce() -> T) -> Option<T> {
    for _ in 0..10 {
        // SAFETY: opened and closed on the same thread around `f`.
        if unsafe { OpenClipboard(None) }.is_ok() {
            let result = f();
            // SAFETY: closes what was opened above.
            let _ = unsafe { CloseClipboard() };
            return Some(result);
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

pub fn text() -> Option<String> {
    with_open(|| {
        // SAFETY: the clipboard is open; the CF_UNICODETEXT handle is a UTF-16 global of `GlobalSize` bytes,
        // read only up to its first NUL and within that size.
        unsafe {
            let mem = HGLOBAL(GetClipboardData(CF_UNICODETEXT).ok()?.0);
            let ptr = GlobalLock(mem).cast::<u16>().cast_const();
            if ptr.is_null() {
                return None;
            }
            let units = slice::from_raw_parts(ptr, GlobalSize(mem) / 2);
            let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..len]);
            let _ = GlobalUnlock(mem);
            Some(text)
        }
    })
    .flatten()
}

/// Copies every global-memory format; `None` when the clipboard couldn't be opened.
pub fn snapshot() -> Option<Snapshot> {
    with_open(|| {
        let mut formats = Vec::new();
        let mut format = 0;
        loop {
            // SAFETY: the clipboard is open; enumeration ends with 0.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            if !is_global(format) {
                continue;
            }
            // SAFETY: the clipboard is open; a global-memory format's handle is an HGLOBAL of `GlobalSize` bytes.
            let bytes = unsafe {
                let Ok(handle) = GetClipboardData(format) else {
                    continue;
                };
                let mem = HGLOBAL(handle.0);
                let ptr = GlobalLock(mem).cast::<u8>().cast_const();
                if ptr.is_null() {
                    continue;
                }
                let bytes = slice::from_raw_parts(ptr, GlobalSize(mem)).to_vec();
                let _ = GlobalUnlock(mem);
                bytes
            };
            formats.push((format, bytes));
        }
        Snapshot(formats)
    })
}

/// # Safety
/// The clipboard must be open and emptied by this thread.
unsafe fn put(format: u32, bytes: &[u8]) -> bool {
    // SAFETY: the global is sized for `bytes` and owned by the clipboard after SetClipboardData.
    unsafe {
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else {
            return false;
        };
        let dst = GlobalLock(mem).cast::<u8>();
        if dst.is_null() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        let _ = GlobalUnlock(mem);
        if SetClipboardData(format, Some(HANDLE(mem.0))).is_err() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        true
    }
}

pub fn set_text(text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();
    with_open(|| {
        // SAFETY: the clipboard is open and emptied on this thread before `put`.
        unsafe {
            if EmptyClipboard().is_err() || !put(CF_UNICODETEXT, &bytes) {
                return false;
            }
            keep_out_of_history();
            true
        }
    })
    .unwrap_or(false)
}

/// Marks what we put on the clipboard as short-lived: not for the Win+V history, the cloud clipboard or clipboard
/// managers. Each format holds a DWORD 0.
///
/// # Safety
/// The clipboard must be open and emptied by this thread.
unsafe fn keep_out_of_history() {
    for name in [
        w!("ExcludeClipboardContentFromMonitorProcessing"),
        w!("CanIncludeInClipboardHistory"),
        w!("CanUploadToCloudClipboard"),
    ] {
        // SAFETY: plain Win32 calls; the zeroed global is owned by the clipboard after SetClipboardData.
        unsafe {
            let format = RegisterClipboardFormatW(name);
            let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, 4) else {
                continue;
            };
            if format == 0 || SetClipboardData(format, Some(HANDLE(mem.0))).is_err() {
                let _ = GlobalFree(Some(mem));
            }
        }
    }
}

/// Puts back what `snapshot()` saw. No snapshot (the clipboard couldn't be read) → nothing is changed, so our
/// converted text stays rather than the clipboard being emptied. Waits longer than a read: the pasting app may
/// still hold the clipboard.
pub fn restore(saved: Option<Snapshot>) {
    let Some(Snapshot(formats)) = saved else {
        debug_log!("clipboard not restored: no snapshot");
        return;
    };
    for _ in 0..10 {
        let done = with_open(|| {
            // SAFETY: the clipboard is open on this thread; emptied before the formats are put back.
            unsafe {
                EmptyClipboard().is_ok()
                    && formats.iter().all(|(format, bytes)| put(*format, bytes))
            }
        });
        if done == Some(true) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    debug_log!("clipboard not restored: {} formats", formats.len());
}
