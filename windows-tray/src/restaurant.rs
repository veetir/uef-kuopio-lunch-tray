//! Static restaurant catalogue and provider-specific lookup helpers.

use time::{OffsetDateTime, Weekday};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Upstream provider implementation used to fetch and parse a restaurant's menu.
pub enum Provider {
    Compass,
    CompassRss,
    Antell,
    HuomenJson,
    PranzeriaHtml,
}

#[derive(Debug, Clone, Copy)]
/// Static metadata for a supported restaurant.
pub struct Restaurant {
    pub code: &'static str,
    pub name: &'static str,
    pub provider: Provider,
    pub antell_slug: Option<&'static str>,
    pub rss_cost_number: Option<&'static str>,
    pub huomen_api_base: Option<&'static str>,
    pub compass_fallback_language: Option<&'static str>,
    pub url: Option<&'static str>,
}

const CORE_RESTAURANTS: [Restaurant; 5] = [
    Restaurant {
        code: "0437",
        name: "Snellmania",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: None,
    },
    Restaurant {
        code: "snellari-rss",
        name: "Cafe Snellari",
        provider: Provider::CompassRss,
        antell_slug: None,
        rss_cost_number: Some("4370"),
        huomen_api_base: None,
        compass_fallback_language: None,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/cafe-snellari/",
        ),
    },
    Restaurant {
        code: "0436",
        name: "Canthia",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: None,
    },
    Restaurant {
        code: "0439",
        name: "Tietoteknia",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: None,
    },
    Restaurant {
        code: "huomen-bioteknia",
        name: "Hyvä Huomen Bioteknia",
        provider: Provider::HuomenJson,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: Some(
            "https://europe-west1-luncher-7cf76.cloudfunctions.net/api/v1/week/a96b7ccf-2c3d-432a-8504-971dbb6d55d3/active",
        ),
        compass_fallback_language: None,
        url: Some("https://hyvahuomen.fi/bioteknia/"),
    },
];

const ANTELL_RESTAURANTS: [Restaurant; 2] = [
    Restaurant {
        code: "antell-round",
        name: "Antell Round",
        provider: Provider::Antell,
        antell_slug: Some("round"),
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: Some("https://antell.fi/lounas/kuopio/round/"),
    },
    Restaurant {
        code: "antell-highway",
        name: "Antell Highway",
        provider: Provider::Antell,
        antell_slug: Some("highway"),
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: Some("https://antell.fi/lounas/kuopio/highway/"),
    },
];

const EXTRA_RESTAURANTS: [Restaurant; 3] = [
    Restaurant {
        code: "043601",
        name: "Mediteknia",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopisto-mediteknia/",
        ),
    },
    Restaurant {
        code: "pranzeria-html",
        name: "Pranzeria Sorrento",
        provider: Provider::PranzeriaHtml,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: None,
        url: Some("https://www.sorrento.fi/pranzeria/"),
    },
    Restaurant {
        code: "3488",
        name: "Caari",
        provider: Provider::Compass,
        antell_slug: None,
        rss_cost_number: None,
        huomen_api_base: None,
        compass_fallback_language: Some("fi"),
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/caari/",
        ),
    },
];

/// Returns the enabled restaurants in the same order used by UI navigation.
pub fn available_restaurants(enable_antell: bool) -> Vec<Restaurant> {
    let mut list = Vec::new();
    list.extend_from_slice(&CORE_RESTAURANTS);
    if enable_antell {
        list.extend_from_slice(&ANTELL_RESTAURANTS);
    }
    list.extend_from_slice(&EXTRA_RESTAURANTS);
    list
}

/// Resolves a restaurant code to metadata, falling back to the default restaurant.
pub fn restaurant_for_code(code: &str, enable_antell: bool) -> Restaurant {
    let list = available_restaurants(enable_antell);
    list.into_iter()
        .find(|r| r.code == code)
        .unwrap_or(CORE_RESTAURANTS[0])
}

/// Resolves the restaurant used for a numeric shortcut index, if present.
pub fn restaurant_for_shortcut_index(index: usize, enable_antell: bool) -> Option<Restaurant> {
    available_restaurants(enable_antell).get(index).copied()
}

/// Returns the Compass fetch language after applying provider-specific fallback rules.
pub fn compass_fetch_language(restaurant: Restaurant, requested_language: &str) -> &str {
    if requested_language == "en" {
        restaurant
            .compass_fallback_language
            .unwrap_or(requested_language)
    } else {
        requested_language
    }
}

/// Returns the effective language used when fetching data for the given provider.
pub fn effective_fetch_language(restaurant: Restaurant, requested_language: &str) -> String {
    match restaurant.provider {
        Provider::Compass => compass_fetch_language(restaurant, requested_language).to_string(),
        _ => requested_language.to_string(),
    }
}

/// Reports whether the restaurant is considered closed on the current local weekday.
pub fn is_hard_closed_today(restaurant: Restaurant) -> bool {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    is_hard_closed_on_weekday(restaurant, now.weekday())
}

/// Reports whether the restaurant is considered closed on a specific weekday.
pub fn is_hard_closed_on_weekday(restaurant: Restaurant, weekday: Weekday) -> bool {
    match weekday {
        Weekday::Sunday => true,
        Weekday::Saturday => restaurant.code != "0437",
        _ => false,
    }
}

/// Returns the stable provider key used in cache filenames and logging.
pub fn provider_key(provider: Provider) -> &'static str {
    match provider {
        Provider::Compass => "compass",
        Provider::CompassRss => "compass-rss",
        Provider::Antell => "antell",
        Provider::HuomenJson => "huomen-json",
        Provider::PranzeriaHtml => "pranzeria",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restaurant_order_matches_shortcut_order() {
        let codes: Vec<&str> = available_restaurants(true)
            .into_iter()
            .map(|restaurant| restaurant.code)
            .collect();
        assert_eq!(
            codes,
            vec![
                "0437",
                "snellari-rss",
                "0436",
                "0439",
                "huomen-bioteknia",
                "antell-round",
                "antell-highway",
                "043601",
                "pranzeria-html",
                "3488"
            ]
        );
    }

    #[test]
    fn caari_english_fetch_falls_back_to_finnish() {
        let caari = restaurant_for_code("3488", true);
        assert_eq!(compass_fetch_language(caari, "en"), "fi");
        assert_eq!(compass_fetch_language(caari, "fi"), "fi");
    }

    #[test]
    fn saturday_only_snellmania_is_not_hard_closed() {
        let saturday = Weekday::Saturday;
        assert!(!is_hard_closed_on_weekday(
            restaurant_for_code("0437", true),
            saturday
        ));
        assert!(is_hard_closed_on_weekday(
            restaurant_for_code("huomen-bioteknia", true),
            saturday
        ));
        assert!(is_hard_closed_on_weekday(
            restaurant_for_code("pranzeria-html", true),
            saturday
        ));
    }

    #[test]
    fn sunday_is_hard_closed_for_every_restaurant() {
        for restaurant in available_restaurants(true) {
            assert!(is_hard_closed_on_weekday(restaurant, Weekday::Sunday));
        }
    }
}
