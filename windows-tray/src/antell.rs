//! Antell HTML parsing helpers.

use crate::format::{normalize_text, split_component_suffix};
use crate::model::{MenuGroup, NutritionalValue, RecipeInfo, TodayMenu};
use html_escape::decode_html_entities;
use scraper::{Html, Selector};
use std::collections::HashMap;

fn element_text(element: &scraper::element_ref::ElementRef) -> String {
    let raw = element.text().collect::<Vec<_>>().join(" ");
    let decoded = decode_html_entities(&raw);
    normalize_text(decoded.as_ref())
}

/// Parses an Antell lunch page into the normalized menu format used by the popup.
pub fn parse_antell_html(html: &str, today_key: &str) -> TodayMenu {
    let document = Html::parse_document(html);
    let section_sel = Selector::parse("section.menu-section").unwrap();
    let title_sel = Selector::parse("h2.menu-title").unwrap();
    let price_sel = Selector::parse("h2.menu-price").unwrap();
    let item_sel = Selector::parse("ul.menu-list > li").unwrap();

    let mut menus = Vec::new();

    for section in document.select(&section_sel) {
        let items: Vec<String> = section
            .select(&item_sel)
            .map(|item| {
                let raw = item.text().collect::<Vec<_>>().join(" ");
                let decoded = decode_html_entities(&raw);
                normalize_text(decoded.as_ref())
            })
            .filter(|text| !text.is_empty())
            .collect();
        if items.is_empty() {
            continue;
        }
        let item_count = items.len();

        let name = section
            .select(&title_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_else(|| "Menu".to_string());
        let price = section
            .select(&price_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_default();

        menus.push(MenuGroup {
            name,
            price,
            components: items,
            component_recipe_ids: vec![None; item_count],
            component_recipe_details: vec![None; item_count],
        });
    }

    TodayMenu {
        date_iso: today_key.to_string(),
        lunch_time: String::new(),
        menus,
    }
}

/// Enriches an Antell print-menu parse with best-effort item details from the normal page.
pub fn enrich_antell_menu_details(menu: &mut TodayMenu, html: &str, weekday: &str) {
    let details = parse_antell_detail_lookup(html, weekday);
    if details.is_empty() {
        return;
    }

    for group in &mut menu.menus {
        ensure_detail_slots(group);
        for (idx, component) in group.components.iter().enumerate() {
            let (main, _) = split_component_suffix(component);
            let key = detail_lookup_key(&main);
            let Some(detail) = details.get(&key).cloned() else {
                continue;
            };
            group.component_recipe_ids[idx] = Some(detail.recipe_id);
            group.component_recipe_details[idx] = Some(detail);
        }
    }
}

fn ensure_detail_slots(group: &mut MenuGroup) {
    if group.component_recipe_ids.len() < group.components.len() {
        group
            .component_recipe_ids
            .resize(group.components.len(), None);
    }
    if group.component_recipe_details.len() < group.components.len() {
        group
            .component_recipe_details
            .resize(group.components.len(), None);
    }
}

fn parse_antell_detail_lookup(html: &str, weekday: &str) -> HashMap<String, RecipeInfo> {
    let document = Html::parse_document(html);
    let panel_id = format!("panel-{}", weekday_title(weekday));
    let item_selector = Selector::parse(&format!(
        "section#{} ul.accordion__list > li",
        css_escape_simple(&panel_id)
    ))
    .ok();
    let fallback_item_selector = Selector::parse("ul.accordion__list > li").ok();
    let Some(item_selector) = item_selector.or(fallback_item_selector) else {
        return HashMap::new();
    };
    let button_sel = Selector::parse("button.accordion__button").unwrap();
    let content_sel = Selector::parse("div.accordion__content").unwrap();
    let tooltip_body_sel = Selector::parse("div.tooltip__body").unwrap();
    let diets_sel = Selector::parse("div.accordion__footer__special-diets p").unwrap();
    let paragraph_sel = Selector::parse("p").unwrap();

    let mut out = HashMap::new();
    for item in document.select(&item_selector) {
        let name = item
            .select(&button_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let content = item.select(&content_sel).next();
        let ingredients = item
            .select(&tooltip_body_sel)
            .next()
            .map(|el| element_text(&el))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                content.and_then(|content| {
                    content.select(&paragraph_sel).find_map(|paragraph| {
                        labeled_paragraph_value(&paragraph, &["Ainesosat", "Ingredients"])
                    })
                })
            })
            .unwrap_or_default();
        if ingredients.is_empty() {
            continue;
        }

        let nutrition_line = content
            .and_then(|content| {
                content.select(&paragraph_sel).find_map(|paragraph| {
                    labeled_paragraph_value(
                        &paragraph,
                        &["Ravintoarvot (100 g)", "Nutritional values (100 g)"],
                    )
                })
            })
            .unwrap_or_default();
        let co2_line = content
            .and_then(|content| {
                content.select(&paragraph_sel).find_map(|paragraph| {
                    labeled_paragraph_value(&paragraph, &["Hiilijalanjälki", "Carbon footprint"])
                })
            })
            .unwrap_or_default();
        let diets = item
            .select(&diets_sel)
            .next()
            .map(|el| element_text(&el))
            .unwrap_or_default();

        let recipe_id = stable_antell_recipe_id(&name);
        out.insert(
            detail_lookup_key(&name),
            RecipeInfo {
                recipe_id,
                name,
                ingredients_cleaned: ingredients,
                nutritional_values: parse_antell_nutrition_values(&nutrition_line),
                kg_co2e_per100g: parse_first_number(&co2_line),
                diets,
            },
        );
    }
    out
}

fn labeled_paragraph_value(
    paragraph: &scraper::element_ref::ElementRef,
    labels: &[&str],
) -> Option<String> {
    let text = element_text(paragraph);
    let lower = text.to_lowercase();
    for label in labels {
        let label_lower = label.to_lowercase();
        if let Some(pos) = lower.find(&label_lower) {
            let after_label = pos + label_lower.len();
            let value = text
                .get(after_label..)
                .unwrap_or_default()
                .trim_start_matches([':', ' '])
                .trim();
            if !value.is_empty() {
                return Some(normalize_text(value));
            }
        }
    }
    None
}

fn parse_antell_nutrition_values(text: &str) -> Vec<NutritionalValue> {
    let mut values = Vec::new();
    for part in text.split(',').map(normalize_text) {
        let lower = part.to_lowercase();
        let Some(amount) = parse_first_number(&part) else {
            continue;
        };
        let unit = if lower.contains("kcal") { "kcal" } else { "g" }.to_string();
        let name =
            if lower.contains("kcal") || lower.contains("energia") || lower.contains("energy") {
                "EnergyKcal"
            } else if lower.contains("hiilihydra")
                || lower.contains("carbohydrate")
                || lower.contains("carbs")
            {
                "Carbohydrates"
            } else if lower.contains("proteiin") || lower.contains("protein") {
                "Protein"
            } else if (lower.contains("rasva") || lower.contains("fat"))
                && !lower.contains("tyydytt")
                && !lower.contains("saturated")
            {
                "Fat"
            } else {
                continue;
            };
        if values
            .iter()
            .any(|entry: &NutritionalValue| entry.name == name)
        {
            continue;
        }
        values.push(NutritionalValue {
            name: name.to_string(),
            amount,
            unit,
        });
    }
    values
}

fn parse_first_number(text: &str) -> Option<f32> {
    let mut number = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == ',' || ch == '.' {
            number.push(if ch == ',' { '.' } else { ch });
        } else if !number.is_empty() {
            break;
        }
    }
    if number.is_empty() {
        None
    } else {
        number.parse::<f32>().ok()
    }
}

