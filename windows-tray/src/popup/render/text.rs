//! Lower-level text, selection, and highlight drawing helpers.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentColors {
    pub(super) normal: COLORREF,
    pub(super) highlight: COLORREF,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentFonts {
    pub(super) normal: HFONT,
    pub(super) highlight: HFONT,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentStyle {
    pub(super) fonts: SegmentFonts,
    pub(super) colors: SegmentColors,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LinePlacement {
    pub(super) x: i32,
    pub(super) y: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RowBounds {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) line_height: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SelectionOverlay {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) bg_color: COLORREF,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RowCaptureContext {
    pub(super) bounds: RowBounds,
    pub(super) hdc: HDC,
    pub(super) font: HFONT,
}

pub(super) fn draw_main_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    placement: LinePlacement,
    style: SegmentStyle,
) {
    let mut cursor = placement.x;
    for (text, highlighted) in segments {
        let font = if *highlighted {
            style.fonts.highlight
        } else {
            style.fonts.normal
        };
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(
                hdc,
                if *highlighted {
                    style.colors.highlight
                } else {
                    style.colors.normal
                },
            );
        }
        draw_text_line(hdc, text, cursor, placement.y);
        cursor += text_width_with_font(hdc, font, text);
    }
}

pub(super) fn text_segments_width(
    hdc: HDC,
    segments: &[(String, bool)],
    normal_font: HFONT,
    highlight_font: HFONT,
) -> i32 {
    segments
        .iter()
        .map(|(text, highlighted)| {
            let font = if *highlighted {
                highlight_font
            } else {
                normal_font
            };
            text_width_with_font(hdc, font, text)
        })
        .sum()
}

pub(super) fn draw_selection_bg_for_row(
    hdc: HDC,
    row: &WrappedRow,
    bounds: RowBounds,
    selection: SelectionOverlay,
) {
    let start = max(row.start, selection.start);
    let end = min(row.end, selection.end);
    if start >= end {
        return;
    }
    let local_start = start.saturating_sub(row.start);
    let local_end = end.saturating_sub(row.start);
    let Some(left_slice) = row.text.get(..local_start) else {
        return;
    };
    let Some(right_slice) = row.text.get(..local_end) else {
        return;
    };
    let left_width = text_width(hdc, left_slice);
    let right_width = text_width(hdc, right_slice);
    let rect = RECT {
        left: bounds.left + left_width,
        top: bounds.top,
        right: bounds.left + right_width,
        bottom: bounds.top + bounds.line_height - 1,
    };
    fill_solid_rect(hdc, &rect, selection.bg_color);
}

pub(super) fn draw_selection_bg_for_segments(
    hdc: HDC,
    row: &WrappedRow,
    segments: &[(String, bool)],
    bounds: RowBounds,
    selection: SelectionOverlay,
    fonts: SegmentFonts,
) {
    let start = max(row.start, selection.start);
    let end = min(row.end, selection.end);
    if start >= end {
        return;
    }
    let local_start = start.saturating_sub(row.start);
    let local_end = end.saturating_sub(row.start);
    let left_width = segmented_local_width(hdc, segments, fonts, local_start);
    let right_width = segmented_local_width(hdc, segments, fonts, local_end);
    let rect = RECT {
        left: bounds.left + left_width,
        top: bounds.top,
        right: bounds.left + right_width,
        bottom: bounds.top + bounds.line_height - 1,
    };
    fill_solid_rect(hdc, &rect, selection.bg_color);
}

pub(super) fn add_selectable_row(
    layout: &mut SelectableLayout,
    item_id: usize,
    row: &WrappedRow,
    context: RowCaptureContext,
) {
    layout.rows.push(SelectableRow {
        item_id,
        start: row.start,
        end: row.end,
        left: context.bounds.left,
        top: context.bounds.top,
        bottom: context.bounds.top + context.bounds.line_height,
        boundaries: row_boundaries(context.hdc, context.font, &row.text),
    });
}

pub(super) fn add_selectable_segmented_row(
    layout: &mut SelectableLayout,
    item_id: usize,
    row: &WrappedRow,
    context: RowCaptureContext,
    segments: &[(String, bool)],
    fonts: SegmentFonts,
) {
    layout.rows.push(SelectableRow {
        item_id,
        start: row.start,
        end: row.end,
        left: context.bounds.left,
        top: context.bounds.top,
        bottom: context.bounds.top + context.bounds.line_height,
        boundaries: row_boundaries_for_segments(context.hdc, segments, fonts),
    });
}

/// Identifies a run of text whose character geometry is already known.
///
/// Boundaries are offsets from the start of the row, so they do not depend on
/// where the row is drawn — only on the glyphs and the fonts measuring them. That
/// is what makes them cacheable across paints at all: a selection drag, a hover,
/// and an animation frame all redraw identical rows at identical widths.
///
/// The font handles are part of the key, and they are only meaningful as such
/// because `create_fonts` keeps them for the life of the process. If fonts ever
/// go back to being created and destroyed per paint, this cache must key on the
/// font's description instead, or it will return another font's measurements.
#[derive(PartialEq, Eq, Hash)]
struct RowBoundaryKey {
    fonts: (isize, isize),
    text: String,
}

/// Enough for every row of every restaurant at a couple of themes. Cleared
/// wholesale on overflow rather than evicted: the contents are rebuilt in one
/// paint, so a rare full clear costs far less than tracking recency.
const ROW_BOUNDARY_CACHE_LIMIT: usize = 1024;

thread_local! {
    static ROW_BOUNDARY_CACHE: RefCell<HashMap<RowBoundaryKey, Arc<Vec<SelectableBoundary>>>> =
        RefCell::new(HashMap::new());
}

/// Clears memoised row geometry. Only needed if fonts stop being permanent.
#[cfg(test)]
fn clear_row_boundary_cache() {
    ROW_BOUNDARY_CACHE.with(|cache| cache.borrow_mut().clear());
}

fn cached_row_boundaries(
    key: RowBoundaryKey,
    measure: impl FnOnce() -> Vec<SelectableBoundary>,
) -> Arc<Vec<SelectableBoundary>> {
    if let Some(hit) = ROW_BOUNDARY_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        crate::perf::count_row_boundary_hit();
        return hit;
    }
    crate::perf::count_row_boundary_miss();
    let boundaries = Arc::new(measure());
    ROW_BOUNDARY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= ROW_BOUNDARY_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, boundaries.clone());
    });
    boundaries
}

