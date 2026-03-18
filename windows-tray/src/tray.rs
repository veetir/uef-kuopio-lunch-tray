use crate::app::AppState;
use crate::log::log_line;
use crate::restaurant::available_restaurants;
use crate::util::to_wstring;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, GetCursorPos, LoadIconW, LoadImageW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HICON, HMENU, IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE,
    MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, TPM_LEFTALIGN,
    TPM_RIGHTBUTTON, WM_NULL,
};

pub const CMD_RESTAURANT_0437: u16 = 2001;
pub const CMD_RESTAURANT_0439: u16 = 2002;
pub const CMD_RESTAURANT_0436: u16 = 2003;
pub const CMD_RESTAURANT_SNELLARI_RSS: u16 = 2004;
pub const CMD_RESTAURANT_HUOMEN_BIOTEKNIA: u16 = 2005;
pub const CMD_RESTAURANT_ANTELL_HIGHWAY: u16 = 2006;
pub const CMD_RESTAURANT_ANTELL_ROUND: u16 = 2007;
pub const CMD_RESTAURANT_MEDITEKNIA: u16 = 2008;
pub const CMD_RESTAURANT_PRANZERIA: u16 = 2009;
pub const CMD_RESTAURANT_CAARI: u16 = 2010;
pub const CMD_LANGUAGE_FI: u16 = 2101;
pub const CMD_LANGUAGE_EN: u16 = 2102;
pub const CMD_TOGGLE_SHOW_PRICES: u16 = 2201;
pub const CMD_TOGGLE_SHOW_ALLERGENS: u16 = 2202;
pub const CMD_TOGGLE_HIGHLIGHT_G: u16 = 2203;
pub const CMD_TOGGLE_HIGHLIGHT_VEG: u16 = 2204;
pub const CMD_TOGGLE_HIGHLIGHT_L: u16 = 2205;
pub const CMD_TOGGLE_SHOW_STUDENT_PRICE: u16 = 2206;
pub const CMD_TOGGLE_SHOW_STAFF_PRICE: u16 = 2207;
pub const CMD_TOGGLE_SHOW_GUEST_PRICE: u16 = 2208;
pub const CMD_TOGGLE_HIDE_EXPENSIVE_STUDENT: u16 = 2209;
pub const CMD_THEME_LIGHT: u16 = 2211;
pub const CMD_THEME_DARK: u16 = 2212;
pub const CMD_THEME_BLUE: u16 = 2213;
pub const CMD_THEME_GREEN: u16 = 2214;
pub const CMD_THEME_AMBER: u16 = 2220;
pub const CMD_TOGGLE_STARTUP: u16 = 2215;
pub const CMD_TOGGLE_LOGGING: u16 = 2216;
pub const CMD_OPEN_APPDATA_DIR: u16 = 2217;
pub const CMD_THEME_TELETEXT1: u16 = 2218;
pub const CMD_THEME_TELETEXT2: u16 = 2219;
pub const CMD_WIDGET_SCALE_NORMAL: u16 = 2225;
pub const CMD_WIDGET_SCALE_125: u16 = 2226;
pub const CMD_WIDGET_SCALE_150: u16 = 2227;
pub const CMD_TOGGLE_ANIMATIONS: u16 = 2228;
pub const CMD_REFRESH_NOW: u16 = 2301;
pub const CMD_REFRESH_OFF: u16 = 2400;
pub const CMD_REFRESH_60: u16 = 2401;
pub const CMD_REFRESH_240: u16 = 2402;
pub const CMD_REFRESH_1440: u16 = 2403;
pub const CMD_SUBMIT_FEEDBACK: u16 = 2998;
pub const CMD_QUIT: u16 = 2999;
const TRAY_ICON_ID: u32 = 1;
const TRAY_ICON_RESOURCE_LIGHT: u16 = 1;
const TRAY_ICON_RESOURCE_DARK: u16 = 2;
const PERSONALIZE_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
const SYSTEM_USES_LIGHT_THEME: &str = "SystemUsesLightTheme";

#[derive(Clone, Copy)]
struct TrayIconSet {
    light: HICON,
    dark: HICON,
}

static TRAY_ICONS: OnceLock<TrayIconSet> = OnceLock::new();

