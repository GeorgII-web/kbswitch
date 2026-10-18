//! "Start with Windows" = this exe in the current user's Run key; the registry is the truth.

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
    RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{PCWSTR, Result, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("kbswitch");

/// The Run command for this exe: its quoted full path.
fn command() -> std::io::Result<String> {
    Ok(format!("\"{}\"", std::env::current_exe()?.display()))
}

fn registered() -> Option<String> {
    let mut size = 0u32;
    // SAFETY: the first call returns the size in bytes, the second fills a buffer of at least that size.
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut data = vec![0u16; size.div_ceil(2) as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let len = data.iter().position(|&u| u == 0).unwrap_or(data.len());
        Some(String::from_utf16_lossy(&data[..len]))
    }
}

/// True only when the Run value starts this exe, not a copy elsewhere.
pub fn is_enabled() -> bool {
    registered().is_some_and(|cmd| command().is_ok_and(|own| own == cmd))
}

pub fn set(on: bool) -> Result<()> {
    let data: Option<Vec<u8>> = if on {
        Some(
            command()?
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect(),
        )
    } else {
        None
    };
    let mut key = HKEY::default();
    // SAFETY: `key` is opened here and closed below; `data` outlives the call.
    unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), KEY_SET_VALUE, &mut key).ok()?;
        let result = match &data {
            Some(data) => RegSetValueExW(key, VALUE, Some(0), REG_SZ, Some(data)),
            None => RegDeleteValueW(key, VALUE),
        };
        let _ = RegCloseKey(key);
        result.ok()
    }
}
