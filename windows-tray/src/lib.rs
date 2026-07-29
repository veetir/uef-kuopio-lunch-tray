pub mod api;
#[cfg(not(all(feature = "bench", not(windows))))]
pub mod app;
pub mod cache;
pub mod custom_themes;
pub mod favorites;
pub mod format;
pub mod log;
pub mod model;
pub mod popup;
pub mod restaurant;
pub mod settings;
#[cfg(not(all(feature = "bench", not(windows))))]
pub mod startup;
pub mod state;
#[cfg(not(all(feature = "bench", not(windows))))]
pub mod tray;
pub mod update;
pub mod util;
#[cfg(not(all(feature = "bench", not(windows))))]
pub mod winmsg;

pub mod perf;

#[cfg(feature = "bench")]
pub mod bench_support;
