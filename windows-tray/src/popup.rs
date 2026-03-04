use crate::api;
use crate::app::{AppState, FetchStatus};
use crate::cache;
use crate::favorites;
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, split_component_suffix, student_price_eur,
    text_for, PriceGroups,
};
use crate::gpu::{CrtProfile, GpuPresenter};
use crate::log::log_line;
use crate::model::TodayMenu;
use crate::restaurant::{available_restaurants, Provider, Restaurant};
use crate::settings::Settings;
use crate::util::to_wstring;
use anyhow::{anyhow, Context};
use std::cmp::{max, min};
use std::sync::{Arc, Mutex, OnceLock};
use time::{OffsetDateTime, UtcOffset};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, FillRect, GetDeviceCaps, GetMonitorInfoW, GetTextExtentPoint32W,
    GetTextMetricsW, InvalidateRect, MonitorFromPoint, SelectObject, SetBkMode, SetTextColor,
    TextOutW, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HFONT, HGDIOBJ,
    LOGPIXELSY, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, KillTimer, SetTimer, SetWindowPos, ShowWindow,
    HWND_TOPMOST, SWP_SHOWWINDOW, SW_HIDE,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 10;
const POPUP_MAX_WIDTH: i32 = 525;
const POPUP_MIN_WIDTH: i32 = 320;
const POPUP_MAX_CONTENT_WIDTH: i32 = POPUP_MAX_WIDTH - PADDING_X * 2;
const POPUP_MIN_CONTENT_WIDTH: i32 = POPUP_MIN_WIDTH - PADDING_X * 2;
const HEADER_HEIGHT: i32 = 46;
const HEADER_BUTTON_SIZE: i32 = 30;
const HEADER_BUTTON_GAP: i32 = 8;
const LOADING_HINT_DELAY_MS: i64 = 250;
const MAX_DYNAMIC_LINES: usize = 35;
const POPUP_ANIM_INTERVAL_MS: u32 = 33;
const POPUP_OPEN_ANIM_MS: i64 = 120;
const POPUP_CLOSE_ANIM_MS: i64 = 90;
const POPUP_SWITCH_ANIM_MS: i64 = 120;
const POPUP_SWITCH_OFFSET_PX: i32 = 6;
const FAVORITES_RELOAD_INTERVAL_MS: i64 = 1000;
const BULLET_PREFIX: &str = "▸ ";

