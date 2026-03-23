//! User-defined custom themes loaded from a `themes.json` file in the app data
//! directory.  When the file is missing the app regenerates it with a single
//! example theme so users have a starting point to copy and modify.

use crate::settings::settings_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use std::sync::OnceLock;
use windows::Win32::Foundation::COLORREF;

static CUSTOM_THEMES: OnceLock<Mutex<Vec<CustomThemeDef>>> = OnceLock::new();

/// JSON-level representation of one custom theme (hex color strings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemeEntry {
    pub name: String,
    pub bg_color: String,
    pub body_text_color: String,
    pub heading_color: String,
    pub header_title_color: String,
    pub suffix_color: String,
    pub suffix_highlight_color: String,
    pub favorite_highlight_color: String,
    pub selection_bg_color: String,
    pub header_bg_color: String,
    pub button_bg_color: String,
    pub divider_color: String,
}

/// Parsed custom theme with COLORREF values ready for rendering.
#[derive(Debug, Clone)]
pub struct CustomThemeDef {
    pub name: String,
    pub bg_color: COLORREF,
    pub body_text_color: COLORREF,
    pub heading_color: COLORREF,
    pub header_title_color: COLORREF,
    pub suffix_color: COLORREF,
    pub suffix_highlight_color: COLORREF,
    pub favorite_highlight_color: COLORREF,
    pub selection_bg_color: COLORREF,
    pub header_bg_color: COLORREF,
    pub button_bg_color: COLORREF,
    pub divider_color: COLORREF,
}

fn parse_hex_color(hex: &str) -> Option<COLORREF> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(COLORREF(
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
    ))
}

fn parse_entry(entry: &CustomThemeEntry) -> Option<CustomThemeDef> {
    Some(CustomThemeDef {
        name: entry.name.clone(),
        bg_color: parse_hex_color(&entry.bg_color)?,
        body_text_color: parse_hex_color(&entry.body_text_color)?,
        heading_color: parse_hex_color(&entry.heading_color)?,
        header_title_color: parse_hex_color(&entry.header_title_color)?,
        suffix_color: parse_hex_color(&entry.suffix_color)?,
        suffix_highlight_color: parse_hex_color(&entry.suffix_highlight_color)?,
        favorite_highlight_color: parse_hex_color(&entry.favorite_highlight_color)?,
        selection_bg_color: parse_hex_color(&entry.selection_bg_color)?,
        header_bg_color: parse_hex_color(&entry.header_bg_color)?,
        button_bg_color: parse_hex_color(&entry.button_bg_color)?,
        divider_color: parse_hex_color(&entry.divider_color)?,
    })
}

fn themes_path() -> std::path::PathBuf {
    settings_dir().join("themes.json")
}

fn default_themes_json() -> Vec<CustomThemeEntry> {
    vec![CustomThemeEntry {
        name: "Custom1".to_string(),
        bg_color: "#1a1a2e".to_string(),
        body_text_color: "#e0e0e0".to_string(),
        heading_color: "#e94560".to_string(),
        header_title_color: "#ffffff".to_string(),
        suffix_color: "#7a7a9e".to_string(),
        suffix_highlight_color: "#e94560".to_string(),
        favorite_highlight_color: "#f5c518".to_string(),
        selection_bg_color: "#16213e".to_string(),
        header_bg_color: "#0f3460".to_string(),
        button_bg_color: "#16213e".to_string(),
        divider_color: "#533483".to_string(),
    }]
}

fn load_from_disk() -> Vec<CustomThemeDef> {
    let path = themes_path();
    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(_) => {
            let defaults = default_themes_json();
            if let Ok(json) = serde_json::to_string_pretty(&defaults) {
                let _ = fs::create_dir_all(settings_dir());
                let _ = fs::write(&path, json);
            }
            match serde_json::to_string(&defaults) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            }
        }
    };
    let entries: Vec<CustomThemeEntry> = match serde_json::from_str(&data) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries.iter().filter_map(|e| parse_entry(e)).collect()
}

fn get_cache() -> &'static Mutex<Vec<CustomThemeDef>> {
    CUSTOM_THEMES.get_or_init(|| Mutex::new(load_from_disk()))
}

/// Returns the currently cached list of custom themes.
pub fn custom_themes() -> Vec<CustomThemeDef> {
    get_cache().lock().unwrap().clone()
}

/// Reloads custom themes from disk, picking up any user edits to `themes.json`.
pub fn reload_custom_themes() {
    let themes = load_from_disk();
    *get_cache().lock().unwrap() = themes;
}

/// Looks up a custom theme by name (case-insensitive).
pub fn find_custom_theme(name: &str) -> Option<CustomThemeDef> {
    let lower = name.to_ascii_lowercase();
    custom_themes()
        .into_iter()
        .find(|t| t.name.to_ascii_lowercase() == lower)
}

