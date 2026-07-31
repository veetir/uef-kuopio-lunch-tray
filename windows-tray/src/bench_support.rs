use crate::api;
use crate::format;
use crate::model::{
    MenuGroup, MenuGroupPresentation, MenuPrice, NutritionalValue, PriceAudience, RecipeInfo,
    TodayMenu,
};
use crate::popup;
use crate::restaurant::{restaurant_for_code, Provider, Restaurant};
use crate::settings::{LunchItemDisplayMode, Settings};
use crate::state::{AppState, FetchStatus};

pub const FIXTURE_DATE: &str = "2026-07-24";
pub const LUNCH_API_JSON: &str = include_str!("../../api/test/fixtures/contract-menu.json");

pub struct ProviderFixture {
    pub name: &'static str,
    pub raw_payload: &'static str,
    pub provider: Provider,
    pub restaurant: Restaurant,
    pub language: &'static str,
}

pub fn provider_fixtures() -> Vec<ProviderFixture> {
    vec![ProviderFixture {
        name: "lunch_api_json",
        raw_payload: LUNCH_API_JSON,
        provider: Provider::LunchApi,
        restaurant: restaurant_for_code("tietoteknia", true),
        language: "fi",
    }]
}

pub fn parse_provider_fixture(fixture: &ProviderFixture) -> usize {
    let output = api::parse_cached_payload(
        fixture.raw_payload,
        fixture.provider,
        fixture.restaurant,
        fixture.language,
    )
    .expect("fixture parses");
    output
        .today_menu
        .as_ref()
        .map(|menu| menu.menus.iter().map(|group| group.components.len()).sum())
        .unwrap_or_default()
}

pub fn sample_app_state() -> AppState {
    AppState {
        settings: sample_settings(),
        status: FetchStatus::Ok,
        loading_started_epoch_ms: 0,
        error_message: String::new(),
        stale_network_error: false,
        today_menu: Some(sample_today_menu()),
        restaurant_name: "Tietoteknia".to_string(),
        restaurant_url: "https://example.invalid/tietoteknia".to_string(),
        raw_payload: LUNCH_API_JSON.repeat(4),
        provider: Provider::LunchApi,
        payload_date: FIXTURE_DATE.to_string(),
        stale_date: false,
    }
}

pub fn bench_build_line_count(state: &AppState) -> usize {
    popup::bench_build_line_count(state)
}

pub fn bench_favorite_match_range_count(text: &str, snippets: &[String]) -> usize {
    popup::bench_favorite_match_range_count(text, snippets)
}

pub fn bench_snapshot_clone(state: &AppState) -> usize {
    crate::perf::count_snapshot_clone(
        estimated_app_state_clone_bytes(state),
        estimated_app_state_clone_strings(state),
    );
    let cloned = state.clone();
    cloned.raw_payload.len()
        + cloned
            .today_menu
            .as_ref()
            .map(|menu| {
                menu.menus
                    .iter()
                    .map(|group| group.components.len())
                    .sum::<usize>()
            })
            .unwrap_or_default()
}

pub fn bench_split_component_suffix(input: &str) -> usize {
    let (main, suffix) = format::split_component_suffix(input);
    main.len() + suffix.len()
}

fn sample_settings() -> Settings {
    Settings {
        restaurant_code: "tietoteknia".to_string(),
        language: "fi".to_string(),
        refresh_minutes: 1440,
        show_prices: true,
        show_student_price: true,
        show_staff_price: true,
        show_guest_price: true,
        show_price_group_names: false,
        lunch_item_display_mode: LunchItemDisplayMode::Standard,
        theme: "dark".to_string(),
        show_restaurant_index_numbers: false,
        widget_scale: "normal".to_string(),
        show_allergens: true,
        highlight_gluten_free: true,
        highlight_veg: true,
        highlight_lactose_free: true,
        animations_enabled: true,
        enable_antell_restaurants: true,
        enable_logging: false,
        last_updated_epoch_ms: 0,
    }
}

fn sample_today_menu() -> TodayMenu {
    TodayMenu {
        date_iso: FIXTURE_DATE.to_string(),
        lunch_time: "10:30-14:00".to_string(),
        menus: vec![
            sample_group(
                "Lounasbuffet",
                vec![
                    sample_price("13.30", &[PriceAudience::Staff, PriceAudience::Guest]),
                    sample_price("3.10", &[PriceAudience::Student]),
                ],
                1000,
                &[
                    "Harkapapua tikka masala (G, L, M, Veg)",
                    "Paahdettuja siemenia (G, L, M, Veg)",
                ],
            ),
            sample_group(
                "Paaruoka",
                vec![
                    sample_price("13.30", &[PriceAudience::Staff, PriceAudience::Guest]),
                    sample_price("3.10", &[PriceAudience::Student]),
                ],
                2000,
                &[
                    "Makean hapanta tofu-kasviswokkia - Tofua Kung Pao (*, A, G, ILM, L, M, Veg, VS)",
                    "BBQ maustettua broileripastaa ja soijapapuja (*, A, L, M, VS)",
                    "Tummaa riisia (*, G, L, M, Veg)",
                ],
            ),
            sample_group(
                "Jalkiruoka",
                vec![sample_price(
                    "1.80",
                    &[PriceAudience::Student, PriceAudience::Staff, PriceAudience::Guest],
                )],
                3000,
                &[
                    "Aprikoosipiirakkaa (A, L, M)",
                    "Kinuskikermavaahtoa (A, G, L)",
                ],
            ),
        ],
    }
}