static POPUP_LINE_BUDGET_CACHE: OnceLock<Mutex<Option<PopupLineBudgetCache>>> = OnceLock::new();
static POPUP_ANIMATION: OnceLock<Mutex<Option<PopupAnimation>>> = OnceLock::new();
static FAVORITES_CACHE: OnceLock<Mutex<FavoritesCache>> = OnceLock::new();
static POPUP_SELECTION_STATE: OnceLock<Mutex<PopupSelectionState>> = OnceLock::new();
static POPUP_FONT_CACHE: OnceLock<Mutex<Option<PopupFontCache>>> = OnceLock::new();
static POPUP_GPU_RENDERER: OnceLock<Mutex<Option<GpuPresenter>>> = OnceLock::new();
static POPUP_GPU_SURFACE: OnceLock<Mutex<Option<GpuSurface>>> = OnceLock::new();
static POPUP_GPU_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub const POPUP_ANIM_TIMER_ID: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupLineBudgetKey {
    today_key: String,
    language: String,
    theme: String,
    dpi_y: i32,
    enable_antell_restaurants: bool,
    show_prices: bool,
    show_student_price: bool,
    show_staff_price: bool,
    show_guest_price: bool,
    hide_expensive_student_meals: bool,
    show_allergens: bool,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RestaurantCacheSignature {
    code: String,
    mtime_ms: i64,
}

#[derive(Debug, Clone)]
struct PopupLineBudgetCache {
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
    max_wrapped_lines: Option<usize>,
    max_content_width_px: Option<i32>,
}

#[derive(Debug, Clone)]
enum Line {
    Heading(String),
    Text(String),
    MenuItem {
        main: String,
        suffix_segments: Vec<(String, bool)>,
    },
    Spacer,
}

#[derive(Debug, Clone)]
struct WrappedRow {
    text: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct SelectableBoundary {
    byte_index: usize,
    x_offset: i32,
}

#[derive(Debug, Clone)]
struct SelectableRow {
    item_id: usize,
    start: usize,
    end: usize,
    left: i32,
    top: i32,
    bottom: i32,
    boundaries: Vec<SelectableBoundary>,
}

#[derive(Debug, Clone, Default)]
struct SelectableLayout {
    hwnd: HWND,
    items: Vec<String>,
    rows: Vec<SelectableRow>,
}

#[derive(Debug, Clone)]
struct DrawCapture {
    layout: SelectableLayout,
}

#[derive(Debug, Clone)]
struct SelectionDrag {
    item_id: usize,
    anchor: usize,
    current: usize,
}

#[derive(Debug, Clone, Copy)]
struct SelectionRange {
    item_id: usize,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Default)]
struct PopupSelectionState {
    layout: Option<SelectableLayout>,
    drag: Option<SelectionDrag>,
}

#[derive(Debug, Clone, Default)]
struct FavoritesSnapshot {
    snippets_lower: Arc<[String]>,
}

#[derive(Debug, Clone, Default)]
struct FavoritesCache {
    loaded: bool,
    mtime_ms: i64,
    next_check_epoch_ms: i64,
    snapshot: FavoritesSnapshot,
}

#[derive(Debug, Clone)]
enum PopupAnimationKind {
    Open {
        lines: Arc<Vec<Line>>,
        title: Arc<String>,
    },
    Close {
        lines: Arc<Vec<Line>>,
        title: Arc<String>,
    },
    Switch {
        old_lines: Arc<Vec<Line>>,
        new_lines: Arc<Vec<Line>>,
        old_title: Arc<String>,
        new_title: Arc<String>,
        direction: i32,
    },
}

#[derive(Debug, Clone)]
struct PopupAnimation {
    hwnd: HWND,
    start_epoch_ms: i64,
    duration_ms: i64,
    kind: PopupAnimationKind,
}

#[derive(Debug, Clone)]
enum PopupAnimationFrame {
    Open {
        lines: Arc<Vec<Line>>,
        title: Arc<String>,
        progress: f32,
    },
    Close {
        lines: Arc<Vec<Line>>,
        title: Arc<String>,
        progress: f32,
    },
    Switch {
        old_lines: Arc<Vec<Line>>,
        new_lines: Arc<Vec<Line>>,
        old_title: Arc<String>,
        new_title: Arc<String>,
        direction: i32,
        progress: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PopupFontCacheKey {
    theme: [u8; 16],
    dpi_y: i32,
}

#[derive(Debug, Clone, Copy)]
struct PopupFonts {
    normal: HFONT,
    bold: HFONT,
    small: HFONT,
    small_bold: HFONT,
}

#[derive(Debug)]
struct PopupFontCache {
    key: PopupFontCacheKey,
    fonts: PopupFonts,
}

#[derive(Debug)]
struct GpuSurface {
    width: i32,
    height: i32,
    dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderButtonAction {
    Prev,
    Next,
    Close,
}

#[derive(Debug, Clone, Copy)]
struct HeaderLayout {
    prev: RECT,
    next: RECT,
    close: RECT,
}

pub fn toggle_popup(hwnd: HWND, state: &AppState) {
    if is_visible(hwnd) {
        begin_close_animation(hwnd, state);
    } else {
        show_popup(hwnd, state);
    }
}

pub fn show_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let (x, y) = position_near_point(width, height, cursor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_point(width, height, anchor);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    unsafe {
        let (width, height) = desired_size(hwnd, state);
        let (x, y) = position_near_tray_rect(width, height, tray_rect);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        begin_open_animation(hwnd, state);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn resize_popup_keep_position(hwnd: HWND, state: &AppState) {
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
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        InvalidateRect(hwnd, None, true);
    }
}

pub fn hide_popup(hwnd: HWND) {
    unsafe {
        clear_animation_state(hwnd);
        clear_selection_state(hwnd);
        let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn clear_font_cache() {
    let cache_lock = POPUP_FONT_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut cache) = cache_lock.lock() {
        if let Some(old) = cache.take() {
            unsafe {
                DeleteObject(old.fonts.normal);
                DeleteObject(old.fonts.bold);
                DeleteObject(old.fonts.small);
                DeleteObject(old.fonts.small_bold);
            }
        }
    }
}

fn begin_open_animation(hwnd: HWND, state: &AppState) {
    start_animation(
        hwnd,
        POPUP_OPEN_ANIM_MS,
        PopupAnimationKind::Open {
            lines: Arc::new(build_lines(state)),
            title: Arc::new(header_title(state)),
        },
    );
}

fn start_animation(hwnd: HWND, duration_ms: i64, kind: PopupAnimationKind) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        *guard = Some(PopupAnimation {
            hwnd,
            start_epoch_ms: now_epoch_ms(),
            duration_ms: duration_ms.max(1),
            kind,
        });
    }
    unsafe {
        let _ = SetTimer(hwnd, POPUP_ANIM_TIMER_ID, POPUP_ANIM_INTERVAL_MS, None);
        InvalidateRect(hwnd, None, true);
    }
}

fn clear_animation_state(hwnd: HWND) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if guard.as_ref().is_some_and(|anim| anim.hwnd == hwnd) {
            *guard = None;
        }
    }
}

fn current_animation_frame(hwnd: HWND) -> Option<PopupAnimationFrame> {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    let guard = store.lock().ok()?;
    let anim = guard.as_ref()?;
    if anim.hwnd != hwnd {
        return None;
    }
    let elapsed = now_epoch_ms().saturating_sub(anim.start_epoch_ms);
    let progress = (elapsed as f32 / anim.duration_ms.max(1) as f32).clamp(0.0, 1.0);
    match &anim.kind {
        PopupAnimationKind::Open { lines, title } => Some(PopupAnimationFrame::Open {
            lines: Arc::clone(lines),
            title: Arc::clone(title),
            progress,
        }),
        PopupAnimationKind::Close { lines, title } => Some(PopupAnimationFrame::Close {
            lines: Arc::clone(lines),
            title: Arc::clone(title),
            progress,
        }),
        PopupAnimationKind::Switch {
            old_lines,
            new_lines,
            old_title,
            new_title,
            direction,
        } => Some(PopupAnimationFrame::Switch {
            old_lines: Arc::clone(old_lines),
            new_lines: Arc::clone(new_lines),
            old_title: Arc::clone(old_title),
            new_title: Arc::clone(new_title),
            direction: *direction,
            progress,
        }),
    }
}

pub fn begin_close_animation(hwnd: HWND, state: &AppState) {
    if !is_visible(hwnd) {
        return;
    }
    clear_selection_state(hwnd);
    start_animation(
        hwnd,
        POPUP_CLOSE_ANIM_MS,
        PopupAnimationKind::Close {
            lines: Arc::new(build_lines(state)),
            title: Arc::new(header_title(state)),
        },
    );
}

pub fn begin_switch_animation(
    hwnd: HWND,
    old_state: &AppState,
    new_state: &AppState,
    direction: i32,
) {
    clear_selection_state(hwnd);
    start_animation(
        hwnd,
        POPUP_SWITCH_ANIM_MS,
        PopupAnimationKind::Switch {
            old_lines: Arc::new(build_lines(old_state)),
            new_lines: Arc::new(build_lines(new_state)),
            old_title: Arc::new(header_title(old_state)),
            new_title: Arc::new(header_title(new_state)),
            direction,
        },
    );
}

pub fn tick_animation(hwnd: HWND) {
    let now = now_epoch_ms();
    let mut active = false;
    let mut finished = false;
    let mut hide_after = false;

    {
        let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
        let mut guard = match store.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        if let Some(anim) = guard.as_ref() {
            if anim.hwnd == hwnd {
                active = true;
                let elapsed = now.saturating_sub(anim.start_epoch_ms);
                if elapsed >= anim.duration_ms.max(1) {
                    finished = true;
                    hide_after = matches!(anim.kind, PopupAnimationKind::Close { .. });
                }
            }
        }
        if finished {
            *guard = None;
        }
    }

    unsafe {
        if !active {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            return;
        }
        if finished {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            if hide_after {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
        }
        InvalidateRect(hwnd, None, true);
    }
}

pub fn header_button_at(hwnd: HWND, x: i32, y: i32) -> Option<HeaderButtonAction> {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let width = rect.right - rect.left;
        let layout = header_layout(width);
        if point_in_rect(&layout.prev, x, y) {
            return Some(HeaderButtonAction::Prev);
        }
        if point_in_rect(&layout.next, x, y) {
            return Some(HeaderButtonAction::Next);
        }
        if point_in_rect(&layout.close, x, y) {
            return Some(HeaderButtonAction::Close);
        }
        None
    }
}

pub fn begin_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let layout = match state.layout.as_ref() {
        Some(value) if value.hwnd == hwnd => value,
        _ => return false,
    };
    let Some((row, anchor_index)) = hit_test_row(layout, x, y) else {
        return false;
    };
    state.drag = Some(SelectionDrag {
        item_id: row.item_id,
        anchor: anchor_index,
        current: anchor_index,
    });
    unsafe {
        InvalidateRect(hwnd, None, true);
    }
    true
}

pub fn update_text_selection(hwnd: HWND, x: i32, y: i32) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let item_id = match state.drag.as_ref() {
        Some(drag) => drag.item_id,
        None => return,
    };
    let next_index = {
        let layout = match state.layout.as_ref() {
            Some(value) if value.hwnd == hwnd => value,
            _ => return,
        };
        let Some((row, idx)) = hit_test_row_for_item(layout, item_id, x, y) else {
            return;
        };
        if row.item_id != item_id {
            return;
        }
        idx
    };
    let Some(drag) = state.drag.as_mut() else {
        return;
    };
    if drag.current != next_index {
        drag.current = next_index;
        unsafe {
            InvalidateRect(hwnd, None, true);
        }
    }
}

pub fn finish_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    let snippet = {
        let mut state = match selection_state().lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let Some(mut drag) = state.drag.take() else {
            return false;
        };
        {
            let layout = match state.layout.as_ref() {
                Some(value) if value.hwnd == hwnd => value,
                _ => return false,
            };
            if let Some((_, next_index)) = hit_test_row_for_item(layout, drag.item_id, x, y) {
                drag.current = next_index;
            }
        }

        let layout = match state.layout.as_ref() {
            Some(value) if value.hwnd == hwnd => value,
            _ => return false,
        };
        let selected = selected_range(drag.anchor, drag.current);
        if selected.0 == selected.1 {
            None
        } else {
            layout.items.get(drag.item_id).and_then(|text| {
                text.get(selected.0..selected.1)
                    .map(|value| favorites::normalize_snippet(value))
                    .filter(|value| !value.is_empty())
            })
        }
    };

    let Some(value) = snippet else {
        unsafe {
            InvalidateRect(hwnd, None, true);
        }
        return false;
    };

    if favorites::toggle_snippet(&value).is_err() {
        unsafe {
            InvalidateRect(hwnd, None, true);
        }
        return false;
    }

    invalidate_favorites_cache();
    unsafe {
        InvalidateRect(hwnd, None, true);
    }
    true
}

pub fn cancel_text_selection(hwnd: HWND) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    if state.drag.is_some() {
        state.drag = None;
        unsafe {
            InvalidateRect(hwnd, None, true);
        }
    }
}

pub fn text_selection_active(hwnd: HWND) -> bool {
    let state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    state
        .layout
        .as_ref()
        .is_some_and(|layout| layout.hwnd == hwnd)
        && state.drag.is_some()
}

pub fn paint_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.0 == 0 {
            return;
        }

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        if state.settings.renderer_backend == "gpu" {
            if let Err(err) = paint_popup_gpu(hwnd, state, &rect) {
                let detail = format!("{:#}", err);
                log_line(&format!("gpu render failed: {}", detail.replace('\n', " | ")));
                record_gpu_error(&detail);
                paint_popup_to_hdc(hwnd, state, hdc, rect);
                let headline = detail.lines().next().unwrap_or("GPU renderer error");
                draw_gpu_error_line(hdc, &rect, &format!("GPU renderer error: {}", headline));
            } else {
                clear_gpu_error();
            }
        } else {
            clear_gpu_error();
            paint_popup_to_hdc(hwnd, state, hdc, rect);
        }
        EndPaint(hwnd, &ps);
    }
}

