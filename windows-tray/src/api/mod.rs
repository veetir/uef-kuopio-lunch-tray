//! Normalized lunch API fetch and parse logic.

use crate::format::normalize_text;
use crate::log::log_line;
use crate::model::TodayMenu;
use crate::restaurant::{
    available_restaurants, effective_fetch_language, provider_key, restaurant_for_code, Provider,
    Restaurant,
};
use crate::settings::Settings;
use std::collections::HashSet;
use std::sync::OnceLock;
use time::{Date, Month, OffsetDateTime, Time, UtcOffset, Weekday};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// High-level fetch mode used for logging and fetch policy decisions.
pub enum FetchMode {
    Current,
    Background,
    Direct,
}

impl FetchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Background => "background",
            Self::Direct => "direct",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Reason a fetch was requested.
pub enum FetchReason {
    StartupMissingCache,
    StartupStaleDate,
    StartupRefreshInterval,
    ManualRefresh,
    RefreshTimer,
    MidnightRollover,
    StaleDateCheck,
    RetryTimer,
    SelectionMissingCache,
    SelectionStaleDate,
    SelectionRefreshInterval,
    LanguageSwitchMissingCache,
    LanguageSwitchStaleDate,
    LanguageSwitchRefreshInterval,
    PrefetchMissingCache,
    PrefetchStaleDate,
    PrintTodayCli,
}

impl FetchReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartupMissingCache => "startup_missing_cache",
            Self::StartupStaleDate => "startup_stale_date",
            Self::StartupRefreshInterval => "startup_refresh_interval",
            Self::ManualRefresh => "manual_refresh",
            Self::RefreshTimer => "refresh_timer",
            Self::MidnightRollover => "midnight_rollover",
            Self::StaleDateCheck => "stale_date_check",
            Self::RetryTimer => "retry_timer",
            Self::SelectionMissingCache => "selection_missing_cache",
            Self::SelectionStaleDate => "selection_stale_date",
            Self::SelectionRefreshInterval => "selection_refresh_interval",
            Self::LanguageSwitchMissingCache => "language_switch_missing_cache",
            Self::LanguageSwitchStaleDate => "language_switch_stale_date",
            Self::LanguageSwitchRefreshInterval => "language_switch_refresh_interval",
            Self::PrefetchMissingCache => "prefetch_missing_cache",
            Self::PrefetchStaleDate => "prefetch_stale_date",
            Self::PrintTodayCli => "print_today_cli",
        }
    }
}

#[derive(Debug, Clone)]
/// Context attached to a fetch request for logging and policy decisions.
pub struct FetchContext {
    pub mode: FetchMode,
    pub reason: FetchReason,
    pub detail: String,
}

impl FetchContext {
    pub fn new(mode: FetchMode, reason: FetchReason) -> Self {
        Self {
            mode,
            reason,
            detail: String::new(),
        }
    }
}

/// Result of fetching or parsing a lunch API payload.
pub struct FetchOutput {
    pub ok: bool,
    pub is_stale: bool,
    pub error_message: String,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub provider: Provider,
    pub raw_json: String,
    pub payload_date: String,
}

/// Fetches today's menu for the currently selected restaurant.
pub fn fetch_today(settings: &Settings, context: &FetchContext) -> FetchOutput {
    let restaurant = restaurant_for_code(
        &settings.restaurant_code,
        settings.enable_antell_restaurants,
    );
    let fetch_language = effective_fetch_language(restaurant, &settings.language);
    let result = fetch_lunch_api(settings, restaurant, context);

    log_fetch_result(
        context,
        restaurant,
        &settings.language,
        &fetch_language,
        &result,
    );
    result
}

/// Fetches and parses the cache-only daily snapshot for every supported restaurant.
pub fn fetch_daily_snapshot(
    settings: &Settings,
    context: &FetchContext,
) -> anyhow::Result<Vec<(String, FetchOutput)>> {
    let date = local_today_key();
    let url = format!(
        "{}/snapshot?language={}&date={}",
        LUNCH_API_BASE_URL, settings.language, date
    );
    log_line(&format!(
        "snapshot request mode={} reason={} language={} url={}",
        context.mode.as_str(),
        context.reason.as_str(),
        settings.language,
        url,
    ));

    let client = lunch_api_client()?;
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()?;
    let status = response.status();
    let body = response.text()?;
    anyhow::ensure!(
        status.is_success(),
        "Lunch API returned HTTP {}",
        status.as_u16()
    );
    let results = parse_lunch_api_snapshot_payload(&body, &settings.language, &date)?;
    log_line(&format!(
        "snapshot result mode={} reason={} language={} menus={}",
        context.mode.as_str(),
        context.reason.as_str(),
        settings.language,
        results.len(),
    ));
    Ok(results)
}

