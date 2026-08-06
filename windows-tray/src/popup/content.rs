use super::*;
use time::{Date, Month, OffsetDateTime, Time, UtcOffset, Weekday};

const CLOSING_SOON_MINUTES: u16 = 15;

pub(super) fn build_lines(state: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();
    let closure_notice = api::closure_notice(&state.raw_payload, &state.settings.language);

    if state.stale_date {
        lines.push(Line::StaleNotice(text_for(
            &state.settings.language,
            "staleDate",
        )));
    }

    let show_loading_hint = state.status == FetchStatus::Loading
        && state.today_menu.is_none()
        && state.loading_started_epoch_ms > 0
        && now_epoch_ms().saturating_sub(state.loading_started_epoch_ms) >= LOADING_HINT_DELAY_MS;

    if show_loading_hint {
        lines.push(Line::Text(text_for(&state.settings.language, "loading")));
    }

    if let Some((date, hours)) =
        date_and_time_parts(state.today_menu.as_ref(), &state.settings.language)
    {
        let hours_status = hours_status(&hours);
        lines.push(Line::DateTime {
            date,
            hours,
            hours_status,
            stale: state.stale_date,
        });
    }

    match &state.today_menu {
        Some(menu) => {
            if !menu.menus.is_empty() {
                let price_groups = PriceGroups {
                    student: state.settings.show_student_price,
                    staff: state.settings.show_staff_price,
                    guest: state.settings.show_guest_price,
                    names: state.settings.show_price_group_names,
                };
                let rendered_groups = append_menus(
                    &mut lines,
                    menu,
                    MenuRenderOptions {
                        provider: state.provider,
                        show_prices: state.settings.show_prices,
                        price_groups,
                        restaurant_code: &state.settings.restaurant_code,
                        language: &state.settings.language,
                        display_mode: state.settings.lunch_item_display_mode,
                        show_allergens: state.settings.show_allergens,
                        highlight_gluten_free: state.settings.highlight_gluten_free,
                        highlight_veg: state.settings.highlight_veg,
                        highlight_lactose_free: state.settings.highlight_lactose_free,
                    },
                );
                if rendered_groups == 0 && state.status != FetchStatus::Loading {
                    push_no_menu_or_closure_notice(
                        &mut lines,
                        closure_notice.as_deref(),
                        &state.settings.language,
                    );
                }
            } else if state.status != FetchStatus::Loading {
                push_no_menu_or_closure_notice(
                    &mut lines,
                    closure_notice.as_deref(),
                    &state.settings.language,
                );
            }
        }
        None => {
            if state.status != FetchStatus::Loading {
                push_no_menu_or_closure_notice(
                    &mut lines,
                    closure_notice.as_deref(),
                    &state.settings.language,
                );
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
        lines.push(Line::StatusText(text_for(
            &state.settings.language,
            stale_key,
        )));
    }

    if !state.error_message.is_empty() && state.status != FetchStatus::Ok {
        lines.push(Line::StatusText(format!(
            "{}: {}",
            text_for(&state.settings.language, "fetchError"),
            state.error_message
        )));
    }

    lines
}

fn push_no_menu_or_closure_notice(lines: &mut Vec<Line>, notice: Option<&str>, language: &str) {
    if let Some(notice) = notice {
        lines.push(Line::ClosureNotice(notice.to_string()));
    } else {
        lines.push(Line::Text(text_for(language, "noMenu")));
    }
}

#[derive(Debug, Clone, Copy)]
struct MenuRenderOptions<'a> {
    provider: Provider,
    show_prices: bool,
    price_groups: PriceGroups,
    restaurant_code: &'a str,
    language: &'a str,
    display_mode: crate::settings::LunchItemDisplayMode,
    show_allergens: bool,
    highlight_gluten_free: bool,
    highlight_veg: bool,
    highlight_lactose_free: bool,
}

fn append_menus(lines: &mut Vec<Line>, menu: &TodayMenu, options: MenuRenderOptions) -> usize {
    let mut rendered_groups = 0;
    let expanded_recipe_key = super::interaction::expanded_recipe_key();
    let favorites = current_favorites_snapshot();
    let mut recipe_instance_id = 0usize;
    let mut groups: Vec<RenderableGroup<'_>> = menu
        .menus
        .iter()
        .enumerate()
        .filter_map(|(index, group)| renderable_group(index, group, options))
        .collect();
    groups.sort_by(compare_renderable_groups);

    for render_group in groups {
        let group = render_group.group;
        let renderable_components = renderable_group_components(group);
        if renderable_components.is_empty()
            && group.presentation != MenuGroupPresentation::GeneralOffer
        {
            continue;
        }

        let category = render_group.category;
        if group.presentation == MenuGroupPresentation::GeneralOffer {
            lines.push(Line::Heading(render_group.heading));
            for (main, _, _, _) in render_group.components {
                lines.push(Line::Text(main));
            }
            rendered_groups += 1;
            continue;
        }
        if options.display_mode == crate::settings::LunchItemDisplayMode::Classic
            && !render_group.heading.is_empty()
        {
            lines.push(Line::Heading(render_group.heading));
        }
        rendered_groups += 1;
        let mut rendered_component_count = 0usize;
        for (main, suffix, recipe_id, recipe_detail) in render_group.components {
            let recipe_key = recipe_id.map(|recipe_id| {
                let key = RecipeExpansionKey {
                    recipe_id,
                    instance_id: recipe_instance_id,
                };
                recipe_instance_id += 1;
                key
            });
            let is_primary_component = rendered_component_count == 0
                || options.display_mode == crate::settings::LunchItemDisplayMode::Classic;
            let price_prefix = if is_primary_component {
                render_group.price_prefix.clone()
            } else {
                None
            };
            let reserve_prefix = if is_primary_component {
                None
            } else {
                render_group.price_prefix.clone()
            };
            let show_bullet = is_primary_component;
            let ingredient_alert = recipe_detail
                .as_ref()
                .is_some_and(|detail| ingredient_alert_matches(detail, &favorites));
            if !options.show_allergens || suffix.is_empty() {
                lines.push(Line::MenuItem {
                    show_bullet,
                    price_prefix: price_prefix.clone(),
                    reserve_prefix: reserve_prefix.clone(),
                    main: main.clone(),
                    suffix_segments: Vec::new(),
                    recipe_key,
                    ingredient_alert,
                });
            } else {
                let segments = build_suffix_segments(
                    &suffix,
                    options.highlight_gluten_free,
                    options.highlight_veg,
                    options.highlight_lactose_free,
                );
                lines.push(Line::MenuItem {
                    show_bullet,
                    price_prefix: price_prefix.clone(),
                    reserve_prefix: reserve_prefix.clone(),
                    main: main.clone(),
                    suffix_segments: segments,
                    recipe_key,
                    ingredient_alert,
                });
            }
            if recipe_key.is_some() && recipe_key == expanded_recipe_key {
                if let Some(detail) = recipe_detail.as_ref() {
                    let rows = recipe_detail_rows(detail, options.language);
                    if !rows.is_empty() {
                        lines.push(Line::RecipeDetail { rows });
                    }
                }
            }
            rendered_component_count += 1;
        }
        if options.display_mode == crate::settings::LunchItemDisplayMode::Standard
            && !category.is_empty()
        {
            lines.push(Line::Subheading {
                text: category.clone(),
                reserve_prefix: render_group.price_prefix.clone(),
            });
        }
    }
    rendered_groups
}

#[derive(Debug)]
struct RenderableGroup<'a> {
    group: &'a MenuGroup,
    components: Vec<(String, String, Option<u32>, Option<RecipeInfo>)>,
    category: String,
    heading: String,
    price_prefix: Option<String>,
    sort_prices: Vec<f32>,
    original_index: usize,
}

