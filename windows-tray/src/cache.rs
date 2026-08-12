//! Disk cache helpers for lunch API responses.

use crate::restaurant::{provider_key, Provider};
use anyhow::Context;
use std::fs;
use std::path::PathBuf;

/// Returns the cache directory used for fetched API responses.
pub fn cache_dir() -> PathBuf {
    crate::settings::settings_dir().join("cache")
}

/// Returns the cache file path for a provider, restaurant, and language combination.
pub fn cache_path(provider: Provider, code: &str, language: &str) -> PathBuf {
    cache_dir().join(cache_filename(provider, code, language))
}

/// Returns the marker file path used by local mock-cache testing.
pub fn mock_cache_mode_path() -> PathBuf {
    cache_dir().join("mock-cache-mode")
}

/// Returns true when local mock-cache testing should suppress network refreshes.
pub fn mock_cache_mode_enabled() -> bool {
    mock_cache_mode_path().is_file()
}

fn cache_filename(provider: Provider, code: &str, language: &str) -> String {
    let ext = match provider {
        Provider::LunchApi => "json",
        Provider::Compass => "json",
    };
    format!(
        "{}__{}__{}.{}",
        sanitize_key_segment(provider_key(provider)),
        sanitize_key_segment(code),
        sanitize_key_segment(language),
        ext
    )
}

fn legacy_cache_path(provider: Provider, code: &str, language: &str) -> PathBuf {
    let ext = match provider {
        Provider::LunchApi => "json",
        Provider::Compass => "json",
    };
    let filename = format!("{}|{}|{}.{}", provider_key(provider), code, language, ext);
    cache_dir().join(filename)
}

fn sanitize_key_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Reads a cached payload from the current or legacy cache filename format.
pub fn read_cache(provider: Provider, code: &str, language: &str) -> Option<String> {
    let path = cache_path(provider, code, language);
    match fs::read_to_string(&path) {
        Ok(data) => Some(strip_json_bom(data)),
        Err(_) => {
            let legacy_path = legacy_cache_path(provider, code, language);
            fs::read_to_string(legacy_path).ok().map(strip_json_bom)
        }
    }
}

fn strip_json_bom(data: String) -> String {
    data.strip_prefix('\u{feff}').unwrap_or(&data).to_string()
}

/// Returns the cache modification time in epoch milliseconds, if available.
pub fn cache_mtime_ms(provider: Provider, code: &str, language: &str) -> Option<i64> {
    let path = cache_path(provider, code, language);
    let metadata = fs::metadata(&path)
        .or_else(|_| fs::metadata(legacy_cache_path(provider, code, language)))
        .ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as i64)
}

/// Writes an API response to the cache.
pub fn write_cache(
    provider: Provider,
    code: &str,
    language: &str,
    payload: &str,
) -> anyhow::Result<()> {
    let dir = cache_dir();
    fs::create_dir_all(&dir).context("create cache dir")?;
    let path = cache_path(provider, code, language);
    fs::write(&path, payload).with_context(|| format!("write cache file {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::strip_json_bom;

    #[test]
    fn strips_utf8_bom_from_cached_json() {
        assert_eq!(
            strip_json_bom("\u{feff}{\"ok\":true}".to_string()),
            "{\"ok\":true}"
        );
        assert_eq!(strip_json_bom("{\"ok\":true}".to_string()), "{\"ok\":true}");
    }
}