fn row_boundaries(hdc: HDC, font: HFONT, text: &str) -> Arc<Vec<SelectableBoundary>> {
    let key = RowBoundaryKey {
        fonts: (font.0, font.0),
        text: text.to_string(),
    };
    cached_row_boundaries(key, || measure_row_boundaries(hdc, font, text))
}

fn measure_row_boundaries(hdc: HDC, font: HFONT, text: &str) -> Vec<SelectableBoundary> {
    let mut out = Vec::new();
    out.push(SelectableBoundary {
        byte_index: 0,
        x_offset: 0,
    });
    for (idx, ch) in text.char_indices() {
        let boundary = idx + ch.len_utf8();
        let x = text
            .get(..boundary)
            .map(|prefix| text_width_with_font(hdc, font, prefix))
            .unwrap_or(0);
        out.push(SelectableBoundary {
            byte_index: boundary,
            x_offset: x,
        });
    }
    out
}

fn row_boundaries_for_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    fonts: SegmentFonts,
) -> Arc<Vec<SelectableBoundary>> {
    // The highlight flag changes which font measures a segment, so it has to be
    // part of the key. `\u{1}` cannot occur in menu text, which keeps segment
    // boundaries unambiguous without allocating a structured key per row.
    let mut text = String::new();
    for (segment, highlighted) in segments {
        text.push(if *highlighted { '\u{1}' } else { '\u{2}' });
        text.push_str(segment);
    }
    let key = RowBoundaryKey {
        fonts: (fonts.normal.0, fonts.highlight.0),
        text,
    };
    cached_row_boundaries(key, || {
        measure_row_boundaries_for_segments(hdc, segments, fonts)
    })
}

