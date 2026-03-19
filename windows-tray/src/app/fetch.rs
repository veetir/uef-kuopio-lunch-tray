use super::*;

impl App {
    /// Loads cached menu data for the currently selected restaurant, if available.
    pub fn load_cache_for_current(&self) -> bool {
        let (restaurant, language) = {
            let state = self.state.lock().unwrap();
            (
                restaurant_for_code(
                    &state.settings.restaurant_code,
                    state.settings.enable_antell_restaurants,
                ),
                state.settings.language.clone(),
            )
        };
        if is_hard_closed_today(restaurant) {
            let result = api::closed_today_fetch_output(restaurant, &language);
            self.apply_cached_result(&result);
            log_line(&format!(
                "closed-day synthetic state provider={} code={} language={}",
                provider_key(restaurant.provider),
                restaurant.code,
                language
            ));
            return true;
        }

        let cached_date = if restaurant.provider == Provider::Antell {
            cache::cache_mtime_ms(restaurant.provider, restaurant.code, &language)
                .and_then(date_key_from_epoch_ms)
        } else {
            None
        };

        if self.load_memory_for(
            restaurant.code,
            &language,
            restaurant.provider,
            cached_date.as_deref(),
        ) {
            log_line(&format!(
                "memory cache hit provider={} code={} language={}",
                provider_key(restaurant.provider),
                restaurant.code,
                language
            ));
            return true;
        }

        if let Some(raw) = cache::read_cache(restaurant.provider, restaurant.code, &language) {
            match api::parse_cached_payload(&raw, restaurant.provider, restaurant, &language) {
                Ok(result) => {
                    let mut result = result;
                    if let Some(date_key) = cached_date {
                        result.payload_date = date_key;
                    }
                    self.apply_cached_result(&result);
                    self.store_memory_from_fetch_output(restaurant.code, &language, &result);
                    log_line(&format!(
                        "cache hit provider={} code={} language={}",
                        provider_key(restaurant.provider),
                        restaurant.code,
                        language
                    ));
                    return true;
                }
                Err(err) => {
                    let mut state = self.state.lock().unwrap();
                    state.status = FetchStatus::Error;
                    state.loading_started_epoch_ms = 0;
                    state.error_message = err.to_string();
                    state.stale_network_error = false;
                    log_line(&format!(
                        "cache parse error provider={} code={} language={} err={}",
                        provider_key(restaurant.provider),
                        restaurant.code,
                        language,
                        err
                    ));
                    return false;
                }
            }
        }
        log_line(&format!(
            "cache miss provider={} code={} language={}",
            provider_key(restaurant.provider),
            restaurant.code,
            language
        ));
        false
    }

    fn apply_cached_result(&self, result: &FetchOutput) {
        let mut state = self.state.lock().unwrap();
        state.raw_payload = result.raw_json.clone();
        state.restaurant_name = result.restaurant_name.clone();
        state.restaurant_url = result.restaurant_url.clone();
        state.today_menu = result.today_menu.clone();
        state.provider = result.provider;
        state.payload_date = result.payload_date.clone();
        update_stale_date(&mut state);
        if result.ok {
            state.status = FetchStatus::Ok;
            state.loading_started_epoch_ms = 0;
            state.error_message.clear();
            state.stale_network_error = false;
        } else {
            state.status = FetchStatus::Error;
            state.loading_started_epoch_ms = 0;
            state.error_message = result.error_message.clone();
            state.stale_network_error = false;
        }
    }

    fn store_memory_from_fetch_output(&self, code: &str, language: &str, result: &FetchOutput) {
        let key = menu_cache_key(code, language);
        let entry = MemoryMenuEntry {
            ok: result.ok,
            error_message: result.error_message.clone(),
            today_menu: result.today_menu.clone(),
            restaurant_name: result.restaurant_name.clone(),
            restaurant_url: result.restaurant_url.clone(),
            provider: result.provider,
            raw_payload: result.raw_json.clone(),
            payload_date: result.payload_date.clone(),
        };
        let mut cache = self.memory_menu_cache.lock().unwrap();
        cache.insert(key, entry);
    }

