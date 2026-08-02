//! Popup subsystem facade.
//!
//! The public functions in this module are the stable entry points used by the
//! window procedure. Rendering, layout, animation, and interaction details live
//! in smaller submodules behind this facade.

use crate::api;
use crate::favorites;
use crate::format::{
    date_and_time_parts, menu_group_title_for_restaurant, menu_heading_for_restaurant,
    menu_price_for_restaurant_display, normalize_text, price_values_for_sort,
    split_component_suffix, text_for, PriceGroups,
};
use crate::model::{MenuGroup, MenuGroupPresentation, RecipeInfo, TodayMenu};
use crate::restaurant::{available_restaurants, Provider, Restaurant};
use crate::settings::Settings;
use crate::state::{AppState, FetchStatus};
use crate::util::to_wstring;
use std::cmp::{max, min};
use std::sync::{Arc, Mutex, OnceLock};
use time::OffsetDateTime;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, EndPaint, FillRect, GetDeviceCaps, GetMonitorInfoW,
    GetTextExtentPoint32W, GetTextMetricsW, IntersectClipRect, InvalidateRect, MonitorFromPoint,
    RestoreDC, SaveDC, SelectObject, SetBkMode, SetTextColor, TextOutW, HDC, HFONT, LOGPIXELSY,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect, KillTimer, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_STYLE, HWND_TOPMOST, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WS_CAPTION,
    WS_THICKFRAME,
};

const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 10;
const LINE_GAP: i32 = 2;
const ANCHOR_GAP: i32 = 0;
const POPUP_MAX_WIDTH: i32 = 525;
const POPUP_MIN_WIDTH: i32 = 320;
const HEADER_HEIGHT: i32 = 44;
const HEADER_BUTTON_SIZE: i32 = 26;
const HEADER_BUTTON_GAP: i32 = 8;
const LOADING_HINT_DELAY_MS: i64 = 250;
const MAX_DYNAMIC_LINES: usize = 35;
const POPUP_ANIM_INTERVAL_MS: u32 = 8;
const POPUP_HEADER_PRESS_MS: i64 = 90;
const POPUP_OPEN_ANIM_MS: i64 = 120;
const POPUP_CLOSE_ANIM_MS: i64 = 90;
const POPUP_SWITCH_ANIM_MS: i64 = 120;
const POPUP_INTERRUPTED_SWITCH_ANIM_MS: i64 = 80;
const POPUP_SWITCH_OFFSET_PX: i32 = 6;
/// Switches landing on top of each other before this many have stacked up scale
/// the header title's dither from none to full.
const POPUP_SWITCH_TURBULENCE_SATURATION: f32 = 4.0;
/// Densest a trail marker may be drawn. Held well below solid so the trail always
/// reads as a ghost of the active marker rather than competing with it for which
/// restaurant you are actually on.
const POPUP_MARKER_TRAIL_MAX_DITHER: f32 = 0.45;
/// Longest comet trail the rail will draw. Kept short deliberately: two or three
/// ghosts are enough to read as direction and speed, and every one past that is
/// just more of the rail lit at once.
const POPUP_MARKER_TRAIL_MAX_LEN: i32 = 3;
/// The most of the rail the trail may occupy, as one part in this many. Without
/// it a long spin lights a fixed number of markers regardless of how many there
/// are, so the same trail that reads as a highlight on a long rail swallows a
/// short one.
const POPUP_MARKER_TRAIL_RAIL_FRACTION: i32 = 3;
const HEADER_MARKER_DOT_SIZE: i32 = 5;
const HEADER_MARKER_GAP: i32 = 8;
const HEADER_MARKER_HIT_SIZE: i32 = 16;
/// Distance from the header's bottom edge to the top of the marker rail. Sets
/// how much room is left above the rail for the title to sit on the button
/// midline instead of being pushed up off it.
const HEADER_MARKER_BOTTOM_GAP: i32 = 9;
const FAVORITES_RELOAD_INTERVAL_MS: i64 = 1000;
const POPUP_DESIRED_SIZE_CACHE_LIMIT: usize = 32;
const HEADER_TITLE_BUTTON_MARGIN: i32 = 12;
const RECIPE_DETAIL_PAD_X: i32 = 8;
const RECIPE_DETAIL_PAD_Y: i32 = 5;
const RECIPE_DETAIL_ROW_GAP: i32 = 2;
const RECIPE_DETAIL_MARGIN_Y: i32 = 3;
const RECIPE_DETAIL_MAX_VISIBLE_ROWS: usize = 14;
const RECIPE_DETAIL_SCROLLBAR_WIDTH: i32 = 5;
const RECIPE_DETAIL_WHEEL_ROWS: i32 = 3;
const NOTICE_PAD_X: i32 = 8;
const NOTICE_PAD_Y: i32 = 5;
const NOTICE_MARGIN_Y: i32 = 3;
const BASE_DPI: i32 = 96;