fn renderable_group<'a>(
    original_index: usize,
    group: &'a MenuGroup,
    options: MenuRenderOptions,
) -> Option<RenderableGroup<'a>> {
    let components = renderable_group_components(group);
    if components.is_empty() && group.presentation != MenuGroupPresentation::GeneralOffer {
        return None;
    }
    let category = menu_group_title_for_restaurant(group, options.restaurant_code);
    let heading = menu_heading_for_restaurant(
        group,
        options.restaurant_code,
        options.provider,
        options.show_prices,
        options.price_groups,
    );
    // Standard and Compact draw the price inline, ahead of the dish name on the
    // same row, so the prefix competes with the text it labels. Group names make
    // it roughly three times wider ("Opiskelija 2,95 € / Henkilokunta 6,19 €
    // / Vierailija 6,22 €"), which overruns the row: the renderer then floors the
    // remaining width and clips the dish name away entirely. Classic puts the
    // price on its own heading line and has room for the names.
    let mut price_groups = options.price_groups;
    if options.display_mode != crate::settings::LunchItemDisplayMode::Classic {
        price_groups.names = false;
    }
    let price_text = menu_price_for_restaurant_display(
        group,
        options.restaurant_code,
        options.provider,
        options.show_prices,
        price_groups,
    );
    let price_prefix = if options.display_mode == crate::settings::LunchItemDisplayMode::Classic
        || group.presentation == MenuGroupPresentation::GeneralOffer
        || price_text.is_empty()
    {
        None
    } else {
        Some(format!("{}   ", price_text))
    };
    let sort_prices = if price_text.is_empty() {
        price_values_for_sort(&group.price)
    } else {
        price_values_for_sort(&price_text)
    };

    Some(RenderableGroup {
        group,
        components,
        category,
        heading,
        price_prefix,
        sort_prices,
        original_index,
    })
}

