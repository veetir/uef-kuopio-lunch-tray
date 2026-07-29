//! Shared data models used across provider parsing and popup rendering.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Normalized menu content for the current day.
pub struct TodayMenu {
    pub date_iso: String,
    pub lunch_time: String,
    pub menus: Vec<MenuGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A rendered menu section containing a heading, optional price, and component lines.
pub struct MenuGroup {
    pub name: String,
    pub price: String,
    #[serde(default)]
    pub prices: Vec<MenuPrice>,
    #[serde(default)]
    pub presentation: MenuGroupPresentation,
    pub components: Vec<String>,
    pub component_recipe_ids: Vec<Option<u32>>,
    pub component_recipe_details: Vec<Option<RecipeInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuPrice {
    pub amount: String,
    #[serde(default)]
    pub audiences: Vec<PriceAudience>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriceAudience {
    Student,
    Staff,
    Guest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuGroupPresentation {
    #[default]
    Standard,
    GeneralOffer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Extra Compass recipe information shown when a menu component is expanded.
pub struct RecipeInfo {
    pub recipe_id: u32,
    #[allow(dead_code)]
    pub name: String,
    #[serde(default)]
    pub ingredients_cleaned: String,
    #[serde(default)]
    pub nutritional_values: Vec<NutritionalValue>,
    #[serde(default)]
    #[serde(rename = "kgCO2ePer100g")]
    pub kg_co2e_per100g: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    pub diets: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// One nutrition value from the Compass recipe endpoint.
pub struct NutritionalValue {
    pub name: String,
    pub amount: f32,
    pub unit: String,
}
