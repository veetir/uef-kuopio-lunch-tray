//! Popup subsystem facade.
//!
//! The public functions in this module are the stable entry points used by the
//! window procedure. Rendering, layout, animation, and interaction details live
//! in smaller submodules behind this facade.

use crate::api;
use crate::app::{AppState, FetchStatus};
use crate::cache;
use crate::favorites;
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, renderable_menu_components,
    student_price_eur, text_for, PriceGroups,
};
use crate::model::TodayMenu;
use crate::restaurant::{available_restaurants, is_hard_closed_today, Provider, Restaurant};
use crate::settings::Settings;
use crate::util::to_wstring;
use std::cmp::{max, min};
use std::sync::{Mutex, OnceLock};
use time::{OffsetDateTime, UtcOffset};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, EndPaint, FillRect, GetDeviceCaps, GetMonitorInfoW,
    GetTextExtentPoint32W, GetTextMetricsW, InvalidateRect, MonitorFromPoint, SelectObject,
    SetBkMode, SetTextColor, TextOutW, HDC, HFONT, LOGPIXELSY, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, KillTimer, SetTimer, SetWindowPos, ShowWindow,
    HWND_TOPMOST, SWP_SHOWWINDOW, SW_HIDE,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 0;
const POPUP_MAX_WIDTH: i32 = 525;
const POPUP_MIN_WIDTH: i32 = 320;
const HEADER_HEIGHT: i32 = 46;
const HEADER_BUTTON_SIZE: i32 = 30;
const HEADER_BUTTON_GAP: i32 = 8;
const LOADING_HINT_DELAY_MS: i64 = 250;
const MAX_DYNAMIC_LINES: usize = 35;
const POPUP_ANIM_INTERVAL_MS: u32 = 33;
const POPUP_HEADER_PRESS_MS: i64 = 90;
const POPUP_OPEN_ANIM_MS: i64 = 120;
const POPUP_CLOSE_ANIM_MS: i64 = 90;
const POPUP_SWITCH_ANIM_MS: i64 = 120;
const POPUP_SWITCH_OFFSET_PX: i32 = 6;
const FAVORITES_RELOAD_INTERVAL_MS: i64 = 1000;
const POPUP_DESIRED_SIZE_CACHE_LIMIT: usize = 32;
const BULLET_PREFIX: &str = "▸ ";
const HEADER_TITLE_BUTTON_MARGIN: i32 = 12;

static POPUP_LINE_BUDGET_CACHE: OnceLock<Mutex<Option<PopupLineBudgetCache>>> = OnceLock::new();
static POPUP_LINE_SIGNATURE_CACHE: OnceLock<Mutex<Option<PopupLineSignatureCache>>> =
    OnceLock::new();
static POPUP_DESIRED_SIZE_CACHE: OnceLock<Mutex<Vec<PopupDesiredSizeCacheEntry>>> = OnceLock::new();
static POPUP_ANIMATION: OnceLock<Mutex<Option<PopupAnimation>>> = OnceLock::new();
static FAVORITES_CACHE: OnceLock<Mutex<FavoritesCache>> = OnceLock::new();
static POPUP_SELECTION_STATE: OnceLock<Mutex<PopupSelectionState>> = OnceLock::new();
static POPUP_HEADER_PRESS: OnceLock<Mutex<Option<HeaderButtonPress>>> = OnceLock::new();

pub const POPUP_ANIM_TIMER_ID: usize = 100;
pub const POPUP_HEADER_PRESS_TIMER_ID: usize = 101;