fn compare_renderable_groups(
    left: &RenderableGroup<'_>,
    right: &RenderableGroup<'_>,
) -> std::cmp::Ordering {
    match (
        left.group.presentation == MenuGroupPresentation::GeneralOffer,
        right.group.presentation == MenuGroupPresentation::GeneralOffer,
    ) {
        (true, true) => return left.original_index.cmp(&right.original_index),
        (true, false) => return std::cmp::Ordering::Less,
        (false, true) => return std::cmp::Ordering::Greater,
        (false, false) => {}
    }
    compare_price_vectors_desc(&left.sort_prices, &right.sort_prices)
        .then_with(|| left.original_index.cmp(&right.original_index))
}

fn compare_price_vectors_desc(left: &[f32], right: &[f32]) -> std::cmp::Ordering {
    let max_len = left.len().max(right.len());
    for idx in 0..max_len {
        match (left.get(idx), right.get(idx)) {
            (Some(a), Some(b)) => {
                if let Some(ordering) = b.partial_cmp(a) {
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                }
            }
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            (None, None) => break,
        }
    }
    std::cmp::Ordering::Equal
}

fn ingredient_alert_matches(detail: &RecipeInfo, favorites: &FavoritesSnapshot) -> bool {
    if favorites.ingredient_snippets_lower.is_empty() {
        return false;
    }
    let ingredients = normalize_text(&detail.ingredients_cleaned).to_lowercase();
    if ingredients.is_empty() {
        return false;
    }
    favorites
        .ingredient_snippets_lower
        .iter()
        .any(|snippet| !snippet.is_empty() && ingredients.contains(snippet))
}

fn renderable_group_components(
    group: &MenuGroup,
) -> Vec<(String, String, Option<u32>, Option<RecipeInfo>)> {
    let mut out = Vec::new();
    for (idx, component) in group.components.iter().enumerate() {
        let component = normalize_text(component);
        if component.is_empty() {
            continue;
        }
        let (main, suffix) = split_component_suffix(&component);
        if main.is_empty() {
            continue;
        }
        let recipe_id = group.component_recipe_ids.get(idx).copied().flatten();
        let recipe_detail = group.component_recipe_details.get(idx).cloned().flatten();
        out.push((main, suffix, recipe_id, recipe_detail));
    }
    out
}

