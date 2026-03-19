#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

/// Converts a UTF-8 Rust string into a null-terminated UTF-16 buffer for Win32 APIs.
pub fn to_wstring(value: &str) -> Vec<u16> {
    #[cfg(target_os = "windows")]
    {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    #[cfg(not(target_os = "windows"))]
    {
        value.encode_utf16().chain(Some(0)).collect()
    }
}
