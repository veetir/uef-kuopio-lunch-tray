//! Low-overhead diagnostic counters used by benchmarks and opt-in Windows builds.

#[cfg(feature = "perf-counters")]
use std::cell::RefCell;
#[cfg(feature = "perf-counters")]
use std::fmt::Write as _;
#[cfg(feature = "perf-counters")]
use std::fs::{create_dir_all, OpenOptions};
#[cfg(feature = "perf-counters")]
use std::io::Write as _;
#[cfg(feature = "perf-counters")]
use std::path::PathBuf;
#[cfg(feature = "perf-counters")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "perf-counters")]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

/// Win32/GDI entry points called by the popup paint implementation.
///
/// These are API-call counts, not syscall counts. GDI may batch eligible calls
/// in its user-mode client before entering the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum GdiCall {
    BeginPaint,
    EndPaint,
    BitBlt,
    MaskBlt,
    CreateBitmap,
    CreateCompatibleBitmap,
    CreateCompatibleDc,
    CreateFont,
    CreateSolidBrush,
    DeleteDc,
    DeleteObject,
    FillRect,
    GetDeviceCaps,
    GetTextExtent,
    GetTextMetrics,
    IntersectClipRect,
    RestoreDc,
    SaveDc,
    SelectObject,
    SetBkMode,
    SetTextColor,
    TextOut,
}

pub const GDI_CALL_COUNT: usize = 22;

impl GdiCall {
    #[cfg(feature = "perf-counters")]
    const ALL: [Self; GDI_CALL_COUNT] = [
        Self::BeginPaint,
        Self::EndPaint,
        Self::BitBlt,
        Self::MaskBlt,
        Self::CreateBitmap,
        Self::CreateCompatibleBitmap,
        Self::CreateCompatibleDc,
        Self::CreateFont,
        Self::CreateSolidBrush,
        Self::DeleteDc,
        Self::DeleteObject,
        Self::FillRect,
        Self::GetDeviceCaps,
        Self::GetTextExtent,
        Self::GetTextMetrics,
        Self::IntersectClipRect,
        Self::RestoreDc,
        Self::SaveDc,
        Self::SelectObject,
        Self::SetBkMode,
        Self::SetTextColor,
        Self::TextOut,
    ];

    #[cfg(feature = "perf-counters")]
    const fn name(self) -> &'static str {
        match self {
            Self::BeginPaint => "BeginPaint",
            Self::EndPaint => "EndPaint",
            Self::BitBlt => "BitBlt",
            Self::MaskBlt => "MaskBlt",
            Self::CreateBitmap => "CreateBitmap",
            Self::CreateCompatibleBitmap => "CreateCompatibleBitmap",
            Self::CreateCompatibleDc => "CreateCompatibleDC",
            Self::CreateFont => "CreateFontW",
            Self::CreateSolidBrush => "CreateSolidBrush",
            Self::DeleteDc => "DeleteDC",
            Self::DeleteObject => "DeleteObject",
            Self::FillRect => "FillRect",
            Self::GetDeviceCaps => "GetDeviceCaps",
            Self::GetTextExtent => "GetTextExtentPoint32W",
            Self::GetTextMetrics => "GetTextMetricsW",
            Self::IntersectClipRect => "IntersectClipRect",
            Self::RestoreDc => "RestoreDC",
            Self::SaveDc => "SaveDC",
            Self::SelectObject => "SelectObject",
            Self::SetBkMode => "SetBkMode",
            Self::SetTextColor => "SetTextColor",
            Self::TextOut => "TextOutW",
        }
    }

    #[cfg(feature = "perf-counters")]
    const fn is_documented_batch_candidate(self) -> bool {
        matches!(self, Self::BitBlt | Self::MaskBlt | Self::TextOut)
    }
}