fn sample_group(
    name: &str,
    prices: Vec<MenuPrice>,
    first_recipe_id: u32,
    components: &[&str],
) -> MenuGroup {
    let details: Vec<Option<RecipeInfo>> = components
        .iter()
        .enumerate()
        .map(|(idx, component)| Some(sample_recipe(first_recipe_id + idx as u32, component)))
        .collect();
    let price = prices
        .iter()
        .map(|price| format!("{} e", price.amount.replace('.', ",")))
        .collect::<Vec<_>>()
        .join(" / ");
    MenuGroup {
        name: name.to_string(),
        price,
        prices,
        presentation: MenuGroupPresentation::Standard,
        components: components
            .iter()
            .map(|component| component.to_string())
            .collect(),
        component_recipe_ids: details
            .iter()
            .map(|detail| detail.as_ref().map(|recipe| recipe.recipe_id))
            .collect(),
        component_recipe_details: details,
    }
}

fn sample_price(amount: &str, audiences: &[PriceAudience]) -> MenuPrice {
    MenuPrice {
        amount: amount.to_string(),
        audiences: audiences.to_vec(),
    }
}

fn sample_recipe(recipe_id: u32, name: &str) -> RecipeInfo {
    RecipeInfo {
        recipe_id,
        name: name.to_string(),
        ingredients_cleaned: "vetta, tomaattisosetta, pastaa, broilerin paistisuikaleita, \
            soijapapuja, sipulia, paprikaa, rypsioljya, jodioitua suolaa, sokeria, \
            valkosipulia, muunnettua maissitarkkelysta, yrtteja, savustettua maltodekstriinia"
            .to_string(),
        nutritional_values: vec![
            NutritionalValue {
                name: "EnergyKcal".to_string(),
                amount: 142.0,
                unit: "kcal".to_string(),
            },
            NutritionalValue {
                name: "Protein".to_string(),
                amount: 7.4,
                unit: "g".to_string(),
            },
            NutritionalValue {
                name: "Carbohydrates".to_string(),
                amount: 19.5,
                unit: "g".to_string(),
            },
            NutritionalValue {
                name: "Fat".to_string(),
                amount: 3.6,
                unit: "g".to_string(),
            },
        ],
        kg_co2e_per100g: Some(0.25),
        diets: "G, L, M".to_string(),
    }
}

fn estimated_app_state_clone_bytes(state: &AppState) -> usize {
    let mut total = state.error_message.len()
        + state.restaurant_name.len()
        + state.restaurant_url.len()
        + state.raw_payload.len()
        + state.payload_date.len()
        + state.settings.restaurant_code.len()
        + state.settings.language.len()
        + state.settings.theme.len()
        + state.settings.widget_scale.len();
    if let Some(menu) = &state.today_menu {
        total += menu.date_iso.len() + menu.lunch_time.len();
        for group in &menu.menus {
            total += group.name.len() + group.price.len();
            total += group.components.iter().map(String::len).sum::<usize>();
            total += group
                .prices
                .iter()
                .map(|price| price.amount.len())
                .sum::<usize>();
            for detail in group.component_recipe_details.iter().flatten() {
                total += detail.name.len() + detail.ingredients_cleaned.len() + detail.diets.len();
                total += detail
                    .nutritional_values
                    .iter()
                    .map(|value| value.name.len() + value.unit.len())
                    .sum::<usize>();
            }
        }
    }
    total
}

fn estimated_app_state_clone_strings(state: &AppState) -> usize {
    let mut total = 9usize;
    if let Some(menu) = &state.today_menu {
        total += 2;
        for group in &menu.menus {
            total += 2 + group.components.len() + group.prices.len();
            for detail in group.component_recipe_details.iter().flatten() {
                total += 3 + detail.nutritional_values.len() * 2;
            }
        }
    }
    total
}

#[cfg(all(test, feature = "perf-counters"))]
mod tests {
    use super::*;

    #[test]
    fn perf_counter_smoke_over_bench_paths() {
        crate::perf::reset();
        for fixture in provider_fixtures() {
            let _ = parse_provider_fixture(&fixture);
        }
        let state = sample_app_state();
        let _ = bench_build_line_count(&state);
        let _ = bench_snapshot_clone(&state);
        let counters = crate::perf::snapshot();
        eprintln!("perf counter smoke: {counters:?}");
        assert!(counters.snapshot_cloned_bytes > 0);
        assert!(counters.snapshot_cloned_strings > 0);
    }
}