    fn load_memory_for(
        &self,
        code: &str,
        language: &str,
        provider: Provider,
        antell_payload_date: Option<&str>,
    ) -> bool {
        let key = menu_cache_key(code, language);
        let mut entry = {
            let cache = self.memory_menu_cache.lock().unwrap();
            cache.get(&key).cloned()
        };
        let Some(mut entry) = entry.take() else {
            return false;
        };

        if provider == Provider::Antell {
            if let Some(date_key) = antell_payload_date {
                entry.payload_date = date_key.to_string();
            }
        }

        let mut state = self.state.lock().unwrap();
        state.raw_payload = entry.raw_payload;
        state.restaurant_name = entry.restaurant_name;
        state.restaurant_url = entry.restaurant_url;
        state.today_menu = entry.today_menu;
        state.provider = entry.provider;
        state.payload_date = entry.payload_date;
        update_stale_date(&mut state);
        state.loading_started_epoch_ms = 0;
        state.stale_network_error = false;
        if entry.ok {
            state.status = FetchStatus::Ok;
            state.error_message.clear();
        } else {
            state.status = FetchStatus::Error;
            state.error_message = entry.error_message;
        }
        true
    }

    /// Starts any startup refresh work that should happen after cached state is restored.
    pub fn maybe_refresh_on_startup(&self) {
        self.maybe_refresh_current_with_reasons(
            "startup",
            RefreshNeedReasons {
                missing: FetchReason::StartupMissingCache,
                stale: FetchReason::StartupStaleDate,
                interval: FetchReason::StartupRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers a timer-driven refresh for the currently selected restaurant.
    pub fn refresh_current_from_timer(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::RefreshTimer),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers the daily midnight refresh path.
    pub fn refresh_current_at_midnight(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::MidnightRollover),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: false,
            },
        );
    }

    /// Triggers a user-requested refresh for the currently selected restaurant.
    pub fn refresh_current_manually(&self) {
        self.start_refresh_for_code(
            &self.current_code(),
            FetchContext::new(FetchMode::Current, FetchReason::ManualRefresh),
            StartOptions {
                mark_loading_when_empty: true,
                bypass_cooldown: true,
            },
        );
    }

    /// Starts a retry fetch after a previous refresh failure.
    pub fn start_refresh_retry(&self) {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip("retry_timer", &target, "hard_closed_today");
            return;
        }

