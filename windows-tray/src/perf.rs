#[cfg(feature = "perf-counters")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "perf-counters")]
static REGEX_COMPILATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static TEXT_WIDTH_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static SNAPSHOT_CLONED_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static SNAPSHOT_CLONED_STRINGS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static LAYOUT_BUDGET_CACHE_READS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-counters")]
static LAYOUT_BUDGET_CACHE_PARSES: AtomicU64 = AtomicU64::new(0);

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_regex_compilation() {
    #[cfg(feature = "perf-counters")]
    REGEX_COMPILATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_text_width_call() {
    #[cfg(feature = "perf-counters")]
    TEXT_WIDTH_CALLS.fetch_add(1, Ordering::Relaxed);
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_snapshot_clone(bytes: usize, strings: usize) {
    #[cfg(feature = "perf-counters")]
    {
        SNAPSHOT_CLONED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
        SNAPSHOT_CLONED_STRINGS.fetch_add(strings as u64, Ordering::Relaxed);
    }
    #[cfg(not(feature = "perf-counters"))]
    {
        let _ = (bytes, strings);
    }
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_layout_budget_cache_read() {
    #[cfg(feature = "perf-counters")]
    LAYOUT_BUDGET_CACHE_READS.fetch_add(1, Ordering::Relaxed);
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_layout_budget_cache_parse() {
    #[cfg(feature = "perf-counters")]
    LAYOUT_BUDGET_CACHE_PARSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "perf-counters")]
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub regex_compilations: u64,
    pub text_width_calls: u64,
    pub snapshot_cloned_bytes: u64,
    pub snapshot_cloned_strings: u64,
    pub layout_budget_cache_reads: u64,
    pub layout_budget_cache_parses: u64,
}

#[cfg(feature = "perf-counters")]
pub fn snapshot() -> Snapshot {
    Snapshot {
        regex_compilations: REGEX_COMPILATIONS.load(Ordering::Relaxed),
        text_width_calls: TEXT_WIDTH_CALLS.load(Ordering::Relaxed),
        snapshot_cloned_bytes: SNAPSHOT_CLONED_BYTES.load(Ordering::Relaxed),
        snapshot_cloned_strings: SNAPSHOT_CLONED_STRINGS.load(Ordering::Relaxed),
        layout_budget_cache_reads: LAYOUT_BUDGET_CACHE_READS.load(Ordering::Relaxed),
        layout_budget_cache_parses: LAYOUT_BUDGET_CACHE_PARSES.load(Ordering::Relaxed),
    }
}

#[cfg(feature = "perf-counters")]
pub fn reset() {
    REGEX_COMPILATIONS.store(0, Ordering::Relaxed);
    TEXT_WIDTH_CALLS.store(0, Ordering::Relaxed);
    SNAPSHOT_CLONED_BYTES.store(0, Ordering::Relaxed);
    SNAPSHOT_CLONED_STRINGS.store(0, Ordering::Relaxed);
    LAYOUT_BUDGET_CACHE_READS.store(0, Ordering::Relaxed);
    LAYOUT_BUDGET_CACHE_PARSES.store(0, Ordering::Relaxed);
}