pub fn restaurant_command_id(code: &str) -> Option<u16> {
    match code {
        "0437" => Some(CMD_RESTAURANT_0437),
        "snellari-rss" => Some(CMD_RESTAURANT_SNELLARI_RSS),
        "0436" => Some(CMD_RESTAURANT_0436),
        "0439" => Some(CMD_RESTAURANT_0439),
        "huomen-bioteknia" => Some(CMD_RESTAURANT_HUOMEN_BIOTEKNIA),
        "antell-round" => Some(CMD_RESTAURANT_ANTELL_ROUND),
        "antell-highway" => Some(CMD_RESTAURANT_ANTELL_HIGHWAY),
        "043601" => Some(CMD_RESTAURANT_MEDITEKNIA),
        "pranzeria-html" => Some(CMD_RESTAURANT_PRANZERIA),
        "3488" => Some(CMD_RESTAURANT_CAARI),
        _ => None,
    }
}

pub fn restaurant_code_for_command(cmd: u16) -> Option<&'static str> {
    match cmd {
        CMD_RESTAURANT_0437 => Some("0437"),
        CMD_RESTAURANT_SNELLARI_RSS => Some("snellari-rss"),
        CMD_RESTAURANT_0436 => Some("0436"),
        CMD_RESTAURANT_0439 => Some("0439"),
        CMD_RESTAURANT_HUOMEN_BIOTEKNIA => Some("huomen-bioteknia"),
        CMD_RESTAURANT_ANTELL_ROUND => Some("antell-round"),
        CMD_RESTAURANT_ANTELL_HIGHWAY => Some("antell-highway"),
        CMD_RESTAURANT_MEDITEKNIA => Some("043601"),
        CMD_RESTAURANT_PRANZERIA => Some("pranzeria-html"),
        CMD_RESTAURANT_CAARI => Some("3488"),
        _ => None,
    }
}

pub fn add_tray_icon(hwnd: HWND, callback_message: u32) -> anyhow::Result<()> {
    unsafe {
        let icon = select_tray_icon();
        let mut data = NOTIFYICONDATAW::default();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = callback_message;
        data.hIcon = icon;
        let tip = to_wstring("Compass Lunch");
        let mut sz_tip = [0u16; 128];
        for (idx, ch) in tip.iter().enumerate().take(sz_tip.len() - 1) {
            sz_tip[idx] = *ch;
        }
        data.szTip = sz_tip;

        let ok = Shell_NotifyIconW(NIM_ADD, &mut data).as_bool();
        if !ok {
            return Err(anyhow::anyhow!("Shell_NotifyIconW NIM_ADD failed"));
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &mut data);
    }
    Ok(())
}

pub fn refresh_tray_icon(hwnd: HWND) {
    unsafe {
        let mut data = NOTIFYICONDATAW::default();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        data.uFlags = NIF_ICON;
        data.hIcon = select_tray_icon();
        if !Shell_NotifyIconW(NIM_MODIFY, &mut data).as_bool() {
            log_line("tray icon refresh failed");
        }
    }
}

pub fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut data = NOTIFYICONDATAW::default();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        let _ = Shell_NotifyIconW(NIM_DELETE, &mut data);
    }
}

pub fn tray_icon_rect(hwnd: HWND) -> Option<RECT> {
    let mut ident = NOTIFYICONIDENTIFIER::default();
    ident.cbSize = std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32;
    ident.hWnd = hwnd;
    ident.uID = TRAY_ICON_ID;
    unsafe { Shell_NotifyIconGetRect(&ident).ok() }
}

fn select_tray_icon() -> HICON {
    let icons = *TRAY_ICONS.get_or_init(load_tray_icons);
    let system_light = system_uses_light_theme().unwrap_or(false);
    if system_light {
        icons.dark
    } else {
        icons.light
    }
}

fn load_tray_icons() -> TrayIconSet {
    let fallback = unsafe { LoadIconW(None, PCWSTR(32512u16 as *const u16)).unwrap_or_default() };
    let light = load_icon_variant("icon-light.ico", TRAY_ICON_RESOURCE_LIGHT).unwrap_or(fallback);
    let dark = load_icon_variant("icon-dark.ico", TRAY_ICON_RESOURCE_DARK).unwrap_or(light);
    log_line("tray icon set loaded");
    TrayIconSet { light, dark }
}

fn load_icon_variant(file_name: &str, resource_id: u16) -> Option<HICON> {
    if let Some(icon) = load_icon_from_resource(resource_id) {
        return Some(icon);
    }
    if let Some(path) = find_icon_path(file_name) {
        if let Some(icon) = load_icon_from_file(&path) {
            log_line(&format!("using tray icon from file: {}", path.display()));
            return Some(icon);
        }
    }
    None
}

