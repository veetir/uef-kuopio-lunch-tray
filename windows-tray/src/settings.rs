//! Persistent user settings stored under the app data directory.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LunchItemDisplayMode {
    Classic,
    Standard,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Persisted settings that drive fetch behavior and popup rendering.
pub struct Settings {
    pub restaurant_code: String,
    pub language: String,
    pub refresh_minutes: u32,
    pub show_prices: bool,
    pub show_student_price: bool,
    pub show_staff_price: bool,
    pub show_guest_price: bool,
    pub show_price_group_names: bool,
    pub lunch_item_display_mode: LunchItemDisplayMode,
    pub theme: String,
    pub show_restaurant_index_numbers: bool,
    pub widget_scale: String,
    pub show_allergens: bool,
    pub highlight_gluten_free: bool,
    pub highlight_veg: bool,
    pub highlight_lactose_free: bool,
    pub animations_enabled: bool,
    pub enable_antell_restaurants: bool,
    pub enable_logging: bool,
    pub last_updated_epoch_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            restaurant_code: "snellmania".to_string(),
            language: "fi".to_string(),
            refresh_minutes: 1440,
            show_prices: true,
            show_student_price: true,
            show_staff_price: true,
            show_guest_price: false,
            show_price_group_names: false,
            lunch_item_display_mode: LunchItemDisplayMode::Classic,
            theme: "dark".to_string(),
            show_restaurant_index_numbers: false,
            widget_scale: "normal".to_string(),
            show_allergens: true,
            highlight_gluten_free: false,
            highlight_veg: false,
            highlight_lactose_free: false,
            animations_enabled: true,
            enable_antell_restaurants: true,
            enable_logging: false,
            last_updated_epoch_ms: 0,
        }
    }
}

/// Directory name used for app data through release 1.4.2, when the app was
/// still called Compass Lunch. Kept only so [`migrate_legacy_data_dir`] can find
/// an existing install's settings, favorites, and custom themes.
const LEGACY_DATA_DIR: &str = "compass-lunch";
const DATA_DIR: &str = "LunchTray";

/// Returns the directory used for settings, logs, favorites, and related app data.
pub fn settings_dir() -> PathBuf {
    data_root().join(DATA_DIR)
}

fn data_root() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).to_path_buf()
}

/// Moves a pre-1.4.3 `compass-lunch` data directory to the current name.
///
/// Called once at startup, after the single-instance guard is held so no other
/// instance has the files open. A plain rename is tried first; if the directory
/// cannot be moved (locked by an indexer, or spanning a junction) the individual
/// payload files are copied instead, so a user never silently loses their
/// favorites or custom themes. Doing nothing is always an acceptable outcome
/// here — the app falls back to defaults — so every failure is swallowed.
pub fn migrate_legacy_data_dir() {
    let current = settings_dir();
    if current.exists() {
        return;
    }
    let legacy = data_root().join(LEGACY_DATA_DIR);
    if !legacy.is_dir() {
        return;
    }
    if fs::rename(&legacy, &current).is_ok() {
        return;
    }
    if fs::create_dir_all(&current).is_err() {
        return;
    }
    for name in ["settings.json", "favorites.json", "themes.json"] {
        let from = legacy.join(name);
        if from.is_file() {
            let _ = fs::copy(&from, current.join(name));
        }
    }
}

/// Returns the full path of the persisted settings JSON file.
pub fn settings_path() -> PathBuf {
    settings_dir().join("settings.json")
}