#[cfg(feature = "perf-counters")]
struct PopupCounters {
    started_at: Instant,
    paint_active: bool,
    current_paint_calls: u64,
    paints: u64,
    animated_paints: u64,
    repaint_requests: u64,
    mouse_moves: u64,
    mouse_wheels: u64,
    key_downs: u64,
    animation_ticks: u64,
    selection_changes: u64,
    restaurant_switches: u64,
    mouse_move_ns: u64,
    max_mouse_move_ns: u64,
    paint_ns: u64,
    max_paint_ns: u64,
    client_pixels: u64,
    dirty_pixels: u64,
    full_client_paints: u64,
    max_calls_per_paint: u64,
    row_boundary_hits: u64,
    row_boundary_misses: u64,
    text_width_hits: u64,
    text_width_misses: u64,
    gdi_calls: [u64; GDI_CALL_COUNT],
}

#[cfg(feature = "perf-counters")]
impl Default for PopupCounters {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            paint_active: false,
            row_boundary_hits: 0,
            row_boundary_misses: 0,
            text_width_hits: 0,
            text_width_misses: 0,
            current_paint_calls: 0,
            paints: 0,
            animated_paints: 0,
            repaint_requests: 0,
            mouse_moves: 0,
            mouse_wheels: 0,
            key_downs: 0,
            animation_ticks: 0,
            selection_changes: 0,
            restaurant_switches: 0,
            mouse_move_ns: 0,
            max_mouse_move_ns: 0,
            paint_ns: 0,
            max_paint_ns: 0,
            client_pixels: 0,
            dirty_pixels: 0,
            full_client_paints: 0,
            max_calls_per_paint: 0,
            gdi_calls: [0; GDI_CALL_COUNT],
        }
    }
}

#[cfg(feature = "perf-counters")]
thread_local! {
    static POPUP_COUNTERS: RefCell<PopupCounters> = RefCell::new(PopupCounters::default());
}

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

/// Counts one executed popup paint API call while a paint guard is active.
#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_gdi_call(call: GdiCall) {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        let mut counters = counters.borrow_mut();
        if counters.paint_active {
            counters.gdi_calls[call as usize] += 1;
            counters.current_paint_calls += 1;
        }
    });
    #[cfg(not(feature = "perf-counters"))]
    let _ = call;
}