unsafe fn paint_popup_to_hdc(hwnd: HWND, state: &AppState, hdc: HDC, rect: RECT) {
    let width = rect.right - rect.left;
    let palette = theme_palette(&state.settings.theme);
    let brush = CreateSolidBrush(palette.bg_color);
    FillRect(hdc, &rect, brush);
    DeleteObject(brush);
    SetBkMode(hdc, TRANSPARENT);

    let fonts = fonts_for_paint(hdc, &state.settings.theme);
    let normal_font = fonts.normal;
    let bold_font = fonts.bold;
    let small_font = fonts.small;
    let small_bold_font = fonts.small_bold;
    let old_font = SelectObject(hdc, normal_font);

    let metrics = text_metrics(hdc, normal_font);
    let line_height = metrics.tmHeight as i32 + LINE_GAP;
    let content_width = (width - PADDING_X * 2).max(40);
    let animation = current_animation_frame(hwnd);
    let favorites = current_favorites_snapshot();

    let header_rect = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.top + HEADER_HEIGHT,
    };
    let header_brush = CreateSolidBrush(palette.header_bg_color);
    FillRect(hdc, &header_rect, header_brush);
    DeleteObject(header_brush);

    let layout = header_layout(width);
    draw_header_button(
        hdc,
        &layout.prev,
        "<",
        palette.button_bg_color,
        palette.body_text_color,
        normal_font,
    );
    draw_header_button(
        hdc,
        &layout.next,
        ">",
        palette.button_bg_color,
        palette.body_text_color,
        normal_font,
    );
    draw_header_button(
        hdc,
        &layout.close,
        "X",
        palette.button_bg_color,
        palette.body_text_color,
        normal_font,
    );

    let divider_rect = RECT {
        left: rect.left,
        top: header_rect.bottom - 1,
        right: rect.right,
        bottom: header_rect.bottom,
    };
    let divider_brush = CreateSolidBrush(palette.divider_color);
    FillRect(hdc, &divider_rect, divider_brush);
    DeleteObject(divider_brush);

    if let Some(frame) = animation {
        clear_selection_layout(hwnd);
        match frame {
            PopupAnimationFrame::Open {
                lines,
                title,
                progress,
            } => {
                let y_offset = ((1.0 - progress) * POPUP_SWITCH_OFFSET_PX as f32).round() as i32;
                let layer_body_text =
                    lerp_color(palette.bg_color, palette.body_text_color, progress);
                let layer_heading = lerp_color(palette.bg_color, palette.heading_color, progress);
                let layer_title =
                    lerp_color(palette.bg_color, palette.header_title_color, progress);
                let layer_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                let layer_suffix_highlight =
                    lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                let layer_favorites =
                    lerp_color(palette.bg_color, palette.favorite_highlight_color, progress);
                draw_content_layer(
                    hdc,
                    title.as_str(),
                    lines.as_ref(),
                    DrawLayerParams {
                        width,
                        content_width,
                        body_text_color: layer_body_text,
                        heading_color: layer_heading,
                        header_title_color: layer_title,
                        suffix_color: layer_suffix,
                        suffix_highlight_color: layer_suffix_highlight,
                        favorite_highlight_color: layer_favorites,
                        selection_bg_color: palette.selection_bg_color,
                        layout: &layout,
                        metrics: &metrics,
                        line_height,
                        normal_font,
                        bold_font,
                        small_font,
                        small_bold_font,
                        favorites: &favorites,
                        selection: None,
                        capture: None,
                        y_offset,
                    },
                );
            }
            PopupAnimationFrame::Close {
                lines,
                title,
                progress,
            } => {
                let y_offset = -((progress * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                let layer_body_text =
                    lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                let layer_heading =
                    lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                let layer_title =
                    lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                let layer_suffix =
                    lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                let layer_suffix_highlight = lerp_color(
                    palette.bg_color,
                    palette.suffix_highlight_color,
                    1.0 - progress,
                );
                let layer_favorites = lerp_color(
                    palette.bg_color,
                    palette.favorite_highlight_color,
                    1.0 - progress,
                );
                draw_content_layer(
                    hdc,
                    title.as_str(),
                    lines.as_ref(),
                    DrawLayerParams {
                        width,
                        content_width,
                        body_text_color: layer_body_text,
                        heading_color: layer_heading,
                        header_title_color: layer_title,
                        suffix_color: layer_suffix,
                        suffix_highlight_color: layer_suffix_highlight,
                        favorite_highlight_color: layer_favorites,
                        selection_bg_color: palette.selection_bg_color,
                        layout: &layout,
                        metrics: &metrics,
                        line_height,
                        normal_font,
                        bold_font,
                        small_font,
                        small_bold_font,
                        favorites: &favorites,
                        selection: None,
                        capture: None,
                        y_offset,
                    },
                );
            }
            PopupAnimationFrame::Switch {
                old_lines,
                new_lines,
                old_title,
                new_title,
                direction,
                progress,
            } => {
                let dir = if direction >= 0 { 1 } else { -1 };
                let old_offset = -dir * ((progress * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                let new_offset =
                    dir * (((1.0 - progress) * POPUP_SWITCH_OFFSET_PX as f32).round() as i32);
                let old_body_text =
                    lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                let old_heading =
                    lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                let old_title_color =
                    lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                let old_suffix = lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                let old_suffix_highlight = lerp_color(
                    palette.bg_color,
                    palette.suffix_highlight_color,
                    1.0 - progress,
                );
                let old_favorites = lerp_color(
                    palette.bg_color,
                    palette.favorite_highlight_color,
                    1.0 - progress,
                );
                let new_body_text = lerp_color(palette.bg_color, palette.body_text_color, progress);
                let new_heading = lerp_color(palette.bg_color, palette.heading_color, progress);
                let new_title_color =
                    lerp_color(palette.bg_color, palette.header_title_color, progress);
                let new_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                let new_suffix_highlight =
                    lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                let new_favorites =
                    lerp_color(palette.bg_color, palette.favorite_highlight_color, progress);
                draw_content_layer(
                    hdc,
                    old_title.as_str(),
                    old_lines.as_ref(),
                    DrawLayerParams {
                        width,
                        content_width,
                        body_text_color: old_body_text,
                        heading_color: old_heading,
                        header_title_color: old_title_color,
                        suffix_color: old_suffix,
                        suffix_highlight_color: old_suffix_highlight,
                        favorite_highlight_color: old_favorites,
                        selection_bg_color: palette.selection_bg_color,
                        layout: &layout,
                        metrics: &metrics,
                        line_height,
                        normal_font,
                        bold_font,
                        small_font,
                        small_bold_font,
                        favorites: &favorites,
                        selection: None,
                        capture: None,
                        y_offset: old_offset,
                    },
                );
                draw_content_layer(
                    hdc,
                    new_title.as_str(),
                    new_lines.as_ref(),
                    DrawLayerParams {
                        width,
                        content_width,
                        body_text_color: new_body_text,
                        heading_color: new_heading,
                        header_title_color: new_title_color,
                        suffix_color: new_suffix,
                        suffix_highlight_color: new_suffix_highlight,
                        favorite_highlight_color: new_favorites,
                        selection_bg_color: palette.selection_bg_color,
                        layout: &layout,
                        metrics: &metrics,
                        line_height,
                        normal_font,
                        bold_font,
                        small_font,
                        small_bold_font,
                        favorites: &favorites,
                        selection: None,
                        capture: None,
                        y_offset: new_offset,
                    },
                );
            }
        }
    } else {
        let lines = build_lines(state);
        let title = header_title(state);
        let selection = current_selection_range(hwnd);
        let mut capture = DrawCapture {
            layout: SelectableLayout {
                hwnd,
                ..Default::default()
            },
        };
        draw_content_layer(
            hdc,
            &title,
            &lines,
            DrawLayerParams {
                width,
                content_width,
                body_text_color: palette.body_text_color,
                heading_color: palette.heading_color,
                header_title_color: palette.header_title_color,
                suffix_color: palette.suffix_color,
                suffix_highlight_color: palette.suffix_highlight_color,
                favorite_highlight_color: palette.favorite_highlight_color,
                selection_bg_color: palette.selection_bg_color,
                layout: &layout,
                metrics: &metrics,
                line_height,
                normal_font,
                bold_font,
                small_font,
                small_bold_font,
                favorites: &favorites,
                selection: selection.as_ref(),
                capture: Some(&mut capture),
                y_offset: 0,
            },
        );
        store_selection_layout(capture.layout);
    }

    SelectObject(hdc, old_font);
}

fn paint_popup_gpu(hwnd: HWND, state: &AppState, rect: &RECT) -> anyhow::Result<()> {
    let width = (rect.right - rect.left).max(1);
    let height = (rect.bottom - rect.top).max(1);
    let profile = CrtProfile::from_settings(&state.settings.crt_profile);
    let shutdown_progress = if matches!(profile, CrtProfile::Full) {
        match current_animation_frame(hwnd) {
            Some(PopupAnimationFrame::Close { progress, .. }) => progress.clamp(0.0, 1.0),
            _ => 0.0,
        }
    } else {
        0.0
    };

    let surface_lock = POPUP_GPU_SURFACE.get_or_init(|| Mutex::new(None));
    let mut surface_guard = surface_lock
        .lock()
        .map_err(|_| anyhow!("GPU surface mutex poisoned"))?;
    if surface_guard
        .as_ref()
        .is_none_or(|s| s.width != width || s.height != height)
    {
        destroy_gpu_surface(&mut surface_guard);
        *surface_guard = Some(create_gpu_surface(width, height)?);
    }
    let surface = surface_guard
        .as_mut()
        .ok_or_else(|| anyhow!("Failed to initialize GPU surface"))?;

    unsafe {
        paint_popup_to_hdc(hwnd, state, surface.dc, *rect);
    }

    let byte_len = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let frame = unsafe { std::slice::from_raw_parts(surface.bits as *const u8, byte_len) };

    let renderer_lock = POPUP_GPU_RENDERER.get_or_init(|| Mutex::new(None));
    let mut renderer_guard = renderer_lock
        .lock()
        .map_err(|_| anyhow!("GPU renderer mutex poisoned"))?;
    let needs_new = renderer_guard
        .as_ref()
        .is_none_or(|renderer| renderer.hwnd() != hwnd);
    if needs_new {
        *renderer_guard =
            Some(GpuPresenter::new(hwnd, width, height).context("initialize D3D11 presenter")?);
    }
    let renderer = renderer_guard
        .as_mut()
        .ok_or_else(|| anyhow!("GPU presenter unavailable"))?;
    renderer
        .render_bgra_frame(frame, width, height, profile, shutdown_progress)
        .context("render CRT frame")?;

    Ok(())
}

fn create_gpu_surface(width: i32, height: i32) -> anyhow::Result<GpuSurface> {
    unsafe {
        let dc = CreateCompatibleDC(HDC(0));
        if dc.0 == 0 {
            return Err(anyhow!("CreateCompatibleDC failed"));
        }
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };
        let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            windows::Win32::Foundation::HANDLE(0),
            0,
        )
        .context("CreateDIBSection failed")?;
        let old_bitmap = SelectObject(dc, bitmap);
        if old_bitmap.0 == 0 {
            DeleteObject(bitmap);
            DeleteDC(dc);
            return Err(anyhow!("SelectObject for DIB section failed"));
        }
        if bits_ptr.is_null() {
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            return Err(anyhow!("CreateDIBSection returned null bits pointer"));
        }
        Ok(GpuSurface {
            width,
            height,
            dc,
            bitmap,
            old_bitmap,
            bits: bits_ptr as usize,
        })
    }
}

fn destroy_gpu_surface(surface: &mut Option<GpuSurface>) {
    let Some(surface) = surface.take() else {
        return;
    };
    unsafe {
        let _ = SelectObject(surface.dc, surface.old_bitmap);
        let _ = DeleteObject(surface.bitmap);
        let _ = DeleteDC(surface.dc);
    }
}

fn draw_gpu_error_line(hdc: HDC, rect: &RECT, message: &str) {
    unsafe {
        let old = SetTextColor(hdc, rgb(255, 96, 96));
        let clipped = fit_text_to_width(
            hdc,
            message,
            (rect.right - rect.left - PADDING_X * 2).max(80),
        );
        let y = (rect.bottom - 18).max(HEADER_HEIGHT + 4);
        draw_text_line(hdc, &clipped, PADDING_X, y);
        let _ = SetTextColor(hdc, old);
    }
}

fn record_gpu_error(message: &str) {
    let lock = POPUP_GPU_ERROR.get_or_init(|| Mutex::new(None));
    if let Ok(mut error) = lock.lock() {
        *error = Some(message.to_string());
    }
}

fn clear_gpu_error() {
    let lock = POPUP_GPU_ERROR.get_or_init(|| Mutex::new(None));
    if let Ok(mut error) = lock.lock() {
        *error = None;
    }
}

pub fn take_gpu_error() -> Option<String> {
    let lock = POPUP_GPU_ERROR.get_or_init(|| Mutex::new(None));
    match lock.lock() {
        Ok(mut error) => error.take(),
        Err(_) => None,
    }
}

pub fn shutdown_gpu_renderer() {
    if let Some(lock) = POPUP_GPU_RENDERER.get() {
        if let Ok(mut renderer) = lock.lock() {
            *renderer = None;
        }
    }
    if let Some(lock) = POPUP_GPU_SURFACE.get() {
        if let Ok(mut surface) = lock.lock() {
            destroy_gpu_surface(&mut surface);
        }
    }
    clear_gpu_error();
}

struct DrawLayerParams<'a> {
    width: i32,
    content_width: i32,
    body_text_color: COLORREF,
    heading_color: COLORREF,
    header_title_color: COLORREF,
    suffix_color: COLORREF,
    suffix_highlight_color: COLORREF,
    favorite_highlight_color: COLORREF,
    selection_bg_color: COLORREF,
    layout: &'a HeaderLayout,
    metrics: &'a TEXTMETRICW,
    line_height: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    favorites: &'a FavoritesSnapshot,
    selection: Option<&'a SelectionRange>,
    capture: Option<&'a mut DrawCapture>,
    y_offset: i32,
}