fn measure_row_boundaries_for_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    fonts: SegmentFonts,
) -> Vec<SelectableBoundary> {
    let mut out = Vec::new();
    out.push(SelectableBoundary {
        byte_index: 0,
        x_offset: 0,
    });
    let mut byte_index = 0usize;
    let mut x = 0;
    for (text, highlighted) in segments {
        let font = if *highlighted {
            fonts.highlight
        } else {
            fonts.normal
        };
        let segment_start_byte = byte_index;
        let segment_start_x = x;
        for (idx, ch) in text.char_indices() {
            let boundary = idx + ch.len_utf8();
            byte_index = segment_start_byte + boundary;
            x = segment_start_x
                + text
                    .get(..boundary)
                    .map(|prefix| text_width_with_font(hdc, font, prefix))
                    .unwrap_or(0);
            out.push(SelectableBoundary {
                byte_index,
                x_offset: x,
            });
        }
    }
    out
}

fn segmented_local_width(
    hdc: HDC,
    segments: &[(String, bool)],
    fonts: SegmentFonts,
    target: usize,
) -> i32 {
    let mut seen = 0usize;
    let mut width = 0;
    for (text, highlighted) in segments {
        let font = if *highlighted {
            fonts.highlight
        } else {
            fonts.normal
        };
        if seen + text.len() <= target {
            width += text_width_with_font(hdc, font, text);
            seen += text.len();
            continue;
        }
        let take = target.saturating_sub(seen);
        if take > 0 {
            if let Some(part) = text.get(..take) {
                width += text_width_with_font(hdc, font, part);
            }
        }
        break;
    }
    width
}

pub(super) fn segments_for_row(
    full_text: &str,
    row_start: usize,
    row_end: usize,
    ranges: &[(usize, usize)],
) -> Vec<(String, bool)> {
    let Some(row_slice) = full_text.get(row_start..row_end) else {
        return vec![(String::new(), false)];
    };
    let mut out = Vec::new();
    let mut cursor = row_start;
    for (start, end) in ranges {
        let overlap_start = max(*start, row_start);
        let overlap_end = min(*end, row_end);
        if overlap_start >= overlap_end {
            continue;
        }
        if cursor < overlap_start {
            if let Some(normal) = full_text.get(cursor..overlap_start) {
                out.push((normal.to_string(), false));
            }
        }
        if let Some(highlight) = full_text.get(overlap_start..overlap_end) {
            out.push((highlight.to_string(), true));
        }
        cursor = overlap_end;
    }
    if cursor < row_end {
        if let Some(rest) = full_text.get(cursor..row_end) {
            out.push((rest.to_string(), false));
        }
    }
    if out.is_empty() {
        out.push((row_slice.to_string(), false));
    }
    out
}

pub(super) fn favorite_match_ranges(
    text: &str,
    favorites: &FavoritesSnapshot,
) -> Vec<(usize, usize)> {
    if text.is_empty() || favorites.snippets_lower.is_empty() {
        return Vec::new();
    }
    let lower_text = text.to_lowercase();
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for snippet_lower in &favorites.snippets_lower {
        if snippet_lower.is_empty() {
            continue;
        }
        let mut search_start = 0usize;
        while search_start < lower_text.len() {
            let Some(found) = lower_text[search_start..].find(snippet_lower) else {
                break;
            };
            let start = search_start + found;
            let end = start + snippet_lower.len();
            if text.get(start..end).is_some() {
                candidates.push((start, end));
            }
            search_start = end;
        }
    }

    // Prefer longer snippets first so nested favorites like "tofu" and
    // "tofu curry" resolve to one highlight instead of overlapping segments.
    candidates.sort_by(|a, b| {
        let len_a = a.1.saturating_sub(a.0);
        let len_b = b.1.saturating_sub(b.0);
        len_b.cmp(&len_a).then(a.0.cmp(&b.0))
    });

    let mut kept: Vec<(usize, usize)> = Vec::new();
    for range in candidates {
        if kept.iter().any(|existing| ranges_overlap(*existing, range)) {
            continue;
        }
        kept.push(range);
    }
    kept.sort_by_key(|range| range.0);
    kept
}

fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    max(a.0, b.0) < min(a.1, b.1)
}

pub(super) fn draw_text_segments(
    hdc: HDC,
    segments: &[(String, bool)],
    placement: LinePlacement,
    style: SegmentStyle,
) {
    let mut cursor = placement.x;
    for (text, highlighted) in segments {
        let font = if *highlighted {
            style.fonts.highlight
        } else {
            style.fonts.normal
        };
        let color = if *highlighted {
            style.colors.highlight
        } else {
            style.colors.normal
        };
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(hdc, color);
        }
        draw_text_line(hdc, text, cursor, placement.y);
        cursor += text_width(hdc, text);
    }
}

