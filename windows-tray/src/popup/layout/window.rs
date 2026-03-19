//! Window placement, popup sizing, and font creation helpers.

use super::cache::{
    cached_desired_size, desired_size_cache_key, popup_cached_layout_budget,
    update_desired_size_cache,
};
use super::text::{measure_lines_layout, text_metrics, text_width_with_font};
use super::*;

pub(in crate::popup) fn show_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let (x, y) = position_near_point(width, height, cursor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let scale = popup_scale(&state.settings);
        let (x, y) = position_near_tray_rect(width, height, tray_rect, scale.anchor_gap);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn resize_popup_keep_position(hwnd: HWND, state: &AppState) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            show_popup(hwnd, state);
            return;
        }
        let (width, height) = desired_size(hwnd, state);
        let anchor = POINT {
            x: rect.right,
            y: rect.bottom,
        };
        let (x, y) = position_near_point(width, height, anchor);
        if rect.left == x
            && rect.top == y
            && (rect.right - rect.left) == width
            && (rect.bottom - rect.top) == height
        {
            request_repaint(hwnd);
            return;
        }
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn hide_popup(hwnd: HWND) {
    unsafe {
        clear_animation_state(hwnd);
        clear_selection_state(hwnd);
        clear_header_button_press(hwnd);
        let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
        let _ = KillTimer(hwnd, POPUP_HEADER_PRESS_TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

pub(in crate::popup) fn header_layout(width: i32, scale: &PopupScale) -> HeaderLayout {
    let top = (scale.header_height - scale.header_button_size) / 2;
    let prev = RECT {
        left: scale.padding_x,
        top,
        right: scale.padding_x + scale.header_button_size,
        bottom: top + scale.header_button_size,
    };
    let next = RECT {
        left: prev.right + scale.header_button_gap,
        top,
        right: prev.right + scale.header_button_gap + scale.header_button_size,
        bottom: top + scale.header_button_size,
    };
    let close = RECT {
        left: width - scale.padding_x - scale.header_button_size,
        top,
        right: width - scale.padding_x,
        bottom: top + scale.header_button_size,
    };
    HeaderLayout { prev, next, close }
}

pub(in crate::popup) fn header_title(state: &AppState) -> String {
    let list = available_restaurants(state.settings.enable_antell_restaurants);
    if list.is_empty() {
        return "Compass Lunch".to_string();
    }

    let index = list
        .iter()
        .position(|entry| entry.code == state.settings.restaurant_code)
        .unwrap_or(0);
    format!("{} ({}/{})", list[index].name, index + 1, list.len())
}

fn max_header_title_width(hdc: HDC, font: HFONT, settings: &Settings) -> i32 {
    let list = available_restaurants(settings.enable_antell_restaurants);
    if list.is_empty() {
        return text_width_with_font(hdc, font, "Compass Lunch");
    }
    let total = list.len();
    let mut max_width = 0;
    for (idx, restaurant) in list.iter().enumerate() {
        let title = format!("{} ({}/{})", restaurant.name, idx + 1, total);
        max_width = max(max_width, text_width_with_font(hdc, font, &title));
    }
    max_width
}

fn desired_size(hwnd: HWND, state: &AppState) -> (i32, i32) {
    unsafe {
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        if let Some(key) = desired_size_cache_key(state, dpi_y) {
            if let Some(size) = cached_desired_size(&key) {
                windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
                return size;
            }
        }

        let scale = popup_scale(&state.settings);
        let (normal_font, bold_font, small_font, small_bold_font) =
            create_fonts(hdc, &state.settings.theme, scale.factor);
        let current_lines = build_lines(state);
        let current_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &current_lines,
            scale.max_content_width,
        );
        let budget = popup_cached_layout_budget(
            state,
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            dpi_y,
        );
        let target_content_width = budget
            .max_content_width_px
            .unwrap_or(current_metrics.required_content_width)
            .clamp(scale.min_content_width, scale.max_content_width);
        let current_wrapped_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &current_lines,
            target_content_width,
        );
        let mut target_lines = budget
            .max_wrapped_lines
            .unwrap_or(current_wrapped_metrics.wrapped_line_count);
        if budget.max_wrapped_lines.is_some() {
            target_lines = target_lines.max(current_wrapped_metrics.wrapped_line_count);
        }
        target_lines = target_lines.min(MAX_DYNAMIC_LINES);
        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + scale.line_gap;
        let height =
            scale.header_height + (target_lines as i32 * line_height) + scale.padding_y * 2;
        let title_width = max_header_title_width(hdc, bold_font, &state.settings);
        let title_button_margin = scale_px(HEADER_TITLE_BUTTON_MARGIN, scale.factor);
        let header_reserved = scale.padding_x * 2
            + scale.header_button_size * 3
            + scale.header_button_gap
            + title_button_margin * 2;
        let header_required_width = title_width + header_reserved;
        let width_candidate = max(
            target_content_width + scale.padding_x * 2,
            header_required_width,
        );
        let max_width = max(scale.max_width, header_required_width);
        let width = width_candidate.clamp(scale.min_width, max_width);
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        let size = (
            width,
            height.max(scale.header_height + scale_px(120, scale.factor)),
        );
        if let Some(key) = desired_size_cache_key(state, dpi_y) {
            update_desired_size_cache(key, size);
        }
        size
    }
}

pub(in crate::popup) fn create_fonts(
    hdc: HDC,
    theme: &str,
    scale_factor: f32,
) -> (HFONT, HFONT, HFONT, HFONT) {
    unsafe {
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY);
        let height_normal = -MulDiv(scale_px(12, scale_factor).max(8), dpi, 72);
        let height_small = -MulDiv(scale_px(10, scale_factor).max(7), dpi, 72);
        let face = to_wstring(theme_font_family(theme));

        let normal = CreateFontW(
            height_normal,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let bold = CreateFontW(
            height_normal,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let small = CreateFontW(
            height_small,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        let small_bold = CreateFontW(
            height_small,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        (normal, bold, small, small_bold)
    }
}

fn position_near_point(width: i32, height: i32, point: POINT) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut work_area = RECT::default();
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work_area = info.rcWork;
        }

        let mut x = point.x - width;
        let mut y = point.y - height;
        if x < work_area.left {
            x = work_area.left;
        }
        if y < work_area.top {
            y = work_area.top;
        }
        if x + width > work_area.right {
            x = work_area.right - width;
        }
        if y + height > work_area.bottom {
            y = work_area.bottom - height;
        }

        (x, y)
    }
}

fn position_near_tray_rect(
    width: i32,
    height: i32,
    tray_rect: RECT,
    anchor_gap: i32,
) -> (i32, i32) {
    unsafe {
        let center = POINT {
            x: (tray_rect.left + tray_rect.right) / 2,
            y: (tray_rect.top + tray_rect.bottom) / 2,
        };
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut work_area = RECT::default();
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work_area = info.rcWork;
        }

        let mut x = tray_rect.right - width;
        let mut y = tray_rect.top - height - anchor_gap;

        if y < work_area.top {
            y = tray_rect.bottom + anchor_gap;
        }
        if y + height > work_area.bottom {
            y = (tray_rect.top - height - anchor_gap).max(work_area.top);
        }

        if x < work_area.left {
            x = work_area.left;
        }
        if x + width > work_area.right {
            x = work_area.right - width;
        }
        if y < work_area.top {
            y = work_area.top;
        }
        if y + height > work_area.bottom {
            y = work_area.bottom - height;
        }

        (x, y)
    }
}
