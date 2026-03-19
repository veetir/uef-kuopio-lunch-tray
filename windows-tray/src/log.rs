//! Minimal file-based diagnostic logging.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_ENABLED: AtomicBool = AtomicBool::new(false);

fn log_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base)
        .join("compass-lunch")
        .join("compass-lunch.log")
}

/// Enables or disables diagnostic logging globally for the current process.
pub fn set_enabled(enabled: bool) {
    LOG_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Appends a single timestamped log line when logging is enabled.
pub fn log_line(message: &str) {
    if !LOG_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] {}", ts, message);
    }
}
