//! Solid fills through reused brushes.
//!
//! Popup chrome is drawn from a lot of small rectangles: bevel edges are four
//! fills each, bullets are two or three, and the header button glyphs are drawn
//! one horizontal scanline at a time. Creating and destroying a brush around each
//! of those cost three GDI calls to paint a rectangle, and after the font and
//! measurement caches landed it was the largest single item left in a paint --
//! 140 `FillRect` with 73 `CreateSolidBrush` and 74 `DeleteObject` behind them.
//!
//! Every one of those brushes was a solid brush in a colour the theme had already
//! used. Keeping them makes a fill one call instead of three.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Graphics::Gdi::HBRUSH;

/// Colours worth keeping a brush for. A theme palette is a couple of dozen
/// colours, and the derived bevel and stale tones perhaps as many again, so this
/// holds several themes' worth. Past it, fills fall back to create-and-destroy
/// rather than growing without bound.
///
/// The cap matters because these brushes are **never deleted**, on the same
/// reasoning as the popup fonts: a live handle cannot be freed safely from here,
/// and a handle that might be reissued is a handle that cannot be cached. GDI
/// allows 10,000 objects per process, so this stays three orders of magnitude
/// clear of that.
const BRUSH_CACHE_LIMIT: usize = 256;

thread_local! {
    static SOLID_BRUSHES: RefCell<HashMap<u32, HBRUSH>> = RefCell::new(HashMap::new());
}

/// Fills `rect` with a solid `color`.
///
/// Degenerate rectangles are dropped here rather than at each call site: scanline
/// glyph drawing generates them at the tips of triangles and the ends of strokes.
pub(super) fn fill_solid_rect(hdc: HDC, rect: &RECT, color: COLORREF) {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    unsafe {
        match cached_brush(color) {
            Some(brush) => {
                FillRect(hdc, rect, brush);
            }
            None => {
                let brush = CreateSolidBrush(color);
                FillRect(hdc, rect, brush);
                let _ = DeleteObject(brush);
            }
        }
    }
}

/// A kept brush for `color`, or `None` when the cache is full or GDI refused.
fn cached_brush(color: COLORREF) -> Option<HBRUSH> {
    cached_brush_with(color, |color| unsafe { CreateSolidBrush(color) })
}

/// The bookkeeping half, with brush creation injected so it can be exercised off
/// Windows. Linking a test binary against GDI is not possible on the development
/// host, and the part worth testing is the keying and the cap, not the call.
fn cached_brush_with(color: COLORREF, create: impl FnOnce(COLORREF) -> HBRUSH) -> Option<HBRUSH> {
    SOLID_BRUSHES.with(|cache| {
        if let Some(brush) = cache.borrow().get(&color.0) {
            return Some(*brush);
        }
        let mut cache = cache.borrow_mut();
        if cache.len() >= BRUSH_CACHE_LIMIT {
            return None;
        }
        let brush = create(color);
        if brush.0 == 0 {
            return None;
        }
        cache.insert(color.0, brush);
        Some(brush)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for GDI, handing out a distinct handle per call so the tests can
    /// tell a reused brush from a freshly created one.
    fn fake_brush(color: COLORREF) -> HBRUSH {
        HBRUSH(color.0 as isize + 1)
    }

    fn reset() {
        SOLID_BRUSHES.with(|cache| cache.borrow_mut().clear());
    }

    /// Colour is the whole key, so two fills in the same colour share a brush and
    /// two different colours must not. A collision here would paint chrome in the
    /// wrong colour.
    #[test]
    fn a_colour_keeps_one_brush() {
        reset();
        let magenta = COLORREF(0x8515C7);
        let first = cached_brush_with(magenta, fake_brush);
        let second = cached_brush_with(magenta, |_| panic!("created a second brush"));
        assert_eq!(first, second);
        assert_ne!(first, cached_brush_with(COLORREF(0x85C715), fake_brush));
    }

    /// Past the cap the caller has to fall back rather than keep allocating, since
    /// these handles are never released.
    #[test]
    fn the_cache_stops_growing_at_the_cap() {
        reset();
        for index in 0..BRUSH_CACHE_LIMIT as u32 {
            assert!(cached_brush_with(COLORREF(index), fake_brush).is_some());
        }
        assert!(cached_brush_with(COLORREF(9_999), fake_brush).is_none());
        SOLID_BRUSHES.with(|cache| assert_eq!(cache.borrow().len(), BRUSH_CACHE_LIMIT));
    }

    /// A colour already held stays available after the cap is reached; only new
    /// colours fall back.
    #[test]
    fn a_full_cache_still_serves_what_it_holds() {
        reset();
        for index in 0..BRUSH_CACHE_LIMIT as u32 {
            cached_brush_with(COLORREF(index), fake_brush);
        }
        assert_eq!(
            cached_brush_with(COLORREF(7), |_| panic!("recreated a held brush")),
            Some(fake_brush(COLORREF(7)))
        );
    }

    /// GDI returning a null handle must not be cached, or every later fill in that
    /// colour would draw with nothing.
    #[test]
    fn a_refused_brush_is_not_cached() {
        reset();
        assert!(cached_brush_with(COLORREF(3), |_| HBRUSH(0)).is_none());
        SOLID_BRUSHES.with(|cache| assert!(cache.borrow().is_empty()));
    }
}
