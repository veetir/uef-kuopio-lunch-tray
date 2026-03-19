use anyhow::{anyhow, Context};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::cmp::Ordering;

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/veetir/uef-kuopio-lunch-tray/releases/latest";
const GITHUB_RELEASES_URL: &str = "https://github.com/veetir/uef-kuopio-lunch-tray/releases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckResult {
    LatestPublished {
        current_version: String,
        release_url: String,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        html_url: String,
    },
    NewerThanLatestPublished {
        current_version: String,
        latest_version: String,
        releases_url: String,
    },
}

#[derive(Debug, Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    html_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct AppVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

pub fn current_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn check_for_updates() -> anyhow::Result<UpdateCheckResult> {
    let current_version = current_app_version().to_string();
    let current = parse_version(current_app_version())
        .ok_or_else(|| anyhow!("Invalid current app version: {}", current_app_version()))?;
    let release = fetch_latest_release()?;
    let latest_version = normalize_release_version(&release.tag_name)
        .ok_or_else(|| anyhow!("Unsupported release tag format: {}", release.tag_name))?;
    let latest = parse_version(&latest_version)
        .ok_or_else(|| anyhow!("Invalid latest release version: {}", latest_version))?;

    Ok(classify_update_result(
        current_version,
        current,
        latest_version,
        latest,
        &release.html_url,
    ))
}

fn fetch_latest_release() -> anyhow::Result<GithubLatestRelease> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build GitHub client")?;
    let response = client
        .get(GITHUB_LATEST_RELEASE_URL)
        .header(ACCEPT, "application/vnd.github+json")
        .header(
            USER_AGENT,
            format!("compass-lunch-windows/{}", current_app_version()),
        )
        .send()
        .context("request latest GitHub release")?
        .error_for_status()
        .context("GitHub latest release request failed")?;
    let body = response.text().context("read GitHub release payload")?;
    parse_latest_release(&body)
}

fn parse_latest_release(json: &str) -> anyhow::Result<GithubLatestRelease> {
    serde_json::from_str(json).context("parse latest GitHub release JSON")
}

fn normalize_release_version(raw_tag: &str) -> Option<String> {
    let trimmed = raw_tag.trim();
    let trimmed = trimmed
        .strip_prefix("windows-")
        .or_else(|| trimmed.strip_prefix("Windows-"))
        .unwrap_or(trimmed);
    let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let parsed = parse_version(trimmed)?;
    Some(parsed.to_string())
}

fn parse_version(raw: &str) -> Option<AppVersion> {
    let mut parts = raw.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(AppVersion {
        major,
        minor,
        patch,
    })
}

impl AppVersion {
    fn to_string(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn classify_update_result(
    current_version: String,
    current: AppVersion,
    latest_version: String,
    latest: AppVersion,
    latest_release_url: &str,
) -> UpdateCheckResult {
    match latest.cmp(&current) {
        Ordering::Greater => UpdateCheckResult::UpdateAvailable {
            current_version,
            latest_version,
            html_url: latest_release_url.to_string(),
        },
        Ordering::Equal => UpdateCheckResult::LatestPublished {
            current_version,
            release_url: latest_release_url.to_string(),
        },
        Ordering::Less => UpdateCheckResult::NewerThanLatestPublished {
            current_version,
            latest_version,
            releases_url: GITHUB_RELEASES_URL.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_windows_release_tag() {
        assert_eq!(
            normalize_release_version("windows-v1.3.2"),
            Some("1.3.2".to_string())
        );
    }

    #[test]
    fn normalize_plain_v_release_tag() {
        assert_eq!(
            normalize_release_version("v1.3.2"),
            Some("1.3.2".to_string())
        );
    }

    #[test]
    fn reject_invalid_release_tag() {
        assert_eq!(normalize_release_version("windows-latest"), None);
    }

    #[test]
    fn compare_equal_versions() {
        assert_eq!(parse_version("1.3.2"), parse_version("1.3.2"));
    }

    #[test]
    fn compare_patch_versions() {
        assert!(parse_version("1.3.3") > parse_version("1.3.2"));
    }

    #[test]
    fn compare_minor_versions() {
        assert!(parse_version("1.4.0") > parse_version("1.3.9"));
    }

    #[test]
    fn compare_major_versions() {
        assert!(parse_version("2.0.0") > parse_version("1.9.9"));
    }

    #[test]
    fn parse_latest_release_payload() {
        let payload = r#"{
          "tag_name": "windows-v1.3.2",
          "html_url": "https://github.com/veetir/uef-kuopio-lunch-tray/releases/tag/windows-v1.3.2"
        }"#;
        let release = parse_latest_release(payload).expect("release payload");
        assert_eq!(release.tag_name, "windows-v1.3.2");
        assert_eq!(
            release.html_url,
            "https://github.com/veetir/uef-kuopio-lunch-tray/releases/tag/windows-v1.3.2"
        );
    }

    #[test]
    fn classify_equal_versions_as_latest_published() {
        let result = classify_update_result(
            "1.3.2".to_string(),
            parse_version("1.3.2").unwrap(),
            "1.3.2".to_string(),
            parse_version("1.3.2").unwrap(),
            "https://example.test/releases/tag/windows-v1.3.2",
        );
        assert_eq!(
            result,
            UpdateCheckResult::LatestPublished {
                current_version: "1.3.2".to_string(),
                release_url: "https://example.test/releases/tag/windows-v1.3.2".to_string(),
            }
        );
    }

    #[test]
    fn classify_newer_local_version_as_newer_than_latest() {
        let result = classify_update_result(
            "1.3.3".to_string(),
            parse_version("1.3.3").unwrap(),
            "1.3.2".to_string(),
            parse_version("1.3.2").unwrap(),
            "https://example.test/releases/tag/windows-v1.3.2",
        );
        assert_eq!(
            result,
            UpdateCheckResult::NewerThanLatestPublished {
                current_version: "1.3.3".to_string(),
                latest_version: "1.3.2".to_string(),
                releases_url: GITHUB_RELEASES_URL.to_string(),
            }
        );
    }
}
