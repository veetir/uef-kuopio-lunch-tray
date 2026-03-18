use crate::app::{App, FetchApplyOutcome, FetchMessage};
use crate::log::log_line;
use crate::popup;
use crate::restaurant::available_restaurants;
use crate::tray;
use crate::util::to_wstring;
use std::sync::{Mutex, OnceLock};
use time::{OffsetDateTime, Time};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW, GetWindowRect, KillTimer,
    LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, SetTimer,
    SetWindowLongPtrW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, IDYES,
    MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, WM_ACTIVATE, WM_APP, WM_COMMAND, WM_CONTEXTMENU,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER, WNDCLASSEXW,
};

pub const TRAY_WND_CLASS: &str = "CompassLunchTrayWindow";
pub const POPUP_WND_CLASS: &str = "CompassLunchPopupWindow";

pub const WM_TRAY_CALLBACK: u32 = WM_APP + 1;
pub const WM_APP_FETCH_COMPLETE: u32 = WM_APP + 2;
pub const WM_APP_SHOW_EXISTING: u32 = WM_APP + 3;

pub const TIMER_REFRESH: usize = 1;
pub const TIMER_MIDNIGHT: usize = 2;
pub const TIMER_HOVER_CHECK: usize = 3;
pub const TIMER_STALE_CHECK: usize = 4;
pub const TIMER_RETRY_FETCH: usize = 5;
const TRAY_CLOSE_SUPPRESS_OPEN_MS: i64 = 250;

static LAST_POPUP_CLOSE_REQUEST_MS: OnceLock<Mutex<i64>> = OnceLock::new();

