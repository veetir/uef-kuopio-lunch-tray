//! Shared data models used across provider parsing and popup rendering.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
/// Raw Compass JSON payload root.
pub struct ApiResponse {
    #[serde(rename = "RestaurantName")]
    pub restaurant_name: Option<String>,
    #[serde(rename = "RestaurantUrl")]
    pub restaurant_url: Option<String>,
    #[serde(rename = "MenusForDays")]
    pub menus_for_days: Option<Vec<ApiMenuDay>>,
    #[serde(rename = "ErrorText")]
    pub error_text: Option<String>,
}

#[derive(Debug, Deserialize)]
/// Raw Compass JSON payload for a single menu day.
pub struct ApiMenuDay {
    #[serde(rename = "Date")]
    pub date: Option<String>,
    #[serde(rename = "LunchTime")]
    pub lunch_time: Option<String>,
    #[serde(rename = "SetMenus")]
    pub set_menus: Option<Vec<ApiSetMenu>>,
}

#[derive(Debug, Deserialize)]
/// Raw Compass JSON payload for a single named menu section.
pub struct ApiSetMenu {
    #[serde(rename = "SortOrder")]
    pub sort_order: Option<i32>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Price")]
    pub price: Option<String>,
    #[serde(rename = "Components")]
    pub components: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
/// Normalized menu content for the current day.
pub struct TodayMenu {
    pub date_iso: String,
    pub lunch_time: String,
    pub menus: Vec<MenuGroup>,
}

#[derive(Debug, Clone)]
/// A rendered menu section containing a heading, optional price, and component lines.
pub struct MenuGroup {
    pub name: String,
    pub price: String,
    pub components: Vec<String>,
}