pub(super) fn draw_text_line(hdc: HDC, text: &str, x: i32, y: i32) {
    let wide = to_wstring(text);
    unsafe {
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = TextOutW(hdc, x, y, slice);
        }
    }
}

pub(super) fn fit_text_to_width(hdc: HDC, text: &str, max_width: i32) -> String {
    let clean = normalize_text(text);
    if clean.is_empty() || max_width <= 0 {
        return String::new();
    }
    if text_width(hdc, &clean) <= max_width {
        return clean;
    }

    let ellipsis = "...";
    let ellipsis_width = text_width(hdc, ellipsis);
    if ellipsis_width >= max_width {
        return ellipsis.to_string();
    }

    let mut out = String::new();
    for ch in clean.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);
        candidate.push_str(ellipsis);
        if text_width(hdc, &candidate) > max_width {
            break;
        }
        out.push(ch);
    }

    let mut trimmed = out.trim_end().to_string();
    trimmed.push_str(ellipsis);
    trimmed
}

/// Fill for a pressed button under the flat vocabulary.
///
/// Darkening is the usual move, but it does nothing to an already-black face.
/// Near-black faces brighten instead, so the press always registers.
fn pressed_fill(face: COLORREF) -> COLORREF {
    if contrast_ratio(face, rgb(0, 0, 0)) < 2.0 {
        lerp_color(face, rgb(255, 255, 255), 0.22)
    } else {
        lerp_color(face, rgb(0, 0, 0), 0.28)
    }
}

/// The mark on a header button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeaderGlyph {
    Prev,
    Next,
    Close,
}

/// Stroke width of the close cross, scaled from the button it sits in.
fn cross_stroke(button_size: i32) -> i32 {
    // Falls to a single pixel on the smallest buttons, which is what the
    // original title-bar cross used at that size.
    (button_size / 14).max(1)
}

fn fill_rect(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: COLORREF) {
    fill_solid_rect(
        hdc,
        &RECT {
            left,
            top,
            right,
            bottom,
        },
        color,
    );
}

/// Solid navigation arrow, built one scanline at a time.
///
/// The same construction as a scrollbar arrow: a flat back edge and a single
/// apex pixel, with each row one pixel narrower than the last. Solid shapes read
/// as deliberate at this size in a way strokes do not — a chevron thick enough
/// to be visible in a 33px button is also thick enough to look clumsy.
fn draw_nav_arrow(hdc: HDC, rect: &RECT, point_left: bool, color: COLORREF) {
    let button_size = (rect.right - rect.left).min(rect.bottom - rect.top);
    // 40% looked right on Teletext's low-contrast white-on-blue and far too
    // heavy in black-on-grey. Perceived weight is area times contrast, so the
    // proportion has to suit the theme that renders it hardest.
    let mut height = (button_size * 32 / 100).max(5);
    if height % 2 == 0 {
        // Odd, so the apex lands on a whole pixel instead of straddling two.
        height += 1;
    }
    let half = height / 2;
    let width = half + 1;
    let left = (rect.left + rect.right) / 2 - width / 2;
    let top = (rect.top + rect.bottom) / 2 - half;

    for row in 0..height {
        let run = width - (row - half).abs();
        if run <= 0 {
            continue;
        }
        let (x0, x1) = if point_left {
            (left + width - run, left + width)
        } else {
            (left, left + run)
        };
        fill_rect(hdc, x0, top + row, x1, top + row + 1, color);
    }
}

/// Close cross, drawn the way a bitmap font would: one horizontal run per row,
/// stepping across by a pixel each time.
///
/// Keeps the diagonals thin and even. Stamping a square per step instead gives
/// a staircase whose perpendicular thickness is about 1.4x the nominal stroke,
/// which is how the first attempt ended up looking hand-drawn.
fn draw_close_cross(hdc: HDC, rect: &RECT, color: COLORREF) {
    let button_size = (rect.right - rect.left).min(rect.bottom - rect.top);
    let stroke = cross_stroke(button_size);
    let span = (button_size * 38 / 100).max(stroke * 3 + 1);
    let rows = span - stroke + 1;
    let left = (rect.left + rect.right) / 2 - span / 2;
    let top = (rect.top + rect.bottom) / 2 - rows / 2;

    for row in 0..rows {
        let y = top + row;
        let down_right = left + row;
        fill_rect(hdc, down_right, y, down_right + stroke, y + 1, color);
        let down_left = left + span - stroke - row;
        fill_rect(hdc, down_left, y, down_left + stroke, y + 1, color);
    }
}