/// Times one invocation of the popup's complete `WM_PAINT` renderer.
#[must_use]
pub struct PopupPaintGuard {
    #[cfg(feature = "perf-counters")]
    started_at: Instant,
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn begin_popup_paint() -> PopupPaintGuard {
    #[cfg(feature = "perf-counters")]
    {
        POPUP_COUNTERS.with(|counters| {
            let mut counters = counters.borrow_mut();
            counters.paints += 1;
            counters.paint_active = true;
            counters.current_paint_calls = 0;
        });
        PopupPaintGuard {
            started_at: Instant::now(),
        }
    }
    #[cfg(not(feature = "perf-counters"))]
    PopupPaintGuard {}
}

/// Times the complete popup `WM_MOUSEMOVE` handler, including hit testing and
/// cursor updates but excluding the deferred `WM_PAINT` it may request.
#[must_use]
pub struct MouseMoveGuard {
    #[cfg(feature = "perf-counters")]
    started_at: Instant,
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn begin_mouse_move() -> MouseMoveGuard {
    #[cfg(feature = "perf-counters")]
    {
        POPUP_COUNTERS.with(|counters| counters.borrow_mut().mouse_moves += 1);
        MouseMoveGuard {
            started_at: Instant::now(),
        }
    }
    #[cfg(not(feature = "perf-counters"))]
    MouseMoveGuard {}
}

impl Drop for MouseMoveGuard {
    fn drop(&mut self) {
        #[cfg(feature = "perf-counters")]
        {
            let elapsed_ns = self.started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            POPUP_COUNTERS.with(|counters| {
                let mut counters = counters.borrow_mut();
                counters.mouse_move_ns = counters.mouse_move_ns.saturating_add(elapsed_ns);
                counters.max_mouse_move_ns = counters.max_mouse_move_ns.max(elapsed_ns);
            });
        }
    }
}

impl Drop for PopupPaintGuard {
    fn drop(&mut self) {
        #[cfg(feature = "perf-counters")]
        {
            let elapsed_ns = self.started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            POPUP_COUNTERS.with(|counters| {
                let mut counters = counters.borrow_mut();
                counters.paint_active = false;
                counters.paint_ns = counters.paint_ns.saturating_add(elapsed_ns);
                counters.max_paint_ns = counters.max_paint_ns.max(elapsed_ns);
                counters.max_calls_per_paint = counters
                    .max_calls_per_paint
                    .max(counters.current_paint_calls);
            });
        }
    }
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_animated_paint() {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| counters.borrow_mut().animated_paints += 1);
}

#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn record_paint_region(
    client_width: i32,
    client_height: i32,
    dirty_left: i32,
    dirty_top: i32,
    dirty_right: i32,
    dirty_bottom: i32,
) {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        let client_width = client_width.max(0) as u64;
        let client_height = client_height.max(0) as u64;
        let dirty_width = (dirty_right - dirty_left).max(0) as u64;
        let dirty_height = (dirty_bottom - dirty_top).max(0) as u64;
        let mut counters = counters.borrow_mut();
        counters.client_pixels = counters
            .client_pixels
            .saturating_add(client_width.saturating_mul(client_height));
        counters.dirty_pixels = counters
            .dirty_pixels
            .saturating_add(dirty_width.saturating_mul(dirty_height));
        if dirty_left <= 0
            && dirty_top <= 0
            && dirty_right as u64 >= client_width
            && dirty_bottom as u64 >= client_height
        {
            counters.full_client_paints += 1;
        }
    });
    #[cfg(not(feature = "perf-counters"))]
    let _ = (
        client_width,
        client_height,
        dirty_left,
        dirty_top,
        dirty_right,
        dirty_bottom,
    );
}

macro_rules! popup_counter {
    ($name:ident, $field:ident) => {
        #[cfg_attr(not(feature = "perf-counters"), inline(always))]
        pub fn $name() {
            #[cfg(feature = "perf-counters")]
            POPUP_COUNTERS.with(|counters| counters.borrow_mut().$field += 1);
        }
    };
}

popup_counter!(count_repaint_request, repaint_requests);
popup_counter!(count_mouse_wheel, mouse_wheels);
popup_counter!(count_key_down, key_downs);
popup_counter!(count_animation_tick, animation_ticks);
popup_counter!(count_selection_change, selection_changes);
popup_counter!(count_restaurant_switch, restaurant_switches);

#[cfg(feature = "perf-counters")]
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub regex_compilations: u64,
    pub text_width_calls: u64,
    pub snapshot_cloned_bytes: u64,
    pub snapshot_cloned_strings: u64,
    pub layout_budget_cache_reads: u64,
    pub layout_budget_cache_parses: u64,
    pub elapsed_ms: u64,
    pub paints: u64,
    pub animated_paints: u64,
    pub repaint_requests: u64,
    pub mouse_moves: u64,
    pub mouse_wheels: u64,
    pub key_downs: u64,
    pub animation_ticks: u64,
    pub selection_changes: u64,
    pub restaurant_switches: u64,
    pub mouse_move_ns: u64,
    pub max_mouse_move_ns: u64,
    pub paint_ns: u64,
    pub max_paint_ns: u64,
    pub client_pixels: u64,
    pub dirty_pixels: u64,
    pub full_client_paints: u64,
    pub max_calls_per_paint: u64,
    pub row_boundary_hits: u64,
    pub row_boundary_misses: u64,
    pub text_width_hits: u64,
    pub text_width_misses: u64,
    pub gdi_calls: [u64; GDI_CALL_COUNT],
}

