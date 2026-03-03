use crate::antell;
use crate::format::{normalize_optional, normalize_text};
use crate::model::{ApiResponse, ApiSetMenu, MenuGroup, TodayMenu};
use crate::restaurant::{restaurant_for_code, Provider, Restaurant};
use crate::settings::Settings;
use anyhow::{anyhow, Context};
use html_escape::decode_html_entities;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::OnceLock;
use time::{Month, OffsetDateTime};

pub struct FetchOutput {
    pub ok: bool,
    pub error_message: String,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub provider: Provider,
    pub raw_json: String,
    pub payload_date: String,
}

pub fn fetch_today(settings: &Settings) -> FetchOutput {
    let restaurant = restaurant_for_code(
        &settings.restaurant_code,
        settings.enable_antell_restaurants,
    );
    match restaurant.provider {
        Provider::Compass => fetch_compass(settings, restaurant),
        Provider::CompassRss => fetch_compass_rss(settings, restaurant),
        Provider::Antell => fetch_antell(settings, restaurant),
        Provider::HuomenJson => fetch_huomen(settings, restaurant),
    }
}

fn fetch_compass(settings: &Settings, restaurant: Restaurant) -> FetchOutput {
    let url = format!(
        "https://www.compass-group.fi/menuapi/feed/json?costNumber={}&language={}",
        restaurant.code, settings.language
    );
    let client = match shared_http_client() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err,
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

fn fetch_compass_rss(settings: &Settings, restaurant: Restaurant) -> FetchOutput {
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

    let client = match shared_http_client() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err,
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

fn fetch_huomen(settings: &Settings, restaurant: Restaurant) -> FetchOutput {
    let huomen_api_base = match restaurant.huomen_api_base {
        Some(value) if !value.trim().is_empty() => value.trim(),
        _ => {
            return FetchOutput {
                ok: false,
                error_message: "Missing Huomen API base URL".to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::HuomenJson,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    let separator = if huomen_api_base.contains('?') {
        "&"
    } else {
        "?"
    };
    let url = format!(
        "{}{}language={}",
        huomen_api_base, separator, settings.language
    );

    let client = match shared_http_client() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err,
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::HuomenJson,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    match client.get(&url).send() {
        Ok(resp) => match resp.text() {
            Ok(text) => match parse_huomen_payload(&text, restaurant, &settings.language) {
                Ok(output) => output,
                Err(err) => FetchOutput {
                    ok: false,
                    error_message: err.to_string(),
                    today_menu: None,
                    restaurant_name: restaurant.name.to_string(),
                    restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                    provider: Provider::HuomenJson,
                    raw_json: text,
                    payload_date: String::new(),
                },
            },
            Err(err) => FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::HuomenJson,
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
            provider: Provider::HuomenJson,
            raw_json: String::new(),
            payload_date: String::new(),
        },
    }
}

pub fn parse_cached_payload(
    raw_payload: &str,
    provider: Provider,
    restaurant: Restaurant,
    language: &str,
) -> anyhow::Result<FetchOutput> {
    match provider {
        Provider::Compass => {
            let api: ApiResponse =
                serde_json::from_str(raw_payload).context("parse cached JSON")?;
            Ok(parse_response(api, raw_payload.to_string()))
        }
        Provider::CompassRss => Ok(parse_compass_rss_payload(raw_payload, restaurant, language)),
        Provider::Antell => {
            let today_key = local_today_key();
            let today_menu = antell::parse_antell_html(raw_payload, &today_key);
            Ok(FetchOutput {
                ok: true,
                error_message: String::new(),
                today_menu: Some(today_menu),
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider,
                raw_json: raw_payload.to_string(),
                payload_date: String::new(),
            })
        }
        Provider::HuomenJson => parse_huomen_payload(raw_payload, restaurant, language),
    }
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

fn fetch_antell(settings: &Settings, restaurant: Restaurant) -> FetchOutput {
    let today_key = local_today_key();
    let slug = match restaurant.antell_slug {
        Some(s) => s,
        None => {
            return FetchOutput {
                ok: false,
                error_message: "Missing Antell slug".to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };
    let weekday = weekday_token();
    let url = if settings.language == "en" && restaurant.code == "antell-round" {
        format!(
            "https://antell.fi/en/lunch/kuopio/{}/?print_lunch_list_day=1&print_lunch_day=panel-{}",
            slug, weekday
        )
    } else {
        format!(
            "https://antell.fi/lounas/kuopio/{}/?print_lunch_day={}&print_lunch_list_day=1",
            slug, weekday
        )
    };
    let client = match shared_http_client() {
        Ok(c) => c,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err,
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    let response = client.get(&url).send();
    match response {
        Ok(resp) => match resp.text() {
            Ok(text) => {
                let today_menu = antell::parse_antell_html(&text, &today_key);
                FetchOutput {
                    ok: true,
                    error_message: String::new(),
                    today_menu: Some(today_menu),
                    restaurant_name: restaurant.name.to_string(),
                    restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                    provider: Provider::Antell,
                    raw_json: text,
                    payload_date: today_key,
                }
            }
            Err(err) => FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::Antell,
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
            provider: Provider::Antell,
            raw_json: String::new(),
            payload_date: String::new(),
        },
    }
}

fn shared_http_client() -> Result<&'static Client, String> {
    static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    let init = HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|err| err.to_string())
    });
    match init {
        Ok(client) => Ok(client),
        Err(err) => Err(err.clone()),
    }
}

fn parse_compass_rss_payload(
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

fn parse_huomen_payload(
    json_text: &str,
    restaurant: Restaurant,
    language: &str,
) -> anyhow::Result<FetchOutput> {
    let parsed: Value = serde_json::from_str(json_text).context("parse Huomen JSON")?;

    if parsed
        .get("success")
        .and_then(Value::as_bool)
        .is_some_and(|success| !success)
    {
        let message = localized_field(parsed.get("message"), language);
        return Err(anyhow!(if message.is_empty() {
            "Huomen API returned an error".to_string()
        } else {
            message
        }));
    }

    let days = parsed
        .pointer("/data/week/days")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Missing week.days in Huomen payload"))?;

    let expected_iso = local_today_key();
    let mut day_match: Option<&Value> = None;
    let mut fallback_payload_date = String::new();

    for day in days {
        let date = normalize_text(
            day.get("dateString")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        if !date.is_empty() && (fallback_payload_date.is_empty() || date > fallback_payload_date) {
            fallback_payload_date = date.clone();
        }
        if date == expected_iso {
            day_match = Some(day);
            break;
        }
    }

    let mut lunch_lines = Vec::new();
    if let Some(day) = day_match {
        let is_closed = day
            .get("isClosed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !is_closed {
            if let Some(lunches) = day.get("lunches").and_then(Value::as_array) {
                for lunch in lunches {
                    let line = huomen_lunch_line(lunch, language);
                    if !line.is_empty() {
                        lunch_lines.push(line);
                    }
                }
            }
        }
    }

    let provider_date_valid = day_match.is_some();
    let menu_date_iso = if provider_date_valid {
        expected_iso.clone()
    } else {
        String::new()
    };

    let restaurant_name = {
        let value = localized_field(parsed.pointer("/data/location/name"), language);
        if value.is_empty() {
            restaurant.name.to_string()
        } else {
            value
        }
    };

    let restaurant_url = restaurant.url.unwrap_or_default().to_string();

    let today_menu = if provider_date_valid {
        Some(TodayMenu {
            date_iso: expected_iso,
            lunch_time: String::new(),
            menus: vec![MenuGroup {
                name: if language == "fi" {
                    "Lounas".to_string()
                } else {
                    "Lunch".to_string()
                },
                price: String::new(),
                components: lunch_lines,
            }],
        })
    } else {
        None
    };

    Ok(FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu,
        restaurant_name,
        restaurant_url,
        provider: Provider::HuomenJson,
        raw_json: json_text.to_string(),
        payload_date: if provider_date_valid {
            menu_date_iso
        } else {
            fallback_payload_date
        },
    })
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
    rss_item_re()
        .captures(xml_text)
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn parse_rss_menu_date_iso(date_text: &str) -> String {
    let clean = normalize_text(date_text);
    if clean.is_empty() {
        return String::new();
    }

    let captures = match date_token_re().captures(&clean) {
        Some(c) => c,
        None => return String::new(),
    };

    let day = captures
        .get(1)
        .and_then(|m| m.as_str().parse::<u8>().ok())
        .unwrap_or(0);
    let month = captures
        .get(2)
        .and_then(|m| m.as_str().parse::<u8>().ok())
        .unwrap_or(0);
    let mut year = captures
        .get(3)
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .unwrap_or(0);

    if day == 0 || month == 0 || year <= 0 {
        return String::new();
    }

    if year < 100 {
        year += 2000;
    }

    let month_enum = match Month::try_from(month) {
        Ok(value) => value,
        Err(_) => return String::new(),
    };

    if time::Date::from_calendar_date(year, month_enum, day).is_err() {
        return String::new();
    }

    format!("{:04}-{:02}-{:02}", year, month, day)
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

    if trailing_allergen_group_re().is_match(&line) {
        return line;
    }

    let compact = trailing_punct_re().replace(&line, "").to_string();

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

    if let Some(captures) = trailing_star_re().captures(&main_text) {
        if let Some(raw_main) = captures.get(1) {
            main_text = normalize_text(raw_main.as_str());
            suffix_tokens.insert(0, "*".to_string());
        }
    }

    loop {
        let captures = match trailing_token_re().captures(&main_text) {
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

    let mut components = Vec::new();
    for captures in paragraph_re().captures_iter(&decoded) {
        let line = captures
            .get(1)
            .map(|m| normalize_rss_component_line(&strip_html_text(m.as_str())))
            .unwrap_or_default();
        if !line.is_empty() {
            components.push(line);
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

fn strip_html_text(raw_html: &str) -> String {
    let without_tags = html_tag_re().replace_all(raw_html, " ").to_string();
    normalize_text(decode_html_entities(&without_tags).as_ref())
}

fn rss_item_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<item\b[^>]*>([\s\S]*?)</item>").expect("rss item regex"))
}

fn date_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d{1,2})[-./](\d{1,2})[-./](\d{2,4})").expect("date regex"))
}

fn trailing_allergen_group_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\((?:\*|[A-Za-z]{1,8})(?:\s*,\s*(?:\*|[A-Za-z]{1,8}))*\)\s*$")
            .expect("trailing allergen regex")
    })
}

fn trailing_punct_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s*[;,]\s*$").expect("trailing punct regex"))
}

fn trailing_star_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.*\S)\s*\*$").expect("trailing star regex"))
}

fn trailing_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.*\S)\s+([A-Za-z*]{1,4})$").expect("trailing token regex"))
}

fn paragraph_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<p[^>]*>([\s\S]*?)</p>").expect("paragraph regex"))
}