#[derive(Debug, Clone, Copy)]
struct PopupScale {
    factor: f32,
    padding_x: i32,
    padding_y: i32,
    line_gap: i32,
    anchor_gap: i32,
    max_width: i32,
    min_width: i32,
    max_content_width: i32,
    min_content_width: i32,
    header_height: i32,
    header_button_size: i32,
    header_button_gap: i32,
    switch_offset_px: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupLineBudgetKey {
    today_key: String,
    language: String,
    theme: String,
    widget_scale: String,
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
struct PopupLineSignatureCache {
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupDesiredSizeKey {
    today_key: String,
    enable_antell_restaurants: bool,
    language: String,
    theme: String,
    widget_scale: String,
    dpi_y: i32,
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

#[derive(Debug, Clone)]
struct PopupDesiredSizeCacheEntry {
    key: PopupDesiredSizeKey,
    width: i32,
    height: i32,
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
    snippets_lower: Vec<String>,
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
        lines: Vec<Line>,
        title: String,
    },
    Close {
        lines: Vec<Line>,
        title: String,
    },
    Switch {
        old_lines: Vec<Line>,
        new_lines: Vec<Line>,
        old_title: String,
        new_title: String,
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
        lines: Vec<Line>,
        title: String,
        progress: f32,
    },
    Close {
        lines: Vec<Line>,
        title: String,
        progress: f32,
    },
    Switch {
        old_lines: Vec<Line>,
        new_lines: Vec<Line>,
        old_title: String,
        new_title: String,
        direction: i32,
        progress: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Action associated with a header button hit-test.
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

#[derive(Debug, Clone, Copy)]
struct HeaderButtonPress {
    hwnd: HWND,
    action: HeaderButtonAction,
    until_epoch_ms: i64,
}

mod animation;
mod content;
mod interaction;
mod layout;
mod render;
mod theme;

/// Shows the popup near the current cursor location.
pub fn show_popup(hwnd: HWND, state: &AppState) {
    layout::show_popup(hwnd, state);
}

/// Shows the popup using an explicit screen-space anchor point.
pub fn show_popup_at(hwnd: HWND, state: &AppState, anchor: POINT) {
    layout::show_popup_at(hwnd, state, anchor);
}

/// Shows the popup anchored to the tray icon rectangle when available.
pub fn show_popup_for_tray_icon(hwnd: HWND, state: &AppState, tray_rect: RECT) {
    layout::show_popup_for_tray_icon(hwnd, state, tray_rect);
}

/// Recomputes popup size while keeping the current anchored position.
pub fn resize_popup_keep_position(hwnd: HWND, state: &AppState) {
    layout::resize_popup_keep_position(hwnd, state);
}

/// Clears cached layout budgets after a settings change that affects wrapping.
pub fn invalidate_layout_budget_cache() {
    layout::invalidate_layout_budget_cache();
}

/// Hides the popup immediately without animation.
pub fn hide_popup(hwnd: HWND) {
    layout::hide_popup(hwnd);
}

/// Starts the navigation button press feedback animation.
pub fn press_navigation_button(hwnd: HWND, direction: i32) {
    animation::press_navigation_button(hwnd, direction);
}

/// Advances the header button press feedback timer.
pub fn tick_header_button_press(hwnd: HWND) {
    animation::tick_header_button_press(hwnd);
}

/// Starts the popup close animation when animations are enabled.
pub fn begin_close_animation(hwnd: HWND, state: &AppState) {
    animation::begin_close_animation(hwnd, state);
}

/// Starts the restaurant-switch animation between two popup states.
pub fn begin_switch_animation(
    hwnd: HWND,
    old_state: &AppState,
    new_state: &AppState,
    direction: i32,
) {
    animation::begin_switch_animation(hwnd, old_state, new_state, direction);
}

/// Advances the popup animation timer and repaints as needed.
pub fn tick_animation(hwnd: HWND) {
    animation::tick_animation(hwnd);
}

/// Returns the header button under the given client-space point, if any.
pub fn header_button_at(
    hwnd: HWND,
    settings: &Settings,
    x: i32,
    y: i32,
) -> Option<HeaderButtonAction> {
    interaction::header_button_at(hwnd, settings, x, y)
}

/// Starts a text selection drag in the popup content area.
pub fn begin_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    interaction::begin_text_selection(hwnd, x, y)
}

/// Updates the active text selection drag.
pub fn update_text_selection(hwnd: HWND, x: i32, y: i32) {
    interaction::update_text_selection(hwnd, x, y);
}

/// Finishes the current text selection and copies it to the clipboard.
pub fn finish_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    interaction::finish_text_selection(hwnd, x, y)
}

/// Cancels any active text selection state for the popup.
pub fn cancel_text_selection(hwnd: HWND) {
    interaction::cancel_text_selection(hwnd);
}

/// Reports whether a text selection drag is currently active.
pub fn text_selection_active(hwnd: HWND) -> bool {
    interaction::text_selection_active(hwnd)
}

/// Paint entry point used by the popup window procedure.
pub fn paint_popup(hwnd: HWND, state: &AppState) {
    render::paint_popup(hwnd, state);
}

fn request_repaint(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, None, false);
    }
}

fn popup_animations_enabled(settings: &Settings) -> bool {
    settings.animations_enabled
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

fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

#[allow(non_snake_case)]
fn MulDiv(n_number: i32, n_numerator: i32, n_denominator: i32) -> i32 {
    ((n_number as i64 * n_numerator as i64) / n_denominator as i64) as i32
}