        let (refresh_minutes, payload_date, has_payload) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.refresh_minutes,
                state.payload_date.clone(),
                !state.raw_payload.is_empty(),
            )
        };

        let Some(_) = refresh_need_for_target(
            &target,
            refresh_minutes,
            &payload_date,
            has_payload,
            now_epoch_ms(),
        ) else {
            log_probe_skip("retry_timer", &target, "cache_fresh");
            return;
        };

        self.start_refresh_for_code(
            target.restaurant.code,
            FetchContext::new(FetchMode::Current, FetchReason::RetryTimer),
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }

    fn current_code(&self) -> String {
        let state = self.state.lock().unwrap();
        state.settings.restaurant_code.clone()
    }

    fn current_target(&self) -> FetchTarget {
        let state = self.state.lock().unwrap();
        fetch_target_for_code(
            &state.settings,
            &state.settings.restaurant_code,
            state.settings.enable_antell_restaurants,
        )
    }

    fn start_refresh_for_code(
        &self,
        code: &str,
        context: FetchContext,
        options: StartOptions,
    ) -> bool {
        let hwnd = self.hwnd_tray();
        let now = now_epoch_ms();
        let (settings, target, is_current_code) = {
            let mut state = self.state.lock().unwrap();
            let mut settings = state.settings.clone();
            settings.restaurant_code = code.to_string();
            let target =
                fetch_target_for_code(&settings, code, state.settings.enable_antell_restaurants);
            let is_current = state.settings.restaurant_code == code;
            if is_current && options.mark_loading_when_empty && state.raw_payload.is_empty() {
                state.status = FetchStatus::Loading;
                state.loading_started_epoch_ms = now;
            }
            if is_current {
                state.error_message.clear();
            }
            (settings, target, is_current)
        };

        if is_hard_closed_today(target.restaurant) {
            log_fetch_probe("gate", &context, &target, "skip", "hard_closed_today");
            return false;
        }

        {
            let mut request_states = self.request_states.lock().unwrap();
            let entry = request_states.entry(target.key.clone()).or_default();
            if entry.in_flight {
                let detail = format!(
                    "in_flight=true last_reason={}",
                    entry
                        .last_reason
                        .map(|reason| reason.as_str())
                        .unwrap_or("-")
                );
                log_fetch_probe("gate", &context, &target, "skip", &detail);
                return false;
            }

            if !options.bypass_cooldown && now < entry.cooldown_until_epoch_ms {
                let remaining_ms = entry.cooldown_until_epoch_ms.saturating_sub(now);
                let detail = format!(
                    "cooldown_remaining_ms={} failures={} last_reason={}",
                    remaining_ms,
                    entry.consecutive_failures,
                    entry
                        .last_reason
                        .map(|reason| reason.as_str())
                        .unwrap_or("-")
                );
                log_fetch_probe("gate", &context, &target, "skip", &detail);
                return false;
            }

            entry.in_flight = true;
            entry.last_attempt_epoch_ms = now;
            entry.last_reason = Some(context.reason);
        }

        let requested_code = code.to_string();
        let requested_language = settings.language.clone();
        let requested_effective_language = target.effective_language.clone();
        let request_key = target.key.clone();

        if is_current_code {
            log_fetch_probe("gate", &context, &target, "allow", "");
        } else {
            log_fetch_probe("prefetch_gate", &context, &target, "allow", "");
        }

        std::thread::spawn(move || {
            let result = api::fetch_today(&settings, &context);
            let message = FetchMessage {
                requested_code,
                requested_language,
                requested_effective_language,
                request_key,
                context,
                result,
            };
            let boxed = Box::new(message);
            let ptr = Box::into_raw(boxed) as isize;
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    crate::winmsg::WM_APP_FETCH_COMPLETE,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(ptr),
                );
            }
        });
        true
    }

    /// Applies a completed fetch message on the UI thread and updates cached state.
    pub fn apply_fetch_message(&self, message: FetchMessage) -> FetchApplyOutcome {
        let FetchMessage {
            requested_code,
            requested_language,
            requested_effective_language,
            request_key,
            context,
            result,
        } = message;
        let now = now_epoch_ms();

        let (current_code, current_language, enable_antell) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.restaurant_code.clone(),
                state.settings.language.clone(),
                state.settings.enable_antell_restaurants,
            )
        };
        let current_target =
            fetch_target_for_values(&current_code, &current_language, enable_antell);
        let is_current_request = requested_code == current_code
            && requested_effective_language == current_target.effective_language;
        let alias_language = if is_current_request && current_language != requested_language {
            Some(current_language.clone())
        } else {
            None
        };
        let cooldown_ms = self.finish_request_state(&request_key, &context, result.ok, now);
        let stale_no_menu_cooldown_ms =
            self.apply_stale_no_menu_cooldown(&request_key, &result, now);

        if !is_current_request {
            if result.ok {
                self.persist_result_for_languages(
                    &requested_code,
                    &result,
                    &[requested_language.as_str()],
                );
                log_line(&format!(
                    "fetch apply mode=background reason={} code={} ui_language={} fetch_language={} outcome=success request_key={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                ));
                if stale_no_menu_cooldown_ms > 0 {
                    log_line(&format!(
                        "fetch cooldown mode=background reason={} code={} request_key={} cooldown_ms={} detail=stale_no_today_menu payload_date={}",
                        context.reason.as_str(),
                        requested_code,
                        request_key,
                        stale_no_menu_cooldown_ms,
                        result.payload_date,
                    ));
                }
                FetchApplyOutcome::BackgroundSuccess
            } else {
                log_line(&format!(
                    "fetch apply mode=background reason={} code={} ui_language={} fetch_language={} outcome=failure request_key={} cooldown_ms={} err={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                    cooldown_ms,
                    result.error_message,
                ));
                FetchApplyOutcome::BackgroundFailure
            }
        } else {
            let mut state = self.state.lock().unwrap();
            if result.ok {
                state.status = FetchStatus::Ok;
                state.loading_started_epoch_ms = 0;
                state.error_message.clear();
                state.stale_network_error = false;
                state.raw_payload = result.raw_json.clone();
                state.restaurant_name = result.restaurant_name.clone();
                state.restaurant_url = result.restaurant_url.clone();
                state.today_menu = result.today_menu.clone();
                state.provider = result.provider;
                state.payload_date = result.payload_date.clone();
                update_stale_date(&mut state);
                state.settings.last_updated_epoch_ms = now;
                if let Err(err) = save_settings(&state.settings) {
                    log_line(&format!("save settings failed: {}", err));
                }
                log_line(&format!(
                    "fetch apply mode=current reason={} code={} ui_language={} fetch_language={} outcome=success request_key={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                ));
                if stale_no_menu_cooldown_ms > 0 {
                    log_line(&format!(
                        "fetch cooldown mode=current reason={} code={} request_key={} cooldown_ms={} detail=stale_no_today_menu payload_date={}",
                        context.reason.as_str(),
                        requested_code,
                        request_key,
                        stale_no_menu_cooldown_ms,
                        result.payload_date,
                    ));
                }
                drop(state);
                let mut languages = vec![requested_language.as_str()];
                if let Some(alias_language) = alias_language.as_deref() {
                    languages.push(alias_language);
                }
                self.persist_result_for_languages(&requested_code, &result, &languages);
                FetchApplyOutcome::CurrentSuccess
            } else {
                if !state.raw_payload.is_empty() {
                    state.status = FetchStatus::Stale;
                    state.loading_started_epoch_ms = 0;
                    state.stale_network_error = is_probable_network_error(&result.error_message);
                } else {
                    state.status = FetchStatus::Error;
                    state.loading_started_epoch_ms = 0;
                    state.stale_network_error = false;
                }
                state.error_message = result.error_message.clone();
                log_line(&format!(
                    "fetch apply mode=current reason={} code={} ui_language={} fetch_language={} outcome=failure request_key={} cooldown_ms={} err={}",
                    context.reason.as_str(),
                    requested_code,
                    requested_language,
                    requested_effective_language,
                    request_key,
                    cooldown_ms,
                    result.error_message,
                ));
                FetchApplyOutcome::CurrentFailure
            }
        }
    }

    fn apply_stale_no_menu_cooldown(
        &self,
        request_key: &str,
        result: &FetchOutput,
        now: i64,
    ) -> u32 {
        if !result.ok || result.today_menu.is_some() {
            return 0;
        }
        let payload_date = result.payload_date.trim();
        if payload_date.is_empty() || payload_date == today_key() {
            return 0;
        }
        let mut request_states = self.request_states.lock().unwrap();
        let Some(entry) = request_states.get_mut(request_key) else {
            return 0;
        };
        entry.cooldown_until_epoch_ms = now.saturating_add(STALE_NO_MENU_COOLDOWN_MS as i64);
        STALE_NO_MENU_COOLDOWN_MS
    }

    fn finish_request_state(
        &self,
        request_key: &str,
        context: &FetchContext,
        ok: bool,
        now: i64,
    ) -> u32 {
        let mut request_states = self.request_states.lock().unwrap();
        let entry = request_states.entry(request_key.to_string()).or_default();
        entry.in_flight = false;
        entry.last_reason = Some(context.reason);
        if ok {
            entry.last_success_epoch_ms = now;
            entry.consecutive_failures = 0;
            entry.cooldown_until_epoch_ms = 0;
            0
        } else {
            entry.last_failure_epoch_ms = now;
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
            let delay_ms = retry_delay_ms_for_failures(entry.consecutive_failures);
            entry.cooldown_until_epoch_ms = now.saturating_add(delay_ms as i64);
            delay_ms
        }
    }

    fn persist_result_for_languages(&self, code: &str, result: &FetchOutput, languages: &[&str]) {
        if result.raw_json.is_empty() {
            return;
        }

        for language in languages {
            if let Err(err) = cache::write_cache(result.provider, code, language, &result.raw_json)
            {
                log_line(&format!(
                    "cache write failed code={} language={} err={}",
                    code, language, err
                ));
                continue;
            }
            self.store_memory_from_fetch_output(code, language, result);
        }
    }

    /// Refreshes when selection changed and the cached data is missing or stale enough.
    pub fn maybe_refresh_on_selection(&self) {
        self.maybe_refresh_current_with_reasons(
            "selection",
            RefreshNeedReasons {
                missing: FetchReason::SelectionMissingCache,
                stale: FetchReason::SelectionStaleDate,
                interval: FetchReason::SelectionRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }

    /// Refreshes when the UI language changed and the matching cache is missing or stale.
    pub fn maybe_refresh_on_language_switch(&self) {
        self.maybe_refresh_current_with_reasons(
            "language_switch",
            RefreshNeedReasons {
                missing: FetchReason::LanguageSwitchMissingCache,
                stale: FetchReason::LanguageSwitchStaleDate,
                interval: FetchReason::LanguageSwitchRefreshInterval,
            },
            StartOptions {
                mark_loading_when_empty: false,
                bypass_cooldown: false,
            },
        );
    }
    /// Refreshes if the cached payload date no longer matches the local day.
    pub fn check_stale_date_and_refresh(&self) {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip("stale_check", &target, "hard_closed_today");
            let mut state = self.state.lock().unwrap();
            state.stale_date = false;
            return;
        }

        let should_refresh = {
            let mut state = self.state.lock().unwrap();
            let today_key = today_key();
            if !state.payload_date.is_empty() {
                let stale = state.payload_date != today_key;
                state.stale_date = stale;
                stale
            } else {
                state.stale_date = false;
                false
            }
        };
        if should_refresh {
            self.start_refresh_for_code(
                target.restaurant.code,
                FetchContext::new(FetchMode::Current, FetchReason::StaleDateCheck),
                StartOptions {
                    mark_loading_when_empty: false,
                    bypass_cooldown: false,
                },
            );
        } else {
            log_probe_skip("stale_check", &target, "not_stale");
        }
    }

    /// Returns the next retry delay derived from recent fetch failures.
    pub fn current_retry_delay_ms(&self) -> u32 {
        let now = now_epoch_ms();
        let target = self.current_target();
        let request_states = self.request_states.lock().unwrap();
        let Some(entry) = request_states.get(&target.key) else {
            return 1_000;
        };
        if entry.cooldown_until_epoch_ms <= now {
            1_000
        } else {
            entry.cooldown_until_epoch_ms.saturating_sub(now).max(1_000) as u32
        }
    }

    /// Prefetches menus for non-selected restaurants to improve switching latency.
    pub fn prefetch_enabled_restaurants(&self) {
        let now = now_epoch_ms();
        {
            let mut last_prefetch = self.last_prefetch_ms.lock().unwrap();
            if now.saturating_sub(*last_prefetch) < 5 * 60_000 {
                let target = self.current_target();
                log_probe_skip("prefetch", &target, "recently_ran");
                return;
            }
            *last_prefetch = now;
        }

        let (settings, current_code) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.clone(),
                state.settings.restaurant_code.clone(),
            )
        };
        let today = today_key();
        let restaurants = available_restaurants(settings.enable_antell_restaurants);

        let mut queued = 0usize;
        for restaurant in restaurants {
            if restaurant.code == current_code {
                continue;
            }
            if is_hard_closed_today(restaurant) {
                let target = FetchTarget {
                    restaurant,
                    ui_language: settings.language.clone(),
                    effective_language: effective_fetch_language(restaurant, &settings.language),
                    key: request_state_key(
                        restaurant.code,
                        &effective_fetch_language(restaurant, &settings.language),
                    ),
                };
                log_probe_skip("prefetch", &target, "hard_closed_today");
                continue;
            }
            let target = fetch_target_for_code(
                &settings,
                restaurant.code,
                settings.enable_antell_restaurants,
            );
            let need = match cache::cache_mtime_ms(
                restaurant.provider,
                restaurant.code,
                &settings.language,
            ) {
                None => Some(FetchReason::PrefetchMissingCache),
                Some(ts) => match date_key_from_epoch_ms(ts) {
                    Some(date) if date != today => Some(FetchReason::PrefetchStaleDate),
                    Some(_) => None,
                    None => Some(FetchReason::PrefetchStaleDate),
                },
            };
            let Some(reason) = need else {
                log_probe_skip("prefetch", &target, "cache_fresh");
                continue;
            };
            if self.start_refresh_for_code(
                restaurant.code,
                FetchContext::new(FetchMode::Background, reason),
                StartOptions {
                    mark_loading_when_empty: false,
                    bypass_cooldown: false,
                },
            ) {
                queued = queued.saturating_add(1);
            }
        }
        log_line(&format!("prefetch queued={}", queued));
    }

    fn maybe_refresh_current_with_reasons(
        &self,
        trigger: &str,
        reasons: RefreshNeedReasons,
        options: StartOptions,
    ) -> bool {
        let target = self.current_target();
        if is_hard_closed_today(target.restaurant) {
            log_probe_skip(trigger, &target, "hard_closed_today");
            return false;
        }

        let (refresh_minutes, payload_date, has_payload) = {
            let state = self.state.lock().unwrap();
            (
                state.settings.refresh_minutes,
                state.payload_date.clone(),
                !state.raw_payload.is_empty(),
            )
        };

        let need = refresh_need_for_target(
            &target,
            refresh_minutes,
            &payload_date,
            has_payload,
            now_epoch_ms(),
        );

        let Some(need) = need else {
            log_probe_skip(trigger, &target, "cache_fresh");
            return false;
        };

        if need == RefreshNeed::RefreshIntervalElapsed && refresh_minutes == 0 {
            log_probe_skip(trigger, &target, "refresh_interval_disabled");
            return false;
        }

        let reason = match need {
            RefreshNeed::MissingCache => reasons.missing,
            RefreshNeed::StaleDate => reasons.stale,
            RefreshNeed::RefreshIntervalElapsed => reasons.interval,
        };
        self.start_refresh_for_code(
            target.restaurant.code,
            FetchContext::new(FetchMode::Current, reason),
            options,
        )
    }
}