fn draw_content_layer(hdc: HDC, title: &str, lines: &[Line], params: DrawLayerParams<'_>) {
    unsafe {
        SelectObject(hdc, params.bold_font);
        SetTextColor(hdc, params.header_title_color);
    }

    let clipped_title = fit_text_to_width(
        hdc,
        title,
        (params.layout.close.left - params.layout.next.right - 24).max(40),
    );
    let title_width = text_width(hdc, &clipped_title);
    let title_x = ((params.width - title_width) / 2).max(params.layout.next.right + 12);
    let title_y = ((HEADER_HEIGHT - params.metrics.tmHeight as i32) / 2 - 1) + params.y_offset;
    draw_text_line(hdc, &clipped_title, title_x, title_y);

    let bullet_width = text_width_with_font(hdc, params.normal_font, BULLET_PREFIX);
    let main_wrap_width = (params.content_width - bullet_width).max(24);

    let mut y = HEADER_HEIGHT + PADDING_Y + params.y_offset;
    let mut capture = params.capture;
    for line in lines {
        match line {
            Line::Heading(text) => {
                unsafe {
                    SelectObject(hdc, params.bold_font);
                    SetTextColor(hdc, params.heading_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, PADDING_X, y);
                        y += params.line_height;
                    }
                }
            }
            Line::Text(text) => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, PADDING_X, y);
                        y += params.line_height;
                    }
                }
            }
            Line::MenuItem {
                main,
                suffix_segments,
            } => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let favorite_ranges = favorite_match_ranges(main, params.favorites);
                let styled_width = text_with_suffix_width(
                    hdc,
                    params.normal_font,
                    params.small_font,
                    params.small_bold_font,
                    main,
                    suffix_segments,
                    bullet_width,
                );
                let item_id = if let Some(ref mut draw_capture) = capture {
                    let id = draw_capture.layout.items.len();
                    draw_capture.layout.items.push(main.clone());
                    Some(id)
                } else {
                    None
                };
                let selected_item_range = item_id
                    .and_then(|id| params.selection.filter(|sel| sel.item_id == id).copied());

                if styled_width <= params.content_width {
                    let mut suffix_width = 0;
                    for (segment, bold) in suffix_segments {
                        let font = if *bold {
                            params.small_bold_font
                        } else {
                            params.small_font
                        };
                        unsafe {
                            SelectObject(hdc, font);
                        }
                        suffix_width += text_width(hdc, segment);
                    }
                    let max_main = (params.content_width - bullet_width - suffix_width - 4).max(24);
                    unsafe {
                        SelectObject(hdc, params.normal_font);
                        SetTextColor(hdc, params.body_text_color);
                    }
                    let clipped_main = fit_text_to_width(hdc, main, max_main);
                    let line_x = PADDING_X + bullet_width;
                    let row = WrappedRow {
                        start: 0,
                        end: clipped_main.len(),
                        text: clipped_main.clone(),
                    };
                    let row_segments =
                        segments_for_row(&clipped_main, row.start, row.end, &favorite_ranges);
                    draw_text_line(hdc, BULLET_PREFIX, PADDING_X, y);
                    if let Some(selection) = selected_item_range {
                        draw_selection_bg_for_row(
                            hdc,
                            line_x,
                            y,
                            params.line_height,
                            &row,
                            selection.start,
                            selection.end,
                            params.selection_bg_color,
                        );
                    }
                    draw_main_segments(
                        hdc,
                        &row_segments,
                        line_x,
                        y,
                        params.normal_font,
                        params.body_text_color,
                        params.favorite_highlight_color,
                    );
                    if let Some(ref mut draw_capture) = capture {
                        add_selectable_row(
                            &mut draw_capture.layout,
                            item_id.unwrap_or(0),
                            &row,
                            line_x,
                            y,
                            params.line_height,
                            hdc,
                            params.normal_font,
                        );
                    }
                    if !suffix_segments.is_empty() {
                        let main_width =
                            text_width_with_font(hdc, params.normal_font, &clipped_main);
                        let suffix_x = line_x + main_width + 4;
                        if suffix_x < (PADDING_X + params.content_width) {
                            draw_text_segments(
                                hdc,
                                suffix_segments,
                                suffix_x,
                                y + 1,
                                params.small_font,
                                params.small_bold_font,
                                params.suffix_color,
                                params.suffix_highlight_color,
                            );
                        }
                    }
                    y += params.line_height;
                    continue;
                }

                let wrapped_main = wrap_text_to_width_with_font_rows(
                    hdc,
                    params.normal_font,
                    main,
                    main_wrap_width,
                );
                if wrapped_main.is_empty() {
                    y += params.line_height;
                } else {
                    for (idx, row) in wrapped_main.iter().enumerate() {
                        let line_x = PADDING_X + bullet_width;
                        if idx == 0 {
                            draw_text_line(hdc, BULLET_PREFIX, PADDING_X, y);
                        }
                        if let Some(selection) = selected_item_range {
                            draw_selection_bg_for_row(
                                hdc,
                                line_x,
                                y,
                                params.line_height,
                                row,
                                selection.start,
                                selection.end,
                                params.selection_bg_color,
                            );
                        }
                        let row_segments =
                            segments_for_row(main, row.start, row.end, &favorite_ranges);
                        draw_main_segments(
                            hdc,
                            &row_segments,
                            line_x,
                            y,
                            params.normal_font,
                            params.body_text_color,
                            params.favorite_highlight_color,
                        );
                        if let Some(ref mut draw_capture) = capture {
                            add_selectable_row(
                                &mut draw_capture.layout,
                                item_id.unwrap_or(0),
                                row,
                                line_x,
                                y,
                                params.line_height,
                                hdc,
                                params.normal_font,
                            );
                        }
                        y += params.line_height;
                    }
                }

                if !suffix_segments.is_empty() {
                    let suffix_plain = flatten_suffix_segments(suffix_segments);
                    if !suffix_plain.is_empty() {
                        let wrapped_suffix = wrap_text_to_width_with_font(
                            hdc,
                            params.small_font,
                            &suffix_plain,
                            params.content_width,
                        );
                        if wrapped_suffix.len() == 1 {
                            draw_text_segments(
                                hdc,
                                suffix_segments,
                                PADDING_X + bullet_width,
                                y + 1,
                                params.small_font,
                                params.small_bold_font,
                                params.suffix_color,
                                params.suffix_highlight_color,
                            );
                            y += params.line_height;
                        } else if wrapped_suffix.is_empty() {
                            y += params.line_height;
                        } else {
                            unsafe {
                                SelectObject(hdc, params.small_font);
                                SetTextColor(hdc, params.suffix_color);
                            }
                            for row in wrapped_suffix {
                                draw_text_line(hdc, &row, PADDING_X + bullet_width, y);
                                y += params.line_height;
                            }
                        }
                    }
                }
            }
            Line::Spacer => {
                y += params.line_height / 2;
            }
        }
    }
}

