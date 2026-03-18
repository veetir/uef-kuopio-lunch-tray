use super::{
    local_today_key, log_fetch_attempt, parse_date_iso, strip_html_text, FetchContext, FetchOutput,
};
use crate::format::{normalize_optional, normalize_text};
use crate::model::{ApiResponse, ApiSetMenu, MenuGroup, TodayMenu};
use crate::restaurant::{compass_fetch_language, Provider, Restaurant};
use crate::settings::Settings;
use anyhow::Context;
use html_escape::decode_html_entities;
use regex::Regex;
use reqwest::blocking::Client;

pub(super) fn fetch_compass(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
    let fetch_language = compass_fetch_language(restaurant, &settings.language);
    let url = format!(
        "https://www.compass-group.fi/menuapi/feed/json?costNumber={}&language={}",
        restaurant.code, fetch_language
    );
    log_fetch_attempt(
        context,
        restaurant,
        &settings.language,
        fetch_language,
        &url,
    );
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: String::new(),
                restaurant_url: String::new(),
                provider: Provider::Compass,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    let response = client.get(&url).send();
    let mut raw_json = String::new();
    let api: ApiResponse = match response {
        Ok(resp) => match resp.text() {
            Ok(text) => {
                raw_json = text.clone();
                match serde_json::from_str(&text) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        return FetchOutput {
                            ok: false,
                            error_message: err.to_string(),
                            today_menu: None,
                            restaurant_name: String::new(),
                            restaurant_url: String::new(),
                            provider: Provider::Compass,
                            raw_json,
                            payload_date: String::new(),
                        };
                    }
                }
            }
            Err(err) => {
                return FetchOutput {
                    ok: false,
                    error_message: err.to_string(),
                    today_menu: None,
                    restaurant_name: String::new(),
                    restaurant_url: String::new(),
                    provider: Provider::Compass,
                    raw_json,
                    payload_date: String::new(),
                };
            }
        },
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: String::new(),
                restaurant_url: String::new(),
                provider: Provider::Compass,
                raw_json,
                payload_date: String::new(),
            };
        }
    };

    parse_response(api, raw_json)
}

