use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub restaurant_code: String,
    pub language: String,
    pub refresh_minutes: u32,
    pub show_prices: bool,
    pub show_student_price: bool,
    pub show_staff_price: bool,
    pub show_guest_price: bool,
    pub hide_expensive_student_meals: bool,
    pub theme: String,
    pub widget_scale: String,
    pub show_allergens: bool,
    pub highlight_gluten_free: bool,
    pub highlight_veg: bool,
    pub highlight_lactose_free: bool,
    pub enable_antell_restaurants: bool,
    pub enable_logging: bool,
    pub last_updated_epoch_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            restaurant_code: "0437".to_string(),
            language: "fi".to_string(),
            refresh_minutes: 1440,
            show_prices: false,
            show_student_price: true,
            show_staff_price: true,
            show_guest_price: false,
            hide_expensive_student_meals: false,
            theme: "dark".to_string(),
            widget_scale: "normal".to_string(),
            show_allergens: true,
            highlight_gluten_free: false,
            highlight_veg: false,
            highlight_lactose_free: false,
            enable_antell_restaurants: true,
            enable_logging: false,
            last_updated_epoch_ms: 0,
        }
    }
}

pub fn settings_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).join("compass-lunch")
}

pub fn settings_path() -> PathBuf {
    settings_dir().join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(data) => decode_settings(&data).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> anyhow::Result<()> {
    let dir = settings_dir();
    fs::create_dir_all(&dir)?;
    let data = serde_json::to_string_pretty(settings)?;
    fs::write(dir.join("settings.json"), data)?;
    Ok(())
}

#[derive(Default, Deserialize)]
struct RawSettings {
    restaurant_code: Option<String>,
    language: Option<String>,
    refresh_minutes: Option<u32>,
    show_prices: Option<bool>,
    show_student_price: Option<bool>,
    show_staff_price: Option<bool>,
    show_guest_price: Option<bool>,
    hide_expensive_student_meals: Option<bool>,
    theme: Option<String>,
    widget_scale: Option<String>,
    dark_mode: Option<bool>,
    show_allergens: Option<bool>,
    hide_allergens: Option<bool>,
    highlight_gluten_free: Option<bool>,
    highlight_veg: Option<bool>,
    highlight_lactose_free: Option<bool>,
    enable_logging: Option<bool>,
    last_updated_epoch_ms: Option<i64>,
}

fn decode_settings(data: &str) -> anyhow::Result<Settings> {
    let raw: RawSettings = serde_json::from_str(data)?;
    let defaults = Settings::default();
    let show_allergens = raw.show_allergens.unwrap_or_else(|| {
        raw.hide_allergens
            .map(|hide| !hide)
            .unwrap_or(defaults.show_allergens)
    });

    let theme = raw
        .theme
        .as_deref()
        .map(normalize_theme)
        .or_else(|| {
            raw.dark_mode.map(|dark| {
                if dark {
                    "dark".to_string()
                } else {
                    "light".to_string()
                }
            })
        })
        .unwrap_or_else(|| defaults.theme.clone());
    let widget_scale = raw
        .widget_scale
        .as_deref()
        .map(normalize_widget_scale)
        .unwrap_or_else(|| defaults.widget_scale.clone());

    Ok(Settings {
        restaurant_code: raw.restaurant_code.unwrap_or(defaults.restaurant_code),
        language: raw.language.unwrap_or(defaults.language),
        refresh_minutes: raw.refresh_minutes.unwrap_or(defaults.refresh_minutes),
        show_prices: raw.show_prices.unwrap_or(defaults.show_prices),
        show_student_price: raw
            .show_student_price
            .unwrap_or(defaults.show_student_price),
        show_staff_price: raw.show_staff_price.unwrap_or(defaults.show_staff_price),
        show_guest_price: raw.show_guest_price.unwrap_or(defaults.show_guest_price),
        hide_expensive_student_meals: raw
            .hide_expensive_student_meals
            .unwrap_or(defaults.hide_expensive_student_meals),
        theme,
        widget_scale,
        show_allergens,
        highlight_gluten_free: raw
            .highlight_gluten_free
            .unwrap_or(defaults.highlight_gluten_free),
        highlight_veg: raw.highlight_veg.unwrap_or(defaults.highlight_veg),
        highlight_lactose_free: raw
            .highlight_lactose_free
            .unwrap_or(defaults.highlight_lactose_free),
        // Antell is always enabled; keep the field for backward-compatible settings serialization.
        enable_antell_restaurants: true,
        enable_logging: raw.enable_logging.unwrap_or(defaults.enable_logging),
        last_updated_epoch_ms: raw
            .last_updated_epoch_ms
            .unwrap_or(defaults.last_updated_epoch_ms),
    })
}

pub fn normalize_theme(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "dark" => "dark".to_string(),
        "blue" => "blue".to_string(),
        "green" => "green".to_string(),
        "teletext1" => "teletext1".to_string(),
        "teletext2" => "teletext2".to_string(),
        _ => "dark".to_string(),
    }
}

pub fn normalize_widget_scale(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "normal" | "100" | "100%" => "normal".to_string(),
        "125" | "125%" => "125".to_string(),
        "150" | "150%" => "150".to_string(),
        _ => "normal".to_string(),
    }
}