/// Draws a header button's mark, centred in `rect`.
///
/// Drawn rather than typed for the same reason the bullets and rail markers
/// are: as font glyphs, `X` was a capital letter and the chevrons were
/// punctuation, so the mark filled whatever share of the button the theme's
/// font happened to give it, and which share depended on the family.
pub(super) fn draw_header_glyph(hdc: HDC, glyph: HeaderGlyph, rect: &RECT, color: COLORREF) {
    if (rect.right - rect.left).min(rect.bottom - rect.top) < 6 {
        return;
    }
    match glyph {
        HeaderGlyph::Prev => draw_nav_arrow(hdc, rect, true, color),
        HeaderGlyph::Next => draw_nav_arrow(hdc, rect, false, color),
        HeaderGlyph::Close => draw_close_cross(hdc, rect, color),
    }
}

pub(super) fn draw_header_button(
    hdc: HDC,
    rect: &RECT,
    glyph: HeaderGlyph,
    bg_color: COLORREF,
    glyph_color: COLORREF,
    pressed: bool,
    hovered: bool,
    edge: ChromeEdge,
) {
    let mut button_rect = *rect;
    let bg = if pressed && edge.style.press_shifts_fill() {
        pressed_fill(bg_color)
    } else if hovered {
        lerp_color(bg_color, glyph_color, 0.14)
    } else {
        bg_color
    };
    // Bevel themes signal the press with the edge flip alone, the way a real
    // push button does; only the flat vocabulary shrinks the fill.
    if pressed
        && edge.style.press_shifts_fill()
        && button_rect.right - button_rect.left > 4
        && button_rect.bottom - button_rect.top > 4
    {
        button_rect.left += 1;
        button_rect.top += 1;
        button_rect.right -= 1;
        button_rect.bottom -= 1;
    }
    fill_solid_rect(hdc, &button_rect, bg);
    draw_edge(hdc, &button_rect, edge.button(pressed), bg);
    let nudge = if pressed { 1 } else { 0 };
    let glyph_rect = RECT {
        left: button_rect.left + nudge,
        top: button_rect.top + nudge,
        right: button_rect.right + nudge,
        bottom: button_rect.bottom + nudge,
    };
    draw_header_glyph(hdc, glyph, &glyph_rect, glyph_color);
}

#[cfg(test)]
mod tests {
    use super::{
        cached_row_boundaries, clear_row_boundary_cache, cross_stroke, favorite_match_ranges,
        pressed_fill, ranges_overlap, segments_for_row, Arc, RowBoundaryKey, SelectableBoundary,
        ROW_BOUNDARY_CACHE_LIMIT,
    };
    use crate::popup::theme::{contrast_ratio, rgb};

    fn boundaries(count: usize) -> Vec<SelectableBoundary> {
        (0..count)
            .map(|index| SelectableBoundary {
                byte_index: index,
                x_offset: index as i32 * 7,
            })
            .collect()
    }

    fn key(normal: isize, highlight: isize, text: &str) -> RowBoundaryKey {
        RowBoundaryKey {
            fonts: (normal, highlight),
            text: text.to_string(),
        }
    }

    /// The whole point: an unchanged row measured once is reused, and the reuse
    /// shares the same allocation rather than copying it.
    #[test]
    fn an_unchanged_row_is_measured_once() {
        clear_row_boundary_cache();
        let first = cached_row_boundaries(key(1, 1, "Kaali-omenasalaatti"), || boundaries(3));
        let second = cached_row_boundaries(key(1, 1, "Kaali-omenasalaatti"), || {
            panic!("measured a row that was already known")
        });
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn different_text_is_measured_separately() {
        clear_row_boundary_cache();
        let first = cached_row_boundaries(key(1, 1, "Linssisalaatti"), || boundaries(3));
        let second = cached_row_boundaries(key(1, 1, "Riisi"), || boundaries(5));
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.len(), 5);
    }