pub(super) fn fetch_compass_rss(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
    let rss_cost_number = match restaurant.rss_cost_number {
        Some(value) if !value.trim().is_empty() => value.trim(),
        _ => {
            return FetchOutput {
                ok: false,
                error_message: "Missing RSS cost number".to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::CompassRss,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    let url = format!(
        "https://www.compass-group.fi/menuapi/feed/rss/current-day?costNumber={}&language={}",
        rss_cost_number, settings.language
    );
    log_fetch_attempt(
        context,
        restaurant,
        &settings.language,
        &settings.language,
        &url,
    );

    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::CompassRss,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    match client.get(&url).send() {
        Ok(resp) => match resp.text() {
            Ok(text) => parse_compass_rss_payload(&text, restaurant, &settings.language),
            Err(err) => FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::CompassRss,
                raw_json: String::new(),
                payload_date: String::new(),
            },
        },
        Err(err) => FetchOutput {
            ok: false,
            error_message: err.to_string(),
            today_menu: None,
            restaurant_name: restaurant.name.to_string(),
            restaurant_url: restaurant.url.unwrap_or_default().to_string(),
            provider: Provider::CompassRss,
            raw_json: String::new(),
            payload_date: String::new(),
        },
    }
}

pub(super) fn parse_cached_compass_payload(raw_payload: &str) -> anyhow::Result<FetchOutput> {
    let api: ApiResponse = serde_json::from_str(raw_payload).context("parse cached JSON")?;
    Ok(parse_response(api, raw_payload.to_string()))
}

fn parse_response(api: ApiResponse, raw_json: String) -> FetchOutput {
    let error_text = normalize_optional(api.error_text.as_deref());
    if !error_text.is_empty() {
        return FetchOutput {
            ok: false,
            error_message: error_text,
            today_menu: None,
            restaurant_name: normalize_optional(api.restaurant_name.as_deref()),
            restaurant_url: normalize_optional(api.restaurant_url.as_deref()),
            provider: Provider::Compass,
            raw_json,
            payload_date: String::new(),
        };
    }

    let today_key = local_today_key();
    let menus_for_days = api.menus_for_days.unwrap_or_default();
    let mut today_menu: Option<TodayMenu> = None;
    let mut fallback_payload_date = String::new();
    let mut payload_date = String::new();

    for day in menus_for_days {
        let date_key = normalize_optional(day.date.as_deref())
            .split('T')
            .next()
            .unwrap_or("")
            .to_string();
        if !date_key.is_empty()
            && (fallback_payload_date.is_empty() || date_key > fallback_payload_date)
        {
            fallback_payload_date = date_key.clone();
        }
        if date_key == today_key {
            let lunch_time = normalize_optional(day.lunch_time.as_deref());
            let set_menus = day.set_menus.unwrap_or_default();
            let menus = normalize_menus(set_menus);
            today_menu = Some(TodayMenu {
                date_iso: today_key.clone(),
                lunch_time,
                menus,
            });
            payload_date = today_key.clone();
            break;
        }
    }

    if payload_date.is_empty() {
        payload_date = fallback_payload_date;
    }

    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu,
        restaurant_name: normalize_optional(api.restaurant_name.as_deref()),
        restaurant_url: normalize_optional(api.restaurant_url.as_deref()),
        provider: Provider::Compass,
        raw_json,
        payload_date,
    }
}

fn normalize_menus(set_menus: Vec<ApiSetMenu>) -> Vec<MenuGroup> {
    let mut menus_with_idx: Vec<(usize, ApiSetMenu)> = set_menus.into_iter().enumerate().collect();
    let has_sort = menus_with_idx.iter().any(|(_, m)| m.sort_order.is_some());
    if has_sort {
        menus_with_idx
            .sort_by_key(|(idx, menu)| (menu.sort_order.unwrap_or(*idx as i32), *idx as i32));
    }
    menus_with_idx
        .into_iter()
        .map(|(_, menu)| MenuGroup {
            name: normalize_optional(menu.name.as_deref()),
            price: normalize_optional(menu.price.as_deref()),
            components: menu
                .components
                .unwrap_or_default()
                .into_iter()
                .map(|c| normalize_text(&c))
                .filter(|c| !c.is_empty())
                .collect(),
        })
        .collect()
}

pub(super) fn parse_compass_rss_payload(
    xml_text: &str,
    restaurant: Restaurant,
    language: &str,
) -> FetchOutput {
    let payload_text = String::from(xml_text);
    let channel_raw = parse_rss_tag_raw(&payload_text, "channel");
    let search_base = if channel_raw.is_empty() {
        payload_text.as_str()
    } else {
        channel_raw.as_str()
    };
    let item_raw = parse_rss_item_raw(search_base);

    let channel_title = strip_html_text(&parse_rss_tag_raw(search_base, "title"));
    let item_title = strip_html_text(&parse_rss_tag_raw(&item_raw, "title"));
    let item_guid = strip_html_text(&parse_rss_tag_raw(&item_raw, "guid"));
    let item_link = strip_html_text(&parse_rss_tag_raw(&item_raw, "link"));
    let description_raw = parse_rss_tag_raw(&item_raw, "description");

    let mut menu_date_iso = parse_rss_menu_date_iso(&item_title);
    if menu_date_iso.is_empty() {
        menu_date_iso = parse_rss_menu_date_iso(&item_guid);
    }

    let today = local_today_key();
    let is_date_today = !menu_date_iso.is_empty() && menu_date_iso == today;
    let components = parse_rss_components(&description_raw);

    let restaurant_name = if !channel_title.is_empty() {
        channel_title
    } else {
        restaurant.name.to_string()
    };
    let restaurant_url = if !item_link.is_empty() {
        item_link
    } else {
        restaurant.url.unwrap_or_default().to_string()
    };

    let today_menu = if is_date_today {
        Some(TodayMenu {
            date_iso: today,
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: if language == "fi" {
                    "Lounas".to_string()
                } else {
                    "Lunch".to_string()
                },
                price: String::new(),
                components,
            }],
        })
    } else {
        None
    };

    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu,
        restaurant_name,
        restaurant_url,
        provider: Provider::CompassRss,
        raw_json: payload_text,
        payload_date: menu_date_iso,
    }
}