fn load_icon_from_resource(resource_id: u16) -> Option<HICON> {
    let hinstance = unsafe { GetModuleHandleW(None) }.ok()?;
    unsafe {
        let handle = LoadImageW(
            hinstance,
            PCWSTR(resource_id as usize as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )
        .ok()?;
        Some(HICON(handle.0))
    }
}

fn load_icon_from_file(path: &Path) -> Option<HICON> {
    let wide = to_wstring(path.to_string_lossy().as_ref());
    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(wide.as_ptr()),
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        )
        .ok()?;
        Some(HICON(handle.0))
    }
}

fn find_icon_path(file_name: &str) -> Option<PathBuf> {
    let mut buffer = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(None, &mut buffer) } as usize;
    if len == 0 {
        return None;
    }
    let exe = String::from_utf16_lossy(&buffer[..len]);
    let exe_path = PathBuf::from(exe);
    let exe_dir = exe_path.parent()?.to_path_buf();

    let candidates = [
        exe_dir.join("assets").join(file_name),
        exe_dir.join("..").join("assets").join(file_name),
        exe_dir.join("..").join("..").join("assets").join(file_name),
        exe_dir
            .join("..")
            .join("..")
            .join("..")
            .join("assets")
            .join(file_name),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn system_uses_light_theme() -> Option<bool> {
    let key = to_wstring(PERSONALIZE_KEY);
    let value = to_wstring(SYSTEM_USES_LIGHT_THEME);
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
    };
    if ok {
        Some(data != 0)
    } else {
        None
    }
}

