use super::*;

impl App {
    /// Starts an asynchronous update check if one is not already in flight.
    pub fn start_update_check(&self) -> bool {
        {
            let mut in_flight = self.update_check_in_flight.lock().unwrap();
            if *in_flight {
                log_line("update check skipped: already in flight");
                return false;
            }
            *in_flight = true;
        }

        let hwnd = self.hwnd_tray();
        let in_flight = Arc::clone(&self.update_check_in_flight);
        log_line(&format!(
            "update check start current_version={}",
            update::current_app_version()
        ));
        std::thread::spawn(move || {
            let outcome = match update::check_for_updates() {
                Ok(update::UpdateCheckResult::LatestPublished {
                    current_version,
                    release_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=latest_published version={} release_url={}",
                        current_version, release_url
                    ));
                    UpdateCheckOutcome::LatestPublished {
                        current_version,
                        release_url,
                    }
                }
                Ok(update::UpdateCheckResult::UpdateAvailable {
                    current_version,
                    latest_version,
                    html_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=update_available current_version={} latest_version={} release_url={}",
                        current_version, latest_version, html_url
                    ));
                    UpdateCheckOutcome::UpdateAvailable {
                        current_version,
                        latest_version,
                        release_url: html_url,
                    }
                }
                Ok(update::UpdateCheckResult::NewerThanLatestPublished {
                    current_version,
                    latest_version,
                    releases_url,
                }) => {
                    log_line(&format!(
                        "update check result outcome=newer_than_latest current_version={} latest_version={} releases_url={}",
                        current_version, latest_version, releases_url
                    ));
                    UpdateCheckOutcome::NewerThanLatestPublished {
                        current_version,
                        latest_version,
                        releases_url,
                    }
                }
                Err(err) => {
                    let message = err.to_string();
                    log_line(&format!(
                        "update check result outcome=failure err={}",
                        message
                    ));
                    UpdateCheckOutcome::Failed { message }
                }
            };

            let boxed = Box::new(UpdateCheckMessage { outcome });
            let ptr = Box::into_raw(boxed) as isize;
            unsafe {
                let posted = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    crate::winmsg::WM_APP_UPDATE_CHECK_COMPLETE,
                    WPARAM(0),
                    LPARAM(ptr),
                )
                .is_ok();
                if !posted {
                    log_line("update check post failed");
                    let mut state = in_flight.lock().unwrap();
                    *state = false;
                }
            }
        });
        true
    }

    /// Marks the current update check as completed.
    pub fn finish_update_check(&self) {
        let mut in_flight = self.update_check_in_flight.lock().unwrap();
        *in_flight = false;
    }
}