    /// Font handles are part of the key because the same glyphs measure
    /// differently at another size or weight. Missing this would show as
    /// selection highlights landing off the text after a theme or scale change.
    #[test]
    fn the_same_text_in_another_font_is_measured_again() {
        clear_row_boundary_cache();
        let normal = cached_row_boundaries(key(1, 1, "Lounas"), || boundaries(3));
        let bold = cached_row_boundaries(key(2, 2, "Lounas"), || boundaries(4));
        assert!(!Arc::ptr_eq(&normal, &bold));
        assert_eq!(bold.len(), 4);
    }

    /// Segment rows encode the highlight flag into the key, so a row whose
    /// favourite match moved is not served another row's geometry.
    #[test]
    fn segment_rows_key_on_which_font_measures_each_segment() {
        clear_row_boundary_cache();
        let plain = cached_row_boundaries(key(1, 2, "\u{2}Riisi"), || boundaries(3));
        let highlighted = cached_row_boundaries(key(1, 2, "\u{1}Riisi"), || boundaries(6));
        assert!(!Arc::ptr_eq(&plain, &highlighted));
        assert_eq!(highlighted.len(), 6);
    }

    /// The cache is cleared wholesale on overflow, so it must stay correct across
    /// that boundary rather than serving a stale entry.
    #[test]
    fn overflow_clears_the_cache_without_returning_stale_geometry() {
        clear_row_boundary_cache();
        for index in 0..=ROW_BOUNDARY_CACHE_LIMIT {
            let text = format!("row {index}");
            cached_row_boundaries(key(1, 1, &text), || boundaries(2));
        }
        let refreshed = cached_row_boundaries(key(1, 1, "row 0"), || boundaries(9));
        assert_eq!(refreshed.len(), 9);
    }

    #[test]
    fn cross_stroke_stays_thin_at_every_button_size() {
        // Smallest button the scale can produce, up through a 4x DPI one.
        for button_size in 18..=130 {
            let stroke = cross_stroke(button_size);
            let span = (button_size * 38 / 100).max(stroke * 3 + 1);
            assert!(stroke >= 1, "button {button_size} lost its stroke");
            assert!(
                stroke * 4 <= span,
                "button {button_size} stroke {stroke} is heavy for a {span} span"
            );
        }
    }

    #[test]
    fn cross_stroke_grows_with_the_button() {
        assert!(cross_stroke(96) > cross_stroke(32));
    }

    #[test]
    fn pressed_fill_brightens_a_black_button_face() {
        let black = rgb(0, 0, 0);
        let pressed = pressed_fill(black);

        assert!(
            contrast_ratio(pressed, black) > 1.5,
            "a black face must visibly change when pressed"
        );
    }

    #[test]
    fn pressed_fill_darkens_a_light_button_face() {
        let face = rgb(200, 200, 200);
        let pressed = pressed_fill(face);

        assert!(contrast_ratio(pressed, face) > 1.2);
        assert!(pressed.0 < face.0, "a light face darkens under the press");
    }

    use crate::popup::FavoritesSnapshot;

    #[test]
    fn favorite_match_ranges_prefers_longest_non_overlapping_matches() {
        let favorites = FavoritesSnapshot {
            snippets_lower: vec!["tofu".to_string(), "tofu curry".to_string()],
            ingredient_snippets_lower: Vec::new(),
        };
        let ranges = favorite_match_ranges("Spicy tofu curry bowl", &favorites);
        assert_eq!(ranges, vec![(6, 16)]);
    }

    #[test]
    fn segments_for_row_splits_highlighted_ranges() {
        let segments = segments_for_row("abcdef", 1, 5, &[(2, 4)]);
        assert_eq!(
            segments,
            vec![
                ("b".to_string(), false),
                ("cd".to_string(), true),
                ("e".to_string(), false),
            ]
        );
    }

    #[test]
    fn ranges_overlap_requires_real_overlap() {
        assert!(ranges_overlap((1, 4), (3, 6)));
        assert!(!ranges_overlap((1, 3), (3, 5)));
    }
}
