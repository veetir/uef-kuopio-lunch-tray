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
                        show_allergens: state.settings.show_allergens,
                        highlight_gluten_free: state.settings.highlight_gluten_free,
                        highlight_veg: state.settings.highlight_veg,
                        highlight_lactose_free: state.settings.highlight_lactose_free,
                        hide_expensive_student_meals: state.settings.hide_expensive_student_meals,
                    },
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

#[derive(Debug, Clone, Copy)]
struct MenuRenderOptions<'a> {
    provider: Provider,
    show_prices: bool,
    price_groups: PriceGroups,
    restaurant_code: &'a str,
    language: &'a str,
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
    for group in &menu.menus {
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

        let heading = menu_heading_for_restaurant(
            group,
            &options.restaurant_code,
            options.provider,
            options.show_prices,
            options.price_groups,
        );
        lines.push(Line::Heading(heading));
        rendered_groups += 1;
        for (main, suffix, recipe_id, recipe_detail) in renderable_components {
            let ingredient_alert = recipe_detail
                .as_ref()
                .is_some_and(|detail| ingredient_alert_matches(detail, &favorites));
            if !options.show_allergens || suffix.is_empty() {
                lines.push(Line::MenuItem {
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
        }
    }
    rendered_groups
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
    use crate::model::NutritionalValue;

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
}