/// Parses a previously cached lunch API response.
pub fn parse_cached_payload(
    raw_payload: &str,
    provider: Provider,
    restaurant: Restaurant,
    language: &str,
) -> anyhow::Result<FetchOutput> {
    debug_assert_eq!(provider, Provider::LunchApi);
    parse_lunch_api_payload(raw_payload, restaurant, language)
}

const LUNCH_API_BASE_URL: &str = "https://lunch.veeti.dev/v1";
static LUNCH_API_CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();

fn lunch_api_client() -> anyhow::Result<&'static reqwest::blocking::Client> {
    LUNCH_API_CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent(format!("LunchTray-Windows/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|err| err.to_string())
        })
        .as_ref()
        .map_err(|err| anyhow::anyhow!(err.clone()))
}

fn fetch_lunch_api(
    settings: &Settings,
    restaurant: Restaurant,
    context: &FetchContext,
) -> FetchOutput {
    let date = local_today_key();
    let url = format!(
        "{}/restaurants/{}/menu?language={}&date={}",
        LUNCH_API_BASE_URL, restaurant.code, settings.language, date
    );
    log_fetch_attempt(
        context,
        restaurant,
        &settings.language,
        &settings.language,
        &url,
    );

    let request = match lunch_api_client() {
        Ok(client) => client
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send(),
        Err(err) => return failed_api_output(restaurant, String::new(), err.to_string()),
    };

    match request {
        Ok(response) => {
            let status = response.status();
            match response.text() {
                Ok(body) if status.is_success() => {
                    match parse_lunch_api_payload(&body, restaurant, &settings.language) {
                        Ok(result) => result,
                        Err(err) => failed_api_output(restaurant, body, err.to_string()),
                    }
                }
                Ok(body) => failed_api_output(
                    restaurant,
                    body,
                    format!("Lunch API returned HTTP {}", status.as_u16()),
                ),
                Err(err) => failed_api_output(restaurant, String::new(), err.to_string()),
            }
        }
        Err(err) => failed_api_output(restaurant, String::new(), err.to_string()),
    }
}

pub fn failed_fetch_output(restaurant: Restaurant, error_message: String) -> FetchOutput {
    failed_api_output(restaurant, String::new(), error_message)
}

