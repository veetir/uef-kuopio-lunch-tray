use crate::format::normalize_text;
use crate::settings::settings_dir;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct FavoritesList {
    pub snippets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FavoritesFile {
    #[serde(default)]
    favorite_snippets: Vec<String>,
}

pub fn favorites_path() -> PathBuf {
    settings_dir().join("favorites.json")
}

pub fn favorites_mtime_ms() -> Option<i64> {
    let metadata = fs::metadata(favorites_path()).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as i64)
}

pub fn normalize_snippet(value: &str) -> String {
    normalize_text(value)
}

pub fn load_favorites() -> FavoritesList {
    let path = favorites_path();
    let data = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return FavoritesList::default(),
    };

    let parsed: FavoritesFile = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(_) => return FavoritesList::default(),
    };

    FavoritesList {
        snippets: dedupe_normalized(parsed.favorite_snippets),
    }
}

pub fn toggle_snippet(value: &str) -> anyhow::Result<bool> {
    let normalized = normalize_snippet(value);
    if normalized.is_empty() {
        return Ok(false);
    }

    let list = load_favorites().snippets;
    let key = normalized.to_lowercase();
    if list.iter().any(|entry| entry.to_lowercase() == key) {
        let filtered = remove_variant_family(list, &normalized);
        save_favorites(&filtered)?;
        Ok(false)
    } else {
        let mut list = list;
        list.push(normalized);
        let deduped = dedupe_normalized(list);
        save_favorites(&deduped)?;
        Ok(true)
    }
}

fn save_favorites(snippets: &[String]) -> anyhow::Result<()> {
    let dir = settings_dir();
    fs::create_dir_all(&dir).context("create settings dir for favorites")?;
    let payload = FavoritesFile {
        favorite_snippets: snippets.to_vec(),
    };
    let data = serde_json::to_string_pretty(&payload)?;
    let path = favorites_path();
    fs::write(&path, data).with_context(|| format!("write favorites file {}", path.display()))?;
    Ok(())
}

fn dedupe_normalized(items: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        let normalized = normalize_snippet(&item);
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_lowercase();
        if seen.iter().any(|entry| entry == &key) {
            continue;
        }
        seen.push(key);
        out.push(normalized);
    }
    out
}

fn remove_variant_family(list: Vec<String>, selected: &str) -> Vec<String> {
    let selected = normalize_snippet(selected);
    if selected.is_empty() {
        return dedupe_normalized(list);
    }
    let selected_lower = selected.to_lowercase();

    let mut kept = Vec::new();
    for entry in list {
        let normalized = normalize_snippet(&entry);
        if normalized.is_empty() {
            continue;
        }
        let entry_lower = normalized.to_lowercase();
        if is_variant_family(&entry_lower, &selected_lower) {
            continue;
        }
        kept.push(normalized);
    }
    dedupe_normalized(kept)
}

fn is_variant_family(a_lower: &str, b_lower: &str) -> bool {
    if a_lower == b_lower {
        return true;
    }

    let (longer, shorter) = if a_lower.len() >= b_lower.len() {
        (a_lower, b_lower)
    } else {
        (b_lower, a_lower)
    };
    if shorter.is_empty() || !longer.contains(shorter) {
        return false;
    }

    contains_at_word_boundary(longer, shorter) || is_single_edge_trim_variant(longer, shorter)
}

fn contains_at_word_boundary(longer: &str, shorter: &str) -> bool {
    if shorter.is_empty() || longer.is_empty() {
        return false;
    }
    longer.match_indices(shorter).any(|(start, _)| {
        let end = start + shorter.len();
        is_word_boundary(longer, start, end)
    })
}

fn is_single_edge_trim_variant(longer: &str, shorter: &str) -> bool {
    if longer.len() != shorter.len() + 1 {
        return false;
    }
    longer.starts_with(shorter) || longer.ends_with(shorter)
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let prev_ok = text[..start]
        .chars()
        .next_back()
        .map(|ch| !ch.is_alphanumeric())
        .unwrap_or(true);
    let next_ok = text[end..]
        .chars()
        .next()
        .map(|ch| !ch.is_alphanumeric())
        .unwrap_or(true);
    prev_ok && next_ok
}

#[cfg(test)]
mod tests {
    use super::{is_variant_family, remove_variant_family};

    #[test]
    fn removes_near_miss_single_char_variant() {
        let input = vec![
            "lasagn".to_string(),
            "lasagne".to_string(),
            "pizza".to_string(),
        ];
        let out = remove_variant_family(input, "lasagne");
        assert_eq!(out, vec!["pizza".to_string()]);
    }

    #[test]
    fn removes_boundary_contained_variants() {
        let input = vec![
            "tofu".to_string(),
            "tofu teriyaki".to_string(),
            "spicy tofu".to_string(),
            "soup".to_string(),
        ];
        let out = remove_variant_family(input, "tofu");
        assert_eq!(out, vec!["soup".to_string()]);
    }

    #[test]
    fn keeps_unrelated_inner_substrings() {
        let input = vec![
            "ham".to_string(),
            "hamburger".to_string(),
            "soup".to_string(),
        ];
        let out = remove_variant_family(input, "ham");
        assert_eq!(out, vec!["hamburger".to_string(), "soup".to_string()]);
    }

    #[test]
    fn family_rule_matches_case_insensitively() {
        assert!(is_variant_family(
            "lasagne",
            "LASAGN".to_lowercase().as_str()
        ));
        assert!(is_variant_family("tofu teriyaki", "tofu"));
        assert!(!is_variant_family("hamburger", "ham"));
    }
}