fn draw_main_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    x: i32,
    y: i32,
    font: HFONT,
    normal_color: COLORREF,
    highlight_color: COLORREF,
) {
    let mut cursor = x;
    for (text, highlighted) in segments {
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(
                hdc,
                if *highlighted {
                    highlight_color
                } else {
                    normal_color
                },
            );
        }
        draw_text_line(hdc, text, cursor, y);
        cursor += text_width_with_font(hdc, font, text);
    }
}

fn draw_selection_bg_for_row(
    hdc: HDC,
    row_x: i32,
    row_y: i32,
    line_height: i32,
    row: &WrappedRow,
    sel_start: usize,
    sel_end: usize,
    selection_bg_color: COLORREF,
) {
    let start = max(row.start, sel_start);
    let end = min(row.end, sel_end);
    if start >= end {
        return;
    }
    let local_start = start.saturating_sub(row.start);
    let local_end = end.saturating_sub(row.start);
    let Some(left_slice) = row.text.get(..local_start) else {
        return;
    };
    let Some(right_slice) = row.text.get(..local_end) else {
        return;
    };
    let left_width = text_width(hdc, left_slice);
    let right_width = text_width(hdc, right_slice);
    let rect = RECT {
        left: row_x + left_width,
        top: row_y,
        right: row_x + right_width,
        bottom: row_y + line_height - 1,
    };
    unsafe {
        let brush = CreateSolidBrush(selection_bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
    }
}

fn add_selectable_row(
    layout: &mut SelectableLayout,
    item_id: usize,
    row: &WrappedRow,
    row_x: i32,
    row_y: i32,
    line_height: i32,
    hdc: HDC,
    font: HFONT,
) {
    layout.rows.push(SelectableRow {
        item_id,
        start: row.start,
        end: row.end,
        left: row_x,
        top: row_y,
        bottom: row_y + line_height,
        boundaries: row_boundaries(hdc, font, &row.text),
    });
}

fn row_boundaries(hdc: HDC, font: HFONT, text: &str) -> Vec<SelectableBoundary> {
    let mut out = Vec::new();
    out.push(SelectableBoundary {
        byte_index: 0,
        x_offset: 0,
    });
    let mut x = 0;
    for (idx, ch) in text.char_indices() {
        let mut single = String::new();
        single.push(ch);
        x += text_width_with_font(hdc, font, &single);
        out.push(SelectableBoundary {
            byte_index: idx + ch.len_utf8(),
            x_offset: x,
        });
    }
    out
}

fn segments_for_row(
    full_text: &str,
    row_start: usize,
    row_end: usize,
    ranges: &[(usize, usize)],
) -> Vec<(String, bool)> {
    let Some(row_slice) = full_text.get(row_start..row_end) else {
        return vec![(String::new(), false)];
    };
    let mut out = Vec::new();
    let mut cursor = row_start;
    for (start, end) in ranges {
        let overlap_start = max(*start, row_start);
        let overlap_end = min(*end, row_end);
        if overlap_start >= overlap_end {
            continue;
        }
        if cursor < overlap_start {
            if let Some(normal) = full_text.get(cursor..overlap_start) {
                out.push((normal.to_string(), false));
            }
        }
        if let Some(highlight) = full_text.get(overlap_start..overlap_end) {
            out.push((highlight.to_string(), true));
        }
        cursor = overlap_end;
    }
    if cursor < row_end {
        if let Some(rest) = full_text.get(cursor..row_end) {
            out.push((rest.to_string(), false));
        }
    }
    if out.is_empty() {
        out.push((row_slice.to_string(), false));
    }
    out
}

fn favorite_match_ranges(text: &str, favorites: &FavoritesSnapshot) -> Vec<(usize, usize)> {
    if text.is_empty() || favorites.snippets_lower.is_empty() {
        return Vec::new();
    }
    let lower_text = text.to_lowercase();
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for snippet_lower in favorites.snippets_lower.iter() {
        if snippet_lower.is_empty() {
            continue;
        }
        let mut search_start = 0usize;
        while search_start < lower_text.len() {
            let Some(found) = lower_text[search_start..].find(snippet_lower) else {
                break;
            };
            let start = search_start + found;
            let end = start + snippet_lower.len();
            if text.get(start..end).is_some() {
                candidates.push((start, end));
            }
            search_start = end;
        }
    }

    candidates.sort_by(|a, b| {
        let len_a = a.1.saturating_sub(a.0);
        let len_b = b.1.saturating_sub(b.0);
        len_b.cmp(&len_a).then(a.0.cmp(&b.0))
    });

    let mut kept: Vec<(usize, usize)> = Vec::new();
    for range in candidates {
        if kept.iter().any(|existing| ranges_overlap(*existing, range)) {
            continue;
        }
        kept.push(range);
    }
    kept.sort_by_key(|range| range.0);
    kept
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    max(a.0, b.0) < min(a.1, b.1)
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

fn wrap_text_to_width_with_font_rows(
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

fn wrap_text_to_width_with_font(hdc: HDC, font: HFONT, text: &str, max_width: i32) -> Vec<String> {
    wrap_text_to_width_with_font_rows(hdc, font, text, max_width)
        .into_iter()
        .map(|row| row.text)
        .collect()
}

fn wrap_text_to_width(hdc: HDC, text: &str, max_width: i32) -> Vec<String> {
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

fn text_width_with_font(hdc: HDC, font: HFONT, text: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let width = text_width(hdc, text);
        SelectObject(hdc, old);
        width
    }
}

fn text_with_suffix_width(
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

fn flatten_suffix_segments(segments: &[(String, bool)]) -> String {
    let mut out = String::new();
    for (segment, _) in segments {
        out.push_str(segment);
    }
    normalize_text(&out)
}

fn draw_text_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    x: i32,
    y: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    normal_color: COLORREF,
    highlight_color: COLORREF,
) {
    let mut cursor = x;
    for (text, bold) in segments {
        let font = if *bold { bold_font } else { normal_font };
        let color = if *bold { highlight_color } else { normal_color };
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(hdc, color);
        }
        draw_text_line(hdc, text, cursor, y);
        cursor += text_width(hdc, text);
    }
}

fn draw_text_line(hdc: HDC, text: &str, x: i32, y: i32) {
    let wide = to_wstring(text);
    unsafe {
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = TextOutW(hdc, x, y, slice);
        }
    }
}

fn fit_text_to_width(hdc: HDC, text: &str, max_width: i32) -> String {
    let clean = normalize_text(text);
    if clean.is_empty() || max_width <= 0 {
        return String::new();
    }
    if text_width(hdc, &clean) <= max_width {
        return clean;
    }

    let ellipsis = "...";
    let ellipsis_width = text_width(hdc, ellipsis);
    if ellipsis_width >= max_width {
        return ellipsis.to_string();
    }

    let mut out = String::new();
    for ch in clean.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);
        candidate.push_str(ellipsis);
        if text_width(hdc, &candidate) > max_width {
            break;
        }
        out.push(ch);
    }

    let mut trimmed = out.trim_end().to_string();
    trimmed.push_str(ellipsis);
    trimmed
}