#[cfg(feature = "perf-counters")]
pub fn snapshot() -> Snapshot {
    let popup = POPUP_COUNTERS.with(|counters| {
        let counters = counters.borrow();
        Snapshot {
            elapsed_ms: counters
                .started_at
                .elapsed()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            paints: counters.paints,
            animated_paints: counters.animated_paints,
            repaint_requests: counters.repaint_requests,
            mouse_moves: counters.mouse_moves,
            mouse_wheels: counters.mouse_wheels,
            key_downs: counters.key_downs,
            animation_ticks: counters.animation_ticks,
            selection_changes: counters.selection_changes,
            restaurant_switches: counters.restaurant_switches,
            mouse_move_ns: counters.mouse_move_ns,
            max_mouse_move_ns: counters.max_mouse_move_ns,
            paint_ns: counters.paint_ns,
            max_paint_ns: counters.max_paint_ns,
            client_pixels: counters.client_pixels,
            dirty_pixels: counters.dirty_pixels,
            full_client_paints: counters.full_client_paints,
            max_calls_per_paint: counters.max_calls_per_paint,
            row_boundary_hits: counters.row_boundary_hits,
            row_boundary_misses: counters.row_boundary_misses,
            text_width_hits: counters.text_width_hits,
            text_width_misses: counters.text_width_misses,
            gdi_calls: counters.gdi_calls,
            ..Default::default()
        }
    });
    Snapshot {
        regex_compilations: REGEX_COMPILATIONS.load(Ordering::Relaxed),
        text_width_calls: TEXT_WIDTH_CALLS.load(Ordering::Relaxed),
        snapshot_cloned_bytes: SNAPSHOT_CLONED_BYTES.load(Ordering::Relaxed),
        snapshot_cloned_strings: SNAPSHOT_CLONED_STRINGS.load(Ordering::Relaxed),
        layout_budget_cache_reads: LAYOUT_BUDGET_CACHE_READS.load(Ordering::Relaxed),
        layout_budget_cache_parses: LAYOUT_BUDGET_CACHE_PARSES.load(Ordering::Relaxed),
        ..popup
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
    POPUP_COUNTERS.with(|counters| *counters.borrow_mut() = PopupCounters::default());
}

#[cfg(feature = "perf-counters")]
fn per_second(count: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        0.0
    } else {
        count as f64 * 1000.0 / elapsed_ms as f64
    }
}

#[cfg(feature = "perf-counters")]
fn per_paint(value: u64, paints: u64) -> f64 {
    if paints == 0 {
        0.0
    } else {
        value as f64 / paints as f64
    }
}

#[cfg(feature = "perf-counters")]
fn wall_percent(elapsed_ns: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        0.0
    } else {
        elapsed_ns as f64 * 100.0 / (elapsed_ms as f64 * 1_000_000.0)
    }
}

