use super::{log_fetch_attempt, parse_dot_date_iso, strip_html_text, FetchContext, FetchOutput};
use crate::format::normalize_text;
use crate::model::{MenuGroup, TodayMenu};
use crate::restaurant::{Provider, Restaurant};
use crate::settings::Settings;
use regex::Regex;
use reqwest::blocking::Client;
use time::{Date, OffsetDateTime};

const PRANZERIA_MAX_INFERRED_DATE_DISTANCE_DAYS: i32 = 14;

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
    parse_pranzeria_payload_for_date(
        html_text,
        restaurant,
        OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .date(),
    )
}

fn parse_pranzeria_payload_for_date(
    html_text: &str,
    restaurant: Restaurant,
    today_date: Date,
) -> FetchOutput {
    let payload_text = html_text.to_string();
    let block_re = match Regex::new(r"(?is)<(?:p|h[1-6]|li)\b[^>]*>([\s\S]*?)</(?:p|h[1-6]|li)>") {
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

    let today = date_iso(today_date);
    let mut lines_by_date: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut current_date_iso = String::new();
    let mut price_parts: Vec<String> = Vec::new();

    for captures in block_re.captures_iter(&payload_text) {
        let line = strip_html_text(captures.get(1).map(|m| m.as_str()).unwrap_or_default());
        if line.is_empty() {
            continue;
        }

        if let Some(parts) = extract_pranzeria_price_parts(&line) {
            for part in parts {
                if !price_parts.iter().any(|existing| existing == &part) {
                    price_parts.push(part);
                }
            }
            continue;
        }

        if let Some((date_iso, trailing)) = parse_pranzeria_day_header(&line, today_date) {
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
                    price: order_pranzeria_price_parts(price_parts).join(" / "),
                    components: lunch_lines,
                    component_recipe_ids: Vec::new(),
                    component_recipe_details: Vec::new(),
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

fn parse_pranzeria_day_header(line_text: &str, today_date: Date) -> Option<(String, String)> {
    let clean = normalize_text(line_text);
    if clean.is_empty() {
        return None;
    }

    let weekday_pattern = pranzeria_weekday_pattern();
    let date_pattern = pranzeria_date_pattern();
    let weekday_first = Regex::new(&format!(
        r"(?i)^(?:{weekday_pattern})\s+(?P<date>{date_pattern})(?P<rest>.*)$"
    ))
    .ok()?;
    if let Some(captures) = weekday_first.captures(&clean) {
        let date_iso = parse_pranzeria_date_iso(captures.name("date")?.as_str(), today_date)?;
        let trailing =
            sanitize_pranzeria_header_remainder(captures.name("rest").map(|m| m.as_str()));
        return Some((date_iso, trailing));
    }

    let date_first = Regex::new(&format!(r"(?i)^(?P<date>{date_pattern})(?P<rest>.*)$")).ok()?;
    let captures = date_first.captures(&clean)?;
    let date_text = captures.name("date")?.as_str();
    if looks_like_pranzeria_time_range(date_text, captures.name("rest").map(|m| m.as_str())) {
        return None;
    }

    let date_iso = parse_pranzeria_date_iso(date_text, today_date)?;
    let trailing = sanitize_pranzeria_header_remainder(captures.name("rest").map(|m| m.as_str()));
    Some((date_iso, trailing))
}

fn parse_pranzeria_date_iso(date_text: &str, today_date: Date) -> Option<String> {
    let clean = normalize_text(date_text);
    if clean.is_empty() {
        return None;
    }

    if let Some(captures) = Regex::new(r"^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$")
        .ok()?
        .captures(&clean)
    {
        let year = captures.get(1)?.as_str().parse::<i32>().ok()?;
        let month = captures.get(2)?.as_str().parse::<u8>().ok()?;
        let day = captures.get(3)?.as_str().parse::<u8>().ok()?;
        return build_pranzeria_date(year, month, day).map(date_iso);
    }

    if let Some(date_iso) = parse_dot_date_iso(&clean) {
        return Some(date_iso);
    }

    let captures = Regex::new(r"^(\d{1,2})[./-](\d{1,2})(?:[./-](\d{2,4}))?\.?$")
        .ok()?
        .captures(&clean)?;
    let day = captures.get(1)?.as_str().parse::<u8>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u8>().ok()?;

    if let Some(year_text) = captures.get(3).map(|m| m.as_str()) {
        let mut year = year_text.parse::<i32>().ok()?;
        if year < 100 {
            year += 2000;
        }
        return build_pranzeria_date(year, month, day).map(date_iso);
    }

    [
        today_date.year() - 1,
        today_date.year(),
        today_date.year() + 1,
    ]
    .into_iter()
    .filter_map(|year| {
        let candidate = build_pranzeria_date(year, month, day)?;
        let distance = (candidate.to_julian_day() - today_date.to_julian_day()).abs();
        Some((distance, candidate))
    })
    .filter(|(distance, _)| *distance <= PRANZERIA_MAX_INFERRED_DATE_DISTANCE_DAYS)
    .min_by_key(|(distance, candidate)| {
        (
            *distance,
            (candidate.year() - today_date.year()).abs(),
            candidate.month() as u8,
            candidate.day(),
        )
    })
    .map(|(_, candidate)| date_iso(candidate))
}

fn build_pranzeria_date(year: i32, month: u8, day: u8) -> Option<Date> {
    let month = time::Month::try_from(month).ok()?;
    Date::from_calendar_date(year, month, day).ok()
}

fn prune_pranzeria_header_text(value: &str) -> String {
    let weekday_re = match Regex::new(&format!(r"(?i)\b{}\b", pranzeria_weekday_pattern())) {
        Ok(value) => value,
        Err(_) => return normalize_text(value),
    };
    let without_weekdays = weekday_re.replace_all(value, " ");
    let without_markers = match Regex::new(r"^[\s:,\-|/]+|[\s:,\-|/]+$") {
        Ok(value) => value.replace_all(&without_weekdays, " ").to_string(),
        Err(_) => without_weekdays.to_string(),
    };
    normalize_text(&without_markers)
}

fn sanitize_pranzeria_header_remainder(rest: Option<&str>) -> String {
    prune_pranzeria_header_text(rest.unwrap_or_default())
}

fn looks_like_pranzeria_time_range(date_text: &str, rest: Option<&str>) -> bool {
    if date_text.contains('/') || date_text.matches('.').count() > 1 || date_text.contains('-') {
        return false;
    }

    let rest = normalize_text(rest.unwrap_or_default());
    if !rest.starts_with('-') {
        return false;
    }

    Regex::new(r"^-\s*\d{1,2}[.:]\d{2}")
        .ok()
        .is_some_and(|re| re.is_match(&rest))
}

fn pranzeria_weekday_pattern() -> &'static str {
    "maanantai|tiistai|keskiviikko|torstai|perjantai|lauantai|sunnuntai|monday|tuesday|wednesday|thursday|friday|saturday|sunday"
}

fn pranzeria_date_pattern() -> &'static str {
    r"(?:\d{4}[-/]\d{1,2}[-/]\d{1,2}|\d{1,2}[./-]\d{1,2}(?:[./-]\d{2,4})?\.?)"
}

fn date_iso(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
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

fn extract_pranzeria_price_parts(line_text: &str) -> Option<Vec<String>> {
    let clean = normalize_text(line_text);
    if clean.is_empty() {
        return None;
    }
    let re = Regex::new(
        r"(?i)\b(?P<label>SALAATTILOUNAS|LOUNASBUFFET|SOPIMUSLOUNAS)\b\s+(?P<price>\d{1,2}[,.]\d{2})\s*€",
    )
    .ok()?;
    let mut parts = Vec::new();
    for captures in re.captures_iter(&clean) {
        let label = normalize_pranzeria_price_label(captures.name("label")?.as_str());
        let price = captures.name("price")?.as_str().replace('.', ",");
        parts.push(format!("{label} {price} €"));
    }
    (!parts.is_empty()).then_some(parts)
}

fn order_pranzeria_price_parts(parts: Vec<String>) -> Vec<String> {
    let order = ["Salaattilounas", "Lounasbuffet", "Sopimuslounas"];
    let mut out = Vec::new();
    for label in order {
        if let Some(part) = parts.iter().find(|part| part.starts_with(label)) {
            out.push(part.clone());
        }
    }
    for part in parts {
        if !out.iter().any(|existing| existing == &part) {
            out.push(part);
        }
    }
    out
}

fn normalize_pranzeria_price_label(label: &str) -> &'static str {
    match label.to_ascii_uppercase().as_str() {
        "SALAATTILOUNAS" => "Salaattilounas",
        "LOUNASBUFFET" => "Lounasbuffet",
        "SOPIMUSLOUNAS" => "Sopimuslounas",
        _ => "Lounas",
    }
}

fn normalize_pranzeria_lines(raw_lines: Vec<String>) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in raw_lines {
        let clean = normalize_text(&raw);
        if clean.is_empty() {
            continue;
        }
        for line in split_fused_pranzeria_items(&clean) {
            if lines.last().is_some_and(|existing| existing == &line) {
                continue;
            }
            lines.push(line);
        }
    }
    lines
}

fn split_fused_pranzeria_items(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = normalize_text(line);
    if current.is_empty() {
        return out;
    }

    while let Some(split_at) = fused_pranzeria_split_index(&current) {
        let left = normalize_text(&current[..split_at]);
        if !left.is_empty() {
            out.push(left);
        }
        current = normalize_text(&current[split_at..]);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn fused_pranzeria_split_index(line: &str) -> Option<usize> {
    let dish_start_re = Regex::new(
        r"\b(?:Pasta|Pollo|Manzo|Maiale|Salmone|Gnocchi|Cotoletta|Porco|Spezzatino|Lasagne|Risotto|Ravioli|Tagliatelle|Spaghetti|Fusilli|Penne|Rigatoni)\b",
    )
    .ok()?;
    let allergen_tail_re =
        Regex::new(r"(?i)(?:\b(?:G|L|M|V|VG)\b|pyydet(?:t|)äessä\s+G|pyydettäessä\s+G)\s*$")
            .ok()?;

    for m in dish_start_re.find_iter(line).skip(1) {
        let before = line[..m.start()].trim_end();
        if allergen_tail_re.is_match(before) {
            return Some(m.start());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restaurant::restaurant_for_code;
    use time::Month;

    #[test]
    fn pranzeria_payload_parses_current_day_lines_with_yearless_headers() {
        let today = Date::from_calendar_date(2026, Month::March, 30).expect("valid date");
        let html = "\
            <p><b>Maanantai 30.3.</b></p>\
            <p>Salaatti- &amp; AntipastoBuffet</p>\
             <p>Spezzatino Di Manzo (L, G)</p>\
             <p>Roomalainen focacciapizzabuffet</p>\
             <p>L = Laktoositon</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        assert!(parsed.ok);
        assert_eq!(parsed.payload_date, "2026-03-30");
        let today_menu = parsed.today_menu.expect("today_menu");
        assert_eq!(today_menu.menus.len(), 1);
        assert_eq!(today_menu.menus[0].name, "Lounas");
        assert_eq!(
            today_menu.menus[0].components[0],
            "Salaatti- & AntipastoBuffet"
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

    #[test]
    fn pranzeria_payload_parses_full_year_headers() {
        let today = Date::from_calendar_date(2026, Month::March, 30).expect("valid date");
        let html = "\
            <p>Maanantai 30.3.2026</p>\
            <p>Salaatti- &amp; AntipastoBuffet</p>\
            <p>Pasta Al Forno</p>\
            <p>L = Laktoositon</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert!(today_menu.menus[0]
            .components
            .iter()
            .any(|line| line == "Pasta Al Forno"));
    }

    #[test]
    fn pranzeria_payload_parses_date_without_weekday() {
        let today = Date::from_calendar_date(2026, Month::March, 30).expect("valid date");
        let html = "\
            <p>30.3.</p>\
            <p>Salaatti- &amp; AntipastoBuffet</p>\
            <p>Pollo Limone</p>\
            <p>31.3.</p>\
            <p>Pasta for tomorrow</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert!(today_menu.menus[0]
            .components
            .iter()
            .any(|line| line == "Pollo Limone"));
        assert!(!today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("tomorrow")));
    }

    #[test]
    fn pranzeria_payload_handles_mixed_header_formats() {
        let today = Date::from_calendar_date(2026, Month::April, 2).expect("valid date");
        let html = "\
            <p>Torstai 02.4.</p>\
            <p>Porco Aglio &amp; Zenzero</p>\
            <p>Roomalainen focacciapizzabuffet</p>\
            <p>Perjantai 27.3.2026</p>\
            <p>EI LOUNASTA!</p>\
            <p>L = Laktoositon</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert!(today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("Porco Aglio")));
        assert!(!today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("EI LOUNASTA")));
    }

    #[test]
    fn pranzeria_payload_reads_heading_and_list_tags() {
        let today = Date::from_calendar_date(2026, Month::March, 30).expect("valid date");
        let html = "\
            <h6>Maanantai 30.3.2026</h6>\
            <li>Salaatti- &amp; AntipastoBuffet</li>\
            <li>Polpette Alla Cacciatora</li>\
            <li>L = Laktoositon</li>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert!(today_menu.menus[0]
            .components
            .iter()
            .any(|line| line.contains("Polpette Alla Cacciatora")));
    }

    #[test]
    fn pranzeria_payload_splits_fused_items_after_allergen_tail() {
        let today = Date::from_calendar_date(2026, Month::June, 26).expect("valid date");
        let html = "\
            <p>Perjantai 26.6.2026</p>\
            <p>Pollo Aglio &amp; Parmigiano (Kanan Siipiä Valkosipulilla &amp; Parmesaanilla) L Pasta Tonnarella (Pastaa Kermaisessa Tonnikalakastikkeessa) Pyydettäessä G</p>\
            <p>L = Laktoositon</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert_eq!(
            today_menu.menus[0].components,
            vec![
                "Pollo Aglio & Parmigiano (Kanan Siipiä Valkosipulilla & Parmesaanilla) L"
                    .to_string(),
                "Pasta Tonnarella (Pastaa Kermaisessa Tonnikalakastikkeessa) Pyydettäessä G"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn pranzeria_payload_reads_price_summary() {
        let today = Date::from_calendar_date(2026, Month::June, 26).expect("valid date");
        let html = "\
            <p>SALAATTILOUNAS 10.90 € (SIS. SALAATTI, ANTIPASTOPÖYTÄ, KAHVI &amp; JÄLKIRUOKA)
            LOUNASBUFFET 14.00 € (SIS. SALAATTI, ANTIPASTOPÖYTÄ, PIZZA, PASTA, PÄÄRUOKA, KAHVI &amp; JÄLKIRUOKA)
            SOPIMUSLOUNAS 13.80 €</p>\
            <p>Perjantai 26.6.2026</p>\
            <p>Salaatti- &amp; AntipastoBuffet</p>";

        let parsed = parse_pranzeria_payload_for_date(
            html,
            restaurant_for_code("pranzeria-html", true),
            today,
        );
        let today_menu = parsed.today_menu.expect("today_menu");
        assert_eq!(
            today_menu.menus[0].price,
            "Salaattilounas 10,90 € / Lounasbuffet 14,00 € / Sopimuslounas 13,80 €"
        );
    }

    #[test]
    fn pranzeria_header_infers_previous_year_for_new_year_week() {
        let today = Date::from_calendar_date(2026, Month::January, 1).expect("valid date");
        let (date_iso, trailing) =
            parse_pranzeria_day_header("Maanantai 29.12. Salaatti", today).expect("header");
        assert_eq!(date_iso, "2025-12-29");
        assert_eq!(trailing, "Salaatti");
    }

    #[test]
    fn pranzeria_header_rejects_time_ranges() {
        let today = Date::from_calendar_date(2026, Month::March, 30).expect("valid date");
        assert!(parse_pranzeria_day_header("10.30-14.00", today).is_none());
    }
}