fn detail_lookup_key(value: &str) -> String {
    normalize_text(value).to_lowercase()
}

fn stable_antell_recipe_id(name: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in detail_lookup_key(name).bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash.max(1)
}

fn weekday_title(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn css_escape_simple(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{enrich_antell_menu_details, parse_antell_html};

    #[test]
    fn enriches_antell_print_menu_with_normal_page_details() {
        let print_html = r#"
            <section class="menu-section">
              <div class="menu-section__header">
                <h2 class="menu-title">Kotiruokalounas</h2>
                <h2 class="menu-price">12,50/3,10€</h2>
              </div>
              <ul class="menu-list">
                <li>Jauhelihatacoja (G, L, M)</li>
              </ul>
            </section>
        "#;
        let detail_html = r#"
            <section id="panel-Tuesday" class="tabpanel">
              <ul class="accordion__list">
                <li>
                  <div class="accordion">
                    <button class="accordion__button">Jauhelihatacoja</button>
                    <div class="accordion__content" hidden>
                      <p><b>Allergeenit</b>: Ei allergeeneja</p>
                      <p class="nutritional-values"><b>Ravintoarvot (100 g)</b>: 160.34 kcal energiaa, 14.88 g hiilihydraattia, 7.88 g proteiinia, 7.22 g rasvaa</p>
                      <p><b>Hiilijalanjälki</b>: 0.29 CO₂ e kg/100g</p>
                      <p><b>Ainesosat</b>: Lyhyt lista</p>
                      <div class="tooltip"><div class="tooltip__body">Pitkä ainesosalista</div></div>
                    </div>
                  </div>
                  <div class="accordion__footer"><div class="accordion__footer__special-diets"><p>G, L, M</p></div></div>
                </li>
              </ul>
            </section>
        "#;

        let mut menu = parse_antell_html(print_html, "2026-06-23");
        enrich_antell_menu_details(&mut menu, detail_html, "tuesday");

        let group = &menu.menus[0];
        assert!(group.component_recipe_ids[0].is_some());
        let detail = group.component_recipe_details[0].as_ref().unwrap();
        assert_eq!(detail.name, "Jauhelihatacoja");
        assert_eq!(detail.ingredients_cleaned, "Pitkä ainesosalista");
        assert_eq!(detail.kg_co2e_per100g, Some(0.29));
        assert_eq!(detail.diets, "G, L, M");
        assert!(detail
            .nutritional_values
            .iter()
            .any(|entry| entry.name == "Protein" && (entry.amount - 7.88).abs() < 0.01));
    }
}
