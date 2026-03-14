use crate::util::to_wstring;
use windows::core::PCWSTR;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPEN_CREATE_OPTIONS,
    REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "CompassLunch";

pub fn is_enabled() -> bool {
    let subkey = to_wstring(RUN_KEY);
    let value = to_wstring(VALUE_NAME);
    let mut size: u32 = 0;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .is_ok()
            && size > 0
    }
}

pub fn set_enabled(enable: bool) -> anyhow::Result<()> {
    if enable {
        let path = exe_path().ok_or_else(|| anyhow::anyhow!("exe path not found"))?;
        set_run_value(&path)?;
    } else {
        remove_run_value()?;
    }
    Ok(())
}

fn set_run_value(path: &str) -> anyhow::Result<()> {
    let subkey = to_wstring(RUN_KEY);
    let value = to_wstring(VALUE_NAME);
    let path_wide = to_wstring(path);
    let data =
        unsafe { std::slice::from_raw_parts(path_wide.as_ptr() as *const u8, path_wide.len() * 2) };

    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPEN_CREATE_OPTIONS(REG_OPTION_NON_VOLATILE.0),
            REG_SAM_FLAGS(KEY_SET_VALUE.0),
            None,
            &mut key,
            None,
        )?;
        let result = RegSetValueExW(key, PCWSTR(value.as_ptr()), 0, REG_SZ, Some(data));
        let _ = RegCloseKey(key);
        result?;
    }
    Ok(())
}

fn remove_run_value() -> anyhow::Result<()> {
    let subkey = to_wstring(RUN_KEY);
    let value = to_wstring(VALUE_NAME);
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            REG_SAM_FLAGS(KEY_SET_VALUE.0 | KEY_QUERY_VALUE.0),
            &mut key,
        )
        .is_ok()
        {
            let _ = RegDeleteValueW(key, PCWSTR(value.as_ptr()));
            let _ = RegCloseKey(key);
        }
    }
    Ok(())
}

fn exe_path() -> Option<String> {
    let mut buffer = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(None, &mut buffer) } as usize;
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len]))
}
