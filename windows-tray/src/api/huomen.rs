use super::{local_today_key, log_fetch_attempt, FetchContext, FetchOutput};
use crate::format::normalize_text;
use crate::model::{MenuGroup, TodayMenu};
use crate::restaurant::{Provider, Restaurant};
use crate::settings::Settings;
use anyhow::{anyhow, Context};
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn fetch_huomen(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
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

pub(super) fn parse_huomen_payload(
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