static POPUP_LINE_BUDGET_CACHE: OnceLock<Mutex<Option<PopupLineBudgetCache>>> = OnceLock::new();
static POPUP_LINE_SIGNATURE_CACHE: OnceLock<Mutex<Option<PopupLineSignatureCache>>> =
    OnceLock::new();
static POPUP_DESIRED_SIZE_CACHE: OnceLock<Mutex<Vec<PopupDesiredSizeCacheEntry>>> = OnceLock::new();
static POPUP_ANIMATION: OnceLock<Mutex<Option<PopupAnimation>>> = OnceLock::new();
static FAVORITES_CACHE: OnceLock<Mutex<FavoritesCache>> = OnceLock::new();
static POPUP_SELECTION_STATE: OnceLock<Mutex<PopupSelectionState>> = OnceLock::new();
static POPUP_HEADER_PRESS: OnceLock<Mutex<Option<HeaderButtonPress>>> = OnceLock::new();
static POPUP_HEADER_HOVER: OnceLock<Mutex<Option<HeaderButtonHover>>> = OnceLock::new();

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
    show_price_group_names: bool,
    lunch_item_display_mode: crate::settings::LunchItemDisplayMode,
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
    max_extra_height_px: Option<i32>,
}

#[derive(Debug, Clone)]
struct PopupLineSignatureCache {
    key: PopupLineBudgetKey,
    signatures: Vec<RestaurantCacheSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupDesiredSizeKey {
    today_key: String,
    restaurant_code: String,
    status: FetchStatus,
    error_message: String,
    stale_network_error: bool,
    stale_date: bool,
    expanded_recipe_key: Option<RecipeExpansionKey>,
    enable_antell_restaurants: bool,
    /// Index numbers append " (n/total)" to every title, which widens the
    /// header and so the popup. Without it in the key, sizes cached before the
    /// setting was toggled are served alongside freshly computed ones and the
    /// popup resizes as the user scrolls between restaurants.
    show_restaurant_index_numbers: bool,
    language: String,
    theme: String,
    widget_scale: String,
    dpi_y: i32,
    show_prices: bool,
    show_student_price: bool,
    show_staff_price: bool,
    show_guest_price: bool,
    show_price_group_names: bool,
    lunch_item_display_mode: crate::settings::LunchItemDisplayMode,
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
    DateTime {
        date: String,
        hours: String,
        hours_status: HoursStatus,
        stale: bool,
    },
    Subheading {
        text: String,
        reserve_prefix: Option<String>,
    },
    Text(String),
    StatusText(String),
    StaleNotice(String),
    ClosureNotice(String),
    MenuItem {
        show_bullet: bool,
        price_prefix: Option<String>,
        reserve_prefix: Option<String>,
        main: String,
        suffix_segments: Vec<(String, bool)>,
        recipe_key: Option<RecipeExpansionKey>,
        ingredient_alert: bool,
    },
    RecipeDetail {
        rows: Vec<RecipeDetailRow>,
    },
    Spacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoursStatus {
    Unknown,
    Open,
    ClosingSoon,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecipeExpansionKey {
    recipe_id: u32,
    instance_id: usize,
}

#[derive(Debug, Clone)]
struct RecipeDetailRow {
    label: String,
    value: String,
    selectable: bool,
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
    item_recipe_keys: Vec<Option<RecipeExpansionKey>>,
    item_ingredient_flags: Vec<bool>,
    rows: Vec<SelectableRow>,
    recipe_scroll_rect: Option<RECT>,
    recipe_scroll_max_offset_px: i32,
    recipe_scroll_line_height: i32,
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
    start_x: i32,
    start_y: i32,
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
    expanded_recipe_key: Option<RecipeExpansionKey>,
    recipe_scroll_offset_px: i32,
}

#[derive(Debug, Clone, Default)]
struct FavoritesSnapshot {
    snippets_lower: Vec<String>,
    ingredient_snippets_lower: Vec<String>,
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
        title: String,
    },
    Close {
        lines: Arc<Vec<Line>>,
        title: String,
    },
    Switch {
        old_lines: Arc<Vec<Line>>,
        new_lines: Arc<Vec<Line>>,
        old_title: String,
        new_title: String,
        direction: i32,
        interrupted: bool,
        turbulence: f32,
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
        title: String,
        progress: f32,
    },
    Close {
        lines: Arc<Vec<Line>>,
        title: String,
        progress: f32,
    },
    Switch {
        old_lines: Arc<Vec<Line>>,
        new_lines: Arc<Vec<Line>>,
        old_title: String,
        new_title: String,
        direction: i32,
        progress: f32,
        interrupted: bool,
        turbulence: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Action associated with a header button hit-test.
pub enum HeaderButtonAction {
    Prev,
    Next,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Cursor affordance for the popup point under the mouse.
pub enum PopupCursorKind {
    Arrow,
    Hand,
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

#[derive(Debug, Clone, Copy)]
struct HeaderButtonHover {
    hwnd: HWND,
    action: HeaderButtonAction,
}

mod animation;
mod border;
mod bullet;
mod content;
mod dither;
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

/// Clears transient text selection and expanded recipe details for this popup.
pub fn clear_interaction_state(hwnd: HWND) {
    interaction::clear_selection_state(hwnd);
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

/// Returns the restaurant marker index under the given client-space point.
pub fn header_marker_index_at(hwnd: HWND, settings: &Settings, x: i32, y: i32) -> Option<usize> {
    interaction::header_marker_index_at(hwnd, settings, x, y)
}

/// Updates the currently hovered header button and returns true when it changed.
pub fn update_hovered_header_button(hwnd: HWND, action: Option<HeaderButtonAction>) -> bool {
    animation::update_hovered_header_button(hwnd, action)
}

/// Returns the cursor affordance for the given client-space popup point.
pub fn cursor_kind_at(hwnd: HWND, settings: &Settings, x: i32, y: i32) -> PopupCursorKind {
    if header_button_at(hwnd, settings, x, y).is_some() {
        return PopupCursorKind::Hand;
    }
    if header_marker_index_at(hwnd, settings, x, y).is_some() {
        return PopupCursorKind::Hand;
    }
    interaction::content_cursor_kind_at(hwnd, x, y).unwrap_or(PopupCursorKind::Arrow)
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

/// Scrolls a capped recipe detail block at the given client-space point.
pub fn scroll_recipe_detail_at(hwnd: HWND, x: i32, y: i32, delta: i32) -> bool {
    interaction::scroll_recipe_detail_at(hwnd, x, y, delta)
}

#[cfg(feature = "bench")]
pub fn bench_build_line_count(state: &AppState) -> usize {
    content::build_lines(state).len()
}

#[cfg(feature = "bench")]
pub fn bench_favorite_match_range_count(text: &str, snippets_lower: &[String]) -> usize {
    render::bench_favorite_match_range_count(text, snippets_lower)
}

/// Collapses the expanded recipe detail block when the point is inside it.
pub fn collapse_recipe_detail_at(hwnd: HWND, x: i32, y: i32) -> bool {
    interaction::collapse_recipe_detail_at(hwnd, x, y)
}

/// Paint entry point used by the popup window procedure.
pub fn paint_popup(hwnd: HWND, state: &AppState) {
    render::paint_popup(hwnd, state);
}

pub fn request_repaint(hwnd: HWND) {
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