fn recipe_detail_rows(detail: &RecipeInfo, language: &str) -> Vec<RecipeDetailRow> {
    let mut rows = Vec::new();
    let ingredients = normalize_text(&detail.ingredients_cleaned);
    if !ingredients.is_empty() {
        rows.push(RecipeDetailRow {
            label: text_for(language, "ingredients"),
            value: ingredients,
            selectable: true,
        });
    }
    let nutrition = compact_nutrition_line(detail);
    if !nutrition.is_empty() {
        rows.push(RecipeDetailRow {
            label: text_for(language, "nutrition"),
            value: nutrition,
            selectable: false,
        });
    }
    if let Some(co2) = detail.kg_co2e_per100g {
        rows.push(RecipeDetailRow {
            label: "CO2e".to_string(),
            value: format!("{:.2} kg / 100 g", co2),
            selectable: false,
        });
    }
    if rows.is_empty() {
        rows.push(RecipeDetailRow {
            label: "Recipe ID".to_string(),
            value: detail.recipe_id.to_string(),
            selectable: false,
        });
    }
    rows
}

fn compact_nutrition_line(detail: &RecipeInfo) -> String {
    let wanted = [
        ("EnergyKcal", ""),
        ("Protein", "protein"),
        ("Carbohydrates", "carbs"),
        ("Fat", "fat"),
    ];
    let mut parts = Vec::new();
    for (key, label) in wanted {
        if let Some(value) = detail
            .nutritional_values
            .iter()
            .find(|entry| entry.name == key)
        {
            let value_text = format!("{} {}", format_amount(value.amount), value.unit);
            parts.push(if label.is_empty() {
                value_text
            } else {
                format!("{} {}", value_text, label)
            });
        }
    }
    parts.join(", ")
}

fn format_amount(value: f32) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
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
        let mut ingredient_snippets_lower = Vec::new();
        for snippet in loaded.snippets {
            let normalized = favorites::normalize_snippet(&snippet);
            if normalized.is_empty() {
                continue;
            }
            snippets_lower.push(normalized.to_lowercase());
        }
        for snippet in loaded.ingredient_snippets {
            let normalized = favorites::normalize_snippet(&snippet);
            if normalized.is_empty() {
                continue;
            }
            ingredient_snippets_lower.push(normalized.to_lowercase());
        }
        cache.snapshot = FavoritesSnapshot {
            snippets_lower,
            ingredient_snippets_lower,
        };
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

fn hours_status(hours: &str) -> HoursStatus {
    let Some(now_minutes) = current_helsinki_minutes() else {
        return HoursStatus::Unknown;
    };
    hours_status_at(hours, now_minutes)
}

fn hours_status_at(hours: &str, now_minutes: u16) -> HoursStatus {
    let Some((opens_at, closes_at)) = parse_hours_interval(hours) else {
        return HoursStatus::Unknown;
    };
    if closes_at <= opens_at {
        return HoursStatus::Unknown;
    }
    if now_minutes < opens_at || now_minutes >= closes_at {
        return HoursStatus::Closed;
    }
    if closes_at.saturating_sub(now_minutes) <= CLOSING_SOON_MINUTES {
        HoursStatus::ClosingSoon
    } else {
        HoursStatus::Open
    }
}

fn parse_hours_interval(hours: &str) -> Option<(u16, u16)> {
    let times = parse_time_tokens(hours);
    if times.len() >= 2 {
        Some((times[0], times[1]))
    } else {
        None
    }
}

fn parse_time_tokens(value: &str) -> Vec<u16> {
    let mut times = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    for idx in 0..chars.len() {
        if !chars[idx].is_ascii_digit() {
            continue;
        }
        if idx > 0 && chars[idx - 1].is_ascii_digit() {
            continue;
        }

        let mut end_hour = idx;
        while end_hour < chars.len() && chars[end_hour].is_ascii_digit() {
            end_hour += 1;
        }
        if end_hour == idx || end_hour - idx > 2 || chars.get(end_hour) != Some(&':') {
            continue;
        }

        let minute_start = end_hour + 1;
        let minute_end = minute_start + 2;
        if minute_end > chars.len()
            || !chars[minute_start].is_ascii_digit()
            || !chars[minute_start + 1].is_ascii_digit()
            || chars.get(minute_end).is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }

        let hour = chars[idx..end_hour]
            .iter()
            .collect::<String>()
            .parse::<u16>()
            .ok();
        let minute = chars[minute_start..minute_end]
            .iter()
            .collect::<String>()
            .parse::<u16>()
            .ok();
        let (Some(hour), Some(minute)) = (hour, minute) else {
            continue;
        };
        if hour < 24 && minute < 60 {
            times.push(hour * 60 + minute);
        }
    }
    times
}

