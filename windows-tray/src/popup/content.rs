use super::*;

pub(super) fn build_lines(state: &AppState) -> Vec<Line> {
    let mut lines = Vec::new();
    let closure_notice = seasonal_closure_notice(
        &state.settings.restaurant_code,
        &state.settings.language,
        OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .date(),
    );

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
                        hide_expensive_student_meals: state.settings.hide_expensive_student_meals,
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

fn push_no_menu_or_closure_notice(lines: &mut Vec<Line>, notice: Option<&str>, language: &str) {
    if let Some(notice) = notice {
        lines.push(Line::Text(notice.to_string()));
    } else {
        lines.push(Line::Text(text_for(language, "noMenu")));
    }
}

fn seasonal_closure_notice(code: &str, language: &str, today: time::Date) -> Option<String> {
    let closure = seasonal_closure_for(code)?;
    if today < closure.start || today > closure.end {
        return None;
    }
    let start = closure_display_date(closure.start);
    let end = closure_display_date(closure.end);
    if language == "fi" {
        Some(format!(
            "{} on suljettu ajalla {}-{}. Lounaslistaa ei ole saatavilla tälle päivälle.",
            closure.fi_name, start, end
        ))
    } else {
        Some(format!(
            "{} is closed from {} to {}. No lunch menu is available for today.",
            closure.en_name, start, end
        ))
    }
}

struct SeasonalClosure {
    start: time::Date,
    end: time::Date,
    fi_name: &'static str,
    en_name: &'static str,
}

fn seasonal_closure_for(code: &str) -> Option<SeasonalClosure> {
    use time::Month;
    let date = |month, day| time::Date::from_calendar_date(2026, month, day).ok();
    let (start, end, fi_name, en_name) = match code {
        "043601" => (
            date(Month::May, 4)?,
            date(Month::August, 16)?,
            "Ravintola Mediteknia",
            "Restaurant Mediteknia",
        ),
        "3488" => (
            date(Month::June, 9)?,
            date(Month::August, 9)?,
            "Caari",
            "Caari",
        ),
        "snellari-rss" => (
            date(Month::May, 8)?,
            date(Month::August, 30)?,
            "Cafe Snellari",
            "Cafe Snellari",
        ),
        "0437" => (
            date(Month::July, 4)?,
            date(Month::July, 19)?,
            "Snellmania",
            "Snellmania",
        ),
        "0436" => (
            date(Month::June, 18)?,
            date(Month::August, 9)?,
            "Canthia",
            "Canthia",
        ),
        "antell-highway" => (
            date(Month::June, 22)?,
            date(Month::August, 2)?,
            "Ravintola Antell Highway",
            "Restaurant Antell Highway",
        ),
        "huomen-bioteknia" => (
            date(Month::July, 6)?,
            date(Month::August, 2)?,
            "Ravintola Hyvä Huomen",
            "Restaurant Hyvä Huomen",
        ),
        "antell-round" => (
            date(Month::June, 29)?,
            date(Month::August, 2)?,
            "Ravintola Antell Round",
            "Restaurant Antell Round",
        ),
        _ => return None,
    };
    Some(SeasonalClosure {
        start,
        end,
        fi_name,
        en_name,
    })
}

fn closure_display_date(date: time::Date) -> String {
    format!(
        "{:02}-{:02}-{:04}",
        date.day(),
        date.month() as u8,
        date.year()
    )
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
    hide_expensive_student_meals: bool,
}

fn append_menus(lines: &mut Vec<Line>, menu: &TodayMenu, options: MenuRenderOptions) -> usize {
    let mut rendered_groups = 0;
    let expanded_recipe_id = super::interaction::expanded_recipe_id();
    let favorites = current_favorites_snapshot();
    let mut groups: Vec<RenderableGroup<'_>> = menu
        .menus
        .iter()
        .enumerate()
        .filter_map(|(index, group)| renderable_group(index, group, options))
        .collect();
    groups.sort_by(compare_renderable_groups);

    for render_group in groups {
        let group = render_group.group;
        if options.provider == Provider::Compass && options.hide_expensive_student_meals {
            if let Some(price) = student_price_eur(&group.price) {
                if price > 4.0 {
                    continue;
                }
            }
        }

        let renderable_components = renderable_group_components(group);
        if renderable_components.is_empty() {
            continue;
        }

        let category = render_group.category;
        if options.display_mode == crate::settings::LunchItemDisplayMode::Legacy {
            lines.push(Line::Heading(render_group.heading));
        }
        if price_note_style(options.provider) && !render_group.price_text.is_empty() {
            lines.push(Line::Subheading {
                text: translate_price_note(
                    options.provider,
                    &render_group.price_text,
                    options.language,
                ),
                reserve_prefix: None,
            });
        }
        rendered_groups += 1;
        let list_components = options.provider == Provider::PranzeriaHtml;
        let mut rendered_component_count = 0usize;
        for (main, suffix, recipe_id, recipe_detail) in render_group.components {
            let is_primary_component = rendered_component_count == 0
                || list_components
                || options.display_mode == crate::settings::LunchItemDisplayMode::Legacy;
            let price_prefix = if is_primary_component {
                if list_components || price_note_style(options.provider) {
                    None
                } else {
                    render_group.price_prefix.clone()
                }
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
                    recipe_id,
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
                    recipe_id,
                    ingredient_alert,
                });
            }
            if recipe_id.is_some() && recipe_id == expanded_recipe_id {
                if let Some(detail) = recipe_detail.as_ref() {
                    let rows = recipe_detail_rows(detail, options.language);
                    if !rows.is_empty() {
                        lines.push(Line::RecipeDetail { rows });
                    }
                }
            }
            rendered_component_count += 1;
        }
        if options.display_mode == crate::settings::LunchItemDisplayMode::Standard {
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
    price_text: String,
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
    if components.is_empty() {
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
    let price_text = menu_price_for_restaurant_display(
        group,
        options.restaurant_code,
        options.provider,
        options.show_prices,
        options.price_groups,
    );
    let price_prefix = if options.display_mode == crate::settings::LunchItemDisplayMode::Legacy
        || price_note_style(options.provider)
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
        price_text: price_text.clone(),
        price_prefix,
        sort_prices,
        original_index,
    })
}

