//! Static restaurant catalogue and API lookup helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Price parsing mode retained for legacy formatting helpers.
pub enum Provider {
    LunchApi,
    Compass,
}

#[derive(Debug, Clone, Copy)]
/// Static metadata for a supported restaurant.
pub struct Restaurant {
    pub code: &'static str,
    pub name: &'static str,
    pub provider: Provider,
    pub url: Option<&'static str>,
}

const CORE_RESTAURANTS: [Restaurant; 5] = [
    Restaurant {
        code: "snellmania",
        name: "Snellmania",
        provider: Provider::LunchApi,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopistosnellmania/",
        ),
    },
    Restaurant {
        code: "cafe-snellari",
        name: "Cafe Snellari",
        provider: Provider::LunchApi,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/cafe-snellari/",
        ),
    },
    Restaurant {
        code: "canthia",
        name: "Canthia",
        provider: Provider::LunchApi,
        url: None,
    },
    Restaurant {
        code: "tietoteknia",
        name: "Tietoteknia",
        provider: Provider::LunchApi,
        url: None,
    },
    Restaurant {
        code: "hyva-huomen-bioteknia",
        name: "Hyvä Huomen Bioteknia",
        provider: Provider::LunchApi,
        url: Some("https://hyvahuomen.fi/bioteknia/"),
    },
];

const ANTELL_RESTAURANTS: [Restaurant; 2] = [
    Restaurant {
        code: "antell-round",
        name: "Antell Round",
        provider: Provider::LunchApi,
        url: Some("https://antell.fi/lounas/kuopio/round/"),
    },
    Restaurant {
        code: "antell-highway",
        name: "Antell Highway",
        provider: Provider::LunchApi,
        url: Some("https://antell.fi/lounas/kuopio/highway/"),
    },
];

const EXTRA_RESTAURANTS: [Restaurant; 3] = [
    Restaurant {
        code: "mediteknia",
        name: "Mediteknia",
        provider: Provider::LunchApi,
        url: Some(
            "https://www.compass-group.fi/ravintolat-ja-ruokalistat/foodco/kaupungit/kuopio/ita-suomen-yliopisto-mediteknia/",
        ),
    },
    Restaurant {
        code: "pranzeria-sorrento",
        name: "Pranzeria Sorrento",
        provider: Provider::LunchApi,
        url: Some("https://www.sorrento.fi/pranzeria/"),
    },
    Restaurant {
        code: "caari",
        name: "Caari",
        provider: Provider::LunchApi,
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
    let code = permanent_restaurant_id(code);
    let list = available_restaurants(enable_antell);
    list.into_iter()
        .find(|r| r.code == code)
        .unwrap_or(CORE_RESTAURANTS[0])
}

/// Maps identifiers persisted by pre-API releases to the public API IDs.
pub fn permanent_restaurant_id(code: &str) -> &str {
    match code {
        "0437" => "snellmania",
        "snellari-rss" => "cafe-snellari",
        "0436" => "canthia",
        "0439" => "tietoteknia",
        "huomen-bioteknia" => "hyva-huomen-bioteknia",
        "043601" => "mediteknia",
        "pranzeria-html" => "pranzeria-sorrento",
        "3488" => "caari",
        _ => code,
    }
}

/// Resolves the restaurant used for a numeric shortcut index, if present.
pub fn restaurant_for_shortcut_index(index: usize, enable_antell: bool) -> Option<Restaurant> {
    available_restaurants(enable_antell).get(index).copied()
}

/// The API accepts the same language keys as the UI.
pub fn effective_fetch_language(_restaurant: Restaurant, requested_language: &str) -> String {
    requested_language.to_string()
}

/// Returns the stable provider key used in cache filenames and logging.
pub fn provider_key(provider: Provider) -> &'static str {
    match provider {
        Provider::LunchApi => "lunch-api",
        Provider::Compass => "compass",
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
                "snellmania",
                "cafe-snellari",
                "canthia",
                "tietoteknia",
                "hyva-huomen-bioteknia",
                "antell-round",
                "antell-highway",
                "mediteknia",
                "pranzeria-sorrento",
                "caari"
            ]
        );
    }

    #[test]
    fn legacy_ids_resolve_to_permanent_ids() {
        assert_eq!(restaurant_for_code("0439", true).code, "tietoteknia");
        assert_eq!(
            restaurant_for_code("pranzeria-html", true).code,
            "pranzeria-sorrento"
        );
    }

    #[test]
    fn api_fetch_language_uses_requested_language() {
        let caari = restaurant_for_code("caari", true);
        assert_eq!(effective_fetch_language(caari, "en"), "en");
    }
}