fn parse_rss_tag_raw(xml_text: &str, tag_name: &str) -> String {
    let pattern = format!(
        r"(?is)<{}(?:\s+[^>]*)?>([\s\S]*?)</{}>",
        regex::escape(tag_name),
        regex::escape(tag_name)
    );
    Regex::new(&pattern)
        .ok()
        .and_then(|re| re.captures(xml_text))
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn parse_rss_item_raw(xml_text: &str) -> String {
    Regex::new(r"(?is)<item\b[^>]*>([\s\S]*?)</item>")
        .ok()
        .and_then(|re| re.captures(xml_text))
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn parse_rss_menu_date_iso(date_text: &str) -> String {
    parse_date_iso(date_text, r"(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})").unwrap_or_default()
}

fn is_rss_allergen_token(token: &str) -> bool {
    let clean = normalize_text(token)
        .trim_end_matches(['.', ';', ':'])
        .to_string();
    if clean.is_empty() {
        return false;
    }
    if clean == "*" {
        return true;
    }

    if clean.len() == 1 && clean.chars().all(|ch| ch.is_ascii_uppercase()) {
        return true;
    }

    let upper = clean.to_uppercase();
    upper == "VEG" || upper == "VS" || upper == "ILM"
}

fn normalize_rss_allergen_token(token: &str) -> String {
    let clean = normalize_text(token)
        .trim_end_matches(['.', ';', ':'])
        .to_string();
    if clean.is_empty() {
        return String::new();
    }
    if clean == "*" {
        return "*".to_string();
    }

    let upper = clean.to_uppercase();
    if upper == "VEG" {
        "Veg".to_string()
    } else {
        upper
    }
}

fn normalize_rss_component_line(raw_line: &str) -> String {
    let line = normalize_text(raw_line);
    if line.is_empty() {
        return String::new();
    }

    if Regex::new(r"\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)\s*$")
        .ok()
        .is_some_and(|re| re.is_match(&line))
    {
        return line;
    }

    let compact = Regex::new(r"\s*[;,]\s*$")
        .ok()
        .map(|re| re.replace(&line, "").to_string())
        .unwrap_or_else(|| line.clone());

    let parts: Vec<String> = compact
        .split(',')
        .map(normalize_text)
        .filter(|segment| !segment.is_empty())
        .collect();

    if parts.len() < 2 {
        return compact;
    }

    let mut suffix_tokens: Vec<String> = Vec::new();
    for idx in (0..parts.len()).rev() {
        let candidate = normalize_text(&parts[idx]);
        if !is_rss_allergen_token(&candidate) {
            break;
        }
        let normalized = normalize_rss_allergen_token(&candidate);
        if normalized.is_empty() {
            break;
        }
        suffix_tokens.insert(0, normalized);
    }

    if suffix_tokens.is_empty() {
        return compact;
    }

    let mut main_text =
        normalize_text(&parts[..parts.len().saturating_sub(suffix_tokens.len())].join(", "));
    if main_text.is_empty() {
        return compact;
    }

    let star_re = Regex::new(r"^(.*\S)\s*\*$").ok();
    if let Some(re) = star_re {
        if let Some(captures) = re.captures(&main_text) {
            if let Some(raw_main) = captures.get(1) {
                main_text = normalize_text(raw_main.as_str());
                suffix_tokens.insert(0, "*".to_string());
            }
        }
    }

    let trailing_re = Regex::new(r"^(.*\S)\s+([A-Za-z*]{1,4})$").ok();
    while let Some(re) = &trailing_re {
        let captures = match re.captures(&main_text) {
            Some(c) => c,
            None => break,
        };
        let raw_prefix = match captures.get(1) {
            Some(v) => v.as_str(),
            None => break,
        };
        let raw_token = match captures.get(2) {
            Some(v) => v.as_str(),
            None => break,
        };

        let trailing_token = normalize_rss_allergen_token(raw_token);
        if !is_rss_allergen_token(raw_token) || trailing_token.is_empty() {
            break;
        }

        let next_main = normalize_text(raw_prefix);
        if next_main.is_empty() || next_main == main_text {
            break;
        }

        main_text = next_main;
        suffix_tokens.insert(0, trailing_token);
    }

    format!("{} ({})", main_text, suffix_tokens.join(", "))
}

fn parse_rss_components(description_raw: &str) -> Vec<String> {
    let decoded = decode_html_entities(description_raw).to_string();
    let paragraph_re = Regex::new(r"(?is)<p[^>]*>([\s\S]*?)</p>").ok();

    let mut components = Vec::new();
    if let Some(re) = paragraph_re {
        for captures in re.captures_iter(&decoded) {
            let line = captures
                .get(1)
                .map(|m| normalize_rss_component_line(&strip_html_text(m.as_str())))
                .unwrap_or_default();
            if !line.is_empty() {
                components.push(line);
            }
        }
    }

    if components.is_empty() {
        let fallback = normalize_rss_component_line(&strip_html_text(&decoded));
        if !fallback.is_empty() {
            components.push(fallback);
        }
    }

    components
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiMenuDay, ApiResponse};

    #[test]
    fn compass_parse_keeps_closed_day_as_valid_today() {
        let today = local_today_key();
        let response = ApiResponse {
            restaurant_name: Some("Caari".to_string()),
            restaurant_url: Some("https://example.invalid/caari".to_string()),
            menus_for_days: Some(vec![ApiMenuDay {
                date: Some(format!("{}T00:00:00+00:00", today)),
                lunch_time: None,
                set_menus: Some(Vec::new()),
            }]),
            error_text: None,
        };

        let parsed = parse_response(response, "{}".to_string());
        assert!(parsed.ok);
        assert_eq!(parsed.payload_date, today);
        let today_menu = parsed.today_menu.expect("today_menu");
        assert!(today_menu.menus.is_empty());
    }
}