fn compare_renderable_groups(
    left: &RenderableGroup<'_>,
    right: &RenderableGroup<'_>,
) -> std::cmp::Ordering {
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

fn price_note_style(provider: Provider) -> bool {
    matches!(provider, Provider::PranzeriaHtml | Provider::HuomenJson)
}

fn translate_price_note(provider: Provider, price_text: &str, language: &str) -> String {
    if provider == Provider::PranzeriaHtml {
        return translate_pranzeria_price_summary(price_text, language);
    }
    normalize_text(price_text)
}

fn translate_pranzeria_price_summary(price_text: &str, language: &str) -> String {
    let clean = normalize_text(price_text);
    if language != "en" {
        return clean;
    }
    clean
        .replace("Salaattilounas", "Salad lunch")
        .replace("Lounasbuffet", "Lunch buffet")
        .replace("Sopimuslounas", "Contract lunch")
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
        ("EnergyKcal", "kcal"),
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
            parts.push(format!(
                "{} {} {}",
                format_amount(value.amount),
                value.unit,
                label
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MenuGroup, NutritionalValue, TodayMenu};
    use crate::settings::LunchItemDisplayMode;

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
    fn legacy_layout_renders_heading_then_menu_item() {
        let lines = render_test_lines(LunchItemDisplayMode::Legacy);

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
    fn huomen_price_renders_as_note_not_item_prefix() {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: "Lunch".to_string(),
                price: "Lunch 12,90 € / Soup lunch 10,90 €".to_string(),
                components: vec!["Tofu soup".to_string()],
                component_recipe_ids: Vec::new(),
                component_recipe_details: Vec::new(),
            }],
        };
        let mut lines = Vec::new();
        let mut options = test_options(LunchItemDisplayMode::Standard);
        options.provider = Provider::HuomenJson;
        options.restaurant_code = "huomen-bioteknia";
        append_menus(&mut lines, &menu, options);

        assert!(
            matches!(&lines[0], Line::Subheading { text, reserve_prefix } if text == "Lunch 12,90 € / Soup lunch 10,90 €" && reserve_prefix.is_none())
        );
        assert!(
            matches!(&lines[1], Line::MenuItem { price_prefix, main, .. } if price_prefix.is_none() && main == "Tofu soup")
        );
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
        options.provider = Provider::Antell;
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
    fn seasonal_closure_notice_is_limited_to_2026_interval() {
        use time::Month;
        let inside = time::Date::from_calendar_date(2026, Month::June, 26).expect("valid date");
        let outside = time::Date::from_calendar_date(2026, Month::August, 17).expect("valid date");

        let notice = seasonal_closure_notice("043601", "en", inside).expect("notice");

        assert!(notice.contains("Restaurant Mediteknia is closed"));
        assert!(notice.contains("04-05-2026"));
        assert!(notice.contains("16-08-2026"));
        assert!(seasonal_closure_notice("043601", "en", outside).is_none());
    }

    #[test]
    fn seasonal_closure_notice_localizes_finnish() {
        use time::Month;
        let inside = time::Date::from_calendar_date(2026, Month::June, 26).expect("valid date");
        let notice = seasonal_closure_notice("snellari-rss", "fi", inside).expect("notice");

        assert!(notice.contains("Cafe Snellari on suljettu"));
        assert!(notice.contains("08-05-2026"));
        assert!(notice.contains("30-08-2026"));

        let snellmania = time::Date::from_calendar_date(2026, Month::July, 4).expect("valid date");
        let notice = seasonal_closure_notice("0437", "fi", snellmania).expect("notice");

        assert!(notice.contains("Snellmania on suljettu"));
        assert!(notice.contains("04-07-2026"));
        assert!(notice.contains("19-07-2026"));
    }

    #[test]
    fn seasonal_closure_notice_covers_snellmania_summer_2026_in_english() {
        use time::Month;
        let inside = time::Date::from_calendar_date(2026, Month::July, 19).expect("valid date");
        let outside = time::Date::from_calendar_date(2026, Month::July, 20).expect("valid date");

        let notice = seasonal_closure_notice("0437", "en", inside).expect("notice");

        assert!(notice.contains("Snellmania is closed"));
        assert!(notice.contains("04-07-2026"));
        assert!(notice.contains("19-07-2026"));
        assert!(seasonal_closure_notice("0437", "en", outside).is_none());
    }

    #[test]
    fn seasonal_closure_notice_covers_antell_and_huomen_summer_2026() {
        use time::Month;
        let highway = time::Date::from_calendar_date(2026, Month::June, 22).expect("valid date");
        let huomen = time::Date::from_calendar_date(2026, Month::July, 6).expect("valid date");
        let round = time::Date::from_calendar_date(2026, Month::June, 29).expect("valid date");

        assert!(seasonal_closure_notice("antell-highway", "en", highway)
            .expect("notice")
            .contains("22-06-2026"));
        assert!(seasonal_closure_notice("huomen-bioteknia", "fi", huomen)
            .expect("notice")
            .contains("06-07-2026"));
        assert!(seasonal_closure_notice("antell-round", "en", round)
            .expect("notice")
            .contains("29-06-2026"));
    }

    fn render_test_lines(display_mode: LunchItemDisplayMode) -> Vec<Line> {
        let menu = TodayMenu {
            date_iso: "2026-06-24".to_string(),
            lunch_time: "10:30-13:30".to_string(),
            menus: vec![MenuGroup {
                name: "Main course".to_string(),
                price: "student 3,10 € / staff 8,50 €".to_string(),
                components: vec!["Sweet sour tofu and vegetable wok - Tofu Kung Pao".to_string()],
                component_recipe_ids: Vec::new(),
                component_recipe_details: Vec::new(),
            }],
        };
        let mut lines = Vec::new();
        append_menus(&mut lines, &menu, test_options(display_mode));
        lines
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
            hide_expensive_student_meals: false,
        }
    }

    fn test_group(name: &str, price: &str, component: &str) -> MenuGroup {
        MenuGroup {
            name: name.to_string(),
            price: price.to_string(),
            components: vec![component.to_string()],
            component_recipe_ids: Vec::new(),
            component_recipe_details: Vec::new(),
        }
    }
}
