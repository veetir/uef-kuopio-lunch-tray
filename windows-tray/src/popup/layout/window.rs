//! Window placement, popup sizing, and font creation helpers.

use super::super::border::theme_shadow_enabled;
use super::cache::{
    cached_desired_size, desired_size_cache_key, popup_cached_layout_budget,
    update_desired_size_cache,
};
use super::text::{measure_lines_layout, text_metrics, text_width_with_font};
use super::*;

/// Turns the popup's drop shadow on or off to match the active theme.
///
/// This is the real DWM window shadow, the same one every ordinary window on the
/// desktop casts, rather than the small fixed `CS_DROPSHADOW` tooltip shadow.
/// DWM shadows any window that carries a sizing frame, so the shadow is toggled
/// by adding and removing `WS_THICKFRAME`. The frame itself never shows:
/// `WM_NCCALCSIZE` hands the whole window rect back to the client area and
/// `WM_NCHITTEST` suppresses the resize edges.
///
/// Deliberately *not* done with `DwmExtendFrameIntoClientArea`: that asks DWM to
/// composite a glass band at the window edge, and GDI drawing leaves zero alpha
/// behind, so the band shows through as a pale rim instead of popup content.
///
/// The shadow is composited by DWM out of process, so it costs the app no
/// bitmap, no extra window, and nothing on the paint path.
fn apply_popup_shadow(hwnd: HWND, theme: &str) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION.0 != 0 {
            // `--no-tray` builds a normal captioned window; leave its frame be.
            return;
        }

        // A sizing frame otherwise earns a 1px system border along every edge,
        // drawn in the accent colour. Windows 11 lets us decline it; on Windows
        // 10 the call fails harmlessly and the line stays.
        let border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const _ as *const _,
            std::mem::size_of_val(&border) as u32,
        );

        // Windows 11 rounds the corners of any window with a sizing frame,
        // which would be badly wrong on the retro themes.
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );

        let wanted = theme_shadow_enabled(theme);
        if wanted == (style & WS_THICKFRAME.0 != 0) {
            return;
        }
        let updated = if wanted {
            style | WS_THICKFRAME.0
        } else {
            style & !WS_THICKFRAME.0
        };
        SetWindowLongPtrW(hwnd, GWL_STYLE, updated as isize);
        let _ = SetWindowPos(
            hwnd,
            HWND(0),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

pub(in crate::popup) fn show_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        apply_popup_shadow(hwnd, &state.settings.theme);
        let (width, height) = desired_size(hwnd, state);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let (width, height) = constrain_size_to_work_area_near_point(width, height, cursor);
        let (x, y) = position_near_point(width, height, cursor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        apply_popup_shadow(hwnd, &state.settings.theme);
        let (width, height) = desired_size(hwnd, state);
        let (width, height) = constrain_size_to_work_area_near_point(width, height, anchor);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(in crate::popup) fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        apply_popup_shadow(hwnd, &state.settings.theme);
        let (width, height) = desired_size(hwnd, state);
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
        let scale = popup_scale_for_dpi(&state.settings, dpi_y);
        let (width, height) = constrain_size_to_work_area_near_tray_rect(width, height, tray_rect);
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
        let (width, height) = constrain_size_to_work_area_near_point(width, height, anchor);
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

pub(in crate::popup) fn header_marker_rects(
    width: i32,
    settings: &Settings,
    scale: &PopupScale,
) -> Vec<RECT> {
    if settings.show_restaurant_index_numbers {
        return Vec::new();
    }
    let count = available_restaurants(settings.enable_antell_restaurants).len();
    if count <= 1 {
        return Vec::new();
    }

    let dot = scale_px(HEADER_MARKER_DOT_SIZE, scale.factor).max(3);
    let gap = scale_px(HEADER_MARKER_GAP, scale.factor).max(4);
    let hit = scale_px(HEADER_MARKER_HIT_SIZE, scale.factor).max(dot + 6);
    let rail_width = count as i32 * dot + (count.saturating_sub(1) as i32 * gap);
    let start_x = (width - rail_width) / 2;
    let dot_top = scale.header_height - scale_px(HEADER_MARKER_BOTTOM_GAP, scale.factor).max(7);
    let center_y = dot_top + dot / 2;

    (0..count)
        .map(|idx| {
            let left = start_x + idx as i32 * (dot + gap);
            let center_x = left + dot / 2;
            RECT {
                left: center_x - hit / 2,
                top: center_y - hit / 2,
                right: center_x + hit / 2,
                bottom: center_y + hit / 2,
            }
        })
        .collect()
}

/// Drawn sizes of the rail markers: inactive first, then active.
pub(in crate::popup) fn header_marker_sizes(scale: &PopupScale) -> (i32, i32) {
    let inactive = scale_px(HEADER_MARKER_DOT_SIZE, scale.factor).max(3);
    let active = (inactive * 8 / 5).max(inactive + 2);
    (inactive, active)
}

/// Top edge of the marker rail, if one is drawn.
///
/// The title has to clear this. `None` when there is no rail at all, which is
/// the single-restaurant and index-number cases.
pub(in crate::popup) fn header_rail_top(
    width: i32,
    settings: &Settings,
    scale: &PopupScale,
) -> Option<i32> {
    let rects = header_marker_rects(width, settings, scale);
    let first = rects.first()?;
    let (_, active) = header_marker_sizes(scale);
    Some((first.top + first.bottom) / 2 - active / 2)
}

/// Vertical position of the header title.
///
/// Aligned to the centre of the nav buttons, because they are the heaviest
/// things in the header and the eye reads their centre as the bar's midline —
/// a title floating off that line looks wrong even when it is centred on
/// something defensible. It only lifts off that line when the marker rail would
/// otherwise crowd it, and then only by as much as it must.
pub(in crate::popup) fn header_title_y(
    button_center_y: i32,
    text_height: i32,
    rail_top: Option<i32>,
    scale: &PopupScale,
) -> i32 {
    let aligned = button_center_y - text_height / 2;
    let min_gap = scale_px(4, scale.factor).max(3);
    let top_margin = scale_px(2, scale.factor).max(1);
    match rail_top {
        Some(rail_top) if aligned + text_height + min_gap > rail_top => {
            (rail_top - min_gap - text_height).max(top_margin)
        }
        _ => aligned.max(top_margin),
    }
}

pub(in crate::popup) fn header_title(state: &AppState) -> String {
    let list = available_restaurants(state.settings.enable_antell_restaurants);
    if list.is_empty() {
        return "LunchTray".to_string();
    }

    let index = list
        .iter()
        .position(|entry| entry.code == state.settings.restaurant_code)
        .unwrap_or(0);
    if state.settings.show_restaurant_index_numbers {
        format!("{} ({}/{})", list[index].name, index + 1, list.len())
    } else {
        list[index].name.to_string()
    }
}

fn max_header_title_width(hdc: HDC, font: HFONT, settings: &Settings) -> i32 {
    let list = available_restaurants(settings.enable_antell_restaurants);
    if list.is_empty() {
        return text_width_with_font(hdc, font, "LunchTray");
    }
    let mut max_width = 0;
    for (idx, restaurant) in list.iter().enumerate() {
        let title = if settings.show_restaurant_index_numbers {
            format!("{} ({}/{})", restaurant.name, idx + 1, list.len())
        } else {
            restaurant.name.to_string()
        };
        max_width = max(max_width, text_width_with_font(hdc, font, &title));
    }
    max_width
}

fn desired_size(hwnd: HWND, state: &AppState) -> (i32, i32) {
    unsafe {
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        let expanded_recipe_key = super::super::interaction::expanded_recipe_key();
        if let Some(key) = desired_size_cache_key(state, dpi_y, expanded_recipe_key) {
            if let Some(size) = cached_desired_size(&key) {
                windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
                return size;
            }
        }

        let scale = popup_scale_for_dpi(&state.settings, dpi_y);
        let (normal_font, bold_font, _bold_italic_font, small_font, small_bold_font) =
            create_fonts(hdc, &state.settings.theme, scale.factor);
        let bullet_width = bullet_column_width(
            hdc,
            normal_font,
            bullet_style_for_theme(&state.settings.theme),
        );
        let current_lines = build_lines(state);
        let current_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bullet_width,
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
            bullet_width,
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
            bullet_width,
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
        let target_extra_height = budget
            .max_extra_height_px
            .unwrap_or(current_wrapped_metrics.extra_height_px)
            .max(current_wrapped_metrics.extra_height_px);
        let height = scale.header_height
            + (target_lines as i32 * line_height)
            + target_extra_height
            + scale.padding_y * 2;
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
        // The fonts are shared and outlive this call; see `create_fonts`.
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        let size = (
            width,
            height.max(scale.header_height + scale_px(120, scale.factor)),
        );
        if let Some(key) = desired_size_cache_key(state, dpi_y, expanded_recipe_key) {
            update_desired_size_cache(key, size);
        }
        size
    }
}

/// The popup's five fonts, kept for the life of the process.
///
/// Fonts used to be created and destroyed on every paint and every size
/// calculation, which is ten GDI calls a frame for handles that almost never
/// change. More importantly it made font handles meaningless as cache keys,
/// because a new paint produced new handles for identical fonts.
///
/// **These handles are deliberately never deleted.** That is what makes them
/// stable identifiers, which `row_boundaries` relies on to memoise per-character
/// selection geometry. Deleting them would both undo the saving and silently
/// corrupt that cache by letting GDI reissue a handle value for a different font.
///
/// Because they are never freed, every set built has to be *kept and reused*. An
/// earlier version held a single set and replaced it on each theme change, which
/// orphaned five handles every time the font family changed and grew without
/// bound if the user alternated between two themes.
struct CachedFonts {
    height_normal: i32,
    height_small: i32,
    face: String,
    fonts: (HFONT, HFONT, HFONT, HFONT, HFONT),
}

/// Distinct font sets worth keeping. One per (size, face), so the ceiling is the
/// few font families the themes use times the widget scales and DPIs a session
/// visits. Past it, fonts are created uncached rather than evicted: a handle that
/// might be reissued cannot be a cache key, and both the text-width and row
/// geometry caches key on these.
const POPUP_FONT_CACHE_LIMIT: usize = 48;

thread_local! {
    static POPUP_FONTS: std::cell::RefCell<Vec<CachedFonts>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub(in crate::popup) fn create_fonts(
    hdc: HDC,
    theme: &str,
    scale_factor: f32,
) -> (HFONT, HFONT, HFONT, HFONT, HFONT) {
    let height_normal = -MulDiv(scale_px(12, scale_factor).max(8), BASE_DPI, 72);
    let height_small = -MulDiv(scale_px(10, scale_factor).max(7), BASE_DPI, 72);
    let face = theme_font_family(theme);

    let cached = POPUP_FONTS.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|entry| {
                entry.height_normal == height_normal
                    && entry.height_small == height_small
                    && entry.face == face
            })
            .map(|entry| entry.fonts)
    });
    if let Some(fonts) = cached {
        return fonts;
    }

    let fonts = build_fonts(hdc, height_normal, height_small, face);
    POPUP_FONTS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() < POPUP_FONT_CACHE_LIMIT {
            cache.push(CachedFonts {
                height_normal,
                height_small,
                face: face.to_string(),
                fonts,
            });
        }
    });
    fonts
}

