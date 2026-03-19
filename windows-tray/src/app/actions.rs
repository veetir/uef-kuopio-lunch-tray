use super::*;

impl App {
    /// Changes the selected restaurant by its stable restaurant code.
    pub fn set_restaurant(&self, code: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.restaurant_code = code.to_string();
        let restaurant = restaurant_for_code(
            &state.settings.restaurant_code,
            state.settings.enable_antell_restaurants,
        );
        state.provider = restaurant.provider;
        state.restaurant_url = restaurant.url.unwrap_or_default().to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Changes the selected restaurant by its menu order index.
    pub fn set_restaurant_index(&self, index: usize) -> bool {
        let enable_antell = {
            let state = self.state.lock().unwrap();
            state.settings.enable_antell_restaurants
        };
        let Some(restaurant) = restaurant_for_shortcut_index(index, enable_antell) else {
            return false;
        };
        self.set_restaurant(restaurant.code);
        true
    }

    /// Changes the UI language and refreshes any derived state that depends on it.
    pub fn set_language(&self, language: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.language = language.to_string();
        let _ = save_settings(&state.settings);
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Toggles whether menu headings show prices.
    pub fn toggle_show_prices(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_prices = !state.settings.show_prices;
        let _ = save_settings(&state.settings);
    }

    /// Toggles whether allergen suffixes are rendered.
    pub fn toggle_show_allergens(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_allergens = !state.settings.show_allergens;
        let _ = save_settings(&state.settings);
    }

    /// Toggles gluten-free highlighting.
    pub fn toggle_highlight_gluten_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_gluten_free = !state.settings.highlight_gluten_free;
        let _ = save_settings(&state.settings);
    }

    /// Toggles vegetarian highlighting.
    pub fn toggle_highlight_veg(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_veg = !state.settings.highlight_veg;
        let _ = save_settings(&state.settings);
    }

    /// Toggles lactose-free highlighting.
    pub fn toggle_highlight_lactose_free(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.highlight_lactose_free = !state.settings.highlight_lactose_free;
        let _ = save_settings(&state.settings);
    }

    /// Toggles popup open/close/switch animations.
    pub fn toggle_animations(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.animations_enabled = !state.settings.animations_enabled;
        let _ = save_settings(&state.settings);
    }

    /// Toggles student price visibility in Compass price strings.
    pub fn toggle_show_student_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_student_price = !state.settings.show_student_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles staff price visibility in Compass price strings.
    pub fn toggle_show_staff_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_staff_price = !state.settings.show_staff_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles guest price visibility in Compass price strings.
    pub fn toggle_show_guest_price(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.show_guest_price = !state.settings.show_guest_price;
        let _ = save_settings(&state.settings);
    }

    /// Toggles hiding expensive student meals from rendered menus.
    pub fn toggle_hide_expensive_student_meals(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.hide_expensive_student_meals = !state.settings.hide_expensive_student_meals;
        let _ = save_settings(&state.settings);
    }

    /// Updates the configured automatic refresh interval in minutes.
    pub fn set_refresh_minutes(&self, minutes: u32) {
        let mut state = self.state.lock().unwrap();
        state.settings.refresh_minutes = minutes;
        let _ = save_settings(&state.settings);
    }

    /// Moves the restaurant selection backward or forward in the enabled list.
    pub fn cycle_restaurant(&self, direction: i32) {
        let mut state = self.state.lock().unwrap();
        let current = state.settings.restaurant_code.as_str();
        let list = available_restaurants(state.settings.enable_antell_restaurants);
        let mut idx = list.iter().position(|c| c.code == current).unwrap_or(0) as i32;
        idx += direction;
        if idx < 0 {
            idx = list.len() as i32 - 1;
        } else if idx >= list.len() as i32 {
            idx = 0;
        }
        state.settings.restaurant_code = list[idx as usize].code.to_string();
        state.provider = list[idx as usize].provider;
        state.restaurant_url = list[idx as usize].url.unwrap_or_default().to_string();
        state.raw_payload.clear();
        state.today_menu = None;
        state.payload_date.clear();
        state.stale_date = false;
        state.status = FetchStatus::Idle;
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
    }

    /// Writes the current settings snapshot to disk.
    pub fn persist_settings(&self) {
        let settings = {
            let state = self.state.lock().unwrap();
            state.settings.clone()
        };
        let _ = save_settings(&settings);
    }

    /// Opens the current restaurant URL in the system browser, if available.
    pub fn open_current_url(&self) {
        let url = {
            let state = self.state.lock().unwrap();
            state.restaurant_url.clone()
        };
        if url.is_empty() {
            return;
        }
        self.open_target(&url);
    }

    /// Opens the app data directory used for settings, cache, and logs.
    pub fn open_appdata_dir(&self) {
        let dir = settings_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log_line(&format!("failed to create appdata dir: {}", err));
            return;
        }
        let path = dir.to_string_lossy().to_string();
        self.open_target(&path);
    }

    /// Opens the configured feedback URL in the system browser.
    pub fn open_feedback_url(&self) {
        self.open_target("https://github.com/veetir/uef-kuopio-lunch-tray/issues");
    }

    /// Opens a release or releases page in the system browser.
    pub fn open_release_url(&self, url: &str) {
        if url.trim().is_empty() {
            return;
        }
        self.open_target(url);
    }

    /// Returns the configured automatic refresh interval in minutes.
    pub fn refresh_minutes(&self) -> u32 {
        let state = self.state.lock().unwrap();
        state.settings.refresh_minutes
    }
    /// Changes the active popup theme.
    pub fn set_theme(&self, theme: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.theme = normalize_theme(theme);
        let _ = save_settings(&state.settings);
    }

    /// Changes the popup scale preset.
    pub fn set_widget_scale(&self, value: &str) {
        let mut state = self.state.lock().unwrap();
        state.settings.widget_scale = normalize_widget_scale(value);
        let _ = save_settings(&state.settings);
    }

    /// Toggles diagnostic logging and persists the updated setting.
    pub fn toggle_logging(&self) {
        let mut state = self.state.lock().unwrap();
        state.settings.enable_logging = !state.settings.enable_logging;
        set_log_enabled(state.settings.enable_logging);
        if state.settings.enable_logging {
            log_line("logging enabled");
        }
        let _ = save_settings(&state.settings);
    }
    fn open_target(&self, target: &str) {
        let wide = crate::util::to_wstring(target);
        unsafe {
            ShellExecuteW(
                None,
                windows::core::PCWSTR(crate::util::to_wstring("open").as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}