/// Loads settings from disk and falls back to defaults on missing or invalid data.
pub fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(data) => decode_settings(&data).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Saves the provided settings snapshot to disk.
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
    show_price_group_names: Option<bool>,
    lunch_item_display_mode: Option<String>,
    theme: Option<String>,
    show_restaurant_index_numbers: Option<bool>,
    widget_scale: Option<String>,
    dark_mode: Option<bool>,
    show_allergens: Option<bool>,
    hide_allergens: Option<bool>,
    highlight_gluten_free: Option<bool>,
    highlight_veg: Option<bool>,
    highlight_lactose_free: Option<bool>,
    animations_enabled: Option<bool>,
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
        restaurant_code: crate::restaurant::permanent_restaurant_id(
            raw.restaurant_code
                .as_deref()
                .unwrap_or(&defaults.restaurant_code),
        )
        .to_string(),
        language: raw.language.unwrap_or(defaults.language),
        refresh_minutes: raw.refresh_minutes.unwrap_or(defaults.refresh_minutes),
        // Existing installs without this key predate the price-first layout.
        // Keep their old hidden-price behavior; brand-new installs use the
        // current default from Settings::default().
        show_prices: raw.show_prices.unwrap_or(false),
        show_student_price: raw
            .show_student_price
            .unwrap_or(defaults.show_student_price),
        show_staff_price: raw.show_staff_price.unwrap_or(defaults.show_staff_price),
        show_guest_price: raw.show_guest_price.unwrap_or(defaults.show_guest_price),
        show_price_group_names: raw
            .show_price_group_names
            .unwrap_or(defaults.show_price_group_names),
        // Existing installs have no saved display mode. Keep their current layout
        // instead of switching the menu structure during upgrade.
        lunch_item_display_mode: raw
            .lunch_item_display_mode
            .as_deref()
            .map(normalize_lunch_item_display_mode)
            .unwrap_or(LunchItemDisplayMode::Classic),
        theme,
        show_restaurant_index_numbers: raw
            .show_restaurant_index_numbers
            .unwrap_or(defaults.show_restaurant_index_numbers),
        widget_scale,
        show_allergens,
        highlight_gluten_free: raw
            .highlight_gluten_free
            .unwrap_or(defaults.highlight_gluten_free),
        highlight_veg: raw.highlight_veg.unwrap_or(defaults.highlight_veg),
        highlight_lactose_free: raw
            .highlight_lactose_free
            .unwrap_or(defaults.highlight_lactose_free),
        animations_enabled: raw
            .animations_enabled
            .unwrap_or(defaults.animations_enabled),
        // Antell is always enabled; keep the field for backward-compatible settings serialization.
        enable_antell_restaurants: true,
        enable_logging: raw.enable_logging.unwrap_or(defaults.enable_logging),
        last_updated_epoch_ms: raw
            .last_updated_epoch_ms
            .unwrap_or(defaults.last_updated_epoch_ms),
    })
}

/// Normalizes user-facing theme values to the supported internal theme keys.
///
/// Built-in themes are returned as their canonical lowercase key.  Custom
/// themes defined in `themes.json` are returned using the canonical name from
/// the file so that menu check-marks match correctly.
pub fn normalize_theme(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "dark" => "dark".to_string(),
        "grandpa" | "windows 95" | "win95" => "grandpa".to_string(),
        "grandma" => "grandma".to_string(),
        "blue" => "blue".to_string(),
        "green" => "green".to_string(),
        "amber" => "amber".to_string(),
        // Barbie was retired in favour of Grandma, which covers the same hues
        // with a referent the other themes share. Kept as an alias so existing
        // `settings.json` files do not fall through to the unknown-theme
        // default and silently land on Dark.
        "barbie" => "grandma".to_string(),
        "teletext1" => "teletext1".to_string(),
        "teletext2" => "teletext2".to_string(),
        _ => {
            if let Some(custom) = crate::custom_themes::find_custom_theme(value) {
                custom.name
            } else {
                "dark".to_string()
            }
        }
    }
}

/// Normalizes user-facing scale values to the supported scale presets.
pub fn normalize_widget_scale(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "small" | "100" | "100%" => "small".to_string(),
        "normal" | "125" | "125%" => "normal".to_string(),
        "large" | "150" | "150%" => "large".to_string(),
        _ => "normal".to_string(),
    }
}

