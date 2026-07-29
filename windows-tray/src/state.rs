use crate::model::TodayMenu;
use crate::restaurant::Provider;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// High-level fetch status for the currently selected restaurant.
pub enum FetchStatus {
    Idle,
    Loading,
    Ok,
    Stale,
    Error,
}

#[derive(Debug, Clone)]
/// Snapshot of UI-visible application state consumed by popup and tray rendering.
pub struct AppState {
    pub settings: Settings,
    pub status: FetchStatus,
    pub loading_started_epoch_ms: i64,
    pub error_message: String,
    pub stale_network_error: bool,
    pub today_menu: Option<TodayMenu>,
    pub restaurant_name: String,
    pub restaurant_url: String,
    pub raw_payload: String,
    pub provider: Provider,
    pub payload_date: String,
    pub stale_date: bool,
    pub api_stale: bool,
}
