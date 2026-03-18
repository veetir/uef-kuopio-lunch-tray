//! Menu provider fetch and parse logic.

mod antell_provider;
mod compass;
mod huomen;
mod pranzeria;

use crate::format::normalize_text;
use crate::log::log_line;
use crate::model::TodayMenu;
use crate::restaurant::{
    effective_fetch_language, is_hard_closed_today, provider_key, restaurant_for_code, Provider,
    Restaurant,
};
use crate::settings::Settings;
use html_escape::decode_html_entities;
use regex::Regex;
use time::{Month, OffsetDateTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn fetch_today(settings: &Settings, context: &FetchContext) -> FetchOutput {
    let restaurant = restaurant_for_code(
        &settings.restaurant_code,
        settings.enable_antell_restaurants,
    );
    let fetch_language = effective_fetch_language(restaurant, &settings.language);
    if is_hard_closed_today(restaurant) {
        log_fetch_skip(
            context,
            restaurant,
            &settings.language,
            &fetch_language,
            "hard_closed_today",
            "",
        );
        return closed_today_fetch_output(restaurant, &settings.language);
    }

    let result = match restaurant.provider {
        Provider::Compass => compass::fetch_compass(settings, restaurant, context),
        Provider::CompassRss => compass::fetch_compass_rss(settings, restaurant, context),
        Provider::Antell => antell_provider::fetch_antell(settings, restaurant, context),
        Provider::HuomenJson => huomen::fetch_huomen(settings, restaurant, context),
        Provider::PranzeriaHtml => pranzeria::fetch_pranzeria(settings, restaurant, context),
    };

    log_fetch_result(
        context,
        restaurant,
        &settings.language,
        &fetch_language,
        &result,
    );
    result
}

pub fn parse_cached_payload(
    raw_payload: &str,
    provider: Provider,
    restaurant: Restaurant,
    language: &str,
) -> anyhow::Result<FetchOutput> {
    if is_hard_closed_today(restaurant) {
        return Ok(closed_today_fetch_output(restaurant, language));
    }

    match provider {
        Provider::Compass => compass::parse_cached_compass_payload(raw_payload),
        Provider::CompassRss => Ok(compass::parse_compass_rss_payload(
            raw_payload,
            restaurant,
            language,
        )),
        Provider::Antell => Ok(antell_provider::parse_cached_antell_payload(
            raw_payload,
            restaurant,
        )),
        Provider::PranzeriaHtml => Ok(pranzeria::parse_pranzeria_payload(raw_payload, restaurant)),
        Provider::HuomenJson => huomen::parse_huomen_payload(raw_payload, restaurant, language),
    }
}

pub fn closed_today_fetch_output(restaurant: Restaurant, _language: &str) -> FetchOutput {
    let today = local_today_key();
    FetchOutput {
        ok: true,
        error_message: String::new(),
        today_menu: Some(TodayMenu {
            date_iso: today.clone(),
            lunch_time: String::new(),
            menus: Vec::new(),
        }),
        restaurant_name: restaurant.name.to_string(),
        restaurant_url: restaurant.url.unwrap_or_default().to_string(),
        provider: restaurant.provider,
        raw_json: String::new(),
        payload_date: today,
    }
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

fn log_fetch_skip(
    context: &FetchContext,
    restaurant: Restaurant,
    ui_language: &str,
    fetch_language: &str,
    decision: &str,
    extra: &str,
) {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(" {}", extra)
    };
    let detail = if context.detail.is_empty() {
        String::new()
    } else {
        format!(" detail={}", context.detail)
    };
    log_line(&format!(
        "fetch skip mode={} reason={} decision={} code={} provider={} ui_language={} fetch_language={}{}{}",
        context.mode.as_str(),
        context.reason.as_str(),
        decision,
        restaurant.code,
        provider_key(restaurant.provider),
        ui_language,
        fetch_language,
        detail,
        extra,
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

pub(super) fn strip_html_text(raw_html: &str) -> String {
    let without_tags = Regex::new(r"<[^>]*>")
        .ok()
        .map(|re| re.replace_all(raw_html, " ").to_string())
        .unwrap_or_else(|| raw_html.to_string());
    normalize_text(decode_html_entities(&without_tags).as_ref())
}

pub(super) fn parse_dot_date_iso(date_text: &str) -> Option<String> {
    parse_date_iso(date_text, r"(\d{1,2})\.(\d{1,2})\.(\d{2,4})")
}

pub(super) fn parse_date_iso(date_text: &str, pattern: &str) -> Option<String> {
    let clean = normalize_text(date_text);
    if clean.is_empty() {
        return None;
    }

    let regex = Regex::new(pattern).ok()?;
    let captures = regex.captures(&clean)?;

    let day = captures.get(1)?.as_str().parse::<u8>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u8>().ok()?;
    let mut year = captures.get(3)?.as_str().parse::<i32>().ok()?;

    if day == 0 || month == 0 || year <= 0 {
        return None;
    }

    if year < 100 {
        year += 2000;
    }

    let month_enum = match Month::try_from(month) {
        Ok(value) => value,
        Err(_) => return None,
    };

    if time::Date::from_calendar_date(year, month_enum, day).is_err() {
        return None;
    }

    Some(format!("{:04}-{:02}-{:02}", year, month, day))
}

pub(super) fn weekday_token() -> &'static str {
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

pub(super) fn local_today_key() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let date = now.date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restaurant::restaurant_for_code;

    #[test]
    fn closed_today_output_has_empty_menu_for_today() {
        let restaurant = restaurant_for_code("huomen-bioteknia", true);
        let result = closed_today_fetch_output(restaurant, "en");
        assert!(result.ok);
        assert_eq!(result.payload_date, local_today_key());
        let today_menu = result.today_menu.expect("today_menu");
        assert_eq!(today_menu.date_iso, local_today_key());
        assert!(today_menu.menus.is_empty());
    }
}