pub fn show_context_menu(hwnd: HWND, state: &AppState) {
    unsafe {
        let menu = build_context_menu(state);
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            SetForegroundWindow(hwnd);
            TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                hwnd,
                None,
            );
            let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

fn build_context_menu(state: &AppState) -> HMENU {
    unsafe {
        let menu = CreatePopupMenu().expect("CreatePopupMenu");

        let restaurant_menu = CreatePopupMenu().expect("CreatePopupMenu");
        for restaurant in available_restaurants(state.settings.enable_antell_restaurants) {
            if let Some(cmd) = restaurant_command_id(restaurant.code) {
                append_menu_item(
                    restaurant_menu,
                    cmd,
                    restaurant.name,
                    state.settings.restaurant_code == restaurant.code,
                );
            }
        }
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            restaurant_menu.0 as usize,
            PCWSTR(to_wstring("Default restaurant").as_ptr()),
        );

        let language_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_item(
            language_menu,
            CMD_LANGUAGE_FI,
            "Suomi",
            state.settings.language == "fi",
        );
        append_menu_item(
            language_menu,
            CMD_LANGUAGE_EN,
            "English",
            state.settings.language == "en",
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            language_menu.0 as usize,
            PCWSTR(to_wstring("Language").as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        let theme_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_item(
            theme_menu,
            CMD_THEME_LIGHT,
            "Light",
            state.settings.theme == "light",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_DARK,
            "Dark",
            state.settings.theme == "dark",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_BLUE,
            "Blue",
            state.settings.theme == "blue",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_GREEN,
            "Green",
            state.settings.theme == "green",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_AMBER,
            "Amber",
            state.settings.theme == "amber",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_TELETEXT1,
            "Teletext 1",
            state.settings.theme == "teletext1",
        );
        append_menu_item(
            theme_menu,
            CMD_THEME_TELETEXT2,
            "Teletext 2",
            state.settings.theme == "teletext2",
        );
        let _ = AppendMenuW(theme_menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_menu_toggle(
            theme_menu,
            CMD_TOGGLE_ANIMATIONS,
            "Enable animations",
            state.settings.animations_enabled,
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            theme_menu.0 as usize,
            PCWSTR(to_wstring("Theme").as_ptr()),
        );
        let widget_scale_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_item(
            widget_scale_menu,
            CMD_WIDGET_SCALE_NORMAL,
            "Normal",
            state.settings.widget_scale == "normal",
        );
        append_menu_item(
            widget_scale_menu,
            CMD_WIDGET_SCALE_125,
            "125%",
            state.settings.widget_scale == "125",
        );
        append_menu_item(
            widget_scale_menu,
            CMD_WIDGET_SCALE_150,
            "150%",
            state.settings.widget_scale == "150",
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            widget_scale_menu.0 as usize,
            PCWSTR(to_wstring("Widget size").as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        append_menu_toggle(
            menu,
            CMD_TOGGLE_SHOW_PRICES,
            "Show prices",
            state.settings.show_prices,
        );
        let price_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_toggle(
            price_menu,
            CMD_TOGGLE_SHOW_STUDENT_PRICE,
            "Student",
            state.settings.show_student_price,
        );
        append_menu_toggle(
            price_menu,
            CMD_TOGGLE_SHOW_STAFF_PRICE,
            "Staff",
            state.settings.show_staff_price,
        );
        append_menu_toggle(
            price_menu,
            CMD_TOGGLE_SHOW_GUEST_PRICE,
            "Guest",
            state.settings.show_guest_price,
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            price_menu.0 as usize,
            PCWSTR(to_wstring("Price groups").as_ptr()),
        );
        append_menu_toggle(
            menu,
            CMD_TOGGLE_HIDE_EXPENSIVE_STUDENT,
            "Hide expensive student meals",
            state.settings.hide_expensive_student_meals,
        );
        append_menu_toggle(
            menu,
            CMD_TOGGLE_SHOW_ALLERGENS,
            "Show allergens",
            state.settings.show_allergens,
        );
        let highlight_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_toggle_enabled(
            highlight_menu,
            CMD_TOGGLE_HIGHLIGHT_G,
            "G",
            state.settings.highlight_gluten_free,
            state.settings.show_allergens,
        );
        append_menu_toggle_enabled(
            highlight_menu,
            CMD_TOGGLE_HIGHLIGHT_VEG,
            "Veg",
            state.settings.highlight_veg,
            state.settings.show_allergens,
        );
        append_menu_toggle_enabled(
            highlight_menu,
            CMD_TOGGLE_HIGHLIGHT_L,
            "L",
            state.settings.highlight_lactose_free,
            state.settings.show_allergens,
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            highlight_menu.0 as usize,
            PCWSTR(to_wstring("Highlight allergens").as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        let refresh_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_item(
            refresh_menu,
            CMD_REFRESH_OFF,
            "Off",
            state.settings.refresh_minutes == 0,
        );
        append_menu_item(
            refresh_menu,
            CMD_REFRESH_60,
            "60 minutes",
            state.settings.refresh_minutes == 60,
        );
        append_menu_item(
            refresh_menu,
            CMD_REFRESH_240,
            "240 minutes",
            state.settings.refresh_minutes == 240,
        );
        append_menu_item(
            refresh_menu,
            CMD_REFRESH_1440,
            "1440 minutes",
            state.settings.refresh_minutes == 1440,
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            refresh_menu.0 as usize,
            PCWSTR(to_wstring("Auto refresh").as_ptr()),
        );
        append_menu_item(menu, CMD_REFRESH_NOW, "Refresh now", false);

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        append_menu_toggle(
            menu,
            CMD_TOGGLE_STARTUP,
            "Run at startup",
            crate::startup::is_enabled(),
        );
        let developer_menu = CreatePopupMenu().expect("CreatePopupMenu");
        append_menu_toggle(
            developer_menu,
            CMD_TOGGLE_LOGGING,
            "Enable logging",
            state.settings.enable_logging,
        );
        append_menu_item(
            developer_menu,
            CMD_OPEN_APPDATA_DIR,
            "Open app data folder",
            false,
        );
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            developer_menu.0 as usize,
            PCWSTR(to_wstring("Developer").as_ptr()),
        );

        append_menu_item(menu, CMD_SUBMIT_FEEDBACK, "Submit feedback", false);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_menu_item(menu, CMD_QUIT, "Quit", false);

        menu
    }
}

fn append_menu_item(menu: HMENU, id: u16, label: &str, checked: bool) {
    unsafe {
        let flags = if checked {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(to_wstring(label).as_ptr()));
    }
}

fn append_menu_toggle(menu: HMENU, id: u16, label: &str, enabled: bool) {
    unsafe {
        let flags = if enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(to_wstring(label).as_ptr()));
    }
}

fn append_menu_toggle_enabled(menu: HMENU, id: u16, label: &str, checked: bool, enabled: bool) {
    unsafe {
        let mut flags = MF_STRING;
        if checked {
            flags |= MF_CHECKED;
        }
        if !enabled {
            flags |= MF_DISABLED | MF_GRAYED;
        }
        let _ = AppendMenuW(menu, flags, id as usize, PCWSTR(to_wstring(label).as_ptr()));
    }
}

pub fn disabled_menu_item(menu: HMENU, label: &str) {
    unsafe {
        let _ = AppendMenuW(
            menu,
            MF_STRING | MF_DISABLED | MF_GRAYED,
            0,
            PCWSTR(to_wstring(label).as_ptr()),
        );
    }
}