fn failed_api_output(
    restaurant: Restaurant,
    raw_json: String,
    error_message: String,
) -> FetchOutput {
    FetchOutput {
        ok: false,
        is_stale: false,
        error_message,
        today_menu: None,
        restaurant_name: restaurant.name.to_string(),
        restaurant_url: restaurant.url.unwrap_or_default().to_string(),
        provider: Provider::LunchApi,
        raw_json,
        payload_date: String::new(),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiMenuResponse {
    api_version: String,
    schema_version: u32,
    restaurant: ApiRestaurant,
    date: String,
    service: ApiService,
    #[serde(default)]
    offers: Vec<ApiOffer>,
    #[serde(default)]
    groups: Vec<ApiGroup>,
    #[serde(default)]
    freshness: ApiFreshness,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiSnapshotResponse {
    api_version: String,
    schema_version: u32,
    requested_language: String,
    date: String,
    menus: Vec<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRestaurant {
    id: String,
    name: ApiLocalizedText,
    website_url: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiLocalizedText {
    fi: String,
    en: String,
}

#[derive(Debug, serde::Deserialize)]
struct ApiService {
    status: String,
    #[serde(default)]
    hours: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFreshness {
    #[serde(default)]
    is_stale: bool,
}

#[derive(Debug, serde::Deserialize)]
struct ApiPrice {
    amount: String,
    #[serde(default)]
    audiences: Vec<crate::model::PriceAudience>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiOffer {
    label: String,
    price: ApiPrice,
    #[serde(default)]
    description: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiGroup {
    #[serde(default)]
    title: String,
    #[serde(default)]
    prices: Vec<ApiPrice>,
    #[serde(default)]
    items: Vec<ApiItem>,
    sort_order: i32,
}

#[derive(Debug, serde::Deserialize)]
struct ApiItem {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    recipe: Option<ApiRecipe>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRecipe {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    ingredients: String,
    #[serde(default)]
    nutrition_per100g: Vec<crate::model::NutritionalValue>,
    #[serde(rename = "co2eKilogramsPer100Grams")]
    co2e_kilograms_per100g: Option<f32>,
    #[serde(default)]
    diets: Vec<String>,
}

fn parse_lunch_api_payload(
    raw_payload: &str,
    restaurant: Restaurant,
    language: &str,
) -> anyhow::Result<FetchOutput> {
    let mut payload: ApiMenuResponse = serde_json::from_str(raw_payload)?;
    anyhow::ensure!(
        payload.api_version == "v1" && payload.schema_version == 1,
        "unsupported Lunch API response"
    );
    anyhow::ensure!(
        payload.restaurant.id == restaurant.code,
        "Lunch API restaurant mismatch"
    );
    if !matches!(
        payload.service.status.as_str(),
        "serving" | "closed" | "noMenu" | "unknown"
    ) {
        payload.service.status = "unknown".to_string();
    }

    let restaurant_name = if language == "en" {
        payload.restaurant.name.en
    } else {
        payload.restaurant.name.fi
    };
    let restaurant_url = payload
        .restaurant
        .website_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .or(restaurant.url)
        .unwrap_or_default()
        .to_string();
    if payload.service.status == "unknown" {
        return Ok(FetchOutput {
            ok: false,
            is_stale: payload.freshness.is_stale,
            error_message: if language == "fi" {
                "Ruokalistaa ei saatavilla".to_string()
            } else {
                "Menu unavailable".to_string()
            },
            today_menu: None,
            restaurant_name,
            restaurant_url,
            provider: Provider::LunchApi,
            raw_json: raw_payload.to_string(),
            payload_date: payload.date,
        });
    }

    let today_menu = if payload.service.status == "closed" {
        None
    } else {
        let mut menus = Vec::new();
        for offer in payload.offers {
            let price = api_prices_display(std::slice::from_ref(&offer.price), language);
            let prices = vec![crate::model::MenuPrice {
                amount: offer.price.amount,
                audiences: offer.price.audiences,
            }];
            menus.push((
                -1000,
                crate::model::MenuGroup {
                    name: normalize_text(&offer.label),
                    price,
                    prices,
                    presentation: crate::model::MenuGroupPresentation::GeneralOffer,
                    components: if normalize_text(&offer.description).is_empty() {
                        Vec::new()
                    } else {
                        vec![normalize_text(&offer.description)]
                    },
                    component_recipe_ids: Vec::new(),
                    component_recipe_details: Vec::new(),
                },
            ));
        }
        for group in payload.groups {
            let mut components = Vec::new();
            let mut recipe_ids = Vec::new();
            let mut recipe_details = Vec::new();
            for item in group.items {
                let mut component = normalize_text(&item.name);
                let description = normalize_text(&item.description);
                if !description.is_empty() && description != component {
                    component.push_str(" – ");
                    component.push_str(&description);
                }
                let tags: Vec<String> = item
                    .tags
                    .iter()
                    .map(|tag| normalize_text(tag))
                    .filter(|tag| !tag.is_empty())
                    .collect();
                if !tags.is_empty() {
                    component.push_str(" (");
                    component.push_str(&tags.join(", "));
                    component.push(')');
                }
                if component.is_empty() {
                    continue;
                }
                let detail = item.recipe.map(recipe_from_api);
                recipe_ids.push(detail.as_ref().map(|recipe| recipe.recipe_id));
                recipe_details.push(detail);
                components.push(component);
            }
            if components.is_empty() {
                continue;
            }
            let prices: Vec<crate::model::MenuPrice> = group
                .prices
                .iter()
                .map(|price| crate::model::MenuPrice {
                    amount: price.amount.clone(),
                    audiences: price.audiences.clone(),
                })
                .collect();
            menus.push((
                group.sort_order,
                crate::model::MenuGroup {
                    name: normalize_text(&group.title),
                    price: api_prices_display(&group.prices, language),
                    prices,
                    presentation: crate::model::MenuGroupPresentation::Standard,
                    components,
                    component_recipe_ids: recipe_ids,
                    component_recipe_details: recipe_details,
                },
            ));
        }
        menus.sort_by_key(|(sort_order, _)| *sort_order);
        Some(TodayMenu {
            date_iso: payload.date.clone(),
            lunch_time: normalize_text(&payload.service.hours),
            menus: menus.into_iter().map(|(_, menu)| menu).collect(),
        })
    };

    Ok(FetchOutput {
        ok: true,
        is_stale: payload.freshness.is_stale,
        error_message: String::new(),
        today_menu,
        restaurant_name,
        restaurant_url,
        provider: Provider::LunchApi,
        raw_json: raw_payload.to_string(),
        payload_date: payload.date,
    })
}

fn parse_lunch_api_snapshot_payload(
    raw_payload: &str,
    language: &str,
    expected_date: &str,
) -> anyhow::Result<Vec<(String, FetchOutput)>> {
    let payload: ApiSnapshotResponse = serde_json::from_str(raw_payload)?;
    anyhow::ensure!(
        payload.api_version == "v1" && payload.schema_version == 1,
        "unsupported Lunch API snapshot"
    );
    anyhow::ensure!(
        payload.requested_language == language,
        "Lunch API snapshot language mismatch"
    );
    anyhow::ensure!(
        payload.date == expected_date,
        "Lunch API snapshot date mismatch"
    );

    let restaurants = available_restaurants(true);
    let mut seen = HashSet::new();
    let mut results = Vec::with_capacity(restaurants.len());
    for menu in payload.menus {
        let Some(code) = menu
            .get("restaurant")
            .and_then(|restaurant| restaurant.get("id"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let Some(restaurant) = restaurants
            .iter()
            .copied()
            .find(|restaurant| restaurant.code == code)
        else {
            continue;
        };
        if !seen.insert(code.to_string()) {
            continue;
        }
        let Ok(raw_menu) = serde_json::to_string(&menu) else {
            continue;
        };
        let Ok(result) = parse_lunch_api_payload(&raw_menu, restaurant, language) else {
            continue;
        };
        results.push((code.to_string(), result));
    }

    anyhow::ensure!(
        !results.is_empty(),
        "Lunch API snapshot contained no supported restaurants"
    );
    Ok(results)
}

fn api_prices_display(prices: &[ApiPrice], language: &str) -> String {
    let mut parts = Vec::new();
    for price in prices {
        let amount = format_euro_amount(&price.amount);
        if price.audiences.is_empty() {
            parts.push(amount);
            continue;
        }
        for audience in &price.audiences {
            let label = match (language, audience) {
                ("fi", crate::model::PriceAudience::Student) => "Opiskelija",
                ("fi", crate::model::PriceAudience::Staff) => "Henkilökunta",
                ("fi", crate::model::PriceAudience::Guest) => "Vierailija",
                (_, crate::model::PriceAudience::Student) => "Student",
                (_, crate::model::PriceAudience::Staff) => "Staff",
                (_, crate::model::PriceAudience::Guest) => "Guest",
            };
            parts.push(format!("{} {}", label, amount));
        }
    }
    parts.join(" / ")
}

fn format_euro_amount(amount: &str) -> String {
    format!("{} €", normalize_text(amount).replace('.', ","))
}

fn recipe_from_api(recipe: ApiRecipe) -> crate::model::RecipeInfo {
    crate::model::RecipeInfo {
        recipe_id: stable_recipe_id(&recipe.id),
        name: recipe.name,
        ingredients_cleaned: recipe.ingredients,
        nutritional_values: recipe.nutrition_per100g,
        kg_co2e_per100g: recipe.co2e_kilograms_per100g,
        diets: recipe.diets.join(", "),
    }
}

fn stable_recipe_id(value: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash.max(1)
}

/// Returns an API-provided closure notice for the selected language.
pub fn closure_notice(raw_payload: &str, language: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        service: ApiService,
        closure: Option<Closure>,
        #[serde(default)]
        date: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Closure {
        ends_on: String,
        reason: Option<String>,
    }
    let payload: Envelope = serde_json::from_str(raw_payload).ok()?;
    if payload.service.status != "closed" {
        return None;
    }
    let Some(closure) = payload.closure else {
        return Some(if language == "fi" {
            "Suljettu".to_string()
        } else {
            "Closed".to_string()
        });
    };
    let mut notice = if language == "fi" {
        format!(
            "Suljettu {} asti",
            display_closure_until_date(&closure.ends_on, &payload.date, language)
        )
    } else {
        format!(
            "Closed until {}",
            display_closure_until_date(&closure.ends_on, &payload.date, language)
        )
    };
    if let Some(reason) = closure.reason.map(|value| normalize_text(&value)) {
        if !reason.is_empty() {
            notice.push_str(". ");
            notice.push_str(&reason);
        }
    }
    Some(notice)
}

fn display_closure_until_date(value: &str, reference_date: &str, language: &str) -> String {
    let Some((year, month, day)) = parse_iso_date(value) else {
        return value.to_string();
    };
    let include_year = parse_iso_date(reference_date)
        .map(|(reference_year, _, _)| reference_year != year)
        .unwrap_or(true);

    let month_index = usize::try_from(month.saturating_sub(1)).unwrap_or(usize::MAX);
    if language == "fi" {
        const MONTHS: [&str; 12] = [
            "tammikuuta",
            "helmikuuta",
            "maaliskuuta",
            "huhtikuuta",
            "toukokuuta",
            "kesäkuuta",
            "heinäkuuta",
            "elokuuta",
            "syyskuuta",
            "lokakuuta",
            "marraskuuta",
            "joulukuuta",
        ];
        let month_name = MONTHS.get(month_index).copied().unwrap_or("");
        if month_name.is_empty() {
            return value.to_string();
        }
        return if include_year {
            format!("{day}. {month_name} {year}")
        } else {
            format!("{day}. {month_name}")
        };
    }

    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_name = MONTHS.get(month_index).copied().unwrap_or("");
    if month_name.is_empty() {
        return value.to_string();
    }
    if include_year {
        format!("{day} {month_name} {year}")
    } else {
        format!("{day} {month_name}")
    }
}

fn parse_iso_date(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

pub(super) fn log_fetch_attempt(
    context: &FetchContext,
    restaurant: Restaurant,
    ui_language: &str,
    fetch_language: &str,
    url: &str,
) {
    let detail = if context.detail.is_empty() {
        String::new()
    } else {
        format!(" detail={}", context.detail)
    };
    log_line(&format!(
        "fetch request mode={} reason={} code={} provider={} ui_language={} fetch_language={} url={}{}",
        context.mode.as_str(),
        context.reason.as_str(),
        restaurant.code,
        provider_key(restaurant.provider),
        ui_language,
        fetch_language,
        url,
        detail,
    ));
}

fn log_fetch_result(
    context: &FetchContext,
    restaurant: Restaurant,
    ui_language: &str,
    fetch_language: &str,
    result: &FetchOutput,
) {
    let payload_date = if result.payload_date.is_empty() {
        "-".to_string()
    } else {
        result.payload_date.clone()
    };
    let detail = if context.detail.is_empty() {
        String::new()
    } else {
        format!(" detail={}", context.detail)
    };
    let err = if result.error_message.is_empty() {
        String::new()
    } else {
        format!(" err={}", result.error_message.replace('\n', " "))
    };
    log_line(&format!(
        "fetch result mode={} reason={} code={} provider={} ui_language={} fetch_language={} ok={} payload_date={} has_today_menu={}{}{}",
        context.mode.as_str(),
        context.reason.as_str(),
        restaurant.code,
        provider_key(restaurant.provider),
        ui_language,
        fetch_language,
        result.ok,
        payload_date,
        result.today_menu.is_some(),
        detail,
        err,
    ));
}

pub(super) fn local_today_key() -> String {
    helsinki_date_key(OffsetDateTime::now_utc())
}

fn helsinki_date_key(now: OffsetDateTime) -> String {
    let year = now.year();
    let daylight_saving_start = last_sunday(year, Month::March)
        .with_time(Time::from_hms(1, 0, 0).expect("valid DST transition time"))
        .assume_utc();
    let daylight_saving_end = last_sunday(year, Month::October)
        .with_time(Time::from_hms(1, 0, 0).expect("valid DST transition time"))
        .assume_utc();
    let offset_hours = if now >= daylight_saving_start && now < daylight_saving_end {
        3
    } else {
        2
    };
    let helsinki_offset =
        UtcOffset::from_hms(offset_hours, 0, 0).expect("valid Helsinki UTC offset");
    let date = now.to_offset(helsinki_offset).date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
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
    use crate::restaurant::restaurant_for_code;

    #[test]
    fn parses_normalized_api_fixture_with_prices_recipes_and_unpriced_items() {
        let restaurant = restaurant_for_code("tietoteknia", true);
        let raw = include_str!("../../../api/test/fixtures/contract-menu.json");
        let result = parse_lunch_api_payload(raw, restaurant, "fi").expect("valid fixture");
        assert!(result.ok);
        assert!(!result.is_stale);
        assert_eq!(result.payload_date, "2026-07-24");
        let today_menu = result.today_menu.expect("today_menu");
        assert_eq!(today_menu.menus.len(), 4);
        assert_eq!(
            today_menu.menus[0].presentation,
            crate::model::MenuGroupPresentation::GeneralOffer
        );
        assert_eq!(today_menu.menus[1].prices.len(), 2);
        assert_eq!(
            today_menu.menus[1].component_recipe_details[0]
                .as_ref()
                .expect("recipe")
                .ingredients_cleaned,
            "Härkäpapu, tomaatti, mausteet"
        );
        assert_eq!(today_menu.menus[3].name, "");
        assert_eq!(today_menu.menus[3].components, vec!["Satokauden kasviksia"]);
    }

    #[test]
    fn preserves_api_stale_marker() {
        let restaurant = restaurant_for_code("tietoteknia", true);
        let mut payload: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api/test/fixtures/contract-menu.json"
        ))
        .expect("menu fixture");
        payload["freshness"]["isStale"] = serde_json::json!(true);
        let result = parse_lunch_api_payload(&payload.to_string(), restaurant, "fi")
            .expect("valid stale fixture");
        assert!(result.ok);
        assert!(result.is_stale);
    }

    #[test]
    fn falls_back_to_local_restaurant_url_when_api_url_is_missing() {
        let restaurant = restaurant_for_code("tietoteknia", true);
        let mut payload: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api/test/fixtures/contract-menu.json"
        ))
        .expect("menu fixture");
        payload["restaurant"]["websiteUrl"] = serde_json::Value::Null;

        let result = parse_lunch_api_payload(&payload.to_string(), restaurant, "fi")
            .expect("valid fixture with missing URL");

        assert_eq!(
            result.restaurant_url,
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/tietoteknia/"
        );
    }

    #[test]
    fn treats_future_service_status_as_unknown() {
        let restaurant = restaurant_for_code("tietoteknia", true);
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api/test/fixtures/contract-menu.json"
        ))
        .expect("fixture");
        let mut future = fixture;
        future["service"]["status"] = serde_json::json!("temporarilyUnavailable");

        let result = parse_lunch_api_payload(&future.to_string(), restaurant, "en")
            .expect("future status should remain readable");
        assert!(!result.ok);
        assert_eq!(result.error_message, "Menu unavailable");
        assert!(result.today_menu.is_none());
    }

    #[test]
    fn uses_helsinki_date_in_winter_and_summer() {
        let winter = Date::from_calendar_date(2026, Month::January, 1)
            .unwrap()
            .with_hms(22, 30, 0)
            .unwrap()
            .assume_utc();
        let summer = Date::from_calendar_date(2026, Month::July, 24)
            .unwrap()
            .with_hms(21, 30, 0)
            .unwrap()
            .assume_utc();

        assert_eq!(helsinki_date_key(winter), "2026-01-02");
        assert_eq!(helsinki_date_key(summer), "2026-07-25");
    }

    #[test]
    fn closure_notice_comes_from_api_payload() {
        let raw = r#"{
            "date":"2026-07-24",
            "service":{"status":"closed"},
            "closure":{
                "startsOn":"2026-06-18",
                "endsOn":"2026-08-09",
                "reason":"Summer break"
            }
        }"#;

        assert_eq!(
            closure_notice(raw, "en").as_deref(),
            Some("Closed until 9 August. Summer break")
        );
        assert_eq!(
            closure_notice(raw, "fi").as_deref(),
            Some("Suljettu 9. elokuuta asti. Summer break")
        );
    }

    #[test]
    fn closure_notice_includes_year_for_cross_year_closure() {
        let raw = r#"{
            "date":"2026-12-20",
            "service":{"status":"closed"},
            "closure":{
                "startsOn":"2026-12-20",
                "endsOn":"2027-01-06"
            }
        }"#;

        assert_eq!(
            closure_notice(raw, "en").as_deref(),
            Some("Closed until 6 January 2027")
        );
    }

    #[test]
    fn rejects_mismatched_restaurant_payload() {
        let restaurant = restaurant_for_code("snellmania", true);
        let raw = include_str!("../../../api/test/fixtures/contract-menu.json");
        assert!(parse_lunch_api_payload(raw, restaurant, "fi").is_err());
    }

    #[test]
    fn parses_daily_snapshot_into_individual_cache_payloads() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api/test/fixtures/contract-menu.json"
        ))
        .expect("menu fixture");
        let menus: Vec<serde_json::Value> = available_restaurants(true)
            .into_iter()
            .map(|restaurant| {
                let mut menu = fixture.clone();
                menu["restaurant"]["id"] = serde_json::json!(restaurant.code);
                menu["restaurant"]["name"]["fi"] = serde_json::json!(restaurant.name);
                menu["restaurant"]["name"]["en"] = serde_json::json!(restaurant.name);
                menu
            })
            .collect();
        let mut menus = menus;
        let mut future_menu = fixture.clone();
        future_menu["restaurant"]["id"] = serde_json::json!("future-restaurant");
        future_menu["restaurant"]["name"]["fi"] = serde_json::json!("Future");
        future_menu["restaurant"]["name"]["en"] = serde_json::json!("Future");
        menus.push(future_menu);
        let mut snapshot = serde_json::json!({
            "apiVersion": "v1",
            "schemaVersion": 1,
            "revision": "test",
            "requestedLanguage": "fi",
            "date": "2026-07-24",
            "restaurants": [],
            "menus": menus
        });

        let results = parse_lunch_api_snapshot_payload(&snapshot.to_string(), "fi", "2026-07-24")
            .expect("valid snapshot");
        assert_eq!(results.len(), 10);
        assert_eq!(results[0].0, "snellmania");
        assert_eq!(results[9].0, "caari");
        assert!(results.iter().all(|(_, result)| result.ok));
        assert!(results
            .iter()
            .all(|(code, result)| result.raw_json.contains(code)));

        snapshot["menus"]
            .as_array_mut()
            .expect("snapshot menus")
            .retain(|menu| menu["restaurant"]["id"] != "caari");
        let partial = parse_lunch_api_snapshot_payload(&snapshot.to_string(), "fi", "2026-07-24")
            .expect("snapshot without one known restaurant");
        assert_eq!(partial.len(), 9);
        assert!(partial.iter().all(|(code, _)| code != "caari"));

        snapshot["menus"]
            .as_array_mut()
            .expect("snapshot menus")
            .push(serde_json::json!({
                "restaurant": {"id": "caari"},
                "service": {"status": "serving"}
            }));
        let malformed = parse_lunch_api_snapshot_payload(&snapshot.to_string(), "fi", "2026-07-24")
            .expect("snapshot with one malformed known restaurant");
        assert_eq!(malformed.len(), 9);
        assert!(malformed.iter().all(|(code, _)| code != "caari"));
    }

    #[test]
    #[ignore = "requires the deployed lunch API"]
    fn live_api_payloads_match_the_windows_contract() {
        let date = local_today_key();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("client");
        for language in ["fi", "en"] {
            let url = format!(
                "{}/snapshot?language={}&date={}",
                LUNCH_API_BASE_URL, language, date
            );
            let raw = client
                .get(url)
                .send()
                .expect("request")
                .error_for_status()
                .expect("successful response")
                .text()
                .expect("response body");
            parse_lunch_api_snapshot_payload(&raw, language, &date)
                .unwrap_or_else(|err| panic!("{language}: {err}"));
        }
    }
}