fn build_fonts(
    _hdc: HDC,
    height_normal: i32,
    height_small: i32,
    face: &str,
) -> (HFONT, HFONT, HFONT, HFONT, HFONT) {
    unsafe {
        let face = to_wstring(face);

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
        let bold_italic = CreateFontW(
            height_normal,
            0,
            0,
            0,
            700,
            1,
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
        (normal, bold, bold_italic, small, small_bold)
    }
}

fn position_near_point(width: i32, height: i32, point: POINT) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let work_area = work_area_for_monitor(monitor);
        let work_width = work_area.right - work_area.left;
        let work_height = work_area.bottom - work_area.top;

        let mut x = point.x - width;
        let mut y = point.y - height;
        if width >= work_width {
            x = work_area.left;
        } else if x < work_area.left {
            x = work_area.left;
        } else if x + width > work_area.right {
            x = work_area.right - width;
        }
        if height >= work_height {
            y = work_area.top;
        } else if y < work_area.top {
            y = work_area.top;
        } else if y + height > work_area.bottom {
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
        let work_area = work_area_for_monitor(monitor);
        let work_width = work_area.right - work_area.left;
        let work_height = work_area.bottom - work_area.top;

        let mut x = tray_rect.right - width;
        let mut y = tray_rect.top - height - anchor_gap;

        if height >= work_height {
            y = work_area.top;
        } else if y < work_area.top {
            y = tray_rect.bottom + anchor_gap;
        }
        if height < work_height && y + height > work_area.bottom {
            y = (tray_rect.top - height - anchor_gap).max(work_area.top);
        }

        if width >= work_width {
            x = work_area.left;
        } else if x < work_area.left {
            x = work_area.left;
        } else if x + width > work_area.right {
            x = work_area.right - width;
        }
        if height >= work_height {
            y = work_area.top;
        } else if y < work_area.top {
            y = work_area.top;
        } else if y + height > work_area.bottom {
            y = work_area.bottom - height;
        }

        (x, y)
    }
}

