//! Central application state and orchestration for the Windows tray app.
//!
//! This module owns persisted settings, in-memory menu state, fetch coordination,
//! update checks, and side effects such as opening external URLs.

mod actions;
mod fetch;
mod update_check;

use crate::api::{self, FetchContext, FetchMode, FetchOutput, FetchReason};
use crate::cache;
use crate::log::{log_line, set_enabled as set_log_enabled};
use crate::model::TodayMenu;
use crate::restaurant::{
    available_restaurants, effective_fetch_language, provider_key, restaurant_for_code,
    restaurant_for_shortcut_index, Provider, Restaurant,
};
use crate::settings::{
    load_settings, normalize_theme, normalize_widget_scale, save_settings, settings_dir, Settings,
};
use crate::state::{AppState, FetchStatus};
use crate::update;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
#[derive(Default, Clone, Copy)]
struct WindowHandles {
    tray: HWND,
    popup: HWND,
}

#[derive(Debug, Clone)]
struct MemoryMenuEntry {
    ok: bool,
    error_message: String,
    today_menu: Option<TodayMenu>,
    restaurant_name: String,
    restaurant_url: String,
    provider: Provider,
    raw_payload: String,
    payload_date: String,
    api_stale: bool,
}

#[derive(Debug, Clone, Default)]
struct RequestState {
    in_flight: bool,
    last_attempt_epoch_ms: i64,
    last_success_epoch_ms: i64,
    last_failure_epoch_ms: i64,
    cooldown_until_epoch_ms: i64,
    consecutive_failures: usize,
    last_reason: Option<FetchReason>,
}