fn html_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]*>").expect("html tag regex"))
}

fn localized_field(value: Option<&Value>, language: &str) -> String {
    let value = match value {
        Some(v) => v,
        None => return String::new(),
    };

    match value {
        Value::String(text) => normalize_text(text),
        Value::Number(num) => normalize_text(&num.to_string()),
        Value::Bool(flag) => normalize_text(&flag.to_string()),
        Value::Object(map) => {
            for key in [language, "fi", "en"] {
                if let Some(candidate) = map.get(key) {
                    let text = localized_field(Some(candidate), language);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }

            for candidate in map.values() {
                let text = localized_field(Some(candidate), language);
                if !text.is_empty() {
                    return text;
                }
            }

            String::new()
        }
        Value::Array(items) => {
            for item in items {
                let text = localized_field(Some(item), language);
                if !text.is_empty() {
                    return text;
                }
            }
            String::new()
        }
        Value::Null => String::new(),
    }
}

fn normalize_huomen_allergen_token(token: &str) -> String {
    let clean = normalize_text(token);
    if clean.is_empty() {
        return String::new();
    }
    if clean == "*" {
        return "*".to_string();
    }

    let upper = clean.to_uppercase();
    if upper == "VEG" {
        return "Veg".to_string();
    }
    if upper.chars().all(|ch| ch.is_ascii_uppercase()) && upper.len() <= 8 {
        return upper;
    }

    clean
}

fn huomen_lunch_line(lunch: &Value, language: &str) -> String {
    let title = localized_field(lunch.get("title"), language);
    if title.is_empty() {
        return String::new();
    }

    let description = localized_field(lunch.get("description"), language);
    let mut line = title.clone();
    if !description.is_empty() && description != title {
        line.push_str(" - ");
        line.push_str(&description);
    }

    let mut allergens = Vec::new();
    let mut seen = HashSet::new();
    if let Some(raw_allergens) = lunch.get("allergens").and_then(Value::as_array) {
        for raw in raw_allergens {
            let token = normalize_huomen_allergen_token(&localized_field(
                raw.get("abbreviation"),
                language,
            ));
            if token.is_empty() {
                continue;
            }
            let key = token.to_uppercase();
            if seen.insert(key) {
                allergens.push(token);
            }
        }
    }

    if !allergens.is_empty() {
        line.push_str(" (");
        line.push_str(&allergens.join(", "));
        line.push(')');
    }

    normalize_text(&line)
}

fn weekday_token() -> &'static str {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    match now.weekday() {
        time::Weekday::Monday => "monday",
        time::Weekday::Tuesday => "tuesday",
        time::Weekday::Wednesday => "wednesday",
        time::Weekday::Thursday => "thursday",
        time::Weekday::Friday => "friday",
        time::Weekday::Saturday => "saturday",
        time::Weekday::Sunday => "sunday",
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