fn constrain_size_to_work_area_near_point(width: i32, height: i32, point: POINT) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        constrain_size_to_work_area(width, height, work_area_for_monitor(monitor))
    }
}

fn constrain_size_to_work_area_near_tray_rect(
    width: i32,
    height: i32,
    tray_rect: RECT,
) -> (i32, i32) {
    unsafe {
        let center = POINT {
            x: (tray_rect.left + tray_rect.right) / 2,
            y: (tray_rect.top + tray_rect.bottom) / 2,
        };
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        constrain_size_to_work_area(width, height, work_area_for_monitor(monitor))
    }
}

fn constrain_size_to_work_area(width: i32, height: i32, work_area: RECT) -> (i32, i32) {
    let max_width = (work_area.right - work_area.left).max(1);
    let max_height = (work_area.bottom - work_area.top).max(1);
    (width.min(max_width), height.min(max_height))
}

unsafe fn work_area_for_monitor(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> RECT {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        info.rcWork
    } else {
        RECT::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale() -> PopupScale {
        popup_scale_for_dpi(&Settings::default(), BASE_DPI)
    }

    #[test]
    fn marker_active_size_exceeds_inactive() {
        let (inactive, active) = header_marker_sizes(&scale());

        assert!(
            active > inactive,
            "the active marker must carry more weight"
        );
    }

    #[test]
    fn title_sits_on_the_button_midline_when_it_fits() {
        let scale = scale();
        let text_height = 24;
        let button_center = 27;

        let y = header_title_y(button_center, text_height, Some(200), &scale);

        assert_eq!(y, button_center - text_height / 2);
    }

    /// The title used to centre on the whole header and ended up almost
    /// touching the rail.
    #[test]
    fn title_lifts_off_the_midline_only_to_clear_the_rail() {
        let scale = scale();
        let text_height = 24;
        let button_center = 27;
        let rail_top = 36;

        let y = header_title_y(button_center, text_height, Some(rail_top), &scale);

        assert!(y < button_center - text_height / 2, "should lift");
        assert!(y + text_height < rail_top, "should clear the rail");
    }

    #[test]
    fn title_stays_on_the_midline_without_a_rail() {
        let scale = scale();
        let text_height = 24;
        let button_center = 27;

        assert_eq!(
            header_title_y(button_center, text_height, None, &scale),
            button_center - text_height / 2
        );
    }

    #[test]
    fn title_never_leaves_the_header() {
        let scale = scale();

        assert!(header_title_y(10, 40, Some(12), &scale) >= 1);
    }
}
