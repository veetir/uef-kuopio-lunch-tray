//! Central application state and orchestration for the Windows tray app.
//!
//! This module owns persisted settings, in-memory menu state, fetch coordination,
//! update checks, and side effects such as opening external URLs.

use crate::api::{self, FetchContext, FetchMode, FetchOutput, FetchReason};
use crate::cache;
use crate::log::{log_line, set_enabled as set_log_enabled};
use crate::model::TodayMenu;
use crate::restaurant::{
    available_restaurants, effective_fetch_language, is_hard_closed_today, provider_key,
    restaurant_for_code, restaurant_for_shortcut_index, Provider, Restaurant,
};
use crate::settings::{
    load_settings, normalize_theme, normalize_widget_scale, save_settings, settings_dir, Settings,
};
use crate::update;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// High-level fetch status for the currently selected restaurant.
pub enum FetchStatus {
    Idle,
    Loading,
    Ok,
    Stale,
    Error,
}

#[derive(Debug, Clone)]
/// Snapshot of UI-visible application state consumed by popup and tray rendering.
pub struct AppState {
    pub settings: Settings,
    pub status: FetchStatus,
    pub loading_started_epoch_ms: i64,
    pub error_message: String,
    pub stale_network_error: bool,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub raw_payload: String,
    pub provider: Provider,
    pub payload_date: String,
    pub stale_date: bool,
}

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
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            hwnds: Mutex::new(WindowHandles::default()),
            request_states: Mutex::new(HashMap::new()),
            last_prefetch_ms: Mutex::new(0),
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
        self.state.lock().unwrap().clone()
    }

    /// Loads cached menu data for the currently selected restaurant, if available.
    pub fn load_cache_for_current(&self) -> bool {
        let (restaurant, language) = {
            let state = self.state.lock().unwrap();
            (
                restaurant_for_code(
                    &state.settings.restaurant_code,
                    state.settings.enable_antell_restaurants,
                ),
                state.settings.language.clone(),
            )
        };
        if is_hard_closed_today(restaurant) {
            let result = api::closed_today_fetch_output(restaurant, &language);
            self.apply_cached_result(&result);
            log_line(&format!(
                "closed-day synthetic state provider={} code={} language={}",
                provider_key(restaurant.provider),
                restaurant.code,
                language
            ));
            return true;
        }

        let cached_date = if restaurant.provider == Provider::Antell {
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &language)
                .and_then(date_key_from_epoch_ms)
        } else {
            None
        };

        if self.load_memory_for(
            restaurant.code,
            &language,
            restaurant.provider,
            cached_date.as_deref(),
        ) {
            log_line(&format!(
                "memory cache hit provider={} code={} language={}",
                provider_key(restaurant.provider),
                restaurant.code,
                language
            ));
            return true;
        }

        if let Some(raw) = cache::read_cache(restaurant.provider, restaurant.code, &language) {
            match api::parse_cached_payload(&raw, restaurant.provider, restaurant, &language) {
                Ok(result) => {
                    let mut result = result;
                    if let Some(date_key) = cached_date {
                        result.payload_date = date_key;
                    }
                    self.apply_cached_result(&result);
                    self.store_memory_from_fetch_output(restaurant.code, &language, &result);
                    log_line(&format!(
                        "cache hit provider={} code={} language={}",
                        provider_key(restaurant.provider),
                        restaurant.code,
                        language
                    ));
                    return true;
                }
                Err(err) => {
                    let mut state = self.state.lock().unwrap();
                    state.status = FetchStatus::Error;
                    state.loading_started_epoch_ms = 0;
                    state.error_message = err.to_string();
                    state.stale_network_error = false;
                    log_line(&format!(
                        "cache parse error provider={} code={} language={} err={}",
                        provider_key(restaurant.provider),
                        restaurant.code,
                        language,
                        err
                    ));
                    return false;
                }
            }
        }
        log_line(&format!(
            "cache miss provider={} code={} language={}",
            provider_key(restaurant.provider),
            restaurant.code,
            language
        ));
        false
    }

    fn apply_cached_result(&self, result: &FetchOutput) {
        let mut state = self.state.lock().unwrap();
        state.raw_payload = result.raw_json.clone();
        state.restaurant_name = result.restaurant_name.clone();
        state.restaurant_url = result.restaurant_url.clone();
        state.today_menu = result.today_menu.clone();
        state.provider = result.provider;
        state.payload_date = result.payload_date.clone();
        update_stale_date(&mut state);
        if result.ok {
            state.status = FetchStatus::Ok;
            state.loading_started_epoch_ms = 0;
            state.error_message.clear();
            state.stale_network_error = false;
        } else {
            state.status = FetchStatus::Error;
            state.loading_started_epoch_ms = 0;
            state.error_message = result.error_message.clone();
            state.stale_network_error = false;
        }
    }

    fn store_memory_from_fetch_output(&self, code: &str, language: &str, result: &FetchOutput) {
        let key = menu_cache_key(code, language);
        let entry = MemoryMenuEntry {
            ok: result.ok,
            error_message: result.error_message.clone(),
            today_menu: result.today_menu.clone(),
            restaurant_name: result.restaurant_name.clone(),
            restaurant_url: result.restaurant_url.clone(),
            provider: result.provider,
            raw_payload: result.raw_json.clone(),
            payload_date: result.payload_date.clone(),
        };
        let mut cache = self.memory_menu_cache.lock().unwrap();
        cache.insert(key, entry);
    }

    fn load_memory_for(
        &self,
        code: &str,
        language: &str,
        provider: Provider,
        antell_payload_date: Option<&str>,
    ) -> bool {
        let key = menu_cache_key(code, language);
        let mut entry = {
            let cache = self.memory_menu_cache.lock().unwrap();
            cache.get(&key).cloned()
        };
        let Some(mut entry) = entry.take() else {
            return false;
        };

        if provider == Provider::Antell {
            if let Some(date_key) = antell_payload_date {
                entry.payload_date = date_key.to_string();
            }
        }

        let mut state = self.state.lock().unwrap();
        state.raw_payload = entry.raw_payload;
        state.restaurant_name = entry.restaurant_name;
        state.restaurant_url = entry.restaurant_url;
        state.today_menu = entry.today_menu;
        state.provider = entry.provider;
        state.payload_date = entry.payload_date;
        update_stale_date(&mut state);
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
        if entry.ok {
            state.status = FetchStatus::Ok;
            state.error_message.clear();
        } else {
            state.status = FetchStatus::Error;
            state.error_message = entry.error_message;
        }
        true
    }

    /// Starts any startup refresh work that should happen after cached state is restored.
    pub fn maybe_refresh_on_startup(&self) {
        self.maybe_refresh_current_with_reasons(
            "startup",
            RefreshNeedReasons {
                missing: FetchReason::StartupMissingCache,
                stale: FetchReason::StartupStaleDate,
                interval: FetchReason::StartupRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers a timer-driven refresh for the currently selected restaurant.
    pub fn refresh_current_from_timer(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::RefreshTimer),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers the daily midnight refresh path.
    pub fn refresh_current_at_midnight(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::MidnightRollover),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers a user-requested refresh for the currently selected restaurant.
    pub fn refresh_current_manually(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::ManualRefresh),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: true,
            },
        );
    }

    /// Starts a retry fetch after a previous refresh failure.
    pub fn start_refresh_retry(&self) {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip("retry_timer", &target, "hard_closed_today");
            return;
        }

        let (refresh_minutes, payload_date, has_payload) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.refresh_minutes,
                state.payload_date.clone(),
                !state.raw_payload.is_empty(),
            )
        };

        let Some(_) = refresh_need_for_target(
            &target,
            refresh_minutes,
            &payload_date,
            has_payload,
            now_epoch_ms(),
        ) else {
            log_probe_skip("retry_timer", &target, "cache_fresh");
            return;
        };

        self.start_refresh_for_code(
            target.restaurant.code,
            FetchContext::new(FetchMode::Current, FetchReason::RetryTimer),
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }

    fn current_code(&self) -> String {
        let state = self.state.lock().unwrap();
        state.settings.restaurant_code.clone()
    }

    fn current_target(&self) -> FetchTarget {
        let state = self.state.lock().unwrap();
        fetch_target_for_code(
            &state.settings,
            &state.settings.restaurant_code,
            state.settings.enable_antell_restaurants,
        )
    }

    fn start_refresh_for_code(
        &self,
        code: &str,
        context: FetchContext,
        options: StartOptions,
    ) -> bool {
        let hwnd = self.hwnd_tray();
        let now = now_epoch_ms();
        let (settings, target, is_current_code) = {
            let mut state = self.state.lock().unwrap();
            let mut settings = state.settings.clone();
            settings.restaurant_code = code.to_string();
            let target =
                fetch_target_for_code(&settings, code, state.settings.enable_antell_restaurants);
            let is_current = state.settings.restaurant_code == code;
            if is_current && options.mark_loading_when_empty && state.raw_payload.is_empty() {
                state.status = FetchStatus::Loading;
                state.loading_started_epoch_ms = now;
            }
            if is_current {
                state.error_message.clear();
            }
            (settings, target, is_current)
        };

        if is_hard_closed_today(target.restaurant) {
            log_fetch_probe("gate", &context, &target, "skip", "hard_closed_today");
            return false;
        }

        {
            let mut request_states = self.request_states.lock().unwrap();
            let entry = request_states.entry(target.key.clone()).or_default();
            if entry.in_flight {
                let detail = format!(
                    "in_flight=true last_reason={}",
                    entry
                        .last_reason
                        .map(|reason| reason.as_str())
                        .unwrap_or("-")
                );
                log_fetch_probe("gate", &context, &target, "skip", &detail);
                return false;
            }

            if !options.bypass_cooldown && now < entry.cooldown_until_epoch_ms {
                let remaining_ms = entry.cooldown_until_epoch_ms.saturating_sub(now);
                let detail = format!(
                    "cooldown_remaining_ms={} failures={} last_reason={}",
                    remaining_ms,
                    entry.consecutive_failures,
                    entry
                        .last_reason
                        .map(|reason| reason.as_str())
                        .unwrap_or("-")
                );
                log_fetch_probe("gate", &context, &target, "skip", &detail);
                return false;
            }

            entry.in_flight = true;
            entry.last_attempt_epoch_ms = now;
            entry.last_reason = Some(context.reason);
        }

        let requested_code = code.to_string();
        let requested_language = settings.language.clone();
        let requested_effective_language = target.effective_language.clone();
        let request_key = target.key.clone();

        if is_current_code {
            log_fetch_probe("gate", &context, &target, "allow", "");
        } else {
            log_fetch_probe("prefetch_gate", &context, &target, "allow", "");
        }

        std::thread::spawn(move || {
            let result = api::fetch_today(&settings, &context);
            let message = FetchMessage {
                requested_code,
                requested_language,
                requested_effective_language,
                request_key,
                context,
                result,
            };
            let boxed = Box::new(message);
            let ptr = Box::into_raw(boxed) as isize;
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    crate::winmsg::WM_APP_FETCH_COMPLETE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(ptr),
                );
            }
        });
        true
    }

    /// Applies a completed fetch message on the UI thread and updates cached state.
    pub fn apply_fetch_message(&self, message: FetchMessage) -> FetchApplyOutcome {
        let FetchMessage {
            requested_code,
            requested_language,
            requested_effective_language,
            request_key,
            context,
            result,
        } = message;
        let now = now_epoch_ms();

        let (current_code, current_language, enable_antell) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.restaurant_code.clone(),
                state.settings.language.clone(),
                state.settings.enable_antell_restaurants,
            )
        };
        let current_target =
            fetch_target_for_values(&current_code, &current_language, enable_antell);
        let is_current_request = requested_code == current_code
            && requested_effective_language == current_target.effective_language;
        let alias_language = if is_current_request && current_language != requested_language {
            Some(current_language.clone())
        } else {
            None
        };
        let cooldown_ms = self.finish_request_state(&request_key, &context, result.ok, now);
        let stale_no_menu_cooldown_ms =
            self.apply_stale_no_menu_cooldown(&request_key, &result, now);

        if !is_current_request {
            if result.ok {
                self.persist_result_for_languages(
                    &requested_code,
                    &result,
                    &[requested_language.as_str()],
                );
                log_line(&format!(
                    "fetch apply mode=background reason={} code={} ui_language={} fetch_language={} outcome=success request_key={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                ));
                if stale_no_menu_cooldown_ms > 0 {
                    log_line(&format!(
                        "fetch cooldown mode=background reason={} code={} request_key={} cooldown_ms={} detail=stale_no_today_menu payload_date={}",
                        context.reason.as_str(),
                        requested_code,
                        request_key,
                        stale_no_menu_cooldown_ms,
                        result.payload_date,
                    ));
                }
                FetchApplyOutcome::BackgroundSuccess
            } else {
                log_line(&format!(
                    "fetch apply mode=background reason={} code={} ui_language={} fetch_language={} outcome=failure request_key={} cooldown_ms={} err={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                    cooldown_ms,
                    result.error_message,
                ));
                FetchApplyOutcome::BackgroundFailure
            }
        } else {
            let mut state = self.state.lock().unwrap();
            if result.ok {
                state.status = FetchStatus::Ok;
                state.loading_started_epoch_ms = 0;
                state.error_message.clear();
                state.stale_network_error = false;
                state.raw_payload = result.raw_json.clone();
                state.restaurant_name = result.restaurant_name.clone();
                state.restaurant_url = result.restaurant_url.clone();
                state.today_menu = result.today_menu.clone();
                state.provider = result.provider;
                state.payload_date = result.payload_date.clone();
                update_stale_date(&mut state);
                state.settings.last_updated_epoch_ms = now;
                if let Err(err) = save_settings(&state.settings) {
                    log_line(&format!("save settings failed: {}", err));
                }
                log_line(&format!(
                    "fetch apply mode=current reason={} code={} ui_language={} fetch_language={} outcome=success request_key={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                ));
                if stale_no_menu_cooldown_ms > 0 {
                    log_line(&format!(
                        "fetch cooldown mode=current reason={} code={} request_key={} cooldown_ms={} detail=stale_no_today_menu payload_date={}",
                        context.reason.as_str(),
                        requested_code,
                        request_key,
                        stale_no_menu_cooldown_ms,
                        result.payload_date,
                    ));
                }
                drop(state);
                let mut languages = vec![requested_language.as_str()];
                if let Some(alias_language) = alias_language.as_deref() {
                    languages.push(alias_language);
                }
                self.persist_result_for_languages(&requested_code, &result, &languages);
                FetchApplyOutcome::CurrentSuccess
            } else {
                if !state.raw_payload.is_empty() {
                    state.status = FetchStatus::Stale;
                    state.loading_started_epoch_ms = 0;
                    state.stale_network_error = is_probable_network_error(&result.error_message);
                } else {
                    state.status = FetchStatus::Error;
                    state.loading_started_epoch_ms = 0;
                    state.stale_network_error = false;
                }
                state.error_message = result.error_message.clone();
                log_line(&format!(
                    "fetch apply mode=current reason={} code={} ui_language={} fetch_language={} outcome=failure request_key={} cooldown_ms={} err={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                    cooldown_ms,
                    result.error_message,
                ));
                FetchApplyOutcome::CurrentFailure
            }
        }
    }

    fn apply_stale_no_menu_cooldown(
        &self,
        request_key: &str,
        result: &FetchOutput,
        now: i64,
    ) -> u32 {
        if !result.ok || result.today_menu.is_some() {
            return 0;
        }
        let payload_date = result.payload_date.trim();
        if payload_date.is_empty() || payload_date == today_key() {
            return 0;
        }
        let mut request_states = self.request_states.lock().unwrap();
        let Some(entry) = request_states.get_mut(request_key) else {
            return 0;
        };
        entry.cooldown_until_epoch_ms = now.saturating_add(STALE_NO_MENU_COOLDOWN_MS as i64);
        STALE_NO_MENU_COOLDOWN_MS
    }

    fn finish_request_state(
        &self,
        request_key: &str,
        context: &FetchContext,
        ok: bool,
        now: i64,
    ) -> u32 {
        let mut request_states = self.request_states.lock().unwrap();
        let entry = request_states.entry(request_key.to_string()).or_default();
        entry.in_flight = false;
        entry.last_reason = Some(context.reason);
        if ok {
            entry.last_success_epoch_ms = now;
            entry.consecutive_failures = 0;
            entry.cooldown_until_epoch_ms = 0;
            0
        } else {
            entry.last_failure_epoch_ms = now;
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
            let delay_ms = retry_delay_ms_for_failures(entry.consecutive_failures);
            entry.cooldown_until_epoch_ms = now.saturating_add(delay_ms as i64);
            delay_ms
        }
    }

    fn persist_result_for_languages(&self, code: &str, result: &FetchOutput, languages: &[&str]) {
        if result.raw_json.is_empty() {
            return;
        }

        for language in languages {
            if let Err(err) = cache::write_cache(result.provider, code, language, &result.raw_json)
            {
                log_line(&format!(
                    "cache write failed code={} language={} err={}",
                    code, language, err
                ));
                continue;
            }
            self.store_memory_from_fetch_output(code, language, result);
        }
    }

    /// Changes the selected restaurant by its stable restaurant code.
    pub fn set_restaurant(&self, code: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.restaurant_code = code.to_string();
        let restaurant = restaurant_for_code(
            &state.settings.restaurant_code,
            state.settings.enable_antell_restaurants,
        );
        state.provider = restaurant.provider;
        state.restaurant_url = restaurant.url.unwrap_or_default().to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Changes the selected restaurant by its menu order index.
    pub fn set_restaurant_index(&self, index: usize) -> bool {
        let enable_antell = {
            let state = self.state.lock().unwrap();
            state.settings.enable_antell_restaurants
        };
        let Some(restaurant) = restaurant_for_shortcut_index(index, enable_antell) else {
            return false;
        };
        self.set_restaurant(restaurant.code);
        true
    }

    /// Changes the UI language and refreshes any derived state that depends on it.
    pub fn set_language(&self, language: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.language = language.to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Toggles whether menu headings show prices.
    pub fn toggle_show_prices(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_prices = !state.settings.show_prices;
        let _ = save_settings(&state.settings);
    }

    /// Toggles whether allergen suffixes are rendered.
    pub fn toggle_show_allergens(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_allergens = !state.settings.show_allergens;
        let _ = save_settings(&state.settings);
    }

    /// Toggles gluten-free highlighting.
    pub fn toggle_highlight_gluten_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_gluten_free = !state.settings.highlight_gluten_free;
        let _ = save_settings(&state.settings);
    }

    /// Toggles vegetarian highlighting.
    pub fn toggle_highlight_veg(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_veg = !state.settings.highlight_veg;
        let _ = save_settings(&state.settings);
    }

    /// Toggles lactose-free highlighting.
    pub fn toggle_highlight_lactose_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_lactose_free = !state.settings.highlight_lactose_free;
        let _ = save_settings(&state.settings);
    }

    /// Toggles popup open/close/switch animations.
    pub fn toggle_animations(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.animations_enabled = !state.settings.animations_enabled;
        let _ = save_settings(&state.settings);
    }

    /// Toggles student price visibility in Compass price strings.
    pub fn toggle_show_student_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_student_price = !state.settings.show_student_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles staff price visibility in Compass price strings.
    pub fn toggle_show_staff_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_staff_price = !state.settings.show_staff_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles guest price visibility in Compass price strings.
    pub fn toggle_show_guest_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_guest_price = !state.settings.show_guest_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles hiding expensive student meals from rendered menus.
    pub fn toggle_hide_expensive_student_meals(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.hide_expensive_student_meals = !state.settings.hide_expensive_student_meals;
        let _ = save_settings(&state.settings);
    }

    /// Updates the configured automatic refresh interval in minutes.
    pub fn set_refresh_minutes(&self, minutes: u32) {
        let mut state = self.state.lock().unwrap();
        state.settings.refresh_minutes = minutes;
        let _ = save_settings(&state.settings);
    }

    /// Moves the restaurant selection backward or forward in the enabled list.
    pub fn cycle_restaurant(&self, direction: i32) {
        let mut state = self.state.lock().unwrap();
        let current = state.settings.restaurant_code.as_str();
        let list = available_restaurants(state.settings.enable_antell_restaurants);
        let mut idx = list.iter().position(|c| c.code == current).unwrap_or(0) as i32;
        idx += direction;
        if idx < 0 {
            idx = list.len() as i32 - 1;
        } else if idx >= list.len() as i32 {
            idx = 0;
        }
        state.settings.restaurant_code = list[idx as usize].code.to_string();
        state.provider = list[idx as usize].provider;
        state.restaurant_url = list[idx as usize].url.unwrap_or_default().to_string();
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Writes the current settings snapshot to disk.
    pub fn persist_settings(&self) {
        let settings = {
            let state = self.state.lock().unwrap();
            state.settings.clone()
        };
        let _ = save_settings(&settings);
    }

    /// Opens the current restaurant URL in the system browser, if available.
    pub fn open_current_url(&self) {
        let url = {
            let state = self.state.lock().unwrap();
            state.restaurant_url.clone()
        };
        if url.is_empty() {
            return;
        }
        self.open_target(&url);
    }

    /// Opens the app data directory used for settings, cache, and logs.
    pub fn open_appdata_dir(&self) {
        let dir = settings_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log_line(&format!("failed to create appdata dir: {}", err));
            return;
        }
        let path = dir.to_string_lossy().to_string();
        self.open_target(&path);
    }

    /// Opens the configured feedback URL in the system browser.
    pub fn open_feedback_url(&self) {
        self.open_target("https://github.com/veetir/uef-kuopio-lunch-tray/issues");
    }

    /// Opens a release or releases page in the system browser.
    pub fn open_release_url(&self, url: &str) {
        if url.trim().is_empty() {
            return;
        }
        self.open_target(url);
    }

    /// Starts an asynchronous update check if one is not already in flight.
    pub fn start_update_check(&self) -> bool {
        {
            let mut in_flight = self.update_check_in_flight.lock().unwrap();
            if *in_flight {
                log_line("update check skipped: already in flight");
                return false;
            }
            *in_flight = true;
        }

        let hwnd = self.hwnd_tray();
        let in_flight = Arc::clone(&self.update_check_in_flight);
        log_line(&format!(
            "update check start current_version={}",
            update::current_app_version()
        ));
        std::thread::spawn(move || {
            let outcome = match update::check_for_updates() {
                Ok(update::UpdateCheckResult::LatestPublished {
                    current_version,
                    release_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=latest_published version={} release_url={}",
                        current_version, release_url
                    ));
                    UpdateCheckOutcome::LatestPublished {
                        current_version,
                        release_url,
                    }
                }
                Ok(update::UpdateCheckResult::UpdateAvailable {
                    current_version,
                    latest_version,
                    html_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=update_available current_version={} latest_version={} release_url={}",
                        current_version, latest_version, html_url
                    ));
                    UpdateCheckOutcome::UpdateAvailable {
                        current_version,
                        latest_version,
                        release_url: html_url,
                    }
                }
                Ok(update::UpdateCheckResult::NewerThanLatestPublished {
                    current_version,
                    latest_version,
                    releases_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=newer_than_latest current_version={} latest_version={} releases_url={}",
                        current_version, latest_version, releases_url
                    ));
                    UpdateCheckOutcome::NewerThanLatestPublished {
                        current_version,
                        latest_version,
                        releases_url,
                    }
                }
                Err(err) => {
                    let message = err.to_string();
                    log_line(&format!(
                        "update check result outcome=failure err={}",
                        message
                    ));
                    UpdateCheckOutcome::Failed { message }
                }
            };

            let boxed = Box::new(UpdateCheckMessage { outcome });
            let ptr = Box::into_raw(boxed) as isize;
            unsafe {
                let posted = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    crate::winmsg::WM_APP_UPDATE_CHECK_COMPLETE,
                    WPARAM(0),
                    LPARAM(ptr),
                )
                .is_ok();
                if !posted {
                    log_line("update check post failed");
                    let mut state = in_flight.lock().unwrap();
                    *state = false;
                }
            }
        });
        true
    }

    /// Marks the current update check as completed.
    pub fn finish_update_check(&self) {
        let mut in_flight = self.update_check_in_flight.lock().unwrap();
        *in_flight = false;
    }

    /// Returns the configured automatic refresh interval in minutes.
    pub fn refresh_minutes(&self) -> u32 {
        let state = self.state.lock().unwrap();
        state.settings.refresh_minutes
    }

    /// Refreshes when selection changed and the cached data is missing or stale enough.
    pub fn maybe_refresh_on_selection(&self) {
        self.maybe_refresh_current_with_reasons(
            "selection",
            RefreshNeedReasons {
                missing: FetchReason::SelectionMissingCache,
                stale: FetchReason::SelectionStaleDate,
                interval: FetchReason::SelectionRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }

    /// Refreshes when the UI language changed and the matching cache is missing or stale.
    pub fn maybe_refresh_on_language_switch(&self) {
        self.maybe_refresh_current_with_reasons(
            "language_switch",
            RefreshNeedReasons {
                missing: FetchReason::LanguageSwitchMissingCache,
                stale: FetchReason::LanguageSwitchStaleDate,
                interval: FetchReason::LanguageSwitchRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }

    /// Changes the active popup theme.
    pub fn set_theme(&self, theme: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.theme = normalize_theme(theme);
        let _ = save_settings(&state.settings);
    }

    /// Changes the popup scale preset.
    pub fn set_widget_scale(&self, value: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.widget_scale = normalize_widget_scale(value);
        let _ = save_settings(&state.settings);
    }

    /// Toggles diagnostic logging and persists the updated setting.
    pub fn toggle_logging(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.enable_logging = !state.settings.enable_logging;
        set_log_enabled(state.settings.enable_logging);
        if state.settings.enable_logging {
            log_line("logging enabled");
        }
        let _ = save_settings(&state.settings);
    }

    /// Refreshes if the cached payload date no longer matches the local day.
    pub fn check_stale_date_and_refresh(&self) {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip("stale_check", &target, "hard_closed_today");
            let mut state = self.state.lock().unwrap();
            state.stale_date = false;
            return;
        }

        let should_refresh = {
            let mut state = self.state.lock().unwrap();
            let today_key = today_key();
            if !state.payload_date.is_empty() {
                let stale = state.payload_date != today_key;
                state.stale_date = stale;
                stale
            } else {
                state.stale_date = false;
                false
            }
        };
        if should_refresh {
            self.start_refresh_for_code(
                target.restaurant.code,
                FetchContext::new(FetchMode::Current, FetchReason::StaleDateCheck),
                StartOptions {
                    mark_loading_when_empty: false,
                    bypass_cooldown: false,
                },
            );
        } else {
            log_probe_skip("stale_check", &target, "not_stale");
        }
    }

    /// Returns the next retry delay derived from recent fetch failures.
    pub fn current_retry_delay_ms(&self) -> u32 {
        let now = now_epoch_ms();
        let target = self.current_target();
        let request_states = self.request_states.lock().unwrap();
        let Some(entry) = request_states.get(&target.key) else {
            return 1_000;
        };
        if entry.cooldown_until_epoch_ms <= now {
            1_000
        } else {
            entry.cooldown_until_epoch_ms.saturating_sub(now).max(1_000) as u32
        }
    }

    /// Prefetches menus for non-selected restaurants to improve switching latency.
    pub fn prefetch_enabled_restaurants(&self) {
        let now = now_epoch_ms();
        {
            let mut last_prefetch = self.last_prefetch_ms.lock().unwrap();
            if now.saturating_sub(*last_prefetch) < 5 * 60_000 {
                let target = self.current_target();
                log_probe_skip("prefetch", &target, "recently_ran");
                return;
            }
            *last_prefetch = now;
        }

        let (settings, current_code) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.clone(),
                state.settings.restaurant_code.clone(),
            )
        };
        let today = today_key();
        let restaurants = available_restaurants(settings.enable_antell_restaurants);

        let mut queued = 0usize;
        for restaurant in restaurants {
            if restaurant.code == current_code {
                continue;
            }
            if is_hard_closed_today(restaurant) {
                let target = FetchTarget {
                    restaurant,
                    ui_language: settings.language.clone(),
                    effective_language: effective_fetch_language(restaurant, &settings.language),
                    key: request_state_key(
                        restaurant.code,
                        &effective_fetch_language(restaurant, &settings.language),
                    ),
                };
                log_probe_skip("prefetch", &target, "hard_closed_today");
                continue;
            }
            let target = fetch_target_for_code(
                &settings,
                restaurant.code,
                settings.enable_antell_restaurants,
            );
            let need = match cache::cache_mtime_ms(
                restaurant.provider,
                restaurant.code,
                &settings.language,
            ) {
                None => Some(FetchReason::PrefetchMissingCache),
                Some(ts) => match date_key_from_epoch_ms(ts) {
                    Some(date) if date != today => Some(FetchReason::PrefetchStaleDate),
                    Some(_) => None,
                    None => Some(FetchReason::PrefetchStaleDate),
                },
            };
            let Some(reason) = need else {
                log_probe_skip("prefetch", &target, "cache_fresh");
                continue;
            };
            if self.start_refresh_for_code(
                restaurant.code,
                FetchContext::new(FetchMode::Background, reason),
                StartOptions {
                    mark_loading_when_empty: false,
                    bypass_cooldown: false,
                },
            ) {
                queued = queued.saturating_add(1);
            }
        }
        log_line(&format!("prefetch queued={}", queued));
    }

    fn maybe_refresh_current_with_reasons(
        &self,
        trigger: &str,
        reasons: RefreshNeedReasons,
        options: StartOptions,
    ) -> bool {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip(trigger, &target, "hard_closed_today");
            return false;
        }

        let (refresh_minutes, payload_date, has_payload) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.refresh_minutes,
                state.payload_date.clone(),
                !state.raw_payload.is_empty(),
            )
        };

        let need = refresh_need_for_target(
            &target,
            refresh_minutes,
            &payload_date,
            has_payload,
            now_epoch_ms(),
        );

        let Some(need) = need else {
            log_probe_skip(trigger, &target, "cache_fresh");
            return false;
        };

        if need == RefreshNeed::RefreshIntervalElapsed && refresh_minutes == 0 {
            log_probe_skip(trigger, &target, "refresh_interval_disabled");
            return false;
        }

        let reason = match need {
            RefreshNeed::MissingCache => reasons.missing,
            RefreshNeed::StaleDate => reasons.stale,
            RefreshNeed::RefreshIntervalElapsed => reasons.interval,
        };
        self.start_refresh_for_code(
            target.restaurant.code,
            FetchContext::new(FetchMode::Current, reason),
            options,
        )
    }
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
    let restaurant = restaurant_for_code(
        &state.settings.restaurant_code,
        state.settings.enable_antell_restaurants,
    );
    if is_hard_closed_today(restaurant) {
        state.stale_date = false;
    } else if !state.payload_date.is_empty() {
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
    now_ms: i64,
) -> Option<RefreshNeed> {
    if !has_payload {
        return Some(RefreshNeed::MissingCache);
    }

    let today = today_key();
    if !payload_date.is_empty() && payload_date != today {
        return Some(RefreshNeed::StaleDate);
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

impl App {
    fn open_target(&self, target: &str) {
        let wide = crate::util::to_wstring(target);
        unsafe {
            ShellExecuteW(
                None,
                windows::core::PCWSTR(crate::util::to_wstring("open").as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_state_key_uses_effective_fetch_language() {
        let target = fetch_target_for_values("3488", "en", true);
        assert_eq!(target.effective_language, "fi");
        assert_eq!(target.key, "3488|fi");
    }

    #[test]
    fn refresh_need_marks_missing_payload_as_missing_cache() {
        let target = fetch_target_for_values("0437", "fi", true);
        assert_eq!(
            refresh_need_for_target(&target, 1440, "", false, now_epoch_ms()),
            Some(RefreshNeed::MissingCache)
        );
    }

    #[test]
    fn refresh_need_marks_old_payload_date_as_stale() {
        let target = fetch_target_for_values("0437", "fi", true);
        assert_eq!(
            refresh_need_for_target(&target, 1440, "2001-01-01", true, now_epoch_ms()),
            Some(RefreshNeed::StaleDate)
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
}
