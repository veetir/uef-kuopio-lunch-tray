use super::{local_today_key, log_fetch_attempt, weekday_token, FetchContext, FetchOutput};
use crate::antell;
use crate::restaurant::{Provider, Restaurant};
use crate::settings::Settings;
use reqwest::blocking::Client;

pub(super) fn fetch_antell(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
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

pub(super) fn parse_cached_antell_payload(
    raw_payload: &str,
    restaurant: Restaurant,
) -> FetchOutput {
    let today_key = local_today_key();
    let today_menu = antell::parse_antell_html(raw_payload, &today_key);
    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu: Some(today_menu),
        restaurant_name: restaurant.name.to_string(),
        restaurant_url: restaurant.url.unwrap_or_default().to_string(),
        provider: Provider::Antell,
        raw_json: raw_payload.to_string(),
        payload_date: String::new(),
    }
}
