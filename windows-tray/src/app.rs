use crate::api::{self, FetchOutput};
use crate::cache;
use crate::log::{log_line, set_enabled as set_log_enabled};
use crate::model::TodayMenu;
use crate::restaurant::{
    available_restaurants, is_hard_closed_today, provider_key, restaurant_for_code,
    restaurant_for_shortcut_index, Provider,
};
use crate::settings::{load_settings, normalize_theme, save_settings, settings_dir, Settings};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use windows::Win32::Foundation::HWND;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    Idle,
    Loading,
    Ok,
    Stale,
    Error,
}

#[derive(Debug, Clone)]
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

pub struct App {
    pub no_tray: bool,
    state: Arc<Mutex<AppState>>,
    hwnds: Mutex<WindowHandles>,
    hover_point: Mutex<Option<(i32, i32)>>,
    context_menu_open: Mutex<bool>,
    in_flight_codes: Mutex<HashSet<String>>,
    retry_step: Mutex<usize>,
    last_prefetch_ms: Mutex<i64>,
    memory_menu_cache: Mutex<HashMap<String, MemoryMenuEntry>>,
}

pub struct FetchMessage {
    pub requested_code: String,
    pub requested_language: String,
    pub result: FetchOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchApplyOutcome {
    CurrentSuccess,
    CurrentFailure,
    BackgroundSuccess,
    BackgroundFailure,
}

impl App {
    pub fn new(no_tray: bool) -> Self {
        let settings = load_settings();
        set_log_enabled(settings.enable_logging);
        let state = AppState {
            provider: restaurant_for_code(&settings.restaurant_code, settings.enable_antell_restaurants).provider,
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
            no_tray,
            state: Arc::new(Mutex::new(state)),
            hwnds: Mutex::new(WindowHandles::default()),
            hover_point: Mutex::new(None),
            context_menu_open: Mutex::new(false),
            in_flight_codes: Mutex::new(HashSet::new()),
            retry_step: Mutex::new(0),
            last_prefetch_ms: Mutex::new(0),
            memory_menu_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_hwnds(&self, tray: HWND, popup: HWND) {
        let mut hwnds = self.hwnds.lock().unwrap();
        hwnds.tray = tray;
        hwnds.popup = popup;
    }

    pub fn hwnd_tray(&self) -> HWND {
        self.hwnds.lock().unwrap().tray
    }

    pub fn hwnd_popup(&self) -> HWND {
        self.hwnds.lock().unwrap().popup
    }

    pub fn snapshot(&self) -> AppState {
        self.state.lock().unwrap().clone()
    }

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

    pub fn start_refresh(&self) {
        let code = {
            let state = self.state.lock().unwrap();
            state.settings.restaurant_code.clone()
        };
        let _ = self.start_refresh_for_code(&code, true);
    }

    pub fn start_refresh_retry(&self) {
        let code = {
            let state = self.state.lock().unwrap();
            state.settings.restaurant_code.clone()
        };
        let _ = self.start_refresh_for_code(&code, false);
    }

    fn start_refresh_for_code(&self, code: &str, mark_loading_when_empty: bool) -> bool {
        {
            let mut in_flight = self.in_flight_codes.lock().unwrap();
            if in_flight.contains(code) {
                return false;
            }
            in_flight.insert(code.to_string());
        }

        let hwnd = self.hwnd_tray();
        let (settings, requested_language, is_current_code) = {
            let mut state = self.state.lock().unwrap();
            let is_current = state.settings.restaurant_code == code;
            if is_current && mark_loading_when_empty && state.raw_payload.is_empty() {
                state.status = FetchStatus::Loading;
                state.loading_started_epoch_ms = now_epoch_ms();
            }
            if is_current {
                state.error_message.clear();
            }
            let mut settings = state.settings.clone();
            settings.restaurant_code = code.to_string();
            let requested_language = settings.language.clone();
            (settings, requested_language, is_current)
        };

        if is_current_code {
            log_line(&format!("refresh start code={}", code));
        } else {
            log_line(&format!("prefetch start code={}", code));
        }

        let requested_code = code.to_string();
        std::thread::spawn(move || {
            let result = api::fetch_today(&settings);
            let message = FetchMessage {
                requested_code,
                requested_language,
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

    pub fn apply_fetch_message(&self, message: FetchMessage) -> FetchApplyOutcome {
        let FetchMessage {
            requested_code,
            requested_language,
            result,
        } = message;

        {
            let mut in_flight = self.in_flight_codes.lock().unwrap();
            in_flight.remove(&requested_code);
        }

        let current_code = {
            let state = self.state.lock().unwrap();
            state.settings.restaurant_code.clone()
        };

        if requested_code != current_code {
            if result.ok {
                if !result.raw_json.is_empty() {
                    if let Err(err) = cache::write_cache(
                        result.provider,
                        &requested_code,
                        &requested_language,
                        &result.raw_json,
                    ) {
                        log_line(&format!(
                            "background cache write failed code={} err={}",
                            requested_code, err
                        ));
                    }
                    self.store_memory_from_fetch_output(
                        &requested_code,
                        &requested_language,
                        &result,
                    );
                }
                FetchApplyOutcome::BackgroundSuccess
            } else {
                log_line(&format!(
                    "background refresh failed code={} err={}",
                    requested_code, result.error_message
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
                state.settings.last_updated_epoch_ms = now_epoch_ms();
                if let Err(err) = save_settings(&state.settings) {
                    log_line(&format!("save settings failed: {}", err));
                }
                if !result.raw_json.is_empty() {
                    if let Err(err) = cache::write_cache(
                        state.provider,
                        &requested_code,
                        &requested_language,
                        &result.raw_json,
                    ) {
                        log_line(&format!(
                            "cache write failed code={} language={} err={}",
                            requested_code, requested_language, err
                        ));
                    }
                }
                log_line(&format!("refresh ok code={}", requested_code));
                drop(state);
                if !result.raw_json.is_empty() {
                    self.store_memory_from_fetch_output(
                        &requested_code,
                        &requested_language,
                        &result,
                    );
                }
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
                    "refresh failed code={} err={}",
                    requested_code, result.error_message
                ));
                FetchApplyOutcome::CurrentFailure
            }
        }
    }

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

    pub fn toggle_show_prices(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_prices = !state.settings.show_prices;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_show_allergens(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_allergens = !state.settings.show_allergens;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_highlight_gluten_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_gluten_free = !state.settings.highlight_gluten_free;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_highlight_veg(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_veg = !state.settings.highlight_veg;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_highlight_lactose_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_lactose_free = !state.settings.highlight_lactose_free;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_show_student_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_student_price = !state.settings.show_student_price;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_show_staff_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_staff_price = !state.settings.show_staff_price;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_show_guest_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_guest_price = !state.settings.show_guest_price;
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_hide_expensive_student_meals(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.hide_expensive_student_meals =
            !state.settings.hide_expensive_student_meals;
        let _ = save_settings(&state.settings);
    }

    pub fn set_refresh_minutes(&self, minutes: u32) {
        let mut state = self.state.lock().unwrap();
        state.settings.refresh_minutes = minutes;
        let _ = save_settings(&state.settings);
    }

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

    pub fn persist_settings(&self) {
        let settings = {
            let state = self.state.lock().unwrap();
            state.settings.clone()
        };
        let _ = save_settings(&settings);
    }

    pub fn open_current_url(&self) {
        let url = {
            let state = self.state.lock().unwrap();
            state.restaurant_url.clone()
        };
        if url.is_empty() {
            return;
        }
        let wide = crate::util::to_wstring(&url);
        unsafe {
            windows::Win32::UI::Shell::ShellExecuteW(
                None,
                windows::core::PCWSTR(crate::util::to_wstring("open").as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }

    pub fn open_appdata_dir(&self) {
        let dir = settings_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log_line(&format!("failed to create appdata dir: {}", err));
            return;
        }
        let path = dir.to_string_lossy().to_string();
        let wide = crate::util::to_wstring(&path);
        unsafe {
            windows::Win32::UI::Shell::ShellExecuteW(
                None,
                windows::core::PCWSTR(crate::util::to_wstring("open").as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }

    pub fn refresh_minutes(&self) -> u32 {
        let state = self.state.lock().unwrap();
        state.settings.refresh_minutes
    }

    pub fn maybe_refresh_on_selection(&self) {
        let (restaurant, language, refresh_minutes) = {
            let state = self.state.lock().unwrap();
            (
                restaurant_for_code(
                    &state.settings.restaurant_code,
                    state.settings.enable_antell_restaurants,
                ),
                state.settings.language.clone(),
                state.settings.refresh_minutes,
            )
        };

        if is_hard_closed_today(restaurant) {
            return;
        }

        if refresh_minutes == 0 {
            return;
        }

        let now = now_epoch_ms();
        let should_fetch = match cache::cache_mtime_ms(restaurant.provider, restaurant.code, &language) {
            None => true,
            Some(ts) => now.saturating_sub(ts) >= (refresh_minutes as i64) * 60_000,
        };

        if should_fetch {
            let _ = self.start_refresh_for_code(restaurant.code, false);
        }
    }

    pub fn restaurant_name(&self) -> String {
        let state = self.state.lock().unwrap();
        state.restaurant_name.clone()
    }

    pub fn set_theme(&self, theme: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.theme = normalize_theme(theme);
        let _ = save_settings(&state.settings);
    }

    pub fn toggle_logging(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.enable_logging = !state.settings.enable_logging;
        set_log_enabled(state.settings.enable_logging);
        if state.settings.enable_logging {
            log_line("logging enabled");
        }
        let _ = save_settings(&state.settings);
    }

    pub fn check_stale_date_and_refresh(&self) {
        let should_refresh = {
            let mut state = self.state.lock().unwrap();
            let restaurant = restaurant_for_code(
                &state.settings.restaurant_code,
                state.settings.enable_antell_restaurants,
            );
            if is_hard_closed_today(restaurant) {
                state.stale_date = false;
                false
            } else {
            let today_key = today_key();
            if !state.payload_date.is_empty() {
                let stale = state.payload_date != today_key;
                state.stale_date = stale;
                stale
            } else {
                state.stale_date = false;
                false
            }
            }
        };
        if should_refresh {
            self.start_refresh_retry();
        }
    }

    pub fn next_retry_delay_ms(&self) -> u32 {
        let mut step = self.retry_step.lock().unwrap();
        let delay = match *step {
            0 => 10_000,
            1 => 30_000,
            2 => 60_000,
            _ => 5 * 60_000,
        };
        *step = step.saturating_add(1);
        delay
    }

    pub fn reset_retry_backoff(&self) {
        let mut step = self.retry_step.lock().unwrap();
        *step = 0;
    }

    pub fn prefetch_enabled_restaurants(&self) {
        let now = now_epoch_ms();
        {
            let mut last_prefetch = self.last_prefetch_ms.lock().unwrap();
            if now.saturating_sub(*last_prefetch) < 5 * 60_000 {
                return;
            }
            *last_prefetch = now;
        }

        let (settings, current_code) = {
            let state = self.state.lock().unwrap();
            (state.settings.clone(), state.settings.restaurant_code.clone())
        };
        let today = today_key();
        let restaurants = available_restaurants(settings.enable_antell_restaurants);

        let mut queued = 0usize;
        for restaurant in restaurants {
            if restaurant.code == current_code {
                continue;
            }
            if is_hard_closed_today(restaurant) {
                continue;
            }
            let stale_or_missing = match cache::cache_mtime_ms(
                restaurant.provider,
                restaurant.code,
                &settings.language,
            ) {
                None => true,
                Some(ts) => match date_key_from_epoch_ms(ts) {
                    Some(date) => date != today,
                    None => true,
                },
            };
            if stale_or_missing && self.start_refresh_for_code(restaurant.code, false) {
                queued += 1;
            }
        }
        log_line(&format!("prefetch queued={}", queued));
    }

    pub fn set_hover_point(&self, x: i32, y: i32) {
        let mut point = self.hover_point.lock().unwrap();
        *point = Some((x, y));
    }

    pub fn clear_hover_point(&self) {
        let mut point = self.hover_point.lock().unwrap();
        *point = None;
    }

    pub fn hover_point(&self) -> Option<(i32, i32)> {
        let point = self.hover_point.lock().unwrap();
        *point
    }

    pub fn set_context_menu_open(&self, open: bool) {
        let mut flag = self.context_menu_open.lock().unwrap();
        *flag = open;
    }

    pub fn is_context_menu_open(&self) -> bool {
        let flag = self.context_menu_open.lock().unwrap();
        *flag
    }
}

pub fn now_epoch_ms() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn menu_cache_key(code: &str, language: &str) -> String {
    format!("{}|{}", language, code)
}

fn today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!("{:04}-{:02}-{:02}", date.year(), date.month() as u8, date.day())
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
    Some(format!("{:04}-{:02}-{:02}", date.year(), date.month() as u8, date.day()))
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
