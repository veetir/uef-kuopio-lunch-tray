use super::*;

pub(super) fn build_lines(state: &AppState) -> Vec<Line> {
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
                let rendered_groups = append_menus(
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
                if rendered_groups == 0 && state.status != FetchStatus::Loading {
                    lines.push(Line::Text(text_for(&state.settings.language, "noMenu")));
                }
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
) -> usize {
    let mut rendered_groups = 0;
    for group in &menu.menus {
        if provider == Provider::Compass && hide_expensive_student_meals {
            if let Some(price) = student_price_eur(&group.price) {
                if price > 4.0 {
                    continue;
                }
            }
        }

        let renderable_components = renderable_menu_components(group);
        if renderable_components.is_empty() {
            continue;
        }

        let heading = menu_heading(group, provider, show_prices, price_groups);
        lines.push(Line::Heading(heading));
        rendered_groups += 1;
        for (main, suffix) in renderable_components {
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
    rendered_groups
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

pub(super) fn current_favorites_snapshot() -> FavoritesSnapshot {
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
        cache.snapshot = FavoritesSnapshot { snippets_lower };
        cache.mtime_ms = mtime;
        cache.loaded = true;
    }
    cache.next_check_epoch_ms = now + FAVORITES_RELOAD_INTERVAL_MS;
    cache.snapshot.clone()
}

pub(super) fn invalidate_favorites_cache() {
    let cache_lock = FAVORITES_CACHE.get_or_init(|| Mutex::new(FavoritesCache::default()));
    if let Ok(mut cache) = cache_lock.lock() {
        cache.loaded = false;
        cache.next_check_epoch_ms = 0;
        cache.mtime_ms = -1;
    }
}