fn current_helsinki_minutes() -> Option<u16> {
    let now = OffsetDateTime::now_utc();
    let offset = helsinki_offset_for(now)?;
    let local_time = now.to_offset(offset).time();
    Some(local_time.hour() as u16 * 60 + local_time.minute() as u16)
}

fn helsinki_offset_for(now: OffsetDateTime) -> Option<UtcOffset> {
    let year = now.year();
    let daylight_saving_start = last_sunday(year, Month::March)
        .with_time(Time::from_hms(1, 0, 0).ok()?)
        .assume_utc();
    let daylight_saving_end = last_sunday(year, Month::October)
        .with_time(Time::from_hms(1, 0, 0).ok()?)
        .assume_utc();
    let offset_hours = if now >= daylight_saving_start && now < daylight_saving_end {
        3
    } else {
        2
    };
    UtcOffset::from_hms(offset_hours, 0, 0).ok()
}

fn last_sunday(year: i32, month: Month) -> Date {
    let mut date =
        Date::from_calendar_date(year, month, 31).expect("March and October have 31 days");
    while date.weekday() != Weekday::Sunday {
        date = date.previous_day().expect("valid previous day");
    }
    date
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MenuGroup, NutritionalValue, TodayMenu};
    use crate::settings::{LunchItemDisplayMode, Settings};

    #[test]
    fn recipe_detail_rows_use_finnish_labels_for_finnish_ui() {
        let detail = RecipeInfo {
            recipe_id: 42,
            name: "Soup".to_string(),
            ingredients_cleaned: "vesi, suola".to_string(),
            nutritional_values: vec![NutritionalValue {
                name: "Protein".to_string(),
                amount: 3.2,
                unit: "g".to_string(),
            }],
            kg_co2e_per100g: None,
            diets: String::new(),
        };

        let rows = recipe_detail_rows(&detail, "fi");

        assert_eq!(rows[0].label, "Ainesosat");
        assert!(rows[0].selectable);
        assert_eq!(rows[1].label, "Ravintoarvot");
        assert!(!rows[1].selectable);
    }

    #[test]
    fn nutrition_does_not_repeat_kcal_unit() {
        let detail = RecipeInfo {
            recipe_id: 42,
            name: "Soup".to_string(),
            ingredients_cleaned: String::new(),
            nutritional_values: vec![NutritionalValue {
                name: "EnergyKcal".to_string(),
                amount: 93.0,
                unit: "kcal".to_string(),
            }],
            kg_co2e_per100g: None,
            diets: String::new(),
        };

        assert_eq!(compact_nutrition_line(&detail), "93 kcal");
    }

    #[test]
    fn stale_date_renders_notice_and_stale_date_time_row() {
        let state = AppState {
            settings: Settings {
                language: "en".to_string(),
                ..Settings::default()
            },
            status: FetchStatus::Ok,
            loading_started_epoch_ms: 0,
            error_message: String::new(),
            stale_network_error: false,
            today_menu: Some(TodayMenu {
                date_iso: "2026-07-28".to_string(),
                lunch_time: "10:30-14:00".to_string(),
                menus: Vec::new(),
            }),
            restaurant_name: "Snellmania".to_string(),
            restaurant_url: String::new(),
            raw_payload: String::new(),
            provider: Provider::LunchApi,
            payload_date: "2026-07-28".to_string(),
            stale_date: true,
            api_stale: false,
        };

        let lines = build_lines(&state);

        assert!(matches!(&lines[0], Line::StaleNotice(text) if text == "Stale menu"));
        assert!(
            matches!(&lines[1], Line::DateTime { date, hours, stale, .. } if date == "28.7.2026" && hours == "10:30-14:00" && *stale)
        );
    }

    #[test]
    fn hours_status_tracks_open_closing_soon_and_closed_intervals() {
        assert_eq!(hours_status_at("10:30-14:00", 12 * 60), HoursStatus::Open);
        assert_eq!(
            hours_status_at("10:30-14:00", 13 * 60 + 45),
            HoursStatus::ClosingSoon
        );
        assert_eq!(hours_status_at("10:30-14:00", 14 * 60), HoursStatus::Closed);
        assert_eq!(hours_status_at("10:30-14:00", 10 * 60), HoursStatus::Closed);
    }

    #[test]
    fn hours_status_parses_prose_with_clear_time_tokens_only() {
        assert_eq!(
            hours_status_at("Lunch 10:30–13:30", 13 * 60 + 20),
            HoursStatus::ClosingSoon
        );
        assert_eq!(
            hours_status_at("Closed until 9 August", 12 * 60),
            HoursStatus::Unknown
        );
        assert_eq!(hours_status_at("10-14", 12 * 60), HoursStatus::Unknown);
    }

    #[test]
    fn classic_layout_renders_heading_then_menu_item() {
        let lines = render_test_lines(LunchItemDisplayMode::Classic);

        assert!(matches!(&lines[0], Line::Heading(text) if text == "Main course - 3,10 €"));
        assert!(
            matches!(&lines[1], Line::MenuItem { main, .. } if main == "Sweet sour tofu and vegetable wok - Tofu Kung Pao")
        );
    }

    #[test]
    fn standard_layout_renders_price_first_item_with_secondary_category() {
        let lines = render_test_lines(LunchItemDisplayMode::Standard);

        assert!(
            matches!(&lines[0], Line::MenuItem { price_prefix, main, .. } if price_prefix.as_deref() == Some("3,10 €   ") && main == "Sweet sour tofu and vegetable wok - Tofu Kung Pao")
        );
        assert!(
            matches!(&lines[1], Line::Subheading { text, reserve_prefix } if text == "Main course" && reserve_prefix.as_deref() == Some("3,10 €   "))
        );
    }

    #[test]
    fn compact_layout_renders_price_first_item_without_category() {
        let lines = render_test_lines(LunchItemDisplayMode::Compact);

        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], Line::MenuItem { price_prefix, main, .. } if price_prefix.as_deref() == Some("3,10 €   ") && main == "Sweet sour tofu and vegetable wok - Tofu Kung Pao")
        );
    }

    #[test]
    fn general_offer_renders_as_heading_and_description() {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: "Lunch".to_string(),
                price: "12,90 €".to_string(),
                prices: Vec::new(),
                presentation: MenuGroupPresentation::GeneralOffer,
                components: vec!["Tofu soup".to_string()],
                component_recipe_ids: Vec::new(),
                component_recipe_details: Vec::new(),
            }],
        };
        let mut lines = Vec::new();
        let mut options = test_options(LunchItemDisplayMode::Standard);
        options.provider = Provider::LunchApi;
        options.restaurant_code = "hyva-huomen-bioteknia";
        append_menus(&mut lines, &menu, options);

        assert!(matches!(&lines[0], Line::Heading(text) if text == "Lunch - 12,90 €"));
        assert!(matches!(&lines[1], Line::Text(text) if text == "Tofu soup"));
    }

    #[test]
    fn closure_notice_uses_dedicated_line_style() {
        let mut lines = Vec::new();

        push_no_menu_or_closure_notice(&mut lines, Some("Closed 24.6.-30.6."), "en");

        assert!(matches!(&lines[0], Line::ClosureNotice(text) if text == "Closed 24.6.-30.6."));
    }

    #[test]
    fn standard_layout_sorts_groups_by_visible_price_descending() {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: String::new(),
            menus: vec![
                test_group("Soup", "student 1,46 €", "Bataattikeittoa"),
                test_group("Lunch", "student 2,95 €", "Rapeaa yrttikalaa"),
                test_group("Dessert", "student 0,66 €", "Suklaamoussea"),
                test_group("Vegetable lunch", "student 1,87 €", "Mifua Margherita"),
            ],
        };
        let mut lines = Vec::new();
        append_menus(
            &mut lines,
            &menu,
            test_options(LunchItemDisplayMode::Standard),
        );

        let mains: Vec<&str> = lines
            .iter()
            .filter_map(|line| match line {
                Line::MenuItem { main, .. } => Some(main.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            mains,
            vec![
                "Rapeaa yrttikalaa",
                "Mifua Margherita",
                "Bataattikeittoa",
                "Suklaamoussea"
            ]
        );
    }

    #[test]
    fn antell_style_prices_sort_by_second_value_when_first_value_ties() {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: String::new(),
            menus: vec![
                test_group("A", "12,50/3,10€", "Lower student price"),
                test_group("B", "12,50/5,90€", "Higher student price"),
                test_group("C", "12,50/3,10€", "Same lower price"),
            ],
        };
        let mut lines = Vec::new();
        let mut options = test_options(LunchItemDisplayMode::Standard);
        options.provider = Provider::LunchApi;
        append_menus(&mut lines, &menu, options);

        let mains: Vec<&str> = lines
            .iter()
            .filter_map(|line| match line {
                Line::MenuItem { main, .. } => Some(main.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            mains,
            vec![
                "Higher student price",
                "Lower student price",
                "Same lower price"
            ]
        );
    }

    #[test]
    fn standard_layout_keeps_multi_component_group_as_one_meal_block() {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: "Main course".to_string(),
                price: "student 3,10 €".to_string(),
                prices: Vec::new(),
                presentation: MenuGroupPresentation::Standard,
                components: vec![
                    "Chicken rissoles".to_string(),
                    "Roasted potatoes".to_string(),
                    "Tzatsiki yoghurt".to_string(),
                ],
                component_recipe_ids: Vec::new(),
                component_recipe_details: Vec::new(),
            }],
        };
        let mut lines = Vec::new();
        append_menus(
            &mut lines,
            &menu,
            test_options(LunchItemDisplayMode::Standard),
        );

        assert!(matches!(
            &lines[0],
            Line::MenuItem {
                show_bullet: true,
                price_prefix,
                reserve_prefix: None,
                main,
                ..
            } if price_prefix.as_deref() == Some("3,10 €   ") && main == "Chicken rissoles"
        ));
        assert!(matches!(
            &lines[1],
            Line::MenuItem {
                show_bullet: false,
                price_prefix: None,
                reserve_prefix,
                main,
                ..
            } if reserve_prefix.as_deref() == Some("3,10 €   ") && main == "Roasted potatoes"
        ));
        assert!(matches!(
            &lines[2],
            Line::MenuItem {
                show_bullet: false,
                price_prefix: None,
                reserve_prefix,
                main,
                ..
            } if reserve_prefix.as_deref() == Some("3,10 €   ") && main == "Tzatsiki yoghurt"
        ));
        assert!(
            matches!(&lines[3], Line::Subheading { text, reserve_prefix } if text == "Main course" && reserve_prefix.as_deref() == Some("3,10 €   "))
        );
    }

    #[test]
    fn duplicate_recipe_ids_expand_only_selected_occurrence() {
        let menu = TodayMenu {
            date_iso: "2026-07-30".to_string(),
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: "Lunch".to_string(),
                price: "student 2,95 €".to_string(),
                prices: Vec::new(),
                presentation: MenuGroupPresentation::Standard,
                components: vec!["Rice".to_string(), "Rice".to_string()],
                component_recipe_ids: vec![Some(42), Some(42)],
                component_recipe_details: vec![
                    Some(test_recipe(42, "first rice ingredients")),
                    Some(test_recipe(42, "second rice ingredients")),
                ],
            }],
        };
        super::super::interaction::set_expanded_recipe_key_for_test(Some(RecipeExpansionKey {
            recipe_id: 42,
            instance_id: 1,
        }));

        let mut lines = Vec::new();
        append_menus(
            &mut lines,
            &menu,
            test_options(LunchItemDisplayMode::Standard),
        );
        super::super::interaction::set_expanded_recipe_key_for_test(None);

        let recipe_details: Vec<&Vec<RecipeDetailRow>> = lines
            .iter()
            .filter_map(|line| match line {
                Line::RecipeDetail { rows } => Some(rows),
                _ => None,
            })
            .collect();

        assert_eq!(recipe_details.len(), 1);
        assert_eq!(recipe_details[0][0].value, "second rice ingredients");
        assert!(matches!(&lines[0], Line::MenuItem { main, .. } if main == "Rice"));
        assert!(matches!(&lines[1], Line::MenuItem { main, .. } if main == "Rice"));
        assert!(matches!(&lines[2], Line::RecipeDetail { .. }));
    }

    fn render_test_lines(display_mode: LunchItemDisplayMode) -> Vec<Line> {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: "10:30-13:30".to_string(),
            menus: vec![MenuGroup {
                name: "Main course".to_string(),
                price: "student 3,10 € / staff 8,50 €".to_string(),
                prices: Vec::new(),
                presentation: MenuGroupPresentation::Standard,
                components: vec!["Sweet sour tofu and vegetable wok - Tofu Kung Pao".to_string()],
                component_recipe_ids: Vec::new(),
                component_recipe_details: Vec::new(),
            }],
        };
        let mut lines = Vec::new();
        append_menus(&mut lines, &menu, test_options(display_mode));
        lines
    }

    fn all_groups_named(display_mode: LunchItemDisplayMode) -> MenuRenderOptions<'static> {
        let mut options = test_options(display_mode);
        options.price_groups = PriceGroups {
            student: true,
            staff: true,
            guest: true,
            names: true,
        };
        options
    }

    #[test]
    fn inline_price_prefix_drops_group_names_outside_classic() {
        // Named prices for all three groups are wider than the popup, and the
        // inline prefix competes with the dish name on the same row: the
        // renderer floored the leftover width and clipped the name away.
        for mode in [
            LunchItemDisplayMode::Standard,
            LunchItemDisplayMode::Compact,
        ] {
            let group = test_group("Lounas", "Opiskelija 2,95 € / Henkilökunta 6,19 €", "Soup");
            let renderable =
                renderable_group(0, &group, all_groups_named(mode)).expect("renderable group");
            let prefix = renderable.price_prefix.expect("inline price prefix");
            assert!(
                !prefix.contains("Opiskelija"),
                "{mode:?} kept group names in the inline prefix: {prefix}"
            );
        }
    }

    #[test]
    fn classic_heading_still_shows_group_names() {
        // Classic puts the price on its own heading line, so it has the room.
        let group = test_group("Lounas", "Opiskelija 2,95 € / Henkilökunta 6,19 €", "Soup");
        let renderable =
            renderable_group(0, &group, all_groups_named(LunchItemDisplayMode::Classic))
                .expect("renderable group");
        assert!(renderable.price_prefix.is_none());
        assert!(renderable.heading.contains("Opiskelija"));
    }

    fn test_options(display_mode: LunchItemDisplayMode) -> MenuRenderOptions<'static> {
        MenuRenderOptions {
            provider: Provider::Compass,
            show_prices: true,
            price_groups: PriceGroups {
                student: true,
                staff: false,
                guest: false,
                names: false,
            },
            restaurant_code: "0437",
            language: "en",
            display_mode,
            show_allergens: true,
            highlight_gluten_free: false,
            highlight_veg: false,
            highlight_lactose_free: false,
        }
    }

    fn test_group(name: &str, price: &str, component: &str) -> MenuGroup {
        MenuGroup {
            name: name.to_string(),
            price: price.to_string(),
            prices: Vec::new(),
            presentation: MenuGroupPresentation::Standard,
            components: vec![component.to_string()],
            component_recipe_ids: Vec::new(),
            component_recipe_details: Vec::new(),
        }
    }

    fn test_recipe(recipe_id: u32, ingredients: &str) -> RecipeInfo {
        RecipeInfo {
            recipe_id,
            name: "Rice".to_string(),
            ingredients_cleaned: ingredients.to_string(),
            nutritional_values: Vec::new(),
            kg_co2e_per100g: None,
            diets: String::new(),
        }
    }
}
