//! Lower-level text, selection, and highlight drawing helpers.

use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentColors {
    pub(super) normal: COLORREF,
    pub(super) highlight: COLORREF,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SegmentFonts {
    pub(super) normal: HFONT,
    pub(super) bold: HFONT,
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
            style.fonts.bold
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
    bold_font: HFONT,
) -> i32 {
    segments
        .iter()
        .map(|(text, highlighted)| {
            let font = if *highlighted { bold_font } else { normal_font };
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
    unsafe {
        let brush = CreateSolidBrush(selection.bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
    }
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

fn row_boundaries(hdc: HDC, font: HFONT, text: &str) -> Vec<SelectableBoundary> {
    let mut out = Vec::new();
    out.push(SelectableBoundary {
        byte_index: 0,
        x_offset: 0,
    });
    let mut x = 0;
    for (idx, ch) in text.char_indices() {
        let mut single = String::new();
        single.push(ch);
        x += text_width_with_font(hdc, font, &single);
        out.push(SelectableBoundary {
            byte_index: idx + ch.len_utf8(),
            x_offset: x,
        });
    }
    out
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
    for (text, bold) in segments {
        let font = if *bold {
            style.fonts.bold
        } else {
            style.fonts.normal
        };
        let color = if *bold {
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

pub(super) fn draw_header_button(
    hdc: HDC,
    rect: &RECT,
    label: &str,
    bg_color: COLORREF,
    text_color: COLORREF,
    font: HFONT,
    pressed: bool,
) {
    let mut button_rect = *rect;
    let bg = if pressed {
        lerp_color(bg_color, rgb(0, 0, 0), 0.28)
    } else {
        bg_color
    };
    if pressed
        && button_rect.right - button_rect.left > 4
        && button_rect.bottom - button_rect.top > 4
    {
        button_rect.left += 1;
        button_rect.top += 1;
        button_rect.right -= 1;
        button_rect.bottom -= 1;
    }
    unsafe {
        let brush = CreateSolidBrush(bg);
        FillRect(hdc, &button_rect, brush);
        DeleteObject(brush);
        SelectObject(hdc, font);
        SetTextColor(hdc, text_color);
    }
    let label_width = text_width(hdc, label);
    let metrics = text_metrics(hdc, font);
    let x = button_rect.left
        + ((button_rect.right - button_rect.left - label_width) / 2).max(0)
        + if pressed { 1 } else { 0 };
    let y = button_rect.top
        + ((button_rect.bottom - button_rect.top - metrics.tmHeight as i32) / 2).max(0)
        + if pressed { 1 } else { 0 };
    draw_text_line(hdc, label, x, y);
}

#[cfg(test)]
mod tests {
    use super::{favorite_match_ranges, ranges_overlap, segments_for_row};
    use crate::popup::FavoritesSnapshot;

    #[test]
    fn favorite_match_ranges_prefers_longest_non_overlapping_matches() {
        let favorites = FavoritesSnapshot {
            snippets_lower: vec!["tofu".to_string(), "tofu curry".to_string()],
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
