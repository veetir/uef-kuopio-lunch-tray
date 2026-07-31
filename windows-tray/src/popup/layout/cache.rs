//! Cache and cached-layout helpers used by popup sizing.

use super::text::measure_lines_layout;
use super::*;

pub(in crate::popup) fn invalidate_layout_budget_cache() {
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

pub(super) fn popup_cached_layout_budget(
    state: &AppState,
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    dpi_y: i32,
) -> CachedLayoutBudget {
    // Cache the largest wrapped layout seen in today's cached menus so popup
    // size stays stable while the user switches between restaurants.
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
        dpi_y,
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
        show_price_group_names: settings.show_price_group_names,
        lunch_item_display_mode: settings.lunch_item_display_mode,
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
            crate::cache::cache_mtime_ms(restaurant.provider, restaurant.code, &settings.language)
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
            max_extra_height_px: entry.max_extra_height_px,
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
            max_extra_height_px: budget.max_extra_height_px,
        });
    }
}

pub(super) fn desired_size_cache_key(
    state: &AppState,
    dpi_y: i32,
    expanded_recipe_key: Option<RecipeExpansionKey>,
) -> Option<PopupDesiredSizeKey> {
    if state.status == FetchStatus::Loading {
        return None;
    }

    Some(PopupDesiredSizeKey {
        today_key: local_today_key(),
        restaurant_code: state.settings.restaurant_code.clone(),
        status: state.status,
        error_message: state.error_message.clone(),
        stale_network_error: state.stale_network_error,
        stale_date: state.stale_date,
        expanded_recipe_key,
        enable_antell_restaurants: state.settings.enable_antell_restaurants,
        language: state.settings.language.clone(),
        theme: state.settings.theme.clone(),
        widget_scale: state.settings.widget_scale.clone(),
        dpi_y,
        show_prices: state.settings.show_prices,
        show_student_price: state.settings.show_student_price,
        show_staff_price: state.settings.show_staff_price,
        show_guest_price: state.settings.show_guest_price,
        show_price_group_names: state.settings.show_price_group_names,
        lunch_item_display_mode: state.settings.lunch_item_display_mode,
        show_allergens: state.settings.show_allergens,
        highlight_gluten_free: state.settings.highlight_gluten_free,
        highlight_veg: state.settings.highlight_veg,
        highlight_lactose_free: state.settings.highlight_lactose_free,
    })
}

pub(super) fn cached_desired_size(key: &PopupDesiredSizeKey) -> Option<(i32, i32)> {
    let cache = POPUP_DESIRED_SIZE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = cache.lock().ok()?;
    let index = guard.iter().position(|entry| entry.key == *key)?;
    let entry = guard.remove(index);
    let size = (entry.width, entry.height);
    guard.push(entry);
    Some(size)
}

pub(super) fn update_desired_size_cache(key: PopupDesiredSizeKey, size: (i32, i32)) {
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
    dpi_y: i32,
) -> CachedLayoutBudget {
    let settings = &state.settings;
    let scale = popup_scale_for_dpi(settings, dpi_y);
    let mut max_wrapped_lines: Option<usize> = None;
    let mut max_content_width_px: Option<i32> = None;
    let mut max_extra_height_px: Option<i32> = None;

    for restaurant in available_restaurants(settings.enable_antell_restaurants) {
        let raw = match crate::cache::read_cache(
            restaurant.provider,
            restaurant.code,
            &settings.language,
        ) {
            Some(payload) => {
                crate::perf::count_layout_budget_cache_read();
                payload
            }
            None => continue,
        };

        crate::perf::count_layout_budget_cache_parse();
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
        max_extra_height_px = Some(max_extra_height_px.map_or(metrics.extra_height_px, |prev| {
            prev.max(metrics.extra_height_px)
        }));
    }

    CachedLayoutBudget {
        max_wrapped_lines,
        max_content_width_px,
        max_extra_height_px,
    }
}

fn is_today_valid_cache(
    parsed: &api::FetchOutput,
    _restaurant: Restaurant,
    _settings: &Settings,
    today_key: &str,
) -> bool {
    !parsed.payload_date.is_empty() && parsed.payload_date == today_key
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
        api_stale: parsed.is_stale,
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

#[cfg(test)]
mod tests {
    use super::desired_size_cache_key;
    use crate::restaurant::Provider;
    use crate::settings::Settings;
    use crate::state::{AppState, FetchStatus};

    #[test]
    fn desired_size_key_tracks_status_and_restaurant_line_inputs() {
        let base = test_state();
        let base_key = desired_size_cache_key(&base, 96, None).unwrap();

        let mut different_restaurant = base.clone();
        different_restaurant.settings.restaurant_code = "tietoteknia".to_string();
        assert_ne!(
            base_key,
            desired_size_cache_key(&different_restaurant, 96, None).unwrap()
        );

        let mut stale = base.clone();
        stale.status = FetchStatus::Stale;
        stale.stale_network_error = true;
        stale.stale_date = true;
        assert_ne!(base_key, desired_size_cache_key(&stale, 96, None).unwrap());

        let mut error = base.clone();
        error.status = FetchStatus::Error;
        error.error_message = "provider failed".to_string();
        assert_ne!(base_key, desired_size_cache_key(&error, 96, None).unwrap());
    }

    fn test_state() -> AppState {
        AppState {
            settings: Settings::default(),
            status: FetchStatus::Ok,
            loading_started_epoch_ms: 0,
            error_message: String::new(),
            stale_network_error: false,
            today_menu: None,
            restaurant_name: "Tietoteknia".to_string(),
            restaurant_url: String::new(),
            raw_payload: String::new(),
            provider: Provider::LunchApi,
            payload_date: String::new(),
            stale_date: false,
            api_stale: false,
        }
    }
}