#[cfg(feature = "perf-counters")]
pub fn format_report(batch_limit: Option<u32>) -> String {
    let counters = snapshot();
    let total_gdi_calls: u64 = counters.gdi_calls.iter().sum();
    let batch_candidates: u64 = GdiCall::ALL
        .iter()
        .filter(|call| call.is_documented_batch_candidate())
        .map(|call| counters.gdi_calls[*call as usize])
        .sum();
    let dirty_percent = if counters.client_pixels == 0 {
        0.0
    } else {
        counters.dirty_pixels as f64 * 100.0 / counters.client_pixels as f64
    };
    let mut out = String::new();
    let _ = writeln!(out, "=== Compass Lunch popup performance counters ===");
    let _ = writeln!(
        out,
        "scope=popup paint API calls; executed calls, not syscalls"
    );
    let _ = writeln!(
        out,
        "notice=counter instrumentation adds overhead; use a normal release for absolute CPU"
    );
    let _ = writeln!(out, "elapsed_ms={}", counters.elapsed_ms);
    match batch_limit {
        Some(limit) => {
            let _ = writeln!(out, "gdi_batch_limit={limit}");
        }
        None => {
            let _ = writeln!(out, "gdi_batch_limit=unavailable");
        }
    }
    let _ = writeln!(out, "events:");
    let _ = writeln!(
        out,
        "  paints={} ({:.2}/s), animated={}",
        counters.paints,
        per_second(counters.paints, counters.elapsed_ms),
        counters.animated_paints
    );
    let _ = writeln!(
        out,
        "  repaint_requests={} ({:.2}/s), paints_per_request={:.3}",
        counters.repaint_requests,
        per_second(counters.repaint_requests, counters.elapsed_ms),
        per_paint(counters.paints, counters.repaint_requests)
    );
    let _ = writeln!(
        out,
        "  mouse_moves={} ({:.2}/s), mouse_wheels={} ({:.2}/s), key_downs={} ({:.2}/s)",
        counters.mouse_moves,
        per_second(counters.mouse_moves, counters.elapsed_ms),
        counters.mouse_wheels,
        per_second(counters.mouse_wheels, counters.elapsed_ms),
        counters.key_downs,
        per_second(counters.key_downs, counters.elapsed_ms)
    );
    let _ = writeln!(
        out,
        "  animation_ticks={} ({:.2}/s), selection_changes={} ({:.2}/s), restaurant_switches={} ({:.2}/s)",
        counters.animation_ticks,
        per_second(counters.animation_ticks, counters.elapsed_ms),
        counters.selection_changes,
        per_second(counters.selection_changes, counters.elapsed_ms),
        counters.restaurant_switches,
        per_second(counters.restaurant_switches, counters.elapsed_ms)
    );
    let _ = writeln!(out, "mouse_move_handler:");
    let _ = writeln!(
        out,
        "  total_ms={:.3}, wall_occupancy={:.2}%, average_ms={:.3}, max_ms={:.3}",
        counters.mouse_move_ns as f64 / 1_000_000.0,
        wall_percent(counters.mouse_move_ns, counters.elapsed_ms),
        per_paint(counters.mouse_move_ns, counters.mouse_moves) / 1_000_000.0,
        counters.max_mouse_move_ns as f64 / 1_000_000.0
    );
    let _ = writeln!(out, "paint:");
    let _ = writeln!(
        out,
        "  total_ms={:.3}, wall_occupancy={:.2}%, average_ms={:.3}, max_ms={:.3}",
        counters.paint_ns as f64 / 1_000_000.0,
        wall_percent(counters.paint_ns, counters.elapsed_ms),
        per_paint(counters.paint_ns, counters.paints) / 1_000_000.0,
        counters.max_paint_ns as f64 / 1_000_000.0
    );
    let _ = writeln!(
        out,
        "  dirty_area={:.2}%, full_client_paints={}/{}",
        dirty_percent, counters.full_client_paints, counters.paints
    );
    let _ = writeln!(
        out,
        "  api_calls={}, average_calls_per_paint={:.2}, max_calls_per_paint={}",
        total_gdi_calls,
        per_paint(total_gdi_calls, counters.paints),
        counters.max_calls_per_paint
    );
    let _ = writeln!(
        out,
        "  documented_batch_candidates={}, other_or_barrier_calls={}",
        batch_candidates,
        total_gdi_calls.saturating_sub(batch_candidates)
    );
    let row_lookups = counters.row_boundary_hits + counters.row_boundary_misses;
    let hit_rate = if row_lookups == 0 {
        0.0
    } else {
        counters.row_boundary_hits as f64 * 100.0 / row_lookups as f64
    };
    let _ = writeln!(
        out,
        "  row_geometry: hits={}, misses={}, hit_rate={:.2}%, measured_rows_per_paint={:.2}",
        counters.row_boundary_hits,
        counters.row_boundary_misses,
        hit_rate,
        per_paint(counters.row_boundary_misses, counters.paints)
    );
    let width_lookups = counters.text_width_hits + counters.text_width_misses;
    let width_hit_rate = if width_lookups == 0 {
        0.0
    } else {
        counters.text_width_hits as f64 * 100.0 / width_lookups as f64
    };
    let _ = writeln!(
        out,
        "  text_width: hits={}, misses={}, hit_rate={:.2}%, measured_per_paint={:.2}",
        counters.text_width_hits,
        counters.text_width_misses,
        width_hit_rate,
        per_paint(counters.text_width_misses, counters.paints)
    );
    let _ = writeln!(out, "paint_api_calls:");
    for call in GdiCall::ALL {
        let _ = writeln!(
            out,
            "  {}={}",
            call.name(),
            counters.gdi_calls[call as usize]
        );
    }
    let _ = writeln!(out, "legacy_counters:");
    let _ = writeln!(
        out,
        "  text_width_calls={}, regex_compilations={}",
        counters.text_width_calls, counters.regex_compilations
    );
    let _ = writeln!(
        out,
        "  snapshot_cloned_bytes={}, snapshot_cloned_strings={}",
        counters.snapshot_cloned_bytes, counters.snapshot_cloned_strings
    );
    let _ = writeln!(
        out,
        "  layout_budget_cache_reads={}, layout_budget_cache_parses={}",
        counters.layout_budget_cache_reads, counters.layout_budget_cache_parses
    );
    out
}