fn draw_header_button(
    hdc: HDC,
    rect: &RECT,
    label: &str,
    bg_color: COLORREF,
    text_color: COLORREF,
    font: HFONT,
) {
    unsafe {
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, rect, brush);
        DeleteObject(brush);
        SelectObject(hdc, font);
        SetTextColor(hdc, text_color);
    }
    let label_width = text_width(hdc, label);
    let metrics = text_metrics(hdc, font);
    let x = rect.left + ((rect.right - rect.left - label_width) / 2).max(0);
    let y = rect.top + ((rect.bottom - rect.top - metrics.tmHeight as i32) / 2).max(0);
    draw_text_line(hdc, label, x, y);
}

fn header_layout(width: i32) -> HeaderLayout {
    let top = (HEADER_HEIGHT - HEADER_BUTTON_SIZE) / 2;
    let prev = RECT {
        left: PADDING_X,
        top,
        right: PADDING_X + HEADER_BUTTON_SIZE,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    let next = RECT {
        left: prev.right + HEADER_BUTTON_GAP,
        top,
        right: prev.right + HEADER_BUTTON_GAP + HEADER_BUTTON_SIZE,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    let close = RECT {
        left: width - PADDING_X - HEADER_BUTTON_SIZE,
        top,
        right: width - PADDING_X,
        bottom: top + HEADER_BUTTON_SIZE,
    };
    HeaderLayout { prev, next, close }
}

fn header_title(state: &AppState) -> String {
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

fn text_metrics(hdc: HDC, font: HFONT) -> TEXTMETRICW {
    unsafe {
        let old = SelectObject(hdc, font);
        let mut metrics = TEXTMETRICW::default();
        GetTextMetricsW(hdc, &mut metrics);
        SelectObject(hdc, old);
        metrics
    }
}

fn text_width(hdc: HDC, text: &str) -> i32 {
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
        let fonts = fonts_for_paint(hdc, &state.settings.theme);
        let normal_font = fonts.normal;
        let bold_font = fonts.bold;
        let small_font = fonts.small;
        let small_bold_font = fonts.small_bold;
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        let current_lines = build_lines(state);
        let current_metrics = measure_lines_layout(
            hdc,
            normal_font,
            bold_font,
            small_font,
            small_bold_font,
            &current_lines,
            POPUP_MAX_CONTENT_WIDTH,
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
            .clamp(POPUP_MIN_CONTENT_WIDTH, POPUP_MAX_CONTENT_WIDTH);
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
        let line_height = metrics.tmHeight as i32 + LINE_GAP;
        let height = HEADER_HEIGHT + (target_lines as i32 * line_height) + PADDING_Y * 2;
        let width = (target_content_width + PADDING_X * 2).clamp(POPUP_MIN_WIDTH, POPUP_MAX_WIDTH);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

        (width, height.max(HEADER_HEIGHT + 120))
    }
}

fn create_fonts(hdc: HDC, theme: &str) -> (HFONT, HFONT, HFONT, HFONT) {
    unsafe {
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY);
        let height_normal = -MulDiv(12, dpi, 72);
        let height_small = -MulDiv(10, dpi, 72);
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

fn theme_key(theme: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (idx, byte) in theme.as_bytes().iter().take(16).enumerate() {
        out[idx] = byte.to_ascii_lowercase();
    }
    out
}

fn fonts_for_paint(hdc: HDC, theme: &str) -> PopupFonts {
    let dpi_y = unsafe { GetDeviceCaps(hdc, LOGPIXELSY) };
    let key = PopupFontCacheKey {
        theme: theme_key(theme),
        dpi_y,
    };
    let cache_lock = POPUP_FONT_CACHE.get_or_init(|| Mutex::new(None));
    let mut cache = match cache_lock.lock() {
        Ok(value) => value,
        Err(_) => {
            let (normal, bold, small, small_bold) = create_fonts(hdc, theme);
            return PopupFonts {
                normal,
                bold,
                small,
                small_bold,
            };
        }
    };

    if let Some(existing) = cache.as_ref() {
        if existing.key == key {
            return existing.fonts;
        }
    }

    if let Some(old) = cache.take() {
        unsafe {
            DeleteObject(old.fonts.normal);
            DeleteObject(old.fonts.bold);
            DeleteObject(old.fonts.small);
            DeleteObject(old.fonts.small_bold);
        }
    }

    let (normal, bold, small, small_bold) = create_fonts(hdc, theme);
    let fonts = PopupFonts {
        normal,
        bold,
        small,
        small_bold,
    };
    *cache = Some(PopupFontCache { key, fonts });
    fonts
}

fn build_lines(state: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();

    if state.stale_date {
        lines.push(Line::Heading("[STALE]".to_string()));
    }

    let show_loading_hint = state.status == FetchStatus::Loading
        && state.today_menu.is_none()
        && state.loading_started_epoch_ms > 0
        && now_epoch_ms().saturating_sub(state.loading_started_epoch_ms) >= LOADING_HINT_DELAY_MS;

    if show_loading_hint {
        lines.push(Line::Text(text_for(&state.settings.language, "loading")));
    }

    let date_line = date_and_time_line(state.today_menu.as_ref(), &state.settings.language);
    if !date_line.is_empty() {
        lines.push(Line::Heading(date_line));
    }

    match &state.today_menu {
        Some(menu) => {
            if !menu.menus.is_empty() {
                let price_groups = PriceGroups {
                    student: state.settings.show_student_price,
                    staff: state.settings.show_staff_price,
                    guest: state.settings.show_guest_price,
                };
                append_menus(
                    &mut lines,
                    menu,
                    state.provider,
                    state.settings.show_prices,
                    price_groups,
                    state.settings.show_allergens,
                    state.settings.highlight_gluten_free,
                    state.settings.highlight_veg,
                    state.settings.highlight_lactose_free,
                    state.settings.hide_expensive_student_meals,
                );
            } else if state.status != FetchStatus::Loading {
                lines.push(Line::Text(text_for(&state.settings.language, "noMenu")));
            }
        }
        None => {
            if state.status != FetchStatus::Loading {
                lines.push(Line::Text(text_for(&state.settings.language, "noMenu")));
            }
        }
    }

    if state.status == FetchStatus::Stale {
        lines.push(Line::Spacer);
        let stale_key = if state.stale_network_error {
            "staleNetwork"
        } else {
            "stale"
        };
        lines.push(Line::Text(text_for(&state.settings.language, stale_key)));
    }

    if !state.error_message.is_empty() && state.status != FetchStatus::Ok {
        lines.push(Line::Text(format!(
            "{}: {}",
            text_for(&state.settings.language, "fetchError"),
            state.error_message
        )));
    }

    lines
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
    let signatures = cache_signatures(&state.settings);
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

fn cache_signatures(settings: &Settings) -> Vec<RestaurantCacheSignature> {
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
    let mut max_wrapped_lines: Option<usize> = None;
    let mut max_content_width_px: Option<i32> = None;

    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let raw = match cache::read_cache(restaurant.provider, restaurant.code, &settings.language)
        {
            Some(payload) => payload,
            None => continue,
        };

        let parsed = match api::parse_cached_payload(
            &raw,
            restaurant.provider,
            restaurant,
            &settings.language,
        ) {
            Ok(value) => value,
            Err(_) => continue,
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
            POPUP_MAX_CONTENT_WIDTH,
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
        has_payload: !parsed.raw_json.is_empty()
            || parsed.today_menu.is_some()
            || !parsed.payload_date.is_empty(),
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
        let mut info = MONITORINFO::default();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
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

fn position_near_tray_rect(width: i32, height: i32, tray_rect: RECT) -> (i32, i32) {
    unsafe {
        let center = POINT {
            x: (tray_rect.left + tray_rect.right) / 2,
            y: (tray_rect.top + tray_rect.bottom) / 2,
        };
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO::default();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut work_area = RECT::default();
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            work_area = info.rcWork;
        }

        let mut x = tray_rect.right - width;
        let mut y = tray_rect.top - height - ANCHOR_GAP;

        if y < work_area.top {
            y = tray_rect.bottom + ANCHOR_GAP;
        }
        if y + height > work_area.bottom {
            y = (tray_rect.top - height - ANCHOR_GAP).max(work_area.top);
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

fn append_menus(
    lines: &mut Vec<Line>,
    menu: &TodayMenu,
    provider: Provider,
    show_prices: bool,
    price_groups: PriceGroups,
    show_allergens: bool,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
    hide_expensive_student_meals: bool,
) {
    for group in &menu.menus {
        if provider == Provider::Compass && hide_expensive_student_meals {
            if let Some(price) = student_price_eur(&group.price) {
                if price > 4.0 {
                    continue;
                }
            }
        }

        let heading = menu_heading(group, provider, show_prices, price_groups);
        lines.push(Line::Heading(heading));
        for component in &group.components {
            let component = normalize_text(component);
            if component.is_empty() {
                continue;
            }
            let (main, suffix) = split_component_suffix(&component);
            if main.is_empty() {
                continue;
            }
            if !show_allergens || suffix.is_empty() {
                lines.push(Line::MenuItem {
                    main,
                    suffix_segments: Vec::new(),
                });
            } else {
                let segments = build_suffix_segments(
                    &suffix,
                    highlight_gluten_free,
                    highlight_veg,
                    highlight_lactose_free,
                );
                lines.push(Line::MenuItem {
                    main,
                    suffix_segments: segments,
                });
            }
        }
    }
}

fn build_suffix_segments(
    suffix: &str,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
) -> Vec<(String, bool)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut token_mode = false;

    let push_token = |token: &str, out: &mut Vec<(String, bool)>| {
        if token.is_empty() {
            return;
        }
        let upper = token.to_uppercase();
        let highlight = (upper == "G" && highlight_gluten_free)
            || (upper == "VEG" && highlight_veg)
            || (upper == "L" && highlight_lactose_free);
        out.push((token.to_string(), highlight));
    };

    for ch in suffix.chars() {
        if ch.is_alphabetic() {
            if !token_mode {
                if !current.is_empty() {
                    segments.push((current.clone(), false));
                    current.clear();
                }
                token_mode = true;
            }
            current.push(ch);
        } else {
            if token_mode {
                push_token(&current, &mut segments);
                current.clear();
                token_mode = false;
            }
            current.push(ch);
        }
    }

    if !current.is_empty() {
        if token_mode {
            push_token(&current, &mut segments);
        } else {
            segments.push((current, false));
        }
    }

    segments
}

fn selection_state() -> &'static Mutex<PopupSelectionState> {
    POPUP_SELECTION_STATE.get_or_init(|| Mutex::new(PopupSelectionState::default()))
}

fn clear_selection_layout(hwnd: HWND) {
    if let Ok(mut state) = selection_state().lock() {
        if state
            .layout
            .as_ref()
            .is_some_and(|layout| layout.hwnd == hwnd)
        {
            state.layout = None;
        }
        state.drag = None;
    }
}

fn clear_selection_state(hwnd: HWND) {
    clear_selection_layout(hwnd);
}

fn store_selection_layout(layout: SelectableLayout) {
    if let Ok(mut state) = selection_state().lock() {
        if let Some(ref existing_drag) = state.drag {
            let keep_drag = state
                .layout
                .as_ref()
                .is_some_and(|old| old.hwnd == layout.hwnd)
                && state
                    .layout
                    .as_ref()
                    .is_some_and(|old| old.items.get(existing_drag.item_id).is_some())
                && layout.items.get(existing_drag.item_id).is_some();
            if !keep_drag {
                state.drag = None;
            }
        }
        state.layout = Some(layout);
    }
}

fn current_selection_range(hwnd: HWND) -> Option<SelectionRange> {
    let state = selection_state().lock().ok()?;
    let layout = state.layout.as_ref()?;
    if layout.hwnd != hwnd {
        return None;
    }
    let drag = state.drag.as_ref()?;
    let item = layout.items.get(drag.item_id)?;
    let (mut start, mut end) = selected_range(drag.anchor, drag.current);
    start = start.min(item.len());
    end = end.min(item.len());
    if start >= end {
        return None;
    }
    Some(SelectionRange {
        item_id: drag.item_id,
        start,
        end,
    })
}

fn hit_test_row(layout: &SelectableLayout, x: i32, y: i32) -> Option<(&SelectableRow, usize)> {
    let row = layout
        .rows
        .iter()
        .find(|row| y >= row.top && y <= row.bottom)?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn hit_test_row_for_item(
    layout: &SelectableLayout,
    item_id: usize,
    x: i32,
    y: i32,
) -> Option<(&SelectableRow, usize)> {
    let mut nearest_row: Option<&SelectableRow> = None;
    let mut nearest_distance = i32::MAX;

    for row in &layout.rows {
        if row.item_id != item_id {
            continue;
        }

        if y >= row.top && y <= row.bottom {
            let local = row_byte_index_from_x(row, x);
            return Some((row, row.start + local));
        }

        let distance = if y < row.top {
            row.top - y
        } else {
            y - row.bottom
        };
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest_row = Some(row);
        }
    }

    let row = nearest_row?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn row_byte_index_from_x(row: &SelectableRow, x: i32) -> usize {
    if row.boundaries.is_empty() {
        return 0;
    }
    let rel_x = (x - row.left).max(0);
    let mut selected = 0usize;
    for boundary in &row.boundaries {
        if boundary.x_offset <= rel_x {
            selected = boundary.byte_index;
        } else {
            break;
        }
    }
    selected.min(row.end.saturating_sub(row.start))
}

fn selected_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn current_favorites_snapshot() -> FavoritesSnapshot {
    let now = now_epoch_ms();
    let cache_lock = FAVORITES_CACHE.get_or_init(|| Mutex::new(FavoritesCache::default()));
    let mut cache = match cache_lock.lock() {
        Ok(value) => value,
        Err(_) => return FavoritesSnapshot::default(),
    };
    if cache.loaded && now < cache.next_check_epoch_ms {
        return cache.snapshot.clone();
    }

    let mtime = favorites::favorites_mtime_ms().unwrap_or(-1);
    if !cache.loaded || mtime != cache.mtime_ms {
        let loaded = favorites::load_favorites();
        let mut snippets_lower = Vec::new();
        for snippet in loaded.snippets {
            let normalized = favorites::normalize_snippet(&snippet);
            if normalized.is_empty() {
                continue;
            }
            snippets_lower.push(normalized.to_lowercase());
        }
        cache.snapshot = FavoritesSnapshot {
            snippets_lower: snippets_lower.into(),
        };
        cache.mtime_ms = mtime;
        cache.loaded = true;
    }
    cache.next_check_epoch_ms = now + FAVORITES_RELOAD_INTERVAL_MS;
    cache.snapshot.clone()
}

fn invalidate_favorites_cache() {
    let cache_lock = FAVORITES_CACHE.get_or_init(|| Mutex::new(FavoritesCache::default()));
    if let Ok(mut cache) = cache_lock.lock() {
        cache.loaded = false;
        cache.next_check_epoch_ms = 0;
        cache.mtime_ms = -1;
    }
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn point_in_rect(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn lerp_color(from: COLORREF, to: COLORREF, t: f32) -> COLORREF {
    let p = t.clamp(0.0, 1.0);
    let (fr, fg, fb) = color_channels(from);
    let (tr, tg, tb) = color_channels(to);
    let r = fr as f32 + (tr as f32 - fr as f32) * p;
    let g = fg as f32 + (tg as f32 - fg as f32) * p;
    let b = fb as f32 + (tb as f32 - fb as f32) * p;
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn color_channels(color: COLORREF) -> (u8, u8, u8) {
    let value = color.0;
    let r = (value & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = ((value >> 16) & 0xFF) as u8;
    (r, g, b)
}

#[derive(Debug, Clone, Copy)]
struct ThemePalette {
    bg_color: COLORREF,
    body_text_color: COLORREF,
    heading_color: COLORREF,
    header_title_color: COLORREF,
    suffix_color: COLORREF,
    suffix_highlight_color: COLORREF,
    favorite_highlight_color: COLORREF,
    selection_bg_color: COLORREF,
    header_bg_color: COLORREF,
    button_bg_color: COLORREF,
    divider_color: COLORREF,
}

fn theme_palette(theme: &str) -> ThemePalette {
    match theme {
        "light" => ThemePalette {
            bg_color: COLORREF(0x00FFFFFF),
            body_text_color: COLORREF(0x00000000),
            heading_color: COLORREF(0x00000000),
            header_title_color: COLORREF(0x00000000),
            suffix_color: COLORREF(0x00808080),
            suffix_highlight_color: COLORREF(0x00808080),
            favorite_highlight_color: COLORREF(0x00996600),
            selection_bg_color: COLORREF(0x00CDEBFF),
            header_bg_color: COLORREF(0x00F3F3F3),
            button_bg_color: COLORREF(0x00DDDDDD),
            divider_color: COLORREF(0x00C9C9C9),
        },
        "blue" => ThemePalette {
            bg_color: COLORREF(0x00562401),
            body_text_color: COLORREF(0x00FFFFFF),
            heading_color: COLORREF(0x00FFFFFF),
            header_title_color: COLORREF(0x00FFFFFF),
            suffix_color: COLORREF(0x00E7C7A7),
            suffix_highlight_color: COLORREF(0x00E7C7A7),
            favorite_highlight_color: COLORREF(0x0000D6FF),
            selection_bg_color: COLORREF(0x003E2B1A),
            header_bg_color: COLORREF(0x00733809),
            button_bg_color: COLORREF(0x00804A1A),
            divider_color: COLORREF(0x00834D1F),
        },
        "green" => ThemePalette {
            bg_color: COLORREF(0x00000000),
            body_text_color: COLORREF(0x0000D000),
            heading_color: COLORREF(0x0000D000),
            header_title_color: COLORREF(0x0000D000),
            suffix_color: COLORREF(0x00009000),
            suffix_highlight_color: COLORREF(0x0000D000),
            favorite_highlight_color: COLORREF(0x0000FFFF),
            selection_bg_color: COLORREF(0x001A2F1A),
            header_bg_color: COLORREF(0x000B1A0B),
            button_bg_color: COLORREF(0x00142D14),
            divider_color: COLORREF(0x00142D14),
        },
        "teletext1" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(255, 255, 255),
            heading_color: rgb(0, 255, 255),
            header_title_color: rgb(255, 255, 0),
            suffix_color: rgb(0, 255, 0),
            suffix_highlight_color: rgb(255, 0, 255),
            favorite_highlight_color: rgb(255, 255, 0),
            selection_bg_color: rgb(0, 0, 180),
            header_bg_color: rgb(0, 0, 180),
            button_bg_color: rgb(0, 0, 140),
            divider_color: rgb(255, 0, 0),
        },
        "teletext2" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(225, 255, 225),
            heading_color: rgb(255, 0, 255),
            header_title_color: rgb(0, 96, 255),
            suffix_color: rgb(0, 255, 150),
            suffix_highlight_color: rgb(255, 255, 0),
            favorite_highlight_color: rgb(0, 255, 255),
            selection_bg_color: rgb(0, 96, 255),
            header_bg_color: rgb(0, 215, 0),
            button_bg_color: rgb(0, 145, 0),
            divider_color: rgb(255, 0, 255),
        },
        _ => ThemePalette {
            bg_color: COLORREF(0x00000000),
            body_text_color: COLORREF(0x00FFFFFF),
            heading_color: COLORREF(0x00FFFFFF),
            header_title_color: COLORREF(0x00FFFFFF),
            suffix_color: COLORREF(0x00B0B0B0),
            suffix_highlight_color: COLORREF(0x00B0B0B0),
            favorite_highlight_color: COLORREF(0x0000D6FF),
            selection_bg_color: COLORREF(0x00303030),
            header_bg_color: COLORREF(0x00101010),
            button_bg_color: COLORREF(0x00202020),
            divider_color: COLORREF(0x00202020),
        },
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

fn theme_font_family(theme: &str) -> &'static str {
    match theme {
        "teletext1" | "teletext2" => "Consolas",
        _ => "Segoe UI",
    }
}

fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

#[allow(non_snake_case)]
fn MulDiv(n_number: i32, n_numerator: i32, n_denominator: i32) -> i32 {
    ((n_number as i64 * n_numerator as i64) / n_denominator as i64) as i32
}