/// Normalizes user-facing lunch item display mode values to the supported presets.
pub fn normalize_lunch_item_display_mode(value: &str) -> LunchItemDisplayMode {
    match value.to_ascii_lowercase().as_str() {
        "classic" | "legacy" => LunchItemDisplayMode::Classic,
        "compact" => LunchItemDisplayMode::Compact,
        "standard" => LunchItemDisplayMode::Standard,
        _ => LunchItemDisplayMode::Classic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_default_uses_classic_layout_with_all_prices() {
        let settings = Settings::default();

        assert_eq!(
            settings.lunch_item_display_mode,
            LunchItemDisplayMode::Classic
        );
        assert!(settings.show_prices);
        assert!(settings.show_student_price);
        assert!(settings.show_staff_price);
        // Guest is off out of the box: an outsider to UEF is very unlikely to be
        // installing this, and the third price group is what pushes the price
        // string wide enough to crowd the layout.
        assert!(!settings.show_guest_price);
        assert!(!settings.show_price_group_names);
        assert!(!settings.show_restaurant_index_numbers);
        assert!(settings.show_allergens);
    }

    #[test]
    fn missing_lunch_item_display_mode_keeps_existing_settings_on_classic() {
        let settings = decode_settings(r#"{"language":"en","theme":"blue"}"#).unwrap();

        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme, "blue");
        assert!(!settings.show_prices);
        assert_eq!(
            settings.lunch_item_display_mode,
            LunchItemDisplayMode::Classic
        );
    }

    #[test]
    fn lunch_item_display_mode_decodes_supported_values() {
        let settings = decode_settings(r#"{"lunch_item_display_mode":"classic"}"#).unwrap();

        assert_eq!(
            settings.lunch_item_display_mode,
            LunchItemDisplayMode::Classic
        );

        let settings = decode_settings(r#"{"lunch_item_display_mode":"legacy"}"#).unwrap();

        assert_eq!(
            settings.lunch_item_display_mode,
            LunchItemDisplayMode::Classic
        );

        let settings = decode_settings(r#"{"lunch_item_display_mode":"compact"}"#).unwrap();

        assert_eq!(
            settings.lunch_item_display_mode,
            LunchItemDisplayMode::Compact
        );
    }

    #[test]
    fn widget_scale_decodes_named_and_legacy_percentage_values() {
        let settings = decode_settings(r#"{"widget_scale":"small"}"#).unwrap();
        assert_eq!(settings.widget_scale, "small");

        let settings = decode_settings(r#"{"widget_scale":"normal"}"#).unwrap();
        assert_eq!(settings.widget_scale, "normal");

        let settings = decode_settings(r#"{"widget_scale":"large"}"#).unwrap();
        assert_eq!(settings.widget_scale, "large");

        let settings = decode_settings(r#"{"widget_scale":"100%"}"#).unwrap();
        assert_eq!(settings.widget_scale, "small");

        let settings = decode_settings(r#"{"widget_scale":"125"}"#).unwrap();
        assert_eq!(settings.widget_scale, "normal");

        let settings = decode_settings(r#"{"widget_scale":"150"}"#).unwrap();
        assert_eq!(settings.widget_scale, "large");
    }

    #[test]
    fn grandma_theme_decodes() {
        let settings = decode_settings(r#"{"theme":"Grandma"}"#).unwrap();
        assert_eq!(settings.theme, "grandma");
    }

    #[test]
    fn retired_barbie_theme_migrates_to_grandma() {
        let settings = decode_settings(r#"{"theme":"barbie"}"#).unwrap();
        assert_eq!(settings.theme, "grandma");
    }

    #[test]
    fn grandpa_theme_decodes_supported_aliases() {
        let settings = decode_settings(r#"{"theme":"grandpa"}"#).unwrap();
        assert_eq!(settings.theme, "grandpa");

        let settings = decode_settings(r#"{"theme":"Windows 95"}"#).unwrap();
        assert_eq!(settings.theme, "grandpa");
    }

    #[test]
    fn legacy_restaurant_code_migrates_to_public_api_id() {
        let settings = decode_settings(r#"{"restaurant_code":"0439"}"#).unwrap();
        assert_eq!(settings.restaurant_code, "tietoteknia");

        let settings = decode_settings(r#"{"restaurant_code":"pranzeria-html"}"#).unwrap();
        assert_eq!(settings.restaurant_code, "pranzeria-sorrento");
    }
}
