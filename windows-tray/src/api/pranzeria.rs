use super::{
    local_today_key, log_fetch_attempt, parse_dot_date_iso, strip_html_text, FetchContext,
    FetchOutput,
};
use crate::format::normalize_text;
use crate::model::{MenuGroup, TodayMenu};
use crate::restaurant::{Provider, Restaurant};
use crate::settings::Settings;
use regex::Regex;
use reqwest::blocking::Client;

pub(super) fn fetch_pranzeria(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
    let url = restaurant
        .url
        .unwrap_or("https://www.sorrento.fi/pranzeria/");
    log_fetch_attempt(
        context,
        restaurant,
        &settings.language,
        &settings.language,
        url,
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
                restaurant_url: url.to_string(),
                provider: Provider::PranzeriaHtml,
                raw_json: String::new(),
                payload_date: String::new(),
            };
        }
    };

    match client.get(url).send() {
        Ok(resp) => match resp.text() {
            Ok(text) => parse_pranzeria_payload(&text, restaurant),
            Err(err) => FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: url.to_string(),
                provider: Provider::PranzeriaHtml,
                raw_json: String::new(),
                payload_date: String::new(),
            },
        },
        Err(err) => FetchOutput {
            ok: false,
            error_message: err.to_string(),
            today_menu: None,
            restaurant_name: restaurant.name.to_string(),
            restaurant_url: url.to_string(),
            provider: Provider::PranzeriaHtml,
            raw_json: String::new(),
            payload_date: String::new(),
        },
    }
}

pub(super) fn parse_pranzeria_payload(html_text: &str, restaurant: Restaurant) -> FetchOutput {
    let payload_text = html_text.to_string();
    let paragraph_re = match Regex::new(r"(?is)<p\b[^>]*>([\s\S]*?)</p>") {
        Ok(value) => value,
        Err(err) => {
            return FetchOutput {
                ok: false,
                error_message: err.to_string(),
                today_menu: None,
                restaurant_name: restaurant.name.to_string(),
                restaurant_url: restaurant.url.unwrap_or_default().to_string(),
                provider: Provider::PranzeriaHtml,
                raw_json: payload_text,
                payload_date: String::new(),
            };
        }
    };

    let today = local_today_key();
    let mut lines_by_date: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut current_date_iso = String::new();

    for captures in paragraph_re.captures_iter(&payload_text) {
        let line = strip_html_text(captures.get(1).map(|m| m.as_str()).unwrap_or_default());
        if line.is_empty() {
            continue;
        }

        if let Some((date_iso, trailing)) = parse_pranzeria_day_header(&line) {
            current_date_iso = date_iso.clone();
            let day_lines = lines_by_date.entry(date_iso).or_default();
            if !trailing.is_empty() {
                day_lines.push(trailing);
            }
            continue;
        }

        if current_date_iso.is_empty() {
            continue;
        }

        if is_pranzeria_legend_line(&line) {
            break;
        }

        lines_by_date
            .entry(current_date_iso.clone())
            .or_default()
            .push(line);
    }

    let provider_date_valid = lines_by_date.contains_key(&today);
    let lunch_lines = normalize_pranzeria_lines(lines_by_date.remove(&today).unwrap_or_default());
    let menu_date_iso = if provider_date_valid {
        today.clone()
    } else {
        String::new()
    };

    let today_menu = if provider_date_valid {
        Some(TodayMenu {
            date_iso: today.clone(),
            lunch_time: String::new(),
            menus: if lunch_lines.is_empty() {
                Vec::new()
            } else {
                vec![MenuGroup {
                    name: "Lounas".to_string(),
                    price: String::new(),
                    components: lunch_lines,
                }]
            },
        })
    } else {
        None
    };

    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu,
        restaurant_name: restaurant.name.to_string(),
        restaurant_url: restaurant.url.unwrap_or_default().to_string(),
        provider: Provider::PranzeriaHtml,
        raw_json: payload_text,
        payload_date: menu_date_iso,
    }
}

fn parse_pranzeria_day_header(line_text: &str) -> Option<(String, String)> {
    let clean = normalize_text(line_text);
    let re = Regex::new(
        r"^(Maanantai|Tiistai|Keskiviikko|Torstai|Perjantai|Lauantai|Sunnuntai)\s+(\d{1,2}\.\d{1,2}\.\d{2,4})(?:\s+(.+))?$",
    )
    .ok()?;
    let captures = re.captures(&clean)?;
    let date_iso = parse_dot_date_iso(captures.get(2)?.as_str())?;
    let trailing = normalize_text(captures.get(3).map(|m| m.as_str()).unwrap_or_default());
    Some((date_iso, trailing))
}

fn is_pranzeria_legend_line(line_text: &str) -> bool {
    let clean = normalize_text(line_text);
    if clean.is_empty() {
        return false;
    }

    if Regex::new(r"^(?:L|G|M|V|VG)\s*=")
        .ok()
        .is_some_and(|re| re.is_match(&clean))
    {
        return true;
    }

    clean.contains("Laktoositon")
        || clean.contains("Gluteeniton")
        || clean.contains("Maidoton")
        || clean.contains("Kasvis")
        || clean.contains("Vegaani")
}

fn normalize_pranzeria_lines(raw_lines: Vec<String>) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in raw_lines {
        let clean = normalize_text(&raw);
        if clean.is_empty() {
            continue;
        }
        if lines.last().is_some_and(|existing| existing == &clean) {
            continue;
        }
        lines.push(clean);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restaurant::restaurant_for_code;
    use time::OffsetDateTime;

    #[test]
    fn pranzeria_payload_parses_current_day_lines() {
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let weekday_fi = match now.weekday() {
            time::Weekday::Monday => "Maanantai",
            time::Weekday::Tuesday => "Tiistai",
            time::Weekday::Wednesday => "Keskiviikko",
            time::Weekday::Thursday => "Torstai",
            time::Weekday::Friday => "Perjantai",
            time::Weekday::Saturday => "Lauantai",
            time::Weekday::Sunday => "Sunnuntai",
        };
        let date_text = format!(
            "{:02}.{:02}.{:04}",
            now.date().day(),
            now.date().month() as u8,
            now.date().year()
        );
        let html = format!(
            "<p>{} {} Salaatti- &amp;AntipastoBuffet</p>\
             <p>Spezzatino Di Manzo (L, G)</p>\
             <p>Roomalainen focacciapizzabuffet</p>\
             <p>L = Laktoositon</p>",
            weekday_fi, date_text
        );

        let parsed = parse_pranzeria_payload(&html, restaurant_for_code("pranzeria-html", true));
        assert!(parsed.ok);
        assert_eq!(parsed.payload_date, local_today_key());
        let today_menu = parsed.today_menu.expect("today_menu");
        assert_eq!(today_menu.menus.len(), 1);
        assert_eq!(today_menu.menus[0].name, "Lounas");
        assert_eq!(
            today_menu.menus[0].components[0],
            "Salaatti- &AntipastoBuffet"
        );
        assert!(today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("Spezzatino Di Manzo")));
        assert!(!today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("Laktoositon")));
    }
}