#[derive(Debug, Clone)]
struct FetchTarget {
    restaurant: Restaurant,
    ui_language: String,
    effective_language: String,
    key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshNeed {
    MissingCache,
    StaleDate,
    ApiStale,
    RefreshIntervalElapsed,
}

#[derive(Debug, Clone, Copy)]
struct RefreshNeedReasons {
    missing: FetchReason,
    stale: FetchReason,
    interval: FetchReason,
}

#[derive(Debug, Clone, Copy)]
struct StartOptions {
    mark_loading_when_empty: bool,
    bypass_cooldown: bool,
}

const STALE_NO_MENU_COOLDOWN_MS: u32 = 10 * 60_000;

/// Top-level application object shared by the tray and popup windows.
pub struct App {
    state: Arc<Mutex<AppState>>,
    hwnds: Mutex<WindowHandles>,
    request_states: Mutex<HashMap<String, RequestState>>,
    last_prefetch_ms: Mutex<i64>,
    snapshot_fetch_in_flight: Arc<Mutex<bool>>,
    memory_menu_cache: Mutex<HashMap<String, MemoryMenuEntry>>,
    update_check_in_flight: Arc<Mutex<bool>>,
}

/// Message posted back to the UI thread when a menu fetch finishes.
pub struct FetchMessage {
    pub requested_code: String,
    pub requested_language: String,
    pub requested_effective_language: String,
    pub request_key: String,
    pub context: FetchContext,
    pub result: FetchOutput,
}

/// Message posted back to the UI thread when an update check finishes.
pub struct UpdateCheckMessage {
    pub outcome: UpdateCheckOutcome,
}

#[derive(Debug, Clone)]
/// Result of comparing the running app version against the latest published release.
pub enum UpdateCheckOutcome {
    LatestPublished {
        current_version: String,
        release_url: String,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        release_url: String,
    },
    NewerThanLatestPublished {
        current_version: String,
        latest_version: String,
        releases_url: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Outcome of applying a completed fetch to the current application state.
pub enum FetchApplyOutcome {
    CurrentSuccess,
    CurrentStale,
    CurrentFailure,
    BackgroundSuccess,
    BackgroundFailure,
}

impl App {
    /// Creates a new application state container with persisted settings loaded.
    pub fn new() -> Self {
        let settings = load_settings();
        set_log_enabled(settings.enable_logging);
        let state = AppState {
            provider: restaurant_for_code(
                &settings.restaurant_code,
                settings.enable_antell_restaurants,
            )
            .provider,
            settings,
            status: FetchStatus::Idle,
            loading_started_epoch_ms: 0,
            error_message: String::new(),
            stale_network_error: false,
            today_menu: None,
            restaurant_name: String::new(),
            restaurant_url: String::new(),
            raw_payload: String::new(),
            payload_date: String::new(),
            stale_date: false,
            api_stale: false,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            hwnds: Mutex::new(WindowHandles::default()),
            request_states: Mutex::new(HashMap::new()),
            last_prefetch_ms: Mutex::new(0),
            snapshot_fetch_in_flight: Arc::new(Mutex::new(false)),
            memory_menu_cache: Mutex::new(HashMap::new()),
            update_check_in_flight: Arc::new(Mutex::new(false)),
        }
    }

    /// Stores the tray and popup window handles after window creation.
    pub fn set_hwnds(&self, tray: HWND, popup: HWND) {
        let mut hwnds = self.hwnds.lock().unwrap();
        hwnds.tray = tray;
        hwnds.popup = popup;
    }

    /// Returns the hidden tray message window handle.
    pub fn hwnd_tray(&self) -> HWND {
        self.hwnds.lock().unwrap().tray
    }

    /// Returns the popup window handle.
    pub fn hwnd_popup(&self) -> HWND {
        self.hwnds.lock().unwrap().popup
    }

    /// Returns a cloned snapshot of the current UI-visible state.
    pub fn snapshot(&self) -> AppState {
        let state = self.state.lock().unwrap();
        crate::perf::count_snapshot_clone(
            estimated_app_state_clone_bytes(&state),
            estimated_app_state_clone_strings(&state),
        );
        state.clone()
    }

    /// Returns a cloned settings snapshot without cloning the current menu payload.
    pub fn settings_snapshot(&self) -> Settings {
        self.state.lock().unwrap().settings.clone()
    }
}

fn estimated_app_state_clone_bytes(state: &AppState) -> usize {
    let mut total = state.error_message.len()
        + state.restaurant_name.len()
        + state.restaurant_url.len()
        + state.raw_payload.len()
        + state.payload_date.len()
        + state.settings.restaurant_code.len()
        + state.settings.language.len()
        + state.settings.theme.len()
        + state.settings.widget_scale.len();
    if let Some(menu) = &state.today_menu {
        total += menu.date_iso.len() + menu.lunch_time.len();
        for group in &menu.menus {
            total += group.name.len() + group.price.len();
            total += group.components.iter().map(String::len).sum::<usize>();
            total += group
                .prices
                .iter()
                .map(|price| price.amount.len())
                .sum::<usize>();
            for detail in group.component_recipe_details.iter().flatten() {
                total += detail.name.len() + detail.ingredients_cleaned.len() + detail.diets.len();
                total += detail
                    .nutritional_values
                    .iter()
                    .map(|value| value.name.len() + value.unit.len())
                    .sum::<usize>();
            }
        }
    }
    total
}

fn estimated_app_state_clone_strings(state: &AppState) -> usize {
    let mut total = 9usize;
    if let Some(menu) = &state.today_menu {
        total += 2;
        for group in &menu.menus {
            total += 2 + group.components.len() + group.prices.len();
            for detail in group.component_recipe_details.iter().flatten() {
                total += 3 + detail.nutritional_values.len() * 2;
            }
        }
    }
    total
}

/// Returns the current local epoch timestamp in milliseconds.
pub fn now_epoch_ms() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn menu_cache_key(code: &str, language: &str) -> String {
    format!("{}|{}", language, code)
}

fn request_state_key(code: &str, effective_language: &str) -> String {
    format!("{}|{}", code, effective_language)
}

fn fetch_target_for_code(settings: &Settings, code: &str, enable_antell: bool) -> FetchTarget {
    let restaurant = restaurant_for_code(code, enable_antell);
    let effective_language = effective_fetch_language(restaurant, &settings.language);
    FetchTarget {
        restaurant,
        ui_language: settings.language.clone(),
        key: request_state_key(code, &effective_language),
        effective_language,
    }
}

fn fetch_target_for_values(code: &str, language: &str, enable_antell: bool) -> FetchTarget {
    let restaurant = restaurant_for_code(code, enable_antell);
    let effective_language = effective_fetch_language(restaurant, language);
    FetchTarget {
        restaurant,
        ui_language: language.to_string(),
        key: request_state_key(code, &effective_language),
        effective_language,
    }
}

fn today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn update_stale_date(state: &mut AppState) {
    if !state.payload_date.is_empty() {
        state.stale_date = state.payload_date != today_key();
    } else {
        state.stale_date = false;
    }
}

fn date_key_from_epoch_ms(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    let secs = ms / 1000;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    let mut dt = OffsetDateTime::from_unix_timestamp(secs).ok()?;
    dt = dt.replace_nanosecond(nanos).ok()?;
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let local = dt.to_offset(offset);
    let date = local.date();
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    ))
}

fn is_probable_network_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "timed out",
        "timeout",
        "dns",
        "network",
        "connection",
        "connect",
        "host",
        "resolve",
        "name or service not known",
        "temporary failure",
        "connection reset",
        "connection refused",
        "tls",
        "certificate",
        "os error",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn refresh_need_for_target(
    target: &FetchTarget,
    refresh_minutes: u32,
    payload_date: &str,
    has_payload: bool,
    api_stale: bool,
    now_ms: i64,
) -> Option<RefreshNeed> {
    if !has_payload {
        return Some(RefreshNeed::MissingCache);
    }

    let today = today_key();
    if !payload_date.is_empty() && payload_date != today {
        return Some(RefreshNeed::StaleDate);
    }
    if api_stale {
        return Some(RefreshNeed::ApiStale);
    }

    if refresh_minutes == 0 {
        return None;
    }

    let cache_age_ms = cache::cache_mtime_ms(
        target.restaurant.provider,
        target.restaurant.code,
        &target.ui_language,
    )
    .map(|mtime| now_ms.saturating_sub(mtime));

    match cache_age_ms {
        None => Some(RefreshNeed::MissingCache),
        Some(age_ms) if age_ms >= (refresh_minutes as i64) * 60_000 => {
            Some(RefreshNeed::RefreshIntervalElapsed)
        }
        _ => None,
    }
}

fn retry_delay_ms_for_failures(consecutive_failures: usize) -> u32 {
    match consecutive_failures {
        0 | 1 => 10_000,
        2 => 30_000,
        3 => 60_000,
        _ => 5 * 60_000,
    }
}

fn log_fetch_probe(
    phase: &str,
    context: &FetchContext,
    target: &FetchTarget,
    decision: &str,
    detail: &str,
) {
    let detail = if detail.is_empty() {
        String::new()
    } else {
        format!(" detail={}", detail)
    };
    log_line(&format!(
        "fetch gate phase={} mode={} reason={} decision={} code={} provider={} ui_language={} fetch_language={}{}",
        phase,
        context.mode.as_str(),
        context.reason.as_str(),
        decision,
        target.restaurant.code,
        provider_key(target.restaurant.provider),
        target.ui_language,
        target.effective_language,
        detail,
    ));
}

fn log_probe_skip(trigger: &str, target: &FetchTarget, detail: &str) {
    log_line(&format!(
        "fetch probe trigger={} decision=skip code={} provider={} ui_language={} fetch_language={} detail={}",
        trigger,
        target.restaurant.code,
        provider_key(target.restaurant.provider),
        target.ui_language,
        target.effective_language,
        detail,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_state_key_uses_requested_api_language() {
        let target = fetch_target_for_values("caari", "en", true);
        assert_eq!(target.effective_language, "en");
        assert_eq!(target.key, "caari|en");
    }

    #[test]
    fn refresh_need_marks_missing_payload_as_missing_cache() {
        let target = fetch_target_for_values("0437", "fi", true);
        assert_eq!(
            refresh_need_for_target(&target, 1440, "", false, false, now_epoch_ms()),
            Some(RefreshNeed::MissingCache)
        );
    }

    #[test]
    fn refresh_need_marks_old_payload_date_as_stale() {
        let target = fetch_target_for_values("0437", "fi", true);
        assert_eq!(
            refresh_need_for_target(&target, 1440, "2001-01-01", true, false, now_epoch_ms()),
            Some(RefreshNeed::StaleDate)
        );
    }

    #[test]
    fn refresh_need_marks_old_api_generation_as_stale() {
        let target = fetch_target_for_values("0437", "fi", true);
        assert_eq!(
            refresh_need_for_target(&target, 1440, &today_key(), true, true, now_epoch_ms()),
            Some(RefreshNeed::ApiStale)
        );
    }

    #[test]
    fn retry_delay_caps_after_repeated_failures() {
        assert_eq!(retry_delay_ms_for_failures(1), 10_000);
        assert_eq!(retry_delay_ms_for_failures(2), 30_000);
        assert_eq!(retry_delay_ms_for_failures(3), 60_000);
        assert_eq!(retry_delay_ms_for_failures(4), 300_000);
        assert_eq!(retry_delay_ms_for_failures(10), 300_000);
    }

    #[cfg(feature = "perf-counters")]
    #[test]
    fn settings_snapshot_does_not_count_full_state_clone() {
        let app = App::new();
        {
            let mut state = app.state.lock().unwrap();
            state.raw_payload = "x".repeat(16_384);
        }

        crate::perf::reset();
        let _ = app.settings_snapshot();
        let counters = crate::perf::snapshot();
        assert_eq!(counters.snapshot_cloned_bytes, 0);
        assert_eq!(counters.snapshot_cloned_strings, 0);

        let _ = app.snapshot();
        let counters = crate::perf::snapshot();
        assert!(counters.snapshot_cloned_bytes >= 16_384);
        assert!(counters.snapshot_cloned_strings > 0);
    }
}