pub fn register_window_classes(
    hinstance: windows::Win32::Foundation::HINSTANCE,
) -> anyhow::Result<()> {
    unsafe {
        let tray_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(tray_wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(to_wstring(TRAY_WND_CLASS).as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&tray_class) == 0 {
            return Err(anyhow::anyhow!("RegisterClassExW for tray failed"));
        }

        let popup_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(popup_wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(to_wstring(POPUP_WND_CLASS).as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&popup_class) == 0 {
            return Err(anyhow::anyhow!("RegisterClassExW for popup failed"));
        }
    }
    Ok(())
}

pub unsafe extern "system" fn tray_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            LRESULT(1)
        }
        WM_TRAY_CALLBACK => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let event_raw = lparam.0 as u32;
            let event = event_raw & 0xFFFF;
            if event != WM_MOUSEMOVE {
                log_line(&format!(
                    "tray callback event=0x{:04x} raw=0x{:08x}",
                    event, event_raw
                ));
            }
            match event {
                WM_MOUSEMOVE => {}
                WM_LBUTTONUP => {
                    log_line("tray left click");
                    let popup_hwnd = app.hwnd_popup();
                    if popup_is_visible(popup_hwnd) {
                        note_popup_close_request();
                        app.persist_settings();
                        let state = app.snapshot();
                        popup::begin_close_animation(popup_hwnd, &state);
                    } else if popup_close_requested_recently() {
                        log_line("tray left click ignored: popup close in progress/recent");
                    } else {
                        log_line("tray left click open popup");
                        let state = app.snapshot();
                        if let Some(rect) = tray::tray_icon_rect(hwnd) {
                            popup::show_popup_for_tray_icon(popup_hwnd, &state, rect);
                        } else if let Some(cursor_point) = cursor_point() {
                            popup::show_popup_at(popup_hwnd, &state, cursor_point);
                        } else {
                            popup::show_popup(popup_hwnd, &state);
                        }
                        let _ = SetForegroundWindow(popup_hwnd);
                    }
                }
                WM_RBUTTONUP => {}
                WM_CONTEXTMENU => {
                    log_line("tray context menu");
                    app.persist_settings();
                    let state = app.snapshot();
                    popup::begin_close_animation(app.hwnd_popup(), &state);
                    app.set_context_menu_open(true);
                    tray::show_context_menu(hwnd, &state);
                    app.set_context_menu_open(false);
                }
                WM_MBUTTONUP => {
                    log_line("tray middle click");
                    app.open_current_url();
                }
                WM_MOUSEWHEEL => {}
                _ => {}
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => LRESULT(0),
        WM_APP_SHOW_EXISTING => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let popup_hwnd = app.hwnd_popup();
            if popup_is_visible(popup_hwnd) {
                let _ = SetForegroundWindow(popup_hwnd);
            } else {
                let state = app.snapshot();
                if let Some(rect) = tray::tray_icon_rect(hwnd) {
                    popup::show_popup_for_tray_icon(popup_hwnd, &state, rect);
                } else if let Some(cursor_point) = cursor_point() {
                    popup::show_popup_at(popup_hwnd, &state, cursor_point);
                } else {
                    popup::show_popup(popup_hwnd, &state);
                }
                let _ = SetForegroundWindow(popup_hwnd);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            tray::refresh_tray_icon(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let cmd = (wparam.0 & 0xffff) as u16;
            handle_command(hwnd, app, cmd);
            LRESULT(0)
        }
        WM_TIMER => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            match wparam.0 as usize {
                TIMER_REFRESH => {
                    app.refresh_current_from_timer();
                }
                TIMER_MIDNIGHT => {
                    app.refresh_current_at_midnight();
                    schedule_midnight_timer(hwnd);
                }
                TIMER_HOVER_CHECK => {
                    handle_hover_check(hwnd, app);
                }
                TIMER_STALE_CHECK => {
                    handle_stale_check(hwnd, app);
                }
                TIMER_RETRY_FETCH => {
                    let _ = KillTimer(hwnd, TIMER_RETRY_FETCH);
                    app.start_refresh_retry();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_FETCH_COMPLETE => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let ptr = lparam.0 as *mut FetchMessage;
            if !ptr.is_null() {
                let message = *Box::from_raw(ptr);
                let outcome = app.apply_fetch_message(message);
                match outcome {
                    FetchApplyOutcome::CurrentSuccess => {
                        popup::invalidate_layout_budget_cache();
                        cancel_retry_timer(hwnd);
                        app.prefetch_enabled_restaurants();
                        let state = app.snapshot();
                        if popup_is_visible(app.hwnd_popup()) {
                            popup::resize_popup_keep_position(app.hwnd_popup(), &state);
                        } else {
                            popup::hide_popup(app.hwnd_popup());
                        }
                    }
                    FetchApplyOutcome::CurrentFailure => {
                        let delay = app.current_retry_delay_ms();
                        schedule_retry_timer(hwnd, delay);
                        let state = app.snapshot();
                        if popup_is_visible(app.hwnd_popup()) {
                            popup::resize_popup_keep_position(app.hwnd_popup(), &state);
                        }
                    }
                    FetchApplyOutcome::BackgroundSuccess => {
                        popup::invalidate_layout_budget_cache();
                    }
                    FetchApplyOutcome::BackgroundFailure => {}
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let app = app_from_hwnd(hwnd);
            if !app.is_null() {
                let app_ref = &*(app);
                app_ref.persist_settings();
                tray::remove_tray_icon(hwnd);
                let _ = DestroyWindow(app_ref.hwnd_popup());
                drop(Box::from_raw(app));
            }
            cancel_retry_timer(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub unsafe extern "system" fn popup_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            LRESULT(1)
        }
        WM_PAINT => {
            let app = app_from_hwnd(hwnd);
            if !app.is_null() {
                let app = &*(app);
                let state = app.snapshot();
                popup::paint_popup(hwnd, &state);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DPICHANGED => {
            let app = app_from_hwnd(hwnd);
            if !app.is_null() {
                let app = &*(app);
                let dpi_x = (wparam.0 & 0xFFFF) as u16;
                let dpi_y = ((wparam.0 >> 16) & 0xFFFF) as u16;
                log_line(&format!("popup dpi changed: {}x{}", dpi_x, dpi_y));
                if popup_is_visible(hwnd) {
                    let state = app.snapshot();
                    popup::resize_popup_keep_position(hwnd, &state);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            let app = app_from_hwnd(hwnd);
            if wparam.0 == 0 {
                popup::cancel_text_selection(hwnd);
                note_popup_close_request();
                if !app.is_null() {
                    let app = &*(app);
                    app.persist_settings();
                    let state = app.snapshot();
                    popup::begin_close_animation(hwnd, &state);
                } else {
                    popup::hide_popup(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let x = (lparam.0 as u32 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as i16 as i32;
            let state = app.snapshot();
            if popup::header_button_at(hwnd, &state.settings, x, y).is_none()
                && popup::begin_text_selection(hwnd, x, y)
            {
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let key = wparam.0 as u32;
            match key {
                0x1B => {
                    popup::cancel_text_selection(hwnd);
                    app.persist_settings();
                    let state = app.snapshot();
                    popup::begin_close_animation(hwnd, &state);
                }
                0x25 | 0x41 => {
                    cycle_popup_restaurant(hwnd, app, -1);
                }
                0x27 | 0x44 => {
                    cycle_popup_restaurant(hwnd, app, 1);
                }
                _ => {
                    if let Some(index) = popup_shortcut_index(key) {
                        select_popup_restaurant_index(hwnd, app, index);
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 as u32 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as i16 as i32;
            popup::update_text_selection(hwnd, x, y);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let x = (lparam.0 as u32 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as i16 as i32;
            if popup::text_selection_active(hwnd) {
                let _ = popup::finish_text_selection(hwnd, x, y);
                return LRESULT(0);
            }
            let state = app.snapshot();
            if let Some(action) = popup::header_button_at(hwnd, &state.settings, x, y) {
                match action {
                    popup::HeaderButtonAction::Prev => {
                        cycle_popup_restaurant(hwnd, app, -1);
                    }
                    popup::HeaderButtonAction::Next => {
                        cycle_popup_restaurant(hwnd, app, 1);
                    }
                    popup::HeaderButtonAction::Close => {
                        app.persist_settings();
                        let state = app.snapshot();
                        popup::begin_close_animation(hwnd, &state);
                        return LRESULT(0);
                    }
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            popup::cancel_text_selection(hwnd);
            let app = app_from_hwnd(hwnd);
            if !app.is_null() {
                let app = &*(app);
                app.persist_settings();
                let state = app.snapshot();
                popup::begin_close_animation(hwnd, &state);
                let state = app.snapshot();
                tray::show_context_menu(app.hwnd_tray(), &state);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let app = app_from_hwnd(hwnd);
            if app.is_null() {
                return LRESULT(0);
            }
            let app = &*(app);
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as i32;
            if delta > 0 {
                cycle_popup_restaurant(hwnd, app, -1);
            } else if delta < 0 {
                cycle_popup_restaurant(hwnd, app, 1);
            } else {
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 as usize == popup::POPUP_ANIM_TIMER_ID {
                popup::tick_animation(hwnd);
                return LRESULT(0);
            }
            if wparam.0 as usize == popup::POPUP_HEADER_PRESS_TIMER_ID {
                popup::tick_header_button_press(hwnd);
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn cycle_popup_restaurant(hwnd: HWND, app: &App, direction: i32) {
    let old_state = app.snapshot();
    popup::press_navigation_button(hwnd, direction);
    app.cycle_restaurant(direction);
    let _ = app.load_cache_for_current();
    app.maybe_refresh_on_selection();
    let new_state = app.snapshot();
    popup::resize_popup_keep_position(hwnd, &new_state);
    popup::begin_switch_animation(hwnd, &old_state, &new_state, direction);
}

fn popup_shortcut_index(key: u32) -> Option<usize> {
    match key {
        0x31..=0x39 => Some((key - 0x31) as usize),
        0x30 => Some(9),
        0x61..=0x69 => Some((key - 0x61) as usize),
        0x60 => Some(9),
        _ => None,
    }
}

fn select_popup_restaurant_index(hwnd: HWND, app: &App, index: usize) {
    let old_state = app.snapshot();
    let restaurants = available_restaurants(old_state.settings.enable_antell_restaurants);
    let old_index = restaurants
        .iter()
        .position(|restaurant| restaurant.code == old_state.settings.restaurant_code)
        .unwrap_or(0);

    if !app.set_restaurant_index(index) {
        return;
    }

    let _ = app.load_cache_for_current();
    app.maybe_refresh_on_selection();

    let new_state = app.snapshot();
    let new_index = restaurants
        .iter()
        .position(|restaurant| restaurant.code == new_state.settings.restaurant_code)
        .unwrap_or(index);
    if new_index == old_index {
        return;
    }

    let direction = if new_index > old_index { 1 } else { -1 };
    popup::resize_popup_keep_position(hwnd, &new_state);
    popup::begin_switch_animation(hwnd, &old_state, &new_state, direction);
}

fn handle_command(hwnd: HWND, app: &App, cmd: u16) {
    if let Some(code) = tray::restaurant_code_for_command(cmd) {
        app.set_restaurant(code);
        let _ = app.load_cache_for_current();
        app.maybe_refresh_on_selection();
        if popup_is_visible(app.hwnd_popup()) {
            let state = app.snapshot();
            popup::resize_popup_keep_position(app.hwnd_popup(), &state);
        }
        return;
    }

    match cmd {
        tray::CMD_LANGUAGE_FI => {
            app.set_language("fi");
            let _ = app.load_cache_for_current();
            app.maybe_refresh_on_language_switch();
        }
        tray::CMD_LANGUAGE_EN => {
            app.set_language("en");
            let _ = app.load_cache_for_current();
            app.maybe_refresh_on_language_switch();
        }
        tray::CMD_TOGGLE_SHOW_PRICES => {
            app.toggle_show_prices();
        }
        tray::CMD_TOGGLE_SHOW_ALLERGENS => {
            app.toggle_show_allergens();
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_TOGGLE_HIGHLIGHT_G => {
            app.toggle_highlight_gluten_free();
        }
        tray::CMD_TOGGLE_HIGHLIGHT_VEG => {
            app.toggle_highlight_veg();
        }
        tray::CMD_TOGGLE_HIGHLIGHT_L => {
            app.toggle_highlight_lactose_free();
        }
        tray::CMD_TOGGLE_SHOW_STUDENT_PRICE => {
            app.toggle_show_student_price();
        }
        tray::CMD_TOGGLE_SHOW_STAFF_PRICE => {
            app.toggle_show_staff_price();
        }
        tray::CMD_TOGGLE_SHOW_GUEST_PRICE => {
            app.toggle_show_guest_price();
        }
        tray::CMD_TOGGLE_HIDE_EXPENSIVE_STUDENT => {
            app.toggle_hide_expensive_student_meals();
        }
        tray::CMD_THEME_LIGHT => {
            app.set_theme("light");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_DARK => {
            app.set_theme("dark");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_BLUE => {
            app.set_theme("blue");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_GREEN => {
            app.set_theme("green");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_AMBER => {
            app.set_theme("amber");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_TELETEXT1 => {
            app.set_theme("teletext1");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_THEME_TELETEXT2 => {
            app.set_theme("teletext2");
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_WIDGET_SCALE_NORMAL => {
            app.set_widget_scale("normal");
        }
        tray::CMD_WIDGET_SCALE_125 => {
            app.set_widget_scale("125");
        }
        tray::CMD_WIDGET_SCALE_150 => {
            app.set_widget_scale("150");
        }
        tray::CMD_TOGGLE_ANIMATIONS => {
            app.toggle_animations();
            if popup_is_visible(app.hwnd_popup()) {
                let state = app.snapshot();
                popup::resize_popup_keep_position(app.hwnd_popup(), &state);
            }
        }
        tray::CMD_TOGGLE_STARTUP => {
            let enable = !crate::startup::is_enabled();
            if let Err(err) = crate::startup::set_enabled(enable) {
                log_line(&format!("startup toggle failed: {}", err));
            }
        }
        tray::CMD_TOGGLE_LOGGING => {
            let is_enabled = app.snapshot().settings.enable_logging;
            if !is_enabled && !confirm_enable_logging(hwnd) {
                return;
            }
            app.toggle_logging();
        }
        tray::CMD_OPEN_APPDATA_DIR => {
            app.open_appdata_dir();
        }
        tray::CMD_REFRESH_NOW => {
            app.refresh_current_manually();
        }
        tray::CMD_REFRESH_OFF => {
            app.set_refresh_minutes(0);
            schedule_refresh_timer(hwnd, 0);
        }
        tray::CMD_REFRESH_60 => {
            app.set_refresh_minutes(60);
            schedule_refresh_timer(hwnd, 60);
        }
        tray::CMD_REFRESH_240 => {
            app.set_refresh_minutes(240);
            schedule_refresh_timer(hwnd, 240);
        }
        tray::CMD_REFRESH_1440 => {
            app.set_refresh_minutes(1440);
            schedule_refresh_timer(hwnd, 1440);
        }
        tray::CMD_SUBMIT_FEEDBACK => {
            app.open_feedback_url();
        }
        tray::CMD_QUIT => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
    }
    if popup_is_visible(app.hwnd_popup()) {
        let state = app.snapshot();
        popup::resize_popup_keep_position(app.hwnd_popup(), &state);
    }
}

fn confirm_enable_logging(hwnd: HWND) -> bool {
    let title = to_wstring("Enable logging?");
    let message = to_wstring(
        "Diagnostic logging writes app activity to a file in the App Data folder.\n\
The file can grow over time while logging is enabled.\n\n\
Do you want to enable logging now?",
    );
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        ) == IDYES
    }
}

fn schedule_refresh_timer(hwnd: HWND, minutes: u32) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_REFRESH);
        if minutes > 0 {
            let interval = minutes * 60 * 1000;
            let _ = SetTimer(hwnd, TIMER_REFRESH, interval, None);
        }
    }
}

pub fn schedule_timers(hwnd: HWND, minutes: u32) {
    schedule_refresh_timer(hwnd, minutes);
    schedule_midnight_timer(hwnd);
    schedule_stale_timer(hwnd);
}

fn schedule_midnight_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_MIDNIGHT);
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let date = now.date();
        let next_date = date.next_day().unwrap_or(date);
        let next_midnight = OffsetDateTime::new_in_offset(next_date, Time::MIDNIGHT, now.offset());
        let duration = next_midnight - now;
        let millis = duration.whole_milliseconds().max(1000) as u32;
        let _ = SetTimer(hwnd, TIMER_MIDNIGHT, millis, None);
    }
}

fn schedule_stale_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_STALE_CHECK);
        let interval = 4 * 60 * 60 * 1000;
        let _ = SetTimer(hwnd, TIMER_STALE_CHECK, interval, None);
    }
}

fn schedule_retry_timer(hwnd: HWND, delay_ms: u32) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_RETRY_FETCH);
        let _ = SetTimer(hwnd, TIMER_RETRY_FETCH, delay_ms.max(1000), None);
    }
    log_line(&format!("scheduled retry in {} ms", delay_ms.max(1000)));
}

fn cancel_retry_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_RETRY_FETCH);
    }
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn note_popup_close_request() {
    let store = LAST_POPUP_CLOSE_REQUEST_MS.get_or_init(|| Mutex::new(0));
    if let Ok(mut guard) = store.lock() {
        *guard = now_epoch_ms();
    }
}

fn popup_close_requested_recently() -> bool {
    let store = LAST_POPUP_CLOSE_REQUEST_MS.get_or_init(|| Mutex::new(0));
    let now = now_epoch_ms();
    match store.lock() {
        Ok(guard) => now.saturating_sub(*guard) <= TRAY_CLOSE_SUPPRESS_OPEN_MS,
        Err(_) => false,
    }
}

fn popup_is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

fn app_from_hwnd(hwnd: HWND) -> *mut App {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App }
}

fn start_hover_timer(hwnd: HWND) {
    unsafe {
        let _ = SetTimer(hwnd, TIMER_HOVER_CHECK, 200, None);
    }
}

fn stop_hover_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_HOVER_CHECK);
    }
}

fn handle_hover_check(hwnd: HWND, app: &App) {
    let popup_hwnd = app.hwnd_popup();
    if !popup_is_visible(popup_hwnd) {
        stop_hover_timer(hwnd);
        return;
    }

    let cursor = match cursor_point() {
        Some(pt) => pt,
        None => {
            popup::hide_popup(popup_hwnd);
            stop_hover_timer(hwnd);
            return;
        }
    };

    let mut rect = RECT::default();
    let in_popup = unsafe { GetWindowRect(popup_hwnd, &mut rect).is_ok() }
        && point_in_rect(&rect, cursor.x, cursor.y);

    let in_tray_rect = tray::tray_icon_rect(app.hwnd_tray())
        .map(|rect| point_near_rect(&rect, cursor.x, cursor.y, 12))
        .unwrap_or(false);
    let in_tray_hover = app
        .hover_point()
        .map(|(x, y)| (cursor.x - x).abs() <= 32 && (cursor.y - y).abs() <= 32)
        .unwrap_or(false);
    let in_tray = in_tray_rect || in_tray_hover;

    if !(in_popup || in_tray) {
        popup::hide_popup(popup_hwnd);
        stop_hover_timer(hwnd);
        app.clear_hover_point();
    }
}

fn handle_stale_check(hwnd: HWND, app: &App) {
    app.check_stale_date_and_refresh();
    if popup_is_visible(app.hwnd_popup()) {
        let state = app.snapshot();
        popup::resize_popup_keep_position(app.hwnd_popup(), &state);
    }
    let _ = hwnd;
}

fn cursor_point() -> Option<POINT> {
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_ok() {
        Some(pt)
    } else {
        None
    }
}

fn point_in_rect(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn point_near_rect(rect: &RECT, x: i32, y: i32, padding: i32) -> bool {
    x >= rect.left - padding
        && x <= rect.right + padding
        && y >= rect.top - padding
        && y <= rect.bottom + padding
}
