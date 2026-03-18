use super::animation::{begin_open_animation, clear_animation_state, clear_header_button_press};
use super::content::build_lines;
use super::interaction::clear_selection_state;
use super::theme::theme_font_family;
use super::*;

pub(super) fn scale_px(base: i32, factor: f32) -> i32 {
    ((base as f32) * factor).round() as i32
}

fn widget_scale_factor(value: &str) -> f32 {
    match value {
        "125" => 1.25,
        "150" => 1.50,
        _ => 1.0,
    }
}

pub(super) fn popup_scale(settings: &Settings) -> PopupScale {
    let factor = widget_scale_factor(&settings.widget_scale);
    let padding_x = scale_px(PADDING_X, factor).max(8);
    let padding_y = scale_px(PADDING_Y, factor).max(6);
    let min_width = scale_px(POPUP_MIN_WIDTH, factor).max(220);
    let max_width = scale_px(POPUP_MAX_WIDTH, factor).max(min_width);
    let max_content_width = (max_width - padding_x * 2).max(40);
    let min_content_width = (min_width - padding_x * 2).max(40);

    PopupScale {
        factor,
        padding_x,
        padding_y,
        line_gap: scale_px(LINE_GAP, factor).max(1),
        anchor_gap: scale_px(ANCHOR_GAP, factor).max(0),
        max_width,
        min_width,
        max_content_width,
        min_content_width,
        header_height: scale_px(HEADER_HEIGHT, factor).max(30),
        header_button_size: scale_px(HEADER_BUTTON_SIZE, factor).max(18),
        header_button_gap: scale_px(HEADER_BUTTON_GAP, factor).max(4),
        switch_offset_px: scale_px(POPUP_SWITCH_OFFSET_PX, factor).max(2),
    }
}

pub(super) fn show_popup(hwnd: HWND, state: &AppState) {
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

pub(super) fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(super) fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let scale = popup_scale(&state.settings);
        let (x, y) = position_near_tray_rect(width, height, tray_rect, scale.anchor_gap);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        request_repaint(hwnd);
    }
}

