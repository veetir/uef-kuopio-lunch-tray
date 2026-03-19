//! Disk cache helpers for provider payloads.

use crate::restaurant::{provider_key, Provider};
use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the cache directory used for fetched provider payloads.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).join("compass-lunch").join("cache")
}

/// Returns the cache file path for a provider, restaurant, and language combination.
pub fn cache_path(provider: Provider, code: &str, language: &str) -> PathBuf {
    cache_dir().join(cache_filename(provider, code, language))
}

fn cache_filename(provider: Provider, code: &str, language: &str) -> String {
    let ext = match provider {
        Provider::Compass => "json",
        Provider::CompassRss => "xml",
        Provider::Antell => "html",
        Provider::HuomenJson => "json",
        Provider::PranzeriaHtml => "html",
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
        Provider::Compass => "json",
        Provider::CompassRss => "xml",
        Provider::Antell => "html",
        Provider::HuomenJson => "json",
        Provider::PranzeriaHtml => "html",
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
        Ok(data) => Some(data),
        Err(_) => {
            let legacy_path = legacy_cache_path(provider, code, language);
            fs::read_to_string(legacy_path).ok()
        }
    }
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

/// Writes a provider payload to the normalized cache location.
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