#[cfg(feature = "perf-counters")]
pub fn report_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base)
        .join("compass-lunch")
        .join("performance.log")
}

#[cfg(feature = "perf-counters")]
pub fn append_report(batch_limit: Option<u32>) -> std::io::Result<PathBuf> {
    let path = report_path();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "unix_timestamp={timestamp}")?;
    file.write_all(format_report(batch_limit).as_bytes())?;
    writeln!(file)?;
    Ok(path)
}

#[cfg(all(feature = "perf-counters", target_os = "windows"))]
pub fn current_gdi_batch_limit() -> Option<u32> {
    Some(unsafe { windows::Win32::Graphics::Gdi::GdiGetBatchLimit() })
}

#[cfg(all(feature = "perf-counters", not(target_os = "windows")))]
pub fn current_gdi_batch_limit() -> Option<u32> {
    None
}

#[cfg(all(test, feature = "perf-counters"))]
mod tests {
    use super::*;

    #[test]
    fn paint_calls_are_counted_only_inside_a_paint() {
        reset();
        count_gdi_call(GdiCall::TextOut);
        {
            let _paint = begin_popup_paint();
            count_gdi_call(GdiCall::TextOut);
            count_gdi_call(GdiCall::SelectObject);
        }
        let counters = snapshot();
        assert_eq!(counters.paints, 1);
        assert_eq!(counters.gdi_calls[GdiCall::TextOut as usize], 1);
        assert_eq!(counters.gdi_calls[GdiCall::SelectObject as usize], 1);
        assert_eq!(counters.max_calls_per_paint, 2);
    }

    #[test]
    fn report_distinguishes_batch_candidates_from_barriers() {
        reset();
        {
            let _paint = begin_popup_paint();
            count_gdi_call(GdiCall::TextOut);
            count_gdi_call(GdiCall::BitBlt);
            count_gdi_call(GdiCall::SelectObject);
        }
        let report = format_report(Some(310));
        assert!(report.contains("gdi_batch_limit=310"));
        assert!(report.contains("documented_batch_candidates=2, other_or_barrier_calls=1"));
    }
}

/// Rows whose character geometry was reused from a previous paint.
#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_row_boundary_hit() {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        counters.borrow_mut().row_boundary_hits += 1;
    });
}

/// Rows whose character geometry had to be measured.
#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_row_boundary_miss() {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        counters.borrow_mut().row_boundary_misses += 1;
    });
}

/// Strings whose width was reused from an earlier measurement.
#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_text_width_hit() {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        counters.borrow_mut().text_width_hits += 1;
    });
}

/// Strings that had to be measured through GDI.
#[cfg_attr(not(feature = "perf-counters"), inline(always))]
pub fn count_text_width_miss() {
    #[cfg(feature = "perf-counters")]
    POPUP_COUNTERS.with(|counters| {
        counters.borrow_mut().text_width_misses += 1;
    });
}