pub(super) fn resize_popup_keep_position(hwnd: HWND, state: &AppState) {
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

pub(super) fn invalidate_layout_budget_cache() {
    let budget_cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = budget_cache.lock() {
        *guard = None;
    }

    let signature_cache = POPUP_LINE_SIGNATURE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = signature_cache.lock() {
        *guard = None;
    }

    let desired_size_cache = POPUP_DESIRED_SIZE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = desired_size_cache.lock() {
        guard.clear();
    }
}

pub(super) fn hide_popup(hwnd: HWND) {
    unsafe {
        clear_animation_state(hwnd);
        clear_selection_state(hwnd);
        clear_header_button_press(hwnd);
        let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
        let _ = KillTimer(hwnd, POPUP_HEADER_PRESS_TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

fn measure_lines_layout(
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    lines: &[Line],
    wrap_content_width: i32,
) -> LineLayoutMetrics {
    let wrap_width = wrap_content_width.max(40);
    let bullet_width = text_width_with_font(hdc, normal_font, BULLET_PREFIX);
    let main_wrap_width = (wrap_width - bullet_width).max(24);
    let mut required_content_width = 0;
    let mut wrapped_line_count = 0usize;

    for line in lines {
        match line {
            Line::Heading(text) => {
                let width = text_width_with_font(hdc, bold_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, bold_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::Text(text) => {
                let width = text_width_with_font(hdc, normal_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, normal_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::MenuItem {
                main,
                suffix_segments,
            } => {
                let styled_width = text_with_suffix_width(
                    hdc,
                    normal_font,
                    small_font,
                    small_bold_font,
                    main,
                    suffix_segments,
                    bullet_width,
                );
                required_content_width = required_content_width.max(styled_width);
                if styled_width <= wrap_width {
                    wrapped_line_count += 1;
                } else {
                    let main_rows =
                        wrapped_line_count_for_text(hdc, normal_font, main, main_wrap_width).max(1);
                    wrapped_line_count += main_rows;
                    if !suffix_segments.is_empty() {
                        let suffix_plain = flatten_suffix_segments(suffix_segments);
                        if !suffix_plain.is_empty() {
                            let suffix_rows = wrapped_line_count_for_text(
                                hdc,
                                small_font,
                                &suffix_plain,
                                main_wrap_width,
                            )
                            .max(1);
                            wrapped_line_count += suffix_rows;
                        }
                    }
                }
            }
            Line::Spacer => {
                wrapped_line_count += 1;
            }
        }
    }

    LineLayoutMetrics {
        required_content_width,
        wrapped_line_count,
    }
}

fn wrapped_line_count_for_text(hdc: HDC, font: HFONT, text: &str, max_width: i32) -> usize {
    let wrapped = wrap_text_to_width_with_font(hdc, font, text, max_width);
    wrapped.len()
}

pub(super) fn wrap_text_to_width_with_font_rows(
    hdc: HDC,
    font: HFONT,
    text: &str,
    max_width: i32,
) -> Vec<WrappedRow> {
    unsafe {
        let old = SelectObject(hdc, font);
        let wrapped = wrap_text_to_width_rows(hdc, text, max_width);
        SelectObject(hdc, old);
        wrapped
    }
}

pub(super) fn wrap_text_to_width_with_font(
    hdc: HDC,
    font: HFONT,
    text: &str,
    max_width: i32,
) -> Vec<String> {
    wrap_text_to_width_with_font_rows(hdc, font, text, max_width)
        .into_iter()
        .map(|row| row.text)
        .collect()
}

pub(super) fn wrap_text_to_width(hdc: HDC, text: &str, max_width: i32) -> Vec<String> {
    wrap_text_to_width_rows(hdc, text, max_width)
        .into_iter()
        .map(|row| row.text)
        .collect()
}

fn wrap_text_to_width_rows(hdc: HDC, text: &str, max_width: i32) -> Vec<WrappedRow> {
    let clean = normalize_text(text);
    if clean.is_empty() {
        return Vec::new();
    }
    let limit = max_width.max(16);
    if text_width(hdc, &clean) <= limit {
        return vec![WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        }];
    }

    let words = word_bounds(&clean);
    if words.is_empty() {
        return vec![WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        }];
    }

    let mut rows: Vec<WrappedRow> = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_end: usize = 0;
    for word in words {
        let candidate_range = match current_start {
            Some(start) => start..word.end,
            None => word.start..word.end,
        };
        let candidate_text = &clean[candidate_range.clone()];
        if text_width(hdc, candidate_text) <= limit {
            if current_start.is_none() {
                current_start = Some(word.start);
            }
            current_end = word.end;
            continue;
        }

        if let Some(start) = current_start {
            let text = clean[start..current_end].to_string();
            rows.push(WrappedRow {
                text,
                start,
                end: current_end,
            });
            current_start = None;
            current_end = 0;
        }

        let single_word = &clean[word.start..word.end];
        if text_width(hdc, single_word) <= limit {
            current_start = Some(word.start);
            current_end = word.end;
        } else {
            rows.extend(split_long_token_to_width_rows(
                hdc, &clean, word.start, word.end, limit,
            ));
        }
    }

    if let Some(start) = current_start {
        rows.push(WrappedRow {
            text: clean[start..current_end].to_string(),
            start,
            end: current_end,
        });
    }

    if rows.is_empty() {
        rows.push(WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        });
    }
    rows
}

#[derive(Debug, Clone, Copy)]
struct WordBounds {
    start: usize,
    end: usize,
}

fn word_bounds(text: &str) -> Vec<WordBounds> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                out.push(WordBounds { start: s, end: idx });
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(s) = start {
        out.push(WordBounds {
            start: s,
            end: text.len(),
        });
    }
    out
}

fn split_long_token_to_width_rows(
    hdc: HDC,
    full_text: &str,
    start: usize,
    end: usize,
    max_width: i32,
) -> Vec<WrappedRow> {
    let mut rows = Vec::new();
    let token = &full_text[start..end];
    let mut current = String::new();
    let mut current_start = start;
    let mut current_end = start;

    for (offset, ch) in token.char_indices() {
        let ch_len = ch.len_utf8();
        let mut candidate = current.clone();
        candidate.push(ch);
        if !current.is_empty() && text_width(hdc, &candidate) > max_width {
            rows.push(WrappedRow {
                text: current.clone(),
                start: current_start,
                end: current_end,
            });
            current.clear();
            current_start = start + offset;
        }
        current.push(ch);
        current_end = start + offset + ch_len;
    }
    if !current.is_empty() {
        rows.push(WrappedRow {
            text: current,
            start: current_start,
            end: current_end,
        });
    }
    if rows.is_empty() {
        rows.push(WrappedRow {
            text: token.to_string(),
            start,
            end,
        });
    }
    rows
}

pub(super) fn text_width_with_font(hdc: HDC, font: HFONT, text: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let width = text_width(hdc, text);
        SelectObject(hdc, old);
        width
    }
}

pub(super) fn text_with_suffix_width(
    hdc: HDC,
    normal_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    main: &str,
    segments: &[(String, bool)],
    bullet_width: i32,
) -> i32 {
    let main_width = text_width_with_font(hdc, normal_font, main);
    if segments.is_empty() {
        return bullet_width + main_width;
    }

    let mut suffix_width = 0;
    for (segment, bold) in segments {
        let font = if *bold { small_bold_font } else { small_font };
        suffix_width += text_width_with_font(hdc, font, segment);
    }
    bullet_width + main_width + suffix_width + 4
}

pub(super) fn flatten_suffix_segments(segments: &[(String, bool)]) -> String {
    let mut out = String::new();
    for (segment, _) in segments {
        out.push_str(segment);
    }
    normalize_text(&out)
}

pub(super) fn header_layout(width: i32, scale: &PopupScale) -> HeaderLayout {
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

pub(super) fn header_title(state: &AppState) -> String {
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

pub(super) fn text_metrics(hdc: HDC, font: HFONT) -> TEXTMETRICW {
    unsafe {
        let old = SelectObject(hdc, font);
        let mut metrics = TEXTMETRICW::default();
        GetTextMetricsW(hdc, &mut metrics);
        SelectObject(hdc, old);
        metrics
    }
}

pub(super) fn text_width(hdc: HDC, text: &str) -> i32 {
    let wide = to_wstring(text);
    unsafe {
        let mut size = windows::Win32::Foundation::SIZE::default();
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = GetTextExtentPoint32W(hdc, slice, &mut size);
        }
        size.cx
    }
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

pub(super) fn create_fonts(
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

#[derive(Debug, Clone, Copy)]
struct CachedLayoutBudget {
    max_wrapped_lines: Option<usize>,
    max_content_width_px: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
struct LineLayoutMetrics {
    required_content_width: i32,
    wrapped_line_count: usize,
}

fn popup_cached_layout_budget(
    state: &AppState,
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    dpi_y: i32,
) -> CachedLayoutBudget {
    let today_key = local_today_key();
    let key = line_budget_key(&state.settings, &today_key, dpi_y);
    let signatures = cache_signatures(&state.settings, &key);
    if let Some(budget) = cached_line_budget(&key, &signatures) {
        return budget;
    }

    let budget = max_today_cached_layout_budget(
        state,
        &today_key,
        hdc,
        normal_font,
        bold_font,
        small_font,
        small_bold_font,
    );
    update_line_budget_cache(key, signatures, budget);
    budget
}

fn line_budget_key(settings: &Settings, today_key: &str, dpi_y: i32) -> PopupLineBudgetKey {
    PopupLineBudgetKey {
        today_key: today_key.to_string(),
        language: settings.language.clone(),
        theme: settings.theme.clone(),
        widget_scale: settings.widget_scale.clone(),
        dpi_y,
        enable_antell_restaurants: settings.enable_antell_restaurants,
        show_prices: settings.show_prices,
        show_student_price: settings.show_student_price,
        show_staff_price: settings.show_staff_price,
        show_guest_price: settings.show_guest_price,
        hide_expensive_student_meals: settings.hide_expensive_student_meals,
        show_allergens: settings.show_allergens,
        highlight_gluten_free: settings.highlight_gluten_free,
        highlight_veg: settings.highlight_veg,
        highlight_lactose_free: settings.highlight_lactose_free,
    }
}

fn cache_signatures(
    settings: &Settings,
    key: &PopupLineBudgetKey,
) -> Vec<RestaurantCacheSignature> {
    let cache = POPUP_LINE_SIGNATURE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.as_ref() {
            if entry.key == *key {
                return entry.signatures.clone();
            }
        }
    }

    let mut signatures = Vec::new();
    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let mtime_ms =
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
                .unwrap_or(-1);
        signatures.push(RestaurantCacheSignature {
            code: restaurant.code.to_string(),
            mtime_ms,
        });
    }

    if let Ok(mut guard) = cache.lock() {
        *guard = Some(PopupLineSignatureCache {
            key: key.clone(),
            signatures: signatures.clone(),
        });
    }

    signatures
}

fn cached_line_budget(
    key: &PopupLineBudgetKey,
    signatures: &[RestaurantCacheSignature],
) -> Option<CachedLayoutBudget> {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.key == *key && entry.signatures == signatures {
        Some(CachedLayoutBudget {
            max_wrapped_lines: entry.max_wrapped_lines,
            max_content_width_px: entry.max_content_width_px,
        })
    } else {
        None
    }
}

fn update_line_budget_cache(
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
    budget: CachedLayoutBudget,
) {
    let cache = POPUP_LINE_BUDGET_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(PopupLineBudgetCache {
            key,
            signatures,
            max_wrapped_lines: budget.max_wrapped_lines,
            max_content_width_px: budget.max_content_width_px,
        });
    }
}

fn desired_size_cache_key(state: &AppState, dpi_y: i32) -> Option<PopupDesiredSizeKey> {
    if state.status == FetchStatus::Loading {
        return None;
    }

    Some(PopupDesiredSizeKey {
        today_key: local_today_key(),
        enable_antell_restaurants: state.settings.enable_antell_restaurants,
        language: state.settings.language.clone(),
        theme: state.settings.theme.clone(),
        widget_scale: state.settings.widget_scale.clone(),
        dpi_y,
        show_prices: state.settings.show_prices,
        show_student_price: state.settings.show_student_price,
        show_staff_price: state.settings.show_staff_price,
        show_guest_price: state.settings.show_guest_price,
        hide_expensive_student_meals: state.settings.hide_expensive_student_meals,
        show_allergens: state.settings.show_allergens,
        highlight_gluten_free: state.settings.highlight_gluten_free,
        highlight_veg: state.settings.highlight_veg,
        highlight_lactose_free: state.settings.highlight_lactose_free,
    })
}

fn cached_desired_size(key: &PopupDesiredSizeKey) -> Option<(i32, i32)> {
    let cache = POPUP_DESIRED_SIZE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = cache.lock().ok()?;
    let index = guard.iter().position(|entry| entry.key == *key)?;
    let entry = guard.remove(index);
    let size = (entry.width, entry.height);
    guard.push(entry);
    Some(size)
}

fn update_desired_size_cache(key: PopupDesiredSizeKey, size: (i32, i32)) {
    let cache = POPUP_DESIRED_SIZE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut guard) = cache.lock() {
        if let Some(index) = guard.iter().position(|entry| entry.key == key) {
            guard.remove(index);
        }
        guard.push(PopupDesiredSizeCacheEntry {
            key,
            width: size.0,
            height: size.1,
        });
        while guard.len() > POPUP_DESIRED_SIZE_CACHE_LIMIT {
            guard.remove(0);
        }
    }
}

fn max_today_cached_layout_budget(
    state: &AppState,
    today_key: &str,
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
) -> CachedLayoutBudget {
    let settings = &state.settings;
    let scale = popup_scale(settings);
    let mut max_wrapped_lines: Option<usize> = None;
    let mut max_content_width_px: Option<i32> = None;

    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let parsed = if is_hard_closed_today(restaurant) {
            api::closed_today_fetch_output(restaurant, &settings.language)
        } else {
            let raw =
                match cache::read_cache(restaurant.provider, restaurant.code, &settings.language) {
                    Some(payload) => payload,
                    None => continue,
                };

            match api::parse_cached_payload(
                &raw,
                restaurant.provider,
                restaurant,
                &settings.language,
            ) {
                Ok(value) => value,
                Err(_) => continue,
            }
        };

        if !parsed.ok || !is_today_valid_cache(&parsed, restaurant, settings, today_key) {
            continue;
        }

        let candidate_state =
            popup_state_from_cached_result(settings, restaurant, &parsed, today_key);
        let candidate_lines = build_lines(&candidate_state);
        let metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &candidate_lines,
            scale.max_content_width,
        );
        max_wrapped_lines = Some(
            max_wrapped_lines.map_or(metrics.wrapped_line_count, |prev| {
                prev.max(metrics.wrapped_line_count)
            }),
        );
        max_content_width_px = Some(
            max_content_width_px.map_or(metrics.required_content_width, |prev| {
                prev.max(metrics.required_content_width)
            }),
        );
    }

    CachedLayoutBudget {
        max_wrapped_lines,
        max_content_width_px,
    }
}

fn is_today_valid_cache(
    parsed: &api::FetchOutput,
    restaurant: Restaurant,
    settings: &Settings,
    today_key: &str,
) -> bool {
    if is_hard_closed_today(restaurant) {
        return true;
    }

    match restaurant.provider {
        Provider::Antell => {
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
                .and_then(date_key_from_epoch_ms)
                .is_some_and(|date| date == today_key)
        }
        _ => !parsed.payload_date.is_empty() && parsed.payload_date == today_key,
    }
}

fn popup_state_from_cached_result(
    settings: &Settings,
    restaurant: Restaurant,
    parsed: &api::FetchOutput,
    today_key: &str,
) -> AppState {
    let restaurant_name = if parsed.restaurant_name.is_empty() {
        restaurant.name.to_string()
    } else {
        parsed.restaurant_name.clone()
    };

    AppState {
        settings: settings.clone(),
        status: if parsed.ok {
            FetchStatus::Ok
        } else {
            FetchStatus::Error
        },
        loading_started_epoch_ms: 0,
        error_message: parsed.error_message.clone(),
        stale_network_error: false,
        today_menu: parsed.today_menu.clone(),
        restaurant_name,
        restaurant_url: parsed.restaurant_url.clone(),
        raw_payload: String::new(),
        provider: restaurant.provider,
        payload_date: parsed.payload_date.clone(),
        stale_date: !parsed.payload_date.is_empty() && parsed.payload_date != today_key,
    }
}

fn local_today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn date_key_from_epoch_ms(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }

    let secs = ms / 1000;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    let mut dt = OffsetDateTime::from_unix_timestamp(secs).ok()?;
    dt = dt.replace_nanosecond(nanos).ok()?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local = dt.to_offset(offset);
    let date = local.date();
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    ))
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
